//! The window's navigation guard: the app's own origin, and no other.

use aoide_desktop_app::navigation_allowed;

fn url(text: &str) -> tauri::Url {
    text.parse().expect("a url")
}

#[test]
fn the_guard_admits_the_apps_own_origin() {
    assert!(navigation_allowed(&url("tauri://localhost")), "the app's URL on Linux has no slash");
    assert!(navigation_allowed(&url("tauri://localhost/index.html")));
}

#[test]
fn the_guard_refuses_every_other_origin() {
    for text in ["tauri://localhostx", "https://localhost/", "http://tauri.localhost/", "file:///"] {
        assert!(!navigation_allowed(&url(text)), "{text} is refused");
    }
}
