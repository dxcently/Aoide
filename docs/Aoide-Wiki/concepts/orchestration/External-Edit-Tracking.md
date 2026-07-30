---
type: concept
created: 2026-07-30
tags: [aoide, orchestration, conductor, session, git, editor]
---

# External Edit Tracking — catching a human's direct file edits (plan)

**Status: specified; not implemented (khoa, 2026-07-30). Nothing on this page
exists in `pkgs/aoide/src` today — this is the build plan.**

## The problem it solves

An orchestrating Claude session sees the files it touches through its own
`Edit`/`Write` tool calls. It has no way to see a file touched any other way —
most commonly a human opening `nvim` inside a conducted shell and saving
changes there. Today that edit is invisible to the orchestrator until it
happens to re-read the file, so a stale in-context view of a file and a
human's fresh-on-disk edit can silently diverge.

## Detection — built on `conduct`'s existing PTY tick

[[Conductor-Channel|`conduct`]] already ticks a conducted shell's foreground
process on every poll, and that tick already knows how to recognize an editor:
`EDITOR_BASENAMES` (`nvim`, `vim`, `vi`, `nano`, `emacs`, `hx`, `micro`) and
`friendly_editor_command` (`pkgs/aoide/src/graph.rs`) name the foreground
process as `"<editor> <file>"` for the `activity` field today. This design
extends the same tick with a before/after snapshot around exactly that
editor window:

1. **Enter.** The foreground process's basename lands in the editor set →
   snapshot `git status --porcelain` in the shell's cwd, but only when the cwd
   resolves to a repo (`git rev-parse --show-toplevel` succeeds) — plus a
   timestamp. This is the "before" state.
2. **Leave.** The foreground process drops back out of the editor set (the
   bare shell prompt returns) → take an "after" `git status --porcelain`
   snapshot.
3. **Diff.** The before/after snapshots are diffed to the set of paths that
   became newly modified or added during that specific edit window — not
   every dirty path in the repo, which can include changes that pre-date the
   edit. A `git diff --stat` between the two snapshots rides alongside as a
   natural extra field: how much changed, not only which files.

## The stage file — `song/stage/edits.json`

A new stage file joins `sessions.json`/`hooks.json`/`graph.json`/
`projects.json`, written with the identical atomic write-temp-then-rename
pattern every stage file uses ([[Widget-Bridge-Contract]]). One record per
detected edit event:

| field | meaning |
|---|---|
| `sessionId` | the conducted shell's session id |
| `parentSessionId` | the owning orchestrator session — the [[Session-Graph]] `spawned` edge that says WHO to report back to |
| `repoRoot` / `cwd` | the resolved git toplevel and the shell's cwd at edit time |
| `editor` | the editor basename (`nvim`, `vim`, …) |
| `paths` | the changed file paths, relative to `repoRoot` |
| `diffstat` | the `git diff --stat` summary between the before/after snapshots |
| `startedAt` / `endedAt` | the edit session's enter/leave timestamps |

The file is append-only in spirit — an editor session that touches nothing
writes no record, but a busy shell can still accumulate many — so it needs a
pruning mechanism to stay bounded (see the `ack` verb below), the same shape
as `graph prune`/`graph reap` keeping `sessions.json` from growing unbounded.

## Report-back — how the orchestrator learns about it

### v1, concrete: a pull model

A new CLI verb reads the stage file:

```
aoide graph edits [--session <id>] [--since <timestamp>] [--json]
aoide graph edits ack --id <editId>
```

`graph edits` lists external edit events, optionally scoped to a session or a
time window — the call an orchestrator makes at the start of a turn, or right
before it is about to `Edit` a file itself, to check for a conflict with a
change it does not yet know about. `graph edits ack --id <editId>` marks an
event acknowledged and prunes it, keeping the stage file from growing
unbounded the same way `graph prune` keeps `sessions.json` bounded.

### Still open: how the orchestrator learns without polling

Claude Code's hook surface runs one direction — session→bridge (SessionStart,
UserPromptSubmit, PreToolUse, …) — so nothing today can interrupt a running
turn to push new information the other way, bridge→session. Two candidate
mechanisms are on record for closing that gap, and choosing between them —
or deciding neither is worth it — is an open thread (`ingest/log.md`), not a
decision this page makes:

- **(a) PTY injection.** Reuse `graph send`-style injection ([[Conductor-Channel]])
  to type a system-like notice into the orchestrator's own terminal ahead of
  its next prompt read. The real, flagged risk: this collides with a human
  mid-keystroke in that same terminal.
- **(b) A visual badge.** A small marker on the session's row in the
  Conductor/Terminals gadgets, extending the existing `say`/`activity` field
  pattern ([[Widget-Bridge-Contract]]), that a human notices and relays. This
  sidesteps the injection-collision risk entirely, at the cost of depending
  on a human noticing it.

## Scope boundary — git repos only, v1

A conducted shell whose cwd does not resolve to a git repo is simply not
tracked by this feature in v1: no mtime-scanning fallback, no attempt to
generalize edit-detection beyond `git status`. Khoa's own framing for the
feature is "it can use git for this" — the scope stays narrow to that.

## Related

- [[Conductor-Channel]] — the PTY tick and `graph send` injection door this feature extends and reuses.
- [[Session-Graph]] — the `parentSessionId` spawned edge that resolves who owns a conducted shell.
- [[Widget-Bridge-Contract]] — the stage-file contract (atomic write-temp-then-rename, one file per concern) this design follows, and the `say`/`activity` field pattern candidate (b) extends.
- [[Agent-Hooking]] — the session-registration doors a conducted shell already goes through; this feature is additive to the same `sessions.json`/`hooks.json` machinery.
