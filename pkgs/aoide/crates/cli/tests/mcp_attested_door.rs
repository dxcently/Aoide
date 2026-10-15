//! The MCP door is attested like the CLI: a send or a spawn that arrives over
//! `aoide mcp serve --stdio` is judged by the sealed session its server process
//! runs under, found by walking that process's own `/proc` ancestry, and by
//! nothing the caller types.
//!
//! This drives the REAL binary. A stand-in daemon answers the key fetch with a
//! test keypair's public half; `sessions.json` holds a session `orch` sealed
//! over the pid of a `sh` the test started, and the server is that `sh`'s
//! child, so the production resolver (`real_attested_sender`) walks to `orch`
//! with nothing injected. A second server, started by this test directly, has
//! no sealed ancestor at all and is the unattested control.

#![cfg(unix)]

use aoide_storage::records::{SessionRecord, SessionsFile};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

const AOIDE: &str = env!("CARGO_BIN_EXE_aoide");

/// One `aoide mcp serve --stdio` and the pipe to drive it.
struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    next_id: u64,
}

impl Server {
    /// Start it with the lane's env, directly or as the child of `sh`.
    fn start(world: &World, under_sh: bool) -> Server {
        let mut command = if under_sh {
            let mut sh = Command::new("sh");
            // The trailing `exit` stops sh from exec'ing the server in its own
            // place, so the server is a child of the pid the seal names.
            sh.arg("-c").arg(r#""$0" mcp serve --stdio; exit $?"#).arg(AOIDE);
            sh
        } else {
            let mut direct = Command::new(AOIDE);
            direct.args(["mcp", "serve", "--stdio"]);
            direct
        };
        command
            .env_remove("AOIDE_SESSION_ID")
            .env_remove("AOIDE_CONDUCT_AUTOGATE")
            .env("AOIDE_ROOT", world.dir.join("root"))
            .env("AOIDE_STAGE_DIR", world.dir.join("stage"))
            .env("AOIDE_STATE_DIR", world.dir.join("state"))
            .env("XDG_RUNTIME_DIR", world.dir.join("run"))
            .env("AOIDE_AUDIT_LOG", world.dir.join("log"))
            .env("AOIDE_DAEMON_SOCKET", world.dir.join("d.sock"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn().expect("start the MCP server");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        Server { child, stdin, lines, next_id: 0 }
    }

    /// The pid of the process this test started (the `sh`, when under one).
    fn pid(&self) -> u32 {
        self.child.id()
    }

    /// One JSON-RPC request and its response, bounded so a wedged server fails
    /// the test rather than hanging it.
    fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let request = json!({ "jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params });
        let stdin = self.stdin.as_mut().expect("stdin open");
        writeln!(stdin, "{request}").unwrap();
        stdin.flush().unwrap();
        let line = self.lines.recv_timeout(Duration::from_secs(20)).expect("the server answered");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }

    /// `tools/call`, with the tool's own `Outcome` parsed out of the content.
    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let response = self.rpc("tools/call", json!({ "name": tool, "arguments": arguments }));
        let text = response["result"]["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("no tool result: {response}"));
        serde_json::from_str(text).unwrap_or_else(|e| panic!("outcome not JSON ({e}): {text}"))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The test's private box: a stage, a runtime dir, an audit log and a daemon
/// socket that answers the key fetch, all under one short path in `TMPDIR`.
struct World {
    dir: PathBuf,
    keypair: aoide_storage::identity::Keypair,
}

impl World {
    fn new(tag: &str) -> World {
        let dir = std::env::temp_dir().join(format!("av-mcp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for sub in ["stage", "state", "run", "root"] {
            std::fs::create_dir_all(dir.join(sub)).unwrap();
        }
        let keypair = aoide_storage::identity::mint_ephemeral().unwrap();
        let socket = dir.join("d.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let reply = format!("{}\n", json!({ "ok": true, "daemon": "aoided", "sealPubkeyHex": keypair.info().pubkey_hex }));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut line = String::new();
                let _ = BufReader::new(stream.try_clone().unwrap()).read_line(&mut line);
                let _ = (&stream).write_all(reply.as_bytes());
            }
        });
        World { dir, keypair }
    }

    fn sessions_path(&self) -> PathBuf {
        self.dir.join("stage").join("sessions.json")
    }

    fn pending(&self) -> Vec<Value> {
        let raw = std::fs::read_to_string(self.dir.join("stage").join("pending.json")).unwrap_or_default();
        serde_json::from_str::<Value>(&raw)
            .ok()
            .and_then(|v| v["pending"].as_array().cloned())
            .unwrap_or_default()
    }

    /// A conductable session whose control socket this test listens on.
    fn target(&self, id: &str, parent: Option<&str>) -> (SessionRecord, UnixListener) {
        let socket = self.dir.join(format!("{id}.sock"));
        let listener = UnixListener::bind(&socket).unwrap();
        let record = SessionRecord {
            session_id: id.to_string(),
            agent: "claude".to_string(),
            state: "idle".to_string(),
            conductable: Some(true),
            socket: Some(socket.to_string_lossy().into_owned()),
            parent_session_id: parent.map(str::to_string),
            // A child a spawn registered under an attested parent carries that
            // parent's spawn stamp; the autogate honours only a stamped edge
            // (`attested_parent_of`). A bare `parent` is a forgeable edge.
            attested_spawner: parent.map(str::to_string),
            ..Default::default()
        };
        (record, listener)
    }

    /// A session record for the live process `pid`, sealed by the stand-in
    /// daemon's key the way the real daemon seals every pid-carrying record.
    fn sealed(&self, id: &str, pid: u32) -> SessionRecord {
        let issued_at = 1_700_000_000;
        let identity = aoide_storage::sealed_id::SealedIdentity {
            session_id: id.to_string(),
            pid: pid as i32,
            pid_starttime: aoide_storage::attest::pid_starttime(pid as i32).expect("a live pid"),
            origin_class: String::new(),
            issued_at,
        };
        SessionRecord {
            session_id: id.to_string(),
            agent: "claude".to_string(),
            state: "idle".to_string(),
            pid: Some(pid),
            seal: Some(aoide_storage::sealed_id::mint_seal(&self.keypair, &identity)),
            sealed_issued_at: Some(issued_at),
            ..Default::default()
        }
    }

    fn write_sessions(&self, sessions: Vec<SessionRecord>) {
        let file = SessionsFile { schema_version: "0".to_string(), sessions };
        std::fs::write(self.sessions_path(), serde_json::to_string(&file).unwrap()).unwrap();
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn nothing_connected(listener: &UnixListener) -> bool {
    listener.set_nonblocking(true).unwrap();
    matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock)
}

fn audit_has(dir: &Path, needle: &str) -> bool {
    std::fs::read_to_string(dir.join("log")).unwrap_or_default().contains(needle)
}

/// The whole door through the real binary. A server whose ancestry reaches the
/// sealed session `orch` delivers to `orch`'s own child, with the attested
/// session in the provenance and no `--yes`; it parks a send to anyone else's
/// and cannot approve what it parked. A server with no sealed ancestor parks
/// the same send, and parks a spawn whole: nothing launches.
#[test]
fn the_real_mcp_server_is_attested_by_its_ancestry_and_still_never_approves() {
    let world = World::new("attested");
    let (child, child_listener) = world.target("orch-child", Some("orch"));
    let (other, other_listener) = world.target("someone-else", None);

    let mut attested = Server::start(&world, true);
    // The child handle is the `sh`; the server is its child, so the seal names
    // the server's parent and the walk reaches it one step up.
    let sh_pid = attested.pid();
    world.write_sessions(vec![world.sealed("orch", sh_pid), child, other]);
    attested.rpc("initialize", json!({}));

    let accepted = aoide_test_support::expect_delivery(child_listener, "an attested MCP parent's send is delivered");
    let out = attested.call("send", json!({ "id": "orch-child", "text": "hi", "submit": true, "from": "boss" }));
    let got = accepted.join().unwrap();
    assert_eq!(out["status"], "ok", "{out}");
    assert_eq!(out["data"]["state"], "delivered", "{out}");
    assert_eq!(out["data"]["gate"], "autogate-parent", "{out}");
    assert_eq!(String::from_utf8(got).unwrap(), "from orch: hi\r", "the sealed session, not the typed label");
    assert!(world.pending().is_empty(), "a delivered send parks nothing");

    let out = attested.call("send", json!({ "id": "someone-else", "text": "hello" }));
    assert_eq!(out["data"]["state"], "pending", "an unrelated target is not orch's child: {out}");
    assert!(nothing_connected(&other_listener));
    let held = world.pending();
    assert_eq!(held.len(), 1, "{held:?}");
    assert_eq!(held[0]["from"], "mcp:orch", "{held:?}");

    // `--yes` is closed to MCP: the registry refuses it for an attested caller
    // too, before the send handler runs, so nothing new is held.
    let out = attested.call("send", json!({ "id": "someone-else", "text": "forced", "yes": true }));
    assert_eq!(out["status"], "usage", "--yes is refused even for an attested caller: {out}");
    assert!(out["message"].as_str().unwrap().contains("`--yes` is not open to the mcp door"), "{out}");
    let out = attested.call("session.pending.approve", json!({ "id": "0" }));
    assert_eq!(out["status"], "usage", "{out}");
    assert!(out["message"].as_str().unwrap().contains("is not open to the mcp door"), "{out}");
    assert_eq!(world.pending().len(), 1, "approving over MCP released nothing and held nothing new");
    assert!(nothing_connected(&other_listener));
    assert!(audit_has(&world.dir, "\"door\":\"mcp\""), "the door is audited");
    drop(attested);

    // The control: the same server, started by this test, under no sealed
    // session. Its send parks, its spawn parks whole, and nothing launches.
    let mut unattested = Server::start(&world, false);
    unattested.rpc("initialize", json!({}));
    let out = unattested.call("send", json!({ "id": "someone-else", "text": "unattested" }));
    assert_eq!(out["data"]["state"], "pending", "{out}");
    assert!(world.pending().iter().any(|p| p["text"] == "unattested" && p["from"] == "mcp"), "{:?}", world.pending());

    // The MCP `spawn` tool carries one positional string, so the wrapped
    // command is a single program: a script that leaves a marker if it runs.
    let marker = world.dir.join("launched");
    let script = world.dir.join("agent.sh");
    std::fs::write(&script, format!("#!/bin/sh\ntouch {}\n", marker.display())).unwrap();
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let out = unattested.call("spawn", json!({ "command": script, "id": "held-spawn" }));
    assert_eq!(out["status"], "ok", "{out}");
    assert_eq!(out["data"]["state"], "pending", "{out}");
    assert_eq!(out["data"]["launched"], false, "{out}");
    std::thread::sleep(Duration::from_millis(500));
    assert!(!marker.exists(), "the held spawn ran");
    let spawn = world.pending().into_iter().find(|p| p["sessionId"] == "held-spawn").expect("the spawn is held");
    assert_eq!(spawn["from"], "mcp");
    assert_eq!(spawn["spawn"]["args"][0], script.to_string_lossy().as_ref());
}
