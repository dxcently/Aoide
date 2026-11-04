//! The write and lock family: the atomic write and its private form, the
//! private create and append, the owner-only directory, the migration copy,
//! the registry seed, the lock files, the process probe and acts, and the
//! host seams (`detach`, `link_dir`, `atomic_symlink`, `path_is_under`,
//! `looks_absolute_any_host`).
//!
//! A member that makes a directory for a write, a copy or a lock file takes
//! the directory step as its `make_dir` argument and calls it where it makes
//! that directory, so none has a form without it: a crate that guards a tree
//! passes its guard, and a caller that wants none passes a plain create. Two
//! creations sit outside the step: `atomic_symlink` makes its link's parent
//! with a plain create, and `secure_private_dir`'s Windows arm makes its leaf
//! owner-only itself, after the step on the parent. The Windows half is
//! `fs_windows.rs`.

use std::io::Write;

/// The Windows half of this module: the lock, the no-clobber rename and the
/// link-preserving copy, whose Unix shapes have no Windows spelling. A
/// sibling file is not a non-`mod.rs` parent's default resolution, hence the
/// explicit `#[path]`, the same shape the protocol crate's
/// `feed`/`feed_windows.rs` pair uses.
#[cfg(windows)]
#[path = "fs_windows.rs"]
mod fs_windows;

// ── the lock, as both hosts' callers see it ──────────────────────────────

/// Take the exclusive lock on an open lock file, blocking until it is free.
/// Unix `flock(LOCK_EX)`; Windows `LockFileEx`. `false` means it was not
/// taken — every caller here already reads that as "not held", never as
/// "assume held".
///
/// `pub` (not `pub(crate)`): the conduct crate's own file-lock probe
/// (`graph/codex_app.rs::lock_is_held`) and its tests ask this exact question
/// of a file on the SAME host, so they call these three functions rather than
/// a second per-host lock mechanism (the workspace rule, "no cross-crate
/// copying": widen the symbol, never fork it).
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

/// Move `old` → `new` wholesale: a no-op unless `old` exists. An empty `new`
/// is a root something created before migration ran and is replaced; a `new`
/// that holds data is never clobbered — the move is narrated and `old` stays. `rename` first (same filesystem, the common case); a
/// cross-filesystem rename falls back to a recursive copy, removed from the
/// source only once the copy fully lands — a crash mid-copy leaves `old`
/// intact rather than a half-moved tree with `new` looking "done." A move
/// that fails is narrated, never a panic: the tree stays at the pre-migration
/// path, still usable, and boot goes on. A tree that moved is then handed to
/// `make_dir` (the rename made `new` with the legacy tree's mode), and a step
/// that fails is the one `Err`: the tree is now wide where every reader
/// looks.
pub fn migrate_dir(
    old: &std::path::Path,
    new: &std::path::Path,
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    if !old.exists() {
        return Ok(());
    }
    if new.exists() && std::fs::remove_dir(new).is_err() {
        eprintln!(
            "{} stays where it is: {} already holds data",
            old.display(),
            new.display()
        );
        return Ok(());
    }
    let Some(parent) = new.parent() else { return Ok(()) };
    if let Err(e) = make_dir(parent) {
        eprintln!(
            "cannot create {} ({e}) — {} stays at the old root",
            parent.display(),
            old.display()
        );
        return Ok(());
    }
    if std::fs::rename(old, new).is_err() {
        let tmp = temp_path(new, "migrate-tmp", std::process::id());
        if copy_dir_recursive(old, &tmp, make_dir).is_ok() && std::fs::rename(&tmp, new).is_ok() {
            let _ = std::fs::remove_dir_all(old);
        } else {
            let _ = std::fs::remove_dir_all(&tmp);
            eprintln!(
                "could not migrate {} to {} — staying at the old root",
                old.display(),
                new.display()
            );
            return Ok(());
        }
    }
    make_dir(new)
        .map_err(|e| std::io::Error::new(e.kind(), format!("{e} (migrated from {})", old.display())))
}

/// Move one file `old` → `new` — the single-file counterpart to
/// [`migrate_dir`], for the audit log (a flat file, not a directory).
/// Same no-clobber/no-panic discipline.
pub fn migrate_file(
    old: &std::path::Path,
    new: &std::path::Path,
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
) {
    if !old.exists() || new.exists() {
        return;
    }
    let Some(parent) = new.parent() else { return };
    if let Err(e) = make_dir(parent) {
        eprintln!(
            "cannot create {} ({e}) — {} stays at the old root",
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
            "could not migrate {} to {} — staying at the old root",
            old.display(),
            new.display()
        );
    }
}

/// Recursive directory copy: every file and subdir, symlinks preserved as
/// symlinks (never followed). Built for [`migrate_dir`]'s cross-filesystem
/// fallback; `pub` (not crate-private) because the rice crate's runtime
/// rices seed (`commands::rice::seed_rices_from_templates`, task #41)
/// reuses it verbatim for its own whole-tree copy rather than re-walking a
/// second way — both callers want the SAME semantics (unconditional, no
/// byte-diff/merge — the destination is always known-absent going in).
pub fn copy_dir_recursive(
    src: &std::path::Path,
    dst: &std::path::Path,
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    make_dir(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path, make_dir)?;
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
        std::fs::rename(&tmp, link).inspect_err(|_e| {
            let _ = std::fs::remove_file(&tmp);
        })
    }
    #[cfg(not(unix))]
    {
        let _ = (target, link);
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "symlinks are unix-only here"))
    }
}


/// A watch over the atomic writes of some paths. [`atomic_write_bytes`] runs
/// its rename inside [`around_write`](WriteWatch::around_write) of every live
/// watch registered for the path ([`watch_writes`]). A watch calls `rename`
/// exactly once and returns its result. It never writes a watched path from
/// inside `rename`, nor while it holds its own lock around the call.
pub trait WriteWatch: Send + Sync {
    /// Runs `rename` once, with whatever this watch does around the write of
    /// `path`, and returns `rename`'s result.
    fn around_write(
        &self,
        path: &std::path::Path,
        rename: &mut dyn FnMut() -> std::io::Result<()>,
    ) -> std::io::Result<()>;
}

/// Every registered watch, in registration order. A registration holds its
/// watch weakly, so it lasts as long as the watch's last `Arc`.
static WRITE_WATCHES: std::sync::Mutex<Vec<WriteWatchEntry>> = std::sync::Mutex::new(Vec::new());

struct WriteWatchEntry {
    paths: Vec<std::path::PathBuf>,
    watch: std::sync::Weak<dyn WriteWatch>,
}

/// Registers `watch` for the atomic writes of `paths`. The registration lasts
/// as long as the watch's last `Arc`; a dead entry is dropped by the next write
/// that scans the registry.
pub fn watch_writes(paths: Vec<std::path::PathBuf>, watch: std::sync::Weak<dyn WriteWatch>) {
    WRITE_WATCHES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(WriteWatchEntry { paths, watch });
}

/// The live watches registered for `path`, in registration order. The registry
/// lock is released before any of them runs.
fn live_write_watches(path: &std::path::Path) -> Vec<std::sync::Arc<dyn WriteWatch>> {
    let mut registry = WRITE_WATCHES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut live = Vec::new();
    registry.retain(|entry| {
        let Some(watch) = entry.watch.upgrade() else {
            return false;
        };
        if entry.paths.iter().any(|p| p == path) {
            live.push(watch);
        }
        true
    });
    live
}

/// Runs `rename` inside each of `watches`, the first one outermost.
fn around_write_watches(
    path: &std::path::Path,
    watches: &[std::sync::Arc<dyn WriteWatch>],
    rename: &mut dyn FnMut() -> std::io::Result<()>,
) -> std::io::Result<()> {
    match watches.split_first() {
        None => rename(),
        Some((first, rest)) => first.around_write(path, &mut || around_write_watches(path, rest, rename)),
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
/// The temp is `<stem>.<n>.tmp.<pid>`, `n` unique to this write. The write is
/// atomic across processes and across the threads of one process: writers of
/// one target each rename their own temp, so a reader sees one whole content.
/// On success the rename replaces the target and removes the temp in one step.
/// A failure in EITHER half — the write dying
/// partway, or the rename refusing — strands the temp we just made, so both
/// unlink it. And a write INTERRUPTED between create and rename — a SIGKILL, or a
/// power-cut (a stale `graph.tmp.464255` was found on disk) — can never clean up
/// after itself, so every write also sweeps the whole directory for temps left by
/// a pid that is no longer alive ([`sweep_stale_temps`]).
///
/// Bytes-oriented; [`atomic_write`] is the `&str` convenience wrapper every
/// existing JSON/text caller uses. Binary payloads (a widget QML file carried
/// verbatim into the runtime tree) route through here directly instead of
/// paying a lossy UTF-8 round-trip. The rename runs inside the live
/// [`WriteWatch`]es of `path` ([`watch_writes`]).
pub fn atomic_write_bytes(
    path: &std::path::Path,
    contents: &[u8],
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    atomic_write_bytes_impl(path, contents, None, make_dir)
}

/// Every temp this module names takes the next value of [`TEMP_SEQ`], so two
/// writes in one process never share a temp, threads included.
static TEMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// `target` with its extension replaced by `<n>.<kind>.<pid>`, `n` the next
/// [`TEMP_SEQ`] value. The counter sits before `.tmp.`, so [`sweep_stale_temps`],
/// which reads the pid after the last `.tmp.`, still reads the pid.
fn temp_path(target: &std::path::Path, kind: &str, pid: u32) -> std::path::PathBuf {
    let n = TEMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    target.with_extension(format!("{n}.{kind}.{pid}"))
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
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
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
        make_dir(parent)?;
    }
    let tmp = temp_path(&target, "tmp", std::process::id());
    // ONE failure path for both halves. A write that dies partway (ENOSPC inside
    // `write_all`) strands its temp exactly as a failed rename does, and a temp
    // stranded that way is worse than one the rename left: the file it belongs to
    // may never be written again (an outbox entry is keyed by a msgid that occurs
    // once), so nothing would ever come back for it.
    let res = match write_temp_file(&tmp, contents, create_mode, make_dir) {
        Ok(()) => around_write_watches(path, &live_write_watches(path), &mut || std::fs::rename(&tmp, &target)),
        Err(e) => Err(e),
    };
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
/// `atomic_write_bytes` caller's behavior byte-for-byte. `make_dir` is the
/// directory step, run on `tmp`'s parent before the create.
pub fn write_temp_file(
    tmp: &std::path::Path,
    contents: &[u8],
    create_mode: Option<u32>,
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    if let Some(parent) = tmp.parent() {
        make_dir(parent)?;
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
    // `owner_only` attaches AT CREATION, which is the only shape with no
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
            let file = crate::owner_only::create_truncating(tmp)?;
            match crate::owner_only::file_privacy(tmp)? {
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
pub fn atomic_write(
    path: &std::path::Path,
    contents: &str,
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    atomic_write_bytes(path, contents.as_bytes(), make_dir)
}

/// Atomic write of a SENSITIVE file, locked to `0600` (owner rw only) with
/// NO window at any wider mode — the precedent for private material that
/// must never enter a `Serialize`/`Deserialize` type (the identity keypair's
/// private-key file: the storage crate's `identity.rs` is the first caller),
/// mirroring the secrets crate's `store.rs`
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
pub fn atomic_write_private(
    path: &std::path::Path,
    contents: &[u8],
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    atomic_write_bytes_impl(path, contents, Some(0o600), make_dir)
}

/// Detach a `Command` before it is spawned: its own session/process group, no
/// inherited console, so the child outlives the process that started it — the
/// ONE seam for that posture (the workspace rule, "no cross-crate
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
/// `windows_sys`: two bits are not worth an import. No `CREATE_NO_WINDOW`: the
/// windowed caller's whole point is a terminal that draws, and
/// `DETACHED_PROCESS` alone already denies the child an inherited console.
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
/// (`conduct`'s `hooks install` skill links, the desktop half's `~/rice` link) —
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
pub fn create_new_private(
    path: &std::path::Path,
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
) -> std::io::Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        make_dir(parent)?;
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
        let file = match crate::owner_only::create_new(path) {
            Ok(file) => file,
            Err(crate::owner_only::CreateError::Exists) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::AlreadyExists,
                    format!("{} already exists", path.display()),
                ))
            }
            Err(crate::owner_only::CreateError::Failed(e)) => return Err(e),
        };
        match crate::owner_only::file_privacy(path)? {
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
pub fn open_private_append(
    path: &std::path::Path,
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
) -> std::io::Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        make_dir(parent)?;
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
        match crate::owner_only::create_new(path) {
            Ok(file) => match crate::owner_only::file_privacy(path)? {
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
            Err(crate::owner_only::CreateError::Exists) => {
                let file = std::fs::OpenOptions::new().append(true).open(path)?;
                match crate::owner_only::file_privacy(path)? {
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
            Err(crate::owner_only::CreateError::Failed(e)) => Err(e),
        }
    }
}

/// Create `dir` (if absent) and lock it down to `0700` (owner rwx only) —
/// the directory-level half of [`atomic_write_private`]'s discipline
/// (review rider: nothing else in the storage crate secured the DIRECTORY a
/// private file lives in, only the file itself, so a state dir left at
/// `create_dir_all`'s umask-derived default — 0755 under this box's normal
/// 022 — would leave `identity/`'s directory entries world-LISTABLE even
/// with `ed25519.key` itself locked to 0600). Mirrors the secrets crate's
/// `home::secure_dir` for one directory rather than that crate's whole
/// secrets home. `create_dir_all` is
/// idempotent on an already-present directory, and so is the chmod that
/// follows it — calling this on every mint (not only the very first one)
/// costs nothing and never regresses a directory some earlier run already
/// locked down. On Unix the chmod acts on a descriptor opened without
/// following a link at `dir` itself, so a `dir` that is a symlink is refused
/// and its target keeps its mode. A `dir` with a `..` in it is refused before
/// anything is created or changed, since the directory it climbs to is not one
/// this function was asked to own.
pub fn secure_private_dir(
    dir: &std::path::Path,
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
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
        make_dir(dir)?;
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
    // Windows: the ancestors go through `make_dir` (their mode is not
    // this function's business), and the leaf gets its policy attached AT
    // creation, then read back. `ensure_private_dir` tightens an existing
    // directory that is not private yet, and refuses one whose policy the
    // filesystem would not honor.
    #[cfg(windows)]
    {
        if let Some(parent) = dir.parent() {
            make_dir(parent)?;
        }
        crate::owner_only::ensure_private_dir(dir)
    }
}

/// `dir` opened read-only as a directory, refusing a link at its last component:
/// `ELOOP` for a link, `ENOTDIR` for anything that is not a directory.
#[cfg(unix)]
pub fn open_dir_nofollow(dir: &std::path::Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(dir)
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

/// Whether THIS thread holds `dir`'s lock right now: true inside
/// [`with_lock_in`] or [`try_lock_in`] on `dir` after a successful lock.
pub fn lock_held_in(dir: &std::path::Path) -> bool {
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

/// Runs `f` under the exclusive lock on `dir/.stage.lock`, best-effort and
/// re-entrant per lock file. A domain that owns a stage directory of its own
/// locks it through here: the rice stage does, through the rice crate's `lock`.
///
/// Best-effort, and re-entrant on the thread that already holds it: a nested
/// call runs its closure directly. Every other thread still waits, and two
/// processes still serialise. If the lock file cannot be created or locked, `f`
/// runs unlocked rather than blocking on a hiccup, and nothing is recorded as
/// held.
pub fn with_lock_in<T>(
    dir: &std::path::Path,
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
    f: impl FnOnce() -> T,
) -> T {
    if lock_held_in(dir) {
        return f();
    }
    // The lock file is a write like any other: when `dir` cannot be created it is
    // not, and `f` runs unlocked as the best-effort path says.
    let lock = make_dir(dir).ok().and_then(|()| {
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

/// Fail-closed sibling of [`with_lock_in`], and re-entrant the same way: a
/// nested call under a held lock runs `f` directly. A lock that can't be taken
/// rejects the work rather than running it unlocked. Blocks on `LOCK_EX` (a
/// concurrent opener waits rather than races); it returns `Err` only when
/// `make_dir` refuses `dir` or the lock file can't be opened or created, not
/// when another holder briefly has it.
pub fn try_lock_in<T>(
    dir: &std::path::Path,
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
    f: impl FnOnce() -> T,
) -> Result<T, String> {
    if lock_held_in(dir) {
        return Ok(f());
    }
    lock_path(dir.join(STAGE_LOCK_FILE), make_dir, || {
        let _held = HeldLock::take(dir);
        f()
    })
}

/// Fail-closed `flock(LOCK_EX)` on a dedicated file at `path`, held for the
/// closure's duration: open-or-create, lock (blocking — a concurrent opener
/// waits rather than races), run `f`, unlock. Shared by [`try_lock_in`]
/// (its lock file is the directory's `.stage.lock`) and the storage crate's
/// mail doorbell ring (`mail::with_ring_lock`, whose lock file is
/// `mail_dir()/.ring.lock` — a SEPARATE file: the ring is held across socket
/// I/O for the whole select-inject-stamp sequence, far longer than any stage
/// mutator ever holds `.stage.lock`, so the two must never share one file or
/// a slow ring would stall every session-graph writer on the desktop).
/// Not re-entrant (each call opens its own fd) — a caller must never nest two
/// calls against the same path.
pub fn lock_path<T>(
    path: std::path::PathBuf,
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
    f: impl FnOnce() -> T,
) -> Result<T, String> {
    if let Some(dir) = path.parent() {
        make_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
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

/// Is `pid` a live process? The one liveness probe: POSIX `kill(pid, 0)`
/// — `0`/`EPERM` live, `ESRCH` absent, any other errno conservatively live. Live
/// is not identity: a recycled pid is live.
///
/// **Windows**: `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` +
/// `GetExitCodeProcess` out of `win_proc`, with the same
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
        probe_verdict(std::io::Error::last_os_error().raw_os_error())
    }
    #[cfg(windows)]
    {
        crate::win_proc::is_alive(pid)
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
        let _ = crate::win_proc::terminate(pid);
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
/// `win_proc::wait_for_exit` for the table.
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
        match crate::win_proc::wait_for_exit(pid, block) {
            crate::win_proc::Waited::Exited => Waited::Exited,
            crate::win_proc::Waited::Running => Waited::Running,
            crate::win_proc::Waited::NotWaitable => Waited::NotWaitable,
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
/// named `<stem>.<n>.tmp.<pid>` whose `<pid>` is no longer a live process,
/// whatever its stem. An atomic write interrupted between create and rename (SIGKILL,
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
/// `<stem>.<n>.migrate-tmp.<pid>` and `<stem>.<n>.seed.<pid>` temps stay clear
/// of it.
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
        // Only a well-formed `<stem>.<n>.tmp.<pid>` whose pid is dead is swept; our
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
/// This is the restart-survival seam. A seeding process that is bound to its
/// session restarts with it — a system switch or a compositor restart stops and
/// starts it — and seeds on every start. The old unconditional re-seed wiped
/// every live session to `[]` on each such restart, and a record can only
/// re-register from a NEW `graph session start`, so already-running sessions
/// vanished from the roster/DAG until they happened to re-emit. Preserving a
/// valid file keeps live sessions across the restart window; a
/// missing/half-written/corrupt file still gets the empty v0 shape so the
/// process always comes up sane. Returns
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
pub fn seed_if_absent(
    path: &std::path::Path,
    empty_body: &str,
    key: &str,
    make_dir: fn(&std::path::Path) -> std::io::Result<()>,
) -> Option<String> {
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
            atomic_write(path, empty_body, make_dir)
                .ok()
                .map(|_| path.to_string_lossy().into_owned())
        }
        Err(_) => {
            // Absent (or unreadable). Seed with create-if-absent so a roster that
            // appeared mid-boot is never overwritten. A distinct `<n>.seed.<pid>`
            // temp keeps it clear of `sweep_stale_temps`'s `.tmp.<pid>` pattern.
            if let Some(parent) = path.parent() {
                if let Err(e) = make_dir(parent) {
                    eprintln!("cannot create {} ({e}) — {} not seeded", parent.display(), path.display());
                    return None;
                }
            }
            let tmp = temp_path(path, "seed", std::process::id());
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

    /// The directory step these tests pass: a plain create, with no guard.
    fn create_dir_all(dir: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)
    }

    /// Held by the two tests that set the process umask. Each restores the umask
    /// it found, so two of them interleaved could leave the process at 000.
    #[cfg(unix)]
    static UMASK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[cfg(unix)]
    fn unix_mode(dir: &std::path::Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(dir).unwrap().permissions().mode() & 0o777
    }

    /// Assert a FILE carries the private policy on THIS host: mode `0o600` on
    /// Unix, the owner-only DACL read back from the object on Windows.
    #[cfg(unix)]
    fn assert_private_file(path: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "{} must be 0600, got {mode:o}", path.display());
    }

    #[cfg(windows)]
    fn assert_private_file(path: &std::path::Path) {
        let refusal = crate::owner_only::file_privacy(path).unwrap();
        assert!(refusal.is_none(), "{} must carry the owner-only DACL: {refusal:?}", path.display());
    }

    /// The directory half, which only the Windows tests ask: the same owner-only
    /// DACL, where the mask's directory meanings cover list and add.
    #[cfg(windows)]
    fn assert_private_dir(dir: &std::path::Path) {
        let refusal = crate::owner_only::dir_privacy(dir).unwrap();
        assert!(refusal.is_none(), "{} must carry the owner-only DACL: {refusal:?}", dir.display());
    }

    // ── The restart-survival seam: seed preserves a live roster ──────────────

    #[test]
    fn seed_preserves_a_live_roster_and_replaces_a_corrupt_one() {
        // The transient-drop regression: a restart of the process that seeds
        // must NOT wipe a populated sessions.json. `seed_if_absent` preserves a
        // valid registry, replaces a corrupt/half-written one with the empty v0
        // shape, and seeds an absent one.
        let dir = std::env::temp_dir().join(format!("ground-seed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("sessions.json");

        let empty_body = r#"{"schemaVersion":"0","sessions":[]}"#;

        // A populated, valid registry survives untouched (the fix).
        let populated = r#"{"schemaVersion":"0","sessions":[{"sessionId":"live"}]}"#;
        std::fs::write(&path, populated).unwrap();
        assert_eq!(
            seed_if_absent(&path, empty_body, "sessions", create_dir_all),
            None,
            "a live roster is preserved across a restart seed"
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), populated);

        // A VALID-but-EMPTY roster is a real registry too — preserved, not re-seeded.
        std::fs::write(&path, empty_body).unwrap();
        assert_eq!(
            seed_if_absent(&path, empty_body, "sessions", create_dir_all),
            None,
            "a valid empty roster is preserved (classified present, not corrupt)"
        );

        // JSON-valid but WRONG SHAPE (`sessions` is an object, not an array) is
        // classified corrupt and replaced with the empty v0 shape.
        std::fs::write(&path, r#"{"schemaVersion":"0","sessions":{}}"#).unwrap();
        assert!(seed_if_absent(&path, empty_body, "sessions", create_dir_all).is_some());
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(v["sessions"].is_array() && v["sessions"].as_array().unwrap().is_empty());

        // Non-JSON / half-written garbage is likewise replaced.
        std::fs::write(&path, "{ not json at all").unwrap();
        assert!(seed_if_absent(&path, empty_body, "sessions", create_dir_all).is_some());
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert!(v["sessions"].is_array() && v["sessions"].as_array().unwrap().is_empty());

        // An absent file is seeded.
        let fresh = dir.join("hooks.json");
        assert!(seed_if_absent(&fresh, r#"{"schemaVersion":"0","hooks":[]}"#, "hooks", create_dir_all).is_some());
        assert!(fresh.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_sweeps_a_leaked_temp_from_a_dead_pid() {
        // A power-cut / SIGKILL between create and rename leaves a stranded
        // `<stem>.<n>.tmp.<pid>`, or the older `<stem>.tmp.<pid>` this test
        // plants (a real `graph.tmp.464255` was found on disk). The
        // next successful write of the same file sweeps a dead pid's temp but
        // spares a live node's in-flight one.
        let dir = std::env::temp_dir().join(format!("ground-tmpsweep-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let target = dir.join("graph.json");

        let leaked = dir.join(format!("graph.tmp.{}", u32::MAX)); // pid above pid_max — never alive
        std::fs::write(&leaked, "half-written").unwrap();
        let live_node = dir.join(format!("graph.tmp.{ALWAYS_LIVE_PID}"));
        std::fs::write(&live_node, "in-flight").unwrap();

        atomic_write(&target, "{}", create_dir_all).unwrap();

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
        let dir = std::env::temp_dir().join(format!("ground-tmpsweep-foreign-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let dead = u32::MAX; // above pid_max — never alive
        let orphan = dir.join(format!("aaaa1111.tmp.{dead}"));
        let in_flight = dir.join(format!("bbbb2222.tmp.{ALWAYS_LIVE_PID}"));
        // Deliberately distinct temp shapes owned by other writers
        // (`migrate_dir`, `seed_if_absent`) — `.tmp.` is a whole
        // separator, so neither is ours to remove.
        let migrate = temp_path(&dir.join("cccc3333"), "migrate-tmp", dead);
        let seed = temp_path(&dir.join("dddd4444"), "seed", dead);
        for p in [&orphan, &in_flight, &migrate, &seed] {
            std::fs::write(p, "stranded").unwrap();
        }

        atomic_write(&dir.join("unrelated.json"), "{}", create_dir_all).unwrap();

        assert!(!orphan.exists(), "a dead pid's temp is swept whatever its stem");
        assert!(in_flight.exists(), "a live pid's in-flight temp is left untouched");
        assert!(migrate.exists(), "`<n>.migrate-tmp.<pid>` is another writer's shape");
        assert!(seed.exists(), "`<n>.seed.<pid>` is another writer's shape");

        // A write that cannot land leaves no temp of its own: renaming onto a
        // non-empty directory fails, and the same unlink covers it.
        let blocked = dir.join("blocked.json");
        std::fs::create_dir_all(&blocked).unwrap();
        std::fs::write(blocked.join("occupant"), "x").unwrap();
        assert!(atomic_write(&blocked, "{}", create_dir_all).is_err());
        let own_suffix = format!(".tmp.{}", std::process::id());
        assert!(
            !std::fs::read_dir(&dir).unwrap().flatten().any(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.starts_with("blocked.") && name.ends_with(&own_suffix)
            }),
            "a failed write never leaves its own temp behind"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_sweep_reads_the_pid_of_a_counter_named_temp() {
        // The counter sits before `.tmp.`, so the sweep's split after the last
        // `.tmp.` still reads the pid: a dead pid's temp goes, while ours and a
        // live other pid's stay.
        let dir = std::env::temp_dir().join(format!("ground-tmpsweep-counter-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("graph.json");
        let dead = temp_path(&target, "tmp", u32::MAX); // pid above pid_max — never alive
        let own = temp_path(&target, "tmp", std::process::id());
        let live = temp_path(&target, "tmp", ALWAYS_LIVE_PID);
        for p in [&dead, &own, &live] {
            std::fs::write(p, "stranded").unwrap();
        }

        atomic_write(&target, "{}", create_dir_all).unwrap();

        assert!(!dead.exists(), "a dead pid's temp in the counter form is swept");
        assert!(own.exists(), "our own in-flight temp is spared");
        assert!(live.exists(), "a live other pid's temp is spared");
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
        let dir = std::env::temp_dir().join(format!("ground-atomic-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let real = dir.join("real.json");
        let link = dir.join("link.json");
        std::fs::write(&real, "seed").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        atomic_write(&link, "first", create_dir_all).unwrap();
        assert!(
            std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink(),
            "the symlink itself must survive the write, not get replaced by rename()"
        );
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "first", "the LINK TARGET carries the content");
        assert_eq!(std::fs::read_to_string(&link).unwrap(), "first", "reading through the link agrees");

        // A second write must keep working the same way — the fix isn't a
        // one-shot "first write creates a real file" side effect.
        atomic_write(&link, "second", create_dir_all).unwrap();
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
        let dir = std::env::temp_dir().join(format!("ground-atomic-symlink-rel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sub = dir.join("drafts").join("neon-night");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir_all(dir.join("stage")).unwrap();
        let real = sub.join("theme.json");
        std::fs::write(&real, "seed").unwrap();
        let link = dir.join("stage").join("theme.json");
        std::os::unix::fs::symlink("../drafts/neon-night/theme.json", &link).unwrap();

        atomic_write(&link, "routed", create_dir_all).unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read_to_string(&real).unwrap(), "routed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_bytes_round_trips_binary_content() {
        // The bytes-oriented primitive `atomic_write` now delegates to — a
        // widget QML file (or any non-UTF-8 payload) must round-trip exactly,
        // not just the `&str` callers `atomic_write` itself covers.
        let dir = std::env::temp_dir().join(format!("ground-atomic-bytes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("binary.dat");

        let bytes: &[u8] = &[0u8, 159, 146, 150];
        atomic_write_bytes(&path, bytes, create_dir_all).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_write_creates_a_plain_file_when_path_is_not_a_symlink() {
        // Regression coverage: the overwhelming common case (no symlink at
        // all) must behave exactly as before this fix.
        let dir = std::env::temp_dir().join(format!("ground-atomic-plain-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plain.json");

        atomic_write(&path, "content", create_dir_all).unwrap();
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
        let dir = std::env::temp_dir().join(format!("ground-atomic-private-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.key");

        atomic_write_private(&path, b"private bytes", create_dir_all).unwrap();
        assert_private_file(&path);
        assert_eq!(std::fs::read(&path).unwrap(), b"private bytes");

        // A second write to the SAME path (the re-mint-never-happens case,
        // but the primitive itself must stay correct either way) is still
        // locked to the same policy afterward.
        atomic_write_private(&path, b"replaced", create_dir_all).unwrap();
        assert_private_file(&path);
        assert_eq!(std::fs::read(&path).unwrap(), b"replaced");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_private_temp_is_created_already_0600_before_any_rename() {
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
        let dir = std::env::temp_dir().join(format!("ground-write-temp-file-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let tmp = dir.join("secret.key.tmp");

        write_temp_file(&tmp, b"private seed bytes", Some(0o600), create_dir_all).unwrap();
        assert_private_file(&tmp);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 879f7b78: a temp left under the private name by a write killed between
    /// create and rename keeps its wider mode through `O_TRUNC`. `write_temp_file`
    /// removes such a leftover before it creates the temp, so the temp is `0600`
    /// and the rename that follows publishes `0600`. The leftover reaches a name
    /// a later write uses only through pid reuse: a dead writer's temp, under a
    /// pid another process takes. The guard in `write_temp_file` is what covers
    /// that collision.
    #[cfg(unix)]
    #[test]
    fn write_temp_file_never_keeps_a_leftovers_wider_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("ground-leftover-temp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("ed25519.key");
        let leftover = temp_path(&target, "tmp", std::process::id());
        std::fs::write(&leftover, b"stale bytes").unwrap();
        std::fs::set_permissions(&leftover, std::fs::Permissions::from_mode(0o644)).unwrap();

        write_temp_file(&leftover, b"private seed bytes", Some(0o600), create_dir_all).unwrap();
        std::fs::rename(&leftover, &target).unwrap();

        assert_eq!(std::fs::read(&target).unwrap(), b"private seed bytes");
        assert_eq!(unix_mode(&target), 0o600, "the published file took the leftover's wider mode");
        assert!(!leftover.exists(), "the leftover temp must be gone after the write");
        let _ = std::fs::remove_dir_all(&dir);
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
        let _g = UMASK.lock().unwrap_or_else(|e| e.into_inner());
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("ground-atomic-private-umask-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.key");

        // SAFETY: umask(2) takes a plain mode_t and cannot fail; restored
        // unconditionally immediately after, under `UMASK`, which the other
        // umask test holds too (umask is process-wide, wider even than an env
        // var).
        let old_umask = unsafe { libc::umask(0o000) };
        let result = atomic_write_private(&path, b"private seed bytes", create_dir_all);
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
    /// leaves its temp behind at whatever mode it was made with. A later
    /// process that reuses that pid, at the same counter value, writes to the
    /// same name, and `O_TRUNC` keeps the leftover's mode rather than applying
    /// `0600` again. This test plants a `0644` leftover at the name a write by
    /// our own pid takes (the reuse case, without a dead process to stage it),
    /// then calls `write_temp_file` and renames the temp over the private name
    /// itself, and requires the result to come out `0600` with the new bytes.
    #[cfg(unix)]
    #[test]
    fn write_temp_file_never_keeps_a_wider_leftover_for_a_later_rename() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("ground-stale-private-temp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sessions.json");
        let leftover = temp_path(&path, "tmp", std::process::id());
        std::fs::write(&leftover, b"leftover").unwrap();
        std::fs::set_permissions(&leftover, std::fs::Permissions::from_mode(0o644)).unwrap();

        write_temp_file(&leftover, b"fresh", Some(0o600), create_dir_all).unwrap();
        std::fs::rename(&leftover, &path).unwrap();
        assert_private_file(&path);
        assert_eq!(std::fs::read(&path).unwrap(), b"fresh");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `atomic_write_private` publishes `0600` even in a wide directory beside
    /// a `0644` file. The directory's `0777` is set explicitly, so the umask
    /// does not narrow it. A private write that passed no mode would publish
    /// the umask-default `0644` here.
    #[cfg(unix)]
    #[test]
    fn atomic_write_private_publishes_0600_beside_a_wide_file() {
        use std::os::unix::fs::PermissionsExt;
        let _g = UMASK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("ground-private-beside-wide-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o777)).unwrap();
        let neighbour = dir.join("neighbour.json");
        std::fs::write(&neighbour, b"wide").unwrap();
        std::fs::set_permissions(&neighbour, std::fs::Permissions::from_mode(0o644)).unwrap();
        let target = dir.join("ed25519.key");

        // Pinned to the default 022 under `UMASK`, which the sibling test
        // above takes too, so a private write that passed no mode publishes
        // `0644` here whatever the ambient umask is.
        // SAFETY: umask(2) cannot fail; restored straight after the call.
        let old_umask = unsafe { libc::umask(0o022) };
        let result = atomic_write_private(&target, b"private seed bytes", create_dir_all);
        unsafe { libc::umask(old_umask) };
        result.unwrap();

        assert_private_file(&target);
        assert_eq!(std::fs::read(&target).unwrap(), b"private seed bytes");
        assert_eq!(unix_mode(&neighbour), 0o644, "the neighbour's mode must not change");
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
        let dir = std::env::temp_dir().join(format!("ground-secure-dir-tighten-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let before = crate::owner_only::dir_privacy(&dir).unwrap();
        assert!(before.is_some(), "the fixture must start with a policy this crate would not accept: {before:?}");

        secure_private_dir(&dir, create_dir_all).unwrap();
        assert_private_dir(&dir);

        // And it stays idempotent: a second call over an already-private
        // directory neither fails nor regresses it.
        secure_private_dir(&dir, create_dir_all).unwrap();
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
        let dir = std::env::temp_dir().join(format!("ground-secure-dir-repropagate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // The child the host wrote: inherited ACEs only.
        let inherited = dir.join("inherited.json");
        std::fs::write(&inherited, "payload").unwrap();
        assert!(std::fs::read(&inherited).is_ok(), "the fixture must start readable");

        secure_private_dir(&dir, create_dir_all).unwrap();

        let after = std::fs::read(&inherited);
        assert!(
            after.is_err(),
            "NT should have stripped the child's inherited ACEs with the parent's policy: {after:?}"
        );

        // The child THIS crate wrote: its own protected policy, unaffected by
        // any later change to its directory's.
        let owned = dir.join("owned.json");
        {
            let mut f = crate::owner_only::create_truncating(&owned).unwrap();
            f.write_all(b"payload").unwrap();
        }
        secure_private_dir(&dir, create_dir_all).unwrap();
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
        let base = std::env::temp_dir().join(format!("ground-secure-dir-junction-{}", std::process::id()));
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
        let reason = crate::owner_only::dir_privacy(&link).unwrap();
        assert!(reason.is_some(), "a junction is not the object the path resolves to: {reason:?}");

        let err = secure_private_dir(&link, create_dir_all).expect_err("a junction must be refused, never repaired");
        let text = err.to_string();
        assert!(text.contains("junction") || text.contains("symbolic link"), "{text}");

        let _ = std::fs::remove_dir(&link);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A signal that interrupts the wait for a stage lock does not refuse it: the
    /// wait is retried and the lock is taken once its holder lets go. The handler
    /// is installed without `SA_RESTART`, so `flock(2)` really returns `EINTR`.
    #[cfg(unix)]
    #[test]
    fn a_signal_that_interrupts_the_stage_lock_wait_does_not_refuse_it() {
        use std::os::unix::io::AsRawFd;
        extern "C" fn noop(_sig: libc::c_int) {}
        let home = std::env::temp_dir().join(format!("ground-eintr-lock-{}", std::process::id()));
        std::fs::create_dir_all(&home).unwrap();
        let path = home.join("lock");
        let holder = std::fs::OpenOptions::new().create(true).write(true).truncate(false).open(&path).unwrap();
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
        let dir = std::env::temp_dir().join("ground-test-atomic-symlink");
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

    // ── the write watch (`watch_writes`) ─────────────────────────────────────

    /// Records the bytes at its path before and after each rename, and the kind
    /// of any error the rename returned, which it passes on.
    struct Spy {
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        seen: std::sync::Mutex<Vec<Option<Vec<u8>>>>,
        rename_error: std::sync::Mutex<Option<std::io::ErrorKind>>,
    }

    impl super::WriteWatch for Spy {
        fn around_write(
            &self,
            path: &std::path::Path,
            rename: &mut dyn FnMut() -> std::io::Result<()>,
        ) -> std::io::Result<()> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.seen.lock().unwrap().push(std::fs::read(path).ok());
            let result = rename();
            if let Err(e) = &result {
                *self.rename_error.lock().unwrap() = Some(e.kind());
            }
            self.seen.lock().unwrap().push(std::fs::read(path).ok());
            result
        }
    }

    /// A fresh scratch directory for one write-watch test.
    fn watch_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ground-write-watch-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Registers a [`Spy`] for `path` and returns it. The caller's `Arc` keeps
    /// the registration alive.
    fn spy_on(path: &std::path::Path, calls: &std::sync::Arc<std::sync::atomic::AtomicUsize>) -> std::sync::Arc<Spy> {
        let spy = std::sync::Arc::new(Spy {
            calls: std::sync::Arc::clone(calls),
            seen: std::sync::Mutex::new(Vec::new()),
            rename_error: std::sync::Mutex::new(None),
        });
        let watch: std::sync::Arc<dyn super::WriteWatch> = spy.clone();
        super::watch_writes(vec![path.to_path_buf()], std::sync::Arc::downgrade(&watch));
        spy
    }

    fn new_calls() -> std::sync::Arc<std::sync::atomic::AtomicUsize> {
        std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0))
    }

    #[test]
    fn a_write_to_a_watched_path_renames_inside_its_watch() {
        let dir = watch_dir("renames");
        let path = dir.join("watched.json");
        super::atomic_write(&path, "old", create_dir_all).unwrap();
        let calls = new_calls();
        let spy = spy_on(&path, &calls);

        super::atomic_write(&path, "new", create_dir_all).unwrap();

        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(*spy.seen.lock().unwrap(), vec![Some(b"old".to_vec()), Some(b"new".to_vec())]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_write_to_an_unwatched_path_never_calls_the_watch() {
        let dir = watch_dir("unwatched");
        let calls = new_calls();
        let _spy = spy_on(&dir.join("watched.json"), &calls);

        super::atomic_write(&dir.join("other.json"), "bytes", create_dir_all).unwrap();

        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_dropped_write_watch_is_never_called_again() {
        let dir = watch_dir("dropped");
        let path = dir.join("watched.json");
        let calls = new_calls();
        let spy = spy_on(&path, &calls);
        super::atomic_write(&path, "first", create_dir_all).unwrap();
        drop(spy);

        super::atomic_write(&path, "second", create_dir_all).unwrap();

        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_write_whose_temp_cannot_be_created_never_calls_its_watch() {
        let dir = watch_dir("no-temp");
        // A 250-byte stem puts `<stem>.<n>.tmp.<pid>` past NAME_MAX (255), so the
        // create fails whatever the counter's value.
        let path = dir.join(format!("{}.json", "b".repeat(250)));
        let calls = new_calls();
        let _spy = spy_on(&path, &calls);

        let err = super::atomic_write(&path, "x", create_dir_all).unwrap_err();
        assert_eq!(err.raw_os_error(), Some(libc::ENAMETOOLONG), "the create fails on the name, not elsewhere: {err}");

        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_rename_returns_its_error_through_its_watch() {
        let dir = watch_dir("failed-rename");
        // A file cannot be renamed onto a directory, so the rename fails after
        // the temp is written.
        let path = dir.join("is-a-dir");
        std::fs::create_dir_all(&path).unwrap();
        let calls = new_calls();
        let spy = spy_on(&path, &calls);

        let err = super::atomic_write(&path, "x", create_dir_all).expect_err("a rename onto a directory must fail");

        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(*spy.rename_error.lock().unwrap(), Some(err.kind()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
