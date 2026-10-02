use eframe::egui::{self, Align, Layout, Margin, RichText, Sense, Stroke, Vec2};

use super::app::{Page, SoftDownloaderApp};
use super::theme::{self, Icon};
use crate::catalog::{InstallSpec, Package, PackageKind};
use crate::storage::is_installed;

impl SoftDownloaderApp {
    pub(super) fn catalog_page(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing.y = 6.0;
        let title = self
            .category
            .as_ref()
            .map(|id| self.document.catalog.category_path(id))
            .unwrap_or_else(|| match self.kind {
                Some(PackageKind::App) => "Программы".into(),
                Some(PackageKind::Addon) => "Аддоны и утилиты".into(),
                None => "Всё для твоей работы.".into(),
            });
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(RichText::new(title).size(27.0).strong()).truncate());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                theme::pill(ui, "WINDOWS", theme::MUTED);
                if self.loading {
                    ui.spinner();
                } else if ui
                    .add_enabled(!self.queue_active, egui::Button::new("Обновить").small())
                    .clicked()
                {
                    self.load_source(self.active_source.clone());
                }
            });
        });
        ui.label(
            RichText::new("Официальный софт и твои дополнения — в одном месте.")
                .size(13.0)
                .color(theme::MUTED),
        );
        ui.add_space(6.0);
        if self.active_source.is_empty() && self.details.is_none() {
            self.drive_banner(ui);
        }
        if !self.document.diagnostics.is_empty() {
            ui.label(
                RichText::new("Некоторые источники недоступны. Причина — в карточке пакета.")
                    .size(12.0)
                    .color(theme::ORANGE),
            );
        }
        self.search_and_filters(ui);
        let mut packages: Vec<_> = self
            .document
            .catalog
            .packages
            .iter()
            .filter(|p| {
                self.kind.is_none_or(|kind| p.kind == kind)
                    && self.category.as_ref().is_none_or(|category| {
                        self.document
                            .catalog
                            .category_contains(category, &p.category)
                    })
                    && p.matches_search(&self.query)
            })
            .cloned()
            .collect();
        if self.sort_by_name {
            packages.sort_by_cached_key(|p| p.name.to_lowercase());
        }
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} в коллекции", packages.len()))
                    .color(theme::MUTED)
                    .size(12.0),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .add_enabled(
                        !self.queue_active && !self.loading && packages.iter().any(Package::ready),
                        egui::Button::new("Выбрать все").small(),
                    )
                    .clicked()
                {
                    let state = self.installation_state();
                    self.selected.extend(
                        packages
                            .iter()
                            .filter(|p| p.ready() && !is_installed(p, &state))
                            .map(|p| p.id.clone()),
                    );
                }
            });
        });
        ui.add_space(3.0);
        if packages.is_empty() {
            ui.add_space(45.0);
            ui.vertical_centered(|ui| {
                theme::icon(ui, Icon::Search, theme::MUTED);
                ui.label(RichText::new("Ничего не нашлось").size(20.0).strong());
                ui.label(
                    RichText::new("Попробуй другой запрос или сбрось фильтры.").color(theme::MUTED),
                );
                if ui.button("Сбросить фильтры").clicked() {
                    self.query.clear();
                    self.kind = None;
                    self.category = None;
                }
            });
            return;
        }
        self.package_grid(ui, &packages);
    }

    fn drive_banner(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Добавь свои пакеты из Google Диска")
                    .size(12.0)
                    .color(theme::ACCENT),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Подключить Диск  >").clicked() {
                    self.page = Page::Settings;
                    self.details = None;
                }
            });
        });
        ui.add_space(2.0);
    }

    fn search_and_filters(&mut self, ui: &mut egui::Ui) {
        egui::Frame::new()
            .fill(theme::SURFACE)
            .stroke(Stroke::new(1.0, theme::BORDER))
            .corner_radius(9)
            .inner_margin(Margin::symmetric(14, 7))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    theme::icon(ui, Icon::Search, theme::MUTED);
                    let width = (ui.available_width() - 52.0).max(100.0);
                    let input = ui.add(
                        egui::TextEdit::singleline(&mut self.query)
                            .id_salt("catalog-search")
                            .hint_text("Поиск по названию, тегам или издателю…")
                            .desired_width(width)
                            .frame(false)
                            .font(egui::TextStyle::Body),
                    );
                    if ui.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::K)) {
                        input.request_focus();
                    }
                    ui.label(RichText::new("Ctrl K").size(10.0).color(theme::DIM));
                });
            });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            for (label, kind) in [
                ("Все", None),
                ("Программы", Some(PackageKind::App)),
                ("Аддоны", Some(PackageKind::Addon)),
            ] {
                let selected = self.kind == kind;
                let button =
                    egui::Button::new(RichText::new(label).size(12.0).color(if selected {
                        theme::BG
                    } else {
                        theme::MUTED
                    }))
                    .fill(if selected {
                        theme::ACCENT
                    } else {
                        theme::SURFACE
                    })
                    .corner_radius(7);
                if ui.add(button).clicked() {
                    self.kind = kind;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                egui::ComboBox::from_id_salt("sort-order")
                    .width(157.0)
                    .selected_text(if self.sort_by_name {
                        "По названию"
                    } else {
                        "Порядок каталога"
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.sort_by_name, false, "Порядок каталога");
                        ui.selectable_value(&mut self.sort_by_name, true, "По названию");
                    });
            });
        });
        ui.add_space(4.0);
    }

    fn package_grid(&mut self, ui: &mut egui::Ui, packages: &[Package]) {
        egui::ScrollArea::vertical()
            .id_salt("catalog-grid-scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let available = ui.available_width();
                let columns = if available >= 870.0 {
                    3
                } else if available >= 550.0 {
                    2
                } else {
                    1
                };
                let width = (available - 14.0 * (columns - 1) as f32) / columns as f32;
                egui::Grid::new("catalog-cards")
                    .num_columns(columns)
                    .spacing(Vec2::new(14.0, 14.0))
                    .show(ui, |ui| {
                        for (index, package) in packages.iter().enumerate() {
                            ui.push_id(&package.id, |ui| {
                                ui.allocate_ui_with_layout(
                                    Vec2::new(width, 222.0),
                                    Layout::top_down(Align::Min),
                                    |ui| {
                                        self.package_card(ui, package, width);
                                    },
                                );
                            });
                            if (index + 1) % columns == 0 {
                                ui.end_row();
                            }
                        }
                    });
            });
    }

    fn package_card(&mut self, ui: &mut egui::Ui, package: &Package, width: f32) {
        let selected = self.selected.contains(&package.id);
        let installed = is_installed(package, &self.installation_state());
        let stroke = if selected {
            theme::ACCENT.gamma_multiply(0.65)
        } else {
            theme::BORDER
        };
        theme::card_frame()
            .stroke(Stroke::new(1.0, stroke))
            .show(ui, |ui| {
                ui.set_width((width - 34.0).max(180.0));
                ui.set_min_height(188.0);
                let top = ui.cursor().top();
                ui.horizontal(|ui| {
                    theme::app_icon(ui, &package.id, 44.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if !package.enabled {
                            theme::pill(ui, "DEMO", theme::DIM);
                        } else if !package.ready() {
                            theme::pill(
                                ui,
                                if self.loading {
                                    "ЗАГРУЗКА"
                                } else {
                                    "НЕДОСТУПНО"
                                },
                                theme::ORANGE,
                            );
                        } else if installed {
                            theme::icon(ui, Icon::Check, theme::ACCENT);
                        } else {
                            let mut checked = selected;
                            if ui
                                .add_enabled(
                                    !self.queue_active && !self.loading,
                                    egui::Checkbox::without_text(&mut checked),
                                )
                                .on_hover_text("Добавить в список установки")
                                .changed()
                            {
                                if checked {
                                    self.selected.insert(package.id.clone());
                                } else {
                                    self.selected.remove(&package.id);
                                }
                            }
                        }
                    });
                });
                ui.add_space(6.0);
                if ui
                    .add(
                        egui::Label::new(RichText::new(&package.name).size(16.0).strong())
                            .truncate()
                            .sense(Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    self.details = Some(package.id.clone());
                }
                ui.label(
                    RichText::new(format!("{} · {}", package.publisher, package.version))
                        .size(11.0)
                        .color(theme::DIM),
                );
                let mut text = egui::text::LayoutJob::simple(
                    package.description.clone(),
                    egui::FontId::proportional(12.0),
                    theme::MUTED,
                    ui.available_width(),
                );
                text.wrap.max_rows = 2;
                ui.label(text);
                ui.add_space((top + 155.0 - ui.cursor().top()).max(4.0));
                ui.horizontal(|ui| {
                    theme::pill(
                        ui,
                        if package.kind == PackageKind::Addon {
                            "АДДОН"
                        } else {
                            "ПРОГРАММА"
                        },
                        if package.kind == PackageKind::Addon {
                            theme::VIOLET
                        } else {
                            theme::MUTED
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Подробнее >").size(11.0).color(theme::MUTED),
                                )
                                .frame(false)
                                .small(),
                            )
                            .clicked()
                        {
                            self.details = Some(package.id.clone());
                        }
                    });
                });
            });
    }

    pub(super) fn details_panel(&mut self, ctx: &egui::Context) {
        let Some(package) = self
            .details
            .as_ref()
            .and_then(|id| self.document.catalog.package(id))
            .cloned()
        else {
            return;
        };
        egui::SidePanel::right("package-details").exact_width(308.0).resizable(false).frame(egui::Frame::new().fill(theme::SIDEBAR).inner_margin(Margin::symmetric(22, 26))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("О ПАКЕТЕ").size(10.0).color(theme::DIM));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| { if ui.small_button("×").clicked() { self.details = None; } });
            });
            ui.add_space(18.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                theme::app_icon(ui, &package.id, 64.0);
                ui.add_space(10.0);
                ui.label(RichText::new(&package.name).size(22.0).strong());
                ui.label(RichText::new(&package.publisher).color(theme::MUTED));
                ui.add_space(12.0);
                ui.label(RichText::new(&package.description).size(13.0).color(theme::MUTED));
                ui.add_space(12.0);
                detail_line(ui, "Версия", &package.version);
                detail_line(ui, "Группа", &self.document.catalog.category_path(&package.category));
                if let Some(artifact) = &package.artifact {
                     detail_line(ui, "Размер", &if artifact.size == 0 { "Уточняется при загрузке".into() } else { theme::bytes(artifact.size) });
                    detail_line(ui, "Файл", &artifact.file_name);
                    detail_line(ui, "Источник", if self.document.local_root.is_some() && artifact.local_path.is_some() { "Локальная папка" } else if artifact.drive_file_id.is_some() { "Google Drive" } else { "HTTPS" });
                    if artifact.sha256.is_empty() {
                        ui.label(RichText::new("Проверка подписи Windows перед установкой. SHA-256 будет вычислен при загрузке.").size(11.0).color(theme::MUTED));
                    } else { ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("SHA-256  {}…", &artifact.sha256[..12])).monospace().size(10.0).color(theme::DIM)).on_hover_text(&artifact.sha256);
                        if ui.small_button("Копия").clicked() { ui.ctx().copy_text(artifact.sha256.clone()); }
                    }); }
                }
                self.install_details(ui, &package);
                if let Some(message) = self.document.diagnostics.get(&package.id) { ui.label(RichText::new(message).size(12.0).color(theme::ORANGE)); }
                if !package.depends_on.is_empty() {
                    ui.add_space(12.0);
                    ui.label(RichText::new("ЗАВИСИМОСТИ").size(10.0).color(theme::DIM));
                    for dependency in &package.depends_on {
                        let name = self.document.catalog.package(dependency).map(|p| p.name.as_str()).unwrap_or(dependency);
                        if ui.button(name).clicked() { self.details = Some(dependency.clone()); }
                    }
                }
                let related: Vec<_> = self.document.catalog.packages.iter().filter(|p| p.depends_on.contains(&package.id)).cloned().collect();
                if !related.is_empty() {
                    ui.add_space(12.0);
                    ui.label(RichText::new("ДОПОЛНЕНИЯ И УТИЛИТЫ").size(10.0).color(theme::DIM));
                    for addon in related {
                        ui.horizontal(|ui| {
                            let mut checked = self.selected.contains(&addon.id);
                            if ui.add_enabled(addon.ready() && !self.queue_active && !self.loading, egui::Checkbox::without_text(&mut checked)).changed() {
                                if checked { self.selected.insert(addon.id.clone()); } else { self.selected.remove(&addon.id); }
                            }
                            if ui.add(egui::Label::new(&addon.name).truncate().sense(Sense::click())).clicked() { self.details = Some(addon.id.clone()); }
                        });
                    }
                }
                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| { for tag in &package.tags { theme::pill(ui, tag, theme::MUTED); } });
                if let Some(homepage) = &package.homepage { ui.add_space(10.0); ui.hyperlink_to("Сайт разработчика", homepage); }
                ui.add_space(18.0);
                if !package.enabled {
                    ui.label(RichText::new("Это пример карточки. Добавь установщик и ссылку в свой каталог, чтобы сделать пакет доступным.").color(theme::ORANGE).size(12.0));
                } else if is_installed(&package, &self.installation_state()) {
                    theme::pill(ui, "Установлено", theme::ACCENT);
                } else if ui.add_enabled(package.ready() && !self.queue_active && !self.loading, theme::primary(if self.selected.contains(&package.id) { "Убрать из выбранного" } else { "Добавить к установке" })).clicked()
                    && !self.selected.remove(&package.id) {
                    self.selected.insert(package.id.clone());
                }
            });
        });
    }

    fn install_details(&self, ui: &mut egui::Ui, package: &Package) {
        if let Some(spec) = &package.install {
            ui.add_space(8.0);
            detail_line(
                ui,
                "Установка",
                if spec.requires_admin() {
                    "С запросом UAC"
                } else {
                    "Текущий пользователь"
                },
            );
            let description = match spec {
                InstallSpec::Exe { silent_args, .. } => format!("EXE  {}", silent_args.join(" ")),
                InstallSpec::Msi { arguments, .. } => {
                    format!("MSI  /qn /norestart {}", arguments.join(" "))
                }
                InstallSpec::Zip { destination, .. } => {
                    format!("ZIP  {:?}/{}", destination.root, destination.path)
                }
            };
            ui.label(
                RichText::new(description)
                    .monospace()
                    .size(10.0)
                    .color(theme::DIM),
            );
        }
    }
}

fn detail_line(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(RichText::new(label).size(10.0).color(theme::DIM));
    ui.label(RichText::new(value).size(12.0));
}
