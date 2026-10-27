use crate::dictionary::Dictionary;
use crate::tokenizer::LatexTokenizer;
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum LanguageMode {
    #[default]
    Portuguese,
    English,
    Bilingual,
    Disabled,
}

impl LanguageMode {
    pub fn label(&self) -> &'static str {
        match self {
            LanguageMode::Portuguese => "Português (BR)",
            LanguageMode::English => "Inglês (US)",
            LanguageMode::Bilingual => "Bilíngue (PT + EN)",
            LanguageMode::Disabled => "Desativado",
        }
    }

    pub fn short_label(&self) -> &'static str {
        match self {
            LanguageMode::Portuguese => "PT-BR",
            LanguageMode::English => "EN-US",
            LanguageMode::Bilingual => "PT+EN",
            LanguageMode::Disabled => "OFF",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellingIssue {
    pub word: String,
    pub char_start: usize,
    pub char_end: usize,
}

#[derive(Debug, Clone)]
pub struct PersonalDictionary {
    pub custom_words: HashSet<String>,
    pub session_ignored: HashSet<String>,
    pub storage_path: Option<PathBuf>,
}

impl PersonalDictionary {
    pub fn new() -> Self {
        Self {
            custom_words: HashSet::new(),
            session_ignored: HashSet::new(),
            storage_path: None,
        }
    }

    pub fn with_storage(path: PathBuf) -> Self {
        let mut dict = Self::new();
        dict.storage_path = Some(path);
        dict.load();
        dict
    }

    pub fn contains(&self, word: &str) -> bool {
        let lower = word.to_lowercase();
        self.custom_words.contains(&lower) || self.session_ignored.contains(&lower)
    }

    pub fn add_word(&mut self, word: &str) {
        self.custom_words.insert(word.to_lowercase());
        self.save();
    }

    pub fn ignore_word(&mut self, word: &str) {
        self.session_ignored.insert(word.to_lowercase());
    }

    pub fn load(&mut self) {
        if let Some(ref path) = self.storage_path {
            if path.exists() {
                if let Ok(content) = std::fs::read_to_string(path) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() {
                            self.custom_words.insert(trimmed.to_lowercase());
                        }
                    }
                }
            }
        }
    }

    pub fn save(&self) {
        if let Some(ref path) = self.storage_path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let mut list: Vec<&String> = self.custom_words.iter().collect();
            list.sort();
            let content = list.into_iter().cloned().collect::<Vec<String>>().join("\n");
            let _ = std::fs::write(path, content);
        }
    }
}

impl Default for PersonalDictionary {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SpellChecker {
    dict_pt: Dictionary,
    dict_en: Dictionary,
    personal_dict: RefCell<PersonalDictionary>,
}

impl SpellChecker {
    pub fn new() -> Self {
        let pt_bytes = include_bytes!("../data/pt_BR.dict.gz");
        let en_bytes = include_bytes!("../data/en_US.dict.gz");

        let dict_pt = Dictionary::from_gz_bytes(pt_bytes).unwrap_or_default();
        let dict_en = Dictionary::from_gz_bytes(en_bytes).unwrap_or_default();

        let user_dict_path = dirs_or_home_dict_path();
        let personal_dict = RefCell::new(PersonalDictionary::with_storage(user_dict_path));

        Self {
            dict_pt,
            dict_en,
            personal_dict,
        }
    }

    pub fn check_word(&self, word: &str, mode: LanguageMode) -> bool {
        if mode == LanguageMode::Disabled || word.is_empty() {
            return true;
        }

        // Whitelisted technical names
        if matches!(
            word.to_lowercase().as_str(),
            "vibetex"
                | "latex"
                | "pdflatex"
                | "xelatex"
                | "lualatex"
                | "bibtex"
                | "overleaf"
                | "tex"
                | "abnt"
                | "abntex"
                | "abntex2"
                | "github"
                | "gitlab"
                | "doi"
                | "arxiv"
        ) {
            return true;
        }

        // Check personal dictionary
        if self.personal_dict.borrow().contains(word) {
            return true;
        }

        match mode {
            LanguageMode::Portuguese => self.dict_pt.contains(word),
            LanguageMode::English => self.dict_en.contains(word),
            LanguageMode::Bilingual => self.dict_pt.contains(word) || self.dict_en.contains(word),
            LanguageMode::Disabled => true,
        }
    }

    pub fn check_latex(&self, latex_text: &str, mode: LanguageMode) -> Vec<SpellingIssue> {
        if mode == LanguageMode::Disabled {
            return Vec::new();
        }

        let tokens = LatexTokenizer::tokenize(latex_text);
        let mut issues = Vec::new();

        for token in tokens {
            if !self.check_word(&token.word, mode) {
                issues.push(SpellingIssue {
                    word: token.word,
                    char_start: token.char_start,
                    char_end: token.char_end,
                });
            }
        }

        issues
    }

    pub fn suggest(&self, word: &str, mode: LanguageMode, limit: usize) -> Vec<String> {
        match mode {
            LanguageMode::Portuguese => self.dict_pt.suggest(word, limit),
            LanguageMode::English => self.dict_en.suggest(word, limit),
            LanguageMode::Bilingual => {
                let mut pt_sug = self.dict_pt.suggest(word, limit);
                let en_sug = self.dict_en.suggest(word, limit);
                pt_sug.extend(en_sug);
                pt_sug.dedup();
                pt_sug.truncate(limit);
                pt_sug
            }
            LanguageMode::Disabled => Vec::new(),
        }
    }

    pub fn add_to_personal_dict(&self, word: &str) {
        self.personal_dict.borrow_mut().add_word(word);
    }

    pub fn ignore_word(&self, word: &str) {
        self.personal_dict.borrow_mut().ignore_word(word);
    }

    pub fn set_project_dict(&self, project_dir: &Path) {
        let path = project_dir.join(".vibetex_dict.txt");
        let mut cur = self.personal_dict.borrow_mut();
        cur.storage_path = Some(path);
        cur.load();
    }
}

impl Default for SpellChecker {
    fn default() -> Self {
        Self::new()
    }
}

fn dirs_or_home_dict_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".config/vibetex/dictionary.txt")
    } else {
        PathBuf::from(".vibetex_dict.txt")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checker_pt_and_en() {
        let checker = SpellChecker::new();

        // Valid PT words
        assert!(checker.check_word("metodologia", LanguageMode::Portuguese));
        assert!(checker.check_word("conclusão", LanguageMode::Portuguese));
        assert!(checker.check_word("desenvolvimento", LanguageMode::Portuguese));

        // Invalid PT words
        assert!(!checker.check_word("concluzão", LanguageMode::Portuguese));
        assert!(!checker.check_word("metodollogia", LanguageMode::Portuguese));

        // Valid EN words
        assert!(checker.check_word("methodology", LanguageMode::English));
        assert!(checker.check_word("algorithm", LanguageMode::English));
        assert!(!checker.check_word("algorythm", LanguageMode::English));

        // Bilingual mode accepts both
        assert!(checker.check_word("conclusão", LanguageMode::Bilingual));
        assert!(checker.check_word("methodology", LanguageMode::Bilingual));

        // Suggestions
        let sugs = checker.suggest("concluzão", LanguageMode::Portuguese, 3);
        assert!(!sugs.is_empty());
        assert!(sugs.iter().any(|s| s == "conclusão" || s == "conclusões"));
    }

    #[test]
    fn test_checker_latex_prose() {
        let checker = SpellChecker::new();
        let latex = r"
        \section{Introdução}
        Este é um texto com concluzão errada e \cite{silva2020}.
        ";

        let issues = checker.check_latex(latex, LanguageMode::Portuguese);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].word, "concluzão");
    }
}
