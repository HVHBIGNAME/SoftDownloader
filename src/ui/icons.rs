//! Lazily loads real program icons and keeps them in the egui texture cache.
//!
//! Extraction happens on a background thread and only for programs the user can
//! actually see, so a large inventory never blocks the interface.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, ColorImage, Context, TextureHandle, TextureOptions, Vec2};

use crate::system::icons;

/// Requested icon size. Cards render at up to 64 pixels, so one extraction per
/// executable covers every place the icon is drawn.
const ICON_SIZE: u32 = 64;
const MAX_TEXTURES: usize = 512;

/// Shared by every card so one executable is decoded only once.
pub struct IconLoader {
    loading: HashSet<PathBuf>,
    textures: HashMap<PathBuf, TextureHandle>,
    finished: Receiver<(PathBuf, Option<icons::IconImage>)>,
    requests: Sender<PathBuf>,
}

impl IconLoader {
    pub fn new() -> Self {
        let (requests, inbox) = channel::<PathBuf>();
        let (done, finished) = channel::<(PathBuf, Option<icons::IconImage>)>();
        // A detached worker keeps icon decoding off the render thread. When it
        // exits, pending requests simply never complete and stay as placeholders.
        let _ = std::thread::Builder::new()
            .name("icon-loader".into())
            .spawn(move || {
                while let Ok(path) = inbox.recv() {
                    let image = icons::from_executable(&path, ICON_SIZE);
                    if done.send((path, image)).is_err() {
                        break;
                    }
                }
            });
        Self {
            loading: HashSet::new(),
            textures: HashMap::new(),
            finished,
            requests,
        }
    }

    fn collect(&mut self, ctx: &Context) {
        while let Ok((path, image)) = self.finished.try_recv() {
            self.loading.remove(&path);
            if let Some(image) = image {
                let side = image.size as usize;
                let pixels = ColorImage::from_rgba_unmultiplied([side, side], &image.rgba);
                let handle = ctx.load_texture(
                    format!("program:{}", path.display()),
                    pixels,
                    TextureOptions::LINEAR,
                );
                if self.textures.len() >= MAX_TEXTURES {
                    self.textures.clear();
                }
                self.textures.insert(path, handle);
            }
        }
    }

    /// Draws the icon of `path`, scheduling extraction when it is missing.
    ///
    /// Returns `true` when an icon was painted and the caller should skip its
    /// placeholder tile.
    pub fn show(&mut self, ui: &mut egui::Ui, path: Option<&Path>, size: f32) -> bool {
        let Some(path) = path.map(Path::to_owned) else {
            return false;
        };
        self.collect(ui.ctx());
        if let Some(texture) = self.textures.get(&path) {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
            egui::Image::from_texture(texture)
                .fit_to_exact_size(Vec2::splat(size))
                .paint_at(ui, rect);
            return true;
        }
        if self.loading.insert(path.to_owned()) && self.requests.send(path.to_owned()).is_err() {
            self.loading.remove(&path);
        }
        false
    }

    /// Number of decoded icons currently held in the egui texture cache.
    #[cfg(test)]
    pub fn cached(&self) -> usize {
        self.textures.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_executable_never_produces_a_texture() {
        let mut loader = IconLoader::new();
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(!loader.show(ui, None, 44.0));
                assert!(
                    !loader.show(ui, Some(Path::new(r"C:\missing\program.exe")), 44.0),
                    "an unreadable file keeps the placeholder tile"
                );
            });
        });
        assert_eq!(loader.cached(), 0);
    }
}
