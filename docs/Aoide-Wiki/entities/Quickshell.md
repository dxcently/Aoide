---
type: entity
created: 2026-07-25
updated: 2026-07-26
tags: [aoide, shell, ui, qml, quickshell]
source: "[[references/AOIDE-HANDOFF]]"
---

# Quickshell

The shell and UI runtime for Aoide, written in QML. It renders the complete shell surface: workspaces bar (with agent sessions and connection state), notification daemon (implementing `org.freedesktop.Notifications` natively), agent widgets, launcher, OSD, lockscreen, greeter, and wallpaper layer. This replaces the swaync / rofi / hyprlock / swww zoo with a single runtime.

Quickshell reads `stage/notes.json` at runtime, so nearly the full arrangement — colors, typography, geometry, shell widgets — hot-reloads during rehearsal (preview) without a rebuild. GTK/Qt targets require app restarts and are adopt-only for preview purposes.

Communication discipline: Quickshell reads state files from shellbridge and issues commands via the unix socket. It never speaks an agent protocol or MCP directly.

A Quickshell NotificationServer spike (actions + inline reply) is planned for the yomi-strix session. If it lands, Quickshell keeps the full notification daemon role with no interim mako/swaync.

## Implementation (walking skeleton, commit f3ceadf)

The QML skeleton is shipped in `modules/facets/quickshell/qml/`. Two singletons
carry the shared session state: **`NoteState`** watches `stage/notes.json` via a
`FileView` and re-binds every surface's colours in one pass on an atomic
replace (the hot-reload); **`ShellBridge`** is the unix-socket client — the sole
outbound channel from QML (`focusSession(address)` → shellbridge → hyprctl), no
MCP/HTTP/shell-exec from QML. `shell.qml` (a `ShellRoot`) instantiates the two
singletons and the surface widgets (`AoideBar` with the `SessionChip`/
`WorkspaceRow` session-jump widget, `AoideNotifications` + `NotificationCard`,
`AoideLauncher`, `AoideOsd`, `AoideLockscreen`, `AoideGreeter`, `AoideWallpaper`)
— each a stub reading colours from `notes`, kept in a separate file so [[Melete]]
can swap them independently. The facet installs the tree to `~/Aoide/qml` via
home-manager activation. Now that the host runs Aoide live, that deploy target
sits **untracked at the repo root** of the user's fork — the relationship
between the source tree (`modules/facets/quickshell/qml/`) and the deployed
copy needs a decision (open thread). Crucially, `hyprland.conf` is
owned by home-manager's `wayland.windowManager.hyprland`: the compositor facet
writes note + keybind fragments with `mkBefore`, and the Quickshell facet appends
its `exec-once` autostart with `mkAfter`, so the two facets compose the one config
file without collision.

## Nine surfaces (commits 1fedd58 + 41be90f)

Two registered surfaces have since grown real bodies, bringing the registry to
**nine**:

- **`sessionGraph`** (surface #9, `AoideSessionGraph.qml` + `GraphRow.qml`) —
  an overlay hot-reloading `song/stage/graph.json` on the same
  `FileView` pattern as `NoteState`, rendering the [[Session-Graph]] DAG as an
  indented tree. Since 8f4034e it is **dormant** — no keybind, bridge-only —
  kept for a future full-screen DAG view.
- **`agentWidgets`** — no longer empty: it is the [[Gadget-Dock]], a
  **left-edge pinnable popup** of Win7-sidebar-homage gadgets
  (`AoideAgentWidgets.qml`, `GadgetFrame.qml`, and the terminal-manager /
  DAG / clock / meter gadget files). Hidden by default; it slides in on a
  5 px hot-edge hover (pure QML) or on `SUPER+G` (open-and-pin via the
  bridge), and since it holds the DAG gadget it is the primary DAG
  affordance on the desktop.

**`GraphModel.qml`** is the canonical QML graph model (its `buildRows()` was
extracted from the overlay); both the overlay and the dock's `DagGraphGadget`
instantiate it, and any future graph consumer must too — the tree derivation
lives in exactly one place.

## Related

- [[Desktop-Architecture]]
- [[Session-Graph]]
- [[Gadget-Dock]]
- [[shellbridge]]
- [[Self-Ricing]]
- [[Notes]]
- [[Codebase]]
- [[Hyprland]]
