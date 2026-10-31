//! The command ACL under tauri's mock runtime, with the app's real context: the
//! capabilities and permissions the build generated are loaded from the
//! config, and each request names the window it came from.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use aoide_desktop_app::{handler, Confirm, Window};
use aoide_desktop_shell::exec::Exec;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;
use tauri::{WebviewUrl, WebviewWindowBuilder};

struct Refuse;

impl Confirm for Refuse {
    fn confirm(&self, _verb: &str, _text: &str) -> bool {
        false
    }
}

fn fake_aoide() -> PathBuf {
    let exe = std::env::current_exe().expect("the test binary has a path");
    exe.parent().and_then(Path::parent).expect("target/<profile>").join("fake-aoide")
}

fn window(tag: &str) -> (Window, PathBuf) {
    let dir = std::env::temp_dir().join(format!("aoide-app-acl-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a scratch dir is made");
    let record = dir.join("record.jsonl");
    let env = vec![
        ("PATH".into(), "/usr/bin:/bin".into()),
        ("FAKE_AOIDE_RECORD".into(), record.clone().into_os_string()),
    ];
    let exec = Exec::load(&fake_aoide(), &dir.join("home"), env).expect("the fake loads");
    (Window::new(exec, Box::new(Refuse)), record)
}

fn sessions_calls(record: &Path) -> usize {
    fs::read_to_string(record).unwrap_or_default().lines().filter(|l| l.contains("\"session\"")).count()
}

/// A data directory for one window, under the test's temp dir: the webview's
/// default lies under the home directory, which the test does not own.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aoide-app-acl-data-{tag}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("a data directory is made");
    dir
}

fn request(cmd: &str, key: &str) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.to_owned(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().expect("a url"),
        body: InvokeBody::Json(serde_json::json!({})),
        headers: Default::default(),
        invoke_key: key.to_owned(),
    }
}

#[test]
fn a_command_passes_as_main_and_another_label_is_refused() {
    let (window, _record) = window("label");
    let app = mock_builder()
        .manage(Arc::new(window))
        .invoke_handler(handler())
        .build(tauri::generate_context!())
        .expect("the mock app builds");
    let key = app.invoke_key().to_owned();
    let main = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
        .data_directory(scratch("label-main"))
        .build()
        .expect("the main window builds");
    let other = WebviewWindowBuilder::new(&app, "other", WebviewUrl::App("index.html".into()))
        .data_directory(scratch("label-other"))
        .build()
        .expect("the other window builds");
    let passed = get_ipc_response(&main, request("palette", &key));
    assert!(passed.is_ok(), "palette passes as main: {passed:?}");
    let refused = get_ipc_response(&other, request("palette", &key));
    assert!(refused.is_err(), "palette is refused from another label");
}

#[test]
fn a_wrong_invoke_key_never_reaches_the_handler() {
    let (window, record) = window("key");
    let app = mock_builder()
        .manage(Arc::new(window))
        .invoke_handler(handler())
        .build(tauri::generate_context!())
        .expect("the mock app builds");
    let main = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
        .data_directory(scratch("key-main"))
        .build()
        .expect("the main window builds");
    let _ = std::thread::spawn(move || {
        let _ = get_ipc_response(&main, request("sessions", "wrong-key"));
    });
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(sessions_calls(&record), 0, "the handler ran zero times for a wrong key");
}

#[cfg(not(feature = "smoke"))]
#[test]
fn smoke_note_is_refused_as_main_in_a_build_without_the_smoke_feature() {
    let (window, _record) = window("smoke-note");
    let app = mock_builder()
        .manage(Arc::new(window))
        .invoke_handler(handler())
        .build(tauri::generate_context!())
        .expect("the mock app builds");
    let key = app.invoke_key().to_owned();
    let main = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
        .data_directory(scratch("smoke-note-main"))
        .build()
        .expect("the main window builds");
    let refused = get_ipc_response(&main, request("smoke_note", &key));
    assert!(refused.is_err(), "smoke_note is refused as main without the feature");
}

/// A smoke build grants the note to main, and the handler runs. No smoke
/// directory is set in a test, so the handler answers that it is not a smoke run.
#[cfg(feature = "smoke")]
#[test]
fn smoke_note_passes_as_main_in_a_smoke_build() {
    let (window, _record) = window("smoke-note-on");
    let app = mock_builder()
        .manage(Arc::new(window))
        .invoke_handler(handler())
        .build(tauri::generate_context!())
        .expect("the mock app builds");
    let key = app.invoke_key().to_owned();
    let main = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
        .data_directory(scratch("smoke-note-on-main"))
        .build()
        .expect("the main window builds");
    let mut note = request("smoke_note", &key);
    note.body = InvokeBody::Json(serde_json::json!({"note": "a note"}));
    let reached = get_ipc_response(&main, note);
    let text = format!("{reached:?}");
    assert!(text.contains("not a smoke run"), "smoke_note reaches its handler as main: {text}");
}
