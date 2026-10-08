use std::time::Duration;

use anyhow::{Result, ensure};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::Decoder;

pub struct VideoFrame {
    pub size: [usize; 2],
    pub rgba: Vec<u8>,
    pub timestamp: Duration,
}

pub fn validate_file(path: &std::path::Path) -> Result<()> {
    let mut decoder = Decoder::open(path)?;
    ensure!(decoder.next_frame()?.is_some(), "В файле нет видеокадров");
    Ok(())
}

fn rgba_from_bgrx(bytes: &[u8], size: [usize; 2], stride: i32) -> Result<Vec<u8>> {
    let [width, height] = size;
    ensure!(
        width > 0 && height > 0 && width <= 1920 && height <= 1920,
        "Неподдерживаемый размер видеокадра"
    );
    let pitch = stride.unsigned_abs() as usize;
    ensure!(
        pitch >= width * 4
            && pitch
                .checked_mul(height - 1)
                .and_then(|length| length.checked_add(width * 4))
                .is_some_and(|length| length <= bytes.len()),
        "Повреждённый видеокадр"
    );
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        let row = if stride < 0 { height - 1 - y } else { y };
        let start = row * pitch;
        for &[b, g, r, _] in bytes[start..start + width * 4].as_chunks::<4>().0 {
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Ok(rgba)
}

#[cfg(not(windows))]
pub struct Decoder;

#[cfg(not(windows))]
impl Decoder {
    pub fn open(_path: &std::path::Path) -> Result<Self> {
        anyhow::bail!("Видеофоны поддерживаются в Windows")
    }
    pub fn next_frame(&mut self) -> Result<Option<VideoFrame>> {
        anyhow::bail!("Видеофоны поддерживаются в Windows")
    }
    pub fn rewind(&mut self) -> Result<()> {
        anyhow::bail!("Видеофоны поддерживаются в Windows")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_rows_handle_padding_orientation_and_opaque_alpha() {
        let data = [0, 0, 255, 0, 99, 99, 99, 99, 255, 0, 0, 0, 99, 99, 99, 99];
        assert_eq!(
            rgba_from_bgrx(&data, [1, 2], 8).unwrap(),
            [255, 0, 0, 255, 0, 0, 255, 255]
        );
        assert_eq!(
            rgba_from_bgrx(&data, [1, 2], -8).unwrap(),
            [0, 0, 255, 255, 255, 0, 0, 255]
        );
        assert!(rgba_from_bgrx(&data[..6], [1, 2], 8).is_err());
        assert!(rgba_from_bgrx(&data, [1, 2], 2).is_err());
        assert!(rgba_from_bgrx(&data, [0, 2], 8).is_err());
    }
}
