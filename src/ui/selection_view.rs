use eframe::egui::{self, Align, Layout, Margin, RichText, Stroke};

use super::app::{Page, SoftDownloaderApp};
use super::theme;

impl SoftDownloaderApp {
    pub(super) fn selection_bar(&mut self, ctx: &egui::Context) {
        let colors = theme::colors(ctx);
        let visible = self.queue_active
            || self.pending_install.is_some()
            || matches!(self.page, Page::Catalog | Page::Sets) && !self.selected.is_empty()
            || self.page == Page::Installed && !self.selected_removals.is_empty();
        let stacked = ctx.available_rect().width() < 950.0;
        egui::TopBottomPanel::bottom("selection-bar")
            .exact_height(if stacked { 132.0 } else { 90.0 })
            .frame(egui::Frame::new().inner_margin(Margin::symmetric(18, 10)))
            .show_animated(ctx, visible, |ui| {
                colors
                    .card_frame()
                    .stroke(Stroke::new(
                        1.0_f32,
                        colors.border.lerp_to_gamma(colors.accent, 0.35),
                    ))
                    .inner_margin(Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.with_layout(
                            if stacked {
                                Layout::top_down(Align::Min)
                            } else {
                                Layout::left_to_right(Align::Center)
                            },
                            |ui| {
                                if self.pending_install.is_some() {
                                    self.pending_selection(ui);
                                } else if self.queue_active {
                                    self.queue_progress(ui);
                                } else if self.page == Page::Installed {
                                    self.removal_selection(ui);
                                } else {
                                    self.install_selection(ui);
                                }
                            },
                        );
                    });
            });
    }

    fn pending_selection(&mut self, ui: &mut egui::Ui) {
        let count = self
            .pending_install
            .as_ref()
            .map_or(0, |p| p.selected.len());
        ui.horizontal(|ui| {
            ui.spinner();
            ui.vertical(|ui| {
                ui.label(RichText::new(format!("Готовим установку · {count}")).strong());
                ui.label(
                    RichText::new(
                        "Проверяем каталог и установленное ПО. Запуск продолжится автоматически.",
                    )
                    .size(12.0),
                );
            });
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui.button("Отменить подготовку").clicked() {
                self.pending_install = None;
            }
        });
    }

    fn queue_progress(&mut self, ui: &mut egui::Ui) {
        if self.engine.is_paused() {
            ui.label("Пауза после текущей задачи");
        } else {
            ui.spinner();
        }
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
                RichText::new("Зависимости добавятся автоматически")
                    .size(12.0)
                    .color(colors.muted),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .add_enabled(
                    self.install_actions_enabled(),
                    colors.primary(format!("Установить · {}", self.selected.len())),
                )
                .on_hover_text("Запустить выбранные программы с текущим режимом установки")
                .clicked()
            {
                self.prepare_install(self.selected.clone());
            }
            if ui.button("План").clicked() {
                self.request_install(self.selected.clone(), true);
            }
            self.installation_mode_picker(ui);
            ui.menu_button("···", |ui| {
                if ui
                    .add_enabled(
                        self.list_actions_enabled(),
                        egui::Button::new("Сохранить в мои наборы…"),
                    )
                    .clicked()
                {
                    self.open_sets();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.list_actions_enabled(),
                        egui::Button::new("Экспорт JSON…"),
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
