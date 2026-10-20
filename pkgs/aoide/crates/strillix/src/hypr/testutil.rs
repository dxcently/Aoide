//! Shared fixtures for the desktop's Hyprland tests: a fake request socket, a per-test stage
//! dir, a session and terminal builder, and the env save/restore guard. Test-only.

use aoide_conduct::graph::SessionRecord;
use serde_json::Map;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub(crate) fn session(
    id: &str,
    cwd: &str,
    state: &str,
    started: &str,
    parent: Option<&str>,
) -> SessionRecord {
    SessionRecord {
        session_id: id.into(),
        enduring_agent_id: None,
        project: None,
        agent: "claude".into(),
        window_address: format!("0x{id}"),
        cwd: cwd.into(),
        state: state.into(),
        // Every fixture here is a harness-shaped record unless a test says
        // otherwise: a claude wrap whose resolved program is `claude`.
        // `stamp_wrap`/`program_is_a_shell` tests set both fields directly.
        shell: false,
        wrapped_program: Some("claude".into()),
        started_at: started.into(),
        parent_session_id: parent.map(str::to_string),
        remote_parent: None,
        conductable: None,
        socket: None,
        title: None,
        pid: None,
        workspace: None,
        workspace_project: None,
        activity: None,
        kind: None,
        say: None,
        tool: None,
        model: None,
        context_tokens: None,
        context_ceiling: None,
        needs_sudo: None,
        question: None,
        log_path: None,
        petname: None,
        task: None,
        instructions_path: None,
        exit_code: None,
        ended_at: None,
        outcome: None,
        report_to: None,
        hook_ancestry: Vec::new(),
        headless: false,
        spawned: false,
        exempt: false,
        harness_session_id: None,
        session_start_at: None,
        opening_turn: None,
        resumed_from: None,
        origin: None,
        seal: None,
        sealed_issued_at: None,
        sealed_pid_starttime: None,
        sealed_boot_id: None,
        ever_sealed: None,
        attested_spawner: None,
        restore: None,
        sources: None,
        native_role: None,
        extra: Map::new(),
    }
}

/// The fake Hyprland request socket behind [`super::window::hyprctl_clients`]: a
/// listener at `<root>/hypr/test/.socket.sock`, with `XDG_RUNTIME_DIR=<root>` and
/// `HYPRLAND_INSTANCE_SIGNATURE=test` set. It answers `j/clients` with the one
/// window [`fake_hyprctl`]'s shim would print, built from the same vars
/// (`AOIDE_TEST_CLIENT_PID`, `AOIDE_TEST_CLIENT_CLASS`, `AOIDE_TEST_WS`) read per
/// request, so a test that sets them mid-way sees the change. Any other request
/// gets no reply. The guard owns the server thread: dropping it stops and joins
/// the thread, then restores the env. GATED on Unix, like the shim.
#[cfg(unix)]
pub(crate) struct HyprSocket {
    _env: EnvVars,
    stop: Arc<AtomicBool>,
    server: Option<std::thread::JoinHandle<()>>,
}

#[cfg(unix)]
impl Drop for HyprSocket {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(server) = self.server.take() {
            let _ = server.join();
        }
    }
}

#[cfg(unix)]
pub(crate) fn fake_hypr_socket(tag: &str) -> (HyprSocket, PathBuf) {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    let root = unique_stage(tag);
    let sockets = root.join("hypr").join("test");
    std::fs::create_dir_all(&sockets).unwrap();
    let listener = UnixListener::bind(sockets.join(".socket.sock")).unwrap();
    listener.set_nonblocking(true).unwrap();
    let env = EnvVars::save(&[
        "XDG_RUNTIME_DIR",
        "HYPRLAND_INSTANCE_SIGNATURE",
        "AOIDE_TEST_WS",
        "AOIDE_TEST_CLIENT_PID",
        "AOIDE_TEST_CLIENT_CLASS",
    ]);
    std::env::set_var("XDG_RUNTIME_DIR", &root);
    std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "test");
    std::env::set_var("AOIDE_TEST_WS", "3");
    let stop = Arc::new(AtomicBool::new(false));
    let server = {
        let stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut conn, _)) => {
                        conn.set_nonblocking(false).unwrap();
                        conn.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
                        let mut request = [0u8; 64];
                        let n = conn.read(&mut request).unwrap_or(0);
                        if &request[..n] == b"j/clients" {
                            let env_or = |k: &str| std::env::var(k).ok();
                            let ws = env_or("AOIDE_TEST_WS")
                                .and_then(|s| s.parse::<i64>().ok())
                                .unwrap_or(3);
                            let pid = env_or("AOIDE_TEST_CLIENT_PID")
                                .and_then(|s| s.parse::<u32>().ok())
                                .unwrap_or_else(std::process::id);
                            let class = env_or("AOIDE_TEST_CLIENT_CLASS")
                                .unwrap_or_else(|| "kitty".to_string());
                            let reply = serde_json::json!([{
                                "address": "0xAA",
                                "class": class,
                                "title": "t",
                                "pid": pid,
                                "workspace": { "id": ws, "name": "ws" },
                                "mapped": true,
                                "at": [0, 0],
                                "size": [1, 1],
                            }]);
                            let _ = conn.write_all(reply.to_string().as_bytes());
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(_) => break,
                }
            }
        })
    };
    (HyprSocket { _env: env, stop, server: Some(server) }, root)
}

/// Monotonic per-process counter backing `unique_stage`'s directory name —
/// see that function's doc for why a counter, not a nanosecond timestamp.
static STAGE_SEQ: AtomicU64 = AtomicU64::new(0);

/// Returns a fresh, already-created temp directory for one test's stage
/// (frequently doubled as `$XDG_RUNTIME_DIR`, one path segment above where a
/// `session-<id>.sock` control socket gets bound). The directory name is
/// deliberately SHORT: a hash of `tag` plus pid plus a monotonic counter —
/// never the tag text itself, never a full nanosecond timestamp. `AF_UNIX`
/// addresses cap at `sizeof(sun_path)` (108 bytes on Linux), and this path
/// sits under `$TMPDIR`, which varies (`/tmp` bare vs. `/tmp/nix-shell.XXXXXX`
/// under `nix develop`). The old `aoide-graph-<tag>-<pid>-<nanos>` name ate
/// most of that budget on its own — a long tag plus a 19-digit nanosecond
/// timestamp plus a nix `$TMPDIR` plus `/aoide/session-<id>.sock` blew past
/// `SUN_LEN` and panicked, poisoning `env_lock` for every test after it
/// (#75). The hash keeps some of the tag's grep-ability (same tag, same
/// prefix) without its length; the counter — not the timestamp — is what
/// actually guarantees uniqueness between calls in the same process.
pub(crate) fn unique_stage(tag: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    tag.hash(&mut hasher);
    let tag_hash = (hasher.finish() as u32) & 0xffff;
    let seq = STAGE_SEQ.fetch_add(1, Ordering::Relaxed);
    let mut dir = std::env::temp_dir();
    dir.push(format!("ao{:x}-{:x}-{:x}", std::process::id(), tag_hash, seq));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

pub(crate) struct EnvVars {
    keys: Vec<(&'static str, Option<String>)>,
}
impl EnvVars {
    pub(crate) fn save(keys: &[&'static str]) -> Self {
        EnvVars {
            keys: keys.iter().map(|k| (*k, std::env::var(k).ok())).collect(),
        }
    }
}
impl Drop for EnvVars {
    fn drop(&mut self) {
        for (k, v) in &self.keys {
            match v {
                Some(val) => std::env::set_var(k, val),
                None => std::env::remove_var(k),
            }
        }
    }
}

/// A socket that accepts every connection and reads nothing: where a test's aoided is up. The door's
/// probe connects and closes, and the accept loop drops the stream, so nothing is ever sent to it.
/// Bound once per test process.
#[cfg(unix)]
pub(crate) fn accepting_daemon() -> PathBuf {
    static DAEMON: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DAEMON
        .get_or_init(|| {
            let dir = aoide_test_support::unique_tmp("dmn");
            std::fs::create_dir_all(&dir).unwrap();
            let path = dir.join("aoided.sock");
            let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    drop(stream);
                }
            });
            path
        })
        .clone()
}

/// aoided is up for the guard's span: the door finds the daemon listening and execs the core stub.
#[cfg(unix)]
pub(crate) fn core_up() -> EnvVars {
    let env = EnvVars::save(&["AOIDE_DAEMON_SOCKET"]);
    std::env::set_var("AOIDE_DAEMON_SOCKET", accepting_daemon());
    env
}
