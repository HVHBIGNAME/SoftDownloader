use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use anyhow::Result;
use eframe::egui::{self, Align, Layout, Margin, RichText, Stroke, Vec2};

use super::actions::Notification;
use super::icons::IconLoader;
use super::program_lists::{ImportPreview, ImportedSelection};
use super::sounds::{Cue, SoundPlayer};
use super::theme::{self, Icon};
use crate::catalog::{CatalogDocument, Package, PackageKind};
use crate::catalog_filter::CatalogFilter;
use crate::engine::{Engine, JobStatus, WorkerEvent};
use crate::planner::create_plan;
use crate::storage::{Library, Settings, Store};
use crate::uninstall::InstalledProgram;

#[derive(Clone, Copy, PartialEq, Hash)]
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

#[derive(Clone)]
pub(super) struct InstallConfirmation {
    packages: Vec<Package>,
    requested: BTreeSet<String>,
}

pub struct SoftDownloaderApp {
    pub(super) document: CatalogDocument,
    pub(super) store: Store,
    pub(super) engine: Engine,
    events: Receiver<WorkerEvent>,
    pub(super) library: Library,
    pub(super) effective_state: Library,
    pub(super) inventory_warnings: Vec<String>,
    inventory_unverified: BTreeSet<String>,
    inventory_generation: u64,
    pub(super) programs: Vec<InstalledProgram>,
    pub(super) icons: IconLoader,
    pub(super) package_icons: BTreeMap<String, std::path::PathBuf>,
    pub(super) programs_loading: bool,
    pub(super) installed_query: String,
    pub(super) selected_removals: BTreeSet<String>,
    pub(super) settings: Settings,
    appearance: theme::ThemeTransition,
    pub(super) sounds: SoundPlayer,
    pub(super) settings_section: super::settings_view::SettingsSection,
    pub(super) background: super::background::Background,
    pub(super) background_presets: Vec<crate::backgrounds::VideoPreset>,
    pub(super) background_busy: bool,
    pub(super) background_picking: bool,
    pub(super) background_progress: Option<crate::transfer::Progress>,
    pub(super) background_error: Option<String>,
    pub(super) builtin_source: String,
    pub(super) catalog_status: String,
    pub(super) catalog_warnings: Vec<String>,
    pub(super) list_busy: bool,
    pub(super) import_preview: Option<ImportPreview>,
    pub(super) imported_selection: Option<ImportedSelection>,
    pub(super) message: Option<Notification>,
    pub(super) source_input: String,
    pub(super) active_source: String,
    pending_source: Option<String>,
    pending_setting: Option<String>,
    load_generation: u64,
    pub(super) loading: bool,
    pub(super) page: Page,
    pub(super) filter: CatalogFilter,
    pub(super) focus_search: bool,
    pub(super) selected: BTreeSet<String>,
    pub(super) details: Option<String>,
    pub(super) queue: Vec<QueueItem>,
    pub(super) queue_active: bool,
    pub(super) confirm_plan: Option<InstallConfirmation>,
    pub(super) confirm_removal: Option<Vec<InstalledProgram>>,
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
        theme::initialize(&cc.egui_ctx);
        let settings = store.load_settings()?;
        let appearance = theme::ThemeTransition::new(
            &cc.egui_ctx,
            settings.appearance,
            settings.appearance.current_season(),
            settings.reduced_motion,
        );
        let library = store.load_library()?;
        let (engine, events) = Engine::new()?;
        let builtin_source = crate::config::additional_catalog_url()?;
        let source = catalog_override.unwrap_or_else(|| {
            if settings.catalog_source.is_empty() {
                builtin_source.clone()
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
            effective_state: Library::new(),
            inventory_warnings: Vec::new(),
            inventory_unverified: BTreeSet::new(),
            inventory_generation: 0,
            programs: Vec::new(),
            icons: IconLoader::new(cc.egui_ctx.clone())?,
            package_icons: BTreeMap::new(),
            programs_loading: false,
            installed_query: String::new(),
            selected_removals: BTreeSet::new(),
            settings,
            appearance,
            sounds: SoundPlayer::new(cc.egui_ctx.clone())?,
            settings_section: super::settings_view::SettingsSection::Appearance,
            background: super::background::Background::new(&cc.egui_ctx)?,
            background_presets: crate::backgrounds::presets()?,
            background_busy: false,
            background_picking: false,
            background_progress: None,
            background_error: None,
            builtin_source,
            catalog_status: "Обновление каталога…".into(),
            catalog_warnings: Vec::new(),
            list_busy: false,
            import_preview: None,
            imported_selection: None,
            message: None,
            source_input: source.clone(),
            active_source: String::new(),
            pending_source: None,
            pending_setting: None,
            load_generation: 0,
            loading: false,
            page: Page::Catalog,
            filter: CatalogFilter::default(),
            focus_search: false,
            selected: BTreeSet::new(),
            details: None,
            queue: Vec::new(),
            queue_active: false,
            confirm_plan: None,
            confirm_removal: None,
            error: None,
            close_dialog: false,
            close_when_done: false,
        };
        app.start_catalog_load(source, false);
        Ok(app)
    }

    pub(super) fn load_source(&mut self, source: String) {
        if self.queue_active {
            return;
        }
        self.pending_setting = Some(if source == self.builtin_source {
            String::new()
        } else {
            source.clone()
        });
        self.start_catalog_load(source, true);
    }

    pub(super) fn start_catalog_load(&mut self, source: String, force: bool) {
        if self.queue_active {
            return;
        }
        self.loading = true;
        self.error = None;
        self.pending_source = Some(source.clone());
        self.load_generation = self.engine.load_catalog(source, self.store.clone(), force);
    }

    fn poll_events(&mut self) {
        let events: Vec<_> = self.events.try_iter().collect();
        let changed = events.iter().any(|event| {
            matches!(
                event,
                WorkerEvent::Catalog { .. }
                    | WorkerEvent::Programs { .. }
                    | WorkerEvent::Installed(_)
                    | WorkerEvent::Removed { .. }
                    | WorkerEvent::QueueFinished
            )
        });
        for event in events {
            match event {
                WorkerEvent::Catalog { generation, result }
                    if generation == self.load_generation =>
                {
                    self.receive_catalog(result);
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
                WorkerEvent::Programs { generation, result }
                    if generation == self.inventory_generation =>
                {
                    self.programs_loading = false;
                    match result {
                        Ok(inventory) => {
                            self.programs = inventory.programs;
                            self.package_icons.clear();
                            for program in &self.programs {
                                if let Some(path) = &program.icon_path {
                                    for id in program.package_ids.iter().chain(&program.managed_ids)
                                    {
                                        self.package_icons
                                            .entry(id.clone())
                                            .or_insert_with(|| path.clone());
                                    }
                                }
                            }
                            self.icons.refresh();
                            self.inventory_warnings = inventory.warnings;
                            self.inventory_unverified = inventory.unverified;
                            self.selected_removals
                                .retain(|id| self.programs.iter().any(|p| &p.id == id));
                        }
                        Err(error) => {
                            self.inventory_unverified = self.library.keys().cloned().collect();
                            self.error = Some(error);
                        }
                    }
                }
                WorkerEvent::Programs { .. } => {}
                WorkerEvent::Removed { id, managed_ids } => {
                    self.programs.retain(|program| program.id != id);
                    for id in managed_ids {
                        self.library.remove(&id);
                    }
                }
                WorkerEvent::Warning(message) => {
                    self.error = Some(message);
                }
                WorkerEvent::ProgramList(result) => self.receive_program_list(result),
                WorkerEvent::Background(result) => self.receive_background(result),
                WorkerEvent::BackgroundProgress(progress) => {
                    self.background_progress = Some(progress)
                }
                WorkerEvent::QueueFinished => {
                    self.queue_active = false;
                    let cue = if self
                        .queue
                        .iter()
                        .any(|item| matches!(item.status, JobStatus::Failed(_)))
                    {
                        Cue::Error
                    } else {
                        Cue::Success
                    };
                    self.sounds.play(self.settings.sound, cue);
                    self.refresh_programs();
                }
            }
        }
        if changed {
            self.effective_state = crate::inventory::installation_state(
                &self.document.catalog,
                &self.library,
                &self.programs,
            );
            for (id, entry) in &self.library {
                if self.programs_loading || self.inventory_unverified.contains(id) {
                    self.effective_state
                        .entry(id.clone())
                        .or_insert_with(|| entry.clone());
                }
            }
        }
    }

    fn receive_catalog(&mut self, result: Result<crate::catalog_cache::CatalogLoad, String>) {
        self.loading = false;
        match result {
            Ok(loaded) => {
                self.catalog_status = if loaded.cached {
                    format!("Из кэша · {} мин. назад", loaded.age_seconds / 60)
                } else {
                    "Каталог обновлён".into()
                };
                self.catalog_warnings = loaded.warnings;
                self.document = loaded.document;
                self.active_source = self.pending_source.take().unwrap_or_default();
                if let Some(source) = self.pending_setting.take() {
                    self.settings.catalog_source = source;
                    if let Err(error) = self.store.save_settings(&self.settings) {
                        self.error = Some(format!("{error:#}"));
                    }
                }
                self.selected
                    .retain(|id| self.document.catalog.package(id).is_some());
                self.imported_selection = None;
                self.details = None;
                self.filter.category = None;
            }
            Err(error) => {
                self.catalog_status = "Обновление не удалось".into();
                self.error = Some(error);
                self.pending_source = None;
                self.pending_setting = None;
            }
        }
        self.refresh_programs();
    }

    pub(super) fn prepare_install(&mut self, selected: BTreeSet<String>) {
        if !self.list_actions_enabled() || self.has_modal() {
            return;
        }
        match create_plan(&self.document.catalog, &selected, self.installation_state()) {
            Ok(packages) if !packages.is_empty() => {
                self.confirm_plan = Some(InstallConfirmation {
                    packages,
                    requested: selected,
                });
            }
            Ok(_) => self.error = Some("Выбранные программы уже установлены".into()),
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

    pub(super) fn installation_state(&self) -> &Library {
        &self.effective_state
    }

    pub(super) fn refresh_programs(&mut self) {
        self.programs_loading = true;
        self.inventory_generation = self
            .engine
            .scan_programs(self.library.clone(), self.document.catalog.clone());
    }

    pub(super) fn remove_programs(&mut self, ids: &BTreeSet<String>) {
        if !self.list_actions_enabled() || self.has_modal() {
            return;
        }
        let programs: Vec<_> = self
            .programs
            .iter()
            .filter(|p| ids.contains(&p.id) && p.target.can_remove())
            .cloned()
            .collect();
        if programs.is_empty() {
            return;
        }
        let programs = match crate::planner::removal_plan(
            &self.document.catalog,
            self.installation_state(),
            programs,
        ) {
            Ok(programs) => programs,
            Err(error) => {
                self.error = Some(format!("{error:#}"));
                return;
            }
        };
        self.confirm_removal = Some(programs);
    }

    fn start_removal(&mut self, programs: Vec<InstalledProgram>) {
        if !self.list_actions_enabled() {
            return;
        }
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
        let colors = theme::colors(ctx);
        egui::SidePanel::left("navigation")
            .exact_width(224.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(colors.sidebar)
                    .inner_margin(Margin::symmetric(14, 22)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    theme::logo(ui, 34.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 3.0;
                        ui.label(RichText::new("SoftDownloader").strong().size(17.0));
                        ui.label(
                            RichText::new("Программы для Windows")
                                .size(11.0)
                                .color(colors.dim),
                        );
                    });
                });
                ui.add_space(28.0);
                ui.label(RichText::new("Библиотека").size(12.0).color(colors.dim));
                self.main_navigation(ui);
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.label(
                        RichText::new(concat!("v", env!("CARGO_PKG_VERSION"), "  /  Windows"))
                            .size(10.0)
                            .color(colors.dim),
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
                    ui.add_space(10.0);
                    let warning =
                        !self.catalog_warnings.is_empty() || !self.document.diagnostics.is_empty();
                    ui.label(
                        RichText::new(if self.loading {
                            "Обновление каталога…"
                        } else {
                            &self.catalog_status
                        })
                        .size(11.0)
                        .color(if warning {
                            colors.orange
                        } else {
                            colors.muted
                        }),
                    )
                    .on_hover_text(
                        "Каталог сохраняется на час. Кнопка «Обновить» запрашивает свежие данные.",
                    );
                });
            });
    }

    fn main_navigation(&mut self, ui: &mut egui::Ui) {
        let catalog_active = self.page == Page::Catalog && !self.filter.favorites_only;
        if theme::nav(ui, "Каталог", Icon::Grid, catalog_active, None) {
            self.show_catalog(None);
        }
        let favorites = self
            .document
            .catalog
            .packages
            .iter()
            .filter(|p| self.settings.favorites.contains(&p.id))
            .count();
        if theme::nav(
            ui,
            "Избранное",
            Icon::Star,
            self.page == Page::Catalog && self.filter.favorites_only,
            Some(favorites),
        ) {
            self.show_catalog(None);
            self.filter.favorites_only = true;
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

    pub(super) fn show_catalog(&mut self, kind: Option<PackageKind>) {
        self.page = Page::Catalog;
        self.filter = CatalogFilter {
            kind,
            ..Default::default()
        };
        self.imported_selection = None;
        self.details = None;
    }

    fn notice(&mut self, ui: &mut egui::Ui) {
        let colors = theme::colors(ui.ctx());
        if let Some(message) = self.error.clone() {
            egui::Frame::new()
                .fill(colors.red.gamma_multiply(0.08))
                .stroke(Stroke::new(1.0_f32, colors.red.gamma_multiply(0.25)))
                .corner_radius(9)
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui(
                            Vec2::new((ui.available_width() - 48.0).max(100.0), 0.0),
                            |ui| {
                                ui.label(RichText::new(message).color(colors.red).size(12.0));
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
        let colors = theme::colors(ctx);
        let Some(confirmation) = self.confirm_plan.clone() else {
            return;
        };
        let plan = confirmation.packages;
        let response = egui::Modal::new(egui::Id::new("confirm-install")).frame(colors.card_frame().inner_margin(24)).show(ctx, |ui| {
            ui.set_width(510.0);
            theme::heading(ui, "Всё готово к установке", "Зависимости добавлены автоматически. Проверь список.");
            egui::ScrollArea::vertical().max_height(270.0).show(ui, |ui| {
                for (index, package) in plan.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{:02}", index+1)).color(colors.dim));
                        ui.label(RichText::new(&package.name).strong());
                        ui.label(RichText::new(&package.version).color(colors.muted));
                        if !confirmation.requested.contains(&package.id) { theme::pill(ui, "ЗАВИСИМОСТЬ", colors.violet); }
                        if package.install.as_ref().is_some_and(|s| s.requires_admin()) { theme::pill(ui, "UAC", colors.orange); }
                    });
                }
            });
            ui.add_space(14.0);
            ui.label(RichText::new("Будут запущены установщики из выбранного каталога с указанными в нём параметрами. Windows запросит права администратора там, где это необходимо.").color(colors.muted).size(12.0));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.add(colors.primary(format!("Установить · {}", plan.len()))).clicked() { self.confirm_plan = None; self.start_plan(plan.clone()); }
                if ui.button("Вернуться к выбору").clicked() { self.confirm_plan = None; }
            });
        });
        if response.should_close() {
            self.confirm_plan = None;
        }
    }

    pub(super) fn has_modal(&self) -> bool {
        self.confirm_plan.is_some()
            || self.confirm_removal.is_some()
            || self.import_preview.is_some()
            || self.close_dialog
            || self.background_picking
    }

    fn handle_close(&mut self, ctx: &egui::Context) {
        let colors = theme::colors(ctx);
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
            if ui.add(colors.primary("Остановить очередь и закрыть")).clicked() {
                self.engine.cancel(); self.close_when_done = true; self.close_dialog = false;
            }
            if ui.button("Продолжить работу").clicked() { self.close_dialog = false; }
        });
    }
}

impl eframe::App for SoftDownloaderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.appearance.update(
            ctx,
            self.settings.appearance,
            self.settings.appearance.current_season(),
            self.settings.reduced_motion,
        );
        ctx.request_repaint_after(Duration::from_secs(60));
        let colors = theme::colors(ctx);
        self.poll_events();
        self.background
            .update(ctx, &self.settings.background, self.settings.reduced_motion);
        self.icons.collect(ctx);
        if self.loading
            || self.queue_active
            || self.programs_loading
            || self.list_busy
            || self.background_busy
        {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        self.handle_shortcuts(ctx);
        self.sidebar(ctx);
        self.selection_bar(ctx);
        if self.page == Page::Catalog {
            self.details_panel(ctx);
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(colors.bg)
                    .inner_margin(Margin::symmetric(28, 26)),
            )
            .show(ctx, |ui| {
                self.background.paint(
                    ui,
                    &self.settings.background,
                    self.settings.appearance.current_season(),
                );
                self.notice(ui);
                match self.page {
                    Page::Catalog => self.catalog_page(ui),
                    Page::Queue => self.queue_page(ui),
                    Page::Installed => self.installed_page(ui),
                    Page::Settings => self.settings_page(ui),
                }
            });
        self.toast(ctx);
        self.confirmation(ctx);
        if let Some(programs) = super::confirmations::removal(ctx, &mut self.confirm_removal) {
            self.start_removal(programs);
        }
        self.import_dialog(ctx);
        self.handle_close(ctx);
        self.sounds
            .update(ctx, self.settings.sound, self.error.as_deref());
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }
}
