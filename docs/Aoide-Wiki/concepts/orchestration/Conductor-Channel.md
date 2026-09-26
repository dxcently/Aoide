---
type: concept
created: 2026-07-28
updated: 2026-08-27
tags: [aoide, agent, orchestration, conductor, pty, ipc]
source: "[[references/AOIDE-HANDOFF]]"
---

# Conductor Channel — commanding wrapped agents

**Status:** core channel, discovery, and conduct-by-default are live. The
interactive conductor console (Phase ③, an `InputKind::Send` key arm in the
TUI) is the remaining phase — see "Phases" below.

The graph *observes* agents (running · waiting · blocked · done) and can
*jump* to their windows. The conductor channel adds the missing command: a
central controller sends commands INTO a running wrapped agent, and the
[[Conductor-3D-DAG|conductor DAG]] is the interactive surface for it. Spawn a
claude CLI under the conductor and it appears in the graph as a *conductable*
node whose label is its chat title. The central agent, or a human in the
conductor, can then speak into it. Sending a command writes the title onto
the node, so the DAG reads as a live map of who is doing what.

Every terminal runs conducted by default, so it is a tracked, conductable
session out of the box — no opt-in. The parent-autogate rule lets an
orchestrator freely command the children it spawned; sibling sessions
(sharing a live parent) autogate each other too; and the reciprocal holds as
well — a parent automatically hears the children it spawned, as the daemon's
own one-line report off the child's trace, never a prompted send. Headless conduct
(`--headless`, no controlling terminal at all), `spawn` (the detached
launch command), and sender provenance (a delivered payload carries a `from
<sender>: ` prefix) round out the no-terminal case without changing the core
channel's shape: one PTY, one control socket, one gated door per session.

## Why this layer is new (substrate facts)

- A bare spawn with **inherited stdio** only `wait()`s on the child — it
  holds the `Child` but captures no stdin, so there is no channel to type
  into the agent. That gap is why registration alone (the observe-only
  doors) was never enough to steer anything.
- **shellbridge's socket is a separate, narrower channel**
  (`lyra shellbridge --run`): a `{cmd:"focuswindow",address}` line drives
  `hyprctl dispatch focuswindow`, so the dock/roster row-click jumps end to
  end through the daemon socket — window-jump only, not injection. Conduct's
  per-session PTY sockets are the standalone injection path. See
  [[shellbridge]].
- **`libc` 0.2.189 is already vendored** (via crossterm's tree) —
  `openpty`/`forkpty` are ungated, so a PTY is a one-line dep addition of an
  already-locked crate.
- The conductor already has per-panel key arms, an inline text-input mode
  with dispatch-on-Enter (the `L` link handler, `app.rs:652-664,724-786`),
  and one audited `Door::Cli` dispatch seam (`app.rs:522-534`).
- `session_wrap`/`session_conduct` pass `windowAddress: None` at spawn time
  (`graph.rs`) — the cue path only gets an address later, from the event
  listener or hook backfill (see [[Terminal-Commander]]).

## Architecture — the PTY lives in the wrap process

Each conducted session owns **its own PTY and its own control socket** (no
central registry, no single point of failure):

```
 aoide conduct -- claude
   │  openpty(); spawn child on the slave with setsid + TIOCSCTTY
   │  raw-mode the real tty; shuttle both ways:
   │     real stdin  ─► pty master   (you type normally)
   │     pty master  ─► real stdout  (you read normally)
   │  bind  $XDG_RUNTIME_DIR/aoide/session-<id>.sock
   │     socket bytes ─► pty master  (INJECTION — typed into the agent)
   │  SIGWINCH ─► TIOCSWINSZ (resize passthrough)
   │  on exit: unlink socket, do_session_end, mirror child exit code
   ▼
 sessions.json record gains (all additive, v0 round-tripped):
   conductable: true
   socket:      "…/session-<id>.sock"
   title:       "<chat title / current task>"
   origin:      "local"-class | "node:<name>"  (write-once; node:* only from the A2A door)
   seal + sealedIssuedAt:  the daemon-sealed session credential (see [[Session-Graph]])
```

The wrap process is the natural owner: it already holds the `Child`, already
threads `AOIDE_SESSION_ID`, already blocks on `wait()`.

## The commands

- **`aoide conduct [--agent A] [--parent P] [--id I] [--headless] -- <command
  …>`** — the PTY-backed wrap: spawn-first, running→done, exit mirror,
  `AOIDE_SESSION_ID` exported, plus the PTY + control socket +
  `conductable` flag. `conduct` is the one wrapper — `graph wrap`, a
  lightweight observe-only variant with no PTY, is deleted (zero callers).
  `--headless` (below) drops the requirement that the caller have a terminal
  at all; `spawn` is the detached launcher that re-execs `conduct
  --headless` and returns immediately.
- **`aoide send --id <id> [--submit] [--yes] [--from <sender>] --
  <text>`** — the one door both callers use. Connects to the session's
  control socket and injects `<text>`. `--submit` appends the target
  harness's own submit keystroke, resolved at delivery time from the
  target's agent profile: `\n` for claude and pi, `\r` for kimi (its TUI
  submits on carriage return). Every send routes through the aoided gate +
  audit log (below); a delivered send sets the target's `title` to the task
  (auto-rename) and, since aoide owns the PTY, may push an OSC-2 title
  sequence to the real terminal so the window title tracks the flow. `--to
  <name>` resolves a local id/tail4/petname or a `node/<query>` instead of a
  raw `--id`; a node-resolved send always attempts delivery over A2A rather
  than queuing locally — see [[Node-Federation]].
- **Title discovery** — a small hyprctl step (`hyprctl clients -j`, match the
  session's process to a client) populates `windowAddress` and reads the
  live chat title claude already writes to its terminal window, so
  un-conducted sessions still label correctly.

## Headless conduct & `spawn` — no terminal required

Conduct's PTY is the injection surface. Nothing about it requires a real
controlling tty, raw-moded and resized by SIGWINCH: a caller need not be
sitting at a terminal — or be a terminal at all — to launch `conduct`.

- **`aoide conduct --headless [--agent A] [--parent P] [--id I] --
  <command …>`** — the identical PTY-backed session (same registration,
  same control socket, same `send` steering) with NO controlling
  terminal. The multiplexer never reads stdin — nothing to read it from —
  and the pty-master's output mirrors to an append-only, unrotated
  per-session log file: `state/sessions/<sessionId>.log`
  (`$AOIDE_STATE_DIR` when set to an absolute path, else `$AOIDE_ROOT/state/`). The log's path is recorded on
  the session record as the additive `logPath` field (see [[Session-Graph]])
  the instant the file opens, so a caller can tail it to watch the headless
  session think — the conductor is one such reader: Enter on a headless
  session there opens a stripped in-TUI log tail instead of cueing a window.
  Without a controlling tty, `openpty` would otherwise hand the pty a NULL
  winsize (0×0 rows/cols, a geometry full-screen TUIs misrender against or
  refuse outright), so a headless pty falls back to a conventional 80×24
  instead. Everything else — injection socket, exit mirroring,
  `AOIDE_SESSION_ID` export — is byte-identical to the interactive path.
- **`aoide spawn [--agent A] [--parent P] [--id I] [--prompt T] --
  <command …>`** — the DETACHED command over headless conduct. Where
  `conduct`/`wrap` block the calling process until the wrapped agent exits,
  `spawn` re-execs the running `aoide` binary as `conduct --headless … --
  <command …>`, detaches it into its own session (`setsid`, stdio nulled) so
  it outlives this call, and returns as soon as either the child registers
  or a ~3s budget elapses. Registration is proven by a LIVE control-socket
  connect (`UnixStream::connect`, not bare file existence) — a socket FILE
  left behind by a prior, SIGKILLed session with the same `--id` would
  otherwise read as "already registered" before the new process has even
  forked. A failed exec inside the re-exec'd `conduct` registers no ghost
  session (spawn-first, same as `wrap`/`conduct`): the socket simply never
  appears and `spawn` reports `registered: false` honestly. An optional
  `--prompt` is injected only AFTER registration succeeds, through the one
  gated injection door (`send --yes --submit`, in-process) — never a
  direct socket write — the same re-drive shape `session pending approve`
  uses to replay a held entry.

Proven across all three registered agent profiles ([[Agent-Hooking]]): a
headless `claude` answered a prompt into its log with its own
hook-registered session nested as a child of the wrapper session; a headless
`kimi` did the same, its harness session likewise hooked in — the hook door
is harness-agnostic, not claude-specific, since neither harness knows or
cares whether its own stdin is a real tty. A headless `pi` launches and
hooks in the same way, but its provider requests time out (3 retries, 3
failures) — a gap in pi's provider-network path, not in the launch or
hook-registration mechanism.

## Conduct-by-default — every terminal is a conducted session

Aoide is the orchestration core, and "every terminal conductable + tracked
by default" is the default state of the box. It is delivered entirely in the
terminal dendrite — no daemon, no background process.

- **The kitty wrapper.** `modules/dendrites/kitty.nix` defines an
  `aoide-shell` wrapper (`pkgs.writeShellScriptBin`) and points kitty's
  `shell` setting at it, so kitty launches every window's login shell
  THROUGH the wrapper. The wrapper's logic, in order (each branch ends in
  `exec`, so it can never strand the user):
  1. `AOIDE_NO_CONDUCT` set → `exec` the plain login shell (the escape hatch).
  2. `aoide` not on PATH (`command -v aoide` fails) → `exec` the plain login
     shell (never leave the user shell-less if the CLI is broken/absent).
  3. otherwise → `exec aoide conduct --agent shell [--parent
     $AOIDE_SESSION_ID] -- "$login_shell" -l`. It always wraps — each
     terminal is its own session.
  4. belt-and-suspenders → if the conduct exec somehow returns, fall through
     to `exec` the plain login shell anyway.

  The login shell is resolved from `$SHELL`, then passwd (`getent`), then
  `/bin/sh`. Every failure mode degrades to the plain login shell — a broken
  conduct must never make terminals unusable.
- **Parent-nesting builds the DAG tree.** `aoide conduct` exports
  `AOIDE_SESSION_ID` into the shell it runs. When a nested terminal is
  spawned from inside a conducted shell (a child process inheriting that
  env), the wrapper passes `--parent "$AOIDE_SESSION_ID"`, so the new
  session hangs under its spawner in the graph. Sibling kitty windows (which
  inherit kitty's own env, not a shell's) simply become their own root
  sessions — correct either way. A hook-registered session with no explicit
  parent resolves one automatically via `hookAncestry` (see
  [[Session-Graph]]).
- **Double-PTY layering.** kitty allocates a PTY for the window; `conduct`
  allocates a SECOND PTY for the login shell and shuttles bytes between them
  (kitty-pty ↔ conduct's stdin/stdout ↔ conduct-pty master ↔ the shell).
  Raw-mode, SIGWINCH→TIOCSWINSZ resize passthrough, and Ctrl-C-as-a-byte all
  flow through the inner pty. This nesting is the one thing to smoke-test
  live before a rebuild.

## The gate (house rule: one gate, one audit log)

Injecting keystrokes into an agent is privileged, so `send` is gated
through [[aoided]] — the same one policy surface every operation flows
through.

- **Confirm by default.** A send with no standing authorization is recorded
  as a pending injection (atomic stage write to `state/stage/pending.json`)
  and is NOT delivered until approved. The CLI approves inline with `--yes`.
  `aoide session pending list | approve | deny` is the standing read/resolve
  surface over the held queue: `list` enumerates held entries (id = array
  position; malformed entries surface as `state: malformed` rather than
  failing the whole read), `approve <id>` re-drives the entry through this
  exact `send` door (`--yes`, in-process) and removes it, `deny <id>`
  removes it without injecting. This is CLI-only today — the conductor
  TUI's own one-key approve/deny (below) is still Phase ③.
- **Autogate policy.** Four rules ship today, all still audited, in
  priority order: `--yes` ▸ the global switch ▸ parent-of-target ▸
  sibling-of-target ▸ else pending. The fourth is the daemon's own line,
  never a prompted send: child-of-target.
  - **Parent-of-target.** A send delivers without a prompt when the
    sender's own session is the target session's `parentSessionId` — an
    orchestrator freely commanding a child it spawned. Cross-tree or
    unrelated sends stay pending. This is what makes conduct-by-default a
    real orchestration mesh: the parent of a subtree can drive it,
    strangers cannot. The sender's session is kernel-attested, never read
    off a forgeable env var: `aoide send` walks its own real `/proc`
    ancestry to a live session whose seal verifies against the daemon's
    current key (the sealed session credential, [[Session-Graph]]), and the
    decision is a pure, unit-tested predicate over that attested pair:
    `sender_is_parent(sender, target_parent)`. `AOIDE_SESSION_ID` survives
    only as sender attribution (below), removed from every gate predicate.
  - **Sibling-of-target.** A send also delivers without a prompt when
    sender and target are siblings — both parented under the SAME session,
    and that shared parent is itself still live (not `done`). The sender
    side of the pair is the same kernel-attested session the parent rule
    resolves. Sibling
    delivery is ON by default; opt out per-box with
    `AOIDE_CONDUCT_SIBLING_AUTOGATE` set to one of `{0,false,no}` (any other
    value, or unset, leaves it enabled). A self-send is excluded before the
    sibling check ever runs — a session is trivially its own sibling on
    paper, and letting that through would let a prompt-injected send
    deliver text straight back into a session's own input stream, bypassing
    approval. A dead/absent parent or a cross-tree pair stays gated.
  - **Global switch.** `AOIDE_CONDUCT_AUTOGATE` in `{1,true,yes,all}`
    declares a box-wide orchestration mode where every send delivers.
  - **Child-of-target (reciprocal).** The three rules above gate a SEND —
    somebody is trying to speak to a session. This one runs the other way:
    a parent automatically hears the children it spawned, so a child's own
    progress reaches its `parentSessionId` with no prompt and no pending
    entry in between. It is not a send at all. The send door attests the
    SENDER from the running process's own `/proc` ancestry, and inside the
    resident daemon that attested sender is the daemon, never the child — so
    the reciprocal rule could never be `sender_is_parent` made symmetric,
    and it is not implemented as one. It is the daemon's OWN line about the
    parent's child, delivered on the doorbell's path: raw injection into the
    target's transport (a live Claude Code channel socket if one is bound —
    one write, then close — else the control socket with the target's own
    submit keystroke), with no gate, no `pending.json` entry, no provenance
    prefix, no title rename, and no mailbase receipt; each delivery writes
    one audit line with gate label `autogate-child`. The reaper tick is what
    speaks: for each `agent:"eidolon"` child it reads the trace the child's
    harness mirrors beside its journal, picks the highest-priority new event
    since that child's cursor, and renders ONE line —

    ```
    [eidolon <petname>] settled <stop_reason> · <N> calls · <M> min · last: "<say>"
    [eidolon <petname>] cancelled · <N> calls · last: "<say>"
    [eidolon <petname>] died mid-turn · <N> calls · last: "<say>"
    [eidolon <petname>] asking: "<prompt>"
    [eidolon <petname>] wrapping up · <n> calls left      (or · <s> s left)
    [eidolon <petname>] failing · <k> tool errors in a row · last: <tool label>
    [eidolon <petname>] silent <M> min · last: <tool label or say>
    ```

    in that priority order, with a `ToolResult{is_error:true}` among the new
    records riding a higher line as ` · <k> tool errors` and three or more in
    a row standing as their own. Every child-authored fragment — the quoted
    say, prompt and stop reason, and the tool label on the failing and
    silence lines — is untrusted model output: one line, control characters
    stripped, clipped to 80 characters; the quoted ones never allowed to
    start with `/` or `!`. Per-child at-most-once is the
    `state/stage/pingback.json` cursor, claimed before the write. A parent
    that is a bare shell is skipped outright — a line typed into a shell RUNS
    — as is a parent whose record is gone, not conductable, or already
    `done`. Nothing wider than the children it spawned: a stranger's send
    still holds pending. The full statement is `docs/architecture/
    EIDOLON-TRACE.md`'s "Second slice".
  - **Remote-parent — the same relation across machines.** The A2A door's
    rule, not the local socket's, listed here because it answers the same
    question — is the sender this target's parent? — for the one send that
    cannot be asked at the session's own socket: a parent on another box.
    `aoide send --to <node>/<session>` carries its KERNEL-ATTESTED caller as
    the `metadata["aoide/from"]` claim (the same resolution `node spawn`
    stamps into a child's `remoteParent`), and the receiving door delivers
    WITHOUT pending only when all three agree: the request signed with a
    paired node's key this door verified, that key is the target record's
    stored `remoteParent.key`, and the claim equals its stored
    `remoteParent.sessionId`. A claim alone proves nothing — the signature
    rung is what makes it a claim rather than a wish — so an unsigned or
    bearer-only caller with a claim gets one audit line and its send holds
    pending like any stranger's. The hit rides gate label
    `autogate-remote-parent` and does NOT exempt the door-wide bearer, which
    is still checked first. An unruly claim (an id the far door could never
    have stamped) is dropped by the sender — one warning line, `not claiming
    parent: <reason>` — and the send goes out unclaimed rather than failing.
    `CONTRACTS.md` §6 carries the full rule.
- **Sender provenance.** A delivered payload that NAMES the node (carries a
  letter — a real message, not a bare keystroke answer) is prefixed on its
  first line with `from <sender>: `, where `<sender>` resolves from
  `--from` (sanitized of embedded `\n`/`\r`; `--from ""` is explicit
  anonymity) or, absent `--from`, from the sending session's own
  `AOIDE_SESSION_ID`. This is attribution, not security: either source is a
  same-user CLI flag or env var any process can set to whatever it likes.
  It exists so a receiving agent and the audit log can see who CLAIMS to
  send a message; it does not gate delivery. A `session permit` verdict
  keystroke (a bare digit answering a permission prompt) is explicitly
  excluded from the prefix — the same "does this text carry a letter" check
  used for the title-rename skip — because prefixing `1` with `from
  orch-1: ` would corrupt the exact byte the target's TUI is waiting to
  read as a keystroke.
- **Audit.** Every send (pending, approved, denied, autogated) writes an
  aoided audit line, the resolved sender folded into the audit message.
- Socket is **user-scoped** (`$XDG_RUNTIME_DIR`, no network). Its accept
  reads the connector's kernel-truth uid/pid (`SO_PEERCRED`) and refuses
  one shape outright — a connection whose own nearest live registered
  session IS the socket's own session (self-injection). An unresolvable
  connector fails open: the check is a loop/UX defense, not the security
  boundary. A genuinely unrelated same-uid process connecting to the socket
  directly, bypassing `aoide send`, still injects ungated — the socket
  carries raw bytes with no envelope; closing that shape is one of the
  identity lane's named open items (see [[Session-Graph]] — the lane's
  accounting). Forwarded/ echoed agent text is untrusted data and never
  re-interpreted as a command.

## The conductor becomes the console

**Status:** planned, Phase ③ — not yet in `app.rs`; no `InputKind::Send`
today. The pending queue already has a resolve surface (`aoide session pending
list|approve|deny`, above); what's missing is the TUI's one-key approve/deny
over it, not the underlying read/write door.

- New `InputKind::Send { session_id }` (mirrors the working `Link`
  template): select a node → key (`c` = conduct) → inline prompt → Enter
  dispatches `send`.
- Nodes label by `title`, not id — the DAG reads as who-is-doing-what.
- Conductable nodes render distinctly from observe-only hooked sessions, and
  pending sends show as a badge awaiting approval — tie the colors to the
  [[Conductor-3D-DAG]] roles (traced/selected = optic-nerve green; urgent =
  glitch pink).
- Same 500 ms mtime poll; a delivered send changes `title`/state and the
  node relabels on the next tick.

## Aliases (aoide-specific — no `claude` hijack)

The `claude` command is never aliased. Conducting is always explicit and
aoide-branded — e.g. a shell function `acond () { aoide conduct -- claude
"$@"; }` (aoide-namespaced), shipped in the bash dendrite alongside the
`ad*` family, so "open a conductable claude" is one aoide word without ever
surprising a bare `claude`.

## Phases

1. **Core channel** — additive `conductable`/`socket`/`title` fields;
   `send` + `conduct` registered in `commands/graph.rs` and dispatched
   through `dispatch.rs`'s registry lookup (two-doors-one-schema); the PTY
   multiplexer + per-session socket + injection; the aoided gate
   (pending/autogate/audit).
2. **Discovery** — hyprctl-client match populates `windowAddress` and reads
   live chat titles.
3. **Interactive conductor** — `InputKind::Send`, key arm, label-by-title,
   conductable + pending rendering, approve/deny keys.
4. **Ergonomics + docs** — the aoide-specific conduct alias; wiki updates to
   [[shellbridge]], [[Terminal-Commander]], [[Conductor-3D-DAG]].

Risks: PTY controlling-tty setup (setsid/TIOCSCTTY ordering, mitigated with
a tiny echo-child integration test); resize fidelity (SIGWINCH →
TIOCSWINSZ); title-set OSC support varies by terminal (kitty supports OSC 2,
the shipped terminal).

## Related

- [[shellbridge]] — the socket layer the conduct wrap's per-session socket
  stays separate from: the accept loop is live (`focuswindow`), conduct's
  injection path is standalone.
- [[Terminal-Commander]]
- [[Session-Graph]]
- [[Conductor-3D-DAG]]
- [[Agent-Hooking]]
- [[aoided]]
- [[Node-Federation]]
- [[Node-Transport]]
- [[Lexicon]] (conductor-class)
