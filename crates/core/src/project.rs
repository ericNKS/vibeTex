use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompilerEngine {
    Latexmk,
    PdfLatex,
    XeLatex,
    LuaLatex,
    Tectonic,
}

impl Default for CompilerEngine {
    fn default() -> Self {
        Self::Latexmk
    }
}

impl std::fmt::Display for CompilerEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Latexmk => write!(f, "latexmk"),
            Self::PdfLatex => write!(f, "pdflatex"),
            Self::XeLatex => write!(f, "xelatex"),
            Self::LuaLatex => write!(f, "lualatex"),
            Self::Tectonic => write!(f, "tectonic"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub name: String,
    pub main_file: PathBuf,
    pub engine: CompilerEngine,
    pub synctex: bool,
    pub shell_escape: bool, // FALSE by default for security!
    pub bib_engine: String,
    pub output_dir: PathBuf,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            name: "Novo Projeto".to_string(),
            main_file: PathBuf::from("main.tex"),
            engine: CompilerEngine::Latexmk,
            synctex: true,
            shell_escape: false,
            bib_engine: "biber".to_string(),
            output_dir: PathBuf::from(".build"),
        }
    }
}

impl ProjectConfig {
    pub fn new(name: impl Into<String>, main_file: impl AsRef<Path>) -> Self {
        Self {
            name: name.into(),
            main_file: main_file.as_ref().to_path_buf(),
            ..Default::default()
        }
    }
}
