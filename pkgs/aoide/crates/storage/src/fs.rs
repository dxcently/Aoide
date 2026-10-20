//! The core crates' stage-file FS substrate: the runtime root and its core
//! trees (`state/`, the conducting stage), the atomic write, and the locks
//! every core writer routes through.
//!
//! **Runtime root (L-C2, task #107):** every core path hangs off ONE root,
//! [`root`] — `$AOIDE_ROOT` (absolute-path-wins), default `<home>/.aoide`,
//! nix-free. `~/Aoide` demotes to purely the dev git checkout, reached through
//! the separate [`flake_root`] seam (`$AOIDE_FLAKE_ROOT`, default
//! `<home>/Aoide`). [`migrate_root_once`] moves a pre-L-C2 host's
//! `~/Aoide/{state,log}` trees into it — one-shot and idempotent, but
//! deliberately NOT wired into [`root`]'s own resolution (see that function's
//! doc for why): the core binaries' `main()` call it explicitly, once, at
//! process start.
//!
//! **Two stages, two lock files.** The conducting stage is
//! [`conducting_stage_dir`] (`state/stage/`), and [`with_stage_lock`] locks
//! it. The rice stage belongs to `strillix-rice`, which locks its own stage
//! through [`with_lock_in`] on its own directory. A lock file guards only the
//! files of its own domain, so no file is ever written under both locks.

use std::io::Write;

/// The Windows half of this module: the lock, the no-clobber rename and the
/// link-preserving copy, whose Unix shapes have no Windows spelling. A
/// sibling file is not a non-`mod.rs` parent's default resolution, hence the
/// explicit `#[path]`, the same shape `aoide_protocol::feed`'s
/// `feed_windows.rs` uses.
#[cfg(windows)]
#[path = "fs_windows.rs"]
mod fs_windows;

// ── the lock, as both hosts' callers see it ──────────────────────────────

/// Take the exclusive lock on an open lock file, blocking until it is free.
/// Unix `flock(LOCK_EX)`; Windows `LockFileEx`. `false` means it was not
/// taken — every caller here already reads that as "not held", never as
/// "assume held".
///
/// `pub` (not `pub(crate)`): `aoide-conduct`'s own file-lock probe
/// (`graph/codex_app.rs::lock_is_held`) and its tests ask this exact question
/// of a file on the SAME host, so they call these three functions rather than
/// a second per-host lock mechanism (`pkgs/aoide/crates/AGENTS.md`, "no
/// cross-crate copying": widen the symbol, never fork it).
#[cfg(unix)]
pub fn lock_exclusive(file: &std::fs::File) -> bool {
    use std::os::unix::io::AsRawFd;
    // A signal that interrupts the wait is not a refusal: the wait is retried,
    // so a TERM to the wrap cannot leave a stage write unlocked.
    loop {
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0 {
            return true;
        }
        if std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            return false;
        }
    }
}

#[cfg(windows)]
pub fn lock_exclusive(file: &std::fs::File) -> bool {
    fs_windows::lock_exclusive(file).is_ok()
}

/// The non-blocking probe: `Ok(true)` taken, `Ok(false)` another holder has
/// it (`LOCK_NB`'s `EWOULDBLOCK`, `LockFileEx`'s `LOCKFILE_FAIL_IMMEDIATELY`
/// lock violation), `Err` a real failure. The distinction is the whole point
/// for `outbox`'s `.bsy` guard, which skips a busy link and must not mistake
/// "busy" for "broken".
#[cfg(unix)]
pub fn try_lock_exclusive(file: &std::fs::File) -> std::io::Result<bool> {
    use std::os::unix::io::AsRawFd;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        return Ok(true);
    }
    let err = std::io::Error::last_os_error();
    if err.kind() == std::io::ErrorKind::WouldBlock {
        return Ok(false);
    }
    Err(err)
}

#[cfg(windows)]
pub fn try_lock_exclusive(file: &std::fs::File) -> std::io::Result<bool> {
    fs_windows::try_lock_exclusive(file)
}

/// Release the lock. Best-effort and total on both hosts — the callers are a
/// `Drop` guard and the tail of a closure, neither of which has anywhere to
/// report a failure to.
#[cfg(unix)]
pub fn unlock(file: &std::fs::File) {
    use std::os::unix::io::AsRawFd;
    unsafe {
        libc::flock(file.as_raw_fd(), libc::LOCK_UN);
    }
}

#[cfg(windows)]
pub fn unlock(file: &std::fs::File) {
    fs_windows::unlock(file);
}

// ── the private policy, as a test asks about it ──────────────────────────

/// Assert a FILE carries this crate's private policy on THIS host: mode
/// `0o600` on Unix, the owner-only DACL read back from the object on Windows.
/// Test-only and `pub(crate)` so `identity`'s tests ask the same question the
/// same way, rather than answering it with a second mechanism.
#[cfg(all(test, unix))]
pub(crate) fn assert_private_file(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "{} must be 0600, got {mode:o}", path.display());
}

#[cfg(all(test, windows))]
pub(crate) fn assert_private_file(path: &std::path::Path) {
    let refusal = aoide_protocol::owner_only::file_privacy(path).unwrap();
    assert!(refusal.is_none(), "{} must carry the owner-only DACL: {refusal:?}", path.display());
}

/// The directory half — `0o700` on Unix, the same owner-only DACL on
/// Windows, where the mask's directory meanings cover list and add.
#[cfg(all(test, unix))]
pub(crate) fn assert_private_dir(dir: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(dir).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "{} must be 0700, got {mode:o}", dir.display());
}

#[cfg(all(test, windows))]
pub(crate) fn assert_private_dir(dir: &std::path::Path) {
    let refusal = aoide_protocol::owner_only::dir_privacy(dir).unwrap();
    assert!(refusal.is_none(), "{} must carry the owner-only DACL: {refusal:?}", dir.display());
}

/// The runtime root every stage/state/run tree hangs off: `$AOIDE_ROOT`
/// (absolute-path-wins, same discipline as every other override here),
/// default [`default_root`] (`<home>/.aoide`) — core code default, no nix
/// required. Every OTHER path in this file (`stage_dir`, `state_dir`, and
/// everything derived from them) composes from here now instead of
/// `aoide_protocol::aoide_home().join("Aoide")` directly, so a single
/// override relocates the whole tree at once.
///
/// **Deliberately pure — no migration side effect on this path**, unlike
/// [`conducting_stage_dir`]'s own S1 precedent. `stage_dir`/`state_dir`
/// (and this function transitively) are reached by [`with_stage_lock`],
/// the SHARED lock primitive nearly every stage-file writer across the
/// whole workspace routes through regardless of which file it's actually
/// touching (`with_stage_lock`'s own doc: it always locks `stage_dir()`,
/// even for a `conducting_stage_dir`-domain caller) — a live incident
/// during this lane's own development proved that a `state_dir()`-only
/// test (a command's own tests, overriding only `$AOIDE_STATE_DIR`) still
/// reaches `stage_dir()`'s fallback through `with_stage_lock`, and would
/// have silently driven a real migration against the operator's actual
/// `$HOME` the first time such a test ran unguarded. `conducting_stage_dir`
/// has no such shared low-level caller, which is why hanging a migration
/// off ITS resolution is safe while hanging one off `root`'s is not. See
/// [`migrate_root_once`]'s own doc for where the migration actually runs.
pub fn root() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("AOIDE_ROOT") {
        let p = std::path::PathBuf::from(&dir);
        if p.is_absolute() {
            return p;
        }
    }
    default_root()
}

/// `$AOIDE_ROOT`'s own default, `<home>/.aoide` — factored out of [`root`]
/// so [`migrate_root_once`] can name its OWN migration target without
/// hardcoding the literal twice.
fn default_root() -> std::path::PathBuf {
    aoide_protocol::aoide_home().join(".aoide")
}


/// One-shot, idempotent move of the old `~/Aoide/{state,log}` trees into the
/// CURRENT `$AOIDE_ROOT` resolution. `pub`: the binaries call this ONCE, early
/// in their own `main()`, never from a library getter (see [`root`]'s own doc
/// for the incident that settled this). Safe to call more than once and from a
/// test directly; nothing here is gated behind a process-wide `Once`, because
/// nothing here is reachable except by an explicit call.
///
/// The rice trees are NOT here: `strillix-rice`'s `migrate` owns them, and only
/// the strillix binary runs that.
///
/// `old_root == new_root` is a no-op, which also covers an operator setting
/// `AOIDE_ROOT` back to `~/Aoide` itself.
///
/// Each piece is gated on ITS OWN env override being unset: `$AOIDE_STATE_DIR`
/// for `state`, `$AOIDE_AUDIT_LOG` for `log`.
///
/// `Err` on EVERY run while a `state/` that exists cannot be locked owner-only,
/// whether this run moved it ([`migrate_dir`]) or an earlier one did. It is
/// locked first, before any move, so the run that moves a tree and the runs
/// after it refuse the same way. The binaries exit on it rather than read a
/// tree they could not make private. A move that fails stays narrated, with the
/// old tree in place.
pub fn migrate_root_once() -> std::io::Result<()> {
    lock_state_if_present()?;
    let old_root = aoide_protocol::aoide_home().join("Aoide");
    let new_root = root();
    if old_root != new_root {
        if std::env::var("AOIDE_STATE_DIR").is_err() {
            migrate_dir(&old_root.join("state"), &new_root.join("state"))?;
        }
        if std::env::var("AOIDE_AUDIT_LOG").map(|v| v.is_empty()).unwrap_or(true) {
            migrate_file(&old_root.join("log"), &new_root.join("log"));
        }
    }
    migrate_node_state_names()
}


/// Locks [`state_dir`] owner-only when something is there, a dangling link
/// included, and does nothing when nothing is: a start-up that finds no
/// `state/` creates none. A `state/` whose parent is not a directory is not
/// there either; the move that would have put a tree there narrates its own
/// failure.
fn lock_state_if_present() -> std::io::Result<()> {
    let state = state_dir();
    match std::fs::symlink_metadata(&state) {
        Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory) => Ok(()),
        _ => lock_state_dir(&state),
    }
}

/// The peer -> node vocabulary rename (User ruling, 2026-09-07): the four
/// state files/dirs that carried the old noun rename in place, under
/// whatever [`state_dir`] resolves to on this host — independent of the
/// old root move above, so it runs whether or not that move applies.
/// Same no-clobber discipline as [`migrate_dir`]/[`migrate_file`]: a file
/// already written under its new name is left alone, and the old one is
/// never deleted out from under a box that hasn't opened the new binary
/// yet.
fn migrate_node_state_names() -> std::io::Result<()> {
    let state = state_dir();
    migrate_file(&state.join("peers.json"), &state.join("nodes.json"));
    migrate_dir(&state.join("peer-cache"), &state.join("node-cache"))?;
    migrate_file(
        &state.join("peer-pairing-inbound.json"),
        &state.join("node-pairing-inbound.json"),
    );
    migrate_file(
        &state.join("peer-pairing-outbound.json"),
        &state.join("node-pairing-outbound.json"),
    );
    Ok(())
}


/// Move `old` → `new` wholesale: a no-op unless `old` exists. An empty `new`
/// is a root something created before migration ran and is replaced; a `new`
/// that holds data is never clobbered — the move is narrated and `old` stays. `rename` first (same filesystem, the common case); a
/// cross-filesystem rename falls back to a recursive copy, removed from the
/// source only once the copy fully lands — a crash mid-copy leaves `old`
/// intact rather than a half-moved tree with `new` looking "done." A move
/// that fails is narrated, never a panic: the tree stays at the pre-migration
/// path, still usable, and boot goes on. A tree that moved is then locked
/// (the rename made `new` with the legacy tree's mode), and a `state/` that
/// cannot be locked is the one `Err`: the tree is now wide where every reader
/// looks.
pub fn migrate_dir(old: &std::path::Path, new: &std::path::Path) -> std::io::Result<()> {
    if !old.exists() {
        return Ok(());
    }
    if new.exists() && std::fs::remove_dir(new).is_err() {
        eprintln!(
            "aoide: {} stays where it is: {} already holds data",
            old.display(),
            new.display()
        );
        return Ok(());
    }
    let Some(parent) = new.parent() else { return Ok(()) };
    if let Err(e) = ensure_write_dir(parent) {
        eprintln!(
            "aoide: cannot create {} ({e}) — {} stays at the old root",
            parent.display(),
            old.display()
        );
        return Ok(());
    }
    if std::fs::rename(old, new).is_err() {
        let tmp = new.with_extension(format!("migrate-tmp.{}", std::process::id()));
        if copy_dir_recursive(old, &tmp).is_ok() && std::fs::rename(&tmp, new).is_ok() {
            let _ = std::fs::remove_dir_all(old);
        } else {
            let _ = std::fs::remove_dir_all(&tmp);
            eprintln!(
                "aoide: could not migrate {} to {} — staying at the old root",
                old.display(),
                new.display()
            );
            return Ok(());
        }
    }
    ensure_write_dir(new)
        .map_err(|e| std::io::Error::new(e.kind(), format!("{e} (migrated from {})", old.display())))
}

/// Move one file `old` → `new` — the single-file counterpart to
/// [`migrate_dir`], for the audit log (a flat file, not a directory).
/// Same no-clobber/no-panic discipline.
pub fn migrate_file(old: &std::path::Path, new: &std::path::Path) {
    if !old.exists() || new.exists() {
        return;
    }
    let Some(parent) = new.parent() else { return };
    if let Err(e) = ensure_write_dir(parent) {
        eprintln!(
            "aoide: cannot create {} ({e}) — {} stays at the old root",
            parent.display(),
            old.display()
        );
        return;
    }
    if std::fs::rename(old, new).is_ok() {
        return;
    }
    if std::fs::copy(old, new).is_ok() {
        let _ = std::fs::remove_file(old);
    } else {
        eprintln!(
            "aoide: could not migrate {} to {} — staying at the old root",
            old.display(),
            new.display()
        );
    }
}

/// Recursive directory copy: every file and subdir, symlinks preserved as
/// symlinks (never followed). Built for [`migrate_dir`]'s cross-filesystem
/// fallback; `pub` (not crate-private) because `strillix-rice`'s runtime
/// rices seed (`commands::rice::seed_rices_from_templates`, task #41)
/// reuses it verbatim for its own whole-tree copy rather than re-walking a
/// second way — both callers want the SAME semantics (unconditional, no
/// byte-diff/merge — the destination is always known-absent going in).
pub fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    ensure_write_dir(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else if file_type.is_symlink() {
            let target = std::fs::read_link(entry.path())?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(target, &dst_path)?;
            // Windows has no untyped `symlink(2)`: the kind is a flag on the
            // call, so the source's own target is asked what it is. The
            // link is still a link — never followed, never rewritten as a
            // regular file — and a source that no longer resolves lands as
            // a file link rather than as a copy.
            #[cfg(windows)]
            fs_windows::symlink(&entry.path(), target, &dst_path)?;
        } else {
            std::fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}


/// The account/usage runtime state directory: `$AOIDE_ROOT/state/` (default
/// `~/.aoide/state/`).
///
/// A gitignored root-runtime dir (CONTRACTS.md §2), explicitly NOT rice-scoped — account/global runtime like
/// `state/usage.json` lives here, never under the rice tree. Resolution mirrors
/// the conducting stage: prefer `$AOIDE_STATE_DIR` when set to an **absolute** path,
/// else derive `$AOIDE_ROOT/state` from [`root`]. A relative or empty
/// override is ignored — same discipline as the stage dir, so a runtime
/// path is never resolved against an arbitrary cwd.
pub fn state_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("AOIDE_STATE_DIR") {
        let p = std::path::PathBuf::from(&dir);
        if p.is_absolute() {
            return p;
        }
    }
    root().join("state")
}

/// The CONDUCTING stage directory: `$AOIDE_ROOT/state/stage/` (default
/// `~/.aoide/state/stage/`), holding the six core roster files: sessions.json,
/// hooks.json, projects.json, graph.json, pending.json and notifications.json.
/// `aoide`/`aoided` alone read and write them (the "L4 dual-writer surface",
/// `conduct/README.md`, mirrored by `server/src/daemon.rs::stage_roster`).
///
/// `$AOIDE_STAGE_DIR` wins when set to an absolute path, and names this
/// directory and nothing else: the rice stage never follows it.
pub fn conducting_stage_dir() -> std::path::PathBuf {
    if let Some(dir) = stage_override() {
        return dir;
    }
    let dir = state_dir().join("stage");
    MIGRATE_CONDUCTING_STAGE_ONCE.call_once(|| migrate_conducting_stage(&dir));
    dir
}

/// `$AOIDE_STAGE_DIR` when it is absolute: the conducting stage's override.
fn stage_override() -> Option<std::path::PathBuf> {
    std::env::var("AOIDE_STAGE_DIR").ok().map(std::path::PathBuf::from).filter(|p| p.is_absolute())
}

/// The conducting lock's directory, with no side effect: [`conducting_stage_dir`]
/// minus its one-shot migration. The locks resolve through this, so taking a
/// lock never starts a migration.
fn conducting_lock_dir() -> std::path::PathBuf {
    stage_override().unwrap_or_else(|| state_dir().join("stage"))
}

/// The legacy home of the conducting files: `$STRILLIX_ROOT/stage/`, the one
/// stage every core file shared before `state/stage/`. Only
/// [`migrate_conducting_stage`] reads it; it is the one name core keeps from the
/// rice tree. Removal trigger: delete it with `migrate_conducting_stage` once no
/// host holds any of the six files at this path (the one-shot move has run
/// everywhere).
fn legacy_stage_dir() -> std::path::PathBuf {
    std::env::var("STRILLIX_ROOT")
        .ok()
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| aoide_protocol::aoide_home().join(".strillix"))
        .join("stage")
}

/// The six core stage-file names [`conducting_stage_dir`] owns — the same
/// roster `server/src/daemon.rs::stage_roster` watches, named here once so
/// [`migrate_conducting_stage`] doesn't hand-copy the list a second place.
const CONDUCTING_STAGE_FILES: &[&str] = &[
    "sessions.json",
    "hooks.json",
    "projects.json",
    "graph.json",
    "pending.json",
    "notifications.json",
];

static MIGRATE_CONDUCTING_STAGE_ONCE: std::sync::Once = std::sync::Once::new();

/// One-shot, boot-safe move of the six core stage files from their PRE-split
/// home ([`legacy_stage_dir`]) into `new_dir` (`state/stage/`) — the
/// conducting-stage migration. Runs at most once per process
/// ([`MIGRATE_CONDUCTING_STAGE_ONCE`], driven from [`conducting_stage_dir`]'s
/// fallback branch only: an `$AOIDE_STAGE_DIR` override names the conducting
/// stage itself, so there is no legacy home to move from).
///
/// Idempotent and safe to re-run every boot: a file only moves when it exists
/// at the OLD path and does NOT already exist at the new one — a newer
/// new-path file (a fresh boot that already migrated, or one seeded after the
/// move) is never overwritten. `rename` first (same filesystem, the common
/// case); a cross-filesystem rename falls back to copy-then-remove-source so
/// the move still completes rather than silently no-op-ing.
///
/// **Locking:** not [`with_stage_lock`] — that lock is fixed to `stage_dir()`
/// (see its own doc), and the files this function moves no longer live
/// there. A dedicated `.migrate.lock` in `new_dir`, `flock`ed for the
/// migration's duration, is enough to keep two processes racing this exact
/// function (e.g. `aoided` and a concurrently-invoked `aoide` CLI at the same
/// boot) from double-moving a file; best-effort like [`with_stage_lock`] —
/// an unlockable lock file runs the migration unlocked rather than blocking
/// boot on a lock hiccup. Ordinary CORE writers racing the migration itself
/// (a `graph session start` landing mid-move) are not specially guarded
/// beyond this: at most one host runs this migration, once, at the first
/// stage-path resolution of its lifetime — the same "known limitation,
/// acceptable at boot, not a steady-state hazard" class `node_store`'s own
/// process-local locks already document.
fn migrate_conducting_stage(new_dir: &std::path::Path) {
    let old_dir = legacy_stage_dir();
    // The ledger was `herald.json` before the notifications rename: a legacy file
    // in either stage becomes `notifications.json` in the new one, no-clobber.
    migrate_file(&new_dir.join("herald.json"), &new_dir.join("notifications.json"));
    migrate_file(&old_dir.join("herald.json"), &new_dir.join("notifications.json"));
    if old_dir == new_dir || !old_dir.exists() {
        return;
    }
    if let Err(e) = ensure_write_dir(new_dir) {
        // Loud, not fatal: readers treat a missing file as an empty
        // registry, so a silently skipped migration would look like the
        // operator's sessions/projects vanished. Narrate the real cause.
        eprintln!(
            "aoide: cannot create {} ({e}) — conducting stage files remain at {}",
            new_dir.display(),
            old_dir.display()
        );
        return;
    }

    let lock = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(new_dir.join(".migrate.lock"))
        .ok();
    let held = lock.as_ref().map(lock_exclusive).unwrap_or(false);

    for name in CONDUCTING_STAGE_FILES {
        let src = old_dir.join(name);
        let dst = new_dir.join(name);
        if !src.exists() || dst.exists() {
            continue;
        }
        if std::fs::rename(&src, &dst).is_ok() {
            continue;
        }
        // Cross-filesystem fallback: copy into a temp sibling and rename it
        // into place, so a crash mid-copy never leaves a truncated `dst`
        // that the existence check above would forever treat as migrated.
        let tmp = new_dir.join(format!(".{name}.migrate-tmp"));
        let copied = std::fs::copy(&src, &tmp).is_ok() && std::fs::rename(&tmp, &dst).is_ok();
        if copied {
            let _ = std::fs::remove_file(&src);
        } else {
            let _ = std::fs::remove_file(&tmp);
            eprintln!(
                "aoide: stage migration could not move {} to {} — \
                 conducting state stays at the old path for this file",
                src.display(),
                dst.display()
            );
        }
    }

    if held {
        if let Some(f) = &lock {
            unlock(f);
        }
    }
}


/// Headless-conduct transcripts: `$AOIDE_ROOT/state/sessions/` (one
/// `<sessionId>.log` per headless `aoide conduct` session — the pty-master
/// mirror `logPath` on the session record points into).
///
/// Under [`state_dir`], not the conducting stage — same reasoning as
/// [`captures_dir`]: a session's transcript is a durable artifact that
/// outlives one invocation, never rice-scoped, never reset by a `rice
/// mode`/stage-reseed the way live rehearsal state is.
pub fn session_logs_dir() -> std::path::PathBuf {
    state_dir().join("sessions")
}


/// The Aoide FLAKE root: the git checkout `nix eval` shells out against
/// (`crate::widgets`'s rices manifest/registry regeneration, C4/W3) —
/// and, since L-C2, the ONLY seam any repo-coupled feature (`rice declare`'s
/// commit-in step, `aoide soundcheck`, a future hand-edit watcher) reaches
/// the checkout through. Default `<home>/Aoide` — this is the ONE place
/// that spelling still means anything; every runtime tree in this file
/// hangs off [`root`] (default `<home>/.aoide`) instead.
///
/// Deliberately NOT derived from `AOIDE_STAGE_DIR` or [`root`] the way every other
/// path in this file is: those are relocatable per-test so a scratch tmp
/// dir can stand in for the RUNTIME trees (`rice/stage`, `run/qml`) without
/// a real flake anywhere in sight. The committed rices `nix eval` reads
/// (`rices/`, `lib/rice.nix`, `flake.nix`) is not a runtime tree —
/// it is the one git checkout on disk, so relocating it per-test would mean
/// fabricating a working flake (with its own `flake.lock`) in every test
/// that touches `rice stage`, for no reason. `$AOIDE_FLAKE_ROOT`
/// (absolute-path-wins, same precedence as every other override here)
/// exists for the one caller that DOES want to point at a different flake
/// checkout — a fixture flake, or a second clone.
pub fn flake_root() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("AOIDE_FLAKE_ROOT") {
        let p = std::path::PathBuf::from(&dir);
        if p.is_absolute() {
            return p;
        }
    }
    aoide_protocol::aoide_home().join("Aoide")
}


/// Point `link` at `target` in one step: a symlink is made at a temp name in
/// the same directory and renamed over `link`, so a reader sees the old
/// target or the new one, never no link. Replaces a regular file or a symlink
/// at `link`. Unix only; elsewhere it is `Unsupported`.
pub fn atomic_symlink(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let dir = link.parent().unwrap_or_else(|| std::path::Path::new("."));
        std::fs::create_dir_all(dir)?;
        let name = link.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let tmp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);
        std::os::unix::fs::symlink(target, &tmp)?;
        std::fs::rename(&tmp, link).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            e
        })
    }
    #[cfg(not(unix))]
    {
        let _ = (target, link);
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "symlinks are unix-only here"))
    }
}


/// Atomic write-temp-then-rename into a file within a directory —
/// symlink-transparent: if `path` is CURRENTLY a symlink, the temp is
/// renamed into whatever it points at instead, leaving the symlink itself
/// intact.
///
/// POSIX `rename()` replaces whatever directory entry sits at its
/// destination — it does NOT dereference a symlink there and write through
/// it. Without this, the very first write after something pointed `path` at
/// a symlink (rice draft mode's `stage/theme.json` → `rices/<rice>/
/// drafts/<name>/theme.json` routing) would silently REPLACE the symlink
/// with a plain file, breaking the routing after one write. Resolving the
/// link ourselves (`symlink_metadata` to detect it without following,
/// `read_link` to read where it points, resolved against `path`'s parent
/// when the link is relative) and renaming into THAT path instead makes
/// every caller of `atomic_write` symlink-transparent for free — this is
/// general behavior, not draft-specific, since every stage-file writer in
/// the codebase routes through here.
///
/// The temp is `<stem>.tmp.<pid>`; on success the rename replaces the target and
/// removes the temp in one step. A failure in EITHER half — the write dying
/// partway, or the rename refusing — strands the temp we just made, so both
/// unlink it. And a write INTERRUPTED between create and rename — a SIGKILL, or a
/// power-cut (a stale `graph.tmp.464255` was found on disk) — can never clean up
/// after itself, so every write also sweeps the whole directory for temps left by
/// a pid that is no longer alive ([`sweep_stale_temps`]).
///
/// Bytes-oriented; [`atomic_write`] is the `&str` convenience wrapper every
/// existing JSON/text caller uses. Binary payloads (a widget QML file carried
/// verbatim into the runtime tree, `crate::widgets`-equivalent callers) route
/// through here directly instead of paying a lossy UTF-8 round-trip.
pub fn atomic_write_bytes(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    atomic_write_bytes_impl(path, contents, None)
}

/// Shared write-temp-then-rename core for [`atomic_write_bytes`] and
/// [`atomic_write_private`]. `create_mode` is `None` for the ordinary
/// (`atomic_write_bytes`) path — the temp is created via `File::create`,
/// whatever mode the process umask leaves it at, matching every existing
/// caller's behavior byte-for-byte — and `Some(0o600)` for the private
/// path, which creates the temp ALREADY locked down via
/// `OpenOptions::mode` (review fix: an earlier revision created the temp at
/// the default mode and `chmod`ed the FINAL path only after the rename,
/// leaving the private seed briefly world/group-readable under this box's
/// 022 umask between the rename landing and the chmod call — see
/// [`atomic_write_private`]'s own doc for the exact defect and why creating
/// the temp pre-locked closes the window instead of narrowing it).
fn atomic_write_bytes_impl(
    path: &std::path::Path,
    contents: &[u8],
    create_mode: Option<u32>,
) -> std::io::Result<()> {
    let target = match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            let link = std::fs::read_link(path)?;
            if link.is_absolute() {
                link
            } else {
                path.parent().map(|p| p.join(&link)).unwrap_or(link)
            }
        }
        _ => path.to_path_buf(),
    };
    if let Some(parent) = target.parent() {
        ensure_write_dir(parent)?;
    }
    let tmp = target.with_extension(format!("tmp.{}", std::process::id()));
    // ONE failure path for both halves. A write that dies partway (ENOSPC inside
    // `write_all`) strands its temp exactly as a failed rename does, and a temp
    // stranded that way is worse than one the rename left: the file it belongs to
    // may never be written again (an outbox entry is keyed by a msgid that occurs
    // once), so nothing would ever come back for it.
    let res = write_temp_file(&tmp, contents, create_mode).and_then(|()| std::fs::rename(&tmp, &target));
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    sweep_stale_temps(&target);
    res
}

/// Create `tmp` and write `contents` into it, optionally via
/// `OpenOptions::mode(create_mode)` when `create_mode` is `Some` — the ONE
/// place [`atomic_write_bytes_impl`] creates a temp file, factored out so a
/// test can call it directly and inspect the temp's mode BEFORE any rename
/// happens, rather than only observing the final path after the whole
/// write-then-rename round trip completes (see
/// `fs::tests::the_private_temp_is_created_already_0600_before_any_rename`).
/// `create_mode` is `None`'s ordinary case: `File::create`'s default,
/// whatever the process umask leaves it at, matching every existing
/// `atomic_write_bytes` caller's behavior byte-for-byte.
fn write_temp_file(
    tmp: &std::path::Path,
    contents: &[u8],
    create_mode: Option<u32>,
) -> std::io::Result<()> {
    if let Some(parent) = tmp.parent() {
        ensure_write_dir(parent)?;
    }
    #[cfg(unix)]
    let mut f = {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        if let Some(mode) = create_mode {
            use std::os::unix::fs::OpenOptionsExt;
            // `open(2)`'s O_CREAT mode is still subject to the process umask,
            // but 0600 carries no group/other bits for a umask to strip in the
            // first place — the temp is created AT 0600, not narrowed to it
            // afterward, so there is no instant where it exists on disk under
            // any wider mode.
            //
            // `mode` applies only when the file is made. A leftover of the same
            // name (a private write killed between create and rename, under a
            // pid since reused) would keep ITS mode through O_TRUNC, so the
            // leftover goes first and the temp is created exclusively: O_EXCL
            // also refuses to follow a link planted at that name, and if the
            // removal fails the create fails closed.
            let _ = std::fs::remove_file(tmp);
            opts.create_new(true);
            opts.mode(mode);
        }
        opts.open(tmp)?
    };
    // Windows has no mode: the private path's policy is the owner-only DACL
    // `aoide-protocol` attaches AT CREATION, which is the only shape with no
    // window at a wider policy (create-then-tighten is the race
    // `owner_only`'s own doc rejects). `0o600` is the one creation mode that
    // policy represents; anything else is refused BY NAME before a byte is
    // written, never narrowed to owner-only. `None` is the ordinary path —
    // `File::create`'s default, i.e. here the DACL the parent already
    // grants, which is the Windows spelling of "whatever the umask leaves".
    #[cfg(windows)]
    let mut f = match create_mode {
        // The descriptor is a REQUEST, and a filesystem that accepts it
        // without persisting it is not a private file: the object is read
        // back through `owner_only::file_privacy` BEFORE the first payload
        // byte, exactly as `feed_windows` reads its own creation back. The
        // check is not a formality — `atomic_write_private` is the identity
        // keypair, and "created at 0600" is worth nothing if nobody asked.
        // A link at the temp path is refused by the same reader: this module
        // will not write a key through a reparse point it cannot name.
        Some(0o600) => {
            let file = aoide_protocol::owner_only::create_truncating(tmp)?;
            match aoide_protocol::owner_only::file_privacy(tmp)? {
                None => file,
                Some(reason) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!(
                            "refusing the private temp {} this process just created: {reason} \
                             (this filesystem may not persist ACLs, and nothing here writes a private file it cannot prove is owner-only)",
                            tmp.display()
                        ),
                    ))
                }
            }
        }
        Some(mode) => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                format!(
                    "refusing create_mode {mode:o}: native Windows honors only 0o600 (a protected owner-only DACL), \
                     so {mode:#o} is unavailable, not narrowed"
                ),
            ))
        }
        None => std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(tmp)?,
    };
    f.write_all(contents)?;
    f.sync_all()
}

/// `&str` convenience wrapper over [`atomic_write_bytes`] — every existing
/// JSON/text stage-file writer routes through here.
pub fn atomic_write(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    atomic_write_bytes(path, contents.as_bytes())
}

/// Atomic write of a SENSITIVE file, locked to `0600` (owner rw only) with
/// NO window at any wider mode — this crate's own precedent for private
/// material that must never enter a `Serialize`/`Deserialize` type (the
/// identity keypair's private-key file, `identity.rs`'s own module doc is
/// the first caller), mirroring `aoide-secrets/src/store.rs`'s
/// `save_policies`/`save_totp_secret` discipline: secure the TEMP file
/// BEFORE the rename, never the final path after it. Here that means
/// creating the temp with `OpenOptions::mode(0o600)` set from the very
/// first `open(2)` call (see [`atomic_write_bytes_impl`]'s doc) rather than
/// `File::create`-then-`chmod` — since `rename(2)` preserves the SOURCE
/// file's mode when replacing a destination, the temp already being 0600
/// means the destination is 0600 the instant the rename lands, never
/// briefly world/group-readable under the process umask the way a
/// create-then-chmod-the-final-path ordering would leave it (review fix:
/// an earlier revision of this function did exactly that — wrote the temp
/// at `File::create`'s default mode, renamed onto the live path, and
/// `chmod`ed only afterward, leaving `state/identity/ed25519.key` briefly
/// world/group-readable on a 022-umask box between the rename and the
/// chmod). Every OTHER concern (symlink transparency, stale-temp sweeping,
/// the rename-failure cleanup) is identical to [`atomic_write_bytes`] —
/// both route through the same [`atomic_write_bytes_impl`] core.
pub fn atomic_write_private(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    atomic_write_bytes_impl(path, contents, Some(0o600))
}

/// Detach a `Command` before it is spawned: its own session/process group, no
/// inherited console, so the child outlives the process that started it — the
/// ONE seam for that posture (`pkgs/aoide/crates/AGENTS.md`, "no cross-crate
/// copying": `conduct`'s `spawn_detached` and `a2a`'s handler spawn both call
/// this, and neither spells a flag of its own).
///
/// Unix: `setsid(2)` in a `pre_exec` hook — async-signal-safe, and the same
/// discipline every `pre_exec` in this tree keeps (no allocation, no lock). A
/// failure there (already a session leader) is not fatal to the spawn; the child
/// merely inherits our process group.
///
/// Native Windows: the equivalent pair of `CreateProcess` flags, which is a
/// property of the spawn CALL rather than of a hook inside the child, so the two
/// arms share a contract and a name, not a body. `DETACHED_PROCESS` replaces
/// `setsid`'s "no controlling tty" and `CREATE_NEW_PROCESS_GROUP` replaces the
/// session's own group. The flags are spelled here rather than pulled from
/// `windows_sys` so this crate needs no Windows API edge for two bits — the
/// Windows API surface lives in `aoide-protocol` (`win_proc`/`win_unix`/
/// `owner_only`), never here. No `CREATE_NO_WINDOW`: the windowed caller's whole
/// point is a terminal that draws, and `DETACHED_PROCESS` alone already denies
/// the child an inherited console.
pub fn detach(command: &mut std::process::Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: `setsid()` is async-signal-safe and the only call made in this
        // hook.
        unsafe {
            command.pre_exec(|| {
                let _ = libc::setsid();
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
}

/// Point `link` at the directory `source` — the ONE symlink seam the tree has,
/// because the two hosts differ on whether the target's KIND is part of the
/// call: Unix's `symlink(2)` is kind-agnostic, while native Windows has two
/// calls and asks the creator which one it is, so a directory gets
/// `symlink_dir` there, never `symlink_file`. One implementation, two callers
/// (`conduct`'s `hooks install` skill links, `cli`'s `onboard` rice link) —
/// never a second copy.
///
/// A refused creation surfaces its own error to the caller: native Windows
/// needs `SeCreateSymbolicLinkPrivilege` (developer mode or an elevated token)
/// for this call at all. Nothing here works around that by copying a tree
/// instead of linking it.
pub fn link_dir(source: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(source, link)
    }
}

/// Is `path` spelled ABSOLUTE in some host's own grammar?
///
/// **This exists because a remote node's root is not this process's path.** A
/// host root names something on a NODE that may not even run this OS, so a
/// validator that asks only `Path::is_absolute()` is asking THIS host's
/// question: on native Windows that call says no to `/srv/proj` (a root with no
/// drive prefix) and on Unix it says no to `C:\proj`. Neither path is wrong —
/// each is a wrong question, and the cost was real: a Windows node could not
/// record a POSIX node's roots at all.
///
/// So this answers the union of the two grammars the core targets:
///
/// * **POSIX**: a leading `/` — the only absolute form POSIX has, and also the
///   forward-slash spelling of a Windows root-relative path;
/// * **Windows**: a drive-qualified root (`C:\…`, `C:/…`) or the bare `C:`
///   drive root, and a UNC share (`\\server\share…`). `C:foo` is DRIVE-RELATIVE
///   on Windows and is deliberately NOT accepted here.
///
/// One implementation, no `cfg`: the predicate must give the same answer on
/// both hosts, which is the whole point of it.
pub fn looks_absolute_any_host(path: &str) -> bool {
    if path.is_empty() {
        return false;
    }
    // POSIX root, and Windows' own `/`-rooted spelling.
    if path.starts_with('/') {
        return true;
    }
    let bytes = path.as_bytes();
    // A drive-qualified root: `X:` where X is an ASCII letter.
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return bytes.len() == 2 || matches!(bytes[2], b'\\' | b'/');
    }
    // A UNC share, and the `\\?\` extended-length spelling of any path.
    path.starts_with(r"\\")
}

/// Is `path` inside the directory `root` — a DIRECTORY-boundary test, in the
/// separator and prefix grammar of whichever host spelled them?
///
/// The bug this closes was a hard-coded `/`: `cwd.starts_with("{root}/")` is
/// the POSIX separator, so on native Windows a session cwd of `C:\p\proj\sub`
/// under the root `C:\p\proj` matched NOTHING — the roster simply lost its
/// `project:… → session:… "anchors"` edge, silently, while every Linux run was
/// green. Both spellings are accepted on both hosts, because a path recorded by
/// one host's grammar can still name a directory on the other (a shared
/// checkout, a remote worktree).
///
/// **The verbatim prefix is stripped first,** dunce-style: `canonicalize`
/// returns `\\?\C:\p` on Windows, which compares unequal to the `C:\p` a
/// caller (or a record) spells for the same directory. Only the two shapes that
/// are representable without it are rewritten; anything else is left alone.
///
/// One seam: every project-prefix question in the tree asks this
/// (`conduct`'s `model::cwd_under`), so no caller re-derives the separator.
pub fn path_is_under(path: &str, root: &str) -> bool {
    let path = strip_verbatim_prefix(path);
    let root = strip_verbatim_prefix(root);
    if root.is_empty() {
        // An empty root is not a boundary: nothing is inside "".
        return false;
    }
    // `Path::starts_with` is a COMPONENT comparison, which is exactly the
    // relation wanted and is why no separator is spelled here: on Windows a
    // `/` and a `\` are the same separator (so a mixed pair still matches), on
    // Unix only `/` is, and a backslash in a Unix file name stays part of that
    // name. `C:\a\bc` is under `C:\a` (bc is an entry INSIDE a) while `C:\ab`
    // is not — component equality gets both right, and a string prefix with a
    // hard-coded `/` got the Windows half of that wrong for every single path.
    std::path::Path::new(path.as_ref()).starts_with(root.as_ref())
}

/// `\\?\C:\p` → `C:\p`, and `\\?\UNC\srv\share` → `\\srv\share`; every other
/// spelling is returned untouched. The verbatim form is an implementation
/// detail of the Windows API, not a name anyone spells, and leaving it in
/// breaks every comparison against a path that did not come from
/// `canonicalize`.
fn strip_verbatim_prefix(path: &str) -> std::borrow::Cow<'_, str> {
    let Some(rest) = path.strip_prefix(r"\\?\") else {
        return path.into();
    };
    match rest.strip_prefix(r"UNC\") {
        Some(share) => format!(r"\\{share}").into(),
        None => rest.into(),
    }
}

/// Create a PRIVATE file exclusively — `create_new(2)`'s semantics (a second
/// call for the same path fails `AlreadyExists`, never truncating what is
/// already there) with the private policy attached AT CREATION.
/// `spawn`'s write-once instruction sidecar
/// (`conduct/src/graph/spawn.rs::write_instructions_sidecar`) is the caller:
/// that file is written exactly once per session id by construction, so the
/// create-or-open shape [`atomic_write_private`] uses would be a silent
/// overwrite of the instructions a run was started with.
///
/// Unix: `OpenOptions::mode(0o600)` — created AT 0600, never narrowed after
/// (the same no-window ordering [`atomic_write_private`]'s doc argues).
/// Windows: `owner_only::create_new`, whose owner-only DACL is attached by the
/// `CreateFileW` that makes the file, then read back through `file_privacy`
/// BEFORE the first payload byte — the same readback
/// [`atomic_write_private`] performs on its temp, and for the same reason:
/// "created at 0600" is worth nothing if nobody asked the filesystem. The
/// `AlreadyExists` outcome is preserved one-for-one, because the caller's
/// taught refusal keys on it; and a readback that REFUSES removes the file it
/// just made before returning, so the caller's next attempt is a fresh one
/// rather than a taught "stale sidecar" refusal about this process's own file.
pub fn create_new_private(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        ensure_write_dir(parent)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
    }
    #[cfg(windows)]
    {
        let file = match aoide_protocol::owner_only::create_new(path) {
            Ok(file) => file,
            Err(aoide_protocol::owner_only::CreateError::Exists) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("{} already exists", path.display()),
                ))
            }
            Err(aoide_protocol::owner_only::CreateError::Failed(e)) => return Err(e),
        };
        match aoide_protocol::owner_only::file_privacy(path)? {
            None => Ok(file),
            Some(reason) => {
                // The file this call JUST created is removed before the refusal
                // returns: leaving it would make the caller's next attempt answer
                // `AlreadyExists` and print the taught "stale sidecar" refusal
                // about a file this process itself made a moment ago — a
                // confusing, self-inflicted state that only a host which cannot
                // persist the policy can reach.
                drop(file);
                let _ = std::fs::remove_file(path);
                Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!(
                        "refusing the private file {} this process just created: {reason} \
                         (this filesystem may not persist ACLs, and nothing here writes a private file it cannot prove is owner-only; the file was removed again)",
                        path.display()
                    ),
                ))
            }
        }
    }
}

/// Open `path` for APPEND, creating it if absent, with the private discipline
/// [`create_new_private`] states on the CREATE side — the shape a log that
/// grows across runs needs, where `create_new_private`'s one-shot `CREATE_NEW`
/// is the wrong question.
///
/// Unix: `OpenOptions::mode(0o600)` + `create(true)` + `append(true)` —
/// created AT 0600, and `append` means every write lands at the end whatever
/// else shares the file. Windows: the owner-only DACL is attached by the
/// `CreateFileW` that makes the file (`owner_only::create_new`, the same call
/// [`create_new_private`] makes), and an ALREADY-present file — the second
/// run of the same session id — is opened for append and then asked for its
/// privacy, refusing a file this process cannot prove is owner-only rather
/// than appending to something another user may read. `AlreadyExists` is not
/// a failure here: it is the normal second-run path.
///
/// **The two arms are the same discipline on CREATE, and Windows is stricter
/// on OPEN.** Unix's `mode(0o600)` applies only when the file is made, so an
/// existing file whose mode someone has since widened is appended to without a
/// readback; the Windows arm asks the existing file for its policy and refuses
/// a wider one. That asymmetry is deliberate rather than a missing check (the
/// directory is owner-only on both hosts, and the Unix arm's own `chmod`-style
/// readback is what [`atomic_write_private`] performs for the case that needs
/// it), and it is stated here because the shorter claim — "the same
/// discipline" — is only true of the create branch.
pub fn open_private_append(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        ensure_write_dir(parent)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(path)
    }
    #[cfg(windows)]
    {
        match aoide_protocol::owner_only::create_new(path) {
            Ok(file) => match aoide_protocol::owner_only::file_privacy(path)? {
                None => Ok(file),
                Some(reason) => {
                    drop(file);
                    let _ = std::fs::remove_file(path);
                    Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!(
                            "refusing the private file {} this process just created: {reason} \
                             (this filesystem may not persist ACLs, and nothing here writes a \
                             private file it cannot prove is owner-only; the file was removed \
                             again)",
                            path.display()
                        ),
                    ))
                }
            },
            Err(aoide_protocol::owner_only::CreateError::Exists) => {
                let file = std::fs::OpenOptions::new().append(true).open(path)?;
                match aoide_protocol::owner_only::file_privacy(path)? {
                    None => Ok(file),
                    Some(reason) => Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!(
                            "refusing to append to {}: {reason} (this file is not provably \
                             owner-only, and nothing here appends to a private log it cannot \
                             prove is private)",
                            path.display()
                        ),
                    )),
                }
            }
            Err(aoide_protocol::owner_only::CreateError::Failed(e)) => Err(e),
        }
    }
}

/// Create `dir` (if absent) and lock it down to `0700` (owner rwx only) —
/// the directory-level half of [`atomic_write_private`]'s discipline
/// (review rider: nothing else in this crate secured the DIRECTORY a
/// private file lives in, only the file itself, so a state dir left at
/// `create_dir_all`'s umask-derived default — 0755 under this box's normal
/// 022 — would leave `identity/`'s directory entries world-LISTABLE even
/// with `ed25519.key` itself locked to 0600). Mirrors `aoide-secrets`'s
/// `home::secure_dir` for one directory inside this crate's own state tree
/// rather than that crate's whole secrets home. `create_dir_all` is
/// idempotent on an already-present directory, and so is the chmod that
/// follows it — calling this on every mint (not only the very first one)
/// costs nothing and never regresses a directory some earlier run already
/// locked down. On Unix the chmod acts on a descriptor opened without
/// following a link at `dir` itself, so a `dir` that is a symlink is refused
/// and its target keeps its mode. A `dir` with a `..` in it is refused before
/// anything is created or changed, since the directory it climbs to is not one
/// this function was asked to own.
pub fn secure_private_dir(dir: &std::path::Path) -> std::io::Result<()> {
    if dir.components().any(|part| part == std::path::Component::ParentDir) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "{}: refusing to make it owner-only, its path has a `..` in it; name the directory itself",
                dir.display()
            ),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure_write_dir(dir)?;
        let opened = open_dir_nofollow(dir).map_err(|e| match e.raw_os_error() {
            Some(libc::ELOOP) | Some(libc::ENOTDIR) => std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "{}: refusing to make it owner-only, it is a symlink or not a directory; replace it with a real directory",
                    dir.display()
                ),
            ),
            _ => e,
        })?;
        opened.set_permissions(std::fs::Permissions::from_mode(0o700))
    }
    // Windows: the ancestors go through `ensure_write_dir` (their mode is not
    // this function's business), and the leaf gets its policy attached AT
    // creation, then read back. `ensure_private_dir` tightens an existing
    // directory that is not private yet, and refuses one whose policy the
    // filesystem would not honor.
    #[cfg(windows)]
    {
        if let Some(parent) = dir.parent() {
            ensure_write_dir(parent)?;
        }
        aoide_protocol::owner_only::ensure_private_dir(dir)
    }
}

/// Create `dir` and its missing ancestors for a write. A `dir` under
/// [`state_dir`] locks `state/` owner-only first; anywhere else it is a plain
/// `create_dir_all`. Under is judged by name and again by where the kernel
/// will put `dir` ([`resolve_nearest`]), so a path through a symlinked alias,
/// or one with a `..` in it, is under `state/` when it lands there.
pub fn ensure_write_dir(dir: &std::path::Path) -> std::io::Result<()> {
    let state = state_dir();
    if dir.starts_with(&state) || resolve_nearest(dir).starts_with(resolve_nearest(&state)) {
        lock_state_dir(&state)?;
    }
    std::fs::create_dir_all(dir)
}

/// The directory `state/` may never be, judged on the paths it is given:
/// [`lock_state_dir`] passes them all canonical, so a symlink cannot name one
/// of these by another spelling. A filesystem root, or a home directory or one
/// of its parents: a chmod there would lock a directory aoide does not own.
fn shallow_state_reason(state: &std::path::Path, homes: &[std::path::PathBuf]) -> Option<&'static str> {
    if state.parent().is_none() {
        return Some("it is a filesystem root");
    }
    if homes.iter().any(|home| home.starts_with(state)) {
        return Some("it is the home directory or one of its parents");
    }
    None
}

/// Every home the shallow rule protects, each with its parents: a non-empty
/// `$HOME`, the passwd home of the effective uid, and the home `$AOIDE_USER`
/// names (its passwd home, `/home/<user>` without an entry) when non-empty. A
/// run may mean any of them, so all are guarded together.
#[cfg(unix)]
fn guard_homes() -> Vec<std::path::PathBuf> {
    let named = |var: &str| std::env::var_os(var).filter(|v| !v.is_empty());
    let home = named("HOME").map(std::path::PathBuf::from);
    let user = named("AOIDE_USER").map(|user| aoide_protocol::audit::user_home(&user.to_string_lossy()));
    home.into_iter().chain(aoide_protocol::audit::passwd_home()).chain(user).collect()
}

#[cfg(windows)]
fn guard_homes() -> Vec<std::path::PathBuf> {
    vec![aoide_protocol::aoide_home()]
}

/// Where the kernel puts `path`: its longest existing prefix resolved through
/// `canonicalize`, so every symlink and `..` on that prefix is followed, and
/// the components below it, which do not exist yet, applied as written. A
/// `..` among those steps back over the one before it, since that one will be
/// a real directory `create_dir_all` makes (`/a/missing/../state` lands at
/// `/a/state`).
fn resolve_nearest(path: &std::path::Path) -> std::path::PathBuf {
    let mut missing = Vec::new();
    let mut prefix = path;
    loop {
        if let Ok(real) = prefix.canonicalize() {
            return missing.iter().rev().fold(real, |mut acc, part| {
                if *part == std::ffi::OsStr::new("..") {
                    acc.pop();
                } else {
                    acc.push(part);
                }
                acc
            });
        }
        match (prefix.parent(), prefix.components().next_back()) {
            (Some(parent), Some(last)) => {
                missing.push(last.as_os_str());
                prefix = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// `(dev, ino)` of `/` and of each guarded home with each of its parents: the
/// objects [`owner_only_state`] will not change once it has opened one.
#[cfg(unix)]
fn shallow_identities(homes: &[std::path::PathBuf]) -> Vec<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    homes
        .iter()
        .flat_map(|home| home.ancestors())
        .chain(std::iter::once(std::path::Path::new("/")))
        .filter_map(|dir| std::fs::metadata(dir).ok())
        .map(|meta| (meta.dev(), meta.ino()))
        .collect()
}

/// Locks `state/` owner-only, or refuses, naming the path and the reason. A
/// `state` that is relative, has a `..` in it, or is shallow is refused before
/// anything is created or changed; the symlink and permission cases are
/// decided by [`owner_only_state`] on the object it opens.
fn lock_state_dir(state: &std::path::Path) -> std::io::Result<()> {
    if !state.is_absolute() {
        return Err(refused(
            state,
            "its path is relative; set AOIDE_ROOT or AOIDE_STATE_DIR to an absolute path, such as ~/.aoide",
        ));
    }
    if state.components().any(|part| part == std::path::Component::ParentDir) {
        return Err(refused(state, "its path has a `..` in it; name the directory itself, such as ~/.aoide/state"));
    }
    let homes: Vec<_> = guard_homes().iter().map(|home| resolve_nearest(home)).collect();
    if let Some(why) = shallow_state_reason(&resolve_nearest(state), &homes) {
        return Err(refused(
            state,
            &format!("{why}; point AOIDE_STATE_DIR (or AOIDE_ROOT) at a directory aoide owns, such as ~/.aoide/state"),
        ));
    }
    owner_only_state(state, &homes)
}

/// The one refusal a state lock returns: the path, the reason, and the fix.
fn refused(state: &std::path::Path, why: &str) -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("{}: refusing to make state/ owner-only, {why}", state.display()),
    )
}

/// A lock the OS would not make, with its kind kept. A permission refusal
/// names the one fix an owner can make without aoide: a `state/` this user
/// already owns needs its own permission bits back (the open reads it), one
/// another user owns needs a chown.
fn unlockable(state: &std::path::Path, e: std::io::Error) -> std::io::Error {
    let hint = if e.kind() != std::io::ErrorKind::PermissionDenied {
        String::new()
    } else if owned_by_me(state) {
        format!("; chmod u+rwx {}", state.display())
    } else {
        "; chown it to you or point AOIDE_ROOT elsewhere".to_string()
    };
    std::io::Error::new(
        e.kind(),
        format!("{}: state/ cannot be made owner-only (0700): {e}{hint}", state.display()),
    )
}

/// Whether `path` itself (never a link's target) is owned by the effective uid.
#[cfg(unix)]
fn owned_by_me(path: &std::path::Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: `geteuid` takes nothing and cannot fail.
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.uid() == unsafe { libc::geteuid() })
}

#[cfg(windows)]
fn owned_by_me(_path: &std::path::Path) -> bool {
    false
}

/// Unix: opens `state` (see [`open_state`]), refuses the object it opened if it
/// is a shallow one (the path checks ran on names, and a name can be swapped
/// between those checks and this open), and chmods that object to `0700` when
/// it is wider. Windows: the owner-only policy is attached only when this call
/// creates `state/`. An existing `state/` is never re-tightened, since that
/// would strip the inherited ACEs of children already in it, which is not
/// verified here.
#[cfg(unix)]
fn owner_only_state(state: &std::path::Path, homes: &[std::path::PathBuf]) -> std::io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let dir = open_state(state)?;
    let meta = dir.metadata().map_err(|e| unlockable(state, e))?;
    if shallow_identities(homes).contains(&(meta.dev(), meta.ino())) {
        return Err(refused(state, "it resolves to the filesystem root, the home directory or one of its parents"));
    }
    if meta.mode() & 0o777 != 0o700 {
        dir.set_permissions(std::fs::Permissions::from_mode(0o700))
            .map_err(|e| unlockable(state, e))?;
    }
    Ok(())
}

/// Opens `state` as a read-only directory without following a link at its
/// last component (`O_NOFOLLOW`), and creates it `0700` first when absent. A
/// link fails `ELOOP` and anything that is not a directory `ENOTDIR`; both are
/// the symlink refusal. The chmod acts on this descriptor, never on the name.
#[cfg(unix)]
fn open_state(state: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::DirBuilderExt;
    let open = || open_dir_nofollow(state);
    let opened = match open() {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = state.parent() {
                std::fs::create_dir_all(parent).map_err(|e| unlockable(state, e))?;
            }
            match std::fs::DirBuilder::new().mode(0o700).create(state) {
                Ok(()) => {}
                // A racing creator's directory is opened as it is; the open decides.
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(unlockable(state, e)),
            }
            open()
        }
        other => other,
    };
    opened.map_err(|e| match e.raw_os_error() {
        Some(libc::ELOOP) | Some(libc::ENOTDIR) => refused(
            state,
            "state/ is a symlink, or not a directory; aoide keeps its private state in a real directory, so replace it with one or point AOIDE_ROOT elsewhere",
        ),
        _ => unlockable(state, e),
    })
}

/// `dir` opened read-only as a directory, refusing a link at its last component:
/// `ELOOP` for a link, `ENOTDIR` for anything that is not a directory.
#[cfg(unix)]
fn open_dir_nofollow(dir: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(dir)
}

#[cfg(windows)]
fn owner_only_state(state: &std::path::Path, _homes: &[std::path::PathBuf]) -> std::io::Result<()> {
    if std::fs::symlink_metadata(state).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(refused(
            state,
            "state/ is a symlink; aoide keeps its private state in a real directory, so replace it with one or point AOIDE_ROOT elsewhere",
        ));
    }
    if std::fs::metadata(state).is_ok() {
        return Ok(());
    }
    if let Some(parent) = state.parent() {
        std::fs::create_dir_all(parent).map_err(|e| unlockable(state, e))?;
    }
    aoide_protocol::owner_only::ensure_private_dir(state).map_err(|e| unlockable(state, e))
}

/// The file every stage lock is named by, in whichever directory holds it.
const STAGE_LOCK_FILE: &str = ".stage.lock";

thread_local! {
    /// The stage lock files this thread holds, by canonical key ([`lock_key`]).
    /// A nested [`with_lock_in`] on a held key runs inside the outer hold, never
    /// past it.
    static HELD_LOCKS: std::cell::RefCell<Vec<std::path::PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The identity a held stage lock is recorded under: its directory canonicalised,
/// so two spellings of one directory are one lock. (`flock` is per open file
/// description: a second fd on the same file, by any spelling, would block on a
/// lock this thread owns.) A directory that does not exist yet keeps its spelling;
/// nothing can be held on it.
fn lock_key(dir: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()).join(STAGE_LOCK_FILE)
}

fn holds_lock(dir: &std::path::Path) -> bool {
    let key = lock_key(dir);
    HELD_LOCKS.with(|h| h.borrow().contains(&key))
}

/// Records a held stage lock for this thread until it drops. Created only after a
/// successful `flock`, and dropped before the lock is released.
struct HeldLock(std::path::PathBuf);
impl HeldLock {
    fn take(dir: &std::path::Path) -> Self {
        let key = lock_key(dir);
        HELD_LOCKS.with(|h| h.borrow_mut().push(key.clone()));
        HeldLock(key)
    }
}
impl Drop for HeldLock {
    fn drop(&mut self) {
        HELD_LOCKS.with(|h| {
            let mut h = h.borrow_mut();
            if let Some(at) = h.iter().rposition(|k| k == &self.0) {
                h.remove(at);
            }
        });
    }
}

/// Runs `f` under the CONDUCTING stage's lock (`state/stage/.stage.lock`, see
/// [`conducting_stage_dir`]), held for the closure's duration. Every core writer
/// of the six roster files and the `state/` records takes it.
///
/// Best-effort, and re-entrant on the thread that already holds it: a nested
/// call runs its closure directly. Every other thread still waits, and two
/// processes still serialise. If the lock file cannot be created or locked, `f`
/// runs unlocked rather than blocking on a hiccup, and nothing is recorded as
/// held. The rice stage has its own lock on its own directory ([`with_lock_in`]),
/// and no file is written under both, so the two never guard the same file.
pub fn with_stage_lock<T>(f: impl FnOnce() -> T) -> T {
    with_lock_in(&conducting_lock_dir(), f)
}

/// Runs `f` under the exclusive lock on `dir/.stage.lock`, best-effort and
/// re-entrant per lock file (see [`with_stage_lock`]). A domain that owns a stage
/// directory of its own locks it through here: the rice stage does, through
/// `strillix-rice`'s `lock`.
pub fn with_lock_in<T>(dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
    if holds_lock(dir) {
        return f();
    }
    // The lock file is a write like any other: when `dir` cannot be created it is
    // not, and `f` runs unlocked as the best-effort path says.
    let lock = ensure_write_dir(dir).ok().and_then(|()| {
        std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(dir.join(STAGE_LOCK_FILE))
            .ok()
    });
    let held = lock.as_ref().map(lock_exclusive).unwrap_or(false);
    // RAII so an unwind inside `f` clears the record with the fd, never after it:
    // declared AFTER `lock`, so it drops BEFORE it. The normal path releases the
    // lock through the explicit `unlock` below, and the record drops after it,
    // with nothing else of this thread running in between.
    let _held = held.then(|| HeldLock::take(dir));
    let out = f();
    if held {
        if let Some(f) = &lock {
            unlock(f);
        }
    }
    out
}

/// Whether THIS thread holds the conducting stage's lock right now: true inside
/// [`with_stage_lock`] or [`try_stage_lock`] on a successful lock.
pub fn stage_lock_held() -> bool {
    holds_lock(&conducting_lock_dir())
}

/// Fail-closed sibling of [`with_stage_lock`], and re-entrant the same way: a
/// nested call under a held lock runs `f` directly. Mail's base-log append
/// (MAIL.md, "Store") must never let two writers interleave two entries, so a
/// lock that can't be taken has to reject the write rather than run it anyway.
/// Blocks on `LOCK_EX` (a concurrent opener waits, per the architect's ruling
/// that a second writer queues rather than races) — it only returns `Err` when
/// the lock file itself can't be opened or created, not when another writer
/// briefly holds it. The conducting stage's lock.
pub fn try_stage_lock<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    let dir = conducting_lock_dir();
    if holds_lock(&dir) {
        return Ok(f());
    }
    lock_path(dir.join(STAGE_LOCK_FILE), || {
        let _held = HeldLock::take(&dir);
        f()
    })
}

/// Fail-closed `flock(LOCK_EX)` on a dedicated file at `path`, held for the
/// closure's duration: open-or-create, lock (blocking — a concurrent opener
/// waits rather than races), run `f`, unlock. Shared by [`try_stage_lock`]
/// (its lock file is always the conducting stage's `.stage.lock`) and mail's doorbell
/// ring (`aoide_storage::mail::with_ring_lock`, whose lock file is
/// `mail_dir()/.ring.lock` — a SEPARATE file: the ring is held across socket
/// I/O for the whole select-inject-stamp sequence, far longer than any stage
/// mutator ever holds `.stage.lock`, so the two must never share one file or
/// a slow ring would stall every session-graph writer on the desktop).
/// Not re-entrant (each call opens its own fd) — a caller must never nest two
/// calls against the same path.
pub(crate) fn lock_path<T>(path: std::path::PathBuf, f: impl FnOnce() -> T) -> Result<T, String> {
    if let Some(dir) = path.parent() {
        ensure_write_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| format!("cannot open lock file {}: {e}", path.display()))?;
    if !lock_exclusive(&lock) {
        return Err(format!(
            "cannot lock {}: {}",
            path.display(),
            std::io::Error::last_os_error()
        ));
    }
    let out = f();
    unlock(&lock);
    Ok(out)
}

/// Is `pid` a live process? The one liveness probe in core: POSIX `kill(pid, 0)`
/// — `0`/`EPERM` live, `ESRCH` absent, any other errno conservatively live. Live
/// is not identity: a recycled pid is live.
///
/// **Windows**: `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` +
/// `GetExitCodeProcess` out of `aoide_protocol::win_proc`, with the same
/// verdicts one-for-one (`ERROR_INVALID_PARAMETER`/`ERROR_NOT_FOUND` absent,
/// `ERROR_ACCESS_DENIED` live, anything unanswerable live) and the same
/// refusal of a pid that cannot name one process. The reading is the
/// contract; the syscall under it is the host's.
pub fn pid_is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let Some(pid) = probeable_pid(pid) else {
            return false;
        };
        // SAFETY: signal 0 is never delivered; the call only asks whether `pid`
        // exists and whether this process may signal it.
        let rc = unsafe { libc::kill(pid, 0) };
        if rc == 0 {
            return true;
        }
        return probe_verdict(std::io::Error::last_os_error().raw_os_error());
    }
    #[cfg(windows)]
    {
        aoide_protocol::win_proc::is_alive(pid)
    }
}

/// `pid` as a `pid_t`, or `None` for `0`/`pid > pid_t::MAX` — both name a
/// process GROUP, never one process, so `kill` is never handed either.
#[cfg(unix)]
fn probeable_pid(pid: u32) -> Option<libc::pid_t> {
    (pid > 0 && pid <= libc::pid_t::MAX as u32).then_some(pid as libc::pid_t)
}

/// A failed `kill(pid, 0)`: `ESRCH` alone is absent; every other errno (and no
/// errno at all) is conservatively live, so an unanswerable probe never reaps.
#[cfg(unix)]
fn probe_verdict(errno: Option<i32>) -> bool {
    errno != Some(libc::ESRCH)
}

/// End `pid` — the one act core performs on a process it did not spawn.
///
/// **The guarantee is the host's, and it is NOT the same fact on both** (the
/// host-split `docs/architecture/CORE-POSIX.md`'s process row records):
///
/// | | Unix | native Windows |
/// | --- | --- | --- |
/// | primitive | `SIGTERM` | `TerminateProcess` |
/// | what it is | a REQUEST a child may catch | the only primitive there is |
/// | can the child decline? | yes — a trapped, hung or broken child survives it | no |
/// | a clean shutdown | possible: `ssh` can unwind on its own | impossible: the child is gone mid-anything |
/// | when this returns | the request was delivered | the child has ended, or the call failed for want of access |
///
/// So the name is the one word both hosts call their own primitive by, and
/// the ONE thing it promises a caller on either host is **"asked to end, by
/// this host's own means"** — never "cleanly", and on Unix not even "ended":
/// only [`wait_for_exit`]/[`pid_is_alive`] answer that. A caller must be
/// written for the weaker arm (a survivor is possible) and take the stronger
/// one as a bonus; `client::tunnel`'s record-keeping does exactly that, and
/// its module doc says why. **What makes a hard kill acceptable on the
/// Windows arm is the caller's own guard, not this function**: every caller
/// reaches it only after confirming the pid is alive AND its argv is that
/// record's own `ssh … -L <local>:<host>:<remote>` child.
///
/// Failures are deliberately swallowed at every arm — an already-dead pid is
/// the ordinary case this is called in, and a caller's real question is the
/// one [`wait_for_exit`] asks next.
pub fn terminate(pid: u32) {
    #[cfg(unix)]
    {
        if let Some(pid) = probeable_pid(pid) {
            // SAFETY: `pid` is a positive `pid_t` (never a group name), and
            // `SIGTERM` is the one signal a caller here is documented to send.
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
        }
    }
    #[cfg(windows)]
    {
        let _ = aoide_protocol::win_proc::terminate(pid);
    }
}

/// Wait for `pid` to end: `waitpid(pid, …, WNOHANG)`'s three verdicts, with
/// `block = false` (the `WNOHANG` form) or a real `wait(2)` (`block = true`).
///
/// Unix: a REAL wait, so when `pid` is this process's own child the zombie is
/// collected here. Native Windows: `WaitForSingleObject` on a `SYNCHRONIZE`
/// handle — no zombie exists there (the process object is signalled when the
/// process ends and freed with its last handle), and a wait is not restricted
/// to this process's own children, so a pid an EARLIER invocation spawned is
/// still waitable. The three verdicts map one-for-one; see
/// `aoide_protocol::win_proc::wait_for_exit` for the table.
pub fn wait_for_exit(pid: u32, block: bool) -> Waited {
    #[cfg(unix)]
    {
        let Some(pid) = probeable_pid(pid) else {
            return Waited::NotWaitable;
        };
        let mut status: libc::c_int = 0;
        let flags = if block { 0 } else { libc::WNOHANG };
        // SAFETY: `pid` is a positive `pid_t`; `&mut status` is a valid local
        // that `waitpid` writes on a successful reap; `WNOHANG` never blocks.
        let r = unsafe { libc::waitpid(pid, &mut status, flags) };
        if r == pid {
            return Waited::Exited;
        }
        if r == 0 {
            return Waited::Running; // WNOHANG: still running.
        }
        // A negative return (`ECHILD` above all: not, or no longer, a child of
        // this process) means no wait can ever succeed for this pid here.
        Waited::NotWaitable
    }
    #[cfg(windows)]
    {
        match aoide_protocol::win_proc::wait_for_exit(pid, block) {
            aoide_protocol::win_proc::Waited::Exited => Waited::Exited,
            aoide_protocol::win_proc::Waited::Running => Waited::Running,
            aoide_protocol::win_proc::Waited::NotWaitable => Waited::NotWaitable,
        }
    }
}

/// The three verdicts of [`wait_for_exit`]: the process ended, it is still
/// running, or no wait can ever succeed for this pid from this process (the
/// `ECHILD`/unopenable case) — in which case a caller falls back to
/// [`pid_is_alive`] polling, exactly as its Unix arm always has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waited {
    /// The process has ended (and, on Unix, was reaped).
    Exited,
    /// It is still running (only ever the non-blocking form's answer).
    Running,
    /// No wait can succeed here.
    NotWaitable,
}

/// Remove leaked atomic-write temporaries from `path`'s DIRECTORY: any sibling
/// named `<stem>.tmp.<pid>` whose `<pid>` is no longer a live process, whatever
/// its stem. An atomic write interrupted between create and rename (SIGKILL,
/// power-loss, a write that failed mid-flight) can never unlink its own temp — a
/// stale `graph.tmp.464255` sat on disk from a prior day — so the next successful
/// writer of ANY file in that directory sweeps it.
///
/// The sweep is directory-wide, not stem-scoped, because a stem-scoped one only
/// ever reclaims a temp whose own file gets written again: 1519 zero-byte
/// `<msgid>.tmp.<pid>` orphans accumulated in one node's `state/outbox/`, each
/// keyed by a msgid that is minted once and never rewritten, so none of them was
/// reachable. This costs nothing extra — the `read_dir` was already being paid on
/// every write; the old predicate simply threw away every orphan but its own.
///
/// `.tmp.` is matched as a whole separator, so the deliberately distinct
/// `<stem>.migrate-tmp.<pid>` and `<stem>.seed.<pid>` temps stay clear of it.
/// Best-effort and total: any read/parse/remove miss is ignored, and our OWN
/// in-flight temp (live pid) plus every other file are left untouched, so a
/// concurrent node's write is safe.
fn sweep_stale_temps(path: &std::path::Path) {
    let Some(dir) = path.parent() else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some((_, pid_str)) = name.rsplit_once(".tmp.") else {
            continue;
        };
        // Only a well-formed `<stem>.tmp.<pid>` whose pid is dead is swept; our
        // own in-flight temp (same pid) and any non-numeric suffix are spared.
        if let Ok(pid) = pid_str.parse::<u32>() {
            if pid != std::process::id() && !pid_is_alive(pid) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

/// Rename `from` → `to` ONLY if `to` does not already exist — an atomic,
/// no-clobber move. `Ok(true)` renamed; `Ok(false)` the target already existed (a
/// concurrent writer beat us); `Err` any other failure. Linux
/// `renameat2(RENAME_NOREPLACE)` is the atomic primitive [`seed_if_absent`] needs
/// so its file-absent seed can never overwrite a roster that raced in.
fn rename_no_replace(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let cfrom = std::ffi::CString::new(from.as_os_str().as_bytes())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let cto = std::ffi::CString::new(to.as_os_str().as_bytes())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
        let rc = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                cfrom.as_ptr(),
                libc::AT_FDCWD,
                cto.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if rc == 0 {
            return Ok(true);
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EEXIST) {
            Ok(false) // target already there — the concurrent registration wins.
        } else {
            Err(err)
        }
    }
    // Windows' no-clobber move is `MoveFileExW` WITHOUT
    // MOVEFILE_REPLACE_EXISTING — the same three answers, the same meaning.
    #[cfg(windows)]
    {
        fs_windows::rename_no_replace(from, to)
    }
}

/// Seed a stage registry ONLY when it is absent or unreadable — a valid existing
/// file (parses as a JSON object carrying the `key` array) is PRESERVED untouched.
///
/// This is the restart-survival seam. shellbridge is `partOf
/// graphical-session.target` (modules/nucleus/shellbridge.nix), which
/// `BindsTo` the Hyprland session — so a nixos switch / compositor restart
/// STOPS+STARTS shellbridge. The old unconditional re-seed wiped every live
/// session to `[]` on each such restart, and a record can only re-register from a
/// NEW `graph session start`, so already-running claude/conduct sessions vanished
/// from the roster/DAG until they happened to re-emit. Preserving a valid file
/// keeps live sessions across the restart window; a missing/half-written/corrupt
/// file still gets the empty v0 shape so the bridge always comes up sane. Returns
/// the written path when it (re)seeded, `None` when it preserved an existing one
/// or could not create the file's directory (narrated to stderr; nothing is
/// written).
///
/// The file-ABSENT path is race-safe: on a fresh boot a concurrent `graph session
/// start` can land a populated roster BETWEEN our read and our write, so we seed
/// with atomic create-if-absent ([`rename_no_replace`]) — if that roster appeared
/// first the no-replace rename refuses to clobber it and the registration wins.
/// (A corrupt file racing a writer is fine: there is no live roster to lose, and
/// the valid-file path never writes at all, so the restart bug stays closed.)
///
/// `pub`, not private: root `shellbridge::run()` (the socket-loop code that
/// stays in root through Phase 3b) is this function's only caller outside this
/// crate, so it must cross the crate boundary.
pub fn seed_if_absent(path: &std::path::Path, empty_body: &str, key: &str) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(existing) => {
            // A file is present. PRESERVE a valid registry (parses as an object
            // whose `key` is an ARRAY); replace anything else — non-JSON, or
            // JSON of the wrong shape (`"sessions": {}`, or the key missing) —
            // with the empty v0 body. No live roster is at stake here, so the
            // plain atomic write (last-writer-wins) is acceptable.
            let valid = serde_json::from_str::<serde_json::Value>(&existing)
                .ok()
                .map(|v| {
                    v.get(key)
                        .map(serde_json::Value::is_array)
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if valid {
                return None; // a real registry lives here — never clobber it.
            }
            atomic_write(path, empty_body)
                .ok()
                .map(|_| path.to_string_lossy().into_owned())
        }
        Err(_) => {
            // Absent (or unreadable). Seed with create-if-absent so a roster that
            // appeared mid-boot is never overwritten. A distinct `.seed.<pid>`
            // temp keeps it clear of `sweep_stale_temps`'s `.tmp.<pid>` pattern.
            if let Some(parent) = path.parent() {
                if let Err(e) = ensure_write_dir(parent) {
                    eprintln!("aoide: cannot create {} ({e}) — {} not seeded", parent.display(), path.display());
                    return None;
                }
            }
            let tmp = path.with_extension(format!("seed.{}", std::process::id()));
            let seeded = (|| -> std::io::Result<bool> {
                {
                    let mut f = std::fs::File::create(&tmp)?;
                    f.write_all(empty_body.as_bytes())?;
                    f.sync_all()?;
                }
                rename_no_replace(&tmp, path)
            })();
            match seeded {
                Ok(true) => Some(path.to_string_lossy().into_owned()),
                // Ok(false) → a concurrent writer won the race; Err → IO failure.
                // Either way drop our temp and leave whatever is in place.
                _ => {
                    let _ = std::fs::remove_file(&tmp);
                    None
                }
            }
        }
    }
}

// ── Tests (the AOIDE_STAGE_DIR precedence seam; CONTRACTS.md §4) ─────────────
//
// These exercise the conducting-stage resolution, `seed_if_absent` and
// `atomic_write`, which are defined in this module.

#[cfg(test)]
mod tests {
    /// The ONE seam every project-prefix question now goes through, asserted in
    /// BOTH hosts' grammars at once — so its answer cannot depend on which host
    /// is asking, and a Windows cwd under a Windows root matches exactly as a
    /// POSIX pair does. (The live defect: a hard-coded `/` boundary, so on
    /// native Windows no cwd ever matched a root and the roster silently lost
    /// every project "anchors" edge.)
    #[test]
    fn path_is_under_knows_both_hosts_grammars() {
        // POSIX grammar — a `/` is a separator on every host this core runs,
        // so these hold wherever the suite runs.
        assert!(super::path_is_under("/srv/proj/sub", "/srv/proj"));
        assert!(super::path_is_under("/srv/proj/", "/srv/proj"));
        assert!(super::path_is_under("/srv/proj", "/srv/proj"));
        assert!(!super::path_is_under("/srv/projX", "/srv/proj"));
        assert!(!super::path_is_under("/srv/proj", "/srv/projX"));
        assert!(super::path_is_under("/srv/x", "/"));
        assert!(!super::path_is_under("proj/sub", "/srv/proj"));
        assert!(!super::path_is_under("/srv/proj", ""));

        // The verbatim-prefix strip is pure string work, so it is asserted
        // here too: `canonicalize` mints `\\?\…` on Windows, and that spelling
        // must keep naming the same directory as the plain one.
        assert_eq!(super::strip_verbatim_prefix(r"\\?\C:\p").as_ref(), r"C:\p");
        assert_eq!(super::strip_verbatim_prefix(r"\\?\UNC\srv\share").as_ref(), r"\\srv\share");
        assert_eq!(super::strip_verbatim_prefix("/srv/p").as_ref(), "/srv/p");

        // Windows grammar — which separator is a separator is decided by THIS
        // host's own component rules, and the defect being closed (a hard-coded
        // `/`) lived entirely on the Windows side: no cwd ever matched a root
        // there, so every project "anchors" edge was silently absent.
        #[cfg(windows)]
        {
            assert!(super::path_is_under(r"C:\p\proj\sub", r"C:\p\proj"));
            assert!(super::path_is_under(r"C:\p\proj", r"C:\p\proj"));
            assert!(super::path_is_under(r"C:\p\proj\", r"C:\p\proj"));
            assert!(super::path_is_under("C:/p/proj/sub", r"C:\p\proj"));
            assert!(!super::path_is_under(r"C:\p\projX", r"C:\p\proj"));
            assert!(!super::path_is_under(r"C:\p\other", r"C:\p\proj"));
            assert!(super::path_is_under(r"\\?\C:\p\proj\sub", r"C:\p\proj"));
            assert!(super::path_is_under(r"\\?\C:\p\proj\sub", r"\\?\C:\p\proj"));
            assert!(!super::path_is_under(r"\\?\C:\p\projX", r"C:\p\proj"));
            assert!(super::path_is_under(r"\\?\UNC\srv\share\a", r"\\srv\share"));
        }
        // On Unix those are one FILE NAME each (a backslash is an ordinary
        // byte there), so no path relation is claimed and none is invented.
        #[cfg(unix)]
        {
            assert!(!super::path_is_under(r"C:\p\proj\sub", r"C:\p\proj"));
            assert!(super::path_is_under(r"/srv/a\b/c", r"/srv/a\b"));
            assert!(!super::path_is_under(r"/srv/a\bc", r"/srv/a\b"));
        }
    }
    use super::*;
    use aoide_test_support::MigrationEnvGuard;


    /// A scratch path that IS absolute on THIS host, as a `String` for the
    /// env-override APIs. The Unix literals these tests used to write
    /// (`/tmp/...`) are not absolute on Windows — `Path::is_absolute` there
    /// wants a drive or UNC prefix — so the override-resolution code would
    /// silently ignore the value under test and the assertion would then
    /// compare against the FALLBACK path instead of the override.
    /// `temp_dir()` is absolute and writable on every host, so the same
    /// assertion still proves the same thing: an absolute override wins and
    /// a relative one is declined.
    fn scratch(name: &str) -> String {
        std::env::temp_dir().join(name).to_string_lossy().to_string()
    }

    #[test]
    fn with_stage_lock_nests_for_one_thread_and_still_releases_on_unwind() {
        // A nested call on the thread that already holds `.stage.lock` runs its
        // closure directly — asserted from INSIDE both closures, so the test
        // tells re-entrancy apart from "the lock was never taken" (`held` false
        // would lock on its own and the outer asserts would still pass).
        // `aoide-conduct`'s prunes are the callers that need it. `flock` is per
        // open-file-description, so without the flag that second fd would block
        // on a lock this same thread owns — a deadlock rather than a wait.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let stage = std::env::temp_dir().join(format!("aoide-stage-lock-nest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&stage);
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        assert_eq!(
            with_stage_lock(|| {
                assert!(stage_lock_held(), "the outer call holds it");
                with_stage_lock(|| {
                    assert!(
                        stage_lock_held(),
                        "a nested call runs INSIDE the outer hold, not past it"
                    );
                    7
                })
            }),
            7
        );
        assert!(!stage_lock_held(), "released on the way out");

        // An unwind inside the outer closure clears the flag with the fd: a
        // later call still locks (rather than silently running unlocked).
        let panicked = std::panic::catch_unwind(|| with_stage_lock(|| panic!("boom")));
        assert!(panicked.is_err());
        assert!(!stage_lock_held());

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }


    #[test]
    fn flake_root_ignores_the_stage_dir_override_and_honors_its_own() {
        // `flake_root` is deliberately NOT `AOIDE_STAGE_DIR`-relative (unlike
        // every other path above) — see its own doc for why: the committed
        // git checkout `nix eval` reads doesn't move just because a test
        // relocated the runtime trees.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
        let saved_flake = std::env::var("AOIDE_FLAKE_ROOT").ok();

        std::env::set_var("AOIDE_STAGE_DIR", scratch("aoide-flake-root-test/rice/stage"));
        std::env::remove_var("AOIDE_FLAKE_ROOT");
        assert!(
            !flake_root().starts_with(scratch("aoide-flake-root-test")),
            "an AOIDE_STAGE_DIR relocation must not move flake_root: {:?}",
            flake_root()
        );
        assert!(flake_root().ends_with("Aoide"));

        std::env::set_var("AOIDE_FLAKE_ROOT", scratch("aoide-flake-root-test/fixture-flake"));
        assert_eq!(
            flake_root(),
            std::path::PathBuf::from(scratch("aoide-flake-root-test/fixture-flake")),
            "an explicit AOIDE_FLAKE_ROOT wins outright"
        );

        match saved_stage {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        match saved_flake {
            Some(v) => std::env::set_var("AOIDE_FLAKE_ROOT", v),
            None => std::env::remove_var("AOIDE_FLAKE_ROOT"),
        }
    }


    // ── `rice_templates_dir` (L-C3, strillix-carrier lane, task #107) ──────────


    // ── The restart-survival seam: seed preserves a live roster ──────────────

    #[test]
    fn seed_preserves_a_live_roster_and_replaces_a_corrupt_one() {
        // The transient-drop regression: a shellbridge restart must NOT wipe a
        // populated sessions.json. `seed_if_absent` preserves a valid registry,
        // replaces a corrupt/half-written one with the empty v0 shape, and seeds
        // an absent one.
        let dir = std::env::temp_dir().join(format!("aoide-seed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("sessions.json");

        let empty_body = r#"{"schemaVersion":"0","sessions":[]}"#;

        // A populated, valid registry survives untouched (the fix).
        let populated = r#"{"schemaVersion":"0","sessions":[{"sessionId":"live"}]}"#;
        std::fs::write(&path, populated).unwrap();
        assert_eq!(
            seed_if_absent(&path, empty_body, "sessions"),
            None,
            "a live roster is preserved across a restart seed"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), populated);

        // A VALID-but-EMPTY roster is a real registry too — preserved, not re-seeded.
        std::fs::write(&path, empty_body).unwrap();
        assert_eq!(
            seed_if_absent(&path, empty_body, "sessions"),
            None,
            "a valid empty roster is preserved (classified present, not corrupt)"
        );

        // JSON-valid but WRONG SHAPE (`sessions` is an object, not an array) is
        // classified corrupt and replaced with the empty v0 shape.
        std::fs::write(&path, r#"{"schemaVersion":"0","sessions":{}}"#).unwrap();
        assert!(seed_if_absent(&path, empty_body, "sessions").is_some());
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(v["sessions"].is_array() && v["sessions"].as_array().unwrap().is_empty());

        // Non-JSON / half-written garbage is likewise replaced.
        std::fs::write(&path, "{ not json at all").unwrap();
        assert!(seed_if_absent(&path, empty_body, "sessions").is_some());
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(v["sessions"].is_array() && v["sessions"].as_array().unwrap().is_empty());

        // An absent file is seeded.
        let fresh = dir.join("hooks.json");
        assert!(seed_if_absent(&fresh, r#"{"schemaVersion":"0","hooks":[]}"#, "hooks").is_some());
        assert!(fresh.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_sweeps_a_leaked_temp_from_a_dead_pid() {
        // A power-cut / SIGKILL between create and rename leaves a stranded
        // `<stem>.tmp.<pid>` (a real `graph.tmp.464255` was found on disk). The
        // next successful write of the same file sweeps a dead pid's temp but
        // spares a live node's in-flight one.
        let dir = std::env::temp_dir().join(format!("aoide-tmpsweep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let target = dir.join("graph.json");

        let leaked = dir.join(format!("graph.tmp.{}", u32::MAX)); // pid above pid_max — never alive
        std::fs::write(&leaked, "half-written").unwrap();
        let live_node = dir.join(format!("graph.tmp.{ALWAYS_LIVE_PID}"));
        std::fs::write(&live_node, "in-flight").unwrap();

        atomic_write(&target, "{}").unwrap();

        assert!(!leaked.exists(), "a dead pid's leaked temp is swept on the next write");
        assert!(live_node.exists(), "a live pid's in-flight temp is left untouched");
        assert!(target.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_sweep_reclaims_any_dead_pids_temp_whatever_its_stem_and_a_failed_write_leaves_none() {
        // The outbox litter this widening exists for: 1519 zero-byte
        // `<msgid>.tmp.<pid>` orphans, each keyed by a msgid minted once and
        // never rewritten, so a stem-scoped sweep could never reach any of
        // them. Any write into the directory has to reclaim them.
        let dir = std::env::temp_dir().join(format!("aoide-tmpsweep-foreign-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let dead = u32::MAX; // above pid_max — never alive
        let orphan = dir.join(format!("aaaa1111.tmp.{dead}"));
        let in_flight = dir.join(format!("bbbb2222.tmp.{ALWAYS_LIVE_PID}"));
        // Deliberately distinct temp shapes owned by other writers
        // (`migrate_state_tree`, `seed_if_absent`) — `.tmp.` is a whole
        // separator, so neither is ours to remove.
        let migrate = dir.join(format!("cccc3333.migrate-tmp.{dead}"));
        let seed = dir.join(format!("dddd4444.seed.{dead}"));
        for p in [&orphan, &in_flight, &migrate, &seed] {
            std::fs::write(p, "stranded").unwrap();
        }

        atomic_write(&dir.join("unrelated.json"), "{}").unwrap();

        assert!(!orphan.exists(), "a dead pid's temp is swept whatever its stem");
        assert!(in_flight.exists(), "a live pid's in-flight temp is left untouched");
        assert!(migrate.exists(), "`migrate-tmp.<pid>` is another writer's shape");
        assert!(seed.exists(), "`seed.<pid>` is another writer's shape");

        // A write that cannot land leaves no temp of its own: renaming onto a
        // non-empty directory fails, and the same unlink covers it.
        let blocked = dir.join("blocked.json");
        std::fs::create_dir_all(&blocked).unwrap();
        std::fs::write(blocked.join("occupant"), "x").unwrap();
        assert!(atomic_write(&blocked, "{}").is_err());
        assert!(
            !dir.join(format!("blocked.tmp.{}", std::process::id())).exists(),
            "a failed write never leaves its own temp behind"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── the process-liveness probe (POSIX `kill(pid, 0)`) ──

    /// A pid that is always live on THIS host, for the sweep tests below:
    /// `init` on Unix, and on Windows the System process, which is always
    /// pid 4. It must be a live pid that is NOT ours — the sweep spares our
    /// own pid by an explicit check, so using it would prove nothing.
    #[cfg(unix)]
    const ALWAYS_LIVE_PID: u32 = 1;
    #[cfg(windows)]
    const ALWAYS_LIVE_PID: u32 = 4;

    #[test]
    fn pid_is_alive_reads_this_process_and_a_waited_child() {
        assert!(pid_is_alive(std::process::id()));

        // Unix: a forked copy, waited for — the shape the probe's `ESRCH`
        // verdict is written around.
        #[cfg(unix)]
        {
            // SAFETY: the child branch calls only `_exit`, so the forked copy never
            // reaches a panic, the harness, or an atexit handler.
            let child = unsafe { libc::fork() };
            if child == 0 {
                unsafe { libc::_exit(0) };
            }
            assert!(child > 0, "fork failed: {}", std::io::Error::last_os_error());
            assert!(pid_is_alive(child as u32), "a forked child is a live pid");

            let mut status: libc::c_int = 0;
            let waited = loop {
                // SAFETY: `child` is this process's own child and is waited exactly
                // once; `&mut status` is a valid local.
                let r = unsafe { libc::waitpid(child, &mut status, 0) };
                if r != -1 || std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
                    break r;
                }
            };
            assert_eq!(waited, child, "the child is reaped, not merely polled");
            assert!(
                libc::WIFEXITED(status) && libc::WEXITSTATUS(status) == 0,
                "the forked child must exit cleanly, never via a panic: status {status}"
            );
            assert!(!pid_is_alive(child as u32), "a reaped pid names no process");
        }

        // Windows: the same two facts through a child process this one can
        // wait on. The pid is asked about AFTER the wait, while the `Child`
        // (and so the process object) is still open — a pid whose exit code
        // is already known reads absent even as a held handle, which is
        // exactly the "reaped, not merely polled" reading.
        #[cfg(windows)]
        {
            let mut child = std::process::Command::new("cmd.exe")
                .args(["/C", "exit", "0"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .expect("spawning a child process");
            let pid = child.id();
            assert!(pid_is_alive(pid), "a spawned child is a live pid");
            let status = child.wait().expect("waiting for the child");
            assert!(status.success(), "the child must exit cleanly, got {status}");
            assert!(!pid_is_alive(pid), "a waited-for pid names no process");
        }
    }

    #[test]
    fn pid_is_alive_refuses_a_pid_that_cannot_name_one_process() {
        // `kill(0, …)` addresses the caller's own process GROUP, and a value
        // above `pid_t::MAX` wraps to a negative group/broadcast address —
        // neither names ONE process, so both are refused before the syscall
        // rather than answered by a kernel that would call both live.
        // Windows has no process groups, but `0` is its Idle pseudo-process
        // and a pid past the real range is refused by the API: the same two
        // answers, reached the same way (before the probe's own verdict).
        assert!(!pid_is_alive(0));
        assert!(!pid_is_alive(u32::MAX));
        #[cfg(unix)]
        assert!(!pid_is_alive(libc::pid_t::MAX as u32 + 1));
    }

    /// `ESRCH`/`EPERM` are errno names: this test asks the Unix arm's own
    /// verdict table, which has no Windows counterpart (there the verdicts
    /// come from Win32 error codes inside `win_proc`, covered by its own
    /// tests on that host).
    #[cfg(unix)]
    #[test]
    fn probe_verdict_reads_only_esrch_as_absent() {
        assert!(!probe_verdict(Some(libc::ESRCH)), "ESRCH is the one absent verdict");
        assert!(probe_verdict(Some(libc::EPERM)), "another user's process exists — live");
        assert!(
            probe_verdict(Some(libc::EINVAL)),
            "an unexpected errno is unknown, never evidence of death"
        );
        assert!(probe_verdict(None), "an unanswerable probe is never evidence of death");
    }

    // ── atomic_write is symlink-transparent (rice draft mode's routing) ──

    /// Unix-only: creating a symbolic link on native Windows needs
    /// SeCreateSymbolicLinkPrivilege or Developer Mode, which no runner
    /// guarantees, and the thing under test IS the link (`atomic_write`'s
    /// own symlink transparency is `symlink_metadata`/`read_link`, which are
    /// portable — only the fixture cannot be built there). `copy_dir_recursive`'s
    /// Windows arm has its own test in `fs_windows`.
    #[cfg(unix)]
    #[test]
    fn atomic_write_writes_through_a_symlink_leaving_the_link_itself_intact() {
        let dir = std::env::temp_dir().join(format!("aoide-atomic-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let real = dir.join("real.json");
        let link = dir.join("link.json");
        std::fs::write(&real, "seed").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        atomic_write(&link, "first").unwrap();
        assert!(
            std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink(),
            "the symlink itself must survive the write, not get replaced by rename()"
        );
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "first", "the LINK TARGET carries the content");
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "first", "reading through the link agrees");

        // A second write must keep working the same way — the fix isn't a
        // one-shot "first write creates a real file" side effect.
        atomic_write(&link, "second").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "second");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Unix-only for the same reason as the test above: the fixture is a link.
    #[cfg(unix)]
    #[test]
    fn atomic_write_resolves_a_relative_symlink_target() {
        // The exact shape `rice mode draft` creates: stage/theme.json (a
        // relative symlink) → ../../rices/<rice>/drafts/<name>/theme.json.
        let dir = std::env::temp_dir().join(format!("aoide-atomic-symlink-rel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sub = dir.join("drafts").join("neon-night");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(dir.join("stage")).unwrap();
        let real = sub.join("theme.json");
        std::fs::write(&real, "seed").unwrap();
        let link = dir.join("stage").join("theme.json");
        std::os::unix::fs::symlink("../drafts/neon-night/theme.json", &link).unwrap();

        atomic_write(&link, "routed").unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "routed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_bytes_round_trips_binary_content() {
        // The bytes-oriented primitive `atomic_write` now delegates to — a
        // widget QML file (or any non-UTF-8 payload) must round-trip exactly,
        // not just the `&str` callers `atomic_write` itself covers.
        let dir = std::env::temp_dir().join(format!("aoide-atomic-bytes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("binary.dat");

        let bytes: &[u8] = &[0u8, 159, 146, 150];
        atomic_write_bytes(&path, bytes).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_creates_a_plain_file_when_path_is_not_a_symlink() {
        // Regression coverage: the overwhelming common case (no symlink at
        // all) must behave exactly as before this fix.
        let dir = std::env::temp_dir().join(format!("aoide-atomic-plain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plain.json");

        atomic_write(&path, "content").unwrap();
        assert!(!std::fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "content");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The private policy, asked of the host that holds it: this asserts the
    /// SAME promise with each host's own reader ([`assert_private_file`]),
    /// which is why the test name no longer says `0600` — a mode is how Unix
    /// spells owner-only, not what the promise is.
    #[test]
    fn atomic_write_private_locks_the_file_to_the_owner_only_policy() {
        let dir = std::env::temp_dir().join(format!("aoide-atomic-private-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.key");

        atomic_write_private(&path, b"private bytes").unwrap();
        assert_private_file(&path);
        assert_eq!(std::fs::read(&path).unwrap(), b"private bytes");

        // A second write to the SAME path (the re-mint-never-happens case,
        // but the primitive itself must stay correct either way) is still
        // locked to the same policy afterward.
        atomic_write_private(&path, b"replaced").unwrap();
        assert_private_file(&path);
        assert_eq!(std::fs::read(&path).unwrap(), b"replaced");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_private_temp_is_created_already_0600_before_any_rename() {
        // `write_temp_file` reads the state dir (`ensure_write_dir` ->
        // `state_dir()`), which the env-lock holders in this module repoint.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        // The review-bounced defect: an earlier revision of
        // `atomic_write_private` created its temp at `File::create`'s
        // umask-derived default mode and `chmod`ed the FINAL path only
        // AFTER the rename, leaving the live private-key path briefly
        // world/group-readable under this box's 022 umask between the
        // rename landing and the chmod call. Calling `write_temp_file`
        // directly — the one place `atomic_write_bytes_impl` ever creates a
        // temp — proves the mode is right at the moment of CREATION, before
        // any rename has happened at all, rather than only checking the
        // final path after the whole write-then-rename round trip returns
        // (which the OLD, buggy code would also have passed, since its
        // chmod ran before returning — the defect was a window DURING the
        // call, not a wrong end state). Windows' arm of the same window is
        // the DACL attached to `CreateFileW` itself, read back here by
        // `assert_private_file` through the object's own handle.
        let dir = std::env::temp_dir().join(format!("aoide-write-temp-file-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let tmp = dir.join("secret.key.tmp");

        write_temp_file(&tmp, b"private seed bytes", Some(0o600)).unwrap();
        assert_private_file(&tmp);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 879f7b78: a temp left at the private name by a write killed between
    /// create and rename keeps its wider mode through `O_TRUNC`, and a rename
    /// would publish it. The leftover is removed and the temp created fresh at
    /// `0600`, so the file over the private name is never `0644`.
    #[cfg(unix)]
    #[test]
    fn a_private_write_never_publishes_a_leftover_temps_wider_mode() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("aoide-leftover-temp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("ed25519.key");
        let leftover = target.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&leftover, b"stale bytes").unwrap();
        std::fs::set_permissions(&leftover, std::fs::Permissions::from_mode(0o644)).unwrap();

        atomic_write_private(&target, b"private seed bytes").unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"private seed bytes");
        assert_eq!(unix_mode(&target), 0o600, "the published file took the leftover's wider mode");
        assert!(!leftover.exists(), "the leftover temp must be gone after the write");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `write_temp_file` locks a wide `state/` its temp lands in, as every
    /// storage write does, so no temp is written around the gate.
    #[cfg(unix)]
    #[test]
    fn write_temp_file_locks_a_wide_state_it_writes_into() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-temp-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let state = base.join("state");
        std::fs::create_dir_all(state.join("identity")).unwrap();
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("AOIDE_STATE_DIR", &state);

        write_temp_file(&state.join("identity").join("k.tmp"), b"x", Some(0o600)).unwrap();
        assert_eq!(unix_mode(&state), 0o700, "write_temp_file left state/ wide");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The shallow-state predicate, on paths alone: nothing real is read or
    /// changed. Components, not characters, decide a parent, so a sibling that
    /// shares a name prefix is not one.
    #[cfg(unix)]
    #[test]
    fn a_filesystem_root_or_home_or_its_parent_is_never_a_state_dir() {
        let home = &[std::path::PathBuf::from("/home/someone")][..];
        let why_home = Some("it is the home directory or one of its parents");
        assert_eq!(shallow_state_reason(std::path::Path::new("/"), home), Some("it is a filesystem root"));
        assert_eq!(shallow_state_reason(std::path::Path::new("/home/someone"), home), why_home);
        assert_eq!(shallow_state_reason(std::path::Path::new("/home"), home), why_home);
        assert_eq!(shallow_state_reason(std::path::Path::new("/home/someone/.aoide/state"), home), None);
        assert_eq!(shallow_state_reason(std::path::Path::new("/home/someonelse"), home), None);
        assert_eq!(shallow_state_reason(std::path::Path::new("/tmp/x/state"), home), None);
        // Every home given is guarded, not only the first.
        let two = &[std::path::PathBuf::from("/home/someone"), std::path::PathBuf::from("/srv/other")][..];
        assert_eq!(shallow_state_reason(std::path::Path::new("/srv"), two), why_home);
        // With no home to name, the root is still refused and nothing else is.
        assert_eq!(shallow_state_reason(std::path::Path::new("/"), &[]), Some("it is a filesystem root"));
        assert_eq!(shallow_state_reason(std::path::Path::new("/home/someone"), &[]), None);
    }

    /// `lock_state_dir` on a temp `HOME`, never the real one: a state dir that
    /// is the home directory, or its parent, is refused with its mode left
    /// alone; a real state dir under home is still locked `0700`.
    #[cfg(unix)]
    #[test]
    fn a_write_refuses_to_chmod_home_or_its_parent_and_still_locks_a_real_state() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-shallow-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let parent = base.join("parent");
        let home = parent.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("HOME", &home);

        std::env::set_var("AOIDE_STATE_DIR", &home);
        let err = ensure_write_dir(&home.join("x")).expect_err("the home directory must be refused");
        assert!(err.to_string().contains("home directory"), "the refusal must say why: {err}");
        assert_eq!(unix_mode(&home), 0o755, "the home directory was chmodded");

        std::env::set_var("AOIDE_STATE_DIR", &parent);
        let err = ensure_write_dir(&parent.join("x")).expect_err("a parent of home must be refused");
        assert!(err.to_string().contains("parents"), "the refusal must say why: {err}");
        assert_eq!(unix_mode(&parent), 0o755, "a parent of home was chmodded");

        let state = home.join(".aoide").join("state");
        std::env::set_var("AOIDE_STATE_DIR", &state);
        ensure_write_dir(&state.join("stage")).unwrap();
        assert_eq!(unix_mode(&state), 0o700, "a real state dir under home is not locked");
        assert_eq!(unix_mode(&home), 0o755, "locking state/ must not touch home");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A path this test asserts is absent before it runs, and removes after, so
    /// a refusal that failed to hold cannot leave a directory in the working
    /// directory behind.
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn absent(path: &str) -> Self {
            assert!(!std::path::Path::new(path).exists(), "{path} exists in the working directory before the test");
            Scratch(path.into())
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A `..` in the state path is refused by name, before anything resolves.
    /// The directory it climbs to is the parent of `HOME`, which keeps its mode.
    #[cfg(unix)]
    #[test]
    fn a_dot_dot_state_path_is_refused_and_never_chmods_what_it_names() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-dotdot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let parent = base.join("parent");
        let home = parent.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_STATE_DIR", home.join(".."));

        let err = ensure_write_dir(&home.join("..").join("x")).expect_err("a .. state path must be refused");
        assert!(err.to_string().contains("`..`"), "the refusal must name the .. : {err}");
        assert_eq!(unix_mode(&parent), 0o755, "the directory a .. state path names was chmodded");
        assert_eq!(unix_mode(&home), 0o755, "home was chmodded through a .. state path");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A `HOME` reached through a symlink is protected by its real path: a state
    /// dir spelled as the real directory is refused as the home directory.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_home_is_refused_by_its_real_path() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-home-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let real = base.join("real-home");
        std::fs::create_dir_all(&real).unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).unwrap();
        let link = base.join("link-home");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        std::env::set_var("HOME", &link);
        std::env::set_var("AOIDE_STATE_DIR", &real);

        let err = ensure_write_dir(&real.join("x")).expect_err("the real path of a symlinked HOME must be refused");
        assert!(err.to_string().contains("home directory"), "the refusal must say why: {err}");
        assert_eq!(unix_mode(&real), 0o755, "the symlinked home was chmodded through its real path");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The guard covers every home a run could mean, together: a non-empty
    /// `HOME`, the effective uid's passwd home, and `/home/$AOIDE_USER`. An
    /// empty `HOME` counts as unset and leaves the passwd home guarded, never a
    /// guessed one. With `HOME` empty, a relative default root is refused before
    /// anything is created in the working directory, and `/` still is.
    #[cfg(unix)]
    #[test]
    fn every_home_is_guarded_and_a_relative_default_root_writes_nowhere() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        // A sandbox may run as a uid with no passwd entry; the rest still holds.
        let passwd: Vec<_> = aoide_protocol::audit::passwd_home().into_iter().collect();
        assert!(passwd.iter().all(|home| home.is_absolute()), "a passwd home is an absolute path: {passwd:?}");
        std::env::set_var("HOME", "");
        assert_eq!(guard_homes(), passwd, "an empty HOME counts as unset; the passwd home stays");
        std::env::remove_var("HOME");
        assert_eq!(guard_homes(), passwd, "an unset HOME leaves the passwd home, never a guess");
        std::env::set_var("HOME", "/home/someone");
        std::env::set_var("AOIDE_USER", "other");
        let mut all = vec![std::path::PathBuf::from("/home/someone")];
        all.extend(passwd.iter().cloned());
        all.push(std::path::PathBuf::from("/home/other"));
        assert_eq!(guard_homes(), all, "HOME, the passwd home and the AOIDE_USER home are guarded together");
        std::env::remove_var("AOIDE_USER");

        std::env::set_var("HOME", "");
        let _scratch = Scratch::absent(".aoide");
        let err = ensure_write_dir(&state_dir().join("stage")).expect_err("a relative default state must be refused");
        assert!(err.to_string().contains("relative"), "the refusal must say why: {err}");
        assert!(!std::path::Path::new(".aoide").exists(), "a relative state/ was created in the working directory");

        std::env::remove_var("HOME");
        std::env::set_var("AOIDE_STATE_DIR", "/");
        let err = ensure_write_dir(std::path::Path::new("/x")).expect_err("/ must be refused with HOME unset");
        assert!(err.to_string().contains("filesystem root"), "the refusal must say why: {err}");
    }

    /// The lock refuses a relative state path itself. A relative
    /// `AOIDE_STATE_DIR` is ignored, so the state lands under `AOIDE_ROOT`.
    #[cfg(unix)]
    #[test]
    fn a_relative_state_path_is_refused_and_a_relative_override_is_ignored() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let _scratch = Scratch::absent("rel");
        let err = lock_state_dir(std::path::Path::new("rel")).expect_err("a relative state/ must be refused");
        assert!(err.to_string().contains("relative"), "the refusal must say why: {err}");
        assert!(!std::path::Path::new("rel").exists(), "a relative state/ was created in the working directory");

        let base = std::env::temp_dir().join(format!("aoide-rel-override-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("root");
        std::env::set_var("AOIDE_STATE_DIR", "rel");
        std::env::set_var("AOIDE_ROOT", &root);
        std::env::set_var("STRILLIX_ROOT", &root);
        assert_eq!(state_dir(), root.join("state"), "a relative AOIDE_STATE_DIR must be ignored");
        ensure_write_dir(&state_dir().join("stage")).unwrap();
        assert_eq!(unix_mode(&root.join("state")), 0o700);
        assert!(!std::path::Path::new("rel").exists(), "the ignored override was created in the working directory");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A `state/` that is a symlink is refused at the open, which never follows
    /// the last component. The path checks read through the link to its target,
    /// and the target keeps its mode.
    #[cfg(unix)]
    #[test]
    fn a_symlinked_state_is_refused_at_the_open_and_its_target_is_never_chmodded() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-state-open-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let target = base.join("elsewhere");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        let link = base.join("state");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        std::env::set_var("HOME", base.join("home"));

        let err = lock_state_dir(&link).expect_err("a symlinked state/ must be refused at the open");
        assert!(err.to_string().contains("state/ is a symlink"), "the refusal must name the link: {err}");
        assert_eq!(unix_mode(&target), 0o755, "the link's target was chmodded");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The object [`owner_only_state`] opens is checked by identity as well as by
    /// name. Called directly, past the path checks, on the home directory, it is
    /// refused and keeps its mode.
    #[cfg(unix)]
    #[test]
    fn owner_only_state_refuses_a_shallow_object_by_its_identity() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let base = std::env::temp_dir().join(format!("aoide-identity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let home = base.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o755)).unwrap();

        let err =
            owner_only_state(&home, &[home.clone()]).expect_err("the home directory must be refused by identity");
        assert!(err.to_string().contains("home directory"), "the refusal must say why: {err}");
        assert_eq!(unix_mode(&home), 0o755, "the home directory was chmodded past the path checks");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// `HOME` is guarded while `AOIDE_USER` names another home: every home a
    /// run could mean is guarded together, so the one [`aoide_home`] does not
    /// answer keeps its mode.
    #[cfg(unix)]
    #[test]
    fn a_home_aoide_user_does_not_name_is_still_refused() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-two-homes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let home = base.join("home");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_USER", format!("aoide-no-such-user-{}", std::process::id()));
        std::env::set_var("AOIDE_STATE_DIR", &home);

        let err = ensure_write_dir(&home.join("x")).expect_err("HOME must be refused when AOIDE_USER names another");
        assert!(err.to_string().contains("home directory"), "the refusal must say why: {err}");
        assert_eq!(unix_mode(&home), 0o755, "HOME was chmodded because AOIDE_USER named another home");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// `AOIDE_USER` guards the passwd home of the name it gives, not a
    /// `/home/<name>` guess: `AOIDE_USER=root` guards root's passwd home and
    /// not `/home/root`. The old `Path::new("/home").join(user)` put
    /// `/home/root` here, which is the path this must not guard.
    #[cfg(unix)]
    #[test]
    fn aoide_user_guards_its_passwd_home() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        std::env::set_var("AOIDE_USER", "root");
        let homes = guard_homes();
        let root = aoide_protocol::audit::user_home("root");
        assert!(homes.contains(&root), "root's passwd home is guarded: {homes:?}");
        assert!(!homes.contains(&std::path::PathBuf::from("/home/root")), "no /home/root guess: {homes:?}");
    }

    /// The canonical check's own job: refusing a shallow `state/` before
    /// anything is created. A home that does not exist yet, spelled through a
    /// symlinked parent on either side, is refused by its real path and nothing
    /// is made. The identity check after the open would refuse it too, but only
    /// after the open had created it.
    #[cfg(unix)]
    #[test]
    fn a_state_through_a_symlinked_home_not_yet_made_is_refused_and_nothing_is_created() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-unmade-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let real = base.join("real");
        std::fs::create_dir_all(&real).unwrap();
        let link = base.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let unmade = real.join("home");

        std::env::set_var("HOME", link.join("home"));
        std::env::set_var("AOIDE_STATE_DIR", &unmade);
        let err = ensure_write_dir(&unmade.join("x")).expect_err("HOME through a link must be refused by its real path");
        assert!(err.to_string().contains("home directory"), "the refusal must say why: {err}");
        assert!(!unmade.exists(), "the refused state/ was created (HOME spelled through the link)");

        std::env::set_var("HOME", &unmade);
        std::env::set_var("AOIDE_STATE_DIR", link.join("home"));
        let err = ensure_write_dir(&link.join("home").join("x")).expect_err("a state/ through a link must be refused");
        assert!(err.to_string().contains("home directory"), "the refusal must say why: {err}");
        assert!(!unmade.exists(), "the refused state/ was created (state/ spelled through the link)");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// `secure_private_dir` chmods through a descriptor opened without following
    /// a link at `dir`. A `dir` that is a link to a directory outside `state/` is
    /// refused, and that directory keeps its mode.
    #[cfg(unix)]
    #[test]
    fn secure_private_dir_refuses_a_linked_leaf_and_never_chmods_its_target() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-private-leaf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let state = base.join("state");
        std::fs::create_dir_all(&state).unwrap();
        let outside = base.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::os::unix::fs::symlink(&outside, state.join("identity")).unwrap();
        std::env::set_var("HOME", base.join("home"));
        std::env::set_var("AOIDE_STATE_DIR", &state);

        let err = secure_private_dir(&state.join("identity")).expect_err("a linked identity/ must be refused");
        assert!(err.to_string().contains("symlink"), "the refusal must name the link: {err}");
        assert_eq!(unix_mode(&outside), 0o755, "the link's target outside state/ was chmodded");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A `..` in the directory is refused before anything is created or
    /// changed, even when it climbs out of `state/` onto a directory that
    /// exists: that directory keeps its mode.
    #[cfg(unix)]
    #[test]
    fn secure_private_dir_refuses_a_dot_dot_in_its_path_and_never_chmods_what_it_climbs_to() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-private-dotdot-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let state = base.join("state");
        std::fs::create_dir_all(state.join("operator")).unwrap();
        let outside = base.join("home");
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::set_permissions(&outside, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("HOME", base.join("elsewhere"));
        std::env::set_var("AOIDE_STATE_DIR", &state);

        let climbing = state.join("operator").join("..").join("..").join("home");
        let err = secure_private_dir(&climbing).expect_err("a path with a .. in it must be refused");
        assert!(err.to_string().contains("`..`"), "the refusal must say why: {err}");
        assert_eq!(unix_mode(&outside), 0o755, "the directory the path climbs to was chmodded");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A write under `state/` spelled through an alias of one of its parents
    /// locks `state/`: under is judged by where the directory lands, not by how
    /// its path is spelled.
    #[cfg(unix)]
    #[test]
    fn a_write_through_an_alias_locks_the_state_it_lands_in() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-state-alias-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let real = base.join("real");
        let state = real.join("state");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
        let alias = base.join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        std::env::set_var("HOME", base.join("home"));
        std::env::set_var("AOIDE_STATE_DIR", &state);

        ensure_write_dir(&alias.join("state").join("sub")).unwrap();
        assert!(state.join("sub").is_dir(), "the write landed under the real state/");
        assert_eq!(unix_mode(&state), 0o700, "a write through an alias left state/ wide");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A `..` write is judged where the kernel puts it: `a/missing/../state/x`
    /// is created under `a/state` (`create_dir_all` makes `a/missing` on the
    /// way), so it locks `state/` first.
    #[cfg(unix)]
    #[test]
    fn a_dot_dot_write_that_lands_under_state_locks_it() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-state-dotdot-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let a = base.join("a");
        let state = a.join("state");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("HOME", base.join("home"));
        std::env::set_var("AOIDE_STATE_DIR", &state);

        ensure_write_dir(&a.join("missing").join("..").join("state").join("x")).unwrap();
        assert!(state.join("x").is_dir(), "the kernel put the write under state/");
        assert_eq!(unix_mode(&state), 0o700, "a .. write that lands under state/ left it wide");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A `state/` this user owns but cannot open (`0300`, no read bit) is
    /// refused with the fix its owner can make, a chmod, never a chown.
    #[cfg(unix)]
    #[test]
    fn an_owned_state_it_cannot_open_names_chmod_not_chown() {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: `geteuid` takes nothing and cannot fail.
        if unsafe { libc::geteuid() } == 0 {
            eprintln!("skipped an_owned_state_it_cannot_open_names_chmod_not_chown: root opens a 0300 directory");
            return;
        }
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-state-0300-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let state = base.join("state");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o300)).unwrap();
        std::env::set_var("HOME", base.join("home"));
        std::env::set_var("AOIDE_STATE_DIR", &state);

        let result = ensure_write_dir(&state.join("x"));
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o700)).unwrap();
        let msg = result.expect_err("a state/ it cannot open is refused").to_string();
        assert!(msg.contains(&format!("chmod u+rwx {}", state.display())), "the fix must be the owner's chmod: {msg}");
        assert!(!msg.contains("chown"), "a state/ this user owns needs no chown: {msg}");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Unix-only: a umask is a POSIX process-wide mode mask. Its Windows
    /// counterpart is the DACL a parent directory grants, which the test
    /// above already covers through the same readback — and there is no
    /// process-wide knob there to widen it in the first place.
    #[cfg(unix)]
    #[test]
    fn atomic_write_private_locks_the_final_path_to_0600_under_a_permissive_umask() {
        // Complements the test above: end-to-end through the public
        // `atomic_write_private` entry point, forcing a wide-open umask
        // (000 — even more permissive than this box's normal 022, so a
        // regression to "create at the umask default, chmod after" would
        // show up as a would-be-0666 window rather than a merely-0644 one)
        // to prove the FINAL path is 0600 regardless of what the process
        // umask would otherwise have widened a plain `File::create` to.
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("aoide-atomic-private-umask-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.key");

        // SAFETY: umask(2) takes a plain mode_t and cannot fail; restored
        // unconditionally immediately after, under the same env_lock every
        // other process-global-state test in this crate already serializes
        // behind (umask is process-wide, wider even than an env var).
        let old_umask = unsafe { libc::umask(0o000) };
        let result = atomic_write_private(&path, b"private seed bytes");
        unsafe { libc::umask(old_umask) };
        result.unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "the file at its FINAL path must be 0600 immediately after mint even under an 000 umask, got {mode:o}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Unix-only: a private write killed between its create and its rename
    /// leaves `<target>.tmp.<pid>` behind at whatever mode it was made with.
    /// A later process that reuses that pid writes to the same name, and
    /// `O_TRUNC` keeps the leftover's mode rather than applying `0600` again,
    /// so the rename would publish a wider file as the private one. This test
    /// plants a `0644` leftover under our own pid (the reuse case, without a
    /// dead process to stage it) and requires the private file to come out
    /// `0600` with the new bytes.
    #[cfg(unix)]
    #[test]
    fn a_wider_leftover_temp_under_the_private_name_is_never_published() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("aoide-stale-private-temp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sessions.json");
        let leftover = path.with_extension(format!("tmp.{}", std::process::id()));
        std::fs::write(&leftover, b"leftover").unwrap();
        std::fs::set_permissions(&leftover, std::fs::Permissions::from_mode(0o644)).unwrap();

        atomic_write_private(&path, b"fresh").unwrap();
        assert_private_file(&path);
        assert_eq!(std::fs::read(&path).unwrap(), b"fresh");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Windows: `secure_private_dir` on a directory that already exists and is
    /// NOT private must TIGHTEN it, not refuse it and not leave it as it is.
    /// That is the second of the three states `ensure_private_dir` has to
    /// tell apart (absent, present-and-private, present-and-open), and it is
    /// the one a host inherits from an earlier install — the Unix arm's
    /// `chmod 0700` over an existing directory, which is equally idempotent.
    /// The fixture starts wide on purpose: `create_dir_all` leaves the
    /// inherited default DACL, which is exactly the shape being repaired.
    #[cfg(windows)]
    #[test]
    fn secure_private_dir_tightens_a_directory_another_run_left_open() {
        let dir = std::env::temp_dir().join(format!("aoide-secure-dir-tighten-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let before = aoide_protocol::owner_only::dir_privacy(&dir).unwrap();
        assert!(before.is_some(), "the fixture must start with a policy this crate would not accept: {before:?}");

        secure_private_dir(&dir).unwrap();
        assert_private_dir(&dir);

        // And it stays idempotent: a second call over an already-private
        // directory neither fails nor regresses it.
        secure_private_dir(&dir).unwrap();
        assert_private_dir(&dir);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Windows, and the rule this crate's private path is built around,
    /// asserted rather than assumed: **NT re-propagates a container's DACL
    /// change to children whose ACEs are INHERITED.** A child created the
    /// ordinary way has only inherited ACEs; the tighten writes a protected,
    /// `NO_INHERITANCE` policy, so the child's inherited ACEs are stripped
    /// and it ends up with none — unreadable, exactly as the first version of
    /// `config`'s unwritable-directory fixture was.
    ///
    /// That is why nothing in this crate relies on inheritance for private
    /// material: every private write attaches its OWN protected policy at
    /// creation (`create_truncating`), and the second half of this test shows
    /// such a child is untouched by a later tighten of its directory. The
    /// Unix arm has no analogue — `chmod 0700` changes no child's mode — so
    /// the asymmetry is the point of the assertion.
    #[cfg(windows)]
    #[test]
    fn tightening_a_directory_strips_a_child_that_only_inherited_its_access() {
        let dir = std::env::temp_dir().join(format!("aoide-secure-dir-repropagate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // The child the host wrote: inherited ACEs only.
        let inherited = dir.join("inherited.json");
        std::fs::write(&inherited, "payload").unwrap();
        assert!(std::fs::read(&inherited).is_ok(), "the fixture must start readable");

        secure_private_dir(&dir).unwrap();

        let after = std::fs::read(&inherited);
        assert!(
            after.is_err(),
            "NT should have stripped the child's inherited ACEs with the parent's policy: {after:?}"
        );

        // The child THIS crate wrote: its own protected policy, unaffected by
        // any later change to its directory's.
        let owned = dir.join("owned.json");
        {
            let mut f = aoide_protocol::owner_only::create_truncating(&owned).unwrap();
            f.write_all(b"payload").unwrap();
        }
        secure_private_dir(&dir).unwrap();
        assert_eq!(std::fs::read(&owned).unwrap(), b"payload", "a child with its own policy survives a tighten");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Windows: a directory JUNCTION is refused by name. A junction is the
    /// one link kind a non-elevated process can create, and it is exactly
    /// what a user does to redirect a state directory onto another volume —
    /// so it is the shape that would silently make this function report a
    /// repair of the link while the directory callers actually use kept the
    /// default it had.
    #[cfg(windows)]
    #[test]
    fn secure_private_dir_refuses_a_junction_by_name() {
        use std::process::Stdio;
        let base = std::env::temp_dir().join(format!("aoide-secure-dir-junction-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let target = base.join("target");
        std::fs::create_dir_all(&target).unwrap();
        let link = base.join("link");
        let made = std::process::Command::new("cmd.exe")
            .args([
                "/C",
                "mklink",
                "/J",
                link.to_str().unwrap(),
                target.to_str().unwrap(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        assert!(made, "the fixture needs a junction, and `mklink /J` needs no privilege");

        // The reader refuses it as itself, with a reason, and never reports
        // the LINK's policy as the directory's.
        let reason = aoide_protocol::owner_only::dir_privacy(&link).unwrap();
        assert!(reason.is_some(), "a junction is not the object the path resolves to: {reason:?}");

        let err = secure_private_dir(&link).expect_err("a junction must be refused, never repaired");
        let text = err.to_string();
        assert!(text.contains("junction") || text.contains("symbolic link"), "{text}");

        let _ = std::fs::remove_dir(&link);
        let _ = std::fs::remove_dir_all(&base);
    }


    // ── `conducting_stage_dir` / the command-defrag S1 migration ───────────

    const CORE_STAGE_FILE_NAMES: &[&str] =
        &["sessions.json", "hooks.json", "projects.json", "graph.json", "pending.json", "notifications.json"];


    #[test]
    fn migrate_conducting_stage_moves_every_core_file_from_old_to_new() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-basic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_ROOT", home.join(".aoide"));
        std::env::set_var("STRILLIX_ROOT", home.join(".aoide"));

        let old_dir = legacy_stage_dir(); // `<home>/.aoide/stage`, the legacy stage under the pinned `STRILLIX_ROOT`
        let new_dir = state_dir().join("stage");
        std::fs::create_dir_all(&old_dir).unwrap();
        for name in CORE_STAGE_FILE_NAMES {
            std::fs::write(old_dir.join(name), format!("{{\"marker\":\"{name}\"}}")).unwrap();
        }

        migrate_conducting_stage(&new_dir);

        for name in CORE_STAGE_FILE_NAMES {
            assert!(!old_dir.join(name).exists(), "{name} must have moved off the old path");
            let body = std::fs::read_to_string(new_dir.join(name)).unwrap();
            assert!(body.contains(name), "{name}'s content must have moved verbatim, got {body}");
        }

        let _ = std::fs::remove_dir_all(&home);
    }

    #[cfg(unix)]
    fn unix_mode(dir: &std::path::Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(dir).unwrap().permissions().mode() & 0o777
    }

    /// A storage write tightens a wide `state/`, and leaves `state/stage/` at
    /// whatever mode it already has.
    #[cfg(unix)]
    #[test]
    fn a_storage_write_tightens_a_wide_state_and_leaves_state_stage_alone() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-state-tighten-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("AOIDE_ROOT", home.join(".aoide"));
        std::env::set_var("STRILLIX_ROOT", home.join(".aoide"));

        let state = state_dir();
        let stage = state.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&stage, std::fs::Permissions::from_mode(0o755)).unwrap();

        crate::stage::write_stage(&stage.join("sessions.json"), &serde_json::json!({ "v": 0 })).unwrap();

        assert_eq!(unix_mode(&state), 0o700);
        assert_eq!(unix_mode(&stage), 0o755);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A fresh storage write creates `state/` owner-only.
    #[cfg(unix)]
    #[test]
    fn a_fresh_storage_write_creates_state_owner_only() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-state-fresh-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("AOIDE_ROOT", home.join(".aoide"));
        std::env::set_var("STRILLIX_ROOT", home.join(".aoide"));

        let state = state_dir();
        assert!(!state.exists());
        crate::stage::write_stage(&state.join("stage").join("sessions.json"), &serde_json::json!({ "v": 0 })).unwrap();

        assert_eq!(unix_mode(&state), 0o700);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// `/tmp` is root-owned and sticky: writable by this user, and a chmod by
    /// anyone else is refused. The probe is a chmod to the mode `/tmp` already
    /// has, so it changes nothing. `Err` names why the refusal cannot be
    /// provoked here: `/tmp` is absent, or this user can chmod it (root can).
    #[cfg(unix)]
    fn tmp_is_writable_but_not_chmodable() -> Result<std::path::PathBuf, &'static str> {
        let dir = std::path::PathBuf::from("/tmp");
        let meta = std::fs::metadata(&dir)
            .ok()
            .filter(std::fs::Metadata::is_dir)
            .ok_or("/tmp is absent or not a directory")?;
        match std::fs::set_permissions(&dir, meta.permissions()) {
            Err(_) => Ok(dir),
            Ok(()) => Err("this user can chmod /tmp (root can), so no unlockable state/ can be made here"),
        }
    }

    /// Every creator that can make `state/` on a fresh tree leaves it `0700`.
    /// One case per creator, each on its own fresh tree, so a creator that
    /// bypasses [`ensure_write_dir`] fails by name.
    #[cfg(unix)]
    #[test]
    fn every_creator_makes_state_owner_only_on_a_fresh_tree() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-state-creators-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);

        type Creator = fn(&std::path::Path) -> std::io::Result<()>;
        let cases: [(&str, Creator); 13] = [
            ("create_new_private", |s| create_new_private(&s.join("a.json")).map(drop)),
            ("edits::append", |_| {
                crate::edits::append(&crate::edits::EditLine::default())
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            }),
            ("outbox::try_take_poll_lock", |_| {
                crate::outbox::try_take_poll_lock("node-a")
                    .map(drop)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            }),
            ("migrate_dir", |s| {
                let old = s.parent().unwrap().join("legacy");
                std::fs::create_dir_all(old.join("sub"))?;
                migrate_dir(&old, s)
            }),
            ("migrate_file", |s| {
                let old = s.parent().unwrap().join("legacy.log");
                std::fs::create_dir_all(s.parent().unwrap())?;
                std::fs::write(&old, b"x")?;
                migrate_file(&old, &s.join("moved.log"));
                Ok(())
            }),
            ("open_private_append", |s| open_private_append(&s.join("b.log")).map(drop)),
            ("secure_private_dir", |s| secure_private_dir(&s.join("identity"))),
            ("atomic_write_private", |s| atomic_write_private(&s.join("identity").join("k"), b"x")),
            ("ensure_write_dir", |s| ensure_write_dir(&s.join("x").join("y"))),
            ("copy_dir_recursive", |s| {
                let src = s.parent().unwrap().join("copy-src");
                std::fs::create_dir_all(&src)?;
                copy_dir_recursive(&src, &s.join("copy"))
            }),
            ("lock_path", |s| {
                lock_path(s.join("stage").join(".stage.lock"), || ())
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
            }),
            ("seed_if_absent", |s| {
                match seed_if_absent(&s.join("sessions.json"), "{\"sessions\":[]}", "sessions") {
                    Some(_) => Ok(()),
                    None => Err(std::io::Error::new(std::io::ErrorKind::Other, "not seeded")),
                }
            }),
            ("with_stage_lock", |s| {
                std::env::set_var("AOIDE_STAGE_DIR", s.join("stage"));
                with_stage_lock(|| ());
                Ok(())
            }),
        ];
        // Every case runs, so one run names each creator that fails, not only the first.
        let mut failures = Vec::new();
        for (name, creator) in cases {
            let home = base.join(name);
            let _ = std::fs::remove_dir_all(&home);
            std::env::set_var("AOIDE_ROOT", home.join(".aoide"));
            std::env::set_var("STRILLIX_ROOT", home.join(".aoide"));
            let state = state_dir();
            assert!(!state.exists(), "{name}: the fixture must start with no state/");
            match creator(&state) {
                Err(e) => failures.push(format!("{name} failed on a fresh tree: {e}")),
                Ok(()) => {
                    let mode = unix_mode(&state);
                    if mode != 0o700 {
                        failures.push(format!("{name} left state/ at {mode:o}"));
                    }
                }
            }
        }
        let _ = std::fs::remove_dir_all(&base);
        assert!(failures.is_empty(), "creators that do not lock state/ first:\n{}", failures.join("\n"));
    }

    /// The private file creators write into a directory that may already exist
    /// under a wide `state/` (a raw creator made it first), and the write locks
    /// `state/` before it lands.
    #[cfg(unix)]
    #[test]
    fn private_creates_lock_a_wide_state_they_write_into() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-state-private-wide-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("AOIDE_ROOT", home.join(".aoide"));
        std::env::set_var("STRILLIX_ROOT", home.join(".aoide"));

        let state = state_dir();
        let sessions = state.join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
        create_new_private(&sessions.join("a.instructions.md")).unwrap();
        assert_eq!(unix_mode(&state), 0o700, "create_new_private left state/ wide");

        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
        open_private_append(&sessions.join("b.log")).unwrap();
        assert_eq!(unix_mode(&state), 0o700, "open_private_append left state/ wide");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A creator under a `state/` it cannot lock creates nothing and fails
    /// (`lock_path`, `seed_if_absent`), or runs unlocked without its lock file
    /// (`with_stage_lock`). `/tmp` is the fixture because it is writable: the
    /// lock file's directory exists and could take the write, so a discarded
    /// lock error would show as a created file. The `state/` is `/tmp`
    /// itself, and nothing outside this test's own subdirectory is touched.
    #[cfg(unix)]
    #[test]
    fn a_state_it_cannot_lock_takes_no_lock_file_and_no_seed() {
        let tmp = match tmp_is_writable_but_not_chmodable() {
            Ok(tmp) => tmp,
            Err(why) => {
                eprintln!("skipped a_state_it_cannot_lock_takes_no_lock_file_and_no_seed: {why}");
                return;
            }
        };
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = tmp.join(format!("aoide-state-refuse-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let stage = base.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STATE_DIR", &tmp);
        let lock = stage.join(".stage.lock");

        let result = lock_path(lock.clone(), || ());
        let err = result.expect_err("lock_path must refuse a state/ it cannot lock");
        assert!(err.contains("owner-only"), "the refusal must say why: {err}");
        assert!(!lock.exists(), "lock_path created its lock file under an unlocked state/");

        let path = stage.join("sessions.json");
        assert!(
            seed_if_absent(&path, "{\"sessions\":[]}", "sessions").is_none(),
            "seed_if_absent must not seed under a state/ it cannot lock"
        );
        assert!(!path.exists(), "seed_if_absent wrote its file under an unlocked state/");

        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        let ran = std::cell::Cell::new(false);
        with_stage_lock(|| ran.set(true));
        assert!(ran.get(), "with_stage_lock runs its closure unlocked, as its best-effort contract says");
        assert!(!lock.exists(), "with_stage_lock created its lock file under an unlocked state/");

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A `state/` that is a symlink is refused at the open (`O_NOFOLLOW` fails
    /// `ELOOP`), and the link's target is never chmodded: a chmod would follow
    /// the link to a directory the boundary does not cover.
    #[cfg(unix)]
    #[test]
    fn ensure_write_dir_refuses_a_symlinked_state_at_the_open() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let base = std::env::temp_dir().join(format!("aoide-state-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let target = base.join("elsewhere");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        let state = base.join("state");
        std::os::unix::fs::symlink(&target, &state).unwrap();
        std::env::set_var("AOIDE_STATE_DIR", &state);

        let err = ensure_write_dir(&state.join("stage")).expect_err("a symlinked state/ must be refused");
        assert!(err.to_string().contains("state/ is a symlink"), "the refusal must name the link: {err}");
        assert_eq!(unix_mode(&target), 0o755, "the link's target must not be chmodded");
        assert!(!target.join("stage").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn migrate_conducting_stage_second_run_is_a_no_op() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-idempotent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_ROOT", home.join(".aoide"));
        std::env::set_var("STRILLIX_ROOT", home.join(".aoide"));

        let old_dir = legacy_stage_dir();
        let new_dir = state_dir().join("stage");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::write(old_dir.join("sessions.json"), "first-boot").unwrap();

        migrate_conducting_stage(&new_dir);
        assert_eq!(std::fs::read_to_string(new_dir.join("sessions.json")).unwrap(), "first-boot");
        assert!(!old_dir.join("sessions.json").exists());

        // Second call: nothing left at the old path, and the new path is
        // already populated — must run cleanly and change nothing.
        migrate_conducting_stage(&new_dir);
        assert_eq!(
            std::fs::read_to_string(new_dir.join("sessions.json")).unwrap(),
            "first-boot",
            "a second migration pass must be a pure no-op"
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn migrate_conducting_stage_never_clobbers_a_newer_new_path_file() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-no-clobber-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_ROOT", home.join(".aoide"));
        std::env::set_var("STRILLIX_ROOT", home.join(".aoide"));

        let old_dir = legacy_stage_dir();
        let new_dir = state_dir().join("stage");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::create_dir_all(&new_dir).unwrap();
        // A STALE old-path file (predates the boot that already migrated)
        // alongside a NEWER new-path file (written since) — the newer file
        // must win, and the stale old file is left in place for inspection
        // rather than silently destroyed.
        std::fs::write(old_dir.join("sessions.json"), "stale").unwrap();
        std::fs::write(new_dir.join("sessions.json"), "fresh").unwrap();

        migrate_conducting_stage(&new_dir);

        assert_eq!(std::fs::read_to_string(new_dir.join("sessions.json")).unwrap(), "fresh");
        assert_eq!(std::fs::read_to_string(old_dir.join("sessions.json")).unwrap(), "stale");

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn migrate_conducting_stage_leaves_rice_files_where_they_are() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-rice-untouched-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_ROOT", home.join(".aoide"));
        std::env::set_var("STRILLIX_ROOT", home.join(".aoide"));

        let old_dir = legacy_stage_dir();
        let new_dir = state_dir().join("stage");
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::write(old_dir.join("sessions.json"), "core").unwrap();
        std::fs::write(old_dir.join("theme.json"), "rice-palette").unwrap();
        std::fs::write(old_dir.join("mode.json"), "rice-mode").unwrap();

        migrate_conducting_stage(&new_dir);

        assert!(!old_dir.join("sessions.json").exists(), "the core file must have moved");
        assert_eq!(
            std::fs::read_to_string(old_dir.join("theme.json")).unwrap(),
            "rice-palette",
            "rice files are never part of this migration"
        );
        assert_eq!(std::fs::read_to_string(old_dir.join("mode.json")).unwrap(), "rice-mode");
        assert!(!new_dir.join("theme.json").exists());
        assert!(!new_dir.join("mode.json").exists());

        let _ = std::fs::remove_dir_all(&home);
    }

    /// The one test in this binary that resolves [`conducting_stage_dir`]
    /// itself with no `$AOIDE_STAGE_DIR` override — proving the Once-guarded
    /// S1 wiring end to end, not just [`migrate_conducting_stage`] in
    /// isolation. [`MIGRATE_CONDUCTING_STAGE_ONCE`] fires at most once for
    /// the whole test binary, so this must be the ONLY call site in this
    /// module that reaches [`conducting_stage_dir`]'s fallback branch —
    /// every other migration test above calls [`migrate_conducting_stage`]
    /// directly to stay independent of that one-shot guard. `AOIDE_ROOT` is
    /// STILL pinned here (unlike the dedicated `root()` wiring test below):
    /// this test's job is S1's Once, not L-C2's — [`migrate_root_once`] gets
    /// its own isolated proof.
    #[test]
    fn conducting_stage_dir_resolves_under_state_and_migrates_on_first_call() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-wiring-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_ROOT", home.join(".aoide"));
        std::env::set_var("STRILLIX_ROOT", home.join(".aoide"));

        let old_dir = legacy_stage_dir();
        std::fs::create_dir_all(&old_dir).unwrap();
        std::fs::write(old_dir.join("graph.json"), "pre-existing-graph").unwrap();

        let dir = conducting_stage_dir();
        assert_eq!(dir, home.join(".aoide").join("state").join("stage"));
        assert_eq!(
            std::fs::read_to_string(dir.join("graph.json")).unwrap(),
            "pre-existing-graph",
            "conducting_stage_dir()'s own resolution must have driven the migration"
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    // ── `root()` / the L-C2 `~/Aoide` → `$AOIDE_ROOT` migration ────────────

    #[test]
    fn root_honors_absolute_env_override_and_falls_back_to_the_dotaoide_default() {
        // `root()` is pure (see its own doc) — safe to exercise every branch
        // directly, no migration side effect to worry about. `RootEnvGuard`
        // is skipped here on purpose: this test's whole point IS the
        // unset/empty/relative fallback shape.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved_root = std::env::var("AOIDE_ROOT").ok();
        let saved_home = std::env::var("HOME").ok();
        let saved_user = std::env::var("AOIDE_USER").ok();

        std::env::set_var("AOIDE_ROOT", scratch("aoide-root-test"));

        std::env::set_var("STRILLIX_ROOT", scratch("aoide-root-test"));
        assert_eq!(root(), std::path::PathBuf::from(scratch("aoide-root-test")));

        // Empty and relative values are ignored — falls back to the default,
        // same discipline every other override in this module holds.
        // `$HOME` pinned to a scratch dir so the assertion is deterministic
        // across machines, not because `root()` has anything to protect —
        // it never touches disk.
        let home = std::env::temp_dir().join(format!("aoide-root-default-{}", std::process::id()));
        std::env::remove_var("AOIDE_USER");
        std::env::set_var("HOME", &home);

        std::env::set_var("AOIDE_ROOT", "");

        std::env::set_var("STRILLIX_ROOT", "");
        assert_eq!(root(), home.join(".aoide"));
        std::env::set_var("AOIDE_ROOT", "relative/root");
        std::env::set_var("STRILLIX_ROOT", "relative/root");
        assert_eq!(root(), home.join(".aoide"));
        std::env::remove_var("AOIDE_ROOT");
        assert_eq!(root(), home.join(".aoide"));

        match saved_root {
            Some(v) => std::env::set_var("AOIDE_ROOT", v),
            None => std::env::remove_var("AOIDE_ROOT"),
        }
        match saved_home {
            Some(v) => std::env::set_var("HOME", v),
            None => std::env::remove_var("HOME"),
        }
        match saved_user {
            Some(v) => std::env::set_var("AOIDE_USER", v),
            None => std::env::remove_var("AOIDE_USER"),
        }
    }

    /// [`migrate_root_once`] is a plain function, not gated behind a
    /// process-wide `Once` (see its own doc for why) — every test below
    /// calls it directly and is independent of every other.
    #[test]
    fn migrate_root_once_moves_the_core_trees_and_leaves_the_rice_tree_in_place() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-root-basic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);

        let old_stage = home.join("Aoide").join("song").join("stage");
        let old_state = home.join("Aoide").join("state");
        let old_log = home.join("Aoide").join("log");
        std::fs::create_dir_all(&old_stage).unwrap();
        std::fs::write(old_stage.join("livery.json"), "rice-palette").unwrap();
        std::fs::create_dir_all(old_state.join("captures")).unwrap();
        std::fs::write(old_state.join("captures").join("shot.png"), "pixels").unwrap();
        std::fs::write(&old_log, "audit-line\n").unwrap();

        migrate_root_once().unwrap();

        let new_root = home.join(".aoide");
        assert_eq!(
            std::fs::read_to_string(new_root.join("state").join("captures").join("shot.png")).unwrap(),
            "pixels"
        );
        assert_eq!(std::fs::read_to_string(new_root.join("log")).unwrap(), "audit-line\n");
        assert!(!old_state.exists(), "the old state tree must have moved");
        assert!(!old_log.exists(), "the old log file must have moved");
        assert_eq!(
            std::fs::read_to_string(old_stage.join("livery.json")).unwrap(),
            "rice-palette",
            "the rice tree is strillix's to move, never core's"
        );
        assert!(!home.join(".strillix").exists(), "core's migration creates no rice-tree path");

        let _ = std::fs::remove_dir_all(&home);
    }

    /// The rename keeps the legacy tree's mode, and [`migrate_root_once`] locks
    /// the `state/` it moved before anything reads it.
    #[cfg(unix)]
    #[test]
    fn migrate_root_once_leaves_the_state_it_moves_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-root-mode-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        let old_state = home.join("Aoide").join("state");
        std::fs::create_dir_all(old_state.join("identity")).unwrap();
        std::fs::set_permissions(&old_state, std::fs::Permissions::from_mode(0o755)).unwrap();

        migrate_root_once().unwrap();

        let state = home.join(".aoide").join("state");
        assert!(state.join("identity").is_dir(), "the state tree moved");
        assert_eq!(unix_mode(&state), 0o700, "the migrated state/ kept the legacy tree's mode");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A tree that moved but whose `state/` cannot then be locked fails the
    /// migration instead of being narrated past. The legacy `state/` here is a
    /// symlink, so the moved one is too, and the lock refuses it at the open.
    /// The link's target keeps its mode.
    #[cfg(unix)]
    #[test]
    fn migrate_root_once_fails_when_the_state_it_moved_cannot_be_locked() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-root-unlockable-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        let target = home.join("elsewhere");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::create_dir_all(home.join("Aoide")).unwrap();
        std::os::unix::fs::symlink(&target, home.join("Aoide").join("state")).unwrap();

        let err = migrate_root_once().expect_err("a moved state/ that cannot be locked must fail the migration");
        let state = home.join(".aoide").join("state");
        assert!(err.to_string().contains(&*state.to_string_lossy()), "the error must name the path: {err}");
        assert!(err.to_string().contains("symlink"), "the error must say why: {err}");
        assert_eq!(unix_mode(&target), 0o755, "the link's target was chmodded");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The run that moves an unlockable `state/` fails, and so does every run
    /// after it: the second run finds the moved tree already in place and has
    /// nothing to move, so only the lock at the start can refuse it.
    #[cfg(unix)]
    #[test]
    fn migrate_root_once_refuses_an_unlockable_state_on_every_run() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-root-every-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        let target = home.join("elsewhere");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::create_dir_all(home.join("Aoide")).unwrap();
        std::os::unix::fs::symlink(&target, home.join("Aoide").join("state")).unwrap();
        let state = home.join(".aoide").join("state");

        migrate_root_once().expect_err("the run that moves an unlockable state/ must fail");
        assert!(std::fs::symlink_metadata(&state).unwrap().file_type().is_symlink(), "the link moved");
        assert!(std::fs::symlink_metadata(home.join("Aoide").join("state")).is_err(), "nothing is left to move");

        for run in 2..=3 {
            let err = migrate_root_once().expect_err("a later run must refuse the same state/");
            let named = err.to_string().contains(&*state.to_string_lossy());
            assert!(named, "run {run}: the error must name the path: {err}");
            assert!(err.to_string().contains("symlink"), "run {run}: the error must say why: {err}");
        }
        assert_eq!(unix_mode(&target), 0o755, "the link's target was chmodded");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A `state/` in place that cannot be locked fails the migration with no
    /// legacy tree at all: a link, and a link to nothing.
    #[cfg(unix)]
    #[test]
    fn migrate_root_once_refuses_a_state_already_in_place_that_is_a_link() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-root-linked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        let target = home.join("elsewhere");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        let root = home.join(".aoide");
        std::fs::create_dir_all(&root).unwrap();
        let state = root.join("state");
        std::os::unix::fs::symlink(&target, &state).unwrap();

        let err = migrate_root_once().expect_err("a linked state/ must fail the migration");
        assert!(err.to_string().contains("symlink"), "the error must say why: {err}");
        assert_eq!(unix_mode(&target), 0o755, "the link's target was chmodded");

        std::fs::remove_file(&state).unwrap();
        std::os::unix::fs::symlink(home.join("nowhere"), &state).unwrap();
        let err = migrate_root_once().expect_err("a dangling state/ link must fail the migration");
        assert!(err.to_string().contains("symlink"), "the error must say why: {err}");
        assert!(!home.join("nowhere").exists(), "the refusal created the link's target");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A `state/` already in place and wider than owner-only is locked by the
    /// start-up, not only one the migration moved; and a start-up that finds no
    /// `state/` makes none.
    #[cfg(unix)]
    #[test]
    fn migrate_root_once_locks_a_state_in_place_and_makes_none() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-root-inplace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        let state = home.join(".aoide").join("state");

        migrate_root_once().unwrap();
        assert!(!state.exists(), "a start-up with no state/ created one");

        std::fs::create_dir_all(&state).unwrap();
        std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
        migrate_root_once().unwrap();
        assert_eq!(unix_mode(&state), 0o700, "a state/ already in place was left wide");
        let _ = std::fs::remove_dir_all(&home);
    }

    /// A move that fails is narrated, never an `Err`: here `<home>/.aoide` is a
    /// file, so the tree cannot land and stays where it was, and there is no
    /// `state/` to lock.
    #[test]
    fn migrate_root_once_narrates_a_failed_move_and_leaves_the_old_tree() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-root-failed-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        let old_state = home.join("Aoide").join("state");
        std::fs::create_dir_all(&old_state).unwrap();
        std::fs::write(old_state.join("nodes.json"), "kept").unwrap();
        std::fs::write(home.join(".aoide"), "not a directory").unwrap();

        migrate_root_once().expect("a failed move is narrated, not an error");
        migrate_root_once().expect("and so is the next run's");
        assert_eq!(std::fs::read_to_string(old_state.join("nodes.json")).unwrap(), "kept");
        let _ = std::fs::remove_dir_all(&home);
    }


    /// A legacy `herald.json` in the new stage, or in
    /// the legacy stage, arrives as `notifications.json`, once, no-clobber.
    #[test]
    fn migrate_conducting_stage_renames_the_legacy_herald_ledger() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-herald-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        let new_dir = home.join("state").join("stage");
        std::fs::create_dir_all(&new_dir).unwrap();
        std::fs::write(new_dir.join("herald.json"), "ledger").unwrap();
        std::fs::create_dir_all(home.join(".strillix").join("stage")).unwrap();
        std::fs::write(home.join(".strillix").join("stage").join("herald.json"), "older").unwrap();

        migrate_conducting_stage(&new_dir);

        assert_eq!(std::fs::read_to_string(new_dir.join("notifications.json")).unwrap(), "ledger");
        assert!(!new_dir.join("herald.json").exists(), "the in-stage legacy ledger must have moved");
        assert!(home.join(".strillix").join("stage").join("herald.json").exists(), "the older legacy ledger waits, never clobbers");
        let _ = std::fs::remove_dir_all(&home);
    }


    /// The peer -> node state-file rename rides the same [`migrate_root_once`]
    /// entry point, but at [`state_dir`] itself rather than the pre-L-C2
    /// root — it must fire even when there is no pre-L-C2 tree to move.
    #[test]
    fn migrate_root_once_renames_the_old_peer_state_files() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-node-names-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);

        let state = home.join(".aoide").join("state");
        std::fs::create_dir_all(state.join("peer-cache")).unwrap();
        std::fs::write(state.join("peers.json"), "old-registry").unwrap();
        std::fs::write(state.join("peer-cache").join("box-b.json"), "cached-probe").unwrap();
        std::fs::write(state.join("peer-pairing-inbound.json"), "inbound").unwrap();
        std::fs::write(state.join("peer-pairing-outbound.json"), "outbound").unwrap();

        migrate_root_once().unwrap();

        assert_eq!(std::fs::read_to_string(state.join("nodes.json")).unwrap(), "old-registry");
        assert_eq!(
            std::fs::read_to_string(state.join("node-cache").join("box-b.json")).unwrap(),
            "cached-probe"
        );
        assert_eq!(
            std::fs::read_to_string(state.join("node-pairing-inbound.json")).unwrap(),
            "inbound"
        );
        assert_eq!(
            std::fs::read_to_string(state.join("node-pairing-outbound.json")).unwrap(),
            "outbound"
        );
        assert!(!state.join("peers.json").exists());
        assert!(!state.join("peer-cache").exists());
        assert!(!state.join("peer-pairing-inbound.json").exists());
        assert!(!state.join("peer-pairing-outbound.json").exists());

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn migrate_root_once_never_clobbers_an_existing_nodes_json() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-migrate-node-names-no-clobber-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);

        let state = home.join(".aoide").join("state");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(state.join("peers.json"), "stale").unwrap();
        std::fs::write(state.join("nodes.json"), "fresh").unwrap();

        migrate_root_once().unwrap();

        assert_eq!(std::fs::read_to_string(state.join("nodes.json")).unwrap(), "fresh");
        assert_eq!(
            std::fs::read_to_string(state.join("peers.json")).unwrap(),
            "stale",
            "the old file is left alone, not deleted, when the new one already exists"
        );

        let _ = std::fs::remove_dir_all(&home);
    }

    /// A signal that interrupts the wait for a stage lock does not refuse it: the
    /// wait is retried and the lock is taken once its holder lets go. The handler
    /// is installed without `SA_RESTART`, so `flock(2)` really returns `EINTR`.
    #[cfg(unix)]
    #[test]
    fn a_signal_that_interrupts_the_stage_lock_wait_does_not_refuse_it() {
        use std::os::unix::io::AsRawFd;
        extern "C" fn noop(_sig: libc::c_int) {}
        let home = std::env::temp_dir().join(format!("aoide-eintr-lock-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        let path = home.join("lock");
        let holder = std::fs::OpenOptions::new().create(true).write(true).open(&path).unwrap();
        let waiter = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        assert_eq!(unsafe { libc::flock(holder.as_raw_fd(), libc::LOCK_EX) }, 0);

        /// Puts the disposition back when the test ends, a panic included, so no
        /// later test in this process inherits the handler.
        struct Restore(libc::c_int, libc::sigaction);
        impl Drop for Restore {
            fn drop(&mut self) {
                unsafe { libc::sigaction(self.0, &self.1, std::ptr::null_mut()) };
            }
        }
        let usr2 = libc::SIGUSR2;
        let mut old: libc::sigaction = unsafe { std::mem::zeroed() };
        let mut sa: libc::sigaction = unsafe { std::mem::zeroed() };
        sa.sa_sigaction = noop as *const () as libc::sighandler_t;
        unsafe {
            libc::sigemptyset(&mut sa.sa_mask);
            libc::sigaction(usr2, &sa, &mut old);
        }
        let _restore = Restore(usr2, old);
        let (tid_tx, tid_rx) = std::sync::mpsc::channel();
        let blocked = std::thread::spawn(move || {
            tid_tx.send(unsafe { libc::pthread_self() }).unwrap();
            lock_exclusive(&waiter)
        });
        let tid = tid_rx.recv().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(200));
        unsafe { libc::pthread_kill(tid, usr2) };
        std::thread::sleep(std::time::Duration::from_millis(100));
        drop(holder);
        let taken = blocked.join().unwrap();
        let _ = std::fs::remove_dir_all(&home);
        assert!(taken, "the interrupted wait retries and takes the lock");
    }


    #[cfg(unix)]
    #[test]
    fn atomic_symlink_repoints_in_place_and_replaces_a_regular_file() {
        let dir = std::path::PathBuf::from(scratch("aoide-test-atomic-symlink"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("first.conf");
        let second = dir.join("second.conf");
        std::fs::write(&first, "a").unwrap();
        std::fs::write(&second, "b").unwrap();
        let link = dir.join("rice.conf");

        // A regular file at the link is replaced, not followed.
        std::fs::write(&link, "stale").unwrap();
        atomic_symlink(&first, &link).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), first);
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "a");

        // Repointing moves the link; repeating the same call is the same state.
        atomic_symlink(&second, &link).unwrap();
        atomic_symlink(&second, &link).unwrap();
        assert_eq!(std::fs::read_link(&link).unwrap(), second);
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "b");

        // No temp name is left behind beside the link.
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "temp links left behind: {leftovers:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
