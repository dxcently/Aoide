pub mod commands;
pub mod confirm;
#[cfg(feature = "smoke")]
pub mod smoke;
pub mod window;
pub mod wire;

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use confirm::{Confirm, Gtk};
pub use wire::{Push, Sink};
pub use window::Window;

use aoide_desktop_shell::exec::untrusted::visible;
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
        #[cfg(feature = "smoke")]
        commands::smoke_note,
    ]
}

/// Whether the window may navigate to `url`: the app's own origin and no other.
/// Linux's app URL is `tauri://localhost` with no slash, and a non-special
/// scheme's origin is opaque, so the scheme and the host are what is compared.
pub fn navigation_allowed(url: &tauri::Url) -> bool {
    url.scheme() == "tauri" && url.host_str() == Some("localhost")
}

/// The main window, as the app opens it: on `index.html`, in no profile that
/// outlives it, with no devtools, held to its own origin.
pub fn main_window<'a, R: tauri::Runtime, M: Manager<R>>(manager: &'a M) -> tauri::WebviewWindowBuilder<'a, R, M> {
    tauri::WebviewWindowBuilder::new(manager, "main", tauri::WebviewUrl::App("index.html".into()))
        .title("Aoide")
        .incognito(true)
        .devtools(false)
        .on_navigation(navigation_allowed)
}

/// The smoke directory the arguments name. A build without the smoke feature
/// takes no argument, and refuses the one it is given by name.
fn arguments(args: &[OsString]) -> Result<Option<PathBuf>, String> {
    match args {
        [] => Ok(None),
        #[cfg(feature = "smoke")]
        [flag, dir] if flag.to_str() == Some("--smoke") => Ok(Some(PathBuf::from(dir))),
        [flag, ..] => Err(format!("refused argument `{}`: aoide-desktop takes no arguments", visible(&flag.to_string_lossy()))),
    }
}

/// Starts the app: refuses any argument it does not take, pins the program
/// before GTK starts, then opens the one window.
pub fn run() {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let smoke_dir = match arguments(&args) {
        Ok(dir) => dir,
        Err(text) => {
            eprintln!("aoide-desktop: {text}");
            std::process::exit(2);
        }
    };
    #[cfg(feature = "smoke")]
    if let Some(dir) = smoke_dir {
        if let Err(text) = smoke::begin(dir) {
            eprintln!("aoide-desktop: {text}");
            std::process::exit(2);
        }
    }
    #[cfg(not(feature = "smoke"))]
    let _ = smoke_dir;
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
            #[cfg(feature = "smoke")]
            let confirm: Box<dyn Confirm> = Box::new(smoke::Scenario);
            #[cfg(not(feature = "smoke"))]
            let confirm: Box<dyn Confirm> = Box::new(Gtk::new(app.handle().clone()));
            app.manage(Arc::new(Window::new(exec, confirm)));
            let builder = main_window(app);
            #[cfg(feature = "smoke")]
            let builder = builder.on_page_load(|webview, payload| {
                let finished = payload.event() == tauri::webview::PageLoadEvent::Finished;
                eprintln!("aoide-desktop: page load {}", if finished { "finished" } else { "started" });
                if finished {
                    if let Err(text) = smoke::ready() {
                        eprintln!("aoide-desktop: {text}");
                    }
                    if let Ok(title) = webview.title() {
                        let _ = smoke::note(&format!("title {title}"));
                    }
                    if let Err(e) = webview.eval(smoke::probe()) {
                        eprintln!("aoide-desktop: the probe did not run: {e}");
                    }
                }
            });
            builder.build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("the app runs");
}
