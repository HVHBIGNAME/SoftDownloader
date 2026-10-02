use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use super::{capture, run_logged, valid_tool_id};
use crate::installer::InstallOutcome;

#[derive(Clone, Debug, Deserialize)]
pub struct AppxPackage {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "PackageFullName")]
    pub full_name: String,
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "Publisher")]
    pub publisher: String,
}

#[cfg(windows)]
fn powershell() -> Result<PathBuf> {
    Ok(
        crate::installer::windows::system_directory()?
            .join("WindowsPowerShell/v1.0/powershell.exe"),
    )
}

#[cfg(not(windows))]
fn powershell() -> Result<PathBuf> {
    anyhow::bail!("Microsoft Store доступен только в Windows")
}

pub fn installed(cancel: &CancellationToken) -> Result<Vec<AppxPackage>> {
    let script = "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $ErrorActionPreference = 'Stop'; $items = @(Get-AppxPackage | Where-Object { -not $_.IsFramework -and -not $_.IsResourcePackage -and -not $_.NonRemovable -and $_.SignatureKind -ne 'System' } | ForEach-Object { [pscustomobject]@{Name=$_.Name; PackageFullName=$_.PackageFullName; Version=$_.Version.ToString(); Publisher=$_.Publisher} }); ConvertTo-Json -InputObject $items -Compress";
    let output = capture(
        Command::new(powershell()?).args(["-NoProfile", "-NonInteractive", "-Command", script]),
        Duration::from_secs(25),
        cancel,
    )?;
    ensure!(
        output.status.success(),
        "Не удалось прочитать приложения Microsoft Store"
    );
    parse_packages(&output.stdout)
}

pub fn parse_packages(bytes: &[u8]) -> Result<Vec<AppxPackage>> {
    let packages: Vec<AppxPackage> =
        serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes))
            .context("Некорректный список Microsoft Store")?;
    Ok(packages
        .into_iter()
        .filter(|p| valid_tool_id(&p.full_name))
        .collect())
}

pub fn remove(full_name: &str, log: &Path) -> Result<InstallOutcome> {
    ensure!(
        valid_tool_id(full_name),
        "Некорректный идентификатор Microsoft Store"
    );
    let script = format!(
        "$ErrorActionPreference = 'Stop'; Remove-AppxPackage -Package '{full_name}' -ErrorAction Stop"
    );
    let status = run_logged(
        Command::new(powershell()?).args(["-NoProfile", "-NonInteractive", "-Command", &script]),
        log,
    )?;
    ensure!(
        status.success(),
        "Не удалось удалить приложение Microsoft Store. Журнал: {}",
        log.display()
    );
    Ok(InstallOutcome::default())
}
