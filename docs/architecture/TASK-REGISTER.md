# Task register

The canonical shared register for Aoide/AoideOS work. Root
(codex-integration) orchestrates and reviews; the Fable session coordinates
implementation; executors, reviewers and read-only architects run as Eidolon
`ollama:deepseek-v4.1-flash` instances, one per role. One owner per slice,
no duplicate executors. This file is a ledger: it records current state and is
updated in place as work moves; history lives in the commit log and
`docs/Aoide-Wiki/ingest/log.md`.

Fields per entry: status · owner · depends on · evidence · next.

## 1. Session QoL (menu actions, multi-root projects, kill resolution)

- Status: implementation and combined build COMPLETE; isolated pre-activation
  acceptance of build 2 PASSED (2026-09-10, third run; runs 1–2 failed on the
  brief's own shell wrapper, not the build). Activated: yomi runs aoided
  0.0.25, which carries this lane. Live desktop acceptance on the real roster
  PENDING.
- Owner: Fable (integration); root reviews.
- Depends on: nothing.
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
- Next: desktop acceptance on the real roster — kill from a card,
  `sessionAction` reply and timeout, create and edit a project.

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
- Next: the live check with the app open on two threads, against the
  activated 0.0.25 runtime (which carries P-CX-1/2/3/2b/4), is the User's
  gate.
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
- Next: the human end-to-end wake with a mail send per the runbook, the
  tool-busy and GUI cases, onboarding docs for the flag.
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
  shape; 184/184 reproduced, traversal fixes real. Fix-up delta re-review
  (2026-09-13): HIGH-1 RESOLVED (run/qml holds copies, refresh is
  checkout→copy only, a write-through test proves the checkout stays
  byte-identical; `resolve_widget_abs` maps song paths lexically to the
  checkout, never through the root); MEDIUM-3 RESOLVED (`lyra` resets
  SIGPIPE); HIGH-2 PARTIAL — `check_root` skips the live-dir refusal
  when `XDG_RUNTIME_DIR` is unset and never canonicalizes a symlinked
  `--root`; both returned to the owner with named tests (seq 517), then
  integration by pathspec; 189/0/3, golden 54, no foreign hunks. Round 2
  reported (seq 537: live dir from `attest::daemon_socket_path`, candidate
  canonicalized through its deepest existing ancestor, two new tests,
  191/0/3) — delta re-review PASS (08:11Z). The owner then applied root's
  seq 506/507 follow-up on top (seq 557: one camera transform, middle-drag
  and held-Space pan, pointer-anchored Ctrl+wheel zoom, Iconoir toolbar
  copied from the facet's `icons/` tree, computer-use evidence
  f1-verify.md) — round-3 delta review running; owner told hands-off
  (seq 563). Landing = ONE commit with icons I1 (the canvas copies
  `modules/facets/quickshell/icons/**` and `default.nix` runs the lyra
  tests in the nix build), path list `commit-preview-icons.txt`; icon
  wiring applied in the tree by the integrator: 209 passed / 0 / 3,
  golden 57. The combined commit died on a full root filesystem (08:45-08:55Z; nothing committed, log tail repaired). Then two user-facing rail bugs surfaced (owner seq 586/588: rail actions run the DEPLOYED `lyra` from PATH, which has no `preview set`; and the HIGH-2 fix-up refuses the canvas own `--root` because the child env repoints AOIDE_DAEMON_SOCKET under the root). Root seq 590/592: do not ship a dead rail; owner released for ONE corrective set (P7a audited patches + LOW, `lyra` path in preview.json, root validation against the stable real daemon dir with no blanket starts_with, dark icons, P7a-2 styling, real rail-click evidence), then re-review, then the combined commit, pristine build, push. Owner released seq 595. Acceptance additions (root seq
  548, User): lock/unlock state icon; declared/resolved widget
  dependencies (available/missing/unknown + sources — read side a lyra
  command, shape a CONTRACTS entry); editable isolated livery colour
  state through the existing resolver/control API; whole-canvas arrows;
  native picker at the widget folder; anchor/margin arrows. The owner`s
  follow-up plan (seq 539: explicit camera, pointer-anchored zoom, Space/
  middle drag, Iconoir rail via a copy of the facet `icons/` tree into
  run/qml, computer-use verification) is in progress; safety fix-ups are
  gates, never completion. Backlog: no bin resets SIGPIPE (one-line SIG_DFL
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
  APPROVED (root seq 492); origin/main = af0fd74 (pristine builds: e00568c
  → store qr7bdir8…, b719571 → store icw40km1…, letter fanout included;
  2cc85ea and af0fd74 aoide + song checks exit 0);
  pin via dxflake to 535241f / icw40km1…, activation the User's; post-rebuild acceptance = one agent=eidolon record for the
  native pid parented to the wrap, no duplicate, node in graph.json and
  the conductor, state follows the busy flag, no reap while the pid lives.
- BUILD BREAK found by this proof: S2 fdb6682 added `sources` to
  `SessionRecord` and four conductor test fixtures never got it, so the nix
  checkPhase (`cargo test --workspace`) failed for every commit
  fdb6682..fdbea59 while per-crate runs stayed green; fixed 533d89e. Any
  dxflake pin must be ≥ 533d89e. Every conduct/storage brief now adds
  `cargo test --workspace --no-run` (compile only).
- E5 LANDED 5ee097b (2026-09-17, `feat(trace)`): the producer export is
  eidolon's TRACE — every journal record mirrored as one JSON line to
  `<stem>.jsonl` beside the `.eid`, named in presence `meta.json.trace`
  (`~/eidolon` 536c425 on `theme-roles`, NOT pushed — Noah's remote; the
  same commit makes SIGTERM = SIGINT in `drive`/repl and adds `--deadline
  <secs>` with a `TurnDeadline{secs_left}` wrap-up nudge). Aoide reads it:
  `protocol/agents.rs` locate/tail/extractors (model, title, say, the tool
  in flight, context tokens; `tool_use.input` is a JSON STRING, parsed a
  second time), `conduct/graph/eidolon.rs` state = the LAST record
  (`TurnSettled` idle, `Cancelled` stopped, `AskUser{answer:null}`
  awaiting, else working; no trace = the presence rule), and `aoide
  session trace <id> [--tail N] [--follow] [--json]`. Contract:
  CONTRACTS.md §4 + `docs/architecture/EIDOLON-TRACE.md`; the `timeout`
  SIGTERM post-mortem in `EIDOLON-HEADLESS-DISPATCH.md`. Verified against
  real traces from the new build (protocol 153, conduct 796, cli 42 with
  the golden at 82). The INSTALLED eidolon has none of the producer until
  the User rebuilds it; until then every eidolon record stays on the
  presence rule and `session trace` answers with the taught no-trace error.
- E5b ping-back, RULED (User, 2026-09-17: "parent should be able to hear
  its child spawns") and LANDED a5e0f63: the autogate is reciprocal — a
  parent hears the children it spawned (child-of-target, the fourth rule
  in `Conductor-Channel.md`). `conduct/graph/pingback.rs` runs after the
  post-lock `sync_eidolon_sessions()` in every `Door::Daemon` reap: per
  eidolon child it reads the trace, picks ONE line by priority (settled /
  cancelled > died mid-turn > asking > wrapping up > failing > ten minutes
  silent), claims it under the stage lock in `state/stage/pingback.json`
  (`{<child>: {seen, silentAt}}`, at-most-once per record id) and rings
  the parent's doorbell (`[eidolon <petname>] …`, clipped to 80 chars,
  control chars stripped, a leading `/` or `!` spaced; headless parents
  get the submit keystroke; shell parents, done parents and interactive
  parents without a channel are skipped; audit `delivered … (autogate-
  child)`). `sync_eidolon_sessions()` now also returns the records
  dropped this pass so a child that exited between ticks still reports.
  Review found and fixed one defect the executor missed: a clean eidolon
  exit removes its presence dir, so the locate returned nothing and a
  settled headless run was never reported — `eidolon_transcript_locate`
  now falls back to the record's `logPath` (`<stem>.eid` → sibling
  `<stem>.jsonl`), passed from both the live and the dropped path.
  Verified protocol 154, conduct 812 (15 pingback tests), cli 42. Armed on
  the live runtime — aoided 0.0.25 runs this code and the installed eidolon
  answers `eidolon log` — but no live delivery to a real parent is proved.
- E5c MIGRATION (2026-09-21): the mirror generation is superseded. Upstream's
  `48bdf24` review reverted the mirror and re-landed the journal's own
  read-only export — `eidolon log --json <journal> [--after <id>]`, opened
  `open_readonly` since `0432133` — and dropped `--deadline`/`TurnDeadline`
  (SIGTERM parity plus the launcher's own `timeout -s INT` is the wall-clock
  budget; that wrapper is Unix-only and is not Aoide's mechanism). Aoide's
  reader follows: `protocol/src/agents.rs`'s `EIDOLON_PROFILE.transcript`
  answers a mirror only while it is CURRENT (a strictly newer journal is
  positive evidence it is frozen), else the journal when `eidolon log --help`
  proves the read-only generation before each export (no capability answer is
  cached; a runtime replacement between probe and export can still race),
  else `meta.json`; the export runs under four bounds (5 s wall clock, a 1 MiB
  retained window, a 64-entry/8 MiB memo, 16 reader slots released only when a
  reader thread exits), discards rather than infers (deadline, non-zero exit,
  output that never reached EOF, no reader slot), drops its cursor whenever the
  journal cannot be identified (a same-path replacement included), and writes
  one audit line per process per reason. `feed::path_identity` is the ONE
  identity authority and is now crate-visible for that reader.
  `conduct/src/graph/eidolon.rs::read_presence_trace` resolves the trace through
  the same capability every other consumer uses (`TranscriptSpec::locate` +
  `trace`), so a producer that publishes no mirror feeds the state fold too; the
  presence rule is unchanged wherever no trace is reachable. Verified protocol
  171, conduct eidolon 28 (unmodified), parallel. The `trace` docs
  (`EIDOLON-TRACE.md`, CONTRACTS §4, Session-Graph, EIDOLON-HEADLESS-DISPATCH)
  are restated for the two generations. Real-upstream (`d9ff700`) integration is
  the next step; the installed runtime is untouched.
- Conductor-state eval kit `evals/conductor-state/` (same commit): the
  per-node VV classifier's testing kit — `eidolon log` → features → one
  delexicalized state line → condition / decision / risk (the CIA leg at
  stake, the DAD harm if wrong); 106 hand-labelled seeds from 40 real
  journals (14 hinge rows where hand ≠ rule), rule-labelled synthetic
  rows, four runners (verba-volantia, jevlike byte, jevlike + frozen
  Qwen2.5-0.5B, RLCD-style zero-shot Qwen2.5-1.5B) scored by one script
  into `report.md`; devshell `nix develop path:…/evals/conductor-state`.
  FINDING: every trained lane learns the rule (VV 99–100% on plain seeds
  and synthetic, 30 ms/case); no lane beats the rule on the hinge rows
  (VV 21/21/50%, zero-shot 1.5B 0/14/36% at ~6 s/case) and VV's
  calibrated abstention accepts every wrong hinge row at ~98% — judgment
  needs hand-labelled training rows or relation phrases in the writer,
  not a bigger encoder. The secrets-node sketch (VV classifies, abstains
  below margin, the human gate opens on accept + corroboration from the
  other nodes' ledgers, the broker executes, a MAIL letter records) is a
  chart, not a lane: https://claude.ai/code/artifact/f2304e5c-0c94-435f-9fe9-570320a4e549

- Integration preparation: `docs/architecture/AOIDE-VV-JEV.md` records the
  `aoide do` shell-out boundary and JEV's separate drift/completion duties.
  `evals/oversight-readiness/` supplies synthetic prose-brief cases, not
  production labels or a runtime. CPU-only Linux/Windows measurements are
  required; macOS follows later. Model runtimes and adopted weights remain
  unbuilt.

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
  2026-09-13: QML half LANDED 2cc85ea (designer lane, review PASS,
  pristine build + song checks green) and the designer's leftovers
  (revealInPlaybill + left-click focus) LANDED af0fd74, pushed; still not nested on the live
  desktop — three-layer gap:
  the live daemon (build 4) predates 4fa382e/ece83a7 and publishes zero
  app rows, the checkout and live conductor.qml both still group on
  `kind` only, and the live composed copy is Sep 7 so the designer's
  tree edits are not placed; designer asked for the rule + the working
  animation (fixed footprint, parent and nested children) proven on the
  synthetic fixture; daemon deploy + placement are the User's gates) → S-D remote owner
  qualification (design + `link` refusal; the DESIGN half is ruled and
  landed in the remote sub-agents lane, §32: `remoteParent` is a second
  field beside a local-only `parentSessionId`, so no foreign owner is ever
  written into it; the `link` refusal half stays open there) → E3. Brief DONE (Opus,
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
- Status: LANDED eaafa6e (pushed): the facet publishes
  `song/declared/livery.json` (CONTRACTS.md §4) — the declared song's
  committed notes with `aoide.livery.override` applied in one `jq` run to
  two destinations; `commands::rice::notes_source` reads it whenever its
  `"song"` field equals the name being staged (the songbook otherwise),
  and `handle_mode_declarative` resolves that same field first, so bare
  `rice mode declarative` re-pins the declared song rather than the
  currently staged one. `stagingSong` is untouched, so a later bare
  `rice mode stage` still returns to the staged song. Follow-up closed.
  Live activation on osaka/chiyo remains the User's rebuild gate.
  Remaining: the songbook livery.json twins of `rice.nix` are unchecked
  (carried).


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

- Tokens-spent seam (designer, seq 542): no source carries per-session
  cumulative usage (`contextTokens` is the freshest turn`s input-side
  occupancy; Codex capture discards `total_token_usage`; the ledger has
  no usage keys). Proposed ADDITIVE session-record `usage`
  {inputTokens, outputTokens, cacheCreationTokens, cacheReadTokens,
  turns, source, sourceId, asOf}, cumulative per source transcript,
  copied onto the ledger line at end; conduct/storage single-writer
  slice behind ML1/ACP-A1 unless root ranks it; UI renders "—" until it
  lands. Sent to root seq 554.

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
- Deployed runtime on yomi-strix: aoided 0.0.25 (store `biwp6gbf…`), system
  generation 219. Staged ≠ proved ≠ deployed until activation, and
  deployed ≠ accepted until the live check runs.
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
  Root seq 525 (2026-09-13): the User assigns the widget designer
  (`widget-design-fable`) as design/orchestration lead for the TUI
  (left-anchored projects, circular/Unicode tab symbols, Base16); root
  stops visual edits; the graph worker finishes the spatial layout, then
  hands off. Reservation now: root + its subagents + the designer as
  visual lead; still no Fable-dispatched worker. Designer told to design
  first and edit conductor source only after root`s graph handoff is
  reviewed and committed. Root seq 536: the graph must be a RETAINED scene
  (stable world nodes/edges, dragging, positions kept across refresh,
  separate camera, unified render/hit transforms); root`s interim layout
  is not completion. Designer spec at
  `/tmp/aoide-widget-team/design/tui/spec.md` (tab symbols with code
  points/widths, Base16 slot→role, left-anchored Home, §6 retained scene).
  Gate question to root (seq 554): is the graph worker`s handoff still
  coming, or does the designer own the graph outright?
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
  SET B SHIPPED: root fixed the tag chip (ASCII `[tag]`), re-review
  PASS-WITH-FIXUPS applied (graphview `[tag]` mapper, DESIGN.md labels the
  graph layout INTERIM, not the retained scene) → 5c2b7cd (12 files) +
  a5100f7 (cli `conductor_integration.rs` follows the renamed "3 Agents"
  panel) → pushed, origin/main = a5100f7. Pristine build
  `/nix/store/1516hvzilh9r92iz5dgmgg8wgvw9k4wc-aoide-0.0.22` exit 0 on the
  second run; the first run failed on a PRE-EXISTING order/timing race in
  cli `graph_residency_p_d6.rs`: `start_run_loop("hand-edit")` leaves the
  resident tick thread alive for the whole test binary, its sweep re-stages
  graph.json into whatever `AOIDE_STAGE_DIR` is current, and
  `atomic_write_bytes_impl` names the temp `<stem>.tmp.<pid>`, so two
  same-process writers collide (rename ENOENT, reason
  stage-file-unreadable-or-unwritable). Handed to root (seq 614); fix
  candidates: temp suffix pid+thread/counter in storage `fs.rs`, or the
  hand-edit test isolates its run_loop. Designer visual pass (seq 606/608:
  Mark table, marked nav/tree/legends, Home logo band, graph CANVAS_PAD +
  camera centring, Past sessions hoisted to the sidebar root, 128 tests)
  under the SOLE-conductor-visual-writer rule (seq 605: not the deletion/
  confirmation/key-dispatch paths); first independent review bounced —
  TREE MOVED (Past semantics changed again mid-review). Lane rule now: the
  designer sends a "STABLE <n>" letter, I freeze the diff by patch-id,
  review, commit by pathspec; no conductor edits between STABLE and my
  reply. Root seq 612: every agent letter carries `--subject` (the
  a5100f7 store build has it; the installed CLI does not; deploying it is
  the User gate); stale mail-view date / date grouping / canonical session
  links = designer-tasked, serialized here for commit.
  Designer STABLE 1 (seq 647): visual pass frozen (10 files, 376+/183-,
  128 tests, fmt residue = root's 7 lines), Past node per project inside
  its fold + projectless "Active sessions" (live only) + one root Past for
  projectless ended sessions, shared constant for the name; graph still
  INTERIM. Review PASS-WITH-FIXUPS (two test gaps → next increment) →
  COMMITTED 5f59c11, pristine build
  `/nix/store/z8i4bf6a8xi8112a2jp7d255yw6dqz2c-aoide-0.0.22` exit 0;
  on origin/main. Slip recorded: the register push that followed carried
  5f59c11 before its build finished (green afterwards); register commits
  now wait for the build or push by explicit built SHA. Root 642/645
  (User): semantic Base16 colour for mail/logs and Base16 readiness across
  ALL conductor text/interaction states via one shared theme contract, no
  per-panel palettes, no colour-only meaning, NO_COLOR respected,
  light/dark screenshots — designer's next increment, after this review.

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
- Pin ownership (root seq 592): Spark unavailable; claude-mail proposed as sole dxflake pin/lock editor (seq 596: only `nix flake lock --update-input aoide` on the branch, only to a pushed Aoide SHA with a build proof, reported before and after; melete-src re-lock stays the User own request); awaiting root confirmation; no lock edit before it.
- D4 Aoide-side ARCHITECT DONE (`p-dxflake-d4-aoide-brief.md`): three additive outputs `nixosModules.default` / `overlays.default` / `lib.livery`, new `lib/modules.nix` spliced by mkHost at top level (BFS order preserved), delete the `inputs.aoide` read at nucleus/options.nix:352, one `mkBefore` on shellbridge.nix:73 so the yomi-strix toplevel drvPath stays P0; dxflake side P0 on yomi/sakaki, P1 named on chiyo; 5 questions at defaults. A4a executor queued behind the ship. D2/D3/D5 replanning under root 577 dispatched (Opus).
- D2 EXECUTED on the branch (uncommitted, review running): `aggregations.nix`
  deleted, 25 dendrites moved into desktop/hyprland/gaming/server aggregate
  dirs, floor of 10 shared dendrites, hosts import selected dirs + single
  dendrites, stylix/hyprlock plain `mkEnableOption` default false with
  explicit `true` on osaka/yomi, `dx.melete.enable=false` dropped from
  chiyo/osaka; parity P1 at derivation level (systemPackages order only);
  sakaki fails identically before/after on the mneme-src git object
  35b0a4a3 (pre-existing, separate from melete-src). Commit follows the
  review; then D3, then D5; A4a after root answers the D4 questions.
  D2 LANDED a537098 on the branch (pushed; review PASS, two LOWs: no
  report.md, two comment fixes in aoide.nix outside the file list); D3
  executor dispatched (`p-dxflake-d3-dispatch.md`, P0 required, evidence
  `dxflake-d3/`).
  D3 LANDED a165011 (P0 reproduced by review, PASS; flake.nix:182 comment
  fixup folded into D5). D5 dispatched on the brief defaults (Q1 username
  specialArg stays, Q2 sakaki provisional, Q3 openldap floor if fleet-wide).
  D5 LANDED 679e7af (review PASS-WITH-FIXUPS): users/khoa.nix x4 verbatim,
  soundconverter to osaka, openldap stays floor, packages not split (sakaki),
  yomi/openai docs fixed; system.path P0 x3, toplevel P1 by one inert
  authorized_keys line swap — root asked to rule (accept P1 vs 4-line
  lib.mkAfter follow-up). D2-D5 COMPLETE on the branch; A4a and the pin
  edit wait on root.
  ROOT 651/652 (2026-09-13 20:5x): dxflake ownership moves to the other
  Codex; this session hands over (branch at 679e7af, worktree clean,
  briefs + evidence copied to `~/worktrees/handoff-2026-09-13/dxflake/`)
  and dispatches no further dxflake edits. D5 users layout RETRACTED by
  root: one shared `users/khoa/default.nix` selected by host imports, the
  four per-host copies deleted, parity re-proved — Codex owner's
  correction; other D5 improvements retained. authorized_keys P1 accepted,
  no mkAfter. Pin ownership = the Codex migration owner. Aoide-side A4a
  exports remain this session's slice on root's word.
  Root audit 655/656 at 679e7af (filed in the handoff dir): user.nix/HM
  unconditional, no non-HM lane; mkHost still injects walked songbook +
  overlays and reaches input internals; global package list not a
  catalog; stylix floor import + config.aoide reference = compatibility
  debt; immich→nas-mounts undeclared cross-dendrite dependency; shelved
  _librewolf/_hyprpaper reference deleted options. Labels: legacy
  baseline preserved, not final architecture. Codex owner incorporates.
- Status: ARCHITECT brief DONE (`p-dxflake-brief.md`: current state, target
  tree, U1-U8 unresolved with defaults sent to root for ruling, slices D1
  aggregates → D2 home lane → D3a/b roles → D4 portable upstream (needs
  three Aoide exports: nixosModules.default closing over its own core,
  lib.livery.resolve output, overlays.default) → D5 selections → D6
  songbook requests). D1 LANDED a0156ea on the branch (pushed; not merged, nothing
  activated): four aggregates, flake.nix one line, README/AGENTS integral.
  Parity P1, not P0: only `environment.systemPackages` order differs
  (identical multiset, equal priorities, identical input-.drv set —
  independent review PASS-WITH-FIXUPS reproduced all three drvPaths and
  went to derivation level, `dxflake-d1/review.md`); P0 is unreachable
  under rule 7 because nixpkgs flattens imports breadth-first while the
  walker was flat. P1 acceptance asked of root (553/567/568). The
  review's HIGH (README still said auto-discovered) fixed before the
  commit, chiyo drvPath re-proved. D4 Aoide-side exports: ARCHITECT
  (Opus) dispatched for `p-dxflake-d4-aoide-brief.md` — the only slice
  not gated on U1-U3. Flag for the User:
  `hosts/yomi-strix/default.nix:17` sets `aoide.openai.enable` against
  the flake comments. D0
  baseline executor DONE — evidence at
  `scratchpad/dxflake-baseline/baseline.md`: chiyo/osaka/yomi-strix
  evaluate (drvPaths captured); sakaki FAILS eval — `melete-src` lock rev
  b3c6bcb5 missing from the local ~/melete checkout (branch rewritten),
  forced only by sakaki`s `dx.melete.enable`; re-lock is a Spark/root
  call. Acknowledged to root (seq
  512). Flag: dxflake pins Aoide rev 3b168ce, far behind 535241f.

## 21. Conductor role metadata, project conversations, mesh/pairing in the TUI (User via root seq 621)

- Requirement: the TUI lists each agent's assigned ROLE separately from
  its harness and model, shows its parent relationship and an optional
  role mailbox, and says "unassigned" explicitly; mail/conversations are
  project-scoped; mesh trust and pairing are manageable from the TUI
  through Aoide's own commands and the secrets capability, with secret
  references/grants/status visible and no ordinary raw-secret exposure.
  No new mandatory role scheme.
- Existing seams (what the TUI consumes; nothing here is a new store):
  `SessionRecord` already carries `agent` (harness), `model`,
  `native_role` (the harness's own notion: subagent/worker, ece83a7),
  `parent_session_id` (lineage, §13) and `project`; the ASSIGNED role is
  none of these — it is the §15 project-owned optional role
  (`Aoide/reviewer`), bound to sessions by `mail role` (ML3, not built)
  and published as a binding (ML4). Project conversations = the §15
  project inbox (`project:` typed target, ML1 resolver, not built) read
  with per-reader cursors; the TUI mail view is a VIEW of that store
  (§19), never a second one. Mesh trust = `node allow`/`node remove`
  (grants are global per node, §11); pairing = `pair`/`pair watch`/
  `pair reject`/`mesh pair` (windows redesign §5 QUEUED, codes never
  ferried); secrets = the `secrets` command family (`grant`, `revoke`,
  `pending`, `approve`, `dismiss`, `expose`, `watch`; Harnox-backed
  custody §9 is review-only, no migration authorized). `secrets status`
  reports broker-backed policy metadata without values or backend probes.
- Remaining implementation (ordered, one conduct/storage writer at a time,
  behind §15's seam ruling): (1) ML1 typed targets + resolver; (2) ML3
  `mail role` assign/unassign/handoff with the visible binding, additive
  `assignedRole`/`roleMailbox` fields on the published session document
  (graph.json/`session --json`), never on `native_role`; (3) TUI: role
  column + parent + role mailbox + "unassigned" from the published
  document (designer, after the seam lands), project-scoped mail threads
  from the project inbox, Mesh panel actions calling the existing
  `node`/`pair`/`secrets` commands through the local handler + `ring`;
  (4) secrets panel shows references, grant state and pending approvals
  only — the value path stays `secrets expose` under its own gate.
- Permission boundaries (binding for every brief): project membership and
  role assignment grant NO access — mesh trust is the node-level grant
  and stays global; a TUI action never bypasses the daemon gate (`node
  allow`, `secrets grant/approve`, `pair`) — it issues the same command a
  terminal would and shows the gate's answer; pairing codes are typed by
  the User, never displayed to or relayed by an agent; no raw secret value
  is rendered in any TUI surface, log, or letter; `--from` stays
  attribution, a role mailbox never authenticates its assignee; unknown/
  unassigned is displayed as such, never inferred from harness or model;
  cross-host: a project inbox has one authority host (§15), remote rows
  carry no local pid/window actions (§14).
- Operators use Eidolon executors and independent reviewers with Ollama
  `deepseek-v4.1-flash`; root inspects and lands scoped commits. The TUI
  operator slice covers pairing, node trust, configuration and secrets
  metadata/grants through existing commands; implementation and independent
  review are complete. Assigned roles and project-mail work retain their
  §15/§19 dependencies.

## 22. Orchestration graph: runs, work nodes, goals, typed editable edges (User via root seq 630/632/634)

- Requirement: the conductor Graph becomes real orchestration, not
  ancestry drawing. Rename "DAG" → Graph/Orchestration. Named runs inside
  projects; stable WORK nodes separate from executor sessions (objective,
  role/agent, artifact refs, acceptance criteria, state, attempts survive
  restarts); GOAL is a separate identity from the run and the orchestrator
  (goal + deliverables + explicit checks + an attached verifier session;
  several goals per run, several runs per goal; run ended ≠ goal
  achieved). Typed edges: depends-on / hands-off-to / reviews /
  references / contributes-to / verifies / coordinates; spawned-by and
  project membership are observational provenance, never editable claims.
  Free branching (fan-in/fan-out, shared prerequisites, cross-branch
  handoffs); tree is a LAYOUT preference, not a single-parent constraint;
  blocking dependency cycles diagnosed/refused, messaging cycles normal.
  Edits audited, undo/redo, retry = new attempt linked to prior evidence,
  downstream checks marked stale on upstream change; never kills/reparents
  a live process or replays mail. Mail overlay = real msgid/thread/state
  events. Authoritative task/run/edge/attempt records through aoided; UI
  scene positions separate; derived index disposable. CLI first, TUI
  consumes the same mutations. Neo4j reference = principles only, no
  dependency.
- User addition (2026-09-13, direct): rendered for agents, the whole
  forest of terminals, agents and project nodes is a confusing mess. The
  graph DEFAULTS to the currently picked node (project or session) and
  shows the whole tree/graph of agents and terminals it controls or is
  connected to — or nothing beyond itself when it is connected nowhere;
  never force a connection that does not exist. An explicit "ALL" view
  renders every project's graph in one canvas. Focus is a VIEW choice
  over the same authoritative edges (§22 semantics unchanged); belongs to
  phase 1 (designer-side view + selection) and the §23 GraphScene.
- Phases: 1 rename + visual tree/scene + factual session/mail edges +
  inspect/jumps; 2 persisted runs/work nodes/typed edges/goal criteria
  (backend + CLI + TUI); 3 attempts/retry/undo, artifact-bound
  verification, observed mail overlay.
- Owner: Fable = architecture and backend contracts (Opus architect
  dispatched: existing vs missing APIs, storage shape under CONTRACTS §4,
  acceptance incl. branch→join→shared prerequisite→review loop and
  adding a node during a live run); designer = layout/interactions; one
  writer per slice. Status: ARCHITECT RUNNING; nothing mutates the
  backend before the contract is agreed with root.
- Brief DONE (`p-orch-graph-brief.md`): all orchestration nouns missing;
  edges derived only (spawned XOR anchors); one `orchestration.json` +
  append-only `orchestration-ledger.jsonl`; graph.json gains one additive
  `orchestration` key; noun-first run/work/goal/edge commands; blocking
  cycles = depends-on only; §24 source ids as a local unsigned
  `Entry.origin` (LetterContent is deny_unknown_fields). Six questions to
  root with defaults; awaiting agreement.

## 23. Conductor architecture: capability plugins + Ratatui scene widget (User via root seq 636/637)

- Requirement (637): the conductor follows everything-is-a-plugin. Today:
  closed `Panel` enum, central key/mouse matching, app-wide state — no
  dynamic plugins exist and none are claimed. Target: a minimal shell
  (terminal lifecycle, pane placement/focus, action routing, navigation,
  theme) + capability modules (projects/sessions/mail/graph/mesh/
  settings) under conventional dirs in `conductor/src`, each with state,
  views, actions, subscriptions, tests, README/AGENTS, contributing
  through one typed contract (stable ids, display metadata/key hints,
  scoped event handler, render widget, entity-link resolver, required
  capabilities); shared primitives; no reaching into another module's
  state. Compile-time Rust modules (rebuild required) are distinct from
  runtime extensions (versioned manifest/protocol over the existing
  external-command seam; no dylib ABI, no plugin VM without design
  review). Prove with one bounded extension and its remove path.
- Requirement (636): graph rendering stops being a whole-world text grid
  through `Paragraph.scroll`; prototype `GraphScene` as a `StatefulWidget`
  with persistent scene/camera/selection outside render, Canvas layers
  for connectors, fixed card widgets with a shared transform and
  visible-node culling; ListState/TableState/Scrollbar for lists; scoped
  action handling (modal/text first, focused pane next), same action for
  key and click, geometry from the actual render; bounded dirty-redraw
  animation; TestBackend snapshots + real terminal proof. Report the
  retained-scene milestone, not spacing fixes.
- Owner: Fable = architecture integration (Opus architect dispatched:
  concrete tree, plugin contract, compile-time vs runtime scope, first
  proof extension, GraphScene prototype plan against ratatui 0.30);
  designer = UI contract; no overlapping edits. Status: ARCHITECT RUNNING.
- Brief DONE (`p-conductor-plugins-brief.md`): Panel enum + twelve
  touch points per panel audited; graphview rebuilds the whole-world grid
  three times per interaction (render/hit_node/node_order); target =
  shell/ + primitives/ + capabilities/<name>/ with explicit `all()`
  registration (automatic discovery would need a build.rs scan — refused,
  flagged); Capability trait (meta/handle/render/resolve/subscriptions/
  unavailable), render-time hit map so key and click share one ActionId;
  runtime extensions over the existing external-command door with a
  versioned manifest under `/extensions/`, no dylib/VM;
  GraphScene StatefulWidget + Canvas plan; slices S1 shell extraction
  (byte-identical TestBackend goldens) → S2 Log + remove path → S3
  GraphScene (§19 retained-scene milestone) → S4 runtime extension → S5.
  Five questions to root; #1 (who holds the crate now that the designer
  session is closed) blocks S1.

## 24. Letter provenance trails and honest ring status (User via root seq 639)

- Requirement: `ring`/TUI summaries explain each non-wake reason
  (working/deferred, interactive-composer skipped, stale reader,
  genuinely no armed reader) and never imply a wake from a stored letter
  or an injection. Provenance everywhere: agent list, Mesh host/session
  detail and Graph cards show the latest outgoing trail (subject →
  recipient title/petname or role/project mailbox, time, queued/
  delivered/fetched/failed), expandable to a recent timeline; letter →
  exact msgid/thread, endpoint → canonical node; bidirectional. To/Cc
  preserved, fanout copies distinct from the logical letter, recipient
  kind labelled; identity survives rename/restart and cross-host mirror
  dedup; envelopes with only a mailbox show ambiguous/unresolved, never
  an inferred session; role mail is not proof of which worker fetched it
  (receipt/reader identity when recorded). Read-only derived index over
  authoritative mail/session/project records; source ids recorded on
  future sends via the backend contract; old signatures never rewritten;
  spooled outgoing mail included.
- Owner: folded into the §22 architect (same graph/mail overlay
  contract); designer = surfaces. Status: ARCHITECT RUNNING.

## 25. Harnox secrets first-class and easy setup/management across Aoide (User via root seq 625/627)

- Requirement (625): Harnox is the first-class secrets backend, optional
  for unrelated Aoide functions; integrate existing Harnox capabilities,
  no second broker; runtime configuration on plain Linux without Nix or
  repo layout, Nix declaration optional and mapped to the same contract
  with explicit declared-vs-runtime ownership and no duplicate stores;
  the conductor uses the same CLI/daemon APIs for backend setup/status,
  secret references, supported grants/lifecycle (no TUI-only backend);
  states available/locked/unconfigured/unreachable reported accurately;
  values never in lists/logs/transcripts; no replicant server, no
  replicas/sync or credential transfer without a supported Harnox
  contract; reconcile with the latest Noah/Honey proposal (issue #1)
  before implementing. §9 stays review-only until then.
- Requirement (627): "easy" setup and ongoing management applies to ALL
  of Aoide — projects/multiple roots, participating hosts, pairing/trust,
  harness onboarding, roles, session defaults/resurrection, permissions,
  secrets, mail routing, UI preferences. TUI and CLI share one
  authoritative config contract; useful defaults; discoverable current
  settings; clear scopes (global/host/project/session) with effective
  values and inheritance shown; task-oriented edits; validate before
  apply with actionable errors; advanced controls retained; runtime
  fully usable without Nix, optional declaration maps to the same
  capabilities with clear precedence. No speculative universal config
  framework: reuse current command seams, improve concrete flows.
  Acceptance wording everywhere: "easy setup and ongoing management",
  never "simple config".
- Owner: Fable (Opus architect dispatched: inventory of today's config
  surfaces and command seams, the ownership/precedence contract, the
  Harnox capability map against the `secrets` family, first concrete
  flows); designer = TUI settings surfaces after the contract. Status:
  Custody migration remains review-only. The authorized operator slice has
  `secrets status` implemented and independently reviewed: metadata comes
  from the broker; unreachable or refused reads are errors, never an empty
  success. TUI configuration, pairing and grant controls are implemented and
  independently reviewed. Backend setup and custody migration remain separate
  work.
- Brief DONE (`p-easy-config-secrets-brief.md`): 28-row inventory; the
  existing `aoide config`/config.toml becomes a read/validate projection
  (scopes, `config explain`, runtime/declared/default ownership,
  `--dry-run` + consequences), `config set` never writes another domain;
  secrets = Harnox custody only + one new `secrets status`
  (unconfigured/unreachable/locked/available) + `[secrets]` section, six
  redaction points, existence-only `has` blocker carried; the latest
  Noah/Honey proposal is NOT in the repo — six confirmations before any
  custody work. Slices C0-C5 then S0-S2; six questions to root.

## 26. Mail transport: receipt loop and stalled outbox on yomi (root seq 660/663, 2026-09-13)

- Facts (read-only, 2026-09-13 21:2xZ): `~/.aoide/state/outbox/osaka/` on
  YOMI holds 16564 spool entries (+1519 empty `.tmp.3460304` files left by
  `aoide a2a serve`, the live daemon's child, during the disk-full
  windows). 16546 are `type: receipt` envelopes for only SEVEN distinct
  osaka-origin msgids, each re-minted ~2390 times by five reader
  identities (silent-heron, claude-mail, merry-comet, stark-crag,
  codex-integration), ~2050/hour from 2026-09-12 21:00 to 06:00 local,
  stopping when automatic mail wake was paused. All tries=0 (the drain
  never attempted them); the 7 refused entries are the Sep-12 probes that
  hit "allows does not include message" on osaka. Zero duplicate msgids:
  a producer re-mints receipts for already-fetched letters on every wake
  pass. 18 `type: letter` entries are the unique user mail to preserve.
- Acceptance (663): bounded resource behaviour — retries reuse the stored
  envelope, never append; bounded drain batches with backoff; permanent
  refusals visible; queue metrics (depth/age/attempts/errors); explicit
  archive/prune policy, no silent drop, no blind flush of the 16k, no
  deletion of unique mail; regression: a sustained refused peer creates no
  growing duplicates and one recipient cannot starve another. Codex
  `.tmp` marketplace bloat is an external app issue, not this cause.
- User addition (2026-09-13, direct): when writing or editing a letter to
  an agent, the sender CHOOSES whether to ring the doorbell or only queue
  the letter — an explicit send-time option in the CLI (`mail send`) and
  the conductor composer; today `mail send` files and `mail ring` wakes as
  separate commands with no per-letter choice at compose time. Folds into
  §24's honest ring status and §19's composer.
- Route: Aoide remote send yomi→osaka is dead (grant + queue). Direct ssh
  to osaka works (slow); the Codex owner reads `dxflake-codex` (cursor at
  the head). Handoff bundle unpacked at osaka `~/handoff-2026-09-13/
  dxflake/`; handoff letter filed as osaka seq 29.
- Owner: Fable (read-only investigation running; bounded fix by Sonnet
  executor on storage/client/server, one writer; root reviews). Status:
  INVESTIGATING; nothing flushed or deleted.

## 27. Rebuild checkpoint: Osaka + Sakaki (dxflake) and Yomi (Aoide) (User via root seq 664/666/667)

- Order (666/667 correct 664): finish current lanes to a coherent
  checkpoint (preview corrective set, conductor increments, transport
  fix), THEN prepare rebuild revisions and reproducible commands per
  host: Osaka and Sakaki on dxflake (Codex owner; Sakaki is a config
  target, no separate owner), Yomi on Aoide (this session: whole-system
  `.#nixosConfigurations.yomi-strix.config.system.build.toplevel` proof,
  not just `.#aoide`). Aoide architecture migration deferred until live
  Aoide/Lyra portability is proven on dxflake; only minimal additive
  exports before that. Missing pinned source objects → targeted
  owner-approved pins, no broad flake update. No activation.
- Yomi baseline proof: pristine-worktree toplevel build of origin/main
  c0f8fbb PASSED: `/nix/store/nqib1558436qfcbdz48jb3pb1wmlz1jb-nixos-system-yomi-strix-26.11.20260907.dc5d91f`
  exit 0 (command: `nix build .#nixosConfigurations.yomi-strix.config.system.build.toplevel --no-link --print-out-paths --no-write-lock-file --no-update-lock-file` in a clean worktree of c0f8fbb); NOT activated; repeated at the checkpoint SHA. Live system = l2pahyr… (foreign-eidolon
  variant, never the reviewed line).
- Checkpoint report must list: completed fixes, actual UI/build evidence,
  shipped SHA, outstanding deferred work.

## 28. Core portability: Linux and Windows, macOS later

- Requirement: POSIX-compatible shared design; Linux and native Windows are
  the primary targets, with macOS later. Windows requires no WSL, MSYS or
  Cygwin; native platform bindings preserve the same core contracts.
- The shared liveness probe and socket-address layout are repaired.
  `docs/architecture/CORE-POSIX.md` owns the remaining capability matrix;
  source guards and Linux tests do not prove non-Linux runtime support.
- Status: incomplete. Native Windows still encounters Unix-only APIs;
  non-Linux peer identity currently refuses daemon dispatch connections.
- The managed task wrapper (`spawn --task`, `session watch`) is Linux-only for
  the same reasons: its live view reads a conduct-owned PTY transcript, and both
  its record writes and its delivery cursor ride the stage lock. Its earliest
  honest Windows point is headless parity over a pipes-only transport, after
  `CORE-POSIX.md`'s matrix prerequisites are met — no part of the wrapper claims
  Windows support today.

## 29. HTTPS mesh with end-to-end encrypted letters

- Requirement: HTTPS mail carries encrypted payloads from the first slice.
  Origin-bound letter contents and destinations are immutable to relays;
  forwarding authority permits only separately authenticated transit records.
- Use case (User, 2026-09-25): reach an Aoide machine outside the LAN from
  networks where a VPN cannot run and only HTTPS on 443 passes. Doors stay
  loopback-only, so no home machine accepts inbound HTTPS: every node
  connects OUT to a relay. The relay is therefore not optional, and because
  it is not trusted with content, sealed E2E letters are what make it safe.
- Consequence for phasing: the proposal's H1 (direct HTTPS edges) needs an
  inbound listener on a home machine and does not serve this use case; the
  first useful slice is outbound-only nodes plus a relay (today's H4 shape).
  Re-phasing is a proposal amendment, not yet made.
- [HTTPS-MESH-API.md](HTTPS-MESH-API.md) owns the proposed verification,
  key-binding, revocation, receipt recovery and migration contracts.
- Status: reviewed proposal; transport implementation and encryption-library
  profile remain unfinished. HTTPS has no plaintext fallback. Existing SSH
  delivery remains in service until parity and recovery are demonstrated.
- Open: where the relay runs (an always-on host with a public 443 that is
  not a home machine).

## 30. Mail export (letters → one Mneme note per thread)

- Requirement: `aoide mail export` writes each mail thread as one note, so the
  mailbase is searchable from Mneme, which already has `embed_text`,
  `similar_notes` and `semantic_centroid`.
- Target: the `magi` vault (`~/Magi`, synced by syncthing) is not present on
  yomi, where syncthing is inactive. Until the User enables that share, the
  default export directory lives under `state/`.
- Status: LANDED on main (merge of `eidolon/mail-export`, head `a081939`) —
  `6a7799f` landed it, `bf3caa2` refused colliding note
  names and qualified the read-only claim, `6ba0c61` made one block per send
  (`letters:` counts letters, not mailbox copies), and `a081939` gates the join
  on `seq` adjacency beside the shared signed text, the sender and the mailbox
  tie-break, so one send's copies merge only while they land back to back.
  One Markdown note per thread, `--dir` (default `state/mail-export/`),
  READ-ONLY on the mailbase: no cursor advance, no mark, no removal, no ring —
  beyond the same one-shot migrations every mail command runs (mailbase and
  cursor shape) on a first touch, which may mint the identity key. A thread key
  that is not 64 lowercase hex names its note by `x` plus the first 16 hex of
  its sha256; two keys that would name one note refuse the whole run before any
  write. Tests (`aoide-client`):
  `mail_export_groups_a_thread_and_gives_everything_else_its_own_note`,
  `mail_export_skips_receipts`, `mail_export_leaves_every_reader_cursor_where_it_was`,
  `mail_export_rewrites_nothing_when_the_note_already_matches`,
  `mail_export_fences_a_body_that_carries_its_own_fence`,
  `mail_export_gives_a_non_hex_key_the_x_prefixed_stem`,
  `mail_export_never_names_a_note_dot_md`,
  `mail_export_refuses_two_threads_that_share_a_stem`,
  `mail_export_keeps_two_sends_of_the_same_words_apart`,
  `mail_export_keeps_two_fanouts_of_the_same_words_apart`,
  `mail_export_joins_only_copies_that_are_adjacent_in_seq`, the renamed
  `register_mail_wires_all_ten_commands`, and `aoide-cli`'s golden snapshot.
- Owner: Eidolon executor. Depends on: nothing.

## 31. Mesh SSH keys are unrestricted

- Finding (2026-09-25): every mesh key in yomi's `~/.ssh/authorized_keys`
  (`sakaki-to-yomi-strix` old and new, `thinkchiyo-to-yomi`, and a key
  labelled `osaka-to-sakaki` present on yomi) carries no options, so each
  grants a full shell as the User rather than mail delivery.
- Fix: `restrict,command="<aoide door entry>"` on each mesh key, the entry
  point to be read from the SSH transport path; stale or mislabelled keys
  removed on the User's word. The same audit runs on every host.
- Status: not started. Owner: unassigned. Keys are the User's to change.

## 32. Remote sub-agents (parent link, ping-back, watch across nodes)

- Requirement (User, 2026-09-25): any harness agent run through Aoide is a
  watched process whose output is visible and steerable, with no harness
  integrated into Aoide. Agents spawned this way are detected as the spawning
  session's sub-agents, including across machines through its a2a channel.
- Rulings (User, 2026-09-25): Q1, Q3, Q4, Q5, Q6 at the brief's defaults; Q2
  WIDENED — a signed, `verified` node whose grant includes `read` may read
  `ANY` session's watch frame on the far node, so the remote-parent key match
  is not required to READ. It is still required to steer without pending and
  for ping-back history, and unsigned/bearer/address rungs are refused.
  Both machines show the link: the child's node shows `remoteParent` and
  `↑ <node>/<parent>`, the parent's node shows the remote child and that
  child's own local descendants as a nested subtree.
- Shape, as ruled: `remoteParent {node,key,sessionId}` on the child's record —
  a SECOND field, `parentSessionId` stays local-only (§13's rule, never a
  foreign owner in it); `state/stage/remote-children.json` as the caller-side
  ledger; `metadata["aoide/from"]` as the signed caller claim; and the
  ping-back PULLED over the A2A door (never mail, never a child-side push),
  so only the direction the spawn already proved is used.
- Status: S1 LANDED on `eidolon/remote-sub` (`remoteParent` +
  `records::RemoteParent`, `aoide-storage::remote_children`,
  `valid_claimed_session_id`); S2 LANDED (the signed `aoide/from` claim on
  spawn and inject, `node spawn --parent`, the caller-side ledger write on
  the ack; a live `--parent` beats the attestation, the env is never read);
  S3 LANDED — the door honours the claim on the SIGNATURE rung only
  (`claimed_remote_parent`, pure; a weaker rung ignores it and writes one
  `ignored-unsigned-from` audit line; a signed caller's malformed claim is
  `-32602`) and stamps `remoteParent` from the RESOLVED node's name and
  pubkey through `do_spawn` → `stamp_spawn_provenance` →
  `conduct::graph::stamp_remote_parent` (one registration retry loop, two
  change-once stamps; `parentSessionId` stays `None`; no env var exists for
  it, and `resurrect` carries none); S4 LANDED — both sides show the link.
  The child's node publishes `remoteParent {node, sessionId}` on its
  `graph.json` session node and its `session --json` row, resolved to the
  CURRENT `nodes.json` name for the stored key (the stamped label only as a
  fallback), with the roster line tagged `↑ <node>/<sessionId>`; the parent's
  node publishes `remoteChildren [{node, sessionId}]` off its own
  `state/stage/remote-children.json` ledger plus the `↓ <n> remote` roster
  tag, and the ledger's rows leave with the parent that leaves the roster —
  both exit paths call `drop_remote_child_rows` once their own `sessions.json`
  write has landed. No `spawned` edge is minted either way —
  a local id equal to a remote `sessionId` does not become a local parent in
  the projection, though that is pinned by test at the DOCUMENT level only (no
  test drives the autogate, sibling rule, reaper or mailbox against a remote
  child, and `graph link` can still mint one by hand) — and the remote child's
  own local descendants need no new
  wire: the far document already nests under its `node:<name>` root with its
  own `spawned` edges, so `par1 → nodeb/C → nodeb/G` is rendered on `aoide
  session` as an indented chain under the parent's own row, with `--json`
  carrying the same join (`fold`/`subtree` on the `remoteChildren` entry; the
  HIGH finding of the S4 review was that nothing walked the data). The
  conductor TUI's DAG (`graphview.rs`) still reads neither key — a follow-up,
  named and open. One code fact the brief could not know:
  `with_stage_lock` was documented non-re-entrant, and both real
  `prune_done` callers hold it (`reap_inner`, `do_session_start_inner`), so
  the ledger retain (then inside `prune_done_scoped`) deadlocked until
  `aoide_storage::fs::with_stage_lock` was made re-entrant for the holding
  thread (`STAGE_LOCK_HELD`; other threads and other processes still wait,
  and this now covers every prune pass, automatic included); S5 LANDED — the
  door's Inject arm consumes the claim the spawn path already stamps:
  `remote_parent_match(caller, claim, target)` (Signature rung, the target
  record's stored `remoteParent.key` equal to the caller's verified key, and
  its stored `sessionId` equal to the claim — all three or no match) delivers
  without pending, audited `autogate-remote-parent`, riding BOTH rails the
  `sig_autogate` restoration rides, each carrying one transport — the EXEMPTION
  from `origin_for_inject`'s downgrade is what carries the loopback/ssh `-L`
  shape (`should_deliver_now`'s `Loopback` arm delivers unconditionally, so the
  exemption alone is enough there, and without it a tunneled parent coerces to
  `Unknown`, which delivers nothing), and the FOLD into `autogate_match` is what
  carries `ConnOrigin::Remote`'s shape (that arm consults `autogate_match` and
  nothing else; the exemption is inert where the origin was never loopback).
  It overrides neither earlier question — the door-wide
  bearer is still checked FIRST (a door with a token set admits a remote parent
  only if it presents the bearer) and the node's own `autogate` flag need not be
  on, the same independence `send_gate`'s local parent rule has, which is also
  why the rule has no off switch: the levers are unpairing the node (`node
  remove`) or the child ending, and `node allow <n> spawn off` stops only NEW
  children. The `-32602` stays spawn-side: an inject reads a malformed claim as a non-match, exactly as
  an absent one. On the client side `resolve_remote_parent` is now shared by
  both callers, and `send --to` carries that KERNEL-ATTESTED caller as its
  claim but DROPS an unruly one — it sends unclaimed and names the reason on one
  warning line (`not claiming parent: <reason>`) instead of refusing, the S5
  ruling on the previous executor's Q3 (`node spawn` still refuses the call
  outright); S6 LANDED — `tasks/get` with `params.metadata["aoide/frame"]`
  answers with the session's watch frame as one `data` artifact, gated by
  `output_read_admitted` = `read_ok` ∧ signature-rung ∧ the record `verified`
  with `allows∋read` (`node_may_read`, `node_may_spawn`'s twin), so a signed
  reader holding `read` reads ANY session's frame while the remote-parent key
  match stays required for steering (S5) and for S8's history; the refusal is
  `-32011` — this arm's OWN code, minted like Spawn's `-32006` and
  `mailDeposit`'s `-32010` (an orchestrator ruling: `-32007` stays
  `verify_signed_request`'s, decided before this arm runs, so the code alone
  names the arm) — with ONE message whether the session
  exists or not, and the read audits under its own label
  `a2a.tasks/get.frame`. `Task.artifacts`/`Task.history` are optional and
  omitted when absent (a status read stays byte-identical); `watch_frame` is
  `gather` with `raw = false`, `Frame` is now pub with `Deserialize` and
  `for_wire` (nulling `logPath`/`socket`/`instructionsPath`/`suggested`), the
  door clamps tail to `1..=200`, a letter body to 40 lines and the frame to
  256 KiB (oldest letter first, then the oldest output line, `truncated: true`),
  and the `Cf` strip — every format character, not just the bidi marks, now
  including the zero-width set — moved into the shared `clean_line`/
  `clean_block`, so it applies to the local view too (one spec gap found: the
  brief's §10 test list asks for a `linesAfter` refusal case that belongs to
  S8's history ring, which does not exist yet; S6 keeps the Do's own table).
  Tests: aoide-protocol 174, aoide-conduct 881, aoide-server 240 — all pass,
  `cargo test --workspace --no-run` clean. S7 LANDED — `session watch
  <node>/<query>` reads the far frame: `graph::remote` is the hoisted home of
  `node_cached_sessions`/`resolve_remote_query`/`node_session_label` (moved
  out of `send.rs`, which now shares them plus `node_record`,
  `node_cached_graph` and `unresolved_remote`, so `send --to` and the watch
  refuse an unresolvable query with ONE text and ONE `data` shape; the
  "vanished mid-resolution" wording of the unreachable node arm came back with
  the move, and `node_record` now reads the `nodes` slice the caller already
  loaded instead of re-reading `nodes.json`), and the
  `Resolution::Remote` arm of `view.rs` watches instead of refusing. The read
  is `aoide_client::commands::task_get_on_node(node, id, frame_tail)` — signed
  exactly as `send_message_to_node`, over
  `post_json_to_node_with_tunnel_key`/`post_json_capped`/`run_curl_capped`
  (the first call site with its own response cap, `--max-filesize 524288`, and
  the only one to go through the helper that takes the tunnel key explicitly;
  the cap is ~1.6× the honest frame ceiling, not 2× — the door's 256 KiB bounds
  shed content only, so the exempt instruction block's own worst case
  (`BLOCK_LINES_MAX` 400 × `LINE_MAX` 200 × 4 B ≈ 320 KB) is what a real frame
  can reach), parsing
  the `frame` artifact into the ONE `Frame` type both sides share.
  `Frame::clamp_untrusted(node, tail)` re-cleans every string through the
  shared `clean_line`/`clean_block`, re-clamps every count (output to the
  requested tail, mail to `MAIL_RAIL`, a body/instruction block to
  `BLOCK_LINES_MAX`), STRIKES `logPath`/`socket`/`instructionsPath` (the paths
  and control socket of the box that wrote the frame — the renderer gained the
  arm that still shows the instruction TEXT with its path gone), rebuilds
  `suggested` locally as `aoide send --to <node>/<id> --submit -- …`, and
  leaves raw PTY bytes with no path at all on the remote side; the header
  carries `<node>/`. `--snapshot` reads once, live polls every 2 s until the
  far `presence != running` (or Ctrl-C, which the
  local view's own handler serves), and a FAILED poll ends the watch with the
  structured refusal (`Status::Error`, its own `reason`/`code`,
  `followed: true`) rather than an `ok` that hides the diagnosis — with no
  prose printed on a `--json` stream. `-32011` renders as a taught error naming
  the grant and the box it runs on. Review pass over S6+S7 (same branch): a far
  node's own error text is now cleaned at the seam with `common::clean_line`
  for BOTH doors (the watch's `read_refused`, `send`'s remote-delivery failure)
  and for the petname/session id `remote::node_session_label` renders out of a
  peer's cached document — an OSC-52 escape or bidi override in a peer's
  message reached the operator's terminal uncleaned before this, and the tests
  drive a real curl POST at a fake door to prove the cleaned text end to end;
  `-32011` is spelled ONCE (protocol `OUTPUT_READ_REFUSED_CODE`, read by the
  door and by both readers), and the live remote loop is finally tested (the
  `presence` exit with changed-only emits, and the failed poll keeping its
  reason). Two places the brief disagreed with the
  code, both recorded as code facts: `Frame` has no `raw` field at all
  (S6 landed `gather`'s `raw` parameter, not a field, and the wire frame is
  built with it false) — so "force `raw` = false" lands as "there is no
  terminal gate on the remote path", pinned by test; and the brief's §4.3
  signature carries a `lines_after` the Do's own list drops — S7 sends only
  the frame key, and `aoide/linesAfter` stays S8's. Tests: aoide-conduct 892,
  aoide-client 316, aoide-protocol 174, aoide-server 240 — all pass,
  `cargo test --workspace --no-run` clean. S8 LANDED — the ping-back decision
  and the line are two things and a remote parent gets the DECISION:
  `choose_event` answers `Option<PingEvent>` (closed enum, snake_case on the
  wire, every string already `clean`ed at the SENDER so no receiver sanitizes
  for the far node) and `render_line(tag, &event)` renders the one line from
  it, the tag belonging to whoever renders; the local lines are the same bytes
  they were, pinned by the module's own unchanged fixtures. A child whose
  record carries `remoteParent` spools each event to
  `state/stage/pingback-remote.json` through the new
  `aoide-storage::pingback_remote` (CONTRACTS §4) instead of calling
  `deliver`, and NEVER calls it: per-child `seq` from 1, 16 retained, the
  oldest dropped past the cap, `gap` when the ring rolled past the caller's
  cursor and `last` to resync from — the cap is also the wire bound, one
  number. The events are opaque `Value` at that layer on purpose: the closed
  vocabulary is conduct's, and a queue that parsed its own payload would be a
  second definition of the event. `PingEvent::Exited` is the one row that
  comes from the RECORD (`state` folds to `done`, plus its
  `outcome`/`exitCode`) rather than the trace, fired once per child by a new
  `exited` latch in the cursor (serialised only when true, so every existing
  cursor file is byte-identical — `skip_serializing_if is_false`), remote
  children only (Q5's ruled default) and last among the trace rows so it takes
  the silence row's place; a row of the brief's table that had to move, since
  a ring's last event before the exit must be the child's own last word. The
  gather's filter had to widen with it: `tracks(rec) = agent == "eidolon" ||
  remoteParent.is_some()`, because a remote child has NO local
  `parentSessionId` and the old "nobody to tell" gate dropped it before any
  event could be decided. The door serves the ring as `tasks/get` +
  `params.metadata["aoide/linesAfter"]` → `Task.history` (one `data` message,
  `messageId: "pingback"`, the `RingRead{events, gap, last}` under the part's
  `data`), gated by `output_read_admitted` AND
  `history_admitted` = the child's stamped `remoteParent.key` equal to the key
  the caller's signature verified against — the key, never the stored `node`
  label, never a wildcard, an empty stored key matching nobody. §4.4 names no
  code for that refusal, so it reuses `-32011` (`OUTPUT_READ_REFUSED_CODE`)
  with its OWN text: "not this child's parent" is not the same reason as "no
  `read`", and an operator must be able to read which one refused. A request
  carrying BOTH output keys is judged by the STRICTER of the two — asking for
  the history is asking for the history, and the alternative is a frame handed
  out around a silently missing ring — while the same caller's frame-only
  request is still admitted, which is the ruling Q2 widened. `lines_after` is
  `params.metadata` only and reads a non-number as 0 (the tolerant reading
  `frame_tail` already takes). Review pass over the gap the S8 brief left
  open, and closed here: a remote EIDOLON child whose presence the sync
  DROPPED mid-turn published nothing at all (it has no local parent, and
  `DroppedEidolon` carried no `remoteParent`), so its ring could never close —
  `DroppedEidolon` now carries `remote`/`exit_code`/`outcome` off the record
  it was built from, the dropped child is gathered on the same rule as a live
  one, and because a dropped child is never decided again (its cursor entry
  leaves that same pass) it claims its trace row AND its exit in ONE pass.
  One bound named and left as-is: nothing prunes a ring, so its size is 16
  events per child this node ever ran — the same "never reconciled" shape the
  remote-children ledger holds. Tests: aoide-storage 438 (6 new),
  aoide-conduct 896 (4 new), aoide-protocol 174, aoide-server 245 (5 new), all
  pass, `cargo test --workspace --no-run` clean. S9 LANDED — the parent's own
  node pulls what its child published: `conduct`'s `pingback_pull`, in
  `reap()`'s post-lock block one lane over from `pingback`, walks
  `remote-children.json` and — only under `Door::Daemon`, only for a row that
  is not `drained` and whose `parentSessionId` is a live record `deliver`
  could reach — asks that child's node `tasks/get` +
  `aoide/linesAfter` = the row's cursor and renders what comes back HERE.
  `aoide-client` gained `task_history_on_node` (the signed POST shared with
  `task_get_on_node`, a new `commands::HISTORY_PULL_TIMEOUT_SECS` = 5 rather
  than the interactive reads' 15, because this runs inside a ~12 s tick, and
  the existing `FRAME_MAX_RESPONSE_BYTES` for the cap) plus its pure
  builder/parser pair in `client/node.rs`; the tunnel key is the PARENT's
  session id, so the ssh forward is the one `close_all_for_session` already
  closes and the daemon holds no forward of its own. The events are a peer's
  BYTES, so each is re-validated against the closed event set (an unknown kind
  is dropped, never guessed at), every string re-cleaned (control AND Unicode
  `Cf`, clipped to 80) and rendered by the LOCAL `render_line` under a tag
  built from the row — `[<node>/<child id>]`, since the far node supplies no
  tag at all — and `deliver` still applies every skip it always did, now with
  the gate label `autogate-child remote` (the label became a `deliver`
  parameter; the local call site keeps `autogate-child`, and the four target
  predicates moved into one `unreceptive` the pull and the delivery BOTH call,
  so the candidate filter and the writer cannot disagree). The row's cursor
  advances BEFORE the line, to whatever the answer carried — the newest event,
  or `last` for a `gap` with no event to move past — so this lane is
  at-most-once: a failed write loses a line rather than repeating one, which
  the tests pin directly. A `gap` costs one line built from arithmetic
  (`· <n> events lost before this point`), never a fabricated event. `Exited`
  drained ⇒ `remote_children::mark_drained` latches the row (additive,
  omitted while false) and the pull stops for it forever, because a ring is
  never pruned and a departed child never speaks again. A pull failure is
  quiet — one `clean_line`d audit record, `report.skipped`, no retry storm,
  the next tick from the same cursor; the brief names no backoff, so there is
  none, one pull per child per tick. Two judgments beyond the row, both stated
  here: the tag the brief writes as `<node>/<petname|id>` is
  `<node>/<sessionId>` because the ledger stores no petname, and the far node
  is resolved by the row's `key` alone (never the display `node`, so a rename
  cannot break a pull and a re-pair cannot dial a wrong box). Review of the
  lane's own sender found the hole S7's `clean_line` had closed only on the
  READ path: `pingback`'s `clean` filtered `char::is_control`, which does NOT
  cover Unicode `Cf`, so a bidi override or zero-width mark in a child's say,
  prompt, stop reason or tool label still reached a LOCAL parent's line. Fixed
  on the shared sanitizer — `common::strip_unsafe` is now the one filter
  `clean_line` and the ping-back's 80-clipped `clean` both use, no copied
  `is_format` table — and the existing fixtures are byte-identical because
  none of them carries a `Cf`. Tests: aoide-conduct 907 (11 new: the pull's
  line/tag/audit, an unknown kind dropped, a `/`-leading quoted field
  neutralized, a hostile escape+bidi+zero-width string re-cleaned on BOTH
  lanes, a shell parent skipped before any request, the cursor advancing on a
  failed write, the retry from an unchanged cursor beside a healthy child, the
  gap marker with and without an event in the answer, the drain latch, no pull
  for a done/absent parent or an unresolved node, and the `Door::Daemon`
  gate), aoide-storage 439 (1 new), aoide-client 318 (2 new), all pass,
  `cargo test --workspace --no-run` clean. S10 open, in the brief's order
  (cargo builds serialize).
- Review pass over S1–S3 (same branch, `8236675` onward): the caller now
  holds its own winning claim to `valid_claimed_session_id` and refuses
  locally before signing (the "one predicate, both sides" line was
  document-only until then), and its ledger row rejects an ack id outside
  that same shape; `spawn_session_id` mints `a2a-<pid>-<secs>-<n>` (pid +
  whole second collided for two spawns inside one second, leaving one
  record's `remoteParent` to whichever of the two stamps landed last); the
  `-32602` for a malformed claim is applied on the SPAWN side only, so the
  Inject arm ignores it exactly as an absent one; both ledger shapes and
  `records::RemoteParent` flatten unknown keys into `extra`, so a field this
  version does not know survives a rewrite; and the stamped `key` is the key
  `verify_signed_request` actually verified (threaded down as `SignedCaller`
  with the name), not a second lookup of the resolved name.
- Review pass over S4 (same branch, `42218f2` onward): the HIGH finding — the
  link was true as DATA and rendered nowhere, and the subtree was TTL-gated —
  is closed by `who.rs::attach_subtrees`, one join of every `remoteChildren`
  row to the probed fold by `(node name, sessionId)`, run in `collect_roster`
  before any filter, marking each row `Fresh`/`Stale`/`Absent` and filling its
  `subtree` from that fold's own `spawned` edges; both renderings draw the
  chain indented under the parent's own row and `session_view_json` carries the
  same join as `fold`/`subtree`, so a row with no fold behind it is visible as
  `(not pulled)` while a TTL-expired one is `(stale)` rather than dressed up as
  current. The conductor TUI's `graphview.rs` still reads neither key: a named,
  open follow-up. The MED — a pulled document's own text neither sanitized nor
  bounded in the roster — is closed by moving `clean_line` to
  `graph/common.rs` (one definition, `LINE_MAX` shared with `view.rs`'s mail
  fragments) and running every string `who.rs` prints off a pulled document
  through it, in the renderings and the JSON alike; `remote_link` now drops a
  link naming no `node` as well as one with no `sessionId`. The second MED —
  "a ledger row leaves with its parent" was true for one of three exit paths,
  and the retain committed before the caller did — is closed by
  `doc.rs::drop_remote_child_rows`, called by both roster-exit paths AFTER
  their own `sessions.json` write lands. The LOWs: the `fs.rs` normal-path
  comment, the re-entrancy test now asserting `held` from inside both closures,
  the six stale "not re-entrant" statements (`song`'s `take.rs` module doc and
  its two inline notes, plus the two CLI wiki pages), and `current_node_name`
  now taking the caller's already-loaded `nodes.json` (one read per document,
  not one per link).
- Tests (S4 review fixups, `aoide-conduct`):
  `the_remote_chain_renders_under_its_parent_from_b_graph_and_the_json_agrees`
  (fresh/stale/not-pulled, from B's own `build_graph` document),
  `a_hostile_documents_own_text_is_stripped_and_bounded_on_every_rendered_path`,
  `dropping_a_parents_ledger_rows_leaves_a_surviving_parents_alone`,
  `local_only_commands_ignore_node_ids` (extended: the explicit prune's row
  leaves, the survivor's stays), `reap_drops_superseded_done_siblings_on_an_
  otherwise_quiet_pass` (extended: same for the superseded-tombstone path), and
  `common`'s four `clean_line` cases (ANSI, bidi, newline, 1 MB). The ordering
  itself — retain after the write — is code position, not pinned by an
  independent failing-write test.
- Tests (`aoide-storage`, S1):
  `session_record_remote_parent_round_trips_and_stays_absent_when_unset`,
  `remote_parent_round_trips_unknown_fields_beside_it`,
  `append_is_idempotent_on_the_verified_child_identity`,
  `a_missing_or_corrupt_ledger_reads_as_empty`,
  `retain_drops_exactly_the_rejected_rows`,
  `advance_lines_after_is_forward_only_and_ignores_an_unknown_child`,
  `valid_claimed_session_id_admits_exactly_the_contract_shape`.
- Tests (`aoide-client`, S2): `node_spawn_parent_must_name_a_live_local_
  session`, `node_spawn_refuses_a_parent_that_is_not_a_live_local_session`,
  `node_spawn_explicit_parent_beats_the_attestation_and_never_reads_the_
  ambient_env`, `the_ledger_row_is_keyed_on_the_child_identity_and_needs_a_
  parent`, `node_spawn_writes_the_ledger_row_for_its_parent_on_the_ack`,
  `node_spawn_writes_no_ledger_row_without_a_parent` (`commands.rs`);
  `build_message_send_body_writes_the_caller_claim_under_one_key`,
  `the_signed_body_digest_covers_the_caller_claim` (`wire.rs`).
- Tests (S1–S3 review pass, same branch): `aoide-storage` —
  `remote_parent_round_trips_unknown_fields_beside_and_beneath_it`,
  `remote_parent_extra_round_trips_through_the_struct_alone`,
  `unknown_fields_survive_a_ledger_rewrite`; `aoide-client` —
  `an_unruly_claim_is_refused_locally_rather_than_signed_and_shipped`
  (plus unruly ids folded into the existing
  `the_ledger_row_is_keyed_on_the_child_identity_and_needs_a_parent`);
  `aoide-server` —
  `the_remote_parent_keys_on_the_verifying_key_not_a_second_name_lookup`,
  `a_malformed_claim_on_an_inject_shaped_request_is_ignored_not_refused`,
  `spawn_ids_never_collide_within_a_second_and_stay_legal_session_ids`,
  `message_send_resolves_via_signed_caller_producing_signature_rung_
  attribution`,
  `message_send_signed_caller_never_falls_through_to_the_addr_token_ladder`.
- Tests (`aoide-server`, S3): `claimed_remote_parent_is_honoured_on_the_
  signature_rung_only`, `the_remote_parent_is_built_from_the_resolved_node_
  not_the_header_name`, `a_signed_callers_malformed_from_claim_is_refused_
  with_minus_32602` (which also pins the unsigned ignore + its audit line),
  `the_from_claim_is_read_only_off_message_metadata`,
  `the_clients_from_claim_round_trips_through_the_inbound_parser`,
  `parse_message_send_params_extracts_all_four_fields_together`,
  `stamp_spawn_provenance_lands_the_origin_and_the_remote_parent_on_a_
  registered_record`; the pre-existing
  `a2a_spawn_clears_the_daemons_own_session_id_from_the_child` still pins
  the child's env. `aoide-conduct`'s S3 test:
  `stamp_remote_parent_is_change_only_and_leaves_parent_session_id_none`.
- Tests (`aoide-server`, S5): `remote_parent_match_needs_a_signed_caller_its_
  key_and_its_session` (the predicate's whole table),
  `a_remote_parent_steers_its_child_without_pending_with_autogate_off`,
  `..._with_autogate_on`, and
  `a_genuinely_signed_remote_parent_steers_its_child_without_pending` (a
  non-loopback origin, so the `autogate_match` rail is what carries it), with
  `a_remote_parent_mismatch_leaves_todays_result_byte_for_byte` as the miss.
  The S5 review's coverage gap (`f764f82`'s four rows all drove
  `ConnOrigin::Remote`) is closed on the OTHER transport:
  `a_tunneled_remote_parent_steers_its_child_without_pending` (the ssh `-L`
  shape at `ConnOrigin::Loopback`, where only the `origin_for_inject` exemption
  can deliver it — its pending check runs before the byte join, so deleting
  the exemption fails it instead of hanging) and
  `a_door_token_refuses_a_remote_parents_delivery_uniformly` (a door token
  with no bearer presented gets #50's uniform `submitted` Task, nothing
  delivered, nothing queued, and no `autogate-remote-parent` label — the
  three docs that claim the bearer runs first, pinned).
  Tests (`aoide-conduct`, S5):
  `send_to_carries_the_attested_parent_as_its_from_claim` (a fake `curl`
  capturing the body it is handed on stdin proves the claim rides the wire) and
  `an_unruly_claim_still_sends_without_claiming_a_parent` (the ruling above,
  asserted against those same captured bytes).
- Review pass over S5 and S6 (`f764f82`, `94a1151`; code fixed in `060e6e8`):
  the S6 refusal gets its OWN code, `-32011` (orchestrator ruling — it had
  borrowed `-32007`, `verify_signed_request`'s, which the register itself notes
  is decided before the frame arm runs), so every capability-gated arm now
  mints the code it refuses with. The S5 rationale was INVERTED in three
  places (CONTRACTS §6, `server/AGENTS.md`, here): the two rails are not one
  load-bearing and one belt — the exemption is what carries the loopback/ssh
  `-L` shape and the fold is what carries `ConnOrigin::Remote`'s, and each is
  load-bearing for its own transport. MED-2: CONTRACTS §4 said no security
  decision keys on `remoteParent` read off disk, which the deliver-now
  decision does — §4 now states the truth (same-uid is already
  loopback-trusted; the key comparison against the verified signer is what
  binds the read). MED-3: `client/AGENTS.md` still claimed an unruly claim is
  refused flat; it is refused on the SPAWN path only, `send --to` drops it and
  warns. MED-4: `server/README.md` still called the Inject arm's
  `resolved_node` attribution, never a gate; its Signature-rung
  `claimed_identity` is exactly what authorizes the deliver-now. LOW-5: the
  remote-parent source is added to the auto-deliver enumerations in
  A2A-Door.md, Node-Transport.md and PAIRING.md. LOW-7: the warning line now
  prints the unruly id escaped (`{id:?}`) rather than verbatim. LOW-8: the
  rule's missing off switch is documented in CONTRACTS §6 and
  Conductor-Channel.md — `node remove` or the child ending are the levers, and
  `node allow <n> spawn off` stops only NEW children.
- Owner: Eidolon executor. Depends on: §13's S-D design half, ruled here.
