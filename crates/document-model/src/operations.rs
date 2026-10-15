use vibetex_core::{ChangeOrigin, Span, TextDelta};
use vibetex_latex_parser::{FormatKind, SectionLevel, SpacingKind};

/// Semantic document operations that construct targeted TextDeltas.
pub struct DocumentOperations;

impl DocumentOperations {
    /// Applies or updates a formatting command (\textbf, \textit, etc.) over a selection span.
    pub fn apply_format(span: Span, kind: FormatKind, source: &str, origin: ChangeOrigin) -> TextDelta {
        let selected_text = span.slice(source);
        let cmd = kind.command_name();
        let replacement = format!("\\{}{{{}}}", cmd, selected_text);
        TextDelta::new(span, replacement, origin)
    }

    /// Updates the title of an existing section without touching anything else.
    pub fn update_section_title(title_span: Span, new_title: &str, origin: ChangeOrigin) -> TextDelta {
        TextDelta::new(title_span, new_title, origin)
    }

    /// Inserts a new section at a given cursor byte offset.
    pub fn insert_section(cursor: usize, level: SectionLevel, title: &str, origin: ChangeOrigin) -> TextDelta {
        let cmd = level.command_name();
        let replacement = format!("\n\\{}{{{}}}\n", cmd, title);
        TextDelta::new(Span::empty(cursor), replacement, origin)
    }

    /// Inserts a citation \cite{key}.
    pub fn insert_citation(cursor: usize, cite_key: &str, origin: ChangeOrigin) -> TextDelta {
        let replacement = format!(r"\cite{{{}}}", cite_key);
        TextDelta::new(Span::empty(cursor), replacement, origin)
    }

    /// Inserts vertical/horizontal spacing.
    pub fn insert_spacing(cursor: usize, kind: SpacingKind, custom_arg: Option<&str>, origin: ChangeOrigin) -> TextDelta {
        let replacement = match (kind, custom_arg) {
            (SpacingKind::MedSkip, _) => "\n\\medskip\n".to_string(),
            (SpacingKind::SmallSkip, _) => "\n\\smallskip\n".to_string(),
            (SpacingKind::BigSkip, _) => "\n\\bigskip\n".to_string(),
            (SpacingKind::NoIndent, _) => "\\noindent ".to_string(),
            (SpacingKind::NewLine, _) => "\\\\\n".to_string(),
            (SpacingKind::VSpace, Some(val)) => format!("\n\\vspace{{{val}}}\n"),
            (SpacingKind::VSpace, None) => "\n\\vspace{1em}\n".to_string(),
            (SpacingKind::HSpace, Some(val)) => format!(r"\hspace{{{val}}}"),
            (SpacingKind::HSpace, None) => r"\hspace{1em}".to_string(),
        };
        TextDelta::new(Span::empty(cursor), replacement, origin)
    }

    /// Inserts an itemize or enumerate environment.
    pub fn insert_list(cursor: usize, ordered: bool, origin: ChangeOrigin) -> TextDelta {
        let env = if ordered { "enumerate" } else { "itemize" };
        let replacement = format!("\n\\begin{{{env}}}\n  \\item Primeiro item\n  \\item Segundo item\n\\end{{{env}}}\n");
        TextDelta::new(Span::empty(cursor), replacement, origin)
    }

    /// Inserts an equation block.
    pub fn insert_equation(cursor: usize, latex_formula: &str, origin: ChangeOrigin) -> TextDelta {
        let replacement = format!("\n\\begin{{equation}}\n  {latex_formula}\n\\end{{equation}}\n");
        TextDelta::new(Span::empty(cursor), replacement, origin)
    }
}
