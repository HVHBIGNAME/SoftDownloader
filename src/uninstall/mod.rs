#[cfg(windows)]
mod registry;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

use crate::catalog::{ArchiveDestination, Catalog, InstallSpec, Package};
use crate::installer::{self, InstallOutcome};
use crate::storage::{InstalledPackage, Library};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UninstallTarget {
    Registry {
        hive: Hive,
        key: String,
        is_64bit: bool,
    },
    Archive {
        package_id: String,
        destination: ArchiveDestination,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Hive {
    CurrentUser,
    LocalMachine,
}

impl UninstallTarget {
    pub fn id(&self) -> String {
        let serialized =
            serde_json::to_vec(self).expect("uninstall targets contain only JSON values");
        hex::encode(Sha256::digest(serialized))
    }
}

#[derive(Clone, Debug)]
pub struct InstalledProgram {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub quiet: bool,
    pub target: UninstallTarget,
    pub managed_ids: Vec<String>,
}

pub fn scan(library: &Library) -> Result<Vec<InstalledProgram>> {
    #[cfg(windows)]
    let mut programs = registry::scan()?;
    #[cfg(not(windows))]
    let mut programs = Vec::<InstalledProgram>::new();
    for entry in library.values() {
        let Some(target) = &entry.uninstall else {
            continue;
        };
        let id = target.id();
        if let Some(program) = programs.iter_mut().find(|p| p.id == id) {
            program.managed_ids.push(entry.id.clone());
        } else if let UninstallTarget::Archive {
            package_id,
            destination,
        } = target
            && installer::archive::is_registered(package_id, destination)?
        {
            programs.push(InstalledProgram {
                id,
                name: entry.name.clone(),
                version: entry.version.clone(),
                publisher: "Мой каталог · ZIP".into(),
                quiet: true,
                target: target.clone(),
                managed_ids: vec![entry.id.clone()],
            });
        }
    }
    programs.sort_by_cached_key(|p| p.name.to_lowercase());
    Ok(programs)
}

pub fn target_for_package(package: &Package) -> Result<Option<UninstallTarget>> {
    if let Some(InstallSpec::Zip { destination, .. }) = &package.install {
        return Ok(Some(UninstallTarget::Archive {
            package_id: package.id.clone(),
            destination: destination.clone(),
        }));
    }
    let name = normalize_name(&package.name);
    let programs = scan(&Library::new())?;
    let matches: Vec<_> = programs
        .into_iter()
        .filter(|p| normalize_name(&p.name).starts_with(&name))
        .collect();
    Ok(if matches.len() == 1 {
        matches.into_iter().next().map(|p| p.target)
    } else {
        None
    })
}

pub fn normalize_name(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn installation_state(
    catalog: &Catalog,
    library: &Library,
    programs: &[InstalledProgram],
) -> Library {
    let mut state = library.clone();
    for package in &catalog.packages {
        if let Some(entry) = state.get_mut(&package.id) {
            if let Some(target) = &entry.uninstall {
                if let Some(program) = programs.iter().find(|p| p.id == target.id()) {
                    if !program.version.is_empty() && entry.version != program.version {
                        entry.version = program.version.clone();
                        entry.externally_detected = true;
                    }
                } else {
                    state.remove(&package.id);
                }
            }
            continue;
        }
        let name = normalize_name(&package.name);
        let mut matches = programs
            .iter()
            .filter(|p| normalize_name(&p.name).starts_with(&name));
        if let Some(program) = matches.next()
            && matches.next().is_none()
        {
            state.insert(
                package.id.clone(),
                InstalledPackage {
                    id: package.id.clone(),
                    name: program.name.clone(),
                    version: program.version.trim_start_matches('v').to_owned(),
                    sha256: String::new(),
                    installed_at: 0,
                    reboot_required: false,
                    uninstall: Some(program.target.clone()),
                    externally_detected: true,
                },
            );
        }
    }
    state
}

pub fn remove(program: &InstalledProgram, logs: &Path) -> Result<InstallOutcome> {
    match &program.target {
        UninstallTarget::Archive {
            package_id,
            destination,
        } => {
            installer::archive::remove(package_id, destination)?;
            Ok(InstallOutcome::default())
        }
        UninstallTarget::Registry { .. } => remove_registered(program, logs),
    }
}

#[cfg(windows)]
fn remove_registered(program: &InstalledProgram, logs: &Path) -> Result<InstallOutcome> {
    registry::remove(program, logs)
}

#[cfg(not(windows))]
fn remove_registered(_program: &InstalledProgram, _logs: &Path) -> Result<InstallOutcome> {
    anyhow::bail!("Удаление зарегистрированных программ поддерживается только в Windows")
}
