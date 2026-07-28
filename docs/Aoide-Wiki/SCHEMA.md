# SCHEMA — Aoide-Wiki

This is the knowledge wiki for the **Aoide** project on this machine. When you pick up Aoide work, **read the whole wiki** — it is kept small and self-contained for exactly that. This file is its constitution: what the wiki is, the shape it holds, and how it is kept and changed.

## What this is

The standalone wiki for the Aoide project — the agent-agnostic Hyprland/Quickshell desktop framework. One project, one wiki: this is **not** a registry of many projects. Plain markdown, readable in any viewer, authored for agents.

## Read the whole thing

Working entry cost, in order: `Overview.md` → every page under `concepts/`, `entities/`, and `songbook/`. `ingest/index.md` is the catalog if you need to jump; `references/` holds source material (`AOIDE-HANDOFF.md` — the design contract; `AOIDE-DEV-HANDOFF.md` — the development agent's operating manual; `pantheon/` — the design reference stills). The wiki is deliberately small enough to read end to end. Note: `songbook/` mirrors **song-agent design memory** whose home is the songbook under `song/` ([[Song-Anatomy]]); the migration is an Open Thread in `ingest/log.md`.

## The shape

```
Aoide-Wiki/
  SCHEMA.md        ← you are here: this wiki's constitution + note manifest
  Overview.md      ← the Aoide overview / hub
  concepts/        ← ideas, mechanisms, frameworks
  entities/        ← named things (components, tools, hosts)
  songbook/        ← design language + build specs (song-agent design memory mirrored here; home is song/songbook/)
  ingest/
    index.md       ← content catalog
    log.md         ← append-only history + Open Threads
  references/      ← raw sources (AOIDE-HANDOFF.md, AOIDE-DEV-HANDOFF.md, pantheon/ stills)
  protocol/        ← the wiki protocol (Mneme/Melete-owned), staged here; not Aoide content
```

## The wiki protocol

This shape is not bespoke — it comes from the **wiki protocol**, a shipped shape + rule set that Mneme/Melete use to mint and keep project wikis. Aoide's own wiki is the self-managed exception: it lives in the Aoide repo. Mneme/Melete own the protocol; it is staged under `protocol/` and the relocation to them is an Open Thread in `ingest/log.md`. See [[Wiki-Protocol]] for the concept and `protocol/PROTOCOL.md` for the mint/convert procedure. The rules this wiki is kept by live in `protocol/OPERATIONS/`: [[Ingest]], [[Indexing]], [[Lint]], [[Self-Update]], [[Frontmatter]], [[Naming]], [[Wikilinks]], [[Assertion]].

## Two ways you act

**Keeping** — ingest sources, update the index, lint, add crosslinks, log. Act alone. See [[Ingest]], [[Indexing]], [[Lint]].

**Self-update** — change a rule (a `protocol/OPERATIONS/` page) or this file. Run [[Self-Update]]. The user signs off when the change is irreversible.

## What you never do

- Invent frontmatter values you cannot verify from source.
- Delete a source in `references/`.
- Rewrite prose wrapped in sentinel comments (`<!-- BEGIN SENTINEL … -->` … `<!-- END SENTINEL -->`); see [[Ingest]].
- Mint a stub page just to clear a red wikilink — a red link is a breadcrumb, not an error.
- Let the Notes manifest below drift from the files on disk.
- Write history, a rejected alternative, or a what-if into a content page. Pages state the system at HEAD in the present indicative; history goes to `ingest/log.md`, open questions to its `## Open Threads`, actionable work to the dev handoff ledger. See [[Assertion]].
- Invent a new top-level directory — in this wiki OR in the Aoide repo. Both roots are closed. New content lands inside the existing tree at its designated place; look the place up (repo content paths in [[Song-Vocabulary#The Song Map]], repo shape in `CONTRACTS.md` §2). A new root directory is a contract change, not a convenience.

## You maintain yourself

This file and every `protocol/OPERATIONS/` page obey the rules they describe. Changing what a rule says is a self-update ([[Self-Update]]); the self-description test binds this file too — if the shape section or the manifest no longer matches disk, that is a lint failure.

## Notes manifest

Snapshot of this wiki's files, diffed on each lint pass and rewritten at the end. The wiki is meant to be read whole; this manifest exists for the lint self-description check, not to spare you the reading.

snapshot: 2026-07-28

### Tags

agent · aoide · architecture · auto-discovery · base16 · baton · bridge · cli · coding-agent · compositor · conductor · content · daemon · dag · declarative · deployment · design · desktop · drachma · dxflake · extensibility · features · flake · gadget · glyph · governance · graph · harness · hyprland · integration · ipc · knowledge · mcp · melete · meta · mneme · naming · nix · node · onboarding · orchestration · orchestrator · pantheon · pipeline · policy · protocol · pty · qml · quickshell · rebuild · rice · rust · security · session · shell · song · stylix · terminal · theming · tui · ui · vault · wayland · widget · wiki

### Notes

```
Overview.md
SCHEMA.md
concepts/Agent-Interface.md
concepts/Codebase.md
concepts/Content-Pipeline.md
concepts/Desktop-Architecture.md
concepts/Feature-Set.md
concepts/Fork-and-Run.md
concepts/Full-Architecture.md
concepts/Gadget-Dock.md
concepts/Governance.md
concepts/Lexicon.md
concepts/Rebuild-Gate.md
concepts/Self-Ricing.md
concepts/Session-Graph.md
concepts/Snowflake-Anatomy.md
concepts/Song-Anatomy.md
concepts/Song-Vocabulary.md
concepts/Terminal-Commander.md
concepts/Widget-Maker.md
concepts/Wiki-Protocol.md
entities/Agent-Hooking.md
entities/Hyprland.md
entities/Melete.md
entities/Mneme.md
entities/Quickshell.md
entities/Stylix.md
entities/aoide-cli.md
entities/drachma.md
entities/aoided.md
entities/dxflake.md
entities/shellbridge.md
songbook/Baton-3D-DAG.md
songbook/Conductor-Channel.md
songbook/Pantheon-Grammar.md
songbook/Ricing-Protocol.md
ingest/index.md
ingest/log.md
protocol/OPERATIONS/Assertion.md
protocol/OPERATIONS/Frontmatter.md
protocol/OPERATIONS/Indexing.md
protocol/OPERATIONS/Ingest.md
protocol/OPERATIONS/Lint.md
protocol/OPERATIONS/Naming.md
protocol/OPERATIONS/Self-Update.md
protocol/OPERATIONS/Wikilinks.md
protocol/PROTOCOL.md
protocol/SHAPE.md
protocol/_template/Overview.md
protocol/_template/ingest/index.md
protocol/_template/ingest/log.md
references/AOIDE-HANDOFF.md
references/AOIDE-DEV-HANDOFF.md
```
