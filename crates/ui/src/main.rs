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
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };

        let window = AppWindow::new(app, project_dir);
        window.show();
    });

    app.run()
}
