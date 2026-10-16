use eframe::egui;
use serde::Deserialize;

use super::app::{Page, QueueItem, SoftDownloaderApp};
use crate::{engine::JobStatus, storage::Store};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Scenario {
    Mixed,
    Success,
    Cancelled,
    Settings,
    Catalog,
    CatalogList,
    Details,
    Selected,
    Pending,
    Plan,
    Installed,
    Removal,
    Sets,
    Weather,
    Guide,
    News,
    Appearance,
    Sound,
    Background,
    Sources,
    Updates,
    LongNames,
    Import,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PreviewInput {
    Legacy(Scenario),
    Configured {
        scenario: Scenario,
        zoom: f32,
        light: bool,
    },
}

struct PreviewApp(SoftDownloaderApp);

impl eframe::App for PreviewApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        input.events.retain(|event| {
            !matches!(
                event,
                egui::Event::PointerButton { .. }
                    | egui::Event::Key { .. }
                    | egui::Event::Text(_)
                    | egui::Event::Paste(_)
                    | egui::Event::Copy
                    | egui::Event::Cut
            )
        });
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.render(ctx);
    }
}

pub fn run_client_preview(store: Store) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 840.0])
            .with_min_inner_size([1024.0, 700.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "SoftDownloader",
        options,
        Box::new(move |cc| {
            if let Some(gl) = &cc.gl {
                use eframe::glow::HasContext;
                // eframe has made the newly created OpenGL context current here.
                println!("OpenGL renderer: {}", unsafe {
                    gl.get_parameter_string(eframe::glow::RENDERER)
                });
            }
            let input: PreviewInput =
                serde_json::from_slice(&std::fs::read(store.root.join("scenario.json"))?)?;
            let (scenario, zoom, light) = match input {
                PreviewInput::Legacy(scenario) => (scenario, 1.0, false),
                PreviewInput::Configured {
                    scenario,
                    zoom,
                    light,
                } => (scenario, zoom, light),
            };
            if !(1.0..=1.5).contains(&zoom) {
                return Err("Preview zoom must be between 1 and 1.5".into());
            }
            cc.egui_ctx.set_zoom_factor(zoom);
            let mut app = SoftDownloaderApp::from_context(&cc.egui_ctx, store, None)?;
            app.settings.updates.enabled = false;
            app.settings.sound.volume = 0;
            app.settings.weather.enabled = false;
            app.settings.background.video = None;
            app.settings.appearance.season = crate::preferences::SeasonMode::Off;
            app.settings.reduced_motion = true;
            app.settings.appearance.theme = if light {
                crate::preferences::Theme::Light
            } else {
                crate::preferences::Theme::Dark
            };
            app.settings.catalog_layout =
                if matches!(scenario, Scenario::CatalogList | Scenario::LongNames) {
                    crate::storage::CatalogLayout::List
                } else {
                    crate::storage::CatalogLayout::Grid
                };
            app.selected.clear();
            app.catalog_status = "ДЕМО · проверка интерфейса".into();
            app.page = Page::Catalog;
            app.settings_section = super::settings_view::SettingsSection::Installation;
            configure(&mut app, scenario);
            Ok(Box::new(PreviewApp(app)))
        }),
    )
}

fn configure(app: &mut SoftDownloaderApp, scenario: Scenario) {
    use super::settings_view::SettingsSection;
    match scenario {
        Scenario::Mixed | Scenario::Success | Scenario::Cancelled => queue(app, scenario),
        Scenario::Settings
        | Scenario::Appearance
        | Scenario::Sound
        | Scenario::Background
        | Scenario::Sources
        | Scenario::Updates => {
            app.page = Page::Settings;
            app.settings_section = match scenario {
                Scenario::Appearance => SettingsSection::Appearance,
                Scenario::Sound => SettingsSection::Sound,
                Scenario::Background => SettingsSection::Background,
                Scenario::Sources => SettingsSection::Sources,
                Scenario::Updates => SettingsSection::Updates,
                _ => SettingsSection::Installation,
            };
        }
        Scenario::Details => app.details = Some("vscode".into()),
        Scenario::Selected | Scenario::Pending | Scenario::Plan => {
            app.selected = ["vscode", "chrome", "firefox"].map(str::to_owned).into();
            if matches!(scenario, Scenario::Pending) {
                app.programs_loading = true;
                app.request_install(app.selected.clone(), false);
            } else if matches!(scenario, Scenario::Plan) {
                app.request_install(app.selected.clone(), true);
            }
        }
        Scenario::Installed | Scenario::Removal => {
            app.page = Page::Installed;
            app.programs = app
                .document
                .catalog
                .packages
                .iter()
                .take(12)
                .map(|p| crate::uninstall::InstalledProgram {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    version: "2026.10.15.12345 (x64)".into(),
                    publisher: p.publisher.clone(),
                    quiet: false,
                    target: crate::uninstall::UninstallTarget::Detected {
                        path: "preview-only.exe".into(),
                    },
                    package_ids: vec![p.id.clone()],
                    managed_ids: vec![],
                    icon_path: None,
                })
                .collect();
            if matches!(scenario, Scenario::Removal) {
                app.confirm_removal = Some(app.programs.clone());
            }
        }
        Scenario::Sets => app.page = Page::Sets,
        Scenario::Weather => {
            app.page = Page::Weather;
            app.weather.preview = Some(crate::weather::Conditions::demo(
                crate::weather::WeatherKind::Rain,
            ));
        }
        Scenario::Guide => app.experience.guide(),
        Scenario::News => app.experience.news(),
        Scenario::LongNames => {
            for package in app.document.catalog.packages.iter_mut().take(8) {
                package.name = format!(
                    "{} — расширенные инструменты разработки и администрирования",
                    package.name
                );
                package.version = "2026.10.15.12345 (Windows x64)".into();
            }
        }
        Scenario::Import => {
            app.receive_program_list(Ok(Some(crate::engine::ProgramListAction::Imported {
                path: "Демонстрационный список программ.json".into(),
                list: crate::program_list::ProgramList::from_packages(
                    app.document.catalog.packages.iter().take(12),
                ),
            })))
        }
        Scenario::Catalog | Scenario::CatalogList => {}
    }
}

fn queue(app: &mut SoftDownloaderApp, scenario: Scenario) {
    app.page = Page::Queue;
    let statuses = match scenario {
        Scenario::Mixed => vec![JobStatus::Done { reboot_required: true }, JobStatus::Failed("Пример ошибки: загрузка прервана. Проверьте подключение и нажмите «Повторить незавершённые».".into()), JobStatus::Skipped("Пример: необходимая зависимость не установилась.".into()), JobStatus::Cancelled],
        Scenario::Success => vec![JobStatus::Done { reboot_required: false }],
        _ => vec![JobStatus::Cancelled],
    };
    app.queue = statuses
        .into_iter()
        .enumerate()
        .map(|(i, status)| QueueItem {
            id: ["vscode", "blender", "firefox", "chrome"][i].into(),
            name: if i == 1 {
                "Пример программы с длинным названием — дополнительные инструменты".into()
            } else {
                format!("Пример программы {}", i + 1)
            },
            version: "1.0".into(),
            size: 0,
            removal: false,
            status,
            downloaded: 0,
            bytes_per_second: 0.0,
            mode_label: "Тихая установка".into(),
        })
        .collect();
}
