# Eidolon trace — the on-box agent, every step readable by Aoide

Eidolon is Aoide's on-box agent. Aoide already enrols every eidolon session
from outside (`conduct/src/graph/eidolon.rs`, the P-EIDOLON lane: presence
`meta.json` + the socket ping) and draws its parent — but it can see nothing
of what the agent *does*: eidolon's journal (`<stem>.eid`) is bitcode, private
to eidolon, and its event bus is in-process. The trace closes that gap with
one file: eidolon mirrors every journal record as one JSON line, and Aoide
tails it exactly the way it tails every other harness's transcript. From
that single stream Aoide derives state, shows the run, and (later) pings the
parent. Nothing else changes hands.

```
eidolon run …
   Session::append(kind) ──▶ <stem>.eid    bitcode · the truth · eidolon-only
                        └──▶ <stem>.jsonl  one JSON line per record · anyone may read
   $XDG_RUNTIME_DIR/eidolon/<id>/meta.json   { …, "trace": "<stem>.jsonl" }

aoide (reap tick, every session)
   graph/eidolon.rs      meta.json.trace ─▶ tail ─▶ state  working · awaiting · idle · stopped
   EIDOLON_PROFILE       transcript spec ─▶ say · tool · model · context tokens · title
   aoide session trace <id> [--tail N] [--follow] [--json]     the run, step by step
   reap: new records on a child ─▶ one line to the parent        (ping-back, second slice)
   state/stage/pingback.json  per-child cursor: seen · silentAt
```

## The contract (read side: Aoide; write side: eidolon)

**File.** `<sessions_dir>/<stem>.jsonl`, beside `<sessions_dir>/<stem>.eid`
with the same stem (`~/.local/share/eidolon/sessions/1789603005561.jsonl`).
Created on the journal's first record and appended for the journal's life;
`resume` appends to the same file. A read-only open (`eidolon sessions`,
`eidolon log`, the TUI's picker) never creates one. The journal is written
first and is authoritative; the trace mirrors it. A trace write that fails is
reported once on stderr and never fails the turn.

**Line.** `serde_json::to_string(&Record)` + `\n` — the record's own serde
form, which `Record`/`RecordKind` already derive, so the shape is not
designed, only exposed: externally tagged enum, snake_case content blocks.
One consequence worth knowing: a `tool_use` block's `input` is the call's
arguments as a JSON *string* (eidolon's own block form), so a reader parses
it a second time to reach a `command` or `path`. `eidolon log --json
<stem>.eid` prints an existing journal through the same serializer, one line
per record.

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
turn is open). No trace file → today's rule (presence `busy`, TUI-only) —
an older eidolon, still enrolled, still `unknown`-folded-to-`idle`.

**meta.json.** One new field, `"trace": "<absolute path>"`. Absent on an
older eidolon; Aoide's `PresenceMeta` treats it as `Option`.

## Producer changes (eidolon, `~/eidolon`)

1. **Trace mirror** at the single journal chokepoint (`Session::append`,
   `crates/core/src/session/mod.rs`), path = the log's path with extension
   `jsonl`; opened for append on the first record, for a fresh session and a
   `resume` alike (a read-only open never creates one).
2. **`meta.json.trace`** — `Meta` gains the field; `Presence::register`
   takes the path.
3. **SIGTERM = SIGINT.** The headless driver (`crates/cli/src/main.rs`
   `drive`) and the repl `select!` over `signal(SignalKind::terminate())`
   and `ctrl_c()` and cancel the same token: `[cancelling]`, a `Cancelled`
   record, a settled turn, exit 0 — a `timeout(1)` kill no longer leaves an
   unsettled journal. The TUI cancels a turn from its own `esc`
   (`Cmd::Cancel`); a SIGTERM aimed at a live TUI process is not handled.
4. **`--deadline <secs>`** on `run`/`resume`: at `deadline − margin` the
   harness delivers, at the same safe point a steer uses, the seconds form
   of the wrap-up frame (`budget_frame`'s text with "N seconds" in place of
   "N model calls"), journaled as `TurnDeadline { secs_left }` — a new
   variant appended LAST with its ordinal pinned, per the journaled-enum
   rule the `ExternalMessage` doc states; at `deadline` it cancels exactly
   as SIGINT does. Margin: `min(120 s, deadline / 4)`. `budget_frame` and
   the deadline frame share one `wrap_up_frame` wording; the nudge is
   delivered at the next safe point after the margin, so a long tool call
   can push it later, and a run that settles before the wall never sees it.

Known gap, out of scope here: harnox's OpenAI-compatible wire (the one
`ollama:*` models ride) does not map `reasoning`/`reasoning_content` into
`Thinking` blocks — only the Anthropic and Codex wires do — so for those
models the trace's thinking is empty until `~/harnox` maps it. The trace
carries whatever the journal carries; that fix is harnox's.

## Consumer changes (Aoide)

- `protocol/src/agents.rs` — `EIDOLON_PROFILE.transcript`: `locate` reads
  `meta.json.trace` and returns the `.jsonl` when it exists (else
  `meta.json`, today's stand-in); once the presence is gone — eidolon
  removes its dir on a clean exit — the caller's hint is the record's own
  `logPath` and the `.jsonl` beside that `.eid` is returned when it exists,
  which is how a run that settled and left between two ticks is still read;
  `tail` is the ordinary line tail for a
  `.jsonl` (the compacting meta read stays for the stand-in); the
  extractors read the table above from trace lines, meta lines as today.
- `conduct/src/graph/eidolon.rs` — `PresenceMeta.trace`, and the state rule
  above beside the presence rule.
- `aoide session trace <id> [--tail N] [--follow] [--json]` — `<id>`
  resolved like `send --to` (id / tail4 / petname); human form is one line
  per record (`#id  hh:mm:ss  kind  summary` — assistant: thinking dimmed
  and cut, text, tool names; tool result: first line, `!` on error;
  settled: stop reason, tokens); `--json` passes the lines through.
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
  ```

  `<N> calls` counts `tool_use` blocks since the turn's opening
  `UserMessage`/`ExternalMessage` when that record is still in the 1 MiB
  tail (the segment is omitted otherwise, as is `<M> min`); `<say>` is the
  last assistant `text` block; a `ToolResult{is_error:true}` among the new
  records rides a priority-1..3 line as ` · <k> tool errors`. Every
  child-authored fragment — quoted or the bare `<tool label>` — is untrusted
  model output: one line, control characters stripped, clipped to 80
  characters with `…`; the quoted ones never able to start with `/` or `!`.

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
  drops out of the file on the same pass. A `seen` that is not in the tail
  treats the whole tail as new ONCE and says nothing about what it lost.
  `sync_eidolon_sessions` returns the records it dropped this tick
  (`Vec<DroppedEidolon>`) as an ADDITIVE second half — the boolean means
  what it always did — and that is the `died mid-turn` row's only evidence;
  no second liveness probe exists (`reap` is the only sweep).
