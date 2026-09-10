# Task register

The canonical shared register for Aoide/AoideOS work. Root
(codex-integration) orchestrates and reviews; the Fable session coordinates
implementation; Opus designs; Sonnet builds and reviews. One owner per slice,
no duplicate executors. This file is a ledger: it records current state and is
updated in place as work moves; history lives in the commit log and
`docs/Aoide-Wiki/ingest/log.md`.

Fields per entry: status · owner · depends on · evidence · next.

## 1. Session QoL (menu actions, multi-root projects, kill resolution)

- Status: implementation and combined build COMPLETE; live acceptance
  PENDING. Not released; the interactive doorbell is not fixed.
- Owner: Fable (integration); root reviews.
- Depends on: the User activating the built system.
- Evidence: commits through 61afea2 on `main` (pushed); `cargo test` conduct
  597 / storage 402 / cli 42 green, workspace check clean; toplevel build
  `/nix/store/8qbyy7xynz3i9hpa7jlm8xh1j4b8ci2z-nixos-system-yomi-strix-26.11.20260907.dc5d91f`
  exit 0, not activated. Kill-scope label (db862b2) matches
  `conduct::graph::actions::kill_target`.
- Next: pre-activation acceptance against the built artifact under an
  isolated `AOIDE_ROOT`/`XDG_RUNTIME_DIR` with throwaway process fixtures
  (kill from a hosted record, sessionAction reply/timeout/error, project
  create/edit); then the User activates; then desktop acceptance on the real
  roster.

## 2. Desktop Codex / ChatGPT window association

- Status: architecture brief DONE (scratchpad p-codex-desktop-brief.md, Opus,
  read-only); P-CX-1 (pure reconcile core + record shape, kind "app",
  native-id keying) LANDED, conduct 603 green, reviewed; P-CX-2 portable
  discovery IN PROGRESS (User rule: one repeatable process across OSes —
  flock liveness + one parsed `ps` table, /proc only as a Linux tie-break);
  P-CX-3 call site + taught refusals queued.
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
- Next: land P-CX-1 → P-CX-2 → P-CX-3; live check with the app open on two
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

- Status: NOT STARTED as a workstream; parts exist (13-crate split, facet/song
  split P0–P2).
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
- Next: phase (a)/(b) design brief after 2's first slice is dispatched.

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
  a new-build `aoided --version` replaced the live socket path). Bug, backlog;
  until fixed every test brief sets the isolated env on EVERY invocation and
  never calls bare `aoided`.
- Deployed runtime on yomi-strix: aoided 0.0.22 (store `vvvwpzkq…`), predates
  everything above. Staged ≠ proved ≠ deployed until activation.
