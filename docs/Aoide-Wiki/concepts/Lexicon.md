---
type: concept
created: 2026-07-26
updated: 2026-08-25
tags: [aoide, naming, meta, architecture]
---

# Lexicon — the Vocabulary, the Flow, and Why the Words

Every name in Aoide is load-bearing. This page explains the whole vocabulary at once: where each word family comes from, what it names, how the named things flow into each other. Three families — **the Muses** (who acts), **the snowflake** (what is frozen), **the song** (what is performed) — plus the **machinery words** connecting them.

## The Greek stuff — the three original Muses

Before the familiar nine, Greek tradition (Pausanias, describing the cult at Mount Helicon) knew only **three Boeotian Muses**:

| Muse | Greek | Domain | In this system |
|---|---|---|---|
| **Aoide** | Ἀοιδή | song, voice | the running, performing system — the orchestration core singing, headless or as the full AoideOS desktop |
| **Melete** | Μελέτη | practice, exercise | [[Melete]] — the coding agent, the **doer**: it practices, writes, builds, rehearses |
| **Mneme** | Μνήμη | memory | [[Mneme]] — the knowledge server, the **rememberer**: the vault, the wiki, what was learned |

The selection is the thesis: **song is what happens when practice and memory perform together.** A desktop that rices itself needs an actor that does (Melete), a store that remembers (Mneme), a body that sings what they make (Aoide). Melete and Mneme are independently-owned systems Aoide integrates and launches, not sub-components ([[Melete]], [[Mneme]]); the arrow can run the other way too — `melete aoide …` means Melete drives Aoide.

## Architecture is frozen music

The engraved thesis (after Goethe's *"Architektur ist erstarrte Musik"*) splits the system into two halves along one seam:

- **The frozen half** — the nix layer. Immutable, evaluated. The **score**: determines everything, performs nothing.
- **The performed half** — the running desktop. Live, hot-reloadable, ephemeral. The **performance**: what the score sounds like tonight, at this venue.
- **The seam** — [[livery (rename to lyra)]]. Livery values are frozen into the crystal at build time *and* sounded live at runtime (`stage/livery.json`, hyprctl, OSC). Both fan-outs derive from the same `aoide.livery`, so the baked theme and the live preview cannot drift.

Each half gets its own word family, so a sentence always names which side of the seam it stands on.

## The frozen family — snowflake morphology

Chosen because Nix's own logo is a snowflake, because crystals grow by local accretion from a nucleus outward — the same shape as the dendritic walker composing modules — and because no two crystals are alike: same physics (shared upstream flake), unique host instances. Full anatomy: [[Snowflake-Anatomy]].

| Term | Names | Why this word |
|---|---|---|
| **nucleus** | `modules/nucleus/` — daemon, CLI, policy | the seed crystal everything condenses around |
| **dendrite** | `modules/dendrites/` — opt-in feature branches | crystal branches grow outward by accretion; adding one never reshapes the core |
| **facet** | `modules/facets/` — render surfaces (quickshell, stylix, compositor) | the crystal's faces — the only planes that catch light, each reading only livery |
| **walker** | `lib/walk.nix` | walks the tree; every file under a walked dir self-registers, no import lists |
| **snowflake** | your clone | same physics as upstream, unique instance — the point of [[Clone-and-Run]] |

Radial distance from the nucleus encodes the mutation policy ([[Governance]]): the closer to the center, the more it belongs to upstream; the farther out, the more it is yours.

## The performed family — song vocabulary

Full map: [[Song-Vocabulary]]. A **rice is a song**: a thing the system *performs*, differently at each venue. One metaphor, extended consistently, is a namespace — if you know what a liner or a cover is for an album, you know what it is here.

| Term | Is | Term | Is |
|---|---|---|---|
| song | a rice | key | its palette |
| melody | semantic tier | component tier | bar / notif / window overrides |
| instruments | the facets that sound it | venue | the host performing it |
| cover | wallpaper | chimes | notification/system sounds |
| liner | per-song design notes | songbook | cross-cutting design memory |
| rehearsal | live preview (stage) | recording | adopted, committed, rebuilt |
| repertoire | committed songs, collectively | the standard | the shipped standard song (`sonata`) |
| replay | same song, new venue | transpose | same song, new key |
| stage | live state (gitignored) | auditions | the propose gate |

## The machinery words

Terms naming the connective tissue rather than either half:

- **conductor** — the ensemble's tool: `aoide conductor`, the TUI that watches running agent terminals and cues between them, Hyprland as the multiplexer. The conductor-class covers every surface with that duty: the Terminal Commander widget, the DAG gadget, the conductor itself. See [[Terminal-Commander]], [[Session-Graph]].
- **door** — an entry point into the one dispatch layer: `cli`, `mcp`, `daemon`, `a2a` (the TUI rides the cli door). The `a2a` door is bidirectional — aoide serves the protocol and speaks it as a client ([[A2A-Door]]). Every operation enters through a door and exits into the one audit log.
- **livery** — the design tokens AND the engine that dresses every surface in them ([[livery (rename to lyra)]], `crates/song/src/livery/`): one name for the whole token layer, resolved/validated/emitted by livery (`stage/livery.json`, hyprctl, OSC).
- **gate** — the rebuild gate ([[Rebuild-Gate]]): agents propose, the human admits. The single point where the performed half is allowed to re-freeze the crystal.
- **wiki / vault** — Mneme's memory surfaces: this wiki for design context, the vault for content.

## Why the seam is a livery

A **livery** is the single set of house colours a whole retinue wears in unison — a servant, a ship, a herald read at a glance as one household's. The industry term for this layer is *design tokens* (W3C design-tokens format, `CONTRACTS.md` §1), but a token names a single value; the engine dresses *every* surface in one song's identity, which a token alone doesn't capture. One word covers both the values and the act of stamping them.

The seam sits on neither of the two existing axes — the Greek axis (who acts) or the music axis (what is made and performed) — a seam-level name of its own. `aoide.arrangement` sits at the same seam, livery's structural sibling: livery is the song's DRESS (palette · base16 · component tiers · geometry · cover), arrangement is its STRUCTURE — which widget/surface TYPES a song brings into existence (`modules/nucleus/options.nix`), stored in the same `livery.json` and read under the same closed facet whitelist (`AGENTS.md` house rule 5).

## How it all flows

One loop, in the vocabulary:

1. **Melete practices.** The agent writes — a new song, a new dendrite branch, a new widget — entering through a door, landing in the audit log.
2. **The walker freezes.** The nix layer picks up what was written by accretion; the score now contains it.
3. **Rehearsal sounds it.** Before any rebuild, the live side performs the livery from `stage/livery.json` — quickshell surfaces and hyprctl repaint in place, the frozen side untouched.
4. **The gate records it.** On human admission, the rehearsed state bakes through the stylix facet and the compositor, committed to the clone. Rehearsal and recording derive from the same livery, so they cannot disagree.
5. **Mneme remembers.** The liner and songbook take the design decisions, the wiki takes the architecture; the next practice session starts from memory.
6. **Aoide sings.** The desktop is the sum of frozen score and live performance, and the loop starts again.

<!-- narrative -->
## Why a vocabulary at all

Because agents read this system as much as humans do. A consistent metaphor is compression: "transpose the standard and rehearse it at this venue" is a full, precise instruction in seven words, and every one of those words resolves to a literal path or command. The verbiage exists so that the pretty sentence *is* the technical sentence.

## Related

- [[Song-Vocabulary]] — the performed half, term by term
- [[Snowflake-Anatomy]] — the frozen half, layer by layer
- [[livery (rename to lyra)]] — the seam where they meet
- [[Melete]] · [[Mneme]] — the other two Muses
- [[Governance]] · [[Rebuild-Gate]] — the gate and the policy
