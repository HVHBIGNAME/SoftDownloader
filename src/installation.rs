use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::catalog::{InstallSpec, Package};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallMode {
    #[default]
    Silent,
    Interactive,
}

impl InstallMode {
    pub const ALL: [Self; 2] = [Self::Silent, Self::Interactive];

    pub fn label(self) -> &'static str {
        match self {
            Self::Silent => "Тихая установка",
            Self::Interactive => "С мастером",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct InstallationSettings {
    pub mode: InstallMode,
    pub confirm_before_install: bool,
    pub overrides: BTreeMap<String, InstallMode>,
}

impl InstallationSettings {
    pub fn mode_for(&self, id: &str) -> InstallMode {
        self.overrides.get(id).copied().unwrap_or(self.mode)
    }

    pub fn label_for(&self, package: &Package) -> &'static str {
        match package.install {
            Some(InstallSpec::Interactive { .. }) => "Обязательный мастер",
            Some(InstallSpec::Zip { .. } | InstallSpec::Portable { .. }) => "Автоматически · файлы",
            Some(InstallSpec::VscodeExtension { .. }) => "Автоматически · VS Code",
            _ if package.is_manual() => "По инструкции на сайте",
            _ => self.mode_for(&package.id).label(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::CatalogDocument;

    #[test]
    fn per_package_mode_overrides_the_default_without_hiding_mandatory_wizards() {
        let mut settings = InstallationSettings::default();
        assert!(!settings.confirm_before_install);
        settings
            .overrides
            .insert("vscode".into(), InstallMode::Interactive);
        assert_eq!(settings.mode_for("vscode"), InstallMode::Interactive);
        assert_eq!(settings.mode_for("git"), InstallMode::Silent);
        let catalog = CatalogDocument::builtin().unwrap().catalog;
        assert_eq!(
            settings.label_for(catalog.package("3utools").unwrap()),
            "Обязательный мастер"
        );
        assert_eq!(
            settings.label_for(catalog.package("cline").unwrap()),
            "Автоматически · VS Code"
        );
        let frozen = settings.clone();
        settings.mode = InstallMode::Interactive;
        assert_eq!(frozen.mode_for("git"), InstallMode::Silent);
    }
}
