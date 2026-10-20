//! Shared test scaffolding for every aoide crate's test modules: env-var
//! save/restore, a scratch-dir helper, the process-wide env lock, the
//! delivery-fixture rig, and the fixture note payloads several command
//! groups' tests need.
//!
//! Extracted from the root package's `commands/mod.rs::test_support` +
//! `lib.rs::env_lock` (Phase 9 restructure,
//! docs/architecture/PACKAGE-LAYOUT.md) so the domain crates' own
//! `commands` tests use the SAME rig — pulled in as a **dev-dependency**
//! only; nothing in a production build may edge on this crate.

pub mod registry_walk;

use aoide_protocol::{Door, Invocation};
#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(windows)]
use aoide_protocol::win_unix::{UnixListener, UnixStream};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Create-or-truncate an EMPTY file in the shape THIS host's feed reader
/// demands: Unix's mode bits have no such reader, so a plain write is the whole
/// fixture; native Windows reads a feed's policy back from the object and
/// REFUSES one whose DACL still carries inherited ACEs — and `std::fs::write`
/// creates exactly such a file there. So the fixture creates it owner-only,
/// through the same policy seam a real writer uses.
///
/// This is a FIXTURE fact, not a test convenience: without it a follower sees
/// nothing, a daemon's own write is refused, and the failure surfaces as a
/// deadline ("never saw the appended lines in time") rather than as the policy
/// fact it is. ONE seam — `aoide-server`'s `events` and `daemon` fixtures both
/// ask this.
pub fn owner_only_file(path: &std::path::Path) {
    #[cfg(unix)]
    {
        std::fs::write(path, b"").unwrap();
    }
    #[cfg(windows)]
    match aoide_protocol::owner_only::create_new(path) {
        Ok(file) => drop(file),
        Err(aoide_protocol::owner_only::CreateError::Exists) => {
            let file = aoide_protocol::owner_only::create_truncating(path)
                .expect("re-create the fixture's feed file owner-only");
            drop(file);
        }
        Err(aoide_protocol::owner_only::CreateError::Failed(e)) => {
            panic!("create the fixture's feed file owner-only: {e}")
        }
    }
}

/// A SHORT scratch directory under this host's temp dir, bounded for `AF_UNIX`:
/// `sun_path` is 108 bytes with its terminator and native Windows accepts at
/// most 107, while this host's own temp prefix (`%LOCALAPPDATA%\Temp`) spends
/// ~36 of them — so a per-test root that spells its tag and a nanosecond stamp
/// stops fitting the moment a socket file name is appended. Measured, before
/// this seam: 108, 110, 114 and 119 bytes in `aoide-server`'s `a2a` fixtures,
/// all refused by name by the native binder.
///
/// The tag is HASHED rather than spelled: it stays identifiable in a directory
/// dump and stops costing bytes. ONE seam — `a2a`, `shellbridge` and `conduct`'s
/// socket fixtures all ask this instead of composing a path of their own, so the
/// budget is answered in one place.
pub fn short_tmp(tag: &str) -> PathBuf {
    // FNV-1a over the tag, then the FULL 64 bits folded with the pid: the hash
    // keeps the name short (a directory dump still identifies it) and the pid
    // keeps two RUNS apart even when their tags repeat — a stale socket file in
    // a path two runs shared is an `EADDRINUSE` for the next `bind`, which is
    // the failure this shape prevents. Length: `aoide-<16 hex>-<pid%100000>` is
    // 32 chars, ~70 bytes under this host's ~36-byte temp prefix — inside the
    // 107-byte `sun_path` cap a socket needs.
    let hash: u64 = tag.bytes().fold(0xcbf2_9ce4_8422_2325u64, |acc, b| {
        (acc ^ b as u64).wrapping_mul(0x0000_0100_0000_01b3)
    });
    let mut dir = std::env::temp_dir();
    dir.push(format!("aoide-{hash:016x}-{}", std::process::id() % 100_000));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The deadline every delivery fixture in this workspace shares: long enough
/// for the waits a legitimate delivery itself makes first — `wait_ready`'s 2s
/// quiescence window for a hookless target plus `SUBMIT_KEYSTROKE_DELAY`'s
/// 300ms write gap is the longest any of them reaches — and short enough that
/// a delivery the door WITHHELD fails its test instead of parking it.
///
/// **Why a fixture needs this at all.** A delivery fixture binds a listener at
/// the target's own socket and spawns a thread to read what arrives there; the
/// code under test is supposed to dial it. When it does not (a refused arm, an
/// authorisation the fixture no longer holds, a dial that landed elsewhere),
/// an unbounded `accept`/`read_to_end` waits forever — and since these tests
/// hold their crate's `env_lock`, the parked thread takes every test queued
/// behind it down with it: a suite that hangs instead of reporting one
/// failure. `accept_one`/`read_delivery`/`expect_delivery` are that wait,
/// bounded, in ONE place.
pub const DELIVERY_BUDGET: Duration = Duration::from_secs(10);

/// Accept ONE connection by [`DELIVERY_BUDGET`], panicking rather than blocking
/// forever when none arrives. `what` is the fixture's own expectation in its
/// own words ("the parent's steer is DELIVERED, not held pending"), so the
/// failure says WHICH arm broke rather than only that a socket stayed quiet.
///
/// The listener is armed non-blocking and polled, so the wait is a deadline and
/// never a block; the accepted stream carries the same budget as its READ
/// timeout, so the other half — a peer that connects and then goes silent — is
/// bounded too. That read budget is the guarantee on either host: a non-blocking
/// accepted socket would ignore `SO_RCVTIMEO`, so `win_unix::accept` clears the
/// inherited flag best-effort and its `recv_into` honours the budget itself
/// (the socket seam's own contract, `crates/protocol/src/win_unix.rs`).
pub fn accept_one(listener: &UnixListener, what: &str) -> UnixStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + DELIVERY_BUDGET;
    loop {
        match listener.accept() {
            Ok((conn, _)) => {
                conn.set_read_timeout(Some(DELIVERY_BUDGET)).unwrap();
                return conn;
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted
                ) =>
            {
                if Instant::now() >= deadline {
                    panic!("{what}\n  — nothing connected to the fixture's socket within {DELIVERY_BUDGET:?}");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("{what}\n  — the fixture's listener failed: {e}"),
        }
    }
}

/// Everything a DELIVERED peer sends on one connection, up to its close —
/// [`accept_one`]'s other half, and bounded the same way. A peer that connects,
/// writes and then holds the connection open fails by name instead of reading
/// forever.
pub fn read_delivery(conn: &mut UnixStream, what: &str) -> Vec<u8> {
    let deadline = Instant::now() + DELIVERY_BUDGET;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            panic!(
                "{what}\n  — the deliverer connected, sent {} byte(s), and held the connection \
                 open for {DELIVERY_BUDGET:?} without closing",
                buf.len()
            );
        }
        conn.set_read_timeout(Some(left)).unwrap();
        match conn.read(&mut chunk) {
            Ok(0) => return buf,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => continue,
            Err(e) => panic!("{what}\n  — reading the delivered payload failed: {e}"),
        }
    }
}

/// The whole rig, in the shape most fixtures want it: wait on its own thread
/// (the code under test dials while the test body drives it) for the delivery,
/// and hand back every byte. Bounded on both halves by [`accept_one`] /
/// [`read_delivery`]; the caller asserts on the payload, so a withheld delivery
/// is a failed assertion carrying `what` instead of a suite that never returns.
pub fn expect_delivery(listener: UnixListener, what: &'static str) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut conn = accept_one(&listener, what);
        read_delivery(&mut conn, what)
    })
}

/// The stack [`on_small_stack`] runs on. A walk that recurses once per level
/// of a depth the other side chose overflows it within a few thousand levels,
/// and a stack overflow aborts the whole test binary — loudly, by name. A
/// normal thread (2 MiB) or the main thread (8 MiB) would let a chain tens of
/// thousands deep pass silently, which is exactly the fixture that proves
/// nothing.
pub const SMALL_STACK: usize = 256 * 1024;

/// Run `work` to completion on a thread whose stack is [`SMALL_STACK`], and
/// hand back its result (a panic inside it resumes here, so an assertion
/// failure is still that test's failure). The fixture for "this walk must not
/// recurse per level": give it a document a hundred thousand levels deep.
pub fn on_small_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .name("small-stack".into())
        .stack_size(SMALL_STACK)
        .spawn(work)
        .expect("spawn the small-stack thread")
        .join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// `work`'s result and wall time, or `None` when it has not finished by
/// `limit`. The work runs on a thread of its own, so a super-linear one fails
/// the test at the limit instead of holding the suite for minutes (it is left
/// running and dies with the test binary). A panic inside `work` resumes here.
pub fn finishes_within<T: Send + 'static>(
    limit: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<(Duration, T)> {
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let started = Instant::now();
        let out = work();
        let _ = tx.send((started.elapsed(), out));
    });
    match rx.recv_timeout(limit) {
        Ok(done) => Some(done),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => None,
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => match worker.join() {
            Err(panic) => std::panic::resume_unwind(panic),
            Ok(()) => unreachable!("a finished worker has sent its result"),
        },
    }
}

/// The built `aoide` binary beside the calling test's own executable — the
/// REAL program a fixture needs when it must run a hook/dispatcher as its own
/// process (its own argv, env, stdin and its own daemon probe), not an
/// in-process imitation. `cargo test -p <crate>` builds the workspace's bins,
/// so the sibling is there whenever a test binary is.
///
/// ONE seam, because the name is a HOST fact: `std::env::consts::EXE_SUFFIX`
/// (`.exe` on native Windows, empty on Unix). Two copies of this helper spelled
/// a bare `aoide`, so on Windows each reported a binary it had just built as
/// missing — and on the cli's door test that meant the fixture never reached the
/// door at all, so the door's peer-pid stamping was silently unexercised.
pub fn built_aoide_bin() -> PathBuf {
    let test_exe = std::env::current_exe().expect("current_exe resolves under cargo test");
    let profile_dir = test_exe
        .parent() // .../target/<profile>/deps
        .and_then(|p| p.parent()) // .../target/<profile>
        .expect("test exe has a target/<profile>/deps parent");
    let bin = profile_dir.join(format!("aoide{}", std::env::consts::EXE_SUFFIX));
    assert!(
        bin.exists(),
        "expected a pre-built `aoide` binary at {bin:?} — run `cargo build --bin aoide` first"
    );
    // **A STALE binary is worse than a missing one**, and it cost a whole round
    // of native Windows diagnostics: the tests that spawn this binary kept an
    // older executable from a tree that did not contain the code under test, so
    // every measurement described that older tree. The check is a WARNING on
    // the native Windows arm alone (where the tests spawn a separately-built
    // `aoide`), and it compares the executable against the newest SOURCE file
    // under `crates/`, never against the test binary: `cargo test` rebuilds the
    // test while the executable is built by a separate command, so "the test is
    // newer" is the normal state of a correct tree — measured, and a warning
    // that fires there is a warning nobody reads. A source file newer than the
    // executable is the one case that cannot cry wolf: it means the code moved
    // on and this binary did not.
    #[cfg(windows)]
    {
        static NEWEST_SOURCE: std::sync::OnceLock<Option<std::time::SystemTime>> =
            std::sync::OnceLock::new();
        let newest_source = *NEWEST_SOURCE.get_or_init(|| {
            let crates_dir = profile_dir.parent()?.parent()?.join("crates");
            let mut newest: Option<std::time::SystemTime> = None;
            let mut stack = vec![crates_dir];
            while let Some(dir) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&dir) else { continue };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path.extension().is_some_and(|e| e == "rs") {
                        if let Ok(t) = entry.metadata().and_then(|m| m.modified()) {
                            newest = Some(newest.map_or(t, |n| n.max(t)));
                        }
                    }
                }
            }
            newest
        });
        if let (Ok(bin_t), Some(source_t)) = (bin.metadata().and_then(|m| m.modified()), newest_source)
        {
            if bin_t < source_t {
                eprintln!(
                    "warning: the pre-built `aoide` binary at {bin:?} is OLDER than the newest \
                     source under crates/ — it may predate the code under test, and a test that \
                     spawns it would measure an older tree; run `cargo build --bin aoide` again"
                );
            }
        }
    }
    bin
}

/// Run `git -C <dir> <args>` with a fixed identity, for a scratch repository a
/// test builds (`git init`, `add`, `commit`). Panics when git refuses, so a
/// test's setup never passes on a repository that was not made.
pub fn git_in(dir: &std::path::Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=aoide-test", "-c", "user.email=test@aoide.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .status()
        .expect("git on PATH")
        .success();
    assert!(ok, "git {args:?} in {} failed", dir.display());
}

pub fn unique_tmp(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "aoide-dispatch-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The value of header `name` in a raw HTTP message's head (a request or a
/// response): the first blank line ends the head, the first line is the
/// request or status line and is skipped, and the name matches
/// case-insensitively. The value is trimmed; `None` when there is no such line.
pub fn raw_header(raw: &str, name: &str) -> Option<String> {
    let head = raw.split_once("\r\n\r\n").map_or(raw, |(head, _)| head);
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim().to_string())
    })
}

pub fn inv(path: &[&str], args: &[&str]) -> Invocation {
    Invocation {
        path: path.iter().map(|s| s.to_string()).collect(),
        args: args.iter().map(|s| s.to_string()).collect(),
        flags: BTreeMap::new(),
        door: Door::Cli,
    }
}

// Restore env vars on drop so a panicking assertion never leaks state.
pub struct EnvSaver {
    keys: Vec<(&'static str, Option<String>)>,
}
impl EnvSaver {
    pub fn capture(keys: &[&'static str]) -> Self {
        EnvSaver {
            keys: keys.iter().map(|k| (*k, std::env::var(k).ok())).collect(),
        }
    }
}
impl Drop for EnvSaver {
    fn drop(&mut self) {
        for (k, v) in &self.keys {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }
}

/// Point `AOIDE_ROOT` at a fresh, isolated temp directory for the returned
/// guard's lifetime, and clear `AOIDE_STATE_DIR`/`AOIDE_STAGE_DIR` so
/// neither can leave a stale absolute override pointing somewhere else —
/// restored on drop via [`EnvSaver`]. `aoide-storage`'s own `env_lock()`
/// floors nothing (unlike `aoide-conduct`'s and `aoide-server`'s own), so a
/// storage test that sets only `AOIDE_STATE_DIR` still resolves
/// `stage_dir()` — and so `try_stage_lock`'s `.stage.lock` — against the
/// REAL, unset root; that gap is exactly how a fixture once wrote real rows
/// into the live inbox. Pointing `AOIDE_ROOT` itself moves both
/// `state_dir()` and `stage_dir()` off the real root at once. Every
/// `aoide-storage` test that touches `mail` must use this, never
/// `AOIDE_STATE_DIR` alone.
///
/// Also pins `AOIDE_DAEMON_SOCKET` at a path inside the isolated root that
/// nothing ever binds (P-M5a-2: `mail send`'s self branch now forwards
/// `mail ring` through `aoide_client::daemon::daemon_dispatch`, which
/// resolves the daemon socket from `AOIDE_DAEMON_SOCKET` or else
/// `$XDG_RUNTIME_DIR/aoide/aoided.sock` — falling all the way back to the
/// hardcoded `/run/user/1000` when even `XDG_RUNTIME_DIR` is unset). Without
/// this, a `mail send --to self/...` test on a machine with a REAL resident
/// `aoided` dials that live daemon for real and can inject a real nudge
/// line into a real conducted session. `daemon_dispatch` treats a dead
/// socket as an ordinary, silent "no daemon" — exactly the outcome these
/// tests want — so pointing it at a guaranteed-dead path costs nothing.
///
/// Caller must already hold [`env_lock`] (the same convention every other
/// env-touching test here follows — acquired once, at the top of the test,
/// before constructing any guard). Owns the environment only, not the
/// directory: the caller still removes it at the end of the test, the same
/// way every `unique_tmp` caller already does.
pub fn isolated_mail_root(tag: &str) -> (EnvSaver, PathBuf) {
    let env = EnvSaver::capture(&["AOIDE_ROOT", "STRILLIX_ROOT", "AOIDE_STATE_DIR", "AOIDE_STAGE_DIR", "AOIDE_DAEMON_SOCKET"]);
    let root = unique_tmp(tag);
    std::env::set_var("AOIDE_ROOT", &root);
    std::env::set_var("STRILLIX_ROOT", &root);
    std::env::remove_var("AOIDE_STATE_DIR");
    std::env::remove_var("AOIDE_STAGE_DIR");
    std::env::set_var("AOIDE_DAEMON_SOCKET", root.join("no-daemon.sock"));
    (env, root)
}

pub const VALID_NOTES: &str = r##"{ "schemaVersion":"0",
    "palette": {"bg":"#0b1021","fg":"#c8d3f5","accent":"#82aaff","urgent":"#ff757f"} }"##;

// Carries a `window` block (border colours), so `hyprctl` keyword-batch
// construction has something to resolve — VALID_NOTES deliberately does
// not, to exercise the "empty batch" path elsewhere.
pub const NOTES_WITH_WINDOW: &str = r##"{ "schemaVersion":"0",
    "palette": {"bg":"#0b1021","fg":"#c8d3f5","accent":"#82aaff","urgent":"#ff757f"},
    "window": {"border":"#82aaff","borderInactive":"#0b1021"} }"##;

// A rice with palette + window + a full geometry block, for `rice compose`
// tests that need to assert every tier round-trips.
pub const NOTES_WITH_GEOMETRY: &str = r##"{ "schemaVersion":"0",
    "palette": {"bg":"#0b1021","fg":"#c8d3f5","accent":"#82aaff","urgent":"#ff757f"},
    "window": {"border":"#82aaff","borderInactive":"#0b1021"},
    "geometry": {"gapsOut":10,"gapsIn":4,"borderSize":3,"rounding":6,
                 "blurEnabled":false,"blurSize":5,"blurPasses":2} }"##;

// A hostile palette value carrying live Nix interpolation syntax — proves
// `nix_scalar` neutralizes `${…}` rather than letting it round-trip into
// `rice.nix` as a real interpolation (a real injection: a value like
// `"${builtins.readFile /etc/hostname}"` would otherwise EVALUATE).
pub const NOTES_WITH_INTERPOLATION: &str = r##"{ "schemaVersion":"0",
    "palette": {"bg":"${builtins.currentTime}","fg":"#c8d3f5",
                 "accent":"#82aaff","urgent":"#ff757f"} }"##;

/// A suite-wide lock serialising every test that mutates process-global env
/// (`AOIDE_STAGE_DIR`, `PATH`, …). `std::env::set_var` is
/// process-global, so env-touching tests across modules must share ONE mutex or
/// they race each other under the multithreaded test harness.
///
/// Acquisition is poison-tolerant everywhere, by convention:
/// `.lock().unwrap_or_else(|e| e.into_inner())`, never a bare `.unwrap()`.
/// A test that panics while holding this guard has already failed ITSELF, and
/// every critical section sets the env it needs at entry — there is no
/// predecessor state to trust, so the poison carries no information. A bare
/// `.unwrap()` here converts one real failure into a suite-wide cascade that
/// buries the root under dozens of `PoisonError` panics (worst on a loaded
/// builder, where a timing-sensitive test is likeliest to trip first). The
/// same rule applies to every crate-local test guard shaped like this one.
pub fn env_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    &LOCK
}

/// Install a FAKE `hyprctl` on `PATH` (the established shim technique — this
/// crate's own `crates/AGENTS.md` note on replacing a real binary) so the
/// compositor-facing half can be driven end-to-end with no compositor at all.
/// Both `clients -j` and `activeworkspace -j` answer: `clients` prints ONE
/// client whose `pid` is `$AOIDE_TEST_CLIENT_PID` (default: the TEST process's
/// own `$PPID`, where the pid ancestry walk starts), whose `class` is
/// `$AOIDE_TEST_CLIENT_CLASS` (default `kitty`), and whose workspace id comes
/// from `$AOIDE_TEST_WS`. `activeworkspace` prints that same id in the
/// focused-workspace shape. A test that needs a client owned by some other
/// process (an app window its harnesses run under) sets the two client vars.
/// Returns the env guard (restores `PATH`/`HYPRLAND_INSTANCE_SIGNATURE`/
/// `AOIDE_TEST_WS`/`AOIDE_TEST_CLIENT_PID`/`AOIDE_TEST_CLIENT_CLASS` on drop)
/// and the shim directory, for the caller to remove.
///
/// GATED on Unix, with its reason: the shim is a `#!/bin/sh` SCRIPT made
/// executable by a mode, and `CreateProcess` understands neither a shebang nor
/// an extension-less name — the identical gate (and the identical reason) the
/// client crate's `curl`/`zenity` shim groups carry. Its subject is a
/// compositor that does not exist on native Windows (the adapter's own
/// "no adapter" path is what runs there), so a Windows twin would have to
/// build a real `.exe` to fake a program the host never launches. The callers
/// below are the `activeworkspace` and dispatch seams; the client list has its
/// own fake, `fake_hypr_socket`, in conduct's `graph::testutil`.
#[cfg(unix)]
pub fn fake_hyprctl(tag: &str) -> (EnvSaver, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("aoide-fake-hypr-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let shim = dir.join("hyprctl");
    std::fs::write(
        &shim,
        r#"#!/bin/sh
ws=${AOIDE_TEST_WS:-3}
cpid=${AOIDE_TEST_CLIENT_PID:-$PPID}
cclass=${AOIDE_TEST_CLIENT_CLASS:-kitty}
client="{\"address\":\"0xAA\",\"class\":\"$cclass\",\"title\":\"t\",\"pid\":$cpid,\"workspace\":{\"id\":$ws,\"name\":\"ws\"},\"mapped\":true,\"at\":[0,0],\"size\":[1,1]}"
if [ "$1" = "activeworkspace" ]; then
  echo "{\"id\":$ws,\"name\":\"ws\"}"
else
  echo "[$client]"
fi
"#,
    )
    .unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    let env = EnvSaver::capture(&[
        "PATH",
        "HYPRLAND_INSTANCE_SIGNATURE",
        "AOIDE_TEST_WS",
        "AOIDE_TEST_CLIENT_PID",
        "AOIDE_TEST_CLIENT_CLASS",
    ]);
    let path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{path}", dir.display()));
    std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "test");
    std::env::set_var("AOIDE_TEST_WS", "3");
    (env, dir)
}

/// An argv: each item as an owned `String`, for the tests that build one.
pub fn sv(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}
