const COMMANDS: &[&str] = &["palette", "run", "sessions", "chat_open", "chat_say", "chat_close"];

fn main() {
    let capabilities = "./capabilities/main.json";
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .capabilities_path_pattern(capabilities)
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri-build writes the app's permissions");
}
