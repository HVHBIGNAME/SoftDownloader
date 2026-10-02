#[cfg(windows)]
mod registry;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

use crate::catalog::{ArchiveDestination, InstallSpec, Package};
use crate::installer::{self, InstallOutcome};
use crate::storage::Library;

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
    Winget {
        package_id: String,
    },
    VscodeExtension {
        extension_id: String,
    },
    Appx {
        full_name: String,
    },
    Detected {
        path: PathBuf,
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
        hex::encode(Sha256::digest(
            serde_json::to_vec(self).expect("JSON-only uninstall descriptor"),
        ))
    }
    pub fn can_remove(&self) -> bool {
        match self {
            Self::Detected { .. } => false,
            Self::VscodeExtension { .. } => crate::system::vscode::executable().is_ok(),
            _ => true,
        }
    }
    pub fn folder(&self) -> Option<PathBuf> {
        match self {
            Self::Detected { path } => path.parent().map(Path::to_owned),
            Self::Archive { destination, .. } => installer::archive::target_path(destination).ok(),
            _ => None,
        }
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
    pub package_ids: Vec<String>,
    /// Executable that provides the real program icon in the interface.
    pub icon_path: Option<PathBuf>,
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
                publisher: "Мой каталог · portable".into(),
                quiet: true,
                target: target.clone(),
                managed_ids: vec![entry.id.clone()],
                package_ids: vec![entry.id.clone()],
                icon_path: installer::archive::target_path(destination)
                    .ok()
                    .and_then(|folder| crate::system::icons::primary_file(&folder)),
            });
        }
    }
    programs.sort_by_cached_key(|p| p.name.to_lowercase());
    Ok(programs)
}

pub fn target_for_package(package: &Package) -> Result<Option<UninstallTarget>> {
    match &package.install {
        Some(InstallSpec::Zip { destination, .. } | InstallSpec::Portable { destination }) => {
            return Ok(Some(UninstallTarget::Archive {
                package_id: package.id.clone(),
                destination: destination.clone(),
            }));
        }
        Some(InstallSpec::Winget) => {
            return Ok(package.winget_id().map(|id| UninstallTarget::Winget {
                package_id: id.into(),
            }));
        }
        Some(InstallSpec::VscodeExtension { extension_id }) => {
            return Ok(Some(UninstallTarget::VscodeExtension {
                extension_id: extension_id.clone(),
            }));
        }
        _ => {}
    }
    let matches: Vec<_> = scan(&Library::new())?
        .into_iter()
        .filter(|p| crate::inventory::matches_name(package, &p.name))
        .collect();
    Ok(if matches.len() == 1 {
        matches.into_iter().next().map(|p| p.target)
    } else {
        None
    })
}

pub fn remove(program: &InstalledProgram, logs: &Path) -> Result<InstallOutcome> {
    let log = logs.join(format!("remove-{}.log", program.id));
    match &program.target {
        UninstallTarget::Archive {
            package_id,
            destination,
        } => {
            installer::archive::remove(package_id, destination)?;
            Ok(InstallOutcome::default())
        }
        UninstallTarget::Registry { .. } => remove_registered(program, logs),
        UninstallTarget::Winget { package_id } => crate::system::winget::remove(package_id, &log),
        UninstallTarget::VscodeExtension { extension_id } => {
            crate::system::vscode::remove(extension_id, &log)
        }
        UninstallTarget::Appx { full_name } => crate::system::appx::remove(full_name, &log),
        UninstallTarget::Detected { .. } => anyhow::bail!(
            "Программа найдена по файлу, но не зарегистрировала деинсталлятор. Откройте её папку"
        ),
    }
}

#[cfg(windows)]
fn remove_registered(program: &InstalledProgram, logs: &Path) -> Result<InstallOutcome> {
    registry::remove(program, logs)
}
#[cfg(not(windows))]
fn remove_registered(_program: &InstalledProgram, _logs: &Path) -> Result<InstallOutcome> {
    anyhow::bail!("Удаление программ поддерживается в Windows")
}
