# SCHEMA — Aoide-Wiki

This is the knowledge wiki for the **Aoide** project on this machine. When you pick up Aoide work, **read the whole wiki** — it is kept small and self-contained for exactly that. This file is its constitution: what the wiki is, the shape it holds, and how it is kept and changed.

## What this is

The standalone wiki for the Aoide project — the agent-agnostic Hyprland/Quickshell desktop framework. One project, one wiki: this is **not** a registry of many projects. Plain markdown, readable in any viewer, authored for agents.

## Read the whole thing

Working entry cost, in order: `Overview.md` → every page under `concepts/` (including its `orchestration/`, `desktop/`, `song/`, `governance/`, and `cli/` subfolders) and `entities/`. `ingest/index.md` is the catalog if you need to jump; `references/` holds source material (`AOIDE-HANDOFF.md` — the design contract; the handoff and ricing design docs; `pantheon/` — the design reference stills). The dev-facing CLI reference lives at `concepts/cli/` (`CLI-Reference.md` is its hub). The development agent's operating manual is the self-contained set under `protocol/dev/`, entered at `protocol/dev/DEV.md`. The wiki is deliberately small enough to read end to end. **Rice design memory lives in the songbook under `song/`, per song** — each song's current elements in `song/songbook/<name>/design/` ([[Song-Anatomy]]). The cross-cutting house grammar is historical reference at `references/pantheon/pantheon-grammar.md`; `concepts/song/Ricing-Protocol.md` carries protocol only. Design content written about a rice goes to that rice's `design/` folder in the repo, not here.

## The shape

```
Aoide-Wiki/
  SCHEMA.md        ← you are here: this wiki's constitution + note manifest
  Overview.md      ← the Aoide overview / hub
  concepts/        ← ideas, mechanisms, frameworks — grouped by part of Aoide
    orchestration/   ← agent sessions, the graph, the conductor channel, the conductor
    desktop/         ← the Quickshell/Hyprland desktop surfaces
    song/            ← the performed half — song anatomy, vocabulary, self-ricing, ricing protocol
    governance/      ← the rebuild gate, clone-and-run, wiki protocol
    cli/             ← the dev-facing CLI reference: signature/reads/writes/output per command, hub + eight group pages
    (root)           ← cross-cutting: Codebase, Full-Architecture, Snowflake-Anatomy, Lexicon, Package-Layout, Plugin-Architecture
  entities/        ← named things (components, tools, hosts)
  ingest/
    index.md       ← content catalog
    log.md         ← append-only history + Open Threads
  references/      ← raw sources (AOIDE-HANDOFF.md, the handoff + ricing design docs, pantheon/ stills + grammar)
  protocol/        ← the wiki protocol (Mneme/Melete-owned) staged here; not Aoide content
    dev/           ← the dev agent's operating manual: DEV.md + orchestration, verification, and one page per harness. Self-contained — no links out
```

## The wiki protocol

This shape comes from the **wiki protocol**, a shipped shape + rule set that Mneme/Melete use to mint and keep project wikis. Aoide's own wiki is the self-managed exception: it lives in the Aoide repo. Mneme/Melete own the protocol; it is staged under `protocol/` and the relocation to them is an Open Thread in `ingest/log.md`. See [[Wiki-Protocol]] for the concept and `protocol/PROTOCOL.md` for the mint/convert procedure. The rules this wiki is kept by live in `protocol/OPERATIONS/`: [[Ingest]], [[Indexing]], [[Lint]], [[Self-Update]], [[Frontmatter]], [[Naming]], [[Wikilinks]], [[Assertion]], [[Style]].

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
- Write filler: throat-clearing, contrast rhetoric, symmetric slogan lists, empty intensifiers, restating summaries. Every sentence carries a fact an agent needs. See [[Style]].
- Invent a new top-level directory — in this wiki OR in the Aoide repo. Both roots are closed. New content lands inside the existing tree at its designated place; look the place up (repo content paths in [[Song-Vocabulary#The Song Map]], repo shape in `CONTRACTS.md` §2). A new root directory is a contract change, not a convenience.

## You maintain yourself

This file and every `protocol/OPERATIONS/` page obey the rules they describe. Changing what a rule says is a self-update ([[Self-Update]]); the self-description test binds this file too — if the shape section or the manifest no longer matches disk, that is a lint failure.

## Notes manifest

Snapshot of this wiki's files, diffed on each lint pass and rewritten at the end. The wiki is meant to be read whole; this manifest exists for the lint self-description check, not to spare you the reading.

snapshot: 2026-08-25

### Tags

a2a · agent · aliases · aoide · architecture · auto-discovery · base16 · blueprint · bridge · cli · coding-agent · compositor · computer-use · conductor · content · crate · daemon · dag · declarative · deployment · design · desktop · dxflake · editor · extensibility · features · flake · gadget · git · glyph · governance · graph · harness · hooks · hyprland · integration · ipc · keybinds · knowledge · livery · lyra · mcp · melete · meta · mneme · naming · nix · node · notification · onboarding · orchestration · orchestrator · paint · pantheon · pipeline · plugin · pointer · policy · protocol · pty · qml · quickshell · rebuild · reference · rice · rust · schema · screen · secrets · security · session · shell · song · stylix · terminal · theming · totp · tui · ui · upkeep · vault · vision · wayland · widget · wiki

### Notes

```
Overview.md
SCHEMA.md
concepts/Codebase.md
concepts/Full-Architecture.md
concepts/Lexicon.md
concepts/Package-Layout.md
concepts/Plugin-Architecture.md
concepts/Snowflake-Anatomy.md
concepts/cli/CLI-Reference.md
concepts/cli/Conductor-TUI.md
concepts/cli/Content-and-Hooks.md
concepts/cli/Doors-and-Nodes.md
concepts/cli/Graph-and-Conduct.md
concepts/cli/Meta-and-Upkeep.md
concepts/cli/Rice-and-Livery.md
concepts/cli/Screen-Commands.md
concepts/cli/Secrets-Commands.md
concepts/desktop/Controls.md
concepts/desktop/Desktop-Architecture.md
concepts/desktop/Feature-Set.md
concepts/desktop/Gadget-Dock.md
concepts/desktop/Terminal-Commander.md
concepts/desktop/Widget-Bridge-Contract.md
concepts/desktop/Widget-Maker.md
concepts/governance/Clone-and-Run.md
concepts/governance/Governance.md
concepts/governance/Rebuild-Gate.md
concepts/governance/Wiki-Protocol.md
concepts/orchestration/A2A-Door.md
concepts/orchestration/Agent-Hooking.md
concepts/orchestration/Agent-Interface.md
concepts/orchestration/Conductor-3D-DAG.md
concepts/orchestration/Conductor-Channel.md
concepts/orchestration/Content-Pipeline.md
concepts/orchestration/Loop-Protocol.md
concepts/orchestration/Node-Federation.md
concepts/orchestration/Node-Transport.md
concepts/orchestration/Pairing-Ceremony.md
concepts/orchestration/Screen-Control.md
concepts/orchestration/Secrets-Broker.md
concepts/orchestration/Session-Graph.md
concepts/song/Ricing-Protocol.md
concepts/song/Self-Ricing.md
concepts/song/Song-Anatomy.md
concepts/song/Song-Vocabulary.md
entities/Hyprland.md
entities/Melete.md
entities/Mneme.md
entities/Quickshell.md
entities/Stylix.md
entities/aoide-cli.md
entities/aoided.md
entities/dxflake.md
entities/livery.md
entities/lyra.md
entities/shellbridge.md
ingest/index.md
ingest/log.md
protocol/dev/CRAFT.md
protocol/dev/DEV.md
protocol/dev/HARNESS-CLAUDE-CODE.md
protocol/dev/ORCHESTRATION.md
protocol/dev/PRINCIPLES.md
protocol/dev/VERIFICATION.md
protocol/OPERATIONS/Assertion.md
protocol/OPERATIONS/Frontmatter.md
protocol/OPERATIONS/Indexing.md
protocol/OPERATIONS/Ingest.md
protocol/OPERATIONS/Lint.md
protocol/OPERATIONS/Naming.md
protocol/OPERATIONS/Self-Update.md
protocol/OPERATIONS/Style.md
protocol/OPERATIONS/Wikilinks.md
protocol/PROTOCOL.md
protocol/SHAPE.md
protocol/_template/Overview.md
protocol/_template/ingest/index.md
protocol/_template/ingest/log.md
references/AOIDE-HANDOFF.md
references/AOIDE-VS-LANGCHAIN-HANDOFF.md
references/Glass-Stretch-on-a-Rotated-Monitor.md
references/audit-report.md
references/fleshing-out-aoide-ricing.md
references/mneme-shared-memory-and-aoide.md
references/pantheon/pantheon-grammar.md
```
