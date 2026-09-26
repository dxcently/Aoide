# Eidolon trace — the on-box agent, every step readable by Aoide

Eidolon is Aoide's on-box agent. Aoide already enrols every eidolon session
from outside (`conduct/src/graph/eidolon.rs`, the P-EIDOLON lane: presence
`meta.json` + the socket ping) and draws its parent — but it can see nothing
of what the agent *does*: eidolon's journal (`<stem>.eid`) is bitcode, private
to eidolon, and its event bus is in-process. The trace closes that gap with
one stream: one JSON line per journal record, which Aoide tails exactly the way
it tails every other harness's transcript. From that single stream Aoide
derives state, shows the run, and pings the parent. Nothing else changes hands,
and Aoide writes nothing — the records come from the producer, one way or the
other.

```
eidolon run …
   Session::append(kind) ──▶ <stem>.eid    bitcode · the truth · eidolon-only
                        ├──▶ <stem>.jsonl  one JSON line per record · anyone may read
                        │                  (the INSTALLED generation's mirror)
                        └──▶ eidolon log --json <stem>.eid [--after <id>]
                                           (the CURRENT generation's read-only export)
   $XDG_RUNTIME_DIR/eidolon/<id>/meta.json   { …, "log": "<stem>.eid" }

aoide (reap tick, every session)
   EIDOLON_PROFILE       transcript spec ─▶ locate ─▶ mirror · journal · meta.json
   graph/eidolon.rs      trace ─▶ state  working · awaiting · idle · stopped
   reap                  transcript ─▶ say · tool · model · context tokens · title
   aoide session trace <id> [--tail N] [--clip …] [--json]     the run, step by step
   shellbridge sessiontrace ─▶ that command ─▶ one line to a card   (the card query)
   reap: new records on a child ─▶ one line to the parent        (ping-back, second slice)
   state/stage/pingback.json  per-child cursor: seen · silentAt
```

## The contract (read side: Aoide; write side: eidolon)

Two producer generations publish the same trace, and Aoide serves both without
ever confusing them. What tells them apart is evidence, in this order: a mirror
that exists and is still current wins; else the journal itself, and only when
the producer's own `log` proves it is the read-only one; else the presence
file, which is always readable.

**File (installed generation).** `<sessions_dir>/<stem>.jsonl`, beside `<sessions_dir>/<stem>.eid`
with the same stem (`~/.local/share/eidolon/sessions/1789603005561.jsonl`).
Created on the journal's first record and appended for the journal's life;
`resume` appends to the same file. A read-only open (`eidolon sessions`,
`eidolon log`, the TUI's picker) never creates one. The journal is written
first and is authoritative; the trace mirrors it. A trace write that fails is
reported once on stderr and never fails the turn. A mirror is read **only while
it is current**: a journal that exists and is strictly newer than the mirror
beside it is positive evidence that the mirror stopped tracking the run, and a
frozen mirror must never pin a finished turn as live state.

**Door (current generation).** No mirror is written. The journal is exported on
demand, by the producer, through the one door it publishes for exactly this:

```
eidolon log --json <stem>.eid [--after <id>]
```

Read-only by construction — the open is `Session::open_readonly` (upstream
`0432133`) — so a torn tail reads as absent, never as an error and never as a
truncation. `--after <id>` emits only records with an id greater than that one;
ids are the dense `#n` the text rendering prints, and an id at or past the head
reads as "nothing new". The output is one record per line, in journal order,
and a serialize error aborts mid-print with a non-zero exit — so a reader
discards partial output and never infers from it. **Before `0432133` `log`
opened the journal read-write and repaired a torn tail in place**, which is why
Aoide proves which generation it is talking to before running anything:
`eidolon log --help` listing `--after` is checked before each export; no
capability answer is cached. An unavailable or negative answer prevents that
export. Probe and export are separate invocations: replacing the runtime
between them can still race, so this is not an atomic runtime identity check.
It is the ONE place Aoide execs another
harness, and it is why the door, not a second file Aoide would have to write,
is what carries the current generation's records.

**Line.** `serde_json::to_string(&Record)` + `\n` — the record's own serde
form, which `Record`/`RecordKind` already derive, so the shape is not
designed, only exposed: externally tagged enum, snake_case content blocks.
One consequence worth knowing: a `tool_use` block's `input` is the call's
arguments as a JSON *string* (eidolon's own block form), so a reader parses
it a second time to reach a `command` or `path`.

```json
{"id":0,"parent":null,"ts_ms":1789603005561,"kind":{"SessionStart":{"model":"ollama:deepseek-v4.1-flash","cwd":"/home/khoa/Aoide","system":null}}}
{"id":1,"parent":0,"ts_ms":1789603005570,"kind":{"UserMessage":{"role":"user","content":[{"type":"text","text":"# Brief A: …"}]}}}
{"id":2,"parent":1,"ts_ms":1789603009102,"kind":{"AssistantMessage":{"role":"assistant","content":[{"type":"thinking","thinking":"…","signature":"…"},{"type":"text","text":"Let me read the slot catalog first."},{"type":"tool_use","id":"call_8vr43zri","name":"read","input":"{\"path\":\"modules/facets/quickshell/qml/slots.md\"}"}]}}}
{"id":3,"parent":2,"ts_ms":1789603009140,"kind":{"ToolResult":{"tool_use_id":"call_8vr43zri","content":"     1\t# Per-song widget slots — catalog\n…","is_error":false}}}
{"id":120,"parent":119,"ts_ms":1789606380000,"kind":{"TurnBudget":{"calls_left":8}}}
{"id":131,"parent":130,"ts_ms":1789606421000,"kind":{"TurnSettled":{"stop_reason":"end_turn","usage":{"input_tokens":9570000,"output_tokens":71900,"cache_creation_input_tokens":0,"cache_read_input_tokens":9430000}}}}
{"id":77,"parent":76,"ts_ms":1789626990000,"kind":"Cancelled"}
{"id":40,"parent":39,"ts_ms":1789626500000,"kind":{"AskUser":{"call_id":"call_x","prompt":"Overwrite?","answer":null}}}
{"id":41,"parent":40,"ts_ms":1789626501000,"kind":{"ContextSize":{"tokens":134700}}}
{"id":55,"parent":54,"ts_ms":1789626700000,"kind":{"ExternalMessage":{"from":"orchestrator","channel":null,"text":"STOP: write the report now"}}}
```

`id`/`parent` are the journal's own record ids — `parent` is the branch,
so a `fork_at`/`Excluded` shows up as records whose `parent` is not the
previous line. `ts_ms` is eidolon's clock at append. Every `RecordKind`
variant appears; the ones Aoide reads:

| record | Aoide reads it as |
|---|---|
| `SessionStart{model,cwd}` / `ModelChanged{model}` | `model` |
| `UserMessage` (first) | `title` (the first thing asked) |
| `AssistantMessage.content[type=text]` (last) | `say` — what it last said |
| `AssistantMessage.content[type=thinking]` | shown by `session trace`, never summarised into a field |
| `AssistantMessage.content[type=tool_use]` (last) | `tool` — the call in flight, until its `ToolResult` lands |
| `ToolResult{is_error}` | a tool failed (ping-back) |
| `AskUser{answer:null}` | state `awaiting` |
| `TurnSettled` | state `idle`; stop reason + usage |
| `Cancelled` | state `stopped` |
| `TurnBudget{calls_left}` / `TurnDeadline{secs_left}` | the harness told it to wrap up (ping-back) |
| `ContextSize{tokens}` | `context tokens` |
| `PolicyVerdict{outcome}` | shown by `session trace` |
| `ExternalMessage{from,text}` | what a steer said, and who |

**State rule** (the reconciler's, in `graph/eidolon.rs`): the LAST record
decides, and only Aoide's five canonical states are ever written
(`protocol/src/state.rs` `canonical_state`: working · awaiting · stopped ·
idle · done). `TurnSettled` → `idle` (at rest, as `busy: false` reads
today); `Cancelled` → `stopped` (the vocabulary's "turn ended by a stop");
`AskUser` with `answer: null` → `awaiting`; anything else → `working` (a
turn is open). No trace at all → the presence rule (presence `busy`,
TUI-only) — an older eidolon, still enrolled, still
`unknown`-folded-to-`idle`.

**meta.json.** Every generation writes `log` — the session's own journal path.
The installed one adds `"trace": "<absolute path>"`, its mirror; the current
one adds nothing, because there is nothing to add: the journal the presence
already names is what the door exports. Aoide reads this file through the
profile's own locator (`EIDOLON_PROFILE.transcript.locate`), one 4 KiB parse
that yields both paths, so a presence is never parsed twice for the same fact
and a missing `trace` field is ordinary rather than an error.

## Producer changes (eidolon, `~/eidolon`)

1. **Trace mirror** at the single journal chokepoint (`Session::append`,
   `crates/core/src/session/mod.rs`), path = the log's path with extension
   `jsonl`; opened for append on the first record, for a fresh session and a
   `resume` alike (a read-only open never creates one). This is the INSTALLED
   generation's shape, and Aoide still reads it wherever it is still current.
2. **`meta.json.trace`** — `Meta` gains the field; `Presence::register`
   takes the path. Installed generation only; the current `Meta` no longer
   carries it.
3. **Read-only export door** — `eidolon log --json <journal> [--after <id>]`
   over the journal's own records, opened with `Session::open_readonly`, so the
   same command is safe against a live session (the repairing open would
   truncate a mid-append record under the writer). `--after` is a poller's
   cursor: the ids the text rendering prints, dense and monotonic from zero.
   This replaces the mirror rather than adding to it: one published interface
   for a consumer that cannot decode the journal, and no second file to keep in
   step.
4. **SIGTERM = SIGINT.** The headless driver (`crates/cli/src/main.rs`
   `drive`) and the repl `select!` over `signal(SignalKind::terminate())`
   and `ctrl_c()` and cancel the same token: `[cancelling]`, a `Cancelled`
   record, a settled turn, exit 0 — a `timeout(1)` kill no longer leaves an
   unsettled journal. The TUI cancels a turn from its own `esc`
   (`Cmd::Cancel`); a SIGTERM aimed at a live TUI process is not handled.

**Not carried forward.** `--deadline <secs>` and the
`TurnDeadline { secs_left }` record were dropped by the producer's own review
verdict: on the re-landed
SIGTERM parity, the wall-clock budget belongs to whoever launched the run —
`timeout -s INT` on the operator's side — and the harness budgets model calls
(`TurnBudget { calls_left }`) as it always did. `TurnDeadline` survives only as
a LEGACY record kind a journal may still carry; nothing writes it, and a reader
that meets one shows it rather than acting on it.

Known gap, out of scope here: harnox's OpenAI-compatible wire (the one
`ollama:*` models ride) does not map `reasoning`/`reasoning_content` into
`Thinking` blocks — only the Anthropic and Codex wires do — so for those
models the trace's thinking is empty until `~/harnox` maps it. The trace
carries whatever the journal carries; that fix is harnox's.

## Consumer changes (Aoide)

- `protocol/src/agents.rs` — `EIDOLON_PROFILE.transcript`: `locate` answers the
  generation that is installed, in the order the contract above sets — a mirror
  that is current, else the journal itself when `eidolon log --help` proves the
  door read-only (checked again before each export; pre-`0432133` producers
  lack this capability), else `meta.json`, the stand-in it always returned; once
  the presence is gone — eidolon removes its dir on a clean exit — the caller's
  hint is the record's own `logPath` and the `.jsonl` beside that `.eid` is
  returned while it is current, else the `.eid` itself when the door is there,
  else nothing. `tail` is the ordinary line tail for a `.jsonl`, the door's own
  export for a `.eid` (the same window, the same one-MiB bound), and the
  compacting meta read for the stand-in; the extractors read the table above
  from trace lines and meta lines alike. The pull is bounded four ways — the
  child's five-second wall clock, one retained window per journal, the aggregate the memo
  may hold, and the number of reader threads that may be outstanding at once —
  and it degrades rather than growing any of them; a degraded or refused export
  is discarded whole, never inferred from, and named once per process in the
  audit log. The producer replays the journal even with `--after`; an export
  exceeding five seconds is discarded. No journal-size threshold is established.
- `conduct/src/graph/eidolon.rs` — the state rule above beside the presence
  rule, with the trace reached through the harness CAPABILITY
  `TranscriptSpec::locate` + `TranscriptSpec::trace`, the same locator and
  reader every other consumer uses (so a producer that publishes no mirror still
  feeds the state fold), and the presence rule kept exactly as it was whenever
  no trace is reachable.
- `aoide session trace <id> [--tail N] [--follow] [--json] [--clip line|detail]`
  — `<id>` resolved like `send --to` (id / tail4 / petname); human form is one
  line per record (`#id  hh:mm:ss  kind  summary` — assistant: thinking dimmed
  and cut, text, tool names; tool result: first line, `!` on error;
  settled: stop reason, tokens), and the summary is RENDERED from the same
  projector `--json` publishes, so the two cannot drift. `--json` passes the
  lines through byte for byte in `data.lines`, beside `data.steps`: the same
  window PROJECTED, one object per emitted content block, in the record's own
  order — `{ id, ts, kind, text, error, clipped }` with `kind` in `thinking ·
  say · tool · result · settled · user · other`, `id`/`ts` the record's own
  (`ts` epoch ms, null when the record carried none), `clipped` true when the
  text is a kept prefix. It is bounded by construction, never by the journal:
  the `--tail` window, 32 blocks of any one record (a record carrying more gets
  one `other` step naming the omission: `+N more block(s) in this record, never
  projected`), 200 steps for the whole projection with the newest kept
  (`stepsOmitted` names how many that dropped), and `clip` characters per step.
  `--clip line|detail` (default `line`, the one-line spelling the human body
  and `--follow` use) decides how much of each block `steps` keeps: `detail`
  keeps the block's own line breaks (up to 12) and a few hundred characters, and
  adds a `tool_use`'s arguments — the details view's window. An unknown clip
  word, like a non-numeric `--tail`, is a taught usage error, never a silent
  widening. A trace line that is not a readable record contributes no step (it
  keeps its `?` human line and its raw line in `data.lines`).
- The read-only trace QUERY over the bridge — one answered shellbridge verb,
  `{"cmd":"sessiontrace","sessionId":…,"lines":N,"clip":"line|detail"}`, which
  re-execs exactly the command above (`--tail N --clip … --json`) and answers
  one JSON line: `{ok, sessionId, clip, lines, trace, at, steps, stepsOmitted}`
  or `{ok:false, sessionId, clip, lines, reason, message}` carrying the CLI's
  own taught refusal verbatim (`reason` ∈ `no-trace · no-presence ·
  unknown-agent · remote-target · not-found · ambiguous`, plus the door's own
  `no-answer`/`bad-request`). It resolves, reads and prints — it writes nothing.
  Bounded by construction whatever the caller's cadence: `lines` default 12 and
  clamped to 40 from above (a non-number or 0 refused), an unknown `clip`
  refused, the re-exec'd child killed and reaped at a 10 s wall clock (longer
  than the producer pull's own 5 s, so the bound never fires on a slow export),
  its pipes read by slot-counted reader threads with a 2 s EOF grace — a
  DESCENDANT holding a pipe open makes the read DISCARDED, never answered from
  partially. `at` is when the answer was read, and the caller's cadence makes it
  current while a card is looked at: a snapshot, never a token stream.
- Second slice, **ping-back** (`conduct/src/graph/pingback.rs`; the User's
  ruling, 2026-09-17: a parent automatically hears the children it spawned —
  nothing wider, so a stranger's send still holds pending): the reap tick
  remembers the last trace id it saw per eidolon child and, on
  `TurnSettled`, `Cancelled`, a dropped record whose turn is still open
  (`died mid-turn`), `AskUser{answer:null}`, `TurnBudget`/`TurnDeadline`,
  three or more `ToolResult{is_error:true}` in a row, or an open turn with no
  new record for ten minutes (latched, one line per silence), delivers ONE
  line to the parent. The lines, in priority order:

  ```
  [eidolon <petname>] settled <stop_reason> · <N> calls · <M> min · last: "<say>"
  [eidolon <petname>] cancelled · <N> calls · last: "<say>"
  [eidolon <petname>] died mid-turn · <N> calls · last: "<say>"
  [eidolon <petname>] asking: "<prompt>"
  [eidolon <petname>] wrapping up · <n> calls left      (or · <s> s left)
  [eidolon <petname>] failing · <k> tool errors in a row · last: <tool label>
  [eidolon <petname>] silent <M> min · last: <tool label or say>
  [eidolon <petname>] exited               (or · exit <code>, or · <outcome>)
  ```

  `<N> calls` counts `tool_use` blocks since the turn's opening
  `UserMessage`/`ExternalMessage` when that record is still in the 1 MiB
  tail (the segment is omitted otherwise, as is `<M> min`); `<say>` is the
  last assistant `text` block; a `ToolResult{is_error:true}` among the new
  records rides a priority-1..3 line as ` · <k> tool errors`. Every
  child-authored fragment — quoted or the bare `<tool label>` — is untrusted
  model output: one line, every unsafe character stripped (control, and the
  Unicode `Cf` marks — bidi overrides, zero-width joiners — that would
  otherwise reorder or hide the line; one `is_unsafe` set, shared with the
  door's own `clean_line`), clipped to 80 characters with `…`; the quoted ones
  never able to start with `/` or `!`.

  **It is the daemon's own line, not a send.** The send door attests the
  sender from the running process's `/proc` ancestry, so inside the daemon
  the attested sender is the daemon and never the child — which is why the
  reciprocal rule is not `sender_is_parent` made symmetric. The line takes
  the doorbell's path instead: a live Claude Code channel socket if one is
  bound (one write, close, no keystroke), else `write_delivery` + the
  wrap's own submit key for a headless wrap — no gate, no `pending.json`
  entry, no provenance prefix, no title rename, one audit line with gate
  label `autogate-child`. It runs in `reap()`'s post-lock collector block
  after `sync_eidolon_sessions()`, only under `Door::Daemon`, and its result
  never joins `outcome.changed`. A parent that is a bare shell is skipped —
  a line typed into a shell runs — as is one whose record is gone, not
  conductable, or already `done`.

  **At-most-once by a claimed cursor.** `state/stage/pingback.json`
  (`{ "<child id>": { "seen": "<id>", "silentAt": "<id>" } }`) is read,
  decided over and rewritten (atomically) inside one short `.stage.lock`
  section BEFORE the socket write; a crash between the two loses a line,
  which is the safe direction, and a child whose eidolon record is gone
  drops out of the file on the same pass. `seen` is the last trace record id
  examined — the same id the door's own `--after` takes, and NOT the door's
  cursor: that one is process memory inside the protocol reader, an output
  bound that is dropped whenever the journal behind it cannot be identified.
  A `seen` that is not in the tail
  treats the whole tail as new ONCE and says nothing about what it lost.
  `sync_eidolon_sessions` returns the records it dropped this tick
  (`Vec<DroppedEidolon>`) as an ADDITIVE second half — the boolean means
  what it always did — and that is the `died mid-turn` row's only evidence;
  no second liveness probe exists (`reap` is the only sweep).

  **The decision and the line are two things, and a remote parent gets the
  decision.** `choose_event` decides WHICH event a child's new records amount
  to — a closed `PingEvent`, one variant per row of the table above plus
  `exited` below — and `render_line(tag, &event)` renders the line from it,
  taking the tag from whoever renders. Every string in an event is already
  cleaned at the sender (unsafe characters stripped, clipped to 80 with `…`,
  a payload number left a number), because the renderer may be another node's
  code. Local lines are the same bytes they always were.

  **A child whose parent is elsewhere spools instead of delivering.**
  `remoteParent` on the record (the A2A door's stamp, whatever the child's
  harness) sends each event to `state/stage/pingback-remote.json`
  (CONTRACTS.md §4) instead of to `deliver`: at most 16 events per child,
  `seq` monotonic from 1, the oldest dropped past the cap, and a read that
  lost any event says `gap` with `last` to resync from. The entry is stamped
  with the child's own `remoteParent.key` — the key the far parent's door
  gates its read on, and the reason a ring can still be read once the record
  is gone — and with the tick's `at`, which is the ring's whole retention
  clock. The far parent pulls them off this node's own door (`tasks/get` with
  `aoide/linesAfter`, CONTRACTS.md §6) — nothing here pushes, mails or opens
  a route. The claim is written first and the event lands after, the same
  at-most-once direction the cursor itself holds. A ring whose child's record
  has left the roster is kept for a week and then dropped by this same pass
  (`RING_GRACE_SECS`): a ring is rewritten whole on every spool, so carrying
  the dead forever would be paid for by every live event.

  **`exited` is the one row that comes from the record, and the one that
  closes a ring.** A record whose state folds to `done` gives a remote child's
  parent an `exited` event carrying its `outcome`/`exitCode` — no trace needed,
  so a non-eidolon harness gets it too — and it is claimed ONCE per child by
  the cursor's own `exited` latch (a child that keeps no trace has no `seen` to
  advance past its end). It is last among the trace rows and takes the silence
  row's place, so the last word before a child's exit is the child's own. Only
  a remote child ever publishes one: a local parent hears the run's report
  (Q5's ruling). A child the sync DROPPED is never decided again, so a dropped
  remote child claims its trace row and this exit on that one pass.

  **The parent pulls, and only the parent's own node delivers.** The other end
  is `pingback_pull`, one lane over in the same post-lock block: for every row
  in `state/stage/remote-children.json` (CONTRACTS.md §4) whose parent is
  still a live, conductable session on THIS node, it asks that child's node
  `tasks/get` with `aoide/linesAfter` = the row's cursor and renders what comes
  back HERE. The tunnel key is the PARENT's own session id, so the ssh forward
  is the one the parent already owns and closes with the session; the daemon
  holds no forward of its own. The events are a peer's bytes: each is
  re-validated against the closed event set (a kind this version does not know
  is dropped, never guessed at), every string re-cleaned (unsafe characters
  stripped, clipped to 80) and rendered by the SAME `render_line` the local
  lane uses — under a tag naming the box and the child, `[<node>/<child id>]`,
  built from the row because the far node supplies no tag at all. Then
  `deliver()`, as always, with the audit label `autogate-child remote`.

  **The claim is atomic with the read.** The cursor moves through
  `remote_children::claim_lines_after` — one `with_stage_lock` section that
  reads the stored value and advances it, returning the cursor this pass may
  deliver from — and only events past that cursor are delivered, so two
  overlapping passes (the daemon's own loop and a `session reap` re-entering
  through a connection thread) cannot each hand over the same line. A claim
  that cannot be written delivers nothing: a lost line is the safe direction,
  a duplicated one is not. `last` is trusted only under a `gap`, where it is
  the resync the ring documents — without one, an honest ring cannot report a
  `last` beyond the events it sent, and believing a peer's number there would
  drive the cursor past every `seq` the child can ever push. A `gap` costs the
  parent exactly one extra line, built here from that same arithmetic (`· <n>
  events lost before this point`) and never a fabricated event.

  **The pass is bounded and rotated.** `PULL_BUDGET` (six seconds) caps the
  whole loop — the rows are pulled one after another inside the daemon's tick,
  and a node that is merely off burns its request's full timeout — and rows
  the budget cuts off are named (`budget-spent`) so the next pass, which
  starts one row further along, covers them. Once an `exited` has been drained
  the row is latched (`drained`), and so is a row whose child the far node no
  longer holds at all (`-32001`: no record and no ring, the child is gone for
  good). A refusal or a transport failure is retryable instead: no line, one
  audit record, and the next tick asks again from the same cursor — one pull
  per child per tick, with no backoff. A target that can never receive a line
  — no record, a bare shell, not conductable, already `done` — is judged
  before the far node is asked anything, so it neither spends a request nor
  consumes events it would not have shown.
