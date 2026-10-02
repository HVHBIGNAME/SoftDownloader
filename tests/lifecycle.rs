use std::collections::BTreeSet;
use std::io::Write;
use std::time::Duration;

use directories::BaseDirs;
use sha2::{Digest, Sha256};
use softdownloader::{
    catalog::{Catalog, CatalogDocument},
    engine::{Engine, JobStatus, WorkerEvent},
    planner::create_plan,
    storage::Store,
    uninstall,
};
use zip::write::SimpleFileOptions;

#[test]
fn queue_skips_failed_dependencies_and_installs_then_removes_an_independent_addon() {
    let fixture = tempfile::tempdir().unwrap();
    let appdata = BaseDirs::new().unwrap().data_local_dir().to_owned();
    let destination = tempfile::Builder::new()
        .prefix("softdownloader-test-")
        .tempdir_in(&appdata)
        .unwrap();
    let relative = destination.path().file_name().unwrap().to_string_lossy();
    let archive = fixture.path().join("addon.zip");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&archive).unwrap());
    zip.start_file("plugin.txt", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"isolated addon fixture").unwrap();
    zip.finish().unwrap();
    let payload = std::fs::read(&archive).unwrap();
    let hash = hex::encode(Sha256::digest(&payload));
    let package = |id: &str, dependencies: Vec<&str>, valid: bool| {
        serde_json::json!({
            "id":id,"name":id,"version":"1.0","publisher":"Test","description":"Fixture","category":"test","kind":"addon","depends_on":dependencies,
            "artifact":{"file_name":"addon.zip","size":payload.len(),"sha256":if valid {hash.clone()} else {"0".repeat(64)},"local_path":"addon.zip"},
            "install":{"type":"zip","destination":{"root":"local_app_data","path":format!("{relative}/{id}")}}
        })
    };
    let json = serde_json::json!({"schema_version":1,"title":"Test","categories":[{"id":"test","name":"Test"}],"packages":[
        package("host", vec![], false), package("dependent", vec!["host"], true), package("independent", vec![], true)
    ]});
    let portable = b"Portable fixture: these bytes must be copied, never executed";
    std::fs::write(fixture.path().join("portable.exe"), portable).unwrap();
    let mut json = json;
    json["packages"].as_array_mut().unwrap().push(serde_json::json!({
        "id":"portable", "name":"Portable fixture", "version":"1.0", "publisher":"Test", "description":"Fixture", "category":"test",
        "artifact":{"file_name":"portable.exe","size":portable.len(),"sha256":hex::encode(Sha256::digest(portable)),"local_path":"portable.exe"},
        "install":{"type":"portable","destination":{"root":"local_app_data","path":format!("{relative}/portable")}}
    }));
    let catalog = Catalog::parse(&serde_json::to_vec(&json).unwrap()).unwrap();
    let store = Store::at(fixture.path().join("state")).unwrap();
    let library = store.load_library().unwrap();
    let plan = create_plan(
        &catalog,
        &BTreeSet::from(["dependent".into(), "independent".into(), "portable".into()]),
        &library,
    )
    .unwrap();
    let document = CatalogDocument {
        catalog,
        local_root: Some(fixture.path().to_owned()),
        is_demo: false,
        diagnostics: Default::default(),
    };
    let (mut engine, events) = Engine::new().unwrap();
    engine
        .start(plan, &document, library, store.clone())
        .unwrap();
    let mut statuses = std::collections::HashMap::new();
    loop {
        match events.recv_timeout(Duration::from_secs(30)).unwrap() {
            WorkerEvent::Status { id, status } => {
                statuses.insert(id, status);
            }
            WorkerEvent::QueueFinished => break,
            _ => {}
        }
    }
    assert!(matches!(statuses["host"], JobStatus::Failed(_)));
    assert!(matches!(statuses["dependent"], JobStatus::Skipped(_)));
    assert!(matches!(statuses["independent"], JobStatus::Done { .. }));
    assert!(matches!(statuses["portable"], JobStatus::Done { .. }));
    assert_eq!(
        std::fs::read(destination.path().join("portable/portable.exe")).unwrap(),
        portable
    );
    assert!(!destination.path().join("dependent").exists());
    assert_eq!(
        std::fs::read(destination.path().join("independent/plugin.txt")).unwrap(),
        b"isolated addon fixture"
    );
    let library = store.load_library().unwrap();
    assert_eq!(library.len(), 2);
    let ids: BTreeSet<_> = library
        .values()
        .map(|entry| entry.uninstall.as_ref().unwrap().id())
        .collect();
    let programs = uninstall::scan(&library)
        .unwrap()
        .into_iter()
        .filter(|p| ids.contains(&p.id))
        .collect();
    std::thread::sleep(Duration::from_millis(20));
    engine
        .remove(programs, library, document.catalog.clone(), store.clone())
        .unwrap();
    loop {
        match events.recv_timeout(Duration::from_secs(30)).unwrap() {
            WorkerEvent::Status {
                status: JobStatus::Failed(error),
                ..
            } => panic!("{error}"),
            WorkerEvent::QueueFinished => break,
            _ => {}
        }
    }
    assert!(!destination.path().join("independent").exists());
    assert!(!destination.path().join("portable").exists());
    assert!(store.load_library().unwrap().is_empty());
}
