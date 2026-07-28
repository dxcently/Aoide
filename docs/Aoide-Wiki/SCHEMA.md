# SCHEMA — Aoide-Wiki

This is the knowledge wiki for the **Aoide** project on this machine. When you pick up Aoide work, **read the whole wiki** — it is kept small and self-contained for exactly that. This file is its constitution: what the wiki is, the shape it holds, and how it is kept and changed.

## What this is

The standalone wiki for the Aoide project — the agent-agnostic Hyprland/Quickshell desktop framework. One project, one wiki: this is **not** a registry of many projects. Plain markdown, readable in any viewer, authored for agents.

## Read the whole thing

Working entry cost, in order: `Overview.md` → every page under `concepts/` and `entities/`. `ingest/index.md` is the catalog if you need to jump; `references/` holds source material (`AOIDE-HANDOFF.md`). The wiki is deliberately small enough to read end to end.

## The shape

```
Aoide-Wiki/
  SCHEMA.md        ← you are here: this wiki's constitution + note manifest
  Overview.md      ← the Aoide overview / hub
  concepts/        ← ideas, mechanisms, frameworks
  entities/        ← named things (components, tools, hosts)
  ingest/
    index.md       ← content catalog
    log.md         ← append-only history + Open Threads
  references/      ← raw sources (AOIDE-HANDOFF.md)
  protocol/        ← the wiki protocol (Mneme/Melete-owned), staged here; not Aoide content
```

## The wiki protocol

This shape is not bespoke — it comes from the **wiki protocol**, a shipped shape + rule set that Mneme/Melete use to mint and keep project wikis. Aoide's own wiki is the self-managed exception: it lives in the Aoide repo. The protocol is staged under `protocol/`, bound to relocate to Mneme/Melete. See [[Wiki-Protocol]] for the concept and `protocol/PROTOCOL.md` for the mint/convert procedure. The rules this wiki is kept by live in `protocol/OPERATIONS/`: [[Ingest]], [[Indexing]], [[Lint]], [[Self-Update]], [[Frontmatter]], [[Naming]], [[Wikilinks]].

## Two ways you act

**Keeping** — ingest sources, update the index, lint, add crosslinks, log. Act alone. See [[Ingest]], [[Indexing]], [[Lint]].

**Self-update** — change a rule (a `protocol/OPERATIONS/` page) or this file. Run [[Self-Update]]. The user signs off when the change is irreversible.

## What you never do

- Invent frontmatter values you cannot verify from source.
- Delete a source in `references/`.
- Rewrite prose wrapped in sentinel comments (`<!-- BEGIN SENTINEL … -->` … `<!-- END SENTINEL -->`); see [[Ingest]].
- Mint a stub page just to clear a red wikilink — a red link is a breadcrumb, not an error.
- Let the Notes manifest below drift from the files on disk.
- Invent a new top-level directory — in this wiki OR in the Aoide repo. Both roots are closed. New content lands inside the existing tree at its designated place; look the place up (repo content paths in [[Song-Vocabulary#The Song Map]], repo shape in `CONTRACTS.md` §2). A new root directory is a contract change, not a convenience.

## You maintain yourself

This file and every `protocol/OPERATIONS/` page obey the rules they describe. Changing what a rule says is a self-update ([[Self-Update]]); the self-description test binds this file too — if the shape section or the manifest no longer matches disk, that is a lint failure.

## Notes manifest

Snapshot of this wiki's files, diffed on each lint pass and rewritten at the end. The wiki is meant to be read whole; this manifest exists for the lint self-description check, not to spare you the reading.

snapshot: 2026-07-26

### Tags

agent · aoide · architecture · auto-discovery · base16 · bridge · cli · coding-agent · compositor · content · daemon · declarative · deployment · desktop · dxflake · extensibility · features · flake · gadget · governance · graph · harness · hyprland · integration · ipc · knowledge · mcp · melete · meta · mneme · naming · nix · node · notes · onboarding · orchestrator · pipeline · policy · qml · quickshell · rebuild · rice · rust · security · session · shell · stylix · terminal · theming · ui · vault · wayland · widget · wiki

### Notes

```
Overview.md
SCHEMA.md
concepts/Agent-Interface.md
concepts/Codebase.md
concepts/Content-Pipeline.md
concepts/Notes.md
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
concepts/Song-Vocabulary.md
concepts/Terminal-Commander.md
concepts/Widget-Maker.md
concepts/Wiki-Protocol.md
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
ingest/index.md
ingest/log.md
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
```
