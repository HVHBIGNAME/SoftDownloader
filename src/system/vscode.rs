use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, ensure};
use directories::BaseDirs;
use serde::Deserialize;

use super::run_logged;
use crate::installer::InstallOutcome;

pub fn valid_extension_id(id: &str) -> bool {
    let parts: Vec<_> = id.split('.').collect();
    parts.len() == 2
        && parts.iter().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
        })
}

pub fn executable() -> Result<PathBuf> {
    let dirs = BaseDirs::new().context("Нет папки пользователя")?;
    let mut paths = vec![
        dirs.data_local_dir()
            .join("Programs/Microsoft VS Code/Code.exe"),
    ];
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(variable) {
            paths.push(PathBuf::from(root).join("Microsoft VS Code/Code.exe"));
        }
    }
    paths.push(dirs.home_dir().join("scoop/apps/vscode/current/Code.exe"));
    paths
        .into_iter()
        .find(|path| path.is_file())
        .context("VS Code не найден. Установите Microsoft Visual Studio Code")
}

pub fn install(id: &str, log: &Path) -> Result<InstallOutcome> {
    operation(id, true, log)
}
pub fn remove(id: &str, log: &Path) -> Result<InstallOutcome> {
    operation(id, false, log)
}

fn operation(id: &str, install: bool, log: &Path) -> Result<InstallOutcome> {
    ensure!(valid_extension_id(id), "Некорректный ID расширения VS Code");
    let executable = executable()?;
    let cli = executable
        .parent()
        .context("Нет папки VS Code")?
        .join("resources/app/out/cli.js");
    ensure!(cli.is_file(), "Не найден CLI установленного VS Code");
    let mut command = Command::new(executable);
    command
        .env("ELECTRON_RUN_AS_NODE", "1")
        .arg(cli)
        .arg(if install {
            "--install-extension"
        } else {
            "--uninstall-extension"
        })
        .arg(id);
    if install {
        command.arg("--force");
    }
    let status = run_logged(&mut command, log)?;
    ensure!(
        status.success(),
        "Операция VS Code завершилась ошибкой. Журнал: {}",
        log.display()
    );
    Ok(InstallOutcome::default())
}

#[derive(Deserialize)]
struct Extension {
    identifier: Identifier,
    version: String,
}
#[derive(Deserialize)]
struct Identifier {
    id: String,
}

pub fn installed() -> Result<BTreeMap<String, String>> {
    let path = BaseDirs::new()
        .context("Нет папки пользователя")?
        .home_dir()
        .join(".vscode/extensions/extensions.json");
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(error.into()),
    };
    parse_extensions(&bytes)
}

pub fn parse_extensions(bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    ensure!(
        bytes.len() <= 8 * 1024 * 1024,
        "Список расширений VS Code слишком большой"
    );
    let entries: Vec<Extension> = serde_json::from_slice(bytes)?;
    Ok(entries
        .into_iter()
        .filter(|e| valid_extension_id(&e.identifier.id))
        .map(|e| (e.identifier.id.to_ascii_lowercase(), e.version))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_extension_metadata_without_launching_the_editor() {
        let result = parse_extensions(br#"[{"identifier":{"id":"saoudrizwan.claude-dev"},"version":"3.0.0","location":{"path":"unused"}}]"#).unwrap();
        assert_eq!(result["saoudrizwan.claude-dev"], "3.0.0");
        assert!(!valid_extension_id("--install-extension=bad"));
    }
}
