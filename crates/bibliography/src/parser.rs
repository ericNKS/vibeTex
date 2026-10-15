use crate::entry::BibEntry;
use regex::Regex;

pub struct BibParser;

impl BibParser {
    pub fn parse(content: &str) -> Vec<BibEntry> {
        let mut entries = Vec::new();
        let entry_header_re = Regex::new(r"@([a-zA-Z]+)\s*\{\s*([^,\s]+)\s*,").unwrap();

        for mat in entry_header_re.find_iter(content) {
            let start = mat.start();
            let caps = entry_header_re.captures(&content[start..]).unwrap();
            let entry_type = caps.get(1).unwrap().as_str();
            let cite_key = caps.get(2).unwrap().as_str();

            // Find matching closing brace
            let mut depth = 0;
            let mut end_idx = start;
            for (idx, ch) in content[start..].char_indices() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                    if depth == 0 {
                        end_idx = start + idx;
                        break;
                    }
                }
            }

            let entry_slice = &content[start..=end_idx];
            let mut entry = BibEntry::new(entry_type, cite_key);
            Self::parse_fields(entry_slice, &mut entry);
            entries.push(entry);
        }

        entries
    }

    fn parse_fields(slice: &str, entry: &mut BibEntry) {
        let field_re = Regex::new(r#"([a-zA-Z_]+)\s*=\s*(?:\{([^}]*)\}|"([^"]*)"|([0-9]+))"#).unwrap();
        for caps in field_re.captures_iter(slice) {
            let key = caps.get(1).unwrap().as_str();
            let val = caps
                .get(2)
                .or_else(|| caps.get(3))
                .or_else(|| caps.get(4))
                .map(|m| m.as_str().trim())
                .unwrap_or("");
            entry.set_field(key, val);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bib_parser() {
        let bib_content = r"
@article{knuth1984,
  author = {Donald E. Knuth},
  title = {Literate Programming},
  year = {1984},
  journal = {The Computer Journal},
  volume = {27},
  number = {2},
  pages = {97--111}
}

@book{lamport1994,
  author = {Leslie Lamport},
  title = {LaTeX: A Document Preparation System},
  year = {1994},
  publisher = {Addison-Wesley}
}
";

        let entries = BibParser::parse(bib_content);
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].cite_key, "knuth1984");
        assert_eq!(entries[0].entry_type, "article");
        assert_eq!(entries[0].author(), "Donald E. Knuth");
        assert_eq!(entries[0].title(), "Literate Programming");
        assert_eq!(entries[0].year(), "1984");

        assert_eq!(entries[1].cite_key, "lamport1994");
        assert_eq!(entries[1].author(), "Leslie Lamport");
    }
}
