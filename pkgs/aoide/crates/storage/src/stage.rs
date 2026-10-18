//! Stage-file I/O (missing file → empty registry; writes atomic) and the
//! four CONDUCTING stage-file paths.
//!
//! Moved from `graph/model.rs` (Phase 3a restructure,
//! docs/architecture/PACKAGE-LAYOUT.md); re-exported at the old path so every
//! existing `crate::graph::{load_stage, write_stage, sessions_path, …}`
//! caller is untouched.
//!
//! `projects_path`/`graph_path` were `pub(in crate::graph)` in root — now
//! crossing a crate boundary they must be `pub` here; the root shim narrows
//! their re-export back to `pub(in crate::graph)` so the original visibility
//! contract at the root crate boundary is unchanged.
//!
//! **`state/stage/`, not `rice/stage/` (command-defrag lane S1,
//! 2026-08-27).** These four files are core orchestration state — CONTRACTS.md
//! §4 — so they resolve through [`crate::fs::conducting_stage_dir`], not
//! [`crate::fs::stage_dir`] (which still means the rice/paint stage tree).
//! `pending.json`/`notifications.json`, the other two files in the same broker-owned
//! roster, live in `aoide-conduct` instead (`graph::pending_path`,
//! `notifications::notifications_path`) and made the identical switch.

use crate::fs::{atomic_write, conducting_stage_dir};
use serde::Serialize;

pub fn projects_path() -> std::path::PathBuf {
    conducting_stage_dir().join("projects.json")
}
pub fn sessions_path() -> std::path::PathBuf {
    conducting_stage_dir().join("sessions.json")
}
pub fn hooks_path() -> std::path::PathBuf {
    conducting_stage_dir().join("hooks.json")
}
pub fn graph_path() -> std::path::PathBuf {
    conducting_stage_dir().join("graph.json")
}

/// Load one stage file; a missing file is an empty registry (tolerated), a
/// corrupt one is a structured error string.
pub fn load_stage<T: serde::de::DeserializeOwned + Default>(
    path: &std::path::Path,
) -> Result<T, String> {
    match std::fs::read_to_string(path) {
        Ok(s) => serde_json::from_str(&s)
            .map_err(|e| format!("{}: unreadable stage file: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Atomic write of one stage file (write-temp-then-rename, CONTRACTS.md §4).
pub fn write_stage<T: Serialize>(path: &std::path::Path, value: &T) -> Result<(), String> {
    let body = serde_json::to_string_pretty(value)
        .map_err(|e| format!("{}: serialize: {e}", path.display()))?;
    atomic_write(path, &body).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::records::{RestoreSnapshot, SessionsFile};

    /// A `sessions.json` whose `restore` still carries a `typed` key loads with
    /// the key dropped, and the real writer never puts it back on disk.
    #[test]
    fn a_legacy_typed_key_in_sessions_json_loads_and_is_never_written_back() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir()
            .join(format!("aoide-stage-legacy-typed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let _env = aoide_test_support::EnvSaver::capture(&["AOIDE_STAGE_DIR"]);
        std::env::set_var("AOIDE_STAGE_DIR", &dir);

        let path = sessions_path();
        std::fs::write(
            &path,
            r#"{ "schemaVersion": "0", "sessions": [ { "sessionId": "s", "restore": { "cwd": "/w", "idle": true, "argv": null, "typed": "cargo test" } } ] }"#,
        )
        .unwrap();

        let loaded: SessionsFile = load_stage(&path).unwrap();
        assert_eq!(
            loaded.sessions[0].restore,
            Some(RestoreSnapshot { cwd: Some("/w".into()), idle: true, argv: None })
        );
        write_stage(&path, &loaded).unwrap();

        let on_disk = std::fs::read_to_string(&path).unwrap();
        assert!(!on_disk.contains("typed"), "on disk: {on_disk}");
        assert!(!on_disk.contains("cargo test"), "on disk: {on_disk}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
