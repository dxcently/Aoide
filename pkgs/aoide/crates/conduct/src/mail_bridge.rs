//! The mail drain's crate-DAG bridge (messaging plan P-M2, architect's
//! ruling 1: "spool in storage, wire lane in client, bridge through
//! conduct"). `aoide-server` depends on `aoide-client` only as a
//! **dev**-dependency (a production edge there is refused by the manifest,
//! not just a comment) — `aoide-conduct` already carries a REAL
//! `aoide-client` edge (`graph::who` calls `aoide_client::commands::
//! pull_node_live`; `graph::resurrect` calls `aoide_client::commands::
//! spawn_on_node`), so the daemon tick and the door both reach the drain
//! through THIS crate rather than either depending on `aoide-client`
//! directly. No logic of its own lives here — both functions below are
//! thin passthroughs; the drain itself is `aoide_client::mail_wire`'s.

/// Drain one node's outbox once — a passthrough to
/// [`aoide_client::mail_wire::drain_node`]. Called by the door
/// (`aoide-server::a2a::mail_deposit`, best-effort, right after filing a
/// letter or re-filing a duplicate's ack) and by [`drain_all`] below (the
/// daemon tick's sweep).
pub fn drain_node(name: &str) -> Result<(), String> {
    aoide_client::mail_wire::drain_node(name)
}

/// Everything that follows a deposit outcome — a passthrough to
/// [`aoide_client::mail_wire::settle_deposit`]. Called by the door
/// (`aoide-server::a2a::mail_deposit`) once per request, after
/// `aoide_storage::mail::deposit` has already released its own lock. The
/// SAME function receives the envelopes a poll hands over, so the two doors
/// into the mailbase cannot drift on what a filed letter or receipt does.
pub fn settle_deposit(
    envelope: &aoide_storage::mail::Envelope,
    outcome: &aoide_storage::mail::DepositOutcome,
) {
    aoide_client::mail_wire::settle_deposit(envelope, outcome)
}

/// Drain every node with a non-empty outbox — the daemon tick's own call,
/// mirroring `daemon::run_internal_reap`'s "reach a sibling crate's
/// handler on its tick" shape. One node's drain returning `Err` (a genuine
/// local I/O failure — never an ordinary unreachable-node outcome, which
/// `drain_node` records in the entry/link state instead, see its own doc)
/// does not stop the sweep from reaching the rest.
pub fn drain_all() -> Result<(), String> {
    for node in aoide_storage::outbox::nodes_with_outbox()? {
        let _ = drain_node(&node);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_storage::node_store::Node;
    use aoide_storage::outbox::OutboxEntry;

    fn root(tag: &str) -> (aoide_test_support::EnvSaver, std::path::PathBuf) {
        aoide_test_support::isolated_mail_root(tag)
    }

    /// This test process's identity key: what the fake door signs its answers
    /// with, and the key [`keyed_node`] holds on file.
    fn own_key() -> aoide_storage::identity::Keypair {
        aoide_storage::identity::load_or_mint().unwrap().0
    }

    /// A VERIFIED record for `name` under [`own_key`], granted `message` in the
    /// home mesh: a node this box can dial, sign for and check an answer from.
    fn keyed_node(name: &str) -> Node {
        Node {
            name: name.to_string(),
            url: "http://127.0.0.1:1".to_string(), // nothing listens here
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: Some(own_key().info().pubkey_hex),
            verified: true,
            grants: aoide_storage::node_store::grants_in("home", &["message"]),
            narrowed: aoide_storage::node_store::Grants::new(),
            via: None,
            added_at: "2026-09-07T00:00:00Z".to_string(),
        }
    }

    /// A minimal fake `aoide/mailDeposit` door — mirrors
    /// `aoide_client::mail_wire`'s own module-local copy (a tiny
    /// per-module fixture, not a reach across crates). It signs its answer
    /// with [`own_key`], the way a verified door does.
    fn fake_deposit_server(body: &'static str) -> (std::net::TcpListener, u16) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let accepter = listener.try_clone().unwrap();
        std::thread::spawn(move || {
            use std::io::{Read, Write};
            loop {
                let Ok((mut stream, _)) = accepter.accept() else { break };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    continue;
                }
                let request = String::from_utf8_lossy(&buf[..n]).to_string();
                let _ = stream.write_all(signed_reply(&request, &own_key(), 200, body).as_bytes());
            }
        });
        (listener, port)
    }

    /// The full HTTP answer a door sends to `request`, signed with `key` over the
    /// status and the exact body, bound to the request's own signature and nonce.
    fn signed_reply(request: &str, key: &aoide_storage::identity::Keypair, status: u16, body: &str) -> String {
        use aoide_storage::wire_auth::{sign_answer, HEADER_ANSWER_SIGNATURE, HEADER_NONCE, HEADER_SIGNATURE};
        let request_signature = aoide_test_support::raw_header(request, HEADER_SIGNATURE).expect("the request is signed");
        let request_nonce = aoide_test_support::raw_header(request, HEADER_NONCE).expect("the request is signed");
        let signature = sign_answer(key, &request_signature, &request_nonce, status, body.as_bytes());
        format!(
            "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{HEADER_ANSWER_SIGNATURE}: {signature}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    /// `drain_all` walks every node with a non-empty outbox independently
    /// (this module's own doc: one node's `Err` never stops the sweep) — a
    /// permanently dead LINK is a much narrower failure than an `Err`
    /// (`drain_node` never returns one for an unreachable node, see its own
    /// doc), so this pins the stronger, more common case directly: one
    /// dead node's entries staying stuck must never keep another node's
    /// perfectly healthy entries from draining in the SAME sweep.
    #[test]
    fn one_dead_nodes_entries_never_block_another_nodes_drain() {
        let _g = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let (_env, dir) = root("drain-all-one-dead-node");

        let (listener, port) = fake_deposit_server(
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"accepted"}}"#,
        );
        let mut healthy = keyed_node("healthy");
        healthy.url = format!("http://127.0.0.1:{port}/");
        aoide_storage::node_store::save_nodes(&[keyed_node("dead"), healthy, keyed_node("origin-node")]).unwrap();

        let dead_env = aoide_storage::mail::mint_outbound_letter("alice", "dead", "bob", "hi").unwrap();
        aoide_storage::outbox::write_entry("dead", &OutboxEntry::fresh(dead_env)).unwrap();

        let to = aoide_storage::mail::Address { node: "origin-node".to_string(), name: "bob".to_string() };
        let healthy_env = aoide_storage::mail::mint_ack("alice", to, "acked-msgid").unwrap();
        aoide_storage::outbox::write_entry("healthy", &OutboxEntry::fresh(healthy_env)).unwrap();

        // The receipts are addressed to origin-node, a relayed entry: they are
        // sealed to origin-node or withheld, so its binding must be known.
        aoide_storage::seal::learn_binding("origin-node", &aoide_storage::seal::publish_binding().unwrap()).unwrap();

        drain_all().unwrap();

        let dead_remaining = aoide_storage::outbox::list_entries("dead").unwrap();
        assert_eq!(dead_remaining.len(), 1, "the dead node's entry survives a transport failure");

        let healthy_remaining = aoide_storage::outbox::list_entries("healthy").unwrap();
        assert!(
            healthy_remaining.is_empty(),
            "the healthy node's receipt must still retire even though 'dead' drained first"
        );

        drop(listener);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
