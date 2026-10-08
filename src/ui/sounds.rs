use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender, TrySendError},
};
use std::time::{Duration, Instant};

use anyhow::Result;
use eframe::egui;

use crate::preferences::{SoundSettings, SoundStyle};

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Cue {
    Click,
    Success,
    Error,
}

struct Request {
    settings: SoundSettings,
    cue: Cue,
}

pub(super) struct SoundPlayer {
    sender: SyncSender<Request>,
    errors: Receiver<Option<String>>,
    enabled: Arc<AtomicBool>,
    last_request: Option<Instant>,
    last_error: Option<String>,
    pub error: Option<String>,
}

impl SoundPlayer {
    pub fn new(ctx: egui::Context) -> Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<Request>(4);
        let (report, errors) = mpsc::channel();
        let enabled = Arc::new(AtomicBool::new(true));
        let active = enabled.clone();
        std::thread::Builder::new().name("interface-sounds".into()).spawn(move || {
            let mut reported = false;
            while let Ok(mut request) = requests.recv() {
                for newer in requests.try_iter() {
                    if newer.cue >= request.cue { request = newer; }
                }
                if !active.load(Ordering::Relaxed) { continue; }
                let wave = wave(clip(request.settings.style, request.cue), request.settings.volume);
                if play(&wave) {
                    if reported && report.send(None).is_err() { break; }
                    reported = false;
                } else if !reported {
                    reported = true;
                    if report.send(Some("Не удалось воспроизвести звук. Проверьте устройство вывода Windows.".into())).is_err() { break; }
                    ctx.request_repaint();
                }
            }
        })?;
        Ok(Self {
            sender,
            errors,
            enabled,
            last_request: None,
            last_error: None,
            error: None,
        })
    }

    pub fn play(&mut self, settings: SoundSettings, cue: Cue) {
        if !settings.enabled || settings.volume == 0 {
            return;
        }
        if cue == Cue::Click
            && self
                .last_request
                .is_some_and(|time| time.elapsed() < Duration::from_millis(90))
        {
            return;
        }
        self.enabled.store(true, Ordering::Relaxed);
        self.last_request = Some(Instant::now());
        match self.sender.try_send(Request { settings, cue }) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => {
                self.error = Some("Звуковой поток завершился. Перезапустите приложение.".into())
            }
        }
    }

    pub fn update(&mut self, ctx: &egui::Context, settings: SoundSettings, error: Option<&str>) {
        self.enabled
            .store(settings.enabled && settings.volume > 0, Ordering::Relaxed);
        for error in self.errors.try_iter() {
            self.error = error;
        }
        if self.last_error.as_deref() != error {
            self.last_error = error.map(str::to_owned);
            if error.is_some() {
                self.play(settings, Cue::Error);
            }
        }
        let clicked = ctx.output(|output| output.events.iter().any(|event| {
            matches!(event, egui::output::OutputEvent::Clicked(info) if info.enabled && matches!(info.typ,
                egui::WidgetType::Button | egui::WidgetType::Checkbox | egui::WidgetType::SelectableLabel |
                egui::WidgetType::RadioButton | egui::WidgetType::ComboBox | egui::WidgetType::CollapsingHeader))
        }));
        if clicked {
            self.play(settings, Cue::Click);
        }
    }
}

impl Drop for SoundPlayer {
    fn drop(&mut self) {
        self.enabled.store(false, Ordering::Relaxed);
    }
}

fn clip(style: SoundStyle, cue: Cue) -> &'static [u8] {
    match (style, cue) {
        (SoundStyle::Glass, Cue::Click) => include_bytes!("../../assets/sounds/glass-click.pcm"),
        (SoundStyle::Glass, Cue::Success) => {
            include_bytes!("../../assets/sounds/glass-success.pcm")
        }
        (SoundStyle::Glass, Cue::Error) => include_bytes!("../../assets/sounds/glass-error.pcm"),
        (SoundStyle::Wood, Cue::Click) => include_bytes!("../../assets/sounds/wood-click.pcm"),
        (SoundStyle::Wood, Cue::Success) => include_bytes!("../../assets/sounds/wood-success.pcm"),
        (SoundStyle::Wood, Cue::Error) => include_bytes!("../../assets/sounds/wood-error.pcm"),
        (SoundStyle::Digital, Cue::Click) => {
            include_bytes!("../../assets/sounds/digital-click.pcm")
        }
        (SoundStyle::Digital, Cue::Success) => {
            include_bytes!("../../assets/sounds/digital-success.pcm")
        }
        (SoundStyle::Digital, Cue::Error) => {
            include_bytes!("../../assets/sounds/digital-error.pcm")
        }
    }
}

fn wave(pcm: &[u8], volume: u8) -> Vec<u8> {
    let mut wave = Vec::with_capacity(44 + pcm.len());
    wave.extend_from_slice(b"RIFF");
    wave.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    wave.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
    wave.extend_from_slice(&22050_u32.to_le_bytes());
    wave.extend_from_slice(&44100_u32.to_le_bytes());
    wave.extend_from_slice(b"\x02\0\x10\0data");
    wave.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    for &sample in pcm.as_chunks::<2>().0 {
        let value = i32::from(i16::from_le_bytes(sample)).clamp(-2800, 2800)
            * i32::from(volume.min(100))
            / 100;
        wave.extend_from_slice(&(value as i16).to_le_bytes());
    }
    wave
}

#[cfg(windows)]
fn play(wave: &[u8]) -> bool {
    use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_MEMORY, SND_NODEFAULT, SND_SYNC};
    // Synchronous playback keeps the owned WAV alive on this dedicated worker until Windows is done reading it.
    unsafe {
        PlaySoundW(
            wave.as_ptr().cast(),
            std::ptr::null_mut(),
            SND_MEMORY | SND_SYNC | SND_NODEFAULT,
        ) != 0
    }
}

#[cfg(not(windows))]
fn play(_wave: &[u8]) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_recordings_are_short_valid_pcm_and_volume_never_clips() {
        for style in SoundStyle::ALL {
            for cue in [Cue::Click, Cue::Success, Cue::Error] {
                let pcm = clip(style, cue);
                assert!(!pcm.is_empty() && pcm.len() <= 13230 && pcm.len().is_multiple_of(2));
                assert!(pcm.iter().any(|&byte| byte != 0));
                assert!(
                    pcm.as_chunks::<2>()
                        .0
                        .iter()
                        .all(|&sample| i16::from_le_bytes(sample).unsigned_abs() <= 2400)
                );
                assert_eq!(pcm[..2], [0, 0]);
                assert_eq!(pcm[pcm.len() - 2..], [0, 0]);
                let mean_square = pcm
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&sample| f64::from(i16::from_le_bytes(sample)).powi(2))
                    .sum::<f64>()
                    / (pcm.len() / 2) as f64;
                assert!(
                    mean_square.sqrt() < 1100.0,
                    "Recording is too loud on average"
                );
                let full = wave(pcm, 100);
                assert_eq!(&full[44..], pcm);
                assert_eq!(&full[..4], b"RIFF");
                assert_eq!(
                    u32::from_le_bytes(full[40..44].try_into().unwrap()) as usize,
                    pcm.len()
                );
                assert!(wave(pcm, 0)[44..].iter().all(|&byte| byte == 0));
                assert_eq!(wave(pcm, 255), full);
            }
        }
    }

    #[test]
    fn output_limiter_bounds_even_full_scale_input_and_invalid_volume() {
        let pcm: Vec<_> = [i16::MIN, -16000, 0, 16000, i16::MAX]
            .into_iter()
            .flat_map(i16::to_le_bytes)
            .collect();
        for volume in 0..=u8::MAX {
            let output = wave(&pcm, volume);
            assert!(
                output[44..]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .all(|&sample| i16::from_le_bytes(sample).unsigned_abs() <= 2800)
            );
        }
        assert_eq!(wave(&pcm, 100), wave(&pcm, u8::MAX));
    }
}
