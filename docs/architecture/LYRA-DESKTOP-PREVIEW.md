# Contained desktop preview

Status: exploration and implementation candidate, not a completed Lyra feature.

Lyra's preview direction includes whole rice desktops in a contained graphical
surface, alongside individual widget previews. A terminal command may launch
that surface; a launcher is distinct from a terminal rendering the desktop.

## Observed mechanism

This preview idea was discovered through a bug, not an intentional preview
feature. A nested Hyprland instance loaded the normal desktop configuration,
published its display into the shared systemd/D-Bus user environment, and
restarted the shared session target. The normal Quickshell service consequently
rendered on the nested display instead of the primary desktop. Its contained
appearance suggested a whole-rice preview mode; the session hijack itself must
not become part of that implementation.

The supplied screenshot shows a rice inside a bounded region. A read-only live
Hyprland client inspection found a graphical client with class `aquamarine`,
title `aquamarine - WAYLAND-1`, and size 1060×936. Its PID 1910715 resolved to
`/run/current-system/sw/bin/hyprland`. This is evidence consistent with a nested
Hyprland desktop window, not evidence of native terminal QML rendering. The
display association was subsequently verified: the primary compositor used
`wayland-1`, the nested compositor used `wayland-2`, and both the user manager
environment and normal Quickshell process pointed at `wayland-2`. Restoring the
primary display environment and restarting only Quickshell returned its
wallpaper, bar and dock surfaces to the primary compositor. The normal config
also stops the shared session target on compositor shutdown, so retiring that
nested instance needs care.

## Candidate flow

```text
Lyra CLI / preview panel
          |
selected built rice + isolated preview state
          |
owned nested compositor with a chosen output size
          |
Quickshell surfaces + wallpaper + supported desktop configuration
          |
contained interactive desktop preview
```

Reuse the existing widget preview's composition selection, inspection and
annotation interfaces where applicable. Whole-desktop mode needs its own owned
display/session lifecycle; a QML widget canvas is not automatically a nested
desktop renderer.

Useful controls include output resolution/aspect presets, fit and zoom,
pointer/keyboard interaction versus annotation mode, screenshots and source
links, and switching between prepared rice bundles. Embedding the result inside
an existing panel requires separate capture/input integration proof; a working
standalone nested window does not prove embedding.

## Acceptance boundaries

- Preview state, sockets, display and process ownership are isolated from the
  live desktop. Closing a preview stops only its owned processes.
- No preview action reloads the host compositor or changes declared defaults.
- Existing application data, agent sessions and logs remain untouched.
- Host service controls and other real-system bridges must be explicitly
  constrained, simulated or disabled in preview; another display alone is not
  a security or side-effect boundary.
- Selected dependencies and unsupported backend behavior are visible.
- Verify input routing, resize, multiple surfaces, palette reload, cleanup and
  failure recovery using the live UI.
- Document compositor/backend limits. Do not claim full hardware or production
  desktop equivalence from a nested preview.

This is a candidate reuse of an observed windowed desktop, not authorization to
replace the current widget preview or introduce a terminal graphics protocol.
