---
type: concept
created: 2026-08-25
updated: 2026-10-07
tags: [aoide, cli, conductor, tui]
---

# Conductor TUI — the `aoide conductor` Terminal Frontend

The dev-facing reference for the `aoide conductor` interactive terminal UI:
its panels, its keys, what each key dispatches, what each panel reads. One
command backs this page, so it is organized by *panel* rather than by command —
the one deliberate departure from the per-command register the other
`concepts/cli/` pages use. Implementation: `pkgs/aoide/crates/conductor/`
(`app.rs` state and keys, `board.rs` page composition and the project tree,
`ui.rs` panels, `graphview.rs` and `scene.rs` the Graph, `mailview.rs`,
`eventview.rs`, `logtail.rs`, `theme.rs`, `commands.rs`); the crate's own
`README.md` is the full operator manual and `AGENTS.md` its invariants.

The conductor is a frontend only: every keypress that mutates state
dispatches through the same `dispatch::dispatch(Invocation { door: Door::Cli,
.. })` the CLI and MCP doors use, so a conductor action is audited exactly
like a typed command — nothing here computes an outcome independently. The
following stay documented at their own pages, named here and not
re-documented: the PTY control socket and the injection door's mechanics
([[Conductor-Channel]]), the graph model and liveness reaping
([[Session-Graph]]), the 3D-wireframe view ([[Conductor-3D-DAG]], specified
not implemented), the Quickshell terminal widget ([[Terminal-Commander]]),
and the full per-command I/O of `session prune`/`send`/`pending *` and
`focus_session` ([[Graph-and-Conduct]]). The review that measures this
frontend against the conducting job is `docs/architecture/CONDUCTOR-REVIEW.md`.

### aoide conductor

```
aoide conductor [--json]
```

- **Reads:** `state/stage/{projects,sessions,hooks}.json`, mtime-polled every
  ~500 ms (the crossterm poll timeout doubling as the tick);
  `state/session-ledger.jsonl` (past sessions); the mail base and the audit
  log, each as a bounded tail; the roster probe (`aoide_conduct::graph::
  session_roster`, the `session --hosts --json` handler called as a library
  function so the tick writes no audit record, on a worker thread, at most
  every ~15 s while Mesh or Graph is open);
  `node status --json`, `mesh --json`, `pair --json`, `session pending list
  --json`, `config --json`, `secrets pending --json` and `secrets status
  --json` through the injected dispatcher; `song/stage/livery.json`
  (palette — rice staging, so it reads the other tree). `$AOIDE_STAGE_DIR` /
  `$AOIDE_AUDIT_LOG` are both honoured, so a tempdir plus the seed fixture is
  a full offline rig:

  ```sh
  export AOIDE_STAGE_DIR=$(mktemp -d) AOIDE_AUDIT_LOG=$AOIDE_STAGE_DIR/log
  pkgs/aoide/crates/cli/tests/fixtures/seed.sh "$AOIDE_STAGE_DIR"
  aoide conductor
  ```

- **Output:** on the CLI door, `run_cli` dispatches the launch first (one
  audit record, `data: {interactive: true, stageDir}`), then hands the tty
  to a ratatui/crossterm loop: alternate screen + raw mode, mouse capture,
  ten panels, keys `1`–`9` then `0`, `Tab`/`BackTab` to switch, `?` help,
  `q`/`Ctrl-C` quit. On a non-CLI door (an MCP `tools/call`, say) the handler
  returns a "run it from a terminal" outcome instead of blocking that door —
  `interactive: true, door: "non-cli"`.
- **Notes:** not gated. Distinct from `aoide conduct`, which wraps one
  process into the conductor channel rather than raising this UI. Core,
  never `lyra` — the crate carries no wayland/image/song dependency.

## The page

```
 𝄞 CONDUCTOR                     [◆ n Projects] [♜ n Agents] [▣ n Terminals]
 1 ⌂ Home 2 ✉ Mail 3 ♜ Agents 4 ▣ Terminals 5 🖧 Mesh 6 ⚑ Review 7 ◆ Projects 8 ∴ Graph 9 ≡ Log 0 ⚙ Status
 ┌ PROJECTS ───────┬ <panel> ─────────────────────────────────────────────┐
 │ project tree    │ the selected panel's body                            │
 │   Agents (n)    │                                                      │
 │   Terminals (n) │                                                      │
 │   Past (n)      │ [action controls]            canvas / fetch readout  │
 └─────────────────┴──────────────────────────────────────────────────────┘
 hint line
 status line                                      panel keymap
```

`board::NAV` fixes the tab order the number keys and `Tab` follow. Every
workspace panel keeps the project tree on the left: a project folds its
Agents, Terminals and Past groups independently (a terminal an agent
spawned nests under that agent in Agents; Terminals holds all-shell chains); sessions attached to no
project gather under `Active sessions` (live only), and their ended records
form one root-level Past node. `Ctrl-P` moves keyboard focus between the
tree and the body; only the focused region draws a bright selection.
`e` or a right-click opens the context menu for the tree row, graph card,
project or session under the cursor (Details for everything; Open / focus,
Set project, Lead project, Write letter for a live local session; the
recipient chooser, Add folder and Resurrect for a project). Every kind of
thing has one identity mark (`theme::mark`): `⌂` home, `✉` mail, `♜` agent,
`▣` terminal, `🖧` host, `⚑` review, `◆` project, `◇` folder, `∴` graph, `≡`
log, `⚙` status, `◌` past session.

## The ten panels

- **Home (`1`)** — the logo, the action groups (`n` new project, `p` open
  project, `m` correspondence, `H` connected hosts, `L` activity log) and
  the recent projects. Navigation only; nothing dispatches.
- **Mail (`2`)** — the local mail base read non-consumingly: letters grouped
  by signed `threadId`, legacy pair correspondence labelled as such. `n`
  new letter, `s`/`a`/`f` reply / reply all / forward into one form (From,
  To, Cc, Subject, Message, with the recipient tree beside it); `Ctrl-S` or
  the Send control dispatches `mail send` as `conductor-human`. Browsing
  advances no agent cursor and emits no receipt.
- **Agents (`3`) / Terminals (`4`)** — the live roster as an indented list
  under project groups, agents or terminals respectively, with the selected
  row's facts beneath. `Enter` cues a session (see "Enter's destination");
  `h`/`l` fold a group; `L` links the selected session under a typed parent
  id (`graph link`); `a` adds a project root (`project add`); `d` on a group
  header removes that project behind an exact-name confirmation (`project
  remove`); `p` prunes (`session prune`). Opening a Past entry shows ledger
  facts and never focuses or kills a process.
- **Mesh (`5`)** — the roster probe: this box, then every registered node
  with its sessions, headed `● online`, `◐ unreachable (last seen <age>
  ago)` or `○ never pulled`. A session row under an unreachable node is a
  cache row and says so — `· <label>  last-seen · was <state>`, dimmed, no
  live glyph — because the roster core's own `presence` is the only source
  of that fact. Beneath the roster: the registry's trust rows (`node status
  --json`) and each declared mesh's drift (`mesh --json`). `r` forces a
  probe; `s` composes a `send` to the selected session; `e` offers Pair /
  re-pair, the read/spawn/message grant toggles (`node allow`) and
  Unregister behind an exact-name confirmation (`node remove`). A pairing
  code lives only in the ceremony popup, read off a pair outcome's own
  `sas` fields and never scanned out of text.
- **Review (`6`)** — three queues under one cursor: session send/A2A
  approvals (`session pending list`), parked pairing requests (`pair
  --json`), parked secrets TOTP asks (`secrets pending --json`). `a`
  approves / resumes, `d` denies / rejects / dismisses, `r` re-lists. A
  pairing code or a TOTP is typed once into a masked prompt and dropped on
  submit; nothing is replayed. Each queue's read failure renders as its own
  refusal line, never as an empty all-clear.
- **Projects (`7`)** — the registry, one row per project with its extra
  roots beneath. `a` adds a root, `d` removes the selected project behind
  the confirmation.
- **Graph (`8`)** — the retained scene, below.
- **Log (`9`)** — read-only: the audit JSONL's last 200 complete events, the
  selected event's full record (`PgUp`/`PgDn` scroll the detail).
- **Status (`0`)** — the editable config keys (`config --json`, one level
  deep, with the file's provenance) and the secrets broker's references
  (`secrets status --json`: metadata only, never a value). `Enter`/`e` edits
  a key (`config set`) or grants / revokes a secret's consumer (`secrets
  grant|revoke`); the backend's validation is the only gate.

## The Graph panel

A retained scene: cards are one fixed 24×5 preset (title in the top border;
identity, role·state, activity in the rows) at world coordinates
`App::graph.positions` keeps across refreshes, wired top-down (a parent
centred over its children), under a camera that pans and zooms. The forest
is `build_graph` over the local stage — projects, their sessions, spawned
children, and one synthetic `Active sessions` root for the projectless —
followed by one **host card per registered node** off the roster probe
(border `🖧 NODE · online|unreachable|never-pulled`, then the node's name and
`n session(s)` or `last seen <age>`), its
reported sessions flat beneath. A remote card carries no local session id,
so the local actions pass over it — `s` writes to the far agent's
`node/petname` mailbox and `e` offers Details (its Mesh row) and Write
letter; a cached row wears `last-seen` as its
state and `was <state>` as its activity, dimmed. A probe that fails after a
good one keeps the last rows, dimmed, each host card's last row reading
`probe failed <age>`.

| Key | Effect |
|---|---|
| `j`/`k`, `↓`/`↑` | first child / parent — resolved against the whole forest |
| `h`/`l`, `←`/`→` | previous / next sibling — likewise, no wrap |
| `g`/`Home`, `G`/`End` | first / last drawn card |
| `a` | Focus (the selected card's connected component; the gathering root is not a connection) ↔ All |
| `f` | fold / unfold the selected card's children; the bottom border reads `▸ n · k awaiting` |
| `Enter` | cue the selected local session |
| `s` | write a letter to the selected agent — local, or a far one at `node/petname` |
| `e` / right-click | the card's context menu |
| `p` | `session prune` |
| Space + drag, middle drag, wheel, Shift + wheel | pan |
| Ctrl + wheel | zoom 50/75/100/125/150 %, anchored at the pointer |

Navigation never strands the cursor: under Focus the view is derived from
the selection, so `k` on a loose session climbs to the gathering root and
reopens the forest, and `l` reaches a sibling the component did not draw.
Until a drag or a zoom moves it, the camera follows the selection and shows
the forest, never the pad around it: an axis the forest fits in is held
whole (top-aligned, centred across), an overflowing axis centres the
selected card inside the forest's bounds. The readout on the action row —
`Canvas 100% · FOCUS · card 3/20 · rank 1` — says where the cursor stands
in what is drawn. Rendering, hit testing, drag and wheel share one camera
transform; render records nothing for a later event to find.

## What it dispatches

Every mutating keypress builds an `Invocation` and passes it to the injected
`DispatchFn`, so it is audited exactly like a typed command: `session
prune`, `project add`, `project remove`, `project lead`, `graph link`,
`send --to <target> --yes`, `session pending approve|deny`, `pair` with its
flags (never bare, which would raise its own menu) and `pair reject`, `node
allow|remove`, `config set`, `secrets approve|dismiss|grant|revoke`, `mail
send`, `resurrect`. Reads never dispatch a mutation:
the stage-file loads, the ledger, mail and audit tails call their storage
readers directly, and the roster, trust, pending, config and secrets reads
are `--json` invocations of their commands. Cue-session on a live window
calls `focus_session` directly rather than dispatching (there is no CLI
subcommand left to dispatch), writing its own audit line so the one-audit-
log invariant holds.

## Three behaviours a reader will hit

- **The roster probe throttles; the tick is not audited, `r` is.** A stale
  cache fetches immediately on entering Mesh or Graph; otherwise a fetch
  fires at most every ~15 s while either is visible, as a library call. `r`
  in Mesh overrides the window through the audited dispatcher. A landed
  probe is folded into the retained graph scene like a stage refresh; a
  failed one — or an Ok reply with no `nodes` — keeps the previous rows,
  muted, and says so.
- **`session pending list`'s `id` is an array position, not a stable id** —
  resolving one entry shifts every id after it. `App::dispatch` re-lists
  synchronously before the next paint, so a second `a`/`d` in the same visit
  always resolves the row actually on screen.
- **Anything that can cross the broker socket runs on a worker** (`secrets
  pending`, `secrets status`, every secrets mutation): the last rows stay on
  screen with "reading…" beside them, exactly one mutation is in flight at a
  time, and a second is refused with the reason, never queued.

## Enter's destination

`Enter` on a session (`cue_session`) opens the log-tail overlay when the
record carries a `log_path` — set only by `conduct --headless` — reading the
file immediately so content paints on the keypress itself, not the next
tick. Any other live local session calls `focus_session` directly instead. A
Past entry and a remote card take no Enter.

## Terminal restoration

`TermGuard`'s `Drop` leaves the alternate screen, releases the mouse and
disables raw mode; a panic hook does the same before the default hook
prints. Every exit path — clean quit, `q`, a panic inside a view — restores
the tty.

## Related

- [[Conductor-Channel]]
- [[Session-Graph]]
- [[Conductor-3D-DAG]]
- [[Graph-and-Conduct]]
- [[Doors-and-Nodes]]
- [[aoide-cli]]
