---
type: entity
created: 2026-07-25
updated: 2026-07-28
tags: [aoide, shell, ui, qml, quickshell]
source: "[[references/AOIDE-HANDOFF]]"
---

# Quickshell

The shell and UI runtime for Aoide, written in QML. It renders the complete shell surface: workspaces bar (with agent sessions and connection state), notification daemon (implementing `org.freedesktop.Notifications` natively), agent widgets, launcher, OSD, lockscreen, greeter, and wallpaper layer. This replaces the swaync / rofi / hyprlock / swww zoo with a single runtime.

Quickshell reads `stage/drachma.json` at runtime, so nearly the full arrangement — colors, typography, geometry, shell widgets — hot-reloads during rehearsal (preview) without a rebuild. GTK/Qt targets require app restarts and are adopt-only for preview purposes.

Communication discipline: Quickshell reads state files from shellbridge and issues commands via the unix socket. It never speaks an agent protocol or MCP directly.

A Quickshell NotificationServer spike (actions + inline reply) is planned for the yomi-strix session. If it lands, Quickshell keeps the full notification daemon role with no interim mako/swaync.

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
home-manager activation. Now that the host runs Aoide live, that deploy target
sits **untracked at the repo root** of the user's fork — the relationship
between the source tree (`modules/facets/quickshell/qml/`) and the deployed
copy needs a decision (open thread). Crucially, `hyprland.conf` is
owned by home-manager's `wayland.windowManager.hyprland`: the compositor facet
writes drachma + keybind fragments with `mkBefore`, and the Quickshell facet appends
its `exec-once` autostart with `mkAfter`, so the two facets compose the one config
file without collision.

## Nine surfaces

Two registered surfaces have grown real bodies, bringing the registry to
**nine**:

- **`sessionGraph`** (surface #9, `AoideSessionGraph.qml` + `GraphRow.qml`) —
  an overlay hot-reloading `song/stage/graph.json` on the same
  `FileView` pattern as `DrachmaState`, rendering the [[Session-Graph]] DAG as an
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

## Launcher (surface #3, built out 2026-07-28)

`AoideLauncher.qml` graduated from stub to a working, keyboard-driven app
launcher (the rofi replacement). It is a summoned `PanelWindow` on
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
  global, aoide:launcher`. This *replaced* the old `bind = …, exec, aoide shell
  launcher toggle`, which called an **unimplemented CLI stub** (`aoide shell *`
  is not in the command schema). `ShellBridge` is outbound-only, so an in-process
  global shortcut is the cleanest inbound trigger — no new `aoided` verb, no
  inbound socket.
- **Launch is `DesktopEntry.execute()`** — the same Quickshell-native side-effect
  idiom the shell already uses (`WorkspaceRow.activate()`, `BatonGadget` →
  `execDetached`), *not* a shell-out invented in QML. Routing app-launch through
  `aoided` per house rule #6 remains an open contract question (no such verb
  exists today); flagged, not silently baked. See [[references/AOIDE-DEV-HANDOFF]] §7.

The compositor facet also adds `aoide-launcher` to the blur / `ignore_alpha` /
hyprglass namespaces so the pane frosts like the bar and dock. Both the keybind
and the blur rules are baked into `hyprland.conf`, so the launcher needs a gated
`switch` to land live (the QML deploys to the read-only `~/Aoide/qml/` tree the
same way).

## Session service & resilience

The shell surface is started by the **`aoide-quickshell`** systemd *user*
service (not a Hyprland `exec-once`), defined in
`modules/facets/quickshell/default.nix`. The service is the session-assembly
seam: it orders after `graphical-session.target` (so Quickshell inherits a valid
Wayland env), logs to journald (`journalctl --user -u aoide-quickshell`), and
respawns on crash. Three layers keep one bad load from bringing the desktop down
for good:

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
source-of-truth is the live-watched `stage/cover.json` (written only by `aoide
rice preview`), falling back to the baked `AOIDE_WALLPAPER` env store path so the
background survives reboots/rebuilds even though `stage/` is ephemeral. Swap is a
hard cut — no crossfade.

**Operating rule — never override `ExecStart` to a worktree.** For live QML
iteration run a *separate* foreground instance (`qs -p <worktree>/shell.qml`);
do **not** hijack the service's `ExecStart` with a systemd drop-in. On
2026-07-28 a leftover drop-in pinned `ExecStart` to a `.claude/worktrees/…`
path that was later deleted, defeating `ConditionPathExists` (which still
watched the valid baked path) and crash-looping the shell 77× — no bar, no
dock, no wallpaper. The drop-in was the anti-pattern; the fix was to delete it
and let the baked unit stand. See [[references/AOIDE-DEV-HANDOFF]] §7.

## Related

- [[Desktop-Architecture]]
- [[Session-Graph]]
- [[Gadget-Dock]]
- [[shellbridge]]
- [[Self-Ricing]]
- [[drachma]]
- [[Codebase]]
- [[Hyprland]]
