//! aoide-upkeep — mechanical integrity, the WORKING-tree half.
//!
//! `nix flake check` (`lib/checks.nix`'s `fmt`/`lint`/`discovery`) already
//! polices the COMMITTED tree — the git-filtered store copy `nix` evaluates.
//! It structurally cannot see anything gitignored or merely uncommitted:
//! `result`, `state/`, an agent's stray file dropped at repo root a moment
//! ago. That gap is this crate's whole charter. Its own command is `aoide
//! soundcheck` (`commands::register`), a human/agent-invoked report; its own
//! AUTOMATIC wiring is [`checklane`], invoked from `aoide-conduct`'s `session
//! hook` at SessionStart/Stop so the same gap gets flagged without anyone
//! having to remember to ask.
//!
//! **Report-only, forever — this is a binding correction, not a v0
//! shortcut.** Neither `soundcheck` nor [`checklane`] ever moves, deletes,
//! formats, or repairs anything; each only names a problem precisely enough
//! that a human or an agent can go fix it. See `commands`' module doc for the
//! finding format, `scan`'s for exactly which checks live here, and
//! [`checklane`]'s for the SessionStart/Stop split.
//!
//! One-package-one-charter (`docs/architecture/PACKAGE-LAYOUT.md`): repo
//! hygiene is not `aoide-storage`'s "durable session data" charter, so this
//! is its own small crate rather than a stretch of that one (soundcheck
//! design doc, FORK 3, advisor-confirmed).

pub mod checklane;
pub mod commands;
pub mod scan;

/// A crate-wide lock serialising every test that mutates process-global env
/// — mirrors every other domain crate's own `env_lock` (delegates to
/// `aoide-test-support`'s single mutex so a test in THIS crate's binary
/// never races one in a dev-dependency's own suite).
#[cfg(test)]
pub(crate) fn env_lock() -> &'static std::sync::Mutex<()> {
    aoide_test_support::env_lock()
}

/// The one symlink fixture the clutter tests plant: `symlink(2)` on Unix, and
/// on native Windows `symlink_file`/`symlink_dir`, which need
/// `SeCreateSymbolicLinkPrivilege` (Developer Mode, or an elevated token).
/// `false` means this host will not create one, so a caller SAYS SO and
/// returns rather than asserting against a fixture that does not exist — the
/// checks themselves read `symlink_metadata` and have no Unix-only piece, so
/// this is a fixture limit stated at the call site, never a capability hidden
/// behind a gate.
#[cfg(test)]
pub(crate) mod test_fixture {
    /// `true` when the link exists afterwards.
    #[cfg(unix)]
    pub fn plant_symlink(target: &str, link: &std::path::Path) -> bool {
        std::os::unix::fs::symlink(target, link).is_ok()
    }

    #[cfg(windows)]
    pub fn plant_symlink(target: &str, link: &std::path::Path) -> bool {
        std::os::windows::fs::symlink_file(target, link).is_ok()
            || std::os::windows::fs::symlink_dir(target, link).is_ok()
    }
}
