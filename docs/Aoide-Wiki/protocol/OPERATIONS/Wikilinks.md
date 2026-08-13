# Wikilinks

## Rule: all internal references are wikilinks

Every reference to another page in this wiki must use `[[wikilink]]` syntax. Plain filesystem paths (`concepts/Self-Ricing.md`) are forbidden in note bodies.

## Bare names vs. full paths

A wiki is standalone, so every page name is unique within it — prefer the bare name:

```
[[livery]]
[[aoided]]
[[Overview]]
```

Use a fuller relative path only when linking a page that lives in a subfolder the bare name can't reach unambiguously (e.g. a settled source: `[[references/AOIDE-HANDOFF]]`). Concept and entity titles are unique — always bare.

## Heading links

Link to a specific section with `[[Page#Heading]]`. The heading text must be an exact match (case-sensitive):

```
[[Ingest#Flow]]
[[Frontmatter#Required vs Optional]]
```

## External URLs

External URLs belong only in:
- `source:` frontmatter values
- Raw source documents in `references/`

Do not embed bare URLs in page bodies. If a URL must be cited in a body, settle the source into `references/` first and link to it via wikilink.

## Unresolved links are breadcrumbs, not defects

A `[[wikilink]]` that points to a non-existent page is a forward reference — it signals intent, not an error. Do **not** mint a stub page just to resolve a red link.

When you leave an unresolved link:
1. Keep it as-is in the body.
2. Open a thread in the wiki's `ingest/log.md` under `## Open Threads`.
3. Mint the target page only when real content exists to fill it.

## Related

- [[Naming]]
- [[Ingest]]
- [[Lint]]
- [[Indexing]]
