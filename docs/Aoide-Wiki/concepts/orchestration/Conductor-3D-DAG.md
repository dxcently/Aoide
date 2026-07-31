---
type: concept
created: 2026-07-27
updated: 2026-07-28
tags: [aoide, design, conductor, dag, tui, pantheon]
source: "[[references/AOIDE-HANDOFF]]"
---

# Conductor 3D DAG — the daemon-net view (plan)

**Status: PLANNED (khoa, 2026-07-27). Not yet implemented — this is the build plan.**

## Vision

The conductor DAG panel renders the session graph the way Pantheon renders its daemon net (refs: `references/pantheon/art-direction/`, especially the wireframe-cube field still — `3ab9463d-*.png`): **hollow 3D wireframe boxes floating in perspective space**, labels beside their boxes, **lines connecting entities through 3D**, color carrying meaning. Terminal-native, in ratatui.

Color roles (same drachma/base16 contract as the rice):

| element | color | base16 |
|---|---|---|
| project boxes (back plane) | violet | base0E |
| session boxes | wireframe cyan | base0C |
| infrastructure/idle clusters | hologram periwinkle (dim) | base0D |
| traced/selected node + its edge run | optic-nerve green | base0B / palette.hot |
| urgent/failed session | glitch pink | base08 |
| far-depth dimming | grayscale ramp | base03/04 |

## Why this is hard (and why it's tractable)

A terminal has no GPU pipeline — but ratatui's `canvas::Canvas` with `Marker::Braille` gives a 2×4-dots-per-cell raster and already rasterizes arbitrary `Line` shapes, and `ctx.print` places text at canvas coordinates. So the only genuinely new machinery is a **tiny software 3D projector**: world-space boxes → camera transform → perspective divide → canvas coords. No hidden-surface removal needed — the aesthetic IS wireframe; depth is communicated by dimming, not occlusion.

## Architecture (pkgs/aoide, new module `src/conductor/spatial.rs` + rework of `graphview.rs`)

1. **Math core (`spatial.rs`, hand-rolled, zero new crates)** — `Vec3`, a 4×4 (or 3×4) transform, orbit camera (yaw/pitch/distance around scene centroid), perspective projection with the vanishing point at canvas center (matches the rice's vanishing-point rule). ~120 lines + unit tests that project known cube corners to known coords.
2. **Scene layout** — deterministic DAG→3D placement from `graph::build_graph` (never re-derived): projects on a deep back plane (z = far), their sessions mid-field, spawned children forward; x spread by sibling index, y by lane/grouping. Node = 12-edge hollow box sized by kind; label anchored at the box's top-left projected corner (declutter rule below).
3. **Renderer** — painter's sort by centroid depth (far first), each box's edges drawn as `canvas::Line`s with a per-depth color tier (far → base03 gray, near → full role color); edges between nodes are 3D polylines with one elbow (the Pantheon arc read); the traced node's box + its full edge run to root render palette.hot LAST (on top, full-bright). Labels via `ctx.print` — at mini sizes only the selected/traced + project labels print (declutter).
4. **Camera & interaction** — default ¾ orbit view; `h/l` yaw, `j/k` pitch (shifting the existing j/k selection keys to `n/p`), `+/-` zoom, `r` reset, slow idle drift (~0.5°/tick) that pauses on input and is disabled in `--mini`. Existing selection/Enter-to-cue/prune/emit contracts unchanged.
5. **Tests** — `spatial` unit tests (projection math) + `TestBackend` buffer snapshots (a 2-project/3-session fixture from seed.sh renders: N box-edge braille cells present, hot node's green cells present, label text at expected cells). The 2D view stays as a toggle (`v`) and its tests stay green.

## The widget embed ("a small version in a terminal")

Constraint: quickshell 0.3.0 ships no terminal-emulator QML component, and the widget discipline is files-not-processes. Three paths:

- **A (recommended, the embedded gadget):** conductor gains `--panel dag --mini` (compact, no idle drift, labels decluttered) and a `--watch --out <file>` frame-writer mode: on graph.json/trace change it renders ONE frame as ANSI text into `song/stage/dagframe.ans` (atomic write). The quickshell DAG gadget FileView-watches that file and renders it through a small `AnsiText` QML component (SGR 16/256-color subset mapped onto drachma colors). The widget stays passive (no process spawning); the frame-writer runs as a tiny user service or under aoided.
- **B (the full view, exists today as click-through):** the gadget's click spawns kitty running `aoide conductor` (full TUI, real pty). A `--panel dag` start flag lands the user directly in the 3D view. Optionally a positioned floating kitty (`--class aoide-dagpane` + hyprland windowrule) as a pseudo-embed.
- **C (blocked):** the DAG embeds the pty directly and the ANSI parser is deleted. **Status:** blocked on a terminal-emulator QML component in Quickshell — absent as of 0.3.0 (the constraint above).

Ship A + B. C supersedes A's parser and depends on nothing but that upstream component.

## Phases

1. `spatial.rs` + unit tests (pure math, no UI).
2. Canvas braille renderer: boxes/lines/labels/depth-dim, behind a `v` view toggle alongside the current 2D graphview.
3. Layout + interaction + color roles from stage drachma (base16 tier).
4. `--mini`, `--once`/`--watch --out` frame modes + the quickshell AnsiText gadget path.
5. Polish: idle drift, declutter tuning, TestBackend snapshots hardened.

Risks: braille legibility at gadget sizes (mitigate: mini mode drops to fewer, larger boxes); label overlap (declutter rule); none performance-shaped (dozens of nodes, redraw on change only).

*Related: the Pantheon grammar (`song/songbook/default/design/pantheon.md`) — the rice-side language this must rhyme with — [[Song-Anatomy]], entities/aoide-cli, CONTRACTS.md exit codes/door discipline (unchanged by this work).*
