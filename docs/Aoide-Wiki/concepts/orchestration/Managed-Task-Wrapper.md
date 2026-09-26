# Managed task wrapper — a tracked subagent run with mail

A **managed task run** is an ordinary conducted session with a task name. It is
how a person or an orchestrator launches a subagent that is tracked in the
roster, carries the instructions it was started with, has its own inbox, and
reports its own end — with no new daemon, no new store and no new addressing.

**The foreground shape is primary.** `aoide conduct --task <slug>` blocks,
streams the child's output to the caller's terminal while mirroring every byte
to the PTY transcript, and returns one deterministic outcome. `aoide spawn
--task <slug>` is the *same* wrapper, detached: `spawn` re-execs the identical
`conduct` argv, so every behavior below is shared by construction, and every
fact either shape records lands on the same record.

```sh
# foreground: the tool call. Blocks, streams, returns the outcome.
aoide conduct --task fix-flaky --instructions-path /path/brief.md \
    --timeout 900 --report-to conductor -- claude

# detached: the same wrapper, outliving the call
aoide spawn --task fix-flaky --instructions @brief.md \
    --timeout 900 --report-to conductor -- claude

# the child's own inbox, and the report mailbox, are separate names
aoide mail read --for fix-flaky      # what was sent TO the subagent
aoide mail read --for conductor      # what the run reported

# read-only observer: live by default, never needed for the wrapper to work
aoide session watch <id>
aoide session watch <id> --snapshot
```

## Linux and headless harnesses

The wrapper accepts an executable and its arguments after `--`. A harness with
a headless mode uses that mode's normal arguments; no Aoide harness profile,
trace adapter, or model-specific event stream is required. The same Linux PTY
supervisor owns foreground and detached runs.

```
aoide conduct/spawn --task <name> -- <harness> <headless arguments>
  ├─ tracked process + captured output
  ├─ instructions file + child mailbox
  └─ exit/signal/timeout → retained record + completion report
```

Output is whatever the program emits. Exact prompts, reasoning, and tool events
are optional harness capabilities, not prerequisites for tracking the run.
Process liveness proves that the process exists; it does not prove the model is
ready, connected, or making progress. A quiet headless process remains tracked.
The caller supplies the harness's normal initial prompt and asks it to read
`AOIDE_TASK_INSTRUCTIONS` and its Aoide inbox when those features are wanted;
storing instructions alone does not make an arbitrary program read them.

This process supervisor is verified on Linux. Native Windows process and PTY
support remains a separate portability lane; WSL is not the target.

## What a run keeps

| fact | where it lives |
|---|---|
| the task's name | `task` on the session record — also the child's inbox name, and the session's own `title` when none was set |
| the operator's instructions, written once, mode `0600` | `state/sessions/<id>.instructions.md` (`instructionsPath`) |
| the agent's own output | the conduct-owned PTY transcript, `state/sessions/<id>.log` (`logPath`) |
| the run's discriminated end | `outcome` (`exit` / `signal` / `timeout` / `stopped`) with `endedAt`; `exitCode` only for a real exit |
| the report mailbox it reports to | `reportTo` when `--report-to` was given (else the parent's id, else the role mailbox `conductor`) |
| the report's delivery cursor | `state/stage/taskreport.json` |

The run's identity is always `sessionId`. A task name outlives a respawn, so one
inbox can hold several runs' letters — each letter keeps its sender, and a
letter from another run is labelled as such, never merged into this run's.

## The instructions reach the child

`--instructions <text|@file|->` (or the foreground `--instructions-path`) stores
the operator's bytes **once**, at `state/sessions/<id>.instructions.md`, mode
`0600`, created exclusively: a second write for the same session id is a taught
error naming the stale file, never a silent overwrite. The child reads them by
path — `AOIDE_TASK` names its task, `AOIDE_TASK_INSTRUCTIONS` the sidecar — and
the record carries `instructionsPath` for everyone else. Nothing is appended to
the file: no environment dump, no token, no credential material.

Instructions are **stored, never injected**: a first turn remains `--prompt`'s
job, exactly as before. The wrapper adds no injection of its own, and mail text
is never typed into a run — only an explicit `send` types into a session.

## Two mailboxes, never one

| channel | name | reader | contents |
|---|---|---|---|
| **child inbox** | `self/<slug>` | the run's own session id | the letters sent TO the subagent, by anyone. The run's report is **never** filed here. |
| **report mailbox** | `self/<report-name>` | the parent session (or any reader of that role) | the run's own end-of-run report, one letter |

`<report-name>` is `--report-to <name>` when given, else the run's
`parentSessionId` **when that is a legal mailbox name**, else the documented
role mailbox `conductor`. A canonical id that is not a legal mailbox name
(uppercase, punctuation) is **never rewritten** into one — two identities must
not collide on a name — and a run with no parent at all and no `--report-to` is
reported without a letter: its cursor entry, its record and its audit line
still carry the outcome, and the record says `no-mailbox` rather than pretending
a letter exists. Use `--report-to <role>` when the report must outlive the
parent's session.

## Watching a run

`aoide session watch <id>` is **live by default** and follows until the run ends
(or Ctrl-C). `--snapshot` is the single bounded mode, for a door that must
return (MCP/stdin) — it renders one frame from the same gather, so it can never
drift from the live view. `--tail <n>` sets the output window (default 50);
`--json` returns the frame in the registry's envelope.

`<id>` may also name a session **on another node** — `session watch
<node>/<query>` — resolved by the same rule `send --to` uses against that
node's cached graph (`graph pull`'s document; a node never pulled is a taught
`aoide node pull <node>` refusal, never an implicit fetch). The watch then
reads the run's frame OFF that node over `tasks/get`
(`params.metadata["aoide/frame"]`, CONTRACTS.md §6) — the same frame as the
local one, gathered on the box that owns the run and rendered by the same
renderer, with the node prefixing the header line (`yomi/brave-otter (child) ·
claude · running · …`). The far frame is **untrusted data**: every string goes
through the same sanitizers the local view uses, every count is re-clamped to
the local bounds (the requested tail, the mail rail, the block cap), raw PTY
bytes have no path to this terminal at all, and the suggested follow-up is
rebuilt here as `aoide send --to <node>/<id> --submit -- "<text>"` rather than
taken from a line naming a session in that box's namespace. `logPath`,
`socket` and `instructionsPath` are struck on arrival — they name paths and a
control socket on the box that wrote the frame, so a peer's invented
`/etc/shadow` never reads as this run's log; the instruction TEXT still shows.
A far node's own error message is cleaned before it is printed, here and on
`send`, because an OSC-52 escape or a bidi override in a peer's message is a
terminal instruction rather than text. A refusal by the far door's output gate
(`-32011`: no signed, verified node holding `read` on that host) is a taught
error naming the grant and the box it runs on —
`aoide node allow <this box> read on` **on the far node**.

Remote live mode polls every 2 s and stops when the far record's own
`presence` is no longer `running`, or on Ctrl-C: the frame carries the state
that decides, so there is no cursor to keep and nothing to hold open. A poll
that FAILS also ends the watch — with the failure as the result (an error
carrying the reason, the far door's code when it refused, and
`followed: true`), never a cheerful exit that hides why it stopped, since a
dead tunnel and a finished run must not read the same. (Frame deltas over
`tasks/resubscribe` are deliberately deferred — that stream holds a connection
slot for the run's whole life and carries status changes only.)

A frame shows the instructions the run was started with, the run's own output,
the child's mailbox, and where it stands —

* `running`;
* `stopped — not running` (killed, reaped, or a process that has gone);
* `exited <code> at <endedAt>`.

**An exit is not success.** No subject is parsed for a verdict, no verification
state exists anywhere in this path, and the suggested follow-up line
(`aoide send --id <child> --submit -- "<text>"`) is *printed*, never executed —
it deliberately carries no `--yes`, because the gate is the operator's.

The rail is the **child's own unread queue**, read through
`mail::unread_for(slug, Some(child))` — a reader-selected PEEK. The observer
consumes nothing: not the child's mark, not its own, so the same run may be
watched twice without losing a letter and two observers cannot consume each
other's mail. When the CHILD reads, the queue drains for the right reason and
the live view says so:

```
child read through seq 12 at 09:32:41 (0 still unread)
```

A letter that arrives while watching is rendered by the SAME rule as the rail — its
sanitized body beneath its own header — and the rail attaches to the mailbase on the
first tick that finds one, so a base that did not exist when the watch began still
shows the run's FIRST letter (read-only: no cursor of the child's or the observer's
moves). Initial attachment catches up after the opening snapshot, so a letter
filed between that snapshot and attachment is shown without replaying the
opening rail or letters the child already read.

Attribution is per run: a letter from another run of the same task is labelled
`earlier run (<petname|session id>)`, resolved through the live roster and then
the durable ledger, and a sender that is not a run at all is labelled as such.
Every untrusted fragment — a letter's sender, subject and body, the
instructions — is sanitized (control characters stripped, lines clipped), and
raw PTY bytes are printed only when stdout really is a terminal. Nothing read
here is ever treated as a command.

The footer's report line is read from the **report lane's own cursor**, never
inferred from a letter the child happened to send: `filed — msgid …, run ended
…, exited <code> — or timed out / died by signal / stopped (no status) — …, wake …`, or `not filed yet — the daemon tick files it, and the run's
record is retained until it does`.

Refusals are taught, never an empty view: an unknown id, a `sub:` card (which
shares its executor's process and keeps no PTY of its own), and a record that
keeps no conduct-owned PTY.

**Across a node, the same frame rides `tasks/get`.** A signed, verified node
whose `allows` include `read` reads **any** session's frame on this box over the
A2A door (`params.metadata["aoide/frame"]`, CONTRACTS.md §6) and gets the frame
above as one `data` artifact — the same `render` draws it, so a remote frame
reads like a local one, with `logPath`/`instructionsPath`/`socket`/`suggested`
struck because they name paths and a command on the box that wrote them. The
door bounds it: `tail` to 200 lines, each letter's body to 40 lines, the whole
frame to 256 KiB (that cap sheds the oldest letter first, then the oldest output
line, and says `truncated`), and an instruction block it never sheds. The frame
is read-only on both ends: no cursor moves, and raw PTY bytes exist on no wire.
Unsigned, bearer and address callers are refused (`-32011`), as is a signed node
never granted `read` — and the reader's own end re-clamps what arrives before
anything is printed, because a peer's frame is data like any other. This is the
same read `aoide session watch <node>/<query>` performs; the `suggested` line
above is the one that reader rebuilds for itself.

## The end: a closed vocabulary, and a deadline that is not inactivity

`--timeout <secs>` is the **supervisor's wall clock**, on both shapes of the
wrapper. It is never inactivity: a silent child is alive until the deadline, and
a child printing continuously is still killed at it. The deadline is checked on
every multiplex iteration and bounds the poll wait, so it fires on the clock.

On expiry the wrapper kills the **direct child it spawned** (its own process
handle, never a pid resolved through the roster or an ancestry walk, so no
unrelated session can be the target) and records:

* `exit` + `exitCode: n` — a real status;
* `signal` + `signal: "TERM"` — the agent died by a signal, so **no** `exitCode`
  is claimed for it;
* `timeout` + `timeoutSecs: n` + `killedWith` — the post-kill status, secondary
  evidence about the wrapper's own act, never presented as the child's result;
* `stopped` — reaped or killed outside the wrapper: no status, no code.

A kill is therefore never dressed as an exit code, and an absent `exitCode` is a
fact rather than a missing measurement. The outcome vocabulary is closed, and
the facts come from the wrapper's own process status, its own deadline and its
own record — nothing here reads a harness trace, so a wrapper run's errors,
timeouts and completion do not depend on the ping-back path, which stays what it
is for trace harnesses.

## How the report is delivered

A run's report is filed by the **daemon tick**, not by the dying child: a child
cannot be trusted to survive its own report, and a daemon that was down at the
exit instant reports on its next pass, because the trigger is the *record*
(`task` set, `outcome` set, no cursor entry). The lane files through the
**registered** `mail send` implementation, in-process under the daemon's own
authority — the same name validation and filing the CLI door uses — and appends
one audit line. The letter is plain text (readable by `aoide mail read` long
after the run's record is gone), and its first line names the task and the run:

```
[task fix-flaky] exited 7 (run spawn-1234-5678)
```

One **wake** per event: the lane takes the mail-side doorbell once for the
report mailbox, and no second line is injected anywhere. What the ring actually
did — rung, deferred, or skipped with its reason — is recorded in the cursor and
printed by the view, because "nobody was woken" is only answerable if the reason
is. A Codex desktop/app parent is never conducted, so it is never conductable
and cannot be woken at all: for that parent the durable half — the unread letter
plus the record — is the whole report, and the frame says `wake rang:0
skipped:<id>(not-conductable)` rather than implying otherwise. A report mailbox
named by a SESSION ID is readable but never ringable — the reader key for a name
is the name itself, which the ringer never arms — and the lane records exactly
that: `wake rang:0 no-armed-reader:<mailbox>`. Give a report a REAL reader and a
real latch by addressing a role (`--report-to <name>`), under which the parent
session is enrolled.

**Delivery semantics.** The cursor advances only after the letter is really in
the mailbase, so the ordinary case is one letter per run and a filing failure is
retried on the next tick — losing a report cannot happen. The guarantee is
at-least-once, normally exactly once: a crash between a successful filing and
the cursor write re-files on the next pass, and that retry mints a **new**
msgid, so the mailbase's own duplicate memory cannot collapse it and one extra
identically-worded letter may appear. No claim of exactly-once is made.

## Completed runs stay inspectable

Routine cleanup retains every finished task run's record — filed or not — so a
completed run keeps resolving in `session watch`: its instructions, the child's
unread queue, its output, its outcome and its report. Only the explicit
`aoide session prune` may drop a filed task run; an unfiled one is retained even
there. After any explicit prune the durable history remains on disk: the
session-ledger line, the letters, the PTY transcript and the instruction
sidecar. Deleting a session is the user's act, never a side effect of tidying
the roster.

## Platform

This page describes Linux behavior; **native Windows is not supported today and
nothing here claims it**. The wrapper's own additions are plain portable logic
(the peek, the record keys, the outcome vocabulary, a monotonic deadline, the
report lane), but a runnable wrapper still waits on the substrate rows the
project's capability matrix owns: peer credentials for the daemon door, unix
sockets and the runtime dir for the per-session control socket, the stage/state
lock's open-file-description guarantees, `/proc`-based process liveness, verified
termination for the deadline's kill, and a controlling PTY. The matrix's PTY/ctty
row records that: interactive parity needs ConPTY, headless parity over pipes is
the earliest honest Windows milestone, and a `cfg` branch alone proves nothing.
