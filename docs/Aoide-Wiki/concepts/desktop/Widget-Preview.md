---
type: concept
created: 2026-09-13
tags: [aoide, desktop, lyra, quickshell, preview, widgets, design]
---

# Widget-Preview — the canvas a song widget is designed on

`lyra preview` opens one song widget on a human-driven canvas: any size, any
anchor, any viewport, any fixture, against an isolated root that can never
reach the live desktop. It is the design surface for the per-song widgets
([[Widget-Bridge-Contract]] says what a widget reads; `qml/slots.md` says
what each slot is handed) and the successor to the seven single-purpose
`*Preview.qml` screenshot rigs, which stay for their fixed-size snapshots.

## Start

```
lyra preview conductor                       # sonata's conductor.qml, fixture `many`
lyra preview terminals --fixture long-text   # another slot, another fixture set
lyra preview /home/khoa/Aoide/song/songbook/sonata/widgets/conductor.qml
lyra preview set --width 280 --anchor br --viewport portrait --zoom 1
lyra preview set --livery fugue           # any song, `live`, a livery file, or a base16 scheme
lyra preview declare                         # the previewed body + palette become the checkout song's
lyra preview shot --what widget --out /tmp/w.png  # the widget alone at 1:1 (also: screen, canvas, element)
lyra preview tree --at 120,40               # the live element under a point, with its file:line
lyra preview notes --json                    # what the designer highlighted, each resolved to element + file:line
lyra preview --no-launch --json              # build the root only, print it
```

`lyra preview <widget>` is a viewer: it holds the terminal until the canvas
window closes, and closing it (or killing `lyra`) closes the canvas. From an
agent shell launch it detached — `setsid -f lyra preview conductor` — then
drive it with `set`, `shot`, `tree` and `notes` from any other shell. On
Hyprland the window is floated and centered on the focused monitor once it
maps; on any other compositor it opens wherever the compositor puts it.

The positional is a slot name (`conductor`, resolved against `--song`,
default `sonata`), a `song/slot` pair, or a path. A working-tree path under
`song/songbook/<song>/widgets/` is rewritten to `songs/<song>/<file>` so
the widget's `import "../.."` resolves inside the canvas root exactly as it
does in the deployed tree; any other path loads verbatim and its error is
shown on the canvas. The command blocks while the canvas is open and prints
its envelope when the window closes.

## What the canvas is

```
$XDG_RUNTIME_DIR/aoide-preview/            the ISOLATED root (never under aoide/, the daemon's socket dir)
├─ song/stage/livery.json                  the song's committed livery + {"song": …}, or --livery's file
├─ state/stage/{sessions,projects,hooks,herald}.json    one fixture set, copied in
├─ run/qml/*.qml                           COPIES of checkout modules/facets/quickshell/qml/*.qml
├─ run/qml/songs/<song>/                   COPIES of checkout song/songbook/<song>/widgets/ (every song)
├─ run/qml/icons/                          COPIES of checkout modules/facets/quickshell/icons/ (the toolbar's Iconoir glyphs)
├─ run/qml/songs/{manifest,registry}.json  copies of the deployed owner map
├─ preview.json                            the control file — the rail and `lyra preview set` write the same document
└─ preview.pid                             one canvas per root; `--root` for a second
```

Everything under `run/qml/` is a copy, never a symlink into the checkout:
a write through the root lands in the root. The checkout is the only place a
widget is edited. `preview.json`'s `stage` map pairs each watched checkout
file with its copy, and the canvas rewrites those copies before every
reload (auto-reload, the Reload button, the `reload` IPC). A facet file
edited outside the watch list needs `lyra preview --no-launch --root <root>`
to re-stage; that rebuild merges into the existing control file. `--root`
refuses `..` and anything at, under, or symlink-resolving into the live
daemon's own dir (`$XDG_RUNTIME_DIR/aoide`, falling back the way the daemon
does when that's unset — see the canvas isolation note below for why this
is read directly rather than through the daemon's own socket-path lookup).

The stage is a camera over the viewport: the widget and its annotation
overlay are both children of the one scaled, offset viewport item, so a note
drawn after a pan or zoom lands where it was clicked. Middle-drag pans;
holding Space (outside a text field or a Tab-focused control) turns left-drag into a pan and shows the
hand — it masks the annotation tool in use and never changes it; Ctrl+wheel
zooms about the pointer; plain wheel reaches the widget; Fit, 1:1, a typed
zoom, and `lyra preview set --zoom` reset the camera. The camera is a view,
never written to `preview.json`. The toolbar's glyphs are the facet's
`lyra icon resolve` output under `modules/facets/quickshell/icons/`, copied
into the root beside the QML and tinted from the chrome — no network, no icon
font. Hovering a glyph shows its tooltip; every button is a Tab stop, focus
shows the same tooltip, and Space or Return presses it. The anchor grid is
nine directional arrows (centre = `c`), the aspect locks are a lock/unlock
glyph that reflects the current state, and a note row's × deletes it.

The canvas process is `quickshell -p <root>/run/qml/WidgetPreview.qml`
with `AOIDE_ROOT`, `AOIDE_STATE_DIR`, `AOIDE_STAGE_DIR` pointed into the
root and `AOIDE_DAEMON_SOCKET` pointed at a path that never exists — that
repoint is also why the canvas's own rail never runs a bare `lyra` from
PATH: `lyra preview` writes its own `std::env::current_exe()` into
`preview.json`'s `lyra` key at build time, and every rail action spawns
THAT path (`controlDoc.lyra || "lyra"`), since a deployed `lyra` on PATH
may predate the `preview` command family entirely. `resolve_root`'s own
live-daemon gate reads `$XDG_RUNTIME_DIR/aoide` directly for the same
reason — never `AOIDE_DAEMON_SOCKET` — so the repointed socket inside a
canvas never makes that canvas's own root look like the live daemon's dir
and refuse the rail's `--root` calls. Every
widget resolves its reads through `AOIDE_ROOT` ([[Widget-Bridge-Contract]]),
so one environment isolates livery, stage files, and the owner map at
once. The widget is handed a real `LiveryState` and a real `StagingEngine`
(both reading the root) and a **stub bridge**: every `focusSession`,
`recheckSessions`, `sessionAction`, `sendCommand` a card fires lands in the
canvas's action log and nowhere else. No `ShellBridge` is ever
instantiated, so no send, kill, focus, or recheck can reach a real session.

The window is a `FloatingWindow` — a real compositor client, titled
`aoide-widget-preview` — so `lyra screen shot --window <address>` captures
it — `lyra preview shot --what canvas` finds the address by title AND the
root's own quickshell pid (`qs list --all` names it), so two open roots
never capture each other — and it is never a layer surface fighting the
live bar or dock. `rice mode stage` reaps any
`qs -p *Preview.qml` harness by that suffix; the canvas keeps the suffix on
purpose and simply relaunches.

## Controls and their meaning

| control | values | rule |
|---|---|---|
| widget | path field · file picker, opened at the current widget's folder | working-tree path rewritten to `songs/<song>/<file>`; errors shown in the pane |
| widget width / height | number · `auto`, per axis | a number forces `item.width`/`height`; `auto` follows the item's implicit size |
| widget aspect lock | lock/unlock glyph | editing one axis recomputes the other from the current box ratio; ignored while an axis is `auto` |
| anchor · margin | 3×3 arrows (`tl t tr l c r bl b br`) · px | places the widget box inside the viewport; never resizes it |
| viewport | `16:9` 1920×1080 · `16:10` 1920×1200 · `4:3` 1600×1200 · `ultrawide` 2560×1080 · `portrait` 1080×1920 · custom W×H | the surface the widget would live on |
| viewport aspect lock | lock/unlock glyph | editing W recomputes H from the preset ratio and vice versa |
| zoom | `fit` · number | `fit` = min(pane/viewport) capped at 1.0, never upscaling; a number scales exactly. Zoom scales the rendered viewport only — the widget lays out at logical size |
| background | `livery` · `checker` | palette ground, or an 8 px checkerboard to see transparent edges |
| fixture | a set name · a directory | copied into `state/stage/` by `lyra preview set --fixture`; widgets hot-reload through their own file watches |
| palette (livery) | `live` · a song name · a livery file · a base16 scheme (`.json` or flat `.yaml`) | resolved through the livery engine (the `lyra livery emit stage` document, fallbacks applied) into `song/stage/livery.json` with `song` injected; a bare base16 scheme gets its palette from CONTRACTS §1's column (bg=base00, fg=base05, accent=base0D, urgent=base08, hot=base0B), the reverse of the Stylix facet's synthesis. `live` is the resolved stage twin, host overrides included. Every widget recolours in place; a swatch strip shows the five roles and all sixteen base16 slots |
| auto-reload · reload · recreate | on/off · — · — | auto-reload watches every `*.qml` in the song's `widgets/` dir (the `watch` list) and rebuilds the canvas within a second of a save; reload does the same by hand (state returns from `preview.json`); recreate rebuilds only the widget item — an edited body needs a reload, since components are cached per URL |
| declare | arm, then confirm | `lyra preview declare`: the previewed body becomes `song/songbook/<song>/widgets/<slot>.qml` when it came from elsewhere, and the previewed palette becomes the song's `livery.json` tiers when it is not the song's own — byte-identical is a no-op, git and the rebuild stay yours ([[Self-Ricing]]'s `rice declare` vocabulary) |
| annotate | off · pick · rect · ellipse · arrow · line · note | pick outlines the element under the pointer and a click attaches the rail's note to it; the shapes sketch where something should go; every entry is saved to the root's `notes.json` in widget-local coordinates and read back by `lyra preview notes` with its element path and source `file:line` |

Every control writes `preview.json`; `lyra preview set` writes the same
file, so a script, an agent, or the human's hand all drive one state, and
a relaunch resumes where the last one stopped.

## Fixture sets

`modules/facets/quickshell/preview/fixtures/<set>/` holds the four stage
files in their live shapes (CONTRACTS.md §4). Shipped sets: `empty` (no
sessions), `one` (a single working agent), `many` (every kind and state,
nested chains, several projects), `long-text` (over-long titles, models,
petnames, paths; missing fields; unicode). Every id, pid, and socket in a
fixture is fake — nothing names a real process. A new set is a new
directory; `lyra preview` lists whatever directories exist.

## Screenshots for a design pass

```
lyra preview shot --what screen                  # the whole layout, as `lyra screen shot`
lyra preview shot --what canvas                  # the canvas window: rail, stage, log
lyra preview shot --what widget --out /tmp/w.png  # the widget item alone, at its own pixels
lyra preview shot --what element --element "Column[0]/Rectangle[2]#card" --annotated
lyra screen point hover <x> <y>                  # hover a row inside the canvas before a shot
```

`widget` and `element` shots come from the canvas itself (`Item.grabToImage`
through its `preview` IPC target), so they are exact widget pixels with no
chrome and no zoom; `--annotated` keeps the annotation overlay in the
capture. Every shot writes a `<out>.json` sidecar: widget size, scale, the
element path and rect, its source `file:line`, and the notes present.

The before/after discipline is [[Self-Ricing]]'s: capture, read it back,
judge, then show.

## Pointing an agent at an element

The canvas has an annotate rail: `pick` outlines the element under the
pointer and a click attaches a note to it; `rect` · `ellipse` · `arrow` ·
`line` draw placeholders where something should go — a rect or ellipse
shows its box while you drag, an arrow or line draws itself start-to-end;
`note` pins a numbered marker. Everything lands in the root's `notes.json`, in widget-local
coordinates, and `lyra preview notes --json` hands it to an agent as a work
list — for each note the kind, the rect, the element path
(`Type[i]/Type[i]#objectName` from the widget root), and the source
`file:line` it resolves to. `lyra preview tree` prints that same live item
tree joined to a static parse of the widget's QML, so a path in a note is
always one `--at x,y` away from the line that draws it. `--done <n>` and
`--clear` close the loop from the shell, the same way the rail does.
