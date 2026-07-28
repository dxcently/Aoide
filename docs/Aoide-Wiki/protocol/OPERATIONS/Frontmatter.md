# Frontmatter

OPERATIONS pages carry no YAML frontmatter. All other note kinds use the field sets below.

## Field Sets by Note Kind

**Concept** (`concepts/`):

```yaml
type: concept
created: YYYY-MM-DD
tags: [keyword]        # optional
source: "[[references/…]]"  # optional
updated: YYYY-MM-DD    # optional
```

**Entity** (`entities/`):

```yaml
type: entity
created: YYYY-MM-DD
aliases: [Alt Name]    # optional
tags: [keyword]        # optional
source: "[[references/…]]"  # optional
updated: YYYY-MM-DD    # optional
```

**Overview** (wiki root, `Overview.md`):

```yaml
type: overview
created: YYYY-MM-DD
```

**Index** (`ingest/index.md`): `type: index` only — no date field; the catalog is perpetually current.

**Log** (`ingest/log.md`): no frontmatter — a plain append-only document.

## Required vs Optional

| Field | Concept | Entity | Overview | Index | Log |
|-------|---------|--------|----------|-------|-----|
| `type:` | ✓ | ✓ | ✓ | ✓ | — |
| `created:` | ✓ | ✓ | ✓ | — | — |
| `aliases:` | — | opt | — | — | — |
| `tags:` | opt | opt | — | — | — |
| `source:` | opt | opt | — | — | — |
| `updated:` | opt | opt | — | — | — |

## Rules

**No `topic:` field.** Each wiki covers one project; membership is structural (the page lives in this wiki), not a frontmatter tag.

**Unknown required values — leave absent.** When you cannot determine a required field's correct value, omit it entirely. Never invent. A missing field is visible and fixable; a wrong one silently corrupts queries.

**`source:`** points into the project's own `references/` folder, as a wikilink: `source: "[[references/AOIDE-HANDOFF]]"`. External URLs do not belong here; put them in the source document itself.

**`updated:`** set only when you are the agent making the edit in the current session. Do not carry it forward from prior sessions if you did not touch the content.

## Related

- [[Ingest]]
- [[Lint]]
- [[Indexing]]
- [[Naming]]
