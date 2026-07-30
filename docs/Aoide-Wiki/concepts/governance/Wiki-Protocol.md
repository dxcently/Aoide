---
type: concept
created: 2026-07-25
tags: [aoide, wiki, meta]
---

# Wiki Protocol — Standalone Project Wikis

## Decision (2026-07-25)

The wiki capability is **a protocol**: a shipped shape + rule set that any project's wiki conforms to. Each project keeps its own **standalone** wiki, and the protocol is what makes every one the same shape.

## Ownership

- **Mneme/Melete ship and manage the protocol** and use it to mint, convert, and keep most projects' wikis.
- **Aoide's own wiki is the exception** — it lives in the Aoide repo and Aoide manages it itself (this wiki, `Aoide-Wiki`).

## Default location

A wiki lives **in its project's own repo**, wherever a wiki is needed — default `<repo>/wiki/`. It is versioned with the project and travels with it; there is no fixed central directory.

## The protocol bundle

Staged here under `protocol/`; Mneme/Melete own it, and the relocation to them is an Open Thread in `ingest/log.md`:

- `protocol/SHAPE.md` — the canonical wiki shape.
- `protocol/OPERATIONS/` — the rules a conformant wiki is kept by (ingest, lint, indexing, self-update, frontmatter, naming, wikilinks).
- `protocol/PROTOCOL.md` — the mint and convert procedures.
- `protocol/_template/` — the skeleton stamped for a new wiki.

## Two operations

- **Mint** — create a new wiki already in the shape: stamp `_template/`, seed the `ingest/` pair, write `SCHEMA.md` + `Overview.md`, log the mint.
- **Convert** — reshape an existing directory of notes into the shape (sort into `concepts/`/`entities/`, add the `ingest/` pair, backfill frontmatter, write `SCHEMA.md`).

**Status:** both are hand-run — an `aoide` subcommand and the Mneme/Melete integration are specified to automate them; neither exists.

## This wiki

`Aoide-Wiki` is the reference case: the standalone wiki for Aoide itself, self-managed in the Aoide repo at `docs/Aoide-Wiki/`. Because it is small and self-contained, the working rule is to **read the whole wiki** when working on Aoide — see [[SCHEMA]].

## Dogfooding

A conformant wiki is plain markdown in a known shape, so its pages are ordinary [[Content-Pipeline]] content — a project's own wiki can be ingested and queried alongside other domains. The wiki is the shared context layer; [[Agent-Interface]] is the action layer.

## Related

- [[Agent-Interface]]
- [[Content-Pipeline]]
- [[Fork-and-Run]]
- [[Snowflake-Anatomy]]
