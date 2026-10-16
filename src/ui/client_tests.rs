use std::collections::BTreeSet;

use super::{
    app::{Page, QueueItem},
    installation_tests::{click, fixture, frame},
    queue_summary::QueueSummary,
    sounds::Cue,
};
use crate::{engine::JobStatus, installation::InstallMode};

fn item(status: JobStatus) -> QueueItem {
    QueueItem {
        id: "ui-fixture".into(),
        name: "UI fixture".into(),
        version: "1.0".into(),
        size: 0,
        removal: false,
        status,
        downloaded: 0,
        bytes_per_second: 0.0,
        mode_label: "Тихая установка".into(),
    }
}

#[test]
fn total_inventory_failure_is_not_mistaken_for_a_successful_empty_scan() {
    let (root, ctx, mut app) = fixture(false);
    assert!(app.library.is_empty());
    app.receive_programs(Err("Inventory test failure".into()));
    assert!(app.inventory_needs_review());
    click(&mut app, &ctx, "Установить · 1");
    assert!(app.confirm_plan.is_some() && !app.queue_active);
    assert!(!root.path().join("installed").exists());
    click(&mut app, &ctx, "Вернуться");
    app.receive_programs(Ok(crate::inventory::Inventory {
        programs: vec![],
        warnings: vec![],
        unverified: BTreeSet::new(),
    }));
    assert!(!app.inventory_needs_review() && app.error.is_none());
}

#[test]
fn queue_retry_accepts_an_early_click_and_preserves_requested_mode() {
    for (label, mode) in [
        ("Повторить незавершённые", InstallMode::Silent),
        ("Повторить с мастером", InstallMode::Interactive),
    ] {
        let (root, ctx, mut app) = fixture(true);
        app.document.catalog.packages[0].install = Some(crate::catalog::InstallSpec::Exe {
            silent_args: vec!["/quiet".into()],
            requires_admin: false,
        });
        app.page = Page::Queue;
        app.selected.clear();
        app.queue = vec![item(JobStatus::Failed("Fixture download failed".into()))];
        app.programs_loading = true;
        click(&mut app, &ctx, label);
        assert!(app.pending_install.is_some());
        app.programs_loading = false;
        frame(&mut app, &ctx, vec![]);
        let plan = app.confirm_plan.as_ref().unwrap();
        assert_eq!(plan.requested, BTreeSet::from(["ui-fixture".into()]));
        assert_eq!(plan.installation.mode_for("ui-fixture"), mode);
        assert!(!app.queue_active && !root.path().join("installed").exists());
        click(&mut app, &ctx, "Вернуться");
    }
}

#[test]
fn mixed_or_cancelled_queues_do_not_report_full_success() {
    let summary = QueueSummary::from_items(&[
        item(JobStatus::Done {
            reboot_required: true,
        }),
        item(JobStatus::Failed("Failure".into())),
        item(JobStatus::Skipped("Dependency".into())),
        item(JobStatus::Cancelled),
    ]);
    assert_eq!(
        (
            summary.done,
            summary.failed,
            summary.skipped,
            summary.cancelled
        ),
        (1, 1, 1, 1)
    );
    assert_eq!(summary.completed(), 4);
    assert!(summary.reboot);
    assert_eq!(summary.title(), "Нужны действия");
    assert!(summary.cue() == Some(Cue::Error));
    let cancelled = QueueSummary::from_items(&[item(JobStatus::Cancelled)]);
    assert_eq!(cancelled.title(), "Очередь остановлена");
    assert!(cancelled.cue().is_none());
    assert!(QueueSummary::from_items(&[]).cue().is_none());
    let success = QueueSummary::from_items(&[item(JobStatus::Done {
        reboot_required: false,
    })]);
    assert_eq!(success.title(), "Все задачи выполнены");
    assert!(success.cue() == Some(Cue::Success));
}

#[test]
fn installed_search_matches_names_publishers_and_catalog_metadata_after_refresh() {
    use crate::{
        catalog::CatalogDocument,
        uninstall::{InstalledProgram, UninstallTarget},
    };
    let catalog = CatalogDocument::builtin().unwrap().catalog;
    let package = &catalog.packages[0];
    let mut programs = vec![InstalledProgram {
        id: "fixture".into(),
        name: "Редактор Example".into(),
        version: "1".into(),
        publisher: "Vendor".into(),
        quiet: false,
        target: UninstallTarget::Detected {
            path: "fixture.exe".into(),
        },
        managed_ids: vec![],
        package_ids: vec![package.id.clone()],
        icon_path: None,
    }];
    let mut search = super::installed_search::InstalledSearch::default();
    for query in [
        "",
        "   ",
        "РЕДАКТОР",
        "Example Vendor",
        package.name.as_str(),
        "does not exist",
    ] {
        search.prepare(&programs, &catalog, query);
        let expected = format!("{} {}", programs[0].name, programs[0].publisher)
            .to_lowercase()
            .contains(&query.trim().to_lowercase())
            || package.matches_search(query);
        assert_eq!(search.matches(0), expected, "{query}");
    }
    programs[0].name = "Replacement".into();
    programs[0].package_ids.clear();
    search.invalidate();
    search.prepare(&programs, &catalog, "Replacement");
    assert!(search.matches(0));
    search.prepare(&programs, &catalog, "Редактор");
    assert!(!search.matches(0));
}

#[test]
fn static_client_stops_continuous_repaints_after_controls_settle() {
    let (_root, ctx, mut app) = fixture(false);
    app.settings.appearance.season = crate::preferences::SeasonMode::Off;
    for _ in 0..120 {
        frame(&mut app, &ctx, vec![]);
    }
    let output = frame(&mut app, &ctx, vec![]);
    assert!(
        output.viewport_output[&eframe::egui::ViewportId::ROOT].repaint_delay
            >= std::time::Duration::from_secs(1)
    );
}
