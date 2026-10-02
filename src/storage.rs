use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::catalog::Package;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub catalog_source: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstalledPackage {
    pub id: String,
    pub name: String,
    pub version: String,
    pub sha256: String,
    pub installed_at: u64,
    pub reboot_required: bool,
    #[serde(default)]
    pub uninstall: Option<crate::uninstall::UninstallTarget>,
    #[serde(default)]
    pub externally_detected: bool,
}

pub type Library = BTreeMap<String, InstalledPackage>;

pub fn is_installed(package: &Package, library: &Library) -> bool {
    library.get(&package.id).is_some_and(|entry| {
        if package.is_managed() || package.is_manual() {
            return true;
        }
        if package.source.is_some() && !crate::inventory::useful_version(&package.version) {
            return true;
        }
        (crate::inventory::same_version(&entry.version, &package.version)
            || (entry.externally_detected && !crate::inventory::useful_version(&entry.version)))
            && package.artifact.as_ref().is_none_or(|a| {
                entry.externally_detected || a.sha256.is_empty() || a.sha256 == entry.sha256
            })
    })
}

#[derive(Clone, Debug)]
pub struct Store {
    pub root: PathBuf,
    pub cache: PathBuf,
    pub logs: PathBuf,
}

impl Store {
    pub fn open() -> Result<Self> {
        let dirs = ProjectDirs::from("io", "SoftDownloader", "SoftDownloader")
            .context("Не найдена папка данных пользователя")?;
        Self::at(dirs.data_local_dir().to_owned())
    }

    pub fn at(root: PathBuf) -> Result<Self> {
        let store = Self {
            cache: root.join("downloads"),
            logs: root.join("logs"),
            root,
        };
        for directory in [&store.root, &store.cache, &store.logs] {
            std::fs::create_dir_all(directory)
                .with_context(|| format!("Не удалось создать {}", directory.display()))?;
        }
        Ok(store)
    }

    pub fn load_settings(&self) -> Result<Settings> {
        read_or_default(&self.root.join("settings.json"))
    }
    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        write_json(&self.root.join("settings.json"), settings)
    }
    pub fn load_library(&self) -> Result<Library> {
        read_or_default(&self.root.join("installed.json"))
    }
    /// Folder for cached network responses such as the resolved catalog.
    pub fn cache_directory(&self) -> PathBuf {
        self.root.join("catalog-cache")
    }
    pub fn save_library(&self, library: &Library) -> Result<()> {
        write_json(&self.root.join("installed.json"), library)
    }
}

fn read_or_default<T: DeserializeOwned + Default>(path: &Path) -> Result<T> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .with_context(|| format!("Не удалось прочитать {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(error) => Err(error).with_context(|| format!("Не удалось открыть {}", path.display())),
    }
}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("У файла нет родительской папки")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(temporary.as_file_mut(), value)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary
        .persist(path)
        .with_context(|| format!("Не удалось сохранить {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_replace_atomically_and_invalid_json_is_not_ignored() {
        let temporary = tempfile::tempdir().unwrap();
        let store = Store::at(temporary.path().to_owned()).unwrap();
        assert!(store.load_settings().unwrap().catalog_source.is_empty());
        store
            .save_settings(&Settings {
                catalog_source: "one".into(),
            })
            .unwrap();
        store
            .save_settings(&Settings {
                catalog_source: "two".into(),
            })
            .unwrap();
        assert_eq!(store.load_settings().unwrap().catalog_source, "two");
        std::fs::write(store.root.join("settings.json"), b"broken").unwrap();
        assert!(store.load_settings().is_err());
    }
}
