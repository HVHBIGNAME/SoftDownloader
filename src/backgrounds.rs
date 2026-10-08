use std::path::PathBuf;

use anyhow::{Result, ensure};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::{catalog::Artifact, storage::Store, transfer};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

pub fn presets() -> Result<Vec<VideoPreset>> {
    let presets: Vec<VideoPreset> =
        serde_json::from_slice(include_bytes!("../catalog/backgrounds.json"))?;
    for preset in &presets {
        preset.validate()?;
    }
    Ok(presets)
}

impl VideoPreset {
    fn validate(&self) -> Result<()> {
        crate::paths::validate_id(&self.id)?;
        ensure!(
            self.size > 0 && self.size <= 32 * 1024 * 1024,
            "Слишком большой встроенный видеофон"
        );
        ensure!(
            self.sha256.len() == 64 && self.sha256.bytes().all(|c| c.is_ascii_hexdigit()),
            "Некорректный хеш видеофона"
        );
        crate::catalog::validate_https_url(&self.url)?;
        crate::catalog::validate_https_url(&self.source)?;
        Ok(())
    }
}

pub async fn download(
    client: &reqwest::Client,
    preset: &VideoPreset,
    store: &Store,
    cancel: &CancellationToken,
    report: impl Fn(transfer::Progress),
) -> Result<PathBuf> {
    preset.validate()?;
    let folder = store.root.join("backgrounds");
    tokio::fs::create_dir_all(&folder).await?;
    let path = folder.join(format!("{}-{}.mp4", preset.id, &preset.sha256[..12]));
    let artifact = Artifact {
        file_name: format!("{}.mp4", preset.id),
        size: preset.size,
        sha256: preset.sha256.clone(),
        local_path: None,
        drive_file_id: None,
        url: Some(preset.url.clone()),
    };
    transfer::acquire_asset(client, &artifact, &path, cancel, report).await?;
    Ok(tokio::fs::canonicalize(path).await?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn stock_backgrounds_have_pinned_hashes_and_source_attribution() {
        let presets = super::presets().unwrap();
        assert_eq!(presets.len(), 4);
        let ids: std::collections::BTreeSet<_> = presets.iter().map(|preset| &preset.id).collect();
        assert_eq!(ids.len(), presets.len());
    }
}
