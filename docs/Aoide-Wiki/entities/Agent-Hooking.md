---
type: entity
created: 2026-07-27
updated: 2026-07-28
tags: [aoide, agent, session, baton, orchestration, graph]
---

# Agent Hooking — putting ANY agent on the baton

*Authored 2026-07-27 against schema v0 phases (running · waiting · blocked · done). The stage files `song/stage/{sessions,hooks,graph}.json` are the single truth; the baton TUI, the Quickshell widgets (dock roster, DAG trace, bar ✎ cell), and the orchestration daemons all render from them. Anything that writes these files through the doors below becomes a full citizen of the graph.*

## The three doors

Pick by what the agent harness can do. All three converge on the same records; all are idempotent upserts.

### 1. The hook door — harnesses with Claude-Code-shaped hooks

Pipe ONE hook payload as JSON on stdin:

```sh
echo '{"session_id":"…","hook_event_name":"…","cwd":"…","message":"…"}' | aoide graph session hook
```

Event → phase map (the door is a silent no-op for everything else, and NEVER exits non-zero — safe inside any hook config):

| event | phase |
|---|---|
| `SessionStart` | registers the session, `running` |
| `UserPromptSubmit` / `PreToolUse` / `PostToolUse` | `running` |
| `Stop` | `waiting` (turn over, human's move) |
| `Notification`, message contains "permission" | `blocked` (mid-turn, agent NEEDS a human) |
| `Notification`, message contains "waiting for your input" | `blocked` only if currently `running` (an unseen mid-turn question); no-op otherwise |
| `SessionEnd` | `done` |

Hooks give you edges and there is no "unblocked" event, so every path OUT of a permission prompt must land on a mapped event. Approve → the tool runs → `PostToolUse` clears it. Deny with feedback → the model continues → next `PreToolUse`/`Stop`. Reply or interrupt → `UserPromptSubmit`/`Stop`. The set is closed; a blocked flag cannot stick.

The hook door also **discovers the session's window**: at `SessionStart` (and
backfilled on any later hook while still empty) it walks pid-ancestry against
`hyprctl clients -j`, the same match `conduct` uses — so a plain hook-registered
session (no wrapping, no PTY) still gets a `windowAddress` and is
focus-jumpable from the roster, same as a conducted one.

### 2. The explicit verbs — anything scriptable

```sh
aoide graph session start --id "$ID" --agent gemini --cwd "$PWD" [--parent "$PARENT_ID"]
aoide graph session phase --id "$ID" --phase blocked
aoide graph session end   --id "$ID"
```

Phase vocabulary and how the desktop renders it:

| phase | baton glyph | treatment |
|---|---|---|
| `running` | ♪ | working (green in baton; normal row in widgets) |
| `waiting` | 𝄐 | fermata — turn over, awaiting the human |
| `blocked` | 𝄐 | fermata in the urgent role (glitchPink) + pulse; the bar's ✎ cell flips paletteHot→glitchPink and pulses while ANY session is blocked |
| `done` | 𝄂 | final barline, dimmed; `graph prune` sweeps them |

### 3. The wrapper — hookless agents (codex, gemini, aider, anything)

```sh
aoide graph wrap [--agent codex] [--parent "$PARENT_ID"] -- codex --whatever-flags
```

- Spawns with **inherited stdio** — a wrapped TUI runs undisturbed.
- Registers `running` on spawn, resolves `done` on exit — crash included; nothing haunts the roster.
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
when something (a human via the `baton` TUI, or another agent) needs to type
into the session later.

## Per-agent recipes

### Claude Code

`.claude/settings.json` (project or user scope) — six events, one identical command each. The payload arrives on stdin; the door does the mapping:

```json
{
  "hooks": {
    "SessionStart":     [ { "hooks": [ { "type": "command", "command": "command -v aoide >/dev/null && aoide graph session hook >/dev/null 2>&1; exit 0" } ] } ],
    "UserPromptSubmit": [ { "hooks": [ { "type": "command", "command": "command -v aoide >/dev/null && aoide graph session hook >/dev/null 2>&1; exit 0" } ] } ],
    "Notification":     [ { "hooks": [ { "type": "command", "command": "command -v aoide >/dev/null && aoide graph session hook >/dev/null 2>&1; exit 0" } ] } ],
    "PostToolUse":      [ { "hooks": [ { "type": "command", "command": "command -v aoide >/dev/null && aoide graph session hook >/dev/null 2>&1; exit 0" } ] } ],
    "Stop":             [ { "hooks": [ { "type": "command", "command": "command -v aoide >/dev/null && aoide graph session hook >/dev/null 2>&1; exit 0" } ] } ],
    "SessionEnd":       [ { "hooks": [ { "type": "command", "command": "command -v aoide >/dev/null && aoide graph session hook >/dev/null 2>&1; exit 0" } ] } ]
  }
}
```

`Notification` + `PostToolUse` are what make **blocked** visible — without them a session on a permission prompt reads `running` forever.

### Any plain CLI agent (no hook system)

```sh
aoide graph wrap --agent aider -- aider --model sonnet
```

Running→done for free; the graph shows who is out working even when the agent can't speak.

### A harness with its OWN hook/event system

Map its lifecycle onto the explicit verbs. Example shape (pseudo-config for any harness that can run a shell command on events):

```
on_start:    aoide graph session start --id "$MY_ID" --agent myagent --cwd "$PWD"
on_tool:     aoide graph session phase --id "$MY_ID" --phase running
on_ask:      aoide graph session phase --id "$MY_ID" --phase blocked
on_turn_end: aoide graph session phase --id "$MY_ID" --phase waiting
on_exit:     aoide graph session end   --id "$MY_ID"
```

The one rule: whatever fires on "agent asks a human something" maps to `blocked`, and every event that means "moving again" maps back to `running` — keep the clearing set closed.

### Nesting (sub-agents)

Pass `--parent "$AOIDE_SESSION_ID"` (or the `--parent` flag on `session start`) from inside a wrapped/hooked session and the DAG draws the spawned-by edge — orchestrator → worker trees render in the dock and baton automatically. Cycles are checked and refused.

## Related

- `aoide guide` — the terse in-CLI version of this page.
- [[aoide-cli]] — the full command tree, including `conduct` and the interactive `baton` TUI that renders every door's sessions.
- [[Terminal-Commander]] — the graph concept (projects anchor sessions by cwd).
- [[shellbridge]] — its socket accept loop is live for the window-jump verb (`focuswindow`), but session *registration* (start/phase/end) still has no socket verb; the CLI doors above remain the writers — and the permanent fallback.
