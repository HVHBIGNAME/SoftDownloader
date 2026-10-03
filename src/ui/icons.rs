use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, channel, sync_channel};

use anyhow::Result;
use eframe::egui::{self, ColorImage, Context, TextureHandle, TextureOptions, Vec2};

use crate::system::icons;

const ICON_SIZE: u32 = 64;
const MAX_TEXTURES: usize = 512;

pub struct IconLoader {
    loading: HashSet<PathBuf>,
    missing: HashSet<PathBuf>,
    textures: HashMap<PathBuf, (TextureHandle, u64)>,
    finished: Receiver<(PathBuf, Option<icons::IconImage>)>,
    requests: SyncSender<PathBuf>,
    last_use: u64,
}

impl IconLoader {
    pub fn new(ctx: Context) -> Result<Self> {
        let (requests, inbox) = sync_channel::<PathBuf>(64);
        let (done, finished) = channel();
        std::thread::Builder::new()
            .name("icon-loader".into())
            .spawn(move || {
                while let Ok(path) = inbox.recv() {
                    let image = icons::from_executable(&path, ICON_SIZE);
                    if done.send((path, image)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            })?;
        Ok(Self {
            loading: HashSet::new(),
            missing: HashSet::new(),
            textures: HashMap::new(),
            finished,
            requests,
            last_use: 0,
        })
    }

    pub fn refresh(&mut self) {
        self.missing.clear();
    }

    pub fn collect(&mut self, ctx: &Context) {
        while let Ok((path, image)) = self.finished.try_recv() {
            self.loading.remove(&path);
            if let Some(image) = image {
                let side = image.size as usize;
                let handle = ctx.load_texture(
                    format!("program:{}", path.display()),
                    ColorImage::from_rgba_unmultiplied([side, side], &image.rgba),
                    TextureOptions::LINEAR,
                );
                if self.textures.len() >= MAX_TEXTURES {
                    let oldest = self
                        .textures
                        .iter()
                        .min_by_key(|(_, (_, used))| used)
                        .map(|(path, _)| path.clone());
                    if let Some(oldest) = oldest {
                        self.textures.remove(&oldest);
                    }
                }
                self.textures.insert(path, (handle, self.last_use));
            } else {
                self.missing.insert(path);
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, path: Option<&Path>, size: f32) -> bool {
        let Some(path) = path else {
            return false;
        };
        self.last_use += 1;
        if let Some((texture, used)) = self.textures.get_mut(path) {
            *used = self.last_use;
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
            egui::Image::from_texture(&*texture)
                .fit_to_exact_size(Vec2::splat(size))
                .paint_at(ui, rect);
            return true;
        }
        if !self.missing.contains(path) && !self.loading.contains(path) {
            match self.requests.try_send(path.to_owned()) {
                Ok(()) => {
                    self.loading.insert(path.to_owned());
                }
                Err(TrySendError::Full(_)) => {
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(100));
                }
                Err(TrySendError::Disconnected(_)) => {
                    self.missing.insert(path.to_owned());
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_icons_are_not_requested_again_each_frame() {
        let ctx = Context::default();
        let (requests, inbox) = sync_channel(2);
        let (done, finished) = channel();
        let mut loader = IconLoader {
            loading: HashSet::new(),
            missing: HashSet::new(),
            textures: HashMap::new(),
            finished,
            requests,
            last_use: 0,
        };
        let path = Path::new("missing-program.exe");
        let render = |loader: &mut IconLoader| {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    assert!(!loader.show(ui, Some(path), 44.0));
                });
            });
        };
        render(&mut loader);
        assert_eq!(inbox.try_recv().unwrap(), path);
        done.send((path.to_owned(), None)).unwrap();
        loader.collect(&ctx);
        render(&mut loader);
        render(&mut loader);
        assert!(inbox.try_recv().is_err());
        loader.refresh();
        render(&mut loader);
        assert_eq!(inbox.try_recv().unwrap(), path);
    }
}
