# Fleshing out Aoide ricing — the rehearsal, the cue, and the observation stack

Status: a design handout captured from a chat session (2026-08-18), not
LOCKED — the User has steered every section at least once, but the final
"lock" word hasn't been said. Part of it is now code: the take tree and its
commands (§5.2, phase A in §10) and the project-revert groundwork (R0–R2,
§7.2) are landed in the repo; everything else is design. Nothing here is
asserted as existing unless marked EXISTS or LANDED. Execution ordering
today: frontend/rice execution is parked behind backend work (the standing
backend-first rule), so the unbuilt phases wait their turn behind it. A
companion command flesh-out was produced by another agent in a separate
session; it was not found anywhere in the repo at capture time, so this
handout carries only this session's design — fold that document in when it
surfaces.

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
> anything local. No fourth protocol gets built: the ricing harness is
> commands and state files riding the existing three doors (CLI / MCP / A2A, one
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
│    register: hook door │ conduct │ spawn │ A2A(remote) │
│    observe:  hook edges + graph tail (transcript ∨ PTY tee)  │
│    steer:    send (gated) │ A2A message/send (remote)  │
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
| claude, kimi (hook systems) | hook door — `aoide session hook` | free |
| codex, gemini, aider, any CLI | `conduct` (PTY) / `spawn` (detached) | free |
| anything scriptable | explicit commands — `session start/phase/end` | free |
| an A2A-speaking peer (remote, usually) | peer door — `peer add` / `peer spawn` / `send --to` | model turns |

All four converge on the same `sessions/hooks/graph.json` records; a
registered peer folds in as a `kind: "peer"` root node beside the hooked
and conducted ones. The wiki sentence, once locked: *the conductor hooks every
local agent mechanically (hooks, PTY, commands — zero tokens) and speaks A2A
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
  pending graph business (an undelivered `send`, an unanswered
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
     │      (a rice command while a rehearsal is running)
     │    · a pending send addressed to it
     │    · it's marked awaiting and something answered
     │        │
     │       yes ──► "♪ cue: rehearsal neon-night @ check — `lyra rice score`"
     │
     └─ anything else — unregistered, idle, no engagement ──► silence
```

"Unregistered" is not business. Registered-ness never appears in the line.

### 3.3 `graph join` — demand-driven enrollment

`aoide graph join [--agent <name>]` = the explicit-commands door with
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
  │ EDIT EDGES           which file, not just which tool       │ NEW
  │   same hook payload, one field newly read:                 │
  │   tool_input.file_path → pre-image capture (§7.2).         │
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
captures. The tee lives in `conduct`, not `spawn` (spawn stays a bare
detached launch; capture-without-steering is one flag later if wanted).
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
lyra rice rehearse begin <song> [--draft <name>]   — enters mode draft
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
lyra rice rehearse end [--distill]   — draft survives; journal → design log
        ▼
lyra rice declare                    — unchanged: User-gated; a stub today
                                       (exit 64), so the loop's last
                                       mechanical step is the draft and the
                                       commit stays the User's by hand
```

### 5.1 `rice score` — the self-describing step

At any moment, any agent, through any door:

```json
{ "rehearsal": { "song": "sonata", "draft": "neon-night", "step": "check",
                 "take": 21, "parent": 9, "from": "A",
                 "marks": { "A": 9, "B": 14 } },
  "instruction": "Vision-check the live desktop: `lyra screen shot`, verify
                  polarity agreement + widget/bar livery match.",
  "allowed": ["rice take", "rice score advance", "screen shot"],
  "gate": { "advance-to": "review", "requires": null } }
```

`from` names the mark only when the head's `parent` carries one — take 21
here hangs off take 9, which is marked `A`, so the reviewer reads "take 21,
from mark A" without cross-referencing the marks map by hand; off a bare
take with no mark on it, `from` is simply absent. Step instructions live in
the score's step table — one place, versioned with the binary, not
per-harness prompts. That is what keeps step-by-step understanding
agent-agnostic: pushed to capable harnesses, identical when pulled by a
bare CLI agent. `rice score advance` moves the step and refuses when a
gate isn't met.

### 5.2 Takes and marks — mechanical revert

**Status: LANDED** — `rice take`, `rice take mark|list|diff|prune`,
`rice back`, the auto-take at both write entrypoints, and the bare-tty
picker are real commands at HEAD (`crates/storage/src/takes.rs`,
`crates/song/src/commands/take.rs`; phase A, §10). The hook-door drift
check during an active rehearsal is phase C, still design.

- **Takes** — automatic, append-only, cheap: every write to the routed
  draft snapshots `songbook/<song>/drafts/<name>/takes/NNNN.json` (livery +
  cover + meta: timestamp, session id, cause). Captured at the two write
  entrypoints (`rice stage`, `cover set`) directly; hand-edits caught by a
  content-hash check on every hook-door `PostToolUse` during an active
  rehearsal AND ambiently on any `lyra rice` command — a hookless agent's
  edits get taken the next time it touches the CLI. Explicit `rice take`
  for paranoia. Leaning toward hashing cover.json alongside livery on every
  check (one extra file read; not finalized).
- **Takes form a tree, not a line.** Each record carries `parent: <NNNN> |
  null`. Numbering is one monotone counter per draft — never renumbered,
  never per-branch; take 21 is take 21 wherever it hangs, so a selector
  (`--take 21`) stays unambiguous forever. Ancestry is derived by walking
  `parent` back to `null`; nothing is indexed.

  ```
  takes/NNNN.json    one snapshot, carries parent: <NNNN>|null
  takes/head.json    per-draft cursor — where the next take hangs
  takes/marks.json   {"A": 9, "B": 14} — one atomic write per (re)stamp
  ```

- **Branching is implicit.** Revert to a mark, write, and the new take
  hangs off the mark's take as its `parent`. There is no branch command, no
  branch name, no branch registry — two takes sharing a parent *is* the
  branch. Takes after the mark are untouched and still selectable; nothing
  on disk is destroyed by reverting past it.
- **No merge, no rebase, no cherry-pick — ever.** Two lines from a mark
  stay two lines; a User who wants them combined edits the livery by hand.
  The deliberate line against the git-clone slope.
- **`rice back` takes before it writes.** If the live content differs from
  the head take (an un-taken hand-edit), `rice back` snapshots it first
  (cause `drift`) before overwriting — the rail that keeps "nothing is
  destroyed" literally true even for work that was never explicitly taken.
- **Takes exist only in Draft mode.** The takes dir lives inside the
  draft; a Staging-mode write (`rice stage`, `cover set` are reachable
  there too) is never taken — rehearsal already requires draft mode (§11),
  this makes the write-entrypoint boundary explicit.
- **Marks** — named rehearsal marks (the music term for lettered jump
  points), unique per **draft** and advancing globally regardless of
  branch, exactly like rehearsal marks in a score: the `mark` step stamps
  the current take with a letter, meaning *reviewed-and-passed*. A letter
  already in use **moves** to the new take rather than erroring —
  re-marking is a normal correction. Marks live in `takes/marks.json`, not
  on the take record — the record stays write-once. `rice take mark
  <letter>` is the phase-A command; the score's `mark` step calls it.

```
lyra rice back                bare, on a tty: the picker (§7), the
                                head's parent pre-selected as row 1 — one
                                Enter is the one-step undo. Non-tty: a
                                usage error naming `--take`/`--mark`,
                                never reads stdin.
lyra rice back --take 9       that take exactly; head := 9
lyra rice back --mark A       resolve A via takes/marks.json; head :=
                                that take
lyra rice take list           the whole tree: numbers, parents, marks,
                                head
lyra rice take diff           change since the nearest mark on the
                                head's ancestry (reviewer's view)
```

Revert is trivial because of draft routing: write take N's livery through
the existing symlink via `atomic_write`. Every file a revert touches
(livery, cover) is already FileView-watched by the Quickshell surfaces, so
**a revert performs no explicit Quickshell IPC reload of its own** — the
existing watch mechanism is the whole apply path, nothing new to wire.
**Scope line:** takes cover the stage-routed files (livery, cover). Widget
QML bodies are committed songbook files — git is their revert mechanism,
and the `edit` step's instruction says so. No parallel VCS.

**The promise, precisely — a revert restores the live stage exactly, not
the draft directory byte-for-byte.** The livery write lands in the draft
via routing; the cover is restored to the stage only, the same seam
`cover set` already writes. `stage/cover.json` is not symlink-routed the
way `stage/livery.json` is, so the draft directory's own `cover.json` is a
`draft save`-time archive copy that no write path maintains and no read
path consumes today — a revert does not touch it and does not claim to.
What the User sees and what Quickshell renders round-trips exactly; the
draft directory's stale copy is a pre-existing asymmetry this work does
not fix (§11 open items).

### 5.3 Watching — correlation, not new plumbing

During an active rehearsal the hook door also appends to
`stage/rehearsal-journal.jsonl`:

```json
{ "take": 17, "sessionId": "…", "event": "PostToolUse",
  "tool": "Edit", "drift": true }
```

The journal answers *which agent action produced take 17* — what makes
`rice back` meaningful rather than blind. Surfaces: `lyra rice watch`
(live tail), the conductor pane and a dock Rehearsal gadget (both consume
the O3 output pane, built once), and `rehearse end --distill` — the
songbook discipline made mechanical: journal + take timeline summarized
into the song's `design/intent.md` iteration log, so self-ricing's
write-back stops depending on agent virtue.

### 5.4 Reviews and planning — gates, not scripts

Orchestration itself stays out of scope: agents already spawn each other
via `conduct` / `send` / A2A with `--parent` edges. The rehearsal
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
loop** and a **nix development loop**. Neither is designed here; the
maintenance agent, paired with but separate from the ricing agent, is open
tracker work (#35). This
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

The nix-development row's revert substrate is listed as "git" with no
mechanism behind it. §7.2 is that mechanism, one level down from a nix
generation: a generation is stamped with its commit (below), a session is
stamped with its pre-images — neither invents a store, and §7.2's journal is
substrate-blind and rice-free, so this loop would inherit it unchanged.

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
build/switch step, and the intended revert command (`aoide nix back
--generation N`, name provisional) resolves generation → recorded rev →
`git checkout` — flake.lock rides along because it lives in the repo. A
dirty tree builds with `self.rev = null` and cannot round-trip, so the
build step's gate refuses (or loudly journals) a dirty build. No new
snapshot machinery: git + generations + one journal line.

## 7. Rollback and pruning — one command, two modes

Every command here serves **both audiences from one implementation**. There
is no human CLI and no agent CLI; there is one command with two entrances
(`rice back`, `rice take diff`, and the picker are LANDED — phase A, §10;
the `aoide nix` lane is design):

```
                       lyra rice back
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

**`rice back`'s two promises, reconciled.** §5.2 promises bare `rice back`
means "previous take, one agent-action undo"; this section promises
bare-on-a-tty means "the picker." Under the take tree these collide at the
same spelling. Resolution: **the picker wins on a tty**, because the
picker's first row *is* the head's parent, pre-selected — the one-step
undo is a single Enter away, and the picker is still there for anyone who
wants to choose something else instead. Off a tty, `rice back` never opens
the picker and never reads stdin: no selector (`--take`/`--mark`) given is
a straight usage error naming both flags, so a piped or agent invocation
can never hang.

| bare command on a tty | picker rows | on select |
|---|---|---|
| `rice back` | the whole take **tree** (§5.2) rendered flat and numbered for selection, head arrowed, marks bracketed — row 1 is the head's parent, pre-selected — plus a **second lane**, widget commits (recent git commits touching the song's widget bodies) | a take writes through the routing (same as `--take N`); a commit does `git checkout <rev> -- songbook/<song>/…` — working-tree restore, HEAD never moves |
| `rice take diff` | takes/marks to diff against | the diff, no write |
| `aoide nix back` | journaled `{generation · rev · subject · age · dirty?}` | `git checkout <rev>` of the config repo; then **offers** the matching generation rollback as a second, separately-confirmed step |

Two teeth in that table:

- The nix **switch-rollback offer exists only behind the interactive
  confirmation — there is deliberately no flag for it.** That is the
  rebuild gate enforced mechanically: a human at the tty is the gate in
  person; an agent scripting flags can check out code but can never reach
  the switch.
- The widget-commit lane is the "help with rolling back through git"
  command: takes cover the stage-routed files (§5.2), git covers widget
  bodies, and the one picker shows both lanes so the User never has to
  remember which substrate owns which file.

The discipline generalizes as a CLI-wide convention: any command whose flags
select one-of-many (a song, a draft, a take, a session id) grows the same
bare-on-a-tty picker, applied opportunistically as commands get touched — not
as a big retrofit pass.

### 7.1 Pruning — nix entirely, never a second store

**Decision: just nix.** Aoide invents no store, no generation format, no
snapshot layer. Pruning commands are wrappers whose whole contribution is
*selection, correlation, and rails* — nix already does the deleting, and
`session prune` (EXISTS — drops finished session records) already sets the
house meaning of the word.

| command | wraps | protected by default |
|---|---|---|
| `rice take prune [--older-than 14d] [--keep 20] [--all-but-marks]` (LANDED, A9) | aoide's own `takes/` dir — its only native storage | **marked** takes (reviewed-good states); `--force` to take one out |
| `aoide nix prune [--generations 42,43] [--older-than 30d] [--keep 5]` | `nix-env -p /nix/var/nix/profiles/system --delete-generations …`, then optionally `nix store gc` | current + booted generation (nix refuses these anyway — a free rail); any generation the journal flags `keep` |
| `aoide session prune` | EXISTS — done session records | — |

Both prune commands are dry-run-shaped: they print what would go and what it
frees, and the bare-tty mode is a **multi-select** picker (space toggles,
Enter confirms) because pruning is the one place selecting several at once
is the normal case. The symmetry worth keeping: **a mark protects a take
and every take on that take's path back to the root, exactly as `keep`
protects a generation** — one idea, two substrates.

**Protection extends to ancestry.** Under the take tree (§5.2), pruning a
take whose descendants survive **splices** rather than orphans: the
surviving children are re-parented to the pruned take's parent, so
ancestry degrades to "further back," never to broken — `rice take diff`'s
"nearest mark on the ancestry" still resolves once the take in between is
gone. A mark's protection is transitive for the same reason: it is not
enough to protect the marked take alone if a take between it and the root
can still be pruned out from under it. The head and everything on the
head's ancestry are never eligible for pruning, `--force` included —
pruning the ground the draft is currently standing on has no reading.

Consequence of pruning worth stating once: deleting a generation does not
delete its commit. The §6 round-trip degrades gracefully — a pruned
generation still resolves to its rev, so recovery becomes *checkout and
rebuild* instead of *boot straight into it*. Related grounded finding:
`nix.gc.automatic` is **not** configured anywhere in this repo today, so
nothing is silently eating generations behind the journal's back — if it
is ever turned on, its retention and the journal's `keep` flags need to
agree.

### 7.2 Project edits and per-session revert — git as a second substrate

**Decision: git holds the bytes, aoide holds pointers.** §7.1 is "just nix,
never a second store" for the built system; this is the same discipline one
level down, for a *registered project's own working tree*. On every edit-tool
hook, aoide writes the file's pre-edit content into the **project's own git
object store** (`git hash-object -w`), anchors it with a ref so gc can never
collect it, and appends one line to a single append-only journal,
`state/edits.jsonl`. Reverting a session means, for each file it touched,
restoring the earliest pre-image it recorded — gated by a hash check against
what is live now. This is the mechanism §6's nix-development row lists as
"git" with nothing behind it, and it is the same seam §7's widget-commit lane
already promised: a working-tree restore, HEAD never moved, never a commit.

```
PreToolUse, tool ∈ profile.edit_tools, file_path under a registered project
        │
        ▼
   git hash-object -w -- <file>       pre-image → the project's own
   git update-ref refs/aoide/preimage/<sha> <sha>   object store, anchored —
        │                                            gc cannot take it
        ▼
   append {kind:"pre", project, session, path, mode,
           tuid, sha, tool, at}  →  state/edits.jsonl

PostToolUse, same tools
        │
        ▼
   git hash-object -- <file>          fingerprint only — never stored,
        │                             never restored, compared at revert
        ▼
   append {kind:"post", …, sha, at}  →  state/edits.jsonl
```

**Verified on this box:** `git update-ref` accepts a ref that points directly
at a blob (no tree, no commit), and that blob survives `git gc --prune=now`
— probed 2026-08-18. The anchoring rail this section depends on is not a
hope; it is a fact checked against this machine's git before the design was
finalized. The groundwork is LANDED code: the edit journal
(`crates/storage/src/edits.rs`, R1) and the five-operation git seam
(`crates/storage/src/git.rs`, R2, with `git` in the package's
`nativeCheckInputs`). The capture edges and the commands themselves are not
built (R3–R7 — phasing, below).

Only **five** git operations exist anywhere in this design: `rev-parse
--show-toplevel` (is this a repo, and where), `hash-object -w` (store a
pre-image), `hash-object` (fingerprint the live file), `cat-file blob` (read
a pre-image back at revert time), `update-ref` under `refs/aoide/` (anchor).
`git add`, any commit, any HEAD move — structurally absent. A revert is a
working-tree restore, full stop; the User's branch, index, stash, and history
are never touched. A hook running under a minimal-PATH launcher (systemd and
friends) may not find `git` on `PATH` at all — capture then silently records
nothing, consistent with the best-effort posture every capture already has.

**The journal.** One file for every registered project, `state/edits.jsonl`
— not one journal per project, so no project-name-to-path derivation and no
new sanitization surface. Each line is a pointer, never content: a *pre* line
and its matching *post* line share `project`, `session`, `path`, `mode`,
`tool`, `tuid`, `at`, plus each line's own `sha` — `kind` says which one it
is, and `sha` is the single on-disk field name in both cases, never split
into two. `tuid` (the tool-use id from the hook payload) exists so a session
that edits one file twice in flight pairs its own pre and post correctly
rather than by proximity. `sha: null` on a pre-image means the file did not
exist before the edit — **that null is the answer to "which file was made
through aoide."** Appends are lock-free by construction:
one line is far under `PIPE_BUF`, so concurrent hook processes interleave
atomically without a lock — the same reasoning `with_stage_lock`'s own doc
comment already carries for why it is *not* used on a pure append.

**The commands (design — none built yet):**

```
aoide project edits [--project <name>] [--session <id>]
                          [--path <rel>] [--since <Nd|Nh>] [--json]

aoide project back --session <id> [--project <name>] [--force] [--json]
aoide project back                    bare on a tty → the picker
```

`edits` is a pure fold over the journal — no git, no repo required, works on
an unregistered-as-git project exactly as well as a tracked one. `+` marks a
file whose earliest pre-image is `null` — made through aoide, the ask's own
phrasing. `back` is the revert; it needs the project's git toplevel and
refuses cleanly, naming `git init`, when there isn't one — **aoide never runs
`git init` for the User.** Both commands are dual-entrance exactly per §7:
flags and `--json` are the agent door; bare on a tty is the User's door and
opens a picker (rows = sessions, newest first) — that picker rides the
take-tree picker's `aoide_protocol::pick` groundwork (§10's A8, LANDED), and
still ships after the flag door (R5 before R6).

**Per-session revert is exact only for files nothing has touched since.**
That is the honest limit, not a bug: two sessions editing one file is a
three-way merge, and a merge can fail — no storage design changes that. What
*is* decidable and cheap is a hash check. `back --session S` first plans,
then classifies every file S touched into three states, not two — a crash
mid-revert must not strand the User in a state the tool then refuses to
finish:

| state | test | action |
|---|---|---|
| `clean` | live hash == S's recorded post-image | restore S's pre-image (or delete, if `pre == null`) |
| `already-restored` | live hash == the pre-image being restored (file correctly absent, for a creation) | skip — counts as success, not a conflict |
| `moved-on` | anything else, including a file `rm`'d out from under the check | refuse — **no files are written** until every file plans clean or already-restored |

The three-state split is what makes a repeated or crash-interrupted revert
**idempotent**: re-running `back --session S` after a partial revert finishes
the remainder and only then appends the one `revert` line, instead of seeing
its own half-finished work as a new conflict and refusing forever.

A `moved-on` file refuses with one of two messages, because the cause splits
cleanly into two cases and the wrong one is actively misleading:

- **Another aoide session moved on** — the journal names it: *"`path`
  moved on (session `s_9f`, 11:02) — revert `s_9f` first, or pass
  `--force`."* Reverting in **LIFO order composes correctly**: undo the newer
  session and S's files become clean.
- **Nothing in the journal moved it** — a shell write, a hand edit, or a
  `rice back` touched the file, and there is no session to name: *"`path`
  changed outside aoide's view (shell write, hand edit, or a rice revert) —
  `--force` overrides after capturing the current content."* This is where
  the Bash blind spot (below) becomes visible at exactly the moment it
  matters, instead of staying an abstract caveat.

**`--force` overrides, and destroys nothing.** It proceeds past `moved-on`
files, but only after capturing the file's *current* content as a fresh
anchored pre-image first — the discarded bytes stay in the repo's object
store, named in the journal's `revert` line, recoverable with `git cat-file
blob <sha>` for as long as R7's pruning leaves the ref alone. A clean revert
takes the same precaution: even with no conflict, the file's current content
is captured before it is overwritten, so **a revert can itself be reverted.**

**Limitations, stated as limitations, not buried:**

- **The Bash blind spot is permanent.** A `Bash` heredoc, `sed -i`, `mv`, or
  `rm` is invisible to every capture point this design has — the hook door
  only ever sees `tool_input.file_path` for the harness's own edit tools.
  Wrapping every `Bash` call in a `git status` diff was considered and
  rejected: it would journal build-output churn (`target/`) as if an agent
  had "edited" it, and still could not attribute *which* Bash call did it.
  The honest-refusal rail above is the whole mitigation: a Bash-modified file
  fails the hash check at revert time and surfaces as the second refusal
  shape, "changed outside aoide's view" — detected, never silently
  overwritten, but never captured either.
- **A non-git project registers and works — up to a point.** `project
  add` still only validates an absolute, existing directory; whether it is a
  git repo is reported (`git: false`), never fixed for the User. `edits`
  (provenance) needs no repo and works identically either way. `back`
  refuses with `not-a-git-repo`, naming `git init`, and never runs it —
  aoide does not create a repository inside a directory the User named
  without being asked to. (`project add|list|remove` are the LANDED
  registration commands; `edits` and `back` are not built.)
- **`NotebookEdit` carries `notebook_path`, not `file_path`.** Capture reads
  either key, so a notebook edit is captured like any other rather than
  silently producing nothing under a plausible-looking `edit_tools` entry.
- **A symlinked file round-trips by content, not by link.** Capture hashes
  what the symlink points at (git follows symlinks); restore writes through
  the symlink at its target (the same transparency `atomic_write_bytes`
  already has) — content comes back correctly, the link itself is never
  touched either way.

**Composition with rice takes: disjoint substrates, on purpose, with no
exclusion list.** Takes (§5.2) cover the stage-routed draft files
(`stage/livery.json`, `stage/cover.json`, and their draft-mode copies); this
journal covers tracked files inside a registered project's working tree. In
this repo both exist at once and mostly don't overlap — except that a
songbook widget's QML body, which §5.2's scope line already assigns to git
as its revert mechanism, is now covered by this journal the moment a session
edits it through an edit tool: **not a collision, the §7 widget-commit seam
finally arriving by another door.** Where the substrates do genuinely
overlap — an edit-tool write to a routed draft file — both systems record,
redundantly and harmlessly: each revert path trusts only its own hash
comparison, never the other system's state. A `rice back` that rewrites a
livery file reads, to this journal, as an ordinary out-of-view change (the
second refusal shape, above); a `project back` that rewrites it reads, to
the takes system, as drift, and gets a drift take on its next touch. No
coordination command exists between `rice back` and `project back`, and none is
needed — the two rails already compose without one.

**Not designed here, and explicitly not shipping in v0** (the User's call):
a whole-project revert, `project back --commit <rev>` — a working-tree `git
checkout <rev> -- .` with HEAD never moving, answering "revert projects back
to previous states" in its coarse reading rather than the per-session one.
It is a handful of lines once the git seam above exists, and it is exactly
the picker's second lane §7 already promises for the take tree. It is left
for later, same register as §6's nix loops: an intended lane, not a designed
one, because git already does this by hand and the feature aoide uniquely
supplies is the per-session lane above it.

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

The sudo door implements no authentication and holds no secret of its own
(the `aoide secrets` broker is a separate, existing door with its own
storage — this one adds nothing beside it). Factors are
configured in nix on the PAM stack behind the polkit action —
`pam_oath` for TOTP, `pam_u2f` for a FIDO2 key, password alone if that is
what the User wants. The only thing an op declares is an **auth class**
(`admin`, `admin-2fa`), which selects which polkit action id it goes
through; strengthening a class is then a nix edit that needs no aoide
change. This is the same "just nix entirely" answer as §7.1 — the system
already has an authentication stack, and a second one written for this door
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
lines — what aoide is in two sentences, `aoide cue` as the one command to
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

- **Opt-in, never self-installing.** `skill install` is a User command (or an
  orchestrator's), the same boundary hooks install already respects. An
  installed skill's description line is loaded by the *harness's* own
  relevance trigger, not pushed by aoide — which is why this does not
  violate the no-SessionStart-injection rule: the agent's own mechanism
  decides, and the User chose to put it there.
- **One skill, not a family.** A skill per domain would mean several
  always-loaded description lines in every agent's context — the
  front-loading banned in §0. One `aoide` skill, one command, done. (A
  second would only earn its place if a domain's trigger words don't
  overlap "aoide" at all — "rice", "theme", "colors". One line noted;
  not built.)
- **Harness-agnostic, degrading to nothing.** A harness with no skill
  mechanism gets no `SkillSpec` and loses nothing — tiers 0–3 already
  cover it. A new harness that has one is, as ever, a single profile
  entry.

## 10. Phasing

Each phase is reviewed before the next lands, house style. Status at HEAD:
**A (all ten steps) and R0–R2 are LANDED**; every other phase is unbuilt,
and execution is parked behind backend work (the standing backend-first
rule). The open R steps carry tracker rows: R3 is #6 (gated), R4 is #4, R6
is #5. Observation (O0–O3) still lands before the rehearsal machinery (B–F)
— the rehearsal's watch UI consumes it:

- **O0 — the cue**: `aoide cue`, the business-triggered stderr line,
  `graph join`. Pure reuse of existing matching. No injection work.
- **O1 — `graph tail`**: transcript-backed via the profile seam.
- **O2 — PTY tee** in `conduct`: ring-capped `state/output/<id>.log`,
  tail falls back to it, ANSI-strip on read.
- **O3 — conductor/dock output pane** rendering the same resolver.
- **A — takes + `rice back`** (LANDED, all ten steps): the safety floor,
  serialized as ten steps A0–A9 — one cargo-running executor at a time,
  each step reviewed before the next:
  - **A0** — this handout amended to the take-tree model (this section
    included). Prose only; lands before any code so every later step
    reads the amended design.
  - **A1** — the take store (`aoide-storage`): `TakeRecord` gains
    `parent`; the flat monotone counter (ordered by parsed number, never
    filename); `takes/head.json` (falls back to the highest existing take
    when stale or missing); `takes/marks.json`; ancestry/children/reparent
    as pure functions.
  - **A2** — `rice take`: the snapshot core, exposed as unlocked cores
    plus thin `with_stage_lock`-wrapped entrypoints (one lock per
    mutator, never nested), and the explicit command.
  - **A3** — auto-take at the write entrypoints (`rice stage`,
    `cover set`) — a snapshot failure is non-fatal, reported in `data`,
    never turns a successful live write into an error.
  - **A4** — `rice take mark <letter>`: one atomic write of
    `takes/marks.json`; re-marking moves the letter and says so.
  - **A5** — `rice back` (agent entrance) — the step that lands the ask:
    drift-snapshot-first inside one lock, revert-through-routing, no
    Quickshell IPC reload (every file it writes is already
    FileView-watched), a refusal when the routing symlink itself is
    missing or dangling.
  - **A6** — `rice take list`: the tree renderer, whole tree by default,
    `--json` as a flat array carrying parent pointers.
  - **A7** — `rice take diff`: key-wise JSON diff against the nearest
    mark on the head's ancestry.
  - **A8** — the picker (§7): `aoide_protocol::pick`, the bare-`rice
    back`-on-a-tty branch with the head's parent pre-selected; non-tty
    keeps A5's usage refusal verbatim.
  - **A9** — `rice take prune` (§7.1): the ancestry splice, marks and the
    head's ancestry protected by default.

  Widget-commit lane of the §7 picker is scoped out of A0–A9 — it is
  orthogonal to branching and lands as its own later step. §7.2's R2
  (LANDED, `crates/storage/src/git.rs`) carries the tree's first `git`
  shell-out, for a different reason (project revert); the widget-commit
  lane builds on that same seam when it lands.
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

- **P — pruning** (§7.1): `rice take prune` LANDED with `A`; `aoide nix
  prune` with the nix loop. Multi-select picker ships with each.
- **S — the sudo door** (§8): the escalation record, the polkit-backed
  helper, the summons on existing surfaces, `Door::Sudo` audit. Its first
  customer is `aoide nix prune` (deleting system generations needs root),
  which is why S lands with the nix work rather than after it.

**R — project edits and per-session revert** (§7.2): its own serialized
run, R0–R7, one cargo-running executor at a time same as A0–A9, each step
reviewed before the next. R0–R2 are LANDED; R3–R7 are open (tracker: R3 #6,
R4 #4, R6 #5):

- **R0** (LANDED) — this handout amended with §7.2 (this section). Prose
  only, landed before any code.
- **R1** (LANDED) — the edit journal (`aoide-storage`): `EditLine` (with
  `tuid`), append/read, the pure folds a revert plan and the provenance
  query both consume.
- **R2** (LANDED) — the git seam: the five subprocess operations,
  doc-commented never-stages/never-commits/never-moves-HEAD;
  `pkgs/aoide/default.nix` carries `git` in `nativeCheckInputs` so the
  gc-survival test runs under `nix flake check`, not only in a dev shell.
- **R3** (pending, tracker #6) — the capture edges, wired into the hook
  door's existing payload parse without touching `map_hook`. **Gated: does
  not start until a one-turn live capture (`aoide hooks install claude
  --capture`) confirms the shape of a claude `PostToolUse` payload on this
  box** — the `pre` arm is already grounded in existing code, the `post`
  arm is not, and R3 is not written against an assumption.
- **R4** (pending, tracker #4) — `aoide project edits`, the
  provenance query, both renders.
- **R5** — `aoide project back --session`, flags/`--json` entrance —
  the step that lands the ask.
- **R6** (pending, tracker #5) — the bare-on-a-tty picker lane. Its A8
  prerequisite (`aoide_protocol::pick`) is landed; nothing else in R
  depends on it, so R7 may land first.
- **R7** — `project edits prune`: retention for the journal and the
  `refs/aoide/preimage/*` refs no surviving line names.

## 11. Decisions made (unmake at will) and open items

Decided in-session, one line each:

- Rehearsal **requires** draft mode — takes need a home; routing gives
  revert free; plain staging stays informal.
- No new protocol — commands on the existing three doors; ACP dead, AG-UI
  wrong layer; A2A = remote command + status only.
- Widget bodies revert via git, not takes.
- Review gate = different-session-id, not different-agent.
- The cue is pull-only: no SessionStart injection; the ambient line fires
  on business, never on status; `graph join` is suggested by nothing except
  a score step that mechanically requires identity.
- PTY tee in `conduct` only; output logs under `state/`.
- Terse index always, full contract on demand (§0) — applies to the CLI
  help, the MCP tool list, and any context an orchestrator hands a worker.
- One command, two entrances (§7) — agents and the User share every command;
  bare-on-a-tty gets a hand-rolled picker (multi-select where selecting
  several is normal), flags/`--json` bypass it, non-tty never prompts.
- Pruning is nix entirely (§7.1) — wrap `delete-generations` / `store gc`,
  never a second store; marks protect takes as `keep` protects
  generations.
- Privilege goes through the sudo door (§8), never through an agent
  holding a password: declared op ids, nix-admitted allowlist, fresh
  polkit auth per request, single-use default, no free-argv op ever.
  Factors are PAM's (`pam_oath` / `pam_u2f`), selected by an op's auth
  class — the door implements no authentication and stores no secret of
  its own.
- The ban is on an **unattended** switch, not an agent-initiated one
  (§8.2) — an agent may ask; only a live human authentication completes
  it.
- Tier S: aoide ships as one installable agent skill (§9.1), generated
  from the registry, **pointer-only** (never step instructions or a
  command list), installed by a User command into a path the `SkillSpec`
  names. One skill, not a family.
- No generic engagement framework — the rehearsal is the only **built**
  tenant; nix maintenance and nix development are intended siblings (§6),
  and the shared shape gets extracted only when the second tenant lands.
  `stage/rehearsal.json` keeps an imitable, substrate-blind shape as the
  one line of future-proofing.
- Takes are a **tree**, not a line (§5.2): one `parent` pointer per take,
  numbering flat and monotone, never renumbered; ancestry is derived by
  walking `parent`, never indexed.
- Branching is **implicit** at a revert — no branch noun. A branch is a
  derived fact (two takes sharing a parent), not an object; the
  User-facing language is "take 21, from mark A." (`volta` is the reserved
  noun if the User ever wants one — nothing in storage changes either
  way.)
- **No merge, no rebase, no cherry-pick — permanently.** Two lines from a
  mark stay two lines; combining them is editing the livery by hand. The
  explicit line against the git-clone slope.
- Marks live in `takes/marks.json` (a `{"A": take}` map), not a field on
  the take record — one atomic write per (re)stamp, and the take record
  stays write-once.
- The head cursor is per-draft `takes/head.json` — survives draft switches
  and mode round-trips; falls back to the highest existing take when
  stale or absent.
- Bare `rice back` on a tty opens the picker, head's parent pre-selected
  as row 1 — the one-step undo §5.2 promises is a single Enter. Non-tty
  stays flags-only and never reads stdin.
- A revert is **not itself a take** — the pre-revert drift snapshot
  already preserves everything a revert could destroy; recording a
  "revert take" would double every round trip with content that already
  exists at the target number. Phase C journals the revert as an event,
  not a take.
- A revert restores the **live stage** exactly (livery through routing,
  cover to the stage seam `cover set` already writes) — it does not
  restore the draft directory's own `cover.json`, which nothing maintains
  or reads today. A revert triggers no explicit Quickshell IPC reload:
  every file it writes is already FileView-watched.
- `rice take list` shows the whole tree by default; **`--all` is cut** —
  there is no second mode left for it to select.
- `rice take diff` is a key-wise JSON diff (added/removed/changed paths),
  not a text differ over pretty-printed JSON — a reordered key must never
  read as a change.
- Pruning splices: a pruned take's surviving children re-parent to its
  parent; a mark protects its take and every take on the path back to the
  root; the head and its ancestry are never prunable, `--force` included.
- The picker lives in `aoide_protocol::pick` — door behavior, and every
  domain crate already reaches `Door`.
- Project revert (§7.2): git holds the bytes, aoide holds pointers —
  subprocess `git` is the eleventh external tool the tree shells to, not a
  new architectural category, and no `git2`/`gitoxide` dependency is taken.
- One `state/edits.jsonl` for every project, not one journal per project —
  deletes the project-name-to-path derivation and its unvalidated-name
  hazard outright.
- No new noun for "one session's edits inside a project" — the journal
  calls them `pre`/`post` lines, prose says *pre-image*. `part` stays the
  reserved musical candidate if the User ever wants one, matching the
  take-tree's own no-noun-until-asked discipline.
- No `project new` — registration is the only project-level
  provenance the ask describes; a scaffolding command is a different feature.
- `git merge-file` (a three-way merge on conflict) is rejected as the
  conflict policy — it would land literal conflict markers in the User's
  source files from what is supposed to be a *safety* command. All-or-nothing
  refusal with the LIFO hint stands instead.
- `--force` on `project back` is **kept** — the pre-capture makes it
  non-destructive by construction (discarded bytes stay anchored and named
  in the `revert` line), and without it two sessions interleaved on one
  file would strand the User with no in-tool exit.
- The whole-project `project back --commit <rev>` lane is **deferred by the
  User** — not shipping in v0, intended-later exactly like §6's nix loops;
  the per-session lane is the feature aoide uniquely supplies, and git
  already does the coarse revert by hand.

Open / uncertain:

- The other agent's command flesh-out document — not found in the repo at
  capture time; fold in when it surfaces.
- Kimi transcript flush cadence vs `graph tail --follow` liveness.
- Whether auto-take hashes cover.json on every check (leaning yes).
- If real agents ignore the cue line, auto-join for known harness
  ancestries is the noted one-line escalation — not built.
- Whether `stage/cover.json` should eventually get symlink routing like
  `livery.json` — a pre-existing asymmetry (§5.2), not blocking; takes
  work correctly either way, and nothing here depends on the answer.
- The design as a whole awaits the User's explicit LOCK. Landed pieces
  (the take tree, the edit journal, the git seam) are asserted by the CLI
  reference pages on the strength of the code, not of a lock; wiki concept
  pages assert nothing from the unbuilt remainder.
- Whether a claude `PostToolUse` payload carries `tool_input.file_path` is
  still unconfirmed on this box (§7.2) — the `PreToolUse` arm is grounded in
  existing code, kimi's `PostToolUse` is grounded by a live capture already
  on disk, claude's `PostToolUse` is not. R3 is gated on a one-turn live
  capture settling this before it starts.

## Related

- [[Self-Ricing]] — the existing loop, drafts, and mode machinery this rides
- [[Ricing-Protocol]] — creation/application split + the mandatory vision-check
- [[Agent-Hooking]] — the doors and the AgentProfile seam
- [[Agent-Interface]] — CLI trunk, MCP façade, guidance tiers
- [[A2A-Door]] — the wire this handout scopes to remote/peer command
- [[Session-Graph]] · [[Conductor-Channel]] · [[Terminal-Commander]]
