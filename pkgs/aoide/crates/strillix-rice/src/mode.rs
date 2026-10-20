//! `stage/mode.json` — whether the rice system is in STAGING mode (hot-load
//! unlocked: `rice stage`/`wallpaper set` write live), DECLARATIVE mode (nix/
//! home-manager is the only writer; staging writers refuse), or DRAFT mode
//! (`stage/theme.json` is a symlink routed into a saved
//! `rices/<rice>/drafts/<name>/theme.json` — every writer that ever
//! touches the stage lands directly in the draft file, transparently, via
//! [`crate::fs::atomic_write`]'s symlink-resolving rename) — Self-Ricing's
//! mode-toggle extension (khoa, 2026-08-14; draft mode, khoa, 2026-08-14).
//!
//! Absent file = `declarative` (the safe default: nothing has ever unlocked
//! staging writes, so nothing should silently be mutable — the same "an
//! absent stage file means the safe default, never an error" discipline
//! this module's own [`load_mode_marker`] follows).
//!
//! `Draft` is a distinct THIRD mode, not a flag riding on top of `Staging` —
//! routing, not guessing: entering it (`rice mode draft <name>`,
//! `crates/strillix-rice/src/commands/mode.rs`) is the only way to reach a saved
//! draft at all. `rice stage`/`rice mode stage` never auto-prefer or
//! auto-detect a draft; they always mean plain declared content, and are
//! simply unaware that the file they write might currently be routed
//! elsewhere (that unawareness, plus `atomic_write`'s symlink transparency,
//! is the entire mechanism).

use aoide_storage::fs::atomic_write;
use crate::paths::stage_dir;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiceMode {
    Staging,
    Declarative,
    Draft,
}

impl Default for RiceMode {
    fn default() -> Self {
        RiceMode::Declarative
    }
}

/// The active mode marker. `rice` names what a `staging`- or `draft`-mode
/// session is pointed at (absent in the common
/// `declarative`-with-no-marker-file case). `draft` is `Some(name)`
/// **if and only if** `mode == Draft` — every other mode always carries
/// `draft: None`; nothing in the codebase ever sets one without the other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModeMarker {
    #[serde(default)]
    pub mode: RiceMode,
    #[serde(default, rename = "rice", skip_serializing_if = "Option::is_none")]
    pub rice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<String>,
    /// The last rice actively used in `Staging` mode — distinct from `rice`
    /// (the CURRENTLY active rice, which `Declarative` locking legitimately
    /// overwrites). Locking declarative must never touch or clear this
    /// field: it is the only durable memory of "what was I staging" that
    /// survives a declarative round-trip, so a bare `rice mode stage` (no
    /// explicit name — the CLI default, never the RICE menu, which names its rice) can resolve back to it
    /// instead of re-reading `stage/theme.json`'s own `"rice"` field (which
    /// `Declarative` just overwrote to something else). Absent on a fresh
    /// marker or one written before this field existed — callers fall back
    /// to `current_staged_rice()` in that cold-start case.
    #[serde(default, rename = "stagingRice", skip_serializing_if = "Option::is_none")]
    pub staging_rice: Option<String>,
    /// ISO-8601 UTC (`aoide_storage::time::now_iso_utc`) — when this mode was
    /// entered. Empty string on the zero-value `Default` (no marker ever
    /// written), never fabricated.
    #[serde(default)]
    pub since: String,
}

/// The marker path: `stage/mode.json`.
pub fn mode_marker_path() -> std::path::PathBuf {
    stage_dir().join("mode.json")
}

/// The system's CURRENT effective mode. An absent or corrupt marker resolves
/// to the zero-value `declarative` default — never an error (CONTRACTS.md §4
/// additive discipline).
pub fn load_mode_marker() -> ModeMarker {
    std::fs::read_to_string(mode_marker_path())
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// `stage/mode.json`'s format version (CONTRACTS.md §4). Stamped on every save;
/// no reader gates on it (absent or older still loads as the same marker).
pub const MODE_SCHEMA_VERSION: &str = "1";

/// Atomic-write the marker to `stage/mode.json`.
pub fn save_mode_marker(marker: &ModeMarker) -> Result<(), String> {
    let mut value = serde_json::to_value(marker).map_err(|e| format!("serialize mode.json: {e}"))?;
    value["schemaVersion"] = serde_json::json!(MODE_SCHEMA_VERSION);
    let body = serde_json::to_string_pretty(&value)
        .map_err(|e| format!("serialize mode.json: {e}"))?
        + "\n";
    let path = mode_marker_path();
    atomic_write(&path, &body).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn default_marker_is_declarative_with_no_rice_or_draft() {
        let m = ModeMarker::default();
        assert_eq!(m.mode, RiceMode::Declarative);
        assert_eq!(m.rice, None);
        assert_eq!(m.draft, None);
        assert_eq!(m.since, "");
    }

    #[test]
    fn a_saved_marker_is_stamped_with_its_format_version() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _pin = aoide_test_support::EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let dir = std::env::temp_dir().join(format!("aoide-mode-stamp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        aoide_test_support::pin_stage(dir.join("stage"));
        let marker = ModeMarker { mode: RiceMode::Staging, rice: Some("sonata".into()), ..Default::default() };
        save_mode_marker(&marker).unwrap();
        let raw = std::fs::read_to_string(mode_marker_path()).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["schemaVersion"], MODE_SCHEMA_VERSION);
        assert_eq!(v["rice"], "sonata");
        assert_eq!(load_mode_marker(), marker, "the stamp does not change what loads");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_marker_file_loads_as_the_declarative_default() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _pin = aoide_test_support::EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let dir = std::env::temp_dir().join(format!("aoide-mode-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        aoide_test_support::pin_stage(dir.join("stage"));

        let m = load_mode_marker();
        assert_eq!(m, ModeMarker::default());
        assert!(!mode_marker_path().exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_load_round_trips_through_a_temp_stage_dir() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _pin = aoide_test_support::EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let dir = std::env::temp_dir().join(format!("aoide-mode-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        aoide_test_support::pin_stage(dir.join("stage"));

        let marker = ModeMarker {
            mode: RiceMode::Staging,
            rice: Some("moonlight".to_string()),
            draft: None,
            staging_rice: None,
            since: "2026-08-14T00:00:00Z".to_string(),
        };
        save_mode_marker(&marker).unwrap();
        let back = load_mode_marker();
        assert_eq!(back, marker);

        let raw = std::fs::read_to_string(mode_marker_path()).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["mode"], "staging");
        assert_eq!(v["rice"], "moonlight");
        assert!(v.get("draft").is_none(), "draft omitted when None");
        assert!(v.get("stagingRice").is_none(), "stagingRice omitted when None");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn draft_mode_round_trips_with_a_draft_name_and_serializes_lowercase() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _pin = aoide_test_support::EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let dir = std::env::temp_dir().join(format!("aoide-mode-draft-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        aoide_test_support::pin_stage(dir.join("stage"));

        let marker = ModeMarker {
            mode: RiceMode::Draft,
            rice: Some("sonata".to_string()),
            draft: Some("neon-night".to_string()),
            staging_rice: Some("sonata".to_string()),
            since: "2026-08-14T00:00:00Z".to_string(),
        };
        save_mode_marker(&marker).unwrap();
        let back = load_mode_marker();
        assert_eq!(back, marker);

        let raw = std::fs::read_to_string(mode_marker_path()).unwrap();
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["mode"], "draft");
        assert_eq!(v["draft"], "neon-night");
        assert_eq!(v["stagingRice"], "sonata");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_marker_file_loads_as_the_declarative_default() {
        let _g = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _pin = aoide_test_support::EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_ROOT", "STRILLIX_ROOT"]);
        let dir = std::env::temp_dir().join(format!("aoide-mode-corrupt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("stage")).unwrap();
        aoide_test_support::pin_stage(dir.join("stage"));

        std::fs::write(mode_marker_path(), "not json").unwrap();
        let m = load_mode_marker();
        assert_eq!(m, ModeMarker::default());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
