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
- Next: the User activates build 2 (QoL) or asks for build 3 (QoL + desktop
  Codex); then desktop acceptance on the real roster.

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
- Review flag for the User: with ONE app-server running every held lock is
  attributed to it without a per-lock check (brief §3 rule); the brief's open
  question §6.1 leaned the other way (enrol only locks an app-server holds).
  Decide before P-CX-3 wires the call site or accept the §3 rule.
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
- Next: live check with the app open on two
  threads is the User's gate.

## 3. Interactive doorbell (native Claude channel idle wake)

- Status: adapter-independent slice landed (979eb9d); wake proof PENDING and
  not claimed (a fetch receipt is not a wake).
- Owner: proof kit Fable (executor); the proof run itself is human-run in a
  real terminal (an agent may not launch `claude`); channel bridge and ring
  order (P-M5c-2/3) after proof.
- Depends on: proof.
- Evidence: proof kit built and verified offline (node stdio channel server,
  unix socket, RUN.md); claude 2.1.260 carries the hidden flag
  `--dangerously-load-development-channels <servers...>` ("shows a
  confirmation dialog at startup"), `notifications/claude/channel`, and two
  gates that can skip it (managed `allowedChannelPlugins` allowlist, a remote
  "channel gate"). Prerequisite: a human at a real terminal accepts the
  startup dialog for the throwaway project.
- Next: kit ready → User runs → PROVES/FAILS recorded → M5c-2, M5c-3,
  onboarding; P-M5b after.

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
- GATE: slices 1–2 edit `flake.nix`/`lib/`/`tests/` (BUILD.md Wave-0 rule),
  slices 3–5 edit `modules/nucleus/` (house rule 1, upstream-merge only).
  No dispatch until the User sanctions each explicitly.
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
- Next: the User sanctions slice 1 (or not); then one Sonnet executor per
  slice with independent review; phase (b) brief after (a) lands.

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
- Next: root places it; then an Opus brief; nothing dispatched before.

## Carried backlog (verified status, never implicitly done)

- AoideOS/Lyra portability: unverified; folds into 4(e).
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
