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
  D2 the live check. Nothing dispatched.
- Build 4 CONTAMINATED, DO NOT ACTIVATE (2026-09-12): the toplevel
  /nix/store/l2pahyr05ky1r90vdgjq7wnfzhfmf8wv-nixos-system-yomi-strix-26.11.20260907.dc5d91f
  was built from the dirty shared checkout and carries another session's
  STAGED, uncommitted eidolon dendrite (eidolon-0.1.0 + hm config, enabled
  on yomi-strix). Its aoide package 0mg1rj1m… is correct (P-CX-4 audit
  string, channel string, procps on both units) but the system closure is
  not HEAD. Clean rebuild IN PROGRESS from a pristine worktree at HEAD
  2432e06 (scratchpad `build5/`), no activation. Live system stays
  cbbnif0i…. Live check after activation: petnames stable across ticks, a
  genuine close still removes the card, no "ps not on PATH" audit line.

## 3. Interactive doorbell (native Claude channel idle wake)

- PAUSED BY THE USER (seq 220, 2026-09-12): automatic mail-triggered wake
  work and disposable automatic-wake tests are paused. Mail stays the
  durable assignment/handoff carrier; sessions check it on explicit
  conductor nudges, after an idle composer is confirmed. The channel code
  (M5c-2/3) stays in the tree, nothing deleted. The end-to-end runbook
  (scratchpad `p-doorbell-e2e-brief.md`) and a clean HEAD-worktree build
  of aoide/aoided/lyra exist unused; the runner was never launched (the
  harness permission classifier rejected the dispatch, reported seq 217).
  Not blocked on a test; resumes only on the User's word.
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

- Status: phase (a) DESIGNED (scratchpad `p-lyra-migration-a-brief.md`, Opus,
  850 lines, read-only), nothing dispatched. Decided export surface, all from
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
  house rule 1); then slice 1. The User activates a build when ready.
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

- Status: NOT STARTED.
- Owner: Fable.
- Depends on: stable runtime seams from 4(c)/(d); reuse Lyra preview/stage,
  provided FolderDialog only.
- Next: proposal after 4(c).

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


## Carried backlog (verified status, never implicitly done)

- AoideOS/Lyra portability: unverified; folds into 4(e).
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
