use crate::entry::BibEntry;
use crate::parser::BibParser;
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct BibIndex {
    entries: HashMap<String, BibEntry>,
}

impl BibIndex {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn load_bib_file(&mut self, content: &str) {
        let parsed = BibParser::parse(content);
        for entry in parsed {
            self.entries.insert(entry.cite_key.clone(), entry);
        }
    }

    pub fn get(&self, cite_key: &str) -> Option<&BibEntry> {
        self.entries.get(cite_key)
    }

    pub fn all_entries(&self) -> Vec<&BibEntry> {
        let mut list: Vec<&BibEntry> = self.entries.values().collect();
        list.sort_by(|a, b| a.cite_key.cmp(&b.cite_key));
        list
    }

    /// Autocomplete / search references by query matching cite_key, author, or title.
    pub fn search(&self, query: &str) -> Vec<&BibEntry> {
        let q = query.to_lowercase();
        self.entries
            .values()
            .filter(|e| {
                e.cite_key.to_lowercase().contains(&q)
                    || e.author().to_lowercase().contains(&q)
                    || e.title().to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Detects missing citations: keys present in the LaTeX document without a corresponding .bib entry.
    pub fn find_missing(&self, cited_keys: &[String]) -> Vec<String> {
        cited_keys
            .iter()
            .filter(|k| !self.entries.contains_key(*k))
            .cloned()
            .collect()
    }
}
