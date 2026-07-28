# Self-Update

How you change your own rules — an `OPERATIONS/` page or `SCHEMA.md`. Routine vault work (adding pages, running ingest, answering queries) never touches these.

## Maintenance vs. self-update

**Maintenance** — fixing a typo, correcting a broken wikilink, updating a cross-reference to match a rename. Do it and log it; no special procedure required.

**Self-update** — changing what a rule *says*: altering prescribed steps, redefining terms, adding or removing constraints. This page governs those changes.

## Procedure

**1. Locate.** Read the target `OPERATIONS/` page in full. Identify every other rule page that references or depends on the rule you are changing — follow cross-references and wikilinks.

**2. Apply.** Fold the change directly into the live page. Fix all cross-references in sibling pages in the same pass. Do not draft changes in a side file.

**3. Record.** Append a dated entry to the wiki's `ingest/log.md` — including changes that touch `SCHEMA.md` or an `OPERATIONS/` page. Use the `refactor` token for structural rule changes.

**4. Self-description test.** After any self-update, the vault must still describe itself correctly by its own rules. Walk through the updated rule and verify that the vault's actual structure and the other rules remain consistent with it. If the vault cannot describe itself after the change, the change was wrong — revert it and start over.

## `SCHEMA.md` is not exempt

`SCHEMA.md` is a page like any other. It may be amended; the self-description test binds it just as it binds every `OPERATIONS/` page. A spec that cannot be amended by its own process is a stone tablet, not a living document.

## Related

- [[Indexing]]
- [[Lint]]
- [[Ingest]]
