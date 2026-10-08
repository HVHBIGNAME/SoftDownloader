use eframe::egui::{self, Align, Layout, RichText, Vec2};

use super::app::SoftDownloaderApp;
use super::theme;

impl SoftDownloaderApp {
    pub(super) fn receive_background(
        &mut self,
        result: Result<Option<std::path::PathBuf>, String>,
    ) {
        self.background_busy = false;
        self.background_picking = false;
        self.background_progress = None;
        match result {
            Ok(Some(path)) => {
                let previous = self.settings.background.clone();
                self.settings.background.video = Some(path);
                self.settings.background.paused = false;
                if !self.save_preferences() {
                    self.settings.background = previous;
                }
                self.background_error = None;
            }
            Ok(None) => {}
            Err(error) => self.background_error = Some(error),
        }
    }

    pub(super) fn background_settings(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        let previous = self.settings.background.clone();
        colors.card_frame().show(ui, |ui| {
            ui.set_width((ui.available_width() - 2.0).max(150.0));
            ui.label(RichText::new("Видеофон").size(20.0).strong());
            ui.label(RichText::new("Настоящее видео из файла. Воспроизводится по кругу, без звука, средствами Windows.").size(12.0).color(colors.muted));
            ui.horizontal_wrapped(|ui| {
                let enabled = !self.background_busy && !self.list_busy && !self.has_modal();
                if ui.add_enabled(enabled, egui::Button::new("Выбрать свой MP4…")).clicked() {
                    self.background_busy = true;
                    self.background_picking = true;
                    self.background_error = None;
                    self.engine.choose_background();
                }
                if ui.add_enabled(enabled && self.settings.background.video.is_some(), egui::Button::new("Убрать видео")).clicked() {
                    self.settings.background.video = None;
                    self.background_error = None;
                }
            });
            if let Some(path) = &self.settings.background.video {
                ui.add(egui::Label::new(RichText::new(path.file_name().unwrap_or_default().to_string_lossy()).size(12.0).color(colors.muted)).truncate()).on_hover_text(path.display().to_string());
            }
            if self.background.video.texture.is_some() {
                let width = ui.available_width().min(620.0);
                let (preview, _) = ui.allocate_exact_size(Vec2::new(width, width * 9.0 / 16.0), egui::Sense::hover());
                self.background.video.paint(ui.painter(), preview, egui::Color32::WHITE);
            }
            ui.horizontal_wrapped(|ui| {
                theme::percent_slider(ui, &mut self.settings.background.dimming, 45..=95, "Приглушение");
                ui.checkbox(&mut self.settings.background.paused, "Пауза фона и эффектов");
            });
            ui.label(RichText::new("Видео и эффекты приостанавливаются в неактивном окне и при уменьшении анимации. Ваши файлы остаются на компьютере.").size(12.0).color(colors.muted));
        });
        ui.add_space(16.0);
        self.background_downloads(ui);
        if previous != self.settings.background && !self.save_preferences() {
            self.settings.background = previous;
        }
    }

    fn background_downloads(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        ui.label(RichText::new("Готовые ролики").size(18.0).strong());
        for preset in self.background_presets.clone() {
            colors.card_frame().show(ui, |ui| {
                ui.set_width((ui.available_width() - 2.0).max(150.0));
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&preset.name).strong());
                        ui.label(
                            RichText::new(&preset.description)
                                .size(12.0)
                                .color(colors.muted),
                        );
                        ui.hyperlink_to("Источник и лицензия · Mixkit", &preset.source);
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                !self.background_busy && !self.list_busy && !self.has_modal(),
                                egui::Button::new("Скачать и включить"),
                            )
                            .clicked()
                        {
                            self.background_busy = true;
                            self.background_progress = None;
                            self.background_error = None;
                            self.engine
                                .download_background(preset.clone(), self.store.clone());
                        }
                    });
                });
            });
            ui.add_space(8.0);
        }
        if self.background_busy {
            ui.horizontal(|ui| {
                ui.spinner();
                if let Some(progress) = &self.background_progress {
                    ui.label(format!(
                        "{} / {}",
                        theme::bytes(progress.downloaded),
                        theme::bytes(progress.total)
                    ));
                } else {
                    ui.label("Подготавливаем видео…");
                }
                if !self.background_picking && ui.button("Отменить загрузку").clicked()
                {
                    self.engine.cancel_background();
                }
            });
        }
        if let Some(error) = self
            .background_error
            .as_ref()
            .or(self.background.video.error.as_ref())
        {
            ui.label(RichText::new(error).color(colors.orange).size(12.0));
            if self.background.video.error.is_some()
                && ui.button("Повторить открытие видео").clicked()
            {
                self.background.video.restart();
            }
        }
    }
}
