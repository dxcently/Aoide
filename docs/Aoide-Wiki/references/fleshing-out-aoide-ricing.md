# Fleshing out Aoide ricing — the rehearsal, the cue, and the observation stack

Status: a design handout captured from a chat session (2026-08-18), not yet
built and not yet LOCKED — khoa has steered every section at least once, but
the final "lock" word hasn't been said. Nothing here is asserted as existing
unless marked EXISTS. A companion command flesh-out was produced by another
agent in a separate session; it was not found anywhere in the repo at capture
time, so this handout carries only this session's design — fold that document
in when it surfaces.

---

## 0. Scope and the one governing principle

The ask: formalize the rice loop for agents — any agent picks up the ricing
harness, its actions are watched step by step, drafted rice states revert
mechanically, agents know their step and can orchestrate reviews/planning,
all agent-agnostic — and generalize the same machinery so an agent can hook
onto the conductor for any purpose, not just ricing.

The governing principle that survived every revision:

> **Observing is free; conversing costs tokens.** Everything worth watching
> about a local agent already lands somewhere free (hook events, on-disk
> transcripts, a PTY aoide owns). A2A is a chat wire — an agent spends model
> turns composing protocol replies — so it is never the observation path for
> anything local. No fourth protocol gets built: the ricing harness is verbs
> and state files riding the existing three doors (CLI / MCP / A2A, one
> schema). ACP is dead (folded into A2A); AG-UI is the wrong layer.

And the discipline added last (khoa, closing note of the session):

> **Terse index always, full contract on demand.** A connected agent carries
> only the command *index* — names plus one-liners, enough to know aoide has
> a tool for the job — and pulls a command's full contract (schema, flags,
> semantics) only at the moment a task actually needs it: `aoide schema
> --json` scoped per command, `aoide guide <topic>` for prose, MCP tools
> served deferred/terse the same way. Nobody front-loads the whole surface
> into an agent's context.

## 1. The layer stack

```
┌──────────────────────────────────────────────────────────────┐
│  CUE  (orientation — pull-only, never pushed)                │
│    aoide cue · the business-triggered stderr line            │
├──────────────────────────────────────────────────────────────┤
│  CONDUCTOR CITIZENSHIP  (general — any agent, any purpose)   │
│    register: hook door │ graph wrap │ conduct │ A2A(remote)  │
│    observe:  hook edges + graph tail (transcript ∨ PTY tee)  │
│    steer:    graph send (gated) │ A2A message/send (remote)  │
├──────────────────────────────────────────────────────────────┤
│  ENGAGEMENTS  (scoped protocols riding citizenship)          │
│    the rehearsal — the rice loop, formalized (only tenant;   │
│    no generic engagement framework until a second exists)    │
└──────────────────────────────────────────────────────────────┘
```

Each layer points down, never duplicates. Ricing is a *tenant* of the
conductor, not the reason for it.

## 2. Doors — who grips whom (clarified, mostly EXISTS)

The conductor's hold on local agents is mechanical and token-free; A2A is
for agents aoide can only reach as a *peer* (usually remote):

| agent on this box | door onto the conductor | cost |
|---|---|---|
| claude, kimi (hook systems) | hook door — `aoide graph session hook` | free |
| codex, gemini, aider, any CLI | `graph wrap` / `conduct` (PTY) | free |
| anything scriptable | explicit verbs — `graph session start/phase/end` | free |
| an A2A-speaking agent (remote, usually) | A2A door — `a2a agent add` / `message/send` | model turns |

All four converge on the same `sessions/hooks/graph.json` records; an
A2A-registered agent folds in as a `kind: "a2a"` root node beside the hooked
and wrapped ones. The wiki sentence, once locked: *the conductor hooks every
local agent mechanically (hooks, PTY, verbs — zero tokens) and speaks A2A
outward to whatever it can't touch.* Agent-agnosticism lives in the
`AgentProfile` seam (EXISTS) — a new harness is one profile entry.

## 3. The cue — self-orientation, strictly pull

An agent doing aoide-related work learns of `aoide cue` from the pull
surfaces it already reads — `AGENTS.md`, `aoide guide`, bare `aoide`, the
MCP tool index. **No SessionStart context injection, ever** (cut by khoa:
the hook door keeps tracking state passively but injects nothing; an agent
minding its own business never hears from aoide).

### 3.1 `aoide cue [--json]` — the front desk

One read, three answers, pointers not payload (the terse-index discipline):

```json
{
  "identity":   { "registered": false, "matchedBy": null,
                  "hookOn": "aoide graph join --agent <name>" },
  "capability": { "guide": "aoide guide", "schema": "aoide schema --json",
                  "doors": ["cli", "mcp:stdio", "a2a:off"] },
  "assignment": { "engagement": "rehearsal", "song": "sonata",
                  "draft": "neon-night", "step": "check",
                  "instruction": "vision-check the live desktop…",
                  "next": ["screen shot", "rice score advance"] }
}
```

- **identity** — matched session record (`AOIDE_SESSION_ID` env first,
  pid-ancestry against `sessions.json` second — the same match the hook
  door's window discovery already runs), or the exact hook-on command.
- **capability** — pointers to guide/schema; the cue never duplicates them.
- **assignment** — the active engagement's state for THIS session, or
  pending graph business (an undelivered `graph send`, an unanswered
  `awaiting`), or honest `null`.

Bare `aoide` prints the human rendering of the same resolution before the
command summary.

### 3.2 The ambient line — business-triggered, never status-triggered

Every `aoide` invocation may append **at most one line to stderr** (stdout
stays schema-pure for `--json`; `AOIDE_NO_CUE=1` mutes):

```
aoide <cmd> runs
     │
     ├─ command's domain has live business for THIS caller?
     │    · engagement active in the domain just touched
     │      (a rice verb while a rehearsal is running)
     │    · a pending graph send addressed to it
     │    · it's marked awaiting and something answered
     │        │
     │       yes ──► "♪ cue: rehearsal neon-night @ check — `aoide rice score`"
     │
     └─ anything else — unregistered, idle, no engagement ──► silence
```

"Unregistered" is not business. Registered-ness never appears in the line.

### 3.3 `graph join` — demand-driven enrollment

`aoide graph join [--agent <name>]` = the explicit-verbs door with
auto-detection (pid/cwd/tty, window via the hook door's `hyprctl` walk),
keyed by pid-ancestry so later calls re-match without an env var. **Nothing
volunteers it.** Hooking happens top-down, when actually needed:

| who decides | how it lands |
|---|---|
| khoa | wraps/conducts the agent, or tells it to join |
| an orchestrator | spawns via `conduct`/`wrap` (auto-registered), or its brief says `graph join` |
| an engagement | a score step whose mechanics require identity says "`graph join` first" — an instruction inside work already accepted, not ambient pressure |

Known bite: pid-ancestry breaks for double-fork daemonizers (reparent to
init); `AOIDE_SESSION_ID` / `graph join --id` is the manual override.

## 4. The observation stack — output reaches aoide for free

```
                    richness ▲
  ┌────────────────────────────────────────────────────────────┐
  │ TRANSCRIPT TAP      profiled harnesses (claude, kimi)      │ EXISTS
  │   AgentProfile.transcript locates/tails the on-disk        │ (TranscriptSpec)
  │   transcript: prose, tool calls, model, context fill.      │
  ├────────────────────────────────────────────────────────────┤
  │ PTY TEE             ANY agent, hookless included           │ NEW
  │   `conduct` owns the child's tty; tee output to            │
  │   state/output/<session-id>.log, ring-capped.              │
  ├────────────────────────────────────────────────────────────┤
  │ HOOK EDGES          state, not content                     │ EXISTS
  │   working/awaiting/stopped + which tool.                   │
  ├────────────────────────────────────────────────────────────┤
  │ A2A                 REMOTE agents only                     │ EXISTS
  │   command + status; never local observation.               │
  └────────────────────────────────────────────────────────────┘
```

**`aoide graph tail <session-id> [--follow] [--json]`** — one resolver:
transcript when the profile has a `TranscriptSpec`, PTY log otherwise,
structured error naming what it tried when neither exists. The conductor TUI
and a dock output pane render the same resolution — coherence is one
resolver, not per-source UI.

Caveats carried forward: PTY capture of a TUI agent is escape-code soup —
v1 is raw ring log + ANSI-stripped last-N lines ("ugly but true"); a
hand-rolled vt screen model is a later phase only if TUI capture ever
matters (the TUI-heavy harnesses have transcripts). Output logs live under
`state/`, not `stage/` — durable observation artifacts, same reasoning as
captures. The tee lives in `conduct`, not `graph wrap` (wrap stays
inherit-stdio pure; capture-without-steering is one flag later if wanted).
Kimi's `wire.jsonl` flush cadence needs a live check before `--follow`
claims "live" rather than "recent".

## 5. The rehearsal — the rice loop, formalized

A rehearsal is the rice loop *running, with agents bound to it*: a state
machine at `stage/rehearsal.json`. It always iterates **inside a draft**
(requires `rice mode draft` — buys symlink routing, a home for history, and
`rice mode declarative` as the ever-present escape hatch). Plain staging
stays the informal human path. The name fits the existing lexicon:
rehearse = staged/live, record = declared/baked.

```
aoide rice rehearse begin <song> [--draft <name>]   — enters mode draft
        │
        ▼
  ┌── plan ──── edit ──── check ──── review ──── mark ──┐
  │     │         │          │           │         │     │
  │  intent    stage/     screen      verdict   named    │
  │  note      draft      shot +      from a    take     │
  │  touched   writes     vision-     DIFFERENT ("A","B" │
  │  (gate)    (auto-     check       session   — real   │
  │            taken)                 (gate)    rehearsal│
  │                                             marks)   │
  └────────────────────── iterate ◄─────────────────────┘
        │
        ▼
aoide rice rehearse end [--distill]   — draft survives; journal → design log
        ▼
aoide rice declare                    — unchanged: khoa-gated, still planned
```

### 5.1 `rice score` — the self-describing step

At any moment, any agent, through any door:

```json
{ "rehearsal": { "song": "sonata", "draft": "neon-night", "step": "check",
                 "take": 17, "marks": { "A": 9, "B": 14 } },
  "instruction": "Vision-check the live desktop: `aoide screen shot`, verify
                  polarity agreement + widget/bar livery match.",
  "allowed": ["rice take", "rice score advance", "screen shot"],
  "gate": { "advance-to": "review", "requires": null } }
```

Step instructions live in the score's step table — one place, versioned
with the binary, not per-harness prompts. That is what keeps step-by-step
understanding agent-agnostic: pushed to capable harnesses, identical when
pulled by a bare CLI agent. `rice score advance` moves the step and refuses
when a gate isn't met.

### 5.2 Takes and marks — mechanical revert

- **Takes** — automatic, linear, cheap: every write to the routed draft
  snapshots `songbook/<song>/drafts/<name>/takes/NNNN.json` (livery + cover
  + meta: timestamp, session id, cause). Captured at the two write
  entrypoints (`rice stage`, `cover set`) directly; hand-edits caught by a
  content-hash check on every hook-door `PostToolUse` during an active
  rehearsal AND ambiently on any `aoide rice` verb — a hookless agent's
  edits get taken the next time it touches the CLI. Explicit `rice take`
  for paranoia. Leaning toward hashing cover.json alongside livery on every
  check (one extra file read; not finalized).
- **Marks** — named rehearsal marks (the music term for lettered jump
  points): the `mark` step stamps the current take with a letter, meaning
  *reviewed-and-passed*.

```
aoide rice back              → previous take        (one agent-action undo)
aoide rice back --take 9     → that take exactly
aoide rice back --mark A     → last reviewed-good state
aoide rice take list         → the timeline, marks flagged
aoide rice take diff         → change since the last mark (reviewer's view)
```

Revert is trivial because of draft routing: write take N's content through
the existing symlink via `atomic_write`; Quickshell hot-reloads it like any
stage write. No new apply path. **Scope line:** takes cover the
stage-routed files (livery, cover). Widget QML bodies are committed
songbook files — git is their revert mechanism, and the `edit` step's
instruction says so. No parallel VCS.

### 5.3 Watching — correlation, not new plumbing

During an active rehearsal the hook door also appends to
`stage/rehearsal-journal.jsonl`:

```json
{ "take": 17, "sessionId": "…", "event": "PostToolUse",
  "tool": "Edit", "drift": true }
```

The journal answers *which agent action produced take 17* — what makes
`rice back` meaningful rather than blind. Surfaces: `aoide rice watch`
(live tail), the conductor pane and a dock Rehearsal gadget (both consume
the O3 output pane, built once), and `rehearse end --distill` — the
songbook discipline made mechanical: journal + take timeline summarized
into the song's `design/intent.md` iteration log, so self-ricing's
write-back stops depending on agent virtue.

### 5.4 Reviews and planning — gates, not scripts

Orchestration itself stays out of scope: agents already spawn each other
via `conduct` / `graph send` / A2A with `--parent` edges. The rehearsal
adds enforcement:

- `rice review record --verdict pass|fail --notes …` — `score advance`
  from `review` refuses unless the verdict came from a **different session
  id** than the editing one. The executor cannot self-certify. `--solo`
  exists for khoa, flagged in the journal. (Different-*session*, not
  different-*agent*: model identity is unverifiable and harness-specific.)
- The `plan` step gates on an intent-note touch — planning leaves an
  artifact or didn't happen.
- A reviewer spawned through any door runs `rice score`, sees
  `step: review` + `rice take diff`, and knows its job with zero briefing.

## 6. How an agent picks up the harness

Layered by capability, degrading gracefully — the score and cue being
self-describing is what makes every tier land on the same truth:

| tier | mechanism | covers |
|---|---|---|
| 0 | `AGENTS.md` + `aoide guide` (rice-scoped section) + bare `aoide` | anything that reads a repo |
| 1 | the business-triggered cue line (§3.2) — just-in-time, pull-shaped | anything that runs `aoide` |
| 2 | MCP: `rice_score` / `rice_take` / `rice_back` / `cue` in the tool index for free (one schema), served terse with contracts fetched on demand | MCP-speaking agents |
| 3 | A2A: rehearsal state on `tasks/get`; AgentCard advertises the skills; a remote agent joins via `message/send` into the rehearsal's context | external/remote agents |

Nothing in any tier names a harness; a new harness is one `AgentProfile`
entry.

## 7. Phasing

Observation lands first (small, orthogonal, and the rehearsal's watch UI
consumes it); each phase reviewed before the next, house style:

- **O0 — the cue**: `aoide cue`, the business-triggered stderr line,
  `graph join`. Pure reuse of existing matching. No injection work.
- **O1 — `graph tail`**: transcript-backed via the profile seam.
- **O2 — PTY tee** in `conduct`: ring-capped `state/output/<id>.log`,
  tail falls back to it, ANSI-strip on read.
- **O3 — conductor/dock output pane** rendering the same resolver.
- **A — takes + `rice back`**: journaling at the write entrypoints,
  explicit `rice take`, revert-through-routing. Safety floor first.
- **B — the rehearsal state machine**: `rehearse begin/end`, `rice score`
  + step table + `advance`, draft-mode requirement.
- **C — hook correlation**: auto-take on `PostToolUse` drift, the
  rehearsal journal, participant binding.
- **D — the review gate**: `rice review record`, distinct-session
  enforcement, `--solo`.
- **E — pickup surfaces**: `guide` rice section, AGENTS.md; MCP/A2A free.
- **F — watching UI**: `rice watch` + dock Rehearsal gadget on O3,
  `rehearse end --distill`.

## 8. Decisions made (unmake at will) and open items

Decided in-session, one line each:

- Rehearsal **requires** draft mode — takes need a home; routing gives
  revert free; plain staging stays informal.
- No new protocol — verbs on the existing three doors; ACP dead, AG-UI
  wrong layer; A2A = remote command + status only.
- Widget bodies revert via git, not takes.
- Review gate = different-session-id, not different-agent.
- The cue is pull-only: no SessionStart injection; the ambient line fires
  on business, never on status; `graph join` is suggested by nothing except
  a score step that mechanically requires identity.
- PTY tee in `conduct` only; output logs under `state/`.
- Terse index always, full contract on demand (§0) — applies to the CLI
  help, the MCP tool list, and any context an orchestrator hands a worker.
- No generic engagement framework — the rehearsal is the only tenant;
  `stage/rehearsal.json` keeps an imitable shape as the one line of
  future-proofing.

Open / uncertain:

- The other agent's command flesh-out document — not found in the repo at
  capture time; fold in when it surfaces.
- Kimi transcript flush cadence vs `graph tail --follow` liveness.
- Whether auto-take hashes cover.json on every check (leaning yes).
- If real agents ignore the cue line, auto-join for known harness
  ancestries is the noted one-line escalation — not built.
- The design as a whole awaits khoa's explicit LOCK before wiki concept
  pages assert any of it as existing.

## Related

- [[Self-Ricing]] — the existing loop, drafts, and mode machinery this rides
- [[Ricing-Protocol]] — creation/application split + the mandatory vision-check
- [[Agent-Hooking]] — the doors and the AgentProfile seam
- [[Agent-Interface]] — CLI trunk, MCP façade, guidance tiers
- [[A2A-Door]] — the wire this handout scopes to remote/peer command
- [[Session-Graph]] · [[Conductor-Channel]] · [[Terminal-Commander]]
