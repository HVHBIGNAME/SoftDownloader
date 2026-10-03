use std::future::Future;
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::catalog::CatalogDocument;
use crate::storage::Store;

const MAX_AGE: Duration = Duration::from_secs(60 * 60);
const MAX_CACHE_BYTES: usize = 16 * 1024 * 1024;

pub struct CatalogLoad {
    pub document: CatalogDocument,
    pub cached: bool,
    pub age_seconds: u64,
    pub warnings: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CacheEntry {
    version: u32,
    app_version: String,
    source_revision: String,
    stored_at: u64,
    document: CatalogDocument,
}

struct Cache {
    path: PathBuf,
    source_revision: String,
}

impl Cache {
    fn new(store: &Store, source: &str) -> Result<Self> {
        let revision = if source.is_empty() || source.starts_with("https://") {
            source.to_owned()
        } else {
            let mut path = PathBuf::from(source);
            if path.is_dir() {
                path.push("catalog.json");
            }
            let metadata = path.metadata()?;
            format!(
                "{}:{}:{:?}",
                path.canonicalize()?.display(),
                metadata.len(),
                metadata.modified()?
            )
        };
        let key = hex::encode(Sha256::digest(format!(
            "{}:{source}",
            env!("CARGO_PKG_VERSION")
        )));
        Ok(Self {
            path: store.cache_directory().join(format!("{key}.json")),
            source_revision: revision,
        })
    }

    fn read(&self) -> Result<Option<CacheEntry>> {
        let file = match std::fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::new();
        file.take(MAX_CACHE_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= MAX_CACHE_BYTES,
            "Кэш каталога превышает 16 МБ"
        );
        let entry: CacheEntry = serde_json::from_slice(&bytes)?;
        if entry.version != 1
            || entry.app_version != env!("CARGO_PKG_VERSION")
            || entry.source_revision != self.source_revision
        {
            return Ok(None);
        }
        entry.document.catalog.validate()?;
        ensure!(
            entry.stored_at <= now(),
            "Некорректное время сохранения кэша"
        );
        Ok(Some(entry))
    }

    fn write(&self, document: &CatalogDocument) -> Result<()> {
        let entry = CacheEntry {
            version: 1,
            app_version: env!("CARGO_PKG_VERSION").into(),
            source_revision: self.source_revision.clone(),
            stored_at: now(),
            document: document.clone(),
        };
        ensure!(
            serde_json::to_vec(&entry)?.len() <= MAX_CACHE_BYTES,
            "Кэш каталога превышает 16 МБ"
        );
        std::fs::create_dir_all(self.path.parent().context("Нет папки кэша")?)?;
        crate::storage::write_json(&self.path, &entry)
    }
}

pub async fn load(
    client: &reqwest::Client,
    store: &Store,
    source: &str,
    force: bool,
) -> Result<CatalogLoad> {
    let mut result = load_with(store, source.trim(), force, || {
        crate::network::resolve_catalog(client, source)
    })
    .await?;
    crate::discovery::apply_manager_availability(&mut result.document);
    Ok(result)
}

async fn load_with<F, Fut>(
    store: &Store,
    source: &str,
    force: bool,
    fetch: F,
) -> Result<CatalogLoad>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<CatalogDocument>>,
{
    let cache = Cache::new(store, source)?;
    let mut warnings = Vec::new();
    let cached = match cache.read() {
        Ok(cached) => cached,
        Err(error) => {
            warnings.push(format!("Кэш не прочитан, запрошено обновление: {error:#}"));
            None
        }
    };
    if let Some(entry) = &cached {
        let age = now().saturating_sub(entry.stored_at);
        if !force && age < MAX_AGE.as_secs() {
            return Ok(CatalogLoad {
                document: entry.document.clone(),
                cached: true,
                age_seconds: age,
                warnings,
            });
        }
    }
    let mut document = match fetch().await {
        Ok(document) => document,
        Err(error) => {
            let Some(entry) = cached else {
                return Err(error);
            };
            warnings.push(format!(
                "Обновление не удалось; показан сохранённый каталог: {error:#}"
            ));
            return Ok(CatalogLoad {
                document: entry.document,
                cached: true,
                age_seconds: now().saturating_sub(entry.stored_at),
                warnings,
            });
        }
    };
    if let Some(entry) = cached {
        reuse_successful_sources(&mut document, &entry.document);
    }
    document.catalog.validate()?;
    if let Err(error) = cache.write(&document) {
        warnings.push(format!("Не удалось сохранить кэш каталога: {error:#}"));
    }
    Ok(CatalogLoad {
        document,
        cached: false,
        age_seconds: 0,
        warnings,
    })
}

fn reuse_successful_sources(document: &mut CatalogDocument, previous: &CatalogDocument) {
    for package in &mut document.catalog.packages {
        if !document.diagnostics.contains_key(&package.id) || package.artifact.is_some() {
            continue;
        }
        if let Some(old) = previous.catalog.package(&package.id)
            && old.source == package.source
            && old.artifact.is_some()
        {
            package.artifact = old.artifact.clone();
            package.version = old.version.clone();
            if let Some(message) = document.diagnostics.get_mut(&package.id) {
                message.push_str(" · используется сохранённая версия");
            }
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests;
