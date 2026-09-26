//! The node-federation client-side wire builders/parsers (CONTRACTS.md §7):
//! the outbound `aoide/graphSummary` JSON-RPC request body, and parsing a
//! node's response into a `state/node-cache/<name>.json` entry
//! (`aoide-storage::node_store::NodeCacheEntry`).
//!
//! Mirrors `wire.rs`'s separation exactly — the pure wire shapes live here,
//! the curl transport + CLI commands (`node add|remove|pull|status`) live
//! in `commands.rs`, same split `wire.rs`/`commands.rs` hold throughout.
//!
//! The pairing ceremony's wire shapes (P-P2, CONTRACTS.md §6) join the same
//! split: [`build_pair_request_body`]/[`parse_pair_request_response`] for
//! the requester's `aoide/pairRequest` call (carrying a COMMITMENT to its
//! own nonce, never the nonce itself — the commit-then-reveal fix,
//! `aoide_storage::pairing`'s module doc), [`build_pair_reveal_body`]/
//! [`check_pair_reveal_response`] for the requester's immediate follow-up
//! `aoide/pairReveal` call (same `pair` invocation, two
//! sequential POSTs), and [`build_pair_poll_body`]/[`parse_pair_poll_response`]
//! for the requester's `aoide/pairPoll` call (Design A, task #119 — REPLACES
//! the old `aoide/pairApprove` reverse callback: the requester POLLS the
//! approver's door over the SAME forward dial the request/reveal already
//! used, rather than the approver ever dialing back) — the server-side
//! handlers live in `aoide-server::a2a` (`pair_request`/`pair_reveal`/
//! `pair_poll`), never duplicated here; this module only builds/parses the
//! JSON-RPC envelope either side of that wire.
//!
//! The watch frame's own wire shapes (P-RSA S7) join the same split:
//! [`build_task_get_frame_request`] asks a node for one session's frame over
//! `tasks/get` + `metadata["aoide/frame"]`, and [`parse_frame_response`] hands
//! back the frame JSON the far door put in its `frame` artifact — the shape
//! `aoide_conduct::graph::Frame` owns, never a second one here.

use aoide_protocol::wire::JsonRpcRequest;
use aoide_storage::node_store::NodeCacheEntry;
use serde_json::{json, Value};

/// Build the JSON-RPC `aoide/graphSummary` request body `node pull` POSTs to
/// a registered node's endpoint. No params — the method takes none
/// (CONTRACTS.md §7). Pure.
pub fn build_graph_summary_request() -> Value {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: json!(1),
        method: "aoide/graphSummary".to_string(),
        params: json!({}),
    };
    serde_json::to_value(&req).expect("JsonRpcRequest always serializes")
}

/// Parse a node's `aoide/graphSummary` JSON-RPC response into a FRESH
/// [`NodeCacheEntry`] for `name`. Requires `result.schemaVersion == "0"` and
/// a `result.graph` object; `result.instance` rides through verbatim when
/// present. Pure — the HTTP fetch itself is the caller's (`commands.rs`) job.
pub fn parse_graph_summary_response(resp: &Value, name: &str, fetched_at: &str) -> Result<NodeCacheEntry, String> {
    if let Some(err) = resp.get("error") {
        let detail = err.get("message").and_then(Value::as_str).unwrap_or("(no message)");
        return Err(format!("node returned an error: {detail}"));
    }
    let result = resp
        .get("result")
        .ok_or_else(|| "response has no `result`".to_string())?;
    let schema_version = result.get("schemaVersion").and_then(Value::as_str).unwrap_or("");
    if schema_version != "0" {
        return Err(format!("unsupported schemaVersion `{schema_version}` (expected \"0\")"));
    }
    let graph = result
        .get("graph")
        .filter(|g| g.is_object())
        .ok_or_else(|| "response has no `graph` object".to_string())?
        .clone();
    Ok(NodeCacheEntry {
        schema_version: "0".to_string(),
        name: name.to_string(),
        instance: result.get("instance").cloned(),
        graph: Some(graph),
        fetched_at: Some(fetched_at.to_string()),
        stale: false,
        last_error: None,
    })
}

// ── The pairing ceremony (P-P2, CONTRACTS.md §6) ─────────────────────────

/// Build the JSON-RPC `aoide/pairRequest` body `pair` POSTs to
/// the approver's door: this instance's own public key, its own SELF-CLAIMED
/// instance name (`aoide_storage::display::local_host_name`'s chain — the
/// approver's `pair <id>` records this instance under this exact
/// name, so it must name THIS box, never the caller's nickname for the
/// approver; the live yomi↔sakaki ceremony 2026-08-26 caught the crossed
/// reading), a COMMITMENT to a fresh nonce (`commit_hex` —
/// `aoide_storage::pairing::derive_commit(pubkey_hex, nonce_hex)`, the
/// nonce itself stays local until [`build_pair_reveal_body`]'s follow-up
/// call), its own advertised A2A door URL (where the later reveal and
/// approval callbacks are delivered), and OPTIONALLY `self_via` — this
/// instance's own reach-back hop claim (`ssh://[user@]host`, P-PV1, task
/// #131). The wire only ever sees `self_url` as the requester's door; when
/// that door is reached over an ssh tunnel, the approver OBSERVES the
/// connection arriving from loopback and cannot derive a working `via` from
/// the connection itself — `self_via` is the requester's own SELF-ASSERTED
/// claim of the hop that reaches it back, the same trust class as
/// `self_url` (a transport marker only; trust stays in pubkeys + SAS, never
/// this field). Omitted (`None`) when the caller has no such claim, so an
/// old approver — which never looks for `selfVia` at all — sees exactly the
/// shape it always has. Pure.
pub fn build_pair_request_body(pubkey_hex: &str, self_name: &str, commit_hex: &str, self_url: &str, self_via: Option<&str>) -> Value {
    let mut params = json!({ "pubkeyHex": pubkey_hex, "name": self_name, "commitHex": commit_hex, "url": self_url });
    if let Some(via) = self_via {
        params["selfVia"] = json!(via);
    }
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: json!(1),
        method: "aoide/pairRequest".to_string(),
        params,
    };
    serde_json::to_value(&req).expect("JsonRpcRequest always serializes")
}

/// The approver's synchronous `aoide/pairRequest` acknowledgement — its own
/// public identity plus a freshly-minted nonce, everything the requester
/// needs to derive its own copy of the SAS with no further round trip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairRequestAck {
    pub id: String,
    pub pubkey_hex: String,
    pub nonce_hex: String,
    pub expires_at: String,
}

/// Parse the `aoide/pairRequest` response into a [`PairRequestAck`]. Pure.
pub fn parse_pair_request_response(resp: &Value) -> Result<PairRequestAck, String> {
    if let Some(err) = resp.get("error") {
        let detail = err.get("message").and_then(Value::as_str).unwrap_or("(no message)");
        return Err(format!("the node returned an error: {detail}"));
    }
    let result = resp.get("result").ok_or_else(|| "response has no `result`".to_string())?;
    let id = result
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "response has no `id`".to_string())?
        .to_string();
    let pubkey_hex = result
        .get("pubkeyHex")
        .and_then(Value::as_str)
        .ok_or_else(|| "response has no `pubkeyHex`".to_string())?
        .to_string();
    let nonce_hex = result
        .get("nonceHex")
        .and_then(Value::as_str)
        .ok_or_else(|| "response has no `nonceHex`".to_string())?
        .to_string();
    let expires_at = result.get("expiresAt").and_then(Value::as_str).unwrap_or("").to_string();
    Ok(PairRequestAck { id, pubkey_hex, nonce_hex, expires_at })
}

/// Build the JSON-RPC `aoide/pairReveal` body the REQUESTER's `pair`
/// POSTs immediately after `aoide/pairRequest` (same invocation,
/// two sequential POSTs) — `id` is the id the approver's `aoide/pairRequest`
/// response returned; `nonce_hex` is the nonce `commit_hex` already
/// committed to. Pure.
pub fn build_pair_reveal_body(id: &str, nonce_hex: &str) -> Value {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: json!(1),
        method: "aoide/pairReveal".to_string(),
        params: json!({ "id": id, "nonceHex": nonce_hex }),
    };
    serde_json::to_value(&req).expect("JsonRpcRequest always serializes")
}

/// Build the JSON-RPC `aoide/pairPoll` body the REQUESTER's `pair
/// <id>` POSTs to the APPROVER's door (Design A, task #119 — REPLACES
/// the old `aoide/pairApprove` reverse callback), asking "has this been
/// approved yet?" `id` is the SAME id `aoide/pairRequest` returned;
/// `timestamp_iso`/`nonce_hex`/`signature_hex` are the requester's own
/// self-contained signature over
/// `aoide_storage::wire_auth::canonical_string("PAIRPOLL", id, timestamp_iso,
/// nonce_hex, &[])`, signed with the requester's OWN identity — never P-P4's
/// header-based scheme, which needs a verified node record that doesn't
/// exist yet at poll time (`aoide-server::a2a::pair_poll`'s own doc has the
/// full bootstrapping reasoning). Pure.
pub fn build_pair_poll_body(id: &str, timestamp_iso: &str, nonce_hex: &str, signature_hex: &str) -> Value {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: json!(1),
        method: "aoide/pairPoll".to_string(),
        params: json!({ "id": id, "timestampIso": timestamp_iso, "nonceHex": nonce_hex, "signatureHex": signature_hex }),
    };
    serde_json::to_value(&req).expect("JsonRpcRequest always serializes")
}

/// `aoide/pairPoll`'s two possible outcomes (never a third — the door's own
/// existence-oracle discipline collapses "unknown id"/"wrong signer"/"not
/// yet approved" into the SAME [`PairPollStatus::Pending`]). `Approved`
/// carries the approver's own public key, freshly re-derived on that side —
/// the requester's caller ([`crate::commands::approve_outbound`]) is what
/// binds this to the transcript it already holds
/// (`aoide_storage::pairing::mark_outbound_awaiting_confirm`'s own mismatch
/// check), never trusted blindly here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairPollStatus {
    Pending,
    Approved { pubkey_hex: String },
}

/// Parse the `aoide/pairPoll` response. Pure.
pub fn parse_pair_poll_response(resp: &Value) -> Result<PairPollStatus, String> {
    if let Some(err) = resp.get("error") {
        let detail = err.get("message").and_then(Value::as_str).unwrap_or("(no message)");
        return Err(format!("the node refused the poll: {detail}"));
    }
    let result = resp.get("result").ok_or_else(|| "response has no `result`".to_string())?;
    match result.get("status").and_then(Value::as_str) {
        Some("approved") => {
            let pubkey_hex = result
                .get("pubkeyHex")
                .and_then(Value::as_str)
                .ok_or_else(|| "an `approved` poll response has no `pubkeyHex`".to_string())?
                .to_string();
            Ok(PairPollStatus::Approved { pubkey_hex })
        }
        Some("pending") => Ok(PairPollStatus::Pending),
        other => Err(format!("unrecognized poll status: {other:?}")),
    }
}

// ── the watch frame: `tasks/get` + `metadata["aoide/frame"]` ───────────────

/// A frame read that failed. `code` is the FAR door's own JSON-RPC code when
/// it refused — `-32011` is the output-read gate (CONTRACTS.md §6) — and
/// `None` for a transport, HTTP or parse failure; `message` is the door's own
/// text verbatim, never translated, so the caller can tell a refused read
/// from an unreachable node by the code alone instead of by matching prose
/// (the same discipline `SpawnNodeError::reason` holds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameReadError {
    pub code: Option<i64>,
    pub message: String,
}

impl std::fmt::Display for FrameReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.code {
            Some(code) => write!(f, "{} (JSON-RPC {code})", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

/// Build the JSON-RPC `tasks/get` body the WATCHER POSTs to ask a node for one
/// session's watch frame (P-RSA S7, CONTRACTS.md §6): `params.id` is the
/// remote `sessionId`, and `metadata["aoide/frame"]`'s own `tail` is the
/// output-line window. Only `params.metadata` counts on the far side —
/// `message.metadata`, the `message/send` fallback other methods accept, is
/// never read by this arm — so this builder puts the key exactly where the
/// door looks. `aoide_protocol::wire::a2a::FRAME_KEY` is the ONE spelling of
/// that key, shared with the door that serves it. Pure.
pub fn build_task_get_frame_request(id: &str, tail: u64) -> Value {
    let req = JsonRpcRequest {
        jsonrpc: "2.0".to_string(),
        id: json!(1),
        method: "tasks/get".to_string(),
        params: json!({ "id": id, "metadata": { aoide_protocol::wire::a2a::FRAME_KEY: { "tail": tail } } }),
    };
    serde_json::to_value(&req).expect("JsonRpcRequest always serializes")
}

/// Parse a `tasks/get` frame response into the frame JSON itself — the `data`
/// of the artifact `artifactId: "frame"`, which is what
/// `aoide_conduct::graph::Frame` deserializes from. The frame's own SHAPE is
/// not this function's business: this side reads the envelope (a JSON-RPC
/// `error`, a missing `result`, a missing artifact) and hands the `data` over
/// unread, so the type that owns the frame is the only thing that parses it.
/// Pure. A JSON-RPC error keeps the door's own code in
/// [`FrameReadError::code`].
pub fn parse_frame_response(resp: &Value) -> Result<Value, FrameReadError> {
    if let Some(err) = resp.get("error") {
        return Err(FrameReadError {
            code: err.get("code").and_then(Value::as_i64),
            message: err.get("message").and_then(Value::as_str).unwrap_or("(no message)").to_string(),
        });
    }
    let plain = |message: &str| FrameReadError { code: None, message: message.to_string() };
    let result = resp.get("result").ok_or_else(|| plain("response has no `result`"))?;
    let artifacts = result
        .get("artifacts")
        .and_then(Value::as_array)
        .ok_or_else(|| plain("response carries no `artifacts` — the frame was not asked for"))?;
    let artifact = artifacts
        .iter()
        .find(|a| a.get("artifactId").and_then(Value::as_str) == Some(aoide_protocol::wire::a2a::FRAME_ARTIFACT_ID))
        .ok_or_else(|| plain("response carries no `frame` artifact"))?;
    let part = artifact
        .get("parts")
        .and_then(Value::as_array)
        .and_then(|p| p.first())
        .ok_or_else(|| plain("the `frame` artifact carries no part"))?;
    part.get("data")
        .cloned()
        .ok_or_else(|| plain("the `frame` artifact's part carries no `data`"))
}

/// `aoide/pairReveal`'s reply carries only `{ok}` — a JSON-RPC `error`
/// becomes a refusal message; anything else is `Ok(())`. Pure.
pub fn check_pair_reveal_response(resp: &Value) -> Result<(), String> {
    if let Some(err) = resp.get("error") {
        let detail = err.get("message").and_then(Value::as_str).unwrap_or("(no message)");
        return Err(format!("the node refused the reveal: {detail}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_graph_summary_request_matches_the_jsonrpc_shape() {
        let body = build_graph_summary_request();
        assert_eq!(body["jsonrpc"], "2.0");
        assert_eq!(body["method"], "aoide/graphSummary");
        assert!(body["params"].is_object());
    }

    #[test]
    fn build_task_get_frame_request_puts_the_frame_key_where_the_door_reads_it() {
        let body = build_task_get_frame_request("sess-1", 50);
        assert_eq!(body["jsonrpc"], "2.0");
        assert_eq!(body["method"], "tasks/get");
        assert_eq!(body["params"]["id"], "sess-1");
        assert_eq!(body["params"]["metadata"]["aoide/frame"]["tail"], 50);
        // Only `params.metadata` counts on the far side — never
        // `message.metadata`, which `message/send` accepts instead.
        assert!(body["params"]["message"].is_null());
    }

    #[test]
    fn parse_frame_response_hands_back_the_frame_and_keeps_the_door_s_code() {
        let frame = json!({ "sessionId": "sess-1", "presence": "running" });
        let ok = json!({
            "jsonrpc": "2.0", "id": 1,
            "result": {
                "id": "sess-1", "contextId": "sess-1", "kind": "task",
                "status": { "state": "working", "timestamp": "2026-09-21T05:00:00Z" },
                "artifacts": [{
                    "artifactId": "frame", "name": "session watch frame",
                    "parts": [{ "kind": "data", "data": frame }],
                }],
            }
        });
        assert_eq!(parse_frame_response(&ok).unwrap()["presence"], "running");

        // The output-read refusal: the code rides back with the message, which
        // is what lets the caller teach the fix instead of printing "refused".
        // The number comes from the wire's own const, so this test cannot agree
        // with a door that renumbered and still pass.
        let refused = json!({
            "jsonrpc": "2.0", "id": 1,
            "error": {
                "code": aoide_protocol::wire::a2a::OUTPUT_READ_REFUSED_CODE,
                "message": "output read refused",
            }
        });
        let e = parse_frame_response(&refused).unwrap_err();
        assert_eq!(e.code, Some(aoide_protocol::wire::a2a::OUTPUT_READ_REFUSED_CODE));
        assert_eq!(e.message, "output read refused");
        assert!(e.to_string().contains("-32011"), "the code rides the Display too: {e}");

        // A transport-shaped response with no frame in it is an error WITHOUT
        // a code — nothing to teach about a door that answered something else.
        let wrong_shape = json!({ "jsonrpc": "2.0", "id": 1, "result": { "id": "sess-1" } });
        let e = parse_frame_response(&wrong_shape).unwrap_err();
        assert_eq!(e.code, None);
        assert!(e.message.contains("artifacts"), "{}", e.message);
    }

    #[test]
    fn parse_graph_summary_response_extracts_a_fresh_cache_entry() {
        let resp = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "schemaVersion": "0",
                "instance": { "name": "yomi-strix", "url": "http://yomi-strix:8710/", "emittedAt": "2026-08-14T00:00:00Z" },
                "graph": { "schemaVersion": "0", "nodes": [], "edges": [] },
            }
        });
        let entry = parse_graph_summary_response(&resp, "yomi-strix", "2026-08-14T00:05:00Z").unwrap();
        assert_eq!(entry.name, "yomi-strix");
        assert!(!entry.stale);
        assert_eq!(entry.fetched_at.as_deref(), Some("2026-08-14T00:05:00Z"));
        assert_eq!(entry.instance.unwrap()["name"], "yomi-strix");
        assert_eq!(entry.graph.unwrap()["schemaVersion"], "0");
    }

    #[test]
    fn parse_graph_summary_response_rejects_an_error_or_wrong_schema_or_missing_graph() {
        let err_resp = json!({ "jsonrpc": "2.0", "id": 1, "error": { "code": -32601, "message": "method not found" } });
        assert!(parse_graph_summary_response(&err_resp, "p", "T").is_err());

        let bad_schema = json!({ "result": { "schemaVersion": "99", "graph": {} } });
        assert!(parse_graph_summary_response(&bad_schema, "p", "T").is_err());

        let no_graph = json!({ "result": { "schemaVersion": "0" } });
        assert!(parse_graph_summary_response(&no_graph, "p", "T").is_err());

        let empty = json!({});
        assert!(parse_graph_summary_response(&empty, "p", "T").is_err());
    }

    // ── The pairing ceremony (P-P2) ──────────────────────────────────────

    #[test]
    fn build_pair_request_body_matches_the_jsonrpc_shape() {
        let body = build_pair_request_body("pk", "box-b", "commit", "http://a/", None);
        assert_eq!(body["method"], "aoide/pairRequest");
        assert_eq!(body["params"]["pubkeyHex"], "pk");
        assert_eq!(body["params"]["name"], "box-b");
        assert_eq!(body["params"]["commitHex"], "commit");
        assert_eq!(body["params"]["url"], "http://a/");
        assert!(body["params"].get("nonceHex").is_none(), "the nonce itself never rides pairRequest");
        assert!(body["params"].get("selfVia").is_none(), "selfVia is omitted outright when the caller has no claim, never sent null");
    }

    #[test]
    fn build_pair_request_body_carries_self_via_when_given() {
        let body = build_pair_request_body("pk", "box-b", "commit", "http://a/", Some("ssh://khoa@box-b"));
        assert_eq!(body["params"]["selfVia"], "ssh://khoa@box-b");
    }

    #[test]
    fn build_pair_request_body_without_self_via_still_parses_on_an_old_approver_shape() {
        // An old approver's params struct has no `selfVia` field at all —
        // simulate its `Deserialize` over a body this (new) requester sent
        // with no claim, and confirm the shape round-trips with nothing
        // extra required.
        let body = build_pair_request_body("pk", "box-b", "commit", "http://a/", None);
        let params = body["params"].clone();
        assert!(params.get("selfVia").is_none());
        // And the reverse: an old requester's body (no selfVia key at all)
        // must still be exactly what a body with an explicit None produces
        // — proving the field is well and truly absent, not `null`.
        let old_shape = json!({ "pubkeyHex": "pk", "name": "box-b", "commitHex": "commit", "url": "http://a/" });
        assert_eq!(params, old_shape);
    }

    #[test]
    fn parse_pair_request_response_extracts_the_ack() {
        let resp = json!({
            "jsonrpc": "2.0", "id": 1,
            "result": { "id": "abc12345", "pubkeyHex": "b".repeat(64), "nonceHex": "d".repeat(32), "expiresAt": "2026-08-25T04:00:00Z" }
        });
        let ack = parse_pair_request_response(&resp).unwrap();
        assert_eq!(ack.id, "abc12345");
        assert_eq!(ack.pubkey_hex, "b".repeat(64));
        assert_eq!(ack.nonce_hex, "d".repeat(32));
        assert_eq!(ack.expires_at, "2026-08-25T04:00:00Z");
    }

    #[test]
    fn parse_pair_request_response_rejects_an_error_or_missing_fields() {
        let err_resp = json!({ "error": { "code": -32602, "message": "invalid params" } });
        assert!(parse_pair_request_response(&err_resp).is_err());

        let missing_id = json!({ "result": { "pubkeyHex": "a", "nonceHex": "b" } });
        assert!(parse_pair_request_response(&missing_id).is_err());

        let missing_pubkey = json!({ "result": { "id": "x", "nonceHex": "b" } });
        assert!(parse_pair_request_response(&missing_pubkey).is_err());

        assert!(parse_pair_request_response(&json!({})).is_err());
    }

    #[test]
    fn build_pair_reveal_body_matches_the_jsonrpc_shape() {
        let body = build_pair_reveal_body("abc12345", "d".repeat(32).as_str());
        assert_eq!(body["method"], "aoide/pairReveal");
        assert_eq!(body["params"]["id"], "abc12345");
        assert_eq!(body["params"]["nonceHex"], "d".repeat(32));
    }

    #[test]
    fn check_pair_reveal_response_passes_ok_and_surfaces_an_error() {
        assert!(check_pair_reveal_response(&json!({ "result": { "ok": true } })).is_ok());
        assert!(check_pair_reveal_response(&json!({ "error": { "code": -32002, "message": "commitment mismatch" } })).is_err());
    }

    #[test]
    fn build_pair_poll_body_matches_the_jsonrpc_shape() {
        let body = build_pair_poll_body("abc12345", "2026-08-28T00:00:00Z", "n1", "sig");
        assert_eq!(body["method"], "aoide/pairPoll");
        assert_eq!(body["params"]["id"], "abc12345");
        assert_eq!(body["params"]["timestampIso"], "2026-08-28T00:00:00Z");
        assert_eq!(body["params"]["nonceHex"], "n1");
        assert_eq!(body["params"]["signatureHex"], "sig");
    }

    #[test]
    fn parse_pair_poll_response_extracts_pending_and_approved() {
        assert_eq!(parse_pair_poll_response(&json!({ "result": { "status": "pending" } })).unwrap(), PairPollStatus::Pending);
        assert_eq!(
            parse_pair_poll_response(&json!({ "result": { "status": "approved", "pubkeyHex": "b".repeat(64) } })).unwrap(),
            PairPollStatus::Approved { pubkey_hex: "b".repeat(64) }
        );
    }

    #[test]
    fn parse_pair_poll_response_rejects_an_error_missing_pubkey_or_unrecognized_status() {
        let err_resp = json!({ "error": { "code": -32602, "message": "invalid params" } });
        assert!(parse_pair_poll_response(&err_resp).is_err());

        let missing_pubkey = json!({ "result": { "status": "approved" } });
        assert!(parse_pair_poll_response(&missing_pubkey).is_err());

        let bad_status = json!({ "result": { "status": "???" } });
        assert!(parse_pair_poll_response(&bad_status).is_err());

        assert!(parse_pair_poll_response(&json!({})).is_err());
    }
}
