# Naming

## Folder names

**Kind folders** sit at the wiki root, lowercase, no sigil, no number prefix: `concepts/`, `entities/`, `ingest/`, `references/`.

**Each project has its own standalone wiki directory** — e.g. `Aoide-Wiki/`, using the project's source capitalization. There is no `projects/` container; wikis are not nested under one another.

**`OPERATIONS/`** is all-caps — it is the protocol's rule set (staged under `protocol/`), not a wiki kind folder.

Create kind folders lazily — only when the first note of that kind is minted.

## File names

Use readable title case with hyphens for multi-word names: `Notes.md`, `Self-Ricing.md`, `Song-Vocabulary.md`.

**Source capitalization wins** for proper nouns and code identifiers, even when they are lowercase or unconventional: `aoided.md`, `shellbridge.md`.

Files always present at a wiki root: `SCHEMA.md` (all-caps) and `Overview.md` (title case).

## H1 titles

The H1 of each page may be richer than the filename — a subtitle, parenthetical, or fuller description is fine. The filename is the stable identifier; the H1 is the human-readable title.

Examples:
- File: `Notes.md` → H1: `# Notes — the Seam Between Score and Performance`
- File: `aoided.md` → H1: `# aoided`

## Summary table

| Item | Convention | Example |
|------|------------|---------|
| Kind folder | lowercase, no prefix | `concepts/` |
| Wiki directory | source capitalization | `Aoide-Wiki/` |
| Protocol rule folder | ALL-CAPS | `OPERATIONS/` |
| Generic file | title case, hyphens | `Notes.md` |
| Code identifier file | source capitalization | `aoided.md` |
| Wiki root files | as specified | `SCHEMA.md`, `Overview.md` |

## Related

- [[Frontmatter]]
- [[Wikilinks]]
- [[Indexing]]
