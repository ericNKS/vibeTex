use crate::delta::TextDelta;
use crate::diagnostics::Diagnostic;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum DocumentEvent {
    ContentChanged(TextDelta),
    CursorMoved { line: usize, column: usize, byte_offset: usize },
    DocumentSaved(PathBuf),
}

#[derive(Debug, Clone)]
pub enum ProjectEvent {
    FileOpened(PathBuf),
    CompilationStarted,
    CompilationCompleted { success: bool, pdf_path: Option<PathBuf>, diagnostics: Vec<Diagnostic> },
}
