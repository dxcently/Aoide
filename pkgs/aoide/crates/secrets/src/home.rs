//! The secrets home: the directory holding `policy.json`, `backends.json`,
//! the broker's own append-only `audit.log`, and (later phases) the TOTP
//! secret + replay ledger. **One function** — every other module in this
//! crate reaches the secrets home through [`secrets_home`], never re-derives
//! it (the phase brief's own wording).
//!
//! Nix-independent: no nix shell-out, no NixOS assumption anywhere in this
//! module (root `AGENTS.md`'s HARD CONSTRAINT — core, and the secrets broker
//! with it, must build with cargo and run on any Linux).
//!
//! **The default is a placeholder, not yet the deployed reality.** P-V4
//! (deployment) is what actually provisions `/var/lib/aoide-secrets`,
//! chowned to a real `aoide-secrets` system user, via a nix module +
//! tmpfiles rule — until then, this default exists so the code has a
//! concrete answer, and the OWNER is whatever ordinary uid runs `secrets
//! serve`/`secrets add` first. Set `AOIDE_SECRETS_SOCKET`/`AOIDE_SECRETS_HOME`
//! to a writable directory (a tempdir in every test here, or a dev
//! directory by hand) on any host that hasn't run P-V4's module yet —
//! `secrets serve` will fail to bind against the placeholder default with
//! an ordinary permission error, not a panic, which is the expected shape
//! of "not deployed yet".
//!
//! **This module DOES create and lock down the secrets home** (corrected,
//! P-V2 review bounce-fix item 3 — an earlier revision of this doc
//! claimed otherwise, which was false: `broker::serve` and
//! `store::save_policies` both call `std::fs::create_dir_all` on it).
//! [`secure_dir`] is the PERMISSIONS half of that: `create_dir_all` alone
//! honors the process umask (0755 by default), which would leave
//! `policy.json`/`backends.json` world-readable inside a world-searchable
//! directory — every `create_dir_all(secrets_home)` call site in this crate
//! is immediately followed by `secure_dir(secrets_home)`, propagating the
//! error rather than serving/writing into an insecure directory.
//! [`secure_file`] is the matching per-file half, used by `store::
//! save_policies` on `policy.json`.
//!
//! **This module also owns the admin-command identity guard** (the
//! yomi-strix incident, 2026-08-22): [`admin_identity_error`] is the pure
//! decision for an EXISTING home (injected `euid`/`home_owner`,
//! unit-testable without a real stat or a real process uid);
//! [`admin_identity_error_for_missing_home`] is its sibling for a home
//! that does not exist YET — a stat failure is not license to proceed,
//! since `store::save_policies`/`store::save_totp_secret` both
//! `create_dir_all` the home on first write, so an unguarded root caller
//! would CREATE it owned `root:root` (the identical bricking symptom as
//! the yomi-strix incident, just at creation time instead of a reown;
//! found on review, P-V4f follow-up). [`admin_identity_check`] wires both
//! to a real [`effective_uid`] and a real `std::fs::metadata(home)` — see
//! that function's own doc. Every admin command that reads/writes
//! `policy.json`/`totp.secret` calls this BEFORE any such
//! read-modify-write — `commands.rs`'s `require_admin_identity` for the
//! policy-CRUD quintet, `enroll::run` directly for the one other
//! broker-home write outside `commands.rs`'s own dispatch.

use std::io;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::peercred::PeerUser;

/// Resolve the secrets home: `$AOIDE_SECRETS_HOME` when set to a non-blank
/// value, else the placeholder default (see module doc).
pub fn secrets_home() -> PathBuf {
    if let Ok(dir) = std::env::var("AOIDE_SECRETS_HOME") {
        if !dir.trim().is_empty() {
            return PathBuf::from(dir);
        }
    }
    PathBuf::from("/var/lib/aoide-secrets")
}

/// Lock a secrets-home-owned DIRECTORY down to `0700` (owner rwx only).
/// Called immediately after every `create_dir_all(secrets_home)` in this
/// crate — see module doc. The native Windows arm attaches the owner-only
/// policy (`aoide_protocol::owner_only`) rather than a mode, which is the
/// same guarantee in the host's own terms.
pub fn secure_dir(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
    }
    #[cfg(windows)]
    {
        aoide_protocol::owner_only::ensure_private_dir(path)
    }
}

/// Lock a secrets-home-owned FILE down to `0600` (owner rw only). Called
/// after writing a sensitive secrets-home file (`store::save_policies`'s
/// `policy.json`). Native Windows attaches the owner-only DACL — the same
/// guarantee, said in the one vocabulary that host has for it.
pub fn secure_file(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
    }
    #[cfg(windows)]
    {
        aoide_protocol::owner_only::set_file_access(path)
    }
}

/// This process's real effective uid. `std` has no `geteuid(2)` wrapper, but
/// `libc` is already a workspace dependency this crate links for
/// `enroll::local_hostname`'s `gethostname(2)` call (`Cargo.toml`'s
/// comment), so this costs nothing new in the lockfile — zero new
/// dependencies, per this crate's own house rule. Unix only: a host with no
/// uid answers [`effective_user`] with the identity it does have.
#[cfg(unix)]
pub fn effective_uid() -> u32 {
    // SAFETY: `geteuid(2)` takes no arguments, can't fail, and touches no
    // memory this process doesn't already own.
    unsafe { libc::geteuid() }
}

/// This process's OWN user identity, in the host's own terms — the other half
/// of every same-user comparison `peercred::PeerUser` is the first half of.
/// `None` when the host cannot say who this process is (a token read that
/// failed), which every comparison reads as "cannot match", the same
/// fail-closed answer an unidentified peer gets.
pub fn effective_user() -> Option<PeerUser> {
    #[cfg(unix)]
    {
        Some(PeerUser::Uid(effective_uid()))
    }
    #[cfg(windows)]
    {
        aoide_protocol::win_proc::current_user_sid().ok().map(PeerUser::Sid)
    }
}

/// Is this process the host's super-user — the one identity for which the
/// ownership checks below are vacuous, because it can write anywhere
/// regardless of who owns the file? `euid == 0` on Unix.
///
/// **`false` on native Windows, and that is a fact rather than a stub**:
/// there is no uid 0 there, and the hazard root poses — creating the home
/// as an identity other than the broker's own — is answered at creation
/// instead, because every private file and directory this crate creates
/// pins its owner to the creating token user (`aoide_protocol::owner_only`).
/// A caller here therefore has nothing to refuse.
pub fn running_as_root() -> bool {
    #[cfg(unix)]
    {
        effective_uid() == 0
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// The effective uid as a [`PeerUser`] — the shape the admin-identity check
/// compares, for a caller that has a uid rather than a peer.
pub fn peer_user_for_uid(uid: u32) -> PeerUser {
    PeerUser::Uid(uid)
}

/// The admin-command identity guard's decision, PURE so it's testable with
/// injected uids (`euid`/`home_owner`) rather than a real stat + a real
/// process uid — see [`admin_identity_check`] for the live wiring.
/// `None` when `euid` already owns `home` (the expected shape: an admin
/// command run as the broker user, or a test/dev host where one ordinary uid
/// created the home itself). `Some(message)` otherwise, naming the actual
/// home path, the actual owning uid, and the corrective `sudo -u
/// aoide-secrets` spelling — root gets an EXTRA clause spelling out why
/// plain `sudo` is wrong: root can always write regardless of ownership,
/// which is exactly what silently reowns `policy.json` to `root:root` and
/// bricks the broker (the yomi-strix incident, 2026-08-22, this guard
/// exists to make impossible).
/// The identity that owns the object at `path` — the ownership fact Unix
/// answers with `MetadataExt::uid` and native Windows with the object's own
/// owner SID (`aoide_protocol::owner_only::owner_sid`). `None` when the path
/// is not there or its owner cannot be read; never a guessed owner.
pub fn owner_of(path: &Path) -> Option<PeerUser> {
    #[cfg(unix)]
    {
        std::fs::metadata(path).ok().map(|m| PeerUser::Uid(m.uid()))
    }
    #[cfg(windows)]
    {
        aoide_protocol::owner_only::owner_sid(path).ok().flatten().map(PeerUser::Sid)
    }
}

/// An identity as a refusal spells it: `uid 1000` on the host that has uids
/// (the wording every existing message already uses, unchanged), `user
/// S-1-5-…` on the host that has SIDs.
fn describe(user: &PeerUser) -> String {
    match user {
        PeerUser::Uid(uid) => format!("uid {uid}"),
        PeerUser::Sid(sid) => format!("user {sid}"),
    }
}

/// The admin-command identity guard's decision, PURE so it's testable with
/// injected identities rather than a real stat + a real process identity —
/// the live wiring is [`admin_identity_check`].
pub fn admin_identity_error(euid: &PeerUser, home_owner: &PeerUser, home: &Path, subcommand: &str) -> Option<String> {    if euid == home_owner {
        return None;
    }
    let running_as = if *euid == PeerUser::Uid(0) {
        "root (uid 0) — plain `sudo` runs as root, and root CAN write here regardless of file ownership, \
         which is exactly what silently corrupts it"
            .to_string()
    } else {
        describe(euid)
    };
    Some(format!(
        "secrets {subcommand} must run as the broker user ({}, the owner of {}) — this process is running as {running_as}. \
         Run: sudo -u aoide-secrets aoide secrets {subcommand} ...",
        describe(home_owner),
        home.display()
    ))
}

/// The admin-identity guard's decision when `home` does NOT exist yet
/// (PURE, injected `euid` — the missing-home sibling of
/// [`admin_identity_error`]). A stat failure is NOT license to proceed:
/// `store::save_policies`/`store::save_totp_secret` both `create_dir_all`
/// the home on their very first write, so a root caller hitting a missing
/// home would CREATE `policy.json`/`totp.secret` owned `root:root` — the
/// identical bricking symptom as the yomi-strix incident, just at
/// creation time instead of a reown (found on review, P-V4f follow-up,
/// 2026-08-22).
///
/// `None` for any non-root `euid`: a non-root uid bootstrapping its own
/// fresh home (the dev/test tempdir flow, or an explicit `sudo -u
/// aoide-secrets` first run per the deployment doc) is the expected shape
/// and stays allowed. `Some(message)` for `euid == 0` — root must never
/// be the one to create the secrets home, full stop; first-time
/// provisioning belongs to the broker's own systemd unit
/// (`StateDirectory=`) or an explicit `sudo -u aoide-secrets` invocation.
pub fn admin_identity_error_for_missing_home(euid: &PeerUser, home: &Path, subcommand: &str) -> Option<String> {
    if *euid != PeerUser::Uid(0) {
        return None;
    }
    Some(format!(
        "secrets {subcommand} must run as the broker user, not root — {} does not exist yet, and root creating it \
         would leave policy.json/totp.secret owned root:root, bricking the broker before it even starts. \
         First-time provisioning belongs to the broker's own service (systemd's StateDirectory) or an explicit \
         `sudo -u aoide-secrets` run. Run: sudo -u aoide-secrets aoide secrets {subcommand} ...",
        home.display()
    ))
}

/// Live wiring for [`admin_identity_error`]/[`admin_identity_error_for_missing_home`]:
/// stats `home` for its owning uid and compares it against this process's
/// real [`effective_uid`].
///
/// When `home` exists, [`admin_identity_error`] decides (euid vs. the
/// real owner). **When it does not exist yet — or its owner can't be
/// stat'd at all — this falls to [`admin_identity_error_for_missing_home`]
/// rather than passing unconditionally**: nothing has decided who the
/// broker user is until the FIRST admin command creates the home directory,
/// but `store::save_policies`/`store::save_totp_secret` both
/// `create_dir_all` it on that first write, so root reaching this point
/// would CREATE a root-owned home — refused for that reason alone, a
/// non-root uid still passes through to create it itself.
pub fn admin_identity_check(home: &Path, subcommand: &str) -> Option<String> {
    // A process whose OWN identity cannot be read cannot be compared to
    // anything: refused, the same fail-closed answer an unidentified peer
    // gets, rather than a pass.
    let Some(me) = effective_user() else {
        return Some(format!(
            "secrets {subcommand} must run as the broker user, and this process's own identity could not be \
             read on this host — refusing rather than assuming. Run: sudo -u aoide-secrets aoide secrets {subcommand} ..."
        ));
    };
    match owner_of(home) {
        Some(owner) => admin_identity_error(&me, &owner, home, subcommand),
        None => admin_identity_error_for_missing_home(&me, home, subcommand),
    }
}

/// Enrich a secrets-home FILE's I/O error into an actionable message — the
/// POISONED-FILE case (this crate's `AGENTS.md`): [`admin_identity_check`]
/// above already proves this process's euid owns the secrets HOME
/// directory before an admin command ever reads/writes a file inside it, but
/// an individual file (`policy.json`, `totp.secret`, `totp-replay.json`)
/// can still be owned by a stale uid from a historical plain-`sudo` run
/// that predates that guard — the exact "policy.json: Permission denied
/// (os error 13)" the User hit live, with zero indication of WHY. Reached
/// from every admin-command load/save seam (`commands.rs`'s CRUD quintet,
/// `enroll::run`/`enroll::show`'s totp.secret/replay-ledger calls) so this
/// diagnosis lives in exactly ONE place rather than a copy at each of the
/// half-dozen call sites that used to just `format!("policy.json: {e}")`.
///
/// Only [`io::ErrorKind::PermissionDenied`] gets the rich treatment — every
/// other kind (a missing file already reads as an empty policy list
/// upstream in `store::load_policies`; a corrupt JSON body is a data
/// problem, not an ownership one) rides through with just the file path
/// prefixed, same shape as before this function existed. The repair spells
/// `chown --reference=<home>` rather than a literal `chown aoide-secrets:
/// aoide-secrets-access <file>` — this crate never learns a broker
/// username, only uids (`effective_uid`'s whole reason for existing), and
/// `--reference` matches the file's ownership to the secrets home's own
/// without this crate ever resolving one.
pub fn describe_home_file_error(home: &Path, file: &Path, err: &io::Error) -> String {
    if err.kind() != io::ErrorKind::PermissionDenied {
        return format!("{}: {err}", file.display());
    }
    let home_owner = owner_of(home);
    let file_owner = owner_of(file);
    let owner_note = match (home_owner, file_owner) {
        (Some(h), Some(f)) if h != f => format!(
            " — {} is owned by {}, but the secrets home ({}) is owned by {}; this looks \
             like a file poisoned by a historical plain `sudo` run from before the admin-identity \
             guard existed",
            file.display(),
            describe(&f),
            home.display(),
            describe(&h)
        ),
        (Some(h), Some(_)) => format!(
            " — {} and the secrets home are both owned by {}, but this process still cannot \
             write it (check its permission bits)",
            file.display(),
            describe(&h)
        ),
        _ => String::new(),
    };
    format!(
        "{}: permission denied{owner_note}. Fix: sudo chown --reference={} {} (matches its ownership \
         to the secrets home's owner — the broker user)",
        file.display(),
        home.display(),
        file.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── the POSIX-shell fixture class (gated, with the reason) ───────────
    //
    // Every test below carrying `#[cfg(unix)]` drives the backend TEMPLATE
    // mechanism with a POSIX fixture: an `sh -c` command line, or a
    // `#!/bin/sh` shim script on `PATH`. The mechanism itself is portable and
    // HAS a native arm — `sh -c` on Unix, `cmd /C` on native Windows
    // (`backend::run_backend_command`'s own doc) — so these gates name the
    // FIXTURE, never the code under test. The Windows arm of the same path is
    // exercised natively by `backend::tests::a_native_windows_template_*`.

    #[test]
    fn env_override_wins_when_set_and_non_blank() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_SECRETS_HOME").ok();
        std::env::set_var("AOIDE_SECRETS_HOME", "/tmp/aoide-secrets-test-home");
        assert_eq!(secrets_home(), PathBuf::from("/tmp/aoide-secrets-test-home"));
        match saved {
            Some(v) => std::env::set_var("AOIDE_SECRETS_HOME", v),
            None => std::env::remove_var("AOIDE_SECRETS_HOME"),
        }
    }

    #[test]
    fn default_is_the_documented_placeholder_path() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_SECRETS_HOME").ok();
        std::env::remove_var("AOIDE_SECRETS_HOME");
        assert_eq!(secrets_home(), PathBuf::from("/var/lib/aoide-secrets"));
        match saved {
            Some(v) => std::env::set_var("AOIDE_SECRETS_HOME", v),
            None => std::env::remove_var("AOIDE_SECRETS_HOME"),
        }
    }

    #[test]
    fn blank_env_value_falls_back_to_default() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_SECRETS_HOME").ok();
        std::env::set_var("AOIDE_SECRETS_HOME", "   ");
        assert_eq!(secrets_home(), PathBuf::from("/var/lib/aoide-secrets"));
        match saved {
            Some(v) => std::env::set_var("AOIDE_SECRETS_HOME", v),
            None => std::env::remove_var("AOIDE_SECRETS_HOME"),
        }
    }

    fn tmp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aoide-secrets-home-perms-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn secure_dir_sets_owner_only_permissions() {
        let dir = tmp_dir("dir");
        secure_dir(&dir).unwrap();
        let mode = crate::home::mode_of(&dir);
        assert_eq!(mode, 0o700, "secrets home must be owner-rwx-only, got {mode:o}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn secure_file_sets_owner_read_write_only() {
        let dir = tmp_dir("file");
        let file = dir.join("policy.json");
        std::fs::write(&file, b"[]").unwrap();
        secure_file(&file).unwrap();
        let mode = crate::home::mode_of(&file);
        assert_eq!(mode, 0o600, "a secrets-home file must be owner-rw-only, got {mode:o}");
        std::fs::remove_dir_all(&dir).ok();
    }

    // ── admin_identity_error (pure — injected uids, no real stat/geteuid) ──

    #[test]
    fn admin_identity_error_is_none_when_euid_owns_the_home() {
        assert_eq!(admin_identity_error(&PeerUser::Uid(1000), &PeerUser::Uid(1000), Path::new("/var/lib/aoide-secrets"), "add"), None);
        // Root owning its own home (an unusual but not impossible
        // deployment) is also a match, not a special case.
        assert_eq!(admin_identity_error(&PeerUser::Uid(0), &PeerUser::Uid(0), Path::new("/var/lib/aoide-secrets"), "add"), None);
    }

    #[test]
    fn admin_identity_error_flags_root_explicitly_as_the_wrong_uid() {
        let msg = admin_identity_error(&PeerUser::Uid(0), &PeerUser::Uid(1000), Path::new("/var/lib/aoide-secrets"), "add").unwrap();
        assert!(msg.contains("secrets add"), "{msg}");
        assert!(msg.contains("/var/lib/aoide-secrets"), "{msg}");
        assert!(msg.contains("uid 1000"), "{msg}");
        assert!(msg.to_lowercase().contains("root"), "{msg}");
        assert!(msg.contains("sudo -u aoide-secrets aoide secrets add"), "{msg}");
    }

    #[test]
    fn admin_identity_error_flags_an_arbitrary_mismatched_uid() {
        let msg = admin_identity_error(&PeerUser::Uid(1001), &PeerUser::Uid(1000), Path::new("/var/lib/aoide-secrets"), "grant").unwrap();
        assert!(msg.contains("uid 1001"), "{msg}");
        assert!(msg.contains("uid 1000"), "{msg}");
        assert!(msg.contains("sudo -u aoide-secrets aoide secrets grant"), "{msg}");
        // Not root, so no plain-sudo digression.
        assert!(!msg.to_lowercase().contains("plain `sudo`"), "{msg}");
    }

    // ── admin_identity_error_for_missing_home (pure — injected euid) ───────
    //
    // The reviewed gap: a stat failure alone used to pass unconditionally,
    // but `store::save_policies`/`store::save_totp_secret` `create_dir_all`
    // the home on first write, so a root caller hitting a missing home
    // would CREATE it root-owned — the same bricking symptom as an
    // existing-home reown, just at creation time.

    #[test]
    fn admin_identity_error_for_missing_home_refuses_root() {
        let msg = admin_identity_error_for_missing_home(&PeerUser::Uid(0), Path::new("/var/lib/aoide-secrets"), "add").unwrap();
        assert!(msg.contains("secrets add"), "{msg}");
        assert!(msg.contains("/var/lib/aoide-secrets"), "{msg}");
        assert!(msg.to_lowercase().contains("root"), "{msg}");
        assert!(msg.contains("sudo -u aoide-secrets aoide secrets add"), "{msg}");
    }

    #[test]
    fn admin_identity_error_for_missing_home_passes_a_non_root_uid() {
        assert_eq!(
            admin_identity_error_for_missing_home(&PeerUser::Uid(1000), Path::new("/var/lib/aoide-secrets"), "add"),
            None
        );
        assert_eq!(
            admin_identity_error_for_missing_home(&PeerUser::Uid(1), Path::new("/var/lib/aoide-secrets"), "enroll"),
            None
        );
    }

    // ── admin_identity_check (real stat + real effective_uid) ──────────────

    #[test]
    fn admin_identity_check_on_a_missing_home_matches_the_missing_home_rule() {
        let dir = std::env::temp_dir().join(format!(
            "aoide-secrets-home-identity-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        assert!(!dir.exists());
        // Defensive either way (this suite never actually runs as root),
        // but this keeps the assertion honest rather than assuming a
        // non-root test runner.
        let expected = admin_identity_error_for_missing_home(&effective_user().expect("this process has an identity"), &dir, "add");
        assert_eq!(admin_identity_check(&dir, "add"), expected);
        if !running_as_root() {
            assert_eq!(admin_identity_check(&dir, "add"), None, "non-root creating a fresh home must still be allowed");
        }
    }

    /// The ordinary single-user shape: the home this process just made is
    /// owned by this process, so `admin_identity_check` has nothing to report.
    /// PINNED first, because that ownership is not a property of "just made a
    /// directory" on native Windows — an elevated token's fresh directory is
    /// owned by `BUILTIN\Administrators` until the owner-only policy is
    /// attached (`secure_dir`), which is the same pinning the crate's own
    /// creation path does.
    #[test]
    fn admin_identity_check_is_none_when_this_process_owns_the_home() {
        let dir = tmp_dir("identity-owned");
        crate::home::secure_dir(&dir).unwrap();
        assert_eq!(admin_identity_check(&dir, "add"), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    // ── describe_home_file_error (the poisoned-file diagnosis) ─────────────

    #[test]
    fn non_permission_denied_errors_pass_through_with_just_the_path_prefixed() {
        let home = Path::new("/var/lib/aoide-secrets");
        let file = Path::new("/var/lib/aoide-secrets/policy.json");
        let err = io::Error::new(io::ErrorKind::InvalidData, "not valid json");
        let msg = describe_home_file_error(home, file, &err);
        assert_eq!(msg, format!("{}: {err}", file.display()));
        assert!(!msg.contains("chown"), "{msg}");
    }

    #[test]
    fn permission_denied_on_a_pair_that_cannot_be_stat_ed_still_teaches_the_chown_reference_fix() {
        // Neither path exists, so both metadata() calls fail — this is the
        // "cheap where possible" fallback: no owner-mismatch clause, but
        // still the actionable repair.
        let home = Path::new("/nonexistent-aoide-secrets-home-for-test");
        let file = home.join("policy.json");
        let err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let msg = describe_home_file_error(home, &file, &err);
        assert!(msg.contains(&file.display().to_string()), "{msg}");
        assert!(msg.contains("permission denied"), "{msg}");
        assert!(msg.contains(&format!("chown --reference={}", home.display())), "{msg}");
        assert!(!msg.contains("is owned by uid"), "{msg}");
    }

    // cfg(unix): the fixture is a POSIX shell template or a `#!/bin/sh` shim
    // (the module note above names the class; the code under test is portable).
    #[cfg(unix)]
    #[test]
    fn permission_denied_with_a_stat_able_pair_names_the_shared_owning_uid() {
        // Both this process's own tempdir and a file inside it are owned by
        // the SAME uid (this test process's own euid) — exercises the
        // "owners match but still denied" branch without needing a second
        // real uid, which a non-root test runner can't fabricate.
        let dir = tmp_dir("describe-same-owner");
        let file = dir.join("policy.json");
        std::fs::write(&file, b"[]").unwrap();
        let err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let msg = describe_home_file_error(&dir, &file, &err);
        assert!(msg.contains("both owned by uid"), "{msg}");
        assert!(msg.contains(&format!("chown --reference={}", dir.display())), "{msg}");
        std::fs::remove_dir_all(&dir).ok();
    }

    // cfg(unix): the fixture is a POSIX shell template or a `#!/bin/sh` shim
    // (the module note above names the class; the code under test is portable).
    #[cfg(unix)]
    #[test]
    fn a_freshly_created_directory_is_owned_by_this_process() {
        // No root/setuid assumption needed: whatever this process's identity
        // is, a directory it just created belongs to exactly that identity —
        // a uid on Unix, that token user's SID on native Windows.
        let dir = tmp_dir("euid-sanity");
        assert_eq!(owner_of(&dir), effective_user());
        std::fs::remove_dir_all(&dir).ok();
    }
}

// ── the private-policy assertion, in the host's own terms ────────────────
//
// Every privacy test in this crate used to ask `mode() & 0o777`. That is one
// host's spelling of the guarantee; the guarantee itself is "nobody but the
// owner can read it", which Unix answers with mode bits and native Windows
// with the object's DACL. These two helpers are the ONE place the two
// spellings meet, so a test keeps asserting `0o600`/`0o700` — the numbers a
// reader recognises — and gets an answer that is real on whatever host it
// runs on.

/// This object's privacy as the mode a reader would recognise: Unix's actual
/// bits; on native Windows `0o600`/`0o700` for an object whose policy reads
/// back owner-only, `0o644` for one that does not (so a test asserting "this
/// is NOT private" is answered honestly there too).
#[cfg(test)]
pub fn mode_of(path: &Path) -> u32 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).expect("stat").permissions().mode() & 0o777
    }
    #[cfg(windows)]
    {
        let is_dir = std::fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false);
        let reason = if is_dir {
            aoide_protocol::owner_only::dir_privacy(path)
        } else {
            aoide_protocol::owner_only::file_privacy(path)
        };
        match reason.expect("read the object's own policy back") {
            None => {
                if is_dir {
                    0o700
                } else {
                    0o600
                }
            }
            Some(_) => 0o644,
        }
    }
}

/// Arrange `path`'s policy for a test. `0o600`/`0o700` ask for the private
/// policy on either host — the native call on Windows, `chmod` on Unix. A
/// LOOSENING mode is a Unix-only fixture: there is no "world-writable" native
/// policy to attach, and the object keeps whatever default policy it was
/// created with (which is what a test wants when it is arranging an object
/// that must FAIL the privacy check), so the Windows arm is a no-op.
#[cfg(test)]
pub fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
    }
    #[cfg(windows)]
    {
        match mode {
            0o600 => aoide_protocol::owner_only::set_file_access(path),
            0o700 => aoide_protocol::owner_only::ensure_private_dir(path),
            _ => Ok(()),
        }
    }
}
