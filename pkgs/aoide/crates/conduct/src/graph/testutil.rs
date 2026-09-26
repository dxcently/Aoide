//! Shared `#[cfg(test)]` fixtures reused across the `graph` submodule test
//! suites: session/project builders, `Invocation` constructors per command
//! family, a unique per-test stage dir, and the env-var save/restore guard.
//! `pub(crate)` (not `pub(in crate::graph)`): the whole module is
//! `#[cfg(test)]`-gated at its `mod testutil;` declaration in `graph.rs`, so
//! nothing here ships in a non-test build regardless of the wider visibility.

use super::model::{load_stage, sessions_path, write_stage, Project, SessionRecord, SessionsFile};
use super::window::TermWindow;
use aoide_protocol::Invocation;
use serde_json::Map;
use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
pub(crate) fn term_win(addr: &str, class: &str, cwd: &str) -> TermWindow {
    TermWindow {
        address: addr.into(),
        class: class.into(),
        title: String::new(),
        workspace: Some(1),
        pid: Some(4321),
        mapped: true,
        cwd: cwd.into(),
    }
}
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
        // otherwise; `stamp_shell`/`program_is_a_shell` tests set it directly.
        shell: false,
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
        restore: None,
        sources: None,
        native_role: None,
        extra: Map::new(),
    }
}
pub(crate) fn fixture_projects() -> Vec<Project> {
    vec![
        Project {
            name: "nested".into(),
            path: "/home/k/Aoide/sub".into(),
            ..Default::default()
        },
        Project {
            name: "aoide".into(),
            path: "/home/k/Aoide".into(),
            ..Default::default()
        },
    ]
}
/// Install a FAKE `hyprctl` on `PATH` (the established shim technique — this
/// crate's own `crates/AGENTS.md` note on replacing a real binary) so the
/// compositor-facing half can be driven end-to-end with no compositor at all.
/// Both `clients -j` and `activeworkspace -j` answer: `clients` prints ONE
/// client whose `pid` is the TEST process's own (`$PPID` — where the pid
/// ancestry walk starts) and whose workspace id comes from `$AOIDE_TEST_WS`,
/// and `activeworkspace` prints that same id in the focused-workspace shape.
/// Returns the env guard (restores `PATH`/`HYPRLAND_INSTANCE_SIGNATURE`/
/// `AOIDE_TEST_WS` on drop) and the shim directory, for the caller to remove.
pub(crate) fn fake_hyprctl(tag: &str) -> (EnvVars, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("aoide-fake-hypr-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let shim = dir.join("hyprctl");
    std::fs::write(
        &shim,
        r#"#!/bin/sh
ws=${AOIDE_TEST_WS:-3}
client="{\"address\":\"0xAA\",\"class\":\"kitty\",\"title\":\"t\",\"pid\":$PPID,\"workspace\":{\"id\":$ws,\"name\":\"ws\"},\"mapped\":true,\"at\":[0,0],\"size\":[1,1]}"
if [ "$1" = "activeworkspace" ]; then
  echo "{\"id\":$ws,\"name\":\"ws\"}"
else
  echo "[$client]"
fi
"#,
    )
    .unwrap();
    std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).unwrap();
    let env = EnvVars::save(&["PATH", "HYPRLAND_INSTANCE_SIGNATURE", "AOIDE_TEST_WS"]);
    let path = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{path}", dir.display()));
    std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "test");
    std::env::set_var("AOIDE_TEST_WS", "3");
    (env, dir)
}

pub(crate) fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
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
/// A fake A2A door for a REAL curl POST: binds a loopback port, answers EVERY
/// request with `body` until the listener is dropped, and hands back the
/// listener (hold it — dropping it frees the port) plus the `Node.url` to
/// register. Mirrors `aoide-client`'s own `spawn_fake_pair_poll_server`: the
/// established real-curl-real-listener pattern this workspace proves a wire
/// path with, never a mocked transport. What it is FOR here is the far side of
/// an error: a peer's own bytes, hostile ones included, arriving as the
/// `aoide-client` function under test would really receive them.
pub(crate) fn fake_door(body: String) -> (std::net::TcpListener, String) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let accepter = listener.try_clone().unwrap();
    std::thread::spawn(move || {
        use std::io::{Read, Write};
        loop {
            let Ok((mut stream, _)) = accepter.accept() else { break };
            let mut buf = [0u8; 4096];
            if stream.read(&mut buf).unwrap_or(0) == 0 {
                continue;
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (listener, format!("http://127.0.0.1:{port}/"))
}

pub(crate) fn invocation(path: &[&str], args: &[&str]) -> Invocation {
    Invocation {
        path: path.iter().map(|s| s.to_string()).collect(),
        args: args.iter().map(|s| s.to_string()).collect(),
        flags: BTreeMap::new(),
        door: aoide_protocol::Door::Cli,
    }
}
pub(crate) fn conduct_invocation(args: &[&str], flags: &[(&str, &str)]) -> Invocation {
    Invocation {
        path: vec!["conduct".into()],
        args: args.iter().map(|s| s.to_string()).collect(),
        flags: flags
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        door: aoide_protocol::Door::Cli,
    }
}
pub(crate) fn spawn_invocation(args: &[&str], flags: &[(&str, &str)]) -> Invocation {
    Invocation {
        path: vec!["spawn".into()],
        args: args.iter().map(|s| s.to_string()).collect(),
        flags: flags
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        door: aoide_protocol::Door::Cli,
    }
}
pub(crate) fn send_invocation(args: &[&str], flags: &[(&str, &str)]) -> Invocation {
    Invocation {
        path: vec!["send".into()],
        args: args.iter().map(|s| s.to_string()).collect(),
        flags: flags
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        door: aoide_protocol::Door::Cli,
    }
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
/// Binds `$XDG_RUNTIME_DIR` to a fresh, empty `unique_stage` dir for the
/// returned guard's lifetime, restoring whatever it was on drop. `reap()`
/// wires `sync_eidolon_sessions()` into every pass, and that walks
/// `$XDG_RUNTIME_DIR/eidolon` — a test that reaches `reap()` (or
/// `sync_eidolon_sessions`/`eidolon_presence_sessions` directly) without
/// this inherits the ambient runtime dir and can ping whatever real eidolon
/// socket happens to be live on the box running the suite.
pub(crate) fn isolated_xdg_runtime(tag: &str) -> EnvVars {
    let env = EnvVars::save(&["XDG_RUNTIME_DIR"]);
    std::env::set_var("XDG_RUNTIME_DIR", unique_stage(tag));
    env
}
/// Locate the real, already-built `aoide` binary as `current_exe()`'s
/// sibling in the shared `target/<profile>/` dir (`current_exe()` under
/// `cargo test` resolves to `target/<profile>/deps/aoide_conduct-<hash>` —
/// the profile dir's PARENT of `deps/` is where cargo also drops the
/// workspace's own `[[bin]]` outputs). Panics with a clear message rather
/// than silently no-op-ing if it isn't there — the box this ships on has
/// already built it (P1 landed and tested against this same binary).
/// Shared by `spawn.rs`'s and `resurrect.rs`'s own end-to-end tests, both of
/// which point `AOIDE_CONDUCT_SPAWN_EXE` at it so `graph spawn [--windowed]`
/// re-execs a real dispatcher instead of the test harness binary.
pub(crate) fn built_aoide_bin() -> PathBuf {
    let test_exe = std::env::current_exe().expect("current_exe resolves under cargo test");
    let profile_dir = test_exe
        .parent() // .../target/<profile>/deps
        .and_then(|p| p.parent()) // .../target/<profile>
        .expect("test exe has a target/<profile>/deps parent");
    let bin = profile_dir.join("aoide");
    assert!(
        bin.exists(),
        "expected a pre-built `aoide` binary at {bin:?} — run `cargo build --bin aoide` first"
    );
    bin
}
/// Stamp the P-C5 capture onto an already-registered stage record — the one
/// durable trace `conduct.rs::program_is_a_shell`'s verdict leaves on a
/// record, and therefore what
/// [`crate::graph::conduct::wrapped_program_is_a_shell`] reads. Its only real
/// writer is conduct's own ~1 Hz tick, out of reach from a unit test, so the
/// tests that need the shape write it directly — the same direct-field-write
/// idiom `set_petname` (doorbell) and `doc.rs`'s own petname tests already use.
pub(crate) fn stamp_shell_capture(id: &str) {
    let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
    let rec = file
        .sessions
        .iter_mut()
        .find(|s| s.session_id == id)
        .unwrap_or_else(|| panic!("no stage record for `{id}`"));
    rec.restore = Some(aoide_storage::records::RestoreSnapshot::default());
    write_stage(&sessions_path(), &file).unwrap();
}
/// Stamp the DURABLE half instead: `shell = true` with no capture at all —
/// what `conduct`'s own registration writes (via `stamp_shell`) and what a
/// record looks like before its first tick. Separate helper, because the two
/// reads are deliberately independent arms of `wrapped_program_is_a_shell`.
pub(crate) fn stamp_shell_field(id: &str) {
    let mut file: SessionsFile = load_stage(&sessions_path()).unwrap();
    let rec = file
        .sessions
        .iter_mut()
        .find(|s| s.session_id == id)
        .unwrap_or_else(|| panic!("no stage record for `{id}`"));
    rec.shell = true;
    write_stage(&sessions_path(), &file).unwrap();
}
pub(crate) fn flag_invocation(path: &[&str], flags: &[(&str, &str)]) -> Invocation {
    Invocation {
        path: path.iter().map(|s| s.to_string()).collect(),
        args: vec![],
        flags: flags
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        door: aoide_protocol::Door::Cli,
    }
}
