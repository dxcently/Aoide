---
type: entity
created: 2026-07-27
updated: 2026-08-20
tags: [aoide, agent, session, conductor, orchestration, graph]
---

# Agent Hooking — putting ANY agent on the conductor

*The stage files `song/stage/{sessions,hooks,graph}.json` are the single truth; the `aoide conductor` TUI, the [[Gadget-Dock]]'s Conductor/Terminals gadgets, and the orchestration daemons all render from them. Anything that writes these files through the doors below becomes a full citizen of the graph.*

## The three doors

Pick by what the agent harness can do. All three converge on the same records; all are idempotent upserts.

### 1. The hook door — harnesses with a hook system (claude, kimi)

Pipe ONE hook payload as JSON on stdin, naming the harness when it isn't the default:

```sh
echo '{"session_id":"…","hook_event_name":"…","cwd":"…","message":"…"}' | aoide graph session hook --agent claude
```

`--agent` defaults to `claude`; an unknown name is a structured `unknown-agent` error listing the registered profiles. The door resolves the harness's **agent profile** (the seam below) and maps the payload through it — one contract, per-harness tables behind it.

Each profile's `hook_event_map` collapses its harness's event NAMES onto shared semantic classes, which the door then maps to canonical state — claude's map below; kimi's deltas are in the *Kimi Code* recipe further down (the door is a silent no-op for everything unmapped, and NEVER exits non-zero — safe inside any hook config). The five canonical states are `working`/`awaiting`/`stopped`/`idle`/`done` ([[Widget-Bridge-Contract]]); a legacy `running`/`waiting`/`blocked` write from an older door is folded onto this set by a read-side shim (`canonical_state`) the first time anything touches the file, so no migration step is needed:

| event | state |
|---|---|
| `SessionStart` | registers the session, `idle`. A resume folds a `stopped` record back to `idle` (resumed, not yet active) but never resets a live `working`/`awaiting` turn |
| `UserPromptSubmit` | `working` (and, set-once, NAMES the session from the prompt) |
| `PreToolUse` / `PostToolUse` | `working`, setting the owner's `activity` to the tool |
| `Stop` | `stopped` (turn over, human's move — alive at the prompt, and recently so) |
| `Notification`, `permission_prompt` / message contains "permission" | `awaiting` (mid-turn, agent NEEDS a human) |
| `Notification`, `idle_prompt` / message contains "waiting for your input" | `awaiting` only if currently `working` (an unseen mid-turn question); no-op otherwise |
| `SessionEnd` | `done` |

`stopped` and `done` are different things and never collapse: `Stop` ends a TURN,
`SessionEnd` ends the SESSION. The one transition no event delivers is `stopped` →
`idle` — a session simply left alone emits nothing — so the reaper's ~12 s pass ages
it, an hour after the `Stop` that set it. The stop instant is the session's rolling
`hooks.json` `updatedAt`; the decay writes `sessions.json` and `hooks.json` together
so the merge cannot resurrect the warm state.

Hooks give you edges and there is no "un-awaiting" event, so every path OUT of a permission prompt must land on a mapped event. Approve → the tool runs → `PostToolUse` clears it back to `working`. Deny with feedback → the model continues → next `PreToolUse`/`Stop`. Reply or interrupt → `UserPromptSubmit`/`Stop`. The set is closed; an `awaiting` flag cannot stick.

The hook door also **discovers the session's window**: at `SessionStart` (and
backfilled on any later hook while still empty) it walks pid-ancestry against
`hyprctl clients -j`, the same match `conduct` uses — so a plain hook-registered
session (no wrapping, no PTY) still gets a `windowAddress` and is
focus-jumpable from the roster, same as a conducted one.

### 2. The explicit verbs — anything scriptable

```sh
aoide graph session start --id "$ID" --agent gemini --cwd "$PWD" [--parent "$PARENT_ID"]
aoide graph session phase --id "$ID" --phase awaiting
aoide graph session end   --id "$ID"
```

Canonical state vocabulary and how the desktop renders it:

| state | glyph | treatment |
|---|---|---|
| `working` | ♪ | in a turn / running a tool; normal row in the widgets |
| `awaiting` | 𝄐 | needs the human (permission prompt or the idle-input ping); the bar's `✎N` cell flips paletteHot→glitchPink and pulses while ANY session is `awaiting` |
| `stopped` | 𝄁 | the TURN ended and the agent sits at its prompt, within the last hour — alive and warm; a section barline, not the final one |
| `idle` | 𝄽 | at rest and cold — stopped for more than an hour, or freshly created / resumed and not yet active |
| `done` | 𝄂 | final barline, dimmed; `graph prune` sweeps them |

### 3. The wrapper — hookless agents (codex, gemini, aider, anything)

```sh
aoide graph wrap [--agent codex] [--parent "$PARENT_ID"] -- codex --whatever-flags
```

- Spawns with **inherited stdio** — a wrapped TUI runs undisturbed.
- Registers `idle` on spawn, resolves `done` on exit — crash included; nothing haunts the roster.
- Exit mirrors the child (0 ok / 1 otherwise; real code in `data.exitCode`).
- Exports **`AOIDE_SESSION_ID`** into the child, so anything hookable *inside* the wrapped agent can self-report richer phases:

```sh
aoide graph session phase --id "$AOIDE_SESSION_ID" --phase blocked
```

Everything after `--` passes to the child verbatim (the CLI stops flag-parsing there).

### 4. Conducting — the steerable variant of the wrapper

```sh
aoide conduct [--agent codex] [--parent "$PARENT_ID"] -- codex --whatever-flags
```

Same spawn/register/wait/end lifecycle as `graph wrap`, but on a controlling
tty plus a per-session control socket, so an orchestrator can steer the child
afterward:

```sh
aoide graph send --id "$AOIDE_SESSION_ID" [--submit] [--yes] -- some text to type
```

`graph send` is the one gated injection door — held pending approval by
default, `--yes` (or an autogate policy) delivers it, and every outcome is
audited. Use `graph wrap` for pure observe-only registration; use `conduct`
when something (a human via the `conductor` TUI, or another agent) needs to type
into the session later.

**Headless — no terminal required.** `aoide conduct --headless` runs the
identical steerable session with no controlling terminal at all: the pty's
output mirrors to an append-only `state/sessions/<sessionId>.log` instead of a
real screen, and `aoide graph spawn` is the detached verb that launches one
and returns immediately (full mechanism in [[Conductor-Channel]]). Both are
harness-agnostic: the hook door above doesn't care whether its own stdin is a
real tty, so a headless-launched agent hooks itself onto the graph exactly
like a foreground one.

**Live-proven 2026-08-20, the three-harness probe.** A headless `claude`
answered a prompt into its log with its own hook-registered session nested as
a child of the wrapper session. A headless `kimi` did the same — its harness
session likewise hooked in as a child, proving the hook door is genuinely
harness-agnostic rather than claude-shaped-with-kimi-bolted-on. A headless
`pi` launches and hooks in the same way too, but its provider requests time
out (3 retries, 3 failures) — a gap in pi's provider-network path, not in the
launch or hook-registration mechanism; the pi profile and its wiring
(`hooks install pi`, below) are otherwise unaffected. Separately, a real
sibling send between two of these sessions delivered with the
`autogate-sibling` gate label and its provenance prefix visible in the
receiver's log — see [[Conductor-Channel]] for the gate ladder and the
attribution-not-security stance behind that prefix.

## The agent-profile seam — every harness fact behind one table

Everything the bridge knows about a specific agent harness lives on an
`AgentProfile` (`pkgs/aoide/crates/protocol/src/agents.rs`), looked up by name
(`agent_profile(name)`; `known_agents()` lists the registered names — today
`claude` and `kimi`). One profile carries, whole:

- `hook_event_map` — event name → semantic class (`HookClass`); it also
  classifies a Notification's detail via prefixed keys (`ntype:<raw
  notification_type>` exact, `msg:<lowercased message>` substring fallback), so
  event, type, and message can never cross-classify.
- `permission_vocab` — message substrings that force `awaiting` unconditionally
  (claude: `["permission"]`; kimi: empty — it has a dedicated event instead).
- `subagent_tools` — tool names that dispatch a sub-agent (claude: `Task` +
  `Agent`; kimi: `Agent` only, synchronous).
- `normalize_payload` — copies a raw hook payload's harness-native fields onto
  the canonical names the door reads, before `map_hook` runs (identity for
  claude; kimi maps `prompt` content-block arrays → `user_prompt`,
  `tool_call_id` → `tool_use_id`, `agent_name` → `agent_type`).
- `model_ceiling` — the context-window ceiling for a model id.
- `transcript` (`TranscriptSpec`) — locate / tail / say / title / model /
  context_tokens / subagents_dir / find_subagent for the harness's on-disk
  transcript layout.
- `hook_settings` (`SettingsSpec`) — where the harness's hook config lives and
  its format (claude: `~/.claude/settings.json`, JSON; kimi:
  `${KIMI_CODE_HOME:-~/.kimi-code}/config.toml`, TOML).

Every agent-aware consumer dispatches through the profile rather than
hardcoding a harness: the hook door (`--agent` → `map_hook` and the
`SessionStart` window-discovery path in `crates/conduct/src/graph/send.rs`),
the transcript refresh (`crates/conduct/src/graph/session_store.rs`), the
window listener's sub-agent tool check (`crates/conduct/src/graph/window.rs`),
the reaper's transcript probe and same-window dedup (`profile_for` in
`crates/conduct/src/reap.rs` — a record whose agent has no registered profile falls
back to the claude layout), and `graph session start`'s default agent
(`crates/storage/src/session.rs`). A new harness lands as one more entry in the
profile table, not a scatter of conditionals.

## Per-agent recipes

### Claude Code

`.claude/settings.json` — nine events (`SessionStart`, `UserPromptSubmit`,
`PreToolUse`, `PostToolUse`, `Notification`, `SubagentStart`, `SubagentStop`,
`Stop`, `SessionEnd`), one identical command each. The payload arrives on
stdin; the door does the mapping:

```json
{
  "hooks": {
    "SessionStart":     [ { "hooks": [ { "type": "command", "command": "a=$(command -v aoide) || exit 0; \"$a\" graph session hook >/dev/null 2>&1; exit 0" } ] } ],
    "UserPromptSubmit": [ { "hooks": [ { "type": "command", "command": "a=$(command -v aoide) || exit 0; \"$a\" graph session hook >/dev/null 2>&1; exit 0" } ] } ],
    "PreToolUse":       [ { "hooks": [ { "type": "command", "command": "a=$(command -v aoide) || exit 0; \"$a\" graph session hook >/dev/null 2>&1; exit 0" } ] } ],
    "PostToolUse":      [ { "hooks": [ { "type": "command", "command": "a=$(command -v aoide) || exit 0; \"$a\" graph session hook >/dev/null 2>&1; exit 0" } ] } ],
    "Notification":     [ { "hooks": [ { "type": "command", "command": "a=$(command -v aoide) || exit 0; \"$a\" graph session hook >/dev/null 2>&1; exit 0" } ] } ],
    "SubagentStart":    [ { "hooks": [ { "type": "command", "command": "a=$(command -v aoide) || exit 0; \"$a\" graph session hook >/dev/null 2>&1; exit 0" } ] } ],
    "SubagentStop":     [ { "hooks": [ { "type": "command", "command": "a=$(command -v aoide) || exit 0; \"$a\" graph session hook >/dev/null 2>&1; exit 0" } ] } ],
    "Stop":             [ { "hooks": [ { "type": "command", "command": "a=$(command -v aoide) || exit 0; \"$a\" graph session hook >/dev/null 2>&1; exit 0" } ] } ],
    "SessionEnd":       [ { "hooks": [ { "type": "command", "command": "a=$(command -v aoide) || exit 0; \"$a\" graph session hook >/dev/null 2>&1; exit 0" } ] } ]
  }
}
```

Writing it by hand is not required: `aoide hooks install claude` merges the same nine entries into `~/.claude/settings.json` (idempotent JSON merge, the rest of the document preserved — see [[Agent-Interface]]).

`Notification` + `PostToolUse` are what make **blocked** visible — without them a session on a permission prompt reads `running` forever. `SubagentStart` / `SubagentStop` are what put a spawned sub-agent (e.g. an `Agent`-tool call) on the graph as its own row, parented to the session that spawned it, rather than folding invisibly into the parent's activity.

**Scope: global vs. project-local.** `.claude/settings.json` can live at project scope (`<repo>/.claude/settings.json`, tracked per-project, `command -v`-resolves `aoide` but can also hardcode a dev build's path) or at user scope (`~/.claude/settings.json`, applies to every Claude Code session on the machine regardless of cwd). **Only the global file covers sessions started outside a project that carries its own `.claude/settings.json`** — a Claude Code session opened in, say, `~/dxflake` never touches this repo's project-local hooks, so it registers on the graph only if `~/.claude/settings.json` also carries the same nine hooks. Without them, that session is invisible to `aoide graph session hook` entirely: Hyprland's window listener still picks up the enclosing terminal as a bare `shell`-kind row (window address, pid, cwd), but the Claude process itself never becomes an `agent`-kind row with turn state, phase, or a spawned-by edge. As of 2026-07-30, `~/.claude/settings.json` carries the same nine hooks as this repo's project-local file (pointed at whatever `aoide` resolves to on `PATH`, no worktree-specific path baked in) — this is a fresh-machine onboarding step for anyone setting up Claude Code as an Aoide harness: the global file needs the hooks too, not just the repo's.

The settings-file split is specific to harnesses that come through the hook door — claude and kimi each keep their own (profile-declared) settings file, and `aoide hooks install <agent>` writes either one. Harnesses wired through this repo's [[aoide-cli|other doors]] (the wrapper, the explicit verbs) don't have a settings-file split like this one.

### Kimi Code

Kimi speaks the same stdin-JSON hook transport with the same core event names, so it comes through the same door: `--agent kimi`. The profile absorbs the differences (all ground-truthed against captured 0.31.1 payloads):

- **`PermissionRequest` is THE needs-input signal** (→ `awaiting`, unconditional). Kimi's `Notification` carries background-task status, never a permission prompt — there is no notification_type/message vocabulary to classify, and `permission_vocab` is empty.
- **Payload shape.** `UserPromptSubmit.prompt` is an ARRAY of content blocks (its text blocks join into `user_prompt`); tool events carry `tool_call_id`; `SubagentStart`/`SubagentStop` name the child `agent_name`. `normalize_payload` copies these onto the canonical fields before mapping, leaving the kimi-native fields intact.
- **Kimi-only events are ok no-ops** — `PermissionResult`, `Interrupt`, `PreCompact`, `PostCompact`, `StopFailure`, `PostToolUseFailure` all map to `Unknown`. Two 0.31.1 gaps ride on that: `SubagentStop` never fires (a sub-agent node closes on the synchronous `PostToolUse` of its `Agent` tool call instead — kimi's `Agent` tool returns `status: completed` in `tool_output`), and `Stop` does NOT fire on an Esc interrupt, so an interrupted turn reads `working` until the next hook arrives.
- **Transcript layout** — a per-session DIRECTORY at `${KIMI_CODE_HOME:-~/.kimi-code}/sessions/wd_*/<session_id>/` (the `wd_` hash is opaque, so the locator globs for the `<session_id>` child) holding `state.json` (`title`, honored only when `isCustomTitle`) and `agents/main/wire.jsonl` (the transcript; each sub-agent gets its own `agents/agent-<N>/wire.jsonl`). Assistant prose is the last `context.append_loop_event` `content.part` of type `text` (`think` parts are chain-of-thought, not words); the model comes from `usage.record.model` / `llm.request.modelAlias`; context fill is the freshest `usage.record`'s `inputOther + inputCacheRead + inputCacheCreation`. Model ceilings: `k3` → 1M, `k3-256k` / `kimi-for-coding(-highspeed)` → 256K, anything else → the conservative 200K — matched on the basename after the last `/`, because on-disk ids arrive provider-prefixed (`kimi-code/kimi-for-coding`). A kimi sub-node has no transcript probe: 0.31.1 writes no correlator between a hook's `tool_call_id` and its `agent-<N>` dir.

Wiring: `aoide hooks install kimi` (see [[Agent-Interface]]) merges ten `[[hooks]]` tables (the nine core events + `PermissionRequest`) into `config.toml`, honoring `KIMI_CODE_HOME`; by hand, one table per event:

```toml
[[hooks]]
event = "SessionStart"
command = "aoide graph session hook --agent kimi"
timeout = 5
```

An entry carries ONLY `event`/`command`/`timeout` — extra fields (a claude-style `matcher`) make kimi's config fail to load.

Operator notes: kimi's TUI submits on `\r`, not `\n` — `graph send --submit` resolves this automatically from the target's own agent profile (see [[Conductor-Channel]]), so no manual workaround is needed. And kimi's model aliases are provider-prefixed on the CLI too (`-m kimi-code/kimi-for-coding`; a bare alias errors `config.invalid`).

### Any plain CLI agent (no hook system)

```sh
aoide graph wrap --agent aider -- aider --model sonnet
```

Running→done for free; the graph shows who is out working even when the agent can't speak.

### A harness with its OWN hook/event system

Map its lifecycle onto the explicit verbs. Example shape (pseudo-config for any harness that can run a shell command on events):

```
on_start:    aoide graph session start --id "$MY_ID" --agent myagent --cwd "$PWD"
on_tool:     aoide graph session phase --id "$MY_ID" --phase working
on_ask:      aoide graph session phase --id "$MY_ID" --phase awaiting
on_turn_end: aoide graph session phase --id "$MY_ID" --phase stopped
on_exit:     aoide graph session end   --id "$MY_ID"
```

`--phase stop` and `--phase stopped` both mean the TURN ended and both fold to
`stopped`; process exit is `graph session end` (or the `exit`/`finished`/`complete`
vocabulary). A harness that has no turn-end event at all can leave the session
`working` and let its `on_exit` close it.

The one rule: whatever fires on "agent asks a human something" maps to `awaiting`, and every event that means "moving again" maps back to `working` — keep the clearing set closed.

### Nesting (sub-agents)

Pass `--parent "$AOIDE_SESSION_ID"` (or the `--parent` flag on `session start`) from inside a wrapped/hooked session and the DAG draws the spawned-by edge — orchestrator → worker trees render in the dock and conductor automatically. Cycles are checked and refused.

## Related

- `aoide guide` — the terse in-CLI version of this page.
- [[aoide-cli]] — the full command tree, including `conduct` and the interactive `conductor` TUI that renders every door's sessions.
- [[Agent-Interface]] — the CLI trunk the hook door and `hooks install` live on.
- [[Conductor-Channel]] — the send/injection semantics, including per-profile submit keystroke resolution (`\n`, or `\r` for kimi).
- [[Terminal-Commander]] — the graph concept (projects anchor sessions by cwd).
- [[shellbridge]] — its socket accept loop is live for the window-jump verb (`focuswindow`), but session *registration* (start/phase/end) still has no socket verb; the CLI doors above remain the writers — and the permanent fallback.
- [[Widget-Bridge-Contract]] — the full `sessions.json` field contract and canonical-state rules the states above feed.
