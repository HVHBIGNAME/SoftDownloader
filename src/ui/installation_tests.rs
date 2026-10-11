use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use eframe::egui::{self, Event, Pos2, Rect, Vec2};
use sha2::{Digest, Sha256};

use super::app::SoftDownloaderApp;
use crate::catalog::{Catalog, CatalogDocument};
use crate::storage::{Settings, Store};

fn fixture(review: bool) -> (tempfile::TempDir, egui::Context, SoftDownloaderApp) {
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

fn frame(app: &mut SoftDownloaderApp, ctx: &egui::Context, events: Vec<Event>) -> egui::FullOutput {
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

fn click(app: &mut SoftDownloaderApp, ctx: &egui::Context, label: &str) {
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
