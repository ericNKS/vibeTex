use crate::levenshtein::damerau_levenshtein;
use flate2::read::GzDecoder;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};

#[derive(Clone, Debug)]
pub struct Dictionary {
    words: HashMap<String, u32>,
}

impl Dictionary {
    pub fn new() -> Self {
        Self {
            words: HashMap::new(),
        }
    }

    pub fn from_gz_bytes(data: &[u8]) -> Result<Self, std::io::Error> {
        let decoder = GzDecoder::new(data);
        let reader = BufReader::new(decoder);
        let mut words = HashMap::with_capacity(55000);

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let mut parts = trimmed.split_whitespace();
            if let Some(word) = parts.next() {
                let freq = parts
                    .next()
                    .and_then(|f| f.parse::<u32>().ok())
                    .unwrap_or(1);
                words.insert(word.to_lowercase(), freq);
            }
        }

        Ok(Self { words })
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    pub fn add_word(&mut self, word: &str, freq: u32) {
        self.words.insert(word.to_lowercase(), freq);
    }

    pub fn contains(&self, word: &str) -> bool {
        if word.is_empty() {
            return true;
        }

        // Single digit or roman numerals / single letters are usually valid in math/text
        if word.chars().count() == 1 {
            let c = word.chars().next().unwrap();
            if c.is_alphabetic() {
                // 'a', 'e', 'o', 'I', 'A', 'O', 'E' are valid in PT / EN
                let lower = c.to_lowercase().next().unwrap();
                return matches!(lower, 'a' | 'e' | 'o' | 'i' | 'u');
            }
        }

        let lower = word.to_lowercase();
        if self.words.contains_key(&lower) {
            return true;
        }

        // If ends with hyphen (e.g. line-break in text)
        if lower.ends_with('-') && self.words.contains_key(&lower[..lower.len() - 1]) {
            return true;
        }

        // Words with clitic pronouns in Portuguese like 'fazer-se', 'enviar-lhe', 'diz-se'
        if let Some((stem, clitic)) = lower.split_once('-') {
            let is_clitic = matches!(
                clitic,
                "se" | "me" | "te" | "nos" | "vos" | "lhe" | "lhes" | "o" | "a" | "os" | "as"
                    | "lo" | "la" | "los" | "las" | "no" | "na" | "nas"
            );
            if is_clitic && self.words.contains_key(stem) {
                return true;
            }
        }

        false
    }

    pub fn suggest(&self, word: &str, limit: usize) -> Vec<String> {
        let lower = word.to_lowercase();
        let target_len = lower.chars().count();
        if target_len == 0 {
            return Vec::new();
        }

        let is_first_upper = word
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false);
        let is_all_upper = word.chars().all(|c| c.is_uppercase() || !c.is_alphabetic());

        let max_dist = if target_len <= 3 { 1 } else { 2 };

        // Collect candidates within length window
        let mut candidates: Vec<(&str, usize, u32)> = Vec::new();

        for (cand, &freq) in &self.words {
            let cand_len = cand.chars().count();
            if cand_len.abs_diff(target_len) > max_dist {
                continue;
            }

            // Quick check: if first characters are completely different and len > 4, skip for speed
            if target_len >= 5 && cand_len >= 5 {
                let first_t = lower.chars().next().unwrap();
                let first_c = cand.chars().next().unwrap();
                if first_t != first_c {
                    let second_t = lower.chars().nth(1).unwrap();
                    let second_c = cand.chars().nth(1).unwrap();
                    // unless transposed: first_t == second_c && second_t == first_c
                    if !(first_t == second_c && second_t == first_c) {
                        continue;
                    }
                }
            }

            let dist = damerau_levenshtein(&lower, cand);
            if dist <= max_dist {
                candidates.push((cand.as_str(), dist, freq));
            }
        }

        // Sort by: (distance ascending, frequency descending)
        candidates.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| b.2.cmp(&a.2))
        });

        candidates
            .into_iter()
            .take(limit)
            .map(|(c, _, _)| {
                if is_all_upper && target_len > 1 {
                    c.to_uppercase()
                } else if is_first_upper {
                    let mut chars = c.chars();
                    if let Some(first) = chars.next() {
                        first.to_uppercase().collect::<String>() + chars.as_str()
                    } else {
                        c.to_string()
                    }
                } else {
                    c.to_string()
                }
            })
            .collect()
    }
}

impl Default for Dictionary {
    fn default() -> Self {
        Self::new()
    }
}
