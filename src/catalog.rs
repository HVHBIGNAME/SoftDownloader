use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

use crate::paths::{safe_relative_path, validate_id};

pub const DEMO_CATALOG: &str = include_str!("../catalog/demo.json");
pub const BUILTIN_CATALOG: &str = include_str!("../catalog/builtin.json");
pub const EXTENDED_CATALOG: &str = include_str!("../catalog/extended.json");
pub const MAX_ARTIFACT_BYTES: u64 = 1024 * 1024 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub schema_version: u32,
    pub title: String,
    #[serde(default)]
    pub updated_at: Option<String>,
    pub categories: Vec<Category>,
    pub packages: Vec<Package>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Category {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub parent: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    #[default]
    App,
    Addon,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub version: String,
    pub publisher: String,
    pub description: String,
    pub category: String,
    #[serde(default)]
    pub kind: PackageKind,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
    #[serde(default)]
    pub artifact: Option<Artifact>,
    #[serde(default)]
    pub install: Option<InstallSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<crate::discovery::Source>,
    #[serde(default)]
    pub detect: crate::inventory::Detection,
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub file_name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drive_file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum InstallSpec {
    Exe {
        silent_args: Vec<String>,
        #[serde(default)]
        requires_admin: bool,
    },
    Msi {
        #[serde(default)]
        arguments: Vec<String>,
        #[serde(default = "enabled_by_default")]
        requires_admin: bool,
    },
    Zip {
        destination: ArchiveDestination,
        #[serde(default)]
        strip_components: u8,
    },
    Portable {
        destination: ArchiveDestination,
    },
    Interactive {
        #[serde(default)]
        requires_admin: bool,
    },
    Winget,
    VscodeExtension {
        extension_id: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveDestination {
    pub root: ArchiveRoot,
    pub path: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveRoot {
    RoamingAppData,
    LocalAppData,
    Documents,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CatalogDocument {
    pub catalog: Catalog,
    pub local_root: Option<PathBuf>,
    pub is_demo: bool,
    #[serde(default)]
    pub diagnostics: BTreeMap<String, String>,
}

impl CatalogDocument {
    pub fn demo() -> Result<Self> {
        Ok(Self {
            catalog: Catalog::parse(DEMO_CATALOG.as_bytes())?,
            local_root: None,
            is_demo: true,
            diagnostics: BTreeMap::new(),
        })
    }

    pub fn builtin() -> Result<Self> {
        let mut catalog = Catalog::parse(BUILTIN_CATALOG.as_bytes())?;
        let extended: Catalog = serde_json::from_str(EXTENDED_CATALOG)?;
        catalog.merge(extended)?;
        Ok(Self {
            catalog,
            local_root: None,
            is_demo: false,
            diagnostics: BTreeMap::new(),
        })
    }
}

impl Catalog {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
        let catalog: Self = serde_json::from_slice(bytes).context("Некорректный JSON-каталог")?;
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn parse_with_builtin(bytes: &[u8]) -> Result<Self> {
        let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
        let additional: Self =
            serde_json::from_slice(bytes).context("Некорректный JSON-каталог")?;
        ensure!(
            additional.schema_version == 1,
            "Версия каталога не поддерживается"
        );
        let mut base = CatalogDocument::builtin()?.catalog;
        base.merge(additional)?;
        Ok(base)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "Версия каталога {} не поддерживается",
            self.schema_version
        );
        ensure!(!self.title.trim().is_empty(), "Укажите название каталога");
        ensure!(
            self.packages.len() <= 5_000 && self.categories.len() <= 200,
            "Каталог слишком большой"
        );
        let mut categories = HashMap::new();
        for category in &self.categories {
            validate_id(&category.id)?;
            ensure!(
                !category.name.trim().is_empty(),
                "Пустое название группы {}",
                category.id
            );
            ensure!(
                categories.insert(category.id.as_str(), category).is_none(),
                "Повтор группы {}",
                category.id
            );
        }
        for category in &self.categories {
            let mut seen = HashSet::new();
            let mut current = Some(category.id.as_str());
            while let Some(id) = current {
                ensure!(seen.insert(id), "Цикл в группах: {id}");
                let item = categories
                    .get(id)
                    .with_context(|| format!("Группа {id} не найдена"))?;
                current = item.parent.as_deref();
            }
        }
        let mut package_ids = HashSet::new();
        for package in &self.packages {
            ensure!(
                package_ids.insert(&package.id),
                "Повтор пакета {}",
                package.id
            );
            ensure!(
                categories.contains_key(package.category.as_str()),
                "Неизвестная группа {} у {}",
                package.category,
                package.id
            );
            package
                .validate()
                .with_context(|| format!("Пакет {}", package.id))?;
        }
        let all: BTreeSet<String> = self.packages.iter().map(|p| p.id.clone()).collect();
        self.dependency_order(&all)?;
        Ok(())
    }

    pub fn package(&self, id: &str) -> Option<&Package> {
        self.packages.iter().find(|p| p.id == id)
    }

    pub fn merge(&mut self, additional: Self) -> Result<()> {
        ensure!(
            additional.schema_version == 1,
            "Версия дополнительного каталога не поддерживается"
        );
        let mut unique = HashSet::new();
        for category in &additional.categories {
            ensure!(unique.insert(&category.id), "Повтор группы {}", category.id);
        }
        unique.clear();
        for package in &additional.packages {
            ensure!(unique.insert(&package.id), "Повтор пакета {}", package.id);
        }
        self.title = additional.title;
        self.updated_at = additional.updated_at;
        for category in additional.categories {
            if let Some(existing) = self.categories.iter_mut().find(|c| c.id == category.id) {
                *existing = category;
            } else {
                self.categories.push(category);
            }
        }
        for package in additional.packages {
            if let Some(existing) = self.packages.iter_mut().find(|p| p.id == package.id) {
                *existing = package;
            } else {
                self.packages.push(package);
            }
        }
        self.validate()
    }

    pub fn category_contains(&self, parent: &str, child: &str) -> bool {
        let mut current = Some(child);
        while let Some(id) = current {
            if id == parent {
                return true;
            }
            current = self
                .categories
                .iter()
                .find(|c| c.id == id)
                .and_then(|c| c.parent.as_deref());
        }
        false
    }

    pub fn category_path(&self, id: &str) -> String {
        let mut parts = Vec::new();
        let mut current = Some(id);
        while let Some(id) = current {
            let Some(category) = self.categories.iter().find(|c| c.id == id) else {
                break;
            };
            parts.push(category.name.as_str());
            current = category.parent.as_deref();
        }
        parts.reverse();
        parts.join(" / ")
    }

    pub fn category_tree(&self) -> Vec<(&Category, usize)> {
        fn append<'a>(
            categories: &'a [Category],
            parent: Option<&str>,
            depth: usize,
            result: &mut Vec<(&'a Category, usize)>,
        ) {
            for category in categories.iter().filter(|c| c.parent.as_deref() == parent) {
                result.push((category, depth));
                append(categories, Some(&category.id), depth + 1, result);
            }
        }
        let mut result = Vec::with_capacity(self.categories.len());
        append(&self.categories, None, 0, &mut result);
        result
    }

    pub fn dependency_order(&self, selected: &BTreeSet<String>) -> Result<Vec<&Package>> {
        let index: HashMap<&str, &Package> =
            self.packages.iter().map(|p| (p.id.as_str(), p)).collect();
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        for id in selected {
            visit(id, &index, &mut visiting, &mut visited, &mut result)?;
        }
        Ok(result)
    }
}

fn visit<'a>(
    id: &str,
    index: &HashMap<&str, &'a Package>,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
    result: &mut Vec<&'a Package>,
) -> Result<()> {
    if visited.contains(id) {
        return Ok(());
    }
    ensure!(visiting.len() < 64, "Зависимости вложены слишком глубоко");
    ensure!(visiting.insert(id.to_owned()), "Цикл зависимостей: {id}");
    let package = index
        .get(id)
        .with_context(|| format!("Зависимость {id} отсутствует в каталоге"))?;
    for dependency in &package.depends_on {
        visit(dependency, index, visiting, visited, result)?;
    }
    visiting.remove(id);
    visited.insert(id.to_owned());
    result.push(package);
    Ok(())
}

impl Package {
    fn validate(&self) -> Result<()> {
        validate_id(&self.id)?;
        ensure!(
            !self.name.trim().is_empty() && !self.version.trim().is_empty(),
            "Нужны название и версия"
        );
        if let Some(homepage) = &self.homepage {
            validate_https_url(homepage)?;
        }
        if self.enabled && !self.is_manual() {
            ensure!(
                (self.artifact.is_some() || self.source.is_some() || self.is_managed())
                    && self.install.is_some(),
                "Активному пакету нужны artifact/source и install"
            );
        }
        self.detect.validate()?;
        ensure!(
            matches!(self.install, Some(InstallSpec::Winget)) == self.winget_id().is_some(),
            "source.winget и install.winget должны использоваться вместе"
        );
        ensure!(
            !matches!(self.install, Some(InstallSpec::VscodeExtension { .. }))
                || self.source.is_none(),
            "Расширение VS Code использует Marketplace без дополнительного source"
        );
        ensure!(
            !self.is_managed() || self.artifact.is_none(),
            "Управляемому пакету не нужен artifact"
        );
        ensure!(
            !self.is_manual() || (self.install.is_none() && self.artifact.is_none()),
            "Для ручного источника укажите только ссылку и инструкцию"
        );
        if let Some(artifact) = &self.artifact {
            artifact.validate(self.requires_signature())?;
        }
        if let Some(source) = &self.source {
            source.validate()?;
        }
        ensure!(
            !self.requires_signature() || !matches!(self.install, Some(InstallSpec::Zip { .. })),
            "Источник без опубликованного SHA-256 поддерживает только подписанные EXE/MSI"
        );
        if let Some(install) = &self.install {
            install.validate()?;
            if let Some(artifact) = &self.artifact {
                ensure!(
                    artifact
                        .file_name
                        .to_ascii_lowercase()
                        .ends_with(install.extension()),
                    "Расширение установщика не соответствует install.type"
                );
            }
        }
        let mut unique = HashSet::new();
        for id in &self.depends_on {
            validate_id(id)?;
            ensure!(unique.insert(id), "Повтор зависимости {id}");
        }
        Ok(())
    }

    pub fn matches_search(&self, query: &str) -> bool {
        let haystack = format!(
            "{} {} {} {} {}",
            self.name,
            self.publisher,
            self.description,
            self.id,
            self.tags.join(" ")
        )
        .to_lowercase();
        query
            .to_lowercase()
            .split_whitespace()
            .all(|term| haystack.contains(term))
    }

    pub fn ready(&self) -> bool {
        self.enabled
            && !self.is_manual()
            && self.install.is_some()
            && (self.artifact.is_some() || self.is_managed())
    }

    pub fn is_managed(&self) -> bool {
        matches!(
            self.install,
            Some(InstallSpec::Winget | InstallSpec::VscodeExtension { .. })
        )
    }
    pub fn is_manual(&self) -> bool {
        matches!(self.source, Some(crate::discovery::Source::Manual { .. }))
    }
    pub fn winget_id(&self) -> Option<&str> {
        match &self.source {
            Some(crate::discovery::Source::Winget { package_id, .. }) => Some(package_id),
            _ => None,
        }
    }

    pub fn requires_signature(&self) -> bool {
        matches!(self.source, Some(crate::discovery::Source::Website { .. }))
    }

    pub fn winget_repository(&self) -> crate::discovery::WingetRepository {
        match &self.source {
            Some(crate::discovery::Source::Winget { repository, .. }) => *repository,
            _ => crate::discovery::WingetRepository::default(),
        }
    }
}

impl Artifact {
    fn validate(&self, allow_signed_download: bool) -> Result<()> {
        let file = safe_relative_path(&self.file_name)?;
        ensure!(
            file.components().count() == 1,
            "file_name должен быть именем файла"
        );
        ensure!(
            self.size <= MAX_ARTIFACT_BYTES && (self.size > 0 || allow_signed_download),
            "Размер установщика должен быть от 1 байта до 1 ТБ; 0 разрешён для официального сайта"
        );
        ensure!(
            (allow_signed_download && self.sha256.is_empty())
                || (self.sha256.len() == 64
                    && self
                        .sha256
                        .bytes()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())),
            "sha256 должен содержать 64 строчных hex-символа"
        );
        ensure!(
            self.local_path.is_some() || self.drive_file_id.is_some() || self.url.is_some(),
            "Не указан источник установщика"
        );
        ensure!(
            !(self.drive_file_id.is_some() && self.url.is_some()),
            "Укажите drive_file_id или url, но не оба"
        );
        if let Some(path) = &self.local_path {
            safe_relative_path(path)?;
        }
        if let Some(id) = &self.drive_file_id {
            validate_drive_id(id)?;
        }
        if let Some(url) = &self.url {
            validate_https_url(url)?;
        }
        Ok(())
    }
}

impl InstallSpec {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Exe { .. } | Self::Interactive { .. } | Self::Portable { .. } => ".exe",
            Self::Msi { .. } => ".msi",
            Self::Zip { .. } => ".zip",
            Self::Winget | Self::VscodeExtension { .. } => "",
        }
    }

    pub fn requires_admin(&self) -> bool {
        match self {
            Self::Exe { requires_admin, .. }
            | Self::Msi { requires_admin, .. }
            | Self::Interactive { requires_admin } => *requires_admin,
            _ => false,
        }
    }

    fn validate(&self) -> Result<()> {
        match self {
            Self::Exe { silent_args, .. } => {
                ensure!(
                    !silent_args.is_empty(),
                    "Укажите параметры тихой установки EXE"
                );
                validate_arguments(silent_args)?;
            }
            Self::Msi { arguments, .. } => validate_arguments(arguments)?,
            Self::Portable { destination } => {
                safe_relative_path(&destination.path)?;
            }
            Self::Interactive { .. } | Self::Winget => {}
            Self::VscodeExtension { extension_id } => {
                ensure!(
                    crate::system::vscode::valid_extension_id(extension_id),
                    "Некорректный ID расширения VS Code"
                );
            }
            Self::Zip {
                destination,
                strip_components,
            } => {
                safe_relative_path(&destination.path)?;
                ensure!(
                    *strip_components <= 8,
                    "strip_components должен быть от 0 до 8"
                );
            }
        }
        Ok(())
    }
}

fn validate_arguments(arguments: &[String]) -> Result<()> {
    ensure!(
        arguments.iter().all(|a| !a.contains('\0')),
        "Параметры содержат NUL"
    );
    Ok(())
}

pub fn validate_drive_id(value: &str) -> Result<()> {
    ensure!(
        (10..=200).contains(&value.len())
            && value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "Некорректный ID файла Google Drive"
    );
    Ok(())
}

pub fn validate_https_url(value: &str) -> Result<url::Url> {
    let url = url::Url::parse(value).context("Некорректный URL")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("Нужен HTTPS-адрес без логина и пароля");
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_is_valid_and_explicitly_non_installable() {
        let document = CatalogDocument::demo().unwrap();
        assert!(document.catalog.packages.len() >= 8);
        assert!(document.catalog.packages.iter().all(|p| !p.enabled));
    }

    #[test]
    fn rejects_dependency_cycles_and_unknown_categories() {
        let mut catalog = CatalogDocument::demo().unwrap().catalog;
        catalog.packages[0].depends_on = vec![catalog.packages[1].id.clone()];
        catalog.packages[1].depends_on = vec![catalog.packages[0].id.clone()];
        assert!(catalog.validate().unwrap_err().to_string().contains("Цикл"));
        catalog.packages[0].depends_on.clear();
        catalog.packages[1].depends_on.clear();
        catalog.packages[0].category = "missing".into();
        assert!(catalog.validate().is_err());
    }

    #[test]
    fn search_is_case_insensitive_and_requires_every_word() {
        let catalog = CatalogDocument::demo().unwrap().catalog;
        let blender = catalog.package("blender").unwrap();
        assert!(blender.matches_search("BLENDER 3D"));
        assert!(!blender.matches_search("blender nonsense"));
    }
}
