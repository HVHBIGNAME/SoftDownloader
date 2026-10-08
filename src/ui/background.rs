use std::time::{Duration, Instant};

use anyhow::Result;
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};

use super::{theme, video_player::VideoPlayer};
use crate::preferences::{BackgroundSettings, Season};

pub(super) struct Background {
    pub video: VideoPlayer,
    pumpkin: egui::TextureHandle,
    clock: f32,
    last_tick: Instant,
    animate: bool,
}

impl Background {
    pub fn new(ctx: &egui::Context) -> Result<Self> {
        let png =
            eframe::icon_data::from_png_bytes(include_bytes!("../../assets/themes/pumpkin.png"))?;
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [png.width as usize, png.height as usize],
            &png.rgba,
        );
        Ok(Self {
            video: VideoPlayer::default(),
            pumpkin: ctx.load_texture("seasonal-pumpkin", image, egui::TextureOptions::LINEAR),
            clock: 0.0,
            last_tick: Instant::now(),
            animate: false,
        })
    }

    pub fn update(&mut self, ctx: &egui::Context, settings: &BackgroundSettings, reduced: bool) {
        self.animate = !reduced
            && !settings.paused
            && ctx.input(|i| i.focused && !i.viewport().minimized.unwrap_or(false));
        let now = Instant::now();
        if self.animate {
            self.clock += now.duration_since(self.last_tick).as_secs_f32().min(0.10);
        }
        self.last_tick = now;
        self.video
            .update(ctx, settings.video.as_deref(), self.animate);
    }

    pub fn paint(&self, ui: &egui::Ui, settings: &BackgroundSettings, season: Option<Season>) {
        let rect = ui.max_rect();
        let colors = theme::colors(ui.ctx());
        let painter = ui.painter();
        if self.video.texture.is_some() {
            let opacity = f32::from(100 - settings.dimming.clamp(45, 95)) / 100.0;
            self.video
                .paint(painter, rect, Color32::WHITE.gamma_multiply(opacity));
            let top = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + 180.0));
            painter.rect_filled(top, 0, colors.bg.gamma_multiply(0.94));
            let fade = Rect::from_min_max(
                top.left_bottom(),
                Pos2::new(rect.right(), rect.top() + 340.0),
            );
            let mut mesh = egui::Mesh::default();
            for (position, color) in [
                (fade.left_top(), colors.bg.gamma_multiply(0.94)),
                (fade.right_top(), colors.bg.gamma_multiply(0.94)),
                (fade.right_bottom(), Color32::TRANSPARENT),
                (fade.left_bottom(), Color32::TRANSPARENT),
            ] {
                mesh.colored_vertex(position, color);
            }
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            painter.add(egui::Shape::mesh(mesh));
        }
        if settings.effects {
            match season {
                Some(Season::Winter) => self.snow(painter, rect, colors),
                Some(Season::Halloween) => self.halloween(painter, rect, colors),
                None => {}
            }
            if season == Some(Season::Winter) && self.animate {
                ui.ctx().request_repaint_after(Duration::from_millis(33));
            }
        }
    }

    fn snow(&self, painter: &egui::Painter, rect: Rect, colors: theme::Palette) {
        for index in 0..64 {
            let seed = (index * 1973 + 9277) as f32;
            let depth = (seed.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
            let x = ((seed * 0.618).fract() * rect.width()
                + (self.clock * 0.35 + seed).sin() * 14.0)
                .rem_euclid(rect.width().max(1.0));
            let y = ((seed * 0.317).fract() * rect.height() + self.clock * (12.0 + depth * 20.0))
                .rem_euclid(rect.height().max(1.0));
            let color = if colors.dark {
                Color32::from_rgb(207, 229, 250)
            } else {
                Color32::from_rgb(66, 126, 166)
            };
            painter.circle_filled(
                rect.min + Vec2::new(x, y),
                1.0 + depth * 1.8,
                color.gamma_multiply(0.16 + depth * 0.25),
            );
        }
    }

    fn halloween(&self, painter: &egui::Painter, rect: Rect, colors: theme::Palette) {
        let size = Vec2::splat(64.0);
        let anchor = if rect.width() > 640.0 {
            Pos2::new(rect.left() + rect.width() * 0.64, rect.top() + 28.0)
        } else {
            rect.right_bottom() - size * 0.65
        };
        let center = anchor + Vec2::new(0.0, (self.clock * 0.7).sin() * 3.0);
        let glow = 0.025 + 0.008 * (self.clock * 1.2).sin();
        painter.circle_filled(center, 48.0, colors.orange.gamma_multiply(glow));
        painter.image(
            self.pumpkin.id(),
            Rect::from_center_size(center, size),
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE.gamma_multiply(if colors.dark { 0.36 } else { 0.24 }),
        );
    }
}
