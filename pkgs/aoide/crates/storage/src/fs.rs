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
//!
//! **The write and lock family is `ground::fs`.** Each wrapper below keeps
//! its old name and signature and passes [`ensure_write_dir`], this crate's
//! `state/` guard, as ground's directory step; the rest of the family is
//! re-exported by name.

pub use ground::fs::{
    atomic_symlink, detach, link_dir, lock_exclusive, looks_absolute_any_host, path_is_under, pid_is_alive, terminate,
    try_lock_exclusive, unlock, wait_for_exit, Waited, watch_writes, WriteWatch,
};

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
/// **Deliberately pure — no migration side effect on this path.**
/// `stage_dir`/`state_dir` (and this function transitively) are reached by
/// [`with_stage_lock`], the SHARED lock primitive nearly every stage-file
/// writer across the whole workspace routes through regardless of which file
/// it's actually touching. A live incident during development proved that a
/// `state_dir()`-only test (a command's own tests, overriding only
/// `$AOIDE_STATE_DIR`) still reaches the stage paths through `with_stage_lock`,
/// so a migration hung off this resolution would run against the operator's
/// actual `$HOME` the first time such a test ran unguarded. See
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
/// CURRENT `$AOIDE_ROOT` resolution. `pub`: the core binaries call this ONCE, early
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


/// [`ground::fs::migrate_dir`], with this crate's `state/` guard as the directory step.
pub fn migrate_dir(old: &std::path::Path, new: &std::path::Path) -> std::io::Result<()> {
    ground::fs::migrate_dir(old, new, ensure_write_dir)
}

/// [`ground::fs::migrate_file`], with this crate's `state/` guard as the directory step.
pub fn migrate_file(old: &std::path::Path, new: &std::path::Path) {
    ground::fs::migrate_file(old, new, ensure_write_dir)
}

/// [`ground::fs::copy_dir_recursive`], with this crate's `state/` guard as the directory step.
pub fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    ground::fs::copy_dir_recursive(src, dst, ensure_write_dir)
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
/// `~/.aoide/state/stage/`), holding the seven core roster files:
/// sessions.json, hooks.json, projects.json, graph.json, pending.json,
/// notifications.json and jobs.json. `aoide`/`aoided` alone read and write them
/// (the "L4 dual-writer surface", `conduct/README.md`, mirrored by
/// `server/src/daemon.rs::stage_roster`).
///
/// `$AOIDE_STAGE_DIR` wins when set to an absolute path, and names this
/// directory and nothing else: the rice stage never follows it.
pub fn conducting_stage_dir() -> std::path::PathBuf {
    stage_override().unwrap_or_else(|| state_dir().join("stage"))
}

/// `$AOIDE_STAGE_DIR` when it is absolute: the conducting stage's override.
fn stage_override() -> Option<std::path::PathBuf> {
    std::env::var("AOIDE_STAGE_DIR").ok().map(std::path::PathBuf::from).filter(|p| p.is_absolute())
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

/// Whether `path` lies inside [`session_logs_dir`], by its raw spelling or, where
/// both resolve, its canonical one. A path there is a conducted terminal's
/// transcript, never a harness's journal, so a reader of a `logPath` refuses it.
pub fn inside_session_logs(path: &str) -> bool {
    let dir = session_logs_dir();
    let canon_dir = std::fs::canonicalize(&dir).ok();
    let inside = |p: &std::path::Path| {
        p.starts_with(&dir) || canon_dir.as_deref().is_some_and(|d| p.starts_with(d))
    };
    let raw = std::path::Path::new(path);
    inside(raw) || std::fs::canonicalize(raw).is_ok_and(|c| inside(&c))
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


/// [`ground::fs::atomic_write_bytes`], with this crate's `state/` guard as the directory step.
pub fn atomic_write_bytes(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    ground::fs::atomic_write_bytes(path, contents, ensure_write_dir)
}

/// [`ground::fs::atomic_write`], with this crate's `state/` guard as the directory step.
pub fn atomic_write(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    ground::fs::atomic_write(path, contents, ensure_write_dir)
}

/// [`ground::fs::atomic_write_private`], with this crate's `state/` guard as the directory step.
pub fn atomic_write_private(path: &std::path::Path, contents: &[u8]) -> std::io::Result<()> {
    ground::fs::atomic_write_private(path, contents, ensure_write_dir)
}

/// [`ground::fs::create_new_private`], with this crate's `state/` guard as the directory step.
pub fn create_new_private(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    ground::fs::create_new_private(path, ensure_write_dir)
}

/// [`ground::fs::open_private_append`], with this crate's `state/` guard as the directory step.
pub fn open_private_append(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    ground::fs::open_private_append(path, ensure_write_dir)
}

/// [`ground::fs::secure_private_dir`], with this crate's `state/` guard as the directory step.
pub fn secure_private_dir(dir: &std::path::Path) -> std::io::Result<()> {
    ground::fs::secure_private_dir(dir, ensure_write_dir)
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
    let open = || ground::fs::open_dir_nofollow(state);
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

/// Runs `f` under the CONDUCTING stage's lock (`state/stage/.stage.lock`, see
/// [`conducting_stage_dir`]), held for the closure's duration. Every core writer
/// of the seven roster files (jobs.json among them) and the `state/` records
/// takes it.
///
/// Best-effort, and re-entrant on the thread that already holds it: a nested
/// call runs its closure directly. Every other thread still waits, and two
/// processes still serialise. If the lock file cannot be created or locked, `f`
/// runs unlocked rather than blocking on a hiccup, and nothing is recorded as
/// held. The rice stage has its own lock on its own directory ([`with_lock_in`]),
/// and no file is written under both, so the two never guard the same file.
pub fn with_stage_lock<T>(f: impl FnOnce() -> T) -> T {
    with_lock_in(&conducting_stage_dir(), f)
}

/// [`ground::fs::with_lock_in`], with this crate's `state/` guard as the directory step.
pub fn with_lock_in<T>(dir: &std::path::Path, f: impl FnOnce() -> T) -> T {
    ground::fs::with_lock_in(dir, ensure_write_dir, f)
}

/// Whether THIS thread holds the conducting stage's lock right now: true inside
/// [`with_stage_lock`] or [`try_stage_lock`] on a successful lock.
pub fn stage_lock_held() -> bool {
    ground::fs::lock_held_in(&conducting_stage_dir())
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
    ground::fs::try_lock_in(&conducting_stage_dir(), ensure_write_dir, f)
}

/// [`ground::fs::lock_path`], with this crate's `state/` guard as the directory step.
pub(crate) fn lock_path<T>(path: std::path::PathBuf, f: impl FnOnce() -> T) -> Result<T, String> {
    ground::fs::lock_path(path, ensure_write_dir, f)
}

/// [`ground::fs::seed_if_absent`], with this crate's `state/` guard as the directory step.
pub fn seed_if_absent(path: &std::path::Path, empty_body: &str, key: &str) -> Option<String> {
    ground::fs::seed_if_absent(path, empty_body, key, ensure_write_dir)
}

// ── Tests (the AOIDE_STAGE_DIR precedence seam; CONTRACTS.md §4) ─────────────
//
// These exercise the conducting-stage resolution, `seed_if_absent` and
// `atomic_write`, which are defined in this module.

#[cfg(test)]
mod tests {
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

        ground::fs::write_temp_file(&state.join("identity").join("k.tmp"), b"x", Some(0o600), ensure_write_dir).unwrap();
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
            owner_only_state(&home, std::slice::from_ref(&home)).expect_err("the home directory must be refused by identity");
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


    // ── `conducting_stage_dir` resolves, and moves nothing ────────────────

    /// The pre-split conducting files under `$STRILLIX_ROOT/stage/` are not the
    /// roster's: resolving the conducting stage moves none of them and reads
    /// none of them. The legacy files stay where they are, byte for byte, and
    /// the roster loads empty.
    #[test]
    fn conducting_stage_dir_leaves_the_legacy_stage_files_where_they_are() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = MigrationEnvGuard::capture_and_clear();
        let home = std::env::temp_dir().join(format!("aoide-legacy-stage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::env::set_var("HOME", &home);
        std::env::set_var("AOIDE_ROOT", home.join(".aoide"));
        std::env::set_var("STRILLIX_ROOT", home.join(".strillix"));

        let legacy = home.join(".strillix").join("stage");
        std::fs::create_dir_all(&legacy).unwrap();
        let sessions = r#"{"schemaVersion":"0","sessions":[{"sessionId":"legacy"}]}"#;
        std::fs::write(legacy.join("sessions.json"), sessions).unwrap();
        std::fs::write(legacy.join("herald.json"), "ledger").unwrap();

        let new_stage = conducting_stage_dir();
        assert_eq!(new_stage, home.join(".aoide").join("state").join("stage"));

        assert_eq!(
            std::fs::read_to_string(legacy.join("sessions.json")).ok().as_deref(),
            Some(sessions),
            "sessions.json must stay at the legacy path, untouched"
        );
        assert_eq!(
            std::fs::read_to_string(legacy.join("herald.json")).ok().as_deref(),
            Some("ledger"),
            "herald.json must stay at the legacy path, untouched"
        );
        assert!(!new_stage.join("sessions.json").exists(), "no legacy session file may move into the stage");
        assert!(!new_stage.join("notifications.json").exists(), "no legacy herald ledger may rename into the stage");

        let roster: crate::records::SessionsFile =
            crate::stage::load_stage(&crate::stage::sessions_path()).unwrap();
        assert!(roster.sessions.is_empty(), "the roster must not read the legacy session: {:?}", roster.sessions);

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
        let cases: [(&str, Creator); 15] = [
            ("create_new_private", |s| create_new_private(&s.join("a.json")).map(drop)),
            ("edits::append", |_| {
                crate::edits::append(&crate::edits::EditLine::default())
                    .map_err(std::io::Error::other)
            }),
            ("outbox::try_take_poll_lock", |_| {
                crate::outbox::try_take_poll_lock("node-a")
                    .map(drop)
                    .map_err(std::io::Error::other)
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
                    .map_err(std::io::Error::other)
            }),
            ("seed_if_absent", |s| {
                match seed_if_absent(&s.join("sessions.json"), "{\"sessions\":[]}", "sessions") {
                    Some(_) => Ok(()),
                    None => Err(std::io::Error::other("not seeded")),
                }
            }),
            ("with_stage_lock", |s| {
                std::env::set_var("AOIDE_STAGE_DIR", s.join("stage"));
                with_stage_lock(|| ());
                Ok(())
            }),
            ("atomic_write_bytes", |s| atomic_write_bytes(&s.join("b.bin"), b"x")),
            ("try_stage_lock", |s| {
                std::env::set_var("AOIDE_STAGE_DIR", s.join("stage"));
                try_stage_lock(|| ()).map_err(std::io::Error::other)
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

    /// Runs `f` with `AOIDE_STATE_DIR` at a fresh dir whose session logs dir
    /// exists, and restores the variable after.
    #[cfg(unix)]
    fn with_session_logs<T>(name: &str, f: impl FnOnce(&std::path::Path) -> T) -> T {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        in_session_logs(name, f)
    }

    /// Restores `AOIDE_STATE_DIR` and removes the temp root when it drops, so a
    /// closure that panics leaves neither set.
    struct SessionLogsScope {
        saved: Option<String>,
        root: std::path::PathBuf,
    }

    impl Drop for SessionLogsScope {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
            match self.saved.take() {
                Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
                None => std::env::remove_var("AOIDE_STATE_DIR"),
            }
        }
    }

    /// The body of [`with_session_logs`], for a caller that already holds
    /// `crate::env_lock()`.
    fn in_session_logs<T>(name: &str, f: impl FnOnce(&std::path::Path) -> T) -> T {
        let root = std::env::temp_dir().join(format!("aoide-inside-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let _scope = SessionLogsScope { saved: std::env::var("AOIDE_STATE_DIR").ok(), root: root.clone() };
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::fs::create_dir_all(super::session_logs_dir()).unwrap();
        f(&root)
    }

    #[cfg(unix)]
    #[test]
    fn a_panicking_session_logs_scope_leaves_the_state_dir_as_it_was() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let before = std::env::var("AOIDE_STATE_DIR").ok();
        for (name, set) in [("panics-set", Some("/sentinel/state")), ("panics-unset", None)] {
            match set {
                Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
                None => std::env::remove_var("AOIDE_STATE_DIR"),
            }
            let root = std::env::temp_dir().join(format!("aoide-inside-{name}-{}", std::process::id()));
            let caught = std::panic::catch_unwind(|| {
                in_session_logs(name, |_| -> () { panic!("the closure panics") });
            });
            let after = std::env::var("AOIDE_STATE_DIR").ok();
            let root_left = root.exists();
            match &before {
                Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
                None => std::env::remove_var("AOIDE_STATE_DIR"),
            }
            assert!(caught.is_err(), "the closure panics");
            assert_eq!(after.as_deref(), set, "a panic leaves the state dir as it was");
            assert!(!root_left, "a panic leaves no temp root: {}", root.display());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_raw_path_inside_the_session_logs_dir_is_inside() {
        with_session_logs("raw", |_| {
            let raw = super::session_logs_dir().join("t.log");
            assert!(super::inside_session_logs(&raw.to_string_lossy()));
        });
    }

    #[cfg(unix)]
    #[test]
    fn a_dotdot_path_that_comes_back_into_the_session_logs_dir_is_inside() {
        with_session_logs("back-in", |_| {
            let back = super::session_logs_dir().join("..").join("sessions").join("t.log");
            assert!(super::inside_session_logs(&back.to_string_lossy()));
        });
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_into_the_session_logs_dir_is_inside() {
        with_session_logs("link-in", |root| {
            let target = super::session_logs_dir().join("t.log");
            std::fs::write(&target, "").unwrap();
            let link = root.join("elsewhere.log");
            std::os::unix::fs::symlink(&target, &link).unwrap();
            assert!(super::inside_session_logs(&link.to_string_lossy()));
        });
    }

    /// Inside by its raw spelling, so a `..` that leaves the dir is refused.
    #[cfg(unix)]
    #[test]
    fn a_dotdot_path_that_leaves_the_dir_is_inside_by_its_spelling() {
        with_session_logs("leaves", |root| {
            std::fs::write(root.join("state").join("elsewhere.log"), "").unwrap();
            let leaves = super::session_logs_dir().join("..").join("elsewhere.log");
            assert!(super::inside_session_logs(&leaves.to_string_lossy()));
        });
    }
}
