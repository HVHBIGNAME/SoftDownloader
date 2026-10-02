use super::*;
use crate::catalog::{Artifact, CatalogDocument};
use crate::storage::is_installed;
use crate::uninstall::Hive;

fn registered(name: &str, version: &str, key: &str) -> InstalledProgram {
    let target = UninstallTarget::Registry {
        hive: Hive::CurrentUser,
        key: key.into(),
        is_64bit: true,
    };
    InstalledProgram {
        id: target.id(),
        name: name.into(),
        version: version.into(),
        publisher: "Fixture".into(),
        quiet: true,
        target,
        managed_ids: Vec::new(),
        package_ids: Vec::new(),
    }
}

fn recorded(program: &InstalledProgram, id: &str) -> InstalledPackage {
    InstalledPackage {
        id: id.into(),
        name: program.name.clone(),
        version: program.version.clone(),
        sha256: "a".repeat(64),
        installed_at: 123,
        reboot_required: false,
        uninstall: Some(program.target.clone()),
        externally_detected: false,
    }
}

#[test]
fn distinguishes_similar_names_runtime_branches_and_architectures() {
    let catalog = CatalogDocument::builtin().unwrap().catalog;
    let matches = |id, name| matches_name(catalog.package(id).unwrap(), name);
    assert!(matches("blender", "BLENDER"));
    assert!(!matches("blender", "Blender Addon Manager"));
    assert!(!matches("notepad-plus-plus", "Notepad"));
    assert!(matches("firefox", "Mozilla Firefox (x64 ru)"));
    assert!(matches("flclash", "FlClash версия 0.8.98"));
    assert!(matches("happ", "Happ - Proxy Utility 2.9.1(537)"));
    assert!(matches(
        "java-21",
        "Eclipse Temurin JDK with Hotspot 21.0.12+7 (x64)"
    ));
    assert!(!matches(
        "java-8",
        "Eclipse Temurin JDK with Hotspot 21.0.12+7 (x64)"
    ));
    assert!(!matches(
        "java-21",
        "Eclipse Temurin JDK with Hotspot 210.0 (x64)"
    ));
    assert!(matches("python", "Python 3.14.7 (64-bit)"));
    assert!(!matches("python-313", "Python 3.14.7 (64-bit)"));
    assert!(matches(
        "vcredist-x64",
        "Microsoft Visual C++ 2015-2022 Redistributable (x64) - 14.44.35211"
    ));
    assert!(!matches(
        "vcredist-x86",
        "Microsoft Visual C++ 2015-2022 Redistributable (x64) - 14.44.35211"
    ));
    assert!(!matches("prism-cracked", "Prism Launcher"));
}

#[test]
fn binds_registry_aliases_and_recovers_explicit_versions_without_duplicate_rows() {
    let catalog = CatalogDocument::builtin().unwrap().catalog;
    let mut programs = vec![
        registered("2IP StartGuard (v1.1)", "", "startguard"),
        registered("Happ - Proxy Utility", "", "happ"),
        registered("Google Chrome", "154.0.1", "chrome"),
    ];
    bind_names(&catalog, &mut programs).unwrap();
    assert_eq!(programs[0].version, "1.1");
    assert_eq!(programs[0].package_ids, ["2ip-startguard"]);
    let packages = BTreeMap::from([
        ("happ.happ".into(), "< 3.3.6".into()),
        ("google.chrome".into(), "Unknown".into()),
    ]);
    merge_winget(&catalog, &mut programs, &packages);
    assert_eq!(programs.len(), 3);
    assert_eq!(programs[1].version, "Не определена");
    assert_eq!(programs[2].version, "154.0.1");
    assert!(matches!(
        programs[1].target,
        UninstallTarget::Registry { .. }
    ));
}

#[test]
fn observed_managed_install_keeps_hash_checks_and_drops_stale_history() {
    let mut catalog = CatalogDocument::demo().unwrap().catalog;
    let package = catalog
        .packages
        .iter_mut()
        .find(|p| p.id == "blender")
        .unwrap();
    package.version = "1.2".into();
    package.artifact = Some(Artifact {
        file_name: "setup.exe".into(),
        size: 1,
        sha256: "a".repeat(64),
        url: None,
        drive_file_id: None,
        local_path: Some("setup.exe".into()),
    });
    let mut program = registered("Blender", "1.2.0", "blender");
    program.package_ids.push("blender".into());
    let library = Library::from([("blender".into(), recorded(&program, "blender"))]);
    let state = installation_state(&catalog, &library, &[program.clone()]);
    assert!(!state["blender"].externally_detected);
    assert!(is_installed(catalog.package("blender").unwrap(), &state));
    catalog
        .packages
        .iter_mut()
        .find(|p| p.id == "blender")
        .unwrap()
        .artifact
        .as_mut()
        .unwrap()
        .sha256 = "b".repeat(64);
    assert!(!is_installed(catalog.package("blender").unwrap(), &state));
    assert!(!installation_state(&catalog, &library, &[]).contains_key("blender"));

    program.version = "1.3".into();
    let external = installation_state(&catalog, &library, &[program]);
    assert!(external["blender"].externally_detected);
    assert!(external["blender"].sha256.is_empty());
}

#[test]
fn unavailable_source_preserves_only_its_unverified_install_records() {
    let mut programs = [
        registered("Chrome", "1", "chrome"),
        registered("App", "1", "app"),
    ];
    programs[0].target = UninstallTarget::Winget {
        package_id: "Google.Chrome".into(),
    };
    let library = Library::from([
        ("chrome".into(), recorded(&programs[0], "chrome")),
        ("app".into(), recorded(&programs[1], "app")),
    ]);
    let mut inventory = Inventory {
        programs: Vec::new(),
        warnings: Vec::new(),
        unverified: BTreeSet::new(),
    };
    inventory.unavailable(
        "WinGet",
        anyhow::anyhow!("offline fixture"),
        &library,
        |target| matches!(target, UninstallTarget::Winget { .. }),
    );
    assert_eq!(inventory.unverified, BTreeSet::from(["chrome".into()]));
    assert_eq!(inventory.warnings.len(), 1);
}

#[test]
fn normalizes_versions_without_treating_ranges_as_exact_versions() {
    assert!(same_version("v1.2", "1.2.0.0"));
    assert!(!same_version("1.2", "1.2.1"));
    assert!(!same_version("1.2-beta", "1.2"));
    for version in ["< 3.3.6", " Unknown ", "Последняя", "Не определена", ""] {
        assert!(!useful_version(version));
    }
    assert!(expand_path(r"\\server\share\app.exe").is_none());
    assert!(expand_path("C:relative.exe").is_none());
    assert!(expand_path(r"C:\Programs\..\secret.exe").is_none());
}

#[test]
#[cfg(windows)]
fn detects_existing_portable_files_without_executing_or_owning_them() {
    let temporary = tempfile::tempdir().unwrap();
    let file = temporary.path().join("portable.exe");
    let mut catalog = CatalogDocument::demo().unwrap().catalog;
    let package = catalog
        .packages
        .iter_mut()
        .find(|p| p.id == "blender")
        .unwrap();
    package.detect.paths = vec![file.to_string_lossy().into_owned()];
    std::fs::write(&file, []).unwrap();
    let mut programs = Vec::new();
    add_portables(&catalog, &mut programs);
    assert!(programs.is_empty());
    std::fs::write(
        &file,
        b"Not an executable; never launch the detection fixture",
    )
    .unwrap();
    add_portables(&catalog, &mut programs);
    assert_eq!(programs.len(), 1);
    assert_eq!(programs[0].package_ids, ["blender"]);
    assert!(!programs[0].target.can_remove());
    assert_eq!(programs[0].target.folder().unwrap(), temporary.path());
}
