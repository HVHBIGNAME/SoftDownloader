use std::collections::BTreeSet;

use eframe::egui::{self, RichText};

use super::app::SoftDownloaderApp;
use super::theme;
use crate::catalog::{InstallSpec, Package};
use crate::installation::InstallMode;

impl SoftDownloaderApp {
    pub(super) fn installation_mode_picker(&mut self, ui: &mut egui::Ui) {
        let old = self.settings.installation.mode;
        egui::ComboBox::from_id_salt(ui.id().with("install-mode"))
            .width(162.0)
            .selected_text(old.label())
            .show_ui(ui, |ui| {
                for mode in InstallMode::ALL {
                    ui.selectable_value(&mut self.settings.installation.mode, mode, mode.label());
                }
            });
        if old != self.settings.installation.mode && !self.save_preferences() {
            self.settings.installation.mode = old;
        }
    }

    pub(super) fn installation_settings(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        if self
            .winget_status
            .is_none_or(|(checked, _)| checked.elapsed().as_secs() >= 10)
        {
            self.winget_status = Some((
                std::time::Instant::now(),
                crate::system::winget::available(),
            ));
        }
        let winget_available = self.winget_status.is_some_and(|(_, available)| available);
        colors.card_frame().show(ui, |ui| {
            ui.set_width((ui.available_width() - 2.0).max(150.0));
            ui.label(RichText::new("Установка без лишних шагов").size(20.0).strong());
            if winget_available {
                ui.label(RichText::new("WinGet готов к установке программ").color(colors.accent).size(12.0));
            } else {
                ui.label(RichText::new("Для большинства программ нужен WinGet").color(colors.orange));
                ui.hyperlink_to("Установить App Installer Microsoft", "https://apps.microsoft.com/detail/9nblggh4nns1");
            }
            if ui.add(theme::compact("Проверить WinGet снова")).clicked() {
                self.winget_status = None;
                ui.ctx().request_repaint();
            }
            self.installation_mode_picker(ui);
            ui.label("Тихая установка использует штатные параметры пакета. «С мастером» открывает установщик для выбора компонентов и папки.");
            let old = self.settings.installation.confirm_before_install;
            if ui.checkbox(&mut self.settings.installation.confirm_before_install, "Всегда показывать план перед установкой").changed()
                && !self.save_preferences() { self.settings.installation.confirm_before_install = old; }
            ui.label("Если выключено, кнопка «Установить» сразу запускает очередь. «План» позволяет просмотреть её в любой момент.");
            ui.label(RichText::new("Обязательные мастера и запросы UAC остаются видимыми. Архивы и расширения VS Code устанавливаются автоматически. Ручные карточки показывают инструкцию.").size(12.0).color(colors.muted));
        });
        ui.add_space(14.0);
        colors.card_frame().show(ui, |ui| {
            ui.set_width((ui.available_width() - 2.0).max(150.0));
            ui.label(RichText::new("Отдельные режимы программ").strong());
            ui.label("Задайте исключение в подробностях программы. Режим запущенной очереди не изменяется.");
            for (id, mode) in self.settings.installation.overrides.clone() {
                ui.horizontal(|ui| {
                    let name = self.document.catalog.package(&id).map_or(id.as_str(), |p| p.name.as_str());
                    ui.label(format!("{name} · {}", mode.label()));
                    if ui.add(theme::compact("Сбросить")).clicked() {
                        self.settings.installation.overrides.remove(&id);
                        if !self.save_preferences() { self.settings.installation.overrides.insert(id.clone(), mode); }
                    }
                });
            }
        });
    }

    pub(super) fn package_install_actions(&mut self, ui: &mut egui::Ui, package: &Package) {
        let colors = theme::colors(ui.ctx());
        let installed = crate::storage::is_installed(package, self.installation_state());
        if matches!(
            package.install,
            Some(InstallSpec::Exe { .. } | InstallSpec::Msi { .. } | InstallSpec::Winget)
        ) {
            let old = self
                .settings
                .installation
                .overrides
                .get(&package.id)
                .copied();
            let mut choice = old;
            egui::ComboBox::from_id_salt(("package-mode", &package.id))
                .selected_text(choice.map_or("Как в настройках", InstallMode::label))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut choice, None, "Как в настройках");
                    for mode in InstallMode::ALL {
                        ui.selectable_value(&mut choice, Some(mode), mode.label());
                    }
                });
            if choice != old {
                self.set_package_mode(&package.id, choice);
            }
        }
        ui.label(
            RichText::new(self.settings.installation.label_for(package))
                .color(colors.dim)
                .size(12.0),
        );
        if !package.is_manual()
            && !installed
            && ui
                .add_enabled(
                    self.install_actions_enabled() && package.ready(),
                    colors.primary("Установить сейчас"),
                )
                .clicked()
        {
            self.prepare_install(BTreeSet::from([package.id.clone()]));
        }
    }

    pub(super) fn set_package_mode(&mut self, id: &str, mode: Option<InstallMode>) {
        let old = self.settings.installation.overrides.get(id).copied();
        match mode {
            Some(mode) => {
                self.settings.installation.overrides.insert(id.into(), mode);
            }
            None => {
                self.settings.installation.overrides.remove(id);
            }
        }
        if !self.save_preferences() {
            match old {
                Some(mode) => {
                    self.settings.installation.overrides.insert(id.into(), mode);
                }
                None => {
                    self.settings.installation.overrides.remove(id);
                }
            }
        }
    }

    pub(super) fn confirmation(&mut self, ctx: &egui::Context) {
        let Some(plan) = self.confirm_plan.take() else {
            return;
        };
        let colors = theme::colors(ctx);
        let mut start = false;
        let mut close = false;
        let response = egui::Modal::new(egui::Id::new("confirm-install")).frame(colors.dialog_frame().inner_margin(24)).show(ctx, |ui| {
            ui.set_width(570.0);
            theme::heading(ui, "План установки", "Зависимости добавлены, установленное пропущено.");
            if self.inventory_needs_review() {
                ui.label(RichText::new("Проверка установленного ПО завершилась не полностью. Проверьте список, чтобы не переустановить уже имеющиеся программы.").color(colors.orange));
            }
            ui.label(format!("Установить: {} · Уже есть: {} · Недоступно: {}", plan.batch.packages.len(), plan.batch.installed, plan.batch.blocked.len()));
            egui::ScrollArea::vertical().auto_shrink([false, true]).max_height((ctx.content_rect().height() - 320.0).clamp(120.0, 400.0)).show(ui, |ui| {
                for package in &plan.batch.packages {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&package.name).strong());
                        ui.label(plan.installation.label_for(package));
                        if !plan.requested.contains(&package.id) { theme::pill(ui, "ЗАВИСИМОСТЬ", colors.violet); }
                    });
                }
                for package in &plan.batch.blocked {
                    ui.label(RichText::new(format!("{}: {}", package.name, package.reason)).color(colors.orange));
                }
            });
            ui.add_space(10.0);
            ui.label("При необходимости Windows запросит UAC. После старта можно поставить следующие задачи на паузу.");
            ui.horizontal(|ui| {
                if ui.add_enabled(!plan.batch.packages.is_empty(), colors.primary(format!("Установить · {}", plan.batch.packages.len()))).clicked() { start = true; }
                if ui.button("Вернуться").clicked() { close = true; }
            });
        });
        if start {
            self.start_plan(plan);
        } else if !close && !response.should_close() {
            self.confirm_plan = Some(plan);
        }
    }
}
