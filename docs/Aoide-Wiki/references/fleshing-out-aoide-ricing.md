# Fleshing out Aoide ricing — the rehearsal, the cue, and the observation stack

Status: a design handout captured from a chat session (2026-08-18), not yet
built and not yet LOCKED — the User has steered every section at least once, but
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

And the discipline added last (the User, closing note of the session):

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
│    escalate: the sudo door — declared ops, polkit auth (§8)  │
├──────────────────────────────────────────────────────────────┤
│  ENGAGEMENTS  (scoped protocols riding citizenship)          │
│    the rehearsal — the rice loop, formalized (first tenant;  │
│    no generic framework until a second tenant is BUILT)      │
│    intended: nix maintenance · nix development (§6)          │
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
MCP tool index. **No SessionStart context injection, ever** (cut by the User:
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
| the User | wraps/conducts the agent, or tells it to join |
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
aoide rice declare                    — unchanged: User-gated, still planned
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
  exists for the User, flagged in the journal. (Different-*session*, not
  different-*agent*: model identity is unverifiable and harness-specific.)
- The `plan` step gates on an intent-note touch — planning leaves an
  artifact or didn't happen.
- A reviewer spawned through any door runs `rice score`, sees
  `step: review` + `rice take diff`, and knows its job with zero briefing.

## 6. Intended siblings — the nix loops (intentions only, not designed)

The engagement layer has two more claimed tenants: a **nix maintenance
loop** and a **nix development loop**. Neither is designed here — this
section exists so the rehearsal isn't mistaken for the whole story, and so
the shapes below constrain what the rehearsal's machinery may assume. An
engagement is a step table + gates + a journal riding citizenship; what
changes per tenant is the **substrate** — where writes land and what
reverts them:

| engagement | the loop | revert substrate | hard gate |
|---|---|---|---|
| rehearsal (§5) | plan → edit → check → review → mark, inside a draft | takes through draft routing | different-session review verdict |
| nix maintenance | plan → bump (flake inputs) → build → generation diff → review → switch | nix generations + git — **no new snapshot machinery** | the switch stays User-run ([[Rebuild-Gate]]); agents stop at a built, reviewed, un-switched generation |
| nix development | plan → edit → build/test → live-surface check → review → commit | git | tests pass + different-session review before commit — the test→show→confirm→log discipline made mechanical |

What the intentions buy now, for free:

- The cue's `assignment.engagement` field is already a name, not an enum —
  `"rehearsal"` today, `"maintenance"` or `"development"` later with zero
  cue changes.
- The rehearsal must not bake rice-isms into anything shared: the score's
  step-table shape, the journal line shape, and the review-gate mechanics
  are the parts a nix loop would imitate, so they stay substrate-blind.
  Takes/marks are rice-only (the nix loops get revert from nix and git)
  and stay inside `rice`.
- **When the second tenant is built** — not before — the shared engagement
  shape gets extracted (one `score`-like resolver, per-engagement step
  tables). Until then, imitation over abstraction.

Both nix loops inherit the review discipline unchanged: a reviewer session
distinct from the editor, verdicts journaled, `--solo` for the User. The
maintenance loop's terminal step is the one place an engagement ends at a
**human** action by design — the rebuild gate is governance, not a missing
feature.

**Generation ↔ codebase round-trip.** Rolling back the *built system* is
native nix (numbered generations, `nixos-rebuild switch --rollback`, the
boot menu). What nix does **not** do is map a generation back to the config
codebase that produced it — the source is a GC-able build-time dep. The
maintenance engagement closes that gap the takes-journal way: set
`system.configurationRevision = self.rev` so every generation is stamped
with its commit, journal `{generation, gitRev, flakeLockHash}` at each
build/switch step, and the intended revert verb (`aoide nix back
--generation N`, name provisional) resolves generation → recorded rev →
`git checkout` — flake.lock rides along because it lives in the repo. A
dirty tree builds with `self.rev = null` and cannot round-trip, so the
build step's gate refuses (or loudly journals) a dirty build. No new
snapshot machinery: git + generations + one journal line.

## 7. Rollback and pruning — one verb, two modes

Every verb here serves **both audiences from one implementation**. There is
no human CLI and no agent CLI; there is one verb with two entrances:

```
                       aoide rice back
                              │
        ┌─────────────────────┴─────────────────────┐
   flags / --json                            bare, on a tty
   (agents, scripts)                         (the User)
        │                                           │
   selection is given                     numbered picker, multi-select
   executes, structured out               where it makes sense, confirm
        │                                           │
        └──────────► same code path, same rails ────┘
                     same audit line
```

Hand-rolled prompt — rows, a number or a space-toggled set, y/n — no TUI
crate, the same no-new-deps discipline as the A2A server. A non-tty stdout
never prompts, so a piped or agent invocation cannot hang waiting on a
human.

| bare verb on a tty | picker rows | on select |
|---|---|---|
| `rice back` | two labeled lanes: **takes** (NNNN · age · cause · session, marks lettered) and **widget commits** (recent git commits touching the song's widget bodies) | a take writes through the routing (same as `--take N`); a commit does `git checkout <rev> -- songbook/<song>/…` — working-tree restore, HEAD never moves |
| `rice take diff` | takes/marks to diff against | the diff, no write |
| `aoide nix back` | journaled `{generation · rev · subject · age · dirty?}` | `git checkout <rev>` of the config repo; then **offers** the matching generation rollback as a second, separately-confirmed step |

Two teeth in that table:

- The nix **switch-rollback offer exists only behind the interactive
  confirmation — there is deliberately no flag for it.** That is the
  rebuild gate enforced mechanically: a human at the tty is the gate in
  person; an agent scripting flags can check out code but can never reach
  the switch.
- The widget-commit lane is the "help with rolling back through git"
  verb: takes cover the stage-routed files (§5.2), git covers widget
  bodies, and the one picker shows both lanes so the User never has to
  remember which substrate owns which file.

The discipline generalizes as a CLI-wide convention: any verb whose flags
select one-of-many (a song, a draft, a take, a session id) grows the same
bare-on-a-tty picker, applied opportunistically as verbs get touched — not
as a big retrofit pass.

### 7.1 Pruning — nix entirely, never a second store

**Decision: just nix.** Aoide invents no store, no generation format, no
snapshot layer. Pruning verbs are wrappers whose whole contribution is
*selection, correlation, and rails* — nix already does the deleting, and
`graph prune` (EXISTS — drops finished session records) already sets the
house meaning of the word.

| verb | wraps | protected by default |
|---|---|---|
| `rice take prune [--older-than 14d] [--keep 20] [--all-but-marks]` | aoide's own `takes/` dir — its only native storage | **marked** takes (reviewed-good states); `--force` to take one out |
| `aoide nix prune [--generations 42,43] [--older-than 30d] [--keep 5]` | `nix-env -p /nix/var/nix/profiles/system --delete-generations …`, then optionally `nix store gc` | current + booted generation (nix refuses these anyway — a free rail); any generation the journal flags `keep` |
| `aoide graph prune` | EXISTS — done session records | — |

Both prune verbs are dry-run-shaped: they print what would go and what it
frees, and the bare-tty mode is a **multi-select** picker (space toggles,
Enter confirms) because pruning is the one place selecting several at once
is the normal case. The symmetry worth keeping: **a mark protects a take
exactly as `keep` protects a generation** — one idea, two substrates.

Consequence of pruning worth stating once: deleting a generation does not
delete its commit. The §6 round-trip degrades gracefully — a pruned
generation still resolves to its rev, so recovery becomes *checkout and
rebuild* instead of *boot straight into it*. Related grounded finding:
`nix.gc.automatic` is **not** configured anywhere in this repo today, so
nothing is silently eating generations behind the journal's back — if it
is ever turned on, its retention and the journal's `keep` flags need to
agree.

## 8. The sudo door — privilege with a human inside it

This is not a new invention: `modules/nucleus/aoided.nix` already declares
aoided as owner of "the user-gated rebuild pipeline (**polkit pattern**):
agent proposes → user admits → git records", and the compositor facet
already runs **hyprpolkitagent** as a user service, installed explicitly
so that "the aoided rebuild gate's future polkit prompt" would have
something to present a dialog with. Both EXIST. This section specifies the
thing they were left waiting for.

First, the distinction that makes the door worth building:

| path | what it is | verdict |
|---|---|---|
| `needsSudo` (EXISTS) | an agent typed `sudo` inside its own conducted shell and is now **stuck** at a password prompt; aoide detects it and badges the roster card | passive, a *stall* to be rescued — the guide should teach agents away from it |
| the sudo door (NEW) | the agent never runs `sudo`; it **requests a declared operation** and aoide performs it after the User authenticates | the supported path |

```
agent: aoide sudo request nix.prune --older-than 30d --reason "…"
  │   an op id from the nix-declared allowlist — never an argv
  ▼
state/escalations.json → pending
  │   agent polls `aoide sudo status <id>`; DENIED is a normal result,
  │   not an error to retry-loop on
  ▼
summons raised on surfaces that already exist
  (conductor roster card · dock sudo badge · desktop notification)
  ▼
the User authenticates — polkit prompt at the seat (hyprpolkitagent, EXISTS)
  │   the PAM stack decides the factors: password · TOTP · FIDO2 key
  ▼
aoide's privileged helper runs THE DECLARED OP
  │   fixed argv template; agent values only in slots the op declares
  ▼
result + audit (Door::Sudo) + grant expires — single-use by default
```

The rails, each with its one reason:

| rail | why |
|---|---|
| op ids only, never a command line | the A2A door's rule, reused: the caller names the *what*, aoide owns the *how* |
| the allowlist is nix options | admitted at rebuild time = the existing rebuild gate, no new governance |
| fresh auth per request; single-use default, TTL only if an op declares it | an agent never inherits ambient root for its lifetime |
| no `shell` / `exec` / free-argv op, ever | one such op collapses the entire door into `sudo su` |
| no seat → no auth → stays pending, then expires | there is no headless bypass; absence of a human is a denial |
| every request, approval, denial, execution audited | one audit log, `Door::Sudo`, same as every other door |

### 8.1 The second factor is PAM's job, not aoide's

Aoide implements no authentication and holds no secret. Factors are
configured in nix on the PAM stack behind the polkit action —
`pam_oath` for TOTP, `pam_u2f` for a FIDO2 key, password alone if that is
what the User wants. The only thing an op declares is an **auth class**
(`admin`, `admin-2fa`), which selects which polkit action id it goes
through; strengthening a class is then a nix edit that needs no aoide
change. This is the same "just nix entirely" answer as §7.1 — the system
already has an authentication stack, and a second one written by aoide
would be strictly worse.

### 8.2 Reconciling with §6 and §7

A fair objection: §6 says the switch stays User-run and §7 says the
generation-rollback offer has no flag form — does a `nix.switch` op
undo that? No, and the distinction is the point: **the ban is on an
unattended switch, not on an agent-initiated one.** Through this door the
agent can *ask*, but the operation completes only when a human
authenticates at a live prompt, per request, with an audit line. That is
the rebuild gate mechanized — arguably stronger than a hand-typed `sudo`,
which authenticates nothing about *why*. What remains forbidden: any op
that would let the switch happen with no human present.

## 9. How an agent picks up the harness

Layered by capability, degrading gracefully — the score and cue being
self-describing is what makes every tier land on the same truth:

| tier | mechanism | covers |
|---|---|---|
| **S** | **the installable skill** (§9.1) — one generated `SKILL.md` dropped into a harness's skills dir | any agent with a skill mechanism, anywhere on the box — including one that never opens this repo |
| 0 | `AGENTS.md` + `aoide guide` (rice-scoped section) + bare `aoide` | anything that reads a repo |
| 1 | the business-triggered cue line (§3.2) — just-in-time, pull-shaped | anything that runs `aoide` |
| 2 | MCP: `rice_score` / `rice_take` / `rice_back` / `cue` in the tool index for free (one schema), served terse with contracts fetched on demand | MCP-speaking agents |
| 3 | A2A: rehearsal state on `tasks/get`; the AgentCard advertises capabilities (A2A calls these "skills" — unrelated to tier S); a remote agent joins via `message/send` into the rehearsal's context | external/remote agents |

Nothing in any tier names a harness; a new harness is one `AgentProfile`
entry.

### 9.1 Tier S — aoide as an installable agent skill

The gap this closes: the cue is strictly pull-only (§3), which is correct
but leaves an agent that has **never run `aoide` and never read this
repo** with no way to know aoide exists at all. Tier 0 covers
repo-readers. A skill covers everyone else, box-wide — it is the one
surface that reaches an agent working in a different directory entirely.

**Hard rule: the skill is a pointer, not a payload.** Roughly thirty
lines — what aoide is in two sentences, `aoide cue` as the one verb to
run, and where contracts live (`aoide schema --json`, `aoide guide
<topic>`). It must never carry step instructions, a command list, or
contract detail. Those live in the score's step table and the registry;
a skill that copies them is a second source that starts drifting the day
it is written, which is the exact failure the one-schema-three-doors rule
exists to prevent. This is also §0's terse-index discipline in its purest
form: the always-loaded part is one description line, and everything real
is fetched on demand.

```
aoide skill emit                      generated from the SAME registry as
   │                                  the MCP tool list and the AgentCard —
   │                                  a fourth generated artifact, not a
   │                                  hand-maintained file that rots
   ▼
aoide skill install --agent claude    idempotent, never-clobbering, reports
   │                                  added/present — the exact shape of the
   │                                  EXISTING `aoide hooks install <agent>`
   ▼
~/.claude/skills/aoide/SKILL.md       path comes from a `SkillSpec` on the
                                      AgentProfile — a one-field sibling of
                                      the `SettingsSpec` that already sits
                                      there (EXISTS) for hook settings
```

Rules that keep it honest:

- **Opt-in, never self-installing.** `skill install` is a User verb (or an
  orchestrator's), the same boundary hooks install already respects. An
  installed skill's description line is loaded by the *harness's* own
  relevance trigger, not pushed by aoide — which is why this does not
  violate the no-SessionStart-injection rule: the agent's own mechanism
  decides, and the User chose to put it there.
- **One skill, not a family.** A skill per domain would mean several
  always-loaded description lines in every agent's context — the
  front-loading banned in §0. One `aoide` skill, one verb, done. (A
  second would only earn its place if a domain's trigger words don't
  overlap "aoide" at all — "rice", "theme", "colors". One line noted;
  not built.)
- **Harness-agnostic, degrading to nothing.** A harness with no skill
  mechanism gets no `SkillSpec` and loses nothing — tiers 0–3 already
  cover it. A new harness that has one is, as ever, a single profile
  entry.

## 10. Phasing

Observation lands first (small, orthogonal, and the rehearsal's watch UI
consumes it); each phase reviewed before the next, house style:

- **O0 — the cue**: `aoide cue`, the business-triggered stderr line,
  `graph join`. Pure reuse of existing matching. No injection work.
- **O1 — `graph tail`**: transcript-backed via the profile seam.
- **O2 — PTY tee** in `conduct`: ring-capped `state/output/<id>.log`,
  tail falls back to it, ANSI-strip on read.
- **O3 — conductor/dock output pane** rendering the same resolver.
- **A — takes + `rice back`**: journaling at the write entrypoints,
  explicit `rice take`, revert-through-routing, and the bare-on-a-tty
  picker (§7) — the human mode ships with the verb, not after it.
  Safety floor first.
- **B — the rehearsal state machine**: `rehearse begin/end`, `rice score`
  + step table + `advance`, draft-mode requirement.
- **C — hook correlation**: auto-take on `PostToolUse` drift, the
  rehearsal journal, participant binding.
- **D — the review gate**: `rice review record`, distinct-session
  enforcement, `--solo`.
- **E — pickup surfaces**: `guide` rice section, AGENTS.md; MCP/A2A free.
  Plus tier S — `skill emit` + `skill install` and the `SkillSpec` field
  (§9.1). Small and independent of every engagement: it can land as early
  as O0, since a pointer-only skill has nothing to point at but the cue.
- **F — watching UI**: `rice watch` + dock Rehearsal gadget on O3,
  `rehearse end --distill`.

The two cross-cutting additions slot in beside them, both usable long
before any engagement exists:

- **P — pruning** (§7.1): `rice take prune` with `A`; `aoide nix prune`
  with the nix loop. Multi-select picker ships with each.
- **S — the sudo door** (§8): the escalation record, the polkit-backed
  helper, the summons on existing surfaces, `Door::Sudo` audit. Its first
  customer is `aoide nix prune` (deleting system generations needs root),
  which is why S lands with the nix work rather than after it.

## 11. Decisions made (unmake at will) and open items

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
- One verb, two entrances (§7) — agents and the User share every verb;
  bare-on-a-tty gets a hand-rolled picker (multi-select where selecting
  several is normal), flags/`--json` bypass it, non-tty never prompts.
- Pruning is nix entirely (§7.1) — wrap `delete-generations` / `store gc`,
  never a second store; marks protect takes as `keep` protects
  generations.
- Privilege goes through the sudo door (§8), never through an agent
  holding a password: declared op ids, nix-admitted allowlist, fresh
  polkit auth per request, single-use default, no free-argv op ever.
  Factors are PAM's (`pam_oath` / `pam_u2f`), selected by an op's auth
  class — aoide implements no authentication and stores no secret.
- The ban is on an **unattended** switch, not an agent-initiated one
  (§8.2) — an agent may ask; only a live human authentication completes
  it.
- Tier S: aoide ships as one installable agent skill (§9.1), generated
  from the registry, **pointer-only** (never step instructions or a
  command list), installed by a User verb into a path the `SkillSpec`
  names. One skill, not a family.
- No generic engagement framework — the rehearsal is the only **built**
  tenant; nix maintenance and nix development are intended siblings (§6),
  and the shared shape gets extracted only when the second tenant lands.
  `stage/rehearsal.json` keeps an imitable, substrate-blind shape as the
  one line of future-proofing.

Open / uncertain:

- The other agent's command flesh-out document — not found in the repo at
  capture time; fold in when it surfaces.
- Kimi transcript flush cadence vs `graph tail --follow` liveness.
- Whether auto-take hashes cover.json on every check (leaning yes).
- If real agents ignore the cue line, auto-join for known harness
  ancestries is the noted one-line escalation — not built.
- The design as a whole awaits the User's explicit LOCK before wiki concept
  pages assert any of it as existing.

## Related

- [[Self-Ricing]] — the existing loop, drafts, and mode machinery this rides
- [[Ricing-Protocol]] — creation/application split + the mandatory vision-check
- [[Agent-Hooking]] — the doors and the AgentProfile seam
- [[Agent-Interface]] — CLI trunk, MCP façade, guidance tiers
- [[A2A-Door]] — the wire this handout scopes to remote/peer command
- [[Session-Graph]] · [[Conductor-Channel]] · [[Terminal-Commander]]
