use std::collections::BTreeSet;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use anyhow::Result;
use eframe::egui::{self, Align, Color32, Layout, Margin, RichText, Stroke, Vec2};

use super::theme::{self, Icon};
use crate::catalog::{CatalogDocument, Package, PackageKind};
use crate::engine::{Engine, JobStatus, WorkerEvent};
use crate::planner::create_plan;
use crate::storage::{Library, Settings, Store};
use crate::uninstall::{self, InstalledProgram};

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Page {
    Catalog,
    Installed,
    Queue,
    Settings,
}

pub(super) struct QueueItem {
    pub id: String,
    pub name: String,
    pub version: String,
    pub size: u64,
    pub removal: bool,
    pub status: JobStatus,
    pub downloaded: u64,
    pub bytes_per_second: f64,
}

pub struct SoftDownloaderApp {
    pub(super) document: CatalogDocument,
    pub(super) store: Store,
    pub(super) engine: Engine,
    events: Receiver<WorkerEvent>,
    pub(super) library: Library,
    pub(super) programs: Vec<InstalledProgram>,
    pub(super) programs_loading: bool,
    pub(super) installed_query: String,
    pub(super) selected_removals: BTreeSet<String>,
    settings: Settings,
    pub(super) source_input: String,
    pub(super) active_source: String,
    pending_source: Option<String>,
    load_generation: u64,
    pub(super) loading: bool,
    pub(super) page: Page,
    pub(super) query: String,
    pub(super) kind: Option<PackageKind>,
    pub(super) category: Option<String>,
    pub(super) sort_by_name: bool,
    pub(super) selected: BTreeSet<String>,
    pub(super) details: Option<String>,
    pub(super) queue: Vec<QueueItem>,
    pub(super) queue_active: bool,
    pub(super) confirm_plan: Option<Vec<Package>>,
    pub(super) error: Option<String>,
    close_dialog: bool,
    close_when_done: bool,
}

impl SoftDownloaderApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        store: Store,
        catalog_override: Option<String>,
    ) -> Result<Self> {
        theme::apply(&cc.egui_ctx);
        let settings = store.load_settings()?;
        let library = store.load_library()?;
        let (engine, events) = Engine::new()?;
        let source = catalog_override.unwrap_or_else(|| {
            if settings.catalog_source.is_empty() {
                option_env!("SOFTDOWNLOADER_CATALOG_URL")
                    .unwrap_or_default()
                    .to_owned()
            } else {
                settings.catalog_source.clone()
            }
        });
        let mut app = Self {
            document: CatalogDocument::builtin()?,
            store,
            engine,
            events,
            library,
            programs: Vec::new(),
            programs_loading: true,
            installed_query: String::new(),
            selected_removals: BTreeSet::new(),
            settings,
            source_input: source.clone(),
            active_source: String::new(),
            pending_source: None,
            load_generation: 0,
            loading: false,
            page: Page::Catalog,
            query: String::new(),
            kind: None,
            category: None,
            sort_by_name: false,
            selected: BTreeSet::new(),
            details: None,
            queue: Vec::new(),
            queue_active: false,
            confirm_plan: None,
            error: None,
            close_dialog: false,
            close_when_done: false,
        };
        app.engine.scan_programs(app.library.clone());
        app.load_source(source);
        Ok(app)
    }

    pub(super) fn load_source(&mut self, source: String) {
        if self.queue_active {
            return;
        }
        self.loading = true;
        self.error = None;
        self.pending_source = Some(source.clone());
        self.load_generation = self.engine.load_catalog(source);
    }

    fn poll_events(&mut self) {
        let events: Vec<_> = self.events.try_iter().collect();
        for event in events {
            match event {
                WorkerEvent::Catalog { generation, result }
                    if generation == self.load_generation =>
                {
                    self.loading = false;
                    match result {
                        Ok(document) => {
                            self.document = document;
                            self.active_source = self.pending_source.take().unwrap_or_default();
                            self.settings.catalog_source = self.active_source.clone();
                            if let Err(error) = self.store.save_settings(&self.settings) {
                                self.error = Some(format!("{error:#}"));
                            }
                            self.selected.clear();
                            self.details = None;
                            self.category = None;
                        }
                        Err(error) => {
                            self.error = Some(error);
                            self.pending_source = None;
                        }
                    }
                }
                WorkerEvent::Catalog { .. } => {}
                WorkerEvent::Status { id, status } => {
                    if let Some(item) = self.queue.iter_mut().find(|item| item.id == id) {
                        item.status = status;
                    }
                }
                WorkerEvent::Progress { id, progress } => {
                    if let Some(item) = self.queue.iter_mut().find(|item| item.id == id) {
                        item.downloaded = progress.downloaded;
                        if progress.total > 0 {
                            item.size = progress.total;
                        }
                        item.bytes_per_second = progress.bytes_per_second;
                        item.status = if progress.verifying {
                            JobStatus::Verifying
                        } else {
                            JobStatus::Downloading
                        };
                    }
                }
                WorkerEvent::Installed(entry) => {
                    self.library.insert(entry.id.clone(), entry);
                }
                WorkerEvent::Programs(result) => {
                    self.programs_loading = false;
                    match result {
                        Ok(programs) => {
                            self.programs = programs;
                            self.selected_removals
                                .retain(|id| self.programs.iter().any(|p| &p.id == id));
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                WorkerEvent::Removed { id, managed_ids } => {
                    self.programs.retain(|program| program.id != id);
                    for id in managed_ids {
                        self.library.remove(&id);
                    }
                }
                WorkerEvent::Warning(message) => {
                    self.error = Some(message);
                }
                WorkerEvent::QueueFinished => {
                    self.queue_active = false;
                    self.refresh_programs();
                }
            }
        }
    }

    pub(super) fn prepare_install(&mut self, selected: BTreeSet<String>) {
        if self.queue_active || self.loading {
            return;
        }
        match create_plan(
            &self.document.catalog,
            &selected,
            &self.installation_state(),
        ) {
            Ok(plan) if !plan.is_empty() => self.confirm_plan = Some(plan),
            Ok(_) => {
                self.error = Some("Выбранные версии уже установлены через SoftDownloader".into())
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    fn start_plan(&mut self, plan: Vec<Package>) {
        match self.engine.start(
            plan.clone(),
            &self.document,
            self.library.clone(),
            self.store.clone(),
        ) {
            Ok(()) => {
                self.queue = plan
                    .into_iter()
                    .map(|package| QueueItem {
                        id: package.id,
                        name: package.name,
                        version: package.version,
                        size: package
                            .artifact
                            .as_ref()
                            .map(|a| a.size)
                            .unwrap_or_default(),
                        removal: false,
                        status: JobStatus::Queued,
                        downloaded: 0,
                        bytes_per_second: 0.0,
                    })
                    .collect();
                self.queue_active = true;
                self.selected.clear();
                self.details = None;
                self.page = Page::Queue;
                self.error = None;
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    pub(super) fn installation_state(&self) -> Library {
        uninstall::installation_state(&self.document.catalog, &self.library, &self.programs)
    }

    pub(super) fn refresh_programs(&mut self) {
        if self.programs_loading {
            return;
        }
        self.programs_loading = true;
        self.engine.scan_programs(self.library.clone());
    }

    pub(super) fn remove_programs(&mut self, ids: &BTreeSet<String>) {
        if self.queue_active {
            return;
        }
        let programs: Vec<_> = self
            .programs
            .iter()
            .filter(|p| ids.contains(&p.id))
            .cloned()
            .collect();
        if programs.is_empty() {
            return;
        }
        let programs = match crate::planner::removal_plan(
            &self.document.catalog,
            &self.installation_state(),
            programs,
        ) {
            Ok(programs) => programs,
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                return;
            }
        };
        match self.engine.remove(
            programs.clone(),
            self.library.clone(),
            self.document.catalog.clone(),
            self.store.clone(),
        ) {
            Ok(()) => {
                self.queue = programs
                    .into_iter()
                    .map(|program| QueueItem {
                        id: program.id,
                        name: program.name,
                        version: program.version,
                        size: 0,
                        removal: true,
                        status: JobStatus::Queued,
                        downloaded: 0,
                        bytes_per_second: 0.0,
                    })
                    .collect();
                self.queue_active = true;
                self.selected_removals.clear();
                self.page = Page::Queue;
                self.details = None;
                self.error = None;
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("navigation")
            .exact_width(224.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::SIDEBAR)
                    .inner_margin(Margin::symmetric(14, 22)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    theme::logo(ui, 34.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        ui.label(RichText::new("SoftDownloader").strong().size(17.0));
                        ui.label(
                            RichText::new("ТВОЯ КОЛЛЕКЦИЯ СОФТА")
                                .size(8.0)
                                .color(theme::DIM),
                        );
                    });
                });
                ui.add_space(28.0);
                ui.label(RichText::new("БИБЛИОТЕКА").size(10.0).color(theme::DIM));
                self.main_navigation(ui);
                ui.add_space(22.0);
                ui.label(RichText::new("ГРУППЫ").size(10.0).color(theme::DIM));
                self.category_navigation(ui);
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.label(
                        RichText::new(concat!("v", env!("CARGO_PKG_VERSION"), "  /  Windows"))
                            .size(10.0)
                            .color(theme::DIM),
                    );
                    if theme::nav(
                        ui,
                        "Настройки",
                        Icon::Settings,
                        self.page == Page::Settings,
                        None,
                    ) {
                        self.page = Page::Settings;
                        self.details = None;
                    }
                    ui.add_space(6.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), 74.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            egui::Frame::new()
                                .fill(theme::SURFACE)
                                .corner_radius(10)
                                .inner_margin(12)
                                .show(ui, |ui| {
                                    ui.set_width(170.0);
                                    ui.with_layout(Layout::top_down(Align::Min), |ui| {
                                        ui.label(
                                            RichText::new(if self.active_source.is_empty() {
                                                "Официальные источники"
                                            } else if self.document.local_root.is_some() {
                                                "Локальная коллекция"
                                            } else {
                                                "Онлайн-коллекция"
                                            })
                                            .size(12.0)
                                            .strong(),
                                        );
                                        ui.label(
                                            RichText::new(if self.loading {
                                                "Обновляем каталог…"
                                            } else if !self.document.diagnostics.is_empty() {
                                                "Есть недоступные источники"
                                            } else {
                                                "Каталог подключён"
                                            })
                                            .size(11.0)
                                            .color(
                                                if !self.document.diagnostics.is_empty() {
                                                    theme::ORANGE
                                                } else {
                                                    theme::ACCENT
                                                },
                                            ),
                                        );
                                    });
                                });
                        },
                    );
                });
            });
    }

    fn main_navigation(&mut self, ui: &mut egui::Ui) {
        let catalog_active = self.page == Page::Catalog && self.category.is_none();
        if theme::nav(ui, "Каталог", Icon::Grid, catalog_active, None) {
            self.show_catalog(None);
        }
        if theme::nav(
            ui,
            "Установлено",
            Icon::Check,
            self.page == Page::Installed,
            Some(self.programs.len()),
        ) {
            self.page = Page::Installed;
            self.details = None;
        }
        let count = self
            .queue
            .iter()
            .filter(|item| !item.status.is_terminal())
            .count();
        if theme::nav(
            ui,
            "Очередь",
            Icon::Download,
            self.page == Page::Queue,
            (count > 0).then_some(count),
        ) {
            self.page = Page::Queue;
            self.details = None;
        }
    }

    fn show_catalog(&mut self, kind: Option<PackageKind>) {
        self.page = Page::Catalog;
        self.kind = kind;
        self.category = None;
    }

    fn category_navigation(&mut self, ui: &mut egui::Ui) {
        let categories = self.document.catalog.categories.clone();
        egui::ScrollArea::vertical()
            .id_salt("sidebar-groups")
            .max_height((ui.available_height() - 160.0).max(60.0))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for category in categories {
                    let active = self.page == Page::Catalog
                        && self.category.as_deref() == Some(&category.id);
                    let count = self
                        .document
                        .catalog
                        .packages
                        .iter()
                        .filter(|p| {
                            self.document
                                .catalog
                                .category_contains(&category.id, &p.category)
                        })
                        .count();
                    if count == 0 {
                        continue;
                    }
                    ui.horizontal(|ui| {
                        if category.parent.is_some() {
                            ui.add_space(14.0);
                        }
                        if theme::nav(ui, &category.name, Icon::Folder, active, Some(count)) {
                            self.page = Page::Catalog;
                            self.category = Some(category.id.clone());
                            self.kind = None;
                        }
                    });
                }
            });
    }

    fn selection_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("selection-bar")
            .exact_height(76.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::SIDEBAR)
                    .inner_margin(Margin::symmetric(28, 16))
                    .stroke(Stroke::new(1.0, theme::BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    if self.queue_active {
                        ui.spinner();
                        let ready = self.queue.iter().filter(|i| i.status.is_terminal()).count();
                        ui.label(
                            RichText::new(format!("Выполнено {ready} из {}", self.queue.len()))
                                .strong(),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Открыть очередь").clicked() {
                                self.page = Page::Queue;
                            }
                        });
                    } else if self.page == Page::Installed {
                        let count = self.selected_removals.len();
                        ui.label(
                            RichText::new(if count == 0 {
                                "Выбери программы для удаления".into()
                            } else {
                                format!("К удалению: {count}")
                            })
                            .color(theme::MUTED),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    count > 0 && !self.programs_loading,
                                    egui::Button::new(
                                        RichText::new("Удалить выбранные")
                                            .strong()
                                            .color(theme::BG),
                                    )
                                    .fill(theme::RED)
                                    .min_size(Vec2::new(0.0, 40.0)),
                                )
                                .clicked()
                            {
                                self.remove_programs(&self.selected_removals.clone());
                            }
                        });
                    } else if self.selected.is_empty() {
                        theme::icon(ui, Icon::Check, theme::DIM);
                        ui.label(
                            RichText::new("Твоё рабочее пространство начинается здесь")
                                .color(theme::MUTED)
                                .size(13.0),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new("Выбери нужные программы")
                                    .size(12.0)
                                    .color(theme::DIM),
                            );
                        });
                    } else {
                        let plan = create_plan(
                            &self.document.catalog,
                            &self.selected,
                            &self.installation_state(),
                        );
                        let total: u64 = plan
                            .as_ref()
                            .map(|p| {
                                p.iter()
                                    .filter_map(|p| p.artifact.as_ref())
                                    .map(|a| a.size)
                                    .sum()
                            })
                            .unwrap_or_default();
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(format!("Выбрано: {}", self.selected.len())).strong(),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "{} с учётом зависимостей",
                                    theme::bytes(total)
                                ))
                                .size(11.0)
                                .color(theme::MUTED),
                            );
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    !self.loading,
                                    theme::primary("Установить выбранное  >"),
                                )
                                .clicked()
                            {
                                self.prepare_install(self.selected.clone());
                            }
                            if ui.button("Сбросить").clicked() {
                                self.selected.clear();
                            }
                        });
                    }
                });
            });
    }

    fn notice(&mut self, ui: &mut egui::Ui) {
        if let Some(message) = self.error.clone() {
            egui::Frame::new()
                .fill(theme::RED.gamma_multiply(0.08))
                .stroke(Stroke::new(1.0, theme::RED.gamma_multiply(0.25)))
                .corner_radius(9)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui(
                            Vec2::new((ui.available_width() - 48.0).max(100.0), 0.0),
                            |ui| {
                                ui.label(RichText::new(message).color(theme::RED).size(12.0));
                            },
                        );
                        if ui.small_button("×").clicked() {
                            self.error = None;
                        }
                    });
                });
            ui.add_space(12.0);
        }
    }

    fn confirmation(&mut self, ctx: &egui::Context) {
        let Some(plan) = self.confirm_plan.clone() else {
            return;
        };
        egui::Modal::new(egui::Id::new("confirm-install")).show(ctx, |ui| {
            ui.set_width(510.0);
            theme::heading(ui, "Всё готово к установке", "Зависимости добавлены автоматически. Проверь список.");
            egui::ScrollArea::vertical().max_height(270.0).show(ui, |ui| {
                for (index, package) in plan.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{:02}", index+1)).color(theme::DIM));
                        ui.label(RichText::new(&package.name).strong());
                        ui.label(RichText::new(&package.version).color(theme::MUTED));
                        if package.install.as_ref().is_some_and(|s| s.requires_admin()) { theme::pill(ui, "UAC", theme::ORANGE); }
                    });
                }
            });
            ui.add_space(14.0);
            ui.label(RichText::new("Будут запущены установщики из выбранного каталога с указанными в нём параметрами. Windows запросит права администратора там, где это необходимо.").color(theme::MUTED).size(12.0));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.add(theme::primary(format!("Установить · {}", plan.len()))).clicked() { self.confirm_plan = None; self.start_plan(plan.clone()); }
                if ui.button("Вернуться к выбору").clicked() { self.confirm_plan = None; }
            });
        });
    }

    fn handle_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && self.queue_active {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.close_dialog = true;
        }
        if self.close_when_done && !self.queue_active {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if !self.close_dialog {
            return;
        }
        egui::Modal::new(egui::Id::new("confirm-close")).show(ctx, |ui| {
            ui.set_width(420.0);
            theme::heading(ui, "Очередь ещё работает", "Текущая загрузка будет отменена. Уже запущенному установщику дадим завершить работу.");
            if ui.add(theme::primary("Остановить очередь и закрыть")).clicked() {
                self.engine.cancel(); self.close_when_done = true; self.close_dialog = false;
            }
            if ui.button("Продолжить работу").clicked() { self.close_dialog = false; }
        });
    }
}

impl eframe::App for SoftDownloaderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_events();
        if self.loading || self.queue_active || self.programs_loading {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        self.sidebar(ctx);
        self.selection_bar(ctx);
        if self.page == Page::Catalog {
            self.details_panel(ctx);
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BG)
                    .inner_margin(Margin::symmetric(28, 26)),
            )
            .show(ctx, |ui| {
                self.notice(ui);
                match self.page {
                    Page::Catalog => self.catalog_page(ui),
                    Page::Queue => self.queue_page(ui),
                    Page::Installed => self.installed_page(ui),
                    Page::Settings => self.settings_page(ui),
                }
            });
        self.confirmation(ctx);
        self.handle_close(ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::to_normalized_gamma_f32(theme::BG)
    }
}
