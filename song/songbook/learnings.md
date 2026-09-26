# Songbook Learnings

Cross-cutting observations that apply across songs, not any one song's
`design/` folder. The agent reads this before every ricing iteration and
appends after declare/reject decisions (the write-back that is the "self" in
Self-Ricing). Sparse today — this is the first entry.

## `default` (retired 2026-08-14) — the Windows-7/Aero aesthetic

The shipped standard song was originally named `default`, not `sonata`.
Retired outright when `sonata` became the shipped baseline; its files are
git-recoverable (last present at `song/songbook/default/`), not preserved
in the tree.

What it recorded, distilled from its `design/intent.md` Iteration Log:

- **Palette:** Catppuccin Mocha (base16) — chosen for wide ecosystem
  support (Stylix consumes it natively), legible bg/fg contrast, and being
  well-represented in agent training corpora (so an agent composing a new
  song `--from default` could reason about transpositions by name).
- **Component tier:** left fully null — every surface fell back to the
  palette, a deliberately clean baseline to compose from.
- **Aesthetic:** a Windows-7-sidebar homage — the gadget dock's right-edge
  column of gadgets (TERMINALS/DAG/CLOCK/METERS) in ASCII/box-drawing chrome
  (`╔═[ TITLE ]═╗`, tree-limb rules, `[▓▓▓░░░]` gauges), realized as what
  later became the Gadget-Dock. This is the aesthetic the user declared and
  the agent was meant to inherit across generations.
- **Design grammar:** `default` also owned the cross-cutting Pantheon
  wireframe-depth + glyph grammar (hollow 3D outline stacks, neon leaders, a
  vanishing point) layered over the Aero-glass base — relocated, not
  deleted, to `docs/Aoide-Wiki/references/pantheon/pantheon-grammar.md` as
  historical reference. `sonata` draws its own, deliberately divergent
  grammar instead (`song/songbook/sonata/design/greek-grammar.md`).

Full history — every dated log line, the actual `rice.nix`/`livery.json` — is
in git; `git log --follow -- song/songbook/default/` finds it.

## `cadenza` staged live (2026-09-26)

- **`rice stage <song>` reads the RUNTIME songbook**
  (`$AOIDE_ROOT/song/songbook/<song>/widgets`), not the checkout. A song
  written straight into a checkout stages its stale compose scaffold until
  the runtime copy is refreshed from the committed tree. The slot owner map
  (`manifest.json`) comes from `nix eval` of `AOIDE_FLAKE_ROOT`, so it only
  sees committed files.
- **`rice stage <song>` does not survive a `RICE` toggle.** The bar's RICE
  cell runs `rice mode declarative` then `rice mode stage`, and the latter
  re-stages the song recorded in `stage/mode.json`, which `rice stage`
  left on the previous song. Pin with `rice mode stage <song>` instead.
- **Staging swaps every slot a song provides at once.** "One surface at a
  time" is a checking order, not a staging order.
- **The preview canvas cannot catch edge clipping.** A pane whose title is
  cut into its top rule at y=0 loses the glyph tops at the window edge; the
  canvas shot looks the same, so it passed review. Give a surface root a
  half-cell top inset.
- **Measured at rest, live:** quickshell 1.4% CPU over 10s and 266 MB RSS
  with tier-0 glow on, under a 99%-busy machine (noisy). Glow stays on.
- **Derived ties are empty on a normal desktop.** Deriving jack links from
  `graph.json` + `sessions.json` `windowAddress` works, but most spawned
  children are windowless subagents, so no edge joins two jacks. The
  switchboard stays bare live until an agent spawns a windowed session on
  another workspace or core publishes `ties`; prove the derivation with
  edges added in the preview root, and say so.
- **A wallpaper is a cover until song wallpaper slots are hosted.**
  Generate it with a helper in the preview (fixed seed, exact viewport
  size), commit the PNG, and `lyra cover set` the runtime songbook's copy.
- **An inner glow lit from the resting rule colour is invisible.** At
  0.10 alpha of `dim` on CRT black the edge rose by ~6/255. Light the glow
  from `title` and let alpha, not colour, carry rest vs focus.
