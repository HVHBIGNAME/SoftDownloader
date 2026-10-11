use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use semver::Version;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::catalog::Artifact;
use crate::storage::Store;
use crate::{network, transfer};

mod apply;
pub use apply::{Handoff, acknowledge, apply_update, launch};

pub const RELEASES_URL: &str = "https://github.com/HVHBIGNAME/SoftDownloader/releases/latest";
const API_URL: &str = "https://api.github.com/repos/HVHBIGNAME/SoftDownloader/releases/latest";
const ASSET_PREFIX: &str = "https://github.com/HVHBIGNAME/SoftDownloader/releases/download/";
const MAX_EXE_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct UpdateSettings {
    pub enabled: bool,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    size: u64,
    browser_download_url: String,
}

pub struct ReadyUpdate {
    pub version: String,
    pub(crate) artifact: Artifact,
    pub(crate) directory: tempfile::TempDir,
}

#[derive(Default, Deserialize, Serialize)]
pub struct UpdateResult {
    pub version: String,
    pub failed_sha256: Option<String>,
    pub message: String,
}

pub fn previous_result(store: &Store) -> Result<UpdateResult> {
    let path = store.root.join("updates/result.json");
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(UpdateResult::default()),
        Err(error) => Err(error.into()),
    }
}

fn asset<'a>(release: &'a Release, name: &str) -> Result<&'a Asset> {
    let matches: Vec<_> = release
        .assets
        .iter()
        .filter(|asset| asset.name == name)
        .collect();
    ensure!(matches.len() == 1, "В релизе должен быть ровно один {name}");
    let asset = matches[0];
    let expected = format!("{ASSET_PREFIX}{}/{name}", release.tag_name);
    ensure!(
        asset.browser_download_url == expected,
        "Файл обновления находится вне официального релиза"
    );
    Ok(asset)
}

fn newer_release(bytes: &[u8], current: &str) -> Result<Option<Release>> {
    let release: Release = serde_json::from_slice(bytes)?;
    let version = Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )?;
    if release.draft
        || release.prerelease
        || !version.pre.is_empty()
        || version <= Version::parse(current)?
    {
        return Ok(None);
    }
    ensure!(
        version.build.is_empty(),
        "Неожиданный формат версии обновления"
    );
    let executable = asset(&release, "SoftDownloader.exe")?;
    ensure!(
        (1024..=MAX_EXE_BYTES).contains(&executable.size),
        "Некорректный размер обновления"
    );
    ensure!(
        (1..=4096).contains(&asset(&release, "SoftDownloader.exe.sha256")?.size),
        "Некорректный размер контрольной суммы"
    );
    Ok(Some(release))
}

fn checksum(bytes: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(bytes)?;
    let fields: Vec<_> = text.split_whitespace().collect();
    ensure!(
        fields.len() == 2 && fields[1].trim_start_matches('*') == "SoftDownloader.exe",
        "Контрольная сумма относится к другому файлу"
    );
    ensure!(
        fields[0].len() == 64 && fields[0].bytes().all(|b| b.is_ascii_hexdigit()),
        "Некорректный SHA-256 обновления"
    );
    Ok(fields[0].to_ascii_lowercase())
}

pub async fn check_and_download(
    store: &Store,
    cancel: &CancellationToken,
    report: impl Fn(transfer::Progress),
) -> Result<Option<ReadyUpdate>> {
    tokio::select! {
        _ = cancel.cancelled() => anyhow::bail!("Проверка обновления отменена"),
        result = tokio::time::timeout(Duration::from_secs(300), download(store, cancel, report)) => {
            result.context("Истекло время загрузки обновления")?
        }
    }
}

async fn download(
    store: &Store,
    cancel: &CancellationToken,
    report: impl Fn(transfer::Progress),
) -> Result<Option<ReadyUpdate>> {
    let updates = store.root.join("updates");
    std::fs::create_dir_all(&updates)?;
    apply::cleanup_completed(&updates);
    let client = network::client()?;
    let response = client
        .get(API_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let bytes = network::read_limited(response.error_for_status()?, 1024 * 1024).await?;
    let Some(release) = newer_release(&bytes, env!("CARGO_PKG_VERSION"))? else {
        return Ok(None);
    };
    let hash_asset = asset(&release, "SoftDownloader.exe.sha256")?;
    let hash = checksum(
        &network::read_limited(
            network::open_url(&client, &hash_asset.browser_download_url).await?,
            4096,
        )
        .await?,
    )?;
    ensure!(
        previous_result(store)?.failed_sha256.as_deref() != Some(&hash),
        "Предыдущий запуск этого обновления завершился ошибкой. Ожидается исправленный релиз"
    );
    let executable = asset(&release, "SoftDownloader.exe")?;
    let artifact = Artifact {
        file_name: executable.name.clone(),
        size: executable.size,
        sha256: hash,
        url: Some(executable.browser_download_url.clone()),
        local_path: None,
        drive_file_id: None,
    };
    let directory = tempfile::Builder::new()
        .prefix("update-")
        .tempdir_in(updates)?;
    let path = directory.path().join("SoftDownloader.exe");
    transfer::acquire_asset(&client, &artifact, &path, cancel, report).await?;
    validate_executable(&path)?;
    Ok(Some(ReadyUpdate {
        version: release.tag_name.trim_start_matches('v').into(),
        artifact,
        directory,
    }))
}

fn validate_executable(path: &Path) -> Result<()> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let mut dos = [0; 64];
    file.read_exact(&mut dos)?;
    ensure!(&dos[..2] == b"MZ", "Обновление не является EXE");
    let offset = u32::from_le_bytes(dos[60..64].try_into()?);
    ensure!(
        (64..=1024 * 1024).contains(&offset),
        "Повреждён заголовок EXE"
    );
    file.seek(SeekFrom::Start(u64::from(offset)))?;
    let mut pe = [0; 26];
    file.read_exact(&mut pe)?;
    ensure!(
        &pe[..4] == b"PE\0\0" && pe[4..6] == [0x64, 0x86] && pe[24..26] == [0x0b, 0x02],
        "Обновление не является Windows x64 EXE"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
