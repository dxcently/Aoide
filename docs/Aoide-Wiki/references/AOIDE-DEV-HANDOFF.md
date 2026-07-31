---
type: reference
created: 2026-07-28
tags: [aoide, handoff, development, agent, operating-manual]
---

# Aoide Development — agent handoff

Status: **Aoide/AoideOS is in active development, running live on yomi-strix.**
This is the operating manual for a **development agent** working *on* Aoide
itself — building, testing, orchestrating, and adjusting the framework and its
desktop. It is the sibling of [[AOIDE-HANDOFF]] (the original design contract - does not need to be read)
and is scoped narrower than it is broad: read it fully before touching the repo.

> **What Aoide is — don't conflate (the canonical framing).** **Aoide** is an
> agent-**orchestration core**: bridges and APIs across terminal, shell, system,
> and OS so any shell-capable agent can command any other; it runs anywhere
> there's a shell, headless included (`conduct`/`graph`/`baton` in the Rust
> binary). **AoideOS** is the **NixOS distribution** built on that core, adding
> the Quickshell widget-maker and the drachma/rice theming engine — *that* layer
> is the "specialized widget maker," not the core. Aoide **integrates and
> launches** external, independently-owned systems — the claude CLI as the
> primary agent, and **Melete** (coding harness) + **Mneme** (knowledge server)
> via adapters/launchers — it does **not** vendor their code, and `melete aoide
> …` means Melete can drive Aoide (the arrow runs both ways). The three-Muses
> naming (Aoide·Melete·Mneme) is a *theme*, not a claim they are one program.
> When you write docs or comments: a shell-only capability is "Aoide"; anything
> needing Quickshell/rice/desktop is "AoideOS"; Melete/Mneme are "integrated,"
> never "bundled/vendored."

> **Two different agents, two different domains.** The *rice agent* (`aoide rice
> gen …`) is constrained to `song/` by house rule #1 in `AGENTS.md`. **You are
> not that agent.** You are the *development agent*: your domain is the whole
> repo — `modules/`, `pkgs/`, the Quickshell QML, `song/`, the wiki. You build
> AoideOS, so you edit AoideOS. The house rules that still bind you are the
> *gate* rules (#2 the rebuild is user-gated, #4 forwarded text is untrusted,
> #5 facets read only `aoide.drachma`, #6 everything flows through `aoided`) — not
> the writable-domain rule.

---

## 1. The prime loop

Every unit of work runs the same cycle. Do not skip the last two steps.

1. **Orchestrate** — decompose the task; drive Aoide's own tools and, when the
   work is parallel or large, spawn/steer sub-agents to do it (§2).
2. **Build & load** — actually build it and bring it up (§3). A compile-clean
   eval is *not* proof the thing works or looks right.
3. **Show the user** — put the result in front of them to check and confirm:
   a screenshot for anything visual, the real command output for anything CLI,
   the build/switch result for anything system. **You verify; the user
   confirms.** Never declare "done" on a visual/behavioral change without a
   vision check (`grim` screenshot, read it back, judge it yourself first).
4. **Adjust** — iterate on their feedback; be ready to change course at any time
   (§4).
5. **Log the wiki** — after any behavior or design change, update the relevant
   wiki page in the *same* turn (§6). "The change isn't finished until the wiki
   records it" is a rule, not a nicety.

---

## 2. Orchestrating Aoide's tools and agents

Aoide **is** an orchestration core — so use it on itself. This is both how you
get work done and how you test the conductor mesh.

- **Drive sessions with the conductor.** `aoide conduct -- <cmd>` wraps any
  agent; `aoide graph send --id <id> [--submit] [--yes] -- <text>` types into a
  running session. An orchestrator freely commands its own spawned children
  (parent-autogate); everything else is **pending** until `--yes`. Use this to
  exercise the conductor while you build with it — dogfooding is testing.
- **Fan out with sub-agents when it pays — the current tiering** (khoa,
  2026-07-30, superseding the 2026-07-29 shape below): **"Aoide Dev" (the main
  dev session) orchestrates** — it decomposes, dispatches, reviews every diff,
  and lands. **The orchestrator's own model is deliberately unspecified here**
  (khoa, 2026-07-30) — it varies by session and by the harness in use, so this
  spec names the ROLE, never the model. **Coding runs through Melete, on Opus
  5** — the orchestrator does not edit code directly; it dispatches the coding
  task to Melete — the on-box coding agent, run as a **local subagent (`Agent`
  tool), NOT the Melete MCP connector** (khoa, 2026-07-30 — reiterated: MCP is
  not needed here) — and reviews what comes back.
  **Design and review are the higher tier** — Fable and Opus are used for
  designing an approach and reviewing worker output (judgement calls), not for
  mechanical execution. **Wiki maintenance is delegated to a Sonnet 5
  "librarian" agent** (§6) — the orchestrator hands off wiki updates rather
  than writing them itself, so that work stays out of the main context.

  *(Superseded shape, kept for history: 2026-07-29 had the main dev agent
  orchestrating on Opus, with Opus itself coding design-critical work and
  Sonnet generals doing broad search/mechanical sweeps. A 2026-07-30 revision
  pinned the orchestrator to Sonnet 5, then withdrew the pin; a later
  2026-07-30 revision routed coding through Melete on Opus 5. Both are
  superseded by the plan/execute/review pipeline below, same day.)*

  Send independent agents in one batch so they run concurrently; keep the
  *conclusion*, not their file dumps.
- **Plan → execute → review pipeline** (khoa, 2026-07-30 — supersedes both the
  Melete-codes-on-Opus shape and the "review is optional" ruling above).
  Three standing roles, one dispatch each per unit of work: an **Opus 5 agent
  plans** (scopes the change, names files/functions, sequences the steps —
  same job as `Plan` mode/agent, just now a fixed pipeline stage, not an
  occasional detour); a **Sonnet agent executes** the plan — code + CI/CD
  mechanics (build, test, deploy the local loop) — mirroring the plan's steps
  rather than improvising scope; then a **Fable or Opus 5 agent reviews** the
  executor's output before it lands (correctness/coherence read, or a vision
  pass for visual work) — this review step is a standing part of the
  pipeline, not a judgement call to skip. **The orchestrator's job narrows to:
  dispatch each stage, make small edits itself** (typo-class, not
  implementation), **and keep the operations log current** — this file's
  open-flags ledger (§7) plus memory — rather than writing the implementation
  or the review. khoa still reviews visual output himself when he's watching;
  the pipeline's review stage covers the rest.
- **Resume, don't respawn.** An agent that died mid-task on an API error is
  resumed with its context intact (`SendMessage` by id) — a fresh `Agent` call
  starts cold.
- **Test the agents, not just the code.** When you change a hook, a socket, a
  graph verb, or the reaper, prove it end-to-end: spawn a real session, watch it
  register in the DAG, command it, kill it, watch the reaper sweep it. See
  [[Session-Graph]], [[Terminal-Commander]], [[Agent-Hooking]].

---

## 3. Build, load, and show — the concrete mechanics

The verified recipe on yomi-strix (host = `yomi-strix`, user = `khoa`):

```
# eval + build the whole system (validates all nix)
nix build .#nixosConfigurations.yomi-strix.config.system.build.toplevel --no-link --print-out-paths

# activate — USER-GATED (Rebuild-Gate). Only when the user has admitted it.
sudo nix-env -p /nix/var/nix/profiles/system --set <toplevel>
sudo <toplevel>/bin/switch-to-configuration switch

# desktop-only reloads (no full switch needed for QML/hyprctl-live changes)
hyprctl reload                                  # compositor rules / plugins
systemctl --user restart aoide-quickshell.service   # bar / dock / gadgets
aoide rice preview <song>                       # stage drachma.json for hot-reload
qs -p modules/facets/quickshell/qml/shell.qml   # QML load/parse check

# SHOW the user
grim out.png ; grim -g "0,0 1920x60" bar.png    # full + crops → read them back
```

Rules of thumb:

- **The switch is user-gated** ([[Rebuild-Gate]]). You *propose and prepare* the
  build; the user *admits* it. If they've durably authorized you to run switches
  this session, you may — otherwise stop at the built toplevel and hand it over.
- **Prefer the smallest reload that proves the change.** A QML tweak needs a
  quickshell restart, not a full switch; a compositor rule needs `hyprctl
  reload`; a Stylix/base16 or kitty change is nix-baked and needs a rebuild.
- **Nix-baked vs live:** terminal colours, kitty opacity, fonts, window rules
  are baked at build → rebuild to see them. Note values staged via `rice
  preview` hot-reload the shell without a rebuild.
- **Always look.** Screenshot, read it back, and judge coherence yourself
  (light/dark polarity, bar↔terminal↔gadget agreement) *before* showing the
  user — that is the [[Ricing-Protocol|Ricing Protocol]] vision check,
  and it applies to any visual change, not just rices.

---

## 4. Flags, uncertainty, and adjusting on the fly

**Flag behavior; ask when uncertain; never guess silently.** If you are unsure
how something is *supposed* to behave — a socket contract, a gate, a rename, a
compositor rule — raise it (flag it) or ask, rather than shipping a guess as
fact. Uncertainty surfaced early is cheap; a wrong assumption baked into a
switch is not.

Keep an **open-flags ledger** — the behaviors you've raised and the questions
still pending a decision. Two disciplines around it:

- **You may be asked to adjust something and *ignore the open flags* for the
  moment** — make a quick change, park the open questions, move fast. Do it —
  **but reopen the flags afterward.** When the interruption is handled,
  re-surface every open flag you set aside so none is silently dropped. The same
  applies to any *behavior toggle you opened for a test* (a temporarily enabled
  option, a bypassed gate, a debug switch): restore it to its prior state once
  the test is done. Leaving a test flag open is a defect.
- **Be ready to adjust at any time.** The user may interrupt mid-turn to change
  direction, revert a choice, or re-scope. Take the new instruction as current,
  fold it in, and continue — then, once it's handled, return to and reopen
  whatever flags the detour set aside.

The loop: *flag → (maybe) park on request → handle → reopen.* Nothing raised is
ever quietly lost.

---

## 5. Managing the repo (development-mode git)

Aoide is **in active development**, not a released, branch-protected product —
so **branches matter less** here than the usual discipline implies. Small,
coherent, verified changes can land directly.

- **Ask before branching.** If a change looks large, risky, long-running, or
  likely to churn shared files under other agents, **ask the user whether it
  wants its own branch** rather than assuming either way. Default to landing
  directly; escalate to a branch on judgment + their say-so.
- **Shared-worktree discipline.** Other agents may hold the same index. Commit
  with **explicit pathspecs** (`git add -- <your files>`), never `git add -A`;
  never `git reset` a shared index. Leave files another agent is mid-editing
  alone (check `git status` first).
- **Commit trailers** (house standard):
  ```
  Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
  Claude-Session: <the session URL>
  ```
- **Never commit** `.claude/settings.json` (classifier-gated — the user commits
  it by hand), `node_modules`, or the live stage (`song/stage/*` is the user's
  live desktop state — minimal, atomic writes only, not a commit target).
- **Gates before landing:** `nix build .#<pkg>` for touched packages, `nixfmt
  --check` on changed nix, a toplevel eval, and the relevant `aoide … --json`
  smoke check. Stage new files before a flake eval — it can't see untracked
  paths.

---

## 6. Adjusting the wiki (do this every time) — delegated to the librarian

The wiki is part of the deliverable, not documentation-after-the-fact. **The
orchestrator does not write these updates itself** (khoa, 2026-07-30): wiki
maintenance is delegated to a standing **Sonnet 5 "librarian" agent**, so wiki
work stays out of the orchestrator's own context. The librarian's mandate: keep
every page stating **what currently IS the case** — present-indicative, matching
the live repo/system — never a plan or a future intent (that is what open flags
and this handoff are for). It can be handed a broad, standing brief ("bring the
wiki current against the repo") and left to run in the background rather than
scoped to one page per call; the orchestrator dispatches to it whenever a
behavior/design change needs the wiki updated, and moves on without waiting on
or reviewing every line itself.

After any behavior/design change, delegate the following to the librarian:

- Update the page that owns the concept — e.g. a conductor change →
  [[Conductor-Channel]]; a graph/session change → [[Session-Graph]]; a new
  gadget → [[Widget-Maker]] / [[Gadget-Dock]].
- **Rice design goes to the songbook, not the wiki** (khoa, 2026-07-29): any
  design decision about a rice — a key, an opacity, a surface element — lands
  in that song's `song/songbook/<name>/design/` (sonata:
  `design/intent.md`); cross-cutting house grammar in the default rice's
  `song/songbook/default/design/pantheon.md`. The wiki keeps only protocol
  ([[Ricing-Protocol|Ricing Protocol]], under `concepts/song/`) — a protocol
  change still edits the wiki page.
- Follow [[Wiki-Protocol]] / `SCHEMA.md`: `[[Wikilinks]]`, frontmatter, house
  voice. The wiki is small — **read the whole wiki** when in doubt about where a
  fact belongs.
- Record the *why*, not just the *what* — the reasoning that a future agent
  can't reconstruct from the diff (e.g. "brightness is opacity, not colour").

---

## 7. Open flags — live ledger

Per §4, flags raised but not yet closed live HERE so the next agent inherits
them (not in one agent's head). Close a flag by resolving it AND editing this
list; add one the moment you raise it. Current open flags (2026-07-30):

- **[decision · khoa] App-launch exec discipline — `execute()` vs.
  `aoided`.** The new [[Quickshell]] launcher (surface #3, built 2026-07-28)
  launches apps with `DesktopEntry.execute()` — the Quickshell-native
  side-effect idiom already used across the shell (`WorkspaceRow.activate()`,
  `BatonGadget` → `execDetached`), **not** a shell-out invented in QML. House
  rule #6 ("everything flows through `aoided`") would instead route an
  app-launch verb through the bridge, but **no such verb exists** and adding one
  buys nothing over `execute()` while re-introducing a shell-exec in the daemon.
  Chosen: `execute()`. **Open only as a contract question** — if khoa wants *all*
  side effects funnelled through `aoided`, that's a deliberate ruling to make;
  otherwise this closes as-is. Also fixed in passing: `SUPER+SPACE` was bound to
  the unimplemented `aoide shell launcher toggle` CLI stub → now `bind = …,
  global, aoide:launcher` (in-process Hyprland global shortcut). (Note the
  sibling `SUPER+ESCAPE → aoide shell lock` is still an unimplemented `aoide
  shell *` stub — untouched, worth its own pass.) Lands on the next gated
  `switch`. See [[Quickshell]].
- **[cleanup · khoa — songbook consolidation LANDED 2026-07-28/29, one residual
  open] `lib/checks.nix` carries pre-existing nixfmt-1.4.0 drift** (unrelated
  to the songbook restructure that surfaced it) — a formatting-only pass is
  owed. See [[Self-Ricing]].
- **[bug · follow-up] `aoide rice preview <name>` derives the cover by
  song-name convention** (`song/covers/<name>.*` as of 2026-07-29) instead of
  reading `aoide.drachma.wallpaper`. Mitigated live by the
  `AOIDE_WALLPAPER` env baked into the quickshell service, so the wallpaper
  survives rebuilds; a proper fix (preview reads the song's wallpaper note) is
  still owed. See [[Self-Ricing]].
- **[feature · flagged 2026-07-29 by khoa — PARTIALLY RESOLVED 2026-07-30]
  A comprehensive wallpaper switcher, quickshell-native.** The switcher
  surface has since shipped: `AoideWallpaperPicker.qml` (a `GadgetFrame`-hosted
  thumbnail grid over `song/covers/`, keyboard nav, `SUPER+W` → the
  `aoide:wallpaper` global shortcut in `hyprland.nix`) writes `stage/cover.json`,
  which `AoideWallpaper.qml` still renders/hot-swaps as before; `aoide cover
  set <path-or-name>` (real, not stubbed) drives the same seam from the CLI.
  **Still owed**, unchanged from the original ask: `list`/`next` verbs (only
  `set` exists); transition treatment (still a hard `source` swap in
  `AoideWallpaper.qml`, no crossfade `Behavior`); per-monitor selection once
  multi-output lands. Related residual (unchanged): the `qt6.qtimageformats`
  plugin-path export bounds the picker's/`AoideWallpaper`'s format support —
  keep it when touching the service. Open Thread `rice preview --gallery`
  still folds into this. **Open** — narrowed, not closed.
- **[bug · reported 2026-07-29 by khoa] Sessions appear to untrack after a
  rebuild.** Symptom: previously-tracked agent sessions stop showing as
  tracked (roster/DAG/✎N) after a `nixos-rebuild` switch. Not yet diagnosed.
  Seams to check, in likely order: (a) the switch restarts `shellbridge` /
  `aoide-graph-reap` — does a restarted reaper sweep live sessions whose
  hook stream went quiet during the restart window? (b) hook events during
  the restart are lost (socket down) so `sessions.json` entries go stale and
  the reaper later prunes them as dead; (c) the units' PATH/env after
  restart (the hyprctl-on-unit-PATH class of failure — window→session
  matching dying silently as `focus-failed`); (d) stage files themselves
  survive (gitignored, not store-managed), so if the data is intact the loss
  is in matching/reaping, not storage. Repro: track a session, run a switch,
  compare `stage/sessions.json` + `aoide graph emit` before/after. **Open.**
- **[bug · found 2026-07-28 wiki sweep] Melete adapter subscribes to nothing.**
  `modules/nucleus/melete-adapter.nix` sets
  `AOIDE_ADAPTER_SUBSCRIBE=rebuild-proposed,rice-preview-ready,notification-action`,
  but `pkgs/aoide/src/adapter.rs::parse_class()` only accepts
  `audit/gate/rice/content/notification` — every configured name is silently
  dropped, so the default-deny allow-list is effectively empty and the adapter
  forwards nothing. Fix: reconcile the env values to the parser's class
  vocabulary (or widen the parser). See [[Melete]]. **Open.**
- **[bug · found 2026-07-28 wiki sweep] `lib/vmTest.nix` command-count assertion
  is stale.** It asserts `cmd_count == 28`; the CLI now exposes **36** commands
  (`aoide schema --json`), so the vm-boot check can fail against its own tree.
  Bump the assertion or make it non-brittle. See [[Codebase]]. **Open.**
- **[docs · found 2026-07-28] `README.md` (repo root, not in the wiki) lags the
  swept wiki.** Dendrite roster was corrected 2026-07-28 (20, incl.
  `hyprland`); still lagging: shipped song is `sonata` not `hero`; launcher
  trigger is `aoide:launcher` not `aoide shell launcher toggle`; command count
  is 36 (three gated) not 28; `rice preview` is real; `aoide.rebuild` is
  described as shippable but has no `options.nix` option (it's planned).
  **Also now stale (found 2026-07-30):** `README.md:65` still describes a
  clock→`CalendarGadget` popup, deleted this session (see the legacy-widgets
  entry above). Reconcile alongside any songbook-touching pass.
- **[limitation · known] The window→session listener can't resolve a hook-only
  session that has no recorded pid** — it walks the session's pid, and a
  Claude session registered purely via hooks (never conducted) has none. The
  hook-time backfill is retained as the fallback for exactly this case; don't
  remove it. See [[Terminal-Commander]], [[Session-Graph]].
- **[decision · khoa] "Conductor" CLI naming scope.** A `conductor`
  command/alias + brand was proposed (keeping the `aoide` binary/namespace);
  scope (alias/brand vs hard rename) was never confirmed. **Open** — not
  implemented. See [[Conductor-Channel]].
- **[planned · khoa] Dark `moonlight-sonata` song** — the dark counterpart to
  the light `sonata` key, to be added later (khoa: "after merge and rebuild").
- **[planned · future] Default song with Pantheon thematics** — the shipped
  `default` baseline to be composed with the Pantheon grammar. The grammar
  DOC already lives with the default rice
  (`song/songbook/default/design/pantheon.md`, 2026-07-29); composing
  `default`'s actual palette/rice to wear it is the part still open. See
  [[Self-Ricing]].
- **[planned · khoa 2026-07-30 — kept OFF the wiki, flag only]
  External-Edit-Tracking.** Catch a human's direct file edits made via an
  editor (`nvim`/`vim`/…) inside a conducted shell, using git, and report back
  to the orchestrating session that a file changed outside its own `Edit`/
  `Write` tool calls. **Not implemented** — khoa's steer: an unbuilt feature
  lives in this ledger, not as a wiki concept page (a wiki page states what
  currently IS; §6). Design summary, so nothing is lost: `conduct`'s existing
  PTY tick already recognizes an editor foreground process
  (`EDITOR_BASENAMES`/`friendly_editor_command`, `graph.rs`) — snapshot `git
  status --porcelain` when the editor becomes foreground (only when the cwd
  resolves to a repo) and again when it drops back to the bare shell prompt;
  diff the two snapshots to the paths that became newly modified/added during
  that window (not every dirty path in the repo), riding a `git diff --stat`
  alongside. A new stage file, `song/stage/edits.json`, one record per
  detected edit (`sessionId`, `parentSessionId`, `repoRoot`/`cwd`, `editor`,
  `paths`, `diffstat`, `startedAt`/`endedAt`), same atomic write-temp-rename
  contract as the other stage files. v1 CLI, pull-model: `aoide graph edits
  [--session <id>] [--since <ts>] [--json]` to list, `aoide graph edits ack
  --id <editId>` to acknowledge + prune. **Still undecided:** how the
  orchestrator learns without polling — Claude Code's hook surface is
  session→bridge only, so nothing pushes bridge→session mid-turn today. Two
  candidates on record, neither chosen: (a) PTY injection (`graph send`-style)
  to type a notice ahead of the orchestrator's next prompt read — real risk:
  collides with a human mid-keystroke in that same terminal; (b) a visual
  badge on the session's Conductor/Terminals row, extending the `say`/
  `activity` field pattern — sidesteps the collision risk, costs depending on
  a human noticing. Scope is git-repos-only in v1 (khoa: "it can use git for
  this" — no mtime-scanning fallback). See [[Conductor-Channel]],
  [[Session-Graph]], [[Widget-Bridge-Contract]].
- **[flagged · khoa 2026-07-30] Session continuity across a shellbridge
  restart — preference recorded, not designed.** Every gated `switch` restarts
  `shellbridge.service`, which wipes its `RuntimeDirectory`
  (`/run/user/1000/aoide/`) and therefore every live `conduct` process's
  injection socket, even though the `conduct` process itself is untouched and
  keeps running. khoa considered making the socket/runtime path itself immune
  to the restart and called it too risky — fighting the unit's own restart
  semantics rather than working with them. **Preferred direction instead:** a
  conducted session should be able to **detect its socket is gone and
  stop/resume** — notice the door closed and reconnect/re-open once
  shellbridge comes back — rather than the daemon trying to preserve
  continuity across its own restart. Not designed — this is a direction, not
  a spec. Open. See [[Conductor-Channel]].
- **[gap · found in review 2026-07-30] `reconcile_untracked_terminals` has no
  transient-read grace, unlike the reaper.** `sync_untracked_terminal_windows`
  (`pkgs/aoide/src/graph.rs:3328`) only short-circuits when `hyprctl_clients()`
  returns `None` (compositor genuinely unavailable) — but a call that
  *succeeds* with an empty client list (an IPC hiccup, not a real "zero
  windows open") looks identical to "every terminal closed." Because
  `reconcile_untracked_terminals` (`graph.rs:3174`) builds its `desired` set
  purely from that one snapshot, a transient empty read drops every synthetic
  `win:*` terminal record in that pass, then recreates them fresh
  (`..Default::default()`) on the next good tick. The reaper already guards
  the equivalent case for its own liveness sweep (`reap.rs:214`,
  `effective_live_addresses`/`is_recent`: a degenerate empty snapshot falls
  back to pid-only liveness) — this function has no analogous fallback.
  Effect: a visible flicker (Terminals rows vanish/reappear) plus a wasted
  double stage-write/restage, and — relevant to the External-Edit-Tracking
  flag above, which would key in-progress edit state to a shell's sessionId —
  any per-session state not re-derivable from the live window would be
  silently dropped on a false-negative empty read, indistinguishable from a
  real window close. Fix shape: mirror the reaper's grace (don't trust an
  empty `windows` list unless it persists past a short window, or diff
  against the previous snapshot rather than trusting one read). Not fixed.
  Open. See [[Terminal-Commander]].
- **[cleanup · khoa 2026-07-30, audit done] Legacy widget audit — one open
  thread.** `AoideJournal.qml` (dead fold-out journal alternative to the codex
  dock) was confirmed dead and deleted; nothing else orphaned. **Open:** if
  the fold-out journal idea is still wanted, it needs a fresh build against
  current `ConductorGadget`/`TerminalsGadget`/`MetersGadget`/`PowerVitalsGadget`
  — not a resurrection. Also open, unrelated: the `Γ`/`dag.trace` order-mark
  unused in `GadgetFrame`'s map, and the `aoide.surfaces.sessionGraph` registry
  entry with no QML body. See [[Gadget-Dock]].
- **[cleanup · khoa 2026-07-30, second audit done — SUPERSEDED same day]
  Legacy widget audit #2 (non-dock surfaces).** Survey found nothing dead but
  named four surfaces (`AoideGreeter`, `AoideLockscreen`, `AoideOsd`,
  `AoideNotifications`) as unbuilt skeleton/stub chrome. khoa's decision after
  seeing this: delete them rather than leave them dormant — see the very next
  entry below, which is what actually happened. `AoideWallpaperPicker`/
  `AoideLauncher` (also named in that survey) were NOT touched — they're live,
  not stubs.
- **[cleanup · khoa 2026-07-30, LANDED commit `5924c89`] Stripped legacy
  flavor widgets; per-song replacement is planned, not built.** Deleted
  `AoideGreeter.qml`, `AoideLockscreen.qml`, `AoideOsd.qml`,
  `AoideNotifications.qml`, `NotificationCard.qml`, `CalendarGadget.qml`,
  `NowPlayingGadget.qml` from `modules/facets/quickshell/qml/` — song-blind
  stubs/chrome with no per-song identity, cleared by khoa's explicit call to
  make room for a real per-song mechanism rather than leave them dormant.
  Wiring removed from `shell.qml` (the four overlay instantiations) and
  `AoideBar.qml` (the now-playing/calendar `BarPopout`s, plus the `TrayCell`
  component and `openGadget`/`toggleGadget`/`calShown`/`npCell` state that
  only existed to open them) — independently reviewed clean (no dangling
  refs, no orphaned state, no layout breakage). `WorkspaceRow`/`shell.qml`
  itself/the launcher/bar/wallpaper/dock all confirmed untouched, per khoa's
  explicit scope. **Open follow-up (found by the same review):** several docs
  now describe deleted components as if live — `README.md:65`,
  `docs/Aoide-Wiki/entities/Quickshell.md:28-29`,
  `docs/Aoide-Wiki/concepts/Codebase.md:261`,
  `docs/Aoide-Wiki/concepts/Full-Architecture.md:180-181`,
  `docs/Aoide-Wiki/ingest/log.md:17` — wiki pages need the Sonnet librarian
  (§6), `README.md` folds into the existing README-lag flag above. **Planned
  replacement, NOT built this pass:** `song/songbook/sonata/design/
  greek-grammar.md` §7 has the full design (consulted an Opus advisor, then
  independently corrected against `pkgs/drachma`'s actual source before
  writing it up) — a fixed six-slot convention
  (`song/songbook/<name>/widgets/<Slot>.qml`), built into the store for every
  song simultaneously so `aoide rice preview <name>` can still hot-swap a
  widget's BODY live, not just its colours (khoa was explicit this must keep
  working, not regress to rebuild-only). See [[Gadget-Dock]], [[Quickshell]].
- **[plan · khoa 2026-07-30, not yet executed] Conductor/Terminals: kill the
  duplicate claude, name rows by cwd, show `say` on terminals.** (The plan
  file `~/.claude/plans/squishy-tickling-puffin.md` has since been reused
  TWICE for later planning sessions — most recently for the legacy-widget
  strip above — and no longer holds this. **This ledger entry is now the
  authoritative summary**, re-derive the full plan from it if picked up; if
  the plan file gets reused again, that's expected — plan-mode files are
  scratch space, not durable storage, this ledger is.) Shape: extract the
  reaper into
  `pkgs/aoide/src/reap.rs` (behavior-preserving move), add
  `superseded_agent_duplicates` to retire same-window agent-record duplicates
  (fixes a live bug — one terminal was showing three "claude" rows, one
  phantom masking `say`), name untitled rows by cwd basename in both gadgets,
  suppress the Conductor's flat `shell → claude` row, and surface `say` on the
  Terminals claude row. Not started — first candidate for the new
  plan→execute→review pipeline (§2). See [[Conductor-Channel]],
  [[Session-Graph]].

---

## 8. Quick reference

| Need | Do |
| --- | --- |
| Build the system | `nix build .#nixosConfigurations.yomi-strix.…toplevel` |
| Activate (gated) | `nix-env --set` + `switch-to-configuration switch` |
| Reload compositor | `hyprctl reload` |
| Reload shell | `systemctl --user restart aoide-quickshell.service` |
| Stage a song live | `aoide rice preview <song>` |
| Check QML loads | `qs -p …/shell.qml` |
| Show the user | `grim` → read the PNG back → judge → send |
| Command a session | `aoide graph send --id <id> [--submit] [--yes] -- <text>` |
| Machine-readable API | `aoide schema --json` |

## Related

- [[AOIDE-HANDOFF]] — the original design contract (what Aoide *is*)
- [[Agent-Interface]] — the CLI/MCP action layer
- [[Ricing-Protocol]] — the vision-check discipline
- [[Rebuild-Gate]] — why the switch is user-gated
- [[Conductor-Channel]] · [[Session-Graph]] · [[Terminal-Commander]] — the
  orchestration surfaces you both use and test
- [[Codebase]] — how the built repo actually works
