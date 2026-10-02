use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, ensure};
use futures_util::TryStreamExt;
use reqwest::Client;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio_util::{io::StreamReader, sync::CancellationToken};

use crate::catalog::{Artifact, MAX_ARTIFACT_BYTES, Package};
use crate::network;
use crate::paths::resolve_local_file;

#[derive(Clone, Debug)]
pub struct Progress {
    pub downloaded: u64,
    pub total: u64,
    pub bytes_per_second: f64,
    pub verifying: bool,
}

pub struct AcquiredArtifact {
    pub path: PathBuf,
    pub sha256: String,
}

pub async fn acquire(
    client: &Client,
    package: &Package,
    local_root: Option<&Path>,
    cache: &Path,
    cancel: &CancellationToken,
    report: impl Fn(Progress),
) -> Result<AcquiredArtifact> {
    let artifact = package.artifact.as_ref().context("Нет установщика")?;
    let install = package.install.as_ref().context("Нет способа установки")?;
    let cache_key = if artifact.sha256.is_empty() {
        hex::encode(Sha256::digest(
            format!(
                "{}|{}",
                package.version,
                artifact.url.as_deref().unwrap_or_default()
            )
            .as_bytes(),
        ))
    } else {
        artifact.sha256.clone()
    };
    let destination = cache.join(format!(
        "{}-{}{}",
        package.id,
        cache_key,
        install.extension()
    ));
    if !artifact.sha256.is_empty() && tokio::fs::try_exists(&destination).await? {
        report(Progress {
            downloaded: 0,
            total: artifact.size,
            bytes_per_second: 0.0,
            verifying: true,
        });
        match verify_file(&destination, artifact, cancel).await {
            Ok(()) => {
                return Ok(AcquiredArtifact {
                    path: destination,
                    sha256: artifact.sha256.clone(),
                });
            }
            Err(error) if cancel.is_cancelled() => return Err(error),
            Err(_) => tokio::fs::remove_file(&destination).await?,
        }
    }
    ensure!(!cancel.is_cancelled(), "Отменено");
    report(Progress {
        downloaded: 0,
        total: artifact.size,
        bytes_per_second: 0.0,
        verifying: false,
    });
    let (reader, size) = tokio::select! {
        _ = cancel.cancelled() => anyhow::bail!("Отменено"),
        result = artifact_reader(client, artifact, local_root) => result?,
    };
    let mut expected = artifact.clone();
    if expected.size == 0 {
        expected.size = size;
    }
    let sha256 = copy_verified(reader, &expected, &destination, cancel, report).await?;
    Ok(AcquiredArtifact {
        path: destination,
        sha256,
    })
}

async fn artifact_reader(
    client: &Client,
    artifact: &Artifact,
    local_root: Option<&Path>,
) -> Result<(Box<dyn AsyncRead + Send + Unpin>, u64)> {
    if let (Some(root), Some(relative)) = (local_root, &artifact.local_path) {
        let path = resolve_local_file(root, relative)?;
        let file = tokio::fs::File::open(path).await?;
        let size = file.metadata().await?.len();
        ensure!(
            artifact.size == 0 || size == artifact.size,
            "Размер локального файла изменился. Пересчитайте каталог"
        );
        return Ok((Box::new(file), size));
    }
    let response = if let Some(id) = &artifact.drive_file_id {
        network::open_drive(client, id).await?
    } else if let Some(url) = &artifact.url {
        network::open_url(client, url).await?
    } else {
        anyhow::bail!(
            "Для этого установщика нет сетевой ссылки. Подключите локальную папку каталога"
        )
    };
    if let Some(size) = response.content_length() {
        ensure!(
            artifact.size == 0 || size == artifact.size,
            "Размер файла на сервере ({size}) не совпадает с каталогом ({})",
            artifact.size
        );
    }
    let size = response.content_length().unwrap_or_default();
    let stream = response.bytes_stream().map_err(std::io::Error::other);
    Ok((Box::new(StreamReader::new(stream)), size))
}

async fn copy_verified(
    mut reader: Box<dyn AsyncRead + Send + Unpin>,
    artifact: &Artifact,
    destination: &Path,
    cancel: &CancellationToken,
    report: impl Fn(Progress),
) -> Result<String> {
    let parent = destination.parent().context("Нет папки загрузок")?;
    let temporary = tempfile::Builder::new()
        .prefix(".download-")
        .suffix(".part")
        .tempfile_in(parent)?;
    let mut writer = tokio::fs::File::from_std(temporary.reopen()?);
    let mut buffer = vec![0_u8; 256 * 1024];
    let mut hash = Sha256::new();
    let mut downloaded = 0_u64;
    let start = Instant::now();
    let mut last_report = start;
    loop {
        let length = tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("Отменено"),
            result = reader.read(&mut buffer) => result?,
        };
        if length == 0 {
            break;
        }
        downloaded = downloaded
            .checked_add(length as u64)
            .context("Слишком большой файл")?;
        ensure!(
            downloaded <= MAX_ARTIFACT_BYTES && (artifact.size == 0 || downloaded <= artifact.size),
            "Файл больше размера из каталога"
        );
        hash.update(&buffer[..length]);
        writer.write_all(&buffer[..length]).await?;
        if last_report.elapsed() >= Duration::from_millis(120) {
            report(Progress {
                downloaded,
                total: artifact.size,
                bytes_per_second: downloaded as f64 / start.elapsed().as_secs_f64().max(0.001),
                verifying: false,
            });
            last_report = Instant::now();
        }
    }
    report(Progress {
        downloaded,
        total: artifact.size,
        bytes_per_second: 0.0,
        verifying: true,
    });
    ensure!(!cancel.is_cancelled(), "Отменено");
    ensure!(
        downloaded > 0 && (artifact.size == 0 || downloaded == artifact.size),
        "Файл скачан не полностью: {downloaded} из {} байт",
        artifact.size
    );
    let digest = hex::encode(hash.finalize());
    ensure!(
        artifact.sha256.is_empty() || digest == artifact.sha256,
        "SHA-256 не совпадает. Файл не будет запущен; обновите каталог или установщик"
    );
    writer.flush().await?;
    writer.sync_all().await?;
    drop(writer);
    temporary
        .persist(destination)
        .context("Не удалось сохранить проверенный установщик")?;
    Ok(digest)
}

pub async fn verify_file(
    path: &Path,
    artifact: &Artifact,
    cancel: &CancellationToken,
) -> Result<()> {
    let mut file = tokio::fs::File::open(path).await?;
    ensure!(
        file.metadata().await?.len() == artifact.size,
        "Неверный размер файла"
    );
    let mut hash = Sha256::new();
    let mut buffer = vec![0_u8; 256 * 1024];
    let mut total = 0_u64;
    loop {
        let length = tokio::select! {
            _ = cancel.cancelled() => anyhow::bail!("Отменено"),
            result = file.read(&mut buffer) => result?,
        };
        if length == 0 {
            break;
        }
        total += length as u64;
        ensure!(total <= artifact.size, "Файл изменился во время проверки");
        hash.update(&buffer[..length]);
    }
    ensure!(
        total == artifact.size && hex::encode(hash.finalize()) == artifact.sha256,
        "SHA-256 файла не совпадает"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(bytes: &[u8]) -> Artifact {
        Artifact {
            file_name: "setup.exe".into(),
            size: bytes.len() as u64,
            sha256: hex::encode(Sha256::digest(bytes)),
            local_path: Some("setup.exe".into()),
            drive_file_id: None,
            url: None,
        }
    }

    #[tokio::test]
    async fn publishes_only_complete_verified_files_and_cleans_partial_downloads() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("setup.exe");
        let bytes = b"a test installer fixture";
        let expected = artifact(bytes);
        copy_verified(
            Box::new(std::io::Cursor::new(bytes)),
            &expected,
            &destination,
            &CancellationToken::new(),
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(tokio::fs::read(&destination).await.unwrap(), bytes);
        tokio::fs::remove_file(&destination).await.unwrap();
        for bad_bytes in [
            b"short".as_slice(),
            b"a corrupt installer data".as_slice(),
            b"a much larger test installer fixture than expected".as_slice(),
        ] {
            assert!(
                copy_verified(
                    Box::new(std::io::Cursor::new(bad_bytes)),
                    &expected,
                    &destination,
                    &CancellationToken::new(),
                    |_| {}
                )
                .await
                .is_err()
            );
            assert!(!destination.exists());
            assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
        }
    }

    #[tokio::test]
    async fn cancellation_never_leaves_an_executable() {
        let directory = tempfile::tempdir().unwrap();
        let cancel = CancellationToken::new();
        cancel.cancel();
        let result = copy_verified(
            Box::new(std::io::Cursor::new(b"test")),
            &artifact(b"test"),
            &directory.path().join("setup.exe"),
            &cancel,
            |_| {},
        )
        .await;
        assert!(result.is_err());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }
}
