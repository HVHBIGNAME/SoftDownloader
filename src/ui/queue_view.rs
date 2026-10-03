use eframe::egui::{self, Align, Layout, RichText, Vec2};
use std::collections::BTreeSet;

use super::app::{Page, SoftDownloaderApp};
use super::theme;
use crate::engine::JobStatus;

impl SoftDownloaderApp {
    pub(super) fn queue_page(&mut self, ui: &mut egui::Ui) {
        theme::heading(
            ui,
            "Очередь задач",
            "Установка и удаление — последовательно, с понятным результатом.",
        );
        if self.queue.is_empty() {
            self.empty_state(
                ui,
                "Очередь пока пуста",
                "Выбери программы в каталоге или в разделе «Установлено».",
            );
            return;
        }
        ui.horizontal(|ui| {
            let done = self
                .queue
                .iter()
                .filter(|item| matches!(item.status, JobStatus::Done { .. }))
                .count();
            theme::pill(
                ui,
                &format!("{done} из {} выполнено", self.queue.len()),
                theme::ACCENT,
            );
            if self.queue_active {
                ui.spinner();
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.queue_active {
                    if ui
                        .add_enabled(
                            !self.engine.is_cancelling(),
                            egui::Button::new(if self.engine.is_cancelling() {
                                "Останавливаем…"
                            } else {
                                "Остановить"
                            }),
                        )
                        .clicked()
                    {
                        self.engine.cancel();
                    }
                } else if ui.button("Повторить незавершённые").clicked() {
                    let failed: BTreeSet<_> = self
                        .queue
                        .iter()
                        .filter(|item| {
                            matches!(
                                item.status,
                                JobStatus::Failed(_) | JobStatus::Cancelled | JobStatus::Skipped(_)
                            )
                        })
                        .map(|item| item.id.clone())
                        .collect();
                    if self.queue.first().is_some_and(|item| item.removal) {
                        self.remove_programs(&failed);
                    } else if !failed.is_empty() {
                        self.prepare_install(failed);
                    }
                }
            });
        });
        if self.queue_active {
            ui.label(RichText::new("Остановка отменяет загрузки и следующие задачи. Запущенный мастер завершит работу.").size(11.0).color(theme::DIM));
        }
        ui.add_space(14.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for item in &self.queue {
                theme::card_frame().show(ui, |ui| {
                    ui.set_width((ui.available_width()-2.0).max(150.0));
                    let color = match item.status { JobStatus::Done { .. } => theme::ACCENT, JobStatus::Failed(_) => theme::RED, JobStatus::Skipped(_) | JobStatus::Cancelled => theme::ORANGE, _ => theme::MUTED };
                    ui.horizontal(|ui| {
                        theme::app_icon(ui, &item.id, 38.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&item.name).strong());
                            ui.label(RichText::new(format!("{} · {}", if item.removal { "Удаление" } else { "Установка" }, item.version)).color(theme::DIM).size(11.0));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| { theme::pill(ui, item.status.label(), color); });
                    });
                    let progress = match item.status {
                        JobStatus::Done { .. } | JobStatus::Installing | JobStatus::Removing | JobStatus::Verifying => 1.0,
                        _ => if item.size > 0 { item.downloaded as f32 / item.size as f32 } else { 0.0 },
                    };
                    if !matches!(item.status, JobStatus::Queued) {
                        ui.add_space(4.0);
                        ui.add(egui::ProgressBar::new(progress).fill(color).desired_height(5.0).animate(matches!(item.status, JobStatus::Installing | JobStatus::Removing | JobStatus::Verifying)));
                    }
                    match &item.status {
                        JobStatus::Downloading => {
                            let total = if item.size == 0 { "размер уточняется".into() } else { theme::bytes(item.size) };
                            ui.label(RichText::new(format!("{} / {total}    ·    {}/с", theme::bytes(item.downloaded), theme::bytes(item.bytes_per_second as u64))).color(theme::DIM).size(11.0));
                        }
                        JobStatus::Failed(message) | JobStatus::Skipped(message) => { ui.label(RichText::new(message).size(12.0).color(color)); }
                        JobStatus::Installing | JobStatus::Removing => { ui.label(RichText::new("Если появилось окно UAC или штатный мастер, подтверди действие на панели задач.").color(theme::DIM).size(11.0)); }
                        JobStatus::Done { reboot_required: true } => { ui.label(RichText::new("Windows требуется перезагрузка. Выполни её в удобное время.").color(theme::ORANGE).size(11.0)); }
                        _ => {}
                    }
                });
                ui.add_space(5.0);
            }
        });
    }

    pub(super) fn installed_page(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Установлено").size(27.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.list_actions_enabled() && !self.programs.is_empty(),
                        egui::Button::new("Экспорт списка"),
                    )
                    .clicked()
                {
                    self.export_program_list();
                }
                if ui
                    .add_enabled(
                        self.list_actions_enabled(),
                        egui::Button::new("Импорт списка"),
                    )
                    .clicked()
                {
                    self.request_program_list_import();
                }
                if self.list_busy {
                    ui.spinner();
                }
            });
        });
        ui.label(
            RichText::new("Сохраните список, чтобы восстановить программы на другом компьютере.")
                .size(13.0)
                .color(theme::MUTED),
        );
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.installed_query)
                    .hint_text("Поиск установленной программы…")
                    .desired_width((ui.available_width() - 170.0).max(180.0))
                    .margin(10.0),
            );
            if ui
                .add_enabled(
                    !self.queue_active && !self.programs_loading,
                    egui::Button::new("Обновить список"),
                )
                .clicked()
            {
                self.refresh_programs();
            }
            if self.programs_loading {
                ui.spinner();
            }
        });
        if !self.inventory_warnings.is_empty() {
            egui::CollapsingHeader::new(
                RichText::new("Не все источники ответили").color(theme::ORANGE),
            )
            .show(ui, |ui| {
                for warning in &self.inventory_warnings {
                    ui.label(RichText::new(warning).size(11.0).color(theme::MUTED));
                }
            });
        }
        let query = self.installed_query.to_lowercase();
        let programs: Vec<_> = self
            .programs
            .iter()
            .enumerate()
            .filter(|(_, program)| {
                format!("{} {}", program.name, program.publisher)
                    .to_lowercase()
                    .contains(&query)
                    || program.package_ids.iter().any(|id| {
                        self.document
                            .catalog
                            .package(id)
                            .is_some_and(|p| p.matches_search(&query))
                    })
            })
            .map(|(index, _)| index)
            .collect();
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Найдено: {}", programs.len()))
                    .color(theme::DIM)
                    .size(12.0),
            );
            if ui
                .add_enabled(
                    !self.queue_active && !self.programs_loading && !self.loading,
                    egui::Button::new("Выбрать все найденные").small(),
                )
                .clicked()
            {
                self.selected_removals.extend(
                    programs
                        .iter()
                        .map(|&index| &self.programs[index])
                        .filter(|p| p.target.can_remove())
                        .map(|p| p.id.clone()),
                );
            }
            if !self.selected_removals.is_empty() && ui.small_button("Сбросить").clicked() {
                self.selected_removals.clear();
            }
        });
        ui.add_space(12.0);
        if programs.is_empty() && !self.programs_loading {
            self.empty_state(
                ui,
                "Программы не найдены",
                "Измени поисковый запрос или установи программу из каталога.",
            );
            return;
        }
        ui.spacing_mut().item_spacing.y = 8.0;
        egui::ScrollArea::vertical()
            .id_salt("installed-programs")
            .auto_shrink([false, false])
            .show_rows(ui, 68.0, programs.len(), |ui, rows| {
                for &index in &programs[rows] {
                    let program = self.programs[index].clone();
                    ui.push_id(&program.id, |ui| {
                        self.installed_row(ui, &program);
                    });
                }
            });
    }

    fn installed_row(&mut self, ui: &mut egui::Ui, program: &crate::uninstall::InstalledProgram) {
        let width = ui.available_width();
        let store_app = matches!(
            program.target,
            crate::uninstall::UninstallTarget::Appx { .. }
        );
        let name = if store_app && program.package_ids.is_empty() {
            program.name.rsplit('.').next().unwrap_or(&program.name)
        } else {
            &program.name
        };
        let publisher = if store_app {
            "Microsoft Store"
        } else {
            &program.publisher
        };
        theme::card_frame().inner_margin(12).show(ui, |ui| {
            ui.set_width((width - 24.0).max(150.0));
            ui.horizontal(|ui| {
                let mut selected = self.selected_removals.contains(&program.id);
                if ui
                    .add_enabled(
                        self.list_actions_enabled() && program.target.can_remove(),
                        egui::Checkbox::without_text(&mut selected),
                    )
                    .on_hover_text("Выбрать для удаления")
                    .changed()
                {
                    if selected {
                        self.selected_removals.insert(program.id.clone());
                    } else {
                        self.selected_removals.remove(&program.id);
                    }
                }
                if !self.icons.show(ui, program.icon_path.as_deref(), 36.0) {
                    theme::app_icon(ui, name, 36.0);
                }
                let text_width = (ui.available_width() - 48.0).max(160.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(text_width, 44.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_min_width(text_width);
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.add(egui::Label::new(RichText::new(name).strong()).truncate())
                            .on_hover_text(&program.name);
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("{} · {}", program.version, publisher))
                                    .size(11.0)
                                    .color(theme::DIM),
                            )
                            .truncate(),
                        );
                    },
                );
                ui.menu_button("···", |ui| {
                    ui.label(
                        RichText::new(if program.quiet {
                            "Поддерживает тихое удаление"
                        } else {
                            "Штатный мастер удаления"
                        })
                        .size(11.0)
                        .color(theme::MUTED),
                    );
                    if let Some(folder) = program.target.folder()
                        && ui.button("Открыть папку").clicked()
                    {
                        if let Err(error) = crate::system::open_folder(&folder) {
                            self.error = Some(format!("{error:#}"));
                        }
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            self.list_actions_enabled() && program.target.can_remove(),
                            egui::Button::new(RichText::new("Удалить программу").color(theme::RED)),
                        )
                        .clicked()
                    {
                        self.remove_programs(&BTreeSet::from([program.id.clone()]));
                        ui.close();
                    }
                });
            });
        });
    }

    fn empty_state(&mut self, ui: &mut egui::Ui, title: &str, subtitle: &str) {
        ui.add_space(65.0);
        ui.vertical_centered(|ui| {
            theme::logo(ui, 54.0);
            ui.add_space(12.0);
            ui.label(RichText::new(title).size(22.0).strong());
            ui.label(RichText::new(subtitle).color(theme::MUTED));
            ui.add_space(10.0);
            if ui.add(theme::primary("Открыть каталог  >")).clicked() {
                self.page = Page::Catalog;
            }
        });
    }
}
