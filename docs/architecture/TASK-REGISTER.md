# Task register

The canonical shared register for Aoide/AoideOS work. Root
(codex-integration) orchestrates and reviews; the Fable session coordinates
implementation; Opus designs; Sonnet builds and reviews. One owner per slice,
no duplicate executors. This file is a ledger: it records current state and is
updated in place as work moves; history lives in the commit log and
`docs/Aoide-Wiki/ingest/log.md`.

Fields per entry: status · owner · depends on · evidence · next.

## 1. Session QoL (menu actions, multi-root projects, kill resolution)

- Status: implementation and combined build COMPLETE; isolated pre-activation
  acceptance of build 2 PASSED (2026-09-10, third run; runs 1–2 failed on the
  brief's own shell wrapper, not the build). Live desktop acceptance PENDING
  the User's activation. Not released; the interactive doorbell is not fixed.
- Owner: Fable (integration); root reviews.
- Depends on: the User activating the built system.
- Evidence: commits through 61afea2 on `main` (pushed); `cargo test` conduct
  597 / storage 402 / cli 42 green, workspace check clean; toplevel build
  `/nix/store/8qbyy7xynz3i9hpa7jlm8xh1j4b8ci2z-nixos-system-yomi-strix-26.11.20260907.dc5d91f`
  exit 0, not activated. Kill-scope label (db862b2) matches
  `conduct::graph::actions::kill_target`.
- Acceptance evidence (build-2 binaries, throwaway root, live socket inodes
  identical before/after): both daemons bind; two `conduct` wraps enrol;
  `project add/list/edit/remove` per schema (`edit` REPLACES roots; bad path
  → `invalid-path` usage); `session project --id X --project NAME` assigns;
  shellbridge `sessionaction`: project reply ok:true, kill on an unknown id
  ok:false "session is not registered locally", malformed undying → no reply
  (the QML timeout case), kill on a wrap ok:true → SIGTERM, sleep dead, record
  gone after reap; CLI `session kill` on a wrap → SIGTERM, dead; a second
  kill and an unknown id both say "session is not registered locally".
- Findings: (1) a reaped session and a never-existing id share one refusal
  wording — cosmetic, not blocking; (2) `setsid CMD &` yields the launcher
  pid, not the daemon's — brief fixture note; (3) build 2 predates P-CX-2/3
  (fcc7514, 30ee7c6), so the desktop lane's live check needs a build 3.
- Build 3 (2ccb9bc: QoL + P-CX-1/2/3 + P-CX-2b + migration slice 1):
  `/nix/store/vnsjq6yhfdp3q2a81xi1lc8jd0zkvbx9-nixos-system-yomi-strix-26.11.20260907.dc5d91f`
  exit 0, not activated; its `sw/bin/aoide` carries the P-CX code (the
  `codex-app-unsupported` string present, the deleted "owner ambiguous"
  line absent). Live runtime unchanged.
- Next: the User activates build 3 (build 2 = QoL only; build 3 = QoL +
  desktop Codex); then desktop acceptance on the real roster.

## 2. Desktop Codex / ChatGPT window association

- Status: architecture brief DONE (scratchpad p-codex-desktop-brief.md, Opus,
  read-only); P-CX-1 (pure reconcile core + record shape, kind "app",
  native-id keying) LANDED; P-CX-2 portable discovery LANDED (fcc7514:
  flock liveness + one parsed `ps` table, /proc only as a Linux tie-break,
  ambiguous owner enrols with no pid, non-unix taught-unsupported; conduct
  618 green, reviewed FIX→fixed); P-CX-3 LANDED 30ee7c6 (reaper tick primary
  call site + Hyprland listener promptness, `codex-app-unsupported` send
  refusal, kill refuses "the desktop app owns this thread's process", CODEX_HOME
  test floor; conduct 621 green, reviewed LAND). In-crate work COMPLETE; the
  live two-thread desktop check after activation is the User's gate.
- RULED (root, seq 180): the brief §3 "one app-server owns every held lock"
  rule is rejected; a desktop thread enrols only on positive ownership
  evidence (an app-server fd holding the lock), a tracked CLI record on the
  same id is never touched, a lock with no proven owner gets no app record.
  P-CX-2b LANDED: `lock_holder` is the fd match or nothing, `CodexThread.pid`
  is `u32` so an unowned lock enrols nothing, the lock path is canonicalized,
  plus the mixed CLI+desktop held-lock test (conduct 623 green, reviewed LAND).
  Live evidence (read-only, 2026-09-10): the one held lock is open in exactly
  one process, the `codex … app-server` child of the Electron main. Build 3
  follows the fix, no activation.
- Owner: Fable (one Sonnet executor per slice, independent review).
- Depends on: nothing; cargo serialized with other work.
- Evidence: standing verdict "current adapter unsupported; ownership of the
  actual app server is the unresolved distinction". Requirements: native task
  ids preserved, multiple task records, CLI vs desktop distinct, workspace and
  actual owning-window focus, no false exact-task navigation.
- Finding: the one live codex thread record is being written by the desktop
  app right now yet sits 67h into the 72h stale band and has no window; the
  app-server is a child of the Electron main, `~/.codex/ipc/ipc.sock` has no
  holder, thread→window bindings are opaque, so exact-task navigation is
  never claimed and no codex profile is registered.
- Next: build 3 carries P-CX-1/2/3 + 2b; the live check with the app open on
  two threads after activation is the User's gate.
- Portability (root, seq 190): strict fd evidence fixed the misownership
  but leaves desktop-thread discovery Linux-only. Non-Linux detection is
  UNRESOLVED and registered as such; the cross-OS requirement is not met.
- REGRESSION (User via root, seq 211, 2026-09-12): the two desktop Codex
  cards appear and disappear on the conductor every few seconds. Cause
  found and reproduced live, read-only (Fable, 0.3s roster sampling: both
  `kind:"app"` records dropped and re-minted, `[]` at 08:23:27.187Z, a new
  pair at :27.802, `[]` again at :28.722, while app-server pid 2598256 held
  both locks throughout). Three writers run `sync_codex_app_threads`:
  `lyra shellbridge --run` (unit path carries `pkgs.procps`) observes both
  threads and INSERTS; `aoided.service` and `aoide-graph-reap.service`
  have NO `ps` on their unit PATH, so `process_table()` fails ENOENT,
  `unwrap_or_default` yields zero servers, `lock_holder` proves no owner,
  the thread set reads empty and the reconcile DROPS every app record.
  Root's class diagnosis (failed scan conflated with confirmed exit) is
  confirmed; the gather-outside-lock race is NOT evidenced and is not
  being changed. P-CX-4 LANDED 7a9f865 + f7aaee8 (Sonnet executor,
  independent Sonnet review LAND; conduct 632 → 642 green, clippy and fmt
  clean of new issues, nix fmt + nix-lint green): a scan is `Observed(set)` or
  `Unknown`; only an observed set removes or inserts, Unknown changes no
  record and takes no lock; `lock_is_held` distinguishes released from
  unreadable; a genuine close still disappears; one audit line the first
  time `ps` is missing; tests for failed process scan, unreadable lock and
  fd dirs, mixed CLI+desktop, alternating writers, genuine close. Second
  commit puts `pkgs.procps` on the aoided and graph-reap unit PATHs
  (deployment cure, no activation; the live churn stops after the User
  rebuilds). The audit line and the ID-coupled unknown rule shipped (an
  unreadable fd table aborts the scan only for an id an app record already
  carries). Next: the User rebuilds; the live check is that the two cards
  keep their petnames across ticks and a genuine app close still removes
  them.
- Scope (root, seq 219): what is landed is desktop ASSOCIATION only, not
  complete integration. Root's live snapshot: both ids map to the real
  ChatGPT window 0x5b14c12405c0 / workspace 1; `model` null for both; the
  second thread's title absent; state `idle` even while root is working.
- Follow-on (tracked, not dispatched): native prompt/response/tool/model/
  subagent/activity capture from the app's own records, every shown datum
  carrying its original source pointer; missing data explicit, never a
  fabricated reasoning trace; exact-task navigation stays distinct from
  window focus. Brief DONE (Opus, read-only, scratchpad
  `p-codex-capture-brief.md`): rollouts are written live; working/idle is
  derivable from the task_started/task_complete/turn_aborted bracket, never
  from elapsed time; the title-less thread is a subagent (parent_thread_id,
  nickname) that session_index.jsonl never lists; model sits in
  turn_context; no reasoning exists on disk; occupancy is the last
  input_tokens field alone; a 1 MiB bounded tail, sqlite rejected. Design:
  pure fold + bounded reader in a new `codex_capture.rs`, existing record
  fields plus one additive `sources` pointer map, enrolment and the
  Observed/Unknown seam untouched. Slices S1 fold, S2 reader+sources, S3
  state, S4 subagent edge+nickname title, S5 prompt (gated). Next: root
  R1 guardian_review threads on the roster, R2 `working` on an app record,
  R3 `sources` general vs codex-only; User D1 prompt text on the roster,
  D2 the live check. RULED (root seq 228): R1 enrol labelled, R2 every
  `state` consumer enumerated with file:line before S3 and the app
  kill/send refusals asserted, R3 general field one producer, D1 the
  latest prompt is a captured field. LANDED and pushed: S1 033fead+e601074
  (pure fold), S2 fdb6682 (bounded 1 MiB reader `capture_for`, additive
  `sources` on `SessionRecord`, one call site in `sync_codex_app_threads`,
  merge never writes state/lineage/title) + 537a3b3 (CONTRACTS §4 entry) +
  92c7e9a (per-thread (len,mtime) memo and cached rollout path); each
  reviewed PASS by an independent reviewer; conduct 679, storage 403.
  S2b LANDED 56d535a+b6f3525 (cap-boundary tests, per-tick memo
  eviction; the first cut failed review on a reproduced parallel-test
  race, fixed by a pure `retain_memo_in`; conduct 682). S3 LANDED aadc67b
  (state off the turn bracket, R2 consumer table in the commit body,
  kill/send refusals asserted in `working`; reviewed PASS, conduct 689;
  CONTRACTS §4 corrected 63b2b86). S3b LANDED badfc7a (reviewed PASS, conduct
  708): `session phase`/`session end` refuse a `kind:"app"` record with
  the `codex-app-unsupported` error `send` uses, before any file write;
  the "never publishes awaiting" claim is tightened to the fold vocabulary
  plus every state writer refusing; dead `UNSUPPORTED_PLATFORM`/
  `fold_rollout` removed (workspace compile warning-free). Then S4. Staged and proved,
  not deployed (build 4 predates S1).
- Build 4 = LIVE, CHURN FIX VERIFIED (2026-09-12): the clean HEAD build
  (pristine worktree at 2432e06) is
  /nix/store/cbbnif0ij38zm8y0sb7v6jfxir4k8yrl-nixos-system-yomi-strix-26.11.20260907.dc5d91f
  = system generation 204, activated by the User at 09:06:22Z. Running
  `aoided` (pid 3460303, started 09:06:12Z) is the new package 0mg1rj1m…
  with `ps` on its PATH; both the aoided and graph-reap units carry
  procps-4.0.7. Live roster sampled at 0.3s for 30s: both app cards kept
  their petnames, zero changes (before the fix: a drop-and-re-mint every
  few seconds). No "ps not on PATH" audit line, as expected. Still the
  User's to confirm: a genuine thread close removes its card. A second
  toplevel l2pahyr05ky… exists in the store from the dirty shared checkout
  (another session's staged eidolon dendrite); it was never activated and
  is not a build of HEAD.

## 3. Interactive doorbell (native Claude channel idle wake)

- RESUMED BY THE USER (seq 224/228, 2026-09-12; the seq 220 pause is
  superseded): automatic doorbell wake is important, especially for idle
  agents on other hosts outside direct conductor reach; manual nudges stay
  a fallback, not a replacement. Architecture kept: remote mail reaches
  the paired receiving host, which resolves its own current local reader
  and wakes it through a supported local harness channel; cross-host
  delivery alone never creates a wake path for an unsupported harness.
  M5c-2/3 are in the live daemon since generation 204 (09:06:22Z). The
  isolated TUI E2E (runbook scratchpad `p-doorbell-e2e-brief.md`, live
  store package 0mg1rj1m… in an isolated root) runs IN-SESSION by Fable,
  stepwise; the earlier subagent dispatch was rejected by the harness
  permission classifier (seq 217) and any further rejection is reported
  with the exact blocked action.
- PROVEN 2026-09-12 (isolated rig /tmp/aoide-db, live store package,
  live daemon untouched; evidence scratch `doorbell-e2e/REPORT.md`): a
  real Claude Code 2.1.260 TUI (`--dangerously-load-development-channels
  server:aoide`, project `.mcp.json` → `aoide mcp serve --stdio`,
  channel socket present) enrolled itself by reading `db-box`; IDLE: the
  letter rang the wrap the same second, the channel line appeared in the
  TUI and the session read the letter 3s later; DRAFT: an unsent composer
  line survived the wake untouched; BUSY: the ring was deferred
  (`working`) and fired on the Stop hook, read 3s after the turn ended;
  RELAUNCH: after exit and respawn the ring targeted only the new wrap,
  the dead one reported `unknown`. The session reported each letter and
  refused to obey its body (house rule 4) — the RESPONSE is a report, not
  compliance. Lessons: a project `.mcp.json` server is silently disabled
  when the session inherits `CLAUDE_CODE_CHILD_SESSION` (pre-approve in
  `settings.local.json`, spawn with the marker unset); `send --submit`
  does not submit a pasted turn, a separate bare Enter does; the exited
  wrap leaves an orphan `channel-<id>.sock` (backlog item confirmed).
  Cross-host wake (gate 3, §11) stays blocked on the far-door grants.
- Status: NATIVE WAKE PROVEN (root, 2026-09-11, Claude Code 2.1.260,
  disposable PTY, the Fable kit): idle wake with no keystroke, unsent draft
  survived, a text-generation turn finished before the event was answered;
  evidence `docs/architecture/CLAUDE-CHANNEL-PROOF.md` (root-authored). This
  proves the native mechanism only; the Aoide mail doorbell over it is NOT
  proven. Limitations recorded there: text-generation busy case only;
  tool-busy, GUI terminal and end-to-end mail acceptance remain.
  P-M5c-2 LANDED (3f39628 + review fix a3c540a): `initialize` declares
  `experimental["claude/channel"]` and carries `instructions`; the stdio
  MCP server binds `$XDG_RUNTIME_DIR/aoide/channel-<AOIDE_SESSION_ID>.sock`
  (0600, one path authority `channel_socket_path` beside
  `conduct_socket_path`), one listener thread, each line one
  `notifications/claude/channel` push under the daemon's 1 MiB line cap,
  stdout behind one mutex; no doorbell change, golden unchanged; protocol
  137, conduct 624, server 212, cli 42 green. Staged, not deployed.
  P-M5c-3 LANDED (953fb02): `ring_locked` picks the transport after the
  state gate — a connect to `channel_socket_path(wrap_id)` that succeeds
  wins for any wrap (one-way push, no submit key), else a headless wrap
  rings over the PTY byte-identically, else `interactive-composer` (now
  meaning interactive AND no channel); a stale socket file refuses the
  connect and falls through; same receipt, same latch, a failed write
  reports `write-failed` and leaves the latch armed; conduct 632 green,
  golden unchanged; reviewed LAND. Staged, not deployed. Then onboarding,
  P-M5b.
- Owner: Fable (Sonnet executor per slice, independent review); the proof
  was root's.
- Depends on: nothing; P-M5c-3 on P-M5c-2.
- Evidence: proof kit built and verified offline (node stdio channel server,
  unix socket, RUN.md); claude 2.1.260 carries the hidden flag
  `--dangerously-load-development-channels <servers...>` ("shows a
  confirmation dialog at startup"), `notifications/claude/channel`, and two
  gates that can skip it (managed `allowedChannelPlugins` allowlist, a remote
  "channel gate"). Prerequisite: a human at a real terminal accepts the
  startup dialog for the throwaway project.
- Backlog from review: `reap.rs::sweep_orphan_sockets` sweeps only
  `session-*.sock`; a SIGKILLed MCP subprocess leaves `channel-<id>.sock`
  behind (harmless: connect refuses, unlink-then-bind on restart).
- Next: nothing while paused. When resumed: the end-to-end wake per the
  runbook, tool-busy and GUI cases, onboarding docs for the flag.
## 4. Lyra / AoideOS architecture migration (phased workstream)

- Status: phase (a) COMPLETE (slices 1-3 and 5 landed, below); phase (b)
  slice 1 LANDED, later (b) slices pending; phases (c) and (d) pending.
  Phase (a) design (scratchpad `p-lyra-migration-a-brief.md`, Opus, 850
  lines, read-only) decided the export surface, all from
  `pkgs/aoide/flake.nix`: `packages.{default,aoide,aoide-static}` unchanged;
  NEW `packages.lyra` (= `aoide.rice`, retires the `aoide^out,rice` selector),
  NEW `overlays.default` (one attribute, one build), NEW `nixosModules.default`
  (core `aoide.*` options + aoided unit/tmpfiles/session vars, applies the
  overlay itself) backed by a new plugin dir `pkgs/aoide/module/` with explicit
  imports; one new option `aoide.sessionTarget` so core stops reading
  `aoide.facets.quickshell.enable`. NOT exported: homeModules, per-unit
  modules, `aoide.package`, door/secrets/usage units, songbook manifests, any
  paint, walker removal (phase b). Five slices: (1) overlay, deletes two
  hand-copied lambdas (`lib/mkHost.nix:68`, `tests/vm-boot.nix:76`); (2)
  `packages.lyra` + contract in PACKAGE-LAYOUT; (3) `nixosModules.default` +
  options out of `modules/nucleus/options.nix`; (4) aoided unit out of
  `modules/nucleus/aoided.nix` behind `aoide.sessionTarget`; (5) scratchpad
  consumer flake importing only `path:./pkgs/aoide` as the acceptance. Every
  slice gates on `nix eval` of the yomi-strix toplevel, 3–5 on `nix build`.
- Findings: neither flake exports overlays/nixosModules/homeModules today;
  three nucleus files reach `inputs.quickshell` (`shellbridge.nix:46`,
  `aoided.nix:318`, `secrets.nix:290`) so a core-only consumer walking
  `modules/` still needs that input; `lyra onboard`/widget regen shell out to
  the ROOT flake (`onboard.rs:137`, `widgets.rs:213`), parked for (c)/(f);
  `docs/BUILD.md:3-5` Wave-0 rule is stale under this workstream.
- Authorization: the User authorized the flake/lib/tests/nucleus edits for
  this workstream under bounded owners (root ruling seq 180); runtime
  activation stays the User's separate gate. Slices 1-3 LANDED
  (`overlays.default` 0448974, `packages.lyra` 4415f62, `nixosModules.default`
  with the six core options and the new `pkgs/aoide/module/` plugin dir,
  the aoided unit + tmpfiles + core session vars behind `aoide.sessionTarget`);
  slice 5 LANDED (stranger consumer proof: five evals green with no AoideOS
  import; `module/` filtered out of the package src). PHASE (a) COMPLETE;
  latest toplevel `/nix/store/wzs21qkb80wpaskjb4612q0qb0fpjj57-…` not activated.
  The stranger proof is EVALUATION only (five `nix eval`s of a throwaway
  toplevel); no stranger build ran and nothing was activated anywhere.
  Root acknowledged phase (a) complete (seq 190).
- Phase (b) DESIGNED (scratchpad `p-lyra-migration-b-brief.md`, Opus, 672
  lines, read-only), nothing dispatched. Shape: four aggregates
  (`modules/default.nix` + one per layer, `LC_ALL=C` entry order, `_`
  entries omitted); the songbook keeps ONE typed scan (`lib/songbook.nix`
  exports `songModules` + `strayNixFiles`) instead of a `default.nix`
  outside house rule 1's writable domain; `lib/walk.nix` deleted in slice
  2; house rule 7 reworded from "never an import list" to "never named
  from outside its own directory", CONTRACTS.md the same; `_`-prefix
  re-expressed (loses the walker, keeps `_widgets/` and `lib/pkgs.nix`);
  gate = toplevel drvPath BYTE-IDENTICAL per slice (precondition proved:
  the flake source path is absent from the toplevel's 13012 requisites),
  `aoideOptions` 153 and `songbookManifest` unchanged. Finding:
  `checks.no-song-read` is vacuous (walked paths cannot carry a `song/`
  runtime infix; the banned dirs are gitignored) while ~20 files claim it
  enforces something. Three slices: (1) aggregates + three call sites +
  rule 7/CONTRACTS/README/`modules/README.md` (~215 lines, the 74-line
  import list is mechanical); (2) songbook exports + walker deletion +
  stronger `song-shape`; (3) root AGENTS.md task→directory map + glossary.
- Slice 1 LANDED (ef8cc1c + review fix 4d46d5e, root seq 191): four
  aggregates, three call sites, rule 7 reworded with the songbook carve-out,
  `modules/README.md` new; yomi-strix toplevel drvPath byte-identical,
  `aoideOptions` 153 unchanged, fmt + nix-lint green. Finding, accepted as
  the contract: nixpkgs collects imports breadth-first (`filterModules` is
  a `genericClosure`), so the tree's leaves keep their relative order but
  merge as one unit at their import depth after every depth-1/2 module;
  the VM node drvPath moved on ordering alone (its inline `jq` now precedes
  the tree's packages), yomi-strix holds because its host modules define no
  order-sensitive list. `lib/walk.nix` survives for the songbook until
  slice 2. Evaluated only; nothing built or activated.
- Next: root rules on the brief (Q1 `lib/pkgs.nix` stays, Q2 keep the
  vacuous check until (f), Q3 walk order, Q4 re-express `_`, Q6 skip a song
  without `rice.nix`, Q7 slice 3 in (b)); the User answers Q5 (does the
  rider's "no aggregator or walker" mean docs) and Q8 (songbook scan stays,
  house rule 1); slice 1 landed; then the next (b) slice. The User
  activates a build when ready.
- Owner: Fable (Opus design per phase).
- Depends on: 1 for the runtime seams; docs/glossary/Mneme-optional changes
  accompany each phase, never trail it.
- Phases: (a) portable core exports; (b) explicit `default.nix`/aggregates and
  removal of the walker; (c) compact songbook/runtime split; (d)
  draft/stage/declared lifecycle with palette and Hyprland hotload; (e)
  dxflake/Osaka external-consumer proof; (f) AoideOS host migration and
  obsolete-path removal.
- Docs acceptance (rides each phase): root README human-facing (what it is,
  capabilities, getting started, architecture overview; stale skeleton and
  walker claims removed); root AGENTS.md lean = universal rules + task→
  directory map + compact plain-Linux glossary (package, Nix module, flake,
  host, Home Manager, dendrite, aggregate, default.nix; Aoide, AoideOS, Lyra,
  song/rice, songbook, runtime, bridge; import/enable/build/activate/hotload);
  `modules/README.md` written; progressive navigation, local AGENTS = local
  invariants + verification only; contracts loaded on demand; no aggregator
  or walker; Mneme optional. Test: one host edit finds imports and the right
  check in short hops without rice/protocol docs; a widget edit finds song
  contracts without the Nix lanes; a plain checkout works without Mneme.

## 5. Pairing windows (enrollment windows replace the old ceremony)

- Status: QUEUED; proposal `PAIRING-WINDOW-PROPOSAL.md` (root) committed
  alongside this register; protocol review FIRST.
- Owner: Fable (Opus for protocol review); no competing pairing executors.
- Depends on: 1–3; a reviewed protocol before any code.
- Evidence: proposal §Delivery phases and §Acceptance.
- Next: phase 1 = CLI/UI/wire inventory, threat model, protocol decision,
  and the deletion inventory (old item → replacement or reason → callers and
  state affected → verification that trusted peers survive). No parallel
  old/new ceremonies, no legacy opt-in; pending old requests migrated or
  invalidated; unsupported old peers get upgrade-required.

## 6. Agent canvas

- Status: IN PROGRESS as the widget-preview stack (User request, root-
  launched Fable session `widget-preview-fable`, seq 310/322/328): new
  `lyra preview [<widget>]` + `lyra preview set …` (lyra crate) building an
  isolated root under `$XDG_RUNTIME_DIR/aoide-preview/` (resolved livery,
  fixture stage files, symlinked facet qml + checkout widgets; the child
  env carries no live daemon socket), a `WidgetPreview.qml` canvas in the
  quickshell facet (any widget, width/height per axis, 3×3 anchors,
  viewport presets and custom, aspect lock, zoom, fixture/livery pickers
  incl. `live`/song/file/bare base16, reload, error pane), fixtures under
  `modules/facets/quickshell/preview/fixtures/`, `Widget-Preview.md`.
  P1 (`preview/set/declare`, 107 tests, golden 51) and P2 (canvas, four
  fixture sets, isolation verified) are done in the tree; two path-
  traversal writes in `preview declare` (`..` in the widget field through
  the songs symlink; unvalidated `--slot`) are being fixed and nothing is
  offered for integration until those carry named tests and a review.
  User extensions, same owner, phased after the core: agent tools
  `lyra preview shot|tree|notes` (screen/canvas/widget/element shots via
  the screen crate + canvas grab; live item tree joined to a static QML
  parse → element path + file:line; `notes.json` work list) with a canvas
  annotate mode (`commands/preview_tools.rs`, golden 51→54), then mouse
  editing/markup (seq 388: select/move/resize with handles synced to
  numeric sizes and anchors, zoom/pan, arrows/highlights/freehand/text,
  undo/redo, annotated screenshot export; separate Interact vs
  Edit/Annotate input modes). User usability asks (root seq 441/444,
  fanned to the owner as 439/442): direct marker interaction — a click on
  a numbered marker selects and opens its existing note (never a
  duplicate; hit area + hover; Normal mode without activating widget
  buttons; Escape keeps entered text), blank-canvas click in the Insert
  tool places a marker and opens editing; and the tool-dispatch bug —
  every exposed tool must own working geometry (select, arrow with
  arrowhead, note/pin, rectangle) over the whole canvas including space
  outside the widget, or be removed until it does; check the shared
  selection MouseArea and loader clipping. Owner implements, designer
  validates, evidence = screenshots per gesture. Geometry and annotations
  are preview state under the preview root — never a silent rewrite of widget QML; writing
  geometry back into a checkout widget is a separate, User-gated command.
  Lane rule after the contained 2026-09-12 incident (an executor truncated
  the untracked canvas file through a preview-root symlink; restored
  byte-identical): no writes under any `run/qml`, no symlink from a
  preview root into the checkout. Edit/test only; the integrator lands it
  after independent review.
- Owner: widget-preview-fable (plan + delegation + hands-on pass); Fable
  integrates. Designer session `widget-design-fable` consumes it for the
  card redesign (§13 UI half, now under the User's live-draft workflow).
- Depends on: the reap-scoping slice (carried backlog) before any stage/
  preview acceptance; native `FolderDialog` only, no custom browser.
- Handoff RECEIVED (owner seq 454/457, 2026-09-13T03:46Z): preview.rs
  2237 l, preview_tools.rs 3019 l, WidgetPreview.qml 2092 l, fixtures,
  Widget-Preview.md, lyra README/AGENTS/mod.rs/registry.rs (golden 48→54),
  Cargo +image, slots.md, ingest index; owner reports lyra 184 pass, two
  independent passes on the agent tools, evidence under
  /tmp/aoide-widget-team/preview/evidence, log draft at
  /tmp/aoide-widget-team/preview/log-entry.md. Owner-declared risks:
  run/qml/* in every preview root are SYMLINKS into the checkout (the
  lane rule forbids this — graded in review), Declare double-click
  writes the checkout widget, component instances report the
  definition id, `preview tree | head` panics on SIGPIPE (workspace-wide),
  createObject warnings on load. Independent review: PASS-WITH-FIXUPS —
  two HIGH returned to the owner before integration (the run/qml symlink
  farm into the checkout, guarded by convention only; `--root` never
  validated against the live daemon dir), one MEDIUM on the log draft's
  shape; 184/184 reproduced, traversal fixes real. Integration after the
  fix-up re-review, serialized behind the I1 icon executor (shared lyra
  mod.rs/registry.rs). Backlog: no bin resets SIGPIPE (one-line SIG_DFL
  in `lyra`, `aoide`, `aoided` main()). The seq 441/444 usability asks are
  follow-ups by the same owner after integration.

## 7. Mail attachments (`mail send --attach`, also the handoff carrier)

- Status: QUEUED (root, seq 176, 2026-09-10). Queue addition only; it does
  not disrupt the QoL, Codex or doorbell owners.
- Owner: Fable (brief: Opus) once a slot opens; proposed slot after the
  doorbell lane (3) lands and before migration phase (a).
- Scope ruled by root: `aoide mail send --to node/mailbox --attach PATH
  [--attach PATH…] -- TEXT`; NO `aoide sync`, no standalone attach/sync
  command (rsync already copies files); reuse existing rsync/SSH transport,
  never assume pairing provides SSH credentials; files + integrity manifest
  + receiver-local references in the existing letter; transfer complete ≠
  mail fetched ≠ work acted on; attachment inbox only by default, never
  overwrite an active project, never execute or load received content;
  plain mail stays lightweight and works without rsync when no attachment
  rides; handoffs are ordinary letters with attached notes, no new handoff
  protocol, no automatic executor switching.
- Proposed minimal shape (awaiting root's placement reply): the manifest
  is SIGNED — envelope version "2" adds `attachments: [{name, sha256,
  bytes}]` to the sealed bytes (`header ‖ 0 ‖ text ‖ 0 ‖ manifest`), version
  "1" letters are byte-identical to today; receiver-local state lives in
  the unsigned store `Entry` (`attachments: [{name, ref, state}]`, state
  pending|complete|failed|unavailable), never in the envelope; inbox is
  `<state>/mail/attachments/<msgid>/<name>`; transport is rsync over the
  tunnel module's ONE ssh builder (`client/src/tunnel.rs`) for a node with
  an ssh route, a plain copy for `self/` and same-node mailboxes, taught
  `transport-unavailable` otherwise (letter still delivers, attachment
  stays pending on the sender). Slices: ATT-1 schema + manifest + local
  copy + `mail read`/`show` display of refs and state (no network);
  ATT-2 remote transfer + interrupted-transfer cleanup + retry on the
  next drain; ATT-3 docs (MAIL.md, CONTRACTS.md) + limits.
- Brief must settle: authorization (same gates as `mail send`, paired
  nodes only), path/symlink containment (relative names only, no `..`, no
  symlink following on either side), size limit (default and override),
  cleanup of a partial inbox dir, transport-unavailable behaviour, and that
  `mail read` never reports an attachment fetched from the letter alone.
- Brief: DONE (scratchpad `p-mail-attach-brief.md`, Opus, 482 lines, read-only).
  No rsync exists in the repo, so bytes ride the A2A door in 512 KiB chunks
  (open question for root); envelope v2 only with attachments; receiver state
  in a `state/mail/attachments.json` sidecar; `attach` as a fourth closed
  capability; quota options; rename-publish after hash verify; 7 slices.
- RULED (root, seq 190): PARKED for scope review. A custom chunked A2A
  transfer protocol contradicts the User's simplification intent and is
  not implemented. The brief above stands as the inventory only.
- Minimal brief DONE (scratchpad `p-mail-attach-min-brief.md`, Opus, 498
  lines, read-only). No existing end-to-end transfer to scope (no rsync/scp
  string in `pkgs/aoide`; the one ssh is a `-L` forward; the door's methods
  move no file). Shape: the letter carries a SIGNED manifest (envelope v2,
  the parked layout reused, v1 letters byte-identical, v2 only with one or
  more attachments); bytes move by `scp -B` over the operator's own ssh
  route, one foreground attempt inside `mail send` when the destination
  node itself accepted the deposit, otherwise a paste-ready command is
  printed; the daemon never moves bytes; the receiver opens
  `state/mail/attachments/<msgid>/` 0700 on filing and answers
  `mailDeposit` with that `inbox` path (one new wire field); `mail
  read`/`show` derive available|missing|corrupt at read time by
  symlink_metadata, size, hash (no sidecar, no stored state, never
  "fetched"); 64 MiB per file and 16 files as constants, no config key,
  no quota; direct-edge only, never via a hub; `rm_older_than` prunes the
  dir. Limitations stated: one attempt, no retry/resume/progress, ssh
  credentials are the operator's not pairing's, the inbox is a convention
  not a wall, a pre-v2 peer refuses the whole letter. Five slices: ATT-0
  list flag kind in the shared parser (`--attach a --attach b` last-wins
  fix), ATT-1 envelope v2, ATT-2 inbox + verify + prune, ATT-3 `--attach`
  and honest reading, ATT-4 the `inbox` field and the scp spawn (the only
  slice that spawns anything; ATT-0..3 is a coherent stop).
- Next: root rules (Q1 land ATT-0..3 or stay parked, Q2 the `inbox` field,
  Q4 v2 refusal on old peers, Q6 ATT-0 first); the User answers Q3
  (constants, no config key), Q5 (aoide spawns scp once vs only prints it),
  Q7 (the slot; the register's "before migration (a)" is already past).
  Nothing dispatched until then.

## 8. Done-record lifecycle (killed/ended agents stay on the conductor)

- Status: SUPERSEDED (root, seq 190, the User's word). The conductor shows
  LIVE agent sessions only; ended/killed records leave the active view
  without depending on a live sibling, a window, or a prune, and history
  stays in the ledger. The analysis below (scratchpad
  `p-done-lifecycle-brief.md`, Opus, read-only) stands as cause inventory;
  its narrow collector is possible cleanup, not acceptance. Nothing
  dispatched, roster never mutated by an agent.
- Required by seq 190: preserve idle live agents, app tasks sharing a
  process, and subagents with lifecycle evidence; an orphan of unknown
  liveness is shown as unknown, never as proven-live, never killed to clear
  a card; retire/dismiss semantics specified explicitly and separately;
  the done card's "Kill terminal" label fixed even where the card is
  ordinarily filtered; history/resurrection retained; no double ledger
  writes; fixture plus computer-use proof.
- Cause: `reap.rs::superseded_done_siblings` groups by `windowAddress` and
  skips empty ones, so a windowless (hosted-native) `done` record joins no
  group; liveness judges only non-`done` records; reap-time `prune_done` runs
  only when something was reaped. `aoide session prune` already clears the
  `done` record; no desktop surface exposes prune (bridge verbs: kill,
  undying, project, createproject, editproject). A done card's kill label
  reads "Kill terminal <host>" for an action `kill_target` refuses.
- Proposal (no retention widening): one pure collector for windowless
  `done` agent records under a verified live controller (parent in the
  roster, conductable, not done, live pid, at least one not-done agent
  child), folded into the existing superseded-done drop; app threads and
  subagents excluded by `is_agent_kind`; roster-only, ledger untouched;
  bridge gains a `prune` verb and the done card says "Prune". A parentless
  idle orphan gets nothing automatic; the manual path is `session end` then
  `session prune`.
- Owner: Fable (Sonnet executor per slice, independent review); root rules
  on the proposal first.
- DESIGNED (scratchpad `p-live-view-brief.md`, Opus, 743 lines, read-only;
  citations re-audited against 1bd160c). Decision: the roster IS the
  active view. `state:"done"` becomes a write-through marker: every path
  that sets it drops the record in the SAME locked write through the
  existing `drop_sessions`; the ledger keeps history; `aoide session`
  already excludes done and `storage/ledger.rs` already charters
  `sessions.json` as live-only. Deletes `superseded_done_siblings` and its
  three tests, `prune_done`'s reaped gate, the narrow second drop and
  `data.supersededDone`. Byte-identical: one ledger line per exit via
  `ledger_session_exit`, `session end` idempotence, resurrection (ledger
  anchored), `session prune`, the codex reconcile, `is_session_dead` (no
  band moves, nothing new condemns). Liveness = one pure `liveness_of`
  reading STRUCTURE never activity: E1 own live pid (app excluded), E2 live
  window (compositor absent = conservative), E3 kind app (re-proved each
  pass by the lock sync), E4 inherited through a live conducted ancestor
  (≤32 hops, never through a done hop); else Unknown, published as an
  additive-v0 `liveness:"unknown"` field stamped change-only, never a
  death signal. `aoide session retire --id` = the explicit close: gated,
  ledger exit once then drop, refuses proven-live, subagent, app thread
  and absent id, ok-noop on done; bridge verb first, card second. Card
  ladder: done/unknown records never say "Kill terminal"; Retire row.
  Seven slices (pure fn · stamp · ended leaves roster · `session end` drops
  · retire + bridge + golden · QML ladder · Session-Graph page), 27
  fixtures incl. the unknown-orphan control and the tracked-CLI reconcile
  byte-identity; computer-use recipe read-only before/after with
  a4dc11e8 as subject and b7ceaf45 as control (stays, reads unknown).
- Next: root rules (Q3 retire gated, Q4 no new unknown glyph, Q5 keep
  `session --all`, Q6 delete only the inverted assertion line); the User
  answers Q1 the command word (default `retire`; `dismiss` is taken by
  secrets) and Q2 whether a fresh hook self-report upgrades unknown to
  proven (default no: a self-report is not proof). Then S1.

## 9. Harnox-backed secrets custody (review-only workstream)

- Status: REVIEW DRAFT (root, seq 201/202, 2026-09-11):
  `docs/architecture/HARNOX-SECRETS-PROPOSAL.md`, root-authored, committed
  here for register inclusion. Not an implementation or migration
  authorization; no secret value read or migrated; no public PR or comment
  to the Harnox maintainer.
- Issue FILED (User-authorized, root, seq 207): Harnox issue #1,
  https://github.com/noah427/harnox/issues/1 — Aoide integration with
  machine-local Harnox secret stores; delivery semantics are NOT approved
  until the maintainer answers. No credential accessed or migrated.
- Scope (User via root, seq 210, supersedes seq 209): Harnox is NOT a
  dependency of Aoide core. It is REQUIRED for the optional
  secrets-management capability and FIRST-CLASS through normal Aoide
  commands, configuration and deployment. A secrets-enabled build consumes
  pinned upstream Harnox directly (initially the `secrets` feature only);
  a build without it operates normally and reports secret operations as
  unavailable; NO hidden alternative storage fallback when disabled. Both
  build configurations are verified. Every implementation brief carries
  this paragraph.
- The User's ask: Aoide works with Harnox (the private Rust core shared by
  Mneme and Melete, master bb68a3a) with minimal upstream changes, Aoide
  consuming its seams.
- Root's conclusion: Harnox `secrets` is a feature-gated embedded custody
  library (XChaCha20-Poly1305 file + keyfile, issued tokens,
  `external_value` returning a zeroizing string), NOT a broker daemon or
  policy service; Aoide stays the authority for callers, TOTP, approval,
  audit and the shell-facing interface; secrets-only dependency plus a
  typed native store adapter; no dependency on a running Mneme/Melete; no
  shared multi-process encrypted files (Harnox assumes one writer).
- Open by the draft: Harnox documents no tool/script read path while Aoide
  `secrets exec`/socket delivery intentionally releases a credential to a
  child, so an explicit delivery contract is needed or that use case keeps
  its backend; an additive fallible accessor upstream (absent vs damaged);
  private-dependency distribution for external Nix consumers. Existing
  Aoide gaps restated (self-asserted consumer labels, packaged cross-uid
  origin attestation, `put_gate` without caller auth, best-effort audit)
  are NOT fixed by changing storage.
- Owner: root (draft, Harnox research, one read-only broker audit); Fable
  (critique of the minimal seam and the existing consumer requirements,
  Opus read-only, scratchpad `p-harnox-critique.md`). No duplicate
  research, no implementation.
- Critique DONE (scratchpad `p-harnox-critique.md`, Opus, 349 lines,
  read-only; no Harnox checkout on this box, Harnox facts taken as given).
  Verdict: accept with 12 amendments. Seam is real and small:
  `backend::resolve_backend` is the one funnel for fetch/has/store, so a
  two-variant enum there costs zero lines in `broker.rs` (~250 lines, one
  crate). BLOCKER the draft misses: today's `has` templates are `test -f`
  (existence only); a Harnox `has` that answers by decrypting turns an
  unreadable secret into absent, `put_gate` skips its exists-refusal and
  `store_value` overwrites the only ciphertext. Mismatch 6 overstates
  today (absent and corrupt are already one non-zero `get` exit; what is
  at stake is the taught error strings). Mismatch 4's fallback is every
  secret Aoide has (plaintext crosses the socket for `exec`, `a2a-door`,
  `a2a-client` alike), so a custody-only answer voids the recommendation.
  Mismatches 1/2 already ruled, 9 pre-existing, 8 PART-NEW: `handle_put`
  applies no `admin_gate` on a declared first-class wire, so a
  socket-group member can substitute a policied secret's value (own task).
  Deletion is smaller: the template mechanism survives with the doc-only
  presets; six constants, the age machinery and `pkgs.age` die.
  Distribution is the hardest item: the flake is nixpkgs-only by charter,
  `Cargo.lock` has no git source, and a default-off feature does not keep
  a private dep out of the lock. Authority drifts twice (`issue`/`verify`,
  rotation); both to be excluded from the slice.
- Next: maintainer response on issue #1 before delivery semantics count as
  approved; root applies the amendments; Maintainer questions (delivery
  sanctioned, existence-only `has`, fallible accessor, single writer,
  distribution); the User decides keep-vs-retire generic delivery, whether
  core may take a private input, retiring built-in `file`/`age` and the
  doc-only presets, and any migration; root files the `put`-over-socket
  gap as its own task. No slice before those.
- Maintainer RESPONSE (issue #1 comment 5650085974, 2026-09-13T01:57:55Z,
  relayed by root seq 411, maintainer claims not independently diffed):
  recommends COPYING `src/secrets.rs` and its design into Aoide instead of
  a pinned dependency (private distribution would need a deploy-key
  credential; small module, narrower deps; Aoide owns the consumer
  security discipline; the copy must track upstream fixes, notification
  offered); no upstream change will be made for the Aoide proposal;
  v0.3.5 tagged with `secrets.rs` claimed byte-identical to 0.3.4. The
  maintainer treats the four dependency asks as moot under a copy. NOT
  recorded as approving the `external_value` exec/transfer contract nor
  as answering the missing-vs-corrupt semantics. The User wanted an
  upstream DEPENDENCY, so the copy is NOT APPROVED: adoption, migration
  and deletion stay held for the User. Direct dependency versus a locally
  maintained custody module is a CHANGED ARCHITECTURAL DECISION, not an
  adapter adjustment; core identity/authorization gaps are unchanged. No
  executor, no copy, no PR reply dispatched.

## 10. osaka: quickshell facet goes blank when the monitor is turned off

- Status: REPORTED (mail seq 208, 2026-09-12T03:35Z, from osaka wrap
  conduct-564652 to mailbox `merry-comet`; no live reader of that name on
  yomi, so it sat unread until the User asked; read from the store without
  consuming it). Root cause per the report: a monitor-off drops Hyprland's
  output list for a moment, Qt's wayland QPA creates a placeholder screen,
  the layershell surfaces detach, and `Quickshell.screens` is populated
  below QML so no handler can reset it (matches `health.rs`'s own module
  doc). Usually self-heals in under a second; when it does not, the process
  stays active and paints nothing. The shipped mitigation
  (`aoide-quickshell-healthcheck.timer`, ~15s, 0/15/60/300/900s ladder)
  fired zero real restarts across ~40k ticks on osaka; every observed blip
  self-healed first.
- Two gaps flagged for a second pair of eyes: (1) `HealthOutcome::Blank`
  fired twice (09-04 18:33, 09-05 00:38) with no placeholder line in the
  journal window, so the watchdog declined to act by design (incident #40)
  and nothing would auto-recover a genuinely dead shell; (2) two unrelated
  crash-loops (09-09 00:16 xdg-desktop-portal-hyprland segfault with an
  amdgpu VM-context teardown across the session; 09-10 06:40 pipewire
  socket death under quickshell, one clean restart).
- Owner: none yet. Next: the User decides whether gap (1) gets a read-only
  review of `health.rs`'s Blank arm (Opus) and whether (2) is Aoide's at
  all; nothing dispatched.


## 11. Three-host acceptance (Osaka orchestrates, Yomi develops, Sakaki works)

- Assigned by the User via root (seq 229/235, 2026-09-12). Root oversees
  acceptance; Fable runs the gates in-session; "ready" stays blocked while
  any host path is incomplete. Direct SSH never counts as Aoide
  orchestration (gate 4); operator grant administration over ssh is
  recorded separately from orchestration evidence.
- Baseline, fresh 2026-09-12 18:15Z (`aoide node pull osaka` / `sakaki`,
  both ok, keys preserved, no re-pair): yomi-strix local, aoide 0.0.22
  store `0mg1rj1m…` (generation 204); osaka ssh://khoa@192.168.1.201
  paired/verified, aoided active, aoide 0.0.22 store `lp4chwsg…`; sakaki
  ssh://khoa@192.168.1.202 paired/verified, aoided active, same
  `lp4chwsg…` (both remotes predate P-CX-4 and the doorbell channel).
- Gate 1 (inspect): PASS. Rosters via `node list`; binaries, services and
  grants via read-only ssh (`node pull`/`status` expose no remote
  revision). Before-state grants: yomi → osaka read,spawn,message; yomi →
  sakaki read,spawn; osaka → yomi-strix read,spawn; osaka → sakaki
  read,spawn; sakaki → yomi-strix read,spawn; sakaki → osaka read. No
  autogate anywhere. Evidence: scratch `grants/before-*.json`.
- Gate 2 (tagged mail every direction): FIRST FAILED GATE. Two exact
  causes, both live-proven, tag `g2-1789236939-5021`:
  (a) the aoided unit PATH lacked `ssh`: the daemon's outbox drain fails
  for every loopback-door node ("spawn `ssh` … No such file"), backs off
  60s, and that hold-off also silences the CLI's own attempt (`mail
  outbox` showed tries 0 because the bookkeeping lives in `link.json`); a
  send timed 0.16s after expiry tunnelled and got a real verdict. Fixed on
  main 1d93184 (`pkgs.openssh` on the unit path), NOT deployed.
  (b) both far doors refuse yomi-strix: "paired and validly signed, but
  `allows` does not include `message`". Required directional grants (seq
  235): yomi: sakaki message; osaka: yomi-strix message, sakaki message;
  sakaki: yomi-strix message, osaka message, osaka spawn. Every `node
  allow` mutation, local and over ssh, was refused by the harness
  permission classifier on 2026-09-12; nothing applied, nothing revoked.
  The User runs them on each host's own console (`aoide node allow <node>
  <cap> on`). After-state and fresh verification pending.
- Gate 3 (cross-host incoming-mail wake): not started, needs gate 2. The
  local interactive channel run (§3) is its prerequisite and is in
  progress.
- Gate 4 (Osaka launches disposable workers on Yomi and Sakaki through
  `aoide node spawn`): not started; needs sakaki → osaka spawn.
- Gate 5 (checkpoint/handoff to an explicitly identified Osaka
  orchestrator with a real task, ack, predecessor idle, reconnect
  recovery): not started.
- Capability lacking a scoped mechanism: a per-node conductor autogate
  (nothing in `aoide node` or the schema flips one; only the send gate
  exists). Reported, not invented.
- Consumer side (root, seq 237-242): the three hosts are dxflake consumers
  and the User pulls and rebuilds; Spark is the SOLE dxflake editor
  (Aoide pinned at 1d93184, Osaka default Sonata, one import of upstream
  `modules/default.nix`, nested core subflake bound; pushed dxflake
  29481a9). Root confirmed both aoided user-service paths carry
  procps+openssh and the Osaka toplevel evaluates. Sakaki's full eval is
  blocked at `modules/dendrites/melete.nix` (`meleteSeed`: git object
  `b3c6bcb5…` unavailable) — the Melete owner's pin, not changed here.
  Runtime grants remain NOT applied (classifier denial, seq 241); no
  all-host acceptance is claimed.
- Osaka tracking (root, seq 265-268): the conducted shells on Osaka ran
  real Claude processes with zero native children because
  `~/.claude/settings.json` carried no hooks; a root agent ran the
  existing `aoide hooks install claude` there (backup kept, no restart);
  native enrolment still waits on a genuine hook event. Gates 3-4 on
  Osaka read against that. Sonata meter/power widths were a runtime-copy
  drift (340 vs the shipped 360), corrected on disk by root; no source
  patch.
- Osaka → yomi letter 5ba9a44d (minted 21:11Z, palette/livery override
  topic) NEVER LEFT Osaka (root, seq 273): outbox `tries 0`, link.json
  `lastOutcome: curl failed`, backoff 60. "curl failed" is the exact
  spawn-failure string of the door POST (`storage/src/commands.rs:314`),
  so the aoided unit PATH on the dxflake pin (procps+openssh) lacks
  `curl`: same class as the `ps` and `ssh` incidents; one-line unit-path
  fix on main, deployed by the User. Not received or fetched here.
- The Osaka worker (silent-grove, `conduct-3796858-1789009882`) now
  coordinates directly with this session on the dxflake Sonata Rose Pine
  recolor (`livery.override` reaches baked Stylix/Hyprland but not the
  runtime Quickshell seed, per its letter); it owns the dxflake consumer
  side and its runtime findings; Aoide/Lyra core stays here; authored
  Sonata colours preserved; no duplicate palette implementation before a
  contract is agreed (seq 275).
- Flagged, unexamined: routable door urls are recorded (yomi's record of
  sakaki `http://192.168.1.202:8710/`, osaka's record of yomi
  `http://192.168.1.175:8710/`) against the loopback-only rule.

## 12. Eidolon as a first-class harness (Yomi default)

- Authorized seq 229. Brief `p-eidolon-adapter-brief.md` (Opus, read-only,
  2026-09-12; both repos untouched). Ownership proposed: Rust half
  (`protocol`/`conduct` profile) this lane; Nix half (dendrite, package,
  host line) the existing eidolon worker (session `63290e1c…`, staged
  uncommitted files; coordination letter seq 233 filed, unanswered).
- Findings: the `AgentProfile` table fits (one entry, `pi` as template).
  Eidolon has no outbound hook or event surface (in-process broadcast bus
  only); the machine-readable surface is the presence registry
  `$XDG_RUNTIME_DIR/eidolon/<id>/meta.json` (id, pid, log, cwd, model,
  title, busy) plus a doorbell socket; session ids are deterministic;
  `.eid` logs are framed binary (no say/tool/context); `resume` takes a
  log path; the TUI is Helix-modal (`i` then Enter), so the suffix-only
  `submit_key` needs one closed `compose_prefix` amendment; native
  `eidolon send --from aoide` exists (written for Aoide), parked as a third
  transport. Stated unsupported: hook phases, awaiting/permission
  summons, subagents, context meters, skills, any write under
  `~/.config/eidolon/`.
- Slices: S1 profile entry → S2 meta.json as `TranscriptSpec` → S3
  `compose_prefix` → S4 Nix (eidolon worker) → S5 resume (needs a live
  two-session proof) → S6 native send (parked). Nothing dispatched.
- RULED (User via root, seq 241/245/247/248): Eidolon is installed and
  first-class onboarded/profiled on Yomi and SELECTED for Yomi remote A2A
  worker spawning through the existing spawnAgent/spawnPath mechanism;
  not a replacement of every terminal shell; no new generic spawn-default
  option; Osaka/Sakaki stay Claude. Native `eidolon send --from aoide` is
  preferred over any keystroke path (`compose_prefix` dropped). Capture
  is unsupported until a producer export exists: `eidolon log` goes
  through the writable `Session::open` path (can repair/truncate a live
  journal), `busy` is TUI-only (headless drive never sets it), TUI
  adoption does not refresh presence, no child-parentage contract.
  Owners: Fable = the Aoide adapter (profile, readiness, send arm, resume
  mapping, capture consumer); the existing eidolon worker = packaging
  (dendrite, package, host line) and its `~/eidolon` edits; a root
  executor = producer export + headless busy + adoption refresh in
  `~/eidolon`, after the worker confirms its files (contract relayed to
  `self/eidolon-worker`, unanswered). Root's required slices replace the
  ones above: E1 profile + bounded metadata (pid, deterministic native
  id, stable petname, log path on the record, same-cwd disambiguation,
  moved-wrap resumption) → E2 lifecycle readiness with a real registered
  child record → E3 native send behind the one gated door (draft
  unchanged, deferred→idle wake once, failed write stays armed) → E4
  explicit native-id ↔ log-path resume mapping → E5 capture from the
  producer export, missing facts explicit → E6 parent graph from explicit
  native child evidence only. Brief rev 2 DONE (2026-09-12): the locate
  pid-widening question is withdrawn (records key on the native presence
  id, the codex_app precedent, existing profiles untouched); the child
  record reuses the codex_app Observed/Unknown reconciler and the
  existing pid-ancestry walk; headless eidolon never sets `busy`, so
  non-TUI registrations are `unknown`, never idle. BLOCKER R3 (verified,
  `server/src/a2a.rs:962-979`): the A2A spawn path runs `conduct --agent
  a2a` and writes the raw prompt into the PTY socket, bypassing profile
  dispatch — an eidolon TUI in normal mode would submit an empty
  composer, so `spawnAgent = "eidolon"` is dead on arrival until a
  bounded E0 routes the spawn prompt through profile-aware delivery;
  `server/` had no owner in root's table. RULED (User via root, seq 259):
  R1 YES, an explicit resume mapping by log path persisted through the
  ledger, the native id stays the identity, no silent unsupported
  fallback, path provenance validated before launch; R2 confirmed,
  native `Queued` = accepted for later delivery, armed and not stamped,
  retries deduplicated on the native receipt; R3 E0 ASSIGNED to Fable,
  bounded to `server/src/a2a.rs`: the opening turn goes through gated,
  profile-aware native delivery after positive child discovery and
  readiness, every other profile's behaviour and goldens preserved; R4
  confirmed, the Yomi `spawnAgent` line switches only after E0+E1+E3 are
  tested, packaging may land separately. E1 approved with the native
  presence id as the lookup and no `TranscriptSpec` widening.
- LIVE TARGET (root reviewer seq 260, re-verified read-only 2026-09-12):
  eidolon already runs on yomi-strix. Roster petname plucky-comet is a
  generic shell wrap (`aoide conduct --agent shell -- bash -l`, pid
  3513862, window 0x5b14c0fe76c0) whose bash runs
  `~/.local/bin/eidolon` (pid 3514983); presence `khoa-253b` carries pid,
  log path, cwd, model `claude-cli:opus`, title, `busy:false`; the
  roster has no eidolon record; `command -v eidolon` resolves to a
  DIFFERENT nix-store binary than the running one. First acceptance:
  this session enrols as agent `eidolon` keyed on its native id, parent
  = the wrap via pid ancestry, the wrap's petname preserved, no
  duplicate, no restart, no injection; native send resolves the
  executable from the live pid, never bare PATH.
- Slice order and writers: E1a protocol profile LANDED bc1b05f (reviewed
  PASS, protocol 143; `--wake` verified real; broke two conduct goldens
  listing the known agents, repaired by E1b's first commit) → E1b conduct
  presence reconciler + child record + readiness LANDED df0530e+637666b
  (conduct 706; review FAIL on hermeticity/logPath doc/real fixture ids,
  fix-up commit in flight; enrolment PROVED live, below) → E3
  native send arm → E0 a2a.rs → E4 resume mapping → E5/E6. Brief rev 3
  DONE (Opus, read-only, scratch `p-eidolon-adapter-brief-r3.md`, 752
  lines): E1a gains a `native_send` argv field shaped like `resume_args`
  (`None` for claude/kimi/pi; the one predicate E0/E3 read); two rev-2
  errors withdrawn — `canonical_state("unknown")` is `"idle"`
  (`protocol/src/state.rs:29-37`), so a non-TUI owner cannot be deferred
  by state and the safety comes from the transport (a maildir write
  never touches a composer); `meta.json` is pretty-printed, so `tail`
  re-emits it as one compact line. Native send facts: exit 0 = accepted
  not consumed (`delivered to` vs `written to …inbox`), no receipt id
  exists, a repeat send deposits a second file by design; the nix-store
  `eidolon` is a launcher shim execing `${EIDOLON_BIN:-~/.local/bin/
  eidolon}`, so the executable resolves from the live pid. E3 constraint
  (root seq 265): a succeeded deposit is never re-sent; a retry re-runs
  only the native socket notify; no exactly-once claim. E0: one call
  site (`a2a.rs:1193`), bounded wait copying `stamp_spawn_origin`'s
  poll, timeout = audit error + today's raw write + Task `submitted`.
  E4: resurrection argv is `eidolon tui --session <path>`. E7 RULED (root
  seq 270): after E3/E0, a bounded slice routes an A2A FOLLOW-UP to a
  positively identified eidolon child through the same gated native
  delivery (`decide_send_action`, `a2a.rs:726-733` today refuses it),
  preserving paired signature, capability and conductor approvals and
  every other harness predicate; tests: a permitted peer delivers once to
  the correct child, denied/unsigned/unpaired deposits nothing, a
  dead/stale child refuses, a queued notify retry never redeposits; no
  allow-all predicate. Readiness evidence is the producer observation on
  the record (busy, TUI-or-not), never the canonical display state; the
  live pid's `/proc/<pid>/exe` is the definitive executable.
- E1b proof of enrolment (required before any "complete", seq 270): the
  reviewed E1b commit built from a pristine worktree, an ISOLATED aoided
  (own `AOIDE_ROOT`/`AOIDE_DAEMON_SOCKET`, own `XDG_RUNTIME_DIR` whose
  `eidolon/` entry links to the real presence tree; the reconciler only
  reads `meta.json` and pings the sock with eidolon's own liveness op),
  reporting the isolated stage's wrap record and eidolon record (native
  id, pid, model, title, logPath, parent = the wrap), live daemon
  untouched (socket inodes, MainPID before/after), no injection, no
  restart. Deployment stays the User's gate.
- E1b PROOF RESULT 2026-09-12 22:14Z (store `yrq21pj7…-aoide-0.0.22` from
  pristine worktree 533d89e, isolated aoided under `/tmp/aoide-eid`, its
  `$XDG_RUNTIME_DIR/eidolon` a symlink to the real tree, the live wrap
  record seeded read-only): the isolated daemon's own reap tick enrolled
  `khoa-253b` — `agent:eidolon kind:agent state:idle pid:3514983
  model:claude-cli:opus title:"ng" cwd:/home/khoa logPath:…/sessions/
  1789234755480.eid parentSessionId:conduct-3513862-1789234665`, petname
  minted; the wrap record and `plucky-comet` untouched. Live guard
  identical before/after: aoided.sock/shellbridge.sock inodes 76468/76466,
  MainPID 3460303, presence dir (inbox empty, meta.json sha, sock inode,
  pid start time) unchanged — no injection, no restart. The reconcile's
  ping answers in ~5 ms against the 250 ms budget. Isolated daemon stopped
  by its own pid. Proof is of the production path; the fix-up touches
  tests/docs only, re-run if `resolve_parent` changes. Rig: scratch
  `rig-e1b.sh`. STAGED + PROVED, not deployed. Conductor absence on the
  live desktop (root seq 445/447, 2026-09-13) is this deployment gap: the
  live aoided predates E1b; no unpublished Eidolon code exists; b6d453f
  APPROVED (root seq 492); origin/main = 535241f (pristine builds: e00568c
  → store qr7bdir8…, b719571 → store icw40km1…, letter fanout included);
  pin via dxflake to 535241f / icw40km1…, activation the User's; post-rebuild acceptance = one agent=eidolon record for the
  native pid parented to the wrap, no duplicate, node in graph.json and
  the conductor, state follows the busy flag, no reap while the pid lives.
- BUILD BREAK found by this proof: S2 fdb6682 added `sources` to
  `SessionRecord` and four conductor test fixtures never got it, so the nix
  checkPhase (`cargo test --workspace`) failed for every commit
  fdb6682..fdbea59 while per-crate runs stayed green; fixed 533d89e. Any
  dxflake pin must be ≥ 533d89e. Every conduct/storage brief now adds
  `cargo test --workspace --no-run` (compile only).

## 13. Ownership graph (project inheritance, lineage, sanitized spawn ancestry)

User requirement via root, 2026-09-12 (seq 291/292; explicitly authorized,
no further proposal approval for the bounded fixes). Owner: Fable
(implementation), coordinated with capture S3b and the Eidolon conduct
files; root's two READ-ONLY audits (`eidolon_acceptance_review`:
project/parent inheritance across local/remote/app/subagents;
`chiyo_config_fix`: live roster/QML actions) are not duplicated.

- Requirement: Aoide groups by ownership; a spawned session inherits its
  parent's project; app-controlled uncategorized agents stop exposing
  unusable kill / already-ended errors; the widget shows LIVE agents only
  (root's live-only requirement reaffirmed); historical project sessions
  stay in separate saved/history views. Truthful per-session actions —
  never hide a valid idle task, never kill the shared app process.
- Ruling (seq 292): `graph/model.rs:47` and `who.rs:467` group on the
  record's own project/cwd only. Add ONE shared effective-project resolver:
  own explicit project > owner's effective project > own cwd anchor;
  dangling/cyclic parents safe; a stored project stays an explicit choice
  (no copied child project); clearing an override resumes inheritance;
  owner reassignment propagates. Reuse the existing hook/subagent parent
  edges. Codex native `parent_thread_id` is parsed (`codex_capture.rs:295`)
  and ignored (`codex_app.rs:704`): the next lineage slice applies a
  matching native parent edge preserving `kind:app` and native identity,
  granting NO kill authority. `server/a2a.rs:1116`: clear the daemon's
  `AOIDE_SESSION_ID` for a spawned child beside the origin clearing, keeping
  legitimate local inheritance. `window.rs:333`: the ambient fallback must
  resolve an existing LIVE local record, never a stale id. Remote
  ownership needs a qualified node+session and a project mapping; never
  insert a foreign owner as a local parent. Live symptom: app children
  gentle-crest/spry-dune/stark-reed lose their parent edge although root
  silent-heron's project is aoide.
- Tests required: parent project outside the cwd anchor; nested no-cwd
  children; explicit child override and clear; parent reassignment;
  missing/cyclic parent fallback; stale A2A env; Codex unrelated vs native
  children retaining the app refusal.
- A2A prerequisite (seq 289, root's Osaka proof): `aoide node spawn`
  reached Osaka and returned `submitted` (a2a-1572083-1789251676), but the
  child sat in the native trust dialog for `/home/khoa` (the server's
  default cwd) and inherited `parentSessionId` dcb327d0… (the User's own
  Claude, from the service env) — wrong ancestry. Needed: bounded spawn
  cwd/project-root support, sanitized inherited harness context, readiness
  before the prompt (E0 territory). Not a reason to broaden permissions.
- UI slice (root-owned, seq 293/294): Sonata `conductor.qml`/`SessionMenu.qml`/
  `widget-structure.md` — 0aea55e (on origin): roster shows only
  working/awaiting/stopped/idle; kill disabled with an honest hint for
  app/ended/unknown/subagent; details/project retained. No backend change.
  An explicit `unknown` observation still needs the backend if a failed scan
  retains the record.
- Order: capture S3b (session_store kind gate; LANDED badfc7a, reviewed
  PASS) → S-A resolver + roster callers (LANDED e746e72, PUSHED 3b168ce:
  `model::effective_project_for`, `SessionView.effective_project`
  local-only, conductor per-project count; nine tests; CONTRACTS §4 names
  the stored-vs-effective distinction; built-binary proof on an isolated
  root: the seq 316 child-outside-anchor shape groups under its parent's
  project, evidence `/tmp/aoide-own-{base,sa}/ev`) → S-B sanitized A2A
  ancestry + bounded spawn cwd + live ambient fallback (LANDED d252b55,
  review PASS, fix-up b9af6e8, PUSHED d252b55) → S-A2 publish additive
  `effectiveProject` on graph.json session nodes and `aoide session
  --json` rows, derived by the one resolver, stored `project` untouched,
  local rows only, CONTRACTS §4 (LANDED 387f8ba, review PASS; root
  approved seq 337; widgets read it, never re-derive) → S-C Codex native
  lineage (executing, root priority seq 398: `parent_thread_id` →
  `parentSessionId` on the app record, nickname fills an empty title,
  kind stays `app`, both refusals pinned, fourteen tests, designer
  fixture; dispatch `p-own-sc-dispatch.md`; LANDED 4fa382e, review FAIL on
  one HIGH — same-tick mutual captures passed a frozen-snapshot cycle
  check — FIXED ece83a7 (captures merge first, lineage candidates one at
  a time against the live roster; review PASS), which also carries root
  seq 404: the conductor groups only `kind == subagent`, so the record,
  graph.json node and `session --json` row publish the native thread role
  as additive `nativeRole` from the capture, kind untouched; designer
  mailed the key and fixture, teaches the grouping helper to nest
  `nativeRole == subagent`, never every parented fork; root seq 463
  2026-09-13: still not nested on the live desktop — three-layer gap:
  the live daemon (build 4) predates 4fa382e/ece83a7 and publishes zero
  app rows, the checkout and live conductor.qml both still group on
  `kind` only, and the live composed copy is Sep 7 so the designer's
  tree edits are not placed; designer asked for the rule + the working
  animation (fixed footprint, parent and nested children) proven on the
  synthetic fixture; daemon deploy + placement are the User's gates) → S-D remote owner
  qualification (design + `link` refusal) → E3. Brief DONE (Opus,
  scratch `p-ownership-brief.md`): R1 accepted as ruled (rung 2 above rung
  3; `session project` pins a mis-grouped row), Q2 `effectiveProject` on
  graph.json NOT published yet, Q3 `node spawn --project` deferred (A2A
  wire change, User), Q4 nix option for `AOIDE_A2A_SPAWN_CWD` is a
  separate module commit, Q5 a native parent naming a tracked non-app
  record IS applied.

## 14. Projects as an app, Mesh view in both temples (User scope seq 305-307)

- Requirement (User via root, 2026-09-12): project creation/editing as easy
  as a desktop project app — name, multiple folder roots through the native
  picker (no custom filesystem browser), connected hosts chosen in the same
  flow; a standalone New project entry that needs no session right-click;
  a Mesh section in BOTH Sonata conductor and terminals showing the whole
  connected mesh with live sessions grouped by host, local host included;
  Projects view retained; titles/petnames/harness/model/ids in details;
  capability-aware actions. Offline/unreachable hosts stay visible but their
  cached sessions are never counted or painted live (last-seen/unknown);
  node-scoped ids; remote rows never take local pid/window actions; project
  host membership is organizational, never a grant or implicit pairing;
  host-specific roots are never inferred by copying a local absolute path.
  Backend first through the aoided bridge; no QML-owned registry (rule 7).
- Root audits (seq 306/307, read-only, done): `node_list.rs` already reuses
  `who.rs` `probe_nodes`/`build_mesh_node`/`build_local_node` — one shared
  refresh for both widgets, no per-widget probe, no sweep per frame; live
  evidence: Chiyo unreachable since 2026-08-28 yet its cached child
  `SessionView.presence` reads `online` — child presence must be gated by
  the enclosing host presence/freshness. `SessionMenu.qml` already has
  `FolderDialog` and a removable multi-path editor; `createproject` is a
  `sessionAction(record.sessionId)` and `shellbridge.rs` creates-then-
  assigns, so zero-session creation needs a project-scoped mutation seam,
  never a faked session id. The shared mesh renderer must not pass remote
  rows through `terminals.buildRows` (it dedups/focuses on the local
  `windowAddress`). Discovery candidates stay separate from the connected
  mesh. Local folder dialog cannot browse a remote filesystem: manual
  remote path or an existing remote root, shown not-validated until checked
  through the host. Match by node identity + session id, never by display
  name or cwd string.
- Phases (backend first; each one executor commit, reviewed, serialized by
  the integrator; edit/test only for agents):
  - M1 storage/CONTRACTS (additive): `Project` gains optional selected
    registered-host references with host-local roots where configured;
    legacy empty = local-only. Zero-session project create/edit through the
    existing multi-root project API (`project` commands) and one
    project-scoped bridge op for the widgets. Tests: zero-session create;
    multi-root edit preserves assignments; an offline selected host is
    retained. Owner: Sonnet executor, storage + conduct; after S-A lands
    (both touch `who.rs`/roster).
  - M2 mesh projection: one daemon projection (reusing `who.rs` roster and
    `node_list.rs`) that groups live sessions by host with node-scoped ids,
    host presence/freshness enclosing child presence, cached sessions of an
    unreachable host excluded from live counts, rows minimally enriched
    (title/model/kind/parent when published, nothing invented). Tests:
    duplicate ids across hosts; offline cached `working` excluded from the
    live count; real idle live retained; empty/disconnected mesh. Owner:
    Sonnet executor, conduct; after M1.
  - M3 UI (both temples): persistent Projects/New project entry + collapsible
    Mesh above empty-state content; New project = name + folders + chosen
    connected hosts → Create, then add host/folder/session; shared mesh
    renderer; remote rows capability-aware; both widgets share one refresh.
    Serialized behind the widget-design session's reservation on
    `conductor.qml`/`terminals.qml`/`SessionMenu.qml` (§13 UI half + card
    redesign, root-launched). Owner: UI writer named when M2 lands.
  - M4 preview verification (long names, wrapping, compact menus) and live
    acceptance on a deployed build — the User's gate.
- Brief DONE (Opus, read-only, scratch `p-projects-mesh-brief.md`): M1 =
  additive `Project.hosts:[{name,roots}]` (name is the registered
  `Node.name`; roots verbatim absolute strings, never existence-checked or
  inferred), ONE flag `--host <node>` on the existing `project add|edit|
  remove`, bridge wire command `projectaction` (create|edit|removehost, no
  session id anywhere), graph.json project node `hosts` when non-empty;
  M2 = the Chiyo defect fixed at the one fold point (`build_mesh_node`
  cache arm: cached rows under an unreachable host read `last-seen` or
  `unknown`, `done` survives), enrichment only when published,
  `aoide node list --mesh` (no sweep, candidates absent by construction,
  `id` = `<node>/<sessionId>`, live/cached counts) staging
  `state/stage/mesh.json` for both temples (R1, precedent `aoide usage`;
  put to root); defaults on the five questions taken. M1 LANDED 780818a
  (+ fix-up b98954b, PASS); M2 LANDED 06b2982 (+ fix-up c71810b: the
  document emits `schemaVersion`, the contract example matches the
  writer; PASS; pristine build exit 0). Both staged, not pushed (held
  set), not deployed. M3 is the designer's once the Mesh section data
  is on a deployed binary; M4 after M3.
- Order against the rest: Osaka conductor/wake gates and S-B stay ahead;
  M1 after S-B leaves the conduct crate, M2 after M1.

## 15. Scoped mail addressing (session · project · role; User scope seq 364)

- Coordination (root seq 471/472/474, 2026-09-13): root Codex owns a
  bounded client Subject/To/Cc fanout lane — new `client/src/letter_send.rs`
  + a small `handle_mail_send` branch in `client/src/commands.rs`, new
  `storage/src/letter.rs` + its `lib.rs` export, and the same-node local
  delivery routing fix; AOIDE-LETTER/1 structured signed body inside the
  existing signed text, canonical Header unchanged, one envelope per real
  recipient, per-recipient results, legacy readers see the raw body. Those
  seams are reserved to root until handoff — handed off seq 483-485 (review
  running, see §19). ML1 is NOT dispatched: it
  waits on the seam ruling (seq 393) and on that handoff, and asks that
  every fanout recipient pass one resolve-then-file point with `to.name`
  a bare mailbox so the resolver slots in front. `Mark.held` is dropped
  from ML1: root's pre-delivery hold gate owns holding.

- Requirement (User via root, 2026-09-13): a mailbox today is a free
  role-named endpoint under a node, so a petname and a mailbox name are
  different things and neither is per project or per session; the User
  wants BOTH scopes. (1) A session recipient is displayed and resolved by
  petname/title but bound to the stable native session identity; (2) a
  project inbox survives sessions, hangs off the project record, and is
  read by several participants with independent cursors; (3) role
  mailboxes stay as stable orchestration endpoints with their binding
  visible in Details, never as unexplained aliases. One canonical mail
  store and protocol — scopes are address resolution, not a second mail
  implementation. Typed CLI/UI targets disambiguate session vs project vs
  role; existing `<node>/<name>` addresses keep working; an alias resolves
  to its stable destination BEFORE filing. Addresses are host-qualified —
  the same project name on two machines is never one identity; a
  cross-host project inbox has one explicit authority host, no implicit
  replication. Project membership grants no access; the doorbell notifies
  subscribed/participating readers, never the whole mesh. Delivery,
  fetched, and acted stay separate; a shared cursor never swallows another
  reader's unread (per-mailbox per-reader cursors in `mail.rs` are
  reused). `--from` is attribution only and a node signature never
  authenticates a claimed session name — display never overstates
  identity. Send UI: choose Session or Project, no knowledge of internal
  role mailboxes required.
- Roles (User decision via root, seq 365 then 375): the normal product is
  Session + Project with zero roles configured. A role is OPTIONAL and
  belongs to a project (`Aoide/reviewer`): a stable address that survives
  a change of hands, with a visible assignment to one or more sessions;
  the brief specifies assignment/handoff, unassigned-role behaviour, and
  reader notification instead of assuming or broadcasting. No role
  registry; existing free-named mailboxes get an explicit compatibility/
  migration mapping, never guessed identity. UI shows project, role, and
  current assignee; the default reply target resolves from the source
  binding.
- Required tests: petname collision / relabel / resume; dead target vs
  enduring inbox; two readers with independent unseen; project rename and
  host-qualified names; cross-host reply target; role alias shown.
- Status: ARCHITECT brief written (`p-mail-scope-brief.md`): one store,
  typed prefixes `session:`/`project:`/`role:` on the name half resolving
  to flat names `session-<nativeId>` / `project-<n>` / `role-<p>-<r>`, bare
  names compat forever and never auto-mapped, header unchanged, the only
  storage addition `Mark.held`; slices ML1 resolver → ML2 doorbell → ML3
  roles (one new command path, `mail role`) → ML4 published binding.
  Seam put to root with five questions and one flagged gap (a bridge
  path for the send UI); no executor before agreement, ML1 after M1. Sits on top of
  §14 M1 (`ProjectHost`, `--host`) — no redesign of M1.

## 16. Livery stage seed bypasses the resolver (Osaka evidence, seq 395)

- Defect: `modules/facets/quickshell/default.nix` seeds `activeSongLivery`
  from the song alone and never invokes `lib/livery.nix` `resolve` /
  `overrideMap` / `slotPatch`, so host overrides and an independently
  selectable palette never reach the stage; the resolver header claims
  both fanouts agree ahead of the implementation. Evidence at the dxflake
  pin 3b168ce: Stylix base00 `191724` against a live stage palette bg
  `f2ebde`. Recorded by osaka/dxflake-codex in dxflake
  `docs/livery-consumer-handoff.md`.
- Owner: the upstream stage-seed repair in `modules/facets/quickshell/
  default.nix` + `lib/livery.nix` is Yomi core (one executor, after the
  conduct lanes S-C and M2). Osaka Codex (`osaka/dxflake-codex`) STARTS the
  dxflake architecture and the Lyra consumer implementation now (User,
  seq 407) — bounded phase owners, an isolated branch, and evidence per
  phase, no duplicate upstream writer; gate tests continue as separate
  acceptance, not a blocker. Sonata and generic Lyra/core defects stay
  upstream. The runtime palette contract remains the gate before any
  second palette implementation; the User activation gate is unchanged.
- Status: brief DONE (`p-livery-seed-brief.md`): `overrideMap`/`slotPatch`
  have no callers; fix = `lib/livery.nix` `stagePatch` applied to the
  committed livery.json document, the facet seeds from a `writeText` of
  it, plus one `livery-fanout` flake check; six eval assertions true with
  dxflake's override attached, no-override case byte-identical for all
  five songs. Defaults taken on the three questions (runtime writers
  `rice stage`/`reload` stay unresolved — follow-up; nix not jq; keep the
  base16 assert). LANDED 2c41661 (staged, not pushed, not deployed):
  `stagePatch` + `checks.livery-fanout` (built green), proof 1 all true
  (seed palette.bg == Stylix base00 == #191724 with the osaka override),
  no-override case byte-identical to the live stage twin for sonata and
  round-trip identity for all five songs, nixfmt clean; review PASS (two
  LOW: the checks header count, fixed; a host setting `override.hot` on
  a song whose committed palette has no `hot` key gains that key in the
  staged document — mirrors the option-set behaviour, no reader breaks,
  no host does it today). Log entry appended. Activation of the seed
  remains the User's rebuild gate. Follow-up (own lane): the runtime
  writers `rice stage`/`rice mode`/`reload` need a published override
  source before they can apply the tier. Follow-up:
  the songbook livery.json twins of `rice.nix` are unchecked.

## 17. Agent Client Protocol (ACP) client in core; widget builder as its UI (User scope seq 403/405)

- Requirement: the standard Agent Client Protocol (agentclientprotocol.com,
  v1; stdio transport stable, streamable HTTP draft) — never a renamed
  local JSON-RPC. Roles per the official architecture: Aoide is the ACP
  CLIENT; the selected coding harness exposes the ACP AGENT endpoint
  (native or a verified adapter). The reusable client — initialize/
  version/capability negotiation, native stdio agent subprocess with clean
  JSON-RPC stdout (never PTY injection or echo), `session/new`,
  `session/prompt` with streamed `session/update` (messages, plans, tool
  calls), `session/cancel`, permission requests, connection/error
  lifecycle, `session/load` only when the agent declares it (no fictitious
  resume) — lives in portable Aoide core as an OPTIONAL harness
  capability/adapter with no Nix or QML dependency. CLI/conductor, the
  Lyra widget builder, and later web/Eidolon surfaces consume the SAME
  aoided policy/session/event interface; no parallel wire engine in Lyra
  or QML. No second session authority: ACP session ids map to native
  Aoide session/project ids, parent edges and reconnect identity kept.
  Files/terminals are negotiated client capabilities routed through the
  existing execution policy and audit log; previews stay isolated; the
  live-draft grant stays scoped. Widget operations (load/resize/anchor/
  palette/annotations/render/screenshot) remain existing Lyra tools via
  CLI/MCP or explicitly namespaced extensions, never invented standard
  methods. Context passed: selected widget paths, source/diff, preview
  dimensions/livery/dependencies, annotated screenshot (image only when
  the agent capability allows). Truthful pending/running/completed/
  cancelled tool state; declared-save and rebuild gates unchanged.
  Optional agent connection — manual preview works with no harness; TUI
  fallback stays for harnesses with no verified endpoint; no attaching to
  an arbitrary already-running TUI. Cross-host: the owning node hosts the
  connection and the authenticated mesh routes requests; no network ACP
  port. ACP does not replace store-and-forward mail, project/role
  addressing, or missing transports; idle wake/deposit/fetch semantics
  stay separate. The agent-side facade exposing Aoide-managed agents to
  external clients is a distinct lane, parked until the client lane is
  proven. Pin the protocol/schema version and one maintained SDK (Rust
  fit evaluated); a compatibility matrix names only harness adapters
  actually tested on this box.
- Owners: Fable main (this register) owns the core design; the preview
  owner (`widget-preview-fable`) owns the consuming UI; ONE wire owner.
  Architect first (Opus, design only), plan and seams to root before any
  executor; does not block the preview foundation (P1/P2 and their
  fixes).
- Phases: A0 architect brief → A1 core client + harness proof (one
  adapter, real initialize→prompt→stream→tool/permission→cancel/complete)
  → A2 event/UI integration (builder as client front-end) → A3 cross-host
  acceptance. Acceptance also covers supported load/reconnect,
  unsupported capabilities, errors/disconnect, project mapping, and a real
  render/edit/vision loop.
- Status: ARCHITECT brief written (`p-acp-brief.md`): client in
  `aoide-conduct` (`graph/acp.rs` + `commands/acp.rs`), connection in the
  `aoide acp` wrap, agent session id on `harnessSessionId`, kind `acp`,
  permissions through `pending.json`, terminal capability false in A1,
  hand-rolled newline JSON-RPC with the spec v1 schema vendored as a test
  fixture (SDK 2.1.0 evaluated, rejected for its async runtime); only
  `kimi acp` verified as an endpoint on this box. Plan and five questions
  put to root; no executor before agreement.

## Carried backlog (verified status, never implicitly done)

- Interactive child launch environment (root diagnosis, seq 371): the
  root-launched preview and designer TUIs inherited `NO_COLOR=1` from the
  orchestrator's machine-output environment, so their windows rendered
  colourless while tracking was intact. Acceptance: a user-visible TUI
  launch never inherits the launcher's output suppression — `env -u
  NO_COLOR`, the terminal sets `TERM`, no forced colour or `TERM` hacks;
  fixing a live session is a same-native resume coordinated at a safe
  checkpoint, never a kill or a duplicate launch.
- Reload source is the composed song, not the checkout (designer, seq
  374): `sync_song_widgets` copies from `AOIDE_ROOT/song/songbook/<song>/
  widgets` (the composed copy) into `run/qml`; checkout edits reach the
  live dock only once placed there, and `rice declare` copies that dir
  INTO the checkout — nobody runs it while other writers have tree edits.
  Placement into `~/.aoide` is the User's gate.
- AoideOS/Lyra portability: unverified; folds into 4(e).
- Preview-harness reaping is filename-scoped (root ricing audit, seq 339):
  `song/src/reap.rs` classifies every `*Preview.qml` launched by `qs`/
  `quickshell` as stray on `rice mode stage`, hyprlock likewise — the new
  `WidgetPreview.qml` canvas included. A filename never establishes an owned
  stale process; the sweep must be ownership-scoped (the preview's own
  isolated root / pid ancestry). Slice on the song crate, reviewed, before
  any stage or preview acceptance; the designer runs no blanket sweep to
  validate. Owner: integrator dispatch after the preview stack lands.
- Desktop Codex detection off Linux: UNRESOLVED (seq 190); fd evidence is
  the only positive ownership signal and it is `/proc`-shaped. Owner: 2.
- Osaka/Sakaki deployment: sakaki re-paired 2026-09-10; no deploy.
- P-M5 remainder: M5b free (was held behind M5c-1); M5c-2/3 proof-gated.
- Mneme/Melete plans: the Mneme shared-memory proposal page is an
  uncommitted draft outside this register's scope.
- Onboard bootstrap exception: ACCEPTED by root. Rationale: the clone must
  register its project before any daemon exists;
  `register_bootstrap_project` is the single non-daemon writer, sole caller
  `onboard`.
- aoided has no safe no-op for unknown arguments or `--version`: any such
  invocation runs the daemon against the ambient root (incident 2026-09-10:
  a new-build `aoided --version` replaced the live socket path; repaired by
  `systemctl --user restart aoided.service` on the User's word the same day,
  roster intact). Bug, backlog; until fixed every test brief sets the
  isolated env on EVERY invocation and never calls bare `aoided`.
- Deployed runtime on yomi-strix: aoided 0.0.22 (store `vvvwpzkq…`), predates
  everything above. Staged ≠ proved ≠ deployed until activation.
- USER PRIORITY 2026-09-12 (seq 281/283, supersedes the refactor-first
  reading): (1) Osaka development working — root's `chiyo_config_fix`
  enables dxflake Osaka's existing `aoide.openai` dendrite (ChatGPT/Codex)
  as a separate small config commit (dxflake 155909e pushed, not
  activated); (2) root's `eidolon_acceptance_review` proves BOTH conductor
  input and cross-host doorbell on Osaka with its OWN disposable fixture
  (`osaka-ring-proof-20260912`; no User sessions, no rebuilds, no
  duplicate of the yomi rig); (3) only then the architecture refactor.
  Fable keeps the Eidolon consumer/capture lanes and the mail-status fix;
  no overlapping Osaka test spawn from yomi.
- PROPOSAL QUEUE — dxflake refactor (seq 281/283; proposal only, gated on
  (2) above; MAIN TARGET IS DXFLAKE, run on an Osaka alternative
  branch/worktree, coordinated with the existing Osaka consumer worker, no
  duplicate trees; Aoide changes limited to necessary upstream consumer
  interfaces): the User authorizes direct Eidolon changes for composable
  extension/controller APIs; an optional Aoide management extension;
  Lyra/Nix workflow contracts (progressive repo-contract loading calling
  the existing `aoide`/`lyra` CLI, rebuild admission retained); project
  saved working sets; remote restore queues and web surfaces. Read-only
  critique delivered to root (scratch `p-eidolon-extension-critique.md`):
  saved session = derived view (ledger `logPath` + exit call, E4 scope);
  cross-host resume must be idempotent on a caller key carried by the
  existing outbox (a2a spawn on absent contextId is N-retries = N agents);
  the extension seam is one `native_send` consumer (E3) + the existing
  dendrite. No implementation until the proposal settles.
- Test tempdir names overrun the unix socket path limit:
  `test-support::unique_tmp` builds `aoide-dispatch-<tag>-<pid>-<nanos>`
  under `TMPDIR`, so socket-binding tests (`shellbridge::
  n_concurrent_herald_pushes_all_land_in_the_ledger`, and ~71 more under a
  session-scratchpad `TMPDIR`) fail under any `TMPDIR` longer than bare
  `/tmp`; the nix check phase passes only because its `/build` is short.
  Fix = a short unique name (or a ``-style socket dir
  separate from the data tempdir). Until then briefs use
  `/tmp/aoide-t-<lane>` and accept that one failure by name.
- MAIL DELIVERY STATE (User requirement via root, seq 274/275; owner:
  Fable, after root's read-only brief; files coordinated against E3):
  a filed letter must report queued / retrying / refused /
  accepted-awaiting-signed-ack distinctly from local filing, with the
  transport failure and reason visible before any per-entry attempt,
  retryability and next attempt where known, refusal terminal until
  remediation; delivered / fetched / acted stay separate. Shape ruled:
  `outbox` joins the entry with `link.json` (no new store), `Outcome` ok
  for a durable spool with `data.delivery` carrying the state and
  `nextAttemptAt` (earliest, not guaranteed); one projection in client
  `commands.rs` reused by `send` and `outbox`; CLI first, the widget
  consumes the same backend; pending letters preserved, no purge or
  resend storm. Tests: `tries 0` + link error; refusal precedence;
  accepted pending ack; concurrent removal unknown; post-spool error
  queued + status unavailable. Proven cause 2026-09-12: the daemon
  drain's back-off in `state/outbox/<node>/link.json` silences the CLI's
  own attempt and the listing shows `tries 0` with no outcome.
- Orphan `channel-<id>.sock` after a wrap exits: CONFIRMED live
  (`channel-db-wrap-2.sock`, 2026-09-12); `sweep_orphan_sockets` covers
  only `session-*.sock`. Unowned.
- Routable door urls in node records (yomi's sakaki `http://192.168.1.202
  :8710/`, osaka's yomi `http://192.168.1.175:8710/`): flagged, unexamined
  against the loopback-only rule; needs a reading of what the record's url
  means before any change. Unowned.

## 18. Pack-independent widget icons (Iconify identifiers; User seq 424/428/433)

- Order (root seq 424 → 428 → 433, newest supersedes): Iconoir is the
  preferred editor-control look, Phosphor stays a selectable collection;
  a widget selects an icon by Iconify `collection:name` identifier or
  supplies its own local SVG/image; pinned IconifyJSON collection data and
  import tooling resolve the SELECTED icons into local Qt-compatible
  assets (aliases, dimensions, transforms handled — the raw body alone is
  not enough); no network at render, only selected collections/assets
  ship; the picker exposes collection + searchable names, custom files via
  the existing native picker, and the saved choice and declaration retain
  collection/source/license metadata; pack licenses preserved for the
  pinned version; monochrome assets take the livery tint, intentional
  multicolor art is preserved; actual Qt rendering verified with Iconoir
  and one other set at small sizes (16/20/24, light/dark). Sonata's
  bespoke art is kept. Editor coverage (seq 424): select/cursor, hand/pan,
  text note, arrow/rectangle/highlight, undo/redo, zoom, fit, open/save,
  copy/details, send-to-agent; text stays on primary Send and mode
  controls; tooltips + shortcuts on compact tools.
- Placement: paint, so `lyra` — bridge first (`lyra icon …` resolves and
  catalogs), one reusable QML component second, picker third; rendering
  stays separate from picker/catalog.
- Owner: ONE implementation owner (a Sonnet executor lane, lyra crate, new
  files only while the preview lane's lyra edits are uncommitted; the
  integrator applies the `mod`/registry lines); the preview owner
  implements the picker on top; the designer does the visual comparison
  and owns anything under the live songbook. No duplicate assets writer.
- Status: brief DONE (`p-icon-brief.md`): `lyra icon collections|list|resolve`
  (resolver ports Iconify's build algorithm; selected SVGs + `catalog.json`
  with license/version/mono; pinned data = hashed `fetchurl` at
  `pkgs/iconify-data`, never shipped); tint = MultiEffect alpha mask of a
  livery-coloured rectangle for mono, plain `Image` for multicolor; 18
  controls mapped to verified names in both collections. Facet carries
  `IconCatalog.qml` DATA only; `Icon.qml` is a song helper (CONTRACTS §0)
  via designer handoff. I1 DONE in the tree (uncommitted): `icon.rs`
  1521 lines, 14 tests; `pkgs/iconify-data` hashes reproduced (iconoir
  663723 B, ph 4566288 B); 20 Iconoir controls resolved into
  `modules/facets/quickshell/icons/` with catalog + vendored licences;
  repeat resolve a byte no-op; registry lines held as integrator steps.
  Executor judgment: viewBox captured post-swap per the fetched
  build.ts — review confirmed the executor against the fetched source.
  Review PASS-WITH-FIXUPS: HIGH — a `..` collection or name in an
  identifier escapes `--out` and prune deletes outside it; MEDIUM — mono
  detection ignores `style=` fills. FIX-UP DONE and re-reviewed PASS by
  the same independent reviewer (icon.rs 1843 lines, 18 tests): plain-name
  validation in `parse_selector` (`validate_plain_name`), canonicalized
  confinement before every write (`ensure_write_target_within`) and
  before every prune removal (`path_is_within`; refused entries surface
  as `skipped`), style-aware `detect_mono`, four named tests. The
  reviewer graded by reading: without the registry wiring the module is
  not compiled, so the integrator runs the 18 tests with the wiring
  applied at commit time. Minor gap: the bare-attribute allow-list lacks
  `inherit`/`transparent` (pre-existing, real Iconify data never emits
  them). `catalog.json` schema belongs in a facet
  `icons/README.md` at I2, not CONTRACTS §4. Integration follows the
  preview set (shared lyra registry files).
  Five questions to root at defaults.
  Source work only: no rebuild, no activation. Mail receipt is not
  completion — done means implemented and visually verified.

## 19. Conductor TUI redesign (User priority via root seq 452/458) — lane RESERVED

- Direction (root audit seq 458): the TUI starts on Graph with seven tabs
  and no Home or mail view; Compose is a terminal send, Pending is
  send/A2A approvals, not mail interception. Redesign: vintage message
  board with readable livery contrast and restrained square borders;
  Home (resume projects, live sessions, mesh health, review/failed mail
  counts), Rooms (project/session mail threads with explicit
  participants/hosts and a reply composer), Sessions, Mesh, Review,
  searchable History; graph stays the project/session detail. The
  chatroom is a VIEW of existing mail, never a second store; human
  viewing never consumes an agent inbox cursor nor emits a fetched
  receipt; every letter shows author → recipient, time, id and its real
  queued/delivered/fetched/error state. Hold/review/edit needs daemon
  mail policy before local visibility or remote dispatch: default
  delivery unchanged until the user selects a review scope, a durable
  hold survives TUI closure, stable ids + revision checks on release,
  an edit is an attributed revision preserving the original, release
  exactly once; delivered signed letters are never rewritten or
  "unsent" — correction or reply only. Direct terminal input stays an
  explicitly labelled separate action. Readable 80x24 and 120x40,
  keyboard + mouse, real terminal visual checks. No deployment.
- Owner (User via root seq 460): root Codex and its subagents own the
  design and the ENTIRE `pkgs/aoide/crates/conductor` crate; no Fable
  worker there. Added specifics: persistent project-tree sidebar, live
  AND past sessions, overall mouse support. Root edits/tests only and
  hands off for integration; no concurrent commit/push. Only Fable
  touch to date: four `native_role: None` fixture lines in ece83a7 — an
  additive `SessionRecord` field forces the same one-line collateral in
  four exhaustive test literals there, handed to root as exact lines.
- Active overlaps to reconcile in the plan (not conflicts yet): §15
  scoped mail (`Mark.held` is the one storage addition proposed for
  hold/release; project inbox + roles ARE the Rooms model — the TUI must
  consume that seam, not invent one); the mail-status projection
  (queued/retrying/refused/accepted/delivered/failed, client
  `commands.rs`) is the per-letter state column; §17 ACP routes agent
  permission asks through `pending.json`, so the Review screen and the
  existing Pending view share one queue; a cursor-free human read path
  does not exist yet (`mail read` advances the reader cursor, `--reread`
  too) and belongs to the storage crate; the conduct/storage crates are
  single-writer, so mail policy work serializes behind the lanes above.
- Status: RESERVED; root builds in the tree (board/mailview/eventview
  wired, 97 tests at the last stable slice, graph pan in progress); User
  extension: formal Subject/To/Cc with real CC fanout and per-recipient
  results, Home as a centred nvim/BBS start page with the Aoide logo; the
  mail seams are listed under §15. Per-crate checks only for conductor
  until root hands off; not ready for integration. Root reports (seq 480,
  2026-09-13) the full email form in source (To/Cc/Subject/body,
  Reply/Reply all/Forward, explicit Send, recipient picker) and the
  Subject/Cc backend (LetterContent marker inside the signed text, no
  Header change, fanout through the existing scalar handler, per-recipient
  status, a partial send locks the duplicate resend, canonical local
  hostname routing fixed); backend 3+2+9 and conductor 103 tests green per
  root; uncommitted, unreviewed, not activated. The new mail flags need
  the rebuilt daemon (User gate). Integration queue and ownership reported
  to root (seq 482). HANDOFF RECEIVED (seq 483-485): conductor/**, the
  client letter seams, the storage letter module, MAIL.md; root holds the
  conductor COMMIT for a small composer follow-up (seq 486: no field-edit
  labels, recipient tree beside the composer, click-to-append To/Cc). An
  independent Sonnet review runs over both sets (A backend, B conductor);
  A commits first by exact pathspec once PASS, B after the follow-up and a
  delta re-review. Old-daemon direct-RPC new-flag drop is a separate
  compatibility hazard; the conductor path is local-handler + `ring`.
  REVIEW RESULT: set A backend PASS-WITH-FIXUPS → COMMITTED b719571
  (11 files + CONTRACTS delta; MEDIUM wording: the resend guard is a
  UI-only in-memory flag, MAIL.md says so; LOW: no isolated scalar-path
  hostname-routing test, outcome text says self/ for hostname form). Set
  B conductor FAIL: graphview.rs:691-733 draws the tag chip only for
  Project nodes, pre-existing test
  `ui::tests::graph_panel_draws_nodes_edges_and_tags` fails (114/1) —
  root fixes it in the composer follow-up (seq 494: tags kept as ASCII
  `[tag]`, test updated, 122 tests), then delta re-review and
  commit by pathspec.

## 20. dxflake architecture migration (User order via root seq 510, 2026-09-13)

- Order: start NOW; isolated branch/worktree; baseline first (per-host eval +
  the current rebuild error); then incremental migration with host
  evaluation and package/config parity; conventional `default.nix`
  aggregation instead of the custom walker; opt-in lane imports
  (NixOS/HM/Darwin); shared aggregates/overrides below host; per-host
  user/package selections; portable upstream Aoide/Lyra consumption;
  compact songbook (shared covers, pure-Nix palette files); runtime
  integrations upstream; build only, no switch, no activation; pin/lock
  ownership coordinated (Spark sole editor); unresolved proposal details
  reported, never invented.
- Osaka owner status: root's assignment (yomi msgid 07575e63) never left
  the yomi outbox (tries 0 — the live daemon predates the ssh/curl PATH
  fix); relayed verbatim into osaka's mailbase as osaka seq 23; no armed
  reader on `dxflake-codex` (cursor 0), kind-clover is a Codex desktop
  thread (`codex-app-unsupported`), osaka's architecture worktree has no
  commits ahead of main. Replacement with distinct ownership (the User's
  instruction) = this lane.
- Active branch: `fable/dxflake-architecture`, worktree
  `/home/khoa/worktrees/dxflake-fable` (yomi), from origin/main 1cf1721;
  the User's `~/dxflake` checkout untouched.
- Ownership: Fable lane = structural migration (composition, aggregates,
  lanes, per-host selections, upstream consumption); Osaka dxflake-codex
  (if woken by the User) = Osaka-host consumer specifics; Spark =
  flake.lock / aoide pin; User = rebuild/activation. Ownership notes filed
  on osaka (seq 24 to the palette worker, seq 25 to dxflake-codex).
- Status: ARCHITECT (Opus) writing `p-dxflake-brief.md` (recovered
  proposal, unresolved choices, slices D1..Dn with drvPath parity); D0
  baseline executor (Sonnet) running — evidence to
  `scratchpad/dxflake-baseline/baseline.md`. Acknowledged to root (seq
  512). Flag: dxflake pins Aoide rev 3b168ce, far behind 535241f.
