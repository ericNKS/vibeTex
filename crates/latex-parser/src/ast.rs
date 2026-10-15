use serde::{Deserialize, Serialize};
use vibetex_core::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SectionLevel {
    Part,
    Chapter,
    Section,
    Subsection,
    Subsubsection,
    Paragraph,
}

impl SectionLevel {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Part => "part",
            Self::Chapter => "chapter",
            Self::Section => "section",
            Self::Subsection => "subsection",
            Self::Subsubsection => "subsubsection",
            Self::Paragraph => "paragraph",
        }
    }

    pub fn from_command(name: &str) -> Option<Self> {
        match name {
            "part" => Some(Self::Part),
            "chapter" => Some(Self::Chapter),
            "section" => Some(Self::Section),
            "subsection" => Some(Self::Subsection),
            "subsubsection" => Some(Self::Subsubsection),
            "paragraph" => Some(Self::Paragraph),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FormatKind {
    Bold,        // \textbf
    Italic,      // \textit, \emph
    Underline,   // \underline
    Typewriter,  // \texttt
    SmallCaps,   // \textsc
}

impl FormatKind {
    pub fn command_name(&self) -> &'static str {
        match self {
            Self::Bold => "textbf",
            Self::Italic => "textit",
            Self::Underline => "underline",
            Self::Typewriter => "texttt",
            Self::SmallCaps => "textsc",
        }
    }

    pub fn from_command(name: &str) -> Option<Self> {
        match name {
            "textbf" => Some(Self::Bold),
            "textit" | "emph" => Some(Self::Italic),
            "underline" => Some(Self::Underline),
            "texttt" => Some(Self::Typewriter),
            "textsc" => Some(Self::SmallCaps),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpacingKind {
    MedSkip,
    SmallSkip,
    BigSkip,
    VSpace,
    HSpace,
    NoIndent,
    NewLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MathBlockKind {
    Equation,
    Align,
    DisplayDollar, // $$...$$
    DisplayBracket, // \[...\]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListItem {
    pub content: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlineNode {
    Text { text: String, span: Span },
    Format { kind: FormatKind, inner: String, inner_span: Span, span: Span },
    InlineMath { content: String, span: Span },
    Citation { key: String, command: String, span: Span },
    Reference { label: String, command: String, span: Span },
    Label { key: String, span: Span },
    Protected { raw: String, span: Span },
}

impl InlineNode {
    pub fn span(&self) -> Span {
        match self {
            Self::Text { span, .. } => *span,
            Self::Format { span, .. } => *span,
            Self::InlineMath { span, .. } => *span,
            Self::Citation { span, .. } => *span,
            Self::Reference { span, .. } => *span,
            Self::Label { span, .. } => *span,
            Self::Protected { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AstNode {
    Preamble {
        raw: String,
        span: Span,
    },
    Section {
        level: SectionLevel,
        title: String,
        title_span: Span,
        span: Span,
    },
    Paragraph {
        inlines: Vec<InlineNode>,
        span: Span,
    },
    Environment {
        name: String,
        options: Option<String>,
        body: Vec<AstNode>,
        span: Span,
    },
    Itemize {
        items: Vec<ListItem>,
        span: Span,
    },
    Enumerate {
        items: Vec<ListItem>,
        span: Span,
    },
    MathBlock {
        kind: MathBlockKind,
        content: String,
        span: Span,
    },
    Spacing {
        kind: SpacingKind,
        argument: Option<String>,
        span: Span,
    },
    Comment {
        text: String,
        span: Span,
    },
    ProtectedBlock {
        raw: String,
        description: String,
        span: Span,
    },
}

impl AstNode {
    pub fn span(&self) -> Span {
        match self {
            Self::Preamble { span, .. } => *span,
            Self::Section { span, .. } => *span,
            Self::Paragraph { span, .. } => *span,
            Self::Environment { span, .. } => *span,
            Self::Itemize { span, .. } => *span,
            Self::Enumerate { span, .. } => *span,
            Self::MathBlock { span, .. } => *span,
            Self::Spacing { span, .. } => *span,
            Self::Comment { span, .. } => *span,
            Self::ProtectedBlock { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentAst {
    pub preamble: Option<AstNode>,
    pub nodes: Vec<AstNode>,
    pub full_span: Span,
}

impl DocumentAst {
    pub fn new(full_span: Span) -> Self {
        Self {
            preamble: None,
            nodes: Vec::new(),
            full_span,
        }
    }
}
