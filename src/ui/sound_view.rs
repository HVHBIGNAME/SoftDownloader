use eframe::egui::{self, Align, Layout, RichText};

use super::app::SoftDownloaderApp;
use super::sounds::Cue;
use super::theme;
use crate::preferences::{SoundSettings, SoundStyle};

impl SoftDownloaderApp {
    pub(super) fn sound_settings(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        let previous = self.settings.sound;
        colors.card_frame().show(ui, |ui| {
            ui.set_width((ui.available_width() - 2.0).max(150.0));
            ui.label(RichText::new("Звуки интерфейса").size(20.0).strong());
            ui.checkbox(&mut self.settings.sound.enabled, "Включить звуковой отклик");
            ui.label(RichText::new("Короткие мягкие касания с ограниченной пиковой громкостью. Результат и ошибка обозначаются двумя тихими щелчками.").size(12.0).color(colors.muted));
            ui.add_space(8.0);
            ui.add_enabled_ui(self.settings.sound.enabled, |ui| {
                theme::percent_slider(ui, &mut self.settings.sound.volume, 0..=100, "Громкость");
                ui.add_space(8.0);
                for style in SoundStyle::ALL {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.selectable_value(&mut self.settings.sound.style, style, RichText::new(style.label()).strong());
                            ui.label(RichText::new(style.description()).size(12.0).color(colors.muted));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.add_enabled(self.settings.sound.volume > 0, egui::Button::new("Прослушать")).clicked() {
                                self.sounds.play(SoundSettings { style, ..self.settings.sound }, Cue::Click);
                            }
                        });
                    });
                    ui.add_space(8.0);
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Сигнал завершения").clicked() { self.sounds.play(self.settings.sound, Cue::Success); }
                    if ui.button("Сигнал ошибки").clicked() { self.sounds.play(self.settings.sound, Cue::Error); }
                });
            });
            if let Some(error) = &self.sounds.error {
                ui.label(RichText::new(error).size(12.0).color(colors.orange));
            }
            ui.add_space(8.0);
            ui.hyperlink_to("Звуки Kenney · CC0 и другие материалы оформления", "https://github.com/HVHBIGNAME/SoftDownloader/blob/main/assets/CREDITS.md");
        });
        if previous != self.settings.sound && !self.save_preferences() {
            self.settings.sound = previous;
        }
    }
}
