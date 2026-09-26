//! The DAG computation: `build_graph` (the `graph.json` v0 document), the
//! Unicode tree `render`, and the pure command cores (`would_cycle`,
//! `prune_done`) the command handlers wire I/O around. `restage_graph` is the
//! write-side counterpart every mutating command calls to keep `graph.json` a
//! pure function of the registries.

use super::model::{
    canonical_state, graph_path, hooks_path, load_stage, merged_sessions, projects_path,
    resolved_parent, sessions_path, sorted_projects, write_stage, HookRecord, HooksFile, Project,
    ProjectsFile, SessionRecord, SessionsFile, STAGE_GRAPH_VERSION,
};
use serde_json::{json, Value};
#[cfg(test)]
use serde_json::Map;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::PathBuf;

/// Is `s` conductable RIGHT NOW, for a caller asking "can I reach this
/// session" — the value `build_graph` (and `resolve_graph_document`'s
/// federation wire response, which wraps it verbatim) reports. The STORED
/// `conductable` flag is a permanent fact about the session's NATURE (it IS a
/// conducted PTY wrap) and is never touched here: `window.rs`/`reap.rs`
/// classification (`is_agent_kind`, the lineage checks) keeps reading
/// `s.conductable` directly, because a session does not stop being a
/// conducted wrap just because its socket briefly vanished. But
/// `shellbridge.service` owns `$XDG_RUNTIME_DIR/aoide` with
/// `RuntimeDirectoryPreserve=no`, so a rebuild deletes a live session's
/// socket file out from under it without ever touching the record — a REPORT
/// of conductability additionally needs the socket to still exist on disk, or
/// the graph tells a caller it can reach a session nothing can actually reach.
/// A missing or empty socket path is not-conductable, the same shape
/// `send.rs`'s own gate now calls this to get.
pub(in crate::graph) fn is_conductable_now(s: &SessionRecord) -> bool {
    s.conductable == Some(true)
        && s.socket
            .as_deref()
            .filter(|p| !p.is_empty())
            .is_some_and(|p| std::path::Path::new(p).exists())
}

/// Build the fully resolved graph document (`graph.json` v0 shape). A session
/// with a resolved parent carries only its `spawned` edge; root sessions carry
/// an `anchors` edge to their longest-prefix project (or none, unanchored) —
/// unless that project has a live lead, in which case they carry a `leads`
/// edge from the lead, and the lead itself is the one anchored.
pub fn build_graph(
    projects: &[Project],
    sessions: &[SessionRecord],
    hooks: &[HookRecord],
) -> Value {
    let projects = sorted_projects(projects);
    let sessions = merged_sessions(sessions, hooks);
    let ids: HashSet<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();
    // The compositor block (§E of the core-seams design): the per-workspace
    // rows and the ties between them, `None` when nothing is in play — that
    // gate is what keeps every graph.json predating a workspace binding
    // byte-identical, session `activeAt` included (below).
    let block = workspace_block(&projects, &sessions, hooks);

    let mut nodes: Vec<Value> = Vec::new();
    let mut edges: Vec<Value> = Vec::new();

    // The caller-side ledger (CONTRACTS.md §4): one row per child THIS node
    // spawned on another node. Read once beside the node registry above — the
    // projection below matches rows to a local parent's own session id.
    let remote_children = aoide_storage::remote_children::load_remote_children();
    // `nodes.json` once for the whole document: every link's `key` resolves
    // against it (`model::current_node_name`), and the node fold at the bottom
    // folds the same registry — one read, one parse, both uses.
    let mesh_nodes = aoide_storage::node_store::load_nodes();

    for p in &projects {
        let mut node = json!({
            "id": format!("project:{}", p.name),
            "kind": "project",
            "name": p.name,
            "path": p.path,
            "roots": p.roots(),
        });
        // Host membership (P-14 M1) rides only when non-empty, same
        // ride-only-when-present rule the session node's `title`/`petname`
        // fields already hold below — a project no `--host` invocation ever
        // touched stays byte-for-byte as before this field existed.
        if !p.hosts.is_empty() {
            node["hosts"] = json!(p.hosts);
        }
        if let Some(lead) = &p.lead {
            node["lead"] = json!(lead);
        }
        nodes.push(node);
    }
    for s in &sessions {
        let mut node = json!({
            "id": format!("session:{}", s.session_id),
            "kind": "session",
            "agent": s.agent,
            "cwd": s.cwd,
            "state": s.state,
            "windowAddress": s.window_address,
            "startedAt": s.started_at,
        });
        // Conductor-channel fields ride onto the node only when present, so a
        // legacy/observe-only session stays byte-for-byte as before and the
        // conductor can distinguish conductable nodes + label by title.
        // `conductable` rides the DERIVED value (see `is_conductable_now`),
        // not `s.conductable` verbatim — presence still gates on the stored
        // field alone, so a legacy record stays byte-for-byte absent.
        if s.conductable.is_some() {
            node["conductable"] = json!(is_conductable_now(s));
        }
        if let Some(sock) = &s.socket {
            node["socket"] = json!(sock);
        }
        if let Some(t) = &s.title {
            node["title"] = json!(t);
        }
        if let Some(pid) = s.pid {
            node["pid"] = json!(pid);
        }
        // The workspace the session's window lives on — the hover-preview bridge
        // (concepts/Terminal-Commander). Rides onto the node only when known, so
        // a legacy/off-Hyprland record stays byte-for-byte as before.
        if let Some(ws) = s.workspace {
            node["workspace"] = json!(ws);
        }
        // The live current command / tool, when something is running.
        if let Some(act) = &s.activity {
            node["activity"] = json!(act);
        }
        // Session classification (agent/shell/subagent) — the node's own `kind`
        // already denotes project-vs-session, so this rides as `role`.
        if let Some(k) = &s.kind {
            node["role"] = json!(k);
        }
        // The minted `adjective-noun` display handle (petnames plan) — rides
        // onto the node only when present, so a legacy/petname-less record
        // stays byte-for-byte as before. Display-only: `session_id` above
        // stays the canonical key.
        if let Some(key) = &s.enduring_agent_id {
            node["enduringAgentId"] = json!(key);
        }
        if let Some(pn) = &s.petname {
            node["petname"] = json!(pn);
        }
        // The native harness's own thread-role label (root order seq 404) —
        // `subagent`/`guardian_review`/… verbatim off `SessionRecord.
        // native_role`, rides only when the native harness published one.
        // `role` above (from `kind`) is untouched: a nested app child still
        // publishes `role:"app"`, never reclassified for this.
        if let Some(nr) = &s.native_role {
            node["nativeRole"] = json!(nr);
        }
        // The agent's latest words (transcript tail), when it has spoken.
        if let Some(say) = &s.say {
            node["say"] = json!(say);
        }
        // The agent's latest tool call (transcript-derived), when it has made
        // one. Rides beside `activity` rather than replacing it: `activity` is
        // what is running now, this is what was last reached for.
        if let Some(tool) = &s.tool {
            node["tool"] = json!(tool);
        }
        // The Claude model this session (agent or subagent) is running, when
        // known — absent for shells and until the first assistant turn lands.
        if let Some(m) = &s.model {
            node["model"] = json!(m);
        }
        // The context-window fill of this session's last request (input-side
        // token count off its transcript's freshest assistant `usage`), when
        // known — absent for shells and until the first assistant turn lands.
        // Rides alongside `model` for the same reason: the dock computes the
        // meter (percent + ceiling) itself from the raw count.
        if let Some(ctx) = s.context_tokens {
            node["contextTokens"] = json!(ctx);
        }
        // The context-window ceiling for the node's model (aoide's published fact),
        // so the dock's meter needs no client-side 200k/1M guess. Absent with `model`.
        if let Some(ceil) = s.context_ceiling {
            node["contextCeiling"] = json!(ceil);
        }
        // Blocked on a `sudo` password prompt — a conducted SHELL only; rides
        // onto the node only when true (never a dangling `needsSudo:false`).
        if let Some(true) = s.needs_sudo {
            node["needsSudo"] = json!(true);
        }
        if let Some(project) = &s.project { node["project"] = json!(project); }
        // The project this session RENDERS under (`model::effective_project_for`,
        // own explicit project > owner's effective project > own cwd anchor) —
        // additive, rides beside the STORED `project` above only when the
        // resolver resolves one, so a legacy/unresolvable record stays
        // byte-for-byte as before. `project` above is untouched: it keeps
        // publishing the stored value, resolved or not.
        if let Some(i) = super::model::effective_project_for(s, &sessions, &projects) {
            node["effectiveProject"] = json!(projects[i].name);
        }
        // The PULSE timestamp (§E): the latest hook `updatedAt`, else
        // `startedAt` — the value a tie's and a workspace's own `activeAt` is
        // the max over, and what the song watches advance between two reads.
        // Rides under the SAME gate as the block it feeds: with no workspace in
        // play there is nothing to pulse, and a document that never carried
        // this key must not grow it for no reason.
        if block.is_some() {
            node["activeAt"] = json!(session_active_at(s, hooks));
        }
        // The cross-machine parent link (P-RSA S4, CONTRACTS.md §4). `node` is
        // the CURRENT `nodes.json` name for the stamped `key` — never the
        // stored label when a rename has happened — and `key` itself is
        // deliberately NOT republished: it is identity, and this document is a
        // display projection. Rides only when the record carries one, so a
        // locally-registered record stays byte-for-byte as before. No edge
        // accompanies it: the parent is not on this node, and a local
        // `spawned` edge would have to name a foreign id as a local session.
        if let Some(rp) = &s.remote_parent {
            node["remoteParent"] = json!({
                "node": super::model::current_node_name(&mesh_nodes, &rp.key, &rp.node),
                "sessionId": rp.session_id,
            });
        }
        // This node's own children that live on OTHER nodes, off the ledger
        // above — the mirror of the same field, so both machines publish the
        // link they can vouch for. Same present-only-when-non-empty rule the
        // project node's `hosts` holds.
        let kids: Vec<Value> = remote_children
            .iter()
            .filter(|c| c.parent_session_id == s.session_id)
            .map(|c| {
                json!({
                    "node": super::model::current_node_name(&mesh_nodes, &c.key, &c.node),
                    "sessionId": c.session_id,
                })
            })
            .collect();
        if !kids.is_empty() {
            node["remoteChildren"] = json!(kids);
        }
        nodes.push(node);
        if let Some(parent) = resolved_parent(s, &ids) {
            edges.push(json!({
                "from": format!("session:{parent}"),
                "to": format!("session:{}", s.session_id),
                "kind": "spawned",
            }));
        } else if let Some(i) = super::model::leads_project(s, &projects)
            .or_else(|| super::model::project_for(s, &projects))
        {
            match super::model::lead_over(s, i, &projects, &ids) {
                Some(lead) => edges.push(json!({
                    "from": format!("session:{lead}"),
                    "to": format!("session:{}", s.session_id),
                    "kind": "leads",
                })),
                None => edges.push(json!({
                    "from": format!("project:{}", projects[i].name),
                    "to": format!("session:{}", s.session_id),
                    "kind": "anchors",
                })),
            }
        }
        // Additive `resumed` edge (P-D8, CONTRACTS.md §4): a session revived
        // by `graph resurrect` names the ledger entry's own sessionId it was
        // built from. Rides BESIDE the spawned/anchors edge above, never in
        // place of it — the source id names a session that has, by
        // construction, already left the roster (the whole premise of
        // resurrecting it), so it need not resolve to a node in `ids`.
        if let Some(from) = &s.resumed_from {
            edges.push(json!({
                "from": format!("session:{from}"),
                "to": format!("session:{}", s.session_id),
                "kind": "resumed",
            }));
        }
    }

    // Fold registered NODES into the DAG (CONTRACTS.md §7): each a ROOT node
    // `kind:"node"`, `id:"node:<name>"` — no edges (they anchor to nothing),
    // so the existing spawned/anchors machinery is untouched. A node's own
    // ALREADY-RESOLVED graph document nests as `children` on its node,
    // verbatim, never flattened into this document's own `nodes`/`edges` —
    // so a node's ids can never collide with local ones or another node's,
    // and no new edge vocabulary is needed. Only a FRESH (non-stale, within
    // `NODE_CACHE_TTL_SECS`) cache contributes `children`; a stale or
    // never-pulled node still surfaces (so `node add` is visible
    // immediately) with an explicit `state` and no children — never a
    // crash, never a silently-dropped node. Additive and tolerate-missing:
    // an absent/empty registry (`state/nodes.json`) adds nothing and this
    // whole block is a no-op.
    let now_epoch = aoide_storage::time::parse_iso_utc(&aoide_storage::time::now_iso_utc()).unwrap_or(0);
    for mesh_node in mesh_nodes {
        let mut node = json!({
            "id": format!("node:{}", mesh_node.name),
            "kind": "node",
            "name": mesh_node.name,
            "url": mesh_node.url,
        });
        let cache = aoide_storage::node_store::load_node_cache(&mesh_node.name);
        let fresh = cache
            .as_ref()
            .map(|c| aoide_storage::node_store::is_cache_fresh(c, now_epoch))
            .unwrap_or(false);
        if let Some(entry) = &cache {
            if let Some(fa) = &entry.fetched_at {
                node["fetchedAt"] = json!(fa);
            }
            if let Some(err) = &entry.last_error {
                node["error"] = json!(err);
            }
        }
        if fresh {
            let entry = cache.expect("fresh implies a cache entry was loaded");
            let graph = entry.graph.unwrap_or_else(|| json!({ "nodes": [], "edges": [] }));
            node["state"] = json!("fresh");
            node["children"] = json!({
                "nodes": graph.get("nodes").cloned().unwrap_or_else(|| json!([])),
                "edges": graph.get("edges").cloned().unwrap_or_else(|| json!([])),
            });
        } else {
            node["state"] = json!("stale");
        }
        nodes.push(node);
    }

    let mut doc = json!({
        "schemaVersion": STAGE_GRAPH_VERSION,
        "nodes": nodes,
        "edges": edges,
    });
    // The block rides LAST, and ONLY when some session has a workspace or some
    // project has a binding (§E): a document with neither carries no
    // `workspaces`/`ties` key at all, so a headless host's graph.json is
    // byte-for-byte what it was before this slice.
    if let Some((rows, ties)) = block {
        doc["workspaces"] = json!(rows);
        doc["ties"] = json!(ties);
    }
    doc
}

// ── The compositor block: workspace rows + the ties between them (§E) ──────

/// One session's pulse timestamp (§E): the latest hook `updatedAt` for it,
/// else its own `startedAt`. Timestamps here are all `iso_utc_from_epoch`
/// strings, so the string max IS the chronological max — the same comparison
/// `merged_sessions` already makes when it folds the latest hook phase in.
fn session_active_at(s: &SessionRecord, hooks: &[HookRecord]) -> String {
    hooks
        .iter()
        .filter(|h| h.session_id == s.session_id && !h.updated_at.is_empty())
        .map(|h| h.updated_at.as_str())
        .max()
        .unwrap_or(s.started_at.as_str())
        .to_string()
}

/// One workspace row, while it is being accumulated.
#[derive(Default)]
struct WorkspaceRow {
    project: Option<String>,
    projects: BTreeSet<String>,
    sessions: BTreeSet<String>,
    live: usize,
    working: usize,
    awaiting: usize,
    active_at: String,
}

impl WorkspaceRow {
    fn to_json(&self, ws: i64) -> Value {
        let mut row = json!({
            "workspace": ws,
            "projects": self.projects.iter().cloned().collect::<Vec<_>>(),
            "sessions": self.sessions.iter().cloned().collect::<Vec<_>>(),
            "live": self.live,
            "working": self.working,
            "awaiting": self.awaiting,
            "activeAt": self.active_at,
        });
        // The BINDING rides only when there is one — an unbound workspace is
        // labelled by its sessions' projects instead, never by a null.
        if let Some(project) = &self.project {
            row["project"] = json!(project);
        }
        row
    }
}

/// The workspace a `spawned` tie leaves from: the session's nearest ancestor
/// that REPORTS a workspace (its parent, or above it when the parent is
/// headless) — with its session id, so the tie can name the pair. `None` when
/// no ancestor is windowed (or a malformed cycle, which the visited set stops
/// dead rather than spinning).
fn windowed_ancestor<'a>(
    s: &SessionRecord,
    by_id: &BTreeMap<&'a str, &'a SessionRecord>,
) -> Option<(i64, &'a str)> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut cur = s.parent_session_id.as_deref();
    while let Some(id) = cur {
        if !seen.insert(id) {
            return None;
        }
        let parent = by_id.get(id)?;
        if let Some(ws) = parent.workspace {
            return Some((ws, parent.session_id.as_str()));
        }
        cur = parent.parent_session_id.as_deref();
    }
    None
}

/// The compositor block (§E of `core-seams-design.md`): the per-workspace rows
/// a pad draws and the ties between them, or `None` when NOTHING is in play —
/// no session reports a workspace and no project has a binding. That one gate
/// is what keeps a graph.json with no compositor in it byte-identical, session
/// `activeAt` included (it feeds this block, so it rides with it).
///
/// Definitions, straight from §E:
///
/// - `sessions` lists every session whose record carries that workspace id;
///   `live`/`working`/`awaiting` are counts over the same set. `live` is "not
///   `done`" — a `stopped` session has ended a TURN, never died, so it is
///   live.
/// - `project` is the BINDING, absent when the workspace is unbound.
///   `projects` is the binding PLUS the effective projects of that workspace's
///   live sessions, so an unbound workspace with a project's sessions on it
///   still labels itself (and one with neither stays an empty list).
/// - A **project tie** is one per shared project, for every pair of workspaces
///   whose `projects` intersect — a clique at three or more, which the song
///   collapses itself. A **spawned tie** is the parent's (or nearest windowed
///   ancestor's) workspace to the child's, ONLY when the two differ; a headless
///   child has no workspace and therefore no tie of its own.
/// - `activeAt` is the max over the sessions that contribute to a row or a tie.
///
/// LOCAL sessions only, by construction: `sessions` here IS the local roster,
/// and another node's rows reach this document only nested under a fresh
/// node's `children` — which this never reads.
pub(crate) fn workspace_block(
    projects: &[Project],
    sessions: &[SessionRecord],
    hooks: &[HookRecord],
) -> Option<(Vec<Value>, Vec<Value>)> {
    let mut rows: BTreeMap<i64, WorkspaceRow> = BTreeMap::new();
    // Bindings first: a bound workspace with no session on it still gets a row.
    for p in projects {
        for ws in &p.workspaces {
            let row = rows.entry(*ws).or_default();
            row.project = Some(p.name.clone());
            row.projects.insert(p.name.clone());
        }
    }
    let mut in_play = !rows.is_empty();
    // The parent lookup the two lineage walks share (a headless session's
    // activity, and a spawned tie's leaving end).
    let by_id: BTreeMap<&str, &SessionRecord> =
        sessions.iter().map(|s| (s.session_id.as_str(), s)).collect();
    for s in sessions {
        let at = session_active_at(s, hooks);
        let Some(ws) = s.workspace else {
            // A HEADLESS session has no row of its own — §E: "its activity
            // shows on its parent's workspace". So its pulse folds into the
            // nearest windowed ancestor's row, as ACTIVITY ONLY: no session id
            // joins that row's `sessions`, no count moves, and no tie is
            // drawn. Subagents are most of this product's traffic, and without
            // this fold their work would pulse nothing at all.
            if let Some((ancestor_ws, _)) = windowed_ancestor(s, &by_id) {
                let row = rows.entry(ancestor_ws).or_default();
                if at > row.active_at {
                    row.active_at = at;
                }
            }
            continue;
        };
        in_play = true;
        let state = canonical_state(&s.state);
        let row = rows.entry(ws).or_default();
        row.sessions.insert(s.session_id.clone());
        if state != "done" {
            row.live += 1;
            if let Some(i) = super::model::effective_project_for(s, sessions, projects) {
                row.projects.insert(projects[i].name.clone());
            }
        }
        match state {
            "working" => row.working += 1,
            "awaiting" => row.awaiting += 1,
            _ => {}
        }
        if at > row.active_at {
            row.active_at = at;
        }
    }
    if !in_play {
        return None;
    }

    // Project ties: every pair (a < b) of workspaces claiming the same project.
    let mut by_project: BTreeMap<&str, Vec<i64>> = BTreeMap::new();
    for (ws, row) in &rows {
        for name in &row.projects {
            by_project.entry(name.as_str()).or_default().push(*ws);
        }
    }
    let mut ties: Vec<(i64, i64, String, Value)> = Vec::new();
    for (name, wss) in &by_project {
        for (i, a) in wss.iter().enumerate() {
            for b in &wss[i + 1..] {
                ties.push((
                    *a,
                    *b,
                    format!("project\t{name}"),
                    json!({
                        "kind": "project",
                        "from": a,
                        "to": b,
                        "project": name,
                        "activeAt": std::cmp::max(&rows[a].active_at, &rows[b].active_at),
                    }),
                ));
            }
        }
    }
    // Spawned ties: (from, to) → every pair that crossed, and the latest
    // activity among them. `by_id` above is the same lookup this walk uses.
    let mut spawned: BTreeMap<(i64, i64), (BTreeSet<(String, String)>, String)> = BTreeMap::new();
    for s in sessions {
        let Some(child_ws) = s.workspace else { continue };
        let Some((parent_ws, parent_id)) = windowed_ancestor(s, &by_id) else {
            continue;
        };
        // Same workspace: there is no lane to draw. A headless child never
        // reaches here at all — `child_ws` is the whole reason it does not.
        if parent_ws == child_ws {
            continue;
        }
        let at = std::cmp::max(
            session_active_at(s, hooks),
            session_active_at(by_id[parent_id], hooks),
        );
        let entry = spawned.entry((parent_ws, child_ws)).or_insert_with(|| {
            (BTreeSet::new(), String::new())
        });
        entry.0.insert((parent_id.to_string(), s.session_id.clone()));
        if at > entry.1 {
            entry.1 = at;
        }
    }
    for ((from, to), (pairs, at)) in &spawned {
        let pairs: Vec<Value> = pairs
            .iter()
            .map(|(p, c)| json!([p, c]))
            .collect();
        ties.push((
            *from,
            *to,
            "spawned".to_string(),
            json!({
                "kind": "spawned",
                "from": from,
                "to": to,
                "pairs": pairs,
                "activeAt": at,
            }),
        ));
    }
    // Deterministic order, and the one the switchboard fixture uses: grouped
    // by kind (every project tie, then every spawned one), then by (from, to),
    // then by project name — `"project\t…"` sorting before `"spawned"`.
    ties.sort_by(|a, b| (&a.2, a.0, a.1).cmp(&(&b.2, b.0, b.1)));

    let rows: Vec<Value> = rows.iter().map(|(ws, row)| row.to_json(*ws)).collect();
    Some((rows, ties.into_iter().map(|(_, _, _, v)| v).collect()))
}

// ── The Unicode tree render ─────────────────────────────────────────────────

/// `--focus` marker: matches the full node id or the bare name/sessionId.
fn marker(focus: Option<&str>, id: &str, bare: &str) -> &'static str {
    match focus {
        Some(f) if f == id || f == bare => "▶ ",
        _ => "",
    }
}

/// Render the DAG as a Unicode box-drawing tree. Projects are `◆` roots,
/// sessions are `●` leaves; spawned children nest under their parent; sessions
/// anchored to no project group under a synthetic `(unanchored)` root.
pub fn render(
    projects: &[Project],
    sessions: &[SessionRecord],
    hooks: &[HookRecord],
    focus: Option<&str>,
) -> String {
    let projects = sorted_projects(projects);
    let sessions = merged_sessions(sessions, hooks);
    let ids: HashSet<&str> = sessions.iter().map(|s| s.session_id.as_str()).collect();

    // spawn-children by parent id (already in deterministic session order).
    let mut children: BTreeMap<String, Vec<&SessionRecord>> = BTreeMap::new();
    let mut roots: Vec<&SessionRecord> = Vec::new(); // sessions with no resolved parent
    for s in &sessions {
        match resolved_parent(s, &ids) {
            Some(p) => children.entry(p).or_default().push(s),
            None => roots.push(s),
        }
    }

    // Compact token-count formatting for the `⧉` context tag below: <1000 →
    // raw, ≥1000 → `Nk`, ≥1e6 → `N.NM` — mirrors the dock's own JS formatter
    // (LiveryState.qml `ctxCompact`) so the ASCII tree and the widget read
    // the same count the same way.
    fn compact_tokens(n: u64) -> String {
        if n < 1_000 {
            n.to_string()
        } else if n < 1_000_000 {
            format!("{}k", ((n as f64) / 1_000.0).round() as u64)
        } else {
            format!("{:.1}M", (n as f64) / 1_000_000.0)
        }
    }

    fn session_line(s: &SessionRecord, focus: Option<&str>, host: &str, role: &str) -> String {
        let id = format!("session:{}", s.session_id);
        // The canonical display grammar (petnames plan P3): `<host>/<role>/
        // <petname> (…<tail4>)`, degrading to `<host>/<role>/<sessionId>`
        // (full id) for a legacy/petname-less record — one function, every
        // human surface.
        let label = aoide_storage::display::session_label(s, host, role);
        // The running Claude model, when known — a compact `⟐ <model>` tag
        // (same glyph the gadget dock uses for a subagent's model text)
        // appended after cwd; omitted for shells and anything model-less.
        let model_tag = match s.model.as_deref() {
            Some(m) if !m.is_empty() => format!("  ⟐ {m}"),
            _ => String::new(),
        };
        // The context-window fill of the last request, when known — a compact
        // `⧉ 361k` tag (distinct glyph from the model's `⟐`, so the two never
        // read as the same kind of note); omitted until an assistant turn has
        // produced a usage block.
        let ctx_tag = match s.context_tokens {
            Some(t) if t > 0 => format!("  ⧉ {}", compact_tokens(t)),
            _ => String::new(),
        };
        // A compact marker for a shell blocked on `sudo` — parallel to the
        // model tag above, appended last so it reads as the row's most urgent
        // trailing note.
        let sudo_tag = match s.needs_sudo {
            Some(true) => "  [sudo]",
            _ => "",
        };
        format!(
            "{}● {}  {}  {}  {}{}{}{}",
            marker(focus, &id, &s.session_id),
            label,
            s.agent,
            s.state,
            s.cwd,
            model_tag,
            ctx_tag,
            sudo_tag
        )
    }

    // Recursive spawn-subtree render with a visited guard (a hand-edited
    // stage file could carry a cycle; the renderer must never loop).
    fn render_children(
        out: &mut Vec<String>,
        parent: &str,
        children: &BTreeMap<String, Vec<&SessionRecord>>,
        prefix: &str,
        focus: Option<&str>,
        visited: &mut HashSet<String>,
        host: &str,
    ) {
        let Some(kids) = children.get(parent) else {
            return;
        };
        for (i, kid) in kids.iter().enumerate() {
            if !visited.insert(kid.session_id.clone()) {
                continue;
            }
            let last = i + 1 == kids.len();
            let branch = if last { "└─ " } else { "├─ " };
            // Everything reached through `children` has a resolved parent by
            // construction (that's how it landed in this map) — role is
            // always "child" here, "root" only in the caller's own group.
            out.push(format!(
                "{prefix}{branch}{}",
                session_line(kid, focus, host, "child")
            ));
            let deeper = format!("{prefix}{}", if last { "   " } else { "│  " });
            render_children(out, &kid.session_id, children, &deeper, focus, visited, host);
        }
    }

    // Resolved ONCE per render call (not per node) — every line in this pass
    // shares the same host, matching the plan's "host resolved once per
    // render pass" rule.
    let host = aoide_storage::display::local_host_name();

    let mut out: Vec<String> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();

    // Root sessions grouped per anchoring project; leftovers are unanchored.
    let mut unanchored: Vec<&SessionRecord> = Vec::new();
    let mut per_project: Vec<Vec<&SessionRecord>> = vec![Vec::new(); projects.len()];
    for s in &roots {
        match super::model::leads_project(s, &projects)
            .or_else(|| super::model::project_for(s, &projects))
        {
            Some(i) => match super::model::lead_over(s, i, &projects, &ids) {
                Some(lead) => children.entry(lead.to_string()).or_default().push(s),
                None => per_project[i].push(s),
            },
            None => unanchored.push(s),
        }
    }

    let mut render_group = |out: &mut Vec<String>, head: String, group: &[&SessionRecord]| {
        out.push(head);
        for (i, s) in group.iter().enumerate() {
            visited.insert(s.session_id.clone());
            let last = i + 1 == group.len();
            let branch = if last { "└─ " } else { "├─ " };
            // `group` is always a `roots` slice (per-project or unanchored) —
            // role is always "root" here; children get "child" one level down.
            out.push(format!(
                "{branch}{}",
                session_line(s, focus, &host, "root")
            ));
            let deeper = if last { "   " } else { "│  " };
            render_children(
                &mut *out,
                &s.session_id,
                &children,
                deeper,
                focus,
                &mut visited,
                &host,
            );
        }
    };

    for (i, p) in projects.iter().enumerate() {
        let id = format!("project:{}", p.name);
        let mut head = format!("{}◆ {}  {}", marker(focus, &id, &p.name), p.name, p.path);
        for r in p.roots().into_iter().skip(1) {
            head.push_str(&format!("\n   {r}"));
        }
        render_group(&mut out, head, &per_project[i]);
    }
    if !unanchored.is_empty() {
        render_group(&mut out, "◆ (unanchored)".to_string(), &unanchored);
    }

    if out.is_empty() {
        return "(empty graph — no projects registered, no sessions live)".to_string();
    }
    out.join("\n")
}

// ── Pure command cores (unit-tested; the handlers wire I/O around them) ────

/// Would linking `child → parent` create a cycle? Walks the parent chain from
/// `parent` upward; a visited guard also survives pre-existing bad data.
pub fn would_cycle(sessions: &[SessionRecord], child: &str, parent: &str) -> bool {
    let by_id: BTreeMap<&str, &SessionRecord> = sessions
        .iter()
        .map(|s| (s.session_id.as_str(), s))
        .collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut cur = parent.to_string();
    loop {
        if cur == child {
            return true;
        }
        if !seen.insert(cur.clone()) {
            return false; // pre-existing cycle not involving child — stop.
        }
        match by_id
            .get(cur.as_str())
            .and_then(|s| s.parent_session_id.clone())
        {
            Some(next) if !next.is_empty() => cur = next,
            _ => return false,
        }
    }
}

/// Transitive `kind=="subagent"` descendants of `roots` — NOT including the
/// roots themselves. A Task node carries no pid/window, so cascading it out
/// when its owning session ends/is pruned/is reaped is its PRIMARY cleanup
/// path — `is_session_dead` has no fast signal for one (the reaper's staleness
/// bands can condemn a stranded sub-node left `working` past a week of
/// silence, but that is the slow backstop, not the live cleanup). Walks
/// `parent_session_id` via a fixed-point loop so subagent-of-subagent nesting
/// resolves in one call. Shared by `prune_done` (below) and
/// `do_session_end_inner` (`session_store.rs`) — the two cascade paths that
/// used to diverge (this fix's root cause).
pub(in crate::graph) fn doomed_subagent_descendants(
    sessions: &[SessionRecord],
    roots: &HashSet<&str>,
) -> HashSet<String> {
    let mut doomed: HashSet<String> = HashSet::new();
    loop {
        let mut grew = false;
        for s in sessions {
            if s.kind.as_deref() == Some("subagent")
                && !roots.contains(s.session_id.as_str())
                && !doomed.contains(s.session_id.as_str())
            {
                if let Some(p) = &s.parent_session_id {
                    if roots.contains(p.as_str()) || doomed.contains(p.as_str()) {
                        doomed.insert(s.session_id.clone());
                        grew = true;
                    }
                }
            }
        }
        if !grew {
            break;
        }
    }
    doomed
}

/// The prune computation: drop `done` sessions (+ their hook records); clear
/// `parentSessionId` on surviving children of removed sessions.
pub fn prune_done(
    sessions: Vec<SessionRecord>,
    hooks: Vec<HookRecord>,
) -> (
    Vec<SessionRecord>,
    Vec<HookRecord>,
    Vec<String>,
    Vec<String>,
) {
    prune_done_scoped(sessions, hooks, false)
}

/// [`prune_done`] with the retention scope made explicit.
///
/// A finished managed task run is a HISTORY artifact: its record is what
/// `session watch` resolves to show the instructions, the child's own unread
/// queue, the output, the outcome and the report — so routine cleanup must not
/// make a completed task vanish. An AUTOMATIC sweep (`reap_inner`, and anything
/// else routing through [`prune_done`]) therefore retains EVERY `task`-carrying
/// `done` record THIS BOX ran, filed or not; an unfiled one is retained by both
/// scopes, because the record is the report lane's own trigger. `explicit: true`
/// is the user's own `aoide session prune` — the one path allowed to drop a
/// filed task run, and even then it keeps an UNFILED one. After an explicit
/// prune the history still lives on disk (the session ledger line, the letters,
/// the PTY transcript, the instruction sidecar); only the roster record is gone.
///
/// **A DOOR SUMMON is not this box's history (P-RSA S10 review, H1).** A
/// record whose `origin` is a `node:*` value was spawned by the A2A door on a
/// peer's request (`graph/conduct.rs` refuses that shape from any environment,
/// so the door is its sole writer), and no local sweep would ever have created
/// one. Retaining those forever made the roster a remote caller's to grow: one
/// `metadata["aoide/task"]` spawn per request, each ending `done`, each kept —
/// permanent, revocation-proof state written by a grant the operator can only
/// take back for FUTURE children (`node allow <n> spawn off`, `node remove`).
/// So a door summon is retained only while its report is still owed — the
/// cursor entry is the lane's own trigger, exactly as it is for an unfiled
/// local run — and once filed it is swept like an ordinary finished session,
/// by the automatic sweep AND by `session prune`. Its durable history is
/// untouched (the ledger line, the transcript, the instruction sidecar, the
/// letters, the cursor entry all stay); only the roster record goes, exactly as
/// for a pruned local run. O(live runs) resident instead of O(all runs).
///
/// The `removed` set this returns is also the ledger's own doom list: the
/// caller hands it to [`drop_remote_child_rows`] once its `sessions.json` write
/// has landed (both callers do), because a `state/stage/remote-children.json`
/// row is a link TO a child on a far node and has no meaning once the local half
/// of that link has left the roster. It covers the explicitly-doomed ids and the
/// cascaded subagent descendants alike, since it is the whole set.
pub fn prune_done_scoped(
    sessions: Vec<SessionRecord>,
    hooks: Vec<HookRecord>,
    explicit: bool,
) -> (
    Vec<SessionRecord>,
    Vec<HookRecord>,
    Vec<String>,
    Vec<String>,
) {
    let unfiled = super::taskreport::unfiled_task_runs(&sessions);
    let doomed: HashSet<&str> = sessions
        .iter()
        .filter(|s| s.state == "done")
        .filter(|s| {
            let is_task_run = s.task.as_deref().is_some_and(|t| !t.is_empty());
            if !is_task_run {
                return true; // an ordinary finished session: swept as always
            }
            let filed = !unfiled.contains(s.session_id.as_str());
            // A door summon: kept only while its report is still owed (see the
            // doc above — a remote caller may not grow this roster forever).
            if s.origin.as_deref().is_some_and(aoide_storage::attest::is_node_origin) {
                return filed;
            }
            // A task run THIS BOX ran: retained against every automatic sweep,
            // and retained even by the explicit prune while its report is
            // unfiled.
            explicit && filed
        })
        .map(|s| s.session_id.as_str())
        .collect();
    let dropped = drop_sessions(&sessions, &doomed, hooks);
    dropped
}

/// Drop the caller-side ledger rows of every parent in `removed`
/// ([`aoide_storage::remote_children::retain_remote_children`]) — a
/// `state/stage/remote-children.json` row is a link TO a child on a far node,
/// so it has no meaning once the local half of that link has left the roster.
///
/// BOTH roster-exit paths call this (the explicit sweep and the reaper's
/// superseded-tombstone drop), and each calls it only AFTER its own
/// `sessions.json` write has succeeded: the ledger is a projection of the
/// roster, so a stage write that bailed or failed must never leave the two
/// disagreeing — a row outliving its parent, or a parent outliving its row.
/// Best-effort (a failed retain is reported, never fatal), the same posture
/// `ledger_session_exit` holds on those same two paths.
pub(crate) fn drop_remote_child_rows(removed: &[String]) {
    if removed.is_empty() {
        return;
    }
    let gone: HashSet<&str> = removed.iter().map(String::as_str).collect();
    if let Err(e) = aoide_storage::remote_children::retain_remote_children(|c| {
        !gone.contains(c.parent_session_id.as_str())
    }) {
        eprintln!("[aoide/conduct] remote-children retain failed: {e}");
    }
}

/// Drop exactly `roots` (+ their subagent descendants, + their hook records)
/// from the roster; clear `parentSessionId` on surviving children of anything
/// removed. [`prune_done`] is this over the whole `done` set — factored apart
/// because the reaper needs the same drop over a NARROWER one (the superseded
/// `done` agent siblings of a terminal that still holds a live agent — see
/// `superseded_done_siblings` in `reap.rs`), and a second hand-rolled retain
/// there would have missed the cascade and the parent-clearing this owns. A
/// root absent from `sessions` contributes nothing.
///
/// `sessions` is borrowed (not consumed like `prune_done`'s) so the caller can
/// compute the root set against the same slice it passes in.
pub(crate) fn drop_sessions(
    sessions: &[SessionRecord],
    roots: &HashSet<&str>,
    hooks: Vec<HookRecord>,
) -> (
    Vec<SessionRecord>,
    Vec<HookRecord>,
    Vec<String>,
    Vec<String>,
) {
    let mut removed: Vec<String> = sessions
        .iter()
        .filter(|s| roots.contains(s.session_id.as_str()))
        .map(|s| s.session_id.clone())
        .collect();
    // Cascade: a subagent descendant of anything being dropped is ALSO
    // gone — closes the gap where this function (unlike `do_session_end_inner`)
    // only cleared the child's dangling `parentSessionId` instead of dropping
    // it, stranding un-reapable `kind:"subagent"` ghosts (state stuck
    // "working" forever — see `is_session_dead` in reap.rs).
    let gone_direct: HashSet<&str> = removed.iter().map(String::as_str).collect();
    let cascaded = doomed_subagent_descendants(sessions, &gone_direct);
    removed.extend(cascaded);
    let gone: HashSet<&str> = removed.iter().map(String::as_str).collect();

    let mut cleared: Vec<String> = Vec::new();
    let kept_sessions: Vec<SessionRecord> = sessions
        .iter()
        .filter(|s| !gone.contains(s.session_id.as_str()))
        .cloned()
        .map(|mut s| {
            if matches!(&s.parent_session_id, Some(p) if gone.contains(p.as_str())) {
                s.parent_session_id = None;
                cleared.push(s.session_id.clone());
            }
            s
        })
        .collect();
    let kept_hooks: Vec<HookRecord> = hooks
        .into_iter()
        .filter(|h| !gone.contains(h.session_id.as_str()))
        .collect();

    (kept_sessions, kept_hooks, removed, cleared)
}

/// Append this session's line to the durable ledger (`state/session-ledger.
/// jsonl`, `aoide_storage::ledger` — P-D8, `docs/architecture/AOIDED.md`'s
/// "L5") at the moment it transitions to `done` — the "leaves the roster"
/// instant. The ONE shared call both roster-exit paths route through:
/// `do_session_end_inner` (`session_store.rs`) for a clean end, `reap_inner`
/// (`reap.rs`) for every id its own `reaped` set collects — so the ledger
/// can never double-write or diverge between the two (the test this buys:
/// exactly one line per exit, never two). Best-effort: a write failure is
/// eprintln'd and swallowed, never propagated onto the caller's own
/// stage-write success — same posture as every other best-effort side
/// channel in this crate (the reap toast, the transcript refresh).
/// `restore` (P-C5, durable-sessions plan) is projected verbatim off the
/// live record — never re-derived here, and never a `/proc` read: by the
/// time either roster-exit path calls this, the process this line is about
/// may already be gone (`reap_inner` in particular calls it AFTER setting
/// `state = "done"`), so the record's own `restore` snapshot, captured
/// continuously by the PTY tick while the session was alive, is the only
/// honest source.
pub(crate) fn ledger_session_exit(rec: &SessionRecord, ended_at: &str) {
    let entry = aoide_storage::ledger::LedgerEntry {
        v: 0,
        session_id: rec.session_id.clone(),
        enduring_agent_id: rec.enduring_agent_id.clone(),
        project: rec.project.clone(),
        agent: rec.agent.clone(),
        harness_session_id: rec.harness_session_id.clone(),
        cwd: rec.cwd.clone(),
        title: rec.title.clone(),
        petname: rec.petname.clone(),
        started_at: rec.started_at.clone(),
        ended_at: ended_at.to_string(),
        resumed_from: rec.resumed_from.clone(),
        origin: rec.origin.clone(),
        restore: rec.restore.clone(),
    };
    if let Err(e) = aoide_storage::ledger::append_ledger_entry(&entry) {
        eprintln!(
            "[aoide/conduct] session ledger append failed for {}: {e}",
            rec.session_id
        );
    }
}

/// Re-stage `graph.json` from the CURRENT registries so the document Quickshell
/// hot-reloads never drifts from what `graph view` (and `graph prune`'s
/// manual resync) would compute. Every mutation of projects/sessions calls this, so the staged
/// graph is always a pure function of the registries — the staged doc can no
/// longer go stale behind a `project add`/`remove`/`link`/`prune`.
/// `pub`, not `pub(crate)`: `aoide-server`'s
/// `daemon::reconcile_graph_projection` calls this directly to re-derive
/// after an out-of-band hand edit — the one cross-crate consumer.
pub fn restage_graph() -> Result<PathBuf, String> {
    let p: ProjectsFile = load_stage(&projects_path())?;
    let s: SessionsFile = load_stage(&sessions_path())?;
    let h: HooksFile = load_stage(&hooks_path())?;
    let doc = build_graph(&p.projects, &s.sessions, &h.hooks);
    let path = graph_path();
    write_stage(&path, &doc)?;
    Ok(path)
}

/// Resolve the CURRENT graph document straight off the stage registries —
/// the exact same three-file-load-then-`build_graph` shape [`restage_graph`]
/// runs (minus the write). `pub`, not `pub(crate)`: `aoide-server`'s
/// `aoide/graphSummary` (CONTRACTS.md §7) reuses this so the wire response
/// and a fresh `graph view --json` can never diverge into two
/// graph vocabularies — the whole point of wrapping `build_graph`'s output
/// verbatim rather than inventing a second shape for the federation door.
pub fn resolve_graph_document() -> Result<Value, String> {
    let p: ProjectsFile = load_stage(&projects_path())?;
    let s: SessionsFile = load_stage(&sessions_path())?;
    let h: HooksFile = load_stage(&hooks_path())?;
    Ok(build_graph(&p.projects, &s.sessions, &h.hooks))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::testutil::*;

    #[test]
    fn link_cycle_rejection() {
        let sessions = vec![
            session("a", "/x", "running", "1", None),
            session("b", "/x", "running", "2", Some("a")),
            session("c", "/x", "running", "3", Some("b")),
        ];
        // a → c closes the chain c→b→a: cycle.
        assert!(would_cycle(&sessions, "a", "c"));
        // Self-parented chain data must not loop the walker.
        assert!(would_cycle(&sessions, "b", "b"));
        // A fresh parent is fine.
        assert!(!would_cycle(&sessions, "c", "a"));
        assert!(!would_cycle(&sessions, "a", "unregistered"));
    }
    #[test]
    fn a_live_lead_is_anchored_and_the_rest_of_the_project_hangs_off_it() {
        let mut projects = fixture_projects();
        projects[1].lead = Some("s2".into());
        let sessions = vec![
            session("s1", "/home/k/Aoide", "running", "2026-01-01T00:00:00Z", None),
            session("s2", "/tmp", "running", "2026-01-02T00:00:00Z", None),
            session("s3", "/home/k/Aoide", "idle", "2026-01-03T00:00:00Z", Some("s1")),
        ];
        let doc = build_graph(&projects, &sessions, &[]);
        let edges: Vec<(String, String, String)> = doc["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["from"].as_str().unwrap().into(),
                    e["to"].as_str().unwrap().into(),
                    e["kind"].as_str().unwrap().into(),
                )
            })
            .collect();
        // The lead anchors to the project it leads, whatever its cwd says;
        // the project's other root hangs off the lead; spawned is untouched.
        // Edges land in session order (s1, s2, s3), as everywhere else.
        assert_eq!(
            edges,
            vec![
                ("session:s2".into(), "session:s1".into(), "leads".into()),
                ("project:aoide".into(), "session:s2".into(), "anchors".into()),
                ("session:s1".into(), "session:s3".into(), "spawned".into()),
            ]
        );
        let project = doc["nodes"].as_array().unwrap().iter().find(|n| n["id"] == "project:aoide").unwrap();
        assert_eq!(project["lead"], "s2");

        let host = aoide_storage::display::local_host_name();
        let expected = format!(
            "\
◆ aoide  /home/k/Aoide
└─ ● {host}/root/s2  claude  working  /tmp
   └─ ● {host}/child/s1  claude  working  /home/k/Aoide
      └─ ● {host}/child/s3  claude  idle  /home/k/Aoide
◆ nested  /home/k/Aoide/sub"
        );
        assert_eq!(render(&projects, &sessions, &[], None), expected);

        // A lead that has left the roster changes nothing.
        projects[1].lead = Some("gone".into());
        let doc = build_graph(&projects, &sessions, &[]);
        assert_eq!(doc["edges"][0]["kind"], "anchors");
        assert_eq!(doc["edges"][0]["to"], "session:s1");
    }

    #[test]
    fn render_is_deterministic_snapshot() {
        let projects = fixture_projects();
        let sessions = vec![
            // Deliberately unsorted; ordering must come from (startedAt, id).
            session("s4", "/tmp", "idle", "2026-01-04T00:00:00Z", None),
            session(
                "s2",
                "/home/k/Aoide/sub/x",
                "running",
                "2026-01-02T00:00:00Z",
                None,
            ),
            session(
                "s1",
                "/home/k/Aoide",
                "running",
                "2026-01-01T00:00:00Z",
                None,
            ),
            session(
                "s3",
                "/home/k/elsewhere",
                "idle",
                "2026-01-03T00:00:00Z",
                Some("s1"),
            ),
        ];
        // Hook state merge: s1's latest hook phase becomes its live state, folded
        // to the canonical vocab (working → idle here; latest updatedAt wins).
        let hooks = vec![
            HookRecord {
                session_id: "s1".into(),
                phase: "working".into(),
                updated_at: "2026-01-01T01:00:00Z".into(),
                extra: Map::new(),
            },
            HookRecord {
                session_id: "s1".into(),
                phase: "idle".into(),
                updated_at: "2026-01-01T02:00:00Z".into(),
                extra: Map::new(),
            },
        ];
        // Roster `running` folds to canonical `working`; the merged hook phase and
        // the resting states render verbatim from the one vocabulary. None of
        // these fixtures carry a minted petname, so every head degrades to
        // `<host>/<role>/<sessionId>` — s1/s2/s4 are roots, s3 is s1's child.
        let host = aoide_storage::display::local_host_name();
        let expected = format!(
            "\
◆ aoide  /home/k/Aoide
└─ ● {host}/root/s1  claude  idle  /home/k/Aoide
   └─ ● {host}/child/s3  claude  idle  /home/k/elsewhere
◆ nested  /home/k/Aoide/sub
└─ ● {host}/root/s2  claude  working  /home/k/Aoide/sub/x
◆ (unanchored)
└─ ● {host}/root/s4  claude  idle  /tmp"
        );
        assert_eq!(render(&projects, &sessions, &hooks, None), expected);
        // The focus marker singles out one node.
        let focused = render(&projects, &sessions, &hooks, Some("session:s2"));
        assert!(focused.contains(&format!("└─ ▶ ● {host}/root/s2  claude  working")));
        // Same inputs → same render (deterministic).
        assert_eq!(render(&projects, &sessions, &hooks, None), expected);
    }
    #[test]
    fn render_shows_model_tag_on_agent_and_subagent_nodes_when_known() {
        let projects = fixture_projects();
        let mut parent = session("s1", "/home/k/Aoide", "running", "1", None);
        parent.model = Some("claude-sonnet-5".into());
        let mut sub = session("s2", "/home/k/Aoide", "working", "2", Some("s1"));
        sub.kind = Some("subagent".into());
        sub.model = Some("claude-fable-5".into());
        // A shell (or any model-less record) carries no model — the tag stays
        // absent rather than printing an empty `⟐ `.
        let shell = session("s3", "/home/k/Aoide", "idle", "3", None);
        let sessions = vec![parent, sub, shell];
        let out = render(&projects, &sessions, &[], None);
        let host = aoide_storage::display::local_host_name();
        assert!(
            out.contains(&format!("● {host}/root/s1  claude  working  /home/k/Aoide  ⟐ claude-sonnet-5")),
            "agent node carries its model tag: {out}"
        );
        assert!(
            out.contains(&format!("● {host}/child/s2  claude  working  /home/k/Aoide  ⟐ claude-fable-5")),
            "subagent node carries its own (possibly different) model tag: {out}"
        );
        assert!(
            out.contains(&format!("● {host}/root/s3  claude  idle  /home/k/Aoide\n")),
            "model-less node has no dangling tag: {out}"
        );
        assert!(!out.contains('⟐') || out.matches('⟐').count() == 2, "exactly two model tags: {out}");
    }
    #[test]
    fn graph_document_edges_match_the_render_shape() {
        let projects = fixture_projects();
        let sessions = vec![
            session("s1", "/home/k/Aoide", "running", "1", None),
            session("s3", "/home/k/elsewhere", "idle", "2", Some("s1")),
        ];
        let doc = build_graph(&projects, &sessions, &[]);
        let edges = doc["edges"].as_array().unwrap();
        // s1 anchors under project:aoide; s3 hangs off s1 only (no anchor edge).
        assert!(edges.iter().any(|e| e["from"] == "project:aoide"
            && e["to"] == "session:s1"
            && e["kind"] == "anchors"));
        assert!(edges.iter().any(|e| e["from"] == "session:s1"
            && e["to"] == "session:s3"
            && e["kind"] == "spawned"));
        assert_eq!(edges.len(), 2);
        assert_eq!(doc["schemaVersion"], "0");
    }
    #[test]
    fn the_graph_project_node_carries_every_root() {
        let projects = vec![
            Project {
                name: "two-root".into(),
                path: "/a".into(),
                roots: vec!["/b".into()],
                ..Default::default()
            },
            Project {
                name: "one-root".into(),
                path: "/a".into(),
                ..Default::default()
            },
        ];
        let doc = build_graph(&projects, &[], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let two = nodes.iter().find(|n| n["id"] == "project:two-root").unwrap();
        assert_eq!(two["path"], "/a");
        assert_eq!(two["roots"], json!(["/a", "/b"]));
        let one = nodes.iter().find(|n| n["id"] == "project:one-root").unwrap();
        assert_eq!(
            one["roots"],
            json!(["/a"]),
            "roots is ALWAYS present, even for a one-root project"
        );
    }
    #[test]
    fn the_graph_project_node_carries_hosts_only_when_non_empty() {
        use crate::graph::model::ProjectHost;
        let projects = vec![
            Project {
                name: "hosted".into(),
                path: "/a".into(),
                hosts: vec![ProjectHost { name: "n1".into(), roots: vec!["/srv/n1/a".into()] }],
                ..Default::default()
            },
            Project {
                name: "local-only".into(),
                path: "/a".into(),
                ..Default::default()
            },
        ];
        let doc = build_graph(&projects, &[], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let hosted = nodes.iter().find(|n| n["id"] == "project:hosted").unwrap();
        assert_eq!(hosted["hosts"], json!([{"name": "n1", "roots": ["/srv/n1/a"]}]));
        let local = nodes.iter().find(|n| n["id"] == "project:local-only").unwrap();
        assert!(local.get("hosts").is_none(), "no hosts field when empty: {local}");
    }
    #[test]
    fn render_shows_a_projects_extra_roots_under_its_head() {
        let projects = vec![Project {
            name: "aoide".into(),
            path: "/a".into(),
            roots: vec!["/b".into()],
            ..Default::default()
        }];
        let tree = render(&projects, &[], &[], None);
        let lines: Vec<&str> = tree.lines().collect();
        let head_idx = lines
            .iter()
            .position(|l| l.contains("◆ aoide") && l.contains("/a"))
            .expect("head line present");
        assert!(
            lines[head_idx + 1].contains("/b"),
            "the second root follows the head line: {tree}"
        );
    }
    #[test]
    fn resumed_from_projects_an_additive_resumed_edge_beside_anchors() {
        // P-D8: a revived session carries BOTH its ordinary anchors edge
        // (from `graph resurrect`'s windowed re-registration, unrelated to
        // resurrection) AND an additive `resumed` edge naming the ledger
        // entry's own sessionId — the source need not be a live node (the
        // whole premise of resurrecting it is that it already left the
        // roster), so the edge renders even though `session:old-1` has no
        // matching node.
        let projects = fixture_projects();
        let mut s = session("s1", "/home/k/Aoide", "running", "1", None);
        s.resumed_from = Some("old-1".to_string());
        let doc = build_graph(&projects, &[s], &[]);
        let edges = doc["edges"].as_array().unwrap();
        assert!(edges.iter().any(|e| e["from"] == "project:aoide"
            && e["to"] == "session:s1"
            && e["kind"] == "anchors"));
        assert!(
            edges.iter().any(|e| e["from"] == "session:old-1"
                && e["to"] == "session:s1"
                && e["kind"] == "resumed"),
            "edges: {edges:?}"
        );
        assert_eq!(edges.len(), 2);
    }
    #[test]
    fn prune_clears_orphaned_parent_links() {
        let sessions = vec![
            session("p", "/x", "done", "1", None),
            session("c1", "/x", "running", "2", Some("p")),
            session("c2", "/x", "done", "3", Some("p")),
            session("free", "/x", "idle", "4", None),
        ];
        let hooks = vec![
            HookRecord {
                session_id: "p".into(),
                phase: "Stop".into(),
                updated_at: "1".into(),
                extra: Map::new(),
            },
            HookRecord {
                session_id: "c1".into(),
                phase: "PreToolUse".into(),
                updated_at: "2".into(),
                extra: Map::new(),
            },
        ];
        let (kept_s, kept_h, removed, cleared) = prune_done(sessions, hooks);
        assert_eq!(removed, vec!["p".to_string(), "c2".to_string()]);
        assert_eq!(cleared, vec!["c1".to_string()]);
        let ids: Vec<&str> = kept_s.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(ids, vec!["c1", "free"]);
        assert!(kept_s.iter().all(|s| s.parent_session_id.is_none()));
        // p's hook record went with it; c1's survives.
        assert_eq!(kept_h.len(), 1);
        assert_eq!(kept_h[0].session_id, "c1");
    }
    #[test]
    fn prune_cascades_subagent_descendants_of_a_done_session() {
        let top = session("top", "/x", "done", "1", None);
        let mut sub1 = session("sub:t1", "/x", "working", "2", Some("top"));
        sub1.kind = Some("subagent".into());
        // subagent-of-subagent: multi-level nesting must cascade in one pass.
        let mut sub2 = session("sub:t2", "/x", "working", "3", Some("sub:t1"));
        sub2.kind = Some("subagent".into());
        let free = session("free", "/x", "idle", "4", None);
        let sessions = vec![top, sub1, sub2, free];

        let (kept_s, _kept_h, mut removed, _cleared) = prune_done(sessions, vec![]);
        removed.sort();
        assert_eq!(
            removed,
            vec!["sub:t1".to_string(), "sub:t2".to_string(), "top".to_string()]
        );
        let ids: Vec<&str> = kept_s.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(ids, vec!["free"], "the whole subagent subtree cascades with its done parent");
    }
    #[test]
    fn graph_node_carries_workspace_only_when_known() {
        // The hover-preview bridge is pure data: build_graph stamps `workspace`
        // onto a session node when resolved, and omits it entirely otherwise so a
        // legacy/off-Hyprland record round-trips byte-for-byte.
        let mut with_ws = SessionRecord {
            session_id: "a".into(),
            window_address: "0xaaa".into(),
            ..Default::default()
        };
        with_ws.workspace = Some(4);
        let without_ws = SessionRecord {
            session_id: "b".into(),
            window_address: "0xbbb".into(),
            ..Default::default()
        };
        let doc = build_graph(&[], &[with_ws, without_ws], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let node_a = nodes.iter().find(|n| n["id"] == "session:a").unwrap();
        let node_b = nodes.iter().find(|n| n["id"] == "session:b").unwrap();
        assert_eq!(node_a["workspace"], json!(4));
        assert!(node_b.get("workspace").is_none());
    }

    // ── §E: the compositor block (graph.json `workspaces` + `ties`) ────────

    /// Two workspaces bound to ONE project are a project tie between them, and
    /// the block carries a row for a bound workspace even with no session on
    /// it. The names are the DESIGN's own S3 gate names.
    #[test]
    fn project_tie_between_two_bound_workspaces() {
        let projects = vec![Project {
            name: "aoide".into(),
            workspaces: vec![3, 5],
            ..Default::default()
        }];
        let doc = build_graph(&projects, &[], &[]);
        assert_eq!(
            doc["workspaces"],
            json!([
                { "workspace": 3, "project": "aoide", "projects": ["aoide"], "sessions": [],
                  "live": 0, "working": 0, "awaiting": 0, "activeAt": "" },
                { "workspace": 5, "project": "aoide", "projects": ["aoide"], "sessions": [],
                  "live": 0, "working": 0, "awaiting": 0, "activeAt": "" },
            ]),
            "{doc}"
        );
        assert_eq!(
            doc["ties"],
            json!([
                { "kind": "project", "from": 3, "to": 5, "project": "aoide", "activeAt": "" }
            ]),
            "{doc}"
        );
    }

    /// A spawned tie is drawn only when the two ends sit on DIFFERENT
    /// workspaces, and it leaves from the nearest ancestor that has a window
    /// at all — not merely from the parent.
    #[test]
    fn spawned_tie_only_across_workspaces() {
        let mut parent = session("p", "/home/k/Aoide", "working", "2026-09-26T13:00:00Z", None);
        parent.workspace = Some(2);
        let mut same = session("same", "/home/k/Aoide", "working", "2026-09-26T13:01:00Z", Some("p"));
        same.workspace = Some(2);
        let mut across = session("across", "/home/k/Aoide", "working", "2026-09-26T13:02:00Z", Some("p"));
        across.workspace = Some(7);
        // A headless child of the SAME child, on another workspace: its tie
        // must leave from the parent's window, not from the headless middle.
        let mut middle = session("mid", "/home/k/Aoide", "working", "2026-09-26T13:03:00Z", Some("across"));
        middle.workspace = None;
        let mut far = session("far", "/home/k/Aoide", "working", "2026-09-26T13:04:00Z", Some("mid"));
        far.workspace = Some(9);

        let doc = build_graph(&[], &[parent, same, across, middle, far], &[]);
        assert_eq!(
            doc["ties"],
            json!([
                { "kind": "spawned", "from": 2, "to": 7, "pairs": [["p", "across"]],
                  "activeAt": "2026-09-26T13:02:00Z" },
                { "kind": "spawned", "from": 7, "to": 9, "pairs": [["across", "far"]],
                  "activeAt": "2026-09-26T13:04:00Z" },
            ]),
            "the same-workspace child draws nothing, and the headless middle is \
             stepped over: {doc}"
        );
    }

    /// A child with no window has no tie of its own — its activity is its
    /// parent's, and nothing is invented.
    #[test]
    fn headless_child_makes_no_tie() {
        let mut parent = session("p", "/home/k/Aoide", "working", "2026-09-26T13:00:00Z", None);
        parent.workspace = Some(2);
        let headless = session("h", "/home/k/Aoide", "working", "2026-09-26T13:01:00Z", Some("p"));
        let doc = build_graph(&[], &[parent, headless], &[]);
        assert_eq!(doc["ties"], json!([]), "{doc}");
        // The child is still counted on the workspace its PARENT sits on: the
        // row lists what the roster reports, and a headless session reports no
        // workspace of its own.
        assert_eq!(doc["workspaces"][0]["workspace"], 2);
        assert_eq!(doc["workspaces"][0]["sessions"], json!(["p"]));
    }

    /// `activeAt` is the latest hook `updatedAt` for that session, falling back
    /// to `startedAt` — and a tie's is the max over its two ends.
    #[test]
    fn active_at_is_the_latest_hook() {
        let mut a = session("a", "/home/k/Aoide", "working", "2026-09-26T13:00:00Z", None);
        a.workspace = Some(2);
        let mut b = session("b", "/home/k/Aoide", "working", "2026-09-26T13:00:00Z", Some("a"));
        b.workspace = Some(7);
        let hooks = vec![
            HookRecord { session_id: "a".into(), phase: "working".into(), updated_at: "2026-09-26T13:05:00Z".into(), ..Default::default() },
            // An OLDER hook record for the same session never wins.
            HookRecord { session_id: "a".into(), phase: "idle".into(), updated_at: "2026-09-26T12:00:00Z".into(), ..Default::default() },
            HookRecord { session_id: "b".into(), phase: "working".into(), updated_at: "2026-09-26T14:00:00Z".into(), ..Default::default() },
        ];
        let doc = build_graph(&[], &[a, b], &hooks);
        let nodes = doc["nodes"].as_array().unwrap();
        let node_a = nodes.iter().find(|n| n["id"] == "session:a").unwrap();
        let node_b = nodes.iter().find(|n| n["id"] == "session:b").unwrap();
        assert_eq!(node_a["activeAt"], "2026-09-26T13:05:00Z");
        assert_eq!(node_b["activeAt"], "2026-09-26T14:00:00Z");
        assert_eq!(doc["workspaces"][0]["activeAt"], "2026-09-26T13:05:00Z");
        assert_eq!(doc["workspaces"][1]["activeAt"], "2026-09-26T14:00:00Z");
        assert_eq!(
            doc["ties"][0]["activeAt"], "2026-09-26T14:00:00Z",
            "a tie pulses at the LATER of its two ends: {doc}"
        );

        // No hook at all → `startedAt` is the pulse.
        let mut solo = session("solo", "/home/k/Aoide", "working", "2026-09-26T13:00:00Z", None);
        solo.workspace = Some(2);
        let doc = build_graph(&[], &[solo], &[]);
        assert_eq!(doc["nodes"][0]["activeAt"], "2026-09-26T13:00:00Z");
    }

    /// §E: a headless child has no tie of its own and no row of its own —
    /// "its activity shows on its parent's workspace". Subagents are most of
    /// this product's traffic, so without this fold their work pulses nothing.
    #[test]
    fn headless_child_activity_pulses_its_parents_workspace() {
        let mut parent = session("p", "/home/k/Aoide", "working", "2026-09-26T13:00:00Z", None);
        parent.workspace = Some(2);
        let headless = session("h", "/home/k/Aoide", "working", "2026-09-26T13:01:00Z", Some("p"));
        // A headless grandchild, too: the fold walks to the nearest WINDOWED
        // ancestor, not merely one rung up.
        let grandchild = session("g", "/home/k/Aoide", "working", "2026-09-26T13:02:00Z", Some("h"));
        let hooks = vec![
            HookRecord { session_id: "p".into(), phase: "working".into(), updated_at: "2026-09-26T13:10:00Z".into(), ..Default::default() },
            HookRecord { session_id: "h".into(), phase: "working".into(), updated_at: "2026-09-26T23:59:00Z".into(), ..Default::default() },
            HookRecord { session_id: "g".into(), phase: "working".into(), updated_at: "2026-09-26T14:30:00Z".into(), ..Default::default() },
        ];
        let doc = build_graph(&[], &[parent, headless, grandchild], &hooks);
        let row = &doc["workspaces"][0];
        assert_eq!(row["workspace"], 2);
        assert_eq!(
            row["activeAt"], "2026-09-26T23:59:00Z",
            "the headless child's later activity IS the row's pulse: {doc}"
        );
        assert_eq!(
            row["sessions"],
            json!(["p"]),
            "activity only — no session id joins the row: {row}"
        );
        assert_eq!(row["live"], 1, "no count moves: {row}");
        assert_eq!(doc["ties"], json!([]), "and no tie is drawn: {doc}");
        // The children still carry their OWN pulse on their nodes, which is
        // what a DAG watcher reads.
        let nodes = doc["nodes"].as_array().unwrap();
        let node_h = nodes.iter().find(|n| n["id"] == "session:h").unwrap();
        assert_eq!(node_h["activeAt"], "2026-09-26T23:59:00Z");
        assert!(node_h.get("workspace").is_none());
    }

    /// Nothing in play — no workspace on any session, no binding anywhere —
    /// means no `workspaces`, no `ties`, and no session `activeAt`: the
    /// document is byte-for-byte what it was before this slice.
    #[test]
    fn nothing_in_play_leaves_the_document_byte_identical() {
        let sessions = vec![
            session("a", "/home/k/Aoide", "working", "2026-09-26T13:00:00Z", None),
            session("b", "/home/k/Aoide", "working", "2026-09-26T13:01:00Z", Some("a")),
        ];
        let hooks = vec![HookRecord {
            session_id: "a".into(),
            phase: "awaiting".into(),
            updated_at: "2026-09-26T13:02:00Z".into(),
            ..Default::default()
        }];
        let projects = vec![Project { name: "aoide".into(), path: "/home/k/Aoide".into(), ..Default::default() }];
        let doc = build_graph(&projects, &sessions, &hooks);
        assert!(doc.get("workspaces").is_none(), "{doc}");
        assert!(doc.get("ties").is_none(), "{doc}");
        for n in doc["nodes"].as_array().unwrap() {
            assert!(n.get("activeAt").is_none(), "no pulse off a workspace: {n}");
        }
        // And the same document with NO hook phase merged in at all — the
        // pre-slice shape — keys exactly the same.
        let bare = build_graph(&projects, &sessions, &[]);
        assert_eq!(
            doc["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n["id"].clone())
                .collect::<Vec<_>>(),
            bare["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n["id"].clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(
            doc.as_object().unwrap().keys().collect::<Vec<_>>(),
            bare.as_object().unwrap().keys().collect::<Vec<_>>()
        );
    }
    #[test]
    fn graph_node_carries_model_only_when_known() {
        // Mirrors the workspace test above: `model` rides onto a session node
        // (agent or subagent alike) only when the record has one, so a
        // legacy/model-less record round-trips byte-for-byte.
        let with_model = SessionRecord {
            session_id: "a".into(),
            window_address: "0xaaa".into(),
            model: Some("claude-fable-5".into()),
            ..Default::default()
        };
        let without_model = SessionRecord {
            session_id: "b".into(),
            window_address: "0xbbb".into(),
            ..Default::default()
        };
        let doc = build_graph(&[], &[with_model, without_model], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let node_a = nodes.iter().find(|n| n["id"] == "session:a").unwrap();
        let node_b = nodes.iter().find(|n| n["id"] == "session:b").unwrap();
        assert_eq!(node_a["model"], json!("claude-fable-5"));
        assert!(node_b.get("model").is_none());
    }
    #[test]
    fn graph_node_carries_petname_only_when_known() {
        // Mirrors the model test above: `petname` rides onto a session node
        // only when the record has one minted, so a legacy/petname-less
        // record round-trips byte-for-byte.
        let with_petname = SessionRecord {
            session_id: "a".into(),
            window_address: "0xaaa".into(),
            petname: Some("brave-otter".into()),
            ..Default::default()
        };
        let without_petname = SessionRecord {
            session_id: "b".into(),
            window_address: "0xbbb".into(),
            ..Default::default()
        };
        let doc = build_graph(&[], &[with_petname, without_petname], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let node_a = nodes.iter().find(|n| n["id"] == "session:a").unwrap();
        let node_b = nodes.iter().find(|n| n["id"] == "session:b").unwrap();
        assert_eq!(node_a["petname"], json!("brave-otter"));
        assert!(node_b.get("petname").is_none());
    }
    #[test]
    fn the_graph_session_node_carries_native_role_only_when_present() {
        // Root order seq 404: `nativeRole` rides onto a session node only
        // when the native harness published one — `role` (from `kind`)
        // stays `"app"` regardless, never reclassified for this.
        let with_role = SessionRecord {
            session_id: "a".into(),
            kind: Some("app".into()),
            native_role: Some("subagent".into()),
            ..Default::default()
        };
        let without_role = SessionRecord {
            session_id: "b".into(),
            kind: Some("app".into()),
            ..Default::default()
        };
        let doc = build_graph(&[], &[with_role, without_role], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let node_a = nodes.iter().find(|n| n["id"] == "session:a").unwrap();
        let node_b = nodes.iter().find(|n| n["id"] == "session:b").unwrap();
        assert_eq!(node_a["nativeRole"], json!("subagent"));
        assert_eq!(node_a["role"], json!("app"), "kind stays app, never reclassified");
        assert!(node_b.get("nativeRole").is_none());
    }
    #[test]
    fn graph_json_publishes_effective_project_on_a_child_outside_its_cwd_anchor() {
        // The child's own cwd anchors nowhere; its owner's explicit project
        // is the resolver's rung 2 — additive `effectiveProject` carries that
        // resolved value while the STORED `project` (absent on the child)
        // keeps publishing untouched.
        let projects = vec![Project { name: "aoide".into(), path: "/home/k/Aoide".into(), ..Default::default() }];
        let root = SessionRecord { project: Some("aoide".into()), ..session("s-root", "/home/k/Aoide", "working", "1", None) };
        let child = session("s-child", "/tmp/elsewhere", "working", "2", Some("s-root"));
        let doc = build_graph(&projects, &[root, child], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let node_child = nodes.iter().find(|n| n["id"] == "session:s-child").unwrap();
        assert_eq!(node_child["effectiveProject"], json!("aoide"));
        assert!(node_child.get("project").is_none(), "the stored project stays absent on the child");
    }
    #[test]
    fn graph_json_omits_effective_project_when_nothing_resolves() {
        // No projects registered, no parent to inherit from, no cwd anchor:
        // the resolver yields `None` and the key stays off the node entirely
        // — never a dangling `effectiveProject: null`.
        let solo = session("s-solo", "/tmp/nowhere", "working", "1", None);
        let doc = build_graph(&[], &[solo], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let node = nodes.iter().find(|n| n["id"] == "session:s-solo").unwrap();
        assert!(node.get("effectiveProject").is_none());
    }
    #[test]
    fn graph_node_carries_context_tokens_only_when_known() {
        // Mirrors the workspace/model tests above: `contextTokens` rides onto a
        // session node only when the record has one, so a legacy/pre-assistant-
        // turn record round-trips byte-for-byte.
        let with_ctx = SessionRecord {
            session_id: "a".into(),
            window_address: "0xaaa".into(),
            context_tokens: Some(361_416),
            ..Default::default()
        };
        let without_ctx = SessionRecord {
            session_id: "b".into(),
            window_address: "0xbbb".into(),
            ..Default::default()
        };
        let doc = build_graph(&[], &[with_ctx, without_ctx], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let node_a = nodes.iter().find(|n| n["id"] == "session:a").unwrap();
        let node_b = nodes.iter().find(|n| n["id"] == "session:b").unwrap();
        assert_eq!(node_a["contextTokens"], json!(361_416));
        assert!(node_b.get("contextTokens").is_none());
    }
    #[test]
    fn render_shows_context_tag_when_known() {
        // The ASCII render's `⧉ <compact>` tag mirrors the model tag's `⟐`
        // precedent: present + compact-formatted when known, absent otherwise.
        let mut s = session("a", "/x", "working", "2024-01-01T00:00:00Z", None);
        s.context_tokens = Some(361_416);
        let out = render(&[], &[s], &[], None);
        assert!(out.contains("⧉ 361k"), "expected a compact context tag: {out}");

        let bare = session("b", "/x", "working", "2024-01-01T00:00:00Z", None);
        let out2 = render(&[], &[bare], &[], None);
        assert!(!out2.contains('⧉'), "no dangling context tag: {out2}");
    }
    #[test]
    fn graph_node_carries_needs_sudo_only_when_true() {
        // Mirrors the workspace/model tests above: `needsSudo` rides onto a
        // session node only when Some(true) — never a dangling `false`, and
        // absent entirely for a legacy/not-blocked record.
        let mut blocked = SessionRecord {
            session_id: "a".into(),
            window_address: "0xaaa".into(),
            ..Default::default()
        };
        blocked.needs_sudo = Some(true);
        let free = SessionRecord {
            session_id: "b".into(),
            window_address: "0xbbb".into(),
            ..Default::default()
        };
        let doc = build_graph(&[], &[blocked, free], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let node_a = nodes.iter().find(|n| n["id"] == "session:a").unwrap();
        let node_b = nodes.iter().find(|n| n["id"] == "session:b").unwrap();
        assert_eq!(node_a["needsSudo"], json!(true));
        assert!(node_b.get("needsSudo").is_none());
    }
    #[test]
    fn graph_reports_conductable_only_when_the_socket_still_exists_on_disk() {
        // The read-time fix (`is_conductable_now`): `conductable` is the
        // stored flag AND the socket path existing on disk, never the stored
        // flag echoed verbatim. `shellbridge.service` owns
        // `$XDG_RUNTIME_DIR/aoide` with `RuntimeDirectoryPreserve=no`, so a
        // rebuild deletes a live session's socket file without ever touching
        // the stored record — this is the regression that let `aoide graph
        // --json` report a dead session conductable forever.
        let dir = unique_stage("doc-conductable");
        let sock = dir.join("session-a.sock");
        std::fs::write(&sock, b"").unwrap();

        let rec = SessionRecord {
            session_id: "a".into(),
            window_address: "0xaaa".into(),
            conductable: Some(true),
            socket: Some(sock.to_string_lossy().into_owned()),
            ..Default::default()
        };
        let doc = build_graph(&[], &[rec.clone()], &[]);
        let node = doc["nodes"].as_array().unwrap().iter().find(|n| n["id"] == "session:a").unwrap();
        assert_eq!(node["conductable"], json!(true), "an existing socket reports conductable");

        // The socket vanishes (a rebuild deleting the runtime dir) — the
        // ORIGINAL record is never touched; only the REPORTED value changes.
        std::fs::remove_file(&sock).unwrap();
        let doc2 = build_graph(&[], &[rec.clone()], &[]);
        let node2 = doc2["nodes"].as_array().unwrap().iter().find(|n| n["id"] == "session:a").unwrap();
        assert_eq!(node2["conductable"], json!(false), "a removed socket reports NOT conductable");
        assert_eq!(rec.conductable, Some(true), "the stored field is never modified by a read");

        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn graph_reports_not_conductable_with_no_socket_path() {
        // A record with `conductable: Some(true)` but no socket at all — or
        // an empty one — is not-conductable, the same shape `send.rs`'s own
        // gate already filters for
        // (`rec.socket.clone().filter(|s| !s.is_empty())`).
        let no_socket = SessionRecord {
            session_id: "a".into(),
            window_address: "0xaaa".into(),
            conductable: Some(true),
            socket: None,
            ..Default::default()
        };
        let empty_socket = SessionRecord {
            session_id: "b".into(),
            window_address: "0xbbb".into(),
            conductable: Some(true),
            socket: Some(String::new()),
            ..Default::default()
        };
        let doc = build_graph(&[], &[no_socket, empty_socket], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let node_a = nodes.iter().find(|n| n["id"] == "session:a").unwrap();
        let node_b = nodes.iter().find(|n| n["id"] == "session:b").unwrap();
        assert_eq!(node_a["conductable"], json!(false));
        assert_eq!(node_b["conductable"], json!(false));
    }
    #[test]
    fn conducted_pty_host_classification_is_unaffected_by_a_missing_socket() {
        // The regression this derivation must never cause: a REPORT going
        // false when the socket vanishes must never leak into internal
        // classification, which keeps reading the STORED field directly.
        // `is_agent_kind` (reap.rs) never counts a conducted PTY host as an
        // agent-duplicate candidate — true whether or not its socket exists.
        let rec = SessionRecord {
            session_id: "a".into(),
            window_address: "0xaaa".into(),
            conductable: Some(true),
            socket: Some("/nonexistent/session-a.sock".into()),
            ..Default::default()
        };
        assert!(!crate::reap::is_agent_kind(&rec));
    }
    #[test]
    fn build_graph_folds_a_fresh_node_as_a_root_node_with_nested_children() {
        // CONTRACTS.md §7: a registered node with a FRESH (non-stale,
        // within-TTL) pulled cache folds in as a `kind:"node"` root node
        // whose own resolved graph nests as `children` — never flattened
        // into this document's own top-level `nodes`/`edges`.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let dir = std::env::temp_dir().join(format!("aoide-node-fold-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("AOIDE_STATE_DIR", &dir);

        aoide_storage::node_store::save_nodes(&[aoide_storage::node_store::Node {
            name: "yomi-strix".into(),
            url: "http://yomi-strix:8710/".into(),
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: None,
            verified: false,
            allows: Vec::new(),
            via: None,
            added_at: "2026-08-14T00:00:00Z".into(),
        }])
        .unwrap();
        let node_graph = json!({
            "schemaVersion": "0",
            "nodes": [{ "id": "project:remote", "kind": "project", "name": "remote", "path": "/x" }],
            "edges": [],
        });
        aoide_storage::node_store::save_node_cache(&aoide_storage::node_store::NodeCacheEntry {
            schema_version: "0".into(),
            name: "yomi-strix".into(),
            instance: Some(json!({ "name": "yomi-strix", "url": "http://yomi-strix:8710/" })),
            graph: Some(node_graph.clone()),
            fetched_at: Some(aoide_storage::time::now_iso_utc()),
            stale: false,
            last_error: None,
        })
        .unwrap();

        let doc = build_graph(&[], &[], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let node = nodes
            .iter()
            .find(|n| n["id"] == "node:yomi-strix")
            .expect("mesh node folded in");
        assert_eq!(node["kind"], "node");
        assert_eq!(node["name"], "yomi-strix");
        assert_eq!(node["url"], "http://yomi-strix:8710/");
        assert_eq!(node["state"], "fresh");
        assert_eq!(node["children"]["nodes"], node_graph["nodes"].clone());
        // A fresh node contributes no TOP-LEVEL nodes/edges of its own — its
        // subtree is nested, never merged into this document's flat lists,
        // so a node's ids can never collide with a local session/project id.
        assert!(nodes.iter().all(|n| n["id"] != "project:remote"));
        assert!(doc["edges"].as_array().unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&dir);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }
    #[test]
    fn build_graph_shows_a_stale_or_never_pulled_node_with_no_children() {
        // A registered node is visible IMMEDIATELY on `node add`, before any
        // pull ever succeeds — and stays visible (never silently dropped)
        // once a pull goes stale. Either way: an explicit `state`, no
        // `children`, never a crash.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STATE_DIR").ok();
        let dir = std::env::temp_dir().join(format!("aoide-node-fold-stale-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::env::set_var("AOIDE_STATE_DIR", &dir);

        // Never pulled: no cache file at all.
        aoide_storage::node_store::save_nodes(&[aoide_storage::node_store::Node {
            name: "never-pulled".into(),
            url: "http://never:8710/".into(),
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: None,
            verified: false,
            allows: Vec::new(),
            via: None,
            added_at: "2026-08-14T00:00:00Z".into(),
        }])
        .unwrap();
        let doc = build_graph(&[], &[], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let node = nodes.iter().find(|n| n["id"] == "node:never-pulled").unwrap();
        assert_eq!(node["state"], "stale");
        assert!(node.get("children").is_none());

        // Explicitly stale (a failed pull) — still visible, still no children,
        // and carries the last error for `node status` to surface.
        aoide_storage::node_store::save_nodes(&[aoide_storage::node_store::Node {
            name: "flaky".into(),
            url: "http://flaky:8710/".into(),
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: None,
            verified: false,
            allows: Vec::new(),
            via: None,
            added_at: "2026-08-14T00:00:00Z".into(),
        }])
        .unwrap();
        aoide_storage::node_store::save_node_cache(&aoide_storage::node_store::NodeCacheEntry {
            schema_version: "0".into(),
            name: "flaky".into(),
            instance: None,
            graph: None,
            fetched_at: None,
            stale: true,
            last_error: Some("connection refused".into()),
        })
        .unwrap();
        let doc = build_graph(&[], &[], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let node = nodes.iter().find(|n| n["id"] == "node:flaky").unwrap();
        assert_eq!(node["state"], "stale");
        assert_eq!(node["error"], "connection refused");
        assert!(node.get("children").is_none());

        // An expired-TTL (but not explicitly marked stale) cache is ALSO
        // reported stale by the fold.
        aoide_storage::node_store::save_nodes(&[aoide_storage::node_store::Node {
            name: "expired".into(),
            url: "http://expired:8710/".into(),
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: None,
            verified: false,
            allows: Vec::new(),
            via: None,
            added_at: "2026-08-14T00:00:00Z".into(),
        }])
        .unwrap();
        let ancient = aoide_storage::time::iso_utc_from_epoch(
            aoide_storage::time::parse_iso_utc(&aoide_storage::time::now_iso_utc()).unwrap()
                - aoide_storage::node_store::NODE_CACHE_TTL_SECS as i64
                - 1,
        );
        aoide_storage::node_store::save_node_cache(&aoide_storage::node_store::NodeCacheEntry {
            schema_version: "0".into(),
            name: "expired".into(),
            instance: Some(json!({})),
            graph: Some(json!({ "nodes": [], "edges": [] })),
            fetched_at: Some(ancient),
            stale: false,
            last_error: None,
        })
        .unwrap();
        let doc = build_graph(&[], &[], &[]);
        let nodes = doc["nodes"].as_array().unwrap();
        let node = nodes.iter().find(|n| n["id"] == "node:expired").unwrap();
        assert_eq!(node["state"], "stale");
        assert!(node.get("children").is_none());

        let _ = std::fs::remove_dir_all(&dir);
        match saved {
            Some(v) => std::env::set_var("AOIDE_STATE_DIR", v),
            None => std::env::remove_var("AOIDE_STATE_DIR"),
        }
    }
    #[test]
    fn resolve_graph_document_matches_build_graph_off_the_current_stage() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let saved = std::env::var("AOIDE_STAGE_DIR").ok();
        let stage = unique_stage("resolve-graph-doc");
        std::env::set_var("AOIDE_STAGE_DIR", &stage);

        // Empty stage: still resolves cleanly to the empty v0 shape, matching
        // what `build_graph(&[], &[], &[])` would produce.
        let doc = resolve_graph_document().unwrap();
        assert_eq!(doc, build_graph(&[], &[], &[]));

        match saved {
            Some(v) => std::env::set_var("AOIDE_STAGE_DIR", v),
            None => std::env::remove_var("AOIDE_STAGE_DIR"),
        }
        let _ = std::fs::remove_dir_all(&stage);
    }
    #[test]
    fn render_shows_sudo_marker_when_blocked() {
        let projects = fixture_projects();
        let mut blocked = session("s1", "/home/k/Aoide", "awaiting", "1", None);
        blocked.needs_sudo = Some(true);
        let free = session("s2", "/home/k/Aoide", "idle", "2", None);
        let sessions = vec![blocked, free];
        let out = render(&projects, &sessions, &[], None);
        let host = aoide_storage::display::local_host_name();
        assert!(
            out.contains(&format!("● {host}/root/s1  claude  awaiting  /home/k/Aoide  [sudo]")),
            "sudo-blocked node carries the marker: {out}"
        );
        assert!(
            !out.contains(&format!("{host}/root/s2  claude  idle  /home/k/Aoide  [sudo]")),
            "non-blocked node carries no marker: {out}"
        );
    }
    #[test]
    fn render_session_head_uses_the_display_grammar_petnamed_and_legacy() {
        // The canonical display grammar (petnames plan P3): a petnamed
        // record's tree head is `<host>/<role>/<petname> (…<tail4>)`; a
        // legacy (petname-less) record degrades to `<host>/<role>/
        // <sessionId>` — the FULL id, never a truncated fake. Root vs child
        // role comes from the DAG shape the tree already computes, not a
        // stored field. Trailing glyph vocabulary (⟐/⧉/[sudo]) is untouched
        // by this rendering change.
        let mut root = session("sess-0000-8948", "/home/k/Aoide", "idle", "1", None);
        root.petname = Some("brave-otter".into());
        let mut child = session("sess-0000-1234", "/home/k/Aoide", "idle", "2", Some("sess-0000-8948"));
        child.petname = Some("calm-thorn".into());
        let legacy = session("sess-legacy-full-id", "/home/k/Aoide", "idle", "3", None);
        let sessions = vec![root, child, legacy];

        let out = render(&fixture_projects(), &sessions, &[], None);
        let host = aoide_storage::display::local_host_name();
        assert!(
            out.contains(&format!("● {host}/root/brave-otter (…8948)  claude  idle")),
            "petnamed root renders host/role/petname/tail: {out}"
        );
        assert!(
            out.contains(&format!("● {host}/child/calm-thorn (…1234)  claude  idle")),
            "petnamed child renders host/role/petname/tail: {out}"
        );
        assert!(
            out.contains(&format!("● {host}/root/sess-legacy-full-id  claude  idle")),
            "legacy record degrades to host/role/full-id, never a truncated fake: {out}"
        );
    }
    #[test]
    fn ledger_session_exit_projects_restore_verbatim_even_after_state_done() {
        // P-C5 (durable-sessions plan): `reap_inner` overwrites `state` to
        // "done" BEFORE calling this function — `restore.idle` must survive
        // that mutation untouched, since it is its OWN field, never read
        // back off `state`. This test pins the projection directly against
        // `ledger_session_exit`, independent of which roster-exit path
        // called it.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STATE_DIR"]);
        let state = unique_stage("ledger-restore");
        std::env::set_var("AOIDE_STATE_DIR", &state);

        let mut rec = session("term-1", "/home/khoa/Aoide", "idle", "2026-01-01T00:00:00Z", None);
        rec.restore = Some(aoide_storage::records::RestoreSnapshot {
            cwd: Some("/home/khoa/Aoide".into()),
            idle: true,
            argv: None,
            typed: Some("echo hi".into()),
        });
        rec.state = "done".to_string(); // the reap-path ordering hazard this field survives.

        ledger_session_exit(&rec, "2026-01-01T01:00:00Z");

        let lines = aoide_storage::ledger::read_ledger().unwrap();
        let mine: Vec<_> = lines.iter().filter(|l| l.session_id == "term-1").collect();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].restore, rec.restore);

        let _ = std::fs::remove_dir_all(&state);
    }
    #[test]
    fn ledger_session_exit_writes_explicit_null_when_no_restore() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STATE_DIR"]);
        let state = unique_stage("ledger-restore-null");
        std::env::set_var("AOIDE_STATE_DIR", &state);

        let rec = session("agent-1", "/w", "done", "2026-01-01T00:00:00Z", None);
        ledger_session_exit(&rec, "2026-01-01T01:00:00Z");

        let raw = std::fs::read_to_string(aoide_storage::ledger::session_ledger_path()).unwrap();
        assert!(raw.contains("\"restore\":null"), "line: {raw}");

        let _ = std::fs::remove_dir_all(&state);
    }

    // ── P-RSA S4: the cross-machine parent link, both sides ────────────────

    fn remote_parent(node: &str, key: &str, session_id: &str) -> aoide_storage::records::RemoteParent {
        aoide_storage::records::RemoteParent {
            node: node.to_string(),
            key: key.to_string(),
            session_id: session_id.to_string(),
            extra: Default::default(),
        }
    }

    fn ledger_row(parent: &str, node: &str, key: &str, child: &str) -> aoide_storage::remote_children::RemoteChild {
        aoide_storage::remote_children::RemoteChild {
            parent_session_id: parent.to_string(),
            node: node.to_string(),
            key: key.to_string(),
            session_id: child.to_string(),
            spawned_at: "2026-09-25T00:00:00Z".to_string(),
            lines_after: 0,
            drained: false,
            extra: Default::default(),
        }
    }

    fn node_json_of<'a>(doc: &'a Value, id: &str) -> &'a Value {
        doc["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap_or_else(|| panic!("no node {id} in {doc}"))
    }

    #[test]
    fn a_remote_parent_projects_and_never_becomes_a_local_spawned_edge() {
        // The child side: `remoteParent` rides the node as `{node, sessionId}`
        // (no key — the document is display data) and the record's
        // `parentSessionId` is untouched, so no `spawned` edge is minted and
        // the session stays anchored/root locally.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STATE_DIR"]);
        let state = unique_stage("remote-parent");
        std::env::set_var("AOIDE_STATE_DIR", &state);

        let mut child = session("a2a-1", "/remote/cwd", "running", "1", None);
        child.remote_parent = Some(remote_parent("yomi-strix", "ab".repeat(32).as_str(), "par1"));

        let doc = build_graph(&[], &[child], &[]);
        let node = node_json_of(&doc, "session:a2a-1");
        assert_eq!(node["remoteParent"]["node"], "yomi-strix");
        assert_eq!(node["remoteParent"]["sessionId"], "par1");
        assert!(
            node["remoteParent"].get("key").is_none(),
            "the key is identity, never republished as display data: {node}"
        );
        assert!(
            node.get("remoteChildren").is_none(),
            "a leaf child carries no remoteChildren: {node}"
        );
        assert!(
            doc["edges"].as_array().unwrap().is_empty(),
            "no local `spawned` edge for a parent that is not on this node, and no \
             registered project to anchor to: {doc}"
        );

        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn a_local_id_equal_to_the_remote_session_id_is_not_a_local_parent() {
        // The collision pin. `par1` happens to exist LOCALLY with the very id
        // the far caller signed as its own session id. The projections must
        // never join them: `parentSessionId` is what makes a local parent
        // (autogate grant, sibling rule, project inheritance all read it), and
        // the remote field never writes it.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STATE_DIR"]);
        let state = unique_stage("remote-collision");
        std::env::set_var("AOIDE_STATE_DIR", &state);

        let par1 = session("par1", "/here", "running", "1", None);
        let mut child = session("a2a-1", "/remote/cwd", "running", "2", None);
        child.remote_parent = Some(remote_parent("yomi-strix", "ab".repeat(32).as_str(), "par1"));

        let doc = build_graph(&[], &[par1, child], &[]);
        assert!(
            doc["edges"].as_array().unwrap().is_empty(),
            "the same-named local session must not gain a spawned edge: {doc}"
        );
        assert!(
            node_json_of(&doc, "session:par1").get("remoteChildren").is_none(),
            "and must not gain remoteChildren either — the ledger is the only source of those: {doc}"
        );
        assert_eq!(node_json_of(&doc, "session:a2a-1")["remoteParent"]["sessionId"], "par1");

        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn the_remote_parent_name_follows_a_rename_through_the_key() {
        // Rename safety: `node` is the label stamped at spawn time; `key` is
        // the identity. A reader resolves the CURRENT `nodes.json` name for the
        // key, so renaming the node record re-labels every link instead of
        // orphaning it — and a key no registered node claims falls back to the
        // stored label rather than rendering blank.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STATE_DIR"]);
        let state = unique_stage("remote-rename");
        std::env::set_var("AOIDE_STATE_DIR", &state);

        let key = "ab".repeat(32);
        aoide_storage::node_store::save_nodes(&[aoide_storage::node_store::Node {
            name: "yomi-renamed".into(),
            url: "http://yomi-strix:8710/".into(),
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: Some(key.clone()),
            verified: true,
            allows: Vec::new(),
            via: None,
            added_at: "2026-08-14T00:00:00Z".into(),
        }])
        .unwrap();

        let mut child = session("a2a-1", "/x", "running", "1", None);
        child.remote_parent = Some(remote_parent("yomi-strix", &key, "par1"));
        let mut orphan = session("a2a-2", "/x", "running", "2", None);
        orphan.remote_parent = Some(remote_parent("sakaki", "cd".repeat(32).as_str(), "par2"));

        let doc = build_graph(&[], &[child, orphan], &[]);
        assert_eq!(
            node_json_of(&doc, "session:a2a-1")["remoteParent"]["node"],
            "yomi-renamed",
            "the key resolves to the CURRENT name: {doc}"
        );
        assert_eq!(
            node_json_of(&doc, "session:a2a-2")["remoteParent"]["node"],
            "sakaki",
            "an unregistered key falls back to the stamped label, never blank: {doc}"
        );

        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn the_parent_node_carries_its_ledger_children_and_only_its_own() {
        // The parent side: `remoteChildren` comes off the caller-side ledger,
        // matched on the parent's own session id, and rides only when
        // non-empty (an ordinary record stays byte-for-byte as before).
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STATE_DIR"]);
        let state = unique_stage("remote-children");
        std::env::set_var("AOIDE_STATE_DIR", &state);

        let key = "ab".repeat(32);
        aoide_storage::node_store::save_nodes(&[aoide_storage::node_store::Node {
            name: "nodeb".into(),
            url: "http://nodeb:8710/".into(),
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: Some(key.clone()),
            verified: true,
            allows: Vec::new(),
            via: None,
            added_at: "2026-08-14T00:00:00Z".into(),
        }])
        .unwrap();
        aoide_storage::remote_children::append_remote_child(&ledger_row("par1", "nodeb", &key, "C")).unwrap();
        aoide_storage::remote_children::append_remote_child(&ledger_row("par1", "nodeb", &key, "C2")).unwrap();
        aoide_storage::remote_children::append_remote_child(&ledger_row("someone-else", "nodeb", &key, "X")).unwrap();

        let doc = build_graph(&[], &[session("par1", "/x", "running", "1", None)], &[]);
        let kids = node_json_of(&doc, "session:par1")["remoteChildren"].as_array().unwrap();
        assert_eq!(kids.len(), 2, "only this parent's rows: {doc}");
        assert_eq!(kids[0]["node"], "nodeb");
        assert_eq!(kids[0]["sessionId"], "C");
        assert_eq!(kids[1]["sessionId"], "C2");
        assert!(kids[0].get("key").is_none(), "no key on a display projection: {doc}");

        // A second pass is byte-identical (the projection is derived, never a
        // store), and a session with no rows carries no key at all.
        let again = build_graph(&[], &[session("par1", "/x", "running", "1", None)], &[]);
        assert_eq!(again, doc);
        let alone = build_graph(&[], &[session("par2", "/x", "running", "2", None)], &[]);
        assert!(node_json_of(&alone, "session:par2").get("remoteChildren").is_none());

        let _ = std::fs::remove_dir_all(&state);
    }

    #[test]
    fn dropping_a_parents_ledger_rows_leaves_a_surviving_parents_alone() {
        // A ledger row is the parent-side half of a link; once the parent has
        // left the roster nothing can resolve it, so the row goes with it — and
        // a row belonging to a SURVIVING parent stays. This is the helper both
        // roster-exit paths call after their own `sessions.json` write; each
        // path's own end-to-end half lives beside it (`manage.rs`'s
        // `session prune` test, `reap.rs`'s superseded-tombstone test).
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STATE_DIR"]);
        let state = unique_stage("remote-prune");
        std::env::set_var("AOIDE_STATE_DIR", &state);

        let key = "ab".repeat(32);
        aoide_storage::remote_children::append_remote_child(&ledger_row("done-par", "nodeb", &key, "C")).unwrap();
        aoide_storage::remote_children::append_remote_child(&ledger_row("live-par", "nodeb", &key, "D")).unwrap();

        let sessions = vec![
            session("done-par", "/x", "done", "1", None),
            session("live-par", "/x", "running", "2", None),
        ];
        let (kept, _hooks, removed, _cleared) = prune_done_scoped(sessions, Vec::new(), true);
        assert_eq!(removed, vec!["done-par".to_string()]);
        assert_eq!(kept.len(), 1);

        // Nothing has dropped the rows yet: the prune alone is not the ledger's
        // owner, only the caller that lands the roster is.
        assert_eq!(aoide_storage::remote_children::load_remote_children().len(), 2);

        drop_remote_child_rows(&removed);
        let left = aoide_storage::remote_children::load_remote_children();
        assert_eq!(left.len(), 1, "one row left: {left:?}");
        assert_eq!(left[0].parent_session_id, "live-par");

        // An empty drop set writes nothing at all.
        drop_remote_child_rows(&[]);
        assert_eq!(aoide_storage::remote_children::load_remote_children().len(), 1);

        let _ = std::fs::remove_dir_all(&state);
    }

    /// P-RSA S10 review, H1: a DOOR SUMMON's finished record is not this box's
    /// history. A task-carrying `done` record the door created (origin
    /// `node:*` — the shape `session_conduct` refuses from any environment, so
    /// the door is its only writer) is swept once its report is filed, exactly
    /// like an untasked session; a task run THIS BOX ran is still retained by
    /// the automatic sweep. A run whose report is still OWED is kept either
    /// way, because the cursor entry is the report lane's own trigger.
    ///
    /// Driven through the real prune (`prune_done_scoped`, the one function
    /// `reap_inner` and `session prune` both call), with the cursor file
    /// written exactly as the lane writes it: one entry per FILED run.
    #[test]
    fn a_finished_door_summon_is_pruned_once_reported_while_a_local_task_run_is_kept() {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STATE_DIR", "AOIDE_STAGE_DIR", "XDG_RUNTIME_DIR"]);
        let root = unique_stage("door-retention");
        let stage = root.join("stage");
        std::fs::create_dir_all(&stage).unwrap();
        std::env::set_var("AOIDE_STAGE_DIR", &stage);
        std::env::set_var("AOIDE_STATE_DIR", root.join("state"));
        std::env::set_var("XDG_RUNTIME_DIR", root.join("run"));

        let tasked = |id: &str, slug: &str, origin: Option<&str>| {
            let mut rec = session(id, "/x", "done", "2026-09-25T00:00:00Z", None);
            rec.task = Some(slug.to_string());
            rec.origin = origin.map(str::to_string);
            rec
        };
        std::fs::write(
            crate::graph::taskreport::taskreport_path(),
            serde_json::json!({
                "door-filed": { "outcome": "exit" },
                "local-filed": { "outcome": "exit" },
            })
            .to_string(),
        )
        .unwrap();

        let sessions = vec![
            tasked("door-filed", "t-1", Some("node:peer")),
            tasked("door-owed", "t-2", Some("node:peer")),
            tasked("local-filed", "build", None),
            tasked("local-owed", "build-2", None),
        ];
        let (kept, _hooks, removed, _cleared) = prune_done_scoped(sessions, Vec::new(), false);
        let ids: Vec<&str> = kept.iter().map(|s| s.session_id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["door-owed", "local-filed", "local-owed"],
            "a REPORTED door summon leaves the roster like any untasked session; every run on \
             this box stays, reported or not, and an unreported door run stays until the lane \
             can fire"
        );
        assert_eq!(removed, vec!["door-filed".to_string()]);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn the_remote_childs_local_descendants_nest_under_it_from_a_pulled_graph() {
        // The §10 gate: on A, `par1 → nodeb/C → nodeb/G`, where C and G are
        // both records on B (C spawned by par1 over the door, G spawned
        // locally by C). Nothing here invents a wire: the fixture IS B's own
        // resolved document — the exact bytes `aoide/graphSummary` serves and
        // `node pull` caches — so the far subtree nests through the fold A
        // already does, and `remoteChildren` is only the link into it.
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvVars::save(&["AOIDE_STATE_DIR"]);
        let state = unique_stage("remote-nest");
        std::env::set_var("AOIDE_STATE_DIR", &state);

        // ── B's own records → B's own document (what the door stamped) ──
        let key_a = "ab".repeat(32);
        let mut c = session("C", "/srv/work", "running", "1", None);
        c.remote_parent = Some(remote_parent("yomi", &key_a, "par1"));
        let g = session("G", "/srv/work", "running", "2", Some("C"));
        let b_doc = build_graph(&[], &[c, g], &[]);
        assert_eq!(node_json_of(&b_doc, "session:C")["remoteParent"]["sessionId"], "par1");
        assert!(
            b_doc["edges"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["from"] == "session:C" && e["to"] == "session:G" && e["kind"] == "spawned"),
            "B's own local lineage: {b_doc}"
        );

        // ── A's registry: the far node, freshly pulled ──
        let key_b = "cd".repeat(32);
        aoide_storage::node_store::save_nodes(&[aoide_storage::node_store::Node {
            name: "nodeb".into(),
            url: "http://nodeb:8710/".into(),
            autogate: false,
            token_file: None,
            bearer_secret: None,
            hub: false,
            pubkey: Some(key_b.clone()),
            verified: true,
            allows: Vec::new(),
            via: None,
            added_at: "2026-08-14T00:00:00Z".into(),
        }])
        .unwrap();
        aoide_storage::node_store::save_node_cache(&aoide_storage::node_store::NodeCacheEntry {
            schema_version: "0".into(),
            name: "nodeb".into(),
            instance: Some(json!({ "name": "nodeb" })),
            graph: Some(b_doc),
            fetched_at: Some(aoide_storage::time::now_iso_utc()),
            stale: false,
            last_error: None,
        })
        .unwrap();
        aoide_storage::remote_children::append_remote_child(&ledger_row("par1", "nodeb", &key_b, "C")).unwrap();

        // ── A's own document: par1 + the folded far subtree ──
        let doc = build_graph(&[], &[session("par1", "/home/khoa", "running", "1", None)], &[]);
        let link = &node_json_of(&doc, "session:par1")["remoteChildren"][0];
        assert_eq!(link["node"], "nodeb");
        assert_eq!(link["sessionId"], "C");

        let far = node_json_of(&doc, "node:nodeb");
        assert_eq!(far["state"], "fresh");
        let far_sessions: Vec<&str> = far["children"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n["kind"] == "session")
            .map(|n| n["id"].as_str().unwrap())
            .collect();
        assert_eq!(far_sessions, vec!["session:C", "session:G"], "C AND its own descendant: {far}");
        assert_eq!(
            far["children"]["nodes"][1]["remoteParent"], Value::Null,
            "G is B's OWN local child — a plain spawned edge, no remote link"
        );
        assert_eq!(
            far["children"]["nodes"][0]["remoteParent"]["node"], "yomi",
            "C names the parent it was spawned by, on A"
        );
        let far_edges = far["children"]["edges"].as_array().unwrap();
        assert!(
            far_edges.iter().any(|e| e["from"] == "session:C" && e["to"] == "session:G"),
            "so the walk par1 → nodeb/C → nodeb/G resolves on A: {far}"
        );

        let _ = std::fs::remove_dir_all(&state);
    }
}
