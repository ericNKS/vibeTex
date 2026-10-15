use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BibEntry {
    pub entry_type: String,
    pub cite_key: String,
    pub fields: HashMap<String, String>,
}

impl BibEntry {
    pub fn new(entry_type: impl Into<String>, cite_key: impl Into<String>) -> Self {
        Self {
            entry_type: entry_type.into().to_lowercase(),
            cite_key: cite_key.into(),
            fields: HashMap::new(),
        }
    }

    pub fn set_field(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.fields.insert(key.into().to_lowercase(), value.into());
    }

    pub fn get_field(&self, key: &str) -> Option<&str> {
        self.fields.get(&key.to_lowercase()).map(|s| s.as_str())
    }

    pub fn title(&self) -> &str {
        self.get_field("title").unwrap_or("Sem título")
    }

    pub fn author(&self) -> &str {
        self.get_field("author").unwrap_or("Autor desconhecido")
    }

    pub fn year(&self) -> &str {
        self.get_field("year").unwrap_or("s.d.")
    }

    pub fn formatted_citation(&self) -> String {
        format!("{} ({}) - {}", self.author(), self.year(), self.title())
    }
}
