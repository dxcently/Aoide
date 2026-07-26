# moonlight — Design Intent

**Song:** moonlight (committed rice; the replay fixture)
**Palette:** a cool nocturne — deep indigo base, moonlit silver text, cyan accent
**Cover:** none yet (v0)

---

## Why this song exists

moonlight is the second committed song, added to prove replayability: any host
in the fleet performs it with one line — `aoide.song = "moonlight";` — and the
whole `aoide.notes` fan-out swaps with zero other edits. It is deliberately a
distinct key from the shipped standard (Catppuccin Mocha) so the swap is
obvious at a glance.

## Host-agnostic by construction

This song sets ONLY `aoide.notes`. It names no host, enables no facet or
dendrite, touches no hardware or service. The venue (host) decides its
instruments; moonlight carries only the notes. That is exactly what lets one
score be performed on any host with its own specifics and its own enabled
facet/dendrite set (CONTRACTS.md §5).

## Palette

| Role   | Hex       | Intent |
|--------|-----------|--------|
| bg     | `#0b1021` | deep midnight indigo (base00) |
| fg     | `#c8d3f5` | moonlit silver text (base05) |
| accent | `#82aaff` | cool moon-cyan accent (base0D) |
| urgent | `#ff757f` | muted rose alarm (base08) |

Component tier is all-null (palette everywhere) — the key does the work, no
hidden overrides to undo before a transposition.

## Iteration Log

- 2026-07-26: initial commit as the replay fixture; palette-only, no cover.
