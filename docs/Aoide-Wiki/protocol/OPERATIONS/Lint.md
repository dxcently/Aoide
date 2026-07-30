# Lint

A conformance sweep of a standalone wiki. Output is a **report for user review** — do not apply content fixes without approval.

Two writes are permitted without approval:
1. Appending the lint log entry to `ingest/log.md`.
2. Marking an Open Thread closed in `ingest/log.md ## Open Threads`.

## Check Sequence

Run checks in this order. List findings under the headings below.

**1. Sentinel violations** — any sentinel block whose content differs from the last-written version. This is an integrity failure; list these first, above all other findings.

**2. Broken wikilinks** — distinguish hard breaks (target page does not exist and no Open Thread covers it) from bare-name ambiguity (multiple pages match the bare name). Unresolved links with a covering Open Thread are not broken — they are breadcrumbs; omit them.

**3. Orphan pages** — a concept or entity page that has no inbound wikilink from any other page AND is not listed in `ingest/index.md`.

**4. Frontmatter conformance** — fields missing, mistyped, or present but disallowed for the note kind, per [[Frontmatter]].

**5. Index drift** — every concept and entity page must have exactly one bullet in `ingest/index.md`. Flag both missing entries and stale entries pointing to deleted pages.

**6. Log format violations** — each dated entry must follow `## [YYYY-MM-DD] <action> | <target>` where `<action>` is one of the valid tokens: `mint | ingest | query | lint | rename | refactor`. Flag malformed headings or unrecognised tokens.

**7. Contradictions** — two pages stating the same claim with conflicting facts. Surface the conflict; do not resolve it.

**8. Manifest parity** — the wiki's `SCHEMA.md` Notes manifest must match the files on disk (both directions), and the tag set must reflect the tags in use. Flag any drift.

**9. Assertion violations** — content pages carrying history, definition-by-contrast, or speculation, per [[Assertion]]. Report the offending line and which clause it breaks. Two sub-cases are reported, never auto-fixed: prose that must be *routed* (a what-if with no Open Thread covering it) and a specified-but-unbuilt section with no status label.

The first-pass sweep is grep-able:

```
grep -rniE '\b(used to|no longer|formerly|previously|originally|was (renamed|removed|rejected)|rather than|chosen over|would|could|might|eventually|someday|TBD|probably|what if)\b' concepts/ entities/ Overview.md
```

Matches are candidates, not findings — `never` and other exclusions are invariants and pass ([[Assertion]] "What stays"). Confirm each against the clause it appears to break before listing it.

## Report Format

```
## [YYYY-MM-DD] lint | <scope>

### Sentinel violations (N)
- …

### Broken wikilinks (N)
- …

### Orphan pages (N)
- …

### Frontmatter conformance (N)
- …

### Index drift (N)
- …

### Log format violations (N)
- …

### Contradictions (N)
- …

### Manifest parity (N)
- …

### Assertion violations (N)
- …
```

Omit any section with zero findings.

## Append-Only Rule

Dated log entries are **immutable**. If a prior lint entry contains an error, append a new entry — do not edit the old one.

The `## Open Threads` section of `ingest/log.md` is the only part of the log that may be edited in place: mark a thread closed (keep its body), or add a new thread. Never delete thread bodies.

## Related

- [[Frontmatter]]
- [[Wikilinks]]
- [[Ingest]]
- [[Indexing]]
- [[Self-Update]]
