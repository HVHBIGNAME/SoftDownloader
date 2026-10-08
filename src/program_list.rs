use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use crate::catalog::{Catalog, Package};
use crate::storage::Library;
use crate::uninstall::InstalledProgram;

const FORMAT: &str = "softdownloader.program-list";
pub const MAX_BYTES: usize = 2 * 1024 * 1024;
const MAX_PROGRAMS: usize = 5_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramList {
    pub format: String,
    pub schema_version: u32,
    pub created_at: u64,
    pub app_version: String,
    pub programs: Vec<ProgramEntry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramEntry {
    #[serde(default)]
    pub package_ids: Vec<String>,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub publisher: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchStatus {
    Available,
    Installed,
    Unavailable,
    Unknown,
    Ambiguous,
}

#[derive(Clone, Debug)]
pub struct MatchedProgram {
    pub entry: ProgramEntry,
    pub package_id: Option<String>,
    pub status: MatchStatus,
}

impl ProgramList {
    pub fn from_inventory(programs: &[InstalledProgram]) -> Self {
        let entries: Vec<_> = programs
            .iter()
            .map(|program| ProgramEntry {
                package_ids: program
                    .package_ids
                    .iter()
                    .chain(&program.managed_ids)
                    .cloned()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                name: single_line(&program.name),
                version: single_line(&program.version),
                publisher: single_line(&program.publisher),
            })
            .collect();
        Self::from_entries(entries)
    }

    pub fn from_packages<'a>(packages: impl IntoIterator<Item = &'a Package>) -> Self {
        let mut seen = BTreeSet::new();
        let entries = packages
            .into_iter()
            .filter(|package| seen.insert(package.id.clone()))
            .map(|package| ProgramEntry {
                package_ids: vec![package.id.clone()],
                name: single_line(&package.name),
                version: single_line(&package.version),
                publisher: single_line(&package.publisher),
            })
            .collect();
        Self::from_entries(entries)
    }

    fn from_entries(mut entries: Vec<ProgramEntry>) -> Self {
        entries.sort_by_cached_key(|entry| entry.name.to_lowercase());
        Self {
            format: FORMAT.into(),
            schema_version: 1,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            app_version: env!("CARGO_PKG_VERSION").into(),
            programs: entries,
        }
    }

    pub fn read(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)
            .with_context(|| format!("Не удалось открыть {}", path.display()))?;
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
        Self::parse(&bytes)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= MAX_BYTES, "Список программ превышает 2 МБ");
        let list: Self =
            serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes))
                .context("Нужен JSON-список, экспортированный из SoftDownloader")?;
        list.validate()?;
        Ok(list)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        self.validate()?;
        ensure!(
            serde_json::to_vec_pretty(self)?.len() < MAX_BYTES,
            "Список программ превышает 2 МБ"
        );
        crate::storage::write_json(path, self)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.format == FORMAT,
            "Этот файл не является списком программ SoftDownloader"
        );
        ensure!(
            self.schema_version == 1,
            "Версия списка не поддерживается; обновите SoftDownloader"
        );
        ensure!(
            self.programs.len() <= MAX_PROGRAMS,
            "В списке больше 5000 программ"
        );
        validate_text(&self.app_version, 80)?;
        for entry in &self.programs {
            ensure!(
                !entry.name.trim().is_empty(),
                "В списке есть программа без названия"
            );
            validate_text(&entry.name, 2048).context("Название программы в списке")?;
            validate_text(&entry.publisher, 2048)
                .with_context(|| format!("Издатель «{}»", entry.name))?;
            validate_text(&entry.version, 256)
                .with_context(|| format!("Версия «{}»", entry.name))?;
            ensure!(
                entry.package_ids.len() <= 64,
                "Слишком много ID у одной программы"
            );
            for id in &entry.package_ids {
                crate::paths::validate_id(id)?;
            }
        }
        Ok(())
    }

    pub fn match_catalog(&self, catalog: &Catalog, installed: &Library) -> Vec<MatchedProgram> {
        let matchers: Vec<_> = catalog
            .packages
            .iter()
            .map(|package| (package, crate::inventory::NameMatcher::new(package)))
            .collect();
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        for entry in &self.programs {
            if !entry.package_ids.is_empty() {
                for id in &entry.package_ids {
                    if seen.insert(format!("id:{id}")) {
                        result.push(match_package(
                            entry,
                            Some(id),
                            catalog.package(id),
                            installed,
                        ));
                    }
                }
                continue;
            }
            let normalized = crate::inventory::normalize_name(&entry.name);
            let candidates: Vec<_> = matchers
                .iter()
                .filter(|(_, matcher)| {
                    matcher
                        .as_ref()
                        .is_ok_and(|matcher| matcher.matches_normalized(&entry.name, &normalized))
                })
                .map(|(package, _)| *package)
                .collect();
            if candidates.len() == 1 {
                let package = candidates[0];
                if seen.insert(format!("id:{}", package.id)) {
                    result.push(match_package(
                        entry,
                        Some(&package.id),
                        Some(package),
                        installed,
                    ));
                }
            } else if seen.insert(format!(
                "name:{}:{}",
                entry.name.to_lowercase(),
                entry.publisher.to_lowercase()
            )) {
                result.push(MatchedProgram {
                    entry: entry.clone(),
                    package_id: None,
                    status: if candidates.is_empty() {
                        MatchStatus::Unknown
                    } else {
                        MatchStatus::Ambiguous
                    },
                });
            }
        }
        result.sort_by_cached_key(|row| {
            (
                row.status != MatchStatus::Available,
                row.entry.name.to_lowercase(),
            )
        });
        result
    }
}

fn match_package(
    entry: &ProgramEntry,
    id: Option<&str>,
    package: Option<&Package>,
    installed: &Library,
) -> MatchedProgram {
    let status = match package {
        Some(package) if installed.contains_key(&package.id) => MatchStatus::Installed,
        Some(package) if package.ready() => MatchStatus::Available,
        Some(_) => MatchStatus::Unavailable,
        None => MatchStatus::Unknown,
    };
    MatchedProgram {
        entry: entry.clone(),
        package_id: id.map(str::to_owned),
        status,
    }
}

fn validate_text(value: &str, max: usize) -> Result<()> {
    ensure!(
        value.len() <= max && !value.chars().any(char::is_control),
        "Некорректный текст в списке программ"
    );
    Ok(())
}

fn single_line(value: &str) -> String {
    let words: Vec<_> = value
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|ch| !ch.is_control())
                .collect::<String>()
        })
        .filter(|word| !word.is_empty())
        .collect();
    words.join(" ")
}

#[cfg(test)]
mod tests;
