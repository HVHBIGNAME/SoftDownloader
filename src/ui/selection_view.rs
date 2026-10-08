use eframe::egui::{self, Align, Layout, Margin, RichText, Stroke};

use super::app::{Page, SoftDownloaderApp};
use super::theme;

impl SoftDownloaderApp {
    pub(super) fn selection_bar(&mut self, ctx: &egui::Context) {
        let colors = theme::colors(ctx);
        let visible = self.queue_active
            || self.page == Page::Catalog && !self.selected.is_empty()
            || self.page == Page::Installed && !self.selected_removals.is_empty();
        egui::TopBottomPanel::bottom("selection-bar")
            .exact_height(76.0)
            .frame(
                egui::Frame::new()
                    .fill(colors.sidebar)
                    .inner_margin(Margin::symmetric(28, 16))
                    .stroke(Stroke::new(1.0_f32, colors.border)),
            )
            .show_animated(ctx, visible, |ui| {
                ui.horizontal_centered(|ui| {
                    if self.queue_active {
                        self.queue_progress(ui);
                    } else if self.page == Page::Installed {
                        self.removal_selection(ui);
                    } else {
                        self.install_selection(ui);
                    }
                });
            });
    }

    fn queue_progress(&mut self, ui: &mut egui::Ui) {
        ui.spinner();
        let ready = self.queue.iter().filter(|i| i.status.is_terminal()).count();
        ui.label(RichText::new(format!("Выполнено {ready} из {}", self.queue.len())).strong());
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Открыть очередь").clicked() {
                self.page = Page::Queue;
            }
        });
    }

    fn removal_selection(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        ui.label(
            RichText::new(format!("К удалению: {}", self.selected_removals.len()))
                .color(colors.muted),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(
                    self.list_actions_enabled(),
                    egui::Button::new(RichText::new("Проверить и удалить…").color(colors.red)),
                )
                .clicked()
            {
                self.remove_programs(&self.selected_removals.clone());
            }
            if ui.button("Снять выбор").clicked() {
                self.selected_removals.clear();
            }
        });
    }

    fn install_selection(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        ui.vertical(|ui| {
            ui.label(RichText::new(format!("Выбрано: {}", self.selected.len())).strong());
            ui.label(
                RichText::new("Зависимости — в следующем шаге")
                    .size(11.0)
                    .color(colors.muted),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(
                    self.list_actions_enabled(),
                    colors.primary("Проверить и установить"),
                )
                .on_hover_text("Открыть список программ и зависимостей перед установкой")
                .clicked()
            {
                self.prepare_install(self.selected.clone());
            }
            if ui.button("Проверить выбор").clicked() {
                self.review_selection();
            }
            ui.menu_button("···", |ui| {
                if ui
                    .add_enabled(
                        self.list_actions_enabled(),
                        egui::Button::new("Сохранить набор…"),
                    )
                    .clicked()
                {
                    self.export_package_set(false);
                    ui.close();
                }
                if ui.button("Очистить выбор").clicked() {
                    self.clear_selection();
                    ui.close();
                }
            });
        });
    }
}
