---
type: concept
created: 2026-08-25
updated: 2026-08-27
tags: [aoide, cli, conductor, tui]
---

# Conductor TUI — the `aoide conductor` Terminal Frontend

The dev-facing reference for the `aoide conductor` interactive terminal UI:
its panes, its keys, what each key dispatches, what each pane reads. One
command backs this page, so it is organized by *panel* rather than by command —
the one deliberate departure from the per-command register the other
`concepts/cli/` pages use. Implementation: `pkgs/aoide/crates/conductor/`
(`app.rs`, `ui.rs`, `graphview.rs`, `theme.rs`, `logtail.rs`,
`commands.rs`).

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
`focus_session` ([[Graph-and-Conduct]]).

### aoide conductor

```
aoide conductor [--json]
```

- **Reads:** `state/stage/{projects,sessions,hooks}.json`, mtime-polled every
  ~500 ms (the crossterm poll timeout doubling as the tick); `song/stage/
  livery.json` (palette → ANSI-256 theme — rice staging, so it reads the
  other tree); the audit log (the LOG panel
  tails it). `$AOIDE_STAGE_DIR` / `$AOIDE_AUDIT_LOG` are both honoured, so a
  tempdir plus the seed fixture is a full offline rig:

  ```sh
  export AOIDE_STAGE_DIR=$(mktemp -d) AOIDE_AUDIT_LOG=$AOIDE_STAGE_DIR/log
  pkgs/aoide/crates/cli/tests/fixtures/seed.sh "$AOIDE_STAGE_DIR"
  aoide conductor
  ```

- **Output:** on the CLI door, `run_cli` dispatches the launch first (one
  audit record, `data: {interactive: true, stageDir}`), then hands the tty
  to a ratatui/crossterm loop: alternate screen + raw mode, seven panels,
  keys `1`–`7`/`Tab`/`BackTab` to switch, `?` help, `q`/`Ctrl-C` quit. On a
  non-CLI door (an MCP `tools/call`, say) the handler returns a "run it from
  a terminal" outcome instead of blocking that door — `interactive: true,
  door: "non-cli"`.
- **Notes:** not gated. Distinct from `aoide conduct`, which wraps one
  process into the conductor channel rather than raising this UI. Core,
  never `lyra` — the crate carries no wayland/image/song dependency.

## The seven panels

`Panel::ALL` (`app.rs`): DAG, SESSION, PROJECTS, LOG, STATUS, ROSTER,
PENDING, in that order — the order the `1`–`7` keys and `Tab`/`BackTab`
cycle through. ROSTER and PENDING are later additions, appended last so no
earlier panel's key ever shifts. The panel titles itself SESSION, singular —
the underlying `sessions.json` file and data-shape names stay plural; only
the panel/family name normalizes.

- **DAG (`1`)** — the visual graph: nodes and edges laid out and drawn by
  `graphview`. Selection walks the same preorder node list the layout
  draws. `Enter` on a node with a session cues it (see "Enter's
  destination" below); `p` dispatches `session prune`, the one graph-wide
  command left on this key (`e`/`graph emit` is deleted — every stage
  mutation restages `graph.json` automatically, so there is nothing left
  for the keybinding to trigger).
- **SESSION (`2`)** — the collapsible terminal roster: project group
  headers interleaved with their session subtrees, one flattened selection
  index over the lot (`App::dag_rows`). `Enter` on a session cues it; on a
  group header it toggles the fold. `h`/`-` folds the group under the
  cursor, `l`/`+` unfolds it. `a` opens the inline project-add prompt; `d`
  on a group header removes that project (`project remove` — the
  unanchored pseudo-group has nothing to remove). `L` on a session opens a
  prompt collecting a parent session id and dispatches `graph link`. `p`
  dispatches `session prune` here too.
- **PROJECTS (`3`)** — the flat project registry, sorted by name. `a` opens
  the same project-add prompt SESSION uses; `d` removes the selected
  project (`project remove`).
- **LOG (`4`)** — read-only: tails the audit log.
- **STATUS (`5`)** — read-only: stage status, both trees (`App::stage` for
  `state/stage/`, `App::rice_stage` for `song/stage/`).
- **ROSTER (`6`)** — presence over this box's own sessions plus every
  registered node. Rows are `who --json`'s `Outcome.data`, dispatched
  through the same injected `DispatchFn` and parsed into a flattened row
  list — never re-derived. `who` performs a LIVE network probe of every
  registered node (~2 s/node, parallel) on every invocation, so this pane
  throttles: it re-dispatches at most every ~15 s (`ROSTER_THROTTLE`) while
  visible, never on every ~500 ms tick; `r` forces one fetch regardless of
  the throttle window (a no-op while a fetch is already in flight). The
  dispatch runs on its own `std::thread`, reporting back over an `mpsc`
  channel the tick loop polls without blocking — the one dispatch in this
  crate that skips the synchronous `App::dispatch`, since `who` never
  mutates anything and its live probes would otherwise freeze the tick
  loop. `s` on a selected session row opens the compose prompt, pre-labeled
  with that row's own display-grammar label, and on submit dispatches
  `send --to <target> --yes -- <text>`.
- **PENDING (`7`)** — held `send`/A2A entries, approve/deny. Rows are
  `session pending list --json`'s `Outcome.data`. Unlike ROSTER, this is a
  local file read (no network), so it refreshes synchronously on the tick
  while the pane is visible, and again after every dispatch
  (`reload_all` → `refresh_pending`). `a`/`d` dispatch `session pending
  approve`/`session pending deny` on the selected row.

## Keys

Global, handled before any panel sees the key: `Ctrl-C` quits from
anywhere, including mid-overlay; `?` opens the help overlay (swallows keys
until `?`/`Esc`/`q` closes it); the log-tail overlay swallows keys until
`Esc`/`q`/`Enter` closes it; an open inline input (project-add, link,
compose) consumes text/`Enter`/`Esc` itself. Otherwise: `q` quits, `Tab`/
`BackTab` cycle panels, `1`–`7` select a panel directly.

Per-panel keys (LOG and STATUS take none — read-only):

| Panel | Keys |
|---|---|
| DAG | `j`/`k` (`↓`/`↑`) move; `g`/`Home` jump to the first node, `G`/`End` to the last; `Enter` cue the selected session; `p` `session prune` |
| SESSION | `j`/`k` move; `Enter` cue a session / toggle a group's fold; `h`/`-` fold, `l`/`+` unfold; `a` add a project; `d` on a group header remove that project; `L` on a session link it under a typed parent id; `p` prune |
| PROJECTS | `j`/`k` move; `a` add a project; `d` remove the selected project |
| ROSTER | `j`/`k` move; `r` force a fetch; `s` compose a send to the selected session |
| PENDING | `j`/`k` move; `a` approve; `d` deny |

## What it dispatches

Every mutating keypress builds an `Invocation` and passes it to the injected
`DispatchFn`, so it is audited exactly like a typed command: `session prune`,
`project add`, `project remove`, `graph link`, `send --to
<target> --yes` (ROSTER compose), `session pending approve`, `session pending
deny`. Reads (`who`, `session pending list`, the stage-file loads) never
dispatch — they call the pure graph functions and stage-file loaders
directly. Cue-session on a live window is the one exception on the write
side too: it calls `focus_session` directly rather than dispatching `graph
focus` (deleted — there is no CLI subcommand left to dispatch), writing its
own audit line by hand so the one-audit-log invariant still holds.

## Two behaviours a reader will hit

- **ROSTER's `who` throttles.** A stale cache fetches immediately on
  switching into the pane; otherwise a fetch fires at most every ~15 s
  while the pane is visible. `r` overrides the window unconditionally.
- **`session pending list`'s `id` is an array position, not a stable id** —
  resolving one entry shifts every id after it. `App::dispatch` re-lists
  synchronously (`reload_all` → `refresh_pending`) before the next paint,
  so a second `a`/`d` in the same visit always resolves the row actually on
  screen, never a stale index.

## Enter's destination

`Enter` on a session (`cue_session`) opens the log-tail overlay when the
record carries a `log_path` — set only by `conduct --headless` — reading the
file immediately so content paints on the keypress itself, not the next
tick. Any other session calls `focus_session` directly instead.

## Terminal restoration

`TermGuard`'s `Drop` leaves the alternate screen and disables raw mode; a
panic hook does the same before the default hook prints. Every exit path —
clean quit, `q`, a panic inside a view — restores the tty.

## Related

- [[Conductor-Channel]]
- [[Session-Graph]]
- [[Conductor-3D-DAG]]
- [[Graph-and-Conduct]]
- [[Doors-and-Nodes]]
- [[aoide-cli]]
