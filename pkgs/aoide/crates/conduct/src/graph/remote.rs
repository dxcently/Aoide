//! Addressing a session ON ANOTHER NODE, and refusing what that resolution
//! cannot answer (P-RSA). `send --to <node>/<query>` and `session watch
//! <node>/<query>` are two commands over ONE resolution: a node's CACHED
//! graph document (`state/node-cache/<node>.json`) is the addressing source,
//! never a live pull (the cache is what `node pull`/`node --hosts` keep
//! current; a send or a watch that triggered its own network fetch would not
//! be predictable), and `aoide_storage::addr::resolve` decides the match.
//!
//! **One definition, two doors.** Every function here moved out of `send.rs`
//! in S7, when the watch became the second caller; nothing is copied, so the
//! two commands cannot disagree about what `<node>/<query>` means, which
//! refusals an unresolvable query earns, or how a candidate is labelled.
//!
//! **The node half is the caller's.** `addr::resolve` returns
//! `Resolution::Remote { node, query }` off the caller's own registered node
//! names; this module starts from that pair, finds the [`Node`] record (the
//! bearer/signature/tunnel material lives there, not in the cache) and reads
//! the cache. A node with no cache is a taught error naming `node pull`,
//! never an empty view and never a silent fetch.
//!
//! **Tier 5 never fires here.** Both the resolution and its retry pass
//! `nodes: &[]`, so a nested `<node>/<node>/<query>` cannot resolve; that arm
//! is a named refusal, kept so the invariant is a stated fact rather than a
//! `unwrap` that would hold it by accident.

use super::common;
use aoide_protocol::output::Outcome;
use aoide_storage::addr::{self, LocalCandidate, Resolution};
use aoide_storage::node_store::Node;
use serde_json::{json, Value};

/// Extract every `kind:"session"` node from a node's CACHED graph document
/// as (sessionId, petname, role) triples — role derived from the SAME
/// document's own `spawned` edges. A `who.rs::sessions_from_graph` twin for
/// the two doors that ADDRESS a remote session: that function returns `who`'s
/// own display-only `SessionView`, a shape neither door has a use for — these
/// need only what [`LocalCandidate`] and an error-message label need.
pub(super) fn node_cached_sessions(graph: &Value) -> Vec<(String, Option<String>, &'static str)> {
    let empty: Vec<Value> = Vec::new();
    let nodes = graph.get("nodes").and_then(Value::as_array).unwrap_or(&empty);
    let edges = graph.get("edges").and_then(Value::as_array).unwrap_or(&empty);
    nodes
        .iter()
        .filter(|n| n["kind"] == "session")
        .map(|n| {
            let full_id = n["id"].as_str().unwrap_or("");
            let session_id = full_id.strip_prefix("session:").unwrap_or(full_id).to_string();
            let role = if edges.iter().any(|e| e["kind"] == "spawned" && e["to"] == full_id) {
                "child"
            } else {
                "root"
            };
            let petname = n["petname"].as_str().map(String::from);
            (session_id, petname, role)
        })
        .collect()
}

/// Resolve `query` (the remainder after `node/` — see `aoide_storage::addr`'s
/// tier-5 doc) against `node`'s cached session set. Tries `query` AS TYPED
/// first — this covers the common, DOCUMENTED case (`addr.rs`'s own module
/// doc example: `Remote { node: "yomi-strix", query: "brave-otter" }`, a
/// bare petname) via tiers 1–3 (exact remote id, id tail4, bare petname) —
/// and only on a miss retries the RECONSTRUCTED `<node>/<query>` form, so a
/// `role/petname` remainder (what tier 5 stripped the host segment OFF of —
/// `addr.rs`'s "multi-segment rest… passes it through verbatim" test case)
/// still resolves via tier 4 against the node's own name standing in as
/// `host`. `nodes: &[]` on BOTH attempts: a remote-of-remote is not a shape
/// this phase resolves, so tier 5 can never fire here — see
/// [`unresolved_remote`]'s own `Remote` arm.
pub(super) fn resolve_remote_query(
    node: &str,
    query: &str,
    candidates: &[LocalCandidate<'_>],
) -> Resolution {
    match addr::resolve(query, node, candidates, &[]) {
        Resolution::NotFound => addr::resolve(&format!("{node}/{query}"), node, candidates, &[]),
        other => other,
    }
}

/// A node session's display label for an error message — mirrors
/// `who.rs::sessions_from_graph`'s label construction
/// (`display::session_label` with the node's own name standing in as
/// `host`), so an ambiguous/not-found `<node>/<query>` error names candidates
/// the same way `aoide session --hosts` would already be showing them.
///
/// Everything BUT the node name comes off a document another box wrote — a
/// petname and a session id are a peer's bytes, and `session_label` renders
/// them as they are — so both are cleaned first (`common::clean_line`:
/// control characters and `Cf` marks out, one clipped line), the same
/// sanitizer every other pulled-node field reaches a terminal through
/// (`who.rs`'s own display seam). A label that arrives hostile prints as
/// inert text or not at all, never as a terminal instruction.
pub(super) fn node_session_label(node: &str, session_id: &str, petname: Option<&str>, role: &str) -> String {
    let rec = aoide_storage::records::SessionRecord {
        session_id: common::clean_line(session_id),
        petname: petname.map(common::clean_line),
        ..Default::default()
    };
    aoide_storage::display::session_label(&rec, node, role)
}

/// `node`'s CACHED graph document, or the taught error that says exactly how
/// to get one. No cache at all (the node was never pulled) is a `node pull`
/// pointer, never a silent auto-pull: addressing is READ off the cache, so a
/// command that fetched on its own would make `<node>/<query>` mean something
/// different every time it ran.
pub(super) fn node_cached_graph(cmd: &'static str, node: &str) -> Result<Value, Outcome> {
    let cache = aoide_storage::node_store::load_node_cache(node);
    match cache.and_then(|c| c.graph) {
        Some(graph) => Ok(graph),
        None => Err(Outcome::error(
            cmd,
            format!("node `{node}` has no cached graph — run `aoide node pull {node}` first"),
        )
        .with_data(json!({ "reason": "node-never-pulled", "node": node }))),
    }
}

/// The registered [`Node`] record for a name `addr::resolve` returned, out of
/// the slice the CALLER already loaded (the same slice whose names seeded the
/// resolution) — no second read of `nodes.json`, and no chance of judging the
/// resolution against a list that moved under it. A miss is the resolution
/// naming a node the list just held: unreachable in practice, said plainly
/// ("vanished mid-resolution") instead of dereferenced.
pub(super) fn node_record<'a>(
    cmd: &'static str,
    nodes: &'a [Node],
    node: &str,
) -> Result<&'a Node, Outcome> {
    nodes.iter().find(|p| p.name == node).ok_or_else(|| {
        Outcome::error(cmd, format!("node `{node}` vanished mid-resolution"))
            .with_data(json!({ "reason": "node-not-found", "node": node }))
    })
}

/// The refusal for a remote query that did not resolve to exactly ONE
/// session on `node`: `Ambiguous` (the candidate labels), `NotFound` (the
/// same, as the available set), and the unreachable `Remote` (tier 5 never
/// fires — `nodes: &[]` — so this is a named refusal rather than a panic if
/// that invariant ever drifts). `Local` is the caller's own arm: passing it
/// here is a programming error, and it says so instead of resolving to
/// something arbitrary.
///
/// Pure. `cmd` is the CALLER's own dotted name (`send`, `session.watch`), so
/// each door's refusal carries its own command in `Outcome::cmd` and in the
/// text's own frame; `sess` is [`node_cached_sessions`]' own output, which is
/// what makes every label here the label the roster would show.
pub(super) fn unresolved_remote(
    cmd: &'static str,
    node: &str,
    query: &str,
    sess: &[(String, Option<String>, &'static str)],
    resolved: Resolution,
) -> Outcome {
    let labels = |ids: &[String]| -> Vec<String> {
        ids.iter()
            .filter_map(|id| {
                sess.iter()
                    .find(|(sid, _, _)| sid == id)
                    .map(|(sid, pet, role)| node_session_label(node, sid, pet.as_deref(), role))
            })
            .collect()
    };
    match resolved {
        Resolution::Ambiguous(ids) => Outcome::error(
            cmd,
            format!(
                "`{query}` is ambiguous on node `{node}` — {} session(s) match: {}",
                ids.len(),
                labels(&ids).join(", ")
            ),
        )
        .with_data(json!({ "reason": "ambiguous", "node": node, "query": query, "candidates": ids })),
        Resolution::NotFound => {
            let available = sess
                .iter()
                .map(|(id, pet, role)| node_session_label(node, id, pet.as_deref(), role))
                .collect::<Vec<String>>();
            let hint = if available.is_empty() {
                format!(" (node `{node}` has no cached sessions)")
            } else {
                format!(" — available on `{node}`: {}", available.join(", "))
            };
            Outcome::error(cmd, format!("no session on node `{node}` matches `{query}`{hint}"))
                .with_data(json!({ "reason": "not-found", "node": node, "query": query }))
        }
        Resolution::Remote { .. } => Outcome::error(
            cmd,
            format!("`{query}` resolved to a nested node reference, which is not supported"),
        )
        .with_data(json!({ "reason": "nested-remote-unsupported", "node": node, "query": query })),
        // `Local` is the caller's OWN arm — a door that already holds a
        // sessionId never calls the refuser. Reaching this is a bug in the
        // caller, not a state a far box can produce, so it is named rather
        // than folded into a wildcard that would print an arbitrary refusal
        // for a resolved target.
        Resolution::Local(id) => Outcome::error(
            cmd,
            format!("internal: `{id}` resolved on node `{node}` but was not delivered"),
        )
        .with_data(json!({ "reason": "unresolved-local", "node": node, "sessionId": id })),
    }
}

/// [`node_cached_graph`], [`node_cached_sessions`] and
/// [`resolve_remote_query`] as ONE read — every door's first three steps after
/// `addr::resolve` answered `Remote`. `Ok(id)` is the remote sessionId to
/// address; `Err` is the taught refusal `Outcome` that names the reason, built
/// through [`unresolved_remote`] so both doors refuse identically, and a
/// never-pulled node is [`node_cached_graph`]'s own error unchanged.
pub(super) fn resolve_on_node(cmd: &'static str, node: &str, query: &str) -> Result<String, Outcome> {
    let graph = node_cached_graph(cmd, node)?;
    let sess = node_cached_sessions(&graph);
    let candidates: Vec<LocalCandidate<'_>> = sess
        .iter()
        .map(|(id, pet, role)| LocalCandidate { session_id: id, petname: pet.as_deref(), role })
        .collect();
    match resolve_remote_query(node, query, &candidates) {
        Resolution::Local(id) => Ok(id),
        other => Err(unresolved_remote(cmd, node, query, &sess, other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure, no I/O — mirrors `aoide_storage::addr`'s own table-driven
    /// style, scoped to what this function adds on top of `addr::resolve`
    /// itself: trying `query` exactly as typed first (tiers 1-3), and
    /// only on a miss retrying the reconstructed `<node>/<query>` form
    /// (tier 4, the `role/petname` remainder tier 5 stripped the host off
    /// of).
    #[test]
    fn resolve_remote_query_table() {
        struct Case {
            name: &'static str,
            query: &'static str,
            candidates: Vec<(&'static str, Option<&'static str>, &'static str)>,
            expected: Resolution,
        }
        let node = "yomi-strix";
        let cases = vec![
            Case {
                name: "exact remote id, tried as typed",
                query: "sess-aaaa-1111",
                candidates: vec![("sess-aaaa-1111", Some("brave-otter"), "root")],
                expected: Resolution::Local("sess-aaaa-1111".into()),
            },
            Case {
                name: "id tail4, tried as typed",
                query: "1111",
                candidates: vec![("sess-aaaa-1111", Some("brave-otter"), "root")],
                expected: Resolution::Local("sess-aaaa-1111".into()),
            },
            Case {
                name: "bare petname, tried as typed (the documented common case)",
                query: "brave-otter",
                candidates: vec![("sess-aaaa-1111", Some("brave-otter"), "root")],
                expected: Resolution::Local("sess-aaaa-1111".into()),
            },
            Case {
                name: "role/petname compound falls back to the reconstructed <node>/<query> form",
                query: "root/brave-otter",
                candidates: vec![("sess-aaaa-1111", Some("brave-otter"), "root")],
                expected: Resolution::Local("sess-aaaa-1111".into()),
            },
            Case {
                name: "petname collision on the node's own cache is ambiguous",
                query: "brave-otter",
                candidates: vec![
                    ("sess-aaaa-1111", Some("brave-otter"), "root"),
                    ("sess-bbbb-2222", Some("brave-otter"), "child"),
                ],
                expected: Resolution::Ambiguous(vec!["sess-aaaa-1111".into(), "sess-bbbb-2222".into()]),
            },
            Case {
                name: "no match in either attempt",
                query: "ghost-name",
                candidates: vec![("sess-aaaa-1111", Some("brave-otter"), "root")],
                expected: Resolution::NotFound,
            },
        ];
        for c in cases {
            let candidates: Vec<LocalCandidate<'_>> = c
                .candidates
                .iter()
                .map(|(id, pet, role)| LocalCandidate { session_id: id, petname: *pet, role })
                .collect();
            let got = resolve_remote_query(node, c.query, &candidates);
            assert_eq!(got, c.expected, "case failed: {}", c.name);
        }
    }

    /// A `(sessionId, petname, role)` triple set → a minimal cached-graph
    /// document [`node_cached_sessions`] can extract back out of — a `role:
    /// "child"` entry gets a synthetic `spawned` edge so the role-derivation
    /// half of the extraction is exercised too, mirroring `who.rs`'s own
    /// `node_graph` test fixture.
    fn node_graph_json(sessions: &[(&str, Option<&str>, &str)]) -> Value {
        let nodes: Vec<Value> = sessions
            .iter()
            .map(|(id, petname, _role)| {
                let mut n = json!({
                    "id": format!("session:{id}"), "kind": "session",
                    "state": "working", "cwd": "/x", "agent": "claude",
                });
                if let Some(p) = petname {
                    n["petname"] = json!(p);
                }
                n
            })
            .collect();
        let edges: Vec<Value> = sessions
            .iter()
            .filter(|(_, _, role)| *role == "child")
            .map(|(id, _, _)| json!({ "from": "session:parent", "to": format!("session:{id}"), "kind": "spawned" }))
            .collect();
        json!({ "schemaVersion": "0", "nodes": nodes, "edges": edges })
    }

    /// A peer's own petname and session id reach an error message through
    /// [`node_session_label`], which renders them as they arrived — so they are
    /// cleaned first (`common::clean_line`, the same sanitizer every other
    /// pulled-node field reaches a terminal through): a hostile cached petname
    /// prints as inert text, never as a terminal instruction, however this door
    /// refuses the query.
    #[test]
    fn a_hostile_cached_petname_is_cleaned_out_of_every_refusal() {
        let hostile = "\u{1b}[2J\u{202e}moc.elpmaxe\u{200b}";
        let sess = vec![
            (hostile.to_string(), Some(hostile.to_string()), "root"),
            ("sess-remote-2".to_string(), Some(hostile.to_string()), "child"),
        ];
        // The QUERY is the operator's own typing (never cleaned — it is what
        // they asked for, echoed back); the CANDIDATES come from the far node's
        // cached document, so those are the bytes under test.
        for (query, resolved) in [
            ("collides", Resolution::Ambiguous(vec![hostile.to_string(), "sess-remote-2".to_string()])),
            ("ghost", Resolution::NotFound),
        ] {
            let out = unresolved_remote("send", "yomi", query, &sess, resolved);
            for forbidden in ['\u{1b}', '\u{202e}', '\u{200b}'] {
                assert!(!out.message.contains(forbidden), "{forbidden:?} survived: {}", out.message);
            }
            assert!(out.message.contains("moc.elpmaxe"), "the text itself still shows: {}", out.message);
        }
    }
}
