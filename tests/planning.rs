use softdownloader::catalog::{ArchiveDestination, ArchiveRoot, Catalog, CatalogDocument};
use softdownloader::planner::removal_plan;
use softdownloader::storage::{InstalledPackage, Library};
use softdownloader::uninstall::{InstalledProgram, UninstallTarget};

#[test]
fn marketplace_extension_adds_editor_and_manual_packages_are_not_install_jobs() {
    let catalog = CatalogDocument::builtin().unwrap().catalog;
    let plan = softdownloader::planner::create_plan(
        &catalog,
        &std::collections::BTreeSet::from(["cline".into()]),
        &Library::new(),
    )
    .unwrap();
    assert_eq!(
        plan.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        ["vscode", "cline"]
    );
    assert!(
        softdownloader::planner::create_plan(
            &catalog,
            &std::collections::BTreeSet::from(["ame-wizard".into()]),
            &Library::new()
        )
        .is_err()
    );
}

#[test]
fn category_tree_keeps_nested_groups_next_to_their_parent() {
    let catalog = CatalogDocument::builtin().unwrap().catalog;
    let tree = catalog.category_tree();
    let parent = tree.iter().position(|(c, _)| c.id == "runtimes").unwrap();
    let child = tree.iter().position(|(c, _)| c.id == "java").unwrap();
    assert_eq!(child, parent + 1);
    assert_eq!(tree[child].1, tree[parent].1 + 1);
    assert_eq!(tree.len(), catalog.categories.len());
}

#[test]
fn rejects_incompatible_managers_and_artifacts() {
    let mut catalog = CatalogDocument::builtin().unwrap().catalog;
    let package = catalog
        .packages
        .iter_mut()
        .find(|p| p.id == "chrome")
        .unwrap();
    package.install = Some(softdownloader::catalog::InstallSpec::Exe {
        silent_args: vec!["/S".into()],
        requires_admin: false,
    });
    assert!(catalog.validate().is_err());
    let mut catalog = CatalogDocument::builtin().unwrap().catalog;
    let package = catalog
        .packages
        .iter_mut()
        .find(|p| p.id == "cline")
        .unwrap();
    package.source = Some(softdownloader::discovery::Source::Github {
        repository: "vendor/app".into(),
        asset_pattern: "file.exe".into(),
    });
    assert!(catalog.validate().is_err());
}

#[test]
fn removal_orders_addons_first_and_protects_unselected_dependents() {
    let catalog = CatalogDocument::demo().unwrap().catalog;
    let mut state = Library::new();
    let mut programs = Vec::new();
    for id in ["blender", "blender-addon"] {
        let target = UninstallTarget::Archive {
            package_id: id.into(),
            destination: ArchiveDestination {
                root: ArchiveRoot::LocalAppData,
                path: format!("test/{id}"),
            },
        };
        programs.push(InstalledProgram {
            id: target.id(),
            name: id.into(),
            version: "1".into(),
            publisher: "Test".into(),
            quiet: true,
            target: target.clone(),
            managed_ids: vec![id.into()],
            package_ids: vec![id.into()],
        });
        state.insert(
            id.into(),
            InstalledPackage {
                id: id.into(),
                name: id.into(),
                version: "1".into(),
                sha256: "a".repeat(64),
                installed_at: 0,
                reboot_required: false,
                uninstall: Some(target),
                externally_detected: false,
            },
        );
    }
    assert!(removal_plan(&catalog, &state, vec![programs[0].clone()]).is_err());
    let plan = removal_plan(&catalog, &state, programs).unwrap();
    assert_eq!(
        plan.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        ["blender-addon", "blender"]
    );
}

#[test]
fn additional_catalog_can_reference_builtin_packages_and_rejects_duplicates() {
    let additional = serde_json::json!({"schema_version":1,"title":"Extras","categories":[],"packages":[{
        "id":"http-extra","name":"Extra","version":"1","publisher":"Test","description":"Test","category":"network-tools","kind":"addon","depends_on":["httpdebugger"],"enabled":false
    }]});
    let merged = Catalog::parse_with_builtin(&serde_json::to_vec(&additional).unwrap()).unwrap();
    assert_eq!(
        merged.packages.len(),
        CatalogDocument::builtin().unwrap().catalog.packages.len() + 1
    );
    assert_eq!(
        merged.package("http-extra").unwrap().depends_on,
        ["httpdebugger"]
    );
    let mut duplicate = additional;
    let entry = duplicate["packages"][0].clone();
    duplicate["packages"].as_array_mut().unwrap().push(entry);
    assert!(Catalog::parse_with_builtin(&serde_json::to_vec(&duplicate).unwrap()).is_err());
}
