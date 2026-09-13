# Preview fixtures

Stage-file sets for the widget preview canvas
(`modules/facets/quickshell/qml/WidgetPreview.qml`). `lyra preview
--fixture <name>` copies one set into `$ROOT/state/stage/`, where every
widget reads it exactly as it reads the live desktop's stage: the canvas
never talks to `aoided`, so these four files ARE the world the previewed
widget sees.

Song-blind, like the rest of this facet directory: a fixture describes
session/project/hook/herald *shapes*, never a song's taste.

## The four files

Each set is a directory holding all four, with the same shapes the live
`~/.aoide/state/stage/` files carry (copied from them with `jq`, never
hand-invented):

| file | shape | read by |
|---|---|---|
| `sessions.json` | `{schemaVersion, sessions: […]}` | conductor, terminals, SessionMenu |
| `projects.json` | `{schemaVersion, projects: […]}` | conductor (project grouping) |
| `hooks.json` | `{schemaVersion, hooks: […]}` | conductor (hook rows) |
| `herald.json` | `{schemaVersion, notifications: […]}` | herald surface |

Every id, pid, socket path and hostname in here is FAKE — session ids are
`f1000000-0000-4000-8000-…`, pids start at 999001 — so a fixture can never
be mistaken for a capture of a real machine, and a widget that tries to act
on one reaches the canvas's stub bridge, not a process.

## The sets

| set | exercises |
|---|---|
| `empty` | zero sessions, one project, no hooks, no notifications — the honest empty state every widget must render without collapsing |
| `one` | a single working claude agent (`single-ember`) — the minimum row, and what one-of-everything layout looks like |
| `many` | 17 sessions across two projects: agents, subagents, app records, plain shells, idle/working/awaiting, nested children (`sub:`/`a2a:`/`win:` kinds) — the density case |
| `long-text` | 6 sessions built to break layout: a 132-char title, a 40-char model, a 30-char petname, a 120-char cwd, a multi-line `say`, rows with `title`/`say`/`tool`/`model` missing, Greek/Japanese/emoji — plus a `summons` herald record and 3 hook rows |

`many` is the designer's own set, adopted verbatim from the widget-design
lane, and two things in it depart from the rules above: its `hooks.json` is a
bare `[]` rather than the wrapped shape (the conductor's `o.hooks` guard
tolerates it), and its pids are 410001/410007/410008, below the 999001 floor.
Both are left as authored — the designer owns that content, and the data stays
verbatim so a widget bug found against it reproduces exactly. A new set
follows the rules.

## Adding a set

```sh
mkdir modules/facets/quickshell/preview/fixtures/<name>
cd    modules/facets/quickshell/preview/fixtures/<name>
for f in sessions projects hooks herald; do
  jq '<your edit>' ~/.aoide/state/stage/$f.json > $f.json
done
```

Start from the live file so the keys stay real, then edit values. Keep all
four files present — a widget reading a missing one shows an error pane
entry, which is a fixture bug, not a widget bug. Add the set's name and what
it exercises to the table above in the same commit; nothing else registers a
fixture, `lyra preview` scans this directory.
