---
type: reference
created: 2026-08-30
updated: 2026-08-30
tags: [aoide, code, style, commits, git, protocol]
---

# Craft — repo, code, and commits

How work is made: how the repository is handled, how code is written, and
what a comment and a commit must carry. Harness-agnostic. Dependency of
[[DEV]].

**Scope boundary.** This page governs *the repo* — code, comments, commit
messages, READMEs, briefs, reports. **Wiki content pages are out of scope**:
they keep the wiki's own style and assertion rules (controlled technical
register, present indicative, no filler constructions), which a commit
message does not.

**Style is the editor's own.** These are requirements of substance — what a
piece of writing has to contain and prove. How it sounds is whoever is
editing, and the requirements hold either way.

---

## Handling the repo

Active development, not branch-protected. Small, coherent, verified changes
land directly.

- **Ask before branching.** Large, risky, long-running, or likely to churn
  shared files → ask. Default: land directly.
- **One commit, one coherent change.** A commit that needs "and also" in
  its subject is two commits.
- **Docs ship with the code.** A commit changing a directory's seams,
  invariants, or extension points updates that directory's docs in the SAME
  commit — never a follow-up.
- **Pathspec commits only** — `git commit -- <paths>`. Other sessions hold
  the same index; never stage the whole tree, never reset it.
- **Never commit** a harness's own settings file, `node_modules`, or
  `song/stage/*` / `state/stage/*` (live desktop state, not a commit
  target).
- **No AI co-author trailer.** Push as the repo's configured author.
- **Push at milestones** — lane or phase completion, or on request. Commit
  freely in between.
- **A version bump rides its phase's landing commit**, never its own.

---

## Writing code

**Convention over invention.** The established pattern in the surrounding
code beats a rule invented to enforce your own taste. A new guard, flag, or
config where a convention already answers is a defect, not a safeguard.

**Deleting more wins.** Between two designs that both work, take the one
that removes more code. Between two that remove the same, take the one that
*enables* removing more later. Net lines added is a cost, not an
achievement — a change that deletes is usually the better change.

**Simplest thing that works, only what is in scope.** No speculative
features, no abstractions for a future that has not arrived, no
configurability nobody will touch, no rewriting what already works. A
config section lands with its consumer, not ahead of it.

**Readability and flow over commentary.** Code that needs a paragraph to
explain usually needs restructuring instead. Extract the function, name the
value, split the branch — then the comment is unnecessary and cannot go
stale. Prefer fixing the code to annotating it, every time.

**Fail loudly, never guess.** Reject an unknown key, an unknown capability,
an ambiguous input, by name. Silent tolerance of a typo in a file that
carries policy is the failure mode being refused.

**One authority per fact.** A count, a version, a path, a schema — one
place derives it, everything else reads. A hand-copy with a comment saying
"keep in sync with X" is a staleness schedule, not a safeguard.

**Enforce what review cannot see; document what review catches naturally.**
A violation that lives only in prose is invisible in a diff — that is when
it earns a mechanical check. A breach that is glaring in any diff does not.

---

## Comments

**A comment justifies its existence or it does not ship.** The default is
no comment.

A comment earns its place when it carries what the code cannot:

- the **why** behind a non-obvious choice, and the option rejected
- a **constraint** that is invisible locally — a wire contract, an ordering
  requirement, a trap that bit someone
- a **decision**, stated as current fact

A comment does not earn its place by:

- narrating what the code plainly does
- restating a name that already says it
- recording history — that is the commit message's job
- hedging about a possible future

**Write a decided change as final.** Current state, present tense, not
conditional and not historical. One short note recording the decision, not
a chronicle of how it was reached.

**Never append dated UPDATE or AMENDMENT blocks** to a file. Edit
integrally so it reads as though it was always this way; the change record
goes to the log.

---

## Commits

**Subject:** `type(scope): declarative statement of the new state` — present
tense, lowercase, no trailing period. It describes the tree after the
commit, not the activity that produced it. A subject needing "and also" is
two commits.

**Body.** A commit is the log, so it carries the reasoning and not just the
change:

| carries | shape |
|---|---|
| what is now true | first line; no throat-clearing, no "this commit" |
| why | one reason per claim — a second is padding, or belongs elsewhere |
| the proof | named: a hash, a count, a measured number, a command's real output |
| what was settled with the User | what was proposed, what they decided, what changed course and why |
| what was rejected | one line each, so it is not re-proposed |
| flags | last, one line each — uncertainties, parked branches, a consequence met later |

Findings are bullets, one line each, stated as found — a breakage says it is
broken and for how long. No closing summary; the reader just read it. Length
follows the change: a one-line fix gets a one-line message, and depth is
earned by being architectural, not by being long.

**Never a personal name** — authored content says "the User". A commit that
hides whose call a decision was is a commit whose decision cannot be
revisited.
