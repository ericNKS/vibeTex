use crate::history::HistoryStack;
use thiserror::Error;
use vibetex_core::{ChangeOrigin, TextDelta};
use vibetex_latex_parser::{parse_latex, DocumentAst};

#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("Span fora dos limites do documento: {0}")]
    OutOfBounds(String),
    #[error("Reentrância detectada durante sincronização")]
    ReentrantSync,
}

pub struct SharedDocument {
    canonical_source: String,
    ast: DocumentAst,
    history: HistoryStack,
    is_syncing: bool,
    dirty: bool,
}

impl SharedDocument {
    pub fn new(initial_source: impl Into<String>) -> Self {
        let source = initial_source.into();
        let ast = parse_latex(&source);
        Self {
            canonical_source: source,
            ast,
            history: HistoryStack::new(200),
            is_syncing: false,
            dirty: false,
        }
    }

    pub fn source(&self) -> &str {
        &self.canonical_source
    }

    pub fn ast(&self) -> &DocumentAst {
        &self.ast
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn mark_saved(&mut self) {
        self.dirty = false;
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Applies a TextDelta atomically to the canonical document buffer, updates AST, and records history.
    pub fn apply_delta(&mut self, delta: TextDelta) -> Result<TextDelta, DocumentError> {
        if self.is_syncing {
            return Err(DocumentError::ReentrantSync);
        }

        if delta.span.start > self.canonical_source.len() || delta.span.end > self.canonical_source.len() {
            return Err(DocumentError::OutOfBounds(format!(
                "Span {} além do comprimento {}",
                delta.span,
                self.canonical_source.len()
            )));
        }

        self.is_syncing = true;
        let (updated, inverse) = delta.apply(&self.canonical_source);
        self.canonical_source = updated;
        self.ast = parse_latex(&self.canonical_source);
        self.dirty = true;

        if delta.origin != ChangeOrigin::UndoRedo {
            self.history.push(inverse.clone());
        }

        self.is_syncing = false;
        Ok(inverse)
    }

    /// Performs an Undo operation.
    pub fn undo(&mut self) -> Result<Option<TextDelta>, DocumentError> {
        if let Some(inverse_delta) = self.history.pop_undo() {
            let (updated, redo_inverse) = inverse_delta.apply(&self.canonical_source);
            self.canonical_source = updated;
            self.ast = parse_latex(&self.canonical_source);
            self.history.push_redo(redo_inverse);
            self.dirty = true;
            Ok(Some(inverse_delta))
        } else {
            Ok(None)
        }
    }

    /// Performs a Redo operation.
    pub fn redo(&mut self) -> Result<Option<TextDelta>, DocumentError> {
        if let Some(redo_delta) = self.history.pop_redo() {
            let (updated, undo_inverse) = redo_delta.apply(&self.canonical_source);
            self.canonical_source = updated;
            self.ast = parse_latex(&self.canonical_source);
            self.history.push(undo_inverse);
            self.dirty = true;
            Ok(Some(redo_delta))
        } else {
            Ok(None)
        }
    }

    /// Maps a 0-based byte offset to 1-based (line, column).
    pub fn byte_offset_to_line_col(&self, byte_offset: usize) -> (usize, usize) {
        let safe_offset = byte_offset.min(self.canonical_source.len());
        let slice = &self.canonical_source[..safe_offset];
        let line = slice.chars().filter(|&c| c == '\n').count() + 1;
        let last_newline = slice.rfind('\n').map(|idx| idx + 1).unwrap_or(0);
        let col = slice[last_newline..].chars().count() + 1;
        (line, col)
    }

    /// Maps 1-based (line, column) to 0-based byte offset.
    pub fn line_col_to_byte_offset(&self, target_line: usize, target_col: usize) -> usize {
        let mut current_line = 1;
        let mut current_col = 1;
        let mut offset = 0;

        for (idx, ch) in self.canonical_source.char_indices() {
            if current_line == target_line && current_col == target_col {
                return idx;
            }
            if ch == '\n' {
                current_line += 1;
                current_col = 1;
            } else {
                current_col += 1;
            }
            offset = idx + ch.len_utf8();
        }
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shared_document_editing_and_undo() {
        let initial = r"\documentclass{article}
\begin{document}
\section{Título Original}
Texto explicativo com \textbf{palavra}.
\end{document}";

        let mut doc = SharedDocument::new(initial);
        assert!(!doc.is_dirty());

        // Locate title span
        let title_span = match &doc.ast().nodes[0] {
            vibetex_latex_parser::AstNode::Section { title_span, .. } => *title_span,
            _ => panic!("Esperada seção"),
        };
        assert_eq!(title_span.slice(doc.source()), "Título Original");

        // Edit title
        let delta = TextDelta::new(title_span, "Novo Título Exclusivo", ChangeOrigin::VisualEditor);
        let _ = doc.apply_delta(delta).expect("Aplicação com sucesso");

        assert!(doc.is_dirty());
        assert!(doc.source().contains("Novo Título Exclusivo"));
        assert!(!doc.source().contains("Título Original"));

        // Test Undo
        assert!(doc.can_undo());
        let _ = doc.undo().expect("Undo com sucesso");
        assert_eq!(doc.source(), initial);

        // Test Redo
        assert!(doc.can_redo());
        let _ = doc.redo().expect("Redo com sucesso");
        assert!(doc.source().contains("Novo Título Exclusivo"));
    }
}
