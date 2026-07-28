---
type: concept
created: 2026-07-26
tags: [aoide, naming, meta, architecture]
---

# Lexicon — the Vocabulary, the Flow, and Why the Words

Every name in Aoide is load-bearing. This page is the one place that explains the whole vocabulary at once: where each word family comes from, what it names, and how the named things flow into each other. The families are three — **the Muses** (who acts), **the snowflake** (what is frozen), and **the song** (what is performed) — plus the small set of **machinery words** that connect them.

## The Greek stuff — the three original Muses

Before the familiar nine, Greek tradition (Pausanias, describing the cult at Mount Helicon) knew only **three Boeotian Muses**:

| Muse | Greek | Domain | In this system |
|---|---|---|---|
| **Aoide** | Ἀοιδή | song, voice | the running, performing system — the orchestration core singing, whether headless or as the full AoideOS desktop (not "the desktop" as such: the core runs shell-only too) |
| **Melete** | Μελέτη | practice, exercise | [[Melete]] — the integrated (not vendored) coding agent, the **doer**: it practices, writes, builds, rehearses |
| **Mneme** | Μνήμη | memory | [[Mneme]] — the integrated (not vendored) knowledge server, the **rememberer**: the vault, the wiki, what was learned |

The selection is the thesis: **song is what happens when practice and memory perform together.** A desktop that rices itself needs an actor that does (Melete), a store that remembers (Mneme), and a body that sings what they make (Aoide). The three original Muses are reunited as one system — the software wears the oldest names for the three faculties it actually has.

The naming is a **theme**, though — not an architectural claim. Melete and Mneme are **independently-owned systems** Aoide integrates and launches (the nucleus carries a melete-adapter; `pkgs/{melete,mneme}` are launchers for their self-updating runtimes; the wiki you are reading is served through Mneme). They are the other two thirds of the *name*; they are not sub-components of the Aoide program, and the arrow can run the other way — `melete aoide …` means Melete drives Aoide. The three-Muses trio is why the words fit, not proof the software is one binary.

## Architecture is frozen music

The engraved thesis (after Goethe's *"Architektur ist erstarrte Musik"* — architecture is frozen music) splits the system into two halves along one seam:

- **The frozen half** — the nix layer. Immutable, crystalline, evaluated. It is the **score**: it determines everything and performs nothing.
- **The performed half** — the running desktop. Live, hot-reloadable, ephemeral. It is the **performance**: what the score sounds like tonight, at this venue.
- **The seam** — [[Notes]]. Note values are frozen into the crystal at build time *and* sounded live at runtime (`stage/drachma.json`, hyprctl, OSC). Both fan-outs derive from the same `aoide.drachma`, so the baked theme and the live preview cannot drift.

Each half gets its own word family, so you always know which side of the seam a sentence is standing on.

## The frozen family — snowflake morphology

Chosen because Nix's own logo is a snowflake, because crystals grow by **local accretion from a nucleus outward** — exactly how the dendritic walker composes modules — and because no two crystals are alike: same physics (shared upstream flake), unique host instances. See [[Snowflake-Anatomy]] for the full anatomy; the map:

| Term | Names | Why this word |
|---|---|---|
| **nucleus** | `modules/nucleus/` — daemon, CLI, policy | the seed crystal everything condenses around |
| **dendrite** | `modules/dendrites/` — opt-in feature branches | crystal branches grow outward by accretion; adding one never reshapes the core |
| **facet** | `modules/facets/` — render surfaces (quickshell, stylix, compositor) | the crystal's faces — the only planes that catch light (render appearance), each reading only notes |
| **rime** | `modules/rime/` — rice engine + shipped default | the deposited aesthetic layer on the crystal's surface; also the pun on **rice** |
| **walker** | `lib/walk.nix` | walks the tree; every file under a walked dir self-registers, no import lists |
| **snowflake** | your fork | same physics as upstream, unique instance — the point of [[Fork-and-Run]] |

Radial distance from the nucleus encodes the mutation policy ([[Governance]]): the closer to the center, the more it belongs to upstream; the farther out, the more it is yours.

## The performed family — song vocabulary

The full map lives in [[Song-Vocabulary]]; the logic of the family here. A **rice is a song**: not a static config but a thing the system *performs*, differently at each venue. Once that identification is made, the rest of the vocabulary falls out mechanically — which is the reason it was selected: **one metaphor, extended consistently, is a namespace.** Nobody has to invent or memorize arbitrary names; if you know what a liner or a cover is for an album, you know what it is here.

| Term | Is | Term | Is |
|---|---|---|---|
| song | a rice | key | its palette |
| melody | semantic note tier | arrangement | component note tier |
| instruments | the facets that sound it | venue | the host performing it |
| cover | wallpaper | chimes | notification/system sounds |
| liner | per-song design notes | songbook | cross-cutting design memory |
| rehearsal | live preview (stage) | recording | adopted, committed, rebuilt |
| repertoire | committed songs | the standard | the shipped default song |
| replay | same song, new venue | transpose | same song, new key |
| stage | live state (gitignored) | backstage / auditions | runtime plumbing / propose gate |

## The machinery words

A few terms name the connective tissue rather than either half:

- **door** — an entry point into the one dispatch layer: `cli`, `mcp`, `daemon` (and the TUI rides the cli door). Every operation enters through a door and exits into the one audit log. One body, several doors.
- **gate** — the rebuild gate ([[Rebuild-Gate]]): agents propose, the human admits. The single point where the performed half is allowed to re-freeze the crystal.
- **drachma** — the design tokens AND the engine that mints them ([[drachma]], `pkgs/drachma`): the Greek coin, one name for the whole token layer. "Notes" and "drachma" are one thing (not a values-vs-engine split): the tokens are drachma, resolved/validated/emitted by drachma (`stage/drachma.json`, hyprctl, OSC).
- **baton** — the conductor's tool: `aoide baton`, the TUI that watches the ensemble of running agent terminals and cues between them, with Hyprland as the multiplexer (real windows, not panes). The wider **conductor-class** covers every surface with that duty — the Terminal Commander widget, the DAG gadget, the baton. See [[Terminal-Commander]], [[Session-Graph]]. (Prior art for the behavior: the herdr multiplexer — an external tool, not part of this vocabulary.)
- **wiki / vault** — Mneme's memory surfaces: this wiki for design context, the vault for content.

## How it all flows

One loop, told in the vocabulary:

1. **Melete practices.** The agent writes — a new song in `song/repertoire/`, a new dendrite branch, a new widget. Everything it does enters through a door and lands in the audit log.
2. **The walker freezes.** The nix layer picks up what was written by accretion — dendrites and songs self-register, no import lists — and the score now contains it.
3. **Rehearsal sounds it.** Before any rebuild, the live side performs the notes from `stage/drachma.json` — quickshell surfaces and hyprctl repaint in place. The frozen side is untouched; this is the performance testing the score.
4. **The gate records it.** If the human admits the rebuild, the rehearsed state is recorded — baked through the stylix facet and the compositor, committed to the fork. Rehearsal and recording derive from the same notes, so they cannot disagree.
5. **Mneme remembers.** The liner and songbook take the design decisions; the wiki takes the architecture; the next practice session starts from memory instead of from zero.
6. **Aoide sings.** The desktop is the sum of frozen score and live performance — and the loop starts again, one radial layer at a time.

<!-- narrative -->
## Why a vocabulary at all

Because agents read this system as much as humans do. A consistent metaphor is compression: "transpose the standard and rehearse it at this venue" is a full, precise instruction in seven words, and every one of those words resolves to a literal path or command. The verbiage was not selected to be pretty — it was selected so that the pretty sentence *is* the technical sentence.

## Related

- [[Song-Vocabulary]] — the performed half, term by term
- [[Snowflake-Anatomy]] — the frozen half, layer by layer
- [[Notes]] — the seam where they meet
- [[Melete]] · [[Mneme]] — the other two Muses
- [[Governance]] · [[Rebuild-Gate]] — the gate and the policy
