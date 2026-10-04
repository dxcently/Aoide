//! `aoide node address`: one existing node record moved to another address
//! through the dispatcher, so the audit line is the dispatcher's own.

use aoide::dispatch::{dispatch, Invocation};
use aoide_protocol::output::Status;
use aoide_protocol::Door;
use aoide_storage::node_store::{self, Grants, Node};

fn address(name: &str, address: &str, log: &std::path::Path) -> aoide_protocol::output::Outcome {
    dispatch(&Invocation {
        path: vec!["node".to_string(), "address".to_string()],
        args: vec![name.to_string(), address.to_string()],
        flags: [("audit-log".to_string(), log.to_string_lossy().into_owned())].into_iter().collect(),
        door: Door::Cli,
    })
}

fn paired(name: &str, url: &str, via: Option<&str>) -> Node {
    Node {
        name: name.to_string(),
        url: url.to_string(),
        autogate: false,
        token_file: None,
        bearer_secret: None,
        hub: false,
        pubkey: Some("ab".repeat(32)),
        verified: true,
        grants: node_store::grants_in("home", &["message", "read"]),
        narrowed: Grants::new(),
        via: via.map(str::to_string),
        added_at: "2026-09-26T00:00:00Z".to_string(),
    }
}

fn only_node() -> Node {
    node_store::load_nodes().into_iter().next().expect("the registry holds the node")
}

#[test]
fn a_node_moves_to_each_address_form_and_keeps_its_trust() {
    let _lock = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let (_env, root) = aoide_test_support::isolated_mail_root("node-address");
    let log = root.join("log");
    node_store::save_nodes(&[paired("sakaki", "http://192.168.1.10:8710/", Some("ssh://khoa@192.168.1.10"))]).unwrap();

    let out = address("sakaki", "https://aoide.necoconeco.net", &log);
    assert_eq!(out.status, Status::Ok, "{}", out.message);
    let node = only_node();
    assert_eq!(node.url, "https://aoide.necoconeco.net");
    assert_eq!(node.via, None, "an https address names no tunnel");
    assert!(node.verified && node.pubkey.is_some(), "where a node is reached never touches who it is");
    assert_eq!(node.grant("home"), ["message".to_string(), "read".to_string()]);

    let again = address("sakaki", "https://aoide.necoconeco.net", &log);
    assert_eq!(again.status, Status::Ok, "{}", again.message);
    assert_eq!(again.data.as_ref().unwrap()["changed"], false, "the same address is a no-op");

    let ssh = address("sakaki", "ssh://khoa@192.168.1.10", &log);
    assert_eq!(ssh.status, Status::Ok, "{}", ssh.message);
    let node = only_node();
    assert_eq!(node.via.as_deref(), Some("ssh://khoa@192.168.1.10"));
    assert!(node.url.starts_with("http://127.0.0.1:"), "dialled at the far door's loopback form: {}", node.url);

    let poll = address("sakaki", "poll", &log);
    assert_eq!(poll.status, Status::Ok, "{}", poll.message);
    let node = only_node();
    assert!(node.never_dialled() && node.via.is_none());

    let audit = std::fs::read_to_string(&log).unwrap();
    assert!(
        audit.lines().any(|l| l.contains("node.address") && l.contains("node `sakaki` is now at `poll`")),
        "the dispatcher audited the change: {audit}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_scheme_outside_the_grammar_and_an_unknown_node_are_refused_and_write_nothing() {
    let _lock = aoide_test_support::env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let (_env, root) = aoide_test_support::isolated_mail_root("node-address-refused");
    let log = root.join("log");
    node_store::save_nodes(&[paired("sakaki", "http://192.168.1.10:8710/", None)]).unwrap();

    let bad = address("sakaki", "http://aoide.necoconeco.net", &log);
    assert_eq!(bad.status, Status::Error, "{}", bad.message);
    assert_eq!(bad.data.as_ref().unwrap()["reason"], "invalid-address");
    assert_eq!(only_node().url, "http://192.168.1.10:8710/", "a refused address leaves the record as it was");

    let ghost = address("nobody", "poll", &log);
    assert_eq!(ghost.status, Status::Error, "{}", ghost.message);
    assert_eq!(ghost.data.as_ref().unwrap()["reason"], "unknown-node");
    assert_eq!(node_store::load_nodes().len(), 1);

    let audit = std::fs::read_to_string(&log).unwrap();
    assert_eq!(audit.lines().filter(|l| l.contains("node.address")).count(), 2, "both refusals are audited too: {audit}");
    let _ = std::fs::remove_dir_all(&root);
}
