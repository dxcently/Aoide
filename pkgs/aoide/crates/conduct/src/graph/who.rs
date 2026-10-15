//! The ROSTER core (messaging/presence plan, P-C2; folded under `session` at
//! the session-surface redesign, command-defrag lane X, 2026-08-28) — live
//! presence over this box's own sessions plus every registered node. A
//! PROJECTION, never a store: `build_graph`'s node fold (`doc.rs:156-201`)
//! already folds registered nodes with freshness off their pull CACHE; this
//! module never writes `state/node-cache/<name>.json` — nothing here is a
//! second source of truth for it.
//!
//! **The standalone `aoide who` command is RETIRED (hard cutover, no
//! alias — this exact spelling is now unknown, same as a typo).** Its
//! collection pipeline ([`collect_roster`]) and its host-grouped rendering
//! ([`render_nodes`]/[`node_json`]) both SURVIVE, unchanged in mechanism,
//! now reached at `aoide session --hosts` ([`session_roster`]/
//! [`session_roster_with`]) — the SAME `Roster` [`collect_roster`] builds
//! also feeds bare `session`'s PROJECT-grouped rendering
//! ([`group_by_project`]/[`render_groups`]), so `who`'s old byte-for-byte
//! output survives as one of two renderings behind one command instead of
//! living behind a command of its own. `conductor`'s ROSTER panel
//! (`conductor/src/app.rs`'s `spawn_roster_fetch`) dispatches
//! `session --hosts` now — same `Outcome` shape (`nodes`, byte-identical to
//! `who`'s), so that panel needed no rendering change, only its own dispatch
//! `Invocation`.
//!
//! ## Presence model (User-decided, verbatim from the plan)
//!
//! Every registered node is ALWAYS-ON: a resident door means host-up ==
//! door-answering. So this module probes every registered node LIVE on EVERY
//! invocation — one [`std::thread`] per node (no new deps), each bounded by
//! a short per-node timeout (~2s) enforced by the transport itself (curl's
//! own `--max-time`, inside [`aoide_client::commands::pull_node_live`]) —
//! never a manual join-with-timeout here. There is no pull timer anywhere;
//! the cache is consulted ONLY as the fallback for a node this invocation's
//! live probe fails to reach, so an unreachable node still renders (never
//! silently drops off the roster) with its last-known sessions labeled by
//! the cache's own `fetchedAt`.
//!
//! Node-level presence: `online` (probed successfully just now) |
//! `unreachable` (probe failed, a cache exists) | `never-pulled` (probe
//! failed, no cache ever written). Session-level presence: `online` (state
//! in `working`/`awaiting`/`idle`) | `stale` (`stopped`) | `done` — omitted
//! from a normal listing, kept with `--all`. A remote session from a LIVE
//! reply is classified exactly the same way a local one is — "as
//! trustworthy as local" per the plan. A remote session surfaced from the
//! CACHE fallback instead is never trusted at that same face value:
//! [`cached_presence`] lets `done` alone survive verbatim (`node list` and
//! `session` both filter on that exact string) and rewrites everything else
//! to `last-seen` (the cache carries a `fetchedAt`) or `unknown` (it
//! doesn't) — a node header already saying `unreachable` never fronts a
//! session still claiming to be live.
//!
//! ## The probe seam (why tests need no network)
//!
//! [`probe_nodes`] takes the node list AND a `pull` closure — production
//! wires it to `aoide_client::commands::pull_node_live` (2s), tests inject a
//! closure returning canned `Ok`/`Err` values instantly. This is the ONLY
//! way the per-node-timeout behavior is exercisable in a sandbox at all: the
//! real 2s bound lives inside curl, one process this crate's tests never
//! spawn.
//!
//! ## Filter semantics (`--hosts` rendering only)
//!
//! An optional positional `filter` narrows what's DISPLAYED; it never
//! changes what gets probed (every node is probed regardless — see
//! [`session_roster_with`]). Resolution order: try `storage::addr::resolve`
//! first — `Local`/`Ambiguous` narrows to exactly those local session ids;
//! `Remote{node, query}` narrows to that one node, additionally
//! substring-matching its sessions when `query` is non-empty. A query the
//! resolver can't place at all (`NotFound` — most commonly a plain
//! substring nobody typed as a full grammar token) falls back to a
//! case-sensitive substring match: a node whose own name contains it keeps
//! every session, otherwise only ITS sessions whose id/petname/label
//! contain it survive. Applies to both groupings — it narrows `nodes`
//! BEFORE the host/project split, so a filter behaves identically either
//! way.
//!
//! ## Project attribution (bare `session`'s own grouping)
//!
//! [`project_bucket`] reuses whichever attribution the codebase already
//! computes — never a third one: a registered `projects.json` name
//! ([`super::model::anchor_for`], longest-prefix, PURE string matching, so
//! it resolves identically for a node session's cwd under the fleet's
//! shared-path convention the same way `grant.rs`'s node-spec relativization
//! already leans on) wins when present; else a `.aoide/project.json`
//! manifest found by walking up from the cwd ON THIS HOST'S OWN FILESYSTEM
//! (`aoide_storage::manifest::walk_up`) renders by that directory's own
//! basename — a node's foreign cwd simply never resolves a manifest here
//! (the walk is real `Path::is_file()` checks against THIS filesystem), so
//! it falls through harmlessly rather than lying about a match. Neither
//! resolving lands the session in the trailing [`NO_PROJECT`] bucket.

use super::model::{resolved_parent, HookRecord, Project, SessionRecord};
use aoide_protocol::output::Outcome;
use aoide_protocol::Invocation;
use aoide_storage::addr::{self, LocalCandidate, Resolution};
use aoide_storage::node_store::{Node, NodeCacheEntry};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// A roster-probe pull closure: given a node, return its live resolved graph
/// document (`{nodes, edges}`) or a reason it couldn't be fetched. Boxed so
/// production (`aoide_client::commands::pull_node_live`) and tests (a canned
/// closure) share the exact same call shape.
pub(super) type PullFn = Arc<dyn Fn(&Node) -> Result<Value, String> + Send + Sync>;

/// One session as the roster renders it — local or remote, uniformly.
/// `pub(super)` (fields too) for three consumers: `session_pick`-turned-
/// `grant.rs`'s undying picker (U3) builds its node rows off the same
/// [`sessions_from_graph`] extraction rather than re-parsing a node's cached
/// graph document a second time, and `node_list.rs`'s mesh roster (task
/// #120 P2) does likewise (this crate's own "no cross-crate copying"
/// discipline, applied in-file).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SessionView {
    pub(super) session_id: String,
    pub(super) label: String,
    pub(super) petname: Option<String>,
    pub(super) agent: String,
    pub(super) state: String,
    pub(super) presence: &'static str,
    pub(super) cwd: String,
    pub(super) project: Option<String>,
    /// The project this session RENDERS under, resolved against the owner
    /// chain (`model::effective_project_for`) — `None` for every row built
    /// by [`sessions_from_graph`] (a remote node's own graph document has no
    /// local session slice to walk owners against; S-D owns cross-host
    /// inheritance). [`group_by_project`] prefers this over `project` when
    /// present.
    pub(super) effective_project: Option<String>,
    /// `SessionRecord::exempt` (task #20), carried through for the roster's
    /// one-word tag. Local rows read the real record; a node row has no
    /// cross-host exempt story yet (`grant.rs`'s module doc — out of scope,
    /// not a regression) and always reads `false`.
    pub(super) exempt: bool,
    /// The auto-renamed one-line task title (`SessionRecord.title`) — a
    /// local row reads the record directly, a remote row reads the node's
    /// own published `title` key (`doc.rs`). `None` when neither side ever
    /// set one; nothing here invents, defaults, or infers it (P-14 M2
    /// enrichment — [`build_local_node`], [`sessions_from_graph`]).
    pub(super) title: Option<String>,
    /// The Claude model this session is running (`SessionRecord.model` /
    /// the node's own `model` key). Same absent-stays-absent discipline as
    /// `title`.
    pub(super) model: Option<String>,
    /// Session classification (`SessionRecord.kind`) — a node's own graph
    /// document publishes it under the key `role` (`doc.rs`'s rename,
    /// distinct from this module's local root/child `role` string), read
    /// back here under the record's own noun.
    pub(super) kind: Option<String>,
    /// This session's parent session id, bare (matching `session_id`, never
    /// node-scoped) — a local row reads [`resolved_parent`], a remote row
    /// reads the SAME `spawned` edge [`sessions_from_graph`] already scans
    /// for `role`, its `from` minus the `session:` prefix. `None` for a
    /// root session on either side.
    pub(super) parent: Option<String>,
    /// The native harness's own thread-role label (`SessionRecord.native_role`
    /// / the node's own `nativeRole` key, root order seq 404) — `subagent`,
    /// `guardian_review`, … verbatim, never inferred here. Same absent-stays-
    /// absent discipline as `title`/`model`; `kind` above is untouched by it
    /// either way (a nested app child still reads `kind:"app"`).
    pub(super) native_role: Option<String>,
    /// The parent this session was spawned by on ANOTHER node
    /// (`SessionRecord.remote_parent`, CONTRACTS.md §4) — a local row reads the
    /// record, a remote row reads the node's own published `remoteParent`
    /// key. `None` on every record without one, which is every locally-spawned
    /// session: this is never `parent` above under another name, and it is
    /// never resolved into it.
    pub(super) remote_parent: Option<RemoteLink>,
    /// The children THIS session spawned on other nodes, off the caller-side
    /// ledger (`state/stage/remote-children.json`, matched on
    /// `parentSessionId`) for a local row, or off the node's own published
    /// `remoteChildren` array for a remote one. Empty when there are none —
    /// the ordinary case, and the only one a pre-P-RSA record can be in.
    pub(super) remote_children: Vec<RemoteChild>,
}

/// One cross-machine link endpoint: the far node's CURRENT name and the
/// session id on it. Identity is the node's key (`model::current_node_name`
/// resolves the name from it at projection time), so this carries no key —
/// it is what a display surface shows, never what a gate compares.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct RemoteLink {
    pub(super) node: String,
    pub(super) session_id: String,
}

/// One of a session's own children that lives on ANOTHER node — [`RemoteLink`]'s
/// two display fields plus what the roster could resolve about it: how the node
/// fold stands behind the row ([`Fold`]) and the rows under it on that same far
/// node ([`SubtreeRow`]). Both are `None`/empty until [`attach_subtrees`] runs
/// (a ledger row and a far document's own published link carry no fold — only
/// the roster, which has every node probed, owns one).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct RemoteChild {
    pub(super) node: String,
    pub(super) session_id: String,
    pub(super) fold: Option<Fold>,
    pub(super) subtree: Vec<SubtreeRow>,
}

/// One row of a remote child's subtree: a session on the SAME far node,
/// reached from the fold by that node's own `spawned` edges. Identity is the
/// link's `node` plus this id. A link's rows run in preorder and each says how
/// deep it sits (`0` is a direct child of the link), so a far chain of any
/// depth is a flat list: nothing that builds, draws, clones or drops it
/// recurses per level.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SubtreeRow {
    pub(super) session_id: String,
    pub(super) depth: usize,
}

/// How the node fold stands behind one remote-child link — the join
/// [`attach_subtrees`] makes, read by the two renderings and the JSON alike so
/// neither can name the same row differently. `Fresh`/`Stale` are the fold's
/// own words (`doc.rs`'s node `state`, off `node_store::is_cache_fresh`'s TTL);
/// `Absent` is the row the fold does not carry at all — a link this machine
/// still knows about, so it is shown, never hidden.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Fold {
    Fresh,
    Stale,
    Absent,
}

impl Fold {
    /// The one word for this link: the fold's own (`fresh`, `stale`) or the
    /// roster's for a row the fold cannot back (`not pulled`).
    fn as_str(self) -> &'static str {
        match self {
            Fold::Fresh => "fresh",
            Fold::Stale => "stale",
            Fold::Absent => "not pulled",
        }
    }

    /// The roster's marker — the word above for anything a live pull does not
    /// back, nothing at all for a fresh fold.
    fn marker(self) -> Option<&'static str> {
        (self != Fold::Fresh).then(|| self.as_str())
    }
}

/// One node (this box, or one registered node) as the host-grouped rendering
/// shows it. `pub(super)` (fields too) for a second consumer: `node_list.rs`'s
/// mesh roster (task #120 P2) classifies its paired rows off the SAME
/// probe-outcome/cache fold ([`build_mesh_node`]) rather than re-deriving a
/// second presence model — same discipline as [`SessionView`]'s widening
/// note above.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct NodeView {
    pub(super) name: String,
    pub(super) is_local: bool,
    pub(super) presence: &'static str,
    pub(super) fetched_at: Option<String>,
    pub(super) error: Option<String>,
    /// These sessions did NOT come off a live probe and the cache behind them
    /// is past `node_store::NODE_CACHE_TTL_SECS` — the fold's own `stale`
    /// (`doc.rs`'s node `state`), carried so [`attach_subtrees`] can mark a
    /// subtree that the TTL has already outlived rather than render it as if
    /// it were current. Always `false` for this box's own [`NodeView`].
    pub(super) stale: bool,
    pub(super) sessions: Vec<SessionView>,
}

/// Session-level presence class (module doc's "Presence model").
fn session_presence(state: &str) -> &'static str {
    match state {
        "stopped" => "stale",
        "done" => "done",
        // working | awaiting | idle | anything unrecognized (never invents
        // a WORSE signal than "still here" — `canonical_state` already
        // folds every producer's vocabulary before it lands in a record).
        _ => "online",
    }
}

/// This box's own [`NodeView`] — always `online` (we're running on it right
/// now). `sessions`/`hooks`/`projects` are the caller's already-loaded stage
/// files (`common::load_inputs`); the caller-side remote-children ledger is the
/// one file this reads itself (a two-field row join, never worth widening four
/// call sites' signatures for — the same read `doc.rs::build_graph` makes for
/// the graph projection). `nodes` is the caller's already-loaded registry
/// (`collect_roster`/`node_list_with` load it anyway to probe): resolving a
/// link's `key` to its CURRENT name reads it, and one read for the whole
/// roster is the point of taking it here rather than per row
/// (`model::current_node_name`'s note). `pub(super)` for `node_list.rs` (see
/// [`NodeView`]'s widening note).
pub(super) fn build_local_node(sessions: &[SessionRecord], hooks: &[HookRecord], projects: &[Project], nodes: &[Node], host: &str) -> NodeView {
    let merged = super::model::merged_sessions(sessions, hooks);
    let ids: HashSet<&str> = merged.iter().map(|s| s.session_id.as_str()).collect();
    let remote_children = aoide_storage::remote_children::load_remote_children();
    let sessions = merged
        .iter()
        .map(|s| {
            let parent = resolved_parent(s, &ids);
            let role = if parent.is_some() { "child" } else { "root" };
            let effective_project = super::model::effective_project_for(s, &merged, projects)
                .map(|i| projects[i].name.clone());
            let remote_parent = s.remote_parent.as_ref().map(|rp| RemoteLink {
                node: super::model::current_node_name(nodes, &rp.key, &rp.node),
                session_id: rp.session_id.clone(),
            });
            let remote_children = remote_children
                .iter()
                .filter(|c| c.parent_session_id == s.session_id)
                .map(|c| RemoteChild {
                    node: super::model::current_node_name(nodes, &c.key, &c.node),
                    session_id: c.session_id.clone(),
                    fold: None,
                    subtree: Vec::new(),
                })
                .collect();
            SessionView {
                session_id: s.session_id.clone(),
                label: aoide_storage::display::session_label(s, host, role),
                petname: s.petname.clone(),
                agent: s.agent.clone(),
                state: s.state.clone(),
                presence: session_presence(&s.state),
                cwd: s.cwd.clone(),
                project: s.project.clone(),
                effective_project,
                exempt: s.exempt,
                title: s.title.clone(),
                model: s.model.clone(),
                kind: s.kind.clone(),
                parent,
                native_role: s.native_role.clone(),
                remote_parent,
                remote_children,
            }
        })
        .collect();
    NodeView { name: host.to_string(), is_local: true, presence: "online", fetched_at: None, error: None, stale: false, sessions }
}

/// The first `spawned` edge into each id of a graph document's `edges`, in one
/// pass. A role or a parent is then a hash probe per session, not a scan of
/// every edge: the document is a far node's bytes and its edge count is its
/// own to choose, so a scan per session would be quadratic in what it sends.
/// `remote.rs`'s twin reads the same edges through the same map.
pub(super) fn spawned_into(edges: &[Value]) -> HashMap<&str, &Value> {
    let mut by_child: HashMap<&str, &Value> = HashMap::new();
    for e in edges.iter().filter(|e| e["kind"] == "spawned") {
        if let Some(to) = e["to"].as_str() {
            by_child.entry(to).or_insert(e);
        }
    }
    by_child
}

/// Extract every `kind:"session"` node from a (local or node) resolved graph
/// document into [`SessionView`]s, `role` (root/child, for
/// `display::session_label`) derived from the SAME document's own
/// `spawned` edges — the node computed this document with its own
/// `build_graph`, so its edges carry exactly the shape ours do
/// (`doc.rs:122-134`). `host` is the label prefix — the node's registered
/// name for a remote graph, mirroring the `node/<rest>` grammar
/// `storage::addr` resolves queries against.
pub(super) fn sessions_from_graph(graph: &Value, host: &str) -> Vec<SessionView> {
    let empty: Vec<Value> = Vec::new();
    let nodes = graph.get("nodes").and_then(Value::as_array).unwrap_or(&empty);
    let edges = graph.get("edges").and_then(Value::as_array).unwrap_or(&empty);
    let spawned = spawned_into(edges);
    nodes
        .iter()
        .filter(|n| n["kind"] == "session")
        .map(|n| {
            let full_id = n["id"].as_str().unwrap_or("");
            let session_id = full_id.strip_prefix("session:").unwrap_or(full_id).to_string();
            let spawned_edge = spawned.get(full_id);
            let role = if spawned_edge.is_some() { "child" } else { "root" };
            let parent = spawned_edge
                .and_then(|e| e["from"].as_str())
                .map(|f| f.strip_prefix("session:").unwrap_or(f).to_string());
            let petname = n["petname"].as_str().map(String::from);
            let state = n["state"].as_str().unwrap_or("idle").to_string();
            let rec = aoide_storage::records::SessionRecord {
                session_id: session_id.clone(),
                petname: petname.clone(),
                ..Default::default()
            };
            SessionView {
                label: aoide_storage::display::session_label(&rec, host, role),
                agent: n["agent"].as_str().unwrap_or("").to_string(),
                presence: session_presence(&state),
                state,
                cwd: n["cwd"].as_str().unwrap_or("").to_string(),
                project: n["project"].as_str().map(str::to_owned),
                session_id,
                petname,
                // No owner chain to walk for a remote graph document (S-D
                // owns cross-host inheritance) — a node row keeps today's
                // `project`-only attribution verbatim.
                effective_project: None,
                // No cross-host exempt story yet (module doc's widening
                // note on `SessionView::exempt`) — a node's own graph.json
                // never carries the field either, so this always reads
                // `false`.
                exempt: false,
                title: n["title"].as_str().map(String::from),
                model: n["model"].as_str().map(String::from),
                kind: n["role"].as_str().map(String::from),
                parent,
                native_role: n["nativeRole"].as_str().map(String::from),
                // Both cross-machine fields are published by the far node's own
                // projection (it resolved the names against ITS `nodes.json`),
                // so they are read back verbatim — re-resolving here would mean
                // a second lookup against a registry this document never came
                // from, and the label is display data either way.
                remote_parent: remote_link(&n["remoteParent"]),
                remote_children: n["remoteChildren"]
                    .as_array()
                    .map(|arr| arr.iter().filter_map(remote_child).collect())
                    .unwrap_or_default(),
            }
        })
        .collect()
}

/// One `{node, sessionId}` object as a graph document publishes it — `None` for
/// an absent/empty/mis-shaped one (a foreign node's document is untrusted
/// display data: a link naming no node, or missing its session id, names
/// nothing and is dropped rather than rendered as `↑ /par1`).
fn remote_link(v: &Value) -> Option<RemoteLink> {
    let session_id = v["sessionId"].as_str().filter(|s| !s.is_empty())?;
    let node = v["node"].as_str().filter(|s| !s.is_empty())?;
    Some(RemoteLink { node: node.to_string(), session_id: session_id.to_string() })
}

/// The same shape read as one of a session's own children — the link plus the
/// fold join [`attach_subtrees`] fills in later (empty here: a document's own
/// `remoteChildren` array is not this box's fold).
fn remote_child(v: &Value) -> Option<RemoteChild> {
    let link = remote_link(v)?;
    Some(RemoteChild { node: link.node, session_id: link.session_id, fold: None, subtree: Vec::new() })
}

/// The gate every CACHED session's presence passes through once its host's
/// live probe has failed (module doc's "Presence model") — pure, so the
/// table is unit-testable with no probe or cache I/O. `done` alone survives
/// verbatim (`node list` and `session` both filter `presence != "done"`);
/// everything else reads `last-seen` when the cache carries a `fetchedAt`,
/// `unknown` when it doesn't — a session's own raw state-derived presence
/// (`session_presence`) is never trusted at face value once its host header
/// already says `unreachable`.
fn cached_presence(presence: &str, fetched_at: Option<&str>) -> &'static str {
    if presence == "done" {
        return "done";
    }
    if fetched_at.is_some() {
        "last-seen"
    } else {
        "unknown"
    }
}

/// Pure: classify one node's [`NodeView`] from its live-probe OUTCOME and
/// its already-loaded last cache entry (if any) — no I/O in here at all, so
/// it is trivially unit-testable with synthetic data. The caller
/// ([`collect_roster`], and `node_list.rs`'s `node_list_with` — see
/// [`NodeView`]'s widening note) does the real
/// `node_store::load_node_cache` read and hands the result in.
pub(super) fn build_mesh_node(node: &Node, probe: Result<Value, String>, cache: Option<NodeCacheEntry>) -> NodeView {
    match probe {
        Ok(graph) => NodeView {
            name: node.name.clone(),
            is_local: false,
            presence: "online",
            fetched_at: None,
            error: None,
            // A live probe IS the fold: nothing behind this row can be stale.
            stale: false,
            sessions: sessions_from_graph(&graph, &node.name),
        },
        Err(e) => match cache {
            Some(entry) => {
                let mut sessions =
                    entry.graph.as_ref().map(|g| sessions_from_graph(g, &node.name)).unwrap_or_default();
                for sv in &mut sessions {
                    sv.presence = cached_presence(sv.presence, entry.fetched_at.as_deref());
                }
                let now_epoch =
                    aoide_storage::time::parse_iso_utc(&aoide_storage::time::now_iso_utc()).unwrap_or(0);
                let stale = !aoide_storage::node_store::is_cache_fresh(&entry, now_epoch);
                NodeView {
                    name: node.name.clone(),
                    is_local: false,
                    presence: "unreachable",
                    fetched_at: entry.fetched_at,
                    error: Some(e),
                    stale,
                    sessions,
                }
            }
            None => NodeView {
                name: node.name.clone(),
                is_local: false,
                presence: "never-pulled",
                fetched_at: None,
                error: Some(e),
                stale: false,
                sessions: Vec::new(),
            },
        },
    }
}

/// Live-probe every node in `nodes`, one [`std::thread`] each, via the
/// injected `pull` closure — see the module doc's "The probe seam". Node
/// identity is never at risk of a mismatch on a panicked probe: results are
/// re-paired with `nodes` by INDEX (`zip`), never by anything the spawned
/// thread itself returns.
pub(super) fn probe_nodes(nodes: &[Node], pull: PullFn) -> Vec<(Node, Result<Value, String>)> {
    let handles: Vec<std::thread::JoinHandle<Result<Value, String>>> = nodes
        .iter()
        .cloned()
        .map(|node| {
            let pull = Arc::clone(&pull);
            std::thread::spawn(move || pull(&node))
        })
        .collect();
    nodes
        .iter()
        .cloned()
        .zip(handles)
        .map(|(node, h)| {
            let res = h.join().unwrap_or_else(|_| Err("probe thread panicked".to_string()));
            (node, res)
        })
        .collect()
}

/// Join every remote-child link in the roster to the node fold it names
/// (P-RSA S4 review's HIGH finding). The link is only ever *data* until a
/// view walks it: `remoteChildren` names a session on a far node, and that
/// session's own local descendants are already in this roster — the probed
/// node's [`NodeView`] — so the roster is the one place that can resolve the
/// whole `par1 → nodeb/C → nodeb/G` chain, and it does it once, before
/// filtering or grouping, so both renderings and the JSON read one answer.
///
/// The join key is the pair the link itself carries: `(node name, sessionId)`
/// against a `NodeView`'s name and the session ids inside its fold. A row the
/// fold cannot back is `Fold::Absent` and is still shown — a link is what this
/// machine knows, and hiding it would be a lie of omission — and a row found in
/// a fold past its TTL is `Fold::Stale` (the `NodeView`'s own `stale`, straight
/// off `node_store::is_cache_fresh`), shown and marked rather than dressed up
/// as current.
///
/// Every fold and link here is a far node's choice, so the cost is bounded by
/// the document and not by what it arranges: each fold is indexed once, and
/// within one linking node each of its sessions is drawn under the FIRST of
/// that node's links that reaches it. A link aimed at a session an earlier
/// link of the same node already drew keeps its own row and has no rows under
/// it, so a node's rows under a fold never outnumber the fold's sessions.
/// Claims are scoped to the linking node, so one far node's links cannot
/// blank another's subtree. A far node's link into this box's own fold draws
/// no rows at all: a far document is never allowed to re-home a local session.
pub(super) fn attach_subtrees(nodes: &mut [NodeView]) {
    let joins = {
        let mut by_name: HashMap<&str, usize> = HashMap::new();
        for (i, n) in nodes.iter().enumerate() {
            by_name.entry(n.name.as_str()).or_insert(i);
        }
        let mut indexes: HashMap<usize, FoldIndex> = HashMap::new();
        let mut joins: Vec<(usize, usize, usize, Fold, Vec<SubtreeRow>)> = Vec::new();
        for (ni, n) in nodes.iter().enumerate() {
            for (si, sv) in n.sessions.iter().enumerate() {
                for (ci, child) in sv.remote_children.iter().enumerate() {
                    let (fold, rows) = match by_name.get(child.node.as_str()) {
                        Some(&fi) => {
                            let far = &nodes[fi];
                            let fold = match far.presence {
                                "never-pulled" => Fold::Absent,
                                _ if far.stale => Fold::Stale,
                                _ => Fold::Fresh,
                            };
                            // A far node's link never draws this box's own
                            // sessions: a far document may not re-home them.
                            let reaches_local = far.is_local && !nodes[ni].is_local;
                            if reaches_local {
                                (fold, Vec::new())
                            } else {
                                let index = indexes.entry(fi).or_insert_with(|| FoldIndex::new(&far.sessions));
                                (fold, index.rows_under(ni, &child.session_id))
                            }
                        }
                        None => (Fold::Absent, Vec::new()),
                    };
                    joins.push((ni, si, ci, fold, rows));
                }
            }
        }
        joins
    };
    for (ni, si, ci, fold, rows) in joins {
        let child = &mut nodes[ni].sessions[si].remote_children[ci];
        child.fold = Some(fold);
        child.subtree = rows;
    }
}

/// One node's fold, indexed for the join: who each session spawned (by that
/// fold's own `spawned` edges, `SessionView.parent`), and which sessions each
/// linking node's links have already drawn. Built once per fold, so a walk is
/// one probe per session however many links reach in.
struct FoldIndex<'a> {
    sessions: &'a [SessionView],
    kids: HashMap<&'a str, Vec<usize>>,
    drawn: HashSet<(usize, &'a str)>,
}

impl<'a> FoldIndex<'a> {
    fn new(sessions: &'a [SessionView]) -> Self {
        let mut kids: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, s) in sessions.iter().enumerate() {
            if let Some(parent) = s.parent.as_deref() {
                kids.entry(parent).or_default().push(i);
            }
        }
        FoldIndex { sessions, kids, drawn: HashSet::new() }
    }

    /// The rows under one session id for the link of node `linker`, in
    /// preorder, by that fold's own parentage — an explicit stack, because a
    /// far child's own child has a child and a hostile fold chains them as
    /// deep as it likes. `drawn` bounds the walk: a fold whose parentage loops
    /// (`graph link` mints a parent id unchecked) must cost a wrong row, never
    /// a hung render.
    fn rows_under(&mut self, linker: usize, id: &'a str) -> Vec<SubtreeRow> {
        let mut rows: Vec<SubtreeRow> = Vec::new();
        if !self.drawn.insert((linker, id)) {
            return rows;
        }
        let sessions = self.sessions;
        let kids = &self.kids;
        let drawn = &mut self.drawn;
        let kids_of = |id: &str| kids.get(id).map_or(&[][..], Vec::as_slice);
        let mut stack: Vec<(&[usize], usize)> = vec![(kids_of(id), 0)];
        while let Some((siblings, next)) = stack.last_mut() {
            let Some(&i) = siblings.get(*next) else {
                stack.pop();
                continue;
            };
            *next += 1;
            let s = &sessions[i];
            if !drawn.insert((linker, s.session_id.as_str())) {
                continue;
            }
            rows.push(SubtreeRow { session_id: s.session_id.clone(), depth: stack.len() - 1 });
            stack.push((kids_of(&s.session_id), 0));
        }
        rows
    }
}

fn session_matches_substring(sv: &SessionView, needle: &str) -> bool {
    sv.session_id.contains(needle)
        || sv.petname.as_deref().is_some_and(|p| p.contains(needle))
        || sv.label.contains(needle)
}

/// Narrow the DISPLAYED node/session set — see the module doc's "Filter
/// semantics". `locals` is the FULL (pre `--all`-trim) local session slice,
/// so a filter can still find a `done` session by id even when `--all`
/// would otherwise hide it from the final listing.
fn apply_filter(filter: &str, host: &str, nodes: Vec<NodeView>, locals: &[SessionRecord]) -> Vec<NodeView> {
    let ids: HashSet<&str> = locals.iter().map(|s| s.session_id.as_str()).collect();
    let candidates: Vec<LocalCandidate<'_>> = locals
        .iter()
        .map(|s| {
            let role = if resolved_parent(s, &ids).is_some() { "child" } else { "root" };
            LocalCandidate { session_id: &s.session_id, petname: s.petname.as_deref(), role }
        })
        .collect();
    let node_names: Vec<&str> = nodes.iter().filter(|n| !n.is_local).map(|n| n.name.as_str()).collect();

    match addr::resolve(filter, host, &candidates, &node_names) {
        Resolution::Local(id) => nodes
            .into_iter()
            .filter(|n| n.is_local)
            .map(|mut n| {
                n.sessions.retain(|sv| sv.session_id == id);
                n
            })
            .collect(),
        Resolution::Ambiguous(ids) => nodes
            .into_iter()
            .filter(|n| n.is_local)
            .map(|mut n| {
                n.sessions.retain(|sv| ids.contains(&sv.session_id));
                n
            })
            .collect(),
        Resolution::Remote { node, query } => nodes
            .into_iter()
            .filter(|n| n.name == node)
            .map(|mut n| {
                if !query.is_empty() {
                    n.sessions.retain(|sv| session_matches_substring(sv, &query));
                }
                n
            })
            .collect(),
        Resolution::NotFound => nodes
            .into_iter()
            .filter_map(|mut n| {
                if n.name.contains(filter) {
                    return Some(n);
                }
                n.sessions.retain(|sv| session_matches_substring(sv, filter));
                (!n.sessions.is_empty()).then_some(n)
            })
            .collect(),
    }
}

/// Node-level presence glyph — `online`/`unreachable`/`never-pulled` (this
/// module's doc, "Presence model"). `pub` (re-exported at `graph.rs`
/// alongside [`session_roster`]) for a second consumer: the conductor's
/// ROSTER panel (P-C4) paints the exact same three glyphs over this same
/// `presence` string and must not redraw its own copy of this map — reuse
/// it instead of forking it (crate `AGENTS.md`'s "no cross-crate copying").
pub fn glyph(presence: &str) -> &'static str {
    match presence {
        "online" => "●",
        "unreachable" => "◐",
        "never-pulled" => "○",
        _ => "?",
    }
}

/// One string this process did not write, made safe to print — a pulled
/// document's own `cwd`/`agent`/`state`, a link's `node`/`sessionId`. Every
/// render path in this module goes through here and nowhere else, so the two
/// groupings and the JSON can never hold different rules
/// (`common::clean_line`'s own doc: control characters stripped, flattened,
/// clipped to `LINE_MAX` — a 1 MB field costs a bounded line).
///
/// Identity fields are NOT sanitized in place: `session_id` and `parent` are
/// compared for the fold join above, so they stay exactly what the far node
/// published and only the *display* of them is cleaned.
fn shown(s: &str) -> String {
    super::common::clean_line(s)
}

/// `<node>/<sessionId>` — the roster's one spelling for a cross-machine link
/// endpoint, cleaned as one string ([`shown`], so a hostile id cannot smuggle
/// its own separator in). Shared by the tags and the subtree rows so a parent
/// tag and the row it points at always read the same.
fn endpoint(node: &str, session_id: &str) -> String {
    shown(&format!("{node}/{session_id}"))
}

/// One session row for either rendering: `branch`, then label, agent, state,
/// cwd and the tags — every field off a pulled document through [`shown`].
/// Shared so the host- and project-grouped renders cannot drift apart.
fn session_line(branch: &str, s: &SessionView) -> String {
    let tag = if s.exempt { " exempt" } else { "" };
    format!(
        "{branch}{}  {}  {}  {}{tag}{}",
        shown(&s.label),
        shown(&s.agent),
        shown(&s.state),
        shown(&s.cwd),
        remote_tags(s)
    )
}

/// The lines under one session row that render the remote subtree
/// [`attach_subtrees`] joined to it: each child as `<node>/<sessionId>` with
/// the fold's word beside it when a live pull cannot back the row, then that
/// child's own far descendants indented beneath it, in the same `├─`/`└─`
/// grammar the row above uses. `under` is what that row leaves to its right
/// (`"│  "` when it is not the last of its group, `"   "` when it is).
fn subtree_lines(s: &SessionView, under: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, c) in s.remote_children.iter().enumerate() {
        let last = i + 1 == s.remote_children.len();
        out.push(marked_row(under, last, &endpoint(&c.node, &c.session_id), c.fold.and_then(Fold::marker)));
        push_rows(&mut out, &deeper(under, last), &c.node, &c.subtree);
    }
    out
}

/// How many levels of a far chain's indentation are drawn. A row deeper than
/// this keeps its line — none is dropped — and wears this many levels then
/// `…`, so a chain of any depth costs a bounded prefix per row rather than one
/// that grows with it. The JSON nests to the same depth ([`subtree_json`]).
const INDENT_LEVELS: usize = 32;

/// One line per row, in the row's preorder. The `├─`/`└─` grammar is the
/// recursive one's: a row's prefix is what each ancestor leaves to its right.
fn push_rows(out: &mut Vec<String>, under: &str, node: &str, rows: &[SubtreeRow]) {
    let last = last_siblings(rows);
    // Whether each row on the path to the current one is the last of its group.
    let mut path: Vec<bool> = Vec::new();
    for (r, &is_last) in rows.iter().zip(&last) {
        path.truncate(r.depth);
        path.push(is_last);
        let drawn = (path.len() - 1).min(INDENT_LEVELS);
        let mut prefix = under.to_string();
        for &ancestor_last in &path[..drawn] {
            prefix.push_str(if ancestor_last { "   " } else { "│  " });
        }
        if path.len() - 1 > INDENT_LEVELS {
            prefix.push('…');
        }
        out.push(marked_row(&prefix, is_last, &endpoint(node, &r.session_id), None));
    }
}

/// For each row of a preorder subtree: is it the last child of its parent?
fn last_siblings(rows: &[SubtreeRow]) -> Vec<bool> {
    let mut last = vec![true; rows.len()];
    // The row at each depth of the path to the current one.
    let mut path: Vec<usize> = Vec::new();
    for (i, r) in rows.iter().enumerate() {
        if let Some(&earlier) = path.get(r.depth) {
            last[earlier] = false;
        }
        path.truncate(r.depth);
        path.push(i);
    }
    last
}

/// The continuation an `├─`/`└─` row leaves for the lines under it.
fn deeper(under: &str, last: bool) -> String {
    format!("{under}{}", if last { "   " } else { "│  " })
}

fn marked_row(under: &str, last: bool, name: &str, marker: Option<&str>) -> String {
    let branch = if last { "└─ " } else { "├─ " };
    match marker {
        Some(m) => format!("{under}{branch}{name} ({m})"),
        None => format!("{under}{branch}{name}"),
    }
}

/// The Unicode HOST-grouped render — mirrors `doc.rs::render`'s glyph/branch
/// style (`◆`/`●`/`├─`/`└─`) so every human surface reads the same grammar.
/// `session --hosts`'s renderer; byte-identical to the retired `who`
/// command's own output for a roster with no remote link, and the parent's own
/// row now carries the remote subtree it spawned (`subtree_lines`) instead of
/// leaving that link to the child's own host group alone.
fn render_nodes(nodes: &[NodeView]) -> String {
    let mut out: Vec<String> = Vec::new();
    for n in nodes {
        let head = match n.presence {
            "unreachable" => {
                let seen = n.fetched_at.as_deref().unwrap_or("unknown");
                format!("{} {} — unreachable (last seen {seen})", glyph(n.presence), n.name)
            }
            "never-pulled" => format!("{} {} — never pulled", glyph(n.presence), n.name),
            _ if n.is_local => format!("{} {} (this host)", glyph(n.presence), n.name),
            _ => format!("{} {}", glyph(n.presence), n.name),
        };
        out.push(head);
        for (i, s) in n.sessions.iter().enumerate() {
            let last = i + 1 == n.sessions.len();
            out.push(session_line(if last { "└─ " } else { "├─ " }, s));
            out.extend(subtree_lines(s, &deeper("", last)));
        }
    }
    if out.is_empty() {
        return "(no local sessions, no nodes registered)".to_string();
    }
    out.join("\n")
}

fn node_json(n: &NodeView) -> Value {
    json!({
        "name": shown(&n.name),
        "isLocal": n.is_local,
        "presence": n.presence,
        "fetchedAt": n.fetched_at,
        "error": n.error.as_deref().map(shown),
        "sessions": n.sessions.iter().map(session_view_json).collect::<Vec<_>>(),
    })
}

/// One [`SessionView`] row's JSON shape, shared by [`node_json`] (host-
/// grouped) and [`group_json`] (project-grouped) — `effectiveProject` rides
/// beside the stored `project` only when the resolver resolved one
/// (`None` for every remote row, `build_local_node`'s module doc), the same
/// present-only-when-known convention `doc.rs`'s node builder uses.
///
/// Every display string off a pulled document goes through [`shown`] here too,
/// not only in the two renderings: the conductor's ROSTER panel paints this
/// JSON, and the terminal is not the only place an ANSI-carrying `cwd` would
/// land. `sessionId` is cleaned for the same reason as everywhere else — the
/// value is identity for the fold join above, so only its display is bounded,
/// never the value a comparison reads.
fn session_view_json(s: &SessionView) -> Value {
    let mut v = json!({
        "sessionId": shown(&s.session_id),
        "label": shown(&s.label),
        "petname": s.petname.as_deref().map(shown),
        "agent": shown(&s.agent),
        "state": shown(&s.state),
        "presence": s.presence,
        "cwd": shown(&s.cwd),
        "project": s.project.as_deref().map(shown),
        "exempt": s.exempt,
    });
    if let Some(ep) = &s.effective_project {
        v["effectiveProject"] = json!(shown(ep));
    }
    if let Some(nr) = &s.native_role {
        v["nativeRole"] = json!(shown(nr));
    }
    // The spawned-by link within the same node, present only when known —
    // the edge a remote consumer needs to rank a far session under its
    // parent instead of drawing a node's sessions flat.
    if let Some(p) = &s.parent {
        v["parentSessionId"] = json!(shown(p));
    }
    // The cross-machine parent link (P-RSA S4) — the same present-only-when-
    // known rule the two keys above hold, so an ordinary locally-spawned row
    // stays byte-for-byte as before. `remoteChildren` rides only when
    // non-empty; its endpoints are `{node, sessionId}` exactly as the graph
    // document publishes them, never the ledger's own `key`.
    if let Some(p) = &s.remote_parent {
        v["remoteParent"] = json!({ "node": shown(&p.node), "sessionId": shown(&p.session_id) });
    }
    if !s.remote_children.is_empty() {
        v["remoteChildren"] = json!(
            s.remote_children.iter().map(child_json).collect::<Vec<_>>()
        );
    }
    v
}

/// One remote-child row in the JSON: the same two endpoint fields the graph
/// document publishes, plus — once [`attach_subtrees`] has met it — the fold's
/// own word (`fold`) and the far subtree itself (`subtree`), the very rows
/// [`subtree_lines`] draws. Both ride together or not at all: before the join
/// (a link parsed off a far document, which no fold of ours stands behind) the
/// row stays exactly the shape `doc.rs` publishes.
fn child_json(c: &RemoteChild) -> Value {
    let mut v = json!({ "node": shown(&c.node), "sessionId": shown(&c.session_id) });
    if let Some(fold) = c.fold {
        v["fold"] = json!(fold.as_str());
        v["subtree"] = subtree_json(&c.subtree);
    }
    v
}

/// A link's subtree as the nested JSON it has always been — each row
/// `{ sessionId, subtree: [its children] }` — down to [`INDENT_LEVELS`]. A row
/// deeper than that stays in the list of its deepest nested ancestor, in
/// preorder, with its true `depth` beside it: every consumer's JSON parser has
/// a recursion limit, so a far chain may cost the document only a bounded
/// nesting, and it loses no row.
fn subtree_json(rows: &[SubtreeRow]) -> Value {
    let mut top: Vec<Value> = Vec::new();
    // The nested rows still open, each with the children gathered under it.
    let mut open: Vec<(Value, Vec<Value>)> = Vec::new();
    for r in rows {
        let id = shown(&r.session_id);
        if r.depth < INDENT_LEVELS {
            close_rows(&mut open, &mut top, r.depth);
            open.push((json!({ "sessionId": id }), Vec::new()));
        } else {
            let deep = json!({ "sessionId": id, "subtree": [], "depth": r.depth });
            match open.last_mut() {
                Some((_, kids)) => kids.push(deep),
                None => top.push(deep),
            }
        }
    }
    close_rows(&mut open, &mut top, 0);
    Value::Array(top)
}

/// Finish the open rows deeper than `keep`, each into its parent's list.
fn close_rows(open: &mut Vec<(Value, Vec<Value>)>, top: &mut Vec<Value>, keep: usize) {
    while open.len() > keep {
        let Some((mut row, kids)) = open.pop() else { break };
        row["subtree"] = Value::Array(kids);
        match open.last_mut() {
            Some((_, siblings)) => siblings.push(row),
            None => top.push(row),
        }
    }
}

/// The roster line's cross-machine tags: `↑ <node>/<parent>` for a session
/// spawned elsewhere, `↓ <n> remote` for one that spawned elsewhere — both
/// omitted when absent, so every pre-P-RSA row renders byte-for-byte as before.
/// A session can carry both (spawned on one far node, having spawned on
/// another); identity in the tag is the far node's name plus the bare session
/// id, the only two things this box knows about a link whose other half lives
/// somewhere else. Both endpoints are cleaned ([`endpoint`]/[`shown`]).
fn remote_tags(s: &SessionView) -> String {
    let mut tags = String::new();
    if let Some(p) = &s.remote_parent {
        tags.push_str(&format!("  ↑ {}", endpoint(&p.node, &p.session_id)));
    }
    if !s.remote_children.is_empty() {
        tags.push_str(&format!("  ↓ {} remote", s.remote_children.len()));
    }
    tags
}

/// The trailing catch-all bucket name for a session whose cwd resolves
/// neither a registered project nor a manifest (module doc's "Project
/// attribution") — always sorted last in [`group_by_project`]'s output.
const NO_PROJECT: &str = "(no project)";

/// Attribute one session's cwd to a project bucket for bare `session`'s
/// PROJECT-grouped listing — see the module doc's "Project attribution".
/// `None` means neither attribution resolved; the caller buckets that as
/// [`NO_PROJECT`].
pub(super) fn project_bucket(cwd: &str, projects: &[Project]) -> Option<String> {
    if let Some(i) = super::model::anchor_for(cwd, projects) {
        return Some(projects[i].name.clone());
    }
    let path = std::path::Path::new(cwd);
    if !path.is_absolute() {
        // A session's own cwd is always recorded absolute; a stray relative
        // string (malformed input, a test fixture) must never be walked
        // relative to THIS process's own cwd — that would attribute a
        // session by an accident of where the roster command happens to
        // run, not by anything the session itself carries.
        return None;
    }
    let (root, _manifest) = aoide_storage::manifest::walk_up(std::path::Path::new(within_path_max(cwd)))?;
    Some(root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| root.to_string_lossy().into_owned()))
}

/// The longest directory path the manifest walk is started from: `PATH_MAX`
/// less its NUL, less the `/.aoide/project.json` a manifest adds beneath it.
/// A directory this long still holds one; a longer one cannot.
const WALK_PATH_MAX: usize = 4095 - "/.aoide/project.json".len();

/// `cwd`, or its deepest ancestor short enough to hold a manifest. A pulled
/// cwd is a far node's string and may be any length, while `walk_up` copies the
/// path and `stat`s it once per ancestor — a megabyte of `/a/a/a…` would cost
/// minutes. No directory on this host has a path past `PATH_MAX`, so none of
/// those ancestors can hold a manifest: the first place the full walk could
/// have answered is where this one starts.
fn within_path_max(cwd: &str) -> &str {
    if cwd.len() <= WALK_PATH_MAX {
        return cwd;
    }
    let cut = cwd.as_bytes()[..=WALK_PATH_MAX].iter().rposition(|&b| b == b'/').unwrap_or(0);
    &cwd[..cut.max(1)]
}

/// One project's bucket in bare `session`'s roster — a registered name, a
/// manifest directory's basename, or the trailing [`NO_PROJECT`] catch-all.
#[derive(Debug, Clone, PartialEq)]
struct ProjectGroup {
    name: String,
    sessions: Vec<SessionView>,
}

/// Fold every node's sessions into project buckets — a single pass over
/// `nodes` in the SAME order [`collect_roster`] built them (local first,
/// then each node in probe order), so within a bucket session order mirrors
/// the host-grouped rendering's own order. Buckets are sorted
/// alphabetically by name, [`NO_PROJECT`] always trailing last (module
/// doc's "Project attribution" / the task brief's own wording).
fn group_by_project(nodes: Vec<NodeView>, projects: &[Project]) -> Vec<ProjectGroup> {
    let mut order: Vec<String> = Vec::new();
    let mut buckets: std::collections::HashMap<String, Vec<SessionView>> = std::collections::HashMap::new();
    for node in nodes {
        for sv in node.sessions {
            let name = sv.effective_project.clone()
                .or_else(|| sv.project.clone())
                .or_else(|| project_bucket(&sv.cwd, projects))
                .unwrap_or_else(|| NO_PROJECT.to_string());
            if !buckets.contains_key(&name) {
                order.push(name.clone());
            }
            buckets.entry(name).or_default().push(sv);
        }
    }
    let mut names: Vec<String> = order.into_iter().filter(|n| n != NO_PROJECT).collect();
    names.sort();
    if buckets.contains_key(NO_PROJECT) {
        names.push(NO_PROJECT.to_string());
    }
    names
        .into_iter()
        .map(|name| {
            let sessions = buckets.remove(&name).unwrap_or_default();
            ProjectGroup { name, sessions }
        })
        .collect()
}

/// The Unicode PROJECT-grouped render — same branch/line grammar
/// [`render_nodes`] uses (`◆`/`├─`/`└─`, `label  agent  state  cwd` per
/// session row, plus the remote subtree [`subtree_lines`] draws under a row
/// that spawned off-machine), grouped by project bucket instead of by host.
fn render_groups(groups: &[ProjectGroup]) -> String {
    let mut out: Vec<String> = Vec::new();
    for g in groups {
        // The bucket name is a project's own name — or, for a cwd matching
        // neither, that cwd's basename off a session record that may have come
        // off a pulled document: cleaned like every other printed string.
        out.push(format!("◆ {}", shown(&g.name)));
        for (i, s) in g.sessions.iter().enumerate() {
            let last = i + 1 == g.sessions.len();
            out.push(session_line(if last { "└─ " } else { "├─ " }, s));
            out.extend(subtree_lines(s, &deeper("", last)));
        }
    }
    if out.is_empty() {
        return "(no local sessions, no nodes registered)".to_string();
    }
    out.join("\n")
}

fn group_json(g: &ProjectGroup) -> Value {
    json!({
        "name": shown(&g.name),
        "sessions": g.sessions.iter().map(session_view_json).collect::<Vec<_>>(),
    })
}

/// Everything bare `session`'s two renderings share before they diverge —
/// local sessions/hooks/projects loaded once (`common::load_inputs`), every
/// registered node probed LIVE exactly as the retired `who` command did
/// (module doc's "Presence model"). `host`/`--hosts` rendering and the
/// PROJECT rendering both call this and then diverge purely on how they
/// group/render `nodes`.
pub(super) struct Roster {
    pub(super) host: String,
    pub(super) projects: Vec<Project>,
    pub(super) locals: Vec<SessionRecord>,
    pub(super) nodes: Vec<NodeView>,
}

pub(super) fn collect_roster(cmd: &str, pull: PullFn) -> Result<Roster, Outcome> {
    let (p, s, h) = super::common::load_inputs(cmd)?;
    let host = aoide_storage::display::local_host_name();
    let nodes = aoide_storage::node_store::load_nodes();
    let local_node = build_local_node(&s.sessions, &h.hooks, &p.projects, &nodes, &host);

    let probed = probe_nodes(&nodes, pull);

    let mut nodes = vec![local_node];
    for (node, result) in probed {
        let cache = aoide_storage::node_store::load_node_cache(&node.name);
        nodes.push(build_mesh_node(&node, result, cache));
    }
    // Every fold is assembled before the join that needs all of them: the
    // remote subtree under a parent row is only reachable here, where both the
    // link and the far node's own sessions are in hand.
    attach_subtrees(&mut nodes);

    Ok(Roster { host, projects: p.projects, locals: s.sessions, nodes })
}

/// Per-node live-probe timeout (module doc's presence model — "short
/// per-node timeout ~2s"). One named constant rather than a magic number at
/// the two call sites that need it ([`session_roster`] below, and
/// `node_list.rs`'s own production entry — the SAME probe, so the SAME
/// bound).
pub(super) const NODE_PROBE_TIMEOUT_SECS: u64 = 2;

/// The testable core: everything `session`'s bare listing does EXCEPT
/// choosing the real `pull` closure. `--hosts` renders exactly what the
/// retired `who` command used to (byte-identical message/JSON shape);
/// without it, sessions group by PROJECT instead. `filter`/`--all` apply to
/// either grouping, narrowing `nodes` BEFORE the host/project split.
pub(super) fn session_roster_with(inv: &Invocation, pull: PullFn) -> Outcome {
    let cmd = "session";
    let Roster { host, projects, locals, mut nodes } = match collect_roster(cmd, pull) {
        Ok(r) => r,
        Err(e) => return e,
    };

    let filter = inv.args.first().map(|a| a.trim()).filter(|a| !a.is_empty());
    if let Some(f) = filter {
        nodes = apply_filter(f, &host, nodes, &locals);
    }

    if !inv.flag_present("all") {
        for n in &mut nodes {
            n.sessions.retain(|sv| sv.presence != "done");
        }
    }

    if inv.flag_present("hosts") {
        let total_sessions: usize = nodes.iter().map(|n| n.sessions.len()).sum();
        let message = format!(
            "{} node(s), {} session(s)\n{}",
            nodes.len(),
            total_sessions,
            render_nodes(&nodes)
        );
        let data = json!({
            "host": host,
            "generatedAt": aoide_storage::time::now_iso_utc(),
            "nodes": nodes.iter().map(node_json).collect::<Vec<_>>(),
        });
        Outcome::ok(cmd, message).with_data(data)
    } else {
        let groups = group_by_project(nodes, &projects);
        let total_sessions: usize = groups.iter().map(|g| g.sessions.len()).sum();
        let message = format!(
            "{} project(s), {} session(s)\n{}",
            groups.len(),
            total_sessions,
            render_groups(&groups)
        );
        let data = json!({
            "host": host,
            "generatedAt": aoide_storage::time::now_iso_utc(),
            "projects": groups.iter().map(group_json).collect::<Vec<_>>(),
        });
        Outcome::ok(cmd, message).with_data(data)
    }
}

/// `aoide session [filter] [--hosts] [--json] [--all]` — the real entry
/// point: wires the live probe to `aoide_client::commands::pull_node_live`
/// (the SAME transport `node pull` uses, per the crate's `Cargo.toml` note
/// on the `conduct → client` edge) and hands off to
/// [`session_roster_with`].
pub fn session_roster(inv: &Invocation) -> Outcome {
    let pull: PullFn = Arc::new(|p: &Node| aoide_client::commands::pull_node_live(p, NODE_PROBE_TIMEOUT_SECS, None));
    session_roster_with(inv, pull)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::testutil::*;
    use std::time::Duration;

    fn node(name: &str) -> Node {
        Node {
            name: name.to_string(),
            url: format!("http://{name}/"),
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: None,
            verified: false,
            grants: aoide_storage::node_store::Grants::new(),
            narrowed: aoide_storage::node_store::Grants::new(),
            via: None,
            added_at: "2026-08-14T00:00:00Z".to_string(),
        }
    }

    fn cache(name: &str, fetched_at: &str, graph: Value) -> NodeCacheEntry {
        NodeCacheEntry {
            schema_version: "0".to_string(),
            name: name.to_string(),
            instance: None,
            graph: Some(graph),
            fetched_at: Some(fetched_at.to_string()),
            stale: false,
            last_error: None,
        }
    }

    fn node_graph(sessions: &[(&str, &str, &str, Option<&str>)]) -> Value {
        // (sessionId, state, cwd, petname)
        let nodes: Vec<Value> = sessions
            .iter()
            .map(|(id, state, cwd, petname)| {
                let mut n = json!({ "id": format!("session:{id}"), "kind": "session", "state": state, "cwd": cwd, "agent": "claude" });
                if let Some(p) = petname {
                    n["petname"] = json!(p);
                }
                n
            })
            .collect();
        json!({ "schemaVersion": "0", "nodes": nodes, "edges": [] })
    }

    fn project(name: &str, path: &str) -> Project {
        Project { name: name.to_string(), path: path.to_string(), ..Default::default() }
    }

    // ── session_presence: the three-way session classification ──────────

    #[test]
    fn session_presence_classifies_the_five_canonical_states() {
        assert_eq!(session_presence("working"), "online");
        assert_eq!(session_presence("awaiting"), "online");
        assert_eq!(session_presence("idle"), "online");
        assert_eq!(session_presence("stopped"), "stale");
        assert_eq!(session_presence("done"), "done");
        // Never invents a worse signal for anything unrecognized.
        assert_eq!(session_presence("mystery"), "online");
    }

    // ── sessions_from_graph: role derived from the SAME document's edges ──

    #[test]
    fn sessions_from_graph_derives_role_from_spawned_edges() {
        let graph = json!({
            "schemaVersion": "0",
            "nodes": [
                { "id": "session:root1", "kind": "session", "state": "working", "cwd": "/x", "agent": "claude", "petname": "brave-otter" },
                { "id": "session:child1", "kind": "session", "state": "idle", "cwd": "/x", "agent": "claude" },
                { "id": "project:aoide", "kind": "project", "name": "aoide", "path": "/x" },
            ],
            "edges": [
                { "from": "session:root1", "to": "session:child1", "kind": "spawned" },
            ],
        });
        let sessions = sessions_from_graph(&graph, "yomi-strix");
        assert_eq!(sessions.len(), 2, "the project node is not a session");
        let root = sessions.iter().find(|s| s.session_id == "root1").unwrap();
        assert_eq!(root.label, "yomi-strix/root/brave-otter (…oot1)");
        assert_eq!(root.presence, "online");
        let child = sessions.iter().find(|s| s.session_id == "child1").unwrap();
        assert_eq!(child.label, "yomi-strix/child/child1");
        assert_eq!(child.presence, "online");
        // The row JSON carries the same-node parent, and only when there is one.
        assert_eq!(session_view_json(child)["parentSessionId"], "root1");
        assert!(session_view_json(root).get("parentSessionId").is_none());
    }

    // ── build_mesh_node: the pure probe-outcome + cache classifier ───────

    fn remote_link_json(v: &Value) -> (String, String) {
        (
            v["node"].as_str().unwrap().to_string(),
            v["sessionId"].as_str().unwrap().to_string(),
        )
    }

    #[test]
    fn the_local_rows_carry_the_remote_link_from_the_record_and_the_ledger() {
        // P-RSA S4, this box's own sessions: `remoteParent` off the record,
        // `remoteChildren` off the caller-side ledger (matched on the parent's
        // own id), the node name resolved through the key — so a rename of the
        // node record re-labels both directions.
        let env = Env::set_up("remote-roster-local");
        let key = "ab".repeat(32);
        aoide_storage::node_store::save_nodes(&[aoide_storage::node_store::Node {
            name: "nodeb-renamed".into(),
            url: "http://nodeb/".into(),
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: Some(key.clone()),
            verified: true,
            grants: aoide_storage::node_store::Grants::new(),
            narrowed: aoide_storage::node_store::Grants::new(),
            via: None,
            added_at: "2026-08-14T00:00:00Z".into(),
        }])
        .unwrap();
        aoide_storage::remote_children::append_remote_child(&aoide_storage::remote_children::RemoteChild {
            parent_session_id: "par1".into(),
            node: "nodeb".into(),
            key: key.clone(),
            session_id: "C".into(),
            spawned_at: "2026-09-25T00:00:00Z".into(),
            lines_after: 0,
            drained: false,
            extra: Default::default(),
        })
        .unwrap();

        let mut spawned = session("par1", "/x", "working", "1", None);
        spawned.petname = Some("brave-otter".into());
        let mut far_child = session("child-1", "/x", "idle", "2", None);
        far_child.remote_parent = Some(aoide_storage::records::RemoteParent {
            node: "nodeb".into(),
            key: key.clone(),
            session_id: "par9".into(),
            extra: Default::default(),
        });

        let node = build_local_node(
            &[spawned, far_child],
            &[],
            &[],
            &aoide_storage::node_store::load_nodes(),
            "sakaki",
        );
        let par1 = node.sessions.iter().find(|s| s.session_id == "par1").unwrap();
        assert_eq!(par1.remote_parent, None);
        assert_eq!(
            par1.remote_children,
            vec![RemoteChild {
                node: "nodeb-renamed".into(),
                session_id: "C".into(),
                fold: None,
                subtree: Vec::new(),
            }],
            "the ledger row's node label resolves to the CURRENT name"
        );
        let child = node.sessions.iter().find(|s| s.session_id == "child-1").unwrap();
        assert_eq!(
            child.remote_parent,
            Some(RemoteLink { node: "nodeb-renamed".into(), session_id: "par9".into() })
        );
        assert!(child.remote_children.is_empty());
        assert_eq!(child.parent, None, "a remote parent never becomes a local `parent`");

        // The shared row JSON: both keys present only when known, and no key.
        let par_row = session_view_json(par1);
        assert_eq!(remote_link_json(&par_row["remoteChildren"][0]), ("nodeb-renamed".into(), "C".into()));
        assert!(par_row["remoteChildren"][0].get("key").is_none());
        assert!(par_row.get("remoteParent").is_none());
        let child_row = session_view_json(child);
        assert_eq!(remote_link_json(&child_row["remoteParent"]), ("nodeb-renamed".into(), "par9".into()));
        assert!(child_row.get("remoteChildren").is_none());

        // The roster line, both markers.
        let text = render_nodes(&[node]);
        assert!(text.contains("↓ 1 remote"), "parent side: {text}");
        assert!(text.contains("↑ nodeb-renamed/par9"), "child side: {text}");
        drop(env);
    }

    #[test]
    fn a_remote_node_rows_remote_link_is_read_back_verbatim_and_drops_a_blank_one() {
        // The far document already resolved its names against ITS registry —
        // these are read back, never re-resolved here. A link with no session
        // id names nothing and is dropped rather than rendered as a blank row.
        let graph = json!({
            "nodes": [
                { "id": "session:C", "kind": "session", "state": "working", "cwd": "/srv", "agent": "claude",
                  "remoteParent": { "node": "yomi", "sessionId": "par1" },
                  "remoteChildren": [
                    { "node": "sakaki", "sessionId": "G" },
                    { "node": "sakaki" },
                    { "node": "sakaki", "sessionId": "" },
                  ] },
            ],
            "edges": [],
        });
        let sessions = sessions_from_graph(&graph, "nodeb");
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0].remote_parent,
            Some(RemoteLink { node: "yomi".into(), session_id: "par1".into() })
        );
        assert_eq!(
            sessions[0].remote_children,
            vec![RemoteChild {
                node: "sakaki".into(),
                session_id: "G".into(),
                fold: None,
                subtree: Vec::new(),
            }],
            "the two blank links are dropped, the shaped one survives"
        );
        let row = session_view_json(&sessions[0]);
        assert_eq!(remote_link_json(&row["remoteParent"]), ("yomi".into(), "par1".into()));
        assert_eq!(row["remoteChildren"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn a_hostile_documents_own_text_is_stripped_and_bounded_on_every_rendered_path() {
        // A paired node writes its own `graphSummary`; nothing stops it putting
        // an escape sequence, a bidi override, a newline or a megabyte in its own
        // `cwd`/`agent`/`petname` and in both link endpoints. Every render path —
        // the two groupings AND the JSON the conductor paints — must hand the
        // terminal (and the widget) one clean, bounded line.
        let esc = "\u{1b}[31mred\u{1b}[0m";
        let bidi = "\u{202e}gpj.exe";
        let huge = "x".repeat(1_000_000);
        let graph = json!({
            "schemaVersion": "0",
            "nodes": [{
                "id": "session:C", "kind": "session", "state": "working",
                "cwd": format!("{bidi}/srv/{esc}{huge}"),
                "agent": esc,
                "petname": "brave\n\totter",
                "title": esc,
                "model": esc,
                "remoteParent": { "node": esc, "sessionId": esc },
                "remoteChildren": [{ "node": esc, "sessionId": bidi }],
            }],
            "edges": [],
        });
        let sessions = sessions_from_graph(&graph, "yomi-strix");
        let row = &sessions[0];
        let rendered = render_nodes(&[build_mesh_node(&node("yomi-strix"), Ok(graph.clone()), None)]);
        // The same cwd's basename becomes a project bucket name in the OTHER
        // grouping — a string derived off the same pulled field, so it is
        // cleaned at the render and in the JSON too.
        let grouped = render_groups(&group_by_project(
            vec![build_mesh_node(&node("yomi-strix"), Ok(graph.clone()), None)],
            &[],
        ));

        let json_row = session_view_json(row);
        let fields: Vec<Value> = vec![
            json_row["label"].clone(),
            json_row["cwd"].clone(),
            json_row["agent"].clone(),
            json_row["petname"].clone(),
            json_row["title"].clone(),
            json_row["model"].clone(),
            json_row["remoteParent"].clone(),
            json_row["remoteChildren"].clone(),
        ];
        let mut seen: Vec<String> = rendered.lines().map(str::to_string).collect();
        seen.extend(grouped.lines().map(str::to_string));
        seen.push(json_row.to_string());
        seen.extend(fields.iter().map(Value::to_string));
        for s in &seen {
            assert!(
                !s.chars().any(|c| c.is_control() || c == '\u{202e}'),
                "a control character or bidi override reached the terminal: {s:?}"
            );
        }
        // Bounded at the field, not only in aggregate: every string off the
        // document is at most one clipped line, so the row and the JSON that
        // carry them all are bounded with them.
        for f in &fields {
            assert!(
                f.to_string().chars().count() <= crate::graph::common::LINE_MAX + 16,
                "one bounded line per field: {f}"
            );
        }
        // The two new fields specifically: the 1 MB `cwd` is clipped (not
        // dropped), the escape-marked agent keeps its literal text, and the
        // endpoint the doc published is what both the tag and the JSON print.
        let cwd = json_row["cwd"].as_str().unwrap();
        assert!(cwd.ends_with('…'), "clipped, never truncated silently: {cwd:?}");
        assert_eq!(json_row["agent"], json!("[31mred[0m"));
        assert!(rendered.contains("[31mred[0m"), "the row line is cleaned too: {rendered}");
        assert!(rendered.contains("gpj.exe"), "the bidi override is gone, the text stays: {rendered}");
    }

    #[test]
    fn build_mesh_node_online_when_the_live_probe_succeeds() {
        let p = node("yomi-strix");
        let graph = node_graph(&[("s1", "working", "/x", Some("brave-otter"))]);
        let node = build_mesh_node(&p, Ok(graph), None);
        assert_eq!(node.presence, "online");
        assert!(node.fetched_at.is_none());
        assert!(node.error.is_none());
        assert_eq!(node.sessions.len(), 1);
        assert_eq!(node.sessions[0].session_id, "s1");
    }

    #[test]
    fn build_mesh_node_unreachable_falls_back_to_the_cache() {
        let p = node("yomi-strix");
        let graph = node_graph(&[("s1", "idle", "/x", None)]);
        let node = build_mesh_node(&p, Err("HTTP 000".to_string()), Some(cache("yomi-strix", "2026-08-14T00:05:00Z", graph)));
        assert_eq!(node.presence, "unreachable");
        assert_eq!(node.fetched_at.as_deref(), Some("2026-08-14T00:05:00Z"));
        assert_eq!(node.error.as_deref(), Some("HTTP 000"));
        assert_eq!(node.sessions.len(), 1, "last-known sessions still surface");
    }

    #[test]
    fn build_mesh_node_never_pulled_when_probe_fails_and_no_cache_exists() {
        let p = node("ghost");
        let node = build_mesh_node(&p, Err("could not reach the agent".to_string()), None);
        assert_eq!(node.presence, "never-pulled");
        assert!(node.fetched_at.is_none());
        assert!(node.sessions.is_empty());
    }

    // ── cached_presence: the unreachable-host session gate (P-14 M2) ─────

    #[test]
    fn cached_sessions_of_an_unreachable_host_read_last_seen_not_online() {
        let p = node("chiyo");
        let graph = node_graph(&[("s1", "working", "/x", None)]);
        let node = build_mesh_node(&p, Err("HTTP 000".to_string()), Some(cache("chiyo", "2026-08-14T00:05:00Z", graph)));
        assert_eq!(node.presence, "unreachable");
        assert_eq!(node.sessions[0].presence, "last-seen", "a cached session must never claim to be live under an unreachable header");
    }

    #[test]
    fn a_cached_done_session_stays_done_under_an_unreachable_host() {
        let p = node("chiyo");
        let graph = node_graph(&[("s1", "done", "/x", None)]);
        let node = build_mesh_node(&p, Err("HTTP 000".to_string()), Some(cache("chiyo", "2026-08-14T00:05:00Z", graph)));
        assert_eq!(node.sessions[0].presence, "done", "both node list and session filter on this exact string");
    }

    #[test]
    fn a_cache_without_a_fetched_at_reads_unknown() {
        let p = node("chiyo");
        let graph = node_graph(&[("s1", "idle", "/x", None)]);
        let mut entry = cache("chiyo", "2026-08-14T00:05:00Z", graph);
        entry.fetched_at = None;
        let node = build_mesh_node(&p, Err("HTTP 000".to_string()), Some(entry));
        assert_eq!(node.sessions[0].presence, "unknown");
    }

    #[test]
    fn a_live_hosts_sessions_keep_their_own_presence() {
        let p = node("chiyo");
        let graph = node_graph(&[("s1", "working", "/x", None), ("s2", "done", "/x", None)]);
        let node = build_mesh_node(&p, Ok(graph), None);
        assert_eq!(node.presence, "online");
        let s1 = node.sessions.iter().find(|s| s.session_id == "s1").unwrap();
        let s2 = node.sessions.iter().find(|s| s.session_id == "s2").unwrap();
        assert_eq!(s1.presence, "online");
        assert_eq!(s2.presence, "done");
    }

    // ── enrichment: title/model/kind/parent ride only when published ─────

    #[test]
    fn enrichment_rides_only_when_the_node_published_it() {
        // Local, absent: a bare record publishes nothing extra.
        let bare = session("s1", "/x", "idle", "1", None);
        let node = build_local_node(&[bare], &[], &[], &[], "sakaki");
        let s = &node.sessions[0];
        assert!(s.title.is_none() && s.model.is_none() && s.kind.is_none() && s.parent.is_none());

        // Local, present: the record's own fields ride through, and a
        // resolved parent rides as the bare (not node-scoped) id.
        let rich = SessionRecord {
            title: Some("fix the thing".to_string()),
            model: Some("claude-sonnet-5".to_string()),
            kind: Some("agent".to_string()),
            ..session("s2", "/x", "idle", "2", None)
        };
        let child = session("s3", "/x", "idle", "3", Some("s2"));
        let node = build_local_node(&[rich, child], &[], &[], &[], "sakaki");
        let s2 = node.sessions.iter().find(|s| s.session_id == "s2").unwrap();
        assert_eq!(s2.title.as_deref(), Some("fix the thing"));
        assert_eq!(s2.model.as_deref(), Some("claude-sonnet-5"));
        assert_eq!(s2.kind.as_deref(), Some("agent"));
        assert!(s2.parent.is_none());
        let s3 = node.sessions.iter().find(|s| s.session_id == "s3").unwrap();
        assert_eq!(s3.parent.as_deref(), Some("s2"));

        // Remote, absent: a node's own graph document with no title/model/
        // role key and no spawned edge publishes nothing extra either.
        let bare_graph = json!({
            "schemaVersion": "0",
            "nodes": [{ "id": "session:r1", "kind": "session", "state": "working", "cwd": "/x", "agent": "claude" }],
            "edges": [],
        });
        let bare_remote = &sessions_from_graph(&bare_graph, "yomi-strix")[0];
        assert!(bare_remote.title.is_none() && bare_remote.model.is_none() && bare_remote.kind.is_none() && bare_remote.parent.is_none());

        // Remote, present: `title`/`model`/`role` (read back as `kind`) ride
        // through, and the SAME `spawned` edge role derivation already scans
        // supplies `parent` as the edge's `from` minus its `session:` prefix.
        let rich_graph = json!({
            "schemaVersion": "0",
            "nodes": [
                { "id": "session:root1", "kind": "session", "state": "working", "cwd": "/x", "agent": "claude", "title": "ship it", "model": "claude-opus-5", "role": "agent" },
                { "id": "session:child1", "kind": "session", "state": "idle", "cwd": "/x", "agent": "claude" },
            ],
            "edges": [
                { "from": "session:root1", "to": "session:child1", "kind": "spawned" },
            ],
        });
        let rich_remote = sessions_from_graph(&rich_graph, "yomi-strix");
        let root = rich_remote.iter().find(|s| s.session_id == "root1").unwrap();
        assert_eq!(root.title.as_deref(), Some("ship it"));
        assert_eq!(root.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(root.kind.as_deref(), Some("agent"));
        assert!(root.parent.is_none());
        let child = rich_remote.iter().find(|s| s.session_id == "child1").unwrap();
        assert_eq!(child.parent.as_deref(), Some("root1"));
    }

    // ── probe_nodes: parallel probe, results correctly paired by node ────

    #[test]
    fn probe_nodes_pairs_every_result_with_its_own_node_regardless_of_completion_order() {
        let nodes = vec![node("alpha"), node("beta"), node("gamma")];
        let pull: PullFn = Arc::new(|p: &Node| {
            if p.name == "beta" {
                Err("down".to_string())
            } else {
                Ok(json!({ "nodes": [], "edges": [] }))
            }
        });
        let results = probe_nodes(&nodes, pull);
        assert_eq!(results.len(), 3);
        let by_name: std::collections::HashMap<_, _> =
            results.into_iter().map(|(p, r)| (p.name, r)).collect();
        assert!(by_name["alpha"].is_ok());
        assert!(by_name["beta"].is_err());
        assert!(by_name["gamma"].is_ok());
    }

    // ── exempt (task #20): build_local_node carries the flag, both text
    // renders tag the row ──────────────────────────────────────────────

    #[test]
    fn build_local_node_carries_the_exempt_flag_off_the_record() {
        let sessions = vec![
            SessionRecord { exempt: true, ..session("s1", "/x", "idle", "1", None) },
            session("s2", "/x", "idle", "2", None),
        ];
        let node = build_local_node(&sessions, &[], &[], &[], "sakaki");
        let s1 = node.sessions.iter().find(|s| s.session_id == "s1").unwrap();
        let s2 = node.sessions.iter().find(|s| s.session_id == "s2").unwrap();
        assert!(s1.exempt);
        assert!(!s2.exempt);
    }

    #[test]
    fn render_nodes_tags_an_exempt_row_and_leaves_an_ordinary_one_bare() {
        let nodes = vec![NodeView {
            name: "sakaki".to_string(),
            is_local: true,
            presence: "online",
            fetched_at: None,
            error: None,
            stale: false,
            sessions: vec![
                SessionView { session_id: "s1".into(), label: "l1".into(), petname: None, agent: "claude".into(), state: "idle".into(), presence: "online", cwd: "/x".into(), project: None, effective_project: None, exempt: true, title: None, model: None, kind: None, parent: None, native_role: None, remote_parent: None, remote_children: Vec::new() },
                SessionView { session_id: "s2".into(), label: "l2".into(), petname: None, agent: "claude".into(), state: "idle".into(), presence: "online", cwd: "/x".into(), project: None, effective_project: None, exempt: false, title: None, model: None, kind: None, parent: None, native_role: None, remote_parent: None, remote_children: Vec::new() },
            ],
        }];
        let rendered = render_nodes(&nodes);
        let lines: Vec<&str> = rendered.lines().collect();
        assert!(lines[1].ends_with(" exempt"), "{}", lines[1]);
        assert!(!lines[2].ends_with(" exempt"), "{}", lines[2]);
    }

    #[test]
    fn render_groups_tags_an_exempt_row_the_same_way() {
        let groups = vec![ProjectGroup {
            name: "aoide".to_string(),
            sessions: vec![SessionView {
                session_id: "s1".into(),
                label: "l1".into(),
                petname: None,
                agent: "claude".into(),
                state: "idle".into(),
                presence: "online",
                cwd: "/x".into(), project: None,
                effective_project: None,
                exempt: true,
                title: None, model: None, kind: None, parent: None, native_role: None, remote_parent: None, remote_children: Vec::new(),
            }],
        }];
        let rendered = render_groups(&groups);
        assert!(rendered.lines().last().unwrap().ends_with(" exempt"), "{rendered}");
    }

    // ── apply_filter: local id / node+query / substring fallback ─────────

    fn sample_nodes() -> (Vec<NodeView>, Vec<SessionRecord>) {
        let locals = vec![
            session("sess-aaaa-1111", "/x", "working", "1", None),
            session("sess-bbbb-2222", "/x", "idle", "2", None),
        ];
        let mut locals = locals;
        locals[0].petname = Some("brave-otter".to_string());
        locals[1].petname = Some("calm-thorn".to_string());

        let local_node = build_local_node(&locals, &[], &[], &[], "sakaki");
        let mesh_node = NodeView {
            name: "yomi-strix".to_string(),
            is_local: false,
            presence: "online",
            fetched_at: None,
            error: None,
            stale: false,
            sessions: vec![SessionView {
                session_id: "sess-cccc-3333".to_string(),
                label: "yomi-strix/root/misty-comet (…3333)".to_string(),
                petname: Some("misty-comet".to_string()),
                agent: "claude".to_string(),
                state: "working".to_string(),
                presence: "online",
                cwd: "/y".to_string(), project: None,
                effective_project: None,
                exempt: false,
                title: None, model: None, kind: None, parent: None, native_role: None, remote_parent: None, remote_children: Vec::new(),
            }],
        };
        (vec![local_node, mesh_node], locals)
    }

    #[test]
    fn apply_filter_local_id_narrows_to_the_local_node_and_session_only() {
        let (nodes, locals) = sample_nodes();
        let out = apply_filter("brave-otter", "sakaki", nodes, &locals);
        assert_eq!(out.len(), 1, "the mesh node is dropped entirely");
        assert!(out[0].is_local);
        assert_eq!(out[0].sessions.len(), 1);
        assert_eq!(out[0].sessions[0].session_id, "sess-aaaa-1111");
    }

    #[test]
    fn apply_filter_node_slash_query_narrows_to_that_node_and_substring_matches() {
        let (nodes, locals) = sample_nodes();
        let out = apply_filter("yomi-strix/misty", "sakaki", nodes, &locals);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "yomi-strix");
        assert_eq!(out[0].sessions.len(), 1);
    }

    #[test]
    fn apply_filter_node_slash_with_no_matching_remainder_empties_that_nodes_sessions() {
        let (nodes, locals) = sample_nodes();
        let out = apply_filter("yomi-strix/nonexistent", "sakaki", nodes, &locals);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "yomi-strix");
        assert!(out[0].sessions.is_empty());
    }

    #[test]
    fn apply_filter_notfound_substring_keeps_a_name_matched_node_whole() {
        let (nodes, locals) = sample_nodes();
        let out = apply_filter("yomi", "sakaki", nodes, &locals);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "yomi-strix");
        assert_eq!(out[0].sessions.len(), 1, "name match keeps every session, unfiltered");
    }

    #[test]
    fn apply_filter_notfound_substring_drops_nodes_with_no_match_at_all() {
        let (nodes, locals) = sample_nodes();
        let out = apply_filter("nothing-matches-this", "sakaki", nodes, &locals);
        assert!(out.is_empty());
    }

    // ── project_bucket / group_by_project: attribution + trailing bucket ──

    #[test]
    fn project_bucket_prefers_a_registered_project_over_a_manifest() {
        let projects = vec![project("aoide", "/home/k/Aoide")];
        assert_eq!(project_bucket("/home/k/Aoide/pkgs/aoide", &projects), Some("aoide".to_string()));
    }

    #[test]
    fn project_bucket_falls_back_to_a_manifest_directorys_own_basename() {
        let root = std::env::temp_dir().join(format!("aoide-who-manifest-bucket-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        aoide_storage::manifest::save_manifest(&root, &aoide_storage::manifest::Manifest::default()).unwrap();

        let nested = root.join("sub");
        std::fs::create_dir_all(&nested).unwrap();
        let bucket = project_bucket(nested.to_str().unwrap(), &[]);
        assert_eq!(bucket, root.file_name().map(|n| n.to_string_lossy().into_owned()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn project_bucket_is_none_with_neither_a_registered_project_nor_a_manifest() {
        let root = std::env::temp_dir().join(format!("aoide-who-no-project-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(project_bucket(root.to_str().unwrap(), &[]), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn group_by_project_sorts_named_buckets_alphabetically_with_no_project_trailing() {
        let nodes = vec![NodeView {
            name: "sakaki".to_string(),
            is_local: true,
            presence: "online",
            fetched_at: None,
            error: None,
            stale: false,
            sessions: vec![
                SessionView { session_id: "s1".into(), label: "l1".into(), petname: None, agent: "claude".into(), state: "working".into(), presence: "online", cwd: "/z/nowhere".into(), project: None, effective_project: None, exempt: false, title: None, model: None, kind: None, parent: None, native_role: None, remote_parent: None, remote_children: Vec::new() },
                SessionView { session_id: "s2".into(), label: "l2".into(), petname: None, agent: "claude".into(), state: "working".into(), presence: "online", cwd: "/proj/zeta/x".into(), project: None, effective_project: None, exempt: false, title: None, model: None, kind: None, parent: None, native_role: None, remote_parent: None, remote_children: Vec::new() },
                SessionView { session_id: "s3".into(), label: "l3".into(), petname: None, agent: "claude".into(), state: "working".into(), presence: "online", cwd: "/proj/alpha/x".into(), project: None, effective_project: None, exempt: false, title: None, model: None, kind: None, parent: None, native_role: None, remote_parent: None, remote_children: Vec::new() },
            ],
        }];
        let projects = vec![project("zeta", "/proj/zeta"), project("alpha", "/proj/alpha")];
        let groups = group_by_project(nodes, &projects);
        let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "zeta", NO_PROJECT]);
        assert_eq!(groups[2].sessions[0].session_id, "s1");
    }

    #[test]
    fn group_by_project_attributes_a_remote_sessions_cwd_the_same_host_agnostic_way() {
        // A node's session cwd matching a LOCALLY-registered project's path
        // string (the fleet's shared-path convention, `grant.rs`'s own node
        // relativization leans on the same thing) attributes purely by
        // string match — no filesystem access, so it works identically for
        // a foreign host's cwd.
        let nodes = vec![NodeView {
            name: "yomi-strix".to_string(),
            is_local: false,
            presence: "online",
            fetched_at: None,
            error: None,
            stale: false,
            sessions: vec![SessionView {
                session_id: "r1".into(),
                label: "yomi-strix/root/r1".into(),
                petname: None,
                agent: "claude".into(),
                state: "working".into(),
                presence: "online",
                cwd: "/home/k/Aoide/pkgs/aoide".into(), project: None,
                effective_project: None,
                exempt: false,
                title: None, model: None, kind: None, parent: None, native_role: None, remote_parent: None, remote_children: Vec::new(),
            }],
        }];
        let projects = vec![project("aoide", "/home/k/Aoide")];
        let groups = group_by_project(nodes, &projects);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "aoide");
    }

    #[test]
    fn group_by_project_buckets_a_child_under_its_owners_project() {
        let projects = vec![project("aoide", "/home/k/Aoide")];
        let owner = SessionRecord { project: Some("aoide".to_string()), ..session("s-root", "/home/k/Aoide", "working", "1", None) };
        let child = session("s-child", "/tmp/elsewhere", "working", "2", Some("s-root"));
        let node = build_local_node(&[owner, child], &[], &projects, &[], "sakaki");
        let groups = group_by_project(vec![node], &projects);
        assert_eq!(groups.len(), 1, "the child's own cwd anchors nowhere, but it still lands in its owner's bucket");
        assert_eq!(groups[0].name, "aoide");
        let ids: Vec<&str> = groups[0].sessions.iter().map(|s| s.session_id.as_str()).collect();
        assert!(ids.contains(&"s-child"), "{ids:?}");
    }

    // ── attach_subtrees: the join that turns the link into a rendered chain ──

    /// The line index of the row whose text contains `needle`, plus its own
    /// indent — the two things "under its parent" means in a rendering.
    fn row_at(rendered: &str, needle: &str) -> (usize, usize, String) {
        let i = rendered
            .lines()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no row `{needle}` in:\n{rendered}"));
        let line = rendered.lines().nth(i).unwrap().to_string();
        (i, line.len() - line.trim_start_matches([' ', '│']).len(), line)
    }

    #[test]
    fn the_remote_chain_renders_under_its_parent_from_b_graph_and_the_json_agrees() {
        // P-RSA S4's HIGH finding: `remoteChildren` + the fold were true as
        // DATA and rendered nowhere. `par1 → nodeb/C → nodeb/G`, where C and G
        // are B's own records: C spawned by par1 over the door, G by C on B.
        // The fixture is B's OWN resolved document (the bytes
        // `aoide/graphSummary` serves), so nothing here is a wire fiction — and
        // the join has three outcomes, one per fold state: fresh (B answered),
        // stale (B is unreachable and the cache behind it is past its TTL), and
        // absent (no fold at all) — the last two shown, never hidden.
        let _env = Env::set_up("remote-subtree-view");

        let mut c = session("C", "/srv/work", "running", "1", None);
        c.remote_parent = Some(aoide_storage::records::RemoteParent {
            node: "yomi".into(),
            key: "ab".repeat(32),
            session_id: "par1".into(),
            extra: Default::default(),
        });
        let g = session("G", "/srv/work", "running", "2", Some("C"));
        let b_doc = crate::graph::doc::build_graph(&[], &[c, g], &[]);

        aoide_storage::remote_children::append_remote_child(
            &aoide_storage::remote_children::RemoteChild {
                parent_session_id: "par1".into(),
                node: "nodeb".into(),
                key: "cd".repeat(32),
                session_id: "C".into(),
                spawned_at: "2026-09-25T00:00:00Z".into(),
                drained: false,
                lines_after: 0,
                extra: Default::default(),
            },
        )
        .unwrap();

        let local = || {
            build_local_node(
                &[session("par1", "/home/khoa", "running", "1", None)],
                &[],
                &[],
                &[],
                "yomi",
            )
        };
        let joined = |far: Option<NodeView>| {
            let mut nodes = vec![local()];
            nodes.extend(far);
            attach_subtrees(&mut nodes);
            nodes
        };
        let child_json_of = |nodes: &[NodeView]| node_json(&nodes[0])["sessions"][0]["remoteChildren"][0].clone();

        // ── fresh: B answered the probe ──
        let nodes = joined(Some(build_mesh_node(&node("nodeb"), Ok(b_doc.clone()), None)));
        let rendered = render_nodes(&nodes);
        let (c_at, c_indent, _) = row_at(&rendered, "nodeb/C");
        let (g_at, g_indent, _) = row_at(&rendered, "nodeb/G");
        assert!(c_at < g_at, "C's own descendant comes after it:\n{rendered}");
        assert!(
            g_indent > c_indent,
            "G is indented under C ({g_indent} > {c_indent}):\n{rendered}"
        );
        let par_at = row_at(&rendered, "par1").0;
        assert!(par_at < c_at, "the whole subtree is under par1's own row:\n{rendered}");
        assert!(!rendered.contains("(fresh)"), "a live fold is not marked:\n{rendered}");
        let fresh = child_json_of(&nodes);
        assert_eq!(fresh["fold"], "fresh");
        assert_eq!(fresh["node"], "nodeb");
        assert_eq!(fresh["sessionId"], "C");
        assert_eq!(fresh["subtree"][0]["sessionId"], "G", "the view and the JSON agree");

        // ── stale: the cache is all B left behind, and it is past the TTL ──
        let nodes = joined(Some(build_mesh_node(
            &node("nodeb"),
            Err("HTTP 000".into()),
            Some(cache("nodeb", "2020-01-01T00:00:00Z", b_doc.clone())),
        )));
        let rendered = render_nodes(&nodes);
        assert!(
            rendered.contains("nodeb/C (stale)"),
            "the fold's own word marks a TTL-expired row:\n{rendered}"
        );
        assert!(rendered.contains("nodeb/G"), "the subtree still renders:\n{rendered}");
        assert!(child_json_of(&nodes)["fold"] == "stale");

        // ── absent: the row names a node this box has no fold for ──
        let nodes = joined(None);
        let rendered = render_nodes(&nodes);
        assert!(
            rendered.contains("nodeb/C (not pulled)"),
            "a link with no fold behind it is shown alone, never hidden:\n{rendered}"
        );
        let missing = child_json_of(&nodes);
        assert_eq!(missing["fold"], "not pulled");
        assert_eq!(missing["subtree"], json!([]), "no fold, no rows under it");
    }

    // ── session_roster_with: the full pipeline, injected pull, real local stage I/O ──

    struct Env {
        _guard: std::sync::MutexGuard<'static, ()>,
        stage: std::path::PathBuf,
        state: std::path::PathBuf,
        saved_stage: Option<String>,
        saved_state: Option<String>,
    }
    impl Env {
        fn set_up(tag: &str) -> Self {
            let guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
            let saved_stage = std::env::var("AOIDE_STAGE_DIR").ok();
            let saved_state = std::env::var("AOIDE_STATE_DIR").ok();
            let stage = unique_stage(tag);
            let state = std::env::temp_dir().join(format!(
                "aoide-who-state-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
            ));
            std::env::set_var("AOIDE_STAGE_DIR", &stage);
            std::env::set_var("AOIDE_STATE_DIR", &state);
            Env { _guard: guard, stage, state, saved_stage, saved_state }
        }
    }
    impl Drop for Env {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.stage);
            let _ = std::fs::remove_dir_all(&self.state);
            match &self.saved_stage {
                Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
                None => std::env::remove_var("AOIDE_STAGE_DIR"),
            }
            match &self.saved_state {
                Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
                None => std::env::remove_var("AOIDE_STATE_DIR"),
            }
        }
    }

    fn never_called_pull() -> PullFn {
        Arc::new(|_: &Node| panic!("no nodes registered — pull must never be called"))
    }

    fn hosts_invocation(args: &[&str], flags: &[(&str, &str)]) -> Invocation {
        let mut f: std::collections::BTreeMap<String, String> =
            std::collections::BTreeMap::from([("hosts".to_string(), "true".to_string())]);
        for (k, v) in flags {
            f.insert(k.to_string(), v.to_string());
        }
        Invocation {
            path: vec!["session".into()],
            args: args.iter().map(|s| s.to_string()).collect(),
            flags: f,
            door: aoide_protocol::Door::Cli,
        }
    }

    #[test]
    fn hosts_mode_reports_local_sessions_with_no_nodes_registered() {
        let _env = Env::set_up("no-nodes");
        let sf = super::super::model::SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![session("s1", "/x", "working", "1", None)],
        };
        super::super::model::write_stage(&super::super::model::sessions_path(), &sf).unwrap();

        let out = session_roster_with(&hosts_invocation(&[], &[]), never_called_pull());
        assert_eq!(out.status, aoide_protocol::output::Status::Ok);
        let data = out.data.unwrap();
        let nodes = data["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 1, "local node only");
        assert_eq!(nodes[0]["presence"], "online");
        assert_eq!(nodes[0]["sessions"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn hosts_mode_omits_done_sessions_unless_all_is_passed() {
        let _env = Env::set_up("done-omitted");
        let sf = super::super::model::SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![
                session("s1", "/x", "working", "1", None),
                session("s2", "/x", "done", "2", None),
            ],
        };
        super::super::model::write_stage(&super::super::model::sessions_path(), &sf).unwrap();

        let out = session_roster_with(&hosts_invocation(&[], &[]), never_called_pull());
        let data = out.data.unwrap();
        assert_eq!(data["nodes"][0]["sessions"].as_array().unwrap().len(), 1, "done is omitted");

        let out_all = session_roster_with(&hosts_invocation(&[], &[("all", "true")]), never_called_pull());
        let data_all = out_all.data.unwrap();
        assert_eq!(data_all["nodes"][0]["sessions"].as_array().unwrap().len(), 2, "--all keeps done");
    }

    #[test]
    fn hosts_mode_probes_every_registered_node_and_classifies_by_outcome() {
        let _env = Env::set_up("nodes-probed");
        aoide_storage::node_store::save_nodes(&[node("alpha"), node("beta")]).unwrap();
        // `beta` has a stale cache to fall back on; `alpha` has none.
        aoide_storage::node_store::save_node_cache(&cache(
            "beta",
            "2026-08-14T00:00:00Z",
            node_graph(&[("r1", "idle", "/z", None)]),
        ))
        .unwrap();

        let pull: PullFn = Arc::new(|p: &Node| {
            if p.name == "alpha" {
                Ok(node_graph(&[("r2", "working", "/a", None)]))
            } else {
                Err("unreachable".to_string())
            }
        });
        let out = session_roster_with(&hosts_invocation(&[], &[]), pull);
        let data = out.data.unwrap();
        let nodes: Vec<&Value> = data["nodes"].as_array().unwrap().iter().collect();
        assert_eq!(nodes.len(), 3, "local + alpha + beta");

        let alpha = nodes.iter().find(|n| n["name"] == "alpha").unwrap();
        assert_eq!(alpha["presence"], "online");
        assert_eq!(alpha["sessions"].as_array().unwrap().len(), 1);

        let beta = nodes.iter().find(|n| n["name"] == "beta").unwrap();
        assert_eq!(beta["presence"], "unreachable");
        assert_eq!(beta["fetchedAt"], "2026-08-14T00:00:00Z");
        assert_eq!(beta["sessions"].as_array().unwrap().len(), 1, "last-known session still shown");
    }

    #[test]
    fn hosts_mode_filter_never_changes_which_nodes_get_probed() {
        let _env = Env::set_up("filter-probes-all");
        aoide_storage::node_store::save_nodes(&[node("alpha"), node("beta")]).unwrap();
        let probed = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let probed2 = Arc::clone(&probed);
        let pull: PullFn = Arc::new(move |p: &Node| {
            probed2.lock().unwrap().push(p.name.clone());
            Ok(json!({ "nodes": [], "edges": [] }))
        });
        // A filter that only displays "alpha" must still have probed "beta".
        let out = session_roster_with(&hosts_invocation(&["alpha"], &[]), pull);
        let mut names = probed.lock().unwrap().clone();
        names.sort();
        assert_eq!(names, vec!["alpha".to_string(), "beta".to_string()], "every node was probed");
        let data = out.data.unwrap();
        let nodes = data["nodes"].as_array().unwrap();
        assert!(nodes.iter().all(|n| n["name"] != "beta"), "but only alpha is displayed");
    }

    // ── session_roster_with: PROJECT grouping (bare, no --hosts) ──────────

    fn project_invocation() -> Invocation {
        flag_invocation(&["session"], &[])
    }

    #[test]
    fn bare_mode_groups_local_sessions_by_registered_project() {
        let _env = Env::set_up("project-grouping");
        let sf = super::super::model::SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![session("s1", "/proj/aoide/sub", "working", "1", None)],
        };
        super::super::model::write_stage(&super::super::model::sessions_path(), &sf).unwrap();
        let pf = super::super::model::ProjectsFile {
            schema_version: "0".to_string(),
            projects: vec![project("aoide", "/proj/aoide")],
        };
        super::super::model::write_stage(&super::super::model::projects_path(), &pf).unwrap();

        let out = session_roster_with(&project_invocation(), never_called_pull());
        assert_eq!(out.status, aoide_protocol::output::Status::Ok);
        let data = out.data.unwrap();
        let projects = data["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["name"], "aoide");
        assert_eq!(projects[0]["sessions"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn bare_mode_groups_a_manifest_only_dir_by_its_own_basename() {
        let _env = Env::set_up("project-grouping-manifest");
        let manifest_root = std::env::temp_dir().join(format!("aoide-who-manifest-only-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&manifest_root);
        std::fs::create_dir_all(&manifest_root).unwrap();
        aoide_storage::manifest::save_manifest(&manifest_root, &aoide_storage::manifest::Manifest::default()).unwrap();

        let cwd = manifest_root.to_string_lossy().into_owned();
        let sf = super::super::model::SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![session("s1", &cwd, "working", "1", None)],
        };
        super::super::model::write_stage(&super::super::model::sessions_path(), &sf).unwrap();

        let out = session_roster_with(&project_invocation(), never_called_pull());
        let data = out.data.unwrap();
        let projects = data["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["name"], manifest_root.file_name().unwrap().to_string_lossy().to_string());

        let _ = std::fs::remove_dir_all(&manifest_root);
    }

    #[test]
    fn bare_mode_buckets_a_session_matching_neither_attribution_as_no_project() {
        let _env = Env::set_up("project-grouping-none");
        let lonely = std::env::temp_dir().join(format!("aoide-who-lonely-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&lonely);
        std::fs::create_dir_all(&lonely).unwrap();

        let cwd = lonely.to_string_lossy().into_owned();
        let sf = super::super::model::SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![session("s1", &cwd, "working", "1", None)],
        };
        super::super::model::write_stage(&super::super::model::sessions_path(), &sf).unwrap();

        let out = session_roster_with(&project_invocation(), never_called_pull());
        let data = out.data.unwrap();
        let projects = data["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["name"], NO_PROJECT);

        let _ = std::fs::remove_dir_all(&lonely);
    }

    #[test]
    fn bare_mode_attributes_a_remote_session_to_a_registered_project_by_cwd() {
        let _env = Env::set_up("project-grouping-remote");
        let pf = super::super::model::ProjectsFile {
            schema_version: "0".to_string(),
            projects: vec![project("aoide", "/home/k/Aoide")],
        };
        super::super::model::write_stage(&super::super::model::projects_path(), &pf).unwrap();
        aoide_storage::node_store::save_nodes(&[node("yomi-strix")]).unwrap();

        let pull: PullFn = Arc::new(|_: &Node| Ok(node_graph(&[("r1", "working", "/home/k/Aoide/pkgs/aoide", None)])));
        let out = session_roster_with(&project_invocation(), pull);
        let data = out.data.unwrap();
        let projects = data["projects"].as_array().unwrap();
        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0]["name"], "aoide");
        assert_eq!(projects[0]["sessions"][0]["sessionId"], "r1");
    }

    // ── `session --json` rows: additive `effectiveProject` (S-A2) ────────

    #[test]
    fn session_json_rows_carry_effective_project_beside_the_stored_one() {
        let _env = Env::set_up("effective-project-json");
        let sf = super::super::model::SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![
                SessionRecord { project: Some("aoide".to_string()), ..session("s-root", "/home/k/Aoide", "working", "1", None) },
                SessionRecord { project: Some("other".to_string()), ..session("s-child", "/tmp/elsewhere", "working", "2", Some("s-root")) },
            ],
        };
        super::super::model::write_stage(&super::super::model::sessions_path(), &sf).unwrap();
        let pf = super::super::model::ProjectsFile {
            schema_version: "0".to_string(),
            projects: vec![project("aoide", "/home/k/Aoide"), project("other", "/home/k/Other")],
        };
        super::super::model::write_stage(&super::super::model::projects_path(), &pf).unwrap();

        let out = session_roster_with(&hosts_invocation(&[], &[]), never_called_pull());
        let data = out.data.unwrap();
        let sessions = data["nodes"][0]["sessions"].as_array().unwrap();
        let child = sessions.iter().find(|s| s["sessionId"] == "s-child").unwrap();
        // The child's own explicit choice is never overridden by its owner —
        // both keys carry it, equal to each other.
        assert_eq!(child["project"], "other");
        assert_eq!(child["effectiveProject"], "other");
    }

    #[test]
    fn remote_rows_never_carry_effective_project() {
        let _env = Env::set_up("effective-project-remote");
        aoide_storage::node_store::save_nodes(&[node("yomi-strix")]).unwrap();
        // The remote node's own graph.json publishes a resolvable stored
        // `project` on its session — `sessions_from_graph` has no owner
        // chain to walk (no local session slice for a remote document), so
        // `effectiveProject` must stay absent regardless.
        let pull: PullFn = Arc::new(|_: &Node| {
            Ok(json!({
                "schemaVersion": "0",
                "nodes": [{ "id": "session:r1", "kind": "session", "state": "working", "cwd": "/x", "agent": "claude", "project": "aoide" }],
                "edges": [],
            }))
        });
        let out = session_roster_with(&hosts_invocation(&[], &[]), pull);
        let data = out.data.unwrap();
        let nodes = data["nodes"].as_array().unwrap();
        // Match on `isLocal: false`, not the name alone — this box's own
        // host can legitimately share the fixture's registered node name.
        let remote = nodes.iter().find(|n| n["isLocal"] == false).unwrap();
        let row = &remote["sessions"][0];
        assert_eq!(row["project"], "aoide", "the stored value still publishes");
        assert!(row.get("effectiveProject").is_none(), "no owner chain to walk on a remote row");
    }

    // ── `session --json` rows: additive `nativeRole` (root order seq 404) ─

    #[test]
    fn session_json_rows_carry_native_role_only_when_present() {
        let _env = Env::set_up("native-role-json");
        let sf = super::super::model::SessionsFile {
            schema_version: "0".to_string(),
            sessions: vec![
                session("s-root", "/home/k/Aoide", "working", "1", None),
                SessionRecord {
                    native_role: Some("subagent".to_string()),
                    ..session("s-child", "/home/k/Aoide", "working", "2", Some("s-root"))
                },
            ],
        };
        super::super::model::write_stage(&super::super::model::sessions_path(), &sf).unwrap();

        let out = session_roster_with(&hosts_invocation(&[], &[]), never_called_pull());
        let data = out.data.unwrap();
        let sessions = data["nodes"][0]["sessions"].as_array().unwrap();
        let root = sessions.iter().find(|s| s["sessionId"] == "s-root").unwrap();
        let child = sessions.iter().find(|s| s["sessionId"] == "s-child").unwrap();
        assert!(root.get("nativeRole").is_none(), "a root with no published role stays absent");
        assert_eq!(child["nativeRole"], "subagent");
        // Built from the SAME `session_view_json`, so the project grouping
        // agrees by construction — no second check needed.
    }

    // ── a far document at scale: the join, the rows and the JSON stay flat ──

    /// A far node's resolved document: `(id, spawner)` sessions in order, each
    /// with the `spawned` edge its spawner implies, and `(host, node, id)`
    /// remote-child links riding the session named `host`.
    fn far_doc(sessions: &[(String, Option<String>)], links: &[(String, String, String)]) -> Value {
        let mut kids: HashMap<&str, Vec<Value>> = HashMap::new();
        for (host, node, sid) in links {
            kids.entry(host.as_str()).or_default().push(json!({ "node": node, "sessionId": sid }));
        }
        let nodes: Vec<Value> = sessions
            .iter()
            .map(|(id, _)| {
                let mut n = json!({ "id": format!("session:{id}"), "kind": "session", "state": "idle" });
                if let Some(links) = kids.remove(id.as_str()) {
                    n["remoteChildren"] = json!(links);
                }
                n
            })
            .collect();
        let edges: Vec<Value> = sessions
            .iter()
            .filter_map(|(id, parent)| {
                parent.as_ref().map(|p| json!({ "from": format!("session:{p}"), "to": format!("session:{id}"), "kind": "spawned" }))
            })
            .collect();
        json!({ "schemaVersion": "0", "nodes": nodes, "edges": edges })
    }

    fn tree(rows: &[(&str, Option<&str>)]) -> Vec<(String, Option<String>)> {
        rows.iter().map(|(id, p)| (id.to_string(), p.map(str::to_string))).collect()
    }

    fn link(host: &str, node: &str, id: &str) -> (String, String, String) {
        (host.to_string(), node.to_string(), id.to_string())
    }

    /// `nodea/P` spawned `nodeb/C` and `nodeb/D`; on nodeb, C spawned G and G2
    /// and G spawned H — a three-level far chain under one local-looking row.
    fn three_level_roster() -> Vec<NodeView> {
        let a = far_doc(&tree(&[("P", None)]), &[link("P", "nodeb", "C"), link("P", "nodeb", "D")]);
        let b = far_doc(
            &tree(&[("C", None), ("G", Some("C")), ("H", Some("G")), ("G2", Some("C")), ("D", None)]),
            &[],
        );
        let mut nodes = vec![
            build_mesh_node(&node("nodea"), Ok(a), None),
            build_mesh_node(&node("nodeb"), Ok(b), None),
        ];
        attach_subtrees(&mut nodes);
        nodes
    }

    #[test]
    fn a_three_level_far_chain_renders_and_serializes_as_it_always_did() {
        let nodes = three_level_roster();
        assert_eq!(
            render_nodes(&nodes),
            [
                "● nodea",
                "└─ nodea/root/P    idle    ↓ 2 remote",
                "   ├─ nodeb/C",
                "   │  ├─ nodeb/G",
                "   │  │  └─ nodeb/H",
                "   │  └─ nodeb/G2",
                "   └─ nodeb/D",
                "● nodeb",
                "├─ nodeb/root/C    idle  ",
                "├─ nodeb/child/G    idle  ",
                "├─ nodeb/child/H    idle  ",
                "├─ nodeb/child/G2    idle  ",
                "└─ nodeb/root/D    idle  ",
            ]
            .join("\n")
        );
        let links = node_json(&nodes[0])["sessions"][0]["remoteChildren"].clone();
        assert_eq!(
            links,
            json!([
                {
                    "fold": "fresh", "node": "nodeb", "sessionId": "C",
                    "subtree": [
                        { "sessionId": "G", "subtree": [{ "sessionId": "H", "subtree": [] }] },
                        { "sessionId": "G2", "subtree": [] },
                    ],
                },
                { "fold": "fresh", "node": "nodeb", "sessionId": "D", "subtree": [] },
            ])
        );
    }

    /// `s0 → s1 → … → s(n-1)` on one far node, `links` riding their hosts.
    fn far_chain(n: usize, links: &[(String, String, String)]) -> Value {
        let sessions: Vec<(String, Option<String>)> =
            (0..n).map(|i| (format!("s{i}"), (i > 0).then(|| format!("s{}", i - 1)))).collect();
        far_doc(&sessions, links)
    }

    /// A heap-shaped far fold of `n` sessions (`s(i)` spawned by `s((i-1)/2)`)
    /// with `n / 10` remote-child links riding `s0`, aimed at `s1..` — many
    /// links, one big overlapping subtree.
    fn far_tree(n: usize) -> Value {
        let sessions: Vec<(String, Option<String>)> =
            (0..n).map(|i| (format!("s{i}"), (i > 0).then(|| format!("s{}", (i - 1) / 2)))).collect();
        let links: Vec<(String, String, String)> = (1..=n / 10).map(|k| link("s0", "far", &format!("s{k}"))).collect();
        far_doc(&sessions, &links)
    }

    fn far_roster(doc: Value) -> Vec<NodeView> {
        let mut nodes = vec![build_mesh_node(&node("far"), Ok(doc), None)];
        attach_subtrees(&mut nodes);
        nodes
    }

    /// The nesting depth of a JSON value, measured with a stack of its own.
    fn json_depth(v: &Value) -> usize {
        let (mut deepest, mut stack) = (0, vec![(v, 1usize)]);
        while let Some((v, depth)) = stack.pop() {
            deepest = deepest.max(depth);
            match v {
                Value::Array(a) => stack.extend(a.iter().map(|c| (c, depth + 1))),
                Value::Object(o) => stack.extend(o.values().map(|c| (c, depth + 1))),
                _ => {}
            }
        }
        deepest
    }

    /// Every `sessionId` under a link's `subtree` JSON, with the row's `depth`
    /// when it carries one, in no particular order — an explicit stack, since
    /// the document this reads is the one under test.
    fn subtree_rows(link: &Value) -> Vec<(String, Option<u64>)> {
        let mut rows = Vec::new();
        let mut stack: Vec<&Value> = link["subtree"].as_array().into_iter().flatten().collect();
        while let Some(row) = stack.pop() {
            rows.push((row["sessionId"].as_str().unwrap().to_string(), row["depth"].as_u64()));
            stack.extend(row["subtree"].as_array().into_iter().flatten());
        }
        rows
    }

    #[test]
    fn a_far_chain_past_the_indent_bound_keeps_every_row_and_clips_only_the_prefix() {
        // s0 → s1 → … → s39, the link aimed at s1: rows s2.. at depth 0..=37.
        let nodes = far_roster(far_chain(40, &[link("s0", "far", "s1")]));
        let rendered = render_nodes(&nodes);
        let line = |name: &str| rendered.lines().find(|l| l.ends_with(&format!("└─ far/{name}"))).unwrap().to_string();
        // s0 is not the last session of its node, so its rows sit under `│  `;
        // each chain row is its parent's only child, so each ancestor leaves
        // three spaces.
        let indent = |levels: usize| format!("│     {}", "   ".repeat(levels));
        assert_eq!(line("s2"), format!("{}└─ far/s2", indent(0)));
        assert_eq!(line("s33"), format!("{}└─ far/s33", indent(31)));
        assert_eq!(line("s34"), format!("{}└─ far/s34", indent(32)), "the bound itself still draws whole");
        assert_eq!(line("s35"), format!("{}…└─ far/s35", indent(32)), "past it the indent is clipped");
        assert_eq!(line("s39"), format!("{}…└─ far/s39", indent(32)), "and stays clipped, row after row");
        let drawn = (2..40).filter(|i| rendered.contains(&format!("└─ far/s{i}"))).count();
        assert_eq!(drawn, 38, "no row was dropped to bound the indent");

        // The JSON nests the same 32 levels and keeps the rest in the deepest
        // list, each with its true depth.
        let link = node_json(&nodes[0])["sessions"][0]["remoteChildren"][0].clone();
        let rows = subtree_rows(&link);
        assert_eq!(rows.len(), 38);
        let depth_of = |id: &str| rows.iter().find(|(s, _)| s == id).unwrap().1;
        assert_eq!(depth_of("s33"), None, "nested rows carry no depth");
        assert_eq!(depth_of("s34"), Some(32));
        assert_eq!(depth_of("s39"), Some(37));
        assert!(json_depth(&link) <= 2 * INDENT_LEVELS + 6, "bounded nesting: {}", json_depth(&link));
    }

    #[test]
    fn a_session_is_drawn_under_the_first_link_that_reaches_it_and_no_later_one() {
        // R spawned A and B, A spawned A1. Three links, in order: A, R, A1.
        let fold = tree(&[("R", None), ("A", Some("R")), ("B", Some("R")), ("A1", Some("A"))]);
        let host = tree(&[("P", None)]);
        let doc = far_doc(&host, &[link("P", "nodeb", "A"), link("P", "nodeb", "R"), link("P", "nodeb", "A1")]);
        let mut nodes = vec![
            build_mesh_node(&node("nodea"), Ok(doc), None),
            build_mesh_node(&node("nodeb"), Ok(far_doc(&fold, &[])), None),
        ];
        attach_subtrees(&mut nodes);
        let rows: Vec<Vec<(&str, usize)>> = nodes[0].sessions[0]
            .remote_children
            .iter()
            .map(|c| c.subtree.iter().map(|r| (r.session_id.as_str(), r.depth)).collect())
            .collect();
        assert_eq!(rows[0], vec![("A1", 0)], "the first link draws A's subtree");
        assert_eq!(rows[1], vec![("B", 0)], "R's own subtree skips what A already drew");
        assert!(rows[2].is_empty(), "a link into a drawn session keeps its row and has none under it");
        let rendered = render_nodes(&nodes);
        for id in ["A", "R", "A1"] {
            assert!(rendered.contains(&format!("nodeb/{id}")), "every link is still a row ({id}):\n{rendered}");
        }
    }

    #[test]
    fn one_far_nodes_link_does_not_blank_another_nodes_rows_in_the_same_fold() {
        // nodea and nodec both link to nodeb's X. nodea is first, so its link
        // is the first to reach X; nodec's link keeps X's subtree all the same.
        let fold_b = far_doc(&tree(&[("X", None), ("Y", Some("X"))]), &[]);
        let mut nodes = vec![
            build_mesh_node(&node("nodea"), Ok(far_doc(&tree(&[("P", None)]), &[link("P", "nodeb", "X")])), None),
            build_mesh_node(&node("nodeb"), Ok(fold_b), None),
            build_mesh_node(&node("nodec"), Ok(far_doc(&tree(&[("Q", None)]), &[link("Q", "nodeb", "X")])), None),
        ];
        attach_subtrees(&mut nodes);
        let rows = |n: usize| -> Vec<(&str, usize)> {
            nodes[n].sessions[0].remote_children[0].subtree.iter().map(|r| (r.session_id.as_str(), r.depth)).collect()
        };
        assert_eq!(rows(0), vec![("Y", 0)], "the first node's link draws X's subtree");
        assert_eq!(rows(2), vec![("Y", 0)], "the second node's link keeps its own rows");
    }

    #[test]
    fn a_far_link_into_this_box_draws_none_of_its_sessions() {
        // L2 was spawned here by L1. A far document claiming L1 as its own
        // remote child keeps the row and draws nothing of this box's own.
        let _env = Env::set_up("far-link-into-local");
        let local = build_local_node(
            &[session("L1", "/x", "idle", "1", None), session("L2", "/x", "idle", "2", Some("L1"))],
            &[],
            &[],
            &[],
            "yomi",
        );
        let far = build_mesh_node(&node("nodea"), Ok(far_doc(&tree(&[("P", None)]), &[link("P", "yomi", "L1")])), None);
        let mut nodes = vec![local, far];
        attach_subtrees(&mut nodes);
        let link = &nodes[1].sessions[0].remote_children[0];
        assert_eq!(link.fold, Some(Fold::Fresh), "the fold is this box's own, and it answers");
        assert!(link.subtree.is_empty(), "L2 is this box's own session, and a far document cannot draw it");
    }

    #[test]
    fn a_link_into_a_registered_node_never_pulled_is_not_pulled() {
        // nodeb is registered, its probe failed and it has no cache: the link
        // has no fold behind it, so it is shown as not pulled, not as fresh.
        let mut nodes = vec![
            build_mesh_node(&node("nodea"), Ok(far_doc(&tree(&[("P", None)]), &[link("P", "nodeb", "C")])), None),
            build_mesh_node(&node("nodeb"), Err("HTTP 000".into()), None),
        ];
        attach_subtrees(&mut nodes);
        let link = &nodes[0].sessions[0].remote_children[0];
        assert_eq!(link.fold, Some(Fold::Absent));
        assert!(render_nodes(&nodes).contains("nodeb/C (not pulled)"), "{}", render_nodes(&nodes));
    }

    #[test]
    fn a_fold_whose_parentage_loops_costs_a_wrong_row_never_a_hang() {
        // a ↔ b spawned each other; c is a's. A link into the loop draws each
        // member once and stops.
        let looped = far_doc(&tree(&[("a", Some("b")), ("b", Some("a")), ("c", Some("a"))]), &[]);
        let host = far_doc(&tree(&[("P", None)]), &[link("P", "nodeb", "a")]);
        let mut nodes = vec![
            build_mesh_node(&node("far"), Ok(host), None),
            build_mesh_node(&node("nodeb"), Ok(looped), None),
        ];
        attach_subtrees(&mut nodes);
        let rows: Vec<(&str, usize)> =
            nodes[0].sessions[0].remote_children[0].subtree.iter().map(|r| (r.session_id.as_str(), r.depth)).collect();
        assert_eq!(rows, vec![("b", 0), ("c", 0)], "b once (its way back to a is skipped), then a's other child");
    }

    #[test]
    fn however_many_links_point_into_a_fold_its_rows_never_outnumber_its_sessions() {
        // 3,000 links, every one aimed at the root of a 3,000-session chain.
        let n = 3_000;
        let links: Vec<_> = (0..n).map(|_| link("s0", "far", "s1")).collect();
        let nodes = far_roster(far_chain(n, &links));
        let rows: usize = nodes[0].sessions[0].remote_children.iter().map(|c| c.subtree.len()).sum();
        assert_eq!(rows, n - 2, "the chain is drawn once, under the first link");
        assert_eq!(nodes[0].sessions[0].remote_children.len(), n, "and every link is still a row");
    }

    #[test]
    fn the_first_spawned_edge_into_a_session_names_its_parent() {
        // Two `spawned` edges into `k`: the first wins, as `find` did.
        let graph = json!({
            "nodes": [{ "id": "session:k", "kind": "session" }, { "id": "session:a", "kind": "session" }, { "id": "session:b", "kind": "session" }],
            "edges": [
                { "from": "session:a", "to": "session:k", "kind": "resumed" },
                { "from": "session:a", "to": "session:k", "kind": "spawned" },
                { "from": "session:b", "to": "session:k", "kind": "spawned" },
            ],
        });
        let views = sessions_from_graph(&graph, "far");
        assert_eq!(views.iter().find(|s| s.session_id == "k").unwrap().parent.as_deref(), Some("a"));
        assert_eq!(views.iter().find(|s| s.session_id == "a").unwrap().parent, None);
    }

    #[test]
    fn the_far_join_and_its_renderings_cost_linear_in_the_document() {
        // Everything a far document of `n` sessions and `n / 10` links costs
        // bare `session` before the project grouping: the extraction, the
        // join, both renderings. Eight times the sessions must cost about
        // eight times the time; a quadratic walk costs sixty-four.
        fn cost(n: usize) -> impl FnOnce() -> usize + Send + 'static {
            move || {
                let doc = far_tree(n);
                let views = sessions_from_graph(&doc, "far");
                let mut nodes =
                    vec![NodeView { name: "far".into(), is_local: false, presence: "online", fetched_at: None, error: None, stale: false, sessions: views }];
                attach_subtrees(&mut nodes);
                render_nodes(&nodes).len() + node_json(&nodes[0]).to_string().len()
            }
        }
        // An absolute ceiling, not a linearity proof: a ratio against a
        // baseline that runs in milliseconds is just the ceiling again.
        let limit = Duration::from_secs(10);
        let (big, _) = aoide_test_support::finishes_within(limit, cost(32_000))
            .unwrap_or_else(|| panic!("32k sessions took over {limit:?}: the far join is too slow"));
        println!("far join: 32k sessions {big:?}");
    }

    /// Both renderings of a far document through `pull`, on a small stack and
    /// timed. The env lock is taken on THIS thread: a render that outlives
    /// `limit` is left running, but the lock is released when this returns, so
    /// one slow test fails alone instead of holding every env-locked test.
    /// The lock is not observed by a test: 526 call sites share it, and a
    /// check of it races the legitimate holders, so this is structural.
    fn render_far_within(limit: Duration, pull: PullFn) -> Option<(Duration, (Outcome, Outcome))> {
        let _env = Env::set_up("far-render");
        aoide_storage::node_store::save_nodes(&[node("far")]).unwrap();
        let second = pull.clone();
        aoide_test_support::finishes_within(limit, move || {
            aoide_test_support::on_small_stack(move || {
                let bare = session_roster_with(&project_invocation(), pull);
                let hosts = session_roster_with(&hosts_invocation(&[], &[]), second);
                (bare, hosts)
            })
        })
    }

    #[test]
    fn a_200k_deep_far_chain_renders_bare_session_and_its_json_on_a_small_stack() {
        // The hostile shape: a chain as deep as a document allows, one link
        // into it, and a stack on which a walk that recurses per level dies
        // within a few thousand levels.
        const N: usize = 200_000;
        let doc = Arc::new(far_chain(N, &[link("s0", "far", "s1")]));
        let serve = |doc: Arc<Value>| -> PullFn { Arc::new(move |_: &Node| Ok((*doc).clone())) };
        let (_, (bare, hosts)) = render_far_within(Duration::from_secs(180), serve(doc))
            .expect("a chain two hundred thousand deep renders in seconds, not in the square of its depth");
        for out in [&bare, &hosts] {
            assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{}", out.message);
            assert!(out.message.lines().count() >= 2 * N - 8, "every session and every row: {}", out.message.lines().count());
            let last = out.message.lines().find(|l| l.ends_with(&format!("far/s{}", N - 1)) && l.contains('…')).unwrap();
            assert!(last.chars().count() < 160, "the indent is clipped: {} chars", last.chars().count());
        }
        // The JSON a consumer reads must be readable by one: bounded nesting,
        // so it survives a parser's recursion limit, and every row present.
        let data = hosts.data.unwrap();
        let text = aoide_test_support::on_small_stack(move || serde_json::to_string(&data).unwrap());
        let parsed: Value = serde_json::from_str(&text).expect("the roster JSON parses under serde_json's recursion limit");
        let far = parsed["nodes"].as_array().unwrap().iter().find(|n| n["name"] == "far").unwrap();
        let link = &far["sessions"][0]["remoteChildren"][0];
        let rows = subtree_rows(link);
        assert_eq!(rows.len(), N - 2, "every far row survives into the JSON");
        assert_eq!(rows.iter().find(|(s, _)| *s == format!("s{}", N - 1)).unwrap().1, Some((N - 3) as u64));
        drop(bare);
    }

    #[test]
    fn a_hostile_cwd_a_megabyte_long_is_bucketed_without_walking_every_component() {
        // `/a/a/a/…` half a million components deep: a walk that copies the
        // path and stats it once per ancestor costs minutes.
        let cwd = format!("/{}", "a/".repeat(512 * 1024));
        let (took, bucket) = aoide_test_support::finishes_within(Duration::from_secs(10), move || project_bucket(&cwd, &[]))
            .expect("a megabyte cwd is bucketed in moments, not walked component by component");
        assert_eq!(bucket, None, "it names no directory here, so it names no manifest");
        println!("project_bucket of a 1 MiB cwd: {took:?}");
    }

    #[test]
    fn within_path_max_starts_at_the_deepest_ancestor_that_can_hold_a_manifest() {
        // Without the cut the result is the whole cwd, past what any manifest
        // path can reach. With it, the result is the longest prefix on a
        // component boundary that still fits, so the next component would not.
        let cwd = format!("/{}", "a/".repeat(2_100));
        let cut = within_path_max(&cwd);
        assert!(cut.len() <= WALK_PATH_MAX, "{} bytes is past the bound", cut.len());
        assert_eq!(cwd.as_bytes()[cut.len()], b'/', "the cut lands on a component boundary");
        assert!(cut.len() + 2 > WALK_PATH_MAX, "the next component would still have fit");
        assert_eq!(within_path_max("/short/cwd"), "/short/cwd", "a cwd that fits is walked whole");
    }

    #[test]
    fn a_manifest_in_a_directory_just_inside_the_bound_is_found() {
        // The manifest path of a 4070-byte directory is 4090 bytes, which a
        // real stat accepts. A bound that stopped at 4064 would walk past it.
        let base = std::env::temp_dir().join(format!("aoide-who-bound-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let target_len = 4070;
        let mut dir = base.clone();
        let mut left = target_len - base.as_os_str().len();
        while left > 0 {
            // Each component costs its name plus one '/'; never leave a lone '/'.
            let cost = match left {
                202 => 200,
                203.. => 201,
                _ => left,
            };
            dir.push("d".repeat(cost - 1));
            left -= cost;
        }
        std::fs::create_dir_all(dir.join(".aoide")).unwrap();
        let manifest = serde_json::to_string(&aoide_storage::manifest::Manifest::default()).unwrap();
        std::fs::write(aoide_storage::manifest::manifest_path(&dir), manifest).unwrap();
        assert_eq!(dir.as_os_str().len(), target_len);
        let cwd = format!("{}/{}", dir.to_str().unwrap(), "z".repeat(10));
        assert!(cwd.len() > WALK_PATH_MAX);
        assert_eq!(project_bucket(&cwd, &[]), Some("d".repeat(dir.file_name().unwrap().len())));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_cwd_past_path_max_still_finds_the_manifest_above_it() {
        // A regression guard, not the bound's proof: the ancestors past the
        // cut could not hold a manifest, so the answer is the same with or
        // without it. `within_path_max_starts_at_the_deepest_ancestor...` and
        // `a_manifest_in_a_directory_just_inside_the_bound...` prove the cut.
        let root = std::env::temp_dir().join(format!("aoide-who-long-cwd-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        aoide_storage::manifest::save_manifest(&root, &aoide_storage::manifest::Manifest::default()).unwrap();
        let cwd = format!("{}/{}", root.to_str().unwrap(), "a/".repeat(3_000));
        assert!(cwd.len() > 4096);
        assert_eq!(project_bucket(&cwd, &[]), root.file_name().map(|n| n.to_string_lossy().into_owned()));
        let _ = std::fs::remove_dir_all(&root);
    }
}
