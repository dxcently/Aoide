---
type: concept
created: 2026-08-19
updated: 2026-08-28
tags: [aoide, cli, session, graph, conductor]
---

# Graph & Conduct Commands — the Session-Graph Command Surface

The bare `graph` render, `graph link`, the `session`/`project` families, the
top-level `send`/`spawn`/`resurrect` verbs, and the top-level `conduct`
command are the [[Session-Graph]]'s command surface: session registration and
the project/session DAG, injection into conducted terminals
([[Conductor-Channel]]), window jumps ([[Terminal-Commander]]), the
dead-session reaper, and the `inbox` commands that read back what `send`
delivered. The `graph` prefix itself owns only the read/analysis lens now —
the bare render and `graph link`; everything that acts (`send`/`spawn`/
`resurrect`) or manages session/project lifecycle (`session *`/`project *`)
is its own top-level family (command-defrag task #101, Lane R). `session
hook` is the [[Agent-Hooking]] door agent harnesses (Claude Code, kimi, pi)
fire into, and — like `session start`/`phase`/`end` — is marked `internal`
in the schema: hook plumbing a harness's own lifecycle drives, hidden from
`aoide guide`'s human listing though still enumerated by `schema`/MCP/A2A.
The durable-sessions surface (command-defrag task #101, Lane U) adds
`session undying on|off` (the mark, renamed from its prototype name
"carry"), bare `session` (the undying picker), and a project manifest
(`.aoide/project.json`) that `resurrect` reads bare — see `session undying`,
bare `session`, and `resurrect` below. Handlers live in
`pkgs/aoide/crates/conduct/src/graph/{commands,session_store,send,pending,spawn,resurrect,undying,session_pick,permit,window,conduct,doc,model,common}.rs`
and `pkgs/aoide/crates/conduct/src/reap.rs`; registrations in
`pkgs/aoide/crates/conduct/src/commands/graph.rs` (the registered `path:`
renamed in place — handler names and file layout are unchanged). `inbox
list|read|clear` is the one exception: it lives in
`pkgs/aoide/crates/storage/src/{inbox, commands}.rs` — inbox is state, and
storage already owns the store.

Path resolution (`pkgs/aoide/crates/storage/src/fs.rs`): the conducting stage
dir is `$AOIDE_STAGE_DIR` when absolute, else `$AOIDE_ROOT/state/stage/` — so
`state/stage/{projects,sessions,hooks,graph,pending,herald}.json` all ride
that one override (`aoide_storage::fs::conducting_stage_dir`, distinct from
the rice `stage_dir` under `song/stage/`). The state dir is
`$AOIDE_STATE_DIR` else `$AOIDE_ROOT/state/` (default `~/.aoide/state/`).
Every stage write goes through
`aoide_storage::fs::atomic_write` (temp `<stem>.tmp.<pid>`, `fsync`, rename,
symlink-transparent) and multi-file load-modify-writes serialise through
`with_stage_lock` (a non-reentrant `.stage.lock` flock, still fixed to the
rice `stage_dir()` so conducting writes stay serialised against rice writers
sharing the same lock file). Every mutation re-stages `state/stage/graph.json`
via `restage_graph` so the hot-reloaded document never drifts from the
registries. The audit log resolves to `$AOIDE_ROOT/log` (default
`~/.aoide/log`; `$AOIDE_AUDIT_LOG`, or the `--audit-log` flag, override).

Every command takes `--json`: without it the CLI prints the human `message`
line (plus a `changed:` trailer); with it, an envelope `{status, command,
message, gated, changed, data?}` (`pkgs/aoide/crates/protocol/src/output.rs`).
Exit codes: 0 ok, 1 error, 2 usage, 64 not-implemented (none in this group).
No command in this group is `gated: true` in the schema; `send`'s
pending-approval hold is an internal policy, separate from the user rebuild
gate the flag denotes.

### aoide graph

```
aoide graph [--focus <node>] [--json]
```

- **Reads:** `state/stage/{projects,sessions,hooks}.json` (missing files read
  as empty); also folds in `state/nodes.json` + `state/node-cache/<name>.json`
  (`node:*` root nodes, `children` nested verbatim when the cache is fresh)
  via `build_graph` (`doc.rs`).
- **Output:** text — `"<n> node(s), <e> edge(s)\n"` followed by a Unicode
  box-drawing tree: `◆` project roots, `●` sessions (spawned children nest
  under their parent, project-less sessions under a synthetic `(unanchored)`
  root), `▶ ` marks the `--focus` node (matches `session:<id>`,
  `project:<name>`, or the bare id). `--json`: `data` is the full `graph.json`
  document `{schemaVersion, nodes, edges}` — identical to what every stage
  mutation restages to `state/stage/graph.json` automatically.
- **Notes:** read-only. Session states shown are hook-merged and folded to the
  canonical five (`working | awaiting | stopped | idle | done`) by
  `merged_sessions`.

### aoide project add

```
aoide project add <name> [<path>…] [--new] [--auto-resume] [--json]
```

A project is a set of anchor roots, not one directory. `add` grows that
set — it never replaces it (`project edit` does that).

- **Reads:** `state/stage/projects.json`. `<path>` defaults to the current
  working directory when omitted; give one or more — each is appended as a
  root. Every path is validated (absolute, an existing directory) BEFORE
  anything is written; one bad path in the list refuses the whole call
  (exit 2, `data.reason: "invalid-path"`) because anchoring is absolute-path
  prefix matching. `--new` refuses a name that already exists instead of
  adding roots to it (`data.reason: "exists"`), checked before validation or
  any write.
- **Writes:** `state/stage/projects.json` (atomic; sorted by name,
  `schemaVersion` stamped) then re-stages `state/stage/graph.json` — only when
  something changed.
- **Output:** `data: {name, path, roots, autoResume, file}` — `path` is the
  FIRST root (post-write, never reconstructed from locals), `roots` the full
  ordered list. Idempotent per root: a root already registered is a
  no-change ok; a new root on an existing name appends it; a brand-new name
  registers it with `path` = the first path given and `roots` the full
  list given (deduplicated).
- **Notes:** a session anchors under the longest-prefix matching root of any
  project (`model.rs::anchor_for`), not just its first.

### aoide project edit

```
aoide project edit <name> <path>… [--json]
```

The exact-replacement editor: swaps a project's whole root list for the one
given, atomically. `add`/`remove` are the only ways a project appears or
disappears — `edit` never creates, renames, or deletes one.

- **Reads:** `state/stage/projects.json`. The name must already be
  registered (`Status::Error`, `data.reason: "unknown"` otherwise); at least
  one `<path>` is required (bare `project edit <name>` is a usage error,
  nothing written). Every path is validated the same way `add` validates
  them, before any mutation. Duplicates in the list collapse, first-seen
  order preserved.
- **Writes:** `state/stage/projects.json` + re-staged `graph.json`, only
  when the resulting root list actually differs from the current one.
- **Output:** `data: {name, path, roots, autoResume, file}` — `path` is the
  first path given, `roots` the full ordered replacement list. `autoResume`
  is read back untouched; `edit` never sets or clears it.

### aoide project remove

```
aoide project remove <name> [<path>] [--json]
```

- **Reads:** `state/stage/projects.json`.
- **Writes:** `state/stage/projects.json` + re-staged `graph.json`, only on
  an actual removal.
- **Output:** `data: {name, path, roots, file}`. With no `<path>`: drops the
  whole project (today's original behaviour, byte-for-byte) — an absent name
  is an ok no-op. With `<path>`: drops that one root — if it was the
  project's `path`, the next remaining root is promoted into `path`; if it
  was the last root, the whole project is dropped; a `<path>` that is not
  one of the project's roots is an ok no-op.

### aoide project list

```
aoide project list [--json]
```

- **Reads:** `state/stage/projects.json`.
- **Output:** text — `"N project(s) registered"` plus one `◆ <name>  <path>`
  line each (name-sorted), followed by one indented line per extra root;
  `data: {projects: [...]}`.
- **Notes:** read-only.

### aoide graph link

```
aoide graph link <child> <parent> [--json]
```

- **Reads:** `state/stage/sessions.json`.
- **Writes:** sets `parentSessionId` on the child record (atomic), re-stages
  `graph.json`. No write when already linked.
- **Output:** `data: {child, parent}`; an unregistered parent still records the
  edge with `data.warning: "parent session not (yet) registered; edge recorded
  anyway"`.
- **Notes:** errors (exit 1): `reason: "self-link"`, `"child-not-found"`, or
  `"cycle"` (the walk in `doc.rs::would_cycle`). Ignores `node:*`/`a2a:*` ids
  like any unknown local id.

### aoide session start

`internal` in the schema — hook plumbing a harness's own lifecycle drives,
hidden from `aoide guide`'s human listing, never typed by an operator
directly.

```
aoide session start --id <id> [--agent <name>] [--cwd <dir>] [--window <addr>] [--parent <sessionId>] [--json]
```

- **Writes:** UPSERT into `state/stage/sessions.json` under the stage lock,
  then re-stage `graph.json`. A fresh record starts `idle` with `agent`
  defaulting to `claude`; a re-start updates only the provided fields and
  preserves `startedAt` and the live state (`upsert_session`). A `--parent`
  that would cycle is refused (exit 1, `reason: "cycle"`).
- **Output:** `data: {sessionId, agent, startedAt, inserted, file}`.
- **Notes:** registration-time same-window agent eviction: if the NEW record is
  an agent with a `windowAddress` shared by another live agent record, the
  stale twin is marked `done` and pruned immediately (a terminal hosts one
  foreground agent; conducted PTY hosts and the record's own parent are
  excluded).

### aoide session phase

`internal` in the schema, same standing as `session start` above.

```
aoide session phase --id <id> --phase <phase> [--json]
```

- **Writes:** UPSERTs the hook record in `state/stage/hooks.json` (fresh
  `updatedAt` every call — the per-hook heartbeat) AND lands the canonical
  state on `state/stage/sessions.json` (change-only; legacy vocab like
  `running`/`blocked` migrates in passing; a non-`working` state clears
  `activity`); then re-stages `graph.json`. All under one stage lock.
- **Output:** `data: {sessionId, phase, updatedAt, file}`.
- **Notes:** `--phase` folds to the canonical set via `canonical_state`:
  `working`, `awaiting`, `stopped` (also `stop`), `idle`, `done` (also
  `exit`/`finished`/`complete`); unknown/empty → `idle`. Latest `updatedAt`
  wins on merge. No-op for state purposes on an unregistered id (the hook
  record is still written).

### aoide session end

`internal` in the schema, same standing as `session start` above.

```
aoide session end --id <id> [--json]
```

- **Writes:** `state=done` in `state/stage/sessions.json` (the record stays
  for the widgets' done pose), cascades-drop its whole `sub:*` sub-agent
  subtree (`doomed_subagent_descendants`), mirrors `phase=done` into
  `state/stage/hooks.json`, re-stages `graph.json`.
- **Output:** `data: {sessionId, file}`; an unknown id is an ok no-op.

### aoide session undying

Not internal — an operator command, unlike `start`/`phase`/`end`/`hook` above.
The mark was prototyped under the name "carry" (task #96); this is its
shipped name.

```
aoide session undying (on|off) [--self | --id <id>] [--json]
```

- **Reads:** `state/undying.json` (the durable mark — durable-sessions plan
  P-C4); `state/stage/sessions.json`, only to answer the informational
  `live` field below, never to gate the write.
- **Writes:** `state/undying.json` directly — atomic write, no stage lock, and
  no `daemon_dispatch` routing, unlike every other `session *` command
  above: `undying.json` is not a `state/stage/` file, so it sits outside that
  dual-writer surface entirely. `on` adds the target id to the undying set
  (or refreshes its `markedAt` if already present); `off` removes it.
- **Output:** `data: {sessionId, undying, live}`; `changed` names the
  transition (`"<id>: undying"` / `"<id>: not undying"`) and stays empty on a
  re-mark that changed nothing.
- **Notes:** the target resolves from `--id <id>` (any session id, including
  one that has already left the roster — no roster lookup gates the write,
  which is what makes the mark flippable post-mortem, off a bare ledger id),
  or from `--self`/a bare invocation, both of which read `$AOIDE_SESSION_ID`.
  `--self` and `--id` together is a usage error; neither an `--id` nor a
  resolvable `$AOIDE_SESSION_ID` is likewise a usage error naming both
  flags — never a silent no-op. `live` reports whether the id is currently in
  `sessions.json`; it is informational only. The mark itself is the input
  bare `resurrect`'s selection reads (below) and `spawn --undying` writes at
  birth. On first load, `load_undying` renames a pre-existing
  `state/carry.json` onto `state/undying.json` — a one-shot, narrated,
  never-clobbering migration; the read side tolerates the legacy `"carried"`
  key until the next save normalizes it.

### aoide session (bare)

Cli+tty only — the undying picker. A non-CLI door, no tty, or `--json`
always steers to `session undying on|off --id <id>`, the one scripted
spelling; this command is never duplicated as a machine-reachable form.

```
aoide session [--json]
```

- **Reads:** `state/undying.json` and this host's own `state/stage/
  sessions.json` for the local roster; every registered node's CACHED
  `state/node-cache/<name>.json` (no live pulls) for node rows, via `who.rs`'s
  `sessions_from_graph` (widened `pub(super)` for this second consumer).
- **Writes:** a `tty`+`inquire` multi-select (`aoide_protocol::pick::
  choose_many`) opens over the combined local+node roster, each row
  pre-checked by its current undying state. Confirming toggles land in one
  batch: a local row's mark goes through one `load_undying`, N
  `set_undying` mutations, one `save_undying` — the same discipline
  `session undying`'s own single-id write already holds. A node row's mark
  can't touch `undying.json` (the id lives on the node) — it writes a
  `{host, dir, agent}` spec into the CURRENT project's `.aoide/project.json`
  manifest instead, resolved via the same `walk_up` bare `resurrect` uses
  below; no manifest above cwd reports every node mark/unmark as `skipped`
  with a taught reason while local marks in the same confirm still land. A
  node cwd that cannot relativize under the local project root is rejected
  the same way, never saved with a raw absolute `dir` (`save_manifest`
  refuses the whole batch on an absolute `dir`, so a single bad node spec
  never poisons the local+valid-node rows landing beside it).
- **Output:** `data: {changed: [...], skipped: [...]}` naming each row's
  disposition; a clean cancel out of the picker changes nothing.
- **Notes:** registered as a parent command alongside `session.*`, the same
  pattern bare `graph` sits beside `graph link`. See the manifest section
  under `aoide resurrect` below for `.aoide/project.json`'s shape and
  discovery rule.

### aoide session hook

`internal` in the schema, same standing as `session start` above — this is
the [[Agent-Hooking]] door.

```
aoide session hook [--agent <claude|kimi|pi>] [--json]   # reads ONE hook JSON object from stdin
```

- **Reads:** stdin (one harness hook payload); the agent profile's event map
  (`aoide_protocol::agents`); `state/stage/{sessions,hooks}.json`; env
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
  command's outcome — exit stays 0 so a stage hiccup never breaks the hooked
  session. Only a bogus `--agent` is a real error (exit 1,
  `reason: "unknown-agent"`).
- **Notes:** mapped classes: SessionStart→start, UserPromptSubmit→`working`
  (+name), PreToolUse/PostToolUse→tool start/end (Task/Agent tools spawn,
  re-key on async `isAsync`, and close `sub:` nodes), Stop→`stopped`,
  Notification→`awaiting` (permission prompt, unconditional) or conditional
  `awaiting`-only-if-still-`working` (idle ping), SessionEnd→end. On an
  unconditional `awaiting` it spawns `aoide session permit --id <id>
  [--what <notification message>]` DETACHED (opt-out:
  `AOIDE_HERALD_PERMIT` in {0,false,no,off}); the harness's message rides as
  untrusted display data. A session whose record was ended/pruned mid-process
  self-heals by re-registering on its next event (`hook_ensure_session`).

### aoide spawn

```
aoide spawn [--agent <name>] [--parent <sessionId>] [--id <id>] [--prompt <text>] [--windowed] [--cwd <dir>] [--undying] -- <command …>
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
  registration succeeds, through the one gated injection door — `send --yes
  --submit`, in-process, the exact re-drive shape `session pending approve`
  uses to replay a held entry — never a direct socket write; `data.prompt`
  reads `none` / `delivered` / `skipped-unregistered` / `failed: <reason>`.
- **Notes:** the detached counterpart to `conduct` — this call returns
  immediately while the spawned agent keeps running headless. `--windowed`
  opens a real terminal from `$AOIDE_TERMINAL` (a whitespace-split argv with
  a `{cmd}` placeholder) instead of a detached headless child — the same
  path bare `resurrect` uses to revive a candidate. `--undying` marks the
  spawned session durable in `state/undying.json` once it registers (a
  no-op if it never does) — the same mark `session undying on` sets, so this
  project's whole undying set can later be resurrected together. See
  "Headless conduct" under [[Conductor-Channel]] for the pty/log mechanism
  the re-exec'd child uses.

### aoide resurrect

```
aoide resurrect [--project <name> [--all | --id <ledgerSessionId>]] [--json]
```

**Bare (no flags) — the manifest walk (command-defrag task #101, Lane U).**
With none of `--project`/`--all`/`--id`, `resurrect` walks up from cwd
(`aoide_storage::manifest::walk_up`, lexical `Path::parent()` steps, LEXICAL
not realpath — a symlink inside the project pointing outside it escapes the
guard at use time, accepted under the manifest's host-local trust model) for
the nearest `.aoide/project.json` and revives that manifest's specs
directly — no `projects.json` registration needed at all. Not found, the
command falls through to the flag-mode selection below, and its usage error
then names both misses (`no .aoide/project.json above <cwd> and no
--project/--all/--id given`) only when a walk was genuinely tried; a call
that gave one of the three flags without `--project` (e.g. `--id X` alone)
never attempts a walk and gets the original `--project`-missing usage error
instead. See "The project manifest" below for the file's shape and the
per-spec revival rule; every invocation — including the empty-selection
no-op — writes exactly one audit line.

**Flag mode (`--project`/`--all`/`--id`) — unchanged.**

- **Reads:** `state/session-ledger.jsonl` (the durable, append-only record
  written exactly once at roster exit, P-D8) — resolves `--project` against
  `projects.json` by exact name, then filters ledger entries anchored to it
  via the SAME longest-cwd-prefix rule `build_graph`'s `anchor_for` uses.
  Candidates: `--all` widens to every anchored entry, `--id` narrows to one
  specific ledger `sessionId` (mutually exclusive with `--all`; `--id` wins
  if both are given), and bare-with-`--project` (neither `--all` nor `--id`)
  resumes the project's WHOLE undying set (`state/undying.json`,
  durable-sessions plan P-C4) — anchored entries currently marked durable
  via `session undying on`, minus any id already alive (non-`done`) in
  `sessions.json`, deduped by `sessionId` keeping the entry with the newest
  `endedAt` (an undying id resurrected and exited again can appear twice in
  the append-only ledger). `--all`/`--id` never consult the undying mark. An
  empty selection is an `Outcome::ok` no-op naming the undying set as empty,
  never a silent success. Each candidate is filtered through its harness's
  `AgentProfile.resume_args` — a harness with no verified resume argv
  (unregistered, or never confirmed against the real binary) is skipped,
  not guessed at.
- **Writes:** spawns each surviving candidate via the windowed path (the
  same mechanism `spawn --windowed` uses — a fresh terminal from
  `$AOIDE_TERMINAL` running `<harness> --resume <id>` in the ledger entry's
  own `cwd`). A resurrected session always mints a NEW `sessionId` — ledger
  ids are never recycled — and is stamped `resumedFrom` naming the ledger
  entry's own id; `build_graph` projects that as a `resumed` edge beside
  `spawned`/`anchors` (CONTRACTS.md §4). If the old id was undying, the mark
  transfers onto the new id in the same step (one `save_undying` call, never
  left on the now-dead old id).
- **Pipes to / output:** `data: {project, resurrected: [...], skipped:
  [...], failed: [...]}`. Never a hard error over a per-candidate spawn
  failure (a headless host with no `$AOIDE_TERMINAL`/display) — that
  candidate folds into `failed` instead, so a `--all` batch keeps going
  past one bad candidate. Only genuine usage problems (`--project` missing,
  an unknown project name, an `--id` naming no anchored ledger entry) are
  `Outcome::usage`/`error`.

**The project manifest — `.aoide/project.json` (Lane U).** Bare `resurrect`
reads a v0 manifest, `aoide_storage::manifest`: `{version, sessions: [{host,
dir, agent, command?}]}`. `dir` is project-relative (a lexical-join +
normalize containment guard, `resolve_spec_dir`, refuses any escape past the
project root); `command`, when given, is split on whitespace with NO shell
quoting awareness. The file lives at `<project_root>/.aoide/project.json`
and is HOST-LOCAL and SELF-IGNORING — the first `save_manifest` into a
project root writes a `.aoide/.gitignore` containing `*`, so the manifest
never gets committed: one focused host owns the shape, hand-edited or picker-
written, never synced via git. Each spec resolves independently (one spec's
failure never aborts the rest): a spec whose `host` matches this host
enriches from the ledger — the newest `state/session-ledger.jsonl` entry
whose `cwd`/`agent` match the resolved `dir`/spec's `agent` revives through
the exact `resolve_candidate`/`resurrect_one` path `--id` drives; no match
clean-spawns instead, windowed, the spec's own `command` when given else the
agent's registered default launch (an agent with neither is a taught
`failed[]` entry, never a guessed argv) — the manifest DECIDES WHAT exists,
the ledger only ever decides HOW. Every outcome row carries a `disposition`:
`revived-from-ledger`, `clean-spawned`, `summoned-remote` (below), a bare
`skipped`, or `failed`. Both local revival paths mark the fresh session
undying at successful spawn (`mark_manifest_revival_undying`) — the manifest
spec is itself the durable declaration, so a later resurrect finds it
without re-walking the manifest.

**Remote summon (Lane U4).** A spec whose `host` names a DIFFERENT box is
SUMMONED through the existing node door, never skipped: `spec.host` resolves
against `state/nodes.json` as a node NICKNAME, the same way the undying
picker writes it. Three local refusals land in `failed[]` (tried and
refused, not given up on) before the wire is touched: no node registered
under that name, a registered but UNVERIFIED node, or neither a `command`
nor a registered default launch to summon with. Past those,
`graph::resurrect::summon_remote` calls the identical signed spawn-shaped
`message/send` `aoide node spawn` drives (`client::commands::spawn_on_node`,
extracted from `handle_node_spawn` so both consumers share one
implementation) — no re-implementation of the wire, no shell-out to the
`aoide` CLI, no confirm prompt (a manifest spec is the operator's own
standing declaration). Which agent runs is the NODE's own configured
`aoide.a2a.spawnAgent`, never chosen here — the spec's `command` (or the
agent's default launch) only ever becomes that agent's first typed turn. The
wire carries no working-directory field at all, so a spec's `dir` is never
pushed across; a spec wanting a specific remote directory says so inside its
own `command` (`git -C <path> ...`). A summoned row is never marked undying
on this host — the resurrected id lives on the node.

**Boot auto-resume.** The same command core also backs the daemon's own
boot-time auto-resume trigger — a project's `autoResume` flag (`project add
--auto-resume`, default OFF, only ever set true by the CLI) fires `resurrect
--project <name>` once per boot on `aoided` start (P-D8, `docs/
architecture/AOIDED.md`'s "L5 — harness summoning" and "Open knobs"),
unconditionally for every `autoResume` project — the daemon carries no
liveness check of its own; a project whose whole undying set is already
alive simply resolves to the empty-set no-op above, so one live terminal
never suppresses reviving the rest of a multi-session undying set. This is
the revival half of the liveness story `session reap` sweeps the other side
of: a session a killed terminal could never mark `done` gets reaped off the
live roster, and its ledger entry is what `resurrect` can later bring back
in a fresh terminal. The trigger fires headless fine, but a windowed revive
from boot has one hard limitation: a systemd user unit's environment
freezes at spawn, before any compositor runs, so the daemon can never see
`WAYLAND_DISPLAY` — a boot-triggered windowed resurrect has no terminal to
open and degrades to the taught no-terminal error. The desktop-correct
fix — a graphical-session-side unit running `resurrect` once the compositor
env exists — is designed in `AOIDED.md`'s Open knobs and deliberately
unbuilt: `autoResume` stays default off, and the daemon trigger already
covers the headless case correctly.

### aoide send

```
aoide send (--id <id> | --to <name>) [--submit] [--yes] [--from <sender>] -- <text …>
```

- **Reads:** `state/stage/sessions.json` (resolves the target's `socket` and
  `parentSessionId`, and — for the sibling rule — the SENDER's own record from
  the same already-loaded file); env `AOIDE_SESSION_ID` (the provenance
  fallback only — the gate rules never read it; the sender's session is
  resolved kernel-side, below)
  and `AOIDE_CONDUCT_AUTOGATE` in {1,true,yes,all} (the box-wide
  orchestration-mode switch) and `AOIDE_CONDUCT_SIBLING_AUTOGATE` in
  {0,false,no} (the sibling-rule opt-out).
- **Writes:** pending path — appends `{sessionId, text, submit, queuedAt,
  from?}` to `state/stage/pending.json` (atomic, under the stage lock; `from`
  carries the resolved sender attribution when one resolves, omitted
  otherwise). Delivered path — opens the target's control socket
  (`$XDG_RUNTIME_DIR/aoide/session-<id>.sock`) and writes `<text>` (+ the
  target's own submit keystroke on `--submit`, resolved at delivery time from
  the target session's agent profile — `\n` for claude and pi, `\r` for
  kimi), prefixed with `from <sender>: ` on its first line when a sender
  resolves AND the text names the node (below); then auto-renames the node
  (`title` in `sessions.json` + re-stage) to a one-line ≤60-char form of the
  UNPREFIXED text — UNLESS the text carries no letter at all (a bare keystroke
  answer like `1`), which leaves the title alone AND skips the provenance
  prefix (a `session permit` verdict digit must land byte-exact, not `from
  orch-1: 1`). EVERY outcome appends one audit record (`class: "audit"`,
  `command: "send"`, status `pending|delivered|error`, the unprefixed
  text as `untrusted_data` — never the message — and the resolved sender, if
  any, folded into the message) to the audit log (`$AOIDE_ROOT/log` by
  default).
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
  pending. The sender behind both autogate rules is kernel-attested
  (`sender_is_parent`/`siblings_share_live_parent`): `send` walks its OWN
  real `/proc` ancestry — as unforgeable a kernel fact for the real process
  as a peercred read of it — to a live session whose seal verifies against
  the daemon's current `ping`-fetched key (the sealed session credential,
  [[Session-Graph]]); no seal-verified ancestor means no autogate rule can
  fire and the send parks. The target's control socket adds its own
  accept-time check: `SO_PEERCRED` on every connection, refusing outright
  the one self-injection shape (the connector's nearest live registered
  session IS the socket's own session; an unresolvable connector fails
  open — a loop defense, not the boundary). A same-uid process bypassing
  `send` and connecting raw still injects ungated — a named open item of
  the identity lane ([[Session-Graph]]'s accounting).
  `--from` sets the delivered/queued provenance attribution
  explicitly (sanitized of embedded `\n`/`\r`); omitted, it falls back to
  `AOIDE_SESSION_ID`; `--from ""` is explicit anonymity and skips that
  fallback — attribution only, never authentication, since either source is a
  same-user value any caller can set to whatever it likes. `aoide session
  pending list|approve|deny` is the read/resolve surface over the held queue
  (below) — `approve` re-drives a held entry through this exact door with
  `--yes` and the entry's own `from`, in-process. `session permit`'s verdict
  and the A2A door both re-enter this same function in-process rather than
  reimplementing it. `--to` resolves a name (local id, tail4, or petname;
  `node/<query>` targets a remote session over A2A) in place of a raw `--id`,
  and the two are mutually exclusive. A remote send is always attempted and
  never queues locally; the receiving node gates its own delivery.

### aoide session pending list

```
aoide session pending list [--json]
```

- **Reads:** `state/stage/pending.json`, raw-JSON — a malformed entry (a bare
  string, or an object missing `sessionId`) is listed with `state: malformed`
  rather than failing the whole read.
- **Output:** text — one line per entry; `data: {pending: [...]}` with each
  entry's `id` (its array POSITION — the schema carries no id of its own, so
  positions shift the moment any entry resolves; re-list between multiple
  resolutions in one breath).
- **Notes:** read-only. The queue `send` parks a held injection in (no
  `--yes`, no autogate match) and where the A2A door parks its own held
  injects.

### aoide session pending approve

```
aoide session pending approve <id> [--json]
```

- **Reads:** `state/stage/pending.json`; `<id>` is the entry's list position.
- **Writes:** re-synthesizes and runs the exact `send --id <sessionId>
  --yes -- <text>` (with `--submit`/`--from` carried through) the held entry
  represents, in-process through `session_send` — the SAME door `session
  permit`'s verdict-typing already goes through — then removes the entry from
  `pending.json` under the stage lock. Resolution is the audit line, not a
  persisted archive.
- **Output:** the inner `send` outcome. A malformed or out-of-range id
  fails cleanly (exit 1), leaving the entry untouched — never destroyed on
  failure.

### aoide session pending deny

```
aoide session pending deny <id> [--json]
```

- **Reads:** `state/stage/pending.json`; `<id>` is the entry's list position.
- **Writes:** removes the entry from `pending.json` under the stage lock —
  injects nothing.
- **Output:** `data: {id, sessionId}`. A malformed or out-of-range id fails
  cleanly, leaving the entry untouched.

### aoide session permit

```
aoide session permit --id <id> [--tool <name>] [--what <text>] [--json]
```

- **Reads:** `state/stage/sessions.json` (record, `conductable`/`socket`,
  `title`, `agent` → profile); the harness's verified permission-prompt keys
  (`aoide_protocol::agents::PermissionKeys` — claude and kimi both carry
  verified keys: approve `1` (once-only; the digit alone chooses AND
  confirms, no trailing submit byte), deny `3`; pi has none and a summons
  stays off for it).
- **Writes:** none directly. Publishes the summons card by sending one
  newline-delimited JSON line `{"cmd": "heraldpush", "notification": {…}}` to
  the shellbridge socket `$XDG_RUNTIME_DIR/aoide/shellbridge.sock`; the daemon
  (single writer) folds it into `state/stage/herald.json`. Card: id
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

### aoide session prune

```
aoide session prune [--json]
```

- **Reads:** `state/stage/{sessions,hooks}.json`.
- **Writes:** drops every `done` session (cascading to its `subagent`
  descendants) and its hook records, and clears `parentSessionId` links left
  dangling; both files + re-staged `graph.json` only when something changed.
- **Output:** `data: {removed: [ids], clearedParents: [ids]}`; nothing to do →
  ok `"nothing to prune"`, both arrays empty.
- **Notes:** idempotent. The `done` record itself is written by
  `session end`/the reaper; prune is the sweeper on its own schedule.

### aoide session reap

```
aoide session reap [--announce] [--json]
```

- **Reads:** `state/stage/{sessions,hooks}.json`; `hyprctl clients -j` (window
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
- **Notes:** the ~12 s systemd timer runs this command unannounced; the dock's
  reap control runs it `--announce` via the shellbridge. A false reap of a
  merely-quiet live session self-heals at its next hook event.

### aoide inbox list

```
aoide inbox list [--all] [--json]
```

- **Reads:** `state/inbox.json` (absent → empty, never an error) — the
  durable, per-host, NOT song-scoped record of every message that actually
  landed in a local session (`pkgs/aoide/crates/storage/src/inbox.rs`,
  registered in that same crate's `commands.rs`, not `conduct`). Filed at
  exactly two sites: `send`'s local-delivery success path (covers a
  direct `--id`, a `--to <local target>`, and `pending approve`'s re-drive)
  and the A2A door's spawn-first-turn path — the receive half of a message
  that landed, however it got there.
- **Output:** unread entries only by default, every entry with `--all`; `id`
  is the entry's position in the full stored array (unlike `pending list`,
  marking an entry read does not remove it, so positions stay stable across
  repeated calls — the one thing that CAN still shift a position is the
  200-entry cap dropping the oldest on a new arrival mid-session).
- **Notes:** not gated.

### aoide inbox read

```
aoide inbox read <n> [--json]
```

- **Reads/writes:** `state/inbox.json`; marks the entry at position `n`
  (as shown by `inbox list`) read, under the same stage lock every
  read-modify-write in this tree uses. A bad or out-of-range `n` fails
  cleanly.
- **Notes:** not gated. Idempotent — re-marking an already-read entry is a
  clean no-op, not an error.

### aoide inbox clear

```
aoide inbox clear [--json]
```

- **Writes:** empties `state/inbox.json`; reports how many entries were
  dropped (`0` on an already-empty inbox — a clean no-op, not an error).
- **Notes:** not gated, unconditional — no `--yes`, matching `session prune`'s
  precedent: the command name is the whole blast radius, nothing selective to
  confirm.

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
  stale); registers the session in `state/stage/sessions.json` with
  `conductable: true`, `socket`, the discovered `windowAddress`, and
  `pid` = the conduct process itself; stamps a write-once `origin` off its
  own inherited `AOIDE_SESSION_ORIGIN` env, refusing a `node:*` shape read
  there (a `node:*` value originates only at the A2A door's spawn —
  [[Session-Graph]]); the daemon's tick sweep seals the record
  (`seal`/`sealedIssuedAt`) within ~1s of registration, since this direct
  write never passes through the dispatch handler. Shell ticks push live
  `cwd`/`activity`/`state`/`needsSudo` (change-only, re-staging `graph.json`);
  `do_session_end` on exit whatever happened. Spawns `<command>` on a fresh
  PTY (`openpty`; `setsid` + `TIOCSCTTY`; slave dup'd over fds 0/1/2) with
  `AOIDE_SESSION_ID` exported, and puts the real tty in raw mode for the
  duration (an RAII guard restores it on every exit path; SIGWINCH resizes are
  propagated to the PTY master) — all of that raw-mode/resize handling is
  interactive-only and skipped entirely under `--headless` (below).
- **Pipes to / output:** a `poll()` multiplexer shuttles real stdin → PTY
  master → real stdout (the wrapped TUI runs undisturbed), and each accepted
  control-socket connection's bytes → PTY master (that is what `send`
  types into). Exit mirrors the child: 0 ok, 1 otherwise, real code in
  `data.exitCode`; envelope `data: {sessionId, agent, exitCode, conductable,
  socket}`.
- **`--headless`:** a pty session with NO controlling terminal — for a caller
  (a script, `spawn`, an orchestrating agent) that has none to give it.
  The multiplexer never pushes a stdin pollfd (there is nothing to read from)
  and the pty-master's output mirrors to an append-only, unrotated per-session
  log file — `state/sessions/<sessionId>.log` (`$AOIDE_STATE_DIR` when
  absolute, else `$AOIDE_ROOT/state/`) — instead of real stdout; the log's
  path is recorded on
  the session record as the additive `logPath` field the moment the file
  opens. Everything else — registration, the injection socket, `send`
  steering, exit mirroring — is identical to the interactive path. With no
  controlling tty to query, `openpty` would otherwise get a NULL winsize and
  leave the pty at 0×0 (full-screen TUIs misrender against or refuse that
  outright), so a headless pty falls back to a conventional 80×24 instead; the
  interactive no-tty case (rare, e.g. redirected stdin in a test) keeps `None`. A log file that can't be opened (an unwritable
  state dir) degrades to stdout rather than killing the session, the same
  best-effort posture as the socket bind. See "Headless conduct & `spawn`"
  under [[Conductor-Channel]].
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
