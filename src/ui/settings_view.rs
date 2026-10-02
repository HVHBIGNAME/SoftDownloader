use eframe::egui::{self, RichText};

use super::app::SoftDownloaderApp;
use super::theme;

impl SoftDownloaderApp {
    pub(super) fn settings_page(&mut self, ui: &mut egui::Ui) {
        theme::heading(
            ui,
            "Твоя коллекция. Твои правила.",
            "Официальные источники уже подключены. Google Диск — для твоих дополнений.",
        );
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            theme::card_frame().show(ui, |ui| {
                ui.set_width((ui.available_width()-2.0).max(150.0));
                ui.label(RichText::new("Дополнительный каталог").size(17.0).strong());
                ui.label(RichText::new("Ссылка на файл catalog.public.json в Google Drive, прямой HTTPS-адрес или локальный путь к catalog.json.").color(theme::MUTED).size(12.0));
                ui.add_space(6.0);
                ui.add_enabled_ui(!self.queue_active && !self.loading, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.source_input).hint_text("https://drive.google.com/file/d/…/view").desired_width(f32::INFINITY).margin(10.0));
                    ui.horizontal(|ui| {
                        if ui.add(theme::primary("Сохранить и подключить")).clicked() { self.load_source(self.source_input.trim().to_owned()); }
                        if ui.button("Выбрать JSON…").clicked()
                            && let Some(path) = rfd::FileDialog::new().add_filter("Каталог JSON", &["json"]).pick_file() {
                            self.source_input = path.to_string_lossy().into_owned();
                        }
                        if ui.button("Только официальный").clicked() { self.source_input.clear(); self.load_source(String::new()); }
                    });
                });
                if self.loading { ui.horizontal(|ui| { ui.spinner(); ui.label(RichText::new("Загружаем и проверяем каталог…").color(theme::MUTED)); }); }
                if self.queue_active { ui.label(RichText::new("Источник можно изменить после завершения очереди.").color(theme::ORANGE).size(12.0)); }
                if !self.active_source.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("Подключён: {}", self.active_source)).color(theme::ACCENT).size(11.0));
                }
            });
            ui.add_space(14.0);
            theme::card_frame().show(ui, |ui| {
                ui.set_width((ui.available_width()-2.0).max(150.0));
                ui.label(RichText::new("Как подготовить Google Диск").size(17.0).strong());
                ui.add_space(5.0);
                setup_step(ui, "01", "Создай папку SoftDownloader", "В синхронизируемой папке Google Drive будут catalog.json, installers и addons.");
                setup_step(ui, "02", "Добавь программы и аддоны", "Утилита tools/catalog.py создаёт записи, копирует установщики и считает SHA-256.");
                setup_step(ui, "03", "Поделись файлами по ссылке", "Для других пользователей нужны права «Все, у кого есть ссылка» у каталога и каждого установщика. В каталог внеси ID или ссылки на файлы.");
                setup_step(ui, "04", "Опубликуй каталог", "Команда publish создаст catalog.public.json. Вставь ссылку на этот файл в поле выше.");
                ui.hyperlink_to("Пошаговая инструкция на GitHub", "https://github.com/HVHBIGNAME/SoftDownloader/blob/main/docs/google-drive.md");
            });
            ui.add_space(14.0);
            theme::card_frame().show(ui, |ui| {
                ui.set_width((ui.available_width()-2.0).max(150.0));
                ui.label(RichText::new("Данные на этом компьютере").size(17.0).strong());
                ui.label(RichText::new("Проверенные установщики остаются в кэше и используются повторно. EXE/MSI выполняются последовательно; параметры тихой установки задаёт владелец каталога.").color(theme::MUTED).size(12.0));
                for (label, path) in [("Настройки и история", &self.store.root), ("Кэш установщиков", &self.store.cache), ("Логи установки", &self.store.logs)] {
                    ui.add_space(3.0);
                    ui.label(RichText::new(label).size(11.0).color(theme::DIM));
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(path.to_string_lossy()).monospace().size(11.0));
                        if ui.small_button("Копировать путь").clicked() { ui.ctx().copy_text(path.to_string_lossy().into_owned()); }
                    });
                }
            });
            ui.add_space(16.0);
            ui.label(RichText::new("Google Drive может ограничивать скачивания популярных файлов. Права доступа и квоты управляются на стороне Google.").size(11.0).color(theme::DIM));
        });
    }
}

fn setup_step(ui: &mut egui::Ui, number: &str, title: &str, description: &str) {
    ui.horizontal_top(|ui| {
        theme::pill(ui, number, theme::ACCENT);
        ui.vertical(|ui| {
            ui.label(RichText::new(title).strong().size(13.0));
            ui.label(RichText::new(description).size(12.0).color(theme::MUTED));
        });
    });
    ui.add_space(6.0);
}
