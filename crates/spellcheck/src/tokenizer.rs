#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatexToken {
    pub word: String,
    pub char_start: usize,
    pub char_end: usize,
}

pub struct LatexTokenizer;

impl LatexTokenizer {
    /// Extracts prose words with their character offsets from LaTeX source code.
    pub fn tokenize(source: &str) -> Vec<LatexToken> {
        let chars: Vec<char> = source.chars().collect();
        let total_chars = chars.len();
        let mut tokens = Vec::new();
        let mut i = 0;

        while i < total_chars {
            // 1. Skip comments: % until newline (unless escaped \%)
            if chars[i] == '%' && (i == 0 || chars[i - 1] != '\\') {
                while i < total_chars && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }

            // 2. Skip inline math: $ ... $ or $$ ... $$
            if chars[i] == '$' && (i == 0 || chars[i - 1] != '\\') {
                i += 1;
                if i < total_chars && chars[i] == '$' {
                    // $$ ... $$
                    i += 1;
                    while i + 1 < total_chars && !(chars[i] == '$' && chars[i + 1] == '$') {
                        i += 1;
                    }
                    i = (i + 2).min(total_chars);
                } else {
                    // $ ... $
                    while i < total_chars && (chars[i] != '$' || chars[i - 1] == '\\') {
                        i += 1;
                    }
                    if i < total_chars {
                        i += 1;
                    }
                }
                continue;
            }

            // 3. Skip \( ... \) and \[ ... \]
            if chars[i] == '\\' && i + 1 < total_chars && (chars[i + 1] == '(' || chars[i + 1] == '[') {
                let closing = if chars[i + 1] == '(' { ')' } else { ']' };
                i += 2;
                while i + 1 < total_chars && !(chars[i] == '\\' && chars[i + 1] == closing) {
                    i += 1;
                }
                i = (i + 2).min(total_chars);
                continue;
            }

            // 4. Handle commands starting with '\'
            if chars[i] == '\\' {
                i += 1;
                let cmd_start = i;
                while i < total_chars && (chars[i].is_alphabetic() || chars[i] == '*') {
                    i += 1;
                }
                let cmd: String = chars[cmd_start..i].iter().collect();

                // Check commands whose arguments are keys/labels/filenames that must NOT be spellchecked
                if matches!(
                    cmd.as_str(),
                    "cite"
                        | "citep"
                        | "citet"
                        | "citeauthor"
                        | "citeyear"
                        | "ref"
                        | "pageref"
                        | "autoref"
                        | "eqref"
                        | "label"
                        | "documentclass"
                        | "usepackage"
                        | "input"
                        | "include"
                        | "bibliography"
                        | "bibliographystyle"
                        | "url"
                        | "pagestyle"
                        | "thispagestyle"
                        | "pagenumbering"
                        | "hypersetup"
                        | "setlength"
                        | "addtolength"
                        | "rule"
                        | "vspace"
                        | "hspace"
                        | "includegraphics"
                ) {
                    // Skip optional [ ... ]
                    i = Self::skip_brackets(&chars, i);
                    // Skip mandatory { ... }
                    i = Self::skip_braces(&chars, i);
                    continue;
                }

                // Handle \begin{env} and \end{env}
                if cmd == "begin" || cmd == "end" {
                    i = Self::skip_brackets(&chars, i);
                    let env_name = Self::extract_braces_content(&chars, &mut i);

                    // If it's a math or verbatim environment, skip until \end{env}
                    if cmd == "begin"
                        && matches!(
                            env_name.as_str(),
                            "equation"
                                | "equation*"
                                | "align"
                                | "align*"
                                | "gather"
                                | "gather*"
                                | "multline"
                                | "multline*"
                                | "matrix"
                                | "pmatrix"
                                | "bmatrix"
                                | "verbatim"
                                | "lstlisting"
                                | "minted"
                        )
                    {
                        let end_target = format!("\\end{{{}}}", env_name);
                        let end_chars: Vec<char> = end_target.chars().collect();
                        while i + end_chars.len() <= total_chars {
                            if chars[i..i + end_chars.len()] == end_chars[..] {
                                i += end_chars.len();
                                break;
                            }
                            i += 1;
                        }
                    }
                    continue;
                }

                // Any other command (e.g. \textbf, \textit, \section, etc.):
                // We skipped the command name itself, leaving the text arguments inside { } to be tokenized!
                continue;
            }

            // 5. Extract regular word
            if chars[i].is_alphabetic() {
                let word_start = i;
                while i < total_chars && (chars[i].is_alphabetic() || chars[i] == '-' || chars[i] == '\'') {
                    // If hyphen or apostrophe, ensure it's not trailing at end of word
                    if (chars[i] == '-' || chars[i] == '\'') && (i + 1 == total_chars || !chars[i + 1].is_alphabetic()) {
                        break;
                    }
                    i += 1;
                }
                let word: String = chars[word_start..i].iter().collect();

                // Filter out single non-vowels or words with digits
                if !word.is_empty() && (word.chars().count() > 1 || matches!(word.to_lowercase().as_str(), "a" | "e" | "o" | "i" | "u")) {
                    tokens.push(LatexToken {
                        word,
                        char_start: word_start,
                        char_end: i,
                    });
                }
                continue;
            }

            i += 1;
        }

        tokens
    }

    fn skip_brackets(chars: &[char], mut i: usize) -> usize {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i < chars.len() && chars[i] == '[' {
            i += 1;
            let mut depth = 1;
            while i < chars.len() && depth > 0 {
                if chars[i] == '[' {
                    depth += 1;
                } else if chars[i] == ']' {
                    depth -= 1;
                }
                i += 1;
            }
        }
        i
    }

    fn skip_braces(chars: &[char], mut i: usize) -> usize {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i < chars.len() && chars[i] == '{' {
            i += 1;
            let mut depth = 1;
            while i < chars.len() && depth > 0 {
                if chars[i] == '{' {
                    depth += 1;
                } else if chars[i] == '}' {
                    depth -= 1;
                }
                i += 1;
            }
        }
        i
    }

    fn extract_braces_content(chars: &[char], i: &mut usize) -> String {
        while *i < chars.len() && chars[*i].is_whitespace() {
            *i += 1;
        }
        if *i < chars.len() && chars[*i] == '{' {
            *i += 1;
            let start = *i;
            while *i < chars.len() && chars[*i] != '}' {
                *i += 1;
            }
            let content: String = chars[start..*i].iter().collect();
            if *i < chars.len() {
                *i += 1;
            }
            content.trim().to_string()
        } else {
            String::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skip_comments_and_citations() {
        let text = r"
        \documentclass{article}
        \usepackage{amsmath}
        % Este eh um comentario
        \section{Introdução ao Estudo}
        Este é um texto com \cite{silva2020} e fórmula $x^2 + y = 10$.
        Também temos \textbf{destaque} em negrito.
        ";

        let tokens = LatexTokenizer::tokenize(text);
        let words: Vec<&str> = tokens.iter().map(|t| t.word.as_str()).collect();

        // Must include prose:
        assert!(words.contains(&"Introdução"));
        assert!(words.contains(&"ao"));
        assert!(words.contains(&"Estudo"));
        assert!(words.contains(&"Este"));
        assert!(words.contains(&"texto"));
        assert!(words.contains(&"destaque"));
        assert!(words.contains(&"negrito"));

        // Must NOT include LaTeX commands or keys or math:
        assert!(!words.contains(&"documentclass"));
        assert!(!words.contains(&"article"));
        assert!(!words.contains(&"usepackage"));
        assert!(!words.contains(&"amsmath"));
        assert!(!words.contains(&"silva2020"));
        assert!(!words.contains(&"section"));
        assert!(!words.contains(&"textbf"));
        assert!(!words.contains(&"comentario")); // inside comment
    }
}
