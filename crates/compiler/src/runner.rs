use crate::detector::TexDetector;
use crate::log_parser::{CompilationSummary, LogParser};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use thiserror::Error;
use vibetex_core::{CompilerEngine, Diagnostic, ProjectConfig};

#[derive(Debug, Error)]
pub enum CompilerError {
    #[error("Compilador não encontrado: {0}")]
    NotFound(String),
    #[error("Erro de E/S: {0}")]
    Io(#[from] std::io::Error),
    #[error("Tempo limite de compilação excedido ({0:?})")]
    Timeout(Duration),
    #[error("Diretório inválido: {0}")]
    InvalidPath(String),
}

pub struct CompilerRunner;

impl CompilerRunner {
    pub fn compile(
        project_dir: &Path,
        config: &ProjectConfig,
        _timeout: Duration,
    ) -> Result<CompilationSummary, CompilerError> {
        let report = TexDetector::detect_all();

        // Determine executable and effective engine
        let (binary_path, effective_engine) = match config.engine {
            CompilerEngine::Latexmk => {
                if let Some(ref path) = report.latexmk.path {
                    (path.clone(), CompilerEngine::Latexmk)
                } else if let Some(engine) = report.recommended_engine() {
                    let fallback_path = match engine {
                        CompilerEngine::PdfLatex => report.pdflatex.path.clone().unwrap(),
                        CompilerEngine::XeLatex => report.xelatex.path.clone().unwrap(),
                        CompilerEngine::LuaLatex => report.lualatex.path.clone().unwrap(),
                        CompilerEngine::Tectonic => report.tectonic.path.clone().unwrap(),
                        _ => {
                            return Ok(CompilationSummary {
                                success: false,
                                page_count: None,
                                diagnostics: vec![Diagnostic::error(crate::detector::SystemTexReport::installation_instructions())],
                                raw_log: "Nenhum compilador LaTeX instalado.".to_string(),
                            });
                        }
                    };
                    (fallback_path, engine)
                } else {
                    return Ok(CompilationSummary {
                        success: false,
                        page_count: None,
                        diagnostics: vec![Diagnostic::error(crate::detector::SystemTexReport::installation_instructions())],
                        raw_log: "Nenhum compilador LaTeX instalado.".to_string(),
                    });
                }
            }
            CompilerEngine::PdfLatex => (
                report.pdflatex.path.ok_or_else(|| CompilerError::NotFound("pdflatex".into()))?,
                CompilerEngine::PdfLatex,
            ),
            CompilerEngine::XeLatex => (
                report.xelatex.path.ok_or_else(|| CompilerError::NotFound("xelatex".into()))?,
                CompilerEngine::XeLatex,
            ),
            CompilerEngine::LuaLatex => (
                report.lualatex.path.ok_or_else(|| CompilerError::NotFound("lualatex".into()))?,
                CompilerEngine::LuaLatex,
            ),
            CompilerEngine::Tectonic => (
                report.tectonic.path.ok_or_else(|| CompilerError::NotFound("tectonic".into()))?,
                CompilerEngine::Tectonic,
            ),
        };

        let output_dir = project_dir.join(&config.output_dir);
        let _ = std::fs::create_dir_all(&output_dir);

        let mut cmd = Command::new(binary_path);
        cmd.current_dir(project_dir);

        let main_file_str = config.main_file.to_string_lossy();

        match effective_engine {
            CompilerEngine::Tectonic => {
                cmd.arg("-o");
                cmd.arg(&output_dir);
                cmd.arg("-k"); // --keep-intermediates: preserve cached aux and fonts for incremental compiling
                cmd.arg("-C"); // --only-cached: offline-first fast bundle usage
                cmd.arg("-c");
                cmd.arg("minimal");
                if config.synctex {
                    cmd.arg("--synctex");
                }
                cmd.arg(&*main_file_str);
            }
            CompilerEngine::Latexmk => {
                cmd.arg("-pdf");
                if config.synctex {
                    cmd.arg("-synctex=1");
                }
                cmd.arg("-interaction=nonstopmode");
                cmd.arg(format!("-output-directory={}", config.output_dir.display()));

                if config.shell_escape {
                    cmd.arg("-shell-escape");
                } else {
                    cmd.arg("-no-shell-escape");
                }
                cmd.arg(&*main_file_str);
            }
            CompilerEngine::PdfLatex | CompilerEngine::XeLatex | CompilerEngine::LuaLatex => {
                if config.synctex {
                    cmd.arg("-synctex=1");
                }
                cmd.arg("-interaction=nonstopmode");
                cmd.arg(format!("-output-directory={}", config.output_dir.display()));

                if config.shell_escape {
                    cmd.arg("-shell-escape");
                } else {
                    cmd.arg("-no-shell-escape");
                }
                cmd.arg(&*main_file_str);
            }
        }

        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output()?;
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        let combined_log = format!("{}\n{}", stdout_str, stderr_str);

        // Check if a .log file was produced in output_dir
        let log_file_name = config.main_file.with_extension("log");
        let log_path = output_dir.join(log_file_name);
        let raw_full_log = if log_path.exists() {
            std::fs::read_to_string(&log_path).unwrap_or(combined_log)
        } else {
            combined_log
        };

        // Filter out Fontconfig system warnings common on Arch/CachyOS
        let filtered_log: String = raw_full_log
            .lines()
            .filter(|line| !line.starts_with("Fontconfig warning:"))
            .collect::<Vec<_>>()
            .join("\n");

        let mut summary = LogParser::parse(&filtered_log);
        summary.raw_log = filtered_log;
        summary.success = output.status.success() && summary.success;

        Ok(summary)
    }

    /// Helper to find expected PDF path.
    pub fn get_pdf_path(project_dir: &Path, config: &ProjectConfig) -> PathBuf {
        let pdf_name = config.main_file.with_extension("pdf");
        let in_output = project_dir.join(&config.output_dir).join(&pdf_name);
        if in_output.exists() {
            in_output
        } else {
            project_dir.join(&pdf_name)
        }
    }
}
