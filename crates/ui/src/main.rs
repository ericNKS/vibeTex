mod app_window;

use adw::prelude::*;
use app_window::AppWindow;
use std::path::PathBuf;

fn main() -> glib::ExitCode {
    let app = adw::Application::builder()
        .application_id("org.vibetex.VibeTeX")
        .build();

    app.connect_activate(|app| {
        let args: Vec<String> = std::env::args().collect();
        let project_dir = if args.len() > 1 {
            PathBuf::from(&args[1])
        } else {
            let session = vibetex_workspace::SessionManager::load();
            if let Some(ref last_dir) = session.last_project_dir {
                if last_dir.is_dir() {
                    last_dir.clone()
                } else {
                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                }
            } else {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            }
        };

        let window = AppWindow::new(app, project_dir);
        window.show();
    });

    app.run()
}
