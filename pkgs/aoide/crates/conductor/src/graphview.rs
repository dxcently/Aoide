//! The Graph panel — a RETAINED scene of the project/session graph.
//!
//! Where the SESSION roster reads the graph as an indented list (state at a
//! glance), this view draws its *shape*: fixed-size cards standing at their own
//! world coordinates, wired with box-drawing edges, under a camera that pans
//! and zooms over them.
//!
//! Two rules hold here.
//!
//! **The structure is never re-derived.** Nodes and edges come verbatim from
//! [`aoide_conduct::graph::build_graph`] — the same pure function every
//! mutation's `restage_graph()` (and `graph prune`'s manual resync) writes to
//! `state/stage/graph.json` — so the picture on screen is the document on
//! disk. Each session has at most one incoming edge (spawned-by wins over
//! anchors), so the document parses into a forest and the fresh layout is a
//! tree walk: `rank = depth`, parents centred horizontally over their
//! descendant leaves.
//!
//! **The positions are retained, not recomputed.** That tree walk only
//! proposes; [`crate::scene::Positions`] decides. A card already on the canvas
//! keeps its world coordinates when unrelated sessions arrive or end, so the
//! forest stops reshuffling under the operator's cursor between refreshes.
//! Cards are a fixed size in world cells; the camera scales the whole layout
//! rather than switching card presets, and terminal glyphs stay cell-sized and
//! clip inside their card.
//!
//! Drawing is bounded by the viewport, never by the world: edges paint first,
//! cards on top, every write clipped through [`crate::scene::Painter`], so a
//! card the camera cannot see costs a comparison instead of a cell.
//!
//! Tags: read-only. The schema has no tag surface (see the module note in
//! [`crate::theme::session_tags`]); tags found on a session record's
//! round-tripped `extra.tags` are rendered as accent chips, never minted here.

use crate::app::App;
use crate::scene::{Camera, Painter, Placed, View, WorldRect};
use crate::theme;
use aoide_conduct::graph;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Padding, Paragraph, Widget};
use ratatui::Frame;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

/// Vertical gap between depth ranks — just enough room for a wire's stem,
/// spreader and drop, never a wide gutter, since rank stacks eat screen
/// height fastest.
const RANK_GAP: i32 = 2;
/// Fixed card size in world cells; title, identity, and state each have their
/// own line. Cards never change size — the camera does.
const CARD_W: i32 = 24;
const CARD_H: i32 = 5;
/// Horizontal pitch of one leaf slot in the fresh layout — card width plus a
/// readable gap between siblings.
const SLOT: i32 = CARD_W + 6;
/// Empty canvas kept around the forest on every side, so the camera can pan
/// and zoom PAST the outermost cards instead of clamping to their edges.
const CANVAS_PAD: (i32, i32) = (40, 16);

/// What a node is — drives marker, colour, and whether Enter can cue it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Project,
    Session,
    /// The synthetic root gathering sessions anchored to no project (mirrors the
    /// projectless group the Unicode tree render uses).
    Unanchored,
    /// A registered node on the mesh, as the roster probe last saw it: the
    /// root its remote sessions hang from.
    Host,
}

/// One laid-out node: identity, display bits, and its retained world rectangle.
#[derive(Debug, Clone)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    pub label: String,
    pub title: String,
    pub activity: String,
    pub petname: Option<String>,
    pub role: String,
    pub harness: String,
    /// The bare session id (for the focus jump on Enter); `None` for anchors.
    pub session_id: Option<String>,
    pub state: Option<String>,
    pub tags: Vec<String>,
    /// The session's Claude model (agent or subagent's own), when known —
    /// straight off `graph.json`'s node `model` field. `None` for projects,
    /// the synthetic unanchored root, shells, and any session that hasn't
    /// produced an assistant turn yet.
    pub model: Option<String>,
    /// The registered node a remote card lives on; `None` for this box's own
    /// cards. A remote session id never resolves against the local roster,
    /// so no local action (focus, letter, prune) can reach one by mistake.
    pub host: Option<String>,
    /// The far session's own id on a remote card — the handle the Mesh row
    /// is found by. Never `session_id`, which local actions resolve.
    pub remote_id: Option<String>,
    /// The card's facts are a cache, not a live reply: a `last-seen` row, or
    /// any remote card while the last probe failed. Drawn dimmed.
    pub cached: bool,
    /// What a folded card hides: the descendant count and how many of them
    /// are awaiting a human, so a fold never hides a blocked agent silently.
    pub folded: Option<Fold>,
    pub depth: usize,
    pub row: usize,
    /// The card's retained rectangle in world cells.
    pub world: WorldRect,
}

/// The summary a folded card wears on its bottom border.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fold {
    pub hidden: usize,
    pub awaiting: usize,
    pub working: usize,
}

impl Fold {
    /// `▸ 5 · 1 awaiting` — the hidden count, then the most urgent class
    /// present (awaiting over working) so the mark says what matters in
    /// the twenty cells a border holds.
    pub fn label(&self) -> String {
        let mut s = format!("▸ {}", self.hidden);
        if self.awaiting > 0 {
            s.push_str(&format!(" · {} awaiting", self.awaiting));
        } else if self.working > 0 {
            s.push_str(&format!(" · {} working", self.working));
        }
        s
    }

    /// The label at `budget` cells: the full form when it fits, else the
    /// terse `▸5·1!` that keeps the awaiting count ahead of everything —
    /// the one number a fold must never truncate away — and only then an
    /// end-cut.
    pub fn label_fitting(&self, budget: usize) -> String {
        let full = self.label();
        if full.chars().count() <= budget {
            return full;
        }
        let terse = if self.awaiting > 0 {
            format!("▸{}·{}!", self.hidden, self.awaiting)
        } else {
            format!("▸{}", self.hidden)
        };
        if terse.chars().count() <= budget {
            return terse;
        }
        if self.awaiting > 0 {
            let bare = format!("{}!", self.awaiting);
            if bare.chars().count() <= budget {
                return bare;
            }
        }
        truncate_end(&terse, budget)
    }
}

/// Node metadata carried from the parsed document into the DFS.
struct Meta {
    kind: NodeKind,
    label: String,
    title: String,
    role: String,
    harness: String,
    session_id: Option<String>,
    state: Option<String>,
    tags: Vec<String>,
    model: Option<String>,
    host: Option<String>,
    remote_id: Option<String>,
    petname: Option<String>,
    activity: String,
    cached: bool,
}

/// The parsed, world-placed forest and the camera over it.
pub struct Model {
    pub nodes: Vec<Node>,
    /// node id → child node ids, in draw order.
    children: HashMap<String, Vec<String>>,
    /// Indices into `nodes`, in preorder — the slice the current view draws.
    visible: Vec<usize>,
    /// Index into `visible` of the selected card.
    selected: usize,
    camera: Camera,
}

impl Model {
    /// The nodes the current view draws, in preorder.
    pub fn visible(&self) -> impl Iterator<Item = &Node> {
        self.visible.iter().map(|&i| &self.nodes[i])
    }
    pub fn visible_len(&self) -> usize {
        self.visible.len()
    }
    pub fn selected(&self) -> usize {
        self.selected
    }
}

/// The visible nodes in preorder — the single source of truth for both
/// selection (`j`/`k` walk this) and the drawn layout, so the cursor can never
/// land on a node the screen isn't showing.
pub fn node_order(app: &App) -> Vec<Node> {
    let model = build_model(app);
    model.visible().cloned().collect()
}

/// Where the selected card sits in [`node_order`].
pub fn selected_index(app: &App) -> usize {
    build_model(app).selected
}

/// Move the selection onto the `i`th visible card and release the camera back
/// to following it.
pub fn select_index(app: &mut App, i: usize) {
    let model = build_model(app);
    let picked = model.visible().nth(i).map(|n| n.id.clone());
    if let Some(id) = picked {
        pick(app, &model, id);
    }
}

/// Make `id` the selection: the camera follows it again, and every fold
/// above it opens, so the cursor can never sit on a card the view hides.
fn pick(app: &mut App, model: &Model, id: String) {
    let mut cur = id.clone();
    let mut hops = 0;
    while let Some((parent, _)) = model
        .children
        .iter()
        .find(|(p, kids)| kids.contains(&cur) && model.nodes.iter().any(|n| &n.id == *p))
    {
        app.graph.folded.remove(parent);
        cur = parent.clone();
        hops += 1;
        if hops > model.nodes.len() {
            break;
        }
    }
    app.graph.selected = id;
    app.graph.camera.pan = None;
}

/// Move the selection to the sibling before (`forward = false`) or after
/// (`forward = true`) it across the rank: a node sharing this one's parent,
/// in the existing child order, or for a root the previous or next root.
/// Never wraps at a rank's end, and does nothing for an only child.
///
/// The step is resolved against the WHOLE forest, not the drawn slice: under
/// Focus the view is derived from the selection, so landing on a sibling the
/// current component did not draw simply re-forms the view around it. The
/// cursor is never stranded on a card whose neighbours the view hid.
pub fn select_sibling(app: &mut App, forward: bool) {
    let model = build_model(app);
    let Some(id) = model.visible().nth(model.selected).map(|n| n.id.clone()) else {
        return;
    };
    // A root's siblings are the other roots, in forest order — the way from
    // this box's forest across to the host cards and back.
    let roots: Vec<String> = model
        .nodes
        .iter()
        .filter(|n| n.depth == 0)
        .map(|n| n.id.clone())
        .collect();
    // The first DRAWN parent whose children hold this id: a resurrected
    // session also sits under its `resumed` source, a ghost no longer in the
    // roster, and map order must not let that ghost win and strand the key.
    let siblings = model
        .children
        .iter()
        .find(|(p, kids)| kids.contains(&id) && model.nodes.iter().any(|n| &n.id == *p))
        .map(|(_, kids)| kids)
        .unwrap_or(&roots);
    let Some(pos) = siblings.iter().position(|s| s == &id) else {
        return;
    };
    let target = if forward {
        siblings.get(pos + 1)
    } else {
        pos.checked_sub(1).and_then(|p| siblings.get(p))
    };
    if let Some(target_id) = target.filter(|t| model.nodes.iter().any(|n| &n.id == *t)) {
        pick(app, &model, target_id.clone());
    }
}

/// Move the selection up one rank, onto the selected card's parent in the
/// forest — the gathering root included, so a Focus view on a loose session
/// climbs back out to the forest it was picked from.
pub fn select_parent(app: &mut App) {
    let model = build_model(app);
    let Some(id) = model.visible().nth(model.selected).map(|n| n.id.clone()) else {
        return;
    };
    // The first DRAWN parent (see `select_sibling` on the `resumed` ghost).
    let parent = model
        .children
        .iter()
        .find(|(p, kids)| kids.contains(&id) && model.nodes.iter().any(|n| &n.id == *p))
        .map(|(p, _)| p.clone());
    if let Some(parent) = parent {
        pick(app, &model, parent);
    }
}

/// Fold or unfold the selected card's children. A leaf folds nothing.
pub fn toggle_fold(app: &mut App) {
    let model = build_model(app);
    let Some(id) = model.visible().nth(model.selected).map(|n| n.id.clone()) else {
        return;
    };
    let has_children = model
        .children
        .get(&id)
        .is_some_and(|kids| kids.iter().any(|k| model.nodes.iter().any(|n| &n.id == k)));
    if !has_children {
        return;
    }
    if !app.graph.folded.remove(&id) {
        app.graph.folded.insert(id);
    }
}

/// Move the selection down one rank, onto the selected card's first child in
/// draw order. Does nothing on a leaf; a folded card unfolds first, since a
/// step toward what is hidden is a request to see it.
pub fn select_child(app: &mut App) {
    let model = build_model(app);
    let Some(id) = model.visible().nth(model.selected).map(|n| n.id.clone()) else {
        return;
    };
    app.graph.folded.remove(&id);
    let child = model
        .children
        .get(&id)
        .into_iter()
        .flatten()
        .find(|k| model.nodes.iter().any(|n| &n.id == *k))
        .cloned();
    if let Some(child) = child {
        pick(app, &model, child);
    }
}

pub fn selected_node(app: &App) -> Option<Node> {
    let model = build_model(app);
    let node = model.visible().nth(model.selected).cloned();
    node
}

pub fn selected_session_id(app: &App) -> Option<String> {
    selected_node(app).and_then(|node| node.session_id)
}

pub fn session_id_at(area: Rect, app: &App, x: u16, y: u16) -> Option<String> {
    let index = hit_node(area, app, x, y)?;
    node_order(app)
        .get(index)
        .and_then(|node| node.session_id.clone())
}

/// Swap between the focused component and the whole forest.
pub fn toggle_view(app: &mut App) {
    app.graph.view = app.graph.view.toggled();
    app.graph.camera.pan = None;
}

pub fn zoom_label(app: &App) -> &'static str {
    app.graph.camera.zoom_label()
}

pub fn view_label(app: &App) -> &'static str {
    app.graph.view.label()
}

/// Where the cursor stands in what the view draws — `card 3/20 · rank 1` —
/// so an operator knows how much forest the pane is not showing. One model
/// build for both numbers.
pub fn readout(app: &App) -> String {
    let model = build_model(app);
    let depth = model.visible().nth(model.selected).map(|n| n.depth);
    match depth {
        Some(depth) => format!("card {}/{} · rank {depth}", model.selected + 1, model.visible_len()),
        None => "no cards".to_string(),
    }
}

/// Everything a built model depends on besides the forest's inputs: the
/// scene's view, selection, folds and camera, the retained world's
/// generation, and the failed-probe age the host cards print. The inputs
/// themselves — stage, roster — change only through `sync_graph_scene`,
/// which clears the cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelKey {
    view: View,
    selected: String,
    folded: std::collections::BTreeSet<String>,
    camera: Camera,
    generation: u64,
    probe_failed: Option<String>,
    /// The wall clock to the minute: the ages a host card prints (`last seen
    /// 14h07m ago`) are minute-grained, so the build goes stale once a
    /// minute at most and a frame never pays for a clock tick otherwise.
    minute: u64,
}

/// Build the layout model from the canonical graph document and the roster's
/// last word on every registered node — once per distinct key: a frame's
/// render, hit test, extent and origin all read the same build rather than
/// re-reading `nodes.json` and every node cache three to five times per
/// keypress.
pub fn build_model(app: &App) -> Rc<Model> {
    let key = ModelKey {
        view: app.graph.view,
        selected: app.graph.selected.clone(),
        folded: app.graph.folded.clone(),
        camera: app.graph.camera,
        generation: app.graph.positions.generation(),
        probe_failed: app.roster_probe_failed(),
        minute: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() / 60)
            .unwrap_or(0),
    };
    if let Some((k, m)) = app.graph_cache.borrow().as_ref() {
        if *k == key {
            return m.clone();
        }
    }
    let model = Rc::new(build_model_on(app, &aoide_storage::display::local_host_name()));
    *app.graph_cache.borrow_mut() = Some((key, model.clone()));
    model
}

/// [`build_model`] with the host every session label carries stated rather
/// than read from this box.
fn build_model_on(app: &App, host: &str) -> Model {
    let doc = graph::build_graph(&app.projects, &app.sessions, &app.hooks);
    build_model_from(app, &doc, &app.roster_nodes(), host)
}

/// The model over an explicit document and roster — what [`build_model`]
/// reads off the app, split out so a test can hand in a synthetic pair.
///
/// Registered nodes come from the ROSTER, not the document's own `node:*`
/// fold: the fold carries a node's sessions only while its cache is within
/// the five-minute TTL and only `node pull` writes that cache, while the
/// roster is the live probe this frontend already runs and the same rows
/// Mesh paints. A node the probe could not reach keeps its cached sessions,
/// each wearing the cache's word (`last-seen`/`unknown`) in place of a live
/// state, so a remote card never claims more than the roster did.
pub fn build_model_from(
    app: &App,
    doc: &serde_json::Value,
    roster: &[crate::app::RosterNode],
    host: &str,
) -> Model {
    let merged = app.merged();
    let tag_by_id: HashMap<String, Vec<String>> = merged
        .iter()
        .map(|s| (s.session_id.clone(), theme::session_tags(s)))
        .collect();

    let empty = Vec::new();
    let nodes_j = doc["nodes"].as_array().unwrap_or(&empty);
    let edges_j = doc["edges"].as_array().unwrap_or(&empty);

    // Which session node ids are the `to` end of a "spawned" edge — exactly
    // `resolved_parent(...).is_some()` in `conduct`'s own terms (`build_graph`
    // emits "spawned" only for a session with a resolved parent, "anchors"
    // only for a parentless one) — so this is the display grammar's `role`
    // (petnames plan P3) read straight off the edge vocabulary already on the
    // document, no second parent walk needed.
    let spawned_targets: HashSet<&str> = edges_j
        .iter()
        .filter(|e| e.get("kind").and_then(|v| v.as_str()) == Some("spawned"))
        .filter_map(|e| e.get("to").and_then(|v| v.as_str()))
        .collect();
    // Metadata per node id, and the project ids in doc order (sorted by name).
    let mut meta: HashMap<String, Meta> = HashMap::new();
    let mut project_ids: Vec<String> = Vec::new();
    for n in nodes_j {
        let id = n
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if id.is_empty() {
            continue;
        }
        match n.get("kind").and_then(|v| v.as_str()) {
            Some("project") => {
                project_ids.push(id.clone());
                let name = str_field(n, "name");
                meta.insert(
                    id,
                    Meta {
                        kind: NodeKind::Project,
                        label: name,
                        title: str_field(n, "path"),
                        role: "project".into(),
                        harness: String::new(),
                        session_id: None,
                        state: None,
                        tags: Vec::new(),
                        model: None,
                        host: None,
                        remote_id: None,
                        petname: None,
                        activity: String::new(),
                        cached: false,
                    },
                );
            }
            Some("session") => {
                let sid = id.strip_prefix("session:").unwrap_or(&id).to_string();
                let state = str_field(n, "state");
                let tags = tag_by_id.get(&sid).cloned().unwrap_or_default();
                // `model` rides on the node only when the record has one (agent
                // or subagent alike) — absent for shells, so no filter on `role`
                // is needed here.
                let model = n.get("model").and_then(|v| v.as_str()).map(str::to_string);
                // The display grammar (petnames plan P3): `<host>/<role>/
                // <petname> (…<tail4>)`, degrading to `<host>/<role>/
                // <sessionId>` for a legacy/petname-less node. `session_id`
                // (below) stays the bare canonical id — this is the LABEL
                // only, never what Enter's focus jump reads.
                let role = if spawned_targets.contains(id.as_str()) {
                    "child"
                } else {
                    "root"
                };
                let petname = n
                    .get("petname")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let rec = aoide_storage::records::SessionRecord {
                    session_id: sid.clone(),
                    petname,
                    ..Default::default()
                };
                let label = aoide_storage::display::session_label(&rec, host, role);
                meta.insert(
                    id.clone(),
                    Meta {
                        kind: NodeKind::Session,
                        label,
                        title: str_field(n, "title"),
                        role: str_field(n, "role"),
                        harness: str_field(n, "agent"),
                        session_id: Some(sid),
                        state: Some(state),
                        tags,
                        model,
                        host: None,
                        remote_id: None,
                        petname: None,
                        activity: String::new(),
                        cached: false,
                    },
                );
            }
            _ => {}
        }
    }

    // Adjacency + the set of nodes that are somebody's child.
    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    let mut incoming: HashSet<String> = HashSet::new();
    for e in edges_j {
        let from = e.get("from").and_then(|v| v.as_str()).unwrap_or("");
        let to = e.get("to").and_then(|v| v.as_str()).unwrap_or("");
        if from.is_empty() || to.is_empty() {
            continue;
        }
        children
            .entry(from.to_string())
            .or_default()
            .push(to.to_string());
        incoming.insert(to.to_string());
    }

    // Roots: projects first, then a synthetic projectless root gathering
    // session nodes with no incoming edge.
    let mut roots: Vec<String> = project_ids;
    let mut unanchored: Vec<String> = Vec::new();
    for n in nodes_j {
        let id = n.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let is_session = n.get("kind").and_then(|v| v.as_str()) == Some("session");
        if is_session && !incoming.contains(id) {
            unanchored.push(id.to_string());
        }
    }
    if !unanchored.is_empty() {
        let uid = UNANCHORED_ID.to_string();
        meta.insert(
            uid.clone(),
            Meta {
                kind: NodeKind::Unanchored,
                label: crate::app::UNANCHORED.to_string(),
                title: "Sessions without a project".into(),
                role: "group".into(),
                harness: String::new(),
                session_id: None,
                state: None,
                tags: Vec::new(),
                model: None,
                host: None,
                remote_id: None,
                petname: None,
                activity: String::new(),
                cached: false,
            },
        );
        children.insert(uid.clone(), unanchored);
        roots.push(uid);
    }

    // Registered nodes stand after this box's own forest, one root each,
    // their sessions beneath — ranked under a same-node spawner when the
    // row names one, flat under the node otherwise. Ids are prefixed with
    // the node's own root id, so a far session id can never collide with a
    // local one.
    let probe_failed = app.roster_probe_failed();
    for node in roster.iter().filter(|n| !n.is_local) {
        let nid = format!("node:{}", node.name);
        // The border carries the presence word; the rows carry what is
        // short enough for a 20-cell card: the cache's age, the session
        // count, and — on its own row — a probe that has since failed.
        let title = match node.presence.as_str() {
            "online" => format!("{} session(s)", node.sessions.len()),
            "unreachable" => format!(
                "last seen {}",
                node.fetched_at.as_deref().map(theme::age_label).unwrap_or_else(|| "unknown".into())
            ),
            _ => String::new(),
        };
        meta.insert(
            nid.clone(),
            Meta {
                kind: NodeKind::Host,
                label: node.name.clone(),
                title,
                role: "node".into(),
                harness: probe_failed
                    .as_ref()
                    .map(|age| format!("probe failed {age}"))
                    .unwrap_or_default(),
                session_id: None,
                state: Some(node.presence.clone()),
                tags: Vec::new(),
                model: None,
                host: None,
                remote_id: None,
                petname: None,
                activity: String::new(),
                cached: probe_failed.is_some(),
            },
        );
        // Parentage the far node published is taken only where it leads back
        // to the node: a row whose chain never reaches the root — a far
        // `graph link` loop, `A↔B` — falls flat under the node with its
        // children still beneath it, so every row the node counts is a card
        // somebody can see, and a blocked agent in a loop still wears its
        // mark.
        let on_node = |id: &str| node.sessions.iter().any(|s| s.session_id == id);
        let far = |id: &str| format!("{nid}/session:{id}");
        let mut kids: Vec<String> = Vec::new();
        let mut reach: HashSet<String> = HashSet::new();
        let mut pending: Vec<(String, String)> = Vec::new();
        for s in &node.sessions {
            match s.parent.as_deref().filter(|p| on_node(p) && *p != s.session_id) {
                Some(parent) => pending.push((s.session_id.clone(), parent.to_string())),
                None => {
                    kids.push(far(&s.session_id));
                    reach.insert(s.session_id.clone());
                }
            }
        }
        while !pending.is_empty() {
            let before = pending.len();
            let mut rest = Vec::new();
            for (sid, parent) in pending {
                if reach.contains(&parent) {
                    children.entry(far(&parent)).or_default().push(far(&sid));
                    reach.insert(sid);
                } else {
                    rest.push((sid, parent));
                }
            }
            pending = rest;
            if pending.len() == before {
                let (sid, _) = pending.remove(0);
                kids.push(far(&sid));
                reach.insert(sid);
            }
        }
        for s in &node.sessions {
            let sid = far(&s.session_id);
            // A cached row's state is what the far node LAST said; the
            // card's state slot carries the cache's word instead, and the
            // old state rides the activity row as history.
            let (state, activity) = if s.is_cached() {
                (s.presence.clone(), format!("was {}", s.state))
            } else {
                (s.state.clone(), s.cwd.clone())
            };
            meta.insert(
                sid.clone(),
                Meta {
                    kind: NodeKind::Session,
                    label: s.label.clone(),
                    title: String::new(),
                    role: String::new(),
                    harness: s.agent.clone(),
                    // No bare session id: Enter, `s` and the menu resolve a
                    // card through `session_id` against this box's roster,
                    // and a far id must never find a local record there.
                    // The roster label already carries the tail.
                    session_id: None,
                    state: Some(state),
                    tags: Vec::new(),
                    model: None,
                    host: Some(node.name.clone()),
                    remote_id: Some(s.session_id.clone()),
                    petname: s.petname.clone(),
                    activity,
                    cached: s.is_cached() || probe_failed.is_some(),
                },
            );
        }
        if !kids.is_empty() {
            children.insert(nid.clone(), kids);
        }
        roots.push(nid);
    }

    // Preorder DFS: row = push order, depth = distance from the root.
    let mut nodes: Vec<Node> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    for r in &roots {
        walk(r, 0, &meta, &children, &mut visited, &mut nodes);
    }

    place(&mut nodes, &children, &roots, &app.graph.positions);

    let visible = visible_order(&nodes, &children, &app.graph);
    let selected = visible
        .iter()
        .position(|&i| nodes[i].id == app.graph.selected)
        .unwrap_or(0);
    let summaries: Vec<(usize, Fold)> = nodes
        .iter()
        .enumerate()
        .filter(|(_, n)| app.graph.folded.contains(&n.id))
        .map(|(i, n)| (i, fold_summary(&n.id, &nodes, &children)))
        .filter(|(_, f)| f.hidden > 0)
        .collect();
    for (i, fold) in summaries {
        nodes[i].folded = Some(fold);
    }

    for node in nodes.iter_mut().filter(|n| n.host.is_none()) {
        if let Some(record) = node
            .session_id
            .as_ref()
            .and_then(|id| merged.iter().find(|s| &s.session_id == id))
        {
            node.petname = record.petname.clone();
            node.activity = match (
                record.tool.as_deref().filter(|s| !s.is_empty()),
                record
                    .activity
                    .as_deref()
                    .or(record.say.as_deref())
                    .filter(|s| !s.is_empty()),
            ) {
                (Some(tool), Some(activity)) if tool != activity => format!("{tool} · {activity}"),
                (Some(tool), _) => tool.into(),
                (_, Some(activity)) => activity.into(),
                _ => String::new(),
            };
        }
    }
    Model {
        nodes,
        children,
        visible,
        selected,
        camera: app.graph.camera,
    }
}

/// The synthetic gathering root's node id.
pub const UNANCHORED_ID: &str = "unanchored";

fn str_field(v: &serde_json::Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string()
}

fn walk(
    id: &str,
    depth: usize,
    meta: &HashMap<String, Meta>,
    children: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    out: &mut Vec<Node>,
) {
    if !visited.insert(id.to_string()) {
        return; // cycle guard (a hand-edited stage file could carry one)
    }
    if let Some(m) = meta.get(id) {
        out.push(Node {
            id: id.to_string(),
            kind: m.kind,
            label: m.label.clone(),
            title: m.title.clone(),
            activity: m.activity.clone(),
            petname: m.petname.clone(),
            role: m.role.clone(),
            harness: m.harness.clone(),
            session_id: m.session_id.clone(),
            state: m.state.clone(),
            tags: m.tags.clone(),
            model: m.model.clone(),
            host: m.host.clone(),
            remote_id: m.remote_id.clone(),
            cached: m.cached,
            folded: None,
            depth,
            row: out.len(),
            world: WorldRect::new(0, 0, CARD_W, CARD_H),
        });
    }
    if let Some(kids) = children.get(id) {
        for k in kids {
            walk(k, depth + 1, meta, children, visited, out);
        }
    }
}

// ── World placement: a fresh proposal, the retained store decides ───────────

/// Lay the forest out fresh, then resolve it against the retained store and
/// stamp the surviving coordinates onto the nodes.
fn place(
    nodes: &mut [Node],
    children: &HashMap<String, Vec<String>>,
    roots: &[String],
    retained: &crate::scene::Positions,
) {
    let mut slot = 0;
    let mut slots: HashMap<String, i32> = HashMap::new();
    for root in roots {
        lay_slots(root, children, nodes, &mut slot, &mut slots);
        slot += SLOT;
    }
    let fresh: Vec<(String, Placed)> = nodes
        .iter()
        .map(|n| {
            (
                n.id.clone(),
                Placed {
                    x: CANVAS_PAD.0 + slots.get(&n.id).copied().unwrap_or(0),
                    y: CANVAS_PAD.1 + n.depth as i32 * (CARD_H + RANK_GAP),
                    depth: n.depth,
                },
            )
        })
        .collect();
    for (node, placed) in nodes
        .iter_mut()
        .zip(retained.place(&fresh, (CARD_W, CARD_H), SLOT))
    {
        node.world = WorldRect::new(placed.x, placed.y, CARD_W, CARD_H);
    }
}

/// The fresh proposal: leaves take successive horizontal slots, a parent
/// centres over its first and last descendant leaf. Selection order stays
/// preorder.
fn lay_slots(
    id: &str,
    children: &HashMap<String, Vec<String>>,
    nodes: &[Node],
    next: &mut i32,
    out: &mut HashMap<String, i32>,
) -> i32 {
    let Some(node) = nodes.iter().find(|n| n.id == id) else {
        return *next;
    };
    let depth = node.depth;
    let kids: Vec<String> = children
        .get(id)
        .into_iter()
        .flatten()
        .filter(|child| {
            nodes
                .iter()
                .any(|n| &n.id == *child && n.depth == depth + 1)
        })
        .cloned()
        .collect();
    let x = if kids.is_empty() {
        let x = *next;
        *next += SLOT;
        x
    } else {
        let spans: Vec<i32> = kids
            .iter()
            .map(|child| lay_slots(child, children, nodes, next, out))
            .collect();
        (spans[0] + spans[spans.len() - 1]) / 2
    };
    out.insert(id.to_string(), x);
    x
}

// ── The view choice: focused component, or the whole forest ─────────────────

/// Indices of the nodes the current view draws, in preorder.
///
/// `All` is every node. `Focus` — the default — is the connected component
/// around the picked card: the tree of agents and terminals it controls or is
/// connected to, and nothing else. The synthetic gathering root is not a
/// connection, so its edges are not traversed: a terminal that belongs to
/// nothing shows itself alone rather than borrowing a forest of strangers.
/// Picking the gathering root itself still opens the sessions under it.
fn visible_order(
    nodes: &[Node],
    children: &HashMap<String, Vec<String>>,
    scene: &crate::scene::SceneState,
) -> Vec<usize> {
    let hidden = hidden_by_folds(nodes, children, &scene.folded);
    if scene.view == View::All || nodes.is_empty() {
        return (0..nodes.len())
            .filter(|i| !hidden.contains(nodes[*i].id.as_str()))
            .collect();
    }
    let anchor = if nodes.iter().any(|n| n.id == scene.selected) {
        scene.selected.clone()
    } else {
        nodes[0].id.clone()
    };
    let bridged = anchor != UNANCHORED_ID;
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for (parent, kids) in children {
        for kid in kids {
            if bridged && (parent == UNANCHORED_ID || kid == UNANCHORED_ID) {
                continue;
            }
            adjacency.entry(parent).or_default().push(kid);
            adjacency.entry(kid).or_default().push(parent);
        }
    }
    let mut seen: HashSet<&str> = HashSet::from([anchor.as_str()]);
    let mut queue: VecDeque<&str> = VecDeque::from([anchor.as_str()]);
    while let Some(id) = queue.pop_front() {
        for next in adjacency.get(id).into_iter().flatten() {
            if seen.insert(next) {
                queue.push_back(next);
            }
        }
    }
    (0..nodes.len())
        .filter(|&i| seen.contains(nodes[i].id.as_str()) && !hidden.contains(nodes[i].id.as_str()))
        .collect()
}

/// Every descendant of a folded card, by id. A folded card itself stays;
/// what it folds away is everything beneath it.
fn hidden_by_folds<'a>(
    nodes: &'a [Node],
    children: &'a HashMap<String, Vec<String>>,
    folded: &std::collections::BTreeSet<String>,
) -> HashSet<&'a str> {
    let mut hidden: HashSet<&str> = HashSet::new();
    for id in folded {
        let Some(root) = nodes.iter().find(|n| &n.id == id) else {
            continue;
        };
        let mut queue: VecDeque<&str> = VecDeque::from([root.id.as_str()]);
        while let Some(cur) = queue.pop_front() {
            for kid in children.get(cur).into_iter().flatten() {
                if let Some(n) = nodes.iter().find(|n| &n.id == kid) {
                    if hidden.insert(n.id.as_str()) {
                        queue.push_back(n.id.as_str());
                    }
                }
            }
        }
    }
    hidden
}

/// What one folded card hides: the descendants under it and how many are
/// awaiting a human or working.
fn fold_summary(id: &str, nodes: &[Node], children: &HashMap<String, Vec<String>>) -> Fold {
    let one = std::collections::BTreeSet::from([id.to_string()]);
    let hidden = hidden_by_folds(nodes, children, &one);
    let mut fold = Fold {
        hidden: hidden.len(),
        awaiting: 0,
        working: 0,
    };
    for n in nodes.iter().filter(|n| hidden.contains(n.id.as_str())) {
        match theme::classify(n.state.as_deref().unwrap_or("")) {
            theme::StateClass::Awaiting => fold.awaiting += 1,
            theme::StateClass::Working => fold.working += 1,
            _ => {}
        }
    }
    fold
}

// ── Camera geometry: one transform, shared by render, hit test and pan ──────

/// The visible canvas size in scaled cells, padded so the camera reaches past
/// the outermost cards.
pub fn graph_extent(app: &App) -> (i32, i32) {
    extent(&build_model(app))
}

fn extent(model: &Model) -> (i32, i32) {
    let pad = model
        .camera
        .scale_rect(WorldRect::new(0, 0, CANVAS_PAD.0, CANVAS_PAD.1));
    model.visible().fold((pad.w, pad.h), |(w, h), node| {
        let r = model.camera.scale_rect(node.world);
        (w.max(r.right() + pad.w), h.max(r.bottom() + pad.h))
    })
}

/// The camera origin — the scaled point the viewport's top-left shows.
pub fn graph_origin(app: &App, area: Rect) -> (i32, i32) {
    origin(&build_model(app), area)
}

fn origin(model: &Model, area: Rect) -> (i32, i32) {
    let (ew, eh) = extent(model);
    let (pw, ph) = (area.width as i32, area.height as i32);
    let clamp = |v: i32, e: i32, pane: i32| v.clamp(0, (e - pane).max(0));
    if let Some((x, y)) = model.camera.pan {
        return (clamp(x, ew, pw), clamp(y, eh, ph));
    }
    // The camera follows the selection, but shows the forest, never the pad
    // around it: the pad exists so a drag or a zoom can reach past the
    // outermost cards, and a camera nobody has moved has no reason to be
    // there. Along an axis the drawn forest fits in, the pane holds the
    // forest — top-aligned vertically (the roots are the orientation), centred
    // horizontally. Along an axis it overflows, the selected card sits at the
    // centre, clamped so the pane never leaves the forest's own bounds. A card
    // larger than the pane anchors its top-left instead.
    let Some(sel) = model.visible().nth(model.selected) else {
        return (0, 0);
    };
    let r = model.camera.scale_rect(sel.world);
    let (x0, y0, x1, y1) = model.visible().fold(
        (i32::MAX, i32::MAX, i32::MIN, i32::MIN),
        |(x0, y0, x1, y1), n| {
            let b = model.camera.scale_rect(n.world);
            (x0.min(b.x), y0.min(b.y), x1.max(b.right()), y1.max(b.bottom()))
        },
    );
    let follow = |o: i32, len: i32, lo: i32, hi: i32, pane: i32, centre_fit: bool| {
        if hi - lo <= pane {
            if centre_fit {
                lo + (hi - lo) / 2 - pane / 2
            } else {
                lo
            }
        } else if len <= pane {
            (o + len / 2 - pane / 2).clamp(lo, hi - pane)
        } else {
            o.clamp(lo, hi - pane)
        }
    };
    // Clamped at zero only: the padded extent is the DRAG's range, and
    // clamping a follow origin against it would pull the forest back down
    // the pane to keep the bottom pad reachable.
    (
        follow(r.x, r.w, x0, x1, pw, true).max(0),
        follow(r.y, r.h, y0, y1, ph, false).max(0),
    )
}

/// Zoom the camera over the retained world. Terminal glyphs remain cell-sized:
/// the layout is transformed, never relaid out into a different card preset.
pub fn zoom_at(app: &mut App, area: Rect, pointer: (u16, u16), delta: i8) {
    if delta == 0 || !area.contains(ratatui::layout::Position::new(pointer.0, pointer.1)) {
        return;
    }
    let next = app
        .graph
        .camera
        .zoom
        .saturating_add(delta.signum())
        .clamp(crate::scene::ZOOM_MIN, crate::scene::ZOOM_MAX);
    if next == app.graph.camera.zoom {
        return;
    }
    let old = app.graph.camera.scale();
    let new = Camera {
        zoom: next,
        ..app.graph.camera
    }
    .scale();
    let (ox, oy) = graph_origin(app, area);
    let px = (pointer.0 - area.x) as i32;
    let py = (pointer.1 - area.y) as i32;
    // Keep the world point under the pointer under the pointer.
    let x = ((ox + px) * new / old - px).max(0);
    let y = ((oy + py) * new / old - py).max(0);
    app.graph.camera.zoom = next;
    let (w, h) = graph_extent(app);
    app.graph.camera.pan = Some((
        x.min((w - area.width as i32).max(0)),
        y.min((h - area.height as i32).max(0)),
    ));
    app.graph.drag = None;
}

/// The card under a viewport point, as an index into [`node_order`].
///
/// Hit testing runs the same camera transform over the same retained world
/// that render does, so a click and a key resolve the same card.
pub fn hit_node(area: Rect, app: &App, x: u16, y: u16) -> Option<usize> {
    if !area.contains(ratatui::layout::Position::new(x, y)) {
        return None;
    }
    let model = build_model(app);
    let o = origin(&model, area);
    let (px, py) = ((x - area.x) as i32 + o.0, (y - area.y) as i32 + o.1);
    let index = model
        .visible()
        .position(|n| model.camera.scale_rect(n.world).contains(px, py));
    index
}

// ── Rendering: the scene, clipped to the viewport ───────────────────────────

/// The retained graph scene as a widget: edges first, cards on top, every
/// write clipped to the viewport.
pub struct GraphScene<'a> {
    model: &'a Model,
    palette: &'a crate::app::Palette,
    /// The card drawn with the bright selection, or `usize::MAX` for none
    /// (another pane holds the keyboard).
    selected: usize,
}

impl Widget for GraphScene<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let o = origin(self.model, area);
        let cam = self.model.camera;
        let viewport = WorldRect::new(o.0, o.1, area.width as i32, area.height as i32);
        let mut p = Painter::new(buf, area);

        // A wire wears the colour of the live session it leads to (its state
        // hue), so an active agent lights its own connections; project trunks
        // keep the accent.
        let wire = |n: &Node| match n.state.as_deref() {
            Some(s) if n.kind == NodeKind::Session => theme::state_style(s, self.palette),
            _ => theme::accent_style(self.palette),
        };
        let port = |n: &Node| {
            let r = cam.scale_rect(n.world);
            (r, r.x + r.w / 2, wire(n))
        };
        let drawn: HashMap<&str, &Node> =
            self.model.visible().map(|n| (n.id.as_str(), n)).collect();

        for n in self.model.visible() {
            let kids: Vec<&Node> = self
                .model
                .children
                .get(&n.id)
                .into_iter()
                .flatten()
                .filter_map(|k| drawn.get(k.as_str()).copied())
                .collect();
            if kids.is_empty() {
                continue;
            }
            let (pr, px, conn) = port(n);
            let ports: Vec<_> = kids.iter().map(|k| port(k)).collect();
            let child_top = ports.iter().map(|(r, _, _)| r.y).min().unwrap();
            // The junction rank sits midway between the parent's bottom edge
            // and the topmost child's top edge — with uniform ranks that is
            // exactly the rank gap's centre line.
            let jy = pr.bottom().max((pr.bottom() + child_top) / 2);
            let lo = ports.iter().map(|(_, x, _)| *x).min().unwrap().min(px);
            let hi = ports.iter().map(|(_, x, _)| *x).max().unwrap().max(px);
            // Cull the whole bundle when no part of it can be on screen.
            if !WorldRect::new(lo, pr.y, hi - lo + 1, jy - pr.y + 1).intersects(&viewport)
                && !ports
                    .iter()
                    .any(|(r, x, _)| WorldRect::new(*x, jy, 1, r.y - jy + 1).intersects(&viewport))
            {
                continue;
            }
            vline(&mut p, o, px, pr.bottom(), jy, '│', conn);
            hline(&mut p, o, lo, hi, jy, '─', conn);
            for ((r, cx, kc), _) in ports.iter().zip(&kids) {
                vline(&mut p, o, *cx, jy + 1, r.y - 1, '│', *kc);
                set(
                    &mut p,
                    o,
                    *cx,
                    jy,
                    if lo == hi {
                        '│'
                    } else if *cx == lo {
                        '┌'
                    } else if *cx == hi {
                        '┐'
                    } else {
                        '┬'
                    },
                    *kc,
                );
            }
            set(
                &mut p,
                o,
                px,
                jy,
                if lo == hi {
                    '│'
                } else if px == lo {
                    '├'
                } else if px == hi {
                    '┤'
                } else {
                    '┼'
                },
                conn,
            );
        }

        // Cards last, so an edge never draws over the card it arrives at. One
        // scratch buffer carries each card's real-widget render before the
        // clipping Painter blits it into the frame; it is resized only when
        // the zoomed card size actually changes, never per card, since every
        // card in one frame shares the same camera.
        let mut scratch = Buffer::empty(Rect::new(0, 0, 1, 1));
        for (i, n) in self.model.visible().enumerate() {
            let r = cam.scale_rect(n.world);
            if !r.intersects(&viewport) {
                continue; // culled: off camera costs a comparison, not a cell
            }
            let (cw, ch) = (r.w.max(2) as u16, r.h.max(2) as u16);
            let scratch_area = *scratch.area();
            if scratch_area.width != cw || scratch_area.height != ch {
                scratch = Buffer::empty(Rect::new(0, 0, cw, ch));
            } else {
                scratch.reset();
            }
            render_card_into(n, i == self.selected, self.palette, &mut scratch);
            blit_card(&mut p, o, r.x, r.y, &scratch);
        }
    }
}

/// Copies a rendered card's scratch buffer into the frame through the
/// clipping `Painter`, one glyph at a time and width-aware, so a wide
/// character is never split across the blit -- its buffer's own trailing
/// cell (already blanked by `Buffer::set_stringn` when the glyph was drawn)
/// is skipped rather than independently painted over. The buffer itself is
/// never clipped; the Painter is what clips this blit to the viewport,
/// exactly as it clipped the old per-glyph placement.
fn blit_card(p: &mut Painter, o: (i32, i32), cx: i32, cy: i32, card: &Buffer) {
    let area = *card.area();
    for y in 0..area.height {
        let mut x = 0u16;
        while x < area.width {
            let cell = &card[(x, y)];
            let symbol = cell.symbol();
            let width = Span::raw(symbol).width().max(1) as i32;
            p.set(
                cx + x as i32 - o.0,
                cy + y as i32 - o.1,
                symbol,
                width,
                cell.style(),
            );
            x += width as u16;
        }
    }
}

fn set(p: &mut Painter, o: (i32, i32), x: i32, y: i32, ch: char, style: Style) {
    set_cell(p, o, x, y, ch, 1, style);
}

fn set_cell(p: &mut Painter, o: (i32, i32), x: i32, y: i32, ch: char, width: i32, style: Style) {
    let mut buf = [0u8; 4];
    p.set(x - o.0, y - o.1, ch.encode_utf8(&mut buf), width, style);
}

/// A horizontal run, clipped to the viewport before it is walked — the loop is
/// bounded by the pane, never by the canvas.
fn hline(p: &mut Painter, o: (i32, i32), x0: i32, x1: i32, y: i32, ch: char, style: Style) {
    let lo = x0.max(o.0);
    let hi = x1.min(o.0 + p.area().width as i32 - 1);
    for x in lo..=hi {
        set(p, o, x, y, ch, style);
    }
}

fn vline(p: &mut Painter, o: (i32, i32), x: i32, y0: i32, y1: i32, ch: char, style: Style) {
    let lo = y0.max(o.1);
    let hi = y1.min(o.1 + p.area().height as i32 - 1);
    for y in lo..=hi {
        set(p, o, x, y, ch, style);
    }
}

/// Draw the Graph panel into `area`, highlighting the selected node.
pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let model = build_model(app);
    if model.visible_len() == 0 {
        let lines = vec![
            Line::from(""),
            Line::from("  no graph yet — no projects registered, no sessions live.")
                .style(theme::dim()),
            Line::from(""),
            Line::from("  Seed a stage tree:  pkgs/aoide/tests/fixtures/seed.sh $AOIDE_STAGE_DIR")
                .style(theme::dim()),
            Line::from("  Then re-open the conductor — every stage mutation restages the graph.")
                .style(theme::dim()),
        ];
        f.render_widget(Paragraph::new(lines), area);
        return;
    }
    f.render_widget(
        GraphScene {
            model: &model,
            palette: &app.palette,
            selected: if app.sidebar_focused {
                usize::MAX
            } else {
                model.selected
            },
        },
        area,
    );
}

// ── Card content ────────────────────────────────────────────────────────────

/// Renders one card into `buf`, sized and reset by the caller. A bordered
/// `Block` -- `BorderType::Thick`/`Plain` are the exact `┏┓┗┛━┃`/`┌┐└┘─│`
/// glyph sets the hand-drawn border used -- stands in for the old per-corner,
/// per-edge `put()` loop, and `Buffer::set_line` places each already-fitted
/// row in place of the old per-glyph placement loop with its own
/// continuation-cell bookkeeping; `Block`'s own `style` fill and
/// `Buffer::set_stringn` (via `set_line`) already do that cell-width work.
/// Chosen over `Paragraph`: the rows are already an ordered, filtered,
/// enumerated `(text, style)` sequence (dropping an empty row so the ones
/// below it compact upward), and hand-timing each one's `y` to `set_line`
/// needs no `Vec<Line>` collection step `Paragraph` would otherwise want.
fn render_card_into(n: &Node, selected: bool, pal: &crate::app::Palette, buf: &mut Buffer) {
    let area = *buf.area();
    let surface = theme::surface(
        pal,
        if n.kind != NodeKind::Session {
            10
        } else if n.role == "shell" || n.harness == "shell" {
            3
        } else {
            6
        },
    );
    let accent = theme::role_color(
        pal,
        if n.kind == NodeKind::Host {
            theme::Role::Host
        } else if n.kind != NodeKind::Session {
            theme::Role::Project
        } else if n.harness == "shell" || n.role == "terminal" {
            theme::Role::Terminal
        } else {
            theme::Role::Agent
        },
    );
    let border = if selected {
        surface.fg(accent).add_modifier(Modifier::BOLD)
    } else if n.kind != NodeKind::Session {
        surface.fg(accent)
    } else {
        surface.patch(theme::state_style(n.state.as_deref().unwrap_or(""), pal))
    };
    // A cached card is muted whole, like the Mesh row it mirrors.
    let surface = if n.cached { surface.patch(theme::muted(pal)) } else { surface };
    let border = if n.cached { border.patch(theme::muted(pal)) } else { border };
    let state = n.state.as_deref().unwrap_or("");
    let role = if n.role.is_empty() {
        if n.harness == "shell" {
            "terminal"
        } else if !n.harness.is_empty() {
            "agent"
        } else {
            "session"
        }
    } else if n.role == "shell" {
        "terminal"
    } else {
        &n.role
    };
    let mark = theme::mark(match role {
        "project" | "root" => theme::Mark::Project,
        "terminal" => theme::Mark::Terminal,
        "node" => theme::Mark::Host,
        _ if n.kind != NodeKind::Session => theme::Mark::Project,
        _ => theme::Mark::Agent,
    });
    let heading = if state.is_empty() {
        format!("{mark} {}", role.to_uppercase())
    } else {
        format!("{mark} {} · {}", role.to_uppercase(), state)
    };
    let tags = n
        .tags
        .iter()
        .map(|t| format!("[{t}]"))
        .collect::<Vec<_>>()
        .join(" ");
    let detail = match (&n.harness, n.model.as_deref()) {
        (h, Some(m)) if !h.is_empty() => format!("{h} · {m}"),
        (_, Some(m)) => m.into(),
        (h, None) => h.clone(),
    };
    let card_identity = match (&n.petname, n.session_id.as_deref()) {
        (Some(name), Some(id)) => {
            let tail: String = id
                .chars()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            format!("{name} (…{tail})")
        }
        _ => n.label.clone(),
    };
    // The card is 24×5: the top border carries one line of text and the
    // three inner rows the rest. A session's border is its title — or, with
    // no title, its harness and model, so a titleless card still names what
    // runs there exactly once. Inside: the identity (petname and tail), the
    // role and state with any tags, and the activity — or, when the card is
    // idle enough to report none, the harness and model the title row did
    // not already carry. Harness and model otherwise live in the tree row
    // and the Details view. A non-session card's border is its kind.
    let border_budget = area.width.saturating_sub(2) as usize;
    let (top, rows): (String, Vec<(String, Style)>) = if n.kind == NodeKind::Session {
        let titled = !n.title.is_empty();
        let top = if titled { n.title.clone() } else { detail.clone() };
        let heading = if tags.is_empty() {
            heading
        } else {
            format!("{heading} {tags}")
        };
        let third = if !n.activity.is_empty() {
            n.activity.clone()
        } else if titled {
            detail.clone()
        } else {
            String::new()
        };
        (
            top,
            vec![
                (String::new(), surface),
                (heading, surface.patch(theme::state_style(state, pal))),
                (third, surface),
            ],
        )
    } else {
        (
            heading,
            vec![
                (n.label.clone(), surface.add_modifier(Modifier::BOLD)),
                (n.title.clone(), surface),
                (if detail.is_empty() { tags } else { detail }, surface.fg(accent)),
            ],
        )
    };
    let top_style = if n.kind == NodeKind::Session {
        border.add_modifier(Modifier::BOLD)
    } else {
        border.fg(accent).add_modifier(Modifier::BOLD)
    };
    let block = Block::bordered()
        .border_type(if selected {
            BorderType::Thick
        } else {
            BorderType::Plain
        })
        .border_style(border)
        .title(Line::from(Span::styled(
            if border_budget >= 4 {
                format!(" {} ", truncate_end(&top, border_budget - 2))
            } else {
                String::new()
            },
            top_style,
        )))
        .title_bottom(Line::from(Span::styled(
            match n.folded {
                Some(fold) if border_budget >= 4 => {
                    format!(" {} ", fold.label_fitting(border_budget - 2))
                }
                _ => String::new(),
            },
            match n.folded {
                Some(fold) if fold.awaiting > 0 => border.patch(theme::state_style("awaiting", pal)),
                _ => border.add_modifier(Modifier::BOLD),
            },
        )))
        .style(surface)
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    block.render(area, buf);
    let budget = inner.width as usize;
    let rows = rows.into_iter().enumerate().map(|(i, (text, style))| {
        let text = if i == 0 && n.kind == NodeKind::Session {
            fit_label(&card_identity, budget)
        } else {
            truncate_end(&text, budget)
        };
        (text, style)
    });
    for (idx, (text, style)) in rows.enumerate().take(inner.height as usize) {
        buf.set_line(
            inner.x,
            inner.y + idx as u16,
            &Line::from(Span::styled(text, style)),
            inner.width,
        );
    }
}

/// Fit a display-grammar label (`<host>/<role>/<petname> (…<tail4>)`, or the
/// legacy `<host>/<role>/<sessionId>` with no tail bracket) into `budget`
/// characters. The tail4 grep-back handle is the LAST thing to die — it is
/// the only link back to the canonical id once host/role/petname are gone:
///
/// 1. Fits as-is → returned unchanged.
/// 2. `<host>/<role>/` and the ` (…<tail4>)` bracket both preserved; the
///    petname/id between them middle-elided (`hardy-…rbor`) to make room.
/// 3. Host dropped: `<role>/<petname-truncated>… (…<tail4>)`.
/// 4. Role dropped too: `<petname-prefix> (…<tail4>)`.
/// 5. No room for any petname/id at all — just the tail bracket, itself
///    front-truncated if `budget` is smaller than the bracket.
/// 6. No tail to preserve (a legacy label, or `budget` too small for even
///    a lone bracket char) — a blunt front-truncate of the raw label.
fn fit_label(label: &str, budget: usize) -> String {
    let len = label.chars().count();
    if len <= budget {
        return label.to_string();
    }
    if budget == 0 {
        return String::new();
    }

    let (head, tail) = match label.rfind(" (…") {
        Some(i) => (&label[..i], &label[i..]),
        None => (label, ""),
    };
    let tail_len = tail.chars().count();
    let mut segs = head.splitn(3, '/');
    let (host, role, name) = match (segs.next(), segs.next(), segs.next()) {
        (Some(h), Some(r), Some(nm)) => (h, r, nm),
        _ => ("", "", head),
    };

    // Rung 1: host/role/ + middle-elided name + tail.
    if !host.is_empty() && !role.is_empty() {
        let fixed = host.chars().count() + 1 + role.chars().count() + 1 + tail_len;
        if fixed < budget {
            let name_budget = budget - fixed;
            return format!("{host}/{role}/{}{tail}", elide_middle(name, name_budget));
        }
    }
    // Rung 2: role/ + end-truncated name + tail (host dropped).
    if !role.is_empty() {
        let fixed = role.chars().count() + 1 + tail_len;
        if fixed < budget {
            let name_budget = budget - fixed;
            return format!("{role}/{}{tail}", truncate_end(name, name_budget));
        }
    }
    // Rung 3: name-prefix + tail (host AND role dropped).
    if tail_len < budget {
        let name_budget = budget - tail_len;
        let prefix: String = name.chars().take(name_budget).collect();
        return format!("{prefix}{tail}");
    }
    // Rung 4: the tail bracket alone — nothing else survives — itself
    // front-truncated if even the bracket doesn't fit.
    if tail_len > 0 {
        return tail.chars().take(budget).collect();
    }
    // No tail at all (legacy label) — blunt front-truncate, the true last resort.
    label.chars().take(budget).collect()
}

/// Middle-elide `s` into `budget` chars: `hardy-harbor` at budget 7 becomes
/// `har…bor` — keeps BOTH ends visible (the common case has plenty of room
/// for this; see [`fit_label`] rung 1).
fn elide_middle(s: &str, budget: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= budget {
        return s.to_string();
    }
    if budget == 0 {
        return String::new();
    }
    if budget == 1 {
        return "…".to_string();
    }
    let keep = budget - 1; // reserve 1 cell for the ellipsis itself.
    let head_n = keep - keep / 2;
    let tail_n = keep / 2;
    let head: String = chars[..head_n].iter().collect();
    let tail: String = chars[chars.len() - tail_n..].iter().collect();
    format!("{head}…{tail}")
}

/// End-truncate `s` into `budget` chars with a trailing ellipsis (used once
/// the host is already gone — see [`fit_label`] rung 2 — where showing only
/// the FRONT of the petname/id reads more naturally than a middle elision).
fn truncate_end(s: &str, budget: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= budget {
        return s.to_string();
    }
    if budget == 0 {
        return String::new();
    }
    if budget == 1 {
        return "…".to_string();
    }
    let head: String = chars[..budget - 1].iter().collect();
    format!("{head}…")
}

/// Render one card in isolation, at its own retained world size, for tests
/// that inspect a single card's buffer without a `GraphScene`/`Camera`.
#[cfg(test)]
fn block_cells(n: &Node, selected: bool, pal: &crate::app::Palette) -> Buffer {
    block_cells_at(n, selected, pal, n.world.w, n.world.h)
}

/// As [`block_cells`], but at an explicit size — used to probe clipping at
/// sizes the retained world rectangle would not itself produce.
#[cfg(test)]
fn block_cells_at(
    n: &Node,
    selected: bool,
    pal: &crate::app::Palette,
    width: i32,
    height: i32,
) -> Buffer {
    let mut buf = Buffer::empty(Rect::new(0, 0, width.max(2) as u16, height.max(2) as u16));
    render_card_into(n, selected, pal, &mut buf);
    buf
}

/// Coalesce each buffer row's runs of same-style cells into ratatui spans,
/// walking width-aware so a wide glyph's already-blanked trailing cell (see
/// `Buffer::set_stringn`) is stepped over rather than independently visited.
#[cfg(test)]
fn grid_to_lines(buf: &Buffer) -> Vec<Line<'static>> {
    let area = *buf.area();
    (0..area.height)
        .map(|y| {
            let mut spans: Vec<Span<'static>> = Vec::new();
            let mut text = String::new();
            let mut cur: Option<Style> = None;
            let mut x = 0u16;
            while x < area.width {
                let cell = &buf[(x, y)];
                let symbol = cell.symbol();
                let w = Span::raw(symbol).width().max(1) as u16;
                let style = cell.style();
                match cur {
                    Some(s) if s == style => text.push_str(symbol),
                    _ => {
                        if let Some(s) = cur {
                            spans.push(Span::styled(std::mem::take(&mut text), s));
                        }
                        text.push_str(symbol);
                        cur = Some(style);
                    }
                }
                x += w;
            }
            if let Some(s) = cur {
                spans.push(Span::styled(text, s));
            }
            Line::from(spans)
        })
        .collect()
}

/// Flatten a buffer's rows into plain strings, one per row, in column order.
/// The test-only counterpart to `grid_to_lines` for assertions that want raw
/// text rather than styled spans.
#[cfg(test)]
fn buffer_rows(buf: &Buffer) -> Vec<String> {
    let area = *buf.area();
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::scene::Placed;
    use aoide_conduct::graph::{Project, SessionRecord};
    use ratatui::style::Color;
    use serde_json::Map;

    fn session(id: &str, cwd: &str, state: &str, parent: Option<&str>) -> SessionRecord {
        SessionRecord {
            session_id: id.into(),
            enduring_agent_id: None,
            project: None,
            agent: "claude".into(),
            window_address: format!("0x{id}"),
            cwd: cwd.into(),
            state: state.into(),
            shell: false,
            started_at: id.into(),
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
            needs_sudo: None,
            context_ceiling: None,
            log_path: None,
            sources: None,
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
            native_role: None,
            origin: None,
            seal: None,
            sealed_issued_at: None,
            restore: None,
            extra: Map::new(),
        }
    }

    fn aoide() -> Project {
        Project {
            name: "aoide".into(),
            path: "/home/k/Aoide".into(),
            ..Default::default()
        }
    }

    /// Paint the scene exactly as [`render`] does, into a standalone buffer.
    fn paint(app: &App, area: Rect) -> Buffer {
        let model = build_model(app);
        let mut buf = Buffer::empty(area);
        GraphScene {
            model: &model,
            palette: &app.palette,
            selected: model.selected,
        }
        .render(area, &mut buf);
        buf
    }

    fn dump(buf: &Buffer) -> String {
        let area = *buf.area();
        (0..area.height)
            .map(|y| {
                (0..area.width)
                    .map(|x| buf[(area.x + x, area.y + y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn painted(buf: &Buffer) -> usize {
        buf.content()
            .iter()
            .filter(|c| c.symbol() != " " && !c.symbol().is_empty())
            .count()
    }

    fn node<'a>(model: &'a Model, id: &str) -> &'a Node {
        model
            .nodes
            .iter()
            .find(|n| n.session_id.as_deref() == Some(id) || n.id == id || n.label == id)
            .unwrap_or_else(|| panic!("no node {id}"))
    }

    // ── The document → the typed model ─────────────────────────────────────

    #[test]
    fn model_lays_out_projects_then_spawned_children_in_ranks() {
        let app = App::for_test(
            vec![aoide()],
            vec![
                session("root", "/home/k/Aoide", "running", None),
                session("kid", "/home/k/Aoide", "idle", Some("root")),
            ],
            Vec::new(),
        );
        let m = build_model(&app);
        // project (depth 0) → root session (depth 1) → spawned kid (depth 2).
        // Session labels render the display grammar (petnames plan P3), not the
        // bare id — so lookups go through `session_id`, the field that stays
        // the bare canonical id (Enter's focus jump unaffected by the label).
        let (proj, root, kid) = (node(&m, "aoide"), node(&m, "root"), node(&m, "kid"));
        assert_eq!((proj.depth, root.depth, kid.depth), (0, 1, 2));
        assert!(proj.row < root.row && root.row < kid.row);
        assert_eq!(kid.session_id.as_deref(), Some("kid"));
        // Ranks are world coordinates, one card plus one rank gap apart.
        assert_eq!(root.world.y - proj.world.y, CARD_H + RANK_GAP);
        assert_eq!(kid.world.y - root.world.y, CARD_H + RANK_GAP);
        assert_eq!((kid.world.w, kid.world.h), (CARD_W, CARD_H));
    }

    #[test]
    fn unanchored_sessions_gather_under_a_synthetic_root() {
        let app = App::for_test(
            vec![],
            vec![session("loose", "/tmp/x", "idle", None)],
            Vec::new(),
        );
        let m = build_model(&app);
        assert!(m.nodes.iter().any(|n| n.kind == NodeKind::Unanchored));
        assert!(m
            .nodes
            .iter()
            .any(|n| n.session_id.as_deref() == Some("loose")));
    }

    #[test]
    fn model_flows_from_the_graph_document_onto_agent_and_subagent_nodes() {
        let mut root = session("root", "/home/k/Aoide", "running", None);
        root.model = Some("claude-sonnet-5".into());
        let mut sub = session("kid", "/home/k/Aoide", "working", Some("root"));
        sub.kind = Some("subagent".into());
        sub.model = Some("claude-fable-5".into());
        // A shell has no model — the fixture leaves it `None`, mirroring what
        // `extract_model` actually produces for a conducted terminal.
        let shell = session("term", "/home/k/Aoide", "idle", None);
        let app = App::for_test(vec![aoide()], vec![root, sub, shell], Vec::new());
        let m = build_model(&app);
        assert_eq!(node(&m, "root").model.as_deref(), Some("claude-sonnet-5"));
        assert_eq!(node(&m, "kid").model.as_deref(), Some("claude-fable-5"));
        assert_eq!(node(&m, "term").model, None);
    }

    #[test]
    fn tags_flow_from_extra_read_only() {
        let mut s = session("t", "/home/k/Aoide", "running", None);
        s.extra
            .insert("tags".into(), serde_json::json!(["backend", "wip"]));
        let app = App::for_test(vec![aoide()], vec![s], Vec::new());
        let m = build_model(&app);
        assert_eq!(
            node(&m, "t").tags,
            vec!["backend".to_string(), "wip".to_string()]
        );
    }

    #[test]
    fn session_label_renders_the_display_grammar_while_session_id_stays_bare() {
        // Petnames plan P3: `Node.label` is the grammar string —
        // `<host>/<role>/<petname> (…<tail4>)`, legacy degrading to
        // `<host>/<role>/<sessionId>` — but `Node::session_id` stays the bare
        // canonical id no matter what, since Enter/`graph focus` reads that
        // field, never the label.
        let mut root = session("root", "/home/k/Aoide", "running", None);
        root.petname = Some("brave-otter".into());
        let mut kid = session("kid", "/home/k/Aoide", "working", Some("root"));
        kid.petname = Some("calm-thorn".into());
        let legacy = session("legacy-full-id", "/home/k/Aoide", "idle", None);
        let app = App::for_test(vec![aoide()], vec![root, kid, legacy], Vec::new());
        let m = build_model(&app);
        let host = aoide_storage::display::local_host_name();

        assert_eq!(
            node(&m, "root").label,
            format!("{host}/root/brave-otter (…root)")
        );
        assert_eq!(node(&m, "root").session_id.as_deref(), Some("root"));
        assert_eq!(
            node(&m, "kid").label,
            format!("{host}/child/calm-thorn (…kid)")
        );
        assert_eq!(
            node(&m, "legacy-full-id").label,
            format!("{host}/root/legacy-full-id"),
            "legacy (petname-less) node degrades to host/role/full-id"
        );
    }

    // ── Retention: the whole point of a retained scene ─────────────────────

    #[test]
    fn world_coordinates_survive_a_refresh_that_adds_and_removes_sessions() {
        let mut app = App::for_test(
            vec![aoide()],
            vec![
                session("a", "/home/k/Aoide", "working", None),
                session("b", "/home/k/Aoide", "idle", None),
                session("c", "/home/k/Aoide", "idle", None),
            ],
            Vec::new(),
        );
        app.sync_graph_scene();
        let before: HashMap<String, WorldRect> = build_model(&app)
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n.world))
            .collect();
        assert_eq!(app.graph.positions.len(), before.len());

        // `b` ends and a new session arrives. A recomputed layout would hoist
        // `c` into `b`'s lane and shuffle every card under the operator's
        // cursor; the retained scene must not move anything that stayed.
        app.sessions.retain(|s| s.session_id != "b");
        app.sessions
            .push(session("d", "/home/k/Aoide", "working", None));
        app.sync_graph_scene();
        let after = build_model(&app);
        for n in &after.nodes {
            if let Some(was) = before.get(&n.id) {
                assert_eq!(&n.world, was, "{} moved on refresh", n.id);
            }
        }
        let d = node(&after, "d");
        assert!(
            after
                .nodes
                .iter()
                .filter(|n| n.id != d.id)
                .all(|n| !n.world.intersects(&d.world)),
            "the arrival lands clear of every retained card"
        );
        assert!(app.graph.positions.get("session:b").is_none());
    }

    #[test]
    fn selection_names_the_same_card_across_a_refresh() {
        let mut app = App::for_test(
            vec![aoide()],
            vec![session("zulu", "/home/k/Aoide", "working", None)],
            Vec::new(),
        );
        app.sync_graph_scene();
        select_index(&mut app, 1);
        assert_eq!(selected_session_id(&app).as_deref(), Some("zulu"));

        // A session sorting BEFORE the selected one arrives. Under an index
        // this silently moved the cursor onto the newcomer; an id cannot.
        app.sessions
            .insert(0, session("alpha", "/home/k/Aoide", "idle", None));
        app.sync_graph_scene();
        assert_eq!(
            selected_session_id(&app).as_deref(),
            Some("zulu"),
            "the cursor still names the card it was put on"
        );

        // Once the selected card leaves the forest the selection falls back to
        // the first node rather than pointing at nothing.
        app.sessions.retain(|s| s.session_id != "zulu");
        app.sync_graph_scene();
        assert_eq!(app.graph.selected, build_model(&app).nodes[0].id);
    }

    #[test]
    fn a_re_parented_session_moves_to_its_new_rank() {
        let mut app = App::for_test(
            vec![aoide()],
            vec![
                session("parent", "/home/k/Aoide", "working", None),
                session("orphan", "/home/k/Aoide", "idle", None),
            ],
            Vec::new(),
        );
        app.sync_graph_scene();
        let was = node(&build_model(&app), "orphan").world;

        // The spawn edge resolves late: `orphan` is a child now, and a retained
        // rank would draw it ABOVE its own parent.
        app.sessions
            .iter_mut()
            .find(|s| s.session_id == "orphan")
            .unwrap()
            .parent_session_id = Some("parent".into());
        app.sync_graph_scene();
        let m = build_model(&app);
        let now = node(&m, "orphan");
        assert_eq!(now.depth, node(&m, "parent").depth + 1);
        assert_eq!(now.world.y, node(&m, "parent").world.y + CARD_H + RANK_GAP);
        assert_ne!(now.world.y, was.y);
    }

    // ── The view choice ────────────────────────────────────────────────────

    #[test]
    fn focus_draws_the_picked_component_and_all_draws_the_whole_forest() {
        let other = Project {
            name: "dxflake".into(),
            path: "/home/k/dxflake".into(),
            ..Default::default()
        };
        let mut app = App::for_test(
            vec![aoide(), other],
            vec![
                session("mine", "/home/k/Aoide", "working", None),
                session("mykid", "/home/k/Aoide", "idle", Some("mine")),
                session("theirs", "/home/k/dxflake", "idle", None),
            ],
            Vec::new(),
        );
        app.sync_graph_scene();
        assert_eq!(app.graph.view, View::Focus, "focus is the default");

        select_index(&mut app, 1); // the aoide project's own session
        let visible: Vec<String> = node_order(&app).iter().map(|n| n.id.clone()).collect();
        assert!(visible
            .iter()
            .any(|id| id == "project:aoide" || id.contains("aoide")));
        assert!(visible.iter().any(|id| id.ends_with("mine")));
        assert!(visible.iter().any(|id| id.ends_with("mykid")));
        assert!(
            !visible.iter().any(|id| id.ends_with("theirs")),
            "another project's forest is not connected: {visible:?}"
        );

        toggle_view(&mut app);
        assert_eq!(app.graph.view, View::All);
        assert_eq!(node_order(&app).len(), build_model(&app).nodes.len());
        assert!(node_order(&app).iter().any(|n| n.id.ends_with("theirs")));
    }

    #[test]
    fn a_session_connected_to_nothing_shows_itself_alone() {
        let mut app = App::for_test(
            vec![],
            vec![
                session("lonely", "/tmp/a", "idle", None),
                session("stranger", "/tmp/b", "idle", None),
            ],
            Vec::new(),
        );
        app.sync_graph_scene();
        // The synthetic gathering root is a grouping, never a connection —
        // picking one loose terminal must not drag in the other.
        let all = build_model(&app);
        let index = all
            .nodes
            .iter()
            .position(|n| n.session_id.as_deref() == Some("lonely"))
            .unwrap();
        app.graph.selected = all.nodes[index].id.clone();
        let visible = node_order(&app);
        assert_eq!(
            visible.len(),
            1,
            "{:?}",
            visible.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
        assert_eq!(visible[0].session_id.as_deref(), Some("lonely"));

        // Picking the gathering root itself opens the sessions under it.
        app.graph.selected = UNANCHORED_ID.into();
        assert_eq!(node_order(&app).len(), 3);
    }

    /// A `session --hosts --json` outcome: this box (skipped — its sessions
    /// are already the local forest), one unreachable node with a cached row
    /// and one live row, one never-pulled node.
    fn roster_fixture() -> aoide_protocol::output::Outcome {
        let data = serde_json::json!({
            "host": "osaka",
            "generatedAt": "2026-08-21T00:00:00Z",
            "nodes": [
                { "name": "osaka", "isLocal": true, "presence": "online", "fetchedAt": null,
                  "sessions": [
                      { "sessionId": "local", "label": "osaka/root/brave-otter (…ocal)",
                        "petname": "brave-otter", "agent": "claude", "state": "working",
                        "presence": "online", "cwd": "/x" } ] },
                { "name": "yomi-strix", "isLocal": false, "presence": "unreachable",
                  "fetchedAt": "2026-08-20T23:00:00Z", "error": "HTTP 000",
                  "sessions": [
                      { "sessionId": "far1", "label": "yomi-strix/root/misty-comet (…far1)",
                        "petname": "misty-comet", "agent": "codex", "state": "working",
                        "presence": "last-seen", "cwd": "/y" } ] },
                { "name": "sakaki", "isLocal": false, "presence": "online", "fetchedAt": null,
                  "sessions": [
                      { "sessionId": "far2", "label": "sakaki/child/calm-thorn (…far2)",
                        "petname": "calm-thorn", "agent": "shell", "state": "idle",
                        "presence": "online", "cwd": "/z" },
                      { "sessionId": "far3", "label": "sakaki/child/keen-fox (…far3)",
                        "petname": "keen-fox", "agent": "claude", "state": "working",
                        "presence": "online", "cwd": "/z", "parentSessionId": "far2" },
                      { "sessionId": "far4", "label": "sakaki/child/lost-elk (…far4)",
                        "petname": "lost-elk", "agent": "claude", "state": "idle",
                        "presence": "online", "cwd": "/z", "parentSessionId": "elsewhere" } ] },
                { "name": "ghost", "isLocal": false, "presence": "never-pulled",
                  "fetchedAt": null, "error": "could not reach the agent", "sessions": [] },
            ],
        });
        aoide_protocol::output::Outcome::ok("session", "4 node(s)").with_data(data)
    }

    #[test]
    fn registered_nodes_stand_as_host_cards_with_their_roster_rows_beneath() {
        let mut app = App::for_test(
            vec![],
            vec![session("local", "/x", "working", None)],
            Vec::new(),
        );
        app.roster.outcome = Some(roster_fixture());
        app.graph.view = View::All;
        app.sync_graph_scene();
        let m = build_model(&app);

        // This box is the local forest already; it gets no host card, and its
        // roster row is not a second card for the same session.
        assert!(m.nodes.iter().all(|n| n.id != "node:osaka"));
        assert_eq!(m.nodes.iter().filter(|n| n.session_id.as_deref() == Some("local")).count(), 1);

        let yomi = node(&m, "node:yomi-strix");
        assert_eq!(yomi.kind, NodeKind::Host);
        assert_eq!(yomi.depth, 0, "a node is a root of its own");
        assert!(
            yomi.title.starts_with("last seen ") && yomi.title.ends_with(" ago"),
            "the host card says it is cache, and how old: {}",
            yomi.title
        );
        let far1 = node(&m, "node:yomi-strix/session:far1");
        assert_eq!(far1.host.as_deref(), Some("yomi-strix"));
        assert_eq!(far1.depth, 1, "a remote session hangs off its node");
        assert_eq!(far1.state.as_deref(), Some("last-seen"), "a cached row's state is the cache's word");
        assert_eq!(far1.activity, "was working", "its last state rides as history");
        assert_eq!(far1.petname.as_deref(), Some("misty-comet"));

        let sakaki = node(&m, "node:sakaki");
        assert_eq!(sakaki.title, "3 session(s)");
        let far2 = node(&m, "node:sakaki/session:far2");
        assert_eq!(far2.state.as_deref(), Some("idle"), "a live row keeps its state");
        assert_eq!(far2.activity, "/z");
        assert_eq!(far2.harness, "shell");
        // A same-node spawner ranks its child beneath it; a parent the node
        // does not list leaves the row flat under the node.
        let far3 = node(&m, "node:sakaki/session:far3");
        assert_eq!(far3.depth, far2.depth + 1, "far3 hangs off far2");
        assert_eq!(m.children.get("node:sakaki/session:far2").map(Vec::len), Some(1));
        assert_eq!(node(&m, "node:sakaki/session:far4").depth, 1, "an unknown parent means flat");

        let ghost = node(&m, "node:ghost");
        assert_eq!(ghost.title, "");
        assert!(m.children.get("node:ghost").is_none(), "nothing hangs off a node never pulled");

        // Local forest first, nodes after — the gathering root precedes every host.
        let unanchored = m.nodes.iter().position(|n| n.id == UNANCHORED_ID).unwrap();
        assert!(unanchored < m.nodes.iter().position(|n| n.id == "node:yomi-strix").unwrap());

        // Painted: the host card wears the node mark and its name; the cached
        // row shows the cache's word and no live glyph next to it.
        let buf = paint(&app, Rect::new(0, 0, 320, 40));
        let out = dump(&buf);
        assert!(out.contains("🖧 NODE · unreachable"), "{out}");
        assert!(out.contains("yomi-strix"), "{out}");
        assert!(out.contains("AGENT · last-seen"), "{out}");
        assert!(out.contains("was working"), "{out}");
        assert!(out.contains("🖧 NODE · never-pull"), "{out}");
    }

    #[test]
    fn the_model_is_built_once_per_key_and_rebuilt_when_the_scene_or_the_forest_changes() {
        let mut app = App::for_test(vec![], vec![session("a", "/x", "working", None)], Vec::new());
        app.sync_graph_scene();
        let first = build_model(&app);
        assert!(Rc::ptr_eq(&first, &build_model(&app)), "same key, same build");

        app.graph.view = View::All;
        let second = build_model(&app);
        assert!(!Rc::ptr_eq(&first, &second), "a view change is a new key");
        assert!(Rc::ptr_eq(&second, &build_model(&app)));

        app.graph.selected = "session:a".into();
        assert!(!Rc::ptr_eq(&second, &build_model(&app)), "the selection is in the key");

        let before = build_model(&app);
        app.sessions.push(session("b", "/x", "idle", None));
        app.sync_graph_scene();
        let after = build_model(&app);
        assert!(!Rc::ptr_eq(&before, &after), "a sync clears the cache");
        assert_eq!(after.nodes.len(), before.nodes.len() + 1);
    }

    #[test]
    fn a_fold_hides_the_subtree_and_its_mark_counts_the_awaiting_child() {
        let mut app = App::for_test(
            vec![],
            vec![
                session("hub", "/x", "working", None),
                session("a", "/x", "awaiting", Some("hub")),
                session("b", "/x", "working", Some("hub")),
                session("b1", "/x", "idle", Some("b")),
                session("other", "/y", "idle", None),
            ],
            Vec::new(),
        );
        app.graph.view = View::All;
        app.sync_graph_scene();
        let total = node_order(&app).len();
        app.graph.selected = "session:hub".into();
        toggle_fold(&mut app);
        assert!(app.graph.folded.contains("session:hub"));
        let m = build_model(&app);
        assert_eq!(m.visible_len(), total - 3, "a, b and b1 fold away; hub and other stay");
        assert!(m.visible().any(|n| n.id == "session:hub"));
        assert!(m.visible().all(|n| n.id != "session:b1"));
        let hub = node(&m, "hub");
        assert_eq!(
            hub.folded,
            Some(Fold {
                hidden: 3,
                awaiting: 1,
                working: 1
            })
        );
        assert_eq!(hub.folded.unwrap().label(), "▸ 3 · 1 awaiting");
        let out = dump(&paint(&app, Rect::new(0, 0, 160, 30)));
        assert!(out.contains("▸ 3 · 1 awaiting"), "{out}");

        // The siblings across the rank still walk; a step toward the hidden
        // children unfolds the card instead of going nowhere.
        select_sibling(&mut app, true);
        assert_eq!(selected_session_id(&app).as_deref(), Some("other"));
        select_sibling(&mut app, false);
        select_child(&mut app);
        assert!(!app.graph.folded.contains("session:hub"), "j unfolds");
        assert_eq!(selected_session_id(&app).as_deref(), Some("a"));
        assert_eq!(build_model(&app).visible_len(), total);

        // A leaf has nothing to fold; `f` on it changes nothing.
        app.graph.selected = "session:a".into();
        toggle_fold(&mut app);
        assert!(app.graph.folded.is_empty());
        assert!(node(&build_model(&app), "a").folded.is_none());
    }

    #[test]
    fn a_ghost_resumed_edge_never_strands_the_keys() {
        // A resurrected session carries a `resumed` edge from the ledger
        // entry it was built from — a source that has, by construction, left
        // the roster. That ghost parent must never win the parent lookup.
        let app = App::for_test(vec![], vec![], Vec::new());
        let doc = serde_json::json!({
            "schemaVersion": "0",
            "nodes": [
                { "id": "session:root", "kind": "session", "state": "working", "agent": "claude" },
                { "id": "session:kid", "kind": "session", "state": "idle", "agent": "claude" },
                { "id": "session:sib", "kind": "session", "state": "idle", "agent": "claude" },
            ],
            "edges": [
                { "from": "session:ghost", "to": "session:kid", "kind": "resumed" },
                { "from": "session:root", "to": "session:kid", "kind": "spawned" },
                { "from": "session:root", "to": "session:sib", "kind": "spawned" },
                { "from": "session:ghost", "to": "session:sib", "kind": "resumed" },
            ],
        });
        let mut app = app;
        app.graph.view = View::All;
        let m = build_model_from(&app, &doc, &[], "h");
        assert!(m.nodes.iter().all(|n| n.id != "session:ghost"), "the ghost is not drawn");
        // Drive the real key paths over this document by pinning it as the
        // model's source: the helpers rebuild from the app, so the same edges
        // must come from the stage. Session records with parents reproduce
        // the spawned half; the resumed half rides `resumed_from`.
        let mut root = session("root", "/x", "working", None);
        root.resumed_from = None;
        let mut kid = session("kid", "/x", "idle", Some("root"));
        kid.resumed_from = Some("ghost".into());
        let mut sib = session("sib", "/x", "idle", Some("root"));
        sib.resumed_from = Some("ghost".into());
        let mut app = App::for_test(vec![], vec![root, kid, sib], Vec::new());
        app.graph.view = View::All;
        app.sync_graph_scene();
        let m = build_model(&app);
        assert!(
            m.children.get("session:ghost").is_some(),
            "the fixture really carries the ghost edge: {:?}",
            m.children.keys().collect::<Vec<_>>()
        );
        for _ in 0..8 {
            app.graph.selected = "session:kid".into();
            select_parent(&mut app);
            assert_eq!(selected_session_id(&app).as_deref(), Some("root"), "k climbs to the drawn parent");
            app.graph.selected = "session:kid".into();
            select_sibling(&mut app, true);
            assert_eq!(selected_session_id(&app).as_deref(), Some("sib"), "l reaches the drawn sibling");
        }
    }

    #[test]
    fn a_failed_probe_keeps_the_host_cards_dimmed_and_says_so() {
        let mut app = App::for_test(vec![], vec![session("local", "/x", "working", None)], Vec::new());
        app.absorb_roster_outcome(roster_fixture());
        app.graph.view = View::All;
        app.sync_graph_scene();
        assert!(!node(&build_model(&app), "node:sakaki").cached);

        app.absorb_roster_outcome(aoide_protocol::output::Outcome::error("session", "HTTP 000"));
        app.sync_graph_scene();
        let m = build_model(&app);
        let sakaki = node(&m, "node:sakaki");
        assert!(sakaki.cached, "every remote card is a cache once the probe fails");
        assert!(
            sakaki.harness.starts_with("probe failed ") && sakaki.harness.ends_with(" ago"),
            "{}",
            sakaki.harness
        );
        assert!(node(&m, "node:sakaki/session:far2").cached);
        let out = dump(&paint(&app, Rect::new(0, 0, 320, 40)));
        assert!(out.contains("probe failed"), "{out}");
    }

    #[test]
    fn a_far_parent_loop_falls_flat_under_its_node_and_keeps_its_children() {
        let data = serde_json::json!({
            "host": "osaka", "generatedAt": "t",
            "nodes": [
                { "name": "osaka", "isLocal": true, "presence": "online", "fetchedAt": null, "sessions": [] },
                { "name": "far", "isLocal": false, "presence": "online", "fetchedAt": null,
                  "sessions": [
                      { "sessionId": "a", "label": "far/child/a (…a)", "petname": "a-a", "agent": "claude",
                        "state": "awaiting", "presence": "online", "cwd": "/", "parentSessionId": "b" },
                      { "sessionId": "b", "label": "far/child/b (…b)", "petname": "b-b", "agent": "claude",
                        "state": "idle", "presence": "online", "cwd": "/", "parentSessionId": "a" },
                      { "sessionId": "c", "label": "far/child/c (…c)", "petname": "c-c", "agent": "claude",
                        "state": "working", "presence": "online", "cwd": "/", "parentSessionId": "a" },
                      { "sessionId": "d", "label": "far/root/d (…d)", "petname": "d-d", "agent": "claude",
                        "state": "idle", "presence": "online", "cwd": "/" } ] },
            ],
        });
        let mut app = App::for_test(vec![], vec![], Vec::new());
        app.roster.outcome = Some(aoide_protocol::output::Outcome::ok("session", "x").with_data(data));
        app.graph.view = View::All;
        app.sync_graph_scene();
        let m = build_model(&app);
        let host = node(&m, "node:far");
        assert_eq!(host.title, "4 session(s)");
        for id in ["a", "b", "c", "d"] {
            assert!(m.visible().any(|n| n.id == format!("node:far/session:{id}")), "{id} is a card");
        }
        let a = node(&m, "node:far/session:a");
        assert_eq!(a.depth, 1, "the loop's first row falls flat under the node");
        assert_eq!(node(&m, "node:far/session:b").depth, 2, "its partner hangs beneath it");
        assert_eq!(node(&m, "node:far/session:c").depth, 2, "and so does its real child");
        assert_eq!(node(&m, "node:far/session:d").depth, 1);
        // Folding the node still counts every row, awaiting included.
        app.graph.selected = "node:far".into();
        toggle_fold(&mut app);
        assert_eq!(
            node(&build_model(&app), "node:far").folded,
            Some(Fold { hidden: 4, awaiting: 1, working: 1 })
        );
    }

    #[test]
    fn a_fold_label_keeps_the_awaiting_count_when_the_border_is_short() {
        let fold = Fold { hidden: 12, awaiting: 3, working: 4 };
        assert_eq!(fold.label_fitting(30), "▸ 12 · 3 awaiting");
        assert_eq!(fold.label_fitting(10), "▸12·3!");
        assert_eq!(fold.label_fitting(3), "3!");
        let calm = Fold { hidden: 5, awaiting: 0, working: 2 };
        assert_eq!(calm.label_fitting(8), "▸5");
    }

    #[test]
    fn picking_a_card_opens_every_fold_above_it() {
        let mut app = App::for_test(
            vec![],
            vec![
                session("hub", "/x", "working", None),
                session("mid", "/x", "idle", Some("hub")),
                session("leaf", "/x", "idle", Some("mid")),
            ],
            Vec::new(),
        );
        app.graph.view = View::All;
        app.sync_graph_scene();
        app.graph.folded.insert("session:hub".into());
        app.graph.folded.insert("session:mid".into());
        assert!(build_model(&app).visible().all(|n| n.id != "session:leaf"));
        let model = build_model(&app);
        pick(&mut app, &model, "session:leaf".into());
        assert!(app.graph.folded.is_empty(), "both folds above the leaf opened");
        assert_eq!(selected_session_id(&app).as_deref(), Some("leaf"));
        assert!(build_model(&app).visible().any(|n| n.id == "session:leaf"));
    }

    #[test]
    fn a_far_petname_that_is_no_mailbox_name_gets_no_letter() {
        let data = serde_json::json!({
            "host": "osaka", "generatedAt": "t",
            "nodes": [
                { "name": "osaka", "isLocal": true, "presence": "online", "fetchedAt": null, "sessions": [] },
                { "name": "far", "isLocal": false, "presence": "online", "fetchedAt": null,
                  "sessions": [
                      { "sessionId": "x", "label": "far/root/x (…x)", "petname": "x,other/y", "agent": "claude",
                        "state": "idle", "presence": "online", "cwd": "/" } ] },
            ],
        });
        let mut app = App::for_test(vec![], vec![], Vec::new());
        app.roster.outcome = Some(aoide_protocol::output::Outcome::ok("session", "x").with_data(data));
        app.graph.view = View::All;
        app.sync_graph_scene();
        app.panel = crate::app::Panel::Graph;
        app.sidebar_focused = false;
        app.graph.selected = "node:far/session:x".into();
        crate::handle_key(&mut app, crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('s')));
        assert!(app.mail_draft.is_none(), "a comma or slash in a petname would fan the letter out");
        assert!(app.last_outcome.is_some());
        crate::handle_key(&mut app, crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('e')));
        assert_eq!(app.context_menu.as_ref().unwrap().actions, vec![crate::app::ContextAction::Details]);
        assert!(crate::app::is_mailbox_name("misty-comet") && !crate::app::is_mailbox_name("Misty") && !crate::app::is_mailbox_name(""));
    }

    #[test]
    fn a_remote_card_writes_to_its_node_mailbox_and_shows_details_in_mesh() {
        let mut app = App::for_test(vec![], vec![], Vec::new());
        app.roster.outcome = Some(roster_fixture());
        app.graph.view = View::All;
        app.sync_graph_scene();
        app.panel = crate::app::Panel::Graph;
        app.sidebar_focused = false;
        app.graph.selected = "node:yomi-strix/session:far1".into();
        let card = selected_node(&app).unwrap();
        assert_eq!(card.remote_id.as_deref(), Some("far1"));

        crate::handle_key(&mut app, crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('s')));
        assert_eq!(
            app.mail_draft.as_ref().map(|d| d.to.clone()).as_deref(),
            Some("yomi-strix/misty-comet"),
            "s addresses the far agent's own mailbox"
        );
        app.mail_draft = None;

        crate::handle_key(&mut app, crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('e')));
        let menu = app.context_menu.as_ref().expect("the card has a menu");
        assert_eq!(
            menu.actions,
            vec![crate::app::ContextAction::Details, crate::app::ContextAction::WriteLetter]
        );
        app.run_context_action(0);
        assert_eq!(app.panel, crate::app::Panel::Roster, "Details is the Mesh row");
        assert!(matches!(
            app.roster_flat_rows().get(app.roster_sel),
            Some(crate::app::RosterRow::Session { session, .. }) if session.session_id == "far1"
        ));

        // A far terminal has no mailbox: only Details, and s refuses.
        app.panel = crate::app::Panel::Graph;
        app.graph.selected = "node:sakaki/session:far2".into();
        crate::handle_key(&mut app, crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('s')));
        assert!(app.mail_draft.is_none());
        crate::handle_key(&mut app, crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Char('e')));
        assert_eq!(app.context_menu.as_ref().unwrap().actions, vec![crate::app::ContextAction::Details]);
    }

    #[test]
    fn a_remote_session_resolves_no_local_record() {
        // A far session id must never reach `merged()`: Enter, the letter
        // key and the context menu all look a card up there, and finding
        // nothing is what keeps every local action on this box's own rows.
        let mut app = App::for_test(
            vec![],
            vec![session("far1", "/x", "working", None)],
            Vec::new(),
        );
        app.roster.outcome = Some(roster_fixture());
        app.graph.view = View::All;
        app.sync_graph_scene();
        let m = build_model(&app);
        let remote = m
            .nodes
            .iter()
            .find(|n| n.id == "node:yomi-strix/session:far1")
            .unwrap();
        let local = m.nodes.iter().find(|n| n.id == "session:far1").unwrap();
        assert_eq!(local.session_id.as_deref(), Some("far1"));
        assert_eq!(remote.session_id, None, "a far card carries no id a local lookup could hit");
        assert!(remote.host.is_some() && local.host.is_none());
        assert!(remote.world != local.world, "distinct cards, distinct retained positions");

        // Selecting the far card yields no session for Enter or `s` to act on,
        // while its label still shows the far tail.
        app.graph.selected = remote.id.clone();
        assert_eq!(selected_session_id(&app), None);
        let out = dump(&paint(&app, Rect::new(0, 0, 200, 40)));
        assert!(out.contains("yomi-strix") && out.contains("(…far1)"), "{out}");
    }

    #[test]
    fn focus_navigation_walks_the_forest_and_the_view_follows() {
        // Two loose roots, one with a child. Under Focus the gathering root
        // is not a connection, so once the cursor is on `a` the view draws
        // `a` and `a1` alone — but the KEYS must still reach the parent and
        // the sibling the view hid, or the cursor is stranded after one `j`.
        let mut app = App::for_test(
            vec![],
            vec![
                session("a", "/tmp/a", "working", None),
                session("a1", "/tmp/a", "idle", Some("a")),
                session("b", "/tmp/b", "idle", None),
            ],
            Vec::new(),
        );
        app.sync_graph_scene();
        assert_eq!(app.graph.view, View::Focus);
        assert_eq!(app.graph.selected, UNANCHORED_ID);
        let ids = |app: &App| -> Vec<String> {
            node_order(app)
                .iter()
                .map(|n| n.session_id.clone().unwrap_or_else(|| n.id.clone()))
                .collect()
        };

        select_child(&mut app);
        assert_eq!(selected_session_id(&app).as_deref(), Some("a"));
        assert_eq!(ids(&app), vec!["a", "a1"], "focus narrows to a's component");

        select_parent(&mut app);
        assert_eq!(app.graph.selected, UNANCHORED_ID, "k climbs back to the gathering root");
        assert_eq!(ids(&app).len(), 4, "the root opens the whole forest again");

        select_child(&mut app);
        select_sibling(&mut app, true);
        assert_eq!(selected_session_id(&app).as_deref(), Some("b"), "l reaches the hidden sibling");
        assert_eq!(ids(&app), vec!["b"], "and the view re-forms around it");
        select_sibling(&mut app, true);
        assert_eq!(selected_session_id(&app).as_deref(), Some("b"), "no wrap at the rank's end");

        select_sibling(&mut app, false);
        assert_eq!(selected_session_id(&app).as_deref(), Some("a"));

        // A root's siblings are the other roots: the gathering root steps
        // across to a host card and back, so the keyboard reaches every
        // forest the canvas holds.
        app.roster.outcome = Some(roster_fixture());
        app.sync_graph_scene();
        select_parent(&mut app);
        assert_eq!(app.graph.selected, UNANCHORED_ID);
        select_sibling(&mut app, true);
        assert_eq!(app.graph.selected, "node:yomi-strix", "l on a root reaches the next root");
        assert_eq!(ids(&app), vec!["node:yomi-strix", "node:yomi-strix/session:far1"]);
        select_sibling(&mut app, false);
        assert_eq!(app.graph.selected, UNANCHORED_ID);
        select_sibling(&mut app, false);
        assert_eq!(app.graph.selected, UNANCHORED_ID, "the first root has no previous");
        app.roster.outcome = None;
        app.sync_graph_scene();

        select_child(&mut app);
        assert_eq!(selected_session_id(&app).as_deref(), Some("a"));
        select_child(&mut app);
        assert_eq!(selected_session_id(&app).as_deref(), Some("a1"));
        select_child(&mut app);
        assert_eq!(selected_session_id(&app).as_deref(), Some("a1"), "a leaf has no child");
        select_parent(&mut app);
        assert_eq!(selected_session_id(&app).as_deref(), Some("a"));
        assert!(app.graph.camera.pan.is_none(), "every step hands the camera back to the selection");
    }

    #[test]
    fn the_follow_camera_shows_the_forest_not_the_pad() {
        // A forest shorter than the pane starts at its own top row: the
        // canvas pad above the roots is for dragging past them, never the
        // opening picture.
        let mut app = App::for_test(
            vec![],
            vec![
                session("root", "/x", "working", None),
                session("child", "/x", "idle", Some("root")),
            ],
            Vec::new(),
        );
        app.graph.view = View::All;
        app.sync_graph_scene();
        // Taller than the three-rank forest (gathering root, root, child: 27
        // rows) but shorter than that forest plus its bottom pad — the pane a
        // padded-extent clamp would pull the roots back down in.
        let area = Rect::new(0, 0, 60, 50);
        let model = build_model(&app);
        let top = model.visible().map(|n| n.world.y).min().unwrap();
        let (left, right) = model
            .visible()
            .fold((i32::MAX, i32::MIN), |(l, r), n| (l.min(n.world.x), r.max(n.world.right())));
        for sel in 0..model.visible_len() {
            select_index(&mut app, sel);
            let o = origin(&build_model(&app), area);
            assert_eq!(o.1, top, "sel {sel}: the first rank sits on the pane's top row");
            assert!(o.0 <= left && right <= o.0 + 60, "sel {sel}: the forest is centred whole");
        }

        // A rank wider than the pane: the selected card is centred, but the
        // pane never slides past the forest's own edge into the pad.
        let mut wide: Vec<SessionRecord> = vec![session("hub", "/x", "working", None)];
        wide.extend((0..13).map(|i| session(&format!("w{i}"), "/x", "idle", Some("hub"))));
        let mut app = App::for_test(vec![], wide, Vec::new());
        app.graph.view = View::All;
        app.sync_graph_scene();
        let area = Rect::new(0, 0, 54, 14);
        let model = build_model(&app);
        let (x0, x1) = model
            .visible()
            .fold((i32::MAX, i32::MIN), |(l, r), n| (l.min(n.world.x), r.max(n.world.right())));
        let (y0, y1) = model
            .visible()
            .fold((i32::MAX, i32::MIN), |(l, r), n| (l.min(n.world.y), r.max(n.world.bottom())));
        for sel in 0..model.visible_len() {
            select_index(&mut app, sel);
            let m = build_model(&app);
            let o = origin(&m, area);
            let r = m.visible().nth(sel).unwrap().world;
            assert!(o.0 >= x0 && o.0 + 54 <= x1, "sel {sel}: pane inside the forest's width");
            assert!(o.1 >= y0 && o.1 + 14 <= y1, "sel {sel}: pane inside the forest's height");
            assert!(r.x >= o.0 && r.right() <= o.0 + 54, "sel {sel}: the selected card is on screen");
            assert!(r.y >= o.1 && r.bottom() <= o.1 + 14, "sel {sel}: the selected card is on screen");
        }
    }

    // ── Camera: one transform for render, hit test and pan ─────────────────

    #[test]
    fn the_camera_follows_the_selection_and_hit_tests_the_card_it_painted() {
        let mut app = App::for_test(
            vec![],
            vec![
                session("root", "/x", "working", None),
                session("child", "/x", "idle", Some("root")),
            ],
            Vec::new(),
        );
        app.graph.view = View::All;
        let area = Rect::new(3, 2, 44, 12);
        for sel in 0..build_model(&app).visible_len() {
            select_index(&mut app, sel);
            let model = build_model(&app);
            let o = origin(&model, area);
            let r = model
                .camera
                .scale_rect(model.visible().nth(sel).unwrap().world);
            assert!(r.x >= o.0 && r.right() <= o.0 + area.width as i32);
            assert!(r.y >= o.1 && r.bottom() <= o.1 + area.height as i32);
            for dy in 0..r.h {
                for dx in 0..r.w {
                    let (x, y) = (
                        (area.x as i32 + r.x - o.0 + dx) as u16,
                        (area.y as i32 + r.y - o.1 + dy) as u16,
                    );
                    assert_eq!(hit_node(area, &app, x, y), Some(sel), "cell {dx},{dy}");
                }
            }
        }
        select_index(&mut app, 0);
        assert_eq!(
            hit_node(area, &app, area.x, area.y + CARD_H as u16),
            None,
            "the lane gutter is not a card"
        );
    }

    #[test]
    fn a_manual_pan_overrides_the_follow_camera_and_moves_the_hit_map_with_it() {
        let mut app = App::for_test(
            vec![],
            vec![
                session("root", "/x", "working", None),
                session("child", "/x", "idle", Some("root")),
            ],
            Vec::new(),
        );
        app.graph.view = View::All;
        let area = Rect::new(3, 4, 32, 7);
        let model = build_model(&app);
        let child = model
            .visible()
            .position(|n| n.session_id.as_deref() == Some("child"))
            .unwrap();
        let r = model
            .camera
            .scale_rect(model.visible().nth(child).unwrap().world);
        select_index(&mut app, 0);
        app.graph.camera.pan = Some((r.x, r.y));
        assert_eq!(graph_origin(&app, area), (r.x, r.y));
        assert_eq!(hit_node(area, &app, area.x, area.y), Some(child));
        assert_eq!(
            hit_node(area, &app, area.x + r.w as u16 - 1, area.y + r.h as u16 - 1),
            Some(child)
        );
        // A pan past the far edge clamps onto the padded canvas, never past it.
        app.graph.camera.pan = Some((i32::MAX, i32::MAX));
        let (w, h) = graph_extent(&app);
        assert_eq!(
            graph_origin(&app, area),
            (w - area.width as i32, h - area.height as i32)
        );
        // Releasing the pan hands the camera back to the selection.
        app.graph.camera.pan = None;
        let model = build_model(&app);
        let o = origin(&model, area);
        let r = model.camera.scale_rect(model.visible().next().unwrap().world);
        assert_eq!(
            hit_node(area, &app, (area.x as i32 + r.x - o.0) as u16, (area.y as i32 + r.y - o.1) as u16),
            Some(0)
        );
    }

    #[test]
    fn zoom_keeps_the_pointer_over_the_same_card_and_never_relays_out_the_world() {
        let mut app = App::for_test(
            vec![],
            vec![
                session("root", "/x", "working", None),
                session("child", "/x", "idle", Some("root")),
            ],
            Vec::new(),
        );
        app.graph.view = View::All;
        let area = Rect::new(3, 4, 24, 8);
        app.graph.camera.pan = Some((44, 1));
        let pointer = (area.x + 4, area.y + 2);
        let before = hit_node(area, &app, pointer.0, pointer.1);
        let world: Vec<WorldRect> = build_model(&app).nodes.iter().map(|n| n.world).collect();

        zoom_at(&mut app, area, pointer, 1);
        assert_eq!(app.graph.camera.zoom, 1);
        assert_eq!(hit_node(area, &app, pointer.0, pointer.1), before);
        assert_eq!(
            build_model(&app)
                .nodes
                .iter()
                .map(|n| n.world)
                .collect::<Vec<_>>(),
            world,
            "zoom is a camera transform: the retained world never moves"
        );
        zoom_at(&mut app, area, pointer, 1);
        assert_eq!(zoom_label(&app), "150%");
        let pan = app.graph.camera.pan;
        zoom_at(&mut app, area, pointer, 1);
        assert_eq!(app.graph.camera.pan, pan, "the ladder stops at 150%");
        let zoom = app.graph.camera.zoom;
        zoom_at(&mut app, area, (0, 0), -1);
        assert_eq!(
            app.graph.camera.zoom, zoom,
            "a pointer outside the pane is not a zoom"
        );
    }

    #[test]
    fn card_rectangles_match_the_painted_cells_and_the_hit_map_at_every_scale() {
        let mut app = App::for_test(vec![], vec![session("root", "/x", "working", None)], vec![]);
        app.graph.view = View::All;
        let area = Rect::new(2, 3, 60, 20);
        for zoom in crate::scene::ZOOM_MIN..=crate::scene::ZOOM_MAX {
            app.graph.camera.zoom = zoom;
            select_index(&mut app, 1);
            let model = build_model(&app);
            let n = model.visible().nth(1).unwrap();
            assert_eq!(
                (n.world.w, n.world.h),
                (CARD_W, CARD_H),
                "cards never resize"
            );
            let r = model.camera.scale_rect(n.world);
            let cells = block_cells_at(n, true, &app.palette, r.w, r.h);
            assert_eq!(
                (cells.area().width as i32, cells.area().height as i32),
                (r.w, r.h)
            );
            assert!(grid_to_lines(&cells)
                .iter()
                .all(|line| line.width() as i32 == r.w));

            let o = origin(&model, area);
            let buf = paint(&app, area);
            // The painted corner is the selected card's own heavy border.
            let (sx, sy) = (
                (area.x as i32 + r.x - o.0) as u16,
                (area.y as i32 + r.y - o.1) as u16,
            );
            assert_eq!(buf[(sx, sy)].symbol(), "┏", "zoom {zoom}");
            for dy in 0..r.h {
                for dx in 0..r.w {
                    assert_eq!(
                        hit_node(area, &app, (sx as i32 + dx) as u16, (sy as i32 + dy) as u16),
                        Some(1)
                    );
                }
            }
        }
    }

    // ── Painting: bounded by the viewport, cards over edges ────────────────

    #[test]
    fn culling_keeps_the_world_outside_the_camera_out_of_the_buffer() {
        let mut sessions = vec![session("root", "/x", "working", None)];
        for i in 0..40 {
            sessions.push(session(&format!("kid{i}"), "/x", "idle", Some("root")));
        }
        let mut app = App::for_test(vec![], sessions, Vec::new());
        app.graph.view = View::All;
        app.sync_graph_scene();
        let model = build_model(&app);
        assert!(model.visible_len() > 40, "a forest larger than any pane");

        let area = Rect::new(0, 0, 40, 12);
        // Camera parked on the near pad: every card is below and right of it.
        app.graph.camera.pan = Some((0, 0));
        let buf = paint(&app, area);
        assert_eq!(
            painted(&buf),
            0,
            "an empty corner of the canvas paints nothing:\n{}",
            dump(&buf)
        );

        // Camera on the first card: the fortieth child is far off screen and
        // must not reach the buffer, however large the forest is.
        select_index(&mut app, 0);
        app.graph.camera.pan = None;
        let buf = paint(&app, area);
        let text = dump(&buf);
        assert!(painted(&buf) > 0, "the selected card is painted");
        let last = node(&model, "kid39");
        assert!(
            !text.contains(&last.label[..last.label.len().min(12)]),
            "an off-camera card stayed out of the buffer:\n{text}"
        );
        // Nothing painted outside the pane, at any camera position.
        assert!(painted(&buf) <= (area.width * area.height) as usize);
    }

    #[test]
    fn a_clipped_card_shows_a_crop_of_the_real_card_never_a_fabricated_border() {
        // `Block::bordered()` renders into the scratch buffer at the card's
        // real, unclipped size; clipping happens only in `blit_card`'s own
        // bounds check as it copies cells out. If clipping were instead done
        // by handing `Block` a pre-shrunk area, it would draw its OWN border
        // around whatever rectangle survived the clip -- a border that does
        // not exist on the real card. Proven here, for all four edges, by
        // requiring the clipped output to equal an exact crop of the
        // unclipped card: any fabricated glyph at the cut edge fails this.
        let app = App::for_test(vec![], vec![session("root", "/x", "working", None)], vec![]);
        let model = build_model(&app);
        let n = node(&model, "root");
        let card = block_cells(n, false, &app.palette);
        let (cw, ch) = (CARD_W as u16, CARD_H as u16);

        let cases: [(i32, i32, u16, u16); 4] = [
            (0, 1, cw, ch - 1), // clip the top border row
            (0, 0, cw, ch - 1), // clip the bottom border row
            (1, 0, cw - 1, ch), // clip the left border column
            (0, 0, cw - 1, ch), // clip the right border column
        ];
        for (ox, oy, w, h) in cases {
            let area = Rect::new(0, 0, w, h);
            let mut buf = Buffer::empty(area);
            let mut p = Painter::new(&mut buf, area);
            blit_card(&mut p, (ox, oy), 0, 0, &card);
            for y in 0..h {
                for x in 0..w {
                    let got = buf[(x, y)].symbol();
                    let want = card[((x as i32 + ox) as u16, (y as i32 + oy) as u16)].symbol();
                    assert_eq!(
                        got, want,
                        "clip offset ({ox},{oy}) at ({x},{y}): expected a crop of the \
                         real card, not a fabricated edge"
                    );
                }
            }
        }
    }

    #[test]
    fn a_card_fully_outside_the_viewport_paints_nothing() {
        let app = App::for_test(vec![], vec![session("root", "/x", "working", None)], vec![]);
        let model = build_model(&app);
        let n = node(&model, "root");
        let card = block_cells(n, false, &app.palette);
        let area = Rect::new(0, 0, 10, 10);

        for o in [(1000, 1000), (-1000, -1000)] {
            let mut buf = Buffer::empty(area);
            let mut p = Painter::new(&mut buf, area);
            blit_card(&mut p, o, 0, 0, &card);
            assert_eq!(
                painted(&buf),
                0,
                "a card entirely off camera costs a comparison, never a cell"
            );
        }
    }

    #[test]
    fn a_wide_glyph_straddling_the_seam_is_dropped_not_half_painted() {
        // `blit_card` must read each glyph's real display width off the
        // scratch buffer -- if it instead walked one buffer cell at a time
        // assuming width 1, a wide glyph's leading half would look like an
        // ordinary 1-wide glyph to `Painter::set`'s own straddle check and
        // get drawn alone, splitting it. Building a 1-cell viewport in front
        // of a 2-wide glyph forces exactly that choice.
        let mut card = Buffer::empty(Rect::new(0, 0, 4, 1));
        card.set_stringn(0, 0, "界", 4, Style::default());
        let area = Rect::new(0, 0, 1, 1);
        let mut buf = Buffer::empty(area);
        let mut p = Painter::new(&mut buf, area);
        blit_card(&mut p, (0, 0), 0, 0, &card);
        assert_eq!(
            buf[(0, 0)].symbol(),
            " ",
            "a wide glyph that would straddle the seam is dropped whole"
        );
    }

    #[test]
    fn a_card_paints_over_the_wire_that_crosses_it() {
        let app_base = App::for_test(
            vec![],
            vec![
                session("root", "/x", "working", None),
                session("left", "/x", "working", Some("root")),
                session("right", "/x", "idle", Some("root")),
            ],
            Vec::new(),
        );
        let mut app = app_base;
        app.graph.view = View::All;
        let base = build_model(&app);
        // Force a card onto the junction rank: the horizontal spreader between
        // `root` and its two children now runs straight through `right`'s
        // rectangle, so this proves the layering rather than assuming it.
        let ids: Vec<String> = base.nodes.iter().map(|n| n.id.clone()).collect();
        let mut placed: Vec<Placed> = base
            .nodes
            .iter()
            .map(|n| Placed {
                x: n.world.x,
                y: n.world.y,
                depth: n.depth,
            })
            .collect();
        let root = base
            .nodes
            .iter()
            .position(|n| n.id.ends_with("root"))
            .unwrap();
        let right = base
            .nodes
            .iter()
            .position(|n| n.id.ends_with("right"))
            .unwrap();
        let junction = placed[root].y + CARD_H + RANK_GAP / 2;
        placed[right].y = junction - CARD_H / 2;
        app.graph.positions.commit(ids, &placed);

        // A manual pan at the canvas origin, set before the model is built,
        // so `o` and the paint read the same camera.
        app.graph.camera.pan = Some((0, 0));
        let model = build_model(&app);
        let ext = graph_extent(&app);
        let area = Rect::new(0, 0, ext.0 as u16, ext.1 as u16);
        let o = origin(&model, area);
        let buf = paint(&app, area);
        let card = model.camera.scale_rect(node(&model, "right").world);
        assert!(
            (card.y..card.bottom()).contains(&junction),
            "the fixture really does park the card on the trunk"
        );
        // The card owns its interior: the trunk that runs through this rank
        // shows left and right of the card and nowhere inside it.
        for dy in 1..card.h - 1 {
            for dx in 1..card.w - 1 {
                let (x, y) = ((card.x - o.0 + dx) as u16, (card.y - o.1 + dy) as u16);
                let sym = buf[(x, y)].symbol();
                assert!(
                    !matches!(sym, "─" | "├" | "┼" | "┬"),
                    "a wire glyph survived inside the card at {dx},{dy}: {sym}\n{}",
                    dump(&buf)
                );
            }
        }
        // …and the trunk is not gone, only underneath: it still shows in the
        // same row band one column to the left of the card.
        let beside = (card.y..card.bottom())
            .filter(|y| buf[((card.x - o.0 - 1) as u16, (y - o.1) as u16)].symbol() == "─")
            .count();
        assert!(
            beside > 0,
            "the trunk still runs through this row band:\n{}",
            dump(&buf)
        );
    }

    #[test]
    fn branches_take_separate_slots_with_a_centred_parent_and_connected_ports() {
        let mut app = App::for_test(
            vec![],
            vec![
                session("root", "/x", "working", None),
                session("left", "/x", "working", Some("root")),
                session("right", "/x", "idle", Some("root")),
                session("leaf", "/x", "idle", Some("left")),
            ],
            Vec::new(),
        );
        app.graph.view = View::All;
        let model = build_model(&app);
        let (root, left, right, leaf) = (
            node(&model, "root"),
            node(&model, "left"),
            node(&model, "right"),
            node(&model, "leaf"),
        );
        assert_eq!(left.world.x, leaf.world.x);
        assert!(right.world.x >= left.world.x + SLOT);
        assert_eq!(root.world.x, (left.world.x + right.world.x) / 2);

        // At every scale a wire ends ABOVE the card it reaches and the card's
        // corners stay corners — no port glyph punched through a border
        // (the top border carries the title, so its centre cell is text).
        for zoom in crate::scene::ZOOM_MIN..=crate::scene::ZOOM_MAX {
            app.graph.camera.zoom = zoom;
            app.graph.camera.pan = Some((0, 0));
            let ext = graph_extent(&app);
            let area = Rect::new(0, 0, ext.0 as u16, ext.1 as u16);
            let model = build_model(&app);
            let buf = paint(&app, area);
            for n in model.visible() {
                let r = model.camera.scale_rect(n.world);
                let x = (r.x + r.w / 2) as u16;
                if n.depth > 0 {
                    // The drop, or — when the rank gap scales to one row —
                    // the junction itself, sits right above the top edge.
                    assert!(
                        matches!(buf[(x, (r.y - 1) as u16)].symbol(), "│" | "┌" | "┐" | "┬" | "┼" | "├" | "┤"),
                        "zoom {zoom}: the wire reaches the card's top edge, got {:?}",
                        buf[(x, (r.y - 1) as u16)].symbol()
                    );
                }
                for (cx, cy) in [(r.x, r.y), (r.right() - 1, r.y), (r.x, r.bottom() - 1), (r.right() - 1, r.bottom() - 1)] {
                    assert!(
                        matches!(buf[(cx as u16, cy as u16)].symbol(), "┌" | "┐" | "└" | "┘" | "┏" | "┓" | "┗" | "┛"),
                        "zoom {zoom}: corner at {cx},{cy} is {:?}",
                        buf[(cx as u16, cy as u16)].symbol()
                    );
                }
            }
        }
    }

    #[test]
    fn the_junction_sits_in_the_rank_gap_and_the_wire_reaches_both_cards() {
        let mut app = App::for_test(
            vec![],
            vec![
                session("root", "/x", "working", None),
                session("kid", "/x", "idle", Some("root")),
            ],
            Vec::new(),
        );
        app.graph.view = View::All;
        app.graph.camera.pan = Some((0, 0));
        let model = build_model(&app);
        let ext = graph_extent(&app);
        let area = Rect::new(0, 0, ext.0 as u16, ext.1 as u16);
        let buf = paint(&app, area);
        let root = node(&model, "root");
        let junction = (root.world.y + CARD_H + RANK_GAP / 2) as u16;
        let port = (root.world.x + CARD_W / 2) as u16;
        assert_eq!(buf[(port, junction)].symbol(), "│");
        assert_eq!(
            buf[(port, (root.world.y + CARD_H) as u16)].symbol(),
            "│",
            "the wire leaves the parent's bottom edge"
        );
        assert_eq!(
            buf[(port, (node(&model, "kid").world.y - 1) as u16)].symbol(),
            "│",
            "and reaches the child's top edge"
        );
    }

    #[test]
    fn a_narrow_pane_still_paints_the_selected_card() {
        let mut app = App::for_test(
            vec![aoide()],
            vec![
                session("root", "/home/k/Aoide", "working", None),
                session("kid", "/home/k/Aoide", "idle", Some("root")),
            ],
            Vec::new(),
        );
        app.sync_graph_scene();
        // 80x24 and 120x40 are the documented floors; the last two are the
        // degenerate panes a fold or a split can produce.
        for (w, h) in [(80, 24), (120, 40), (20, 6), (4, 2)] {
            for zoom in crate::scene::ZOOM_MIN..=crate::scene::ZOOM_MAX {
                app.graph.camera.zoom = zoom;
                for sel in 0..node_order(&app).len() {
                    select_index(&mut app, sel);
                    let area = Rect::new(0, 0, w, h);
                    let buf = paint(&app, area);
                    assert!(
                        painted(&buf) > 0,
                        "{w}x{h} zoom {zoom} sel {sel} painted nothing"
                    );
                    // Hit testing agrees with what was painted, narrow or not.
                    assert_eq!(
                        hit_node(area, &app, area.x, area.y).is_some(),
                        buf[(0, 0)].symbol() != " ",
                        "{w}x{h} zoom {zoom}: hit map and paint disagree at the corner"
                    );
                }
            }
        }
    }

    // ── Card content ──────────────────────────────────────────────────────

    #[test]
    fn blocks_preserve_metadata_and_palette_contrast() {
        let mut parent = session("root", "/x", "working", None);
        parent.title = Some("Distinct task title".into());
        parent.petname = Some("brave-otter".into());
        parent.model = Some("model-one".into());
        let app = App::for_test(
            vec![],
            vec![parent, session("child", "/x", "idle", Some("root"))],
            vec![],
        );
        let model = build_model(&app);
        let root = node(&model, "root");
        assert_eq!(root.world.x, node(&model, "child").world.x);
        for (bg, fg) in [(0, 15), (15, 0)] {
            let pal = crate::app::Palette {
                bg: Some(bg),
                fg: Some(fg),
                accent: Some(3),
                urgent: Some(1),
                ..Default::default()
            };
            let cells = block_cells(root, true, &pal);
            let text = buffer_rows(&cells).concat();
            assert!(
                text.contains("Distinct task title")
                    && text.contains(" (…root)")
                    && text.contains("claude · model-one")
            );
            assert_eq!(cells[(0, 0)].symbol(), "┏");
            // `surface`'s foreground stays constant across layers -- only the
            // background is layer-mixed -- so layer 0 is as good a probe as
            // the card's real layer for the (layer-independent) fg value.
            assert_eq!(cells[(2, 1)].fg, theme::surface(&pal, 0).fg.unwrap());
            assert_ne!(cells[(2, 1)].bg, Color::Reset);
        }
        let mut wide = root.clone();
        wide.title = "界".repeat(50);
        let lines = grid_to_lines(&block_cells(&wide, false, &app.palette));
        assert!(lines.iter().all(|l| l.width() as i32 == CARD_W));
    }

    #[test]
    fn block_keeps_state_and_identity_on_separate_bounded_lines() {
        // Send-back regression (P3 review): post-P2 every session carries a
        // minted petname, so a REAL row's label is `<host>/<role>/<petname>
        // (…<tail4>)` — routinely 30+ chars on a real box, wider than the
        // whole pre-P3 chip. This fixture deliberately does NOT shrink the
        // host, petname, or session id — it drives the actual overflow path
        // `fit_label` exists for, not a fixture engineered to dodge it.
        let mut root = session(
            "sess-realistically-long-canonical-id-0001",
            "/home/k/Aoide",
            "working",
            None,
        );
        root.petname = Some("hardy-harbor".into()); // wordlist-shaped (petname.rs).
        root.extra
            .insert("tags".into(), serde_json::json!(["backend"]));
        let app = App::for_test(vec![aoide()], vec![root], Vec::new());
        let m = build_model_on(&app, "a-typical-hostname");
        let node = node(&m, "sess-realistically-long-canonical-id-0001");
        assert!(
            node.label.chars().count() as i32 > CARD_W,
            "fixture must exercise the overflow path: {} chars vs CARD_W={CARD_W}",
            node.label.chars().count()
        );

        let block = block_cells(node, false, &app.palette);
        let rendered = buffer_rows(&block).concat();
        assert!(
            rendered.contains("working"),
            "state chip survives: {rendered:?}"
        );
        assert!(
            rendered.contains(" (…"),
            "tail4 handle survives: {rendered:?}"
        );
        assert!(block.area().width as i32 <= CARD_W, "the row fits the card");
    }

    #[test]
    fn fit_label_ladder_preserves_the_tail_longest_and_degrades_in_order() {
        let label = "yomi-strix/child/hardy-harbor (…ab12)";
        assert_eq!(fit_label(label, 100), label);
        // Rung 1: host/role/ + middle-elided name + tail all present.
        let r1 = fit_label(label, 30);
        assert!(
            r1.starts_with("yomi-strix/child/"),
            "rung 1 keeps host/role/: {r1}"
        );
        assert!(r1.ends_with(" (…ab12)"), "rung 1 keeps the tail: {r1}");
        // Rung 2: budget too small for host — role/name/tail only.
        let r2 = fit_label(label, 18);
        assert!(!r2.contains("yomi-strix"), "rung 2 drops the host: {r2}");
        assert!(r2.starts_with("child/"), "rung 2 keeps role/: {r2}");
        assert!(r2.ends_with(" (…ab12)"), "rung 2 keeps the tail: {r2}");
        let tiny = fit_label(label, 8);
        assert!(
            tiny.ends_with(" (…ab12)"),
            "tail survives an 8-cell budget: {tiny}"
        );
        let tinier = fit_label(label, 5);
        assert_eq!(tinier.chars().count(), 5);
        assert!(
            tinier.contains("ab12") || tinier.contains('…'),
            "even a 5-cell budget keeps SOME fragment of the tail or an ellipsis: {tinier}"
        );
    }

    #[test]
    fn a_session_card_carries_its_widget_fields_and_resolves_the_action_target() {
        let mut rec = session("canonical-id", "/x", "working", None);
        rec.title = Some("Review conductor".into());
        rec.petname = Some("calm-rook".into());
        rec.model = Some("fable".into());
        rec.tool = Some("Read".into());
        rec.activity = Some("Inspect graph".into());
        let mut app = App::for_test(vec![], vec![rec], vec![]);
        app.graph.view = View::All;
        select_index(&mut app, 1);
        let model = build_model(&app);
        let cells = block_cells(model.visible().nth(1).unwrap(), true, &app.palette);
        let rows = buffer_rows(&cells);
        let line = |y: usize| rows[y].clone();
        // 24×5: the title rides the top border, then identity, state and
        // activity; harness and model leave a card that has an activity.
        assert!(line(0).contains("Review conductor"), "{rows:?}");
        assert!(line(1).contains("calm-rook"), "{rows:?}");
        assert!(line(2).contains("working"), "{rows:?}");
        assert!(line(3).contains("Read · Inspect graph"), "{rows:?}");
        assert!(!rows.concat().contains("claude · fable"), "{rows:?}");
        assert_eq!(selected_session_id(&app).as_deref(), Some("canonical-id"));

        let area = Rect::new(0, 0, 60, 20);
        let o = origin(&model, area);
        let r = model
            .camera
            .scale_rect(model.visible().nth(1).unwrap().world);
        assert_eq!(
            session_id_at(area, &app, (r.x - o.0) as u16, (r.y - o.1) as u16).as_deref(),
            Some("canonical-id")
        );
        select_index(&mut app, 0);
        assert!(selected_session_id(&app).is_none());
    }

    #[test]
    fn a_titleless_card_prints_its_harness_only_once() {
        // Regression: an empty title used to fall back to printing the
        // harness on its own row, which the detail row ("harness · model",
        // or the bare harness with no model) already carried -- so a
        // titleless card showed its harness twice. Dropping empty rows and
        // compacting the rest means the harness now appears on exactly one
        // row: the detail row.
        let rec = session("r", "/x", "working", None); // title stays None
        let app = App::for_test(vec![], vec![rec], vec![]);
        let model = build_model(&app);
        let cells = block_cells(node(&model, "r"), false, &app.palette);
        let text = buffer_rows(&cells).concat();
        assert_eq!(
            text.matches("claude").count(),
            1,
            "harness appears exactly once on a titleless card: {text:?}"
        );
    }
}
