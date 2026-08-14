---
type: concept
created: 2026-07-25
updated: 2026-08-13
tags: [aoide, desktop, compositor]
source: "[[references/AOIDE-HANDOFF]]"
---

# Desktop Architecture — Compositor, Shell, Bridge

## Compositor

[[Hyprland]] is the compositor and the **only multiplexer**. There is no PTY layer — terminal sessions are windows, not nested multiplexers.

## Shell: Quickshell

[[Quickshell]] is the complete shell surface. It covers:

| Surface | Role |
|---|---|
| Bar | Workspaces, agent sessions, connection state |
| Notification daemon | Native `org.freedesktop.Notifications` implementation |
| Agent widgets | Per-session status and controls |
| Launcher | Keyboard-driven app launcher (`AoideLauncher.qml`): enumerates apps via Quickshell `DesktopEntries`, fuzzy-filters, launches via `DesktopEntry.execute()`; triggered by `SUPER+SPACE` as an in-process Hyprland global shortcut |
| OSD | On-screen display |
| Lockscreen | User-facing lock |
| Greeter | Login greeter |
| Wallpaper layer | Managed wallpaper display |

No mako, swaync, rofi, or hyprlock — Quickshell holds the entire role. Styling comes exclusively from livery; structural patterns draw from unixporn canon (quickshell/ags bars, swaync-style centers, anyrun launchers) but are never copied directly.

## Bridge: shellbridge

[[shellbridge]] is the integration layer between aoided, Quickshell, and Hyprland:

- **Out:** atomic JSON state files written to `stage/` (Quickshell reads these)
- **In:** unix socket commands (QML issues requests here)
- **IPC:** Hyprland IPC consumed by shellbridge, not by QML

**Rule:** no MCP in QML, ever. QML is a display layer; intelligence lives in [[aoided]] and [[shellbridge]].

## Session jump flow

```
widget click → shellbridge unix socket → hyprctl dispatch focuswindow address:…
```

A click in the bar widget reaches the correct terminal window in one hop, with no polling or secondary lookup. This flow — plus a compositor keybind — backs the [[Terminal-Commander]] widget, the live roster of agent-running terminals.

## Event path and security

[[aoided]] emits a neutral event stream. Thin per-agent adapters translate events into agent-specific actions (e.g., melete-adapter → job dispatch). Subscriptions are **default-deny per event class**: an OSD flood cannot trigger unbounded agent runs.

**Trust boundary:** forwarded notification text is untrusted input. Adapters wrap it as data and never execute it as a command. An app title must never be able to command the agent — this is a hard architectural constraint, not a lint suggestion.

## Related

- [[Quickshell]]
- [[Hyprland]]
- [[shellbridge]]
- [[aoided]]
- [[Agent-Interface]]
- [[Terminal-Commander]]
- [[Controls]]
