//! Integration proof for P-D6 — graph residency (`docs/architecture/
//! AOIDED.md`'s "L4"): the stage-file bytes a routed session-write command
//! produces through a REAL resident daemon match the direct-path bytes for
//! the identical input, and `session reap` reaps a dead session over the
//! socket exactly as it does directly.
//!
//! Companion to `daemon_dispatch_door.rs` (P-D4's door-POLICY proof: same
//! registry, same gates, over the daemon door) — this file proves P-D6's
//! own claim instead: routing changes TRANSPORT only, never the write
//! itself. Every dispatch below goes through the ordinary top-level
//! `aoide::dispatch::dispatch` (`Door::Cli`) exactly as a real CLI
//! invocation would; whether it lands on the daemon or the direct fallback
//! is decided entirely by `$AOIDE_DAEMON_SOCKET` pointing at a live socket
//! or a dead path — no test here calls `aoide_client::daemon::daemon_dispatch`
//! directly, so this is the SAME code path a real terminal takes.

use aoide::dispatch::{dispatch, registry};
use aoide_protocol::{Door, Invocation};
use aoide_server::daemon::{run_loop, serve_daemon};
use std::collections::BTreeMap;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Mirrors `aoide_conduct::graph::testutil::unique_stage`'s own SUN_LEN
/// lesson (#75, restated in that module's doc): short, hashed, counter-based
/// — never the raw tag text or a full nanosecond timestamp — so a socket
/// path built under it never risks `AF_UNIX`'s 108-byte cap even when
/// `$TMPDIR` is already a long `nix develop` sandbox path.
static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
fn unique_dir(tag: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    tag.hash(&mut hasher);
    let tag_hash = (hasher.finish() as u32) & 0xffff;
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut dir = std::env::temp_dir();
    dir.push(format!("avpd6-{:x}-{:x}-{:x}", std::process::id(), tag_hash, seq));
    dir
}

fn connect_retrying(socket_path: &Path) -> UnixStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match UnixStream::connect(socket_path) {
            Ok(s) => return s,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            Err(e) => panic!("could not connect to {socket_path:?} in time: {e}"),
        }
    }
}

/// Start a REAL `serve_daemon` (the fully-assembled `registry()`/`dispatch`
/// this crate's own `bin/aoided.rs` injects, not a fixture) on a fresh
/// short-path socket. Blocks until it is actually accepting.
fn start_daemon(tag: &str) -> PathBuf {
    let socket_path = unique_dir(tag).with_extension("sock");
    let events_path = unique_dir(&format!("{tag}-events")).with_extension("jsonl");
    let sp = socket_path.clone();
    std::thread::spawn(move || {
        let _ = serve_daemon(&sp, &events_path, registry(), dispatch);
    });
    let probe = connect_retrying(&socket_path);
    drop(probe);
    socket_path
}

// ── The resident-`run_loop` fixture: its OWN PROCESS, never a bare thread ───
//
// `run_loop`'s ~1s tick loop has no shutdown API, so an in-process fixture
// thread could never be stopped: it outlived its test and kept re-reading the
// process-global `$AOIDE_STAGE_DIR` on every tick, so after that test released
// `aoide_test_support::env_lock`/`EnvSaver` the leaked loop resolved — and
// wrote — whichever stage dir a LATER test had installed, outside that test's
// lock (the observed `graph.json: No such file or directory` write failure).
// A child process pins its env at spawn and dies on command, which is the fix.

/// Name of the single test this file re-execs itself to run as its resident
/// `run_loop`, and the marker that selects it — `#[ignore]`'d so an ordinary
/// suite run never enters the loop in its own harness. The child entry no-ops
/// unless [`CHILD_MARKER_ENV`] is set, so only the parent's own spawn reaches it.
const RUN_LOOP_CHILD_TEST: &str = "run_loop_child_process";
const CHILD_MARKER_ENV: &str = "AOIDE_P_D6_RUN_LOOP_CHILD";

/// The child's own `run_loop` arguments, resolved from the env the parent
/// fixed at spawn (never from the parent's later mutations).
const CHILD_SOCKET_ENV: &str = "AOIDE_P_D6_CHILD_SOCKET";
const CHILD_EVENTS_ENV: &str = "AOIDE_P_D6_CHILD_EVENTS";
const CHILD_LOG_ENV: &str = "AOIDE_P_D6_CHILD_LOG";
const CHILD_PID_FILE_ENV: &str = "AOIDE_P_D6_CHILD_PID_FILE";

/// The resident-`run_loop` fixture, owned: re-execs THIS test binary as the
/// resident daemon (`run_loop(.., registry(), dispatch)` — the pair
/// `bin/aoided.rs` injects), waits until it is accepting, and kills + waits on
/// drop, before the caller's own cleanup. Callers MUST already have
/// `$AOIDE_STAGE_DIR` set: it is handed to the child and left in this
/// process's env, since the routed client half reads it here.
struct RunLoopGuard {
    child: std::process::Child,
    socket_path: PathBuf,
    events_path: PathBuf,
}

impl RunLoopGuard {
    fn start(tag: &str, stage_dir: &Path) -> Self {
        let socket_path = unique_dir(tag).with_extension("sock");
        Self::spawn(tag, stage_dir, &socket_path, None)
    }

    /// [`start`](Self::start) with the socket path and a pid file named by the
    /// caller — used by the readiness-panic regression, which occupies the
    /// socket path to make the child fail to bind and needs the child's own
    /// pid to assert it was reaped.
    fn start_with_pid_file(tag: &str, stage_dir: &Path, socket_path: &Path, pid_file: &Path) -> Self {
        Self::spawn(tag, stage_dir, socket_path, Some(pid_file))
    }

    fn spawn(tag: &str, stage_dir: &Path, socket_path: &Path, pid_file: Option<&Path>) -> Self {
        let socket_path = socket_path.to_path_buf();
        let events_path = unique_dir(&format!("{tag}-events")).with_extension("jsonl");
        let log_path = unique_dir(&format!("{tag}-log"));
        std::fs::create_dir_all(stage_dir).expect("the fixture must stage its own dir");

        // The routed CLIENT half of a test reads this process's env.
        std::env::set_var("AOIDE_DAEMON_SOCKET", &socket_path);

        let mut cmd = Command::new(std::env::current_exe().expect("the test binary's own path"));
        cmd.arg("--exact")
            .arg("--include-ignored")
            .arg(RUN_LOOP_CHILD_TEST)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .env(CHILD_MARKER_ENV, "1")
            .env(CHILD_SOCKET_ENV, &socket_path)
            .env(CHILD_EVENTS_ENV, &events_path)
            .env(CHILD_LOG_ENV, &log_path)
            .env("AOIDE_STAGE_DIR", stage_dir)
            .env("AOIDE_DAEMON_SOCKET", &socket_path);
        if let Some(pid_file) = pid_file {
            cmd.env(CHILD_PID_FILE_ENV, pid_file);
        }
        let child = cmd.spawn().expect("spawning the resident run_loop child");

        // Own the child BEFORE the readiness wait, so a child that never binds
        // is killed + reaped by the guard on the panic path, not leaked.
        let mut guard = RunLoopGuard { child, socket_path, events_path };
        match guard.wait_accepting() {
            Ok(()) => guard,
            Err(e) => panic!("the resident run_loop child never became accepting: {e}"),
        }
    }

    /// Wait (bounded, same shape as [`connect_retrying`]) until the child's
    /// socket accepts a connection.
    fn wait_accepting(&mut self) -> Result<(), std::io::Error> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match UnixStream::connect(&self.socket_path) {
                Ok(_) => return Ok(()),
                Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
                Err(e) => return Err(e),
            }
        }
    }

    /// Is the resident child still alive? Asserted around the routed calls so
    /// an isolation failure can never let a test pass against the direct
    /// fallback while believing it exercised the daemon.
    fn child_is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for RunLoopGuard {
    fn drop(&mut self) {
        // Kill + WAIT: the child must be gone, and reaped, before the caller
        // removes the files it was writing.
        let _ = self.child.kill();
        let _ = self.child.wait();
        std::fs::remove_file(&self.socket_path).ok();
        std::fs::remove_file(&self.events_path).ok();
    }
}

/// The child entry described on [`RUN_LOOP_CHILD_TEST`]: runs the SAME
/// resident `run_loop` a real `aoided` runs (`registry()`/`dispatch` — this
/// crate's own bin injects the identical pair), against the paths/env the
/// parent fixed at spawn. No-op (and so safe to list as an ordinary ignored
/// test) unless [`CHILD_MARKER_ENV`] is set.
#[test]
#[ignore = "fixture entry: the parent re-execs the test binary with --exact to run the resident run_loop; it never returns"]
fn run_loop_child_process() {
    if std::env::var_os(CHILD_MARKER_ENV).is_none() {
        return;
    }
    // Record our own pid before entering the loop (a test may occupy the
    // socket path to prove the readiness-panic path still reaps us).
    if let Some(pid_file) = std::env::var_os(CHILD_PID_FILE_ENV) {
        let _ = std::fs::write(pid_file, format!("{}\n", std::process::id()));
        // Never bind: stay a live, accepting-never child so the parent's
        // readiness wait times out and its panic path is exercised.
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    }
    let socket = PathBuf::from(std::env::var(CHILD_SOCKET_ENV).expect("child socket path"));
    let events = PathBuf::from(std::env::var(CHILD_EVENTS_ENV).expect("child events path"));
    let log = PathBuf::from(std::env::var(CHILD_LOG_ENV).expect("child log path"));
    let _ = run_loop(socket, events, log, registry(), dispatch);
}

/// Read every JSON line currently in the daemon's own events feed whose
/// `kind` is `hand-edit` and whose `payload.file` is `file_name` — used by
/// the task #92 tests below to assert presence/absence without caring about
/// any other line the feed carries.
fn hand_edit_events_for(events_path: &Path, file_name: &str) -> Vec<serde_json::Value> {
    std::fs::read_to_string(events_path)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|rec| rec["kind"] == "hand-edit" && rec["payload"]["file"] == file_name)
        .collect()
}

/// Block until the wall clock crosses a fresh second boundary. Every stage
/// timestamp in this tree is second-precision
/// (`aoide_storage::time::now_iso_utc`, no test clock-injection seam exists
/// today and this phase doesn't add one) — calling this once, then issuing
/// both dispatches back to back immediately after, keeps them inside the
/// SAME second (a local Unix-socket round trip plus one small JSON file
/// write finishes in low single-digit milliseconds, nowhere near the ~1s of
/// headroom this buys), so a literal byte comparison between their outputs
/// can never flake on a second-boundary crossing.
fn wait_for_fresh_second() {
    let start = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    while SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() == start {
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn session_start_inv(id: &str) -> Invocation {
    let mut flags = BTreeMap::new();
    flags.insert("id".to_string(), id.to_string());
    flags.insert("agent".to_string(), "claude".to_string());
    flags.insert("cwd".to_string(), "/tmp/p-d6-cwd".to_string());
    Invocation { path: vec!["session".to_string(), "start".to_string()], args: vec![], flags, door: Door::Cli }
}

/// The phase's own first test requirement: "A routed command round-trips
/// through a test daemon and the projection file matches the direct-path
/// bytes exactly." `session start` is dispatched twice, identically
/// (same id/agent/cwd), once with `$AOIDE_DAEMON_SOCKET` pointed at nothing
/// (the direct fallback runs) and once pointed at a REAL, freshly-started
/// daemon (the routed path runs, and the WRITE itself happens on the
/// daemon's own accept thread — `aoide_conduct::graph::session_store::
/// session_start`'s `daemon_dispatch` call blocks this test's own thread on
/// the reply, so by the time it returns the daemon has already finished its
/// `write_stage` call). `sessions.json`'s bytes are compared in full (minus
/// the one field `do_session_start` mints randomly on every call regardless
/// of routing, `petname` — normalized to a fixed placeholder in both files
/// before comparing, see the comment at the comparison site) — not spot
/// fields — proving the daemon's projection write really is the exact same
/// write the direct path performs for the exact same input.
#[test]
fn routed_session_start_produces_byte_identical_sessions_json_to_the_direct_path() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _saver = aoide_test_support::EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_DAEMON_SOCKET", "XDG_RUNTIME_DIR"]);

    let dir_direct = unique_dir("direct-stage");
    let dir_daemon = unique_dir("daemon-stage");
    std::fs::create_dir_all(&dir_direct).unwrap();
    std::fs::create_dir_all(&dir_daemon).unwrap();

    // Start the daemon EARLY (its bind/accept-loop startup is not part of
    // the timing window below) but don't route to it yet.
    let daemon_socket = start_daemon("byte-identity");
    let dead_socket = unique_dir("byte-identity-dead").with_extension("sock");

    wait_for_fresh_second();

    // Pass 1: direct fallback — no daemon reachable at this socket path.
    std::env::set_var("AOIDE_STAGE_DIR", &dir_direct);
    std::env::set_var("AOIDE_DAEMON_SOCKET", &dead_socket);
    let direct_outcome = dispatch(&session_start_inv("pd6-byte-test"));
    assert_eq!(direct_outcome.status, aoide_protocol::output::Status::Ok, "{direct_outcome:?}");

    // Pass 2: routed — the SAME invocation, now with a live daemon to reach.
    std::env::set_var("AOIDE_STAGE_DIR", &dir_daemon);
    std::env::set_var("AOIDE_DAEMON_SOCKET", &daemon_socket);
    let routed_outcome = dispatch(&session_start_inv("pd6-byte-test"));
    assert_eq!(routed_outcome.status, aoide_protocol::output::Status::Ok, "{routed_outcome:?}");

    // `do_session_start` mints a random whimsical `petname` (no `--petname`
    // flag exists to pin it — `session start`'s flag set is
    // id/agent/cwd/window/parent only) whenever a session has none yet, so
    // it is the ONE field expected to differ between ANY two `start` calls
    // for the same id, daemon routing or not — normalized to a fixed
    // placeholder in BOTH documents before the comparison below, so the
    // comparison still covers every other byte in the file exactly
    // (`schemaVersion`, `sessionId`, `agent`, `windowAddress`, `cwd`,
    // `state`, `startedAt`, `kind` — the whole shape `do_session_start`
    // writes) rather than special-casing the file down to spot fields.
    let normalize = |bytes: &[u8]| -> Vec<u8> {
        let mut v: serde_json::Value = serde_json::from_slice(bytes).expect("sessions.json must be valid JSON");
        if let Some(sessions) = v.get_mut("sessions").and_then(|s| s.as_array_mut()) {
            for s in sessions {
                if let Some(obj) = s.as_object_mut() {
                    obj.insert("petname".to_string(), serde_json::json!("<normalized>"));
                }
            }
        }
        serde_json::to_vec_pretty(&v).unwrap()
    };

    let direct_bytes = std::fs::read(dir_direct.join("sessions.json")).unwrap();
    let daemon_bytes = std::fs::read(dir_daemon.join("sessions.json")).unwrap();
    let direct_norm = normalize(&direct_bytes);
    let daemon_norm = normalize(&daemon_bytes);

    assert_eq!(
        direct_norm, daemon_norm,
        "direct-path sessions.json (petname normalized):\n{}\n\nrouted (through the daemon) sessions.json (petname normalized):\n{}",
        String::from_utf8_lossy(&direct_norm),
        String::from_utf8_lossy(&daemon_norm),
    );

    // The routed Outcome itself carries no daemon-specific marker either —
    // same status/command/message shape as the direct one.
    assert_eq!(routed_outcome.command, direct_outcome.command);

    std::fs::remove_dir_all(&dir_direct).ok();
    std::fs::remove_dir_all(&dir_daemon).ok();
    std::fs::remove_file(&daemon_socket).ok();
}

/// The phase's fourth test requirement: "Reap over the socket reaps."
///
/// The fixture is a session with a dead `pid` — `is_session_dead`'s
/// `pid_signal` (`reap.rs`: `matches!(rec.pid, Some(p) if !proc_exists(p))`)
/// fires unconditionally off a real `/proc/<pid>` check, independent of
/// state/staleness timers or `hyprctl`, which is what makes this
/// deterministic without a live compositor or a wall-clock wait. A `done`
/// -state record would NOT work here: `reap_inner`'s own comment is explicit
/// — "an already-`done` session is prune's job, not a reap" — its sweep
/// filters `state != "done"` before ever consulting `is_session_dead`.
///
/// Dispatches `session reap` at a live daemon exactly as a real terminal
/// would. The daemon's own `serve_daemon` runs the SAME `graph::reap::
/// reap_and_announce` handler `session reap` always runs (no daemon-specific
/// reap logic exists — this phase's own "no logic forks" rule) — the write
/// happens on the daemon's accept thread, and by the time `dispatch` returns
/// here the socket reply already carries its result, so reading
/// `sessions.json` immediately after is race-free.
#[test]
fn graph_reap_over_the_socket_reaps_a_dead_pid_session() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _saver = aoide_test_support::EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_DAEMON_SOCKET", "XDG_RUNTIME_DIR", "PATH"]);

    let dir = unique_dir("reap-stage");
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("AOIDE_STAGE_DIR", &dir);
    std::env::set_var("AOIDE_DAEMON_SOCKET", unique_dir("reap-dead").with_extension("sock"));

    let sf = aoide_storage::records::SessionsFile {
        schema_version: "0".to_string(),
        sessions: vec![aoide_storage::records::SessionRecord {
            session_id: "pd6-reap-target".to_string(),
            agent: "claude".to_string(),
            state: "working".to_string(),
            started_at: aoide_storage::time::now_iso_utc(),
            // No real process anywhere near this pid — `/proc/999999999`
            // does not exist on any Linux box, so `pid_signal` fires with
            // no staleness wait and no window/hyprctl dependency at all.
            pid: Some(999_999_999),
            ..Default::default()
        }],
    };
    aoide_storage::stage::write_stage(&aoide_storage::stage::sessions_path(), &sf).unwrap();

    let before = std::fs::read_to_string(dir.join("sessions.json")).unwrap();
    assert!(before.contains("pd6-reap-target"), "fixture must be present before reap: {before}");

    // Now route `session reap` at a REAL daemon. `session reap`'s registered
    // handler is `reap_and_announce`, not the toast-free `reap` every
    // in-crate unit test calls (`reap.rs`'s own doc: "`reap` itself stays
    // toast-free, so every in-crate caller... gets the sweep without
    // spawning notifiers") — going through the real dispatch path here is
    // the whole point (proving the SOCKET route, not just the sweep logic),
    // so a session this sweep actually changes would otherwise fire a REAL
    // `notify-send` toast on this box (`outcome.changed` non-empty). `PATH`
    // is blanked for just this one call so `Command::new("notify-send")`
    // can't resolve — `announce_reap` falls to its own already-covered
    // eprintln branch instead of touching the live desktop; the daemon
    // shares this process's env, so the blank PATH applies to the write
    // that runs on its accept thread too. `PATH` is restored (via the
    // `EnvSaver` above) before the daemon is even asked anything else.
    let daemon_socket = start_daemon("reap");
    std::env::set_var("AOIDE_DAEMON_SOCKET", &daemon_socket);
    std::env::set_var("PATH", "/nonexistent-aoide-test-empty-bin-dir");

    let reap_out = dispatch(&Invocation { path: vec!["session".to_string(), "reap".to_string()], args: vec![], flags: BTreeMap::new(), door: Door::Cli });
    assert_eq!(reap_out.status, aoide_protocol::output::Status::Ok, "{reap_out:?}");

    let after = std::fs::read_to_string(dir.join("sessions.json")).unwrap();
    assert!(
        !after.contains("pd6-reap-target"),
        "a dead-pid session must be swept by a reap routed over the socket: {after}"
    );

    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_file(&daemon_socket).ok();
}

/// Task #92 regression: `session start`, ROUTED through a resident
/// daemon's `{"op":"dispatch"}`, must never make the #69 hand-edit watcher
/// (`aoide-server`'s `producers::HandEditWatcher`, ticked by `run_loop`)
/// report the daemon's OWN write back to itself as a `hand-edit` event on a
/// later tick — this was the live defect: the dispatched handler wrote
/// `sessions.json` on `handle_conn`'s own connection thread while the
/// watcher's baseline only ever moved on the SEPARATE tick thread, so the
/// very next sweep saw a changed mtime nobody had told it about. Waits
/// across several ~1s tick cycles (long enough that the pre-fix daemon
/// reliably fired the false event by now) before asserting the feed is
/// clean, then proves the watcher itself is still alive by making a truly
/// out-of-band edit (bypassing the daemon entirely) and confirming THAT one
/// still fires on the next tick — the fix re-baselines once per dispatch,
/// it does not blind the watcher going forward.
#[test]
fn dispatched_session_start_produces_no_false_hand_edit_event() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _saver = aoide_test_support::EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_DAEMON_SOCKET", "XDG_RUNTIME_DIR"]);

    let dir = unique_dir("hand-edit-stage");
    std::fs::create_dir_all(&dir).unwrap();
    // Set BEFORE the daemon starts ([`RunLoopGuard`]'s own doc): its
    // `stage_roster()`-seeded baseline must resolve against THIS directory,
    // not whatever `AOIDE_STAGE_DIR` happened to hold before — and the
    // resident loop must live in a process `RunLoopGuard` can actually stop.
    std::env::set_var("AOIDE_STAGE_DIR", &dir);

    let mut daemon = RunLoopGuard::start("hand-edit", &dir);
    let daemon_socket = daemon.socket_path.clone();
    let events_path = daemon.events_path.clone();
    assert!(
        daemon_socket.exists(),
        "the resident child must have bound its socket before the routed call: {daemon_socket:?}"
    );
    assert!(daemon.child_is_alive(), "the resident run_loop child must be up before the routed call");

    // The routed door itself must be the one the client resolves — asserted,
    // not assumed, so this test can never quietly fall back to the direct
    // write and still pass.
    assert_eq!(aoide_client::daemon::socket_path(), daemon_socket);

    // Routed `session start` — the write happens on the daemon's own
    // accept thread (same as `routed_session_start_...` above), exactly the
    // write task #92 mis-reported.
    let outcome = dispatch(&session_start_inv("pd92-hand-edit"));
    assert_eq!(outcome.status, aoide_protocol::output::Status::Ok, "{outcome:?}");
    assert!(dir.join("sessions.json").exists(), "the routed dispatch must have written sessions.json");
    assert!(
        daemon.child_is_alive(),
        "the resident run_loop child died mid-test — the routed call did not exercise it"
    );

    // Several tick cycles' worth of headroom (~1s cadence) — long enough
    // that a stale-baseline false positive would reliably have landed in
    // the feed by now.
    std::thread::sleep(Duration::from_millis(3500));

    let false_positives = hand_edit_events_for(&events_path, "sessions.json");
    assert!(
        false_positives.is_empty(),
        "the daemon's own dispatched write must never be reported as a hand edit: {false_positives:?}"
    );

    // A GENUINE out-of-band edit — made directly on disk, never through the
    // daemon — must still fire on the next tick: the fix re-baselines after
    // a dispatch, it doesn't suppress the watcher permanently.
    let sessions_path = dir.join("sessions.json");
    let mut sf: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&sessions_path).unwrap()).unwrap();
    sf["handEdited"] = serde_json::json!(true);
    std::fs::write(&sessions_path, serde_json::to_vec_pretty(&sf).unwrap()).unwrap();

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut fired = false;
    while Instant::now() < deadline {
        if !hand_edit_events_for(&events_path, "sessions.json").is_empty() {
            fired = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(fired, "a genuine out-of-band edit made after the dispatch must still fire a hand-edit event");

    // Stop the resident loop BEFORE removing anything it writes to — the
    // guard kills and WAITS, so no tick can land in the tree below.
    drop(daemon);
    std::fs::remove_dir_all(&dir).ok();
}

/// Lifecycle proof for [`RunLoopGuard`]: its Drop KILLS and WAITS on the
/// resident child — the pid it owned is a live process before the drop and an
/// absent one after it (`kill(pid, 0)` → ESRCH), so a fixture run can never
/// leave a ticking daemon behind for the next test to trip over. Without this
/// the isolation could regress silently into exactly the leak it replaces.
#[test]
fn run_loop_guard_reaps_the_resident_child_it_owns() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _saver = aoide_test_support::EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_DAEMON_SOCKET"]);

    let dir = unique_dir("guard-reap-stage");
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("AOIDE_STAGE_DIR", &dir);

    let daemon = RunLoopGuard::start("guard-reap", &dir);
    let pid = daemon.child.id();
    assert!(pid_is_alive(pid), "the owned child must be a live process while the guard holds it");

    drop(daemon);

    let deadline = Instant::now() + Duration::from_secs(5);
    while pid_is_alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!pid_is_alive(pid), "RunLoopGuard's Drop must reap the resident child (pid {pid})");
    std::fs::remove_dir_all(&dir).ok();
}

/// The readiness-panic half of the lifecycle: `RunLoopGuard` is constructed
/// BEFORE the accepting wait, so when the child never becomes accepting
/// (`start_with_pid_file`'s child stays alive without binding) and `start`
/// panics, the guard still owns the child as the panic unwinds and its Drop
/// kills + waits on it. The pid is asserted exactly: absent by the time
/// `start` has unwound.
#[test]
fn run_loop_guard_reaps_the_child_even_when_readiness_panics() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _saver = aoide_test_support::EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_DAEMON_SOCKET"]);

    let dir = unique_dir("guard-readiness-stage");
    std::fs::create_dir_all(&dir).unwrap();
    std::env::set_var("AOIDE_STAGE_DIR", &dir);

    let socket = unique_dir("guard-readiness").with_extension("sock");
    let pid_file = unique_dir("guard-readiness-pid");

    let dir_for_child = dir.clone();
    let socket_for_child = socket.clone();
    let pid_file_for_child = pid_file.clone();
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        RunLoopGuard::start_with_pid_file("guard-readiness", &dir_for_child, &socket_for_child, &pid_file_for_child)
    }));
    assert!(panicked.is_err(), "the guard must panic when the child never becomes accepting");

    let child_pid: u32 = std::fs::read_to_string(&pid_file)
        .expect("the child must record its own pid before the readiness wait times out")
        .trim()
        .parse()
        .expect("the recorded pid must parse");
    assert!(
        !pid_is_alive(child_pid),
        "the readiness panic must still reap the run_loop child it spawned (pid {child_pid})"
    );
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_file(&socket).ok();
    std::fs::remove_file(&pid_file).ok();
}

/// Use the core's POSIX liveness probe for the fixture's owned child.
fn pid_is_alive(pid: u32) -> bool {
    aoide_storage::fs::pid_is_alive(pid)
}

fn grant_exempt_inv(id: &str, state: &str) -> Invocation {
    let mut flags = BTreeMap::new();
    flags.insert("id".to_string(), id.to_string());
    Invocation {
        path: vec!["session".to_string(), "grant".to_string()],
        args: vec!["exempt".to_string(), state.to_string()],
        flags,
        door: Door::Cli,
    }
}

/// Task #20's own routing claim (`grant.rs`'s module doc: `exempt_grant`
/// "MUST route through `aoide_client::daemon::daemon_dispatch` first, the
/// same L4 dual-writer discipline every other stage-writing `session *`
/// handler in this crate holds") has never been exercised over a REAL
/// socket — `grant.rs`'s own `exempt_grant`/`session_grant` unit tests all
/// floor `AOIDE_DAEMON_SOCKET` to a dead path (`crate::env_lock`'s P-D6
/// safety net), so only the direct fallback in that file ever runs. This is
/// the twin of `routed_session_start_produces_byte_identical_sessions_json_
/// to_the_direct_path` above for THIS write path: `session grant exempt on
/// --id <id>` is dispatched against an IDENTICAL roster fixture staged in
/// two directories, once with `$AOIDE_DAEMON_SOCKET` pointed at nothing
/// (direct fallback) and once at a REAL, freshly-started daemon (routed —
/// the write happens on the daemon's own accept thread, same as
/// `session_start`'s own proof above), and `sessions.json`'s bytes are
/// compared in full — unlike the `session_start` twin, no field here is
/// randomized (toggling `exempt` touches no other field), so no
/// normalization is needed before the comparison.
///
/// Also covers the cheap adjacent property on the already-routed file:
/// `exempt` carries the same `#[serde(skip_serializing_if = "is_false")]`
/// shape as `spawned` (`records.rs`'s own doc on the field) — present as
/// `"exempt": true` once `on` lands, and gone from the byte stream entirely
/// (not merely flipped to `false`) once a second, routed `off` call lands
/// on the same record — proving the omit-when-false shape survives the
/// socket round trip exactly like the write itself does.
#[test]
fn routed_session_grant_exempt_produces_byte_identical_sessions_json_to_the_direct_path() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let _saver = aoide_test_support::EnvSaver::capture(&["AOIDE_STAGE_DIR", "AOIDE_DAEMON_SOCKET", "XDG_RUNTIME_DIR"]);

    let dir_direct = unique_dir("grant-exempt-direct-stage");
    let dir_daemon = unique_dir("grant-exempt-daemon-stage");
    std::fs::create_dir_all(&dir_direct).unwrap();
    std::fs::create_dir_all(&dir_daemon).unwrap();

    // Start the daemon EARLY, same as the `session_start` twin above.
    let daemon_socket = start_daemon("grant-exempt");
    let dead_socket = unique_dir("grant-exempt-dead").with_extension("sock");

    // An identical roster fixture in BOTH stage dirs — `exempt_grant`
    // requires the target id to already be on the roster (`grant.rs`'s
    // "Fork 2": a dead/unknown id is a refusal, unlike `undying`), so this
    // test needs a fixture-then-dispatch shape (the reap test's own shape
    // above), not the session_start twin's from-nothing shape.
    let fixture = aoide_storage::records::SessionsFile {
        schema_version: "0".to_string(),
        sessions: vec![aoide_storage::records::SessionRecord {
            session_id: "pd6-grant-exempt".to_string(),
            agent: "claude".to_string(),
            state: "idle".to_string(),
            started_at: aoide_storage::time::now_iso_utc(),
            ..Default::default()
        }],
    };
    std::env::set_var("AOIDE_STAGE_DIR", &dir_direct);
    aoide_storage::stage::write_stage(&aoide_storage::stage::sessions_path(), &fixture).unwrap();
    std::env::set_var("AOIDE_STAGE_DIR", &dir_daemon);
    aoide_storage::stage::write_stage(&aoide_storage::stage::sessions_path(), &fixture).unwrap();

    // Pass 1: direct fallback — no daemon reachable at this socket path.
    std::env::set_var("AOIDE_STAGE_DIR", &dir_direct);
    std::env::set_var("AOIDE_DAEMON_SOCKET", &dead_socket);
    let direct_outcome = dispatch(&grant_exempt_inv("pd6-grant-exempt", "on"));
    assert_eq!(direct_outcome.status, aoide_protocol::output::Status::Ok, "{direct_outcome:?}");
    assert_eq!(direct_outcome.data.as_ref().unwrap()["exempt"], true, "{direct_outcome:?}");

    // Pass 2: routed — the SAME invocation, now with a live daemon to reach.
    std::env::set_var("AOIDE_STAGE_DIR", &dir_daemon);
    std::env::set_var("AOIDE_DAEMON_SOCKET", &daemon_socket);
    let routed_outcome = dispatch(&grant_exempt_inv("pd6-grant-exempt", "on"));
    assert_eq!(routed_outcome.status, aoide_protocol::output::Status::Ok, "{routed_outcome:?}");
    assert_eq!(routed_outcome.data.as_ref().unwrap()["exempt"], true, "{routed_outcome:?}");

    let direct_bytes = std::fs::read(dir_direct.join("sessions.json")).unwrap();
    let daemon_bytes = std::fs::read(dir_daemon.join("sessions.json")).unwrap();
    assert_eq!(
        direct_bytes, daemon_bytes,
        "direct-path sessions.json:\n{}\n\nrouted (through the daemon) sessions.json:\n{}",
        String::from_utf8_lossy(&direct_bytes),
        String::from_utf8_lossy(&daemon_bytes),
    );
    assert_eq!(routed_outcome.command, direct_outcome.command);

    let daemon_str = String::from_utf8_lossy(&daemon_bytes);
    assert!(
        daemon_str.contains("\"exempt\": true"),
        "the `on` mark must be present with the skip-if-false shape: {daemon_str}"
    );

    // The cheap adjacent proof: a SECOND routed call, `off`, on the SAME
    // daemon-routed record — `is_false` must omit the field entirely, not
    // merely flip it to `false`, and that omission must survive the socket
    // round trip too.
    let routed_off = dispatch(&grant_exempt_inv("pd6-grant-exempt", "off"));
    assert_eq!(routed_off.status, aoide_protocol::output::Status::Ok, "{routed_off:?}");
    assert_eq!(routed_off.data.as_ref().unwrap()["exempt"], false, "{routed_off:?}");
    let after_off = std::fs::read_to_string(dir_daemon.join("sessions.json")).unwrap();
    assert!(
        // NB: the session id itself is `pd6-grant-exempt`, so the bare
        // substring `"exempt"` alone would always match — the JSON KEY
        // (quoted, with its colon) is the thing that must be gone.
        !after_off.contains("\"exempt\":"),
        "`off`, routed through the daemon, must omit `exempt` entirely (skip-if-false), not just flip it to false: {after_off}"
    );

    std::fs::remove_dir_all(&dir_direct).ok();
    std::fs::remove_dir_all(&dir_daemon).ok();
    std::fs::remove_file(&daemon_socket).ok();
}
