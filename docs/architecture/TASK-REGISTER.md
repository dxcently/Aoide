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
  race, fixed by a pure `retain_memo_in`; conduct 682). Next: S3 `state` under R2, then S4. Staged and proved, not
  deployed (build 4 predates S1).
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
- Slice order and writers: E1a protocol profile (dispatched 2026-09-12,
  no conduct file, parallel to capture S3) → E1b conduct presence
  reconciler + child record + readiness (after S3 frees conduct) → E3
  native send arm → E0 a2a.rs → E4 resume mapping → E5/E6. Brief rev 3
  (Opus, read-only) folds the rulings and the live target.

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
- `mail outbox` hides transport failure: the daemon drain's back-off in
  `state/outbox/<node>/link.json` silences the CLI's own attempt and the
  listing shows `tries 0` with no outcome (proven 2026-09-12, gate 2). The
  hold-off and the last transport error belong on the listing. Unowned.
- Orphan `channel-<id>.sock` after a wrap exits: CONFIRMED live
  (`channel-db-wrap-2.sock`, 2026-09-12); `sweep_orphan_sockets` covers
  only `session-*.sock`. Unowned.
- Routable door urls in node records (yomi's sakaki `http://192.168.1.202
  :8710/`, osaka's yomi `http://192.168.1.175:8710/`): flagged, unexamined
  against the loopback-only rule; needs a reading of what the record's url
  means before any change. Unowned.
