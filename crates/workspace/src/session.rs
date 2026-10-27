use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectSession {
    pub current_file: Option<PathBuf>,
    #[serde(default)]
    pub collapsed_dirs: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalSession {
    pub last_project_dir: Option<PathBuf>,
    pub view_mode: Option<String>,
    pub window_width: i32,
    pub window_height: i32,
    pub pdf_view_mode: Option<String>,
    pub pdf_zoom: Option<f64>,
    #[serde(default)]
    pub projects: HashMap<PathBuf, ProjectSession>,
}

impl Default for GlobalSession {
    fn default() -> Self {
        Self {
            last_project_dir: None,
            view_mode: Some("split".to_string()),
            window_width: 1400,
            window_height: 880,
            pdf_view_mode: Some("continuous".to_string()),
            pdf_zoom: Some(1.0),
            projects: HashMap::new(),
        }
    }
}

pub struct SessionManager;

impl SessionManager {
    pub fn config_file() -> PathBuf {
        let config_dir = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .map(|h| PathBuf::from(h).join(".config"))
                    .unwrap_or_else(|| PathBuf::from("."))
            });
        config_dir.join("vibetex").join("session.json")
    }

    pub fn project_state_file(project_dir: &Path) -> PathBuf {
        project_dir.join(".vibetex").join("state.json")
    }

    pub fn load() -> GlobalSession {
        let path = Self::config_file();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(session) = serde_json::from_str::<GlobalSession>(&content) {
                    return session;
                }
            }
        }
        GlobalSession::default()
    }

    pub fn save(session: &GlobalSession) -> std::io::Result<()> {
        let path = Self::config_file();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(session)?;
        crate::ProjectManager::atomic_write(&path, json.as_bytes())
    }

    pub fn load_project_session(project_dir: &Path, global: &GlobalSession) -> ProjectSession {
        // 1. Try project-local .vibetex/state.json first
        let local_path = Self::project_state_file(project_dir);
        if local_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&local_path) {
                if let Ok(sess) = serde_json::from_str::<ProjectSession>(&content) {
                    return sess;
                }
            }
        }

        // 2. Fallback to global session map
        if let Some(sess) = global.projects.get(project_dir) {
            return sess.clone();
        }

        ProjectSession::default()
    }

    pub fn save_project_session(project_dir: &Path, session: &ProjectSession) -> std::io::Result<()> {
        let local_path = Self::project_state_file(project_dir);
        if let Some(parent) = local_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(session)?;
        crate::ProjectManager::atomic_write(&local_path, json.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_session_serde() {
        let mut session = ProjectSession::default();
        session.current_file = Some(PathBuf::from("/test/main.tex"));
        session.collapsed_dirs = vec![PathBuf::from("/test/capitulos"), PathBuf::from("/test/img")];

        let json = serde_json::to_string(&session).unwrap();
        let restored: ProjectSession = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.current_file, Some(PathBuf::from("/test/main.tex")));
        assert_eq!(restored.collapsed_dirs.len(), 2);
        assert!(restored.collapsed_dirs.contains(&PathBuf::from("/test/capitulos")));
    }

    #[test]
    fn test_global_session_serde() {
        let mut global = GlobalSession::default();
        global.last_project_dir = Some(PathBuf::from("/home/user/meu_projeto"));
        global.view_mode = Some("visual".to_string());
        global.pdf_view_mode = Some("single".to_string());
        global.pdf_zoom = Some(1.25);
        global.window_width = 1600;
        global.window_height = 900;

        let mut proj = ProjectSession::default();
        proj.current_file = Some(PathBuf::from("/home/user/meu_projeto/intro.tex"));
        global.projects.insert(PathBuf::from("/home/user/meu_projeto"), proj);

        let json = serde_json::to_string(&global).unwrap();
        let restored: GlobalSession = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.last_project_dir, Some(PathBuf::from("/home/user/meu_projeto")));
        assert_eq!(restored.view_mode.as_deref(), Some("visual"));
        assert_eq!(restored.pdf_view_mode.as_deref(), Some("single"));
        assert_eq!(restored.pdf_zoom, Some(1.25));
        assert_eq!(restored.window_width, 1600);
        assert_eq!(restored.window_height, 900);
        assert!(restored.projects.contains_key(&PathBuf::from("/home/user/meu_projeto")));
    }

    #[test]
    fn test_save_and_load_project_session() {
        let temp = tempfile::tempdir().unwrap();
        let project_dir = temp.path();

        let mut session = ProjectSession::default();
        session.current_file = Some(project_dir.join("capitulo1.tex"));
        session.collapsed_dirs = vec![project_dir.join("figuras"), project_dir.join("tabelas")];

        SessionManager::save_project_session(project_dir, &session).unwrap();

        let global = GlobalSession::default();
        let loaded = SessionManager::load_project_session(project_dir, &global);

        assert_eq!(loaded.current_file, Some(project_dir.join("capitulo1.tex")));
        assert_eq!(loaded.collapsed_dirs.len(), 2);
        assert!(loaded.collapsed_dirs.contains(&project_dir.join("figuras")));
        assert!(loaded.collapsed_dirs.contains(&project_dir.join("tabelas")));
    }
}

