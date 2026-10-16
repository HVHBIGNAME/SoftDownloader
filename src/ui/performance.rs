use std::time::Instant;

use eframe::egui::{self, Pos2, Rect, Vec2};

use super::app::{Page, SoftDownloaderApp};
use crate::{
    catalog::CatalogDocument,
    preferences::SeasonMode,
    storage::{Settings, Store},
    weather::{Conditions, WeatherKind},
};

#[test]
#[ignore = "Manual release-mode CPU render benchmark; no network or installers"]
fn render_budget() {
    for (name, count, weather, page, query) in [
        ("catalog-idle", 266, false, Page::Catalog, ""),
        ("catalog-storm", 266, true, Page::Catalog, ""),
        ("weather-storm", 266, true, Page::Weather, ""),
        ("large-catalog-storm", 4000, true, Page::Catalog, ""),
        ("installed-idle", 4000, false, Page::Installed, ""),
        (
            "installed-search",
            4000,
            false,
            Page::Installed,
            "benchmark",
        ),
        ("installation-settings", 266, false, Page::Settings, ""),
    ] {
        let root = tempfile::tempdir().unwrap();
        let store = Store::at(root.path().to_owned()).unwrap();
        let mut settings = Settings::default();
        settings.updates.enabled = false;
        settings.sound.enabled = false;
        settings.appearance.season = SeasonMode::Off;
        settings.weather.cinematic = false;
        settings.experience.guide_seen = true;
        settings.experience.last_seen_version = env!("CARGO_PKG_VERSION").into();
        store.save_settings(&settings).unwrap();
        let ctx = egui::Context::default();
        let start = Instant::now();
        let mut app = SoftDownloaderApp::from_context(&ctx, store, None).unwrap();
        let construction = start.elapsed();
        app.page = page;
        app.settings_section = super::settings_view::SettingsSection::Installation;
        app.installed_query = query.into();
        app.weather.preview = weather.then(|| Conditions::demo(WeatherKind::Storm));
        let original = CatalogDocument::builtin().unwrap().catalog.packages;
        app.document.catalog.packages = (0..count)
            .map(|i| {
                let mut package = original[i % original.len()].clone();
                if i >= original.len() {
                    package.id = format!("bench-{i}");
                }
                package
            })
            .collect();
        if page == Page::Installed {
            app.programs = (0..count)
                .map(|i| crate::uninstall::InstalledProgram {
                    id: format!("program-{i}"),
                    name: format!("Benchmark program {i}"),
                    version: "1.0".into(),
                    publisher: "Example".into(),
                    quiet: false,
                    target: crate::uninstall::UninstallTarget::Detected {
                        path: "fixture.exe".into(),
                    },
                    package_ids: vec![app.document.catalog.packages[i].id.clone()],
                    managed_ids: vec![],
                    icon_path: None,
                })
                .collect();
        }
        let mut samples = Vec::new();
        let mut max_vertices = 0;
        let mut first_frame_ms = 0.0;
        for i in 0..220 {
            let start = Instant::now();
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 840.0))),
                    focused: true,
                    time: Some(i as f64 / 60.0),
                    ..Default::default()
                },
                |ctx| app.render(ctx),
            );
            let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);
            let vertices: usize = primitives
                .iter()
                .map(|p| match &p.primitive {
                    egui::epaint::Primitive::Mesh(mesh) => mesh.vertices.len(),
                    _ => 0,
                })
                .sum();
            max_vertices = max_vertices.max(vertices);
            if i == 0 {
                first_frame_ms = start.elapsed().as_secs_f64() * 1000.0;
            }
            if i >= 20 {
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "{name}: construction={:.2}ms first-frame={first_frame_ms:.3}ms cpu-p50={:.3}ms cpu-p95={:.3}ms vertices={max_vertices}",
            construction.as_secs_f64() * 1000.0,
            samples[100],
            samples[190]
        );
    }
}
