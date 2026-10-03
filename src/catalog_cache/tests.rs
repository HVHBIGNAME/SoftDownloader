use super::*;
use std::cell::Cell;

#[tokio::test]
async fn warm_cache_skips_network_but_refresh_and_other_profiles_do_not() {
    let folder = tempfile::tempdir().unwrap();
    let store = Store::at(folder.path().join("one")).unwrap();
    let other = Store::at(folder.path().join("two")).unwrap();
    let calls = Cell::new(0);
    let fetch = || {
        calls.set(calls.get() + 1);
        async { CatalogDocument::builtin() }
    };
    assert!(!load_with(&store, "", false, fetch).await.unwrap().cached);
    assert!(load_with(&store, "", false, fetch).await.unwrap().cached);
    assert_eq!(calls.get(), 1);
    assert!(!load_with(&store, "", true, fetch).await.unwrap().cached);
    assert!(!load_with(&other, "", false, fetch).await.unwrap().cached);
    assert_eq!(calls.get(), 3);
}

#[tokio::test]
async fn stale_fallback_reports_failure_and_does_not_reset_age() {
    let folder = tempfile::tempdir().unwrap();
    let store = Store::at(folder.path().to_owned()).unwrap();
    load_with(&store, "", false, || async { CatalogDocument::builtin() })
        .await
        .unwrap();
    let cache = Cache::new(&store, "").unwrap();
    let mut entry = cache.read().unwrap().unwrap();
    entry.stored_at = now() - MAX_AGE.as_secs() - 1;
    crate::storage::write_json(&cache.path, &entry).unwrap();
    let result = load_with(&store, "", false, || async {
        anyhow::bail!("offline fixture")
    })
    .await
    .unwrap();
    assert!(result.cached);
    assert!(result.age_seconds > MAX_AGE.as_secs());
    assert!(result.warnings[0].contains("offline fixture"));
    assert_eq!(cache.read().unwrap().unwrap().stored_at, entry.stored_at);
}

#[tokio::test]
async fn corrupt_cache_is_rejected_and_local_catalog_changes_invalidate_it() {
    let folder = tempfile::tempdir().unwrap();
    let store = Store::at(folder.path().join("profile")).unwrap();
    let source = folder.path().join("catalog.json");
    std::fs::write(&source, b"old").unwrap();
    let source = source.to_str().unwrap();
    let fetch = || async { CatalogDocument::builtin() };
    load_with(&store, source, false, fetch).await.unwrap();
    let cache = Cache::new(&store, source).unwrap();
    std::fs::write(&cache.path, b"broken").unwrap();
    let result = load_with(&store, source, false, fetch).await.unwrap();
    assert!(!result.cached);
    assert!(!result.warnings.is_empty());
    assert!(
        load_with(&store, source, false, fetch)
            .await
            .unwrap()
            .cached
    );
    std::fs::write(source, b"changed-size").unwrap();
    assert!(
        !load_with(&store, source, false, fetch)
            .await
            .unwrap()
            .cached
    );
}

#[test]
fn failed_sources_keep_previous_artifacts_only_for_the_same_source() {
    let mut previous = CatalogDocument::builtin().unwrap();
    let artifact = crate::catalog::Artifact {
        file_name: "app.exe".into(),
        size: 1,
        sha256: "a".repeat(64),
        local_path: Some("app.exe".into()),
        drive_file_id: None,
        url: None,
    };
    previous
        .catalog
        .packages
        .iter_mut()
        .find(|p| p.id == "flclash")
        .unwrap()
        .artifact = Some(artifact);
    let mut current = CatalogDocument::builtin().unwrap();
    current
        .diagnostics
        .insert("flclash".into(), "offline".into());
    reuse_successful_sources(&mut current, &previous);
    assert!(
        current
            .catalog
            .package("flclash")
            .unwrap()
            .artifact
            .is_some()
    );
    let package = current
        .catalog
        .packages
        .iter_mut()
        .find(|p| p.id == "flclash")
        .unwrap();
    package.artifact = None;
    package.source = None;
    reuse_successful_sources(&mut current, &previous);
    assert!(
        current
            .catalog
            .package("flclash")
            .unwrap()
            .artifact
            .is_none()
    );
}
