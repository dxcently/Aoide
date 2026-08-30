---
type: reference
created: 2026-08-30
updated: 2026-08-30
tags: [aoide, code, style, commits, voice, git, protocol]
---

# Craft — repo, code, and voice

How work is made: how the repository is handled, how code is written, and
how the words shipped inside the repo read. Harness-agnostic. Dependency of
[[DEV]].

**Scope boundary.** This page governs *the repo* — code, comments, commit
messages, READMEs, briefs, reports. **Wiki content pages are out of scope**
and keep the wiki's own style and assertion rules: a controlled technical
register, present indicative, no filler constructions. Do not apply this
page's voice to a wiki page, and do not apply the wiki's register to a
commit message.

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

## Voice — commits, comments, prose

Everything written inside the repo carries one voice. It is blunt and
technical, and it never softens first.

**Schema:**

1. **Lead with the change.** First line states what is now true. No
   throat-clearing, no "this commit", no announcing the subject.
2. **One reason per claim.** A second justification is padding or belongs
   elsewhere.
3. **Show the work.** What was checked, what was ruled out, why this over
   that. A claim that needed evidence carries it — a hash, a count, a
   measured number, a command's real output.
4. **Findings are bullets, one line each.** No closing summary; the reader
   just read it.
5. **Record what was worked out with the User.** A commit is the log, so it
   carries the decision and its provenance — what was proposed, what the
   User decided, what changed course and why. Name an overruled proposal in
   one line so it is not raised again. This is the difference between a log
   of changes and a log of *reasoning*, and the second is the one worth
   keeping.
6. **Flags last, one line each.** Uncertainties, parked branches, a
   consequence the reader will meet later.
7. **Length follows the change.** A one-line fix gets a one-line message.
   Depth is earned by being architectural, not by being long.

**Kaomoji are welcome** in commits, code comments, and dev-facing writing.
They carry tone the words do not, and this repo is written by and for people
who read them. Build one for the moment rather than reaching for a stock
face. Keep them out of a commit *subject* line, which is read in a list, and
out of wiki content pages, which keep the register described above.

**Never:**

- **A personal name.** Authored content says "the User".
- **Apology, hedging, or ceremony.** State the fact. A mistake gets a
  correction, not a preamble about the mistake.
- **Softening a real finding.** If something is broken, the message says it
  is broken, and says for how long.

**Commit subject:** `type(scope): declarative statement of the new state`.
Present tense, lowercase, no trailing period. It describes the tree after
the commit, not the activity that produced it.

**Commit body:** the schema above. State what changed, why it changed, and
what was settled with the User to get there. Where a claim was proven, name
the proof — the identical hash, the negative test that went red, the
measured timing. Where something was rejected, say what and why in one
line, so it is not re-proposed. Where the User overruled a proposal, say so
plainly and record the reasoning that replaced it; a commit that hides
whose call it was is a commit whose decision cannot be revisited.
