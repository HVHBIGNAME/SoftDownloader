use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};

pub fn validate_id(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 64,
        "ID должен содержать 1–64 символа"
    );
    ensure!(
        value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_'),
        "Недопустимый ID: {value}. Используйте a-z, 0-9, - и _"
    );
    Ok(())
}

pub fn safe_relative_path(value: &str) -> Result<PathBuf> {
    ensure!(!value.is_empty(), "Путь не может быть пустым");
    let mut result = PathBuf::new();
    for component in value.split(['/', '\\']) {
        ensure!(
            !component.is_empty() && component != "." && component != "..",
            "Путь должен быть относительным и не содержать . или ..: {value}"
        );
        ensure!(
            !component
                .chars()
                .any(|c| c.is_control() || "<>:\"|?*".contains(c)),
            "Недопустимые символы в пути: {value}"
        );
        ensure!(
            !component.ends_with(['.', ' ']),
            "Компонент пути не может оканчиваться точкой или пробелом: {value}"
        );
        let stem = component
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && matches!(stem.as_bytes()[3], b'1'..=b'9'));
        ensure!(!reserved, "Зарезервированное имя Windows: {component}");
        result.push(component);
    }
    Ok(result)
}

pub fn resolve_local_file(root: &Path, relative: &str) -> Result<PathBuf> {
    let root = root
        .canonicalize()
        .context("Не удалось открыть папку каталога")?;
    let path = root.join(safe_relative_path(relative)?);
    let resolved = path.canonicalize().with_context(|| {
        format!(
            "Файл {} недоступен. Проверьте синхронизацию Google Drive",
            path.display()
        )
    })?;
    ensure!(
        resolved.starts_with(&root),
        "Файл выходит за пределы папки каталога"
    );
    ensure!(resolved.is_file(), "Установщик должен быть обычным файлом");
    Ok(resolved)
}

pub fn reject_symlink_ancestors(root: &Path, target: &Path) -> Result<()> {
    ensure!(
        target.starts_with(root) && target != root,
        "Недопустимая папка установки"
    );
    for ancestor in target.ancestors() {
        if ancestor == root {
            break;
        }
        match std::fs::symlink_metadata(ancestor) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!(
                    "Папка установки содержит символическую ссылку: {}",
                    ancestor.display()
                );
            }
            Ok(metadata) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
                    ensure!(
                        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0,
                        "Папка установки содержит junction/reparse point: {}",
                        ancestor.display()
                    );
                }
                let _ = metadata;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_that_escape_or_alias_on_windows() {
        for value in [
            "../setup.exe",
            "C:\\setup.exe",
            "/setup.exe",
            "\\\\host\\file",
            "a//b",
            "a/./b",
            "NUL.exe",
            "com1/file",
            "a:stream",
            "folder./file",
            "a\\..\\b",
        ] {
            assert!(safe_relative_path(value).is_err(), "accepted {value}");
        }
        assert_eq!(
            safe_relative_path("installers/Мой софт/setup.exe").unwrap(),
            PathBuf::from("installers")
                .join("Мой софт")
                .join("setup.exe")
        );
    }
}
