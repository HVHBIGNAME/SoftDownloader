use eframe::egui::{self, Color32, RichText, Stroke, Vec2};

use super::app::SoftDownloaderApp;
use super::theme;
use crate::preferences::{Appearance, SeasonMode, Theme};

impl SoftDownloaderApp {
    pub(super) fn appearance_settings(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        let previous = (
            self.settings.appearance,
            self.settings.reduced_motion,
            self.settings.background.effects,
        );
        colors.card_frame().show(ui, |ui| {
            ui.set_width((ui.available_width() - 2.0).max(150.0));
            ui.label(RichText::new("Оформление").size(17.0).strong());
            ui.horizontal_wrapped(|ui| {
                for option in Theme::ALL {
                    ui.selectable_value(&mut self.settings.appearance.theme, option, option.label())
                        .on_hover_text(option.description());
                }
            });
            ui.add_space(4.0);
            accent_picker(ui, &mut self.settings.appearance.accent);
            ui.label(RichText::new("Цвет текста автоматически подбирается для читаемости.").size(12.0).color(colors.muted));
            ui.horizontal_wrapped(|ui| {
                ui.label("Праздничное оформление");
                egui::ComboBox::from_id_salt("season-mode")
                    .selected_text(self.settings.appearance.season.label())
                    .show_ui(ui, |ui| {
                        for option in [SeasonMode::Automatic, SeasonMode::Off, SeasonMode::Winter, SeasonMode::Halloween] {
                            ui.selectable_value(&mut self.settings.appearance.season, option, option.label());
                        }
                    });
            });
            ui.label(RichText::new("В декабре и январе — падающий снег, в октябре — тыква и мягкое свечение. Можно включить вручную в любое время.").size(12.0).color(colors.muted));
            ui.checkbox(&mut self.settings.background.effects, "Праздничные эффекты на фоне");
            ui.checkbox(&mut self.settings.reduced_motion, "Уменьшить анимацию");
            ui.label(RichText::new("Короткие переходы и плавная подсветка. Изменения применяются сразу.").size(12.0).color(colors.muted));
        });
        if previous
            != (
                self.settings.appearance,
                self.settings.reduced_motion,
                self.settings.background.effects,
            )
        {
            if !self.save_preferences() {
                (
                    self.settings.appearance,
                    self.settings.reduced_motion,
                    self.settings.background.effects,
                ) = previous;
            }
            ui.ctx().request_repaint();
        }
    }
}

fn accent_picker(ui: &mut egui::Ui, accent: &mut [u8; 3]) {
    let colors = theme::colors(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label("Акцент");
        for (name, rgb) in [
            ("Синий", Appearance::default().accent),
            ("Лайм", [188, 239, 119]),
            ("Лавандовый", [186, 169, 242]),
            ("Персиковый", [238, 178, 117]),
            ("Розовый", [240, 158, 185]),
        ] {
            let color = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            let selected = *accent == rgb;
            let response = ui
                .add(
                    egui::Button::new(" ")
                        .fill(color)
                        .stroke(Stroke::new(1.0_f32, colors.border))
                        .min_size(Vec2::splat(32.0)),
                )
                .on_hover_text(name);
            if selected {
                ui.painter()
                    .circle_filled(response.rect.center(), 2.5, theme::on_color(color));
            }
            if response.clicked() {
                *accent = rgb;
            }
        }
        ui.color_edit_button_srgb(accent)
            .on_hover_text("Свой цвет акцента");
    });
}
