pub mod commands;
pub mod confirm;
pub mod window;
pub mod wire;

use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;

pub use confirm::{Confirm, Gtk};
pub use wire::{Push, Sink};
pub use window::Window;

use aoide_desktop_shell::exec::Exec;
use tauri::Manager;

/// The commands the main window is granted, in the order the handler lists them.
pub fn handler<R: tauri::Runtime>() -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        commands::palette,
        commands::run,
        commands::sessions,
        commands::chat_open,
        commands::chat_say,
        commands::chat_close,
    ]
}

/// Starts the app: refuses any argument, pins the program before GTK starts,
/// then opens the one window.
pub fn run() {
    if std::env::args_os().len() > 1 {
        eprintln!("aoide-desktop takes no arguments");
        std::process::exit(2);
    }
    let base_env: Vec<(OsString, OsString)> = std::env::vars_os().collect();
    let home = base_env
        .iter()
        .find(|(name, _)| name == "HOME")
        .map(|(_, value)| value.clone())
        .unwrap_or_default();
    let exec = match Exec::load(Path::new(env!("AOIDE_DESKTOP_AOIDE")), Path::new(&home), base_env) {
        Ok(exec) => exec,
        Err(e) => {
            eprintln!("aoide-desktop: the pinned aoide does not load: {e}");
            std::process::exit(1);
        }
    };
    let mut exec = Some(exec);
    tauri::Builder::default()
        .invoke_handler(handler())
        .setup(move |app| {
            let exec = exec.take().expect("setup runs once");
            let confirm = Box::new(Gtk::new(app.handle().clone()));
            app.manage(Arc::new(Window::new(exec, confirm)));
            tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::App("index.html".into()))
                .incognito(true)
                .devtools(false)
                .on_navigation(|url| url.as_str().starts_with("tauri://localhost/"))
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("the app runs");
}
