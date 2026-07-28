# Indexing

## Wiki shape

Every conformant wiki is standalone and follows one shape (full spec in [[SHAPE]]):

```
<wiki>/
├── SCHEMA.md       constitution + note manifest; read first
├── Overview.md     hub; H1 = project name
├── concepts/       ideas, mechanisms, frameworks
├── entities/       named things (lazy)
├── ingest/
│   ├── index.md    content catalog (perpetually current)
│   └── log.md      append-only history + Open Threads
└── references/     raw sources settled here (lazy)
```

Create kind folders lazily — `entities/` and `references/` only when first needed. `SCHEMA.md` and `Overview.md` exist from mint.

## The `ingest/` pair

**`ingest/index.md`** is the catalog: every concept and entity page with one bullet each, grouped under `## Concepts` and `## Entities`. It is the jump-in path for a large wiki; for a small one, read the whole wiki instead.

**`ingest/log.md`** is append-only:

```
# <Project> — Log

## Open Threads
<pinned; edited in place to open/close threads>

## [YYYY-MM-DD] <action> | <target>
- …
```

`## Open Threads` is always first, pinned above dated entries. Any file in `ingest/` other than `index.md` and `log.md` is an open work item.

**Before any edit:** read `ingest/log.md` and `ingest/index.md` first. Log the operation in the same session — never defer.

## Mint and convert

A new wiki is minted per the protocol ([[PROTOCOL]]); until the tooling exists, run it by hand:

1. Create the wiki directory in the project's own repo (default `<repo>/wiki/`), then stamp `_template/` into it (substitute `{{PROJECT}}` and `{{DATE}}`).
2. Create `ingest/index.md` and `ingest/log.md`; append a `mint` entry immediately.
3. Write `SCHEMA.md` (constitution + empty note manifest) and `Overview.md` (H1 = project name).
4. Add kind folders and pages lazily via [[Ingest]].

Converting existing notes into this shape reaches the same end state — see [[PROTOCOL]].

## The manifest-diff mechanic

Each wiki's `SCHEMA.md` holds a snapshot manifest of its own note paths plus its tag set. On each index or lint pass:

1. Diff the manifest against the files on disk.
2. Apply changes to the live pages.
3. Rewrite the manifest in `SCHEMA.md` at the end of the pass.

There is no central registry — a wiki describes only itself.

## Query / retrieval path

A small wiki is meant to be read whole. To jump into a large one:

1. Read `SCHEMA.md` — constitution + note manifest.
2. Open `ingest/index.md` — locate the target page(s).
3. Read the target page(s).

Queries that make no durable edit are **not** logged.

## Log action tokens

Valid tokens for `## [YYYY-MM-DD] <action> | <target>` entries:

| Token | When to use |
|-------|-------------|
| `mint` | New wiki created (or an existing directory converted) |
| `ingest` | Source ingested into pages |
| `query` | (Reserved; do not log read-only queries) |
| `lint` | Lint sweep run |
| `rename` | Page or folder renamed |
| `refactor` | Structural change to the wiki (not a rename) |

## Related

- [[Ingest]]
- [[Lint]]
- [[Frontmatter]]
- [[Naming]]
- [[Self-Update]]
