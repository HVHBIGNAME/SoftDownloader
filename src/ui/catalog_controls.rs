use eframe::egui::{self, Align, Layout, Margin, RichText, Stroke, Vec2};

use super::app::SoftDownloaderApp;
use super::theme::{self, Icon};
use crate::catalog::PackageKind;
use crate::catalog_filter::QuickFilter;
use crate::storage::CatalogLayout;

impl SoftDownloaderApp {
    pub(super) fn catalog_heading(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        let title = if self.filter.favorites_only {
            "Избранное"
        } else if self.filter.quick == QuickFilter::Selected {
            "Ваш выбор"
        } else {
            "Каталог"
        };
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 230.0).max(90.0);
            ui.allocate_ui_with_layout(
                Vec2::new(width, 36.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.add(egui::Label::new(RichText::new(title).size(27.0).strong()).truncate())
                        .on_hover_text(title);
                },
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.loading || self.programs_loading {
                    ui.add(egui::Spinner::new().size(14.0))
                        .on_hover_text("Обновляем каталог и список программ на ПК");
                } else if ui
                    .add_enabled(
                        !self.queue_active && !self.list_busy,
                        egui::Button::new("Обновить"),
                    )
                    .clicked()
                {
                    self.start_catalog_load(self.active_source.clone(), true);
                }
                if ui
                    .add_enabled(
                        self.list_actions_enabled(),
                        egui::Button::new("Импорт списка"),
                    )
                    .on_hover_text("Восстановить выбор из JSON-файла · Ctrl+O")
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
            RichText::new(if !self.document.diagnostics.is_empty() {
                "Некоторые источники недоступны. Причина — в подробностях программы."
            } else if self.filter.favorites_only {
                "Ваши программы. Звёздочка сохраняет их в избранное."
            } else if self.filter.quick == QuickFilter::Selected {
                "Уберите лишнее или сохраните набор для другого компьютера."
            } else {
                "Найдите программу и отметьте её для установки."
            })
            .size(13.0)
            .color(if self.document.diagnostics.is_empty() {
                colors.muted
            } else {
                colors.orange
            }),
        );
        ui.add_space(6.0);
        if let Some(imported) = &self.imported_selection {
            let label = format!("Из списка: {}", imported.name);
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::Label::new(RichText::new(label).color(colors.accent).size(12.0))
                        .truncate(),
                );
                if ui.small_button("Весь каталог").clicked() {
                    self.imported_selection = None;
                }
            });
        }
    }

    pub(super) fn search_and_filters(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        egui::Frame::new()
            .fill(colors.surface)
            .stroke(Stroke::new(1.0_f32, colors.border))
            .corner_radius(9)
            .inner_margin(Margin::symmetric(14, 7))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    theme::icon(ui, Icon::Search, colors.muted);
                    let width = (ui.available_width() - 58.0).max(70.0);
                    let input = ui.add(
                        egui::TextEdit::singleline(&mut self.filter.query)
                            .id_salt("catalog-search")
                            .hint_text("Название, тег или издатель…")
                            .desired_width(width)
                            .frame(false),
                    );
                    if self.focus_search {
                        input.request_focus();
                        self.focus_search = false;
                    }
                    if self.filter.query.is_empty() {
                        ui.label(RichText::new("Ctrl K").size(10.0).color(colors.dim));
                    } else if ui
                        .small_button("×")
                        .on_hover_text("Очистить поиск · Esc")
                        .clicked()
                    {
                        self.filter.query.clear();
                    }
                });
            });
        ui.add_space(2.0);
        ui.horizontal_wrapped(|ui| {
            for (label, filter) in [
                ("Все".into(), QuickFilter::All),
                ("Доступные".into(), QuickFilter::Available),
                (
                    format!("Выбрано {}", self.selected.len()),
                    QuickFilter::Selected,
                ),
            ] {
                if theme::filter_chip(ui, label, self.filter.quick == filter).clicked() {
                    if filter == QuickFilter::Selected {
                        self.review_selection();
                    } else {
                        self.filter.quick = filter;
                    }
                }
            }
            self.category_picker(ui);
            self.catalog_view_menu(ui);
        });
        if self.filter.has_constraints() {
            ui.horizontal_wrapped(|ui| {
                if let Some(id) = &self.filter.category {
                    ui.label(
                        RichText::new(self.document.catalog.category_path(id))
                            .size(11.0)
                            .color(colors.dim),
                    );
                }
                if let Some(kind) = self.filter.kind {
                    ui.label(
                        RichText::new(if kind == PackageKind::Addon {
                            "Только аддоны"
                        } else {
                            "Только программы"
                        })
                        .size(11.0)
                        .color(colors.dim),
                    );
                }
                if ui.small_button("Сбросить фильтры").clicked() {
                    self.filter.reset_constraints();
                }
            });
        }
        ui.add_space(4.0);
    }

    fn category_picker(&mut self, ui: &mut egui::Ui) {
        let label = self
            .filter
            .category
            .as_ref()
            .and_then(|id| {
                self.document
                    .catalog
                    .categories
                    .iter()
                    .find(|category| &category.id == id)
            })
            .map_or("Все категории", |category| {
                category.name.as_str()
            });
        egui::ComboBox::from_id_salt("catalog-category")
            .width(184.0)
            .height(310.0)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .selected_text(label)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.filter.category, None, "Все категории");
                for (category, depth) in self.document.catalog.category_tree() {
                    let label = format!("{}{}", "    ".repeat(depth), category.name);
                    ui.selectable_value(
                        &mut self.filter.category,
                        Some(category.id.clone()),
                        label,
                    );
                }
            });
    }

    fn catalog_view_menu(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        let old_layout = self.settings.catalog_layout;
        let old_sort = self.settings.sort_by_name;
        ui.menu_button("Вид", |ui| {
            ui.set_width(232.0);
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            menu_heading(ui, "Отображение", colors);
            for (layout, label) in [
                (CatalogLayout::Grid, "Карточки"),
                (CatalogLayout::List, "Компактный список"),
            ] {
                if menu_option(ui, label, self.settings.catalog_layout == layout, colors) {
                    self.settings.catalog_layout = layout;
                }
            }
            menu_separator(ui, colors);
            if menu_option(ui, "По алфавиту", self.settings.sort_by_name, colors) {
                self.settings.sort_by_name = !self.settings.sort_by_name;
            }
            menu_separator(ui, colors);
            menu_heading(ui, "Тип пакета", colors);
            for (kind, label) in [
                (None, "Программы и аддоны"),
                (Some(PackageKind::App), "Только программы"),
                (Some(PackageKind::Addon), "Только аддоны"),
            ] {
                if menu_option(ui, label, self.filter.kind == kind, colors) {
                    self.filter.kind = kind;
                }
            }
        });
        if (old_layout != self.settings.catalog_layout || old_sort != self.settings.sort_by_name)
            && !self.save_preferences()
        {
            self.settings.catalog_layout = old_layout;
            self.settings.sort_by_name = old_sort;
        }
    }
}

fn menu_heading(ui: &mut egui::Ui, label: &str, colors: theme::Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(232.0, 28.0), egui::Sense::hover());
    ui.painter().text(
        rect.left_center() + Vec2::new(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(12.0),
        colors.muted,
    );
}

fn menu_separator(ui: &mut egui::Ui, colors: theme::Palette) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(232.0, 12.0), egui::Sense::hover());
    ui.painter().line_segment(
        [
            rect.left_center() + Vec2::new(10.0, 0.0),
            rect.right_center() - Vec2::new(10.0, 0.0),
        ],
        Stroke::new(1.0_f32, colors.border),
    );
}

fn menu_option(ui: &mut egui::Ui, label: &str, selected: bool, colors: theme::Palette) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(232.0, 34.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            selected,
            label,
        )
    });
    if response.hovered() || response.has_focus() || selected {
        ui.painter().rect_filled(
            rect,
            5,
            if response.hovered() || response.has_focus() {
                colors.raised
            } else {
                colors.surface.lerp_to_gamma(colors.accent, 0.07)
            },
        );
    }
    ui.painter().text(
        rect.left_center() + Vec2::new(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(14.0),
        colors.text,
    );
    if selected {
        let center = rect.right_center() - Vec2::new(18.0, 0.0);
        ui.painter().add(egui::Shape::line(
            vec![
                center + Vec2::new(-4.0, 0.0),
                center + Vec2::new(-1.0, 3.0),
                center + Vec2::new(5.0, -3.0),
            ],
            Stroke::new(1.6_f32, colors.accent),
        ));
    }
    response.clicked()
}
