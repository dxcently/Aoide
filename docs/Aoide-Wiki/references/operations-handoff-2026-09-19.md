# Aoide operations handoff — 2026-09-19, Claude → ChatGPT

Written for a ChatGPT session picking up this work cold. Read
`AGENTS.md` (repo root) and `docs/agent/README.md` first for house rules;
this file is state, not policy. The older `AOIDE-HANDOFF.md` and
`operations-handoff-2026-09-14.md` are historical — this supersedes them
operationally.

## What Aoide is, in one paragraph

Aoide (`aoide`/`aoided`) is a portable orchestration engine: a CLI/daemon
that tracks every terminal as a conductable session, anchors sessions to
registered projects by cwd, and lets one session command another. Lyra is
the separate desktop/Quickshell paint layer built on top (AoideOS). Core
never depends on nix or Quickshell; only lyra and deployment modules may.
Every mutation goes through `aoided` under one stage lock, one audit log.

## Repo state right now

- Branch `main`, HEAD `47d8625` — "a project may name a lead session; the
  rest of its roots hang off it" (graph feature, landed this session, not
  pushed to any remote beyond local `origin/main` — check `git log
  origin/main..HEAD` before assuming it's shared).
- Working tree: `git status --short` had unrelated pending files at
  session start (`docs/.obsidian/*`, `modules/dendrites/inference.nix`,
  a few `M` docs/hosts) — these predate this session, were never touched
  here, and should stay untouched unless you know what they are.
- Version: `0.0.23`.

## What landed this session (verified, not yet deployed)

**Project lead lane** (commit `47d8625`, amended once after landing to
trim a comment). A project record may now carry `lead: Option<String>`: one
session placed directly under the project node in the graph, with every
other parentless session of that project hanging off the lead via a new
`leads` edge instead of the usual `anchors` edge. Command: `aoide project
lead <name> <session>`, `--none` to clear. Conductor TUI: session
context-menu action "Lead project". Docs updated in the same commit
(CONTRACTS.md projects.json/graph.json, Session-Graph.md, conduct/AGENTS.md
invariant, conductor/README.md action table). Tests green in the devshell:
storage 418, conduct 815, conductor 171, cli 42 (goldens included).

**Not yet done from that lane:** `pkgs/aoide/crates/cli/src/cli.rs:281`
still has a stale comment ("four children" → now five). One-line fix,
outside the sanctioned pathspec at commit time, deliberately left.

**Sleep/suspend diagnosis** (yomi-strix, this session, not yet fixed):
s2idle suspend aborts almost instantly, repeatedly, this boot. Root cause
traced to two Corsair Slipstream wireless USB receivers on bus 3 (behind
PCI bridge `00:08.3` → xHCI `c7:00.0`) with `power/wakeup=enabled` —
any RF chatter from mouse/keyboard reasserts a PME and aborts the sleep
within a second. Wake counters on `c7:00.0/.3/.4/.5/.6` match every
suspend attempt this boot. Proposed fix (not applied, needs a rebuild the
user runs): a udev rule disabling wakeup on vendor `1b1c` (Corsair) USB
devices, added to `hosts/yomi-strix/default.nix`. Osaka unreachable since
09-15, so its own sleep behavior is unverified — if it has a similar
wireless dongle, same fix likely applies.

**Workspace-naming proposal** (discussed, not started): user asked
whether lyra can track/name Hyprland workspaces and group subsequent
projects/sessions under them. Framing given: this is aoide's job (state),
lyra only paints it (house rule 7 — delete every `.qml`, capability must
still work from a shell). Proposed shape: `workspace:<id>` node above
project nodes in the graph, `aoide workspace name <id> <name>` writing
`state/stage/workspaces.json` under the same stage-lock discipline as
projects; sessions already carry a Hyprland workspace id
(`SessionRecord.workspace`) so grouping is free once the node exists.
**Open question left with the user, unanswered:** should the name be a
new aoide-owned record, or should aoide just read/write Hyprland's own
`renameworkspace` name (mirroring rather than owning it)? Ask before
building — it changes where the source of truth lives.

## Standing priority queue (register, not re-litigated)

In order, per the live task-stack memory (`aoide-task-stack.md`,
codex seq 158, STATE 2026-09-17b as of last register write):

1. **`aoide mail export`** — letters → one note per thread for Mneme.
   Mneme already has `embed_text`/`similar_notes`/`semantic_centroid`.
   Target vault `magi` (`~/Magi`, syncthing-synced) is **not present on
   yomi** — syncthing inactive here, so default to an export dir under
   `state/` until the user enables the share. This was next before the
   root-agent/lead lane and the sleep question interrupted it — resume
   here.
2. Conductor TUI panels for config, secrets, and pairing — settable via
   right-click, per the user's original ask this session. Needs a
   `secrets status` seam that doesn't exist yet (register §21/§25).
   Not started.
3. Register commit: today's rulings (JEV = candle+rune, project-lead
   landed, sleep diagnosis) + dxflake re-lock (dxflake has not tracked
   Aoide main since commit `5ee097b`, several commits behind).
4. Ping-back awaiting-parent guard needs a mid-turn ruling from the user
   (queued/defer semantics) — parked, not blocking.
5. `aoide do` VV shell-out, overseer JEV work, rebuild-criteria row, lyra
   lanes — all queued behind the above, unstarted.

## Eidolon state (the DeepSeek dispatch harness)

- `~/eidolon` branch `trace` (upstream master `8a09c4a` + local `df58745`)
  has SIGTERM=SIGINT handling, `--deadline`, and a `.jsonl` trace mirror
  + `log --json`. **User has rebuilt and installed this** —
  `~/.local/bin/eidolon` is the nix build of `trace`, confirmed live
  (smoke run wrote a `.jsonl`).
- Two PRs open against upstream `noah427/eidolon` (`master`), from fork
  `dxcently/eidolon`: PR #7 (`signals-deadline`) and PR #8
  (`trace-jsonl`). Both green, both awaiting upstream review — do not
  push to `noah427/eidolon` directly, fork+PR only.
- E5b ping-back (parent hears children it spawned) is **landed in Aoide
  main** (`a5e0f63`) but **inert** until `aoided` is restarted on that
  code — check whether that restart has happened before assuming
  ping-back is live.
- Orchestration pattern used successfully this session and last: hand a
  bounded, fully-specified brief to `eidolon run --yolo -m
  ollama:deepseek-v4.1-flash --cwd <dir> "<brief>"`, backgrounded with
  `timeout -s INT <secs>`, never `pgrep -f`/`pkill -f` (self-match risk —
  watch by pid file only). Works well for "finish this already-drafted
  lane, get it green, commit by pathspec" tasks with an explicit pathspec
  and explicit rules (no python, no cargo fmt, no `--workspace` test
  runs, docs in the same commit).

## House rules a fresh session must not relearn the hard way

- Cargo only inside `nix develop /home/khoa/Aoide -c env TMPDIR=/tmp
  cargo test -p <crate>` — never `cargo fmt`, never `cargo test
  --workspace` in Aoide.
- Commits: `git commit -q -F <msgfile> -- <pathspec>`, never bare `git
  commit -a` or `git add -A` (the tree reliably has unrelated dirty
  files). Always `git show --stat HEAD` after, to catch anything foreign
  that rode along.
- Trailers on every commit/PR from this point are **Claude Sonnet 5**,
  not Fable — attribution changed mid-session:
  `Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>` and the
  session's `Claude-Session:` URL.
- Doors are loopback-only; never propose a routable bind. SSH tunnels are
  the only cross-box transport.
- yomi rebuilds run free via the broker at every boundary — no asking.
  Cross-host rebuild needs the TOTP gate. Never hold or relay a
  credential, even one typed in chat.
- `song/` is the only nix-module-side writable domain for ricing; core
  crate changes go through the crates tree with docs updated in the same
  commit (house rule 8).
- No python3 anywhere in this repo's tooling — perl heredocs with a
  die-on-no-match `patch()` helper for scripted edits.

## Three-host mesh state (register §11)

- yomi ↔ sakaki: gate 2 (yomi→sakaki mail) now **accepted** — user ran
  `aoide node allow yomi-strix message on` this cycle.
- osaka: offline since 2026-09-15.
- chiyo: offline since 2026-08-28.
- Melete (the personal-AI-hub harness on sakaki) has its own secrets
  store separate from eidolon's; an `ollama` key rotated into eidolon's
  store on sakaki does **not** propagate to Melete's store
  (`~/Melete/secrets.enc`) — Melete job runs there still fail on missing
  key until the user calls `secret_set` from a Melete-native surface
  (Telegram/Agora), or Melete is taught to read eidolon's store. This is
  unresolved and blocking any sakaki-side Melete/ollama runs.

## Unresolved questions for the user (do not guess these)

- Workspace-name ownership: aoide-owned record vs. Hyprland-mirrored name
  (see above).
- Mid-turn ping-back queue/defer ruling.
- Whether to reopen the yomi local-LLM pin (Qwen3.8-27B Q4, 32GB
  carveout) — currently PINNED, do not act until the user reopens it.

---
Co-Authored-By: Claude Sonnet 5 <noreply@anthropic.com>
Claude-Session: https://claude.ai/code/session_018ghjinUWsk6QKjraKJFCmF
