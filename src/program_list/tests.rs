use super::*;
use crate::catalog::CatalogDocument;
use crate::storage::InstalledPackage;
use crate::uninstall::UninstallTarget;

#[test]
fn custom_sets_roundtrip_with_ids_without_exporting_installer_recipes() {
    let catalog = CatalogDocument::builtin().unwrap().catalog;
    let app = catalog.package("vscode").unwrap();
    let manual = catalog.package("eset-premium").unwrap();
    let list = ProgramList::from_packages([app, app, manual]);
    let json = serde_json::to_vec(&list).unwrap();
    let restored = ProgramList::parse(&json).unwrap();
    assert_eq!(restored.programs.len(), 2);
    let rows = restored.match_catalog(&catalog, &Library::new());
    assert!(
        rows.iter()
            .any(|row| row.package_id.as_deref() == Some("vscode")
                && row.status == MatchStatus::Available)
    );
    assert!(
        rows.iter()
            .any(|row| row.package_id.as_deref() == Some("eset-premium")
                && row.status == MatchStatus::Unavailable)
    );
    assert!(!String::from_utf8(json).unwrap().contains("silent_args"));
}

fn list(entries: &[(&[&str], &str)]) -> ProgramList {
    let mut list = ProgramList::from_inventory(&[]);
    list.programs = entries
        .iter()
        .map(|(ids, name)| ProgramEntry {
            package_ids: ids.iter().map(|id| (*id).into()).collect(),
            name: (*name).into(),
            version: "old-version".into(),
            publisher: "".into(),
        })
        .collect();
    list
}

#[test]
fn export_roundtrip_preserves_unknown_apps_but_excludes_machine_paths() {
    let program = InstalledProgram {
        id: "machine-specific".into(),
        name: "Локальный редактор".into(),
        version: "1.2".into(),
        publisher: "Example\r\nCompany\0".into(),
        quiet: false,
        target: UninstallTarget::Detected {
            path: r"C:\Users\private\app.exe".into(),
        },
        managed_ids: vec![],
        package_ids: vec![],
        icon_path: Some(r"C:\private.ico".into()),
    };
    let list = ProgramList::from_inventory(&[program]);
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("Рабочий ПК.json");
    list.save(&path).unwrap();
    list.save(&path).unwrap();
    let read = ProgramList::read(&path).unwrap();
    assert_eq!(read.programs[0].name, "Локальный редактор");
    assert_eq!(read.programs[0].publisher, "Example Company");
    assert!(read.programs[0].package_ids.is_empty());
    let json = std::fs::read_to_string(path).unwrap();
    assert!(!json.contains("private"));
    assert!(!json.contains("uninstall"));
}

#[test]
fn known_ids_are_deduplicated_and_do_not_reinstall_existing_versions() {
    let catalog = CatalogDocument::builtin().unwrap().catalog;
    let list = list(&[
        (&["7zip"], "7-Zip"),
        (&["7zip"], "7-Zip duplicate"),
        (&["vscode"], "VS Code"),
    ]);
    let installed = Library::from([(
        "vscode".into(),
        InstalledPackage {
            id: "vscode".into(),
            name: "VS Code".into(),
            version: "ancient".into(),
            sha256: "".into(),
            installed_at: 0,
            reboot_required: false,
            uninstall: None,
            externally_detected: true,
        },
    )]);
    let rows = list.match_catalog(&catalog, &installed);
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .find(|r| r.package_id.as_deref() == Some("7zip"))
            .unwrap()
            .status,
        MatchStatus::Available
    );
    assert_eq!(
        rows.iter()
            .find(|r| r.package_id.as_deref() == Some("vscode"))
            .unwrap()
            .status,
        MatchStatus::Installed
    );
    let selected = rows
        .iter()
        .filter(|r| r.status == MatchStatus::Available)
        .filter_map(|r| r.package_id.clone())
        .collect();
    let plan = crate::planner::create_plan(&catalog, &selected, &installed).unwrap();
    assert_eq!(plan.len(), 1);
    assert_ne!(plan[0].version, "old-version");
}

#[test]
fn unknown_ids_never_fall_back_to_a_different_app_with_the_same_name() {
    let catalog = CatalogDocument::builtin().unwrap().catalog;
    let rows = list(&[(&["old-custom-app"], "7-Zip"), (&["eset-premium"], "ESET")])
        .match_catalog(&catalog, &Library::new());
    assert!(
        rows.iter().any(|r| r.status == MatchStatus::Unknown
            && r.package_id.as_deref() == Some("old-custom-app"))
    );
    assert!(rows.iter().any(|r| r.status == MatchStatus::Unavailable));
}

#[test]
fn names_without_ids_match_only_when_unambiguous() {
    let mut catalog = CatalogDocument::builtin().unwrap().catalog;
    let list = list(&[(&[], "7-Zip"), (&[], "Unlisted software")]);
    assert!(
        list.match_catalog(&catalog, &Library::new())
            .iter()
            .any(|r| r.status == MatchStatus::Available)
    );
    let mut duplicate = catalog.package("7zip").unwrap().clone();
    duplicate.id = "another-7zip".into();
    catalog.packages.push(duplicate);
    assert!(
        list.match_catalog(&catalog, &Library::new())
            .iter()
            .any(|r| r.status == MatchStatus::Ambiguous)
    );
}

#[test]
fn import_rejects_recipes_future_versions_and_oversized_input() {
    let list = list(&[(&["7zip"], "7-Zip")]);
    let mut value = serde_json::to_value(&list).unwrap();
    value["programs"][0]["install"] = serde_json::json!({"type":"exe", "silent_args":["/S"]});
    assert!(ProgramList::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    value["programs"][0]
        .as_object_mut()
        .unwrap()
        .remove("install");
    value["schema_version"] = 99.into();
    assert!(ProgramList::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    assert!(ProgramList::parse(&vec![b' '; MAX_BYTES + 1]).is_err());
    value["schema_version"] = 1.into();
    value["programs"][0]["package_ids"] = serde_json::json!(["../program"]);
    assert!(ProgramList::parse(&serde_json::to_vec(&value).unwrap()).is_err());
    assert!(ProgramList::parse(b"not json").is_err());
    let mut bytes = b"\xef\xbb\xbf".to_vec();
    bytes.extend(serde_json::to_vec(&list).unwrap());
    assert!(ProgramList::parse(&bytes).is_ok());
}
