use std::collections::BTreeSet;

use anyhow::{Result, ensure};

use crate::catalog::{Catalog, Package};
use crate::storage::{Library, is_installed};
use crate::uninstall::InstalledProgram;

pub fn removal_plan(
    catalog: &Catalog,
    state: &Library,
    mut programs: Vec<InstalledProgram>,
) -> Result<Vec<InstalledProgram>> {
    for program in &mut programs {
        for entry in state.values().filter(|entry| {
            entry
                .uninstall
                .as_ref()
                .is_some_and(|target| target.id() == program.id)
        }) {
            if !program.managed_ids.contains(&entry.id) {
                program.managed_ids.push(entry.id.clone());
            }
        }
    }
    let selected: BTreeSet<_> = programs.iter().map(|p| p.id.as_str()).collect();
    let package_ids: BTreeSet<String> = state
        .values()
        .filter(|entry| {
            entry
                .uninstall
                .as_ref()
                .is_some_and(|target| selected.contains(target.id().as_str()))
        })
        .map(|entry| entry.id.clone())
        .filter(|id| catalog.package(id).is_some())
        .collect();
    for entry in state
        .values()
        .filter(|entry| !package_ids.contains(&entry.id) && catalog.package(&entry.id).is_some())
    {
        let dependencies = catalog.dependency_order(&BTreeSet::from([entry.id.clone()]))?;
        ensure!(
            !dependencies.iter().any(|p| package_ids.contains(&p.id)),
            "Сначала добавьте в удаление зависимый пакет «{}»",
            entry.name
        );
    }
    let order = catalog.dependency_order(&package_ids)?;
    let ranks: std::collections::HashMap<_, _> = order
        .into_iter()
        .rev()
        .enumerate()
        .map(|(index, package)| (package.id.as_str(), index))
        .collect();
    programs.sort_by_key(|program| {
        state
            .values()
            .filter(|entry| {
                entry
                    .uninstall
                    .as_ref()
                    .is_some_and(|target| target.id() == program.id)
            })
            .filter_map(|entry| ranks.get(entry.id.as_str()).copied())
            .min()
            .unwrap_or(usize::MAX)
    });
    Ok(programs)
}

pub fn create_plan(
    catalog: &Catalog,
    selected: &BTreeSet<String>,
    library: &Library,
) -> Result<Vec<Package>> {
    let mut plan = Vec::new();
    for package in catalog.dependency_order(selected)? {
        if is_installed(package, library) {
            continue;
        }
        ensure!(
            package.enabled,
            "Пакет «{}» пока недоступен для установки",
            package.name
        );
        ensure!(
            package.ready(),
            "У пакета «{}» нет установщика",
            package.name
        );
        plan.push(package.clone());
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Artifact, CatalogDocument, InstallSpec};
    use crate::storage::InstalledPackage;

    fn installable_catalog() -> Catalog {
        let mut catalog = CatalogDocument::demo().unwrap().catalog;
        for package in &mut catalog.packages {
            package.enabled = true;
            package.artifact = Some(Artifact {
                file_name: "setup.exe".into(),
                size: 1,
                sha256: "a".repeat(64),
                local_path: Some("setup.exe".into()),
                drive_file_id: None,
                url: None,
            });
            package.install = Some(InstallSpec::Exe {
                silent_args: vec!["/S".into()],
                requires_admin: false,
            });
        }
        catalog
    }

    #[test]
    fn addon_installs_host_first_and_deduplicates_selection() {
        let catalog = installable_catalog();
        let selected = BTreeSet::from(["blender".into(), "blender-addon".into()]);
        let plan = create_plan(&catalog, &selected, &Library::new()).unwrap();
        assert_eq!(
            plan.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
            ["blender", "blender-addon"]
        );
    }

    #[test]
    fn matching_install_is_skipped_but_new_hash_is_an_update() {
        let catalog = installable_catalog();
        let package = catalog.package("blender").unwrap();
        let entry = InstalledPackage {
            id: package.id.clone(),
            name: package.name.clone(),
            version: package.version.clone(),
            sha256: "a".repeat(64),
            installed_at: 0,
            reboot_required: false,
            uninstall: None,
            externally_detected: false,
        };
        let mut library = Library::from([(entry.id.clone(), entry)]);
        let selected = BTreeSet::from(["blender-addon".into()]);
        assert_eq!(create_plan(&catalog, &selected, &library).unwrap().len(), 1);
        library.get_mut("blender").unwrap().sha256 = "b".repeat(64);
        assert_eq!(create_plan(&catalog, &selected, &library).unwrap().len(), 2);
    }
}
