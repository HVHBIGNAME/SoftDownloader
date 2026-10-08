use eframe::egui::{self, Align, Layout, Margin, RichText, Sense, Stroke, Vec2};

use super::app::SoftDownloaderApp;
use super::theme::{self, Icon};
use crate::catalog::{InstallSpec, Package, PackageKind};
use crate::catalog_filter::QuickFilter;
use crate::storage::{CatalogLayout, is_installed};

impl SoftDownloaderApp {
    pub(super) fn catalog_page(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        ui.spacing_mut().item_spacing.y = 6.0;
        self.catalog_heading(ui);
        self.search_and_filters(ui);
        let mut packages: Vec<_> = self
            .document
            .catalog
            .packages
            .iter()
            .filter(|p| {
                self.filter.matches(
                    p,
                    &self.document.catalog,
                    self.installation_state(),
                    &self.settings.favorites,
                    &self.selected,
                ) && self
                    .imported_selection
                    .as_ref()
                    .is_none_or(|list| list.ids.contains(&p.id))
            })
            .cloned()
            .collect();
        if self.settings.sort_by_name {
            packages.sort_by_cached_key(|p| p.name.to_lowercase());
        }
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("Программ: {}", packages.len()))
                    .color(colors.muted)
                    .size(12.0),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.filter.favorites_only || self.filter.quick == QuickFilter::Selected {
                    let has_set = self.document.catalog.packages.iter().any(|p| {
                        if self.filter.favorites_only {
                            self.settings.favorites.contains(&p.id)
                        } else {
                            self.selected.contains(&p.id)
                        }
                    });
                    if ui
                        .add_enabled(
                            self.list_actions_enabled() && has_set,
                            egui::Button::new("Сохранить набор…").small(),
                        )
                        .on_hover_text(if self.filter.favorites_only {
                            "Сохранить всё избранное, независимо от поиска"
                        } else {
                            "Сохранить весь выбор, независимо от поиска"
                        })
                        .clicked()
                    {
                        self.export_package_set(self.filter.favorites_only);
                    }
                }
                if self.filter.quick == QuickFilter::Selected {
                    if ui
                        .add_enabled(
                            !self.queue_active && !self.selected.is_empty(),
                            egui::Button::new("Очистить выбор").small(),
                        )
                        .clicked()
                    {
                        self.clear_selection();
                    }
                    return;
                }
                if ui
                    .add_enabled(
                        self.list_actions_enabled()
                            && packages
                                .iter()
                                .any(|p| p.ready() && !is_installed(p, self.installation_state())),
                        egui::Button::new("Выбрать найденные").small(),
                    )
                    .clicked()
                {
                    let state = self.installation_state().clone();
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
                colors.card_frame().show(ui, |ui| {
                    self.catalog_empty_state(ui);
                });
            });
            return;
        }
        match self.settings.catalog_layout {
            CatalogLayout::Grid => self.package_grid(ui, &packages),
            CatalogLayout::List => self.package_list(ui, &packages),
        }
    }

    fn catalog_empty_state(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        let empty_selection =
            self.filter.quick == QuickFilter::Selected && self.selected.is_empty();
        let empty_favorites = self.filter.favorites_only
            && !self
                .document
                .catalog
                .packages
                .iter()
                .any(|p| self.settings.favorites.contains(&p.id));
        theme::icon(
            ui,
            if empty_favorites {
                Icon::Star
            } else {
                Icon::Search
            },
            colors.muted,
        );
        ui.label(
            RichText::new(if empty_selection {
                "Выбор пока пуст"
            } else if empty_favorites {
                "Соберите своё избранное"
            } else {
                "Ничего не нашлось"
            })
            .size(20.0)
            .strong(),
        );
        ui.label(
            RichText::new(if empty_favorites {
                "Нажмите звёздочку рядом с любимой программой."
            } else if empty_selection {
                "Отмечайте программы в каталоге — здесь будет весь выбор."
            } else {
                "Попробуйте другой запрос или сбросьте фильтры."
            })
            .color(colors.muted),
        );
        if empty_selection || empty_favorites {
            if ui.button("Открыть каталог").clicked() {
                self.show_catalog(None);
            }
        } else if ui.button("Сбросить фильтры").clicked() {
            self.filter.reset_constraints();
            self.imported_selection = None;
        }
    }

    fn package_grid(&mut self, ui: &mut egui::Ui, packages: &[Package]) {
        let columns = if ui.available_width() >= 870.0 {
            3
        } else if ui.available_width() >= 550.0 {
            2
        } else {
            1
        };
        ui.spacing_mut().item_spacing.y = 14.0;
        egui::ScrollArea::vertical()
            .id_salt((
                "catalog-grid-scroll",
                &self.filter,
                self.settings.sort_by_name,
            ))
            .auto_shrink([false, false])
            .show_rows(ui, 222.0, packages.len().div_ceil(columns), |ui, rows| {
                let available = ui.available_width();
                let width = (available - 14.0 * (columns - 1) as f32) / columns as f32;
                egui::Grid::new("catalog-cards")
                    .num_columns(columns)
                    .spacing(Vec2::new(14.0, 14.0))
                    .show(ui, |ui| {
                        let visible =
                            rows.start * columns..(rows.end * columns).min(packages.len());
                        for (index, package) in packages[visible].iter().enumerate() {
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
        let colors = theme::colors(ui.ctx());
        let selected = self.selected.contains(&package.id);
        let installed = is_installed(package, self.installation_state());
        let selection = ui
            .ctx()
            .animate_bool_responsive(ui.id().with("card-selection"), selected);
        let stroke = colors.border.lerp_to_gamma(colors.accent, selection * 0.75);
        let card = colors
            .card_frame()
            .stroke(Stroke::new(1.0_f32, stroke))
            .show(ui, |ui| {
                ui.set_width((width - 34.0).max(180.0));
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.set_min_height(188.0);
                let top = ui.cursor().top();
                ui.horizontal(|ui| {
                    if !self.package_icon(ui, package, 44.0) {
                        theme::app_icon(ui, &package.id, 44.0);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        self.package_control(ui, package, installed, selected);
                        self.favorite_control(ui, package);
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
                let version = if installed {
                    self.effective_state
                        .get(&package.id)
                        .map(|entry| entry.version.as_str())
                        .unwrap_or(&package.version)
                } else {
                    &package.version
                };
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("{} · {version}", package.publisher))
                            .size(11.0)
                            .color(colors.dim),
                    )
                    .truncate(),
                );
                let mut text = egui::text::LayoutJob::simple(
                    package.description.clone(),
                    egui::FontId::proportional(12.0),
                    colors.muted,
                    ui.available_width(),
                );
                text.wrap.max_rows = 2;
                ui.label(text);
                ui.add_space((top + 155.0 - ui.cursor().top()).max(4.0));
                ui.horizontal(|ui| {
                    theme::pill(
                        ui,
                        if package.is_manual() {
                            "Сайт"
                        } else if package.kind == PackageKind::Addon {
                            "Аддон"
                        } else if package.winget_repository()
                            == crate::discovery::WingetRepository::Msstore
                        {
                            "Store"
                        } else if package.winget_id().is_some() {
                            "WinGet"
                        } else if matches!(
                            package.install,
                            Some(InstallSpec::Zip { .. } | InstallSpec::Portable { .. })
                        ) {
                            "Файлы"
                        } else if matches!(package.install, Some(InstallSpec::Interactive { .. })) {
                            "Мастер"
                        } else {
                            "Программа"
                        },
                        if package.kind == PackageKind::Addon {
                            colors.violet
                        } else {
                            colors.muted
                        },
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Подробнее").size(12.0).color(colors.muted),
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
        let hover = ui
            .ctx()
            .animate_bool_responsive(ui.id().with("card-hover"), card.response.contains_pointer());
        if hover > 0.0 && !selected {
            ui.painter().rect_stroke(
                card.response.rect,
                10,
                Stroke::new(
                    1.0_f32,
                    colors.border.lerp_to_gamma(colors.muted, hover * 0.45),
                ),
                egui::StrokeKind::Inside,
            );
        }
    }

    /// Draws the real program icon when the package is already on this PC.
    ///
    /// Returns `true` when an icon was painted so the letter tile is skipped.
    pub(super) fn package_icon(&mut self, ui: &mut egui::Ui, package: &Package, size: f32) -> bool {
        self.icons.show(
            ui,
            self.package_icons
                .get(&package.id)
                .map(|path| path.as_path()),
            size,
        )
    }

    fn package_control(
        &mut self,
        ui: &mut egui::Ui,
        package: &Package,
        installed: bool,
        selected: bool,
    ) {
        use theme::PackageStatus;
        let status = if selected {
            PackageStatus::Selected
        } else if installed {
            PackageStatus::Installed
        } else if package.is_manual() {
            PackageStatus::Manual
        } else if self.loading && !package.ready() {
            PackageStatus::Loading
        } else if !package.ready() {
            PackageStatus::Unavailable
        } else {
            PackageStatus::Available
        };
        let enabled = if selected {
            !self.queue_active
        } else {
            self.list_actions_enabled()
        };
        if theme::package_status(ui, status, enabled).clicked()
            && !self.selected.remove(&package.id)
        {
            self.selected.insert(package.id.clone());
        }
    }

    fn favorite_control(&mut self, ui: &mut egui::Ui, package: &Package) {
        if theme::favorite_button(
            ui,
            self.settings.favorites.contains(&package.id),
            &package.name,
        )
        .clicked()
        {
            self.toggle_favorite(&package.id);
        }
    }

    fn package_list(&mut self, ui: &mut egui::Ui, packages: &[Package]) {
        let colors = theme::colors(ui.ctx());
        colors.card_frame().inner_margin(0).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let (header, _) =
                ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::hover());
            let font = egui::FontId::proportional(12.0);
            ui.painter().text(
                header.left_center() + Vec2::new(62.0, 0.0),
                egui::Align2::LEFT_CENTER,
                "Программа",
                font.clone(),
                colors.muted,
            );
            if header.width() > 640.0 {
                ui.painter().text(
                    header.right_center() - Vec2::new(222.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    "Версия",
                    font.clone(),
                    colors.muted,
                );
            }
            ui.painter().text(
                header.right_center() - Vec2::new(31.0, 0.0),
                egui::Align2::CENTER_CENTER,
                "Статус",
                font,
                colors.muted,
            );
            egui::ScrollArea::vertical()
                .id_salt((
                    "catalog-list-scroll",
                    &self.filter,
                    self.settings.sort_by_name,
                ))
                .auto_shrink([false, false])
                .show_rows(ui, 68.0, packages.len(), |ui, visible| {
                    for package in &packages[visible] {
                        ui.push_id(&package.id, |ui| {
                            self.package_row(ui, package);
                        });
                    }
                });
        });
    }

    fn package_row(&mut self, ui: &mut egui::Ui, package: &Package) {
        let colors = theme::colors(ui.ctx());
        let selected = self.selected.contains(&package.id);
        let installed = is_installed(package, self.installation_state());
        let width = ui.available_width();
        let selection = ui
            .ctx()
            .animate_bool_responsive(ui.id().with("row-selected"), selected);
        let bounds = egui::Rect::from_min_size(ui.next_widget_position(), Vec2::new(width, 68.0));
        let hover = ui
            .ctx()
            .animate_bool_responsive(ui.id().with("row-hover"), ui.rect_contains_pointer(bounds));
        let row = egui::Frame::new()
            .fill(
                colors
                    .surface
                    .lerp_to_gamma(colors.raised, hover * 0.45)
                    .lerp_to_gamma(colors.accent, selection * 0.07),
            )
            .inner_margin(Margin::symmetric(14, 13))
            .show(ui, |ui| {
                ui.set_width((width - 28.0).max(150.0));
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    if !self.package_icon(ui, package, 36.0) {
                        theme::app_icon(ui, &package.id, 36.0);
                    }
                    let version = self
                        .effective_state
                        .get(&package.id)
                        .filter(|_| installed)
                        .map_or(package.version.as_str(), |entry| &entry.version)
                        .to_owned();
                    let wide = width > 640.0;
                    let text_width =
                        (ui.available_width() - 92.0 - if wide { 128.0 } else { 0.0 }).max(70.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(text_width, 42.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_size(Vec2::new(text_width, 42.0));
                            ui.spacing_mut().item_spacing.y = 4.0;
                            if ui
                                .add(
                                    egui::Label::new(RichText::new(&package.name).strong())
                                        .truncate()
                                        .sense(Sense::click()),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text(&package.description)
                                .clicked()
                            {
                                self.details = Some(package.id.clone());
                            }
                            ui.add(
                                egui::Label::new(
                                    RichText::new(if wide {
                                        package.publisher.clone()
                                    } else {
                                        format!("{} · {version}", package.publisher)
                                    })
                                    .size(12.0)
                                    .color(colors.dim),
                                )
                                .truncate(),
                            );
                        },
                    );
                    if wide {
                        ui.allocate_ui_with_layout(
                            Vec2::new(116.0, 42.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(&version).size(12.0).color(colors.muted),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(&version);
                            },
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        self.package_control(ui, package, installed, selected);
                        self.favorite_control(ui, package);
                    });
                });
            });
        ui.painter().line_segment(
            [
                row.response.rect.left_bottom() + Vec2::new(14.0, 0.0),
                row.response.rect.right_bottom() - Vec2::new(14.0, 0.0),
            ],
            Stroke::new(1.0_f32, colors.border.gamma_multiply(0.55)),
        );
    }

    pub(super) fn details_panel(&mut self, ctx: &egui::Context) {
        let colors = theme::colors(ctx);
        let Some(package) = self
            .details
            .as_ref()
            .and_then(|id| self.document.catalog.package(id))
            .cloned()
        else {
            return;
        };
        egui::SidePanel::right("package-details").exact_width(308.0).resizable(false).frame(egui::Frame::new().fill(colors.sidebar).inner_margin(Margin::symmetric(22, 26))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("О программе").size(12.0).color(colors.dim));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| { if ui.small_button("×").clicked() { self.details = None; } });
            });
            ui.add_space(18.0);
            egui::ScrollArea::vertical().show(ui, |ui| {
                if !self.package_icon(ui, &package, 64.0) {
                    theme::app_icon(ui, &package.id, 64.0);
                }
                ui.horizontal(|ui| {
                    self.favorite_control(ui, &package);
                    ui.label(RichText::new(if self.settings.favorites.contains(&package.id) { "В избранном" } else { "В избранное" }).size(12.0).color(colors.muted));
                });
                ui.add_space(10.0);
                ui.label(RichText::new(&package.name).size(22.0).strong());
                ui.label(RichText::new(&package.publisher).color(colors.muted));
                ui.add_space(12.0);
                ui.label(RichText::new(&package.description).size(13.0).color(colors.muted));
                ui.add_space(12.0);
                detail_line(ui, "Версия", &package.version);
                detail_line(ui, "Группа", &self.document.catalog.category_path(&package.category));
                if let Some(artifact) = &package.artifact {
                     detail_line(ui, "Размер", &if artifact.size == 0 { "Уточняется при загрузке".into() } else { theme::bytes(artifact.size) });
                    detail_line(ui, "Файл", &artifact.file_name);
                    detail_line(ui, "Источник", if self.document.local_root.is_some() && artifact.local_path.is_some() { "Локальная папка" } else if artifact.drive_file_id.is_some() { "Google Drive" } else { "HTTPS" });
                    if artifact.sha256.is_empty() {
                        ui.label(RichText::new("Проверка подписи Windows перед установкой. SHA-256 будет вычислен при загрузке.").size(11.0).color(colors.muted));
                    } else { ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("SHA-256  {}…", &artifact.sha256[..12])).monospace().size(10.0).color(colors.dim)).on_hover_text(&artifact.sha256);
                        if ui.small_button("Копия").clicked() { ui.ctx().copy_text(artifact.sha256.clone()); }
                    }); }
                }
                self.install_details(ui, &package);
                if let Some(entry) = self.effective_state.get(&package.id) { detail_line(ui, "На этом ПК", &entry.version); }
                if let Some(crate::discovery::Source::Manual { url, instructions }) = &package.source {
                    ui.add_space(10.0);
                    ui.label(RichText::new(instructions).color(colors.orange).size(12.0));
                    ui.hyperlink_to("Открыть официальный источник", url);
                }
                if let Some(message) = self.document.diagnostics.get(&package.id) { ui.label(RichText::new(message).size(12.0).color(colors.orange)); }
                if !package.depends_on.is_empty() {
                    ui.add_space(12.0);
                    ui.label(RichText::new("Зависимости").size(12.0).color(colors.dim));
                    for dependency in &package.depends_on {
                        let name = self.document.catalog.package(dependency).map(|p| p.name.as_str()).unwrap_or(dependency);
                        if ui.button(name).clicked() { self.details = Some(dependency.clone()); }
                    }
                }
                let related: Vec<_> = self.document.catalog.packages.iter().filter(|p| p.depends_on.contains(&package.id)).cloned().collect();
                if !related.is_empty() {
                    ui.add_space(12.0);
                    ui.label(RichText::new("Дополнения и утилиты").size(12.0).color(colors.dim));
                    for addon in related {
                        ui.horizontal(|ui| {
                            let mut checked = self.selected.contains(&addon.id);
                            if ui.add_enabled(addon.ready() && !is_installed(&addon, self.installation_state()) && !self.queue_active && !self.loading && !self.programs_loading, egui::Checkbox::without_text(&mut checked)).changed() {
                                if checked { self.selected.insert(addon.id.clone()); } else { self.selected.remove(&addon.id); }
                            }
                            if ui.add(egui::Label::new(&addon.name).truncate().sense(Sense::click())).clicked() { self.details = Some(addon.id.clone()); }
                        });
                    }
                }
                ui.add_space(12.0);
                ui.horizontal_wrapped(|ui| { for tag in &package.tags { theme::pill(ui, tag, colors.muted); } });
                if let Some(homepage) = &package.homepage { ui.add_space(10.0); ui.hyperlink_to("Сайт разработчика", homepage); }
                ui.add_space(18.0);
                if is_installed(&package, self.installation_state()) {
                    theme::pill(ui, "Установлено", colors.accent);
                } else if !package.enabled && !self.document.diagnostics.contains_key(&package.id) {
                    ui.label(RichText::new("Это пример карточки. Добавь установщик и ссылку в свой каталог, чтобы сделать пакет доступным.").color(colors.orange).size(12.0));
                } else if !package.is_manual() && ui.add_enabled(package.ready() && !self.queue_active && !self.loading && !self.programs_loading, colors.primary(if self.selected.contains(&package.id) { "Убрать из выбранного" } else { "Добавить к установке" })).clicked()
                    && !self.selected.remove(&package.id) {
                    self.selected.insert(package.id.clone());
                }
            });
        });
    }

    fn install_details(&self, ui: &mut egui::Ui, package: &Package) {
        let colors = theme::colors(ui.ctx());
        if let Some(spec) = &package.install {
            ui.add_space(8.0);
            detail_line(
                ui,
                "Установка",
                if matches!(spec, InstallSpec::Winget) {
                    "По рецепту WinGet"
                } else if matches!(spec, InstallSpec::Interactive { .. }) {
                    "Штатный мастер"
                } else if spec.requires_admin() {
                    "С запросом UAC"
                } else {
                    "Текущий пользователь"
                },
            );
            let description = match spec {
                InstallSpec::Winget => format!(
                    "WinGet · {} · {} · актуальная версия при установке",
                    package.winget_repository().as_str(),
                    package.winget_id().unwrap_or_default()
                ),
                InstallSpec::VscodeExtension { extension_id } => {
                    format!("VS Code Marketplace · {extension_id}")
                }
                InstallSpec::Interactive { .. } => "Штатный мастер установки".into(),
                InstallSpec::Portable { destination } => {
                    format!("Portable · {:?}/{}", destination.root, destination.path)
                }
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
                    .color(colors.dim),
            );
        }
    }
}

fn detail_line(ui: &mut egui::Ui, label: &str, value: &str) {
    let colors = theme::colors(ui.ctx());
    ui.label(RichText::new(label).size(10.0).color(colors.dim));
    ui.label(RichText::new(value).size(12.0));
}
