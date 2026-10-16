use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use eframe::egui::{self, Event, Pos2, Rect, Vec2};
use sha2::{Digest, Sha256};

use super::app::SoftDownloaderApp;
use crate::catalog::{Catalog, CatalogDocument};
use crate::storage::{Settings, Store};

pub(super) fn fixture(review: bool) -> (tempfile::TempDir, egui::Context, SoftDownloaderApp) {
    let appdata = directories::BaseDirs::new()
        .unwrap()
        .data_local_dir()
        .to_owned();
    let root = tempfile::Builder::new()
        .prefix("sd-ui-fixture-")
        .tempdir_in(&appdata)
        .unwrap();
    let payload = b"Portable test fixture: these bytes are copied, never executed";
    std::fs::write(root.path().join("fixture.exe"), payload).unwrap();
    let relative = root
        .path()
        .strip_prefix(&appdata)
        .unwrap()
        .to_string_lossy();
    let catalog = serde_json::json!({"schema_version":1,"title":"Test","categories":[{"id":"test","name":"Test"}],"packages":[{
        "id":"ui-fixture","name":"UI fixture","version":"1.0","publisher":"Test","description":"Test","category":"test",
        "artifact":{"file_name":"fixture.exe","size":payload.len(),"sha256":hex::encode(Sha256::digest(payload)),"local_path":"fixture.exe"},
        "install":{"type":"portable","destination":{"root":"local_app_data","path":format!("{relative}/installed")}}
    }]});
    let store = Store::at(root.path().join("profile")).unwrap();
    let mut settings = Settings {
        reduced_motion: true,
        ..Default::default()
    };
    settings.updates.enabled = false;
    settings.experience.guide_seen = true;
    settings.experience.last_seen_version = env!("CARGO_PKG_VERSION").into();
    settings.sound.volume = 0;
    settings.installation.confirm_before_install = review;
    store.save_settings(&settings).unwrap();
    let ctx = egui::Context::default();
    let mut app = SoftDownloaderApp::from_context(&ctx, store, None).unwrap();
    app.document = CatalogDocument {
        catalog: Catalog::parse(&serde_json::to_vec(&catalog).unwrap()).unwrap(),
        local_root: Some(root.path().into()),
        is_demo: false,
        diagnostics: Default::default(),
    };
    app.selected = BTreeSet::from(["ui-fixture".into()]);
    (root, ctx, app)
}

pub(super) fn frame(
    app: &mut SoftDownloaderApp,
    ctx: &egui::Context,
    events: Vec<Event>,
) -> egui::FullOutput {
    ctx.run(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1024.0, 700.0))),
            focused: true,
            events,
            ..Default::default()
        },
        |ctx| app.render(ctx),
    )
}

fn text_position(output: &egui::FullOutput, label: &str) -> Pos2 {
    fn positions(shape: &egui::Shape, clip: Rect, label: &str, result: &mut Vec<Pos2>) {
        match shape {
            egui::Shape::Text(text) if text.galley.text() == label => {
                let center = text.pos + text.galley.size() * 0.5;
                if clip.contains(center) {
                    result.push(center);
                }
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    positions(shape, clip, label, result);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for shape in &output.shapes {
        positions(&shape.shape, shape.clip_rect, label, &mut found);
    }
    found
        .into_iter()
        .min_by(|left, right| left.y.total_cmp(&right.y))
        .unwrap_or_else(|| panic!("Visible control not found: {label}"))
}

pub(super) fn click(app: &mut SoftDownloaderApp, ctx: &egui::Context, label: &str) {
    frame(app, ctx, vec![]);
    let output = frame(app, ctx, vec![]);
    let position = text_position(&output, label);
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

#[test]
fn one_click_runs_only_the_selected_fixture_and_review_setting_prevents_execution() {
    let (root, ctx, mut app) = fixture(false);
    click(&mut app, &ctx, "Установить · 1");
    assert!(app.confirm_plan.is_none());
    assert!(app.queue_active);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.queue_active && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        frame(&mut app, &ctx, vec![]);
    }
    assert!(!app.queue_active);
    assert!(app.library.contains_key("ui-fixture"), "{:?}", app.error);
    assert_eq!(
        std::fs::read(root.path().join("installed/fixture.exe")).unwrap(),
        std::fs::read(root.path().join("fixture.exe")).unwrap()
    );
    drop(app);
    let (root, ctx, mut app) = fixture(true);
    click(&mut app, &ctx, "Установить · 1");
    assert!(app.confirm_plan.is_some());
    assert!(!root.path().join("installed").exists());
    click(&mut app, &ctx, "Вернуться");
    assert!(app.confirm_plan.is_none() && !app.queue_active);
}

#[test]
fn settings_and_named_sets_are_operable_and_persist_at_minimum_window_size() {
    let (_root, ctx, mut app) = fixture(false);
    click(&mut app, &ctx, "Настройки");
    click(&mut app, &ctx, "Установка");
    click(&mut app, &ctx, "Тихая установка");
    click(&mut app, &ctx, "С мастером");
    assert_eq!(
        app.store.load_settings().unwrap().installation.mode,
        crate::installation::InstallMode::Interactive
    );
    click(&mut app, &ctx, "Всегда показывать план перед установкой");
    assert!(
        app.store
            .load_settings()
            .unwrap()
            .installation
            .confirm_before_install
    );
    click(&mut app, &ctx, "Наборы программ");
    click(&mut app, &ctx, "Сохранить выбор · 1");
    assert_eq!(app.store.load_settings().unwrap().program_sets.len(), 1);
    click(&mut app, &ctx, "···");
    click(&mut app, &ctx, "Удалить набор");
    assert!(app.store.load_settings().unwrap().program_sets.is_empty());
    click(&mut app, &ctx, "Вернуть");
    let saved = app.store.load_settings().unwrap();
    assert_eq!(
        saved.program_sets[0].ids,
        BTreeSet::from(["ui-fixture".into()])
    );
    assert_eq!(saved.restore_selection, app.selected);
}

#[test]
fn early_install_click_waits_for_detection_then_continues_or_can_be_cancelled() {
    let (root, ctx, mut app) = fixture(false);
    app.programs_loading = true;
    click(&mut app, &ctx, "Установить · 1");
    assert!(app.pending_install.is_some() && !app.queue_active);
    assert!(!root.path().join("installed").exists());
    click(&mut app, &ctx, "Отменить подготовку");
    app.programs_loading = false;
    frame(&mut app, &ctx, vec![]);
    assert!(app.pending_install.is_none() && !app.queue_active);
    assert!(!root.path().join("installed").exists());
    app.programs_loading = true;
    click(&mut app, &ctx, "Установить · 1");
    app.programs_loading = false;
    frame(&mut app, &ctx, vec![]);
    assert!(app.pending_install.is_none() && app.queue_active);
    let deadline = Instant::now() + Duration::from_secs(10);
    while app.queue_active && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
        frame(&mut app, &ctx, vec![]);
    }
    assert!(app.library.contains_key("ui-fixture"), "{:?}", app.error);
    assert_eq!(
        std::fs::read(root.path().join("installed/fixture.exe")).unwrap(),
        std::fs::read(root.path().join("fixture.exe")).unwrap()
    );
}

#[test]
fn deferred_install_preserves_review_choice_and_does_not_cross_catalog_sources() {
    let (root, ctx, mut app) = fixture(true);
    app.loading = true;
    click(&mut app, &ctx, "Установить · 1");
    app.settings.installation.confirm_before_install = false;
    app.loading = false;
    frame(&mut app, &ctx, vec![]);
    assert!(app.confirm_plan.is_some() && !app.queue_active);
    assert!(!root.path().join("installed").exists());
    click(&mut app, &ctx, "Вернуться");
    app.loading = true;
    click(&mut app, &ctx, "Установить · 1");
    app.active_source = "changed-source".into();
    app.loading = false;
    frame(&mut app, &ctx, vec![]);
    assert!(app.pending_install.is_none() && !app.queue_active);
    assert!(!root.path().join("installed").exists());
}

#[test]
fn closing_window_during_preparation_never_starts_an_installer() {
    let (root, ctx, mut app) = fixture(false);
    app.programs_loading = true;
    click(&mut app, &ctx, "Установить · 1");
    app.programs_loading = false;
    let mut input = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1024.0, 700.0))),
        ..Default::default()
    };
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let _ = ctx.run(input, |ctx| app.render(ctx));
    assert!(app.pending_install.is_none() && !app.queue_active);
    assert!(!root.path().join("installed").exists());
}

#[test]
fn first_run_guide_and_release_table_are_acknowledged_once_and_help_reopens() {
    let (_root, ctx, mut app) = fixture(false);
    app.settings.experience = Default::default();
    app.experience = super::experience::ExperienceUi::new(&app.settings.experience, false);
    for _ in 0..4 {
        click(&mut app, &ctx, "Далее");
    }
    click(&mut app, &ctx, "Начать работу");
    let saved = app.store.load_settings().unwrap();
    assert!(saved.experience.guide_seen);
    assert!(!super::experience::ExperienceUi::new(&saved.experience, true).open());
    app.settings.experience.last_seen_version = "0.6.1".into();
    app.experience = super::experience::ExperienceUi::new(&app.settings.experience, true);
    click(&mut app, &ctx, "Понятно");
    assert_eq!(
        app.store
            .load_settings()
            .unwrap()
            .experience
            .last_seen_version,
        env!("CARGO_PKG_VERSION")
    );
    click(&mut app, &ctx, "Помощь");
    assert!(app.experience.open());
    click(&mut app, &ctx, "Пропустить");
    assert!(!app.experience.open());
}

#[test]
fn incomplete_detection_requires_review_and_deferred_request_keeps_its_selection_and_mode() {
    use crate::installation::InstallMode;

    for deferred in [false, true] {
        let (root, ctx, mut app) = fixture(false);
        app.inventory_warnings
            .push("Test: detection was incomplete".into());
        app.programs_loading = deferred;
        app.settings.installation.mode = InstallMode::Interactive;
        click(&mut app, &ctx, "Установить · 1");
        if deferred {
            let original = app.pending_install.as_ref().unwrap().selected.clone();
            app.prepare_install(BTreeSet::from(["another-request".into()]));
            assert_eq!(app.pending_install.as_ref().unwrap().selected, original);
            app.selected.clear();
            app.settings.installation.mode = InstallMode::Silent;
            app.programs_loading = false;
            frame(&mut app, &ctx, vec![]);
        }
        let plan = app
            .confirm_plan
            .as_ref()
            .expect("Incomplete inventory must be reviewed");
        assert_eq!(plan.requested, BTreeSet::from(["ui-fixture".into()]));
        assert_eq!(plan.installation.mode, InstallMode::Interactive);
        assert!(!app.queue_active);
        assert!(!root.path().join("installed").exists());
        click(&mut app, &ctx, "Вернуться");
        assert!(app.confirm_plan.is_none() && app.pending_install.is_none());
    }
}

#[test]
fn weather_demo_and_city_dropdown_work_without_starting_installers() {
    let (_root, ctx, mut app) = fixture(false);
    app.settings.weather.cinematic = false;
    click(&mut app, &ctx, "Погода");
    for _ in 0..2 {
        frame(
            &mut app,
            &ctx,
            vec![
                Event::PointerMoved(Pos2::new(850.0, 540.0)),
                Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: Vec2::new(0.0, -400.0),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    click(&mut app, &ctx, "Дождь");
    assert_eq!(
        app.weather.preview.unwrap().kind,
        crate::weather::WeatherKind::Rain
    );
    assert!(!app.settings.weather.enabled && app.settings.weather.city.is_none());
    assert!(!app.queue_active);
    drop(app);
    let (_root, ctx, mut app) = fixture(false);
    click(&mut app, &ctx, "Погода");
    let city = crate::weather::City {
        name: "Test city".into(),
        country: "Country".into(),
        admin1: "Region".into(),
        latitude: 10.0,
        longitude: 20.0,
    };
    app.receive_weather(
        0,
        0,
        Ok(crate::weather::Response::Cities(vec![city.clone()])),
    );
    click(&mut app, &ctx, &city.label());
    let saved = app.store.load_settings().unwrap();
    assert_eq!(saved.weather.city, Some(city));
    assert!(saved.weather.enabled);
    assert!(!app.queue_active);
}

#[test]
fn weather_lighting_and_motion_controls_persist_and_keep_demo_separate() {
    let (_root, ctx, mut app) = fixture(false);
    app.settings.weather.cinematic = false;
    app.weather.preview = Some(crate::weather::Conditions::demo(
        crate::weather::WeatherKind::Clear,
    ));
    click(&mut app, &ctx, "Погода");
    click(&mut app, &ctx, "Ночь");
    assert_eq!(
        app.store.load_settings().unwrap().weather.lighting,
        crate::weather::Lighting::Night
    );
    click(&mut app, &ctx, "Дождь");
    assert_eq!(
        app.weather.preview.unwrap().kind,
        crate::weather::WeatherKind::Rain
    );
    assert!(!app.settings.weather.enabled && app.settings.weather.city.is_none());
    click(&mut app, &ctx, "Включить анимацию");
    let saved = app.store.load_settings().unwrap();
    assert!(!saved.reduced_motion && !saved.background.paused);
    click(&mut app, &ctx, "Пауза");
    assert!(app.store.load_settings().unwrap().background.paused);
    click(&mut app, &ctx, "Авто");
    assert_eq!(
        app.store.load_settings().unwrap().weather.lighting,
        crate::weather::Lighting::Automatic
    );
    click(&mut app, &ctx, "Закрыть демо");
    assert!(app.weather.preview.is_none());
    assert!(!app.settings.weather.enabled);
    assert!(!app.queue_active);
}
