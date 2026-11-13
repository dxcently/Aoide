//! The screen family's durable state paths: the capture directory and the
//! saved pointer position, both under `$STRILLIX_ROOT/state/`. Only the screen
//! commands read or write them.

use strillix_rice::paths::strillix_root;

/// Screen-capture artifacts: `$STRILLIX_ROOT/state/captures/` (`strx screen
/// shot`, PACKAGE-LAYOUT.md Phase-1 `screen` command family).
///
/// A capture is a durable artifact its caller keeps, never rice-scoped and
/// never reset with a stage.
pub fn captures_dir() -> std::path::PathBuf {
    strillix_root().join("state").join("captures")
}

/// Saved pointer position: `$STRILLIX_ROOT/state/pointer-pos.json` (`strx
/// screen point save`/`restore`, Phase 2 of the `screen` command family).
///
/// Beside [`captures_dir`]: a saved cursor position outlives the invocation
/// that saved it (`save` in one process, `restore` in a later one). A single
/// flat file, because only one "current" position exists.
pub fn pointer_state_file() -> std::path::PathBuf {
    strillix_root().join("state").join("pointer-pos.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::EnvSaver;

    /// A scratch dir that is absolute on this host, unique to this test binary.
    fn scratch(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("strillix-screen-paths-{name}-{}", std::process::id()))
    }

    #[test]
    fn captures_and_the_saved_pointer_follow_the_strillix_root_alone() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvSaver::capture(&["STRILLIX_ROOT", "AOIDE_ROOT", "AOIDE_STATE_DIR"]);
        let strillix = scratch("strillix");
        std::env::set_var("STRILLIX_ROOT", &strillix);
        std::env::set_var("AOIDE_ROOT", scratch("core"));
        std::env::set_var("AOIDE_STATE_DIR", scratch("core-state"));
        assert_eq!(captures_dir(), strillix.join("state").join("captures"));
        assert_eq!(pointer_state_file(), strillix.join("state").join("pointer-pos.json"));
    }
}
