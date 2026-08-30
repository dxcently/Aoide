---
type: entity
created: 2026-07-25
updated: 2026-08-29
tags: [aoide, shell, ui, qml, quickshell]
source: "[[references/AOIDE-HANDOFF]]"
---

# Quickshell

The shell and UI runtime for Aoide, written in QML. It renders the bar
(agent sessions, connection state), the notification popup + ledger, the
launcher, the desktop dock, and the wallpaper layer.

Quickshell reads `stage/livery.json` at runtime, so nearly the full rice —
colors, typography, geometry, shell widgets — hot-reloads during rehearsal
(preview) without a rebuild. GTK/Qt targets require app restarts and are
adopt-only for preview purposes.

Communication discipline: Quickshell reads state files from shellbridge and
issues commands via the unix socket. It never speaks an agent protocol or
MCP directly.

## Implementation

The QML skeleton is shipped in `modules/facets/quickshell/qml/`. `shell.qml`
(a `ShellRoot`) instantiates four shared singletons — **`LiveryState`**
(watches `stage/livery.json` via a `FileView`, re-binding every surface's
colours in one pass on an atomic replace, the hot-reload), **`ShellBridge`**
(the unix-socket client, the sole outbound channel from QML —
`focusSession(address)` → shellbridge → hyprctl, no MCP/HTTP/shell-exec from
QML), **`AoideIpc`** (the external-reload IPC target, below), and
**`StagingEngine`** (resolves which song dresses a slot and where its QML
lives) — plus a plain `QtObject` holding cross-widget session state (the
floating-gadget model, the hover-trace link).

Two surfaces are facet-owned `Item`s wrapped in their own `PanelWindow`:
**`AoideWallpaper`** (the background layer, `WlrLayer.Background`) and the
**bar** (`WlrLayer.Top`, exclusive height), whose content is a per-song
widget loaded through `WidgetSlot` (`slot: "bar"`) rather than a facet
component — the song owns its own footprint. **`AoidePanel`** (the
[[Gadget-Dock]]) owns its PanelWindow, layer, and toggle shortcut
internally. Three more surfaces are **`SurfaceSlot`**-rooted overlays —
`powermenu`, `launcher`, and `herald` (the notification popup) — each
resolving a song widget through the same baseline-fallback chain
`WidgetSlot` uses (song override → sonata, the shipped baseline every song
falls back to → nothing), but as a non-visual `QtObject` sibling instead of
a sized `Item`, since each slot's root IS its own `PanelWindow` rather than
content parented into an existing layout. `SongSurfaces` hosts one more
`SurfaceSlot` per surface-kind entry the active song's `aoide.arrangement`
declares (empty for sonata today, so a no-op). `AoideClipboard` and
`GrimoireLedger` are facet-owned utility singletons injected into the
launcher slot as extras; `AoideWallpaperPicker` is the `SUPER+W` wallpaper
switcher.

The repo root carries no `qml/` directory — widget source lives in
`modules/facets/quickshell/qml/` and each active song's `widgets/`, and the
facet's `home.activation.aoideDeployQml` rsyncs the built config tree
(`rsync -a --delete --chmod=u+w`) into `$AOIDE_ROOT/run/qml/` (default
`~/.aoide/run/qml/` — the runtime root, outside the git checkout), which
Quickshell reads as its entry point (`quickshell -p
$AOIDE_ROOT/run/qml/shell.qml`). The deployed tree is self-healing: hot-editing
QML directly under `$AOIDE_ROOT/run/qml/` previews live without a rebuild, and
every activation's rsync reasserts the store's build over any such edit —
the same "switch is the truth, hot edits are the sketch" discipline as
every other stage/preview seam. `hyprland.conf` is owned by
home-manager's `wayland.windowManager.hyprland`: the compositor facet
writes livery + keybind fragments with `mkBefore`, and the Quickshell
facet appends its `exec-once` autostart with `mkAfter`, so the two facets
compose the one config file without collision.

### IPC hot-reload — closing the dynamic-load gap

`LiveryState`'s `FileView` watch (above) covers exactly one tier:
`stage/livery.json`. Quickshell's own built-in file watcher — the thing that
would otherwise auto-reload on any QML edit — only tracks files reached
through a static `import` statement; every song/facet widget loads
dynamically via `Qt.createComponent(url)` (`WidgetSlot`/`SurfaceSlot`, see
[[Widget-Maker]]), which that watcher never sees. Facet-owned QML unreachable
by static import sits in the same blind spot — `shell.qml`, `ShellBridge.qml`,
`StagingEngine.qml`, `WidgetSlot.qml`, `SurfaceSlot.qml`. In practice this
meant editing an existing widget body (or any of those facet files) never
rendered until a full `systemctl --user restart aoide-quickshell.service` —
found via live debugging.

`AoideIpc.qml` closes it: a `Quickshell.Io.IpcHandler` singleton
(`target: "shell"`, one exposed `function reload(): void { Quickshell.reload(false) }`),
instantiated in `shell.qml` alongside `livery`/`bridge`/`stagingEngine`/`shared`
— no properties of its own, wired purely for the side effect. `quickshell ipc
call shell reload` invokes it from outside the process; `lyra quickshell reload`
(`crates/song/src/commands/quickshell.rs`, `crates/song/src/ipc.rs`) shells out to
exactly that call, no-oping to a reported (never fatal) `not-running` status
when `aoide-quickshell.service` isn't up.

`Quickshell.reload(hard: bool)` (`rootwrapper.cpp`'s `reloadGraph()`) is not a
selective "reload what changed" call — it always tears down and rebuilds the
entire scene fresh from `shell.qml`, re-executing every dynamic load along the
way, closer to an in-process restart than a targeted patch. `hard` only
decides whether `PersistentProperties`-marked state survives the rebuild;
this codebase declares none, so `hard:true` and `hard:false` are currently
observably identical — `AoideIpc.qml` passes `false` to match Quickshell's own
built-in auto-reload-on-static-file-change path (`onWatchedFilesChanged()`
also calls `reload(false)`), not because soft matters yet. One consequence
worth carrying forward: a reload resets *all* non-persistent QML state (tray
open/closed, calendar selection, any in-memory session data) exactly like a
restart does — it's just faster, and skips cycling the systemd unit and
re-registering windows with the compositor.

`lyra rice stage` (`crates/song/src/commands/rice.rs`) auto-triggers this
after `sync_song_widgets` reports actually-changed widget files — the
palette/notes tier is already covered by `LiveryState`'s own watch, so a
re-stage with no widget-body changes never reloads. Whether this also closes
the *separate* gap of a brand-new widget file needing a restart to be
discovered at all is unconfirmed — untested against a live instance as of
landing.

### Notification card — three-tier reading order

dunst owns notification delivery — the D-Bus bus name, history, stacking, and
pause levels — but draws nothing (`skip_display` on every rule,
`modules/dendrites/dunst.nix`). Quickshell owns the surface: the `herald`
popup slot and the dock's `herald-center` ledger, both drawn from
`stage/herald.json`. The per-card body
(`song/songbook/sonata/widgets/herald.qml`, the shipped baseline every song
falls back to) reads the `Notification` payload into three differentiated
tiers, top to bottom: the **program title** (a box-drawing frame), the
notification's own **title** (bold serif), and the **context** (dimmer,
indented behind a signature hairline). The program title resolves
`desktopEntry` (`.desktop` suffix stripped) → `appName` → `"notice"`, capped
at 28 characters. When a notification's body is empty and its summary is a
`Title: message` join — multi-word head, 6–59 characters, first `": "` — the
summary splits into title and context. The join shape is what
terminal-forwarded OSC-9 notifications produce (kimi emits `ESC ] 9 ;
title: body`; kitty's OSC 9 handler forwards the whole string as the title
with `app_name` set to `kitty`, the forwarder — the real program is not in
the payload). The card never renders a notification's implicit `default`
action (`identifier == "default"`), which spec senders like kitty attach to
every forwarded OSC-9/99 notification as the click-anywhere activation — it
showed up live as an empty outlined button. Real action buttons render as
before, with the laurel standout fill on the first real one. The ledger
line's right-hand ink is a live arrival clock (HH:MM, gold, refreshed every
30s while the card lives), and the urgency word appears only in the bottom
frame label.

## The registry — nine declared, five with a live body

The facet declares nine `owner = "quickshell"` surfaces
(`modules/facets/quickshell/default.nix`: bar, notifications, launcher, osd,
lockscreen, greeter, wallpaper, agentWidgets, sessionGraph). Five have a
live QML body today: **bar** (song `bar.qml` via `WidgetSlot`),
**notifications** (dunst as daemon, the `herald`/`herald-center` popup and
ledger as the drawn surface), **launcher** (song
`launcher.qml` via `SurfaceSlot`, below), **wallpaper** (`AoideWallpaper.qml`),
and **agentWidgets** (the [[Gadget-Dock]], `AoidePanel.qml`: a left-edge
panel holding four core self-framed gadgets — Conductor, Terminals, Meters,
Power — plus an opt-in claude.ai Usage stele; its fore-edge peeks past the
screen edge at rest, further when a session is `awaiting` and
unacknowledged, and slides fully in on a 6 px hot-edge hover or on `SUPER+G`,
an in-process Hyprland global shortcut the panel itself registers
(`aoide:dock`), not a CLI command).

Four are registry-only, with no QML anywhere in the repo:

- **`osd`**, **`lockscreen`**, **`greeter`** — each is a stand-down
  declaration against [[Stylix]] (`aoide.surfaces.<name>.owner =
  "quickshell"` tells Stylix not to theme its own equivalent target) with no
  facet or song QML implementing the surface. greetd launches Hyprland
  directly as a stub session command; the Quickshell greeter's greetd IPC
  session is unbuilt.
- **`sessionGraph`** — declared but has no QML body: the standalone DAG
  overlay this surface once named is no longer part of the QML tree.
  bare `aoide graph`/`--json` and the `aoide conductor` TUI are the DAG's
  renderers today ([[Session-Graph]]).

## Launcher

`song/songbook/sonata/widgets/launcher.qml` is a working, keyboard-driven
app launcher (the rofi replacement), loaded through the `launcher`
`SurfaceSlot`. It is a summoned `PanelWindow` on `WlrLayer.Overlay`
(namespace `aoide-launcher`) that takes an exclusive keyboard grab only
while shown and is an inert zero-cost layer at rest. It enumerates apps
from Quickshell's built-in `DesktopEntries`, filters on a prefix-ranked
case-insensitive substring as you type, navigates with Up/Down + Ctrl+J/K,
launches on Enter/click, dismisses on Escape / scrim-click. It is a
Pantheon pane (`GadgetFrame`, cream Aero glass, `♪` prompt, lowercase
`launcher.summon` callout), colours strictly from `livery`.

Two design decisions worth carrying forward:

- **Trigger is a Hyprland `GlobalShortcut` (`aoide:launcher`), registered
  in-process** — the compositor binds `SUPER+SPACE` to it via `bind = …,
  global, aoide:launcher`. `ShellBridge` is outbound-only, so an in-process
  global shortcut is the cleanest inbound trigger — no new `aoided` command, no
  inbound socket.
- **Launch is `DesktopEntry.execute()`** — the same Quickshell-native side-effect
  idiom the shell already uses (`WorkspaceRow.activate()`, `ConductorGadget` →
  `execDetached`). Routing app-launch through `aoided` per house rule #6 has
  no such command today; flagged, not silently baked. See
  [[AOIDE-DEV]] §7.

The compositor facet also adds `aoide-launcher` to the blur / `ignore_alpha` /
hyprglass namespaces so the pane frosts like the bar and dock. Both the keybind
and the blur rules are baked into `hyprland.conf`, so the launcher needs a gated
`switch` to land live (the QML rsyncs into `$AOIDE_ROOT/run/qml/` the same way).

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
- `StartLimitIntervalSec = 60` / `StartLimitBurst = 5` — the
  backstop for the gap the `ConditionPathExists` guard can't cover: a
  `shell.qml` that *exists but won't load* (a QML parse error, or an `ExecStart`
  aimed elsewhere by a stray drop-in) exits 255 every respawn, so without a
  limit `Restart=on-failure` thrashes forever. After 5 failures inside 60s
  systemd parks the unit `failed`. Five tries still absorbs a transient failure
  (e.g. Wayland not ready yet).

None of the three catch the placeholder-screen lockup: after a transient
output blip, Qt's wayland QPA backend can fall onto an internal placeholder
screen and never reattach even once the real output returns. The process
stays `active` — no crash, no exit — so systemd has nothing to restart on,
and the desktop (bar/dock/wallpaper/herald) sits bare until something
restarts the unit. `Quickshell.screens` is populated below QML by
`QGuiApplication`'s wayland platform plugin, so no in-process
`Quickshell.reload()`/`onScreensChanged` handler can reach or reset the
stuck state — only a full process re-exec does, which is what the watchdog
below provides.

**`aoide-quickshell-healthcheck.timer`**
(`modules/facets/quickshell/default.nix`, gated on `aoide.lyra.enable`) is
the live watchdog: a oneshot `systemd.user.service` running `lyra quickshell
healthcheck` (`pkgs/aoide/crates/song/src/health.rs`, [[Meta-and-Upkeep]])
20s after the session comes up and every 15s thereafter.
**`home.activation.aoideVerifyRice`** runs the same check again, 5s after
every rebuild's `aoideRestartRice` bounce, so a `switch` reports which of
the two happened — painted, or dropped straight back into the lockup its
own restart was meant to clear — instead of finishing green over a bare
desktop.

The check requires two independent signals before it acts, scoped to the
unit's own `ActiveEnterTimestamp` so a recovered occurrence can never
re-trigger after a restart moves that timestamp forward: the journal
(`There are no outputs - creating placeholder screen`, Qt's own line)
confirms the event happened; `hyprctl layers -j` reporting zero
`aoide-`-namespaced surfaces anywhere confirms it is still true right now. A
failed or unparsable `hyprctl` call reads as unconfirmed, never as the stuck
signal. The surface count is summed system-wide, never per-monitor — no
`PanelWindow` in `modules/facets/quickshell/qml/` binds to a screen, so this
shell always paints exactly one output, and every other enabled monitor
legitimately and permanently carries zero layers by design.

Recovery runs on a retry ladder: minimum gaps of 0s, 15s, 60s, 300s indexed
by how many restarts already sit in the last hour, settling on a 900s floor
it repeats indefinitely — a floor, never a stop, so a genuinely flapping
output is never abandoned. An hour of quiet
resets the ladder to its bottom rung. A notification fires once, only when
the ladder reaches its floor, reporting the output connection as suspect.
The journal carries the authoritative record of what happened: dunst is
configured `skip_display` and only forwards to the herald ledger, which the
stuck shell itself draws, so the notification cannot render while the
failure it describes is still live. `AoideIpc.qml` logs every
`Quickshell.screens` change (`onScreensChanged`) to the journal as a further
breadcrumb — the screen list collapsing at the moment of the lockup, not a
fix; no QML-side fix is possible for the reason above.

**The wallpaper engine *is* this shell.** The wallpaper is a
`wlr-layer-shell` `Background` surface (`AoideWallpaper.qml` inside `shell.qml`),
the sole live painter (Stylix's `hyprpaper` is force-disabled via the
`aoide.surfaces.wallpaper` owner registry; no `swww`/`swaybg`/`mpvpaper`
elsewhere). So a shell crash takes the wallpaper *and* the bar/dock/gadgets with
it in one stroke — they are one process, not four. The wallpaper's own
source-of-truth is the live-watched `stage/cover.json` (written by `lyra rice
stage` when the song's cover derives or, for a direct hot-swap, `lyra cover set <path-or-name>`), falling
back to the baked `AOIDE_WALLPAPER` env store path so the
background survives reboots/rebuilds even though `stage/` is ephemeral. Swap is a
hard cut — no crossfade.

**Operating rule — never override `ExecStart` to a worktree.** For live QML
iteration run a *separate* foreground instance (`qs -p <worktree>/shell.qml`);
do **not** hijack the service's `ExecStart` with a systemd drop-in. A drop-in
that pins `ExecStart` to a worktree path defeats `ConditionPathExists` (which
watches only the baked path) once the worktree path is gone, crash-looping
the shell — no bar, no dock, no wallpaper. See
[[AOIDE-DEV]] §7.

## Related

- [[Desktop-Architecture]]
- [[Session-Graph]]
- [[Gadget-Dock]]
- [[shellbridge]]
- [[Self-Ricing]]
- [[livery]]
- [[Codebase]]
- [[Hyprland]]
- [[Widget-Bridge-Contract]]
- [[Meta-and-Upkeep]]
