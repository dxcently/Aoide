# Conductor review — why `aoide conductor` goes unused, and what makes it worth opening

The conductor is the terminal workspace (`pkgs/aoide/crates/conductor`): ten
tabs over the stage, the mail base, the audit log, the roster probe and the
secrets broker, plus a retained-scene Graph of projects and sessions. This
review measures it against the one job the User has for it — conducting
agents across osaka, sakaki and yomi from a shell — and names, with evidence,
where it falls short. The register's standing rulings bound it: §22 phase 2
(persisted runs, work nodes, typed edges, goals) is frozen behind six open
questions, and §23 (capability plugins, `GraphScene` as a stateful widget)
has five; nothing below builds past either. Improvements are ranked and
tagged **do now** (no ruling needed) or **needs ruling** (the question is
named and an answer recommended).

## What the conductor does today

```
 𝄞 CONDUCTOR                              [◆ 0 Projects] [♜ 13 Agents] [▣ 5 Terminals]
 1 ⌂ Home 2 ✉ Mail 3 ♜ Agents 4 ▣ Terminals 5 🖧 Mesh 6 ⚑ Review 7 ◆ Projects 8 ∴ Graph 9 ≡ Log 0 ⚙ Status
 ┌ PROJECTS (30 cols) ─┬ body ───────────────────────────────────────────────┐
 │ project tree        │ one panel at a time                                 │
 │   Agents (n)        │   Graph: retained scene, 32×7 cards, j/k/h/l, a, p  │
 │   Terminals (n)     │   Mesh: `session --hosts` roster + node trust       │
 │   Past (n)          │   Review: pending sends · pairing · TOTP asks       │
 └─────────────────────┴─────────────────────────────────────────────────────┘
 hint line · status line
```

| Panel | Reads | Mutates (through `DispatchFn`) |
|---|---|---|
| Home | recent projects | nothing (navigation) |
| Mail | mail base, bounded tail | `mail send` (human composer) |
| Agents / Terminals | stage + ledger | `send`, `project add/remove`, focus jump |
| Mesh | `session --hosts`, `node status`, `mesh` | `node allow/remove`, `pair` legs |
| Review | `session pending list`, `pair`, `secrets pending` | approve/deny, pair approve/reject, TOTP |
| Projects | project registry | `project add/remove`, lead |
| Graph | `build_graph` over the local stage | `session prune` |
| Log | audit JSONL, bounded tail | nothing |
| Status | `config`, `secrets status` | `config set`, `secrets grant/revoke` |

Every panel is a view over an existing command's output; the crate adds no
store and no policy. That part is right.

## What a user conducting three hosts needs

Conducting (AGENTS.md) is: see every session on every node, see what each
is doing and whether it is stuck, send text or a letter to one, and know
when a row is a guess rather than a fact. In order of how often the User
does it, judging by the audit log: `send` (450 records), `session trace`
(820), roster reads. None of those three is faster in the conductor than in
the shell today, and the one picture the shell cannot draw — a graph of who
spawned whom across nodes — the conductor draws only for the local box.

## Why it is not used — evidence

**1. It is launched six times in 1.35 million audit records.** `~/.aoide/log`
holds 6 `conductor` launches (4 in September, 2 in October) against 450
`send` and 820 `session trace` invocations. The crate received 88 commits in
the same window. The work went into the tool; the use did not follow. The
count is of `conductor` records; the roster probe the Mesh pane repeats
every fifteen seconds used to dispatch `session` through the audited door,
so a `session` count would have been inflated by every open pane — the
probe is now a library call and writes nothing.

**2. The graph opens on a blank half-screen.** At 160×48 the default Graph
view selects the synthetic `Active sessions` root and centres it, so the
top 15 rows of the canvas are empty pad and only one rank of cards is
visible below it (capture: 15 blank rows, the group card, then three cards
of thirteen). Cause: `CANVAS_PAD` was folded into the extent the follow-camera clamped
against (`graphview::origin`), so the pad was shown whenever the forest was
shorter than the pane. At 80×24 exactly one card fits (capture below).

```
┌ GRAPH ───────────────────────────────────────────────┐
│                                                      │
│           ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓           │
│           ┃ ◆ GROUP                      ┃           │
│           ┃ Active sessions              ┃           │
│           ┃ Sessions without a project   ┃           │
│           ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛           │
│                           │                          │
│────────┬──────────────────┼──────────────────┬───────│
│[All / focus]  [Actions]      Canvas 100% · FOCUS | a │
```

**3. Focus is a trap.** Focus (the default) draws the selected card's
connected component, and the synthetic root "is not a connection"
(`graphview::visible_order`, `bridged`). After one `j` from that root the
selection sat on a session whose parent and siblings were no longer in
`visible`; `k` looked for a shallower node in the visible preorder and found
none (`App::handle_graph_key`), `h`/`l` refused a sibling the view did not
draw (`graphview::select_sibling`). Captured: from `fond-aspen`, `k` and `l`×4 leave the
screen unchanged. The only way out is `a` (All). Since this box has zero
registered projects, every session is under the synthetic root, so the
trap is the normal case here, not an edge.

**4. The graph shows the local box only.** `build_graph` folds every
registered node into the document as a `node:<name>` root, with the node's
own cached graph nested under `children` when the cache is fresh
(`conduct::graph::doc::build_graph`). `graphview::build_model` dropped them,
so yomi's 19 sessions and sakaki never appeared on the canvas. The one cross-host picture is the Mesh list, which is
an indented roster, not a graph.

**5. Cache rows are drawn as live.** The roster core already classifies a
cached session under an unreachable node as `last-seen`/`unknown`
(`conduct::graph::who::cached_presence`), but the conductor parsed only
`label` and `state` off each session row (`App::roster_nodes`) and rendered
`state` with the live glyph (`ui::roster_row_item`): under
`◐ yomi-strix — unreachable (last seen 2026-08-20T23:00:00Z)` a row still
reads `♪ … working`. The header carries a raw ISO stamp, not an age.

**6. The sidebar tree beats the graph at its own job.** The project tree
(30 columns) shows all 18 sessions with lineage indents and titles; the
graph shows three. A 32×7 card spends five rows on title, petname, harness
and model, role and state, and activity — most of which the tree row also
carries. Siblings sit 38 cells apart (`graphview::SLOT`), so a rank of 13
is 494 cells wide against a 126-cell pane.

**7. The sidebar groups by kind and breaks lineage.** `aoide graph` prints
`warm-birch (shell) → glossy-antler (claude) → three shells`; the sidebar
files `glossy-antler` at the root of Agents and the three shells flat under
Terminals, because it groups by kind before lineage (`board::draw_tree`'s
Agents/Terminals groups).

**8. The wiki page describes a different program.** `concepts/cli/
Conductor-TUI.md` documents seven panels (DAG, SESSION, PROJECTS, LOG,
STATUS, ROSTER, PENDING) and keys `1`–`7`; the binary has ten tabs and keys
`1`–`9`, `0`. An agent reading tier 0 is taught keys that do not exist.

**9. Every interaction rebuilds the whole model.** `node_order`,
`selected_index`, `select_index`, `select_sibling`, `hit_node`,
`graph_extent` and `graph_origin` each call `build_model`, which runs
`build_graph` (which reads `nodes.json` and every node's cache file) and
reshapes the roster outcome — so one `j` costs three to five document
builds. Register §23 already recorded
this ("rebuilds the whole-world grid three times per interaction").

## The graph feature's gaps, specifically

| Gap | Where | Ruling needed? |
|---|---|---|
| Blank pad at the default view; one card at 80×24 | `graphview::origin` | no — done |
| Focus traps the cursor; `k`/`h`/`l` dead after one `j` | `graphview::visible_order`, `App::handle_graph_key` | no — done |
| Remote nodes and their sessions absent | `graphview::build_model_from` | no — done, ranked by `parentSessionId` |
| No readout of where the cursor is in the forest (n of N, rank) | `board::draw_actions` | no — done |
| Card too large for the information it carries | `graphview::CARD_W`/`CARD_H` | ruled — done, 24×5 |
| Model rebuilt per call | `graphview::build_model` | ruled — done, memoised on `App` |
| No typed edges, runs, goals | — | frozen (§22 phase 2) |
| Mail overlay on the graph (§24 trails) | — | folded into the §22 architect |

## Ranked improvements

### Done

1. **Selection walks the forest; the view follows.** `j`/`k`/`h`/`l` resolve
   parent, child and siblings against the whole forest — a root's siblings
   are the other roots — and Focus re-forms around the landing card. `k` on
   a loose session climbs to the gathering root; `l` on it reaches the
   first host card. No key is a no-op while a card stands in that direction.
2. **The follow-camera clamps to the forest, not the pad.** A forest that
   fits the pane opens at its own top row, centred across; an overflowing
   axis centres the selected card inside the forest's bounds. The pad stays
   for drags and zoom.
3. **Registered nodes are host cards.** One `🖧 NODE` root per node off the
   roster probe, its reported sessions beneath, ranked under a same-node
   spawner when the row carries `parentSessionId` (an additive key on the
   `session --hosts --json` row) and flat otherwise; the probe keeps ticking
   while Graph is open and a landed probe folds into the retained scene.
4. **Cache rows say they are cache.** Mesh rows carry the backend's
   `presence`: a `last-seen`/`unknown` row renders muted as `· <label>
   last-seen · was <state>`, and the node header says `last seen 2h21m ago`
   instead of a raw stamp. The same word is the graph card's state, and the
   card is muted like the row. A probe that fails after a good one — or an
   Ok reply with no `nodes` — keeps the last rows, muted, each host card's
   last row reading `probe failed <age>`, the Mesh fetch line naming the
   refusal; the canvas never empties in silence. The tick's probe is a
   library call; the manual `r` goes through the audited dispatcher.
5. **Canvas readout.** `Canvas 100% · FOCUS · card 3/20 · rank 1`.
6. **`Conductor-TUI.md`** describes the ten-tab program and its keys.
7. **Compact cards.** One 24×5 preset, two-row rank gap: the title in the
   top border, identity, role·state and activity in the rows; harness and
   model live in the tree row and Details. A rank of 13 is 364 cells.
8. **Per-card fold.** `f` folds a card's children as a view choice in
   `SceneState` keyed by node id; the bottom border reads `▸ n · k
   awaiting` in the awaiting colour, so a blocked agent is never hidden
   silently; `j` on a folded card unfolds it.
9. **One model build per key.** `build_model` is memoised on `App`
   (`ModelKey`: view, selection, folds, camera, the retained world's
   generation, the failed-probe age) and cleared by `sync_graph_scene`.
10. **Lineage-first sidebar.** A row files under Agents when it or any
    ancestor is an agent, so a spawned terminal nests under its agent;
    Terminals holds all-shell chains; a wrapper shell stays there while the
    agent it wraps heads its own tree. The header counts stay by kind.
11. **Remote card actions.** `s` on a far agent opens the composer on
    `node/petname` — a new path into the same gated `mail send`, refused for
    a petname that is not a legal mailbox name; `e` offers Details (its Mesh
    row) and Write letter; nothing else crosses the node line.
12. **The start page sits in the middle.** `board::home_layout` centres the
    column across the body and the block down it at any size, dropping the
    logo before a box would clip.

### Next, no ruling needed

13. **Root spacing.** `graphview::place` advances a full extra slot after
    each root, so forests sit 76 cells apart; one slot of gap would do.

### Open rulings (recorded, not implemented)

14. **Typed edges, runs, goals, mail trails** — §22 phase 2 and §24, frozen
    until the six questions in `p-orch-graph-brief.md` are answered. This
    review adds no question; the recommended default for each stands as
    briefed.

## What the shell does better, and will keep doing better

`aoide send --id <id> -- text` is one line; the conductor's path is open,
`8`, navigate, `e`, pick. The conductor earns its launch only where the
shell has no picture: the cross-node spawn forest, the awaiting queue, and
the roster with honest staleness. Items 1–12 are that picture.
