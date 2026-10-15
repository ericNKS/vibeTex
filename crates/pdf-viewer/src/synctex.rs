#[derive(Debug, Clone, PartialEq)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PdfLocation {
    pub page: usize,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SyncTexData {
    pub records: Vec<(SourceLocation, PdfLocation)>,
}

impl SyncTexData {
    pub fn new() -> Self {
        Self::default()
    }

    /// Finds corresponding PDF location for a source file and line.
    pub fn forward_sync(&self, file: &str, line: usize) -> Option<PdfLocation> {
        self.records
            .iter()
            .find(|(src, _)| src.file.ends_with(file) && src.line == line)
            .map(|(_, pdf)| pdf.clone())
    }

    /// Finds corresponding source location for a click on a PDF page at (x, y).
    pub fn backward_sync(&self, page: usize, x: f64, y: f64, tolerance: f64) -> Option<SourceLocation> {
        self.records
            .iter()
            .find(|(_, pdf)| {
                pdf.page == page && (pdf.x - x).abs() < tolerance && (pdf.y - y).abs() < tolerance
            })
            .map(|(src, _)| src.clone())
    }
}
