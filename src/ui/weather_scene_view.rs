use eframe::egui::{self, Align2, Color32, FontId, RichText, Vec2};

use super::{app::SoftDownloaderApp, sky, theme};
use crate::weather::{Conditions, Lighting, WeatherKind};

impl SoftDownloaderApp {
    pub(super) fn weather_showcase(&mut self, ui: &mut egui::Ui) {
        let Some(scene) = self.weather.visual(&self.settings.weather) else {
            return;
        };
        let colors = theme::colors(ui.ctx());
        colors.card_frame().show(ui, |ui| {
            ui.set_width((ui.available_width() - 2.0).max(150.0));
            ui.spacing_mut().item_spacing.y = 8.0;
            let scene_height = if ui.ctx().content_rect().height() < 760.0 {
                180.0
            } else {
                230.0
            };
            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width(), scene_height),
                egui::Sense::hover(),
            );
            self.background.paint_weather(ui.painter(), rect, scene);
            sky::gradient(
                ui.painter(),
                rect,
                &[
                    (0.0, Color32::from_black_alpha(85)),
                    (0.35, Color32::TRANSPARENT),
                    (0.60, Color32::from_black_alpha(120)),
                    (1.0, Color32::from_black_alpha(215)),
                ],
            );
            let demo = self.weather.preview.is_some();
            let city = if demo {
                "ДЕМО · атмосферная сцена".into()
            } else {
                self.settings
                    .weather
                    .city
                    .as_ref()
                    .map_or_else(String::new, |city| city.name.clone())
            };
            let badge = egui::Rect::from_min_size(
                rect.min + Vec2::splat(14.0),
                Vec2::new(260.0_f32.min(rect.width() - 28.0), 30.0),
            );
            ui.painter()
                .rect_filled(badge, 9, Color32::from_black_alpha(165));
            ui.painter().with_clip_rect(badge).text(
                badge.left_center() + Vec2::new(10.0, 0.0),
                Align2::LEFT_CENTER,
                city,
                FontId::proportional(13.0),
                Color32::WHITE,
            );
            let title = if demo {
                scene.conditions.kind.label().into()
            } else {
                format!("{:.0}°", scene.conditions.temperature)
            };
            ui.painter().text(
                rect.left_bottom() + Vec2::new(22.0, -52.0),
                Align2::LEFT_BOTTOM,
                title,
                FontId::proportional(48.0),
                Color32::WHITE,
            );
            ui.painter().text(
                rect.left_bottom() + Vec2::new(24.0, -22.0),
                Align2::LEFT_BOTTOM,
                format!(
                    "{} · ветер {:.0} км/ч",
                    scene.conditions.kind.label(),
                    scene.conditions.wind_kmh
                ),
                FontId::proportional(14.0),
                Color32::WHITE,
            );
            let manual = if self.settings.weather.lighting == Lighting::Automatic {
                ""
            } else {
                " · вручную"
            };
            ui.painter().text(
                rect.right_bottom() + Vec2::new(-22.0, -22.0),
                Align2::RIGHT_BOTTOM,
                format!("{}{manual}", scene.sunlight.phase.label()),
                FontId::proportional(14.0),
                Color32::WHITE,
            );
            ui.add_space(10.0);
            self.weather_lighting_controls(ui);
            self.weather_demo_controls(ui);
            self.weather_motion_controls(ui);
        });
        ui.add_space(12.0);
    }

    pub(super) fn weather_lighting_controls(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("Освещение")
                .size(13.0)
                .color(theme::colors(ui.ctx()).muted),
        );
        ui.horizontal_wrapped(|ui| {
            for mode in Lighting::ALL {
                if theme::filter_chip(ui, mode.label(), self.settings.weather.lighting == mode)
                    .clicked()
                {
                    self.settings.weather.lighting = mode;
                    if self.weather.scene(&self.settings.weather).is_none() {
                        self.weather.reveal_scene = true;
                        self.weather.preview = Some(Conditions::demo(WeatherKind::Clear));
                    }
                }
            }
        });
        ui.label(RichText::new("Авто — по солнцу в вашем городе, а не часовому поясу ПК. Ручные режимы меняют только оформление.").size(12.0).color(theme::colors(ui.ctx()).muted));
    }

    pub(super) fn weather_demo_controls(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        ui.label(
            RichText::new("Демонстрация погоды")
                .size(13.0)
                .color(colors.muted),
        );
        let columns = if ui.available_width() >= 600.0 { 5 } else { 3 };
        let width = (ui.available_width() - (columns - 1) as f32 * 8.0) / columns as f32;
        egui::Grid::new("weather-demo-choices")
            .num_columns(columns)
            .spacing(Vec2::splat(8.0))
            .show(ui, |ui| {
                for (index, kind) in WeatherKind::ALL.into_iter().enumerate() {
                    if ui
                        .add_sized(
                            Vec2::new(width, 38.0),
                            theme::chip(
                                kind.label(),
                                self.weather.preview.is_some_and(|p| p.kind == kind),
                                colors,
                            ),
                        )
                        .clicked()
                    {
                        self.weather.reveal_scene |=
                            self.weather.scene(&self.settings.weather).is_none();
                        self.weather.preview = Some(Conditions::demo(kind));
                    }
                    if (index + 1) % columns == 0 {
                        ui.end_row();
                    }
                }
            });
        if self.weather.preview.is_some()
            && ui
                .button(if self.settings.weather.city.is_some() {
                    "Вернуть погоду города"
                } else {
                    "Закрыть демо"
                })
                .clicked()
        {
            self.weather.preview = None;
        }
    }

    pub(super) fn weather_motion_controls(&mut self, ui: &mut egui::Ui) {
        let stopped = self.settings.background.paused || self.settings.reduced_motion;
        ui.horizontal_wrapped(|ui| {
            let colors = theme::colors(ui.ctx());
            ui.label(
                RichText::new(if self.settings.reduced_motion {
                    "Движение уменьшено в настройках"
                } else if stopped {
                    "Анимация на паузе"
                } else {
                    "● Анимация включена"
                })
                .size(12.0)
                .color(if stopped {
                    colors.orange
                } else {
                    colors.accent
                }),
            );
            if ui
                .button(if stopped {
                    "Включить анимацию"
                } else {
                    "Пауза"
                })
                .clicked()
            {
                let previous = (
                    self.settings.background.paused,
                    self.settings.reduced_motion,
                );
                self.settings.background.paused = !stopped;
                if stopped {
                    self.settings.reduced_motion = false;
                }
                if !self.save_preferences() {
                    (
                        self.settings.background.paused,
                        self.settings.reduced_motion,
                    ) = previous;
                }
            }
        });
    }
}
