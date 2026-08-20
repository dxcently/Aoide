# Graph & Conduct Verbs — the Session-Graph Command Surface

The `graph` verb group plus the top-level `conduct` command drive the
[[Session-Graph]]: they register and resolve sessions in the stage registries,
shape the project/session DAG, inject text into conducted terminals (the
[[Conductor-Channel]]), jump to windows ([[Terminal-Commander]]), and reap dead
sessions. `graph session hook` is the [[Agent-Hooking]] door agent harnesses
(Claude Code, kimi, pi) fire into. Handlers live in
`pkgs/aoide/crates/conduct/src/graph/{verbs,session_store,send,pending,spawn,permit,window,conduct,doc,model,common}.rs`
and `pkgs/aoide/crates/conduct/src/reap.rs`; registrations in
`pkgs/aoide/crates/conduct/src/commands/graph.rs`.

Path resolution (`pkgs/aoide/crates/storage/src/fs.rs`): the stage dir is
`$AOIDE_STAGE_DIR` when absolute, else `~/Aoide/song/stage/` — so
`song/stage/{projects,sessions,hooks,graph,pending,herald}.json` all ride that
one override. The state dir is `$AOIDE_STATE_DIR` else `~/Aoide/state/`. Every
stage write goes through `aoide_storage::fs::atomic_write` (temp
`<stem>.tmp.<pid>`, `fsync`, rename, symlink-transparent) and multi-file
load-modify-writes serialise through `with_stage_lock` (a non-reentrant
`.stage.lock` flock in the stage dir). Every mutation re-stages
`song/stage/graph.json` via `restage_graph` so the hot-reloaded document never
drifts from the registries. The audit log defaults to `~/Aoide/log`
(`$AOIDE_AUDIT_LOG`, or the `--audit-log` flag, override).

Every command takes `--json`: without it the CLI prints the human `message`
line (plus a `changed:` trailer); with it, an envelope `{status, command,
message, gated, changed, data?}` (`pkgs/aoide/crates/protocol/src/output.rs`).
Exit codes: 0 ok, 1 error, 2 usage, 64 not-implemented (none in this group).
None of these verbs is `gated: true` in the schema — `graph send`'s
pending-approval gate is an internal policy, not the user rebuild gate.

### aoide graph view

```
aoide graph view [--focus <node>] [--json]
```

- **Reads:** `song/stage/{projects,sessions,hooks}.json` (missing files read as
  empty); also folds in `state/a2a-agents.json` (`a2a:*` root nodes) and
  `state/peers.json` + `state/peer-cache/<name>.json` (`peer:*` root nodes,
  `children` nested verbatim when the cache is fresh) via `build_graph`
  (`doc.rs`).
- **Output:** text — `"<n> node(s), <e> edge(s)\n"` followed by a Unicode
  box-drawing tree: `◆` project roots, `●` sessions (spawned children nest
  under their parent, project-less sessions under a synthetic `(unanchored)`
  root), `▶ ` marks the `--focus` node (matches `session:<id>`,
  `project:<name>`, or the bare id). `--json`: `data` is the full `graph.json`
  document `{schemaVersion, nodes, edges}` — identical to what `graph emit`
  stages.
- **Notes:** read-only. Session states shown are hook-merged and folded to the
  canonical five (`working | awaiting | stopped | idle | done`) by
  `merged_sessions`.

### aoide graph project add

```
aoide graph project add <name> [<path>] [--json]
```

- **Reads:** `song/stage/projects.json`. `<path>` defaults to the current
  working directory; a relative or nonexistent path is refused as usage (exit
  2, `data.reason: "invalid-path"`) because anchoring is absolute-path prefix
  matching.
- **Writes:** `song/stage/projects.json` (atomic; sorted by name,
  `schemaVersion` stamped) then re-stages `song/stage/graph.json` — only when
  something changed.
- **Output:** `data: {name, path, file}`. Idempotent: re-adding the same
  name+path is an ok no-change ("already registered"); a new path for an
  existing name updates it in place.
- **Notes:** a session anchors under the longest-prefix matching project root
  (`model.rs::anchor_for`).

### aoide graph project remove

```
aoide graph project remove <name> [--json]
```

- **Reads:** `song/stage/projects.json`.
- **Writes:** `song/stage/projects.json` + re-staged `graph.json`, only on an
  actual removal.
- **Output:** `data: {name, file}`; an absent name is an ok no-op (`{name}`
  only, no `changed`).

### aoide graph project list

```
aoide graph project list [--json]
```

- **Reads:** `song/stage/projects.json`.
- **Output:** text — `"N project(s) registered"` plus one `◆ <name>  <path>`
  line each (name-sorted); `data: {projects: [...]}`.
- **Notes:** read-only.

### aoide graph link

```
aoide graph link <child> <parent> [--json]
```

- **Reads:** `song/stage/sessions.json`.
- **Writes:** sets `parentSessionId` on the child record (atomic), re-stages
  `graph.json`. No write when already linked.
- **Output:** `data: {child, parent}`; an unregistered parent still records the
  edge with `data.warning: "parent session not (yet) registered; edge recorded
  anyway"`.
- **Notes:** errors (exit 1): `reason: "self-link"`, `"child-not-found"`, or
  `"cycle"` (the walk in `doc.rs::would_cycle`). Ignores `peer:*`/`a2a:*` ids
  like any unknown local id.

### aoide graph session start

```
aoide graph session start --id <id> [--agent <name>] [--cwd <dir>] [--window <addr>] [--parent <sessionId>] [--json]
```

- **Writes:** UPSERT into `song/stage/sessions.json` under the stage lock, then
  re-stage `graph.json`. A fresh record starts `idle` with `agent` defaulting
  to `claude`; a re-start updates only the provided fields and preserves
  `startedAt` and the live state (`upsert_session`). A `--parent` that would
  cycle is refused (exit 1, `reason: "cycle"`).
- **Output:** `data: {sessionId, agent, startedAt, inserted, file}`.
- **Notes:** registration-time same-window agent eviction: if the NEW record is
  an agent with a `windowAddress` shared by another live agent record, the
  stale twin is marked `done` and pruned immediately (a terminal hosts one
  foreground agent; conducted PTY hosts and the record's own parent are
  excluded).

### aoide graph session phase

```
aoide graph session phase --id <id> --phase <phase> [--json]
```

- **Writes:** UPSERTs the hook record in `song/stage/hooks.json` (fresh
  `updatedAt` every call — the per-hook heartbeat) AND lands the canonical
  state on `song/stage/sessions.json` (change-only; legacy vocab like
  `running`/`blocked` migrates in passing; a non-`working` state clears
  `activity`); then re-stages `graph.json`. All under one stage lock.
- **Output:** `data: {sessionId, phase, updatedAt, file}`.
- **Notes:** `--phase` folds to the canonical set via `canonical_state`:
  `working`, `awaiting`, `stopped` (also `stop`), `idle`, `done` (also
  `exit`/`finished`/`complete`); unknown/empty → `idle`. Latest `updatedAt`
  wins on merge. No-op for state purposes on an unregistered id (the hook
  record is still written).

### aoide graph session end

```
aoide graph session end --id <id> [--json]
```

- **Writes:** `state=done` in `song/stage/sessions.json` (the record stays for
  the widgets' done pose), cascades-drop its whole `sub:*` sub-agent subtree
  (`doomed_subagent_descendants`), mirrors `phase=done` into
  `song/stage/hooks.json`, re-stages `graph.json`.
- **Output:** `data: {sessionId, file}`; an unknown id is an ok no-op.

### aoide graph session hook

```
aoide graph session hook [--agent <claude|kimi|pi>] [--json]   # reads ONE hook JSON object from stdin
```

- **Reads:** stdin (one harness hook payload); the agent profile's event map
  (`aoide_protocol::agents`); `song/stage/{sessions,hooks}.json`; env
  `AOIDE_SESSION_ID` (threads a launching conductor as the parent),
  `HYPRLAND_INSTANCE_SIGNATURE` + `hyprctl clients -j` + `/proc/<pid>/stat`
  ancestry walk (best-effort window/pid backfill); harness transcripts for the
  `say`/`title`/`model`/`contextTokens` refresh (claude:
  `~/.claude/projects/<munge(cwd)>/<session_id>.jsonl`).
- **Writes:** any of `sessions.json`/`hooks.json` the mapped action implies
  (session start/phase/end, `sub:<id>` sub-agent spawn/re-key/end, set-once
  `title` from the first user prompt, payload-reported `pid` and
  `contextCeiling`), each change-only and followed by a re-stage.
- **Output:** ALWAYS an ok envelope for a payload problem: `data.action:
  "none"` with a `reason` for empty/malformed/unmapped input, or
  `data: {action: "applied", innerStatus, innerData}` wrapping the inner
  verb's outcome — exit stays 0 so a stage hiccup never breaks the hooked
  session. Only a bogus `--agent` is a real error (exit 1,
  `reason: "unknown-agent"`).
- **Notes:** mapped classes: SessionStart→start, UserPromptSubmit→`working`
  (+name), PreToolUse/PostToolUse→tool start/end (Task/Agent tools spawn,
  re-key on async `isAsync`, and close `sub:` nodes), Stop→`stopped`,
  Notification→`awaiting` (permission prompt, unconditional) or conditional
  `awaiting`-only-if-still-`working` (idle ping), SessionEnd→end. On an
  unconditional `awaiting` it spawns `aoide graph permit --id <id>
  [--what <notification message>]` DETACHED (opt-out:
  `AOIDE_HERALD_PERMIT` in {0,false,no,off}); the harness's message rides as
  untrusted display data. A session whose record was ended/pruned mid-process
  self-heals by re-registering on its next event (`hook_ensure_session`).

### aoide graph wrap

```
aoide graph wrap [--agent <name>] [--parent <sessionId>] [--id <id>] -- <command …>
```

- **Reads:** nothing on disk; spawns `<command>` with INHERITED stdio and
  `AOIDE_SESSION_ID=<id>` in its env (defaults: `--id wrap-<pid>-<unixts>`,
  `--agent` = the command's basename).
- **Writes:** `song/stage/sessions.json` — register running (AFTER a
  successful spawn, so a failed exec leaves no ghost; `pid` is the wrap
  process's own pid, the lifecycle owner the reaper watches), then
  `do_session_end` whatever happened; re-stages `graph.json` both times.
- **Output:** the child's stdio passes through untouched. Exit mirrors the
  child: 0 on success, 1 otherwise (a signal kill reports code −1), real code
  in `data.exitCode`; envelope `data: {sessionId, agent, exitCode}`.
- **Notes:** the universal door for hookless agents — anything hookable inside
  can self-report via `graph session phase --id "$AOIDE_SESSION_ID"`.

### aoide graph spawn

```
aoide graph spawn [--agent <name>] [--parent <sessionId>] [--id <id>] [--prompt <text>] -- <command …>
```

- **Reads:** re-execs `current_exe()` (the running `aoide` binary itself) as
  `conduct --headless --agent <agent> --id <id> [--parent <p>] -- <command …>`,
  detached into its own session (`setsid`, stdio nulled) so it OUTLIVES this
  call — the same self-re-exec idiom `server/src/a2a.rs`'s `do_spawn` and
  `shellbridge.rs` use.
- **Writes:** nothing directly — the re-exec'd `conduct --headless` child does
  all the registration (see below). This process only polls for the child's
  control socket (`$XDG_RUNTIME_DIR/aoide/session-<id>.sock`) with a real
  `UnixStream::connect` — never bare file existence, since a SIGKILLed prior
  session with the same `--id` can leave a stale socket FILE the reaper hasn't
  swept yet — for up to 3000 ms before giving up and reporting
  `registered: false` anyway (the spawn itself already succeeded; this is only
  "is it steerable yet"). A bad `<command>` fails the re-exec'd `conduct`'s own
  spawn first, so no ghost session is ever registered — the socket simply
  never appears and `spawn` returns `registered: false` honestly.
- **Pipes to / output:** `data: {sessionId, agent, socket, logPath,
  registered, prompt}`. `--prompt`, when given, is injected only AFTER
  registration succeeds, through the one gated injection door — `graph send
  --yes --submit`, in-process, the exact re-drive shape `graph pending
  approve` uses to replay a held entry — never a direct socket write;
  `data.prompt` reads `none` / `delivered` / `skipped-unregistered` /
  `failed: <reason>`.
- **Notes:** the detached counterpart to `conduct`/`wrap` — this call returns
  immediately while the spawned agent keeps running headless. See "Headless
  conduct" under [[Conductor-Channel]] for the pty/log mechanism the re-exec'd
  child uses.

### aoide graph send

```
aoide graph send --id <id> [--submit] [--yes] [--from <sender>] -- <text …>
```

- **Reads:** `song/stage/sessions.json` (resolves the target's `socket` and
  `parentSessionId`, and — for the sibling rule — the SENDER's own record from
  the same already-loaded file); env `AOIDE_SESSION_ID` (the sender's own id,
  for the parent- and sibling-autogate rules and as the provenance fallback)
  and `AOIDE_CONDUCT_AUTOGATE` in {1,true,yes,all} (the box-wide
  orchestration-mode switch) and `AOIDE_CONDUCT_SIBLING_AUTOGATE` in
  {0,false,no} (the sibling-rule opt-out).
- **Writes:** pending path — appends `{sessionId, text, submit, queuedAt,
  from?}` to `song/stage/pending.json` (atomic, under the stage lock; `from`
  carries the resolved sender attribution when one resolves, omitted
  otherwise). Delivered path — opens the target's control socket
  (`$XDG_RUNTIME_DIR/aoide/session-<id>.sock`) and writes `<text>` (+ `\n` on
  `--submit`), prefixed with `from <sender>: ` on its first line when a sender
  resolves AND the text names the node (below); then auto-renames the node
  (`title` in `sessions.json` + re-stage) to a one-line ≤60-char form of the
  UNPREFIXED text — UNLESS the text carries no letter at all (a bare keystroke
  answer like `1`), which leaves the title alone AND skips the provenance
  prefix (a `graph permit` verdict digit must land byte-exact, not `from
  orch-1: 1`). EVERY outcome appends one audit record (`class: "audit"`,
  `command: "graph.send"`, status `pending|delivered|error`, the unprefixed
  text as `untrusted_data` — never the message — and the resolved sender, if
  any, folded into the message) to `~/Aoide/log`.
- **Output:** pending → exit 0, `data: {id, state: "pending", delivered:
  false, submit, gate}`; delivered → exit 0, `data: {id, state: "delivered",
  delivered: true, submit, title, gate}` with `gate` ∈ `yes | autogate |
  autogate-parent | autogate-sibling`. Errors (exit 1, audited): `reason` ∈
  `session-not-found`, `not-conductable`, `socket-unreachable`,
  `socket-write-failed`.
- **Notes:** the one gated injection door. Gate order: `--yes` →
  `AOIDE_CONDUCT_AUTOGATE` → sender-is-the-target's-parent (an orchestrator
  freely commands children it conducted) → sender-and-target-are-siblings
  sharing a live parent (on by default, opt out with
  `AOIDE_CONDUCT_SIBLING_AUTOGATE`; a self-send is excluded before the sibling
  check ever runs, so a session can never autogate-deliver to itself) → held
  pending. `--from` sets the delivered/queued provenance attribution
  explicitly (sanitized of embedded `\n`/`\r`); omitted, it falls back to
  `AOIDE_SESSION_ID`; `--from ""` is explicit anonymity and skips that
  fallback — attribution only, never authentication, since either source is a
  same-user value any caller can set to whatever it likes. `aoide graph
  pending list|approve|deny` is the read/resolve surface over the held queue
  (below) — `approve` re-drives a held entry through this exact door with
  `--yes` and the entry's own `from`, in-process. `graph permit`'s verdict and
  the A2A door both re-enter this same function in-process rather than
  reimplementing it.

### aoide graph pending list

```
aoide graph pending list [--json]
```

- **Reads:** `song/stage/pending.json`, raw-JSON — a malformed entry (a bare
  string, or an object missing `sessionId`) is listed with `state: malformed`
  rather than failing the whole read.
- **Output:** text — one line per entry; `data: {pending: [...]}` with each
  entry's `id` (its array POSITION — the schema carries no id of its own, so
  positions shift the moment any entry resolves; re-list between multiple
  resolutions in one breath).
- **Notes:** read-only. The queue `graph send` parks a held injection in (no
  `--yes`, no autogate match) and where the A2A door parks its own held
  injects — previously a write-only dead drop nothing read back.

### aoide graph pending approve

```
aoide graph pending approve <id> [--json]
```

- **Reads:** `song/stage/pending.json`; `<id>` is the entry's list position.
- **Writes:** re-synthesizes and runs the exact `graph send --id <sessionId>
  --yes -- <text>` (with `--submit`/`--from` carried through) the held entry
  represents, in-process through `session_send` — the SAME door `graph
  permit`'s verdict-typing already goes through — then removes the entry from
  `pending.json` under the stage lock. Resolution is the audit line, not a
  persisted archive.
- **Output:** the inner `graph send` outcome. A malformed or out-of-range id
  fails cleanly (exit 1), leaving the entry untouched — never destroyed on
  failure.

### aoide graph pending deny

```
aoide graph pending deny <id> [--json]
```

- **Reads:** `song/stage/pending.json`; `<id>` is the entry's list position.
- **Writes:** removes the entry from `pending.json` under the stage lock —
  injects nothing.
- **Output:** `data: {id, sessionId}`. A malformed or out-of-range id fails
  cleanly, leaving the entry untouched.

### aoide graph permit

```
aoide graph permit --id <id> [--tool <name>] [--what <text>] [--json]
```

- **Reads:** `song/stage/sessions.json` (record, `conductable`/`socket`,
  `title`, `agent` → profile); the harness's verified permission-prompt keys
  (`aoide_protocol::agents::PermissionKeys` — only claude's are verified:
  approve `1`, deny `3`).
- **Writes:** none directly. Publishes the summons card by sending one
  newline-delimited JSON line `{"cmd": "heraldpush", "notification": {…}}` to
  the shellbridge socket `$XDG_RUNTIME_DIR/aoide/shellbridge.sock`; the daemon
  (single writer) folds it into `song/stage/herald.json`. Card: id
  `permit-<sessionId>`, `kind: "summons"`, category `x-aoide.permission`,
  `stackTag: aoide-permit-<sessionId>` (a second prompt in the same session
  REPLACES the standing card), `timeoutMs: 0` (never expires on its own).
- **Output:** returns as soon as the card is filed — `data: {id, agent,
  raised: true, card: "permit-<id>"}`. Errors (exit 1, nothing raised):
  `reason` ∈ `session-not-found`, `not-conductable`, `no-permission-keys`,
  `summons-unraisable`.
- **Notes:** the two guards — no control socket, no card; and the verdict is
  typed only while the session is still canonically `awaiting` (else
  `already-answered`, nothing injected). The human's click returns as
  `{"cmd": "heraldverdict", …}` over the shellbridge, and `answer_summons`
  types the bare digit (no newline) through `session_send` in-process with
  `--yes` — audited by the send door.

### aoide graph focus

```
aoide graph focus <node> [--json]
```

- **Reads:** `song/stage/sessions.json` (`<node>` accepts `session:<id>` or the
  bare id); shells out to `hyprctl clients -j` to VERIFY the stored
  `windowAddress` is still live (`hyprctl dispatch focuswindow` exits 0 even on
  a gone window, so verification comes first).
- **Pipes to / output:** `hyprctl dispatch focuswindow address:<addr>` on
  success. Text `focused session '<id>'`; `data: {node, windowAddress,
  dispatcher: "hyprctl"}`.
- **Notes:** errors (exit 1) with `reason` ∈ `session-not-found`,
  `no-window-address`, `hyprctl-unavailable`, `hyprctl-failed`,
  `window-not-found`. The same `focus_window` seam backs the shellbridge's
  click-to-jump (`focussession`, with a workspace-switch fallback the CLI verb
  does not use).

### aoide graph prune

```
aoide graph prune [--json]
```

- **Reads:** `song/stage/{sessions,hooks}.json`.
- **Writes:** drops every `done` session (cascading to its `subagent`
  descendants) and its hook records, and clears `parentSessionId` links left
  dangling; both files + re-staged `graph.json` only when something changed.
- **Output:** `data: {removed: [ids], clearedParents: [ids]}`; nothing to do →
  ok `"nothing to prune"`, both arrays empty.
- **Notes:** idempotent. The `done` record itself is written by
  `session end`/the reaper; prune is the sweeper on its own schedule.

### aoide graph reap

```
aoide graph reap [--announce] [--json]
```

- **Reads:** `song/stage/{sessions,hooks}.json`; `hyprctl clients -j` (window
  liveness + window-owner map — skipped without `HYPRLAND_INSTANCE_SIGNATURE`,
  falling back to pid-only); `/proc/<pid>` existence, `/proc/stat` `btime`
  (boot instant), harness transcript mtimes + tails (profile-dispatched);
  `$XDG_RUNTIME_DIR/aoide/` for orphaned `session-*.sock` files.
- **Writes:** only on change — marks dead sessions `done` in both files and
  prunes them (only when something was reaped), decays `stopped` → `idle`
  after 1 h (`STOPPED_IDLE_AFTER_SECS`, both files), drops orphaned hook
  records and superseded same-window agent tombstones, unlinks orphaned control
  sockets (>60 s settled, id not on the roster, nothing listening on them),
  re-stages `graph.json`. Afterwards (outside the lock) refreshes every
  surviving agent's transcript fields (`say`/`tool`/`model`/context fill),
  reported as `data.refreshed` but deliberately NOT in `changed` — an agent
  merely speaking must not toast.
- **Pipes to / output:** desktop toast via a detached `notify-send
  --app-name=aoide "reaped 𓌳" <message>` — on `--announce` always, otherwise
  only when the sweep changed the roster (`data.announced` records the
  hand-off). Death signals: window gone; pid's `/proc` gone; staleness bands on
  the max of transcript mtime/hook `updatedAt`/`startedAt` — 72 h at rest
  (`idle`/`stopped`), 7 days mid-turn (`working`/`awaiting`), 2 h for
  `subagent` kind — plus the three ghosts: all-evidence-predates-boot,
  parent-gone dependents, orphaned sockets. A live agent pid that is not its
  window's owner vetoes staleness.
- **Output:** `data: {reaped, decayed, orphanHooks, supersededDone,
  orphanSockets, hyprctlAvailable}` (+ `refreshed`, `announced`). NEVER errors
  on nothing-to-reap or an unavailable compositor (exit 0,
  `"nothing to reap (all sessions live)"`).
- **Notes:** the ~12 s systemd timer runs this verb unannounced; the dock's
  reap control runs it `--announce` via the shellbridge. A false reap of a
  merely-quiet live session self-heals at its next hook event.

### aoide graph emit

```
aoide graph emit [--json]
```

- **Reads:** `song/stage/{projects,sessions,hooks}.json` (+ the a2a/peer folds
  `graph view` has).
- **Writes:** the resolved DAG to `song/stage/graph.json` (atomic) — the file
  Quickshell hot-reloads.
- **Output:** `data: {path, nodes, edges}`; `changed` lists the path.
- **Notes:** redundant in steady state (every mutation already re-stages), kept
  as the explicit staging step; byte-identical to what `graph view --json`
  computes.

### aoide conduct

```
aoide conduct [--agent <name>] [--parent <sessionId>] [--id <id>] [--headless] -- <command …>
```

- **Reads:** env `XDG_RUNTIME_DIR` (socket dir; falls back to
  `/run/user/1000`), `HYPRLAND_INSTANCE_SIGNATURE` + `hyprctl clients -j` +
  `/proc` ppid-ancestry (best-effort window discovery); for conducted SHELLS a
  ~1 Hz tick reads the PTY foreground pgid (`tcgetpgrp`), `/proc/<pid>/cwd`,
  `/proc/<pid>/cmdline|comm`, and `/proc/<pid>/task/<pid>/children` (the
  sudo-prompt probe, boosted by a `[sudo] password for` text scan of the
  output stream).
- **Writes:** binds the per-session control socket
  `$XDG_RUNTIME_DIR/aoide/session-<id>.sock` (unlinked on exit and on start if
  stale); registers the session in `song/stage/sessions.json` with
  `conductable: true`, `socket`, the discovered `windowAddress`, and
  `pid` = the conduct process itself; shell ticks push live
  `cwd`/`activity`/`state`/`needsSudo` (change-only, re-staging `graph.json`);
  `do_session_end` on exit whatever happened. Spawns `<command>` on a fresh
  PTY (`openpty`; `setsid` + `TIOCSCTTY`; slave dup'd over fds 0/1/2) with
  `AOIDE_SESSION_ID` exported, and puts the real tty in raw mode for the
  duration (an RAII guard restores it on every exit path; SIGWINCH resizes are
  propagated to the PTY master) — all of that raw-mode/resize handling is
  interactive-only and skipped entirely under `--headless` (below).
- **Pipes to / output:** a `poll()` multiplexer shuttles real stdin → PTY
  master → real stdout (the wrapped TUI runs undisturbed), and each accepted
  control-socket connection's bytes → PTY master (that is what `graph send`
  types into). Exit mirrors the child: 0 ok, 1 otherwise, real code in
  `data.exitCode`; envelope `data: {sessionId, agent, exitCode, conductable,
  socket}`.
- **`--headless`:** a pty session with NO controlling terminal — for a caller
  (a script, `graph spawn`, an orchestrating agent) that has none to give it.
  The multiplexer never pushes a stdin pollfd (there is nothing to read from)
  and the pty-master's output mirrors to an append-only, unrotated per-session
  log file — `state/sessions/<sessionId>.log` (`$AOIDE_STATE_DIR` else
  `~/Aoide/state/`) — instead of real stdout; the log's path is recorded on
  the session record as the additive `logPath` field the moment the file
  opens. Everything else — registration, the injection socket, `graph send`
  steering, exit mirroring — is identical to the interactive path. With no
  controlling tty to query, `openpty` would otherwise get a NULL winsize and
  leave the pty at 0×0 (full-screen TUIs misrender against or refuse that
  outright), so a headless pty falls back to a conventional 80×24 instead; the
  interactive no-tty case (rare, e.g. redirected stdin in a test) keeps its
  historical `None` unchanged. A log file that can't be opened (an unwritable
  state dir) degrades to stdout rather than killing the session, the same
  best-effort posture as the socket bind. See "Headless conduct & `graph
  spawn`" under [[Conductor-Channel]].
- **Notes:** defaults: `--id conduct-<pid>-<unixts>`, `--agent` = the command's
  basename (`shell` is special: only a shell agent gets the live-tick state
  driving — agents' states come from hooks). A bind failure leaves the session
  running but `conductable: false` (never injectable). Only conduct's own
  uncatchable death (SUPER+Q SIGKILL) strands the record — that is the reaper's
  case. `AOIDE_NO_CONDUCT=1` is the terminal-integration escape hatch (honored
  by the kitty launch wrapper in `modules/dendrites/kitty.nix`, not by this
  command): the terminal starts a plain shell instead of `aoide conduct`.

## Related

- [[Session-Graph]]
- [[Terminal-Commander]]
- [[Conductor-Channel]]
- [[Agent-Hooking]]
- [[Screen-Control]]
- [[aoide-cli]]
