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
the same window. The work went into the tool; the use did not follow.

**2. The graph opens on a blank half-screen.** At 160×48 the default Graph
view selects the synthetic `Active sessions` root and centres it, so the
top 15 rows of the canvas are empty pad and only one rank of cards is
visible below it (capture: 15 blank rows, the group card, then three cards
of thirteen). Cause: `CANVAS_PAD = (40, 16)` is folded into the extent the
follow-camera clamps against (`graphview.rs:60`, `origin()` at
`graphview.rs:615-643`), so the pad is shown whenever the forest is shorter
than the pane. At 80×24 exactly one card fits (capture below).

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
(`graphview.rs:554-590`, `bridged`). After one `j` from that root the
selection sits on a session whose parent and siblings are no longer in
`visible`; `k` looks for a shallower node in the visible preorder and finds
none (`app.rs:4349-4354`), `h`/`l` refuse a sibling the view does not draw
(`graphview.rs:184`). Captured: from `fond-aspen`, `k` and `l`×4 leave the
screen unchanged. The only way out is `a` (All). Since this box has zero
registered projects, every session is under the synthetic root, so the
trap is the normal case here, not an edge.

**4. The graph shows the local box only.** `build_graph` folds every
registered node into the document as a `node:<name>` root, with the node's
own cached graph nested under `children` when the cache is fresh
(`conduct/src/graph/doc.rs:273-310`). `build_model` drops them
(`graphview.rs:323`, `_ => {}`), so yomi's 19 sessions and sakaki never
appear on the canvas. The one cross-host picture is the Mesh list, which is
an indented roster, not a graph.

**5. Cache rows are drawn as live.** The roster core already classifies a
cached session under an unreachable node as `last-seen`/`unknown`
(`conduct/src/graph/who.rs:cached_presence`), but the conductor parses only
`label` and `state` off each session row (`app.rs:1557-1562`) and renders
`state` with the live glyph (`ui.rs:947-958`): under
`◐ yomi-strix — unreachable (last seen 2026-08-20T23:00:00Z)` a row still
reads `♪ … working`. The header carries a raw ISO stamp, not an age.

**6. The sidebar tree beats the graph at its own job.** The project tree
(30 columns) shows all 18 sessions with lineage indents and titles; the
graph shows three. A 32×7 card spends five rows on title, petname, harness
and model, role and state, and activity — most of which the tree row also
carries. Siblings sit 38 cells apart (`SLOT = CARD_W + 6`), so a rank of 13
is 494 cells wide against a 126-cell pane.

**7. The sidebar groups by kind and breaks lineage.** `aoide graph` prints
`warm-birch (shell) → glossy-antler (claude) → three shells`; the sidebar
files `glossy-antler` at the root of Agents and the three shells flat under
Terminals, because it groups by kind before lineage (`board.rs:398`).

**8. The wiki page describes a different program.** `concepts/cli/
Conductor-TUI.md` documents seven panels (DAG, SESSION, PROJECTS, LOG,
STATUS, ROSTER, PENDING) and keys `1`–`7`; the binary has ten tabs and keys
`1`–`9`, `0`. An agent reading tier 0 is taught keys that do not exist.

**9. Every interaction rebuilds the whole model.** `node_order`,
`selected_index`, `select_index`, `select_sibling`, `hit_node`,
`graph_extent` and `graph_origin` each call `build_model`, which runs
`build_graph` (which reads `nodes.json` and every node's cache file) — so
one `j` costs three to five document builds. Register §23 already recorded
this ("rebuilds the whole-world grid three times per interaction").

## The graph feature's gaps, specifically

| Gap | Where | Ruling needed? |
|---|---|---|
| Blank pad at the default view; one card at 80×24 | `graphview.rs:615-643` | no |
| Focus traps the cursor; `k`/`h`/`l` dead after one `j` | `graphview.rs:554-590`, `app.rs:4335-4357` | no |
| Remote nodes and their fresh sessions absent | `graphview.rs:323` | no — observational provenance, not §22 phase 2 |
| No readout of where the cursor is in the forest (n of N, rank) | `board.rs:173` | no |
| Card too large for the information it carries | `graphview.rs:53-57` | yes — §19/§23 designer surface |
| Model rebuilt per call | `graphview.rs:139-205` | yes — §23 S3 (`GraphScene` state) |
| No typed edges, runs, goals | — | frozen (§22 phase 2) |
| Mail overlay on the graph (§24 trails) | — | folded into the §22 architect |

## Ranked improvements

### Do now

1. **Selection walks the forest; the view follows.** Navigation
   (`j`/`k`/`h`/`l`) resolves parent and siblings against the whole forest,
   and Focus then draws the component of wherever the cursor lands. `k` on
   a component root climbs to the gathering root and shows the forest
   under it; `l` on a root steps to the next root. No key is a no-op while
   there is a card in that direction. Keeps the §22 user ruling (Focus
   defaults to the picked node; never forces a connection).
2. **The follow-camera clamps to the forest, not the pad.** When no manual
   pan is set, the origin never shows more pad than it must: a forest
   shorter than the pane starts at its own top row; the selection is
   centred only along the axis where the forest overflows. The pad stays
   for drags and zoom.
3. **Remote nodes are cards.** A `node:<name>` root per registered node
   (mark `🖧`), stating `fresh`/`stale`/`never pulled` with the pull age;
   its nested `children` document, when fresh, renders beneath it with the
   same session cards, prefixed so ids cannot collide with local ones.
   Remote cards take no local action (no Enter focus, no prune cascade):
   §21's "remote rows carry no local pid/window actions". The roster poll
   that refreshes the node cache also runs while the Graph is the visible
   panel, so the cards age honestly instead of vanishing after the 5-minute
   TTL with nobody pulling.
4. **Cache rows say they are cache.** Roster session rows carry the
   backend's `presence`; `last-seen` and `unknown` render with their own
   glyph and word, never the live state colour, and the node header says
   `last seen 3h ago` (age), with the ISO stamp kept in the detail.
5. **Canvas readout: cursor position.** The hint row gains `card n of N`
   and the rank, so an operator knows how much forest the pane is hiding.
6. **Rewrite `Conductor-TUI.md`** to the ten-tab program and its real keys,
   organised by panel as before.

### Needs ruling

7. **Compact cards.** Question: may the card shrink to a 24×5 preset
   (title · petname+tail · role/state · activity) with a 2-row rank gap?
   Recommendation: yes, one fixed size, no per-zoom presets (the AGENTS
   invariant stays). Trade-off: harness and model leave the card (they stay
   in the Details popup and the tree row); a rank of 13 drops from 494 to
   364 cells, still wider than a pane, so this helps but does not finish
   the job. Owner: the §19 designer seat, now vacant (§23 question 1).
8. **Collapse a card's children.** Question: is a per-card fold (like the
   tree's `h`/`l`) a view choice (UI, allowed now) or a scene mutation
   (§23 `GraphScene` state)? Recommendation: view choice, keyed on node id
   in `SceneState`, same as `selected`; it is the one feature that makes a
   13-wide rank navigable without scrolling. Trade-off: a folded card hides
   an `awaiting` child; the fold mark must carry the child-state summary.
9. **One model per frame.** Question: does §23 S3 (`GraphScene` state) own
   the cache, or may `App` memoise `build_model` on the stage mtimes now?
   Recommendation: memoise on `App` now (`(mtimes, view, selected)` key),
   S3 moves it later; the per-interaction cost is measurable on a 40-node
   forest with three node caches on disk.
10. **Lineage-first sidebar.** Question: does the tree keep Agents/Terminals
    as top groups (designer STABLE 1, §19) or nest a spawned terminal under
    its agent? Recommendation: nest — the tree is the only view that shows
    everything at once, and kind is already the mark. Trade-off: the
    Terminals count button no longer equals the Terminals group size.
11. **Graph actions on remote cards.** Question: should `s` on a remote
    agent card open the letter composer addressed `node/petname`?
    Recommendation: yes, it is the same `mail send` the Mesh row offers;
    nothing else (no focus, no prune) crosses the node line.
12. **Typed edges, runs, goals, mail trails** — §22 phase 2 and §24, frozen
    until the six questions in `p-orch-graph-brief.md` are answered. This
    review adds no question; the recommended default for each stands as
    briefed.

## What the shell does better, and will keep doing better

`aoide send --id <id> -- text` is one line; the conductor's path is open,
`8`, navigate, `e`, pick. The conductor earns its launch only where the
shell has no picture: the cross-node spawn forest, the awaiting queue, and
the roster with honest staleness. Items 1–5 are that picture.
