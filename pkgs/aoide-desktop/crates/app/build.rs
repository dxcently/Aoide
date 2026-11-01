const COMMANDS: &[&str] = &["palette", "run", "sessions", "chat_open", "chat_say", "chat_close", "asks_open", "asks_close", "smoke_note"];

fn main() {
    // A smoke build reads the smoke capability too. No other build does.
    let capabilities = if std::env::var_os("CARGO_FEATURE_SMOKE").is_some() {
        "./capabilities/*.json"
    } else {
        "./capabilities/main.json"
    };
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .capabilities_path_pattern(capabilities)
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri-build writes the app's permissions");
}
