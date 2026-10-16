use std::time::{Duration, Instant};

use anyhow::Result;
use eframe::egui::{self, Color32, Rect};

use super::weather_transition::{Motion, WeatherMix, WeatherTransition};
use super::{theme, video_player::VideoPlayer};
use crate::preferences::{BackgroundSettings, Season};

pub(super) struct Background {
    pub video: VideoPlayer,
    pumpkin: egui::TextureHandle,
    atmosphere: super::atmosphere::Atmosphere,
    clock: f32,
    last_tick: Instant,
    last_interaction: Instant,
    animate: bool,
    sunlight: Option<crate::weather::Sunlight>,
    frame_interval: Duration,
    weather: Option<WeatherTransition>,
    motion: Motion,
    detail: f32,
    cpu_ms: f32,
    cpu_limited: bool,
    busy: bool,
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
            last_interaction: Instant::now(),
            animate: false,
            sunlight: None,
            frame_interval: Duration::from_millis(33),
            weather: None,
            motion: Motion::default(),
            detail: 1.0,
            cpu_ms: 0.0,
            cpu_limited: false,
            busy: false,
        })
    }

    pub fn update(
        &mut self,
        ctx: &egui::Context,
        settings: &BackgroundSettings,
        reduced: bool,
        weather: Option<crate::weather::Scene>,
    ) {
        self.animate =
            !reduced && !settings.paused && ctx.input(|i| !i.viewport().minimized.unwrap_or(false));
        let now = Instant::now();
        if ctx.input(|i| !i.events.is_empty()) {
            self.last_interaction = now;
        }
        let constrained = self.busy || self.cpu_limited || ctx.input(|i| !i.focused);
        let idle = now.duration_since(self.last_interaction) >= Duration::from_secs(3);
        self.frame_interval = Duration::from_millis(if constrained || idle { 33 } else { 16 });
        let elapsed = now.duration_since(self.last_tick).as_secs_f32().min(0.10);
        if self.animate {
            self.clock += elapsed;
        }
        let detail = if constrained { 0.70 } else { 1.0 };
        self.detail = if self.animate {
            self.detail + (detail - self.detail) * (elapsed * 2.0).min(1.0)
        } else {
            detail
        };
        if let Some(scene) = weather {
            let transition = self
                .weather
                .get_or_insert_with(|| WeatherTransition::new(scene.conditions));
            transition.update(scene.conditions, elapsed, !self.animate);
            if self.animate {
                self.motion.advance(elapsed, transition.current);
            }
        } else {
            self.weather = None;
            self.motion = Motion::default();
        }
        self.sunlight = weather.map(|scene| match self.sunlight {
            Some(previous) if self.animate => {
                previous.mix(scene.sunlight, 1.0 - (-elapsed * 3.0).exp())
            }
            _ => scene.sunlight,
        });
        self.last_tick = now;
        self.video
            .update(ctx, settings.video.as_deref(), self.animate);
    }

    pub fn set_busy(&mut self, busy: bool) {
        self.busy = busy;
    }

    pub fn observe_cpu(&mut self, seconds: Option<f32>) {
        if let Some(seconds) = seconds.filter(|v| v.is_finite() && *v >= 0.0) {
            self.cpu_ms = self.cpu_ms * 0.95 + (seconds * 1000.0).min(100.0) * 0.05;
            if self.cpu_ms > 8.0 {
                self.cpu_limited = true;
            } else if self.cpu_ms < 4.0 {
                self.cpu_limited = false;
            }
        }
    }

    pub fn paint(
        &self,
        ctx: &egui::Context,
        settings: &BackgroundSettings,
        season: Option<Season>,
        weather: Option<crate::weather::Scene>,
    ) {
        let rect = ctx.content_rect();
        let colors = theme::colors(ctx);
        let painter = ctx
            .layer_painter(egui::LayerId::background())
            .with_clip_rect(rect);
        painter.rect_filled(rect, 0, colors.bg);
        if let Some(scene) = weather {
            self.paint_weather(&painter, rect, scene);
            painter.rect_filled(
                rect,
                0,
                colors.bg.gamma_multiply(1.0 - colors.video_opacity),
            );
            if self.animate {
                ctx.request_repaint_after(self.frame_interval);
            }
            return;
        }
        if self.video.has_image() {
            self.video.paint(
                &painter,
                rect,
                Color32::WHITE.gamma_multiply(colors.video_opacity),
            );
        }
        if settings.effects {
            match season {
                Some(Season::Winter) => self.snow(&painter, rect, colors),
                Some(Season::Halloween) => self.halloween(&painter, rect, colors),
                None => {}
            }
            if season.is_some() && self.animate {
                ctx.request_repaint_after(self.frame_interval);
            }
        }
    }

    pub fn paint_weather(
        &self,
        painter: &egui::Painter,
        rect: Rect,
        mut scene: crate::weather::Scene,
    ) {
        if !rect.is_positive() || !painter.clip_rect().intersects(rect) {
            return;
        }
        let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
        scene.sunlight = self.sunlight.unwrap_or(scene.sunlight);
        let mix = self
            .weather
            .as_ref()
            .map_or_else(|| WeatherMix::new(scene.conditions), |w| w.current);
        let motion = if self.weather.is_some() {
            self.motion
        } else {
            let mut motion = Motion::default();
            motion.advance(self.clock, mix);
            motion
        };
        self.atmosphere
            .sky
            .paint(&painter, rect, motion, scene.sunlight, mix);
        let light = scene.sunlight.daylight;
        if self.video.has_image() {
            let tint = Color32::from_rgb(65, 90, 145).lerp_to_gamma(Color32::WHITE, light);
            self.video
                .paint(&painter, rect, tint.gamma_multiply(0.07 + 0.17 * light));
        }
        self.atmosphere
            .paint_weather(&painter, rect, motion, mix, 0.90, self.detail);
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
    fn visible_unfocused_weather_moves_but_pause_minimize_and_reduced_motion_stop_it() {
        let ctx = egui::Context::default();
        let mut background = Background::new(&ctx).unwrap();
        let scene = crate::weather::Scene {
            conditions: crate::weather::Conditions::demo(crate::weather::WeatherKind::Rain),
            sunlight: crate::weather::Lighting::Day.resolve(0.0, 0.0, 0),
        };
        for (focused, minimized, paused, reduced, moving) in [
            (true, false, false, false, true),
            (false, false, false, false, true),
            (true, true, false, false, false),
            (false, false, true, false, false),
            (true, false, false, true, false),
        ] {
            let mut input = egui::RawInput {
                focused,
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .minimized = Some(minimized);
            let before = background.clock;
            background.last_tick = Instant::now() - Duration::from_millis(50);
            let settings = BackgroundSettings {
                paused,
                ..Default::default()
            };
            let _ = ctx.run(input, |ctx| {
                background.update(ctx, &settings, reduced, Some(scene))
            });
            assert_eq!(
                background.clock > before,
                moving,
                "focused={focused}, minimized={minimized}, paused={paused}, reduced={reduced}"
            );
        }
    }

    #[test]
    fn every_weather_kind_has_moving_geometry_and_distinct_day_night_lighting() {
        let ctx = egui::Context::default();
        let mut background = Background::new(&ctx).unwrap();
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(1024.0, 700.0));
        for kind in crate::weather::WeatherKind::ALL {
            let mut frames = Vec::new();
            for (time, mode) in [
                (0.0, crate::weather::Lighting::Day),
                (2.0, crate::weather::Lighting::Day),
                (2.0, crate::weather::Lighting::Night),
            ] {
                background.clock = time;
                let scene = crate::weather::Scene {
                    conditions: crate::weather::Conditions::demo(kind),
                    sunlight: mode.resolve(0.0, 0.0, 0),
                };
                let output = ctx.run(
                    egui::RawInput {
                        screen_rect: Some(rect),
                        ..Default::default()
                    },
                    |ctx| {
                        background.paint_weather(
                            &ctx.layer_painter(egui::LayerId::background()),
                            rect,
                            scene,
                        );
                    },
                );
                let vertices: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Mesh(mesh) => {
                            Some(mesh.vertices.iter().map(|v| (v.pos, v.color)))
                        }
                        _ => None,
                    })
                    .flatten()
                    .collect();
                assert!(
                    vertices
                        .iter()
                        .all(|(pos, _)| pos.x.is_finite() && pos.y.is_finite())
                );
                assert!(vertices.len() < 6000);
                frames.push(vertices);
            }
            assert_ne!(frames[0], frames[1], "{kind:?} must visibly move");
            assert_ne!(
                frames[1], frames[2],
                "{kind:?} must distinguish day and night"
            );
        }
    }

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
