# Eidolon headless dispatch — why two runs died silent, and how a harness should ping back

Report on the 2026-09-16/17 orchestration of five `eidolon run --yolo -m
ollama:deepseek-v4.1-flash` executors/reviewers for the monitor-blip fix
(`1cbac46`, `83bbe1a`). Two of the five were killed by the shell `timeout`
wrapper with nothing written; one of those two had found the only real
defect in the change and the finding was recovered from its journal an hour
later. The rest of this page is the root-cause chain, the recovery that
worked, and a design for the outbound "ping back" the user asked about —
including why Verba Volantia (and Jev, its hosted cousin) is the wrong tool
for that particular job.

Sources: `~/eidolon` (the harness), `~/harnox` (its LLM client), the five
`.eid` journals under `~/.local/share/eidolon/sessions/`, and the console
logs under `~/.claude/jobs/a4574219/tmp/screens/`.

---

## 1. The runs

Wrapper for every run: `timeout 3400 eidolon run --yolo -m
ollama:deepseek-v4.1-flash --cwd ~/Aoide "<brief>"` (no `-s`, so SIGTERM).
Model calls and tool results are counted from the `.eid` journals
(`assistant:` / `result [` records); Ollama retries from the console log.

| run | brief | wall | model calls | tool results | ollama retries | exit | journal |
|---|---|---:|---:|---:|---:|---|---|
| A | QML: per-screen bar, focus-following dock | 57 min | 97 | 127 | 11 | **124** | `settled: false` — edits landed, no report |
| B | Nix: `aoide.arrangement.surfaces` + facet publish | 31 min | 90 | 104 | 11 | 0 | settled, report written |
| C | Rust: `surfaces_fall_short` in the watchdog | 19 min | 79 | 86 | 18 | 0 | settled, report written |
| R1 | Review of A+B+C | 57 min | 110 at kill | 176 | 14 | **124** | `settled: false` — nothing written |
| R2 | Same review, re-briefed | 11 min | 65 | 101 | 12 | 0 | settled, PASS — checked what the brief said |
| R1 resumed | `resume` + `send --wake` STOP | 6 min | 2 | 5 | 1 | 0 | settled, report written |

R1's report (`report-review1-recovered.md`) carried the defect R2 missed:
an EMPTY published `surfaces.json` — what every non-declaring song ships —
took the declared branch and disabled the placeholder watchdog on every
non-sonata host. Fixed in `83bbe1a`. R2 missed it because the brief told
it the empty set was healthy; a reviewer checks the invariant it is handed.

## 2. Root-cause chain

Nothing here is one bug. It is a wall-clock wrapper meeting a harness that
only budgets in model calls, whose only kill signal is the one the wrapper
does not send, and whose console output is a preview.

```
timeout 3400 ─SIGTERM─▶ eidolon (no handler) ─▶ dies mid model call
                                              │
                                              ├─ journal: settled:false, no Cancelled record
                                              ├─ no wrap-up frame ever sent (nudge is call-counted)
                                              └─ console log: 100-char previews → "nothing written"
```

### 2.1 SIGTERM is not handled — `crates/cli/src/main.rs:1991`, `:2102`

Only `tokio::signal::ctrl_c()` is awaited (SIGINT → `CancellationToken` →
`[cancelling the turn]`, journal `Cancelled`). `timeout(1)` sends SIGTERM by
default, so the process is simply gone: no cancel, no wrap-up, the pending
tool call never returns, the journal ends `settled: false`.

*Today:* `timeout -s INT 3400 eidolon run …` — verified to cancel cleanly.
*Eidolon:* join `signal(SignalKind::terminate())` with `ctrl_c()`; treat both
as the same cancel.

### 2.2 The wrap-up nudge counts calls, the wrapper counts seconds — `crates/core/src/agent.rs:1337-1374`

`nudge_at = (max_iterations / 2).clamp(1, 8)` → with the default 128
(`crates/cli/src/config.rs:493`) the harness frame "[From the harness — not
the operator: this turn has N model calls left… reply with what you have
done, what remains…]" (`session/mod.rs:1470 budget_frame`) fires once, at
call 120. A died at call 97, R1 at 110. Neither was ever told to wrap up,
because the budget they actually ran out of — seconds — has no frame.

*Eidolon:* `--deadline <secs>`: at `deadline − margin` inject the same
`budget_frame` with seconds instead of calls; at `deadline` cancel exactly
as SIGINT does. The kill becomes a settled turn with a report.

### 2.3 The console is a preview — `crates/cli/src/term.rs:24 preview()`

Every tool result and assistant line is cut to 100 characters on the
console, and `eidolon log` previews too. So a killed run's console log
shows lines like
`⟵ bash === modules/facets/quickshell/qml/shell.qml === Warning: modules/…`
and an operator reading it concludes the run produced nothing. The `.eid`
holds the full text (`382 KB` for A, `652 KB` for R1); R1's 21 KB finding
was already in there when it was pronounced dead.

*Eidolon:* `--full` (or `--no-preview`) on `run` and `log`; `log`/`resume`
accepting a bare session id (today: "No such file or directory").

### 2.4 Ollama-cloud connect timeouts — `~/harnox/src/llm/http.rs:11`, `consume.rs:71-85`

11-18 `client error (Connect): operation timed out — retrying (n/4)` per
run, each a 15 s connect timeout plus 0.5/1/2 s backoff: roughly 3-5 minutes
of dead time per run. A contributor to the wall clock, not the killer —
every retry in every run succeeded within the 4-attempt policy. The
2026-09-15 memory note blamed this alone; it was wrong about the mechanism.

### 2.5 `resume` continues the runaway — observed

`eidolon resume <path> "<prompt>"` finishes the unsettled turn FIRST: it
re-runs the pending tool call and lets the model carry on, and only
delivers the new prompt at the turn boundary. On R1 the model resumed its
probe loop; the prompt never arrived. Rescue came from the swarm door
instead:

```
eidolon send --wake --from orchestrator Aoide-85f7 "STOP: write the report now …"
```

delivered at the next safe point (`agent.rs:1242`, `:1350`, `:1361` — the
same seam as the budget frame), and the model wrote the report in one call.
**Steering a running headless turn works today.** `resume` alone does not.

### 2.6 Noise: a broken user tool

`~/.config/eidolon/tools/search.rn` fails to compile at every launch
("Missing item eidolon::web_search"). Harmless to these runs; fix or delete.

### 2.7 Orchestrator-side faults (mine)

- Watcher self-match: `until [ "$(pgrep -fc 'eidolon run --yolo')" = 0 ]`
  matches its own command line and never fires; B's completion went
  unnoticed for 44 minutes. Pid-file watchers (`kill -0 $(cat run.pid)`)
  replaced it.
- Contradictory briefs: B said an empty declared set "keeps today's
  behaviour", C said it "is never unhealthy". C implemented C; R2 verified
  C. The defect was authored by the brief, not the executor.

## 3. Ping-back: what the harness should tell an outside agent

The ask: let an orchestrator know when a headless harness is stale or
erroring, with what was produced so far — "maybe using VV to generate the
commands to report back".

### 3.1 Every reportable state is already a typed fact

The harness never has to *decide* it is in trouble; it already records it.
Each event below is a `RecordKind` in `crates/core/src/session/mod.rs` or
one step from one:

| event | where the harness already knows | payload |
|---|---|---|
| `started` | `SessionStart` | session path, model, cwd, brief hash |
| `tool_error` | `ToolResult` with non-zero/`Err` | tool, call id, first line of stderr (full, not preview) |
| `retry_exhausted` | harnox `RetryPolicy` gives up | provider, attempts, last error |
| `nudged` | `TurnBudget { calls_left }` | calls left (or seconds, with `--deadline`) |
| `iteration_limit` | `agent.rs:1686` | calls spent |
| `idle` | no record for N s | seconds since last record, last assistant text |
| `deadline` | new: `--deadline` timer | seconds left |
| `cancelled` | `Cancelled` (SIGINT, and SIGTERM once handled) | signal, records so far, files touched |
| `settled` | `TurnSettled` | final assistant text, cost, files touched |

There is no free text to classify and nothing to generate: the input side
is already structured. A model that maps natural language to a typed call
(Verba Volantia) or that returns typed decisions (Jev) would be given a
struct and asked to produce the same struct. That is why VV is the wrong
tool for the *sending* side.

### 3.2 The doors already exist; only the outbound event is missing

```
eidolon run --report-to <target> …
        │
        ├─ swarm:<session-id|channel>   → eidolon send --from eidolon <id> '<event json>'
        │                                  (crates/swarm: maildir + doorbell, no daemon)
        ├─ aoide:<session-id>           → aoide send --id <id> -- '<event json>'
        │                                  (conductor channel; every terminal is a conducted
        │                                   session; this Claude Code session is one)
        └─ file:<path>                  → append JSONL (a Monitor/until-loop reads it)
```

Aoide's session graph already lists eidolon sessions as children of the
terminal that launched them, so `aoide:<parent>` can be the default target
when the run was launched from a conducted session — no flag at all.

The orchestrator then acts with typed commands it already has: `eidolon
send --wake <id> "STOP…"`, `eidolon resume <id> …`, `eidolon log <id>`.

### 3.3 Where VV does fit

On the *receiving* side, and only when a human is in the loop: an event
forwarded as Telegram text ("R1 idle 9 min, 110 calls, last: 'let me try
the quickshell binary offscreen'") could be VV-dispatched by the operator's
one-liner ("stop it and make it write the report") into `eidolon send
--wake`. That is Melete's surface (`verba_dispatch`), not eidolon's. Between
two agents the event is JSON and needs no model.

### 3.4 Minimal change set for `~/eidolon`

1. SIGTERM handled as SIGINT (`main.rs:1991`, `:2102`).
2. `--deadline <secs>`: seconds-based `budget_frame`, then cancel.
3. `--report-to <target>` emitting the table in §3.1; `Cancelled`/`settled`
   carry files touched and the last assistant text in full.
4. `--full` on `run`/`log`; bare ids for `log`/`resume`.
5. Delete or fix `~/.config/eidolon/tools/search.rn`.

Until then, the operator-side protocol is: `timeout -s INT`, pid-file
watchers, print the session path on launch, and `eidolon send --wake
--from orchestrator <id>` to steer — never `resume` alone for a runaway.

## 4. Jev and Verba Volantia

Jev (TypeSafe AI, announced 2026-09-15; CEO Diogo Almeida, ex-ChatGPT) is
pitched as a "System One model": it returns typed structured outputs only —
decisions, classifications, routes, scores — and does not generate text,
code, or reasoning. Trained with what TypeSafe calls RLCD (Reinforcement
Learning for Calibrated Decisions); priced at $0.042 per million input
tokens with output "free"; the launch claim is 20-200× faster and 40-400×
cheaper than small frontier LLMs used for the same decisions. Early access
only; the demo everyone saw was it playing Doom from typed observations.

| | Verba Volantia (`~/eidolon/crates/verba`) | Jev (TypeSafe AI) |
|---|---|---|
| shape | free text → ONE typed tool call, or abstain | typed/text input → typed decision (enum, score, route, schema) |
| generates | nothing — the call is dispatched, no model turn | nothing — "cannot code and doesn't reason" |
| where it runs | local, ms, a trained bundle checkpoint per installation (`model.safetensors` + `meta.json`) | hosted API, early access |
| training | per-bundle, on the installation's own tool manifests | RLCD on a general decision corpus; calibrated confidence |
| abstention | default: unknown text falls through to the model | calibration score; the caller thresholds |
| cost | none after training | $0.042/M input, output free |
| entry points | `eidolon do`, `verba classify\|spec`, inline `! <cmd>`, `vv` tool, Mneme `dispatch` | REST |

Same category — decide, don't generate — and Jev is a good external
validation of VV's thesis that most agent glue is a decision, not a
completion. Differences that matter here: VV is local and trained against
*this* box's tool surface; Jev is a hosted general decider with calibrated
scores and no local run. Neither is a ping-back mechanism; both sit on the
receiving side of a free-text channel, if anywhere.

Sources: gigazine.net/gsc_news/en/20260916-system-one-jev/ ·
artificialintelligence-news.com/news/chatgpt-pioneer-launches-jev-model-for-programmatic-logic/ ·
therundown.ai/news/typesafe-jev-ai-decisions-software ·
anthonymaio.substack.com/p/jev-the-language-model-that-wont ·
theregister.com/ai-and-ml/2026/09/16/typesafe-ai-debuts-model-for-machines-that-plays-doom/ ·
latent.space/p/ainews-jev-a-system-one-model-that ·
runtimewire.com/article/typesafe-jev-system-one-ai-model-early-access

## 5. Open

- A's journal is still `settled: false`; its edits were integrated by hand.
  Resuming it would re-run its pending `qmllint` call — leave it.
- R1's count is 110 at kill, 115 after the resume turn.
- `namespace_coverage` counts disabled outputs while `real_monitor_count`
  excludes them (R1 §INFO): false-happy only, one-monitor blackout case,
  not fixed.
- `--report-to aoide:<parent>` by default: the launching terminal's id is
  already in the child's environment as `AOIDE_SESSION_ID` (verified on this
  box), so the default costs nothing to resolve.
