use crate::catalog::Catalog;

#[derive(Default)]
pub(super) struct CatalogSearch {
    text: Vec<String>,
    ranks: Vec<usize>,
    query: String,
    terms: Vec<String>,
}

impl CatalogSearch {
    pub fn invalidate(&mut self) {
        self.text.clear();
        self.ranks.clear();
    }

    pub fn prepare(&mut self, catalog: &Catalog, query: &str, sorted: bool) {
        if self.query != query {
            self.query = query.to_owned();
            self.terms = query
                .to_lowercase()
                .split_whitespace()
                .map(str::to_owned)
                .collect();
        }
        if self.text.len() != catalog.packages.len() && (!self.terms.is_empty() || sorted) {
            self.text = catalog.packages.iter().map(|p| p.search_text()).collect();
            let mut order: Vec<_> = catalog
                .packages
                .iter()
                .enumerate()
                .map(|(i, p)| (i, p.name.to_lowercase()))
                .collect();
            order.sort_by(|a, b| a.1.cmp(&b.1));
            self.ranks = vec![0; catalog.packages.len()];
            for (rank, (index, _)) in order.into_iter().enumerate() {
                self.ranks[index] = rank;
            }
        }
    }

    pub fn matches(&self, index: usize) -> bool {
        self.terms
            .iter()
            .all(|term| self.text[index].contains(term))
    }

    pub fn rank(&self, index: usize) -> usize {
        self.ranks[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::CatalogDocument;

    #[test]
    fn indexed_search_matches_uncached_semantics_and_rebuilds_on_catalog_replacement() {
        let mut catalog = CatalogDocument::builtin().unwrap().catalog;
        let mut search = CatalogSearch::default();
        for query in [
            "",
            "   ",
            "BLENDER 3D",
            "visual code",
            "редактор",
            "jdk 21",
            "not found",
        ] {
            search.prepare(&catalog, query, true);
            for (i, package) in catalog.packages.iter().enumerate() {
                assert_eq!(search.matches(i), package.matches_search(query));
            }
        }
        let mut expected: Vec<_> = (0..catalog.packages.len()).collect();
        expected.sort_by_key(|&i| catalog.packages[i].name.to_lowercase());
        let mut indexed: Vec<_> = (0..catalog.packages.len()).collect();
        indexed.sort_unstable_by_key(|&i| search.rank(i));
        assert_eq!(indexed, expected);
        catalog.packages[0].name = "Unique replacement title".into();
        search.invalidate();
        search.prepare(&catalog, "Unique replacement", false);
        assert!(search.matches(0));
    }
}
