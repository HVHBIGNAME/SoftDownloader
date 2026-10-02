use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::Path;

use anyhow::{Context, Result, ensure};
use directories::{BaseDirs, UserDirs};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::catalog::{ArchiveDestination, ArchiveRoot};
use crate::paths::{reject_symlink_ancestors, safe_relative_path};
use crate::storage::write_json;

const MARKER: &str = ".softdownloader-package.json";
const MAX_FILES: usize = 25_000;
const MAX_EXPANDED_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Deserialize, Serialize)]
struct Ownership {
    package_id: String,
}

pub fn install(
    archive: &Path,
    package_id: &str,
    destination: &ArchiveDestination,
    strip: u8,
    cancel: &CancellationToken,
) -> Result<()> {
    let target = target_path(destination)?;
    install_at(archive, package_id, &target, strip, cancel)
}

fn target_path(destination: &ArchiveDestination) -> Result<std::path::PathBuf> {
    let base = BaseDirs::new().context("Не найдены пользовательские папки")?;
    let root = match destination.root {
        ArchiveRoot::RoamingAppData => base.data_dir().to_owned(),
        ArchiveRoot::LocalAppData => base.data_local_dir().to_owned(),
        ArchiveRoot::Documents => UserDirs::new()
            .and_then(|dirs| dirs.document_dir().map(|p| p.to_owned()))
            .context("Не найдена папка Документы")?,
    };
    let root = root
        .canonicalize()
        .context("Корневая папка установки недоступна")?;
    let target = root.join(safe_relative_path(&destination.path)?);
    reject_symlink_ancestors(&root, &target)?;
    Ok(target)
}

pub fn remove(package_id: &str, destination: &ArchiveDestination) -> Result<()> {
    let target = target_path(destination)?;
    remove_at(package_id, &target)
}

fn remove_at(package_id: &str, target: &Path) -> Result<()> {
    let owner: Ownership = serde_json::from_slice(
        &std::fs::read(target.join(MARKER)).context("Папка не управляется SoftDownloader")?,
    )?;
    ensure!(
        owner.package_id == package_id,
        "Папка принадлежит другому пакету"
    );
    for entry in std::fs::read_dir(target)? {
        let entry = entry?;
        if entry.file_name() == MARKER {
            continue;
        }
        let metadata = entry.file_type()?;
        if metadata.is_dir() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    std::fs::remove_file(target.join(MARKER))?;
    std::fs::remove_dir(target)?;
    Ok(())
}

pub fn is_registered(package_id: &str, destination: &ArchiveDestination) -> Result<bool> {
    let path = target_path(destination)?.join(MARKER);
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let owner: Ownership = serde_json::from_slice(&bytes)?;
    Ok(owner.package_id == package_id)
}

fn install_at(
    archive: &Path,
    package_id: &str,
    target: &Path,
    strip: u8,
    cancel: &CancellationToken,
) -> Result<()> {
    if target.exists() {
        let marker = std::fs::read(target.join(MARKER)).context("Папка аддона уже существует и не управляется SoftDownloader. Укажите отдельную папку пакета")?;
        let owner: Ownership = serde_json::from_slice(&marker)?;
        ensure!(
            owner.package_id == package_id,
            "Папка принадлежит другому пакету"
        );
    }
    let parent = target.parent().context("Нет родительской папки аддона")?;
    std::fs::create_dir_all(parent)?;
    let stage = tempfile::Builder::new()
        .prefix(".softdownloader-stage-")
        .tempdir_in(parent)?;
    let payload = stage.path().join("payload");
    std::fs::create_dir(&payload)?;
    extract(archive, &payload, strip, cancel)?;
    write_json(
        &payload.join(MARKER),
        &Ownership {
            package_id: package_id.into(),
        },
    )?;
    ensure!(!cancel.is_cancelled(), "Отменено");
    let backup = tempfile::Builder::new()
        .prefix(".softdownloader-backup-")
        .tempdir_in(parent)?;
    let previous = backup.path().join("previous");
    let replacing = target.exists();
    if replacing {
        std::fs::rename(target, &previous)
            .context("Закройте программу, использующую аддон, и повторите установку")?;
    }
    if let Err(error) = std::fs::rename(&payload, target) {
        if replacing && let Err(rollback) = std::fs::rename(&previous, target) {
            let saved = backup.keep();
            anyhow::bail!(
                "Не удалось установить: {error}. Откат: {rollback}. Предыдущая версия сохранена в {}",
                saved.display()
            );
        }
        return Err(error).context("Не удалось переместить готовый аддон");
    }
    Ok(())
}

fn extract(archive: &Path, target: &Path, strip: u8, cancel: &CancellationToken) -> Result<()> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file).context("Некорректный ZIP-архив")?;
    ensure!(
        zip.len() <= MAX_FILES,
        "В архиве больше {MAX_FILES} записей"
    );
    let mut seen = HashSet::new();
    let mut total = 0_u64;
    let mut files = 0_usize;
    for index in 0..zip.len() {
        ensure!(!cancel.is_cancelled(), "Отменено");
        let mut entry = zip.by_index(index)?;
        ensure!(
            entry
                .unix_mode()
                .is_none_or(|mode| mode & 0o170000 != 0o120000),
            "Символические ссылки в ZIP не поддерживаются"
        );
        let name = entry.name().trim_end_matches(['/', '\\']);
        let relative = safe_relative_path(name).context("Недопустимый путь внутри ZIP")?;
        let relative = relative
            .components()
            .skip(strip as usize)
            .collect::<std::path::PathBuf>();
        if relative.as_os_str().is_empty() {
            continue;
        }
        let key = relative.to_string_lossy().replace('\\', "/").to_lowercase();
        ensure!(key != MARKER, "ZIP содержит служебный файл SoftDownloader");
        let output = target.join(&relative);
        if entry.is_dir() {
            std::fs::create_dir_all(output)?;
            continue;
        }
        ensure!(seen.insert(key), "Повторяющиеся имена файлов внутри ZIP");
        total = total
            .checked_add(entry.size())
            .context("ZIP слишком большой")?;
        ensure!(
            total <= MAX_EXPANDED_BYTES,
            "Распакованный аддон превышает 8 ГБ"
        );
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut writer = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output)?;
        let expected = entry.size();
        let mut written = 0_u64;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            ensure!(!cancel.is_cancelled(), "Отменено");
            let length = entry.read(&mut buffer)?;
            if length == 0 {
                break;
            }
            written = written
                .checked_add(length as u64)
                .context("ZIP слишком большой")?;
            ensure!(
                written <= expected,
                "Размер ZIP-записи изменился при распаковке"
            );
            writer.write_all(&buffer[..length])?;
        }
        ensure!(written == expected, "ZIP-запись распакована не полностью");
        writer.sync_all()?;
        files += 1;
    }
    ensure!(
        files > 0,
        "После strip_components в архиве не осталось файлов"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    fn make_zip(path: &Path, name: &str, bytes: &[u8]) {
        let mut writer = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
        writer
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn blocks_traversal_before_stripping_and_preserves_unmanaged_folders() {
        let temp = tempfile::tempdir().unwrap();
        let zip = temp.path().join("addon.zip");
        let target = temp.path().join("addon");
        make_zip(&zip, "../evil.txt", b"bad");
        assert!(install_at(&zip, "addon", &target, 1, &CancellationToken::new()).is_err());
        assert!(!temp.path().join("evil.txt").exists());
        std::fs::create_dir(&target).unwrap();
        std::fs::write(target.join("personal.txt"), "keep me").unwrap();
        make_zip(&zip, "plugin.py", b"valid");
        assert!(install_at(&zip, "addon", &target, 0, &CancellationToken::new()).is_err());
        assert_eq!(
            std::fs::read_to_string(target.join("personal.txt")).unwrap(),
            "keep me"
        );
    }

    #[test]
    fn installs_and_replaces_only_its_own_directory() {
        let temp = tempfile::tempdir().unwrap();
        let zip = temp.path().join("addon.zip");
        let target = temp.path().join("addon");
        make_zip(&zip, "release/old.py", b"old");
        install_at(&zip, "addon", &target, 1, &CancellationToken::new()).unwrap();
        make_zip(&zip, "release/new.py", b"new");
        install_at(&zip, "addon", &target, 1, &CancellationToken::new()).unwrap();
        assert!(!target.join("old.py").exists());
        assert_eq!(std::fs::read(target.join("new.py")).unwrap(), b"new");
        assert!(install_at(&zip, "another-addon", &target, 1, &CancellationToken::new()).is_err());
    }

    #[test]
    fn failed_update_keeps_the_previous_version() {
        let temp = tempfile::tempdir().unwrap();
        let zip = temp.path().join("addon.zip");
        let target = temp.path().join("addon");
        make_zip(&zip, "plugin.py", b"working version");
        install_at(&zip, "addon", &target, 0, &CancellationToken::new()).unwrap();
        make_zip(&zip, "C:/bad.txt", b"bad");
        assert!(install_at(&zip, "addon", &target, 0, &CancellationToken::new()).is_err());
        assert_eq!(
            std::fs::read(target.join("plugin.py")).unwrap(),
            b"working version"
        );
        assert!(remove_at("wrong-owner", &target).is_err());
        assert!(target.join("plugin.py").exists());
        remove_at("addon", &target).unwrap();
        assert!(!target.exists());
    }
}
