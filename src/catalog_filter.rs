use std::collections::BTreeSet;

use crate::catalog::{Catalog, Package, PackageKind};
use crate::storage::{Library, is_installed};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum QuickFilter {
    #[default]
    All,
    Available,
    Selected,
}

#[derive(Clone, Debug, Default, Hash)]
pub struct CatalogFilter {
    pub query: String,
    pub category: Option<String>,
    pub kind: Option<PackageKind>,
    pub quick: QuickFilter,
    pub favorites_only: bool,
}

impl CatalogFilter {
    pub fn matches(
        &self,
        package: &Package,
        catalog: &Catalog,
        installed: &Library,
        favorites: &BTreeSet<String>,
        selected: &BTreeSet<String>,
    ) -> bool {
        let quick = match self.quick {
            QuickFilter::All => true,
            QuickFilter::Available => package.ready() && !is_installed(package, installed),
            QuickFilter::Selected => selected.contains(&package.id),
        };
        quick
            && (!self.favorites_only || favorites.contains(&package.id))
            && self.kind.is_none_or(|kind| package.kind == kind)
            && self
                .category
                .as_ref()
                .is_none_or(|id| catalog.category_contains(id, &package.category))
            && package.matches_search(&self.query)
    }

    pub fn reset_constraints(&mut self) {
        *self = Self {
            favorites_only: self.favorites_only,
            ..Default::default()
        };
    }

    pub fn has_constraints(&self) -> bool {
        !self.query.trim().is_empty()
            || self.category.is_some()
            || self.kind.is_some()
            || self.quick != QuickFilter::All
    }

    pub fn show_selection(&mut self) {
        *self = Self {
            quick: QuickFilter::Selected,
            ..Default::default()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::CatalogDocument;
    use crate::storage::InstalledPackage;

    #[test]
    fn favorites_search_and_nested_categories_combine_without_mutating_selection() {
        let catalog = CatalogDocument::builtin().unwrap().catalog;
        let package = catalog.package("java-21").unwrap();
        let favorites = BTreeSet::from(["java-21".into()]);
        let selected = BTreeSet::from(["vscode".into()]);
        let mut filter = CatalogFilter {
            favorites_only: true,
            query: "jdk 21".into(),
            category: Some("development".into()),
            ..Default::default()
        };
        assert!(filter.matches(package, &catalog, &Library::new(), &favorites, &selected));
        filter.category = Some("media".into());
        assert!(!filter.matches(package, &catalog, &Library::new(), &favorites, &selected));
        filter.reset_constraints();
        assert!(filter.favorites_only);
        assert!(!filter.has_constraints());
        assert!(filter.matches(package, &catalog, &Library::new(), &favorites, &selected));
        assert_eq!(selected, BTreeSet::from(["vscode".into()]));
    }

    #[test]
    fn available_filter_excludes_installed_manual_and_unresolved_packages() {
        let catalog = CatalogDocument::builtin().unwrap().catalog;
        let filter = CatalogFilter {
            quick: QuickFilter::Available,
            ..Default::default()
        };
        let installed = Library::from([(
            "vscode".into(),
            InstalledPackage {
                id: "vscode".into(),
                name: "VS Code".into(),
                version: "old".into(),
                sha256: String::new(),
                installed_at: 0,
                reboot_required: false,
                uninstall: None,
                externally_detected: true,
            },
        )]);
        for id in ["vscode", "eset-premium", "flclash"] {
            assert!(
                !filter.matches(
                    catalog.package(id).unwrap(),
                    &catalog,
                    &installed,
                    &BTreeSet::new(),
                    &BTreeSet::new()
                ),
                "{id}"
            );
        }
        assert!(filter.matches(
            catalog.package("7zip").unwrap(),
            &catalog,
            &installed,
            &BTreeSet::new(),
            &BTreeSet::new()
        ));
    }

    #[test]
    fn reviewing_selection_clears_hidden_filters_and_keeps_unavailable_choices_visible() {
        let catalog = CatalogDocument::builtin().unwrap().catalog;
        let mut filter = CatalogFilter {
            query: "unrelated".into(),
            category: Some("media".into()),
            favorites_only: true,
            ..Default::default()
        };
        filter.show_selection();
        let selected = BTreeSet::from(["flclash".into()]);
        assert!(filter.matches(
            catalog.package("flclash").unwrap(),
            &catalog,
            &Library::new(),
            &BTreeSet::new(),
            &selected
        ));
        assert!(!filter.matches(
            catalog.package("7zip").unwrap(),
            &catalog,
            &Library::new(),
            &BTreeSet::new(),
            &selected
        ));
    }
}
