# Wiki Protocol — Procedures

The protocol for giving a project a standalone wiki in the shape defined by [[SHAPE]]. It is **shipped and managed by Mneme/Melete**: they apply it to create and keep project wikis. **Aoide's own wiki is the exception** — it lives in the Aoide repo and Aoide manages it itself. Hand-run for now; a future `aoide` subcommand and the Mneme/Melete integration automate it. Concept and rationale: [[Wiki-Protocol]].

## Default location

A wiki is created **in its project's own repo**, wherever a wiki is needed — default `<repo>/wiki/`. There is no fixed central directory; the wiki travels with the project it documents.

## Mint — a new wiki

1. Create the target directory in the project's repo (default `<repo>/wiki/`).
2. Stamp `_template/` into it — `Overview.md` + `ingest/index.md` + `ingest/log.md` — substituting `{{PROJECT}}` and `{{DATE}}`.
3. Write `SCHEMA.md` from the shape: constitution + an empty note manifest.
4. Append a `mint` entry to `ingest/log.md`.
5. Add concept/entity pages via [[Ingest]] as content arrives; keep kind folders lazy.

## Convert — existing notes into a conformant wiki

1. Read the existing directory in place.
2. Sort pages: ideas → `concepts/`, named things → `entities/`, raw sources → `references/`.
3. Backfill frontmatter per [[Frontmatter]] (type/created; never invent a value).
4. Create the `ingest/` pair: build `index.md` from the sorted pages; open `log.md` with a `mint` entry noting the conversion (`refactor` if the wiki already existed in another shape).
5. Write `SCHEMA.md`; run [[Lint]] and fix drift.

## After either

Run [[Lint]] and finalize the `SCHEMA.md` note manifest and tag set so the wiki describes itself correctly.
