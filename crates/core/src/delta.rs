use crate::span::Span;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeOrigin {
    CodeEditor,
    VisualEditor,
    UndoRedo,
    ExternalSync,
    FormatAction,
}

/// Represents an atomic edit operation on a text buffer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDelta {
    pub span: Span,
    pub replacement: String,
    pub origin: ChangeOrigin,
}

impl TextDelta {
    pub fn new(span: Span, replacement: impl Into<String>, origin: ChangeOrigin) -> Self {
        Self {
            span,
            replacement: replacement.into(),
            origin,
        }
    }

    /// Computes the net change in byte length produced by this delta.
    pub fn byte_delta(&self) -> isize {
        self.replacement.len() as isize - self.span.len() as isize
    }

    /// Applies this delta to a canonical source string, returning the updated string and inverse delta for undo.
    pub fn apply(&self, source: &str) -> (String, TextDelta) {
        let prefix = &source[..self.span.start.min(source.len())];
        let original_text = self.span.slice(source).to_string();
        let suffix = &source[self.span.end.min(source.len())..];

        let mut updated = String::with_capacity(prefix.len() + self.replacement.len() + suffix.len());
        updated.push_str(prefix);
        updated.push_str(&self.replacement);
        updated.push_str(suffix);

        let inverse_span = Span::new(
            self.span.start,
            self.span.start + self.replacement.len(),
        );

        let inverse_delta = TextDelta {
            span: inverse_span,
            replacement: original_text,
            origin: ChangeOrigin::UndoRedo,
        };

        (updated, inverse_delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delta_application_and_inverse() {
        let original = r"\section{Introduction} Hello";
        // Replace "Introduction" (starts at index 9, len 12 -> 9..21)
        let span = Span::new(9, 21);
        assert_eq!(span.slice(original), "Introduction");

        let delta = TextDelta::new(span, "Overview of Methods", ChangeOrigin::VisualEditor);
        let (updated, inverse) = delta.apply(original);

        assert_eq!(updated, r"\section{Overview of Methods} Hello");
        assert_eq!(inverse.span, Span::new(9, 9 + "Overview of Methods".len()));
        assert_eq!(inverse.replacement, "Introduction");

        // Applying inverse should restore the original string bit-by-bit
        let (restored, _) = inverse.apply(&updated);
        assert_eq!(restored, original);
    }
}
