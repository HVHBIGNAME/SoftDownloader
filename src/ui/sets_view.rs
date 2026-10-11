use std::collections::BTreeSet;

use eframe::egui::{self, RichText};

use super::app::{Page, SoftDownloaderApp};
use super::theme;
use crate::planner::prepare_batch;
use crate::program_sets::{PRESETS, ProgramSet};

pub(super) struct SetsUi {
    name: String,
    deleted: Option<ProgramSet>,
}

impl Default for SetsUi {
    fn default() -> Self {
        Self {
            name: "Мой компьютер".into(),
            deleted: None,
        }
    }
}

impl SoftDownloaderApp {
    pub(super) fn sets_page(&mut self, ui: &mut egui::Ui) {
        theme::heading(
            ui,
            "Наборы программ",
            "Готовое окружение одной кнопкой. Уже установленное пропускается.",
        );
        ui.horizontal_wrapped(|ui| {
            self.installation_mode_picker(ui);
            if ui.button("Открыть каталог").clicked() {
                self.show_catalog(None);
            }
        });
        ui.add_space(10.0);
        let colors = theme::colors(ui.ctx());
        colors.card_frame().show(ui, |ui| {
            ui.label(RichText::new("Сохранить свой набор").strong());
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.sets_ui.name).desired_width(190.0).char_limit(80).hint_text("Название набора"));
                if ui.add_enabled(!self.selected.is_empty() && self.list_actions_enabled(), egui::Button::new(format!("Сохранить выбор · {}", self.selected.len()))).clicked() {
                    self.save_new_set(self.selected.clone());
                }
                if ui.add_enabled(!self.settings.favorites.is_empty() && self.list_actions_enabled(), egui::Button::new("Из избранного")).clicked() {
                    self.save_new_set(self.settings.favorites.clone());
                }
            });
            ui.label(RichText::new("Наборы сохраняются в приложении. Для другого ПК используйте «Экспорт JSON» в меню набора.").size(11.0).color(colors.muted));
        });
        if let Some(deleted) = self.sets_ui.deleted.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("Удалён набор «{}»", deleted.name));
                if ui.small_button("Вернуть").clicked() {
                    let mut sets = self.settings.program_sets.clone();
                    sets.push(deleted);
                    if self.save_sets(sets) {
                        self.sets_ui.deleted = None;
                    }
                }
            });
        }
        ui.add_space(12.0);
        egui::ScrollArea::vertical()
            .id_salt("program-sets")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if !self.settings.program_sets.is_empty() {
                    ui.label(RichText::new("Мои наборы").size(18.0).strong());
                    for set in self.settings.program_sets.clone() {
                        self.set_card(ui, &set, "Сохранённый набор", true);
                    }
                    ui.add_space(12.0);
                }
                ui.label(RichText::new("Готовые наборы").size(18.0).strong());
                for preset in PRESETS {
                    let set = ProgramSet {
                        name: preset.name.into(),
                        ids: preset.ids.iter().map(|id| id.to_string()).collect(),
                    };
                    self.set_card(ui, &set, preset.description, false);
                }
            });
    }

    fn set_card(&mut self, ui: &mut egui::Ui, set: &ProgramSet, description: &str, saved: bool) {
        let colors = theme::colors(ui.ctx());
        let batch = prepare_batch(&self.document.catalog, &set.ids, self.installation_state());
        ui.push_id((saved, &set.name), |ui| {
            colors.card_frame().show(ui, |ui| {
                ui.set_width((ui.available_width() - 2.0).max(160.0));
                ui.label(RichText::new(&set.name).size(17.0).strong());
                ui.label(RichText::new(description).color(colors.muted).size(12.0));
                ui.label(
                    RichText::new(format!(
                        "{} в наборе · {} к установке с зависимостями · {} уже есть",
                        set.ids.len(),
                        batch.packages.len(),
                        batch.installed
                    ))
                    .size(11.0)
                    .color(colors.dim),
                );
                if !batch.blocked.is_empty() {
                    ui.label(
                        RichText::new(format!(
                            "Требуют действий: {} — см. состав или план",
                            batch.blocked.len()
                        ))
                        .color(colors.orange)
                        .size(11.0),
                    );
                }
                egui::CollapsingHeader::new("Состав набора").show(ui, |ui| {
                    for id in &set.ids {
                        let package = self.document.catalog.package(id);
                        let name = package.map_or(id.as_str(), |p| p.name.as_str());
                        let status = if package.is_some_and(|p| {
                            crate::storage::is_installed(p, self.installation_state())
                        }) {
                            "уже установлено"
                        } else if let Some(blocked) = batch.blocked.iter().find(|p| &p.id == id) {
                            blocked.reason.as_str()
                        } else {
                            "будет установлено"
                        };
                        ui.label(format!("{name} · {status}"));
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .add_enabled(
                            self.list_actions_enabled() && !batch.packages.is_empty(),
                            colors.primary(format!("Установить · {}", batch.packages.len())),
                        )
                        .clicked()
                    {
                        self.prepare_install(set.ids.clone());
                    }
                    if ui
                        .add_enabled(
                            self.list_actions_enabled() && !batch.packages.is_empty(),
                            egui::Button::new("В выбор"),
                        )
                        .clicked()
                    {
                        self.selected.extend(
                            set.ids
                                .iter()
                                .filter(|id| {
                                    self.document.catalog.package(id).is_some_and(|p| {
                                        p.ready()
                                            && !crate::storage::is_installed(
                                                p,
                                                &self.effective_state,
                                            )
                                    })
                                })
                                .cloned(),
                        );
                        self.review_selection();
                        self.save_preferences();
                    }
                    if ui
                        .add_enabled(self.list_actions_enabled(), egui::Button::new("План"))
                        .clicked()
                    {
                        self.request_install(set.ids.clone(), true);
                    }
                    self.set_menu(ui, set, saved);
                });
            });
            ui.add_space(8.0);
        });
    }

    fn save_new_set(&mut self, ids: BTreeSet<String>) {
        match ProgramSet::new(&self.sets_ui.name, ids) {
            Ok(set) => {
                let name = set.name.clone();
                let mut sets = self.settings.program_sets.clone();
                sets.push(set);
                if self.save_sets(sets) {
                    self.notify(format!("Набор «{name}» сохранён"));
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn set_menu(&mut self, ui: &mut egui::Ui, set: &ProgramSet, saved: bool) {
        ui.menu_button("···", |ui| {
            if ui
                .add_enabled(
                    self.list_actions_enabled(),
                    egui::Button::new("Экспорт JSON…"),
                )
                .clicked()
            {
                self.list_busy = true;
                self.engine
                    .export_program_list(crate::program_list::ProgramList::from_ids(
                        &self.document.catalog,
                        &set.ids,
                    ));
                ui.close();
            }
            if saved {
                if ui
                    .add_enabled(
                        !self.selected.is_empty() && self.list_actions_enabled(),
                        egui::Button::new("Заменить текущим выбором"),
                    )
                    .clicked()
                {
                    let mut sets = self.settings.program_sets.clone();
                    if let Some(existing) = sets.iter_mut().find(|item| item.name == set.name) {
                        existing.ids = self.selected.clone();
                    }
                    self.save_sets(sets);
                    ui.close();
                }
                if ui.button("Удалить набор").clicked() {
                    let sets = self
                        .settings
                        .program_sets
                        .iter()
                        .filter(|item| item.name != set.name)
                        .cloned()
                        .collect();
                    if self.save_sets(sets) {
                        self.sets_ui.deleted = Some(set.clone());
                    }
                    ui.close();
                }
            }
        });
    }

    fn save_sets(&mut self, sets: Vec<ProgramSet>) -> bool {
        let previous = std::mem::replace(&mut self.settings.program_sets, sets);
        if self.save_preferences() {
            true
        } else {
            self.settings.program_sets = previous;
            false
        }
    }

    pub(super) fn open_sets(&mut self) {
        self.page = Page::Sets;
        self.details = None;
    }
}
