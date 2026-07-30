---
type: entity
created: 2026-07-25
updated: 2026-07-30
tags: [aoide, shell, ui, qml, quickshell]
source: "[[references/AOIDE-HANDOFF]]"
---

# Quickshell

The shell and UI runtime for Aoide, written in QML. It renders the complete shell surface: workspaces bar (with agent sessions and connection state), notification daemon (implementing `org.freedesktop.Notifications` natively), agent widgets, launcher, OSD, lockscreen, greeter, and wallpaper layer. This replaces the swaync / rofi / hyprlock / swww zoo with a single runtime.

Quickshell reads `stage/drachma.json` at runtime, so nearly the full arrangement — colors, typography, geometry, shell widgets — hot-reloads during rehearsal (preview) without a rebuild. GTK/Qt targets require app restarts and are adopt-only for preview purposes.

Communication discipline: Quickshell reads state files from shellbridge and issues commands via the unix socket. It never speaks an agent protocol or MCP directly.

**Status:** specified; no actions/inline-reply support in the NotificationServer today. The NotificationServer spike (actions + inline reply) is scoped for the yomi-strix session.

## Implementation

The QML skeleton is shipped in `modules/facets/quickshell/qml/`. Two singletons
carry the shared session state: **`DrachmaState`** watches `stage/drachma.json` via a
`FileView` and re-binds every surface's colours in one pass on an atomic
replace (the hot-reload); **`ShellBridge`** is the unix-socket client — the sole
outbound channel from QML (`focusSession(address)` → shellbridge → hyprctl), no
MCP/HTTP/shell-exec from QML. `shell.qml` (a `ShellRoot`) instantiates the two
singletons and the surface widgets (`AoideBar` with the `SessionChip`/
`WorkspaceRow` session-jump widget, `AoideNotifications` + `NotificationCard`,
`AoideLauncher`, `AoideOsd`, `AoideLockscreen`, `AoideGreeter`, `AoideWallpaper`)
— each a stub reading colours from `drachma`, kept in a separate file so [[Melete]]
can swap them independently. The facet installs the tree to `~/Aoide/qml` via
home-manager activation. The host runs Aoide live, so that deploy target sits
**untracked at the repo root** of the user's fork. Crucially, `hyprland.conf`
is owned by home-manager's `wayland.windowManager.hyprland`: the compositor facet
writes drachma + keybind fragments with `mkBefore`, and the Quickshell facet appends
its `exec-once` autostart with `mkAfter`, so the two facets compose the one config
file without collision.

## The registry — nine declared, eight with a live body

The facet declares nine `owner = "quickshell"` surfaces
(`modules/facets/quickshell/default.nix`: bar, notifications, launcher, osd,
lockscreen, greeter, wallpaper, agentWidgets, sessionGraph):

- **`agentWidgets`** — the [[Gadget-Dock]], `AoidePanel.qml`: a **left-edge
  panel** holding four self-framed gadgets (Conductor, Terminals, Meters,
  Power). Its fore-edge peeks past the screen edge at rest — further when a
  session is `awaiting` and unacknowledged — and slides fully in on a 6 px
  hot-edge hover or on `SUPER+G` (an in-process Hyprland global shortcut the
  panel itself registers, `aoide:dock`; not a CLI verb). Its Conductor
  gadget is the desktop's at-a-glance agent view — a beamed session tree,
  not a literal DAG diagram.
- **`sessionGraph`** — declared but has **no QML body**: the standalone DAG
  overlay (`AoideSessionGraph.qml` + `GraphRow.qml`) and the shared
  `GraphModel.qml` it and the dock's former DAG gadget instantiated are no
  longer part of the QML tree. `aoide graph view`/`--json` and the `aoide
  baton` TUI are the DAG's renderers today ([[Session-Graph]]).

## Launcher (surface #3, built out 2026-07-28)

`AoideLauncher.qml` is a working, keyboard-driven app launcher (the rofi
replacement). It is a summoned `PanelWindow` on
`WlrLayer.Overlay` (namespace `aoide-launcher`) that takes an **exclusive
keyboard grab** only while shown and is an inert zero-cost layer at rest. It
enumerates apps from Quickshell's built-in `DesktopEntries`, filters on a
prefix-ranked case-insensitive substring as you type, navigates with
Up/Down + Ctrl+J/K, launches on Enter/click, dismisses on Escape / scrim-click.
It is a Pantheon pane (`GadgetFrame`, cream Aero glass, `♪` prompt, lowercase
`launcher.summon` callout), colours strictly from `drachma`.

Two design decisions worth carrying forward:

- **Trigger is a Hyprland `GlobalShortcut` (`aoide:launcher`), registered
  in-process** — the compositor binds `SUPER+SPACE` to it via `bind = …,
  global, aoide:launcher`. `ShellBridge` is outbound-only, so an in-process
  global shortcut is the cleanest inbound trigger — no new `aoided` verb, no
  inbound socket.
- **Launch is `DesktopEntry.execute()`** — the same Quickshell-native side-effect
  idiom the shell already uses (`WorkspaceRow.activate()`, `BatonGadget` →
  `execDetached`). Routing app-launch through `aoided` per house rule #6 has
  no such verb today; flagged, not silently baked. See
  [[references/AOIDE-DEV-HANDOFF]] §7.

The compositor facet also adds `aoide-launcher` to the blur / `ignore_alpha` /
hyprglass namespaces so the pane frosts like the bar and dock. Both the keybind
and the blur rules are baked into `hyprland.conf`, so the launcher needs a gated
`switch` to land live (the QML deploys to the read-only `~/Aoide/qml/` tree the
same way).

## Session service & resilience

The shell surface is started by the **`aoide-quickshell`** systemd *user*
service, defined in `modules/facets/quickshell/default.nix`. The service is
the session-assembly seam: it orders after `graphical-session.target` (so
Quickshell inherits a valid Wayland env), logs to journald (`journalctl
--user -u aoide-quickshell`), and respawns on crash. Three layers keep one
bad load from bringing the desktop down for good:

- `ConditionPathExists = <shellQmlEntry>` — if the QML entry hasn't landed, the
  unit *declines to start* rather than crash-looping. **But this checks
  existence, not validity.**
- `Restart = "on-failure"` / `RestartSec = 3` — a genuine crash respawns the
  whole shell instead of leaving the desktop bare until next login.
- `StartLimitIntervalSec = 60` / `StartLimitBurst = 5` (added 2026-07-28) — the
  backstop for the gap the `ConditionPathExists` guard can't cover: a
  `shell.qml` that *exists but won't load* (a QML parse error, or an `ExecStart`
  aimed elsewhere by a stray drop-in) exits 255 every respawn, so without a
  limit `Restart=on-failure` thrashes forever. After 5 failures inside 60s
  systemd parks the unit `failed`. Five tries still absorbs a transient failure
  (e.g. Wayland not ready yet).

**The wallpaper engine *is* this shell** — the wallpaper is a
`wlr-layer-shell` `Background` surface (`AoideWallpaper.qml` inside `shell.qml`),
the sole live painter (Stylix's `hyprpaper` is force-disabled via the
`aoide.surfaces.wallpaper` owner registry; no `swww`/`swaybg`/`mpvpaper`
elsewhere). So a shell crash takes the wallpaper *and* the bar/dock/gadgets with
it in one stroke — they are one process, not four. The wallpaper's own
source-of-truth is the live-watched `stage/cover.json` (written by `aoide rice
preview` or, for a direct hot-swap, `aoide cover set <path-or-name>`), falling
back to the baked `AOIDE_WALLPAPER` env store path so the
background survives reboots/rebuilds even though `stage/` is ephemeral. Swap is a
hard cut — no crossfade.

**Operating rule — never override `ExecStart` to a worktree.** For live QML
iteration run a *separate* foreground instance (`qs -p <worktree>/shell.qml`);
do **not** hijack the service's `ExecStart` with a systemd drop-in. A drop-in
that pins `ExecStart` to a worktree path defeats `ConditionPathExists` (which
watches only the baked path) once the worktree path is gone, crash-looping
the shell — no bar, no dock, no wallpaper. See
[[references/AOIDE-DEV-HANDOFF]] §7.

## Related

- [[Desktop-Architecture]]
- [[Session-Graph]]
- [[Gadget-Dock]]
- [[shellbridge]]
- [[Self-Ricing]]
- [[drachma]]
- [[Codebase]]
- [[Hyprland]]
- [[Widget-Bridge-Contract]]
