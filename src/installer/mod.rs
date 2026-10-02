pub(crate) mod archive;
mod signature;
#[cfg(windows)]
pub(crate) mod windows;

pub use signature::verify_signature;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use tokio_util::sync::CancellationToken;

use crate::catalog::{InstallSpec, Package};

#[derive(Clone, Copy, Debug, Default)]
pub struct InstallOutcome {
    pub reboot_required: bool,
}

pub async fn install(
    package: &Package,
    path: PathBuf,
    logs: &Path,
    cancel: &CancellationToken,
) -> Result<InstallOutcome> {
    ensure!(!cancel.is_cancelled(), "Отменено");
    let spec = package
        .install
        .clone()
        .context("Не указан способ установки")?;
    let id = package.id.clone();
    let logs = logs.to_owned();
    let cancel = cancel.clone();
    tokio::task::spawn_blocking(move || match spec {
        InstallSpec::Zip {
            destination,
            strip_components,
        } => {
            archive::install(&path, &id, &destination, strip_components, &cancel)?;
            Ok(InstallOutcome::default())
        }
        spec => install_native(&path, &id, &spec, &logs),
    })
    .await
    .context("Поток установки завершился неожиданно")?
}

#[cfg(windows)]
fn install_native(
    path: &Path,
    id: &str,
    spec: &InstallSpec,
    logs: &Path,
) -> Result<InstallOutcome> {
    windows::install(path, id, spec, logs)
}

#[cfg(not(windows))]
fn install_native(
    _path: &Path,
    _id: &str,
    _spec: &InstallSpec,
    _logs: &Path,
) -> Result<InstallOutcome> {
    anyhow::bail!("EXE/MSI-установка поддерживается только в Windows")
}
