use crate::{catalog::Catalog, uninstall::InstalledProgram};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct InstalledSearch {
    text: Vec<(String, Vec<String>)>,
    query: String,
    normalized: String,
    terms: Vec<String>,
}

impl InstalledSearch {
    pub fn invalidate(&mut self) {
        self.text.clear();
    }

    pub fn prepare(&mut self, programs: &[InstalledProgram], catalog: &Catalog, query: &str) {
        if self.query != query {
            self.query = query.to_owned();
            self.normalized = query.trim().to_lowercase();
            self.terms = self
                .normalized
                .split_whitespace()
                .map(str::to_owned)
                .collect();
        }
        if !self.normalized.is_empty() && self.text.len() != programs.len() {
            let metadata: BTreeMap<_, _> = catalog
                .packages
                .iter()
                .map(|p| (p.id.as_str(), p))
                .collect();
            self.text = programs
                .iter()
                .map(|program| {
                    let packages = program
                        .package_ids
                        .iter()
                        .filter_map(|id| metadata.get(id.as_str()))
                        .map(|p| p.search_text())
                        .collect();
                    (
                        format!("{} {}", program.name, program.publisher).to_lowercase(),
                        packages,
                    )
                })
                .collect();
        }
    }

    pub fn matches(&self, index: usize) -> bool {
        self.normalized.is_empty()
            || self.text[index].0.contains(&self.normalized)
            || self.text[index]
                .1
                .iter()
                .any(|text| self.terms.iter().all(|term| text.contains(term)))
    }
}
