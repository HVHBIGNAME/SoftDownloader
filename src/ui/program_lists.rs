use std::collections::BTreeSet;

use eframe::egui::{self, Align, Layout, RichText, Vec2};

use super::app::SoftDownloaderApp;
use super::theme;
use crate::engine::ProgramListAction;
use crate::program_list::{MatchStatus, MatchedProgram, ProgramList};

pub(super) struct ImportPreview {
    name: String,
    list: ProgramList,
    rows: Vec<MatchedProgram>,
    selected: BTreeSet<String>,
    query: String,
}

pub(super) struct ImportedSelection {
    pub name: String,
    pub ids: BTreeSet<String>,
}

impl SoftDownloaderApp {
    pub(super) fn list_actions_enabled(&self) -> bool {
        !self.loading && !self.programs_loading && !self.queue_active && !self.list_busy
    }

    pub(super) fn request_program_list_import(&mut self) {
        if self.list_actions_enabled() && !self.has_modal() {
            self.list_busy = true;
            self.engine.import_program_list();
        }
    }

    pub(super) fn export_program_list(&mut self) {
        if self.list_actions_enabled() && !self.has_modal() && !self.programs.is_empty() {
            self.list_busy = true;
            self.engine
                .export_program_list(ProgramList::from_inventory(&self.programs));
        }
    }

    pub(super) fn export_package_set(&mut self, favorites: bool) {
        if !self.list_actions_enabled() || self.has_modal() {
            return;
        }
        let ids = if favorites {
            &self.settings.favorites
        } else {
            &self.selected
        };
        let list = ProgramList::from_packages(
            self.document
                .catalog
                .packages
                .iter()
                .filter(|p| ids.contains(&p.id)),
        );
        if !list.programs.is_empty() {
            self.list_busy = true;
            self.engine.export_program_list(list);
        }
    }

    pub(super) fn receive_program_list(
        &mut self,
        result: Result<Option<ProgramListAction>, String>,
    ) {
        self.list_busy = false;
        match result {
            Ok(Some(ProgramListAction::Imported { path, list })) => {
                let rows = list.match_catalog(&self.document.catalog, &self.effective_state);
                let selected = rows
                    .iter()
                    .filter(|r| r.status == MatchStatus::Available)
                    .filter_map(|r| r.package_id.clone())
                    .collect();
                self.import_preview = Some(ImportPreview {
                    name: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    list,
                    rows,
                    selected,
                    query: String::new(),
                });
                self.error = None;
            }
            Ok(Some(ProgramListAction::Exported { path, count })) => {
                self.notify(format!(
                    "Сохранено программ: {count} · {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Ok(None) => {}
            Err(error) => {
                self.error = Some(error);
            }
        }
    }

    pub(super) fn import_dialog(&mut self, ctx: &egui::Context) {
        let colors = theme::colors(ctx);
        let Some(mut preview) = self.import_preview.take() else {
            return;
        };
        let mut close = false;
        let response = egui::Modal::new(egui::Id::new("program-list-import")).frame(colors.card_frame().inner_margin(24)).show(ctx, |ui| {
            ui.set_width(640.0);
            theme::heading(ui, "Восстановить программы", "Выберите, что добавить к установке на этом компьютере.");
            ui.add(egui::Label::new(RichText::new(&preview.name).size(12.0).color(colors.dim)).truncate());
            ui.add_space(8.0);
            let available = preview.rows.iter().filter(|r| r.status == MatchStatus::Available).count();
            let installed = preview.rows.iter().filter(|r| r.status == MatchStatus::Installed).count();
            ui.horizontal_wrapped(|ui| {
                theme::pill(ui, &format!("Доступно: {available}"), colors.accent);
                theme::pill(ui, &format!("Уже установлено: {installed}"), colors.muted);
                let other = preview.rows.len() - available - installed;
                if other > 0 { theme::pill(ui, &format!("Недоступно: {other}"), colors.orange); }
            });
            ui.add_space(8.0);
            ui.add(egui::TextEdit::singleline(&mut preview.query).hint_text("Найти в списке…").desired_width(f32::INFINITY).margin(8.0));
            ui.horizontal(|ui| {
                if ui.small_button("Выбрать доступные").clicked() {
                    preview.selected = preview.rows.iter().filter(|r| r.status == MatchStatus::Available)
                        .filter_map(|r| r.package_id.clone()).collect();
                }
                if ui.small_button("Снять выбор").clicked() { preview.selected.clear(); }
            });
            ui.add_space(4.0);
            let query = preview.query.to_lowercase();
            let rows: Vec<_> = preview.rows.iter().filter(|r| {
                r.entry.name.to_lowercase().contains(&query) || r.package_id.as_ref().is_some_and(|id| id.contains(&query))
            }).collect();
            egui::ScrollArea::vertical().id_salt("import-list-rows").max_height((ctx.content_rect().height() - 460.0).clamp(120.0, 320.0))
                .show_rows(ui, 54.0, rows.len(), |ui, visible| {
                    for row in &rows[visible] {
                        import_row(ui, row, &mut preview.selected);
                    }
                });
            ui.add_space(12.0);
            ui.label(RichText::new("Версии в файле — для справки. Установка использует текущий каталог; зависимости добавятся автоматически.").size(11.0).color(colors.muted));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.add_enabled(!preview.selected.is_empty() && self.list_actions_enabled(), colors.primary(format!("Добавить к выбору · {}", preview.selected.len()))).clicked() {
                    let current = preview.list.match_catalog(&self.document.catalog, &self.effective_state);
                    let ids: BTreeSet<_> = current.into_iter().filter(|row| row.status == MatchStatus::Available)
                        .filter_map(|row| row.package_id).filter(|id| preview.selected.contains(id)).collect();
                    self.selected.extend(ids.clone());
                    self.show_catalog(None);
                    self.imported_selection = Some(ImportedSelection { name: preview.name.clone(), ids });
                    self.message = None;
                    close = true;
                }
                if ui.button("Отмена").clicked() { close = true; }
            });
        });
        if !close && !response.should_close() {
            self.import_preview = Some(preview);
        }
    }
}

fn import_row(ui: &mut egui::Ui, row: &MatchedProgram, selection: &mut BTreeSet<String>) {
    let colors = theme::colors(ui.ctx());
    let available = row.status == MatchStatus::Available;
    ui.push_id((&row.package_id, &row.entry.name), |ui| {
        ui.allocate_ui(Vec2::new(ui.available_width(), 54.0), |ui| {
            ui.horizontal(|ui| {
                let mut checked = row
                    .package_id
                    .as_ref()
                    .is_some_and(|id| selection.contains(id));
                if ui
                    .add_enabled(available, egui::Checkbox::without_text(&mut checked))
                    .changed()
                    && let Some(id) = &row.package_id
                {
                    if checked {
                        selection.insert(id.clone());
                    } else {
                        selection.remove(id);
                    }
                }
                let text_width = (ui.available_width() - 168.0).max(160.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(text_width, 42.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_min_size(Vec2::new(text_width, 42.0));
                        ui.add(
                            egui::Label::new(RichText::new(&row.entry.name).strong()).truncate(),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(
                                    [row.entry.version.as_str(), row.entry.publisher.as_str()]
                                        .into_iter()
                                        .filter(|part| !part.is_empty())
                                        .collect::<Vec<_>>()
                                        .join(" · "),
                                )
                                .size(11.0)
                                .color(colors.dim),
                            )
                            .truncate(),
                        );
                    },
                );
                let (label, color) = match row.status {
                    MatchStatus::Available => ("Можно установить", colors.accent),
                    MatchStatus::Installed => ("Уже установлено", colors.muted),
                    MatchStatus::Unavailable => ("Пока недоступно", colors.orange),
                    MatchStatus::Unknown => ("Нет в каталоге", colors.dim),
                    MatchStatus::Ambiguous => ("Неоднозначное имя", colors.orange),
                };
                ui.label(RichText::new(label).size(11.0).color(color))
                    .on_hover_text(row.package_id.as_deref().unwrap_or(
                        "Сохранено для справки. Выберите подходящую программу в каталоге вручную.",
                    ));
            });
        });
    });
}
