use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver, SyncSender, TrySendError},
};
use std::time::{Duration, Instant};

use anyhow::{Result, ensure};
use eframe::egui::{self, Color32, Pos2, Rect, TextureHandle};

use crate::video::{Decoder, VideoFrame};

struct Worker {
    frames: Receiver<Result<VideoFrame, String>>,
    stopped: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
    }
}

#[derive(Default)]
pub(super) struct VideoPlayer {
    source: Option<PathBuf>,
    worker: Option<Worker>,
    pub texture: Option<TextureHandle>,
    previous: Option<TextureHandle>,
    blend_started: Option<Instant>,
    pub error: Option<String>,
}

impl VideoPlayer {
    pub fn update(&mut self, ctx: &egui::Context, source: Option<&Path>, animate: bool) {
        if self.source.as_deref() != source {
            self.worker = None;
            self.source = source.map(Path::to_owned);
            self.error = None;
            self.previous = if source.is_some() {
                self.texture.take().or(self.previous.take())
            } else {
                self.texture = None;
                None
            };
            self.blend_started = None;
            if let Some(path) = source {
                match spawn(path.to_owned(), ctx.clone()) {
                    Ok(worker) => self.worker = Some(worker),
                    Err(error) => {
                        self.error = Some(format!("{error:#}"));
                        self.previous = None;
                    }
                }
            }
        }
        if let Some(worker) = &self.worker {
            worker.paused.store(!animate, Ordering::Relaxed);
            for result in worker.frames.try_iter() {
                match result {
                    Ok(frame) => {
                        if self.previous.is_some() {
                            self.blend_started.get_or_insert_with(Instant::now);
                        }
                        let image =
                            egui::ColorImage::from_rgba_unmultiplied(frame.size, &frame.rgba);
                        if let Some(texture) = &mut self.texture {
                            texture.set(image, egui::TextureOptions::LINEAR);
                        } else {
                            self.texture = Some(ctx.load_texture(
                                "background-video",
                                image,
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                    Err(error) => {
                        self.error = Some(error);
                        self.previous = None;
                        self.blend_started = None;
                    }
                }
            }
        }
        if (!animate && self.texture.is_some())
            || self
                .blend_started
                .is_some_and(|at| at.elapsed() >= Duration::from_millis(650))
        {
            self.previous = None;
            self.blend_started = None;
        }
    }

    pub fn has_image(&self) -> bool {
        self.texture.is_some() || self.previous.is_some()
    }

    pub fn restart(&mut self) {
        self.worker = None;
        self.source = None;
    }

    pub fn paint(&self, painter: &egui::Painter, rect: Rect, tint: Color32) {
        let blend = if self.previous.is_some() {
            self.blend_started
                .map_or(0.0, |at| (at.elapsed().as_secs_f32() / 0.65).min(1.0))
        } else {
            1.0
        };
        if let Some(previous) = &self.previous {
            painter.image(
                previous.id(),
                rect,
                cover_uv(previous.size_vec2(), rect.size()),
                tint.gamma_multiply(1.0 - blend),
            );
        }
        if let Some(texture) = &self.texture {
            painter.image(
                texture.id(),
                rect,
                cover_uv(texture.size_vec2(), rect.size()),
                tint.gamma_multiply(blend),
            );
        }
    }
}

fn spawn(path: PathBuf, ctx: egui::Context) -> Result<Worker> {
    let (sender, frames) = mpsc::sync_channel(1);
    let stopped = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    let (stop, pause) = (stopped.clone(), paused.clone());
    std::thread::Builder::new()
        .name("video-background".into())
        .spawn(move || {
            if let Err(error) = play(&path, &stop, &pause, &sender, &ctx)
                && sender.send(Err(format!("{error:#}"))).is_ok()
            {
                ctx.request_repaint();
            }
        })?;
    Ok(Worker {
        frames,
        stopped,
        paused,
    })
}

fn play(
    path: &Path,
    stopped: &AtomicBool,
    paused: &AtomicBool,
    sender: &SyncSender<Result<VideoFrame, String>>,
    ctx: &egui::Context,
) -> Result<()> {
    let mut decoder = Decoder::open(path)?;
    let mut origin = None;
    let mut last_presented = None;
    let mut loop_frames = 0;
    while !stopped.load(Ordering::Relaxed) {
        if loop_frames > 0 && paused.load(Ordering::Relaxed) {
            origin = None;
            std::thread::sleep(Duration::from_millis(50));
            continue;
        }
        let Some(frame) = decoder.next_frame()? else {
            ensure!(loop_frames > 0, "В файле нет видеокадров");
            decoder.rewind()?;
            origin = None;
            last_presented = None;
            loop_frames = 0;
            continue;
        };
        loop_frames += 1;
        let (media_start, wall_start) = *origin.get_or_insert((frame.timestamp, Instant::now()));
        let due = frame.timestamp.saturating_sub(media_start);
        ensure!(
            due < Duration::from_secs(86400),
            "Некорректные временные метки видео"
        );
        while let Some(remaining) = due.checked_sub(wall_start.elapsed()) {
            if stopped.load(Ordering::Relaxed) {
                return Ok(());
            }
            if paused.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(remaining.min(Duration::from_millis(10)));
        }
        if last_presented
            .is_some_and(|last| frame.timestamp.saturating_sub(last) < Duration::from_millis(33))
        {
            continue;
        }
        last_presented = Some(frame.timestamp);
        match sender.try_send(Ok(frame)) {
            Ok(()) => ctx.request_repaint(),
            Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => break,
        }
    }
    Ok(())
}

pub(super) fn cover_uv(image: egui::Vec2, target: egui::Vec2) -> Rect {
    let ratio = (target.x / target.y.max(1.0)) / (image.x / image.y.max(1.0));
    let size = if ratio > 1.0 {
        egui::vec2(1.0, 1.0 / ratio)
    } else {
        egui::vec2(ratio, 1.0)
    };
    Rect::from_center_size(Pos2::new(0.5, 0.5), size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paused_transition_keeps_previous_image_until_the_new_frame_arrives() {
        let ctx = egui::Context::default();
        let texture = ctx.load_texture(
            "previous-scene",
            egui::ColorImage::filled([1, 1], Color32::WHITE),
            egui::TextureOptions::LINEAR,
        );
        let (sender, frames) = mpsc::sync_channel(1);
        let source = PathBuf::from("pending-scene.mp4");
        let mut player = VideoPlayer {
            source: Some(source.clone()),
            previous: Some(texture),
            worker: Some(Worker {
                frames,
                stopped: Arc::new(AtomicBool::new(false)),
                paused: Arc::new(AtomicBool::new(false)),
            }),
            ..Default::default()
        };
        player.update(&ctx, Some(&source), false);
        assert!(player.has_image());
        assert!(player.previous.is_some() && player.texture.is_none());
        sender
            .send(Ok(VideoFrame {
                size: [1, 1],
                rgba: vec![10, 20, 30, 255],
                timestamp: Duration::ZERO,
            }))
            .unwrap();
        player.update(&ctx, Some(&source), false);
        assert!(player.has_image());
        assert!(player.previous.is_none() && player.texture.is_some());
        player.update(&ctx, None, false);
        assert!(!player.has_image());
    }

    #[test]
    fn portrait_and_landscape_clips_cover_without_stretching() {
        let portrait = cover_uv(egui::vec2(720.0, 1280.0), egui::vec2(1000.0, 700.0));
        assert_eq!(portrait.width(), 1.0);
        assert!(portrait.height() < 1.0 && portrait.min.y > 0.0);
        let wide = cover_uv(egui::vec2(1920.0, 1080.0), egui::vec2(700.0, 1000.0));
        assert_eq!(wide.height(), 1.0);
        assert!(wide.width() < 1.0 && wide.min.x > 0.0);
    }
}
