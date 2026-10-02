use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;

use anyhow::{Result, ensure};
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::catalog::{Catalog, InstallSpec, Package};
use crate::storage::{InstalledPackage, Library};
use crate::system::{appx, vscode, winget};
use crate::uninstall::{self, InstalledProgram, UninstallTarget};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Detection {
    pub names: Vec<String>,
    pub name_pattern: Option<String>,
    pub winget_ids: Vec<String>,
    pub paths: Vec<String>,
    pub commands: Vec<String>,
    pub appx_names: Vec<String>,
}

impl Detection {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [
                self.names.len(),
                self.winget_ids.len(),
                self.paths.len(),
                self.commands.len(),
                self.appx_names.len()
            ]
            .into_iter()
            .all(|count| count <= 64),
            "Слишком много правил обнаружения"
        );
        ensure!(
            self.names
                .iter()
                .chain(&self.appx_names)
                .all(|name| !name.trim().is_empty() && name.len() <= 256 && !name.contains('\0')),
            "Пустое или слишком длинное имя обнаружения"
        );
        if let Some(pattern) = &self.name_pattern {
            ensure!(
                pattern.len() <= 256 && pattern.starts_with('^') && pattern.ends_with('$'),
                "Шаблон обнаружения должен быть ограничен ^ и $"
            );
            Regex::new(pattern)?;
        }
        ensure!(
            self.winget_ids
                .iter()
                .all(|id| crate::system::valid_tool_id(id)),
            "Некорректный ID WinGet в detect"
        );
        for path in &self.paths {
            ensure!(
                !path.contains('\0')
                    && !path.starts_with(['\\', '/'])
                    && !path.split(['/', '\\']).any(|p| p == ".."),
                "Для обнаружения нужны локальные пути без .. и UNC"
            );
        }
        for command in &self.commands {
            ensure!(
                crate::system::valid_tool_id(command),
                "В detect.commands допускаются только имена файлов"
            );
        }
        Ok(())
    }
}

pub struct Inventory {
    pub programs: Vec<InstalledProgram>,
    pub warnings: Vec<String>,
    pub unverified: BTreeSet<String>,
}

impl Inventory {
    fn unavailable(
        &mut self,
        source: &str,
        error: anyhow::Error,
        library: &Library,
        target: impl Fn(&UninstallTarget) -> bool,
    ) {
        self.warnings.push(format!("{source}: {error:#}"));
        self.unverified.extend(
            library
                .values()
                .filter(|entry| entry.uninstall.as_ref().is_some_and(&target))
                .map(|entry| entry.id.clone()),
        );
    }
}

struct NameMatcher {
    names: HashSet<String>,
    pattern: Option<Regex>,
}

impl NameMatcher {
    fn new(package: &Package) -> Result<Self> {
        Ok(Self {
            names: std::iter::once(&package.name)
                .chain(&package.detect.names)
                .map(|name| normalize_name(name))
                .collect(),
            pattern: package
                .detect
                .name_pattern
                .as_deref()
                .map(Regex::new)
                .transpose()?,
        })
    }

    fn matches(&self, name: &str) -> bool {
        self.names.contains(&normalize_name(name))
            || self.pattern.as_ref().is_some_and(|regex| {
                regex
                    .find(name)
                    .is_some_and(|found| found.start() == 0 && found.end() == name.len())
            })
    }
}

pub fn matches_name(package: &Package, name: &str) -> bool {
    NameMatcher::new(package).is_ok_and(|matcher| matcher.matches(name))
}

pub fn normalize_name(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '+')
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn useful_version(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && !["unknown", "не определена", "последняя", "latest"]
            .iter()
            .any(|unknown| value.to_lowercase() == *unknown)
        && !value.starts_with(['<', '>', '=', '~'])
}

pub fn same_version(left: &str, right: &str) -> bool {
    fn normalize(value: &str) -> String {
        let value = value.trim().trim_start_matches(['v', 'V']);
        if value.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            let mut parts: Vec<_> = value.split('.').collect();
            while parts.len() > 1 && parts.last() == Some(&"0") {
                parts.pop();
            }
            parts.join(".")
        } else {
            value.to_ascii_lowercase()
        }
    }
    useful_version(left) && useful_version(right) && normalize(left) == normalize(right)
}

pub fn scan(catalog: &Catalog, library: &Library, cancel: &CancellationToken) -> Result<Inventory> {
    ensure!(!cancel.is_cancelled(), "Обнаружение программ отменено");
    let mut inventory = Inventory {
        programs: uninstall::scan(library)?,
        warnings: Vec::new(),
        unverified: BTreeSet::new(),
    };
    let (appx_result, winget_result) = std::thread::scope(|scope| {
        let winget_query = scope.spawn(|| winget::installed(cancel));
        let appx_result = appx::installed(cancel);
        let winget_result = winget_query
            .join()
            .map_err(|_| anyhow::anyhow!("Поток запроса WinGet завершился неожиданно"))
            .and_then(|result| result);
        (appx_result, winget_result)
    });
    match appx_result {
        Ok(packages) => {
            for item in packages {
                let matching: Vec<_> = catalog
                    .packages
                    .iter()
                    .filter(|p| {
                        p.detect
                            .appx_names
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(&item.name))
                    })
                    .collect();
                let target = UninstallTarget::Appx {
                    full_name: item.full_name,
                };
                inventory.programs.push(InstalledProgram {
                    id: target.id(),
                    name: matching
                        .first()
                        .map(|p| p.name.clone())
                        .unwrap_or(item.name),
                    version: item.version,
                    publisher: item.publisher,
                    quiet: true,
                    target,
                    managed_ids: Vec::new(),
                    package_ids: matching.iter().map(|p| p.id.clone()).collect(),
                    icon_path: matching
                        .first()
                        .and_then(|package| package_icon_file(package)),
                });
            }
        }
        Err(error) => inventory.unavailable("Microsoft Store", error, library, |target| {
            matches!(target, UninstallTarget::Appx { .. })
        }),
    }
    ensure!(!cancel.is_cancelled(), "Обнаружение программ отменено");
    bind_names(catalog, &mut inventory.programs)?;
    match winget_result {
        Ok(packages) => merge_winget(catalog, &mut inventory.programs, &packages),
        Err(error) => inventory.unavailable("WinGet", error, library, |target| {
            matches!(target, UninstallTarget::Winget { .. })
        }),
    }
    ensure!(!cancel.is_cancelled(), "Обнаружение программ отменено");
    match vscode::installed() {
        Ok(extensions) => merge_extensions(catalog, &mut inventory.programs, &extensions),
        Err(error) => {
            inventory.unavailable("Расширения VS Code", error, library, |target| {
                matches!(target, UninstallTarget::VscodeExtension { .. })
            })
        }
    }
    add_portables(catalog, &mut inventory.programs);
    inventory
        .programs
        .sort_by_cached_key(|p| p.name.to_lowercase());
    Ok(inventory)
}

fn bind_names(catalog: &Catalog, programs: &mut [InstalledProgram]) -> Result<()> {
    let version = Regex::new(r"(?i)(?:\bv|version\s*|версия\s*)(\d+(?:\.\d+)+)")
        .expect("valid version pattern");
    for program in programs.iter_mut() {
        if !useful_version(&program.version)
            && let Some(captures) = version.captures(&program.name)
        {
            program.version = captures[1].into();
        }
        if !useful_version(&program.version) {
            program.version = "Не определена".into();
        }
    }
    for package in &catalog.packages {
        let matcher = NameMatcher::new(package)?;
        for program in programs
            .iter_mut()
            .filter(|p| matches!(p.target, UninstallTarget::Registry { .. }))
        {
            if matcher.matches(&program.name) && !program.package_ids.contains(&package.id) {
                program.package_ids.push(package.id.clone());
            }
        }
    }
    Ok(())
}

pub fn merge_winget(
    catalog: &Catalog,
    programs: &mut Vec<InstalledProgram>,
    packages: &BTreeMap<String, String>,
) {
    for package in &catalog.packages {
        let ids = package
            .winget_id()
            .into_iter()
            .chain(package.detect.winget_ids.iter().map(String::as_str));
        let Some((id, version)) = ids
            .filter_map(|id| packages.get(&id.to_ascii_lowercase()).map(|v| (id, v)))
            .next()
        else {
            continue;
        };
        let target = UninstallTarget::Winget {
            package_id: id.into(),
        };
        if let Some(program) = programs
            .iter_mut()
            .find(|p| p.package_ids.contains(&package.id) || p.id == target.id())
        {
            if !program.package_ids.contains(&package.id) {
                program.package_ids.push(package.id.clone());
            }
            if !useful_version(&program.version) && useful_version(version) {
                program.version = version.clone();
            }
        } else {
            programs.push(InstalledProgram {
                id: target.id(),
                name: package.name.clone(),
                version: if useful_version(version) {
                    version.clone()
                } else {
                    "Не определена".into()
                },
                publisher: package.publisher.clone(),
                quiet: true,
                target,
                managed_ids: Vec::new(),
                package_ids: vec![package.id.clone()],
                icon_path: package_icon_file(package),
            });
        }
    }
}

fn merge_extensions(
    catalog: &Catalog,
    programs: &mut Vec<InstalledProgram>,
    extensions: &BTreeMap<String, String>,
) {
    for package in &catalog.packages {
        let Some(InstallSpec::VscodeExtension { extension_id }) = &package.install else {
            continue;
        };
        if let Some(version) = extensions.get(&extension_id.to_ascii_lowercase()) {
            let target = UninstallTarget::VscodeExtension {
                extension_id: extension_id.clone(),
            };
            if let Some(program) = programs.iter_mut().find(|p| p.id == target.id()) {
                if !program.package_ids.contains(&package.id) {
                    program.package_ids.push(package.id.clone());
                }
            } else {
                programs.push(InstalledProgram {
                    id: target.id(),
                    name: package.name.clone(),
                    version: version.clone(),
                    publisher: package.publisher.clone(),
                    quiet: true,
                    target,
                    managed_ids: Vec::new(),
                    package_ids: vec![package.id.clone()],
                    icon_path: package_icon_file(package),
                });
            }
        }
    }
}

fn add_portables(catalog: &Catalog, programs: &mut Vec<InstalledProgram>) {
    for package in &catalog.packages {
        if programs.iter().any(|p| p.package_ids.contains(&package.id)) {
            continue;
        }
        if let Some(path) = package_icon_file(package) {
            let target = UninstallTarget::Detected { path: path.clone() };
            programs.push(InstalledProgram {
                id: target.id(),
                name: package.name.clone(),
                version: "Не определена".into(),
                publisher: package.publisher.clone(),
                quiet: false,
                target,
                managed_ids: Vec::new(),
                package_ids: vec![package.id.clone()],
                icon_path: Some(path),
            });
        }
    }
}

fn real_file(path: &std::path::Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() > 0)
}

/// Finds the executable a catalog package can borrow an icon from.
///
/// Known install paths win over PATH lookups because they point at the real
/// program rather than a launcher shim.
pub fn package_icon_file(package: &Package) -> Option<PathBuf> {
    package
        .detect
        .paths
        .iter()
        .filter_map(|path| expand_path(path))
        .find(|path| real_file(path))
        .or_else(|| {
            package
                .detect
                .commands
                .iter()
                .find_map(|name| find_in_path(name))
        })
}

pub fn expand_path(template: &str) -> Option<PathBuf> {
    let variable = Regex::new(r"%([^%]+)%").ok()?;
    let mut failed = false;
    let value = variable.replace_all(
        template,
        |captures: &regex::Captures<'_>| match std::env::var(&captures[1]) {
            Ok(value) => value,
            Err(_) => {
                failed = true;
                String::new()
            }
        },
    );
    if failed
        || value.contains('%')
        || value.contains('\0')
        || value.split(['/', '\\']).any(|p| p == "..")
    {
        return None;
    }
    let bytes = value.as_bytes();
    if bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !b"/\\".contains(&bytes[2])
    {
        return None;
    }
    Some(PathBuf::from(value.as_ref()))
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .filter_map(|folder| expand_path(&folder.join(name).to_string_lossy()))
        .find(|path| real_file(path))
}

pub fn installation_state(
    catalog: &Catalog,
    library: &Library,
    programs: &[InstalledProgram],
) -> Library {
    let mut state = Library::new();
    for package in &catalog.packages {
        let previous = library.get(&package.id);
        let matches: Vec<_> = programs
            .iter()
            .filter(|p| p.package_ids.contains(&package.id) || p.managed_ids.contains(&package.id))
            .collect();
        let owned = previous
            .and_then(|entry| entry.uninstall.as_ref())
            .map(UninstallTarget::id);
        let program = matches
            .iter()
            .find(|p| Some(&p.id) == owned.as_ref())
            .or_else(|| matches.iter().find(|p| useful_version(&p.version)))
            .or_else(|| matches.first());
        if let Some(program) = program {
            let recorded = previous.filter(|entry| {
                Some(&program.id) == owned.as_ref()
                    && same_version(&entry.version, &program.version)
            });
            state.insert(
                package.id.clone(),
                InstalledPackage {
                    id: package.id.clone(),
                    name: program.name.clone(),
                    version: program.version.clone(),
                    sha256: recorded.map(|e| e.sha256.clone()).unwrap_or_default(),
                    installed_at: recorded.map(|e| e.installed_at).unwrap_or_default(),
                    reboot_required: recorded.is_some_and(|e| e.reboot_required),
                    uninstall: Some(program.target.clone()),
                    externally_detected: recorded.is_none_or(|entry| entry.externally_detected),
                },
            );
        }
    }
    state
}

#[cfg(test)]
mod tests;
