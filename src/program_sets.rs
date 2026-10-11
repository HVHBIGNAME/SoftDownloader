use std::collections::BTreeSet;

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

pub struct Preset {
    pub name: &'static str,
    pub description: &'static str,
    pub ids: &'static [&'static str],
}

pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Базовый ПК",
        description: "Архивы, видео, PDF и быстрый поиск файлов.",
        ids: &["7zip", "vlc", "sumatra-pdf", "everything"],
    },
    Preset {
        name: "Работа и учёба",
        description: "Офис, заметки, книги и браузер.",
        ids: &["libreoffice", "joplin", "calibre", "firefox"],
    },
    Preset {
        name: "Удобная Windows",
        description: "Быстрый поиск, предпросмотр по пробелу, буфер обмена и передача файлов.",
        ids: &["everything", "quicklook", "ditto", "localsend"],
    },
    Preset {
        name: "Документы и сканы",
        description: "Сканирование с OCR, обработка и чтение PDF.",
        ids: &["naps2", "pdf24", "sumatra-pdf"],
    },
    Preset {
        name: "Разработка",
        description: "VS Code, Git, терминал, Python и инструменты редактора.",
        ids: &[
            "vscode",
            "git",
            "windows-terminal",
            "python",
            "ms-python-python",
            "charliermarsh-ruff",
        ],
    },
    Preset {
        name: "Веб-разработка",
        description: "JavaScript, TypeScript, форматирование и проверка кода.",
        ids: &[
            "vscode",
            "git",
            "nodejs",
            "esbenp-prettier-vscode",
            "dbaeumer-vscode-eslint",
            "postman",
        ],
    },
    Preset {
        name: "Игры и общение",
        description: "Steam, Epic, Discord и библиотеки Visual C++.",
        ids: &[
            "steam",
            "epic-games",
            "discord",
            "vcredist-x64",
            "vcredist-x86",
        ],
    },
    Preset {
        name: "Творчество",
        description: "3D, графика, видео, звук и запись экрана.",
        ids: &["blender", "gimp", "inkscape", "kdenlive", "audacity", "obs"],
    },
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramSet {
    pub name: String,
    pub ids: BTreeSet<String>,
}

impl ProgramSet {
    pub fn new(name: &str, ids: BTreeSet<String>) -> Result<Self> {
        let set = Self {
            name: name.trim().into(),
            ids,
        };
        set.validate()?;
        Ok(set)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            !self.name.trim().is_empty()
                && self.name.chars().count() <= 80
                && !self.name.chars().any(char::is_control),
            "Название набора: от 1 до 80 символов"
        );
        ensure!(
            !self.ids.is_empty() && self.ids.len() <= 5000,
            "Выберите программы для набора"
        );
        for id in &self.ids {
            crate::paths::validate_id(id)?;
        }
        Ok(())
    }
}

pub fn validate_sets(sets: &[ProgramSet]) -> Result<()> {
    ensure!(sets.len() <= 32, "Можно сохранить до 32 наборов");
    let mut names = BTreeSet::new();
    for set in sets {
        set.validate()?;
        ensure!(
            names.insert(set.name.to_lowercase()),
            "Набор с таким названием уже есть"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::CatalogDocument;

    #[test]
    fn all_presets_reference_real_installable_packages() {
        let catalog = CatalogDocument::builtin().unwrap().catalog;
        for preset in PRESETS {
            let ids: BTreeSet<_> = preset.ids.iter().map(|id| id.to_string()).collect();
            assert_eq!(ids.len(), preset.ids.len());
            for id in ids {
                assert!(catalog.package(&id).unwrap().ready(), "{id}");
            }
        }
    }

    #[test]
    fn saved_sets_validate_names_but_preserve_future_catalog_ids() {
        let set = ProgramSet::new("  Мой ПК  ", BTreeSet::from(["future-package".into()])).unwrap();
        assert_eq!(set.name, "Мой ПК");
        validate_sets(std::slice::from_ref(&set)).unwrap();
        let duplicate = ProgramSet::new("мой пк", set.ids.clone()).unwrap();
        assert!(validate_sets(&[set, duplicate]).is_err());
        assert!(ProgramSet::new("", BTreeSet::from(["git".into()])).is_err());
        assert!(ProgramSet::new("Empty", BTreeSet::new()).is_err());
    }

    #[test]
    fn exported_sets_keep_ids_that_are_not_in_this_catalog() {
        let catalog = CatalogDocument::builtin().unwrap().catalog;
        let ids = BTreeSet::from(["git".into(), "future-package".into()]);
        let list = crate::program_list::ProgramList::from_ids(&catalog, &ids);
        let parsed =
            crate::program_list::ProgramList::parse(&serde_json::to_vec(&list).unwrap()).unwrap();
        let exported: BTreeSet<_> = parsed
            .programs
            .iter()
            .flat_map(|p| p.package_ids.clone())
            .collect();
        assert_eq!(exported, ids);
    }
}
