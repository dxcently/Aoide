//! The screen family's durable state paths: the capture directory and the
//! saved pointer position, both under `$AOIDE_ROOT/state/`. Only the screen
//! commands read or write them.

use aoide_storage::fs::state_dir;

/// Screen-capture artifacts: `$AOIDE_ROOT/state/captures/` (`aoide screen shot`,
/// PACKAGE-LAYOUT.md Phase-1 `screen` command family).
///
/// Under [`state_dir`]: a capture is a durable artifact its caller keeps, never
/// rice-scoped and never reset with a stage.
pub fn captures_dir() -> std::path::PathBuf {
    state_dir().join("captures")
}

/// Saved pointer position: `$AOIDE_ROOT/state/pointer-pos.json` (`aoide screen
/// point save`/`restore`, Phase 2 of the `screen` command family).
///
/// Under [`state_dir`], like [`captures_dir`]: a saved cursor position outlives
/// the invocation that saved it (`save` in one process, `restore` in a later
/// one). A single flat file, because only one "current" position exists.
pub fn pointer_state_file() -> std::path::PathBuf {
    state_dir().join("pointer-pos.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_test_support::{pin_roots, EnvSaver};

    /// A scratch dir that is absolute on this host, unique to this test binary.
    fn scratch(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("aoide-screen-paths-{name}-{}", std::process::id()))
    }

    #[test]
    fn captures_nest_under_the_state_dir_and_honor_its_override() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = scratch("captures-root");
        let _env = pin_roots(&root);
        assert_eq!(captures_dir(), root.join("state").join("captures"));

        let state = scratch("captures-state");
        let _state = EnvSaver::capture(&["AOIDE_STATE_DIR"]);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        assert_eq!(captures_dir(), state.join("captures"));
    }

    #[test]
    fn the_pointer_state_file_nests_under_the_state_dir_and_honors_its_override() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let root = scratch("pointer-root");
        let _env = pin_roots(&root);
        assert_eq!(pointer_state_file(), root.join("state").join("pointer-pos.json"));

        let state = scratch("pointer-state");
        let _state = EnvSaver::capture(&["AOIDE_STATE_DIR"]);
        std::env::set_var("AOIDE_STATE_DIR", &state);
        assert_eq!(pointer_state_file(), state.join("pointer-pos.json"));
    }
}
