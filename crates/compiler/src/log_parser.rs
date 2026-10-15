use regex::Regex;
use vibetex_core::{Diagnostic, DiagnosticSeverity};

#[derive(Debug, Clone, Default)]
pub struct CompilationSummary {
    pub success: bool,
    pub page_count: Option<usize>,
    pub diagnostics: Vec<Diagnostic>,
    pub raw_log: String,
}

pub struct LogParser;

impl LogParser {
    pub fn parse(log_content: &str) -> CompilationSummary {
        let mut diagnostics = Vec::new();
        let mut page_count = None;

        let page_regex = Regex::new(r"Output written on .*?\((\d+)\s+pages?").unwrap();

        let lines: Vec<&str> = log_content.lines().collect();
        let mut i = 0;

        while i < lines.len() {
            let line = lines[i];

            // Check for pages
            if let Some(caps) = page_regex.captures(line) {
                if let Some(m) = caps.get(1) {
                    page_count = m.as_str().parse().ok();
                }
            }

            // Check for LaTeX Errors starting with !
            if line.starts_with('!') {
                let err_msg = line.trim_start_matches('!').trim().to_string();
                let mut line_num = None;

                // Check next few lines for "l.XX"
                for j in 1..=4 {
                    if let Some(next_line) = lines.get(i + j) {
                        if next_line.starts_with("l.") {
                            if let Some(space_idx) = next_line[2..].find(' ') {
                                line_num = next_line[2..2 + space_idx].parse().ok();
                            }
                            break;
                        }
                    }
                }

                let mut diag = Diagnostic::error(err_msg);
                if let Some(ln) = line_num {
                    diag.line = Some(ln);
                }
                diagnostics.push(diag);
            } else if line.contains("LaTeX Warning:") {
                let msg_part = line.trim_start_matches("LaTeX Warning:").trim();
                let line_num = if let Some(idx) = line.find("on input line ") {
                    let rest = &line[idx + "on input line ".len()..];
                    rest.trim_end_matches('.').trim().parse::<usize>().ok()
                } else {
                    None
                };
                let mut diag = Diagnostic::warning(msg_part);
                diag.line = line_num;
                diagnostics.push(diag);
            }

            i += 1;
        }

        let has_errors = diagnostics.iter().any(|d| d.severity == DiagnosticSeverity::Error);

        CompilationSummary {
            success: !has_errors,
            page_count,
            diagnostics,
            raw_log: log_content.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_parser_errors_and_pages() {
        let log = r"This is pdfTeX, Version 3.141592653-2.6-1.40.24 (TeX Live 2022)
(./main.tex
! Undefined control sequence.
l.45 \meuComandoErrado
                      {arg}
LaTeX Warning: Citation 'silva2026' on page 1 undefined on input line 52.
Output written on main.pdf (3 pages, 14205 bytes).
Transcript written on main.log.";

        let summary = LogParser::parse(log);
        assert!(!summary.success);
        assert_eq!(summary.page_count, Some(3));
        assert_eq!(summary.diagnostics.len(), 2);
        assert_eq!(summary.diagnostics[0].severity, DiagnosticSeverity::Error);
        assert_eq!(summary.diagnostics[0].line, Some(45));
        assert_eq!(summary.diagnostics[1].severity, DiagnosticSeverity::Warning);
        assert_eq!(summary.diagnostics[1].line, Some(52));
    }
}
