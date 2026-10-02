use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use directories::BaseDirs;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::{capture, run_logged, valid_tool_id};
use crate::discovery::WingetRepository;
use crate::installer::InstallOutcome;

pub fn executable() -> Result<PathBuf> {
    ensure!(cfg!(windows), "WinGet поддерживается только в Windows");
    let path = BaseDirs::new()
        .context("Нет папки пользователя")?
        .data_local_dir()
        .join("Microsoft/WindowsApps/winget.exe");
    ensure!(
        path.is_file(),
        "WinGet не найден. Установите «Установщик приложений» Microsoft из Store"
    );
    Ok(path)
}

pub fn available() -> bool {
    executable().is_ok()
}

pub fn verify(id: &str, repository: WingetRepository, cancel: &CancellationToken) -> Result<()> {
    ensure!(valid_tool_id(id), "Некорректный ID WinGet");
    let output = capture(
        Command::new(executable()?).args([
            "show",
            "--id",
            id,
            "--exact",
            "--source",
            repository.as_str(),
            "--accept-source-agreements",
            "--disable-interactivity",
        ]),
        Duration::from_secs(45),
        cancel,
    )?;
    ensure!(
        output.status.success(),
        "WinGet не подтвердил пакет {id}: {}",
        String::from_utf8_lossy(&output.stdout).trim()
    );
    Ok(())
}

pub fn install(id: &str, repository: WingetRepository, log: &Path) -> Result<InstallOutcome> {
    let arguments = install_arguments(id, repository)?;
    let status = run_logged(Command::new(executable()?).args(arguments), log)?;
    let code = status.code().unwrap_or(-1) as u32;
    let reboot_required = matches!(code, 3010 | 1641 | 0x8A15_0109 | 0x8A15_010B);
    // UPDATE_NOT_APPLICABLE means the requested package is already current.
    ensure!(
        status.success() || code == 0x8A15_002B || reboot_required,
        "WinGet завершился с кодом 0x{code:08X}. Журнал: {}",
        log.display()
    );
    Ok(InstallOutcome { reboot_required })
}

fn install_arguments(id: &str, repository: WingetRepository) -> Result<Vec<String>> {
    ensure!(valid_tool_id(id), "Некорректный ID WinGet");
    Ok([
        "install",
        "--id",
        id,
        "--exact",
        "--source",
        repository.as_str(),
        "--silent",
        "--accept-source-agreements",
        "--accept-package-agreements",
        "--disable-interactivity",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect())
}

pub fn remove(id: &str, log: &Path) -> Result<InstallOutcome> {
    ensure!(valid_tool_id(id), "Некорректный ID WinGet");
    let status = run_logged(
        Command::new(executable()?).args([
            "uninstall",
            "--id",
            id,
            "--exact",
            "--silent",
            "--accept-source-agreements",
            "--disable-interactivity",
        ]),
        log,
    )?;
    ensure!(
        status.success(),
        "WinGet не смог удалить пакет ({:?}). Журнал: {}",
        status.code(),
        log.display()
    );
    Ok(InstallOutcome::default())
}

#[derive(Deserialize)]
struct Export {
    #[serde(rename = "Sources")]
    sources: Vec<ExportSource>,
}

#[derive(Deserialize)]
struct ExportSource {
    #[serde(rename = "Packages")]
    packages: Vec<ExportPackage>,
}

#[derive(Deserialize)]
struct ExportPackage {
    #[serde(rename = "PackageIdentifier")]
    id: String,
    #[serde(rename = "Version", default)]
    version: String,
}

pub fn installed(cancel: &CancellationToken) -> Result<BTreeMap<String, String>> {
    let temporary = tempfile::tempdir()?;
    let file = temporary.path().join("packages.json");
    let result = capture(
        Command::new(executable()?)
            .args(["export", "--output"])
            .arg(&file)
            .args([
                "--include-versions",
                "--source",
                "winget",
                "--accept-source-agreements",
                "--disable-interactivity",
            ]),
        Duration::from_secs(45),
        cancel,
    )?;
    ensure!(
        result.status.success(),
        "WinGet не смог сопоставить установленные пакеты"
    );
    ensure!(
        std::fs::metadata(&file)?.len() <= 8 * 1024 * 1024,
        "Список WinGet слишком большой"
    );
    parse_export(&std::fs::read(file)?)
}

pub fn parse_export(bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    let export: Export =
        serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes))
            .context("Некорректный JSON WinGet")?;
    let mut result = BTreeMap::new();
    for source in export.sources {
        for package in source.packages {
            if valid_tool_id(&package.id) {
                result.insert(package.id.to_ascii_lowercase(), package.version);
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_structured_inventory_instead_of_localized_tables() {
        let bytes = br#"{"Sources":[{"Packages":[{"PackageIdentifier":"Google.Chrome","Version":"145.0"},{"PackageIdentifier":"Happ.Happ","Version":"< 3.3.6"}]}]}"#;
        let packages = parse_export(bytes).unwrap();
        assert_eq!(packages["google.chrome"], "145.0");
        assert_eq!(packages["happ.happ"], "< 3.3.6");
    }

    #[test]
    fn uses_exact_id_and_never_force_flags() {
        let args =
            install_arguments("Microsoft.VCRedist.2015+.x64", WingetRepository::Winget).unwrap();
        assert!(args.iter().any(|a| a == "--exact"));
        assert!(
            !args
                .iter()
                .any(|a| a == "--force" || a == "--ignore-security-hash")
        );
        assert!(install_arguments("--override=bad", WingetRepository::Winget).is_err());
        let store = install_arguments("9PB7GBMCR7N0", WingetRepository::Msstore).unwrap();
        assert!(store.windows(2).any(|pair| pair == ["--source", "msstore"]));
    }
}
