---
type: concept
created: 2026-07-28
updated: 2026-08-01
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

- **`aoide conduct [--agent A] [--parent P] [--id I] -- <command …>`** — the PTY-backed wrap. Same registration semantics as `graph wrap` (spawn-first, running→done, exit mirror, `AOIDE_SESSION_ID` exported) plus the PTY + control socket + `conductable` flag. `graph wrap` stays as the lightweight observe-only wrapper; `conduct` is the controllable one.
- **`aoide graph send --id <id> [--submit] [--yes] -- <text>`** — the one door both callers use. Connects to the session's control socket and injects `<text>` (+ newline if `--submit`). A central orchestrator agent shells this to steer a child; the conductor calls it under the hood. **Every send routes through the aoided gate + audit log** (below). Sets the target's `title` to the task (auto-rename), and — since aoide owns the PTY — may push an OSC-2 title sequence to the real terminal so the window title (hence the node label) tracks the flow.
- **Title discovery** — a small hyprctl step (`hyprctl clients -j`, match the session's process to a client) populates `windowAddress` (fixing the cue) and reads the live **chat title** Claude already writes to its terminal window, so un-conducted sessions still label correctly.

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

- **Confirm by default.** A send with no standing authorization is recorded as a **pending** injection (atomic stage write) and is NOT delivered until approved. The conductor surfaces pending sends for a one-key approve/deny; the CLI approves with `--yes` (or a `graph send-approve` verb).
- **Autogate policy.** Trusted flows auto-approve without a human. Two rules ship today, both still **audited**:
  - **Parent-of-target (SHIPPED).** A send DELIVERS without a prompt when the sender's own `AOIDE_SESSION_ID` equals the target session's `parentSessionId` — i.e. an orchestrator freely commanding a child it spawned. Cross-tree or unrelated sends stay pending. This is what makes conduct-by-default a real orchestration mesh: the parent of a subtree can drive it, strangers cannot. (Decision is a pure, unit-tested predicate: `sender_is_parent(sender, target_parent)`.)
  - **Global switch (SHIPPED).** `AOIDE_CONDUCT_AUTOGATE` in {1,true,yes,all} declares a box-wide orchestration-mode where every send delivers.
  - Gate priority: `--yes` ▸ global switch ▸ parent-of-target ▸ else pending. No richer per-agent rules exist beyond parent-of-target and the global switch.
- **Audit.** Every send (pending, approved, denied, autogated) writes an aoided audit line — the flow of commands through the graph is fully recorded (`~/Aoide/log`).
- Socket is **user-scoped** (`$XDG_RUNTIME_DIR`, no network). Forwarded/echoed agent text is untrusted data and never re-interpreted as a command (existing house rule).

## The conductor becomes the console

**Status:** planned — Phase ③, not yet in `app.rs`; no `InputKind::Send` today.

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
