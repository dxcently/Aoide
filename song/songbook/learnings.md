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
