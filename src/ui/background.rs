use std::time::{Duration, Instant};

use anyhow::Result;
use eframe::egui::{self, Color32, Rect};

use super::{theme, video_player::VideoPlayer};
use crate::preferences::{BackgroundSettings, Season};

pub(super) struct Background {
    pub video: VideoPlayer,
    pumpkin: egui::TextureHandle,
    atmosphere: super::atmosphere::Atmosphere,
    clock: f32,
    last_tick: Instant,
    animate: bool,
}

impl Background {
    pub fn new(ctx: &egui::Context) -> Result<Self> {
        let png = eframe::icon_data::from_png_bytes(include_bytes!(
            "../../assets/themes/halloween-scene.png"
        ))?;
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [png.width as usize, png.height as usize],
            &png.rgba,
        );
        Ok(Self {
            video: VideoPlayer::default(),
            pumpkin: ctx.load_texture("seasonal-pumpkin", image, egui::TextureOptions::LINEAR),
            atmosphere: super::atmosphere::Atmosphere::new(ctx),
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

    pub fn paint(
        &self,
        ctx: &egui::Context,
        settings: &BackgroundSettings,
        season: Option<Season>,
        weather: Option<crate::weather::Conditions>,
    ) {
        let rect = ctx.content_rect();
        let colors = theme::colors(ctx);
        let painter = ctx
            .layer_painter(egui::LayerId::background())
            .with_clip_rect(rect);
        painter.rect_filled(rect, 0, colors.bg);
        if self.video.has_image() {
            let tint = if weather.is_some_and(|scene| !scene.day) {
                Color32::from_rgb(157, 181, 219)
            } else {
                Color32::WHITE
            };
            self.video
                .paint(&painter, rect, tint.gamma_multiply(colors.video_opacity));
        }
        if let Some(scene) = weather {
            self.atmosphere.paint(
                &painter,
                rect,
                self.clock,
                scene,
                if self.video.texture.is_some() {
                    0.18
                } else {
                    0.40
                },
            );
            if self.animate {
                ctx.request_repaint_after(Duration::from_millis(33));
            }
        } else if settings.effects {
            match season {
                Some(Season::Winter) => self.snow(&painter, rect, colors),
                Some(Season::Halloween) => self.halloween(&painter, rect, colors),
                None => {}
            }
            if season.is_some() && self.animate {
                ctx.request_repaint_after(Duration::from_millis(33));
            }
        }
    }

    fn snow(&self, painter: &egui::Painter, rect: Rect, colors: theme::Palette) {
        let scene = crate::weather::Conditions::demo(crate::weather::WeatherKind::Snow);
        self.atmosphere.paint(
            painter,
            rect,
            self.clock,
            scene,
            if colors.dark { 0.6 } else { 0.4 },
        );
    }

    fn halloween(&self, painter: &egui::Painter, rect: Rect, colors: theme::Palette) {
        if !self.video.has_image() {
            let uv = super::video_player::cover_uv(self.pumpkin.size_vec2(), rect.size())
                .shrink(0.012 + 0.008 * (self.clock * 0.10).sin());
            let flicker =
                0.98 + 0.015 * (self.clock * 2.1).sin() + 0.005 * (self.clock * 4.7).sin();
            painter.image(
                self.pumpkin.id(),
                rect,
                uv,
                Color32::WHITE.gamma_multiply(colors.video_opacity.max(0.20) * flicker),
            );
        }
        self.atmosphere.fog(painter, rect, self.clock, 0.16);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preferences::Appearance;
    use eframe::egui::{Pos2, Vec2};

    #[test]
    fn wallpaper_covers_the_viewport_independently_of_panels_and_scroll() {
        for size in [Vec2::new(1280.0, 840.0), Vec2::new(1024.0, 700.0)] {
            for details in [false, true] {
                for scroll in [0.0, 480.0] {
                    let ctx = egui::Context::default();
                    let screen = Rect::from_min_size(Pos2::ZERO, size);
                    let mut background = Background::new(&ctx).unwrap();
                    let texture = ctx.load_texture(
                        "wallpaper-test",
                        egui::ColorImage::filled([2, 2], Color32::WHITE),
                        egui::TextureOptions::LINEAR,
                    );
                    let id = texture.id();
                    background.video.texture = Some(texture);
                    let settings = BackgroundSettings {
                        effects: false,
                        ..Default::default()
                    };
                    let mut theme =
                        theme::ThemeTransition::new(&ctx, Appearance::default(), None, true);
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        |ctx| {
                            theme.update(ctx, Appearance::default(), None, true, Some(72));
                            background.paint(ctx, &settings, None, None);
                            let colors = theme::colors(ctx);
                            egui::SidePanel::left("nav")
                                .exact_width(224.0)
                                .frame(egui::Frame::new().fill(colors.panel_fill()))
                                .show(ctx, |_| {});
                            egui::TopBottomPanel::bottom("selection")
                                .exact_height(76.0)
                                .show(ctx, |_| {});
                            if details {
                                egui::SidePanel::right("details")
                                    .exact_width(308.0)
                                    .show(ctx, |_| {});
                            }
                            egui::CentralPanel::default().frame(egui::Frame::NONE).show(
                                ctx,
                                |ui| {
                                    egui::ScrollArea::vertical()
                                        .vertical_scroll_offset(scroll)
                                        .show(ui, |ui| {
                                            colors.card_frame().show(ui, |ui| {
                                                ui.set_min_height(2000.0);
                                            });
                                        });
                                },
                            );
                        },
                    );
                    let wallpaper: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| {
                            if let egui::Shape::Mesh(mesh) = &shape.shape
                                && mesh.texture_id == id
                            {
                                Some((shape.clip_rect, mesh.calc_bounds()))
                            } else {
                                None
                            }
                        })
                        .collect();
                    assert_eq!(
                        wallpaper,
                        vec![(screen, screen)],
                        "size={size:?}, details={details}, scroll={scroll}"
                    );
                }
            }
        }
    }
}
