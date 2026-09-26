//! The A2A client-side wire builders: resolving a remote AgentCard URL, and
//! building the outbound `message/send` JSON-RPC request body aoide POSTs
//! when reaching a registered node.
//!
//! Moved from root `src/a2a.rs` (Phase 4b restructure,
//! docs/architecture/PACKAGE-LAYOUT.md — the CLIENT-side region of that file;
//! the server-side JSON-RPC/HTTP door, AgentCard building from the LOCAL
//! command registry, and everything else stays in root, Phase 4c's job).
//! Re-exported at the old `crate::a2a::{resolve_card_url,
//! build_message_send_body}` path so every existing caller is untouched.

use aoide_protocol::wire::{JsonRpcRequest, Message, MessageSendParams, Part};
use serde_json::{json, Value};

/// Resolve the AgentCard URL to GET from a user-supplied `url`: if it already
/// points at a card (`…/agent-card.json`) use it verbatim, otherwise treat it
/// as an origin and append the well-known path. Pure.
pub fn resolve_card_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.ends_with("agent-card.json") {
        trimmed.to_string()
    } else {
        format!("{}/.well-known/agent-card.json", trimmed.trim_end_matches('/'))
    }
}

/// Build the JSON-RPC `message/send` request body aoide POSTs when DRIVING a
/// registered node (the outbound half of the bidirectional link). Mirrors
/// the inbound shape the server's `parse_message_send_params` (`src/a2a.rs`)
/// reads. Pure — the caller generates `message_id`, so the body stays
/// deterministic in tests.
///
/// `context_id` threads a target session id for a NODE send (messaging plan
/// P-C3: `graph send --to <node>/<query>` resolves a remote sessionId and
/// hands it here so the receiving node's `message_send` Inject arm can find
/// it — see `crates/server/src/a2a.rs::decide_send_action`). Every OTHER
/// caller (today: `node spawn`, addressing a node with no aoide sessionId
/// to target) passes `None`.
///
/// `from_session` is this caller's OWN session id — the remote-parent claim
/// the receiving door stamps onto the spawned record (P-RSA; CONTRACTS.md
/// §6, [`aoide_protocol::wire::FROM_SESSION_KEY`]). It rides INSIDE the
/// signed body, so the claim is bound to the key that signed it; the door
/// ignores it on every rung but the signature one. `None` leaves `metadata`
/// absent entirely — byte-identical to before this parameter existed.
///
/// `task` (P-RSA S10) is the slug the far node should run the child under
/// ([`aoide_protocol::wire::TASK_KEY`]) — a SPAWN-only directive: it is what
/// makes the remote child a managed run there (task mailbox, exit report,
/// `session watch`'s task view), and its absence is the plain headless
/// session. Both keys ride ONE `message.metadata` object when both are given
/// (the door reads each at its own spot), and `None` for both still leaves
/// `metadata` absent entirely.
pub fn build_message_send_body(
    text: &str,
    message_id: &str,
    context_id: Option<&str>,
    from_session: Option<&str>,
    task: Option<&str>,
) -> Value {
    // One object, built by insertion: a second `json!` for the task would be a
    // second metadata spelling, and last-write-wins would silently drop one
    // key the day both are set.
    let metadata = {
        let mut map = serde_json::Map::new();
        if let Some(id) = from_session {
            map.insert(
                aoide_protocol::wire::FROM_SESSION_KEY.to_string(),
                Value::String(id.to_string()),
            );
        }
        if let Some(slug) = task {
            map.insert(
                aoide_protocol::wire::TASK_KEY.to_string(),
                Value::String(slug.to_string()),
            );
        }
        (!map.is_empty()).then_some(Value::Object(map))
    };
    let params = MessageSendParams {
        message: Message {
            role: "user".to_string(),
            parts: vec![Part {
                kind: "text".to_string(),
                text: Some(text.to_string()),
                extra: Default::default(),
            }],
            message_id: Some(message_id.to_string()),
            context_id: context_id.map(str::to_string),
            metadata,
        },
    };
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: json!(1),
        method: "message/send".to_string(),
        params: serde_json::to_value(&params).expect("MessageSendParams always serializes"),
    };
    serde_json::to_value(&req).expect("JsonRpcRequest always serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_card_url_appends_well_known_unless_already_a_card() {
        assert_eq!(
            resolve_card_url("http://127.0.0.1:8710"),
            "http://127.0.0.1:8710/.well-known/agent-card.json"
        );
        // Trailing slash is not doubled.
        assert_eq!(
            resolve_card_url("http://127.0.0.1:8710/"),
            "http://127.0.0.1:8710/.well-known/agent-card.json"
        );
        // An explicit card URL is used verbatim.
        assert_eq!(
            resolve_card_url("http://h/.well-known/agent-card.json"),
            "http://h/.well-known/agent-card.json"
        );
    }

    // ── The outbound message/send request-body builder (pure) ────────────────
    //
    // The original test also round-tripped this body through the server's
    // `parse_message_send_params` (`src/a2a.rs`) to prove the shapes match —
    // that half stayed in root `a2a.rs`'s own test module (Phase 4b restructure)
    // since `parse_message_send_params` is server-side inbound parsing, not
    // moving to this crate.

    #[test]
    fn build_message_send_body_matches_the_jsonrpc_shape() {
        let body = build_message_send_body("hello there", "mid-123", None, None, None);
        assert_eq!(body["jsonrpc"], "2.0");
        assert_eq!(body["id"], 1);
        assert_eq!(body["method"], "message/send");
        let msg = &body["params"]["message"];
        assert_eq!(msg["role"], "user");
        assert_eq!(msg["messageId"], "mid-123");
        assert!(msg.get("contextId").is_none(), "None stays absent, not null-present");
        let parts = msg["parts"].as_array().unwrap();
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0]["kind"], "text");
        assert_eq!(parts[0]["text"], "hello there");
    }

    #[test]
    fn build_message_send_body_threads_a_context_id_when_given() {
        // P-C3: a node-targeted send carries the resolved remote sessionId
        // as `contextId` so the receiving node's Inject arm can find it.
        let body = build_message_send_body("hello there", "mid-123", Some("sess-9"), None, None);
        assert_eq!(body["params"]["message"]["contextId"], "sess-9");
    }

    #[test]
    fn build_message_send_body_writes_the_task_slug_where_the_door_reads_it() {
        // P-RSA S10: `aoide node spawn --task <slug>` makes the remote child a
        // managed run. The key rides the same `message.metadata` object as the
        // caller claim, and BOTH survive when both are given — one object, two
        // insertions, never a second metadata spelling that drops one.
        let tasked = build_message_send_body("hi", "mid-task-1", None, None, Some("fix-flaky"));
        assert_eq!(
            tasked["params"]["message"]["metadata"][aoide_protocol::wire::TASK_KEY],
            "fix-flaky"
        );
        assert_eq!(
            tasked["params"]["message"]["metadata"].as_object().unwrap().len(),
            1,
            "the slug is the only key when there is no claim"
        );

        let both =
            build_message_send_body("hi", "mid-task-2", None, Some("conduct-1-2"), Some("fix-flaky"));
        let metadata = both["params"]["message"]["metadata"].as_object().unwrap();
        assert_eq!(metadata.len(), 2, "{metadata:?}");
        assert_eq!(metadata[aoide_protocol::wire::FROM_SESSION_KEY], "conduct-1-2");
        assert_eq!(metadata[aoide_protocol::wire::TASK_KEY], "fix-flaky");

        assert!(
            build_message_send_body("hi", "mid-task-3", None, None, None)["params"]["message"]
                .get("metadata")
                .is_none(),
            "naming no task leaves metadata absent, exactly as before the key existed"
        );
    }

    #[test]
    fn build_message_send_body_writes_the_caller_claim_under_one_key() {
        // P-RSA S2: the caller's own session id rides INSIDE the body, so the
        // receiving door's stamp is bound to the signature that covers it. No
        // claim at all leaves `metadata` absent — not an empty object.
        let claiming = build_message_send_body("hi", "mid-1", None, Some("conduct-17991-2"), None);
        assert_eq!(
            claiming["params"]["message"]["metadata"][aoide_protocol::wire::FROM_SESSION_KEY],
            "conduct-17991-2"
        );
        assert_eq!(
            claiming["params"]["message"]["metadata"].as_object().unwrap().len(),
            1,
            "the claim is the only metadata key this builder writes when no task is named"
        );
        let silent = build_message_send_body("hi", "mid-1", None, None, None);
        assert!(
            silent["params"]["message"].get("metadata").is_none(),
            "no claim stays absent, not null-present"
        );
    }

    #[test]
    fn the_signed_body_digest_covers_the_caller_claim() {
        // P-RSA S2: `sign_headers_for_node`'s digest is the WHOLE body bytes,
        // so mutating `aoide/from` after signing must break verification —
        // the property that makes the claim signed rather than decorative.
        // Real ed25519, the same `aoide_storage::wire_auth` pair the server's
        // own verifier uses. `mint_ephemeral` — no disk, no live identity.
        let kp = aoide_storage::identity::mint_ephemeral().unwrap();
        let pubkey_hex = kp.info().pubkey_hex;
        let signed = build_message_send_body("hi", "mid-7", None, Some("conduct-parent-1"), None);
        let signed_bytes = serde_json::to_vec(&signed).unwrap();
        let timestamp = aoide_storage::time::iso_utc_from_epoch(1_800_000_000);
        let canonical = aoide_storage::wire_auth::canonical_string(
            "POST",
            "/",
            &timestamp,
            "nonce-claim-1",
            &signed_bytes,
        );
        let signature = aoide_storage::wire_auth::sign_hex(&kp, canonical.as_bytes());
        assert!(aoide_storage::wire_auth::verify_signature_hex(
            &pubkey_hex,
            canonical.as_bytes(),
            &signature
        ));

        let mut tampered = signed.clone();
        tampered["params"]["message"]["metadata"][aoide_protocol::wire::FROM_SESSION_KEY] =
            serde_json::json!("conduct-parent-2");
        let tampered_bytes = serde_json::to_vec(&tampered).unwrap();
        let tampered_canonical = aoide_storage::wire_auth::canonical_string(
            "POST",
            "/",
            &timestamp,
            "nonce-claim-1",
            &tampered_bytes,
        );
        assert!(
            !aoide_storage::wire_auth::verify_signature_hex(
                &pubkey_hex,
                tampered_canonical.as_bytes(),
                &signature
            ),
            "a swapped parent claim must not verify against the signature over the original body"
        );
    }
}
