# Ingest

The flow by which a raw source becomes structured pages in a project wiki. Runs only on explicit request — never opportunistically.

Before starting, read the project's `ingest/log.md` for prior entries on this source. A match means re-run: update existing pages, do not duplicate.

## Flow

**1. Read the source in place.** Do not move or modify it.

**2. Extract subjects and claims.** Classify each subject as an entity (named thing) or concept (idea/mechanism/framework).

**3. Page-vs-mention decision.** A subject earns its own page only if it carries claims that are independently useful: a definition, mechanism, framework, or set of relationships. A subject that appears only as supporting context becomes a `[[wikilink]]` in another page's `## Related` section — not a standalone note.

When a subject already has a page, add new claims to it — never create a second page for the same subject.

**4. Create or update pages.**
- **Location:** `concepts/` for ideas and frameworks; `entities/` for named things.
- **Frontmatter:** per [[Frontmatter]].
- **Body:** extract and rephrase — do not transcribe source text verbatim (sentinel-wrapped prose excepted, see step 5).
- **Close every page** with a `## Related` section; crosslink fan-out (step 6) populates it.

**5. Protect verbatim human prose.** Any verbatim text from the user's own writing must be wrapped in sentinel markers and is never rewritten on later passes:

```
<!-- BEGIN SENTINEL: human prose — read-only -->
…
<!-- END SENTINEL -->
```

**6. Crosslink fan-out.** After all new and updated pages exist, add bidirectional `[[wikilinks]]` in `## Related` sections. Run once across all pages created this ingest pass.

**7. Settle the source.** Move the raw source into the project's `references/` folder if it is not already there. Sources already in `references/` stay.

**8. Update `ingest/index.md`.** Add one bullet per new page, grouped by kind (`## Concepts`, `## Entities`). Update glosses for changed pages. Do this in the same session — never defer.

**9. Append to `ingest/log.md`.**

```
## [YYYY-MM-DD] ingest | <source title>

- <N concept pages, M entity pages created>
- <pages updated, if any>
- <subjects deferred to mentions, if any>
```

## Idempotency

Re-running ingest on the same source updates existing pages — never duplicates. A prior log entry for the source signals a re-run.

## Related

- [[Frontmatter]]
- [[Wikilinks]]
- [[Indexing]]
- [[Lint]]
