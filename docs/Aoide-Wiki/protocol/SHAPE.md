# Wiki Shape

The canonical shape every wiki minted under the wiki protocol conforms to. The protocol's mint operation stamps it; its convert operation enforces it on an existing directory. Each wiki is **standalone** — there is no central registry.

```
<wiki>/
├── SCHEMA.md      constitution + note manifest; the first thing to read
├── Overview.md    hub; H1 = project name
├── concepts/      ideas, mechanisms, frameworks
├── entities/      named things (created lazily)
├── ingest/
│   ├── index.md   content catalog
│   └── log.md     append-only history + Open Threads
└── references/    raw sources (created when the first source is settled)
```

## Default location

A wiki lives **in its project's own repo**, wherever a wiki is needed — default `<repo>/wiki/`. It travels with the project and is versioned alongside it. This wiki (`Aoide-Wiki`, for the Aoide project) is the reference case: it lives in the Aoide repo at `docs/Aoide-Wiki/`, self-managed.

## Rules

- `SCHEMA.md` and `Overview.md` are required at mint.
- `concepts/` appears with the first concept page; `entities/` and `references/` are lazy.
- A kind folder may nest its pages into domain subfolders (e.g. `concepts/<part>/`) once it grows large enough to warrant grouping — bare-name wikilinks keep this link-safe regardless of depth.
- Each wiki is self-describing: its `SCHEMA.md` carries its own note manifest, diffed on lint. No cross-project index.
- The keeping rules live in `OPERATIONS/`; the mint/convert procedures in `PROTOCOL.md`; the stamped skeleton in `_template/`.
- Frontmatter, naming, and links follow [[Frontmatter]], [[Naming]], [[Wikilinks]].
- Content pages state the system as it is, in the present indicative — [[Assertion]]. History goes to `ingest/log.md`, speculation to its `## Open Threads`.
- Prose holds the dense technical register of [[Style]] — every sentence carries a fact; the filler constructions it bans never enter a page, whichever agent writes it.

This bundle (`SHAPE.md`, `PROTOCOL.md`, `OPERATIONS/`, `_template/`) is the **wiki protocol** — shipped and managed by Mneme/Melete, staged here until it relocates to them. See [[Wiki-Protocol]].
