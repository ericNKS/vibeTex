use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use vibetex_core::ProjectConfig;
use walkdir::WalkDir;

pub struct ProjectManager {
    root: PathBuf,
    config: ProjectConfig,
}

impl ProjectManager {
    pub fn open(root: impl AsRef<Path>) -> std::io::Result<Self> {
        let root = root.as_ref().to_path_buf();
        let config_path = root.join("vibetex.json");

        let config = if config_path.exists() {
            let data = std::fs::read_to_string(&config_path)?;
            serde_json::from_str(&data).unwrap_or_else(|_| Self::default_config_for(&root))
        } else {
            Self::default_config_for(&root)
        };

        Ok(Self { root, config })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config(&self) -> &ProjectConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut ProjectConfig {
        &mut self.config
    }

    pub fn save_config(&self) -> std::io::Result<()> {
        let config_path = self.root.join("vibetex.json");
        let json = serde_json::to_string_pretty(&self.config)?;
        Self::atomic_write(&config_path, json.as_bytes())
    }

    /// Atomically writes content to a file: creates a temp file in the same directory, flushes, then renames.
    pub fn atomic_write(target_path: &Path, content: &[u8]) -> std::io::Result<()> {
        let parent = target_path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;

        let mut temp = NamedTempFile::new_in(parent)?;
        temp.write_all(content)?;
        temp.flush()?;
        temp.persist(target_path).map_err(|e| e.error)?;
        Ok(())
    }

    /// Saves a recovery file `.filename.recovery~` for crash protection.
    pub fn save_recovery(file_path: &Path, content: &str) -> std::io::Result<()> {
        if let Some(parent) = file_path.parent() {
            let file_name = file_path.file_name().unwrap_or_default().to_string_lossy();
            let recovery_name = format!(".{}.recovery~", file_name);
            let recovery_path = parent.join(recovery_name);
            std::fs::write(recovery_path, content)?;
        }
        Ok(())
    }

    /// Automatically detects the main .tex file in the project.
    pub fn detect_main_file(root: &Path) -> PathBuf {
        let mut candidates = Vec::new();

        for entry in WalkDir::new(root).max_depth(3) {
            if let Ok(entry) = entry {
                let p = entry.path();
                if p.is_file() && p.extension().map(|e| e == "tex").unwrap_or(false) {
                    if let Ok(content) = std::fs::read_to_string(p) {
                        if content.contains(r"\documentclass") && content.contains(r"\begin{document}") {
                            let rel = p.strip_prefix(root).unwrap_or(p).to_path_buf();
                            // Strongly prefer "main.tex" if available
                            if rel.file_name().map(|n| n == "main.tex").unwrap_or(false) {
                                return rel;
                            }
                            candidates.push(rel);
                        }
                    }
                }
            }
        }

        candidates.into_iter().next().unwrap_or_else(|| PathBuf::from("main.tex"))
    }

    fn default_config_for(root: &Path) -> ProjectConfig {
        let project_name = root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Projeto LaTeX".to_string());
        let main_file = Self::detect_main_file(root);
        ProjectConfig::new(project_name, main_file)
    }
}
