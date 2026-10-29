//! H1's end-to-end proof, on a loopback fixture: the mail ADAPTER
//! (`aoide mail serve`) receives a signed, SEALED deposit over real TCP, the
//! relay files it and answers `accepted`, and that verified answer delivers the
//! letter at the sender — real bytes, real signatures, real sealing, no mocks
//! anywhere. No receipt letter comes back (ruling 131).
//!
//! **Two REAL isolated roots, and that is why one side is a child process.**
//! A relay and a poller are two boxes with two `state/` trees; `AOIDE_ROOT` is
//! process-global, so two roots in one process would race the serve thread
//! against the client calls reading it. The relay therefore runs as a REAL
//! `aoide mail serve` child (`aoide_test_support::built_aoide_bin()`) with its
//! own environment, and this test process is the poller.
//!
//! **The node records are hand-written VERIFIED records**, which is exactly
//! what P-CHARTER will supply from a charter line: pairing is LAN-only and the
//! adapter serves no pairing method, so a code-level test fabricates them (the
//! gate report's own §5/§6 proposal for H1's pre-charter evidence). Both sides
//! name their peer by `display::local_node_name()` — the name a node presents
//! as a NODE, the one the sender signs with and the one the relay's ack stamps
//! as its origin. On this fixture both boxes are this one host, so that name is
//! the SAME string on both sides, each registry holding it for the other.
//!
//! **`#[ignore]`'d, not skipped**: real loopback TCP, real `curl`, and the real
//! built binary. `cargo test -p aoide-cli` builds the bins; run explicitly with
//! `cargo test -p aoide-cli --test mail_adapter_round_trip -- --ignored`.

mod common;

use aoide::dispatch::dispatch;
use aoide_protocol::output::Status;
use aoide_protocol::Invocation;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The mesh both hand-written records grant `message` in: a request that names
/// none is judged in `[pairing] homeMesh`, whose default this is.
const HOME_MESH: &str = "home";

fn unique_root(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "aoide-h1-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Reserve a free loopback port: bind `:0`, read it back, drop the listener. A
/// tiny re-bind race is acceptable in a test.
fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn cli_invocation(path: &[&str], args: &[&str], flags: &[(&str, &str)]) -> Invocation {
    Invocation {
        path: path.iter().map(|s| s.to_string()).collect(),
        args: args.iter().map(|s| s.to_string()).collect(),
        flags: flags.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
        door: aoide_protocol::Door::Cli,
    }
}

fn wait_for_tcp_up(host_port: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if std::net::TcpStream::connect(host_port).is_ok() {
            return;
        }
        assert!(Instant::now() < deadline, "the adapter at {host_port} never came up within 10s");
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// One hand-written VERIFIED node record — the shape `aoide pair` writes, made
/// by hand because the ceremony is LAN-only and a real run's charter supplies
/// it (P-CHARTER): the `message` grant lives in the HOME mesh, which is the
/// mesh a request that names none is judged in.
fn verified_record(name: &str, url: &str, pubkey_hex: &str) -> aoide_storage::node_store::Node {
    aoide_storage::node_store::Node {
        name: name.to_string(),
        url: url.to_string(),
        token_file: None,
        bearer_secret: None,
        hub: false,
        pubkey: Some(pubkey_hex.to_string()),
        verified: true,
        grants: aoide_storage::node_store::grants_in(HOME_MESH, &["message"]),
        narrowed: aoide_storage::node_store::Grants::new(),
        via: None,
        added_at: "2026-09-26T00:00:00Z".to_string(),
    }
}

/// The filed entries, read straight off disk (no env switch) — the relay's own
/// root is written by a child process this test does not own. A letter files in
/// its box's log (`state/mail/boxes/<name>.log`), a receipt in `receipts.log`.
fn filed_entries(root: &Path) -> Vec<serde_json::Value> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("state/mail/boxes"))
        .map(|dir| dir.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();
    files.sort();
    files.push(root.join("state/mail/receipts.log"));
    files
        .iter()
        .flat_map(|path| std::fs::read_to_string(path).unwrap_or_default().lines().map(str::to_owned).collect::<Vec<_>>())
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(&l).expect("a filed line is JSON"))
        .collect()
}

#[test]
#[ignore = "real loopback TCP + real curl + the built `aoide` binary — run with --ignored (cargo test -p aoide-cli --test mail_adapter_round_trip -- --ignored)"]
fn a_sealed_letter_round_trips_through_the_mail_adapter() {
    let _guard = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let saved_root = std::env::var("AOIDE_ROOT").ok();
    let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
    let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
    let saved_audit = std::env::var("AOIDE_AUDIT_LOG").ok();
    let saved_runtime = std::env::var("XDG_RUNTIME_DIR").ok();
    let saved_session = std::env::var("AOIDE_SESSION_ID").ok();
    let restore = |key: &str, value: Option<String>| match value {
        Some(v) => std::env::set_var(key, v),
        None => std::env::remove_var(key),
    };

    let relay_root = unique_root("relay");
    let sender_root = unique_root("sender");
    let runtime = aoide_test_support::short_tmp("h1-rt");
    // The relay's own node name. It MUST differ from this test process's own
    // (`display::local_node_name()`): both boxes here are this ONE host, and
    // `mail send --to <peer>/…` treats a destination named like this box as a
    // local self-file. `AOIDE_A2A_NODE_NAME` is the existing override
    // `display::local_host_name` reads, so the relay child presents itself as
    // its own box — which is also what a real second host would do.
    const RELAY_NODE_NAME: &str = "h1-relay-box";
    for root in [&relay_root, &sender_root] {
        std::fs::create_dir_all(root.join("state")).unwrap();
        std::fs::create_dir_all(root.join("stage")).unwrap();
    }
    std::env::remove_var("AOIDE_SESSION_ID");
    std::env::remove_var("AOIDE_STATE_DIR");
    std::env::remove_var("AOIDE_STAGE_DIR");
    std::env::set_var("XDG_RUNTIME_DIR", &runtime);

    // The peer's name on BOTH sides: this box's own NODE name, which is what
    // the sender signs as (`X-Aoide-Node`) and what the relay's record for it
    // is kept under.
    let peer_name = aoide_storage::display::local_node_name();
    assert!(
        aoide_storage::node_store::valid_node_name(&peer_name),
        "this host's node name (`{peer_name}`) is not `valid_node_name`-shaped, so it cannot be \
         registered as the peer of a node record — a host fact this fixture cannot fake"
    );

    // ── The relay's own identity, minted FIRST: the sender's record for it
    // carries this key, and the relay signs its answer with it. ─────────────
    std::env::set_var("AOIDE_ROOT", &relay_root);
    std::env::set_var("STRILLIX_ROOT", &relay_root);
    let (relay_key, _) = aoide_storage::identity::load_or_mint().unwrap();
    let relay_pubkey = relay_key.info().pubkey_hex;
    drop(relay_key);

    // ── The sender's own identity, minted in its own root. ─────────────────
    std::env::set_var("AOIDE_ROOT", &sender_root);
    std::env::set_var("STRILLIX_ROOT", &sender_root);
    let (sender_key, _) = aoide_storage::identity::load_or_mint().unwrap();
    let sender_pubkey = sender_key.info().pubkey_hex;
    drop(sender_key);

    let port = free_port();
    let relay_url = format!("http://127.0.0.1:{port}/");

    // ── The two hand-written registries. The relay's record for the sender
    // points at a port nothing listens on, so the relay can never push to the
    // sender: what it holds for the sender leaves only by the sender's own poll
    // (the `poll` node's only inbound path). ────────────────────────────────
    std::env::set_var("AOIDE_ROOT", &relay_root);
    std::env::set_var("STRILLIX_ROOT", &relay_root);
    aoide_storage::node_store::save_nodes(&[verified_record(&peer_name, "http://127.0.0.1:1/", &sender_pubkey)])
        .unwrap();

    // The relay's `conductor` box, made the way an operator makes it. A far
    // deposit to a name with no box is refused `unknown-box`, so the send below
    // has nowhere to file without it.
    let made = dispatch(&cli_invocation(&["mail", "box", "add"], &["conductor"], &[]));
    assert_eq!(made.status, Status::Ok, "mail box add conductor on the relay: {}", made.message);

    std::env::set_var("AOIDE_ROOT", &sender_root);

    std::env::set_var("STRILLIX_ROOT", &sender_root);
    aoide_storage::node_store::save_nodes(&[verified_record(RELAY_NODE_NAME, &relay_url, &relay_pubkey)]).unwrap();

    // ── Raise the RELAY: a real `aoide mail serve` child with its own root. ─
    let mut child = std::process::Command::new(aoide_test_support::built_aoide_bin())
        .args(["mail", "serve", "--port", &port.to_string()])
        .env("AOIDE_ROOT", &relay_root)
        .env("STRILLIX_ROOT", &relay_root)
        .env("AOIDE_AUDIT_LOG", relay_root.join("log"))
        .env("AOIDE_A2A_NODE_NAME", RELAY_NODE_NAME)
        .env("XDG_RUNTIME_DIR", &runtime)
        .env_remove("AOIDE_STATE_DIR")
        .env_remove("AOIDE_STAGE_DIR")
        .env_remove("AOIDE_SESSION_ID")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("the built aoide binary runs");
    wait_for_tcp_up(&format!("127.0.0.1:{port}"));
    // The relay stamps its start second before it binds and refuses a frame stamped
    // at or before that second plus five. Wait for the clock to pass `ready + 5`, as
    // two_mesh_https.rs does, so the first signed call lands past the bound.
    let ready = common::whole_second();
    common::wait_past_start(ready);

    // ── Poll FIRST, send SECOND: a letter is sealed AT MINT when the
    // destination's binding is held, and this poll is what learns it
    // (exchange_bindings rides the top of every poll). ─────────────────────
    let poll_before = dispatch(&cli_invocation(&["mail", "poll"], &[RELAY_NODE_NAME], &[]));
    assert_eq!(poll_before.status, Status::Ok, "poll before send: {}", poll_before.message);
    assert_eq!(
        poll_before.data.as_ref().unwrap()["filed"], 0,
        "nothing is waiting yet: {}",
        poll_before.message
    );

    // ── The deposit: sealed at mint (the binding arrived above), signed, and
    // posted to the adapter's loopback listener. The relay's verified
    // `accepted` is the whole delivery: the send settles its own letter on that
    // same dial, and no receipt is minted for it (ruling 131). ─────────────
    let to = format!("{RELAY_NODE_NAME}/conductor");
    let send = dispatch(&cli_invocation(&["mail", "send"], &["h1 over https"], &[("to", &to)]));
    assert_eq!(send.status, Status::Ok, "mail send: {}", send.message);

    // ── The relay FILED it: the real proof is on the relay's own disk. ──────
    let relay_entries = filed_entries(&relay_root);
    assert_eq!(relay_entries.len(), 1, "the relay filed exactly one entry: {relay_entries:?}");
    assert_eq!(relay_entries[0]["envelope"]["text"], "h1 over https", "{relay_entries:?}");
    assert_eq!(relay_entries[0]["envelope"]["header"]["to"]["name"], "conductor", "{relay_entries:?}");
    assert_eq!(relay_entries[0]["envelope"]["header"]["from"]["node"], peer_name, "{relay_entries:?}");

    // The relay's own audit names the ADAPTER as the listener that took it —
    // tunnel traffic versus door traffic is told apart by that word, not by
    // the origin (a front dials this box from loopback too).
    let relay_log = std::fs::read_to_string(relay_root.join("log")).unwrap_or_default();
    assert!(
        relay_log.contains("a2a.aoide/mailDeposit") && relay_log.contains("via mail-adapter"),
        "the adapter's own audit line, naming its listener: {relay_log}"
    );

    // ── The relay's verified `accepted` delivered the letter: no receipt
    // letter comes back, and the spool toward the relay is empty (ruling 131). ──
    let sender_entries = filed_entries(&sender_root);
    assert!(sender_entries.is_empty(), "no receipt is filed here: {sender_entries:?}");

    let left = aoide_storage::outbox::list_entries(RELAY_NODE_NAME).unwrap_or_default();
    assert!(
        left.is_empty(),
        "the verified answer delivered the letter — nothing is left toward the relay: {left:?}"
    );

    // ── A second poll finds nothing: no duplicate filing, no second receipt
    // (the poll is idempotent by construction — it is a READ of the relay's
    // spool, and the send's own delivery already closed this entry). ──────
    let poll_after = dispatch(&cli_invocation(&["mail", "poll"], &[RELAY_NODE_NAME], &[]));
    assert_eq!(poll_after.status, Status::Ok, "poll after send: {}", poll_after.message);
    assert_eq!(
        poll_after.data.as_ref().unwrap()["filed"], 0,
        "the answer was already taken on the send's own dial: {}",
        poll_after.message
    );
    assert!(filed_entries(&sender_root).is_empty(), "no second receipt on a re-poll");

    // ── Teardown: the child goes first, so no serve thread outlives this. ──
    let _ = child.kill();
    let _ = child.wait();
    restore("AOIDE_ROOT", saved_root);
    restore("AOIDE_STATE_DIR", saved_state);
    restore("AOIDE_STAGE_DIR", saved_stage);
    restore("AOIDE_AUDIT_LOG", saved_audit);
    restore("XDG_RUNTIME_DIR", saved_runtime);
    restore("AOIDE_SESSION_ID", saved_session);
    let _ = std::fs::remove_dir_all(&relay_root);
    let _ = std::fs::remove_dir_all(&sender_root);
    let _ = std::fs::remove_dir_all(&runtime);
}
