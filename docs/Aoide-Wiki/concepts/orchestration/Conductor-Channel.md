---
type: concept
created: 2026-07-28
updated: 2026-08-20
tags: [aoide, agent, orchestration, conductor, pty, ipc]
source: "[[references/AOIDE-HANDOFF]]"
---

# Conductor Channel — commanding wrapped agents (build spec)

**Status: SHIPPING (khoa, 2026-07-28). Phases ①–② landed, plus conduct-by-default.**
Core channel (`conduct` + `graph send` + the PTY multiplexer + the aoided gate),
window-address discovery, and — the headline — **conduct-by-default**: every
kitty window runs its login shell under `aoide conduct`, so EVERY terminal is a
tracked, conductable session out of the box (no opt-in). The parent-autogate rule
is live: an orchestrator freely commands the children it spawned. Phase ③ (the
interactive conductor console) is the remaining work. See the "Conduct-by-default"
section below for the kitty wrapper, escape hatch, nesting, and autogate.

The graph today *observes* agents (running · waiting · blocked · done) and can *jump* to their windows. The conductor channel adds the missing verb: **a central controller sends commands INTO a running wrapped agent**, and the [[Conductor-3D-DAG|conductor DAG]] becomes the interactive surface for it. Spawn a claude CLI under the conductor and it appears in the graph as a *conductable* node whose label is its chat title; the central agent (or you, in the conductor) can then speak into it. Orchestrating a command **writes the title** onto the node, so the DAG reads as a live map of who is doing what.

Since Phases ①–②, the *conducted-agents* plan has landed four more pieces, live-proven on the running system: **headless conduct** (`--headless`, no controlling terminal at all — the caller need not be a terminal itself), **`graph spawn`** (the detached verb that launches a headless conducted agent and returns immediately), **sibling autogate** (children of the same live parent may talk to each other, on by default), and **sender provenance** (a delivered payload carries a `from <sender>: ` prefix). None of this changes the core channel's shape — one PTY, one control socket, one gated door per session — it fills in the no-terminal case and widens the gate ladder. See "Headless conduct & `graph spawn`" and the updated "The gate" section below.

## Why this layer is new (substrate facts)

- `graph wrap`'s `session_wrap` spawns with **inherited stdio** and only `wait()`s — it holds the `Child` but captures no stdin, so there is no channel to type into the agent (`graph.rs`).
- **shellbridge's socket is a separate, narrower channel** (`aoide shellbridge --run`): a `{cmd:"focuswindow",address}` line drives `hyprctl dispatch focuswindow`, so the dock/roster row-click jumps end to end through the daemon socket — window-jump only, not injection. Conduct's per-session PTY sockets stay the injection path, built standalone — see [[shellbridge]].
- **`libc` 0.2.189 is already vendored** (via crossterm's tree) — `openpty`/`forkpty` are ungated, so a PTY is a one-line dep addition of an already-locked crate. No new vendored crate.
- The conductor already has per-panel key arms, an inline text-input mode with dispatch-on-Enter (the `L` link handler, `app.rs:652-664,724-786`), and one audited `Door::Cli` dispatch seam (`app.rs:522-534`).
- `session_wrap`/`session_conduct` pass `windowAddress: None` at spawn time (`graph.rs`) — the cue path only gets an address later, from the event listener or hook backfill (see [[Terminal-Commander]]); discovery closes this gap in the same step that reads chat titles.

## Architecture — the PTY lives in the wrap process

Each conducted session owns **its own PTY and its own control socket** (no central registry, no single point of failure):

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
```

The wrap process is the natural owner: it already holds the `Child`, already threads `AOIDE_SESSION_ID`, already blocks on `wait()`.

## The verbs

- **`aoide conduct [--agent A] [--parent P] [--id I] [--headless] -- <command …>`** — the PTY-backed wrap. Same registration semantics as `graph wrap` (spawn-first, running→done, exit mirror, `AOIDE_SESSION_ID` exported) plus the PTY + control socket + `conductable` flag. `graph wrap` stays as the lightweight observe-only wrapper; `conduct` is the controllable one. `--headless` (below) drops the requirement that the caller have a terminal at all.
- **`aoide graph send --id <id> [--submit] [--yes] [--from <sender>] -- <text>`** — the one door both callers use. Connects to the session's control socket and injects `<text>` (+ the target harness's own submit keystroke if `--submit`, resolved at delivery time from the target session's agent profile — `\n` for claude and pi, `\r` for kimi, whose TUI submits on carriage return rather than newline). A central orchestrator agent shells this to steer a child; the conductor calls it under the hood. **Every send routes through the aoided gate + audit log** (below). Sets the target's `title` to the task (auto-rename), and — since aoide owns the PTY — may push an OSC-2 title sequence to the real terminal so the window title (hence the node label) tracks the flow.
- **Title discovery** — a small hyprctl step (`hyprctl clients -j`, match the session's process to a client) populates `windowAddress` (fixing the cue) and reads the live **chat title** Claude already writes to its terminal window, so un-conducted sessions still label correctly.

## Headless conduct & `graph spawn` — no terminal required

Conduct's PTY has always been the injection surface, but until this landed the *interactive side* — a real controlling tty, raw-moded, resized by SIGWINCH — was load-bearing: something had to be sitting at a terminal to launch `conduct` at all. That is no longer true.

- **`aoide conduct --headless [--agent A] [--parent P] [--id I] -- <command …>`** — the identical PTY-backed session (same registration, same control socket, same `graph send` steering) but with NO controlling terminal. The multiplexer never reads stdin (there is nothing to read it from), and the pty-master's output — everything the wrapped agent would otherwise print to a real screen — mirrors to an **append-only, unrotated per-session log file**: `state/sessions/<sessionId>.log` (`$AOIDE_STATE_DIR`, else `~/Aoide/state/`). The log's path is recorded on the session record as the additive `logPath` field (see [[Session-Graph]]) the instant the file opens, so a caller (or another agent) can tail it to watch the headless session think. Without a controlling tty, `openpty` would otherwise hand the pty a NULL winsize (0×0 rows/cols — a geometry full-screen TUIs misrender against or refuse outright), so a headless pty falls back to a conventional **80×24** instead. Everything else — the injection socket, exit mirroring, `AOIDE_SESSION_ID` export — is byte-identical to the interactive path.
- **`aoide graph spawn [--agent A] [--parent P] [--id I] [--prompt T] -- <command …>`** — the DETACHED verb over headless conduct. Where `conduct`/`wrap` block the calling process until the wrapped agent exits, `spawn` re-execs the running `aoide` binary as `conduct --headless … -- <command …>`, detaches it into its own session (`setsid`, stdio nulled) so it **outlives this call**, and returns as soon as either the child registers or a ~3s budget elapses. Registration is proven by a LIVE control-socket connect (`UnixStream::connect`, not bare file existence) — a socket FILE left behind by a prior, SIGKILLed session with the same `--id` would otherwise read as "already registered" before the new process has even forked. A failed exec inside the re-exec'd `conduct` registers **no ghost session** (spawn-first, same as `wrap`/`conduct`): the socket simply never appears and `spawn` reports `registered: false` honestly. An optional `--prompt` is injected only AFTER registration succeeds, through the one gated injection door (`graph send --yes --submit`, in-process) — never a direct socket write — the same re-drive shape `graph pending approve` (below) uses to replay a held entry.

Live-proven 2026-08-20 across all three registered agent profiles ([[Agent-Hooking]]): a headless `claude` answered a prompt into its log with its own hook-registered session nested as a child of the wrapper session; a headless `kimi` did the same, its harness session likewise hooked in — proof the hook door is harness-agnostic, not claude-specific, since neither harness knows or cares whether its own stdin is a real tty. A headless `pi` launches and hooks in the same way, but its provider requests time out (3 retries, 3 failures) — a gap in pi's provider-network path, not in the launch or hook-registration mechanism.

## Conduct-by-default — every terminal is a conducted session (SHIPPED)

The core-identity behaviour: Aoide is the orchestration core, and "every
terminal conductable + tracked by default" is the default state of the box. It is
delivered entirely in the terminal dendrite — no daemon, no background process.

- **The kitty wrapper.** `modules/dendrites/kitty.nix` defines an `aoide-shell`
  wrapper (`pkgs.writeShellScriptBin`) and points kitty's `shell` setting at it,
  so kitty launches every window's login shell THROUGH the wrapper. The wrapper's
  logic, in order (each branch ends in `exec`, so it can never strand the user):
  1. `AOIDE_NO_CONDUCT` set → `exec` the plain login shell (**the escape hatch**).
  2. `aoide` not on PATH (`command -v aoide` fails) → `exec` the plain login shell
     (never leave the user shell-less if the CLI is broken/absent).
  3. otherwise → `exec aoide conduct --agent shell [--parent $AOIDE_SESSION_ID]
     -- "$login_shell" -l`. It ALWAYS wraps — each terminal is its own session.
  4. belt-and-suspenders → if the conduct exec somehow returns, fall through to
     `exec` the plain login shell anyway.
  The login shell is resolved from `$SHELL`, then passwd (`getent`), then `/bin/sh`.
  **Safety is paramount:** a broken conduct must never make terminals unusable, so
  every failure mode degrades to the plain login shell.
- **Parent-nesting builds the DAG tree.** `aoide conduct` exports `AOIDE_SESSION_ID`
  into the shell it runs. When a nested terminal is spawned from inside a conducted
  shell (a child process inheriting that env), the wrapper passes `--parent
  "$AOIDE_SESSION_ID"`, so the new session hangs under its spawner in the graph.
  Sibling kitty windows (which inherit kitty's own env, not a shell's) simply
  become their own root sessions — correct either way.
- **Double-PTY layering.** kitty allocates a PTY for the window; `conduct`
  allocates a SECOND PTY for the login shell and shuttles bytes between them
  (kitty-pty ↔ conduct's stdin/stdout ↔ conduct-pty master ↔ the shell). Raw-mode,
  SIGWINCH→TIOCSWINSZ resize passthrough, and Ctrl-C-as-a-byte all flow through
  the inner pty. This nesting is the one thing to smoke-test live before a rebuild.

## The gate (house rule: one gate, one audit log)

Injecting keystrokes into an agent is privileged, so **`graph send` is gated through [[aoided]]** — the same one policy surface every operation flows through:

- **Confirm by default.** A send with no standing authorization is recorded as a **pending** injection (atomic stage write to `song/stage/pending.json`) and is NOT delivered until approved. The CLI approves inline with `--yes`; **`aoide graph pending list | approve | deny`** (SHIPPED) is the standing read/resolve surface over the held queue — `list` enumerates held entries (id = array position; malformed entries surface as `state: malformed` rather than failing the whole read), `approve <id>` re-drives the entry through this exact `graph send` door (`--yes`, in-process) and removes it, `deny <id>` removes it without injecting. This is CLI-only today — the conductor TUI's own one-key approve/deny (below) is still Phase ③.
- **Autogate policy.** Trusted flows auto-approve without a human. Three rules ship today, all still **audited**:
  - **Parent-of-target (SHIPPED).** A send DELIVERS without a prompt when the sender's own `AOIDE_SESSION_ID` equals the target session's `parentSessionId` — i.e. an orchestrator freely commanding a child it spawned. Cross-tree or unrelated sends stay pending. This is what makes conduct-by-default a real orchestration mesh: the parent of a subtree can drive it, strangers cannot. (Decision is a pure, unit-tested predicate: `sender_is_parent(sender, target_parent)`.)
  - **Sibling-of-target (SHIPPED, 2026-08-20).** A send also DELIVERS without a prompt when the sender and the target are **siblings** — both parented under the SAME session, and that shared parent is itself still live (not `done`). **The User's decision: sibling delivery is ON BY DEFAULT.** Opt out per-box with `AOIDE_CONDUCT_SIBLING_AUTOGATE` set to one of `{0,false,no}`; any other value (or unset) leaves it enabled. A self-send is excluded before the sibling check ever runs — a session is trivially its own sibling on paper (same parent field read twice), and letting that through would let a prompt-injected send deliver text straight back into a session's own input stream, bypassing approval entirely. A dead/absent parent or a cross-tree pair stays gated. Live-proven: a real sibling send delivered with the `autogate-sibling` label, provenance prefix visible in the receiver's log.
  - **Global switch (SHIPPED).** `AOIDE_CONDUCT_AUTOGATE` in {1,true,yes,all} declares a box-wide orchestration-mode where every send delivers.
  - Gate priority: `--yes` ▸ global switch ▸ parent-of-target ▸ sibling-of-target ▸ else pending. No richer per-agent rules exist beyond these three plus the global switch.
- **Sender provenance (SHIPPED, 2026-08-20).** A delivered payload that NAMES the node (carries a letter — a real message, not a bare keystroke answer) is prefixed on its first line with `from <sender>: `, where `<sender>` resolves from `--from` (sanitized of embedded `\n`/`\r`; `--from ""` is explicit anonymity) or, when `--from` is absent, from the sending session's own `AOIDE_SESSION_ID`. **Attribution, not security** — either source is a same-user CLI flag or env var any process can set to whatever it likes; this exists so a receiving agent and the audit log can see who CLAIMS to send a message, not to gate delivery on that claim (the gate above is unaffected). A `graph permit` verdict keystroke (a bare digit answering a permission prompt) is explicitly excluded from the prefix — the same "does this text carry a letter" check `names_the_node` uses for the title-rename skip — because prefixing `1` with `from orch-1: ` would corrupt the exact byte the target's TUI is waiting to read as a keystroke.
- **Audit.** Every send (pending, approved, denied, autogated) writes an aoided audit line — the flow of commands through the graph is fully recorded (`~/Aoide/log`), the resolved sender folded into the audit message.
- Socket is **user-scoped** (`$XDG_RUNTIME_DIR`, no network). Forwarded/echoed agent text is untrusted data and never re-interpreted as a command (existing house rule).

## The conductor becomes the console

**Status:** planned — Phase ③, not yet in `app.rs`; no `InputKind::Send` today. The pending queue itself already has a resolve surface (`aoide graph pending list|approve|deny`, above) — what is still missing is the TUI's one-key approve/deny over it, not the underlying read/write door.

- New `InputKind::Send { session_id }` (mirrors the working `Link` template): select a node → key (`c` = conduct) → inline prompt → Enter dispatches `graph send`. One new key arm + one `InputKind` + the existing dispatch seam.
- **Nodes label by `title`, not id** — the DAG reads as who-is-doing-what.
- **Conductable nodes render distinctly** from observe-only hooked sessions, and **pending sends** show as a badge awaiting approval — tie the colors to the [[Conductor-3D-DAG]] roles (traced/selected = optic-nerve green; urgent = glitch pink).
- Same 500 ms mtime poll; a delivered send changes `title`/state → the node relabels on the next tick.

## Aliases (aoide-specific — no `claude` hijack)

The `claude` command is never aliased. Conducting is always explicit and aoide-branded — e.g. a shell function `acond () { aoide conduct -- claude "$@"; }` (aoide-namespaced), shipped in the bash dendrite alongside the `ad*` family, so "open a conductable claude" is one aoide word without ever surprising a bare `claude`.

## Phases

1. **Core channel** — additive `conductable`/`socket`/`title` fields; `graph send` + `conduct` registered in `commands/graph.rs` and dispatched through `dispatch.rs`'s registry lookup (two-doors-one-schema); the PTY multiplexer + per-session socket + injection; the aoided gate (pending/autogate/audit). Unit-testable: a fake child echoes injected bytes; a send with `--yes` reaches it, a send without is held pending.
2. **Discovery** — hyprctl-client match → populate `windowAddress` + read live chat titles.
3. **Interactive conductor** — `InputKind::Send`, key arm, label-by-title, conductable + pending rendering, approve/deny keys.
4. **Ergonomics + docs** — the aoide-specific conduct alias; wiki updates to [[shellbridge]], [[Terminal-Commander]], [[Conductor-3D-DAG]].

Risks: PTY controlling-tty setup (setsid/TIOCSCTTY ordering) — mitigate with a tiny echo-child integration test; resize fidelity (SIGWINCH → TIOCSWINSZ); title-set OSC support varies by terminal (kitty supports OSC 2 — the shipped terminal).

## Related

- [[shellbridge]] — the socket layer this finally grows a real verb set on: the accept loop is live (`focuswindow`), and the conduct wrap's per-session socket remains the separate, robust injection path.
- [[Terminal-Commander]] · [[Session-Graph]] · [[Conductor-3D-DAG]] · [[Agent-Hooking]] · [[aoided]] · [[Lexicon]] (conductor-class).
