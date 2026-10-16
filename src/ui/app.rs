use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use anyhow::Result;
use eframe::egui::{self, Align, Layout, Margin, RichText, Stroke, Vec2};

use super::actions::Notification;
use super::icons::IconLoader;
use super::program_lists::{ImportPreview, ImportedSelection};
use super::sounds::SoundPlayer;
use super::theme::{self, Icon};
use crate::catalog::{CatalogDocument, PackageKind};
use crate::catalog_filter::CatalogFilter;
use crate::engine::{Engine, JobStatus, WorkerEvent};
use crate::installation::InstallationSettings;
use crate::planner::{InstallBatch, prepare_batch};
use crate::storage::{Library, Settings, Store};
use crate::uninstall::InstalledProgram;

#[derive(Clone, Copy, PartialEq, Hash)]
pub(super) enum Page {
    Catalog,
    Installed,
    Queue,
    Sets,
    Weather,
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
    pub mode_label: String,
}

#[derive(Clone)]
pub(super) struct InstallConfirmation {
    pub batch: InstallBatch,
    pub requested: BTreeSet<String>,
    pub installation: InstallationSettings,
}

pub(super) struct PendingInstall {
    pub selected: BTreeSet<String>,
    review: bool,
    installation: InstallationSettings,
    source: String,
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
    pub(super) installed_search: super::installed_search::InstalledSearch,
    pub(super) winget_status: Option<(std::time::Instant, bool)>,
    pub(super) selected_removals: BTreeSet<String>,
    pub(super) settings: Settings,
    pub(super) updater: super::update_view::UpdateUi,
    pub(super) weather: super::weather_view::WeatherUi,
    pub(super) experience: super::experience::ExperienceUi,
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
    pub(super) catalog_search: super::catalog_search::CatalogSearch,
    pub(super) focus_search: bool,
    pub(super) selected: BTreeSet<String>,
    pub(super) details: Option<String>,
    pub(super) queue: Vec<QueueItem>,
    pub(super) queue_active: bool,
    pub(super) queue_already_installed: usize,
    pub(super) sets_ui: super::sets_view::SetsUi,
    pub(super) confirm_plan: Option<InstallConfirmation>,
    pub(super) pending_install: Option<PendingInstall>,
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
        let mut app = Self::from_context(&cc.egui_ctx, store, catalog_override)?;
        app.start_catalog_load(app.source_input.clone(), false);
        Ok(app)
    }

    pub(super) fn from_context(
        ctx: &egui::Context,
        store: Store,
        catalog_override: Option<String>,
    ) -> Result<Self> {
        theme::initialize(ctx);
        let settings = store.load_settings()?;
        let experience = super::experience::ExperienceUi::new(
            &settings.experience,
            store.root.join("settings.json").is_file(),
        );
        let selected = settings.restore_selection.clone();
        let updater = super::update_view::UpdateUi::new(&store);
        let weather = super::weather_view::WeatherUi::new(&store);
        let appearance = theme::ThemeTransition::new(
            ctx,
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
        let app = Self {
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
            icons: IconLoader::new(ctx.clone())?,
            package_icons: BTreeMap::new(),
            programs_loading: false,
            installed_query: String::new(),
            installed_search: Default::default(),
            winget_status: None,
            selected_removals: BTreeSet::new(),
            settings,
            updater,
            weather,
            experience,
            appearance,
            sounds: SoundPlayer::new(ctx.clone())?,
            settings_section: super::settings_view::SettingsSection::Appearance,
            background: super::background::Background::new(ctx)?,
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
            source_input: source,
            active_source: String::new(),
            pending_source: None,
            pending_setting: None,
            load_generation: 0,
            loading: false,
            page: Page::Catalog,
            filter: CatalogFilter::default(),
            catalog_search: Default::default(),
            focus_search: false,
            selected,
            details: None,
            queue: Vec::new(),
            queue_active: false,
            queue_already_installed: 0,
            sets_ui: Default::default(),
            confirm_plan: None,
            pending_install: None,
            confirm_removal: None,
            error: None,
            close_dialog: false,
            close_when_done: false,
        };
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
                    self.receive_programs(result);
                }
                WorkerEvent::Programs { .. } => {}
                WorkerEvent::Removed { id, managed_ids } => {
                    self.programs.retain(|program| program.id != id);
                    self.installed_search.invalidate();
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
                WorkerEvent::AppUpdate(result) => self.receive_app_update(result),
                WorkerEvent::Weather {
                    slot,
                    ticket,
                    result,
                } => self.receive_weather(slot, ticket, result),
                WorkerEvent::AppUpdateProgress(progress) => self.updater.progress = Some(progress),
                WorkerEvent::QueueFinished => {
                    self.queue_active = false;
                    if let Some(cue) =
                        super::queue_summary::QueueSummary::from_items(&self.queue).cue()
                    {
                        self.sounds.play(self.settings.sound, cue);
                    }
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
                self.catalog_search.invalidate();
                self.installed_search.invalidate();
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
                if self.settings.restore_selection != self.selected {
                    self.save_preferences();
                }
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

    pub(super) fn receive_programs(&mut self, result: Result<crate::inventory::Inventory, String>) {
        self.programs_loading = false;
        self.installed_search.invalidate();
        match result {
            Ok(inventory) => {
                if self
                    .error
                    .as_ref()
                    .is_some_and(|error| self.inventory_warnings.contains(error))
                {
                    self.error = None;
                }
                self.programs = inventory.programs;
                self.package_icons.clear();
                for program in &self.programs {
                    if let Some(path) = &program.icon_path {
                        for id in program.package_ids.iter().chain(&program.managed_ids) {
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
                self.inventory_warnings = vec![error.clone()];
                self.error = Some(error);
            }
        }
    }

    pub(super) fn prepare_install(&mut self, selected: BTreeSet<String>) {
        self.request_install(selected, false);
    }

    pub(super) fn request_install(&mut self, selected: BTreeSet<String>, review: bool) {
        self.request_install_with_options(selected, review, self.settings.installation.clone());
    }

    pub(super) fn request_install_with_options(
        &mut self,
        selected: BTreeSet<String>,
        review: bool,
        installation: InstallationSettings,
    ) {
        if !self.install_actions_enabled() || self.has_modal() {
            return;
        }
        if self.loading || self.programs_loading {
            self.pending_install = Some(PendingInstall {
                selected,
                review,
                installation,
                source: self
                    .pending_source
                    .as_ref()
                    .unwrap_or(&self.active_source)
                    .clone(),
            });
            return;
        }
        let batch = prepare_batch(&self.document.catalog, &selected, self.installation_state());
        if batch.packages.is_empty() && batch.blocked.is_empty() {
            self.notify("Всё из этого списка уже установлено".into());
            return;
        }
        let plan = InstallConfirmation {
            batch,
            requested: selected,
            installation,
        };
        if review || plan.installation.confirm_before_install || self.inventory_needs_review() {
            self.confirm_plan = Some(plan);
        } else {
            self.start_plan(plan);
        }
    }

    pub(super) fn start_plan(&mut self, plan: InstallConfirmation) {
        let packages = plan.batch.packages;
        let active = !packages.is_empty();
        let result = if active {
            self.engine.start(
                packages.clone(),
                &self.document,
                self.library.clone(),
                self.store.clone(),
                plan.installation.clone(),
            )
        } else {
            Ok(())
        };
        match result {
            Ok(()) => {
                self.queue = packages
                    .into_iter()
                    .map(|package| {
                        let mode_label = plan.installation.label_for(&package).to_owned();
                        QueueItem {
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
                            mode_label,
                        }
                    })
                    .collect();
                let blocked: BTreeSet<_> =
                    plan.batch.blocked.iter().map(|p| p.id.clone()).collect();
                self.queue
                    .extend(plan.batch.blocked.into_iter().map(|package| QueueItem {
                        id: package.id,
                        name: package.name,
                        version: String::new(),
                        size: 0,
                        removal: false,
                        status: JobStatus::Skipped(package.reason),
                        downloaded: 0,
                        bytes_per_second: 0.0,
                        mode_label: "Нужны действия".into(),
                    }));
                self.queue_already_installed = plan.batch.installed;
                self.queue_active = active;
                self.selected
                    .retain(|id| !plan.requested.contains(id) || blocked.contains(id));
                self.details = None;
                self.page = Page::Queue;
                self.error = None;
                self.save_preferences();
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    fn tick_pending_install(&mut self, ctx: &egui::Context) {
        if self.pending_install.is_none() {
            return;
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            self.pending_install = None;
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(100));
        if self.loading
            || self.programs_loading
            || self.list_busy
            || self.has_modal()
            || egui::Popup::is_any_open(ctx)
        {
            return;
        }
        let Some(pending) = self.pending_install.take() else {
            return;
        };
        if pending.source != self.active_source {
            self.notify(
                "Источник каталога изменился. Проверьте выбор и запустите установку снова.".into(),
            );
            return;
        }
        self.request_install_with_options(pending.selected, pending.review, pending.installation);
        ctx.request_repaint();
    }

    pub(super) fn installation_state(&self) -> &Library {
        &self.effective_state
    }

    pub(super) fn inventory_needs_review(&self) -> bool {
        !self.inventory_unverified.is_empty() || !self.inventory_warnings.is_empty()
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
                        mode_label: String::new(),
                    })
                    .collect();
                self.queue_active = true;
                self.queue_already_installed = 0;
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
                    .fill(colors.panel_fill())
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
                ui.add_space(if ctx.content_rect().height() < 620.0 {
                    12.0
                } else {
                    28.0
                });
                ui.label(RichText::new("Библиотека").size(12.0).color(colors.dim));
                egui::ScrollArea::vertical()
                    .id_salt("sidebar-navigation")
                    .max_height((ui.available_height() - 156.0).max(72.0))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        if ctx.content_rect().height() < 620.0 {
                            ui.spacing_mut().item_spacing.y = 6.0;
                        }
                        self.main_navigation(ui);
                    });
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
                    if theme::nav(ui, "Помощь", Icon::Layers, false, None) {
                        self.experience.guide();
                    }
                    ui.add_space(10.0);
                    let warning =
                        !self.catalog_warnings.is_empty() || !self.document.diagnostics.is_empty();
                    ui.add(
                        egui::Label::new(
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
                        .truncate(),
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
        if theme::nav(
            ui,
            "Наборы программ",
            Icon::Layers,
            self.page == Page::Sets,
            Some(self.settings.program_sets.len()),
        ) {
            self.page = Page::Sets;
            self.details = None;
        }
        if theme::nav(ui, "Погода", Icon::Layers, self.page == Page::Weather, None) {
            self.page = Page::Weather;
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
                        if ui
                            .add(theme::compact("×"))
                            .on_hover_text("Скрыть сообщение")
                            .clicked()
                        {
                            self.error = None;
                        }
                    });
                });
            ui.add_space(12.0);
        }
    }

    pub(super) fn has_modal(&self) -> bool {
        self.confirm_plan.is_some()
            || self.experience.open()
            || self.confirm_removal.is_some()
            || self.import_preview.is_some()
            || self.close_dialog
            || self.background_picking
            || self.updater.applying()
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

impl SoftDownloaderApp {
    pub(super) fn render(&mut self, ctx: &egui::Context) {
        self.poll_events();
        if self.updater.applying() {
            self.tick_app_update(ctx);
            return;
        }
        self.tick_weather(ctx);
        let (background_settings, weather_scene) = self.weather_background();
        self.background
            .set_busy(self.queue_active || self.loading || self.programs_loading || self.list_busy);
        self.background.update(
            ctx,
            &background_settings,
            self.settings.reduced_motion,
            weather_scene,
        );
        self.appearance.update(
            ctx,
            self.settings.appearance,
            self.settings.appearance.current_season(),
            self.settings.reduced_motion,
            (self.background.video.texture.is_some()
                || weather_scene.is_some()
                || (self.settings.background.effects
                    && self.settings.appearance.current_season().is_some()))
            .then_some(background_settings.dimming),
        );
        ctx.request_repaint_after(Duration::from_secs(60));
        self.icons.collect(ctx);
        if self.loading
            || self.queue_active
            || self.programs_loading
            || self.list_busy
            || self.background_busy
            || self.updater.busy
        {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        self.handle_shortcuts(ctx);
        self.background.paint(
            ctx,
            &background_settings,
            self.settings.appearance.current_season(),
            weather_scene,
        );
        self.sidebar(ctx);
        self.update_banner(ctx);
        self.selection_bar(ctx);
        if self.page == Page::Catalog {
            self.details_panel(ctx);
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::TRANSPARENT)
                    .inner_margin(Margin::symmetric(28, 26)),
            )
            .show(ctx, |ui| {
                self.notice(ui);
                match self.page {
                    Page::Catalog => self.catalog_page(ui),
                    Page::Queue => self.queue_page(ui),
                    Page::Installed => self.installed_page(ui),
                    Page::Settings => self.settings_page(ui),
                    Page::Sets => self.sets_page(ui),
                    Page::Weather => self.weather_page(ui),
                }
            });
        self.toast(ctx);
        self.confirmation(ctx);
        if let Some(programs) = super::confirmations::removal(ctx, &mut self.confirm_removal) {
            self.start_removal(programs);
        }
        self.import_dialog(ctx);
        self.handle_close(ctx);
        self.tick_pending_install(ctx);
        self.tick_app_update(ctx);
        self.experience_dialogs(ctx);
        self.sounds
            .update(ctx, self.settings.sound, self.error.as_deref());
    }
}

impl eframe::App for SoftDownloaderApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.background.observe_cpu(frame.info().cpu_usage);
        self.render(ctx);
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if !self.save_preferences() {
            eprintln!(
                "Не удалось сохранить выбор перед закрытием: {:?}",
                self.error
            );
        }
    }
}
