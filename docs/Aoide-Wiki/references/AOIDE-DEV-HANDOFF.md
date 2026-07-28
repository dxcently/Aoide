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
- **Fan out with sub-agents when it pays.** The standing orchestration shape:
  **Opus codes, Sonnet generals (broad search / parallel legwork), the main
  session reviews and lands.** Send independent agents in one batch so they run
  concurrently; keep the *conclusion*, not their file dumps.
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
  user — that is the [[design/Ricing-Protocol|Ricing Protocol]] vision check,
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

## 6. Adjusting the wiki (do this every time)

The wiki is part of the deliverable, not documentation-after-the-fact. After any
behavior/design change:

- Update the page that owns the concept — e.g. a ricing/glass/opacity change →
  [[design/Ricing-Protocol|Ricing Protocol]]; a conductor change →
  [[Conductor-Channel]]; a graph/session change → [[Session-Graph]]; a new
  gadget → [[Widget-Maker]] / [[Gadget-Dock]].
- Follow [[Wiki-Protocol]] / `SCHEMA.md`: `[[Wikilinks]]`, frontmatter, house
  voice. The wiki is small — **read the whole wiki** when in doubt about where a
  fact belongs.
- Record the *why*, not just the *what* — the reasoning that a future agent
  can't reconstruct from the diff (e.g. "brightness is opacity, not colour").

---

## 7. Open flags — live ledger

Per §4, flags raised but not yet closed live HERE so the next agent inherits
them (not in one agent's head). Close a flag by resolving it AND editing this
list; add one the moment you raise it. Current open flags (2026-07-28):

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
- **[incident · RESOLVED 2026-07-28] Shell crash-loop from a leftover
  `ExecStart` drop-in.** A hand-written systemd drop-in
  (`~/.config/systemd/user/aoide-quickshell.service.d/override.conf`), left by a
  live `-c`→`-p` verification on the `worktree-devtools-dendrites` branch,
  pinned the shell's `ExecStart` to that worktree's `shell.qml`. The worktree
  was later deleted, so quickshell crash-looped 77× ("Could not open config
  file") — no bar, dock, gadgets, or wallpaper (the wallpaper *is* the shell:
  one `wlr-layer-shell` process, see [[Quickshell]]). The `-c`→`-p` fix it was
  verifying is already baked into the live unit, so the drop-in was redundant
  *and* broken. **Fixed** by deleting the drop-in and restarting; the baked unit
  points at `~/Aoide/qml/shell.qml` and stands correctly. **Rule that now
  binds all agents:** never override the shell service's `ExecStart` to a
  worktree/volatile path — iterate live with a *separate* foreground `qs -p
  <worktree>/shell.qml` instead. **Hardening (built, awaiting a gated switch):**
  added `StartLimitIntervalSec=60`/`StartLimitBurst=5` to the unit so a
  persistent bad load parks `failed` after 5 tries instead of thrashing forever
  — the `ConditionPathExists` guard only checks existence, not validity. Lands
  on the next `switch` (toplevel built, not yet activated per [[Rebuild-Gate]]).
- **[decision · khoa — RESOLVED 2026-07-28] `notes` vs `drachma` naming.**
  Settled: **`drachma` is the single canonical name** — the token *values* and
  the mint are one thing, not a values-vs-engine split. Shipped as `aoide.drachma`,
  `stage/drachma.json`, `DrachmaState.qml`, and the `drachma` package (commit
  0117a79, "the note engine takes the coin"). "Notes" survives only as the
  musical image, never the primary name/filename. Pages reconciled ([[drachma]],
  [[Lexicon]]); the `Notes` concept page was **retired 2026-07-28** and merged
  into [[drachma]], so the wiki now has one page for the token layer (`Notes`
  survives as a drachma alias so dated log history still resolves).
  Residual: a few Quickshell facet bodies still carry
  the `notes.` property-id mid-rename to `drachma.` — cosmetic follow-up, not a
  contract question.
- **[restructure · khoa — RESOLVED 2026-07-28; wiki + code both landed]
  Songbook consolidation + rime retirement.** The single per-song home is now
  `song/songbook/<song>/`, self-contained: `rice.nix`, `drachma.json`, `assets/`
  (wallpaper + cover — was flat `song/covers/`), `palette/` (was `song/keys/`),
  `sounds/` (was `song/chimes/`), `icons/`, `widgets/`, `design/` (per-song
  intent + design memory — was `repertoire/<n>/liner/`). Cross-cutting memory
  (`learnings.md`, `preferences.md`, `update-playbook.md`) sits at `songbook/`
  root. `repertoire/` is subsumed; the allegorical subfolder names are dropped
  for sensible ones. **`backstage/` is CUT** — it never existed on disk and held
  nothing; runtime dirs are exactly `stage/` (live) + `auditions/` (propose gate).
  The **whole wiki was swept to describe this as canonical** (2026-07-28,
  multi-agent), so the **wiki is deliberately AHEAD of the code**. Taxonomy
  (khoa 2026-07-28): **nucleus** is the core every host inherits; **dendrites**
  are opt-in per host; **facets** + **rime** are ricing *machinery* (surfaces +
  engine) that hold no content — all song content lives in `song/songbook/`.
  So the shipped default rice `modules/rime/default/` also moves to
  `song/songbook/default/` (the songbook's one merge-only, upstream-owned song),
  leaving `modules/rime/` as engine-only. Physical migration still owed: move
  `song/repertoire/<n>/` + flat `covers/`/`keys/`/`chimes/` into
  `songbook/<n>/{...}` AND `modules/rime/default/` → `song/songbook/default/`;
  update `pkgs/aoide/src/shellbridge.rs` (`repertoire_notes`/`covers_dir`) +
  `dispatch.rs` (`resolve_rice_notes`/`derive_cover`), per-song `rice.nix`
  wallpaper paths, `lib/mkHost.nix` (walks `song/repertoire/`), `lib/checks.nix`
  `noSongRead` infixes (drop `backstage`), `.gitignore` (drop `song/backstage/`),
  `CONTRACTS.md`. **LANDED 2026-07-28:** `git mv` moved sonata/hero/default →
  `song/songbook/<song>/{rice.nix, drachma.json, assets/, design/}` (history
  preserved); `modules/rime/` deleted (it held no engine — the rice engine is
  `pkgs/drachma` + facets + the walker); Rust resolution rewritten
  (`repertoire_notes`→`songbook_notes`, `covers_dir` dropped, `derive_cover`
  reads `songbook/<name>/assets/`); mkHost/vmTest/flake/checks/.gitignore +
  CONTRACTS/AGENTS/BUILD/README updated; `backstage` gone everywhere. Validated:
  `nix build .#aoide` (cargo tests pass), toplevel eval, `aoide rice preview
  sonata` resolves from songbook. **Residuals still open:** (a)
  `song/songbook/default/rice.nix` borrows hero's wallpaper via
  `../hero/assets/hero.webp` — `TODO(song agent)`: give `default` its own cover;
  (b) thin the design pages to protocol + pointers once the per-song `design/`
  folders hold the memory, and refresh `songbook/sonata/design/intent.md` (still
  describes an indigo nocturne, not the cream key); (c) `lib/checks.nix` carries
  pre-existing nixfmt-1.4.0 drift (unrelated to this change) — a formatting-only
  pass is owed.
- **[bug · follow-up] `aoide rice preview <name>` derives the cover by
  song-name convention** (`covers/<name>.*`; post-migration `songbook/<name>/
  assets/`) instead of reading `aoide.drachma.wallpaper`. Mitigated live by the
  `AOIDE_WALLPAPER` env baked into the quickshell service, so the wallpaper
  survives rebuilds; a proper fix (preview reads the song's wallpaper note) is
  still owed. See [[Self-Ricing]].
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
  swept wiki** on several points the wiki now has right: shipped song is `sonata`
  not `hero`; launcher trigger is the `aoide:launcher` global shortcut not
  `aoide shell launcher toggle`; command count is 36 (three gated) not 28; `rice
  preview` is real; the `aoide.rebuild` capability is described as shippable but
  has no `options.nix` option (it is planned). Reconcile when the song migration
  lands — the README needs the repertoire→songbook + backstage edits anyway.
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
  `default` baseline to be composed with the Pantheon grammar. See
  [[design/Pantheon-Grammar]].

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
- [[design/Ricing-Protocol]] — the vision-check discipline
- [[Rebuild-Gate]] — why the switch is user-gated
- [[Conductor-Channel]] · [[Session-Graph]] · [[Terminal-Commander]] — the
  orchestration surfaces you both use and test
- [[Codebase]] — how the built repo actually works
