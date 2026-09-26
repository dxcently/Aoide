//! The per-user runtime directory every aoide socket on this host lives in —
//! ONE resolution, because eight call sites used to spell it themselves
//! (`tunnel::record_path`, `attest::daemon_socket_path`, `aoide-conduct`'s
//! conductor-control, channel and shellbridge sockets, `client::pair_watch`'s
//! marker dir, `server::daemon`'s socket path, and lyra's preview). One of
//! them already disagreed: a bare `unwrap_or_else("/run/user/1000")` with no
//! non-empty filter turned an EMPTY `XDG_RUNTIME_DIR` into a relative
//! `aoide/…` path.
//!
//! **This is a PURE resolution — a path, never a side effect.** Nothing here
//! creates, chmods or probes anything: `tunnel::list_records`' own tests
//! depend on "the `aoide/` subdirectory does not exist until a writer makes
//! it", and a resolver that created it would be lying to every caller about
//! what it did. Creating `<socket_dir>` is the job of whoever is about to
//! write or bind there — and on the host that does not provide the directory
//! that call is `aoide_protocol::owner_only::ensure_private_dir`, the
//! protected owner-only policy this module's own native test attaches and
//! reads back.
//!
//! **`$XDG_RUNTIME_DIR` is the override on BOTH hosts** — non-empty wins, on
//! native Windows too. That is not decoration: every test fixture that
//! isolates a runtime dir (storage's `with_temp_runtime_dir`, conduct's
//! `EnvVars`, this module's own) sets exactly that variable, and an answer
//! that ignored it on one host would quietly write into — and read from — the
//! machine's real per-user directory instead of the fixture's.
//!
//! **Unix default**: `/run/user/<this process's euid>`. The literal
//! `/run/user/1000` this convention shipped with is the one hard-coded uid in
//! it — identical for the usual uid 1000, wrong for everybody else, and now
//! derived from the process that is asking. Nothing else about the Unix
//! answer changes, and the directory itself stays the session's own
//! (`$XDG_RUNTIME_DIR` is 0700, `/run/user/<uid>` likewise).
//!
//! **Native Windows default**: `%LOCALAPPDATA%`. This host has no XDG runtime
//! dir, and `%LOCALAPPDATA%` is its per-user, non-roaming local store, so the
//! same shape lands under the same kind of place. The host provides neither
//! that directory nor an owner-only policy on a newly created one (an elevated
//! token's default owner is `BUILTIN\Administrators`), so a native socket
//! binder creates it through
//! [`aoide_protocol::owner_only::ensure_private_dir`] — the contract this
//! module's test on that host asserts.
use std::path::PathBuf;

/// `<runtime root>/aoide` — the directory an aoide socket is bound into, and
/// the one authority for it on either host.
pub fn socket_dir() -> PathBuf {
    root().join("aoide")
}

/// The runtime ROOT: `$XDG_RUNTIME_DIR` on either host, else this host's own
/// per-user answer (`/run/user/<euid>` on Unix, `%LOCALAPPDATA%` — falling
/// back to the temp dir when that variable is somehow absent — on native
/// Windows). Private: [`socket_dir`] is the one spelling callers use, so no
/// caller can end up outside the `aoide` segment this convention owns.
fn root() -> PathBuf {
    if let Some(dir) = std::env::var("XDG_RUNTIME_DIR").ok().filter(|s| !s.is_empty()) {
        return PathBuf::from(dir);
    }
    host_default()
}

#[cfg(unix)]
fn host_default() -> PathBuf {
    PathBuf::from(format!("/run/user/{}", euid()))
}

#[cfg(windows)]
fn host_default() -> PathBuf {
    std::env::var("LOCALAPPDATA")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// This process's effective uid. `std` has no `geteuid(2)` wrapper and the
/// call takes no arguments and cannot fail, so this is the whole of it — kept
/// private to this module because the uid appears in exactly one answer here
/// (`/run/user/<uid>`), the same way `aoide-secrets`' home keeps its own.
#[cfg(unix)]
fn euid() -> u32 {
    // SAFETY: `geteuid(2)` takes no arguments, cannot fail, and touches no
    // memory this process does not already own.
    unsafe { libc::geteuid() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Native Windows, where the DEFAULT (no override) is the interesting
    /// half: this host's own per-user store, and a resolution that creates
    /// nothing. The override half is asserted on Unix below, and is what every
    /// isolating fixture on either host relies on.
    #[cfg(windows)]
    #[test]
    fn the_native_default_is_the_per_user_store_and_resolving_creates_nothing() {
        let live = socket_dir();
        assert!(
            live.starts_with(host_default()) && live.ends_with("aoide"),
            "the runtime dir's default must sit under this host's per-user store: {}",
            live.display()
        );

        let scratch = std::env::temp_dir().join(format!("aoide-runtime-probe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch).unwrap();
        std::env::set_var("XDG_RUNTIME_DIR", &scratch);
        assert_eq!(socket_dir(), scratch.join("aoide"), "the override wins on this host too");
        assert!(!socket_dir().exists(), "resolving a path must not create one");
        std::env::remove_var("XDG_RUNTIME_DIR");

        // ...and the policy a binder must attach there, on a directory this
        // test owns (never on the live one: storage's `tunnel` tests assert the
        // live `<socket_dir>` does not exist until a writer makes it, and a
        // test that created it would be littering the operator's real store).
        let policy = scratch.join("policy");
        aoide_protocol::owner_only::ensure_private_dir(&policy).expect("create it owner-only");
        assert_eq!(
            aoide_protocol::owner_only::dir_privacy(&policy).expect("read the policy back"),
            None,
            "a runtime directory made through ensure_private_dir must read back owner-only"
        );
        std::fs::remove_dir_all(&scratch).ok();
    }

    /// The Unix answer is a CONTRACT (eight call sites and their own tests
    /// assert paths under it), so it is pinned here rather than assumed — and
    /// so is the fact that asking for it TOUCHES NOTHING.
    #[cfg(unix)]
    #[test]
    fn the_unix_answer_is_the_runtime_dir_the_callers_already_used() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _s = aoide_test_support::EnvSaver::capture(&["XDG_RUNTIME_DIR"]);

        std::env::set_var("XDG_RUNTIME_DIR", "/run/user/0-probe");
        assert_eq!(socket_dir(), PathBuf::from("/run/user/0-probe/aoide"));
        assert!(!socket_dir().exists(), "resolving a path must not create one");

        // An EMPTY value is not a runtime dir: it falls back to this process's
        // own, rather than becoming a relative `aoide/…` path (what one of the
        // old spellings did with it).
        std::env::set_var("XDG_RUNTIME_DIR", "");
        assert_eq!(
            socket_dir(),
            PathBuf::from(format!("/run/user/{}/aoide", euid())),
            "an empty XDG_RUNTIME_DIR must fall back to this user's own runtime dir"
        );
    }
}
