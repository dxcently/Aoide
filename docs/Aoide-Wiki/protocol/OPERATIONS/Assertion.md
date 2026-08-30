# Assertion

How a page states things. Every content page — `concepts/`, `entities/`, `Overview.md` — asserts the system **as it is at HEAD**, in the present indicative. One tense, one mood.

The rule exists because these pages are read as a specification. A reader cannot tell, from prose alone, whether "the wrapper is no longer spawned" describes today's code or yesterday's; whether "this would grant a scoped path" describes a shipped mechanism or a wish. Mixing state, history, and speculation in one voice makes every sentence require external verification, which defeats the point of writing it down.

## The three clauses

### 1. Current state only

A page describes the system at HEAD. Prior states, migrations, and superseded designs are not page content.

Prohibited constructions: `used to`, `no longer`, `formerly`, `previously`, `before X`, `originally`, `has now been`, `was renamed`, `was removed`, and retrospective framings (`the lesson`, `the mistake was`).

Where history lives instead:

| Kind | Destination |
|---|---|
| What changed and when | `ingest/log.md` — dated, immutable ([[Lint]] append-only rule) |
| Why a change was made | the commit message |
| A superseded design worth keeping | `references/` |

Test: if a sentence would need editing only because the *past* changed, it is history, not description.

### 2. No definition by contrast

A thing is defined by what it is. The alternative not taken is not part of its definition.

Prohibited: `not a X-vs-Y split`, `rather than <thing never built>`, `chosen over`, `was rejected`, `instead of <rejected option>` — and the parenthetical aside that exists only to pre-empt a reader's objection.

Rationale for a decision is legitimate content, but it is *stated*, not *litigated*: give the property the design has, not the debate it won. `Values and mint share one name` is the assertion; `this is not a values-vs-engine split` is the debate.

### 3. Speculation is routed, not written

Open questions and conditionals do not appear as page prose. They are routed to the surface that tracks them:

| Kind | Destination |
|---|---|
| Open design question | `ingest/log.md` → `## Open Threads` |
| Actionable, scoped work | the orchestrator’s task-stack memory ([[DEV]]) |
| A what-if surfaced mid-edit | ask the user, or flag it — do not write it into the page |

Prohibited in page prose: `would`, `could`, `might`, `eventually`, `someday`, `TBD`, `probably`, `open question`, `what if`, `we may want to`.

An agent that discovers a what-if while editing does not resolve it silently in either direction — it neither writes the speculation into the page nor deletes the underlying question. It routes it.

A **specified design** is not an open question, and this clause does not evict it: a design that has been decided but not yet built stays on the page under a status label. See "Documenting the unbuilt" below. The distinction is whether the answer is known — a decided design is documented, an undecided one is routed.

## What stays: invariants

Present-tense exclusion is a specification, not a negation of history. A constraint on the system as it stands is correct and required content:

- `A facet reads aoide.livery and nothing else.`
- `session reap never errors on "nothing to reap".`
- `The agent never holds the password.`

These pass clause 1 — they describe HEAD — and clause 2 — they name a property, not a road not taken.

Prefer the positive form when it is complete (`reads only X` over `never reads Y`). Use `never` when the absence *is* the constraint: a guarantee that an event does not occur has no positive phrasing.

## Documenting the unbuilt

A design that exists in specification but not in code is documented with a **status label plus present indicative about the design** — never with subjunctive prose about the code.

The design exists; therefore statements about the design are present-tense true. The status label carries the state of the *code*, which is the only part that is not yet true.

```
## The `aoide.rebuild` capability

**Status:** specified; no `aoide.rebuild` surface in `modules/nucleus/options.nix`.
Open Thread: `ingest/log.md` — "rebuild capability".

The design grants a passwordless, narrowly-scoped path to the gated commands:
a dedicated no-login agent user; the rebuild as a fixed systemd oneshot unit …
```

Not: `the design, once built, would grant …`, `test could be auto-admitted, but switch would still route …`.

The label is a required part of the section — a specified-but-unbuilt section without one is a clause 3 violation regardless of its tense.

## Scope

Binds every content page. Protocol pages (`SCHEMA.md`, `OPERATIONS/`, `PROTOCOL.md`, `SHAPE.md`) are prescriptive and write in the imperative; clauses 1 and 3 bind them, the phrasing preferences do not.

`ingest/log.md` is exempt by construction: it is the history surface, and its dated entries are immutable.

Changing this page is a [[Self-Update]].

## Related

- [[Lint]] — the assertion-violation check
- [[Self-Update]]
- [[Naming]]
- [[Ingest]]
