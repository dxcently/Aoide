---
type: reference
created: 2026-08-31
updated: 2026-08-31
tags: [aoide, harness, agent, hooks, kimi, protocol]
---

# Harness — Kimi Code

One harness's specifics, beside [[HARNESS-CLAUDE-CODE]]. The protocol itself
([[DEV]], [[ORCHESTRATION]], [[VERIFICATION]]) assumes neither.

Scope: how Kimi Code supplies the tier roles and the session plumbing the
protocol asks for, and the traps that cost real time here.

---

## Wiring it to Aoide

```
aoide hooks install kimi
```

Writes `[[hooks]]` blocks into `~/.kimi-code/config.toml`. The settings
format is TOML, so the door command is the bare
`aoide session hook --agent kimi` — no shell wrapper, no redirect, and none
of the per-event stdout scoping the claude wrapper needs.

**Ten events reach the door, one more than claude:** `SessionStart` ·
`UserPromptSubmit` · `PreToolUse` · `PostToolUse` · `Stop` · `SubagentStart`
· `SubagentStop` · `SessionEnd` · `Notification` · `PermissionRequest`.

`PermissionRequest` is the one kimi alone gets (`events_for`,
`commands/hooks.rs`). It is why `KIMI_PROFILE.permission_vocab` is empty:
kimi signals awaiting through a dedicated event, so the message-substring
matching claude needs never applies here.

**`skills_dir` is `None`.** No skills-directory concept is verified for this
harness, so `hooks install` skips the skill link and says so. A skill path
here would be a guess.

**Never commit this harness's settings file.** The User maintains it.

---

## Supplying the tiers

Kimi is a single model, so it supplies one tier at a time rather than a
range. [[ORCHESTRATION]]'s fallback applies: run architect-shaped prompts on
it with an explicit thinking budget, and still dispatch review as a separate
instance.

The standing lane is **wiki upkeep** — every content page and the
`OPERATIONS` protocol that governs them. Briefs point at
`protocol/OPERATIONS/{Style,Assertion,Lint}.md` rather than restating style
rules, so the wiki's own protocol stays the one authority.

**Writes are serial and never delegated.** Kimi plans the page list, then
edits page by page in its own context, committing at the end. Read-only
fan-out — finding every page naming a term, reviewing an edited page against
ground truth — is allowed and useful, and is kept BATCHED: one subagent
answering many questions, never one per page. Unbounded research fan-out is
what exhausted the quota mid-sweep once already.

Its dispatch tool is `Agent`, confirmed in captured 0.31.1 payloads
(`tool_name: "Agent"`); there is no `Task` alias.

---

## Driving a headless kimi

```
kimi -p <brief>              # headless, one shot
kimi --session <id>          # resume that session by id
```

`--session` with an id resumes that session; without one it opens an
interactive picker. The id is the same `<session_id>` the profile's
transcript locator keys on.

**The permission prompt, and the key that is not the obvious one.** A
headless kimi parks on its own TUI prompt: *"Run this command? / 1. Approve
once / 2. Approve for this session / 3. Reject / 4. Reject with feedback."*

| intent | key | why |
|---|---|---|
| approve this request | `1` | `KIMI_PROFILE.permission_keys.approve` |
| deny | `3` | option 4 is reject-with-feedback, not a bare deny |

**`2` is the session-wide allow-all.** A summons approves one request; using
`2` grants every later request in that session at once. The digit alone both
chooses and confirms — no trailing submit byte.

```
aoide send --id <id> --yes --submit -- "1"
```

**Kimi's TUI submits on `\r`, not `\n`.** A plain newline types the line
without submitting it. `submit_key` in the profile carries this; a hand-rolled
injection that sends `\n` leaves the text sitting unsent on the prompt. The
tree has no such hand-roll left: every pty injection — `send`, the doorbell
ring, `spawn --prompt`, `resurrect`'s restore delivery, the A2A door's opening
turn — writes the text and then the target's own `submit_key` as a separate,
later write (`write_delivery`, resolved through `profile_for_agent`).

---

## Traps

**Quota death is silent and leaves a corpse.** Kimi enforces a 5-hour and a
7-day window. A headless spawn past either dies instantly with
`provider.api_error: 403`, having registered a session and written nothing.
Check the quota before spawning, and tear the dead session out of the roster
with `aoide session end --id <id>` rather than leaving it parked.

**A mid-job quota death needs a verify-and-finish, never a blind redo.** One
sweep died with 23 pages edited, uncommitted, and no log entry. The recovery
is a librarian on another model briefed to the SAME protocol pages, told to
audit what is already in the working tree and finish it.

**A brief that is too large gets a stopping rule, not a fan-out.** Tell it:
stop after committing a coherent subset and report what remains.

**A prompt delivered at spawn can race startup** — logged as delivered, never
landed. Re-sending after the session registers works. This is shared with
[[HARNESS-CLAUDE-CODE]] and is not kimi-specific.

---

## Related

- [[HARNESS-CLAUDE-CODE]]
- [[ORCHESTRATION]]
- [[DEV]]
