use std::path::PathBuf;
use std::process::Command;
use vibetex_core::CompilerEngine;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ToolStatus {
    pub name: String,
    pub path: Option<PathBuf>,
    pub available: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct SystemTexReport {
    pub latexmk: ToolStatus,
    pub pdflatex: ToolStatus,
    pub xelatex: ToolStatus,
    pub lualatex: ToolStatus,
    pub tectonic: ToolStatus,
    pub bibtex: ToolStatus,
    pub biber: ToolStatus,
}

impl SystemTexReport {
    pub fn is_any_compiler_available(&self) -> bool {
        self.latexmk.available
            || self.pdflatex.available
            || self.xelatex.available
            || self.lualatex.available
            || self.tectonic.available
    }

    pub fn recommended_engine(&self) -> Option<CompilerEngine> {
        if self.latexmk.available {
            Some(CompilerEngine::Latexmk)
        } else if self.pdflatex.available {
            Some(CompilerEngine::PdfLatex)
        } else if self.lualatex.available {
            Some(CompilerEngine::LuaLatex)
        } else if self.xelatex.available {
            Some(CompilerEngine::XeLatex)
        } else if self.tectonic.available {
            Some(CompilerEngine::Tectonic)
        } else {
            None
        }
    }

    pub fn installation_instructions() -> &'static str {
        "Nenhum compilador LaTeX foi encontrado no sistema.\n\
         Para instalar uma distribuição completa no Linux:\n\
         - Arch/CachyOS: sudo pacman -S texlive-meta texlive-latexextra\n\
         - Ubuntu/Debian: sudo apt install texlive-latex-extra texlive-bibtex-extra latexmk\n\
         - Fedora: sudo dnf install texlive-scheme-medium latexmk"
    }
}

pub struct TexDetector;

impl TexDetector {
    pub fn detect_all() -> SystemTexReport {
        SystemTexReport {
            latexmk: Self::check_binary("latexmk", &["--version"]),
            pdflatex: Self::check_binary("pdflatex", &["--version"]),
            xelatex: Self::check_binary("xelatex", &["--version"]),
            lualatex: Self::check_binary("lualatex", &["--version"]),
            tectonic: Self::check_binary("tectonic", &["--version"]),
            bibtex: Self::check_binary("bibtex", &["--version"]),
            biber: Self::check_binary("biber", &["--version"]),
        }
    }

    fn check_binary(binary_name: &str, version_args: &[&str]) -> ToolStatus {
        let which_result = which_binary(binary_name);
        let available = which_result.is_some();
        let mut version = None;

        if let Some(ref path) = which_result {
            if let Ok(output) = Command::new(path).args(version_args).output() {
                if output.status.success() {
                    let text = String::from_utf8_lossy(&output.stdout);
                    if let Some(first_line) = text.lines().next() {
                        version = Some(first_line.trim().to_string());
                    }
                }
            }
        }

        ToolStatus {
            name: binary_name.to_string(),
            path: which_result,
            available,
            version,
        }
    }
}

fn which_binary(name: &str) -> Option<PathBuf> {
    #[allow(unused_mut)]
    let mut names = vec![name.to_string()];
    #[cfg(windows)]
    if !name.ends_with(".exe") {
        names.push(format!("{}.exe", name));
    }

    // 1. Check directory next to current executable
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            for n in &names {
                let candidate = exe_dir.join(n);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    // 2. Check PATH
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for n in &names {
                let candidate = dir.join(n);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    // Common fallback directories
    let home = std::env::var("HOME").unwrap_or_default();
    let fallbacks = [
        format!("{}/.local/bin", home),
        format!("{}/.cargo/bin", home),
        "/usr/local/bin".to_string(),
        "/usr/bin".to_string(),
        "/bin".to_string(),
    ];

    for dir in &fallbacks {
        for n in &names {
            let candidate = PathBuf::from(dir).join(n);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}
