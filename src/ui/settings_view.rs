use eframe::egui::{self, Align, Layout, RichText};

use super::app::SoftDownloaderApp;
use super::theme;

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(super) enum SettingsSection {
    Appearance,
    Sound,
    Background,
    Sources,
}

impl SoftDownloaderApp {
    pub(super) fn settings_page(&mut self, ui: &mut egui::Ui) {
        theme::heading(ui, "Настройки", "Внешний вид, звуки и источники программ.");
        ui.horizontal(|ui| {
            for (section, name) in [
                (SettingsSection::Appearance, "Оформление"),
                (SettingsSection::Sound, "Звуки"),
                (SettingsSection::Background, "Видеофон"),
                (SettingsSection::Sources, "Источники и данные"),
            ] {
                ui.selectable_value(&mut self.settings_section, section, name);
            }
        });
        ui.add_space(16.0);
        egui::ScrollArea::vertical()
            .id_salt(("settings", self.settings_section))
            .auto_shrink([false, false])
            .show(ui, |ui| match self.settings_section {
                SettingsSection::Appearance => self.appearance_settings(ui),
                SettingsSection::Sound => self.sound_settings(ui),
                SettingsSection::Background => self.background_settings(ui),
                SettingsSection::Sources => self.source_settings(ui),
            });
    }

    fn source_settings(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        colors.card_frame().show(ui, |ui| {
                ui.set_width((ui.available_width() - 2.0).max(150.0));
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Дополнительный софт").size(17.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| { theme::pill(ui, "GOOGLE DRIVE", colors.accent); });
                });
                ui.label(RichText::new(if self.builtin_source.is_empty() {
                    "Встроенный источник для дополнительных пакетов. Коллекция появится в одном из следующих обновлений."
                } else {
                    "Дополнительные пакеты загружаются со встроенного Google Диска и дополняют основной каталог."
                }).color(colors.muted).size(13.0));
                ui.add_space(6.0);
                ui.label(RichText::new("Основной каталог использует WinGet, GitHub и сайты разработчиков. Настраивать Google Диск для работы приложения не требуется.").color(colors.dim).size(12.0));
                ui.add_space(8.0);
                egui::CollapsingHeader::new("Другой источник · для владельца коллекции").show(ui, |ui| {
                    ui.label(RichText::new("Дополнительный JSON-каталог: HTTPS-ссылка или локальный путь.").size(12.0).color(colors.muted));
                    ui.add_enabled_ui(!self.queue_active && !self.loading, |ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.source_input).hint_text("catalog.public.json · ссылка или путь").desired_width(f32::INFINITY).margin(8.0));
                        ui.horizontal(|ui| {
                            if ui.button("Подключить").clicked() { self.load_source(self.source_input.trim().to_owned()); }
                            if ui.button("Встроенный источник").clicked() {
                                self.source_input = self.builtin_source.clone();
                                self.load_source(self.builtin_source.clone());
                            }
                        });
                    });
                    ui.hyperlink_to("Подготовка дополнительных пакетов", "https://github.com/HVHBIGNAME/SoftDownloader/blob/main/docs/google-drive.md");
                });
            });
        ui.add_space(14.0);
        colors.card_frame().show(ui, |ui| {
            ui.set_width((ui.available_width() - 2.0).max(150.0));
            ui.label(RichText::new("Навигация").size(17.0).strong());
            ui.label(
                RichText::new("Избранное, режим каталога и сортировка сохраняются автоматически.")
                    .size(12.0)
                    .color(colors.muted),
            );
            egui::CollapsingHeader::new("Горячие клавиши").show(ui, |ui| {
                egui::Grid::new("keyboard-shortcuts").show(ui, |ui| {
                    for (key, action) in [
                        ("Ctrl K / Ctrl F", "Поиск"),
                        ("Ctrl O", "Импорт списка"),
                        ("Esc", "Закрыть диалог или подробности; очистить поиск"),
                        ("Ctrl Z", "Вернуть очищенный выбор вне поля ввода"),
                    ] {
                        ui.label(RichText::new(key).monospace().color(colors.accent));
                        ui.label(RichText::new(action).size(12.0).color(colors.muted));
                        ui.end_row();
                    }
                });
            });
        });
        ui.add_space(14.0);
        colors.card_frame().show(ui, |ui| {
                ui.set_width((ui.available_width() - 2.0).max(150.0));
                ui.label(RichText::new("Каталог и данные").size(17.0).strong());
                ui.label(RichText::new(&self.catalog_status).color(colors.accent).size(12.0));
                ui.label(RichText::new("Кэш метаданных — 1 час. «Обновить» в каталоге запрашивает свежие версии. Иконки читаются локально.").size(12.0).color(colors.muted));
                for warning in &self.catalog_warnings { ui.label(RichText::new(warning).color(colors.orange).size(12.0)); }
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Открыть папку данных").clicked()
                        && let Err(error) = crate::system::open_folder(&self.store.root) { self.error = Some(format!("{error:#}")); }
                    if ui.button("Логи установки").clicked()
                        && let Err(error) = crate::system::open_folder(&self.store.logs) { self.error = Some(format!("{error:#}")); }
                    ui.hyperlink_to("Помощь и документация", "https://github.com/HVHBIGNAME/SoftDownloader#readme");
                });
            });
    }
}
