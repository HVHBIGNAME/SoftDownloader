use std::os::windows::ffi::OsStrExt;
use std::path::{Component, Path, Prefix};
use std::time::Duration;

use anyhow::{Context, Result, bail, ensure};
use windows::Win32::Media::MediaFoundation::{
    IMF2DBuffer2, IMFAttributes, IMFMediaBuffer, IMFMediaType, IMFSourceReader,
    MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE,
    MF_SOURCE_READER_ALL_STREAMS, MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED,
    MF_SOURCE_READERF_ENDOFSTREAM, MF_SOURCE_READERF_ERROR, MF_VERSION, MF2DBuffer_LockFlags_Read,
    MFCreateAttributes, MFCreateMediaType, MFCreateSourceReaderFromURL,
    MFGetStrideForBitmapInfoHeader, MFMediaType_Video, MFSTARTUP_FULL, MFShutdown, MFStartup,
    MFVideoFormat_RGB32,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::System::Variant::VT_I8;
use windows::core::{GUID, Interface, PCWSTR};

use super::{VideoFrame, rgba_from_bgrx};

const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

struct Runtime;

impl Runtime {
    fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        }
        if let Err(error) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_FULL) } {
            unsafe {
                CoUninitialize();
            }
            return Err(error).context("Недоступна Windows Media Foundation");
        }
        Ok(Self)
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}

pub struct Decoder {
    reader: IMFSourceReader,
    size: [usize; 2],
    stride: i32,
    _runtime: Runtime,
}

impl Decoder {
    pub fn open(path: &Path) -> Result<Self> {
        ensure!(path.is_absolute(), "Выберите локальный видеофайл");
        let metadata = path.metadata().context("Не удалось открыть видеофайл")?;
        ensure!(
            metadata.is_file() && metadata.len() <= 512 * 1024 * 1024,
            "Нужен видеофайл размером до 512 МБ"
        );
        let runtime = Runtime::new()?;
        let wide = source_path(path);
        let mut attributes: Option<IMFAttributes> = None;
        let reader = unsafe {
            MFCreateAttributes(&mut attributes, 1)?;
            let attributes = attributes.context("Не удалось создать параметры видео")?;
            attributes.SetUINT32(&MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING, 1)?;
            let reader = MFCreateSourceReaderFromURL(PCWSTR(wide.as_ptr()), &attributes)
                .context("Windows не смогла открыть видео. Используйте MP4 с H.264")?;
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)?;
            reader.SetStreamSelection(VIDEO, true)?;
            reader
        };
        configure(&reader)?;
        let (size, stride) = format(&reader)?;
        Ok(Self {
            reader,
            size,
            stride,
            _runtime: runtime,
        })
    }

    pub fn next_frame(&mut self) -> Result<Option<VideoFrame>> {
        for _ in 0..120 {
            let (mut flags, mut timestamp, mut sample) = (0, 0, None);
            unsafe {
                self.reader.ReadSample(
                    VIDEO,
                    0,
                    None,
                    Some(&mut flags),
                    Some(&mut timestamp),
                    Some(&mut sample),
                )?;
            }
            ensure!(
                flags & MF_SOURCE_READERF_ERROR.0 as u32 == 0,
                "Ошибка декодирования видео"
            );
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Ok(None);
            }
            if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
                (self.size, self.stride) = format(&self.reader)?;
            }
            if let Some(sample) = sample {
                let buffer = unsafe { sample.ConvertToContiguousBuffer()? };
                let locked = LockedBuffer::new(buffer, self.size, self.stride)?;
                let bytes = unsafe { std::slice::from_raw_parts(locked.data, locked.length) };
                let rgba = rgba_from_bgrx(bytes, self.size, locked.stride)?;
                return Ok(Some(VideoFrame {
                    size: self.size,
                    rgba,
                    timestamp: Duration::from_micros(timestamp.max(0) as u64 / 10),
                }));
            }
        }
        bail!("В файле не найдены видеокадры")
    }

    pub fn rewind(&mut self) -> Result<()> {
        let mut position = PROPVARIANT::default();
        unsafe {
            let value = &mut *position.Anonymous.Anonymous;
            value.vt = VT_I8;
            value.Anonymous.hVal = 0;
            self.reader.SetCurrentPosition(&GUID::zeroed(), &position)?;
        }
        Ok(())
    }
}

fn source_path(path: &Path) -> Vec<u16> {
    let mut wide: Vec<_> = path.as_os_str().encode_wide().collect();
    if let Some(Component::Prefix(prefix)) = path.components().next() {
        match prefix.kind() {
            Prefix::VerbatimDisk(_) => {
                wide.drain(..4);
            }
            Prefix::VerbatimUNC(_, _) => {
                wide.splice(..8, [92, 92]);
            }
            _ => {}
        }
    }
    wide.push(0);
    wide
}

fn configure(reader: &IMFSourceReader) -> Result<()> {
    unsafe {
        let native = reader.GetNativeMediaType(VIDEO, 0)?;
        let [width, height] = dimensions(&native)?;
        ensure!(
            width <= 8192 && height <= 8192,
            "Разрешение исходного видео превышает 8K"
        );
        let scale = (1280.0 / width as f64).min(720.0 / height as f64).min(1.0);
        let width = ((width as f64 * scale) as u32 / 2 * 2).max(2);
        let height = ((height as f64 * scale) as u32 / 2 * 2).max(2);
        let output = MFCreateMediaType()?;
        output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
        output.SetUINT64(
            &MF_MT_FRAME_SIZE,
            (u64::from(width) << 32) | u64::from(height),
        )?;
        reader
            .SetCurrentMediaType(VIDEO, None, &output)
            .context("Windows не поддерживает этот видеокодек")?;
    }
    Ok(())
}

fn dimensions(media: &IMFMediaType) -> Result<[usize; 2]> {
    let size = unsafe { media.GetUINT64(&MF_MT_FRAME_SIZE)? };
    let dimensions = [(size >> 32) as usize, (size & 0xffff_ffff) as usize];
    ensure!(
        dimensions[0] > 0 && dimensions[1] > 0,
        "Видео имеет нулевой размер"
    );
    Ok(dimensions)
}

fn format(reader: &IMFSourceReader) -> Result<([usize; 2], i32)> {
    unsafe {
        let media = reader.GetCurrentMediaType(VIDEO)?;
        ensure!(
            media.GetGUID(&MF_MT_SUBTYPE)? == MFVideoFormat_RGB32,
            "Неожиданный формат видеокадра"
        );
        let size = dimensions(&media)?;
        let stride = match media.GetUINT32(&MF_MT_DEFAULT_STRIDE) {
            Ok(value) => value as i32,
            Err(_) => MFGetStrideForBitmapInfoHeader(MFVideoFormat_RGB32.data1, size[0] as u32)?,
        };
        Ok((size, stride))
    }
}

enum BufferLock {
    Linear(IMFMediaBuffer),
    Surface(IMF2DBuffer2),
}

struct LockedBuffer {
    buffer: BufferLock,
    data: *mut u8,
    length: usize,
    stride: i32,
}

impl LockedBuffer {
    fn new(buffer: IMFMediaBuffer, size: [usize; 2], stride: i32) -> Result<Self> {
        if let Ok(surface) = buffer.cast::<IMF2DBuffer2>() {
            return Self::surface(surface, size);
        }
        let (mut data, mut length) = (std::ptr::null_mut(), 0);
        unsafe {
            buffer.Lock(&mut data, None, Some(&mut length))?;
        }
        let locked = Self {
            buffer: BufferLock::Linear(buffer),
            data,
            length: length as usize,
            stride,
        };
        ensure!(!data.is_null() && length > 0, "Пустой видеобуфер");
        Ok(locked)
    }

    fn surface(buffer: IMF2DBuffer2, size: [usize; 2]) -> Result<Self> {
        let (mut first_row, mut start, mut length, mut stride) =
            (std::ptr::null_mut(), std::ptr::null_mut(), 0, 0);
        unsafe {
            buffer.Lock2DSize(
                MF2DBuffer_LockFlags_Read,
                &mut first_row,
                &mut stride,
                &mut start,
                &mut length,
            )?;
        }
        let mut locked = Self {
            buffer: BufferLock::Surface(buffer),
            data: start,
            length: length as usize,
            stride,
        };
        ensure!(
            !start.is_null() && !first_row.is_null() && size[1] > 0,
            "Пустая видеоповерхность"
        );
        let first = (first_row as usize)
            .checked_sub(start as usize)
            .context("Некорректное начало видеокадра")?;
        let rows = (stride.unsigned_abs() as usize)
            .checked_mul(size[1] - 1)
            .context("Слишком большой видеокадр")?;
        let offset = if stride < 0 {
            first
                .checked_sub(rows)
                .context("Некорректный шаг видеокадра")?
        } else {
            first
        };
        ensure!(
            offset
                .checked_add(rows)
                .and_then(|end| end.checked_add(size[0] * 4))
                .is_some_and(|end| end <= locked.length),
            "Видеокадр выходит за границы буфера"
        );
        locked.data = unsafe { start.add(offset) };
        locked.length -= offset;
        Ok(locked)
    }
}

impl Drop for LockedBuffer {
    fn drop(&mut self) {
        unsafe {
            match &self.buffer {
                BufferLock::Linear(buffer) => {
                    let _ = buffer.Unlock();
                }
                BufferLock::Surface(buffer) => {
                    let _ = buffer.Unlock2D();
                }
            }
        }
    }
}
