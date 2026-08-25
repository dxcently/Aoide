# Aoide — Log

## Open Threads

### [2026-07-25] open: minor subjects deferred as mentions
Deferred (not yet page-worthy): den (dropped prior art), Style Dictionary, the W3C design-tokens format, Magi. Promote to pages if they accrue independent claims. (Melete and Mneme promoted to entity pages 2026-07-26; the herdr agent-guide pattern captured as the [[Terminal-Commander]] concept 2026-07-26.)

### [2026-07-25] open: livery schema v1
Provisional v0 in use; design-system v1 supersedes and the update playbook migrates. Close when v1 lands (see [[livery]]). (v0 grew two optional tiers 2026-07-27 without a version bump: `palette.hot` and the all-or-nothing `base16` block, both livery-validated — still v0.)

### [2026-07-26] open: post-skeleton build backlog
The walking skeleton (commit f3ceadf, see [[Codebase]]) is complete and verified; the deeper build is queued. Deliberately local for now — the repo has no git remote (user's call), so Melete fleet registration and its code-task flow wait until one exists. The backlog, roughly in order:
- Functional rice loop: real `rice gen` (palette from prompt/wallpaper), ~~`rice preview` writing `song/stage/livery.json` through [[aoide-notes]]~~ **closed**: real since, renamed `rice stage` 2026-08-14 (see [[Self-Ricing]]); `rice adopt` with the gated rebuild, `rice transpose`.
- `aoide onboard` (first-boot flow) and `aoide update` (merge + contract-bump detection).
- Content pipeline verbs made real (`content register→query`, quarantine, liner dogfood).
- `aoide make` — the [[Widget-Maker]] loop.
- Quickshell NotificationServer spike (actions + inline reply); greetd IPC in AoideGreeter. **The spike decides whether Quickshell keeps the notification-daemon role** — that contingency was prose in `concepts/Desktop-Architecture.md` until the 2026-07-28 assertion sweep routed it here.
- Stand up the network-exposed **Aoide connector** ([[Agent-Interface]] tier 3) once the user enables it.
- Close the skeleton gaps in the thread below.

### [2026-07-26] open: walking-skeleton gaps (QML/nix skeleton vs CLI schema)
Found during the implementation ingest at commit f3ceadf (see [[Codebase]]); queued for the Melete backlog:
- Stage files referenced by widgets/dendrites but absent from CONTRACTS §4 v0: `stage/workspaces.json`, `stage/osd.json`, `stage/cover.json`, `stage/obsidian-sub.json` (only `notes`/`sessions`/`hooks` are contract-documented).
- Compositor keybinds reference an `aoide shell {launcher toggle,lock}` command group not in the [[aoide-cli]] schema tree; the obsidian dendrite references `aoide bridge register-window-class`, likewise absent.
- greetd launches Hyprland directly (stub); the Quickshell greeter (greetd IPC) is not yet the session command.
Close each by either implementing the command/contract or removing the dangling reference.

### [2026-07-26] closed (mostly): session-graph follow-ups
Found during the `aoide graph` ingest at commit 0b3a3fd (see [[Session-Graph]]); queued for the Melete backlog:
- ~~Seam mismatch: the Rust `stage_dir()` ignores the `AOIDE_STAGE_DIR` env var that `modules/nucleus/shellbridge.nix` sets on the systemd unit — it derives purely from `$AOIDE_USER`/`$HOME`. Pre-existing; worth fixing.~~ **Closed 2026-07-26** (d03dcf2): `stage_dir()` honours an absolute `$AOIDE_STAGE_DIR`, documented as the "Stage-dir resolution" contract seam in CONTRACTS §4.
- ~~`hyprctl dispatch focuswindow` exits 0 even for an unknown address (Hyprland no-op success), so `graph focus` cannot distinguish "window gone" from success by exit code alone.~~ **Closed 2026-07-26** (d03dcf2): `graph focus` verifies liveness via `hyprctl clients -j` first; a gone window yields structured `window-not-found` exit 1.
- Future work: ~~a Quickshell DAG surface reading `song/stage/graph.json` (viewer parity on the desktop shell)~~ — **closed 2026-07-26** (1fedd58 + 41be90f: the `AoideSessionGraph` overlay + the dock's `DagGraphGadget`, sharing `GraphModel.qml`) — and [[shellbridge]] recording `parentSessionId` at spawn time so `graph link` becomes the manual override rather than the only source (still open; carried into the post-surface thread below).

### [2026-07-26] open: post-surface follow-ups (commits ad3b21a..41be90f)
Found during the ingest of the five commits through 41be90f (see [[Session-Graph]], [[Gadget-Dock]], [[Codebase]]); queued for the Melete backlog:
- **shellbridge socket protocol still stubbed** — needs verbs beyond the focuswindow flow; a prune verb would light up the dock's disabled per-row `[x]`. ~~Spawn-time `parentSessionId` stamping also still pending (carried from the thread above).~~ **Partially closed 2026-07-27** (fa600ae): the `aoide graph session start/phase/end/hook` write door + Claude Code hooks register sessions (with `--parent`) and phase-track them through the stage files; the socket itself remains future work — the CLI door is the interim registrar.
- **`aoide shell …` verbs missing from the schema** — the compositor keybinds (launcher toggle, lock, and — since 8f4034e — dock toggle, which replaced graph toggle as the SUPER+G open-and-pin path to the dock popup) invoke a command group not among the 27 leaves; CLI work. (Extends the `aoide shell` item in the walking-skeleton-gaps thread.)
- **No system-telemetry stage file** — the MeterGadget's direct `/proc/stat` + `/proc/meminfo` reads are the documented interim until one exists.
- **Layer-shell typing v1** for all surfaces, including a proper non-exclusive background layer for the gadget dock.
- ~~**First-iteration host built, not switched** — the yomi-strix toplevel builds on the live box; activation awaits the user's rebuild gate ([[Rebuild-Gate]]).~~ **Closed 2026-07-26**: the user opened the gate and yomi-strix switched onto Aoide the same night — two switches (`aoide-switch` / `aoide-switch2` detached systemd-run units, ~8 s activation each; the second carried the 9404250 nucleus fixes). Rollback = the previous dxflake generation, retained in systemd-boot. See [[Codebase]] and [[Full-Architecture]].
- **Env-var test mutex is module-local** (`shellbridge.rs`) — fine today; a hazard if other modules grow env-touching tests.

### [2026-07-26] open: post-switch follow-ups (yomi-strix live)
Opened after the live switch (see [[Codebase]], [[Full-Architecture]]); queued for the Melete backlog:
- ~~**QML deploy target vs source** — the quickshell facet's home-manager activation deploys the QML tree to `~/Aoide/qml/` at the repo root, so on the live box the deployed copy now sits **untracked in the fork's working tree**. The relationship between source (`modules/facets/quickshell/qml/`) and deploy target needs a decision: commit the deployed tree, point activation elsewhere, or symlink.~~ **Closed 2026-08-01**: activation now rsyncs into the gitignored `~/Aoide/run/qml/` instead of the repo root — see the "still symlinks" follow-up thread below, itself closed the same pass.
- **First-boot desktop verification pending** — the switch is live, but nobody has logged into the greetd → Hyprland → Quickshell session in this state; the surfaces (bar, dock-popup hot edge, launcher) are unproven on real glass.

### [2026-07-27→28] closed (mostly): page ingest + lint pending for 9e2eb61..8f062df
The 2026-07-27 log entry below is **log-only**: 37 commits (the port tail, the rice going live, livery, baton-ratatui, the hero song, the four Pantheon rounds, the session write door) are summarized in the log but NOT yet worked into the pages. Pending: concepts/entities updates (at least [[Codebase]], [[Session-Graph]], [[Gadget-Dock]], [[Quickshell]], [[aoide-cli]], [[livery]], [[shellbridge]], [[Self-Ricing]], [[Notes]]), `ingest/index.md` glosses, and a lint pass that settles the manifest — `design/Pantheon-Grammar.md`, `design/Baton-3D-DAG.md`, `references/pantheon/**` (13 stills), and `references/dxflake-rice-screenshot.png` are on disk but unmanifested (known drift; `design/` may also need a SCHEMA.md shape decision — it is a new top-level directory).
**Closed 2026-07-28** (see the refactor/index/lint entries below): the `design/` shape decision is made — `design/` is now a manifested kind folder (SCHEMA shape + Notes manifest + index `## Design` section), the four design pages carry frontmatter, and the pantheon `.png` stills stay assets (not notes), so they are intentionally unmanifested. livery/sonata/identity/count facts swept across the pages; [[Song-Anatomy]] minted. Residual (still owed): a *deeper* re-ingest of the 37-commit tail into [[Codebase]]/[[Quickshell]]/[[Gadget-Dock]] beyond the surgical fact-fixes done here, and the design/songbook content migration into `song/` (its own `[refactor]` flag in [[AOIDE-DEV]] §7).

### [2026-07-28] open: unbuilt-and-untracked design items (from the assertion sweep)
- **`rice preview --gallery`** — sample apps restarted under a preview env, so GTK/Qt adopt-only surfaces can be eyeballed before adopt. Specified in [[Stylix]]; in no ledger until now.
- **Baton 3D DAG selection keybind** — `j/k` become camera pitch, so node selection moves to `n/p`. Recorded in [[Baton-3D-DAG]] as the design; the keys are unconfirmed against the rest of the baton bindings.
- **A dark `moonlight-sonata` counterpart to `sonata`** (khoa) — the light key's dusk sibling. Not built; cut from the Pantheon grammar (`song/songbook/default/design/pantheon.md`) prose by the sweep and parked here.

### [2026-07-27] open: user-hand gates (the day's landings wait on khoa)
- **Merge + gated switch** — everything from c782689..8f062df runs live only via the worktree QML drop-in + hot-reloaded stage files; the flake side (screenshot/vision dendrites, hyprglass, hero song, base16 terminals, session verbs on PATH) needs merge to main + [[Rebuild-Gate]] switch. Post-switch cleanup: the temporary systemd drop-in and the `result-aoide` fallback path inside `.claude/settings.json`.
- **`.claude/settings.json`** (session-registration hooks) — written and live from disk, but committing it (and copying it to the main checkout) is classifier-reserved for the user's hand.
- **Wiki is not a git repo** — all wiki changes are unversioned saves; `git init` is the user's call.
- **Mixed commit 3d03d95** — optional manual split (`git reset --mixed 8567ff1`), user's hand only.
- **Baton 3D DAG** — planned in `design/Baton-3D-DAG.md`; implementation awaits the user's green light.

### [2026-07-30] closed: External-Edit-Tracking report-back mechanism undecided
~~Whether the orchestrator learns about a detected external edit via PTY injection (risks colliding with a human mid-keystroke), a visual badge on the gadget rows (depends on a human noticing it), or some other mechanism is not decided.~~ **Closed 2026-07-30** — moot: khoa moved the whole feature off the wiki (it was never built) into `references/AOIDE-DEV-HANDOFF.md` §7 "Open flags" as a ledger entry. The report-back question, along with the rest of the design, now lives there rather than as a wiki open thread.

### [2026-07-30] open: `aoide.surfaces.sessionGraph` registry entry has no QML body
The Greek widget rebuild (commit 353390a) retired the standalone DAG overlay (`AoideSessionGraph.qml` + `GraphRow.qml`) and the shared `GraphModel.qml` from the QML tree, but `modules/facets/quickshell/default.nix` still declares `sessionGraph.owner = "quickshell"` in the surface-ownership registry. Whether that entry should be dropped (no surface to own) or a body re-added (a future full-screen DAG view) is a code-side decision, not made in this pass — see [[Session-Graph]], [[Quickshell]].

### [2026-07-31] open: per-slot widget presence has no host-side toggle
A `WidgetSlot` anchor is unconditional wherever a host surface embeds one — presence is decided by which anchors a surface's QML contains, not by any nix option ([[Widget-Maker#The staging engine — a song overrides desktop chrome]]). Whether a per-slot `enable` option should let a host turn an individual slot off, independent of which song variant fills it, is undecided.

### [2026-07-31] open: `palette-usage.md` (per-song palette-role table) not started
A per-song `design/palette-usage.md` is proposed — a table mapping each slot/widget to which palette role it draws on (primary bg/fg/accent/hot, a semantic role, or deliberate structural ink), agent-maintained, so a song author knows which widgets to retune when a primary palette changes. Not built; no file exists yet in any song's `design/`.

### [2026-08-01] closed: `stage/livery.json` is not seeded from the selected song on boot
~~`stage/livery.json` is written by `rice preview`, `cover set`, and the livery emitters — never automatically from the `aoide.song`-selected song's baked values at activation time, so a host that boots without ever having previewed leaves the stage file stale or absent. Whether/how to seed it at activation is undecided.~~ **Closed 2026-08-01** (`99a8447`): `home.activation.aoideSeedStage` (`modules/facets/quickshell/default.nix`) reasserts the active song's committed notes into `stage/livery.json` on every activation. See [[Codebase]], [[livery]], [[Self-Ricing]].

### [2026-08-01] closed: a `commands/` registry + a `graph.rs` domain split are sketched for `pkgs/aoide`, not started
~~`schema.rs`'s command table and `dispatch.rs`'s match are hand-synced (every new command still touches both, even though the CLI/MCP/dispatch surfaces already derive from `schema.rs`); `graph.rs` is over 7,000 lines, more than half the crate. A target shape is sketched: a `commands/` tier where each verb-group module owns its own schema entry, handler, gate, and availability and self-registers into a `Registry` that `dispatch`/`schema --json`/the MCP tool list all derive from, replacing the hand-written table (modeled on Hermes-agent's — github:NousResearch/hermes-agent — self-registering tool registry, and on Claude Code's discrete-tools-behind-a-thin-dispatch shape); explicit aggregation is the mechanism under consideration over `inventory`/`linkme`, since the crate is pure-Nix offline-locked. `graph.rs` would split into a `graph/` domain (`model`/`doc`/`session_store`/`window`/`conduct`/`send` plus `testutil`), mirroring the existing `conductor.rs` + `conductor/` split, behind a stable re-exported public API. The domain split is understood as doable independently and first; the registry to follow once other CLI work settles. Neither has started.~~ **Closed 2026-08-01** (`16b65d6`, `62121a4`): both landed as sketched — `graph.rs` is now a thin re-export root over `graph/{model,doc,common,verbs,window,session_store,conduct,send}.rs`; `registry.rs` + `commands/{meta,rice,cover,stubs,graph,infra}.rs` replace `schema.rs`'s hand-written table, with `dispatch()` reduced to a registry lookup and a golden-snapshot test pinning the command-path set. See [[aoide-cli]], [[Codebase]].

### [2026-08-01] closed: the QML deploy target still symlinks into the repo root; a home-manager-native fix is diagnosed
~~[[Quickshell]] already carries the deploy-target question (`~/Aoide/qml`, untracked at the repo root). The mechanism causing a `switch` to fail there is now understood: `home.file."Aoide/qml"` with `recursive = true` manages that tree as home-manager symlinks, and copying live QML edits over those managed symlinks (the ordinary dev loop) makes `checkLinkTargets` abort on the next `switch` (no `backupFileExtension`/`force` set). A replacement — deploying via `rsync -a --delete` into a gitignored `~/Aoide/run/qml/` instead of managed symlinks at the repo root, with the service repointed there — is sketched but not switched to.~~ **Closed 2026-08-01** (`252c3d4`): switched to. `home.activation.aoideDeployQml` rsyncs into `~/Aoide/run/qml/`; `aoide-quickshell.service` reads `run/qml/shell.qml`; the repo root carries no `qml/` directory. See [[Quickshell]], [[Full-Architecture]].

## [2026-07-25] mint | Aoide-Wiki
- Standalone wiki minted from the librarian `_template` for the Aoide project.

## [2026-07-25] ingest | AOIDE-HANDOFF.md
- 10 concept pages, 6 entity pages created.
- Source settled at references/AOIDE-HANDOFF.md.
- Deferred subjects recorded under Open Threads.

## [2026-07-25] refactor | de-registry → standalone wiki + librarian
- Flattened `projects/Aoide/*` to the wiki root; dropped the multi-project registry framing and the registry-level `log.md`.
- Staged the reusable schema set under `librarian/` (SHAPE.md, librarian.md, OPERATIONS/, _template/) — repo-bound, to move into the Aoide repo.
- Renamed concept `Project-Wikis` → `Wiki-Librarian`; rewrote SCHEMA.md as this wiki's own constitution ("read the whole wiki").
- De-registried the rule pages: Indexing (standalone shape + mint/convert), Lint (manifest parity), Naming (per-wiki dirs), Self-Update (log to ingest/log.md).

## [2026-07-25] lint | full wiki
- No findings. Manifest parity clean (33 notes); index parity clean (16/16 content pages); no broken links (remaining unresolved `[[…]]` are pedagogical examples in OPERATIONS code blocks); no backticked wikilinks in SCHEMA/Overview; no stray registry/`projects/` framing outside this history.

## [2026-07-25] refactor | librarian → wiki protocol
- Recast the librarian as a **shipped protocol** owned/managed by Mneme/Melete; Aoide's own wiki is the self-managed in-repo exception.
- Renamed `librarian/` → `protocol/`, `librarian.md` → `PROTOCOL.md`, concept `Wiki-Librarian` → `Wiki-Protocol`; updated all links.
- Stated the default location for new wikis: the project's own repo (`<repo>/wiki/`), in SHAPE.md, PROTOCOL.md, and the concept.
- Replaced dxflake/invented examples in the rule pages (Naming, Wikilinks) with self-referential ones (Design-Tokens, aoided). dxflake remains only as a real entity page.

## [2026-07-25] lint | full wiki (post-refactor)
- No findings. Manifest parity clean (33 notes); all wikilinks resolve except pedagogical examples in OPERATIONS code blocks.

## [2026-07-26] ingest | Full-Architecture
- Minted `concepts/Full-Architecture.md`: a whole-body synthesis with ASCII visualizations — two-halves/token-seam picture, a master subsystem map, a per-subsystem I/O table, and detail diagrams for the token fan-outs, rice loop, content pipeline, and control plane. Synthesized from existing pages; no new claims beyond the handoff.
- Wired into `Overview.md` (concepts list, first entry) and `ingest/index.md`.
- Manifest: added the note; snapshot bumped to 2026-07-26 (now 34 notes).

## [2026-07-26] lint | full wiki
- No findings. Manifest parity clean (34 notes); all wikilinks in Full-Architecture resolve (self-references to existing concept/entity pages + [[Overview]]); remaining unresolved `[[…]]` are still only the pedagogical examples in OPERATIONS code blocks.

## [2026-07-26] ingest | Terminal-Commander
- Minted `concepts/Terminal-Commander.md` on the user's steer: a herdr-class terminal-commander widget that watches terminals running agents (Melete-spawned or other), shows a live session roster from `song/stage/*.json`, and jumps to any terminal by click (`hyprctl dispatch focuswindow`) or a Hyprland keybind. Grounded in the existing session-jump machinery (shellbridge session+window registration, Claude Code hooks).
- Added to Feature-Set as exemplar group 6 + a matrix row; wired into Overview, ingest/index.md, SCHEMA manifest (now 39 notes; Tags + session, terminal). herdr promoted out of Open Threads.

## [2026-07-26] ingest | Feature-Set + Widget-Maker + Melete + Mneme
- Minted `concepts/Feature-Set.md` (batteries-included catalog: bundled Melete + Mneme; exemplar features — notification→messaging bridge, Tailscale/Cloudflare fleet, scheduled-jobs/timers widget; the one-spine hook diagram + feature matrix). Marked provenance: desktop/agent/pipeline/governance items from the handoff; messaging bridge, fleet exposure, and the timers widget specified as new shipped features.
- Minted `concepts/Widget-Maker.md` on the user's steer that Aoide is fundamentally a specialized, extensible, declarative widget maker — because Melete is a coding agent, integrations are generated (nix + QML + adapter) via the same gated loop as self-ricing, not selected from a plugin menu. Reframed Feature-Set's features as exemplars pointing here; added the thesis to Overview.
- Minted `entities/Melete.md` and `entities/Mneme.md` grounded in the Magi GNOSIS `Melete-Mneme` sub-wiki (read via `Mneme:Sakaki`): the doer/door split, the Boeotian-muses triad (Melete=practice, Mneme=memory, Aoide=song), full feature sets, and each one's part in the Aoide system. Resolves the forward links from Feature-Set/Widget-Maker.
- Wired into Overview, ingest/index.md, and the SCHEMA manifest; snapshot 2026-07-26; manifest now 38 notes; Tags extended (coding-agent, declarative, extensibility, features, harness, integration, knowledge, mcp, melete, mneme, vault, widget).

## [2026-07-26] refactor | repo scaffold shape → docs reconciled
- Built the `~/Aoide` flake scaffold (folders only). Reshaped the surface to the dxflake paradigm: the four snowflake layers grouped under `modules/` (nucleus/dendrites/facets/rime); root reduced to `modules/ hosts/ pkgs/ lib/ docs/ song/` + `.gitignore`.
- Renamed the performed-half folder `Song/` → `song/` (lowercase); onboard symlink is now `~/song` → `~/Aoide/song`.
- Dropped standalone `checks/` (now a flake output backed by `lib/`) and stopped pre-creating runtime dirs (`song/{stage,backstage,auditions}`, root `log/`,`index/`,`catalog/`) — they are gitignored and created at runtime.
- Reconciled docs to match: Snowflake-Anatomy (layer table + layout diagram + prose), Full-Architecture (tree map + checks prose), Song-Vocabulary (facets/rime paths + `~/Aoide/song` symlink), Self-Ricing (`modules/rime/default/`), Design-Tokens (checks prose), Hyprland (`modules/facets/`), Content-Pipeline + Fork-and-Run (`song/` paths). No manifest change (no files added/removed).

## [2026-07-26] ingest | Rebuild-Gate
- Minted `concepts/Rebuild-Gate.md` on the user's steer: how agents rebuild the system, splitting the gate into authentication vs approval. Default mode (capability disabled) is unchanged — the agent builds/`test`-rebuilds freely, then prompts the human to `switch` under their own `sudo`.
- Documented the opt-in `aoide.rebuild` capability: a dedicated no-login agent user + fixed systemd `test`/`switch` units + a per-unit polkit grant (passwordless, narrowly scoped), with the reasoning that this beats giving an autonomous agent a sudo password (a password authenticates a human, not a process; scope + rollback + audit are the real containment). Reconciled with house policy — the capability changes authentication only, not the `switch` approval gate, so *no background rebuilds* holds.
- Expanded the [[Governance]] "Rebuild gate" section with a one-line summary + link and stamped its `updated:`. Wired into Overview, ingest/index.md, and the SCHEMA manifest (now 40 notes; Tags + rebuild, security).

## [2026-07-26] rename | Design-Tokens → Notes
- User steer: "instead of calling them tokens let's fit it to the song theming — they can be the notes." Canonicalized throughout the wiki.
- Renamed `concepts/Design-Tokens.md` → `concepts/Notes.md`; retitled "Notes — the Seam Between Score and Performance".
- Swept 16 `[[Design-Tokens]]` wikilinks to `[[Notes]]` (or `[[Notes|notes]]` where previously `[[Design-Tokens|tokens]]`): Full-Architecture (×4), Overview, Snowflake-Anatomy, Song-Vocabulary, Self-Ricing, Feature-Set, Widget-Maker, Terminal-Commander, Quickshell, Stylix, index (×1), log Open Thread.
- Swept Aoide-vocabulary prose: "design tokens"/"tokens" → "notes" across Full-Architecture, Song-Vocabulary (table + prose), Self-Ricing (loop diagram + per-song structure), Stylix, Quickshell, Desktop-Architecture, Hyprland, Agent-Interface, Feature-Set, Widget-Maker, Terminal-Commander, Governance, Fork-and-Run.
- `stage/tokens.json` → `stage/livery.json`; per-song `tokens.json` → `livery.json`; `aoide.tokens` option → `aoide.livery`; `token package` → `note package`; `token schema v0` → `livery schema v0`.
- KEPT W3C design-tokens format and Style Dictionary by their external names; Notes.md states the mapping: notes are Aoide's design-token layer, the container stays W3C design-tokens.
- Frontmatter tag `tokens` → `notes` in Full-Architecture and Notes.md; SCHEMA Tags updated accordingly.
- SCHEMA manifest: `concepts/Design-Tokens.md` → `concepts/Notes.md`; file count unchanged (40).
- Open Thread "token schema v1" → "livery schema v1"; wikilink updated to `[[Notes]]`.

## [2026-07-26] ingest | Aoide-Connector (Agent-Interface)
- User steer: "aoide should also be connected separately from the mneme mcp and melete mcp connectors — this one specifically for managing aoide and its components."
- Added "Tier 3: the Aoide connector" subsection to `concepts/Agent-Interface.md`: a dedicated user-enabled MCP connector scoped to the rice loop, widget-maker, content pipeline, and daemon status; same one-schema-two-doors rule as stdio MCP; never agent-enabled.
- Added one-line three-connector clarification to `entities/Melete.md` (Melete connector = doer harness) and `entities/Mneme.md` (Mneme connector = vault door).
- Added a control-plane note to `concepts/Full-Architecture.md`: network MCP is the Aoide connector, separated from Mneme and Melete connectors.

## [2026-07-26] lint | full wiki
### Sentinel violations (0)
### Broken wikilinks (0)
- Zero `[[Design-Tokens]]` live wikilinks remaining (one backtick-quoted prose mention in the rename log entry is not a wikilink).
- All `[[Notes]]` links resolve to `concepts/Notes.md` on disk.
### Orphan pages (0)
### Frontmatter conformance (0)
### Index drift (0)
- `ingest/index.md` entry updated: `[[Notes]]` present; `[[Design-Tokens]]` absent.
### Log format violations (0)
### Contradictions (0)
### Manifest parity (0)
- Disk and SCHEMA.md manifest are identical (40 files). `concepts/Notes.md` present on both sides; `concepts/Design-Tokens.md` absent from both.

## [2026-07-26] ingest | Aoide implementation (repo at commit f3ceadf)

- Ingested the **built** Aoide codebase at `~/Aoide` — the walking-skeleton milestone (git history scaffold → wave0 foundation → wave1 subsystems → wave2 integration → notes rename; HEAD f3ceadf, everything verifies green). Source is the repo itself, not the handoff; frontmatter carries no `source:` field, and each new page states a prose "grounded in the repo at commit f3ceadf" line per the Frontmatter rules.
- Minted 3 pages: `concepts/Codebase.md` (how the built repo composes — flake inputs/outputs, `lib/walk.nix` walker + `/_` shelving, `lib/mkHost.nix` walked-tree + host + HM + stylix + the `pkgs.{aoide,aoide-notes}` overlay, `lib/checks.nix` surface-ownership + no-song-read, the `modules/nucleus/options.nix` option contract, the systemd user-unit map, socket + stage-file contracts, repo-file roles, build/verify recipe, real-vs-stubbed status); `entities/aoide-cli.md` (the `aoide` Rust binary — the 19-command tree with real/stub/gated breakdown, `--json` + exit-code conventions 0/1/2/64, `schema --json` as single source of truth + `stageNotesVersion`, MCP one-schema-two-doors, `AOIDE_NOTES_BIN`→PATH location, the `aoided` second binary); `entities/aoide-notes.md` (the Node note engine wrapping Style Dictionary — lint/resolve/emit, the three emitters with atomic stage write, livery schema v0, `packages.aoide-notes` + overlay attr).
- Updated 6 existing pages with implementation facts (short additions, not rewrites): `entities/aoided.md` (unit name, `AOIDE_AUDIT_LOG`/`AOIDE_USER` seams, JSON-lines audit, propose-only gate), `entities/shellbridge.md` (`aoide shellbridge --run`, socket path, sessions/hooks stage files + v0 shapes), `entities/Quickshell.md` (LiveryState/ShellBridge singletons, widget files, install to ~/Aoide/qml via HM, hyprland.conf owned by HM — compositor mkBefore / quickshell mkAfter), `entities/Stylix.md` (stand-down on both module layers filtered by option existence; provisional base16 synthesis), `entities/Melete.md` (the `aoide-melete-adapter` unit + `AOIDE_ADAPTER_SUBSCRIBE` allow-list + metadata-only notification boundary), `concepts/Snowflake-Anatomy.md` (walker + overlay now implemented in `lib/`).
- Crosslink fan-out: bidirectional `## Related` links between the 3 new pages and Agent-Interface, Notes, Snowflake-Anatomy, and the updated entity pages.
- Wired into `Overview.md` (Concepts + Entities), `ingest/index.md`. SCHEMA manifest: added `concepts/Codebase.md`, `entities/aoide-cli.md`, `entities/aoide-notes.md` (40 → 43); Tags extended (+ rust, node).
- Nothing settled to `references/` — the source is the live repo, left unmodified per the standing rule (document, don't modify the code repo).

## [2026-07-26] lint | full wiki (post implementation ingest)

### Manifest parity (0)
- Disk and SCHEMA.md manifest identical (43 files). Added this pass: `concepts/Codebase.md`, `entities/aoide-cli.md`, `entities/aoide-notes.md`. Tags in use match the Tags line (rust, node added).
### Broken wikilinks (0)
- Every `[[…]]` in the 3 new pages and the 8 updated pages resolves to an existing page (or `[[references/AOIDE-HANDOFF]]`). No red links introduced.
### Orphan pages (0)
- Codebase (12 inbound), aoide-cli (7 inbound), aoide-notes (6 inbound); all three also carry an `ingest/index.md` bullet.
### Frontmatter conformance (0)
- The 3 new pages carry `type` + `created` + `tags` (+ `aliases` on the two entities); no `source:` field (source is the repo, not a `references/` doc — grounded via a prose "commit f3ceadf" line per the Frontmatter rules).
### Index drift (0)
- 26 content pages, 26 index bullets, one-to-one, no duplicates.
### Log format violations (0)
### Contradictions (0)

## [2026-07-26] ingest | Song-Replay (design)

User steer: "songs should be able to be replayed on any different host — with host specifics and specific dendrite modules."

The design: a song is host-agnostic score; any host in the fleet can perform it. The new `aoide.song` option (str, default `"default"`) selects the performance per host — one line in `hosts/<host>/default.nix`. Songs self-register like dendrites: `lib/mkHost.nix` walks `song/repertoire/` in addition to `modules/`; each song's `rice.nix` guards itself with `lib.mkIf (config.aoide.song == "<name>")`. Committing a song makes it fleet-available. The shipped standard is song `"default"` (`modules/rime/default/`); `moonlight` exists as the replay fixture. Separation of concerns: the song carries notes only (palette + component tiers); the host carries its specifics and which instruments (facets/dendrites) are enabled. Coverage degrades gracefully when a host lacks an instrument. The `noSongRead` boundary covers runtime dirs only; committed `song/repertoire/**` is versioned score. Transpose vs replay distinction recorded: transpose = new key, same venue; replay = same score, new venue.

Pages updated (no new pages added):
- `concepts/Song-Vocabulary.md` — added "venue" and "replay" rows to the song map; new section "Replay — any song, any host" covering the design, separation of concerns, `aoide.song` option, self-registration, `noSongRead` boundary, and transpose vs replay. Related fan-out extended to include [[Snowflake-Anatomy]] and [[Fork-and-Run]].
- `concepts/Self-Ricing.md` — new section "Adopt, Select, Replay" inserted before "Keys and Transposition": `aoide.song` selector, song self-registration, replay definition, transpose vs replay.
- `concepts/Snowflake-Anatomy.md` — "Dendritic Walker" section extended: songs self-register on the same principle as dendrites; `aoide.song` selector; links to [[Song-Vocabulary#Replay — any song, any host]] and [[Self-Ricing#Adopt, Select, Replay]].
- `concepts/Codebase.md` — "walker and host assembly" updated: `lib/mkHost.nix` walks `song/repertoire/` in addition to `modules/`; song guard pattern; `aoide.song` added to the option contract; `no-song-read` clarified (runtime dirs only, not `song/repertoire/**`).
- `concepts/Fork-and-Run.md` — "Why a Fork" bullet added: committed songs travel with the fork; `aoide.song` selector; link to [[Song-Vocabulary#Replay — any song, any host]].
- `concepts/Full-Architecture.md` — rice-loop diagram note added: adopted songs are fleet-available; `aoide.song` annotation; tree-map `hosts/` line updated to include the song selector; link to [[Song-Vocabulary#Replay — any song, any host]].
- `ingest/index.md` — glosses updated for [[Song-Vocabulary]] and [[Self-Ricing]].
- No new files; manifest unchanged (43 notes).

## [2026-07-26] lint | full wiki (post Song-Replay ingest)

### Sentinel violations (0)
### Broken wikilinks (0)
- All new `[[Song-Vocabulary#Replay — any song, any host]]` and `[[Self-Ricing#Adopt, Select, Replay]]` heading links match exact section headings as written. All other `[[…]]` in updated pages resolve to existing pages.
### Orphan pages (0)
- No pages added; all existing pages retain prior inbound links plus the new cross-links in this pass.
### Frontmatter conformance (0)
- No frontmatter changed; all updated pages retain valid `type`, `created`, `tags`.
### Index drift (0)
- 26 content pages, 26 index bullets; only the glosses for [[Song-Vocabulary]] and [[Self-Ricing]] updated, no pages added or removed.
### Log format violations (0)
### Contradictions (0)
### Manifest parity (0)
- No files added or removed. SCHEMA.md manifest remains 43 files, unchanged. Disk matches manifest.

## [2026-07-26] ingest | Full-Architecture (post-skeleton refresh)

Brought the whole-body synthesis page up to the built repo at HEAD c072461 (walking skeleton verified: flake check green, both packages build, yomi-strix toplevel evals). Structure kept (two-halves diagram, master map, I/O table, seam/rice/pipeline/control-plane details, tree map, Related); contents refreshed:
- New "Status — no longer design-only" section: implemented vs stubbed (exit 64) vs future rungs; local-only note (no git remote, Melete fleet registration deferred); pointer to [[Codebase]] for file-level detail.
- Master map annotated with real/stub status; live fan-out corrected to `aoide-notes emit {stage,hyprctl,osc}` (it was drawn through shellbridge); shellbridge redrawn as the sessions/hooks + socket + Hyprland-IPC box it actually is.
- I/O table: Transport column replaced with a per-subsystem Status column.
- Note-seam section: v0 schema shape (palette + bar/notif/window, null→palette applied in facets), W3C design-tokens container, [[aoide-notes]] bins, atomic stage write; added the three facets as built (8 quickshell-owned surfaces, HM-owned hyprland.conf baked+hyprctl-patched, Stylix base16 synthesis + two-layer stand-down).
- Rice loop: mutating verbs marked exit-64 stubs (`rice lint` real); new song-replay paragraph (`aoide.song` selector, repertoire walked like modules/, self-gating songs, standard = song "default", moonlight fixture, `song-shape` check, CONTRACTS §5) linking [[Song-Vocabulary#Replay — any song, any host]] rather than duplicating it.
- Control plane: now-shipped facts — one schema two doors (`schema --json` → generated MCP façade), the 19-command real/stub split, exit codes 0/1/2/64, the systemd user-unit set (aoided, shellbridge + socket path, aoide-melete-adapter, gated aoide-mcp, aoide-obsidian-register), stage files, the mkHost pkgs overlay; three-connectors bullet sharpened (Mneme = vault door, Melete = doer harness, Aoide connector = dedicated MCP connector for managing Aoide and its components — never via the other two).
- Tree map redrawn to the actual repo (flake.nix, lib/{walk,mkHost,checks}.nix, nucleus files, obsidian dendrite, rime/default as song "default", pkgs/{aoide,notes}, song/repertoire/moonlight, hosts, CONTRACTS/AGENTS/BUILD, root `log`); checks prose now names all three assertions.
- Frontmatter: `updated: 2026-07-26` stamped; [[Codebase]] added to Related. Zero stale Design-Tokens links (page already swept in the rename). No files added or removed; manifest untouched (43).

## [2026-07-26] lint | full wiki (post Full-Architecture refresh)

### Contradictions (2, fixed)
- `concepts/Codebase.md` said "the four versioned contracts" while `CONTRACTS.md` at c072461 (and the refreshed Full-Architecture) carries five (song shape v0 = §5) — fixed in place: "five", §5 added to the list.
- Same page's flake bullet said "the two coupling assertions" and its `lib/checks.nix` list omitted `song-shape` — fixed: "three", `song-shape` bullet added; `updated:` stamped. (Stale remnants of the pre-replay skeleton ingest, surfaced by this refresh.)

All other checks clean:
### Sentinel violations (0)
### Broken wikilinks (0)
- All 21 distinct `[[…]]` targets in the refreshed Full-Architecture resolve on disk; heading links `[[Song-Vocabulary#Replay — any song, any host]]` and `[[Self-Ricing#Adopt, Select, Replay]]` match their exact headings. Full-wiki sweep: the only unresolved forms are backtick-quoted history mentions inside prior log entries (`[[…]]`, `[[Design-Tokens]]`) — not live wikilinks — plus the standing pedagogical examples in OPERATIONS code blocks.
### Orphan pages (0)
### Frontmatter conformance (0)
- Full-Architecture and Codebase both carry valid `type`/`created`/`tags` + the newly stamped optional `updated:`.
### Index drift (0)
- 26 content pages, 26 index bullets; the Full-Architecture gloss still describes the page accurately.
### Log format violations (0)
### Manifest parity (0)
- Disk matches the SCHEMA.md manifest exactly (43 files, both directions); no files added or removed this pass; Tags line unchanged and still matches tags in use.

## [2026-07-26] ingest | aoide graph (repo at commit 0b3a3fd)

- Ingested the `aoide graph` command group landed at `~/Aoide` commit 0b3a3fd (verified: 11/11 cargo tests, `nix flake check` green) — a DAG viewer + management layer for terminals and projects that grows the [[Terminal-Commander]] flat roster into a graph. 19 → 27 CLI commands, all eight implemented (no stubs), all via the single `schema.rs` table so both doors gained them together. Source is the repo itself; new page grounded via a prose "commit 0b3a3fd" line, no `source:` field.
- Minted 1 page: `concepts/Session-Graph.md` — the DAG model (project/session nodes; `anchors` longest-cwd-prefix edges, path-component-aware; `spawned` edges via additive optional `parentSessionId`; spawned-over-anchors precedence with dangling-parent fallback; synthetic `(unanchored)` root; hook-phase-over-roster live-state merge), the viewer (`graph view` Unicode tree + `--focus`/`--json`; `graph emit` → atomic `song/stage/graph.json` for Quickshell hot-reload), the management layer (`project add|remove|list` idempotent registry; `link` with self-link/cycle rejection; `focus` via `hyprctl dispatch focuswindow` with structured exit-1 errors; `prune` on raw `done` state with child un-orphaning), the unknown-field round-trip rule, and the CONTRACTS §4 v0 shapes (`projects.json`, `graph.json`, `parentSessionId` additive).
- Updated 5 pages (short additions, not rewrites): `concepts/Terminal-Commander.md` (roster now has a graph layer, link), `entities/aoide-cli.md` (19 → 27 leaves; the eight graph rows in the command table; guide tier-1 blurb + docs/BUILD.md table carry the group), `entities/shellbridge.md` (projects.json/graph.json stage files, `parentSessionId`, the `AOIDE_STAGE_DIR` seam mismatch), `concepts/Codebase.md` (graph.rs module + 6 unit tests → 11 total; five stage files in the runtime contracts; graph group in real-vs-stub status; no new crates, Cargo.lock untouched), `concepts/Full-Architecture.md` (surgical: grounding bump to 0b3a3fd, 27 commands in the master map + control plane, 15 real verbs in the I/O table, stage-file listing extended).
- Crosslink fan-out: Session-Graph ↔ Terminal-Commander, shellbridge, aoide-cli, Quickshell, Agent-Interface, Codebase (Related back-links added to each).
- Wired into `Overview.md` (Concepts bullet; aoide-cli gloss 19 → 27) and `ingest/index.md` (new bullet; aoide-cli + shellbridge glosses refreshed). SCHEMA manifest: added `concepts/Session-Graph.md` (43 → 44); Tags + graph.
- Open Threads: new "session-graph follow-ups" thread (stage_dir/`AOIDE_STAGE_DIR` seam mismatch; `focuswindow` exit-0 ambiguity; future Quickshell DAG surface + spawn-time `parentSessionId`).
- Nothing settled to `references/` — the source is the live repo, left unmodified.

## [2026-07-26] lint | full wiki (post session-graph ingest)

### Sentinel violations (0)
### Broken wikilinks (0)
- Every `[[…]]` in `concepts/Session-Graph.md` and the 7 updated pages resolves on disk (Session-Graph has 9 inbound sources). Full-wiki sweep: the only unresolved forms remain the backtick-quoted history mentions inside prior log entries (`[[…]]`, `[[Design-Tokens]]`) and the pedagogical examples in OPERATIONS code blocks — not live wikilinks. All heading links (`[[Song-Vocabulary#Replay — any song, any host]]`, `[[Self-Ricing#Adopt, Select, Replay]]`) still match their exact headings.
### Orphan pages (0)
- Session-Graph: 9 inbound wikilink sources plus its `ingest/index.md` bullet.
### Frontmatter conformance (0)
- Session-Graph carries `type` + `created` + `tags`, no `source:` (grounded via a prose "commit 0b3a3fd" line — the source is the repo, not a `references/` doc). The four content-updated pages (Terminal-Commander, Codebase, aoide-cli, shellbridge) carry `updated: 2026-07-26`; Full-Architecture already did. Agent-Interface and Quickshell received Related back-links only (optional `updated:` not stamped).
### Index drift (0)
- 27 content pages, 27 index bullets, one-to-one.
### Log format violations (0)
### Contradictions (0)
- No stale "19 commands" claims remain outside the aoide-cli sentence that deliberately decomposes 27 as 19 + 8 and log history. The Full-Architecture master-map line "reads song/stage/{sessions,hooks}.json" is accurate as-is: the Quickshell DAG surface for graph.json is future work (open thread), so Quickshell reads sessions/hooks today.
### Manifest parity (0)
- Disk matches the SCHEMA.md manifest exactly (44 files, both directions; `concepts/Session-Graph.md` added this pass). Tags line and tags in use are identical sets (`graph` added).

## [2026-07-26] ingest | Aoide repo commits ad3b21a..41be90f (host profile · seams · graph surfaces · vm-boot · gadget dock)

- Ingested the five verified commits landed after 0b3a3fd (HEAD 41be90f; `nix flake check` + the new vm-boot check green): ad3b21a (real yomi-strix hardware profile — first-iteration build), d03dcf2 (stage-dir env seam + `graph focus` window liveness), 1fedd58 (Quickshell Session-Graph surface + SUPER+G), c4843b2 (vm-boot headless QEMU check), 41be90f (gadget dock on the agentWidgets surface). Source is the repo itself; the new page is grounded via a prose "commit 41be90f" line, no `source:` field.
- Minted 1 page: `concepts/Gadget-Dock.md` — the agentWidgets surface realized as the Win7-sidebar-homage gadget column: non-exclusive right-edge posture, `GadgetFrame.qml` box-drawing chrome over Aero-glass note-background blur, the four gadgets (TerminalManagerGadget with its disabled prune `[x]`, DagGraphGadget on the shared GraphModel, ClockGadget, MeterGadget's documented `/proc` interim), the liner-recorded aesthetic decision, and the widget-maker significance.
- Updated 8 pages (short additions, not rewrites): `concepts/Session-Graph.md` (desktop surfaces built — overlay #9 + dock gadget + canonical `GraphModel.qml`; `graph focus` liveness check + the five failure reasons; Open-seams section reconciled to what closed at d03dcf2/1fedd58), `entities/Quickshell.md` (nine surfaces; the two surfaces with real bodies; GraphModel as the one graph model), `entities/shellbridge.md` (the `AOIDE_STAGE_DIR` seam fixed and contract-documented as "Stage-dir resolution", CONTRACTS §4; the socket-verb gap incl. the missing prune verb; the module-local env-test mutex hazard), `entities/aoide-cli.md` (liveness-checked `graph focus`; the `aoide shell` schema gap), `concepts/Codebase.md` (`lib/vmTest.nix` + the vm-boot check's same-assembly node and assertions; the bootable yomi-strix profile section — UUID-pinned filesystems, Strix Halo params, built-not-switched; stage-dir resolution in the runtime contracts; new QML files + nine surfaces; test additions), `concepts/Full-Architecture.md` (surgical: grounding bump to 41be90f; vm-boot in the checks prose + tree map; yomi-strix real-profile status; nine surfaces + the dock in the facet story; master-map stage-file read updated to {sessions,hooks,graph}), `concepts/Terminal-Commander.md` (the roster's desktop gadget), `concepts/Self-Ricing.md` (the default rice's liner now records the Win7 + ASCII aesthetic — songbook discipline in use).
- Crosslink fan-out: Gadget-Dock ↔ Quickshell, Terminal-Commander, Session-Graph, Widget-Maker, Self-Ricing, shellbridge, Codebase (Related back-links added to each; Notes and Rebuild-Gate linked inline).
- Wired into `Overview.md` (Gadget-Dock bullet; Session-Graph + Quickshell glosses refreshed) and `ingest/index.md` (new bullet; Codebase, aoide-cli, shellbridge, Quickshell, Session-Graph glosses refreshed). SCHEMA manifest: added `concepts/Gadget-Dock.md` (44 → 45); Tags + gadget.
- Open Threads: the session-graph follow-ups thread marked closed (stage-dir seam, focuswindow ambiguity, the DAG surface — bodies kept); a new "post-surface follow-ups" thread opened (socket verbs incl. prune + spawn-time parentSessionId, `aoide shell` schema gap, telemetry stage file, layer-shell typing v1, host built-not-switched, module-local env-test mutex).
- Nothing settled to `references/` — the source is the live repo, left unmodified.

## [2026-07-26] lint | full wiki (post ad3b21a..41be90f ingest)

### Sentinel violations (0)
### Broken wikilinks (0)
- Every `[[…]]` in `concepts/Gadget-Dock.md` and the 8 updated pages (plus the Overview/index wiring and the Widget-Maker back-link) resolves on disk, including the protocol-rule links in SCHEMA.md. The only unresolved forms remain the backtick-quoted history mentions inside prior log entries and the pedagogical examples in OPERATIONS code blocks — not live wikilinks. Both standing heading links (`[[Song-Vocabulary#Replay — any song, any host]]`, `[[Self-Ricing#Adopt, Select, Replay]]`) still match their exact headings.
### Orphan pages (0)
- Gadget-Dock: 10 inbound wikilink sources plus its `ingest/index.md` bullet.
### Frontmatter conformance (0)
- Gadget-Dock carries `type` + `created` + `tags`, no `source:` (grounded via a prose "commit 41be90f" line — the source is the repo, not a `references/` doc). The content-updated pages (Session-Graph, Quickshell, shellbridge, aoide-cli, Codebase, Full-Architecture, Terminal-Commander, Self-Ricing) all carry `updated: 2026-07-26`; Widget-Maker and Overview received wiring/back-link changes only.
### Index drift (0)
- 28 content pages, 28 index bullets, one-to-one; refreshed glosses (Codebase, aoide-cli, shellbridge, Quickshell, Session-Graph) still describe their pages accurately.
### Log format violations (0)
### Contradictions (0)
- No stale "eight surfaces"/"8 surfaces" claims remain (Full-Architecture I/O row and facet bullet, Quickshell, and the Overview gloss all say nine). No page still calls the DAG surface future work, claims `stage_dir()` ignores `AOIDE_STAGE_DIR`, or leaves `graph focus` unable to detect a vanished window — the closed-thread bodies in Open Threads are struck through, not live claims. The Stylix stand-down mention of agentWidgets remains accurate for the dock.
### Manifest parity (0)
- Disk matches the SCHEMA.md manifest exactly (45 files, both directions; `concepts/Gadget-Dock.md` added this pass). Tags line and tags in use are identical sets (`gadget` added).

## [2026-07-26] ingest | Aoide repo commits 8f4034e..9404250 + the live switch

- Ingested the two commits after 41be90f (HEAD 9404250; flake check + re-run vm-boot green) **plus a status change bigger than either**: the user opened the rebuild gate and **yomi-strix switched from dxflake onto the Aoide flake** the same night — two switches (`aoide-switch`/`aoide-switch2` detached systemd-run units: `nix-env --profile` set + `switch-to-configuration switch`, ~8 s activation each), previous dxflake generation retained in systemd-boot as rollback; greetd/Hyprland-session-entry/NetworkManager/zram/Strix-Halo-amdgpu all verified active. Source is the repo + live box; no new pages, no `references/` settlement.
- **8f4034e — dock popup redesign** ([[Gadget-Dock]] rewritten in place): the always-visible right-edge column became a hidden-by-default **left-edge pinnable popup** — 5 px full-height hot strip (pure QML, live with the stubbed bridge) or SUPER+G via the new stub verb `aoide shell dock toggle` (replacing `shell graph toggle`; keybind path defined as open-and-pin), 150 ms OutCubic slide, `[+]`/`[■]` pin affordance in a `╔═[ GADGETS ]══[pin]═╗` header (a chrome idiom beyond GadgetFrame's title bar), a real timer-gated 400 ms auto-hide grace (Fable review fix — the first cut's timer gated nothing), one strip+panel hover union via NoButton catchers, explicit hidden/peeking/pinned state table. Gadgets + frames unchanged. `AoideSessionGraph` lost its keybind — dormant/bridge-only, kept as the seam for a future full-screen DAG view; the dock popup is the primary DAG affordance. v1 note sharpened: the dock should claim a non-exclusive top/overlay layer once layer-shell typing lands.
- **9404250 — nucleus baseline gaps found live** (the first switch surfaced what eval, build, AND vm-boot had masked): `aoide`/`aoide-notes` were never on the system profile (units ExecStart store paths, so they ran; by-name invocation didn't) — new `modules/nucleus/packages.nix` installs them plus **git** (load-bearing: flake ops on the fork need it; dxflake's nucleus had it, the essentials-only port cut it); nix-command/flakes weren't enabled (a flake-native system unable to evaluate itself) — new `modules/nucleus/nix.nix` enables them; both gate on `aoide.enable`. `lib/vmTest.nix` had self-provided the binaries on the test node, hiding the gap — it now keeps only `jq` and exercises the real nucleus install path. The generalized **test-masking lesson** (eval green + build green + VM green ≠ complete; a VM test proves only what it doesn't self-provide) is recorded in [[Codebase]]'s vm-boot section.
- Updated 8 pages (rephrased, no transcription): `concepts/Gadget-Dock.md` (posture section rewritten: popup, state table, grace, pin, dormant-overlay note), `concepts/Session-Graph.md` (dock gadget promoted to primary DAG affordance; overlay dormant; grounding bump), `entities/Quickshell.md` (popup posture; dormant overlay; the ~/Aoide/qml deploy-target question), `entities/aoide-cli.md` (`shell dock toggle` in the schema-gap note; grounding bump), `concepts/Codebase.md` (switched-live section replacing built-not-switched; nucleus packages.nix + nix.nix; vmTest jq-only + test-masking lesson), `concepts/Full-Architecture.md` (Status section retitled "running live on yomi-strix"; facet story; tree-map nucleus + hosts lines), `concepts/Rebuild-Gate.md` (one line: the gate's first real exercise), `Overview.md` (live-status sentence; Gadget-Dock + Session-Graph glosses).
- Crosslink fan-out: [[Gadget-Dock]] ↔ [[aoide-cli]] Related links added (both directions); other pairs already linked.
- `ingest/index.md` glosses refreshed (Full-Architecture, Codebase, Session-Graph, Gadget-Dock, Quickshell). No manifest change (no files added/removed; still 45).
- Open Threads: the post-surface thread's built-not-switched item **closed** (switch detail + rollback recorded in the closed body) and its `aoide shell` item extended with `dock toggle`; a new **post-switch follow-ups** thread opened (QML deploy-target-vs-source decision; first-boot desktop verification pending).

## [2026-07-26] lint | full wiki (post live-switch ingest)

### Contradictions (1, fixed)
- `concepts/Gadget-Dock.md`'s DagGraphGadget bullet still said "always-on" after the popup redesign made the dock hidden-by-default — fixed in the same session (now "the desktop's primary DAG affordance now that the overlay is dormant"). Follows the precedent of fixing stale remnants surfaced by the pass that created them.

All other checks clean:
### Sentinel violations (0)
- No sentinel blocks exist in the wiki (the only `BEGIN SENTINEL` string is SCHEMA.md's rule text).
### Broken wikilinks (0)
- Every `[[…]]` target across concepts/, entities/, Overview.md, SCHEMA.md, and ingest/index.md resolves on disk (script-swept). Both standing heading links (`[[Song-Vocabulary#Replay — any song, any host]]`, `[[Self-Ricing#Adopt, Select, Replay]]`) still match their exact headings. The unresolved forms remain only the backtick-quoted history mentions in prior log entries and the pedagogical examples in OPERATIONS code blocks.
### Orphan pages (0)
- No pages added or removed this pass; all pages retain their inbound links and index bullets.
### Frontmatter conformance (0)
- All eight content-updated pages (Gadget-Dock, Session-Graph, Codebase, Full-Architecture, Rebuild-Gate, Quickshell, aoide-cli, Overview) carry valid fields; `updated: 2026-07-26` newly stamped on Gadget-Dock and Rebuild-Gate, already present on the rest (Overview takes no `updated:` per the field table).
### Index drift (0)
- 28 content pages, 28 index bullets, one-to-one; the five refreshed glosses (Full-Architecture, Codebase, Session-Graph, Gadget-Dock, Quickshell) describe their pages accurately.
### Log format violations (0)
- All dated headings validate against `## [YYYY-MM-DD] <action> | <target>` with recognised tokens.
### Manifest parity (0)
- Disk matches the SCHEMA.md Notes manifest exactly (45 files, both directions; nothing added or removed). Tags line unchanged and still matches the tags in use. Consistency sweep: no live "right-edge"/"always-visible"/"built, not switched"/"shell graph toggle" claims remain outside historical framing; every SUPER+G mention now points at the dock popup.

## [2026-07-27] ingest | Aoide repo commits 9e2eb61..8f062df — the port finishes, the rice loads, the first song's design loop

**Log-only entry** (37 commits on `worktree-devtools-dendrites` + wiki file additions; concepts/entities page updates deferred — see the new open thread). Grouped by theme, not per-commit:

- **Port completion** (9e2eb61..e14833e, 2026-07-26 evening): dev-tool dendrites + `ad*` alias family, nvf neovim (full dxflake stack), fonts dendrite (Lekton stylix face; azuki kana later dropped), cover-art note seam (hero.webp), bar waybar-homage + first ASCII gadgets, staff-run ornament vocabulary, compositor keybinds, session assembly (quickshell/polkit services), melete+mneme in the flake, README use/update guides, devtools shelved from baseline (godot/arduino-ide/jupyter).
- **The rice comes alive** (c782689, 4e9a22d): quickshell 0.3.0 API fixes (`FileView.text()` method, no StandardPaths → `Quickshell.env("HOME")`, `qs -p`) — the shell loads and surfaces register; glass blur layerrules land behind named namespaces (Hyprland 0.56 snake_case fields).
- **Vision** (abb61ea): two screenshot dendrites — `aoide.screenshot` (hyprshot+satty, human) and `aoide.vision` (grim/slurp primitives, agent) — enabled on yomi-strix. The design loop's grim + forced-state screenshot idiom dates from here.
- **Dock & bar restructure** (907f8f3, 04d22bf, b08028f, 25bdf57→retired): dock becomes the glass case for the agent pair (TERMINALS+DAG), every other gadget bar-spawned via real `PopupWindow` popouts (the clipped-popout bug: children can't draw past a 28px layer surface); bar v3 geometry (36px) + Aero gloss; Win7 taskbar buttons landed then LEFT the strip in the pantheon redesign (terminals are widget-tracked only, per the user).
- **Baton + toolchain** (1664018, 45633f2, 774c43d, 531704e, 0117a79): the conductor's TUI ported to ratatui/crossterm (DAG panel, roster lens); `aoide rice lint/preview` made real, `graph view` restages after mutations, honest `--help`/unknown-flag handling (43 tests); `pkgs/` walker self-registration; the note engine renamed **livery**.
- **The first song** (4459c7b, 6f6443f, cf43447, 8567ff1): `hero` — dusk-plum palette from `song/covers/hero.webp`, wallpaper reads `stage/cover.json`; **hyprglass** plugin on the bar/dock surfaces; square-corner BW window key; a real all-or-nothing **base16 tier** in the nucleus options → stylix (terminals speak "pantheon bw").
- **The Pantheon design loop, rounds 1–4** (b960332, ab484ea, 279c0e8, 3d03d95, a3e92fd, 8f062df — run as reviewed agent rounds against `references/pantheon/`): wireframe depth stacks over the aero glass → neon dominance (one blaze against a dim field, drawn kinked leaders, glyph unification ♪𝄐𝄽𝄂) → tear-off floating gadgets with trace → the multicolor field (base16 semantic roles wireCyan/holoBlue/violet/glitchPink through livery + LiveryState), vanishing-point depth directions (all offset stacks lean toward screen centre), orchestration callout vocabulary (`gadgets.case`, `baton.control`, `terminals.roster`, `dag.trace`, …), workspace numbers at true screen centre, the 𝄞 clef sized to fit the strip (pop-out apron tried and rolled back), rose accents purged from the bar. Full grammar: `design/Pantheon-Grammar.md` (agent-maintained, new).
- **The session write door** (fa600ae): `aoide graph session start/phase/end/hook` — the missing writer for `sessions.json`/`hooks.json` (the shellbridge socket remains a skeleton; these CLI verbs are the interim registrar). The `hook` verb reads a Claude Code hook payload from stdin and never exits non-zero; `.claude/settings.json` hooks (SessionStart/UserPromptSubmit/Stop/SessionEnd) wire every harness session through it. **This closed the user-visible defect "agents are invisible to the widgets/baton"** — the dock now renders the real session DAG. 53 Rust tests.
- **Known blemish**: 3d03d95 is a mixed commit (a shared-index `git add && git commit` swept a concurrent agent's staged round-3 files into the bar-v4 commit; gates re-run green; splitting requires the user's hand — classifier blocks history rewrites in the shared worktree).
- **Wiki file additions this session** (unmanifested until the next lint): `design/Pantheon-Grammar.md`, `design/Baton-3D-DAG.md` (the planned ratatui 3D wireframe DAG renderer — spatial projector, braille canvas, ANSI-frame widget embed; implementation awaits the user's green light), `references/pantheon/` (5 stills + 8 art-direction stills), `references/dxflake-rice-screenshot.png`; `entities/shellbridge.md` + `entities/aoide-cli.md` edited in place (session verbs documented, socket noted still-future).
- **Status**: everything above is live on yomi-strix ONLY via the worktree QML drop-in + hot-reloaded stage files; the flake side (dendrites, hyprglass, hero song, base16 terminals, session verbs on PATH) waits on the user's merge + gated switch.

## [2026-07-28] refactor | design protocol → songbook model + Song-Anatomy
- **Reworded the design protocol to point at a songbook under `song/`** (khoa steer: per-song design memory is the song agent's domain, not the dev wiki). Added a framing block to `design/Ricing-Protocol.md` and `design/Pantheon-Grammar.md`: the dev wiki documents architecture/protocol; per-song and cross-cutting *design memory* (palette rationales, opacity numbers, the visual grammar) belongs to the song agent in `song/songbook/` (cross-cutting) + `song/repertoire/<name>/liner/` (per-song). Existing content is reframed as "what the songbook records, mirrored here," not deleted; noted the songbook is sparse/aspirational today (`song/songbook/` is a placeholder). Kept the load-bearing protocol (creation/application split, the vision-check) and QML constants in the wiki.
- **Minted [[Song-Anatomy]]** (`concepts/Song-Anatomy.md`) — the performed-half sibling of [[Snowflake-Anatomy]]: the role of every `song/` subfolder verified against the real tree (`repertoire/<name>/` {rice.nix, livery.json, liner/} · `songbook/` · `covers/` · `keys/` · `chimes/` committed; `stage/` · `backstage/` · `auditions/` gitignored runtime), committed-vs-runtime, who writes each, and the six `stage/` files (livery/sessions/hooks/projects/graph/cover.json). Songs on disk: `sonata` (cream Alma-Tadema light key, selected) + `hero` (dusk-plum).
- **Handoff flag added** — `[refactor · khoa]` in [[AOIDE-DEV]] §7: design/songbook content should migrate into `song/`; protocol reworded to point there; content migration pending. Also closed the §7 `[decision] notes vs livery` flag (livery won, shipped — commit 0117a79).

## [2026-07-28] index | design/ + Song-Anatomy + references manifested
- `ingest/index.md`: added [[Song-Anatomy]] under Concepts; added a `## Design` section (Ricing-Protocol, Pantheon-Grammar, Conductor-Channel, Baton-3D-DAG) and a `## References` section (AOIDE-HANDOFF, AOIDE-DEV-HANDOFF).
- `SCHEMA.md`: added `design/` to the shape diagram + the read-whole-thing path; added `concepts/Song-Anatomy.md` and the four `design/*.md` pages to the Notes manifest; snapshot bumped 2026-07-26 → 2026-07-28; tag set extended (baton, conductor, dag, design, glyph, orchestration, pantheon, protocol, pty, song, tui). Pantheon `.png` stills stay assets, not manifested notes.
- Frontmatter backfilled on `design/Pantheon-Grammar.md` (had none) and `design/Baton-3D-DAG.md` (had none) with `type: design`.

## [2026-07-28] lint | full wiki — livery/sonata/identity/counts sweep
- **Songs**: fixed the light key attribution — the cream Alma-Tadema *Unconscious Rivals* key is **`sonata`** (currently selected on yomi-strix), not `hero` (which reverted to its dusk-plum key); `moonlight` is retired (reserved for a planned dark `moonlight-sonata`). Corrected `design/Ricing-Protocol.md`, `design/Pantheon-Grammar.md` (round 5), `entities/Stylix.md`, and the `aoide.song = "moonlight"` replay examples in `concepts/Song-Vocabulary.md` + `concepts/Self-Ricing.md` (→ `sonata`), plus `concepts/Full-Architecture.md` (fixture + tree map).
- **livery, not notes**: `## Note schema v0` → `## The livery schema v0` (`entities/livery.md`, `concepts/Full-Architecture.md`); "Note schema" → "livery schema" (`concepts/Governance.md`); "the note engine" → "the design-token mint"/"the livery engine" (`Overview.md`, `ingest/index.md`); `design/Pantheon-Grammar.md` `notes.paletteAccent` → `livery.paletteAccent`.
- **Identity**: "bundled" → "integrated (not vendored)" for Melete/Mneme (`concepts/Lexicon.md`, `ingest/index.md`). Overview/Widget-Maker framing (AoideOS = distribution/widget-maker; Aoide = shell-only core) already correct — left intact.
- **Command count**: 27 → **28** (verified: the vm-boot check asserts exactly 28; README = 28; `aoide baton` is command 28) in `Overview.md`, `ingest/index.md`, `concepts/Full-Architecture.md` (×2), `concepts/Codebase.md` (×2, with the graph-group delta kept as history).
- **Feature status**: click-to-jump, the window→session socket2 listener, and the holoBlue workspace-hover-preview are already documented (feature agent) across [[Terminal-Commander]]/[[shellbridge]]/[[Session-Graph]] — verified current, not duplicated.
- **Wikilinks**: full sweep — no live broken links. Remaining unresolved forms are OPERATIONS pedagogical examples, immutable dated-log history mentions (`[[Design-Tokens]]`), and `[[aoide-notes]]` (resolves via the entity page's `aoide-notes` alias). Kitty opacity corrected to the re-tuned 0.86 (was frozen at 0.60 in Pantheon round 5).

## [2026-07-28] refactor | "notes" retired as a term — livery is the only token-layer name
- **`concepts/Notes.md` retired and merged into [[livery]]** (khoa steer: "notes should be livery in the concepts"). Both pages already asserted that "notes" and "livery" are *one thing, not a values-vs-engine split* — so keeping a `Notes` concept beside a `livery` entity re-created exactly the split the pages disclaimed. The token layer now has **one page**. Folded into `entities/livery.md`: the score/performance seam framing, "every facet consumes livery and nothing else", the two-fan-outs/one-source diagram + the zero-drift guarantee, the tier-structure table, prior-art (wrap-don't-rewrite), the provisional-v0 note, and Stylix overlap resolution.
- **`Notes` kept as a livery-page alias** (`aliases: [aoide-notes, Notes, notes package, note engine]`) so the dated-log history below — which legitimately says `[[Notes]]` — still resolves. Dated entries keep their facts and dates untouched; only the engine's name is normalized to livery tree-wide, so no earlier spelling of it survives anywhere.
- **Sweep across 28 files** (27 edited + the retired page; 3 parallel agents, partitioned by file, reviewed here): `[[Notes]]`/`[[Notes|notes]]` → `[[livery]]`, de-duplicating where a `## Related` list already carried `[[livery]]` (Codebase, Stylix, Song-Anatomy). Token-layer prose retermed: "note package/engine" → livery package, "note values/colours" → livery values/colours, "the note seam" → the livery seam, "semantic/component note tier" → semantic/component tier, "notes-coloured"/"note-themed" → livery-themed, "carries notes only" → carries livery only, "notes emitter" → livery emitter, "note-schema migrations" → livery-schema migrations. `Full-Architecture.md` frontmatter tag `notes` → `livery`; its two ASCII boxes relabeled `NOTES` → `LIVERY` at identical 13-char width (column geometry verified unchanged against HEAD — the pre-existing right-side connector offset is untouched, not a regression).
- **Four other senses of "note" deliberately preserved** (the whole discernment of this pass): Mneme **vault notes** (a different product); per-song **design/liner notes**; the **musical-note glyph ♪** in UI descriptions ("note-glyph", "ASCII/box-drawing note theming", "tint on the note"); **"songbook note"** = a recorded rejection; and `| Notes |` table headers meaning "remarks". Also left: `notes-binary-unavailable`, a real error-code string in `pkgs/aoide/src/dispatch.rs:246`.
- **Dead-example repair**: `protocol/OPERATIONS/Wikilinks.md` and `protocol/OPERATIONS/Naming.md` taught their rules using `Notes.md` and its exact old H1 as examples — repointed to live pages (`concepts/Self-Ricing.md`, `[[livery]]`, `Song-Vocabulary.md` → `# Song Vocabulary — the Performed Half`).
- **`SCHEMA.md`**: `concepts/Notes.md` dropped from the Notes manifest; tag set corrected — `notes` removed, `livery` added (it had been missing despite `entities/livery.md` carrying it). **Manifest parity re-verified exact: 52 = 52.**
- **`Overview.md` / `ingest/index.md`**: the Notes concept bullet removed and its substance folded into the livery entity gloss (seam + tiers + two-fan-out), so no answer was lost with the page.

## [2026-07-28] refactor | assertion rule adopted — content pages state the system as it is
- **New rule page `protocol/OPERATIONS/Assertion.md`** (khoa steer: *"dont say 'never' or what was, just state what is… 'what ifs' should be asked about or flagged for possible development"*). Three clauses bind every content page (`concepts/`, `entities/`, `design/`, `Overview.md`): **(1) current state only** — no `used to` / `no longer` / `formerly` / retrospective "lesson" framing; history belongs to this log, rationale to the commit message. **(2) no definition by contrast** — a thing is defined by what it is, not by the alternative not taken (`was rejected`, `chosen over`, `not a X-vs-Y split`, the objection-pre-empting aside). **(3) speculation is routed, not written** — `would`/`could`/`might`/`eventually`/`TBD` leave page prose for `## Open Threads` or a dev-handoff flag; an agent that meets a what-if mid-edit asks or flags it rather than resolving it silently in either direction.
- **The carve-out is the load-bearing part.** Present-tense exclusion is a *specification*, not a negation of history: `a facet reads aoide.livery and nothing else`, `graph reap never errors on "nothing to reap"`, `the agent never holds the password` all describe HEAD and stay. The rule prefers the positive form where it is complete (`reads only X` over `never reads Y`) and keeps `never` for when the absence *is* the guarantee. Without this, the rule degrades into a find-and-replace that would have gutted [[Session-Graph]] — whose "never"s were audited one by one this pass and kept in full, 0 removed.
- **Documenting the unbuilt**: a specified-but-unbuilt design is not deleted, it gets a `**Status:**` label plus present-indicative statements *about the design* — the design exists, so statements about it are present-tense true; only the code's state needs the label. [[Rebuild-Gate]]'s `aoide.rebuild` section is the worked example carried in the rule page itself.
- **Wired in**: `SCHEMA.md` (OPERATIONS list + a new "what you never do" bullet), `protocol/SHAPE.md` (rules), `protocol/OPERATIONS/Lint.md` (**check 9, Assertion violations**, with a grep first-pass and the explicit warning that matches are candidates, not findings — invariants pass).
- **Sweep across all 19 concept pages** (7 parallel Sonnet agents, partitioned by file, every diff reviewed here). ~60 fixes. Status labels added to 9 specified-but-unbuilt sections across [[Rebuild-Gate]], [[Widget-Maker]], [[Gadget-Dock]], [[Fork-and-Run]], [[Wiki-Protocol]], [[Governance]], [[Feature-Set]] (×3), [[Full-Architecture]]. `Agent-Interface.md`, `Song-Vocabulary.md`, `Snowflake-Anatomy.md` needed no changes.
- **No history was lost to the sweep** — verified, not assumed. Every narrative the agents removed was already recorded in this log: the test-masking lesson and the git-loss (both in the 9404250 entry above), the two-switch dxflake→Aoide migration (the post-skeleton thread's closed built-not-switched item, and the 2026-07-27 ingest entry), the `aoide shell` schema gap (three places). The pages had been duplicating the log; that duplication is what clause 1 removes. The one genuinely untracked item — *whether the NotificationServer spike passing decides if Quickshell keeps the daemon role* — was routed into the post-skeleton backlog thread above rather than dropped.
- **Protocol pages** are bound by clauses 1 and 3 but write in the imperative, so the phrasing preferences do not apply; `ingest/log.md` is exempt by construction. Fixed under that scope: `SCHEMA.md` ("bound to migrate"/"bound to relocate" ×2), `protocol/PROTOCOL.md` ("a future `aoide` subcommand"), `Overview.md` ("chosen over a sudo password").
- **Two stale facts surfaced by the sweep**, both corrected: `protocol/SHAPE.md` and `concepts/Wiki-Protocol.md` still claimed the wiki lives at `~/Aoide-Wiki` "until that repo exists" — it lives at `docs/Aoide-Wiki/` in the Aoide repo, and `~/Aoide-Wiki` does not exist on disk. Instructive failure mode: one agent stripped the hedge and left behind a *confident falsehood*, which is worse than the hedged staleness it replaced. Caught in review.
- **Manifest parity re-verified exact: 53 = 53** (`Assertion.md` added). Scope note: `entities/` and `design/` were **not** swept this pass — they carry the same drift (e.g. `entities/livery.md` "Building against no schema at all was rejected"; the NotificationServer contingency still duplicated in `entities/Quickshell.md`).

## [2026-07-28] refactor | assertion sweep across entities/ + design/, and design/ → songbook/
- **`design/` renamed to `songbook/`** (khoa steer). The wiki folder that mirrors song-agent design memory now carries the name of the thing it mirrors. `git mv` preserves history; the four pages are unchanged in identity (`Baton-3D-DAG`, `Conductor-Channel`, `Pantheon-Grammar`, `Ricing-Protocol`). The `references/pantheon/` stills **stay in `references/`** — they are source material, not design memory (khoa steer).
- **The rename was two targeted rewrites, not one.** A blind `design/` → `songbook/` substitution would have corrupted the repo's **per-song** `song/songbook/<song>/design/` path into `…/songbook/songbook/…` across [[Self-Ricing]], [[Content-Pipeline]], [[Song-Vocabulary]], [[Gadget-Dock]], [[Full-Architecture]] and the handoff. Only the four page paths and the wiki-folder references were rewritten; all eight per-song `design/` paths verified intact afterward, and a `songbook/songbook` grep confirmed zero corruption.
- **13 `[[design/…]]` links in dated log entries above are left as written.** Dated entries are immutable ([[Lint]] append-only rule); they record the path that was correct when written. They are breadcrumbs, not lint defects — this entry is their covering note.
- **Assertion sweep across all 11 `entities/` and all 4 `songbook/` pages** (5 parallel Sonnet agents, every diff reviewed here). `aoided.md` and `Hyprland.md` needed nothing. Status labels added to [[Stylix]] (×2), [[Quickshell]], [[songbook/Ricing-Protocol]], [[songbook/Conductor-Channel]], [[songbook/Baton-3D-DAG]].
- **Three pieces of engineering history existed ONLY in page prose and are recorded here** — found by checking each removal against this log rather than trusting the sweep:
  - **`hyprctl` must be on the systemd user unit's PATH.** A user unit's default PATH is minimal and excludes the compositor, so `focus_window` and the window→session listener fail with `hyprctl unavailable: No such file or directory` — audited as `focus-failed`, never journalled, so every widget click dies silently while the CLI `graph focus` keeps working on the caller's richer PATH. Fix: unit-level `path = [ pkgs.hyprland ]` on `shellbridge` and `aoide-graph-reap`. **The gotcha:** `path` is a *unit* option, a sibling of `serviceConfig` — nesting it inside `serviceConfig` emits an inert raw `path=` line and PATH stays broken. **The general lesson:** a user service that shells out to desktop tools needs them put on PATH explicitly; a missing binary is invisible until you check the daemon's own env.
  - **The drop-in crash-loop.** A leftover systemd drop-in pinned `ExecStart` to a `.claude/worktrees/…` path that was later deleted. `ConditionPathExists` still watched the valid baked path, so the condition passed and the unit crash-looped 77× — no bar, no dock, no wallpaper. Fix: delete the drop-in and let the baked unit stand. The standing hazard (a drop-in pinning `ExecStart` to a worktree path defeats `ConditionPathExists`) is stated on [[Quickshell]]; the incident is here.
  - **The opacity tuning rounds.** Kitty/bar opacity went 0.72 → kitty 0.60 / bar 0.58 (too transparent — the warm painting bled through and surfaces read beige, not light) → kitty 0.86. The instinct at the low point was to whiten `base00`; khoa's correction drew the real distinction — keep the cream-and-ink colour, make the surface read *brighter*. The durable rule (**brightness is opacity, not colour**) stays on [[songbook/Ricing-Protocol]]; the round-by-round narrative is here. By that page's own rule this is per-song design memory and belongs in `song/songbook/sonata/design/intent.md` — noted in the handoff residual (b).
- **`Pantheon-Grammar` restructured out of changelog shape.** `## Round 4 — … (2026-07-27)` and `## Round 5 — … (2026-07-28)` became `## 5. The multicolor field, and the bar in the same grammar` and `## 6. The light key — cream`; `## 3. What was removed, and why` became `## 3. What the grammar excludes, and why` (its table column `removed` → `excluded`). Verified first that no page anchor-links any of them. Five dangling `round N` pointers repaired across [[Stylix]], [[Song-Anatomy]], `ingest/index.md`, and the page itself; the one legitimate use (a songbook holds a round-by-round iteration log) kept.
- **The rule contradicted itself and was amended.** Clause 3 said unbuilt designs "do not appear as page prose" while "Documenting the unbuilt" said they stay under a status label. Clause 3 now governs **undecided** questions only, with an explicit carve-out: a decided-but-unbuilt design is documented, an undecided one is routed. The test is whether the answer is known. Caught by the [[Self-Update]] self-description test, not by review.
- **One agent claim checked and rejected:** the QML deploy-target question was reported as "tagged inline but never routed" — it is a full Open Thread above with all three options. No action taken. **Manifest parity re-verified exact: 53 = 53.**

## [2026-07-29] refactor | sonata performs its own cover — covers/ restored, hero retired, webp decodable
- **`song/covers/` restored as the shared wallpaper library** (khoa steer: wallpapers are shared assets other consumers can use, not per-song private files — this reverses the per-song `assets/` placement from the 2026-07-28 songbook consolidation and returns to the scaffold-era location, commits 80466b0/d4257e6). `git mv` preserved history: `hero.webp` → `song/covers/sonata.webp`, the Alma-Tadema JPEG alongside it. A song's `rice.nix` references a cover as `../../covers/<file>` — versioned score, legal at eval (`noSongRead` bans only `stage/ · auditions/ · catalog/ · index/`). `derive_cover` (dispatch.rs) repointed to `song/covers/<name>.<ext>` — the `cover.<ext>` probe dropped as ambiguous in a shared dir; cargo suite green (78).
- **The `hero` song is deleted; the songbook holds `default` and `sonata`.** Its dusk cover is now sonata's: the palette+base16+window tiers re-keyed region-by-region from `sonata.webp` (Opus agent with vision, reviewed here against the image) — pale peach-cream cloudbank `base00 #f4e9e2`, the piano's near-black read as plum ink `base05 #3b2f3a`, dusk slate-blue accent, crimson piano-cushion urgent, horizon-green hot; computed light-polarity contrast (base05 10.64:1). `default`'s wallpaper note is `null` (the stylix facet bakes its solid from `palette.bg`) — the long-standing borrow-hero's-cover TODO resolved by construction. The re-key also reconciled a comment/value mismatch: `window.borderInactive` now genuinely recedes to `base01` as its comment always claimed.
- **Why the wallpaper "didn't run" after the rebuild — a format gap, not a path bug.** The baked `AOIDE_WALLPAPER` env and the store path were both correct; quickshell's Qt6 runtime simply carries no webp imageformats plugin (qtbase ships jpeg/png/gif/ico + qtsvg only), so `AoideWallpaper`'s `Image` hit `status Error` and fell back to the solid-`paletteBg` rectangle. The Alma-Tadema cover had masked this — it is a JPEG. Fix: the quickshell facet exports `QT_PLUGIN_PATH=${qt6.qtimageformats}/…/plugins` on the service; the wrapper's `makeWrapper --prefix` preserves the inherited value, and the plugin links against the identical `qtbase-6.11.1` store path (quickshell follows this flake's nixpkgs), so the ABI is exact. **The general lesson:** an asset pipeline is bounded by the *renderer's* decoder set, not the builder's — nix will happily bake a cover no surface can draw.
- **Text-outline survey: not needed, verified not assumed.** Every Text element traces back to a glass backing (bar sheet 0.58, dock/gadget frames 0.72, SessionChip's own rectangle); nothing renders bare on the wallpaper layer. No QML changed.
- **Wiki reconciled in the same pass** (one Sonnet mover + one Opus ricer, every diff reviewed here): cover-home path updated across [[Song-Vocabulary]], [[Song-Anatomy]], [[Self-Ricing]], [[Codebase]], [[Full-Architecture]], [[Snowflake-Anatomy]] (covers carved out as the one shared exception to "all ricing content is per-song"); hero references retired from [[Stylix]], [[songbook/Pantheon-Grammar]], [[songbook/Ricing-Protocol]] (whose worked example now teaches the actual sonata.webp keying); `songbook/sonata/design/intent.md` rewritten to the shipped key — handoff residuals (a) and the intent-refresh half of (b) closed. A **comprehensive quickshell wallpaper switcher** (picker over `song/covers/`, `stage/cover.json` as the seam, crossfade, per-monitor) is flagged in the dev handoff (khoa steer).

## [2026-07-29] refactor | rice design memory lives per-song — Pantheon grammar → the default rice
- **The standing rule (khoa):** any design regarding a rice lands in that song's `song/songbook/<name>/design/`; cross-cutting house grammar is the default rice's design memory. The wiki's `songbook/` folder carries protocol, build specs, and pointer pages only. Stated in `SCHEMA.md` (read-order note + tree), [[songbook/Ricing-Protocol]]'s scope blockquote, [[Song-Anatomy]], and the dev handoff §6 (the every-change checklist now routes rice design to the songbook).
- **The Pantheon grammar migrated** to `song/songbook/default/design/pantheon.md` — the default rice (The Standard) owns the house design language (khoa: "put the pantheon stuff in the default rice"). Content: depth recipe + GadgetFrame constants, glyph grammar tiers, exclusions table, DAG showpiece, multicolor role seam + bar grammar + one-bar-of-music strip (restated in role terms — paletteFg/accent/glitchPink — the song-agnostic form), a new "the glass" section (brightness-is-opacity, one-glass coherence, the kitty matcher gotcha), and the fetch greeting. `songbook/Pantheon-Grammar.md` is now the pointer page; its §-anchors retired, so the three §6 pointers ([[Stylix]], [[Song-Anatomy]] ×2) were repointed at the songbook homes or inlined ([[Song-Anatomy]]'s AOIDE_WALLPAPER note now states the fact and links [[Quickshell]]). `ingest/index.md` catalog line updated. Manifest count unchanged (the page remains, thinned).
- **sonata's current elements recorded** in `song/songbook/sonata/design/intent.md` (khoa: "sonata will have its own design elements now — record whats current"): a surface table — bar sheet/popouts glass 0.45, kitty 0.86 (unfocused fade 0.90), launcher/dock 0.72, workspace strip (plum resting notes, accent swell, glitchPink pulse), window borders `#4f8598`/`#ecdcd8`, the one blaze (✎N cell + trace, horizon-green), cover `yuki-sonata.png` — plus dated iteration lines for the 0.62/0.60 → 0.45/0.45 glass change and this migration. Handoff residual (b) closed.
- **Orchestration model updated in the handoff §2 (khoa):** the main dev agent is an **Opus agent orchestrating** — decomposes, dispatches, reviews every diff, lands. Workers: Opus codes (design-critical/vision), Sonnet generals (search/mechanical sweeps). **Fable advises** — consulted on the decisions and outputs of the Sonnet/Opus workers, a judgement tier over the fan-out, not a worker.

## [2026-07-29] refactor | concepts/ split by part of Aoide; songbook/ dissolved; Pantheon-Grammar retired
- **Wikilinks normalized to bare form first** (khoa-approved restructure, prerequisite for the moves below): every live `[[songbook/X]]` and `[[design/X]]` wikilink across the wiki rewritten to bare `[[X]]` — Overview.md, entities/Stylix.md (×4), references/AOIDE-DEV-HANDOFF.md (×3), ingest/index.md, ingest/log.md's Open Threads (×1; the Pantheon-Grammar Open Thread line is handled below), concepts/Song-Anatomy.md, concepts/Song-Vocabulary.md, and the songbook pages' own outbound links. Dated entries above are untouched — they are immutable breadcrumbs per the Lint append-only rule ([[Lint]]), same convention as the 13 `[[design/…]]` mentions noted 2026-07-28.
- **`songbook/Pantheon-Grammar.md` deleted from the wiki.** Its only home is now the repo file `song/songbook/default/design/pantheon.md` (the default song's design memory) — the wiki page had thinned to a pure pointer at the 2026-07-29 migration above, and a pointer page for repo-resident design memory is no longer load-bearing once the grammar has one canonical home. The ~9 inbound wikilinks were rewritten, not left red: entities/Stylix.md's Related now points at [[Song-Anatomy]]; concepts/Song-Anatomy.md's prose and Related now name the protocol page directly; concepts/song/Ricing-Protocol.md's two mentions (a vision-check aside, a Related bullet) reworded to point at [[Song-Anatomy]] or dropped as redundant with an existing Related entry; concepts/orchestration/Baton-3D-DAG.md's Related now cites the repo path in backticks plus [[Song-Anatomy]]; references/AOIDE-DEV-HANDOFF.md's one live mention repointed to [[Self-Ricing]]; ingest/index.md's Pantheon-Grammar bullet dropped with the rest of the `## Design` section (folded below); the two `ingest/log.md` Open Thread mentions reworded (one bare-ified to [[Baton-3D-DAG]], one reworded to a backtick repo-path mention since the page it named is gone).
- **`concepts/` reorganized by part of Aoide, and the wiki's `songbook/` folder dissolved** (`git mv`, history preserved): new subfolders `concepts/orchestration/` (Agent-Interface, Content-Pipeline, Session-Graph, Terminal-Commander, plus songbook's Conductor-Channel and Baton-3D-DAG), `concepts/desktop/` (Desktop-Architecture, Widget-Maker, Gadget-Dock, Feature-Set), `concepts/song/` (Song-Anatomy, Song-Vocabulary, Self-Ricing, plus songbook's Ricing-Protocol), `concepts/governance/` (Governance, Rebuild-Gate, Fork-and-Run, Wiki-Protocol); `concepts/` root keeps the genuinely cross-cutting four (Codebase, Full-Architecture, Snowflake-Anatomy, Lexicon). The three surviving songbook pages (Conductor-Channel, Baton-3D-DAG, Ricing-Protocol) had carried `type: design` — a note kind never defined in [[Frontmatter]] — so each is restamped `type: concept` on the move, now that they live under `concepts/` and answer to its field set. The wiki `songbook/` directory is empty after the three moves + the Pantheon-Grammar deletion, so it is removed — dissolving a top-level dir, the shape change this restructure exists to make. Bare-name wikilinks made every move link-safe: no in-body link needed rewriting for the move itself, only for the Pantheon-Grammar deletion above.
- **`ingest/index.md` restructured to match**: the `## Design` section (which never conformed to [[Indexing]]'s "grouped under `## Concepts` and `## Entities`" rule) is dissolved — Ricing-Protocol, Conductor-Channel, and Baton-3D-DAG fold into `## Concepts` as ordinary bare-linked entries; the Pantheon-Grammar bullet is dropped, not carried forward.
- **`SCHEMA.md` rewritten**: the "Read the whole thing" note now names the four `concepts/` subfolders and points rice-design-memory readers at `concepts/song/Ricing-Protocol.md` rather than a wiki `songbook/` folder that no longer exists; "## The shape" diagram replaces the `songbook/` line with the four `concepts/` subfolders + the cross-cutting root; the Notes manifest rewritten path-for-path against disk (52 files, down from 53 — the one deletion); snapshot bumped to 2026-07-29.
- **Protocol self-updated for nested kind-subfolders** ([[Self-Update]]-scoped, both edits minimal): `protocol/OPERATIONS/Naming.md` gains a sentence sanctioning a kind folder grouping its pages into domain subfolders once large enough to warrant it; `protocol/SHAPE.md`'s Rules section gains the same sanction. Both cite the bare-name wikilink rule as what keeps nesting link-safe. Two more protocol pages had gone stale describing the old flat shape and are fixed under the same self-update: `protocol/OPERATIONS/Assertion.md`'s scope line dropped `songbook/` from its content-page-kind list (down to `concepts/`, `entities/`, `Overview.md`); `protocol/OPERATIONS/Lint.md`'s first-pass assertion grep dropped the now-nonexistent `songbook/` target (the recursive `concepts/` sweep already covers its subfolders).
- **No content claims changed** — every edit in this pass is link-safety, manifest sync, or the mechanical consequence of the move (frontmatter `type:`, index section folding). No page's substantive assertions were rewritten.

## [2026-07-30] ingest | Widget-Bridge-Contract (the bridge redesign)
- Minted `concepts/desktop/Widget-Bridge-Contract.md`: the normative contract for how the desktop widgets are built as PURE VIEWS of the bridge — grounded in the Phase 0–3 bridge-redesign commits (9d4b176 the `focussession` daemon-resolved jump; 11382bd canonical session state reaching the widgets via `sessions.json`; 391e7bf live cwd/command via the conduct PTY + set-once names + the `flock`'d stage; 2386718 sub-agent detection, the conductor tree, and the wired tool/notification hooks). Documents the `sessions.json` field contract (canonical `state` working/awaiting/idle/done, `kind`, `activity`, `title`/name, `parentSessionId` tree edges), the hook→state machine, the sub-agent tree + musical beaming render, the daemon-resolved jump, the `needsInput ⇔ awaiting` invariant, rebuild-transparency, and the seven rules a widget obeys.
- Wired into `ingest/index.md` (Concepts bullet) and the `SCHEMA.md` Notes manifest (`concepts/desktop/Widget-Bridge-Contract.md`; 52 → 53; snapshot 2026-07-30). Tags unchanged (bridge/ipc/session/widget/quickshell already in use). Cross-linked from [[shellbridge]], [[Quickshell]], [[Widget-Maker]] Related.
- **Status-marked** per [[Assertion]]: the bridge/schema half is committed on main and runs after the next gated `switch` ([[Rebuild-Gate]]); the widgets are being brought into conformance (the pure-view switch, the beamed tree, the `awaiting`-driven dock peek) in the same pass. The page states the contract at HEAD with that marker rather than as already-live.

## [2026-07-30] update | Widget-Bridge-Contract: say, session name, same-window dedup, temple split
- **`say`** added to the `sessions.json` field table — the agent's latest WORDS, tail-read from its Claude Code JSONL transcript (distinct from `activity`, the process); a background Task sub-agent gets its own `say` from its dedicated `subagents/agent-<agent_id>.jsonl`, correlated to its `sub:<tool_use_id>` node via the sibling `.meta.json`'s `toolUseId`. State-machine section now notes the transcript refresh on Stop/PostToolUse/UserPromptSubmit/Notification.
- **Session name** (`title`) clarified as set-once with a source precedence — first user prompt, else Claude's own `custom-title`, else a `graph send` steer.
- **New "One agent per window (dedup)" section**: a compact/resume mints a new `session_id` and the old record orphans `working` (its `pid` is the terminal's, invisible to the liveness reaper), so the bridge collapses same-window AGENT records — at registration (a new agent evicts a live same-window sibling immediately) and in the reaper (keeper = a real on-disk transcript). Shells exempt (a shell + its hosted claude share a window legitimately).
- **New "The two roster temples" section**: Conductor = the AGENT tree named by session `title`, host-shell rows suppressed (one row per terminal), sub-agents beamed with their own `say`; Terminals = the PROCESS view, headline = `activity` (foreground command / edited file / idle shell process), `cwd` as subtext.
- **Status** flipped to live-on-yomi-strix: the follow-up (say + custom-title name + dedup + temple split + the reaper's own `crate::reap` module) is switched in and verified. Reaper extracted from the 5.8k-line `graph.rs` into `pkgs/aoide/src/reap.rs`. No manifest count change (page updated in place).

## [2026-07-30] lint | full wiki — bridge-redesign sweep surfaces a wider gadget-rebuild drift
- **Verified `Widget-Bridge-Contract.md` and its own log entries (above) already match HEAD** — `say`, `title`/`custom-title` precedence, the same-window dedup (registration + `reap.rs`), and the Conductor-is-agent-tree/Terminals-is-process-view split all check out against `pkgs/aoide/src/graph.rs`, `pkgs/aoide/src/reap.rs`, `ConductorGadget.qml`, `TerminalsGadget.qml`. No changes needed there.
- **Checking those facts against the wider wiki surfaced a second, larger drift the bridge-redesign context didn't flag**: a widget rebuild that landed *before* the bridge work (commit 353390a, "Greek pantheon widgets — book-edge dock, 4 temples, glass launcher") had already retired `AoideAgentWidgets.qml`, `AoideSessionGraph.qml`, `GraphRow.qml`, `GraphModel.qml`, `BatonGadget.qml`, `TerminalManagerGadget.qml`, `ClockGadget.qml`, `MeterGadget.qml` from the QML tree, none of which several pages had caught up to. Fixed, grounded by reading the current QML/nix directly rather than the wiki's prior description:
  - The dock is `AoidePanel.qml` (four gadgets: `ConductorGadget.qml`, `TerminalsGadget.qml`, `MetersGadget.qml`, `PowerVitalsGadget.qml`); Calendar/NowPlaying are bar popouts, not dock members.
  - The standalone DAG overlay and the shared `GraphModel.qml` no longer exist; `aoide.surfaces.sessionGraph` is still declared owner-registered in `modules/facets/quickshell/default.nix` but has no QML body backing it (flagged as a residual, not resolved). `aoide graph view`/`--json` and `aoide baton` are the DAG's renderers today; the dock's Conductor gadget gives the desktop an at-a-glance agent-tree view in place of a literal diagram.
  - `SUPER+G` (dock toggle) is confirmed still correct (`modules/dendrites/hyprland.nix`), but is now an in-process Hyprland global shortcut the panel registers itself (`aoide:dock`) — the old `aoide shell dock toggle` CLI-verb framing is obsolete (the launcher/wallpaper picker use the identical pattern). Only `aoide shell lock` (`SUPER+ESCAPE`) remains an invocation of a non-existent `aoide shell` verb group.
  - `pkgs/aoide/src/reap.rs`'s `reap()` command does both the liveness-dead sweep (window/pid gone) AND the same-window agent-duplicate retirement — [[Session-Graph]]'s "Liveness reaping" section only covered the former; noted the latter with a pointer to [[Widget-Bridge-Contract]].
  - **`entities/Agent-Hooking.md` was on the pre-canonical vocabulary** (`running`/`waiting`/`blocked`, no `idle`) despite the canonical `working`/`awaiting`/`idle`/`done` model having landed in the same Phase 0–3 bridge pass this session's context described — verified the exact event→state mapping against `map_hook()` in `graph.rs` (`SessionStart`→`idle`, `Stop`→`idle`, `Notification`→`awaiting`, no separate "blocked") and rewrote both tables plus the wrap/harness examples to match.
- **Fixed** (mechanism-level only; the Greek/kaomoji aesthetic itself is rice design memory and stays out of this wiki per `SCHEMA.md`): `concepts/desktop/Gadget-Dock.md` (rewritten), `concepts/orchestration/Terminal-Commander.md`, `concepts/orchestration/Session-Graph.md`, `entities/Quickshell.md`, `entities/aoide-cli.md`, `entities/Agent-Hooking.md`, `concepts/Full-Architecture.md`, `concepts/Codebase.md`, `Overview.md`, `ingest/index.md`.
- Crosslinked [[Widget-Bridge-Contract]] into [[Gadget-Dock]], [[Session-Graph]], [[Agent-Hooking]] Related sections (it already linked back to all three).
- **Not fixed, deliberately**: `references/AOIDE-DEV-HANDOFF.md`'s own `BatonGadget` mention (§7 open-flags ledger) — that page's dated, attributed history is exempt by design; it is the orchestrator's ledger to close, not this pass's to edit.
- **Manifest/index/wikilink check**: no pages added or removed (53 = 53, both directions); `ingest/index.md` bullets refreshed for the four touched concept/entity pages, still one-to-one; full-wiki wikilink sweep found no new broken links (the only unresolved forms remain the standing OPERATIONS pedagogical examples and immutable dated-log mentions).

### [2026-07-30] fix: Terminals gadget is a single-source view, not a merge
Commit 0dd758d moved terminal-window enumeration out of `TerminalsGadget.qml`
and into the daemon (`graph.rs`'s Hyprland window-event listener,
`sync_untracked_terminal_windows()`): every live, untracked, terminal-class
window now gets a synthetic `kind:"shell"` record (keyed `win:<address>`)
published straight into `sessions.json`, so that file alone is a complete
terminal roster. Two pages still described the old two-source merge
("merging `sessions.json` with Hyprland's own client list") — fixed in
[[Gadget-Dock]]'s Terminals bullet and [[Terminal-Commander]]'s own desktop-
gadget paragraph to state the daemon does the Hyprland work and the widget
is a pure filter/de-dupe-by-window-address view, matching
[[Widget-Bridge-Contract]]. No manifest change (both pages updated in place).
- **New Open Thread** (`## Open Threads` above): whether the `aoide.surfaces.sessionGraph` owner-registry entry should be dropped now that no QML file backs it, or a body re-added — a code-side question, left for the dev/orchestrator side rather than decided here.

## [2026-07-30] ingest | External-Edit-Tracking (planned feature)
- **Minted `concepts/orchestration/External-Edit-Tracking.md`** on khoa's steer (explicitly "make this a planned feature" — documentation only, nothing built): the plan for detecting a human editing files directly inside a conducted shell (opening `nvim` and saving, outside the orchestrator's own `Edit`/`Write` calls), which is invisible to the orchestrator today. Design: `conduct`'s existing PTY tick — already using `EDITOR_BASENAMES`/`friendly_editor_command` (`pkgs/aoide/src/graph.rs`) for the `activity` display — takes a `git status --porcelain` snapshot when the foreground process enters the editor set and another when it leaves, diffs the two to the paths that became newly modified during that specific window (not everything dirty in the repo), and rides a `git diff --stat` alongside; a new stage file `song/stage/edits.json` (same atomic write-temp-then-rename pattern as every other stage file) records one event per detected edit, keyed to the shell's `sessionId` and its `parentSessionId` (the [[Session-Graph]] edge that says who to report back to); a v1 pull-model CLI (`aoide graph edits [--session] [--since] [--json]` + `graph edits ack --id`) is the concrete report-back mechanism. Scoped to git-repo cwds only for v1 — no mtime-scanning fallback, no attempt to generalize beyond git.
- **Open thread carried, not resolved**: how the orchestrator learns about a new edit event WITHOUT polling — Claude Code's hook surface only fires session→bridge, so nothing pushes bridge→session mid-turn today. Two candidates are on record (PTY injection via `graph send`, flagged for its human-mid-keystroke collision risk; a visual badge on the Conductor/Terminals gadget rows, which depends on a human noticing it) and neither is chosen — see the new Open Threads entry below.
- Registered per [[Assertion]]'s "documenting the unbuilt" pattern: `Status: specified; not implemented` up top, present-indicative prose about the design throughout.
- Wired into `SCHEMA.md` (Notes manifest, 53 → 54; snapshot unchanged at 2026-07-30; Tags gained `editor`, `git`) and `ingest/index.md` (new Concepts bullet).
- Cross-linked (both directions, one line each): [[Conductor-Channel]], [[Session-Graph]], [[Agent-Hooking]], [[Widget-Bridge-Contract]].
- Nothing built: no changes to `pkgs/aoide/src`, no QML, no new stage file on disk.

## [2026-07-30] remove | External-Edit-Tracking page retired — moved to the dev handoff ledger
- **Deleted `concepts/orchestration/External-Edit-Tracking.md`** on khoa's steer: the feature is still unbuilt (nothing in `pkgs/aoide/src` implements it), so per the wiki's own present-indicative rule it does not belong here as a page. The full design (PTY-tick before/after `git status` snapshots, the `song/stage/edits.json` stage file, the `aoide graph edits`/`edits ack` pull-model CLI, and the open report-back question) now lives as a single ledger entry in `references/AOIDE-DEV-HANDOFF.md` §7 "Open flags" — content unchanged, just relocated off the wiki.
- Deregistered from `SCHEMA.md` (Notes manifest, 54 → 53; snapshot unchanged at 2026-07-30) and `ingest/index.md` (Concepts bullet removed).
- Removed the four `[[External-Edit-Tracking]]` `## Related` bullets that pointed at it: [[Conductor-Channel]], [[Session-Graph]], [[Agent-Hooking]], [[Widget-Bridge-Contract]] — each page's remaining Related list is otherwise untouched, no dangling wikilinks left.
- The matching Open Threads entry above (report-back mechanism undecided) is marked closed-as-moot rather than deleted, so the log stays an accurate history of the thread's life.
- `references/AOIDE-DEV-HANDOFF.md` itself was edited directly by the orchestrator, not by this pass — out of scope here.

## [2026-07-30] update | three landed commits (basename-clean activity, wsN tag, nvim rustc/cargo) + a real command-count drift (36→37, `cover set`)
- **Commit 822991d** (`generic_command_label`, `pkgs/aoide/src/graph.rs`): the editor-only basename collapse (`friendly_editor_command`) now has a general sibling covering every other command, so a NixOS-wrapped `argv[0]` (confirmed live: `yazi`'s full `/nix/store/…/bin/yazi`) displays as a clean basename instead of the store path. Documented as a new "Activity labels are basename-clean" paragraph in `concepts/desktop/Widget-Bridge-Contract.md`, naming both helpers and the argv[0]-only scope (arguments untouched).
- **Commit 7982971** (`ConductorGadget.qml` + `TerminalsGadget.qml`): each roster row now renders a plain arabic-numeral `wsN` tag next to its state label, reading the already-published `sessions.json` `workspace` field (no schema change). Added one clause to each of `concepts/desktop/Gadget-Dock.md`'s Conductor/Terminals bullets — the existing `workspace`/hover-preview mechanism in `concepts/orchestration/Terminal-Commander.md` was already accurate and needed no change (the tag is an additional direct render of the same field, not a new mechanism).
- **Commit 8ce74dd** (`modules/dendrites/neovim.nix`): nvf's `vim.extraPackages` now carries `pkgs.rustc`/`pkgs.cargo`, scoped to nvim's own wrapped PATH, so its built-in rust-analyzer `root_dir` detection stops failing on a missing `rustc` — the rest of the system stays devShell-only per the existing convention. One sentence added to `concepts/Codebase.md`'s dendrite-set paragraph; `concepts/Full-Architecture.md` has no neovim-specific prose (only the dendrite-name list in its ASCII tree), so left alone.
- **Real drift found and fixed**: `aoide schema --json` reports **37** commands, not 36 — `cover set` (`gated: false`, real, stages `stage/cover.json` for a wallpaper hot-swap) was missing from `entities/aoide-cli.md`'s command table entirely. Fixed the leaf total and added its row (grouped with the other real `rice lint`/`rice preview`-adjacent commands, matching `dispatch.rs`'s own ordering), plus a short paragraph noting it's the CLI-verb slice of the wallpaper-switcher open item (`references/AOIDE-DEV-HANDOFF.md` §7) — the quickshell picker and `list`/`next` verbs remain unbuilt. Verified every other row against `aoide schema --json | jq` directly (gated count = 3, `graph` group = 15 leaves) — no other row was stale.
- **The same stale count propagated wider than `aoide-cli.md`** — fixed each: `Overview.md` and `ingest/index.md`'s `[[aoide-cli]]` bullets (28-command tree → 37, add `cover set`), `concepts/Full-Architecture.md`'s master-map diagram (36 → 37 commands) and subsystem-I/O table (25 → 26 real verbs; stub count of 11 unchanged since `cover set` is real), `concepts/desktop/Feature-Set.md`'s two "36-command surface" mentions (→ 37).
- **Left alone, on purpose**: `concepts/Codebase.md`'s vm-boot-check paragraph quotes `lib/vmTest.nix`'s own hardcoded Python assertion literally (`expected 36 commands`) — fixed only its stale "28" to match that file's actual current text (36). That test file itself was last touched 2026-07-28 (before `cover set` landed) and will now assert 36 against a real 37-command schema — a live code/test mismatch, not a wiki drift; flagged in this pass's report rather than resolved here (code fix is dev-process work, out of the librarian's remit).
- **Also fixed a second, unrelated stale reference surfaced by the same live-source check**: `entities/Quickshell.md` said `stage/cover.json` was "written only by `aoide rice preview`" — no longer true now that `cover set` is real and writes the same file (confirmed both call sites in `dispatch.rs`); reworded to name both writers.
- **Frontmatter**: `updated: 2026-07-30` added to `concepts/desktop/Widget-Bridge-Contract.md` (didn't carry one before) and bumped on `concepts/desktop/Feature-Set.md` (was 2026-07-28); the other touched pages (`Gadget-Dock.md`, `Codebase.md`, `Full-Architecture.md`, `Quickshell.md`, `entities/aoide-cli.md`) already carried today's date. `Overview.md` and `ingest/index.md` take no `updated:` field per [[Frontmatter]].
- **Manifest/wikilink check**: no pages added or removed (53 = 53); no new or broken wikilinks introduced.

## [2026-07-30] rename | `baton` → `conductor` — module, command, page, and gadget-tag cleanup
- **Rust**: `pkgs/aoide/src/baton.rs` + `src/baton/{app,ui,theme,graphview}.rs` → `src/conductor.rs` + `src/conductor/` (`git mv`, history preserved); `pkgs/aoide/tests/baton_integration.rs` → `tests/conductor_integration.rs`. Every `crate::baton::` path reference, doc comment, and user-facing string (the ratatui brand line, the `?` help-overlay title, the launch error message) updated to `conductor`. All 113 lib tests + the renamed integration test pass (`cargo test -p aoide`); one pre-existing, unrelated PTY test (`conduct_injects_socket_bytes_into_the_child`, testing the separate `conduct` verb) hangs in this sandbox on `forkpty` and was excluded from the run — confirmed unrelated to this change.
- **CLI command**: `aoide baton` → `aoide conductor` (`lib.rs`, `dispatch.rs`, `schema.rs`). The `schema.rs` summary now carries an explicit disambiguation sentence — `aoide conductor` (noun, the TUI) vs `aoide conduct` (verb, wraps one process into the conductor channel) — surfaced identically through `schema --json`, the MCP tool list, and the README command table.
- **Page rename**: `concepts/orchestration/Baton-3D-DAG.md` → `Conductor-3D-DAG.md` (`git mv`; a different, still-planned 3D-wireframe-DAG feature, NOT merged with [[Conductor-Channel]]). Inbound `[[Baton-3D-DAG]]` wikilinks fixed in [[Conductor-Channel]], `ingest/index.md`, `SCHEMA.md`.
- **Prose sweep**: every current-state `aoide baton` / "the baton" mention across `README.md`, `AGENTS.md`, `pkgs/aoide/src/guide.rs`, [[Lexicon]] (entry moved to the alphabetical `c`-group as **conductor**), [[aoide-cli]] (the `conduct`/`conductor` section reworked to state the noun/verb contrast directly), [[Codebase]], [[Terminal-Commander]], [[Session-Graph]], `Overview.md`, [[Full-Architecture]], [[shellbridge]], [[Gadget-Dock]], [[Agent-Hooking]], [[Quickshell]], `references/AOIDE-DEV-HANDOFF.md`, `modules/nucleus/shellbridge.nix`, `lib/vmTest.nix` reworded to `conductor`.
- **Stale `BatonGadget`/`[ baton ]`/`baton.control` cleanup (khoa's explicit yes, plan §2f)**: comment-only `BatonGadget` mentions renamed to `ConductorGadget` (the live component's actual name) in `AoideLauncher.qml`, `AoideWallpaperPicker.qml`, `GadgetFrame.qml`, [[Quickshell]], `references/AOIDE-DEV-HANDOFF.md`, and `song/songbook/default/design/pantheon.md`; the rendered gadget tag `[ baton ]` → `[ conductor ]` in `ConductorGadget.qml`, cross-checked against `song/songbook/sonata/design/greek-grammar.md`'s description of the same tag; the coordinated `baton.control` gadget-title key → `conductor.control` everywhere it appears (`GadgetFrame.qml`'s glyph map + comment, `greek-grammar.md`, `pantheon.md`). **One deliberate exception, left as `BatonGadget`**: `greek-grammar.md`'s "AS BUILT" widget-by-widget list of components genuinely DELETED in the pantheon rebuild (`AoideAgentWidgets`, `BatonGadget`, `TerminalManagerGadget`, `DagGraphGadget`, …) — that `BatonGadget.qml` was a real, separate, no-longer-existing file, not a stale name for the still-live `ConductorGadget.qml`; renaming it there would falsely claim the current gadget was deleted, so it stays as an accurate historical record (same treatment as this log's own dated entries below).
- **Left untouched, on purpose**: this log's own historical dated entries above (their `[[Baton-3D-DAG]]` links now dangle to the renamed page — accepted ledger drift, not fixed); `.obsidian/workspace.json`; `song/stage/{graph,sessions}.json` (transient runtime data); the untracked root `qml/` duplicate.
- **Verification**: `grep -rin baton pkgs/ --include=*.rs` → 0 hits. `grep -rin baton docs/ song/songbook/ modules/ lib/ AGENTS.md README.md` → 0 hits outside this log (incl. the exception noted above, and this entry itself, which necessarily mentions "baton" while describing the rename). `grep -rn 'Baton-3D-DAG' docs/` → only this log's history and `.obsidian/workspace.json`. `grep -rn 'crate::baton\|aoide::baton\|baton/theme.rs\|src/baton'` (excl. untracked `qml/`, `song/stage/`) → 0 hits.

## [2026-07-31] update | Grimoire chrome rebuilt as a rectangular open book (three same-day iterations)
- **`AoideLauncher.qml` chrome rebuilt from scratch, three times in one sitting on khoa's live steers**: the deliberate −6° resting tilt read as a bug ("the grimoire is tilted?") → ground-up rebuild ordered (parameters: Pantheon, glass+glow, book-with-search-bar); rebuild #1 kept the old slab/meander/cartouche bones and was rejected ("exactly like the old one"); rebuild #2 (curved silhouette + `holoBlue` depth stack + leader-line callouts) was cut back ("remove the pantheon holo-blue framing stuff — make it a rectangular book"). Functional core (shortcut wiring, `GrimoireLedger`, chapters, search ranking, leaf-turn, honest sparse states) byte-identical throughout.
- **The standing chrome**: two rectangular Aero-glass pages with gold margin keylines meeting at a shaded gutter crease; ink fore-edge stack + glass cover-boards band ("GRIMOIRE · βίβλος" spine inscription, dangling accent ribbon) below; a floating glass incantation strip above the book as the search bar; ruled manuscript-line rows (selected line's rule ignites `paletteHot` — the one neon element); book-native navigation (thumb-index tabs ♪/α β γ…, dog-eared corners, running headers, folios); summon = the book swings open from its spine line, rest pose dead flat, zero rotation.
- **Deployment gotcha recorded**: the live shell reads the untracked `~/Aoide/qml/` working copy, not `modules/facets/quickshell/qml/` — the first rebuild "still looked old" because only the repo module had changed. Synced + `aoide-quickshell.service` restart thereafter; verified live (clean restart, screenshot vision-check, light palette; dark-palette pass still owed).
- Ledger record updated in `references/AOIDE-DEV-HANDOFF.md` (`[design · reworked 2026-07-31]` entry). No content page asserts the old chrome, so no other page edits; nothing added or removed from `SCHEMA.md`'s manifest.

## [2026-07-31] ingest | the rice engine gains per-song widget slots, a geometry tier, `rice mint`, and a live-apply preview

- **Widget slots are keyed by filename, not a fixed enum.** The quickshell facet's build copies any `widgets/*.qml` file a committed song carries into `$out/qml/songs/<name>/<slot>.qml`, keyed by filename, alongside a generated `manifest.json` (`{song: [slots]}`) the staging engine reads to answer `has(song, slot)`/`source(song, slot)`. Being carried by the build is not being rendered: a slot shows on screen only once a host surface embeds a `WidgetSlot` anchor naming that exact slot — `calendar` (`AoideBar`'s popout) and `notifications` (`AoideNotifications`'s card repeater) are the two wired anchors today. A new catalog page, `modules/facets/quickshell/qml/slots.md`, records which slot names have a wired anchor (and what extras each anchor passes) plus the shape every widget file follows: an `Item` root, `required property var notes`/`bridge` injected by every anchor unconditionally, slot-specific extras as their own `required property`, `implicit*` sizing, and never `config.*`. Documented in [[Widget-Maker]].
- **A geometry tier joins the palette and component tiers.** `aoide.livery.geometry` (`modules/nucleus/options.nix`) carries `gapsOut`, `gapsIn`, `borderSize`, `rounding`, `blurEnabled`, `blurSize`, `blurPasses` — every field `nullOr`. The compositor facet reads the tier directly and falls back field-by-field to its existing opinionated defaults (`8`/`6`/`2`/`0`/`true`/`8`/`3`) for anything unset, so a song that sets no geometry produces the same `hyprland.conf` as one that omits the block. The tier sits outside the standalone Node note package's own schema — `livery lint` never validates it; it rides `stage/livery.json` unvalidated, at the same additive-optional `schemaVersion "0"` posture as the base16 block. Documented in [[livery]] and [[Self-Ricing]].
- **`rice preview` now applies geometry and border colours to the running compositor.** A new `pkgs/aoide/src/hypr.rs` builds one `hyprctl --batch` `keyword` list, in a fixed order (gaps → border size → border colours → rounding → blur), emitting a keyword only for a field that actually resolves — an unset geometry field is skipped, not defaulted, so the call never fights a host's baked config or a user's own out-of-band tweak. The call is a no-op off Hyprland (guarded on `HYPRLAND_INSTANCE_SIGNATURE`) and never fails the preview outcome; it never runs `hyprctl reload`, since every field it touches is live-settable via `keyword` and a reload would re-read the baked config from disk, discarding anything else live on the compositor. This sharpens the rice loop's truth/sketch split: `aoide.song` selecting a song and rebuilding is the truth (bakes `hyprland.conf`, themes every nix-manageable app via [[Stylix]], deploys the song's widgets, sets the boot default); `rice preview` is the sketch — a live compositor call plus a `stage/livery.json` write, no rebuild.
- **`aoide rice mint <name>` scaffolds a new committed song directly.** (`--from <song>` · `--force` · `--json`; alias `rice new`; `gated: false`.) It writes `song/songbook/<name>/rice.nix` (a self-gating `lib.mkIf (config.aoide.song == "<name>")` block copying the `palette`/`window`/`geometry` tiers from `--from`, defaulting to `default` — the only `.nix` file the scaffold writes, so the folder satisfies the `song-shape` check unaided), a `livery.json` mirror of the same values, an honest-empty `design/intent.md` that points at the widget-slot catalog and the update playbook rather than fabricating design rationale, and `widgets/.gitkeep`. `name` and `--from` are validated against a strict `^[a-z0-9][a-z0-9-]*$` pattern (rejecting path traversal); rendering a copied livery value into `rice.nix` escapes `$` and quotes attribute keys, so a value containing `${…}` cannot round-trip into live Nix interpolation. Being an ordinary schema command puts it in `schema --json` and the MCP tool list, so an agent bootstraps a song through the same door a human would. The command tree grows to **38 leaves**. Documented in [[aoide-cli]] and [[Self-Ricing]]; command counts refreshed across `Overview.md`, `ingest/index.md`, [[Full-Architecture]] (master map + I/O table), and [[Feature-Set]] (two "N-command surface" mentions). `lib/vmTest.nix`'s own hardcoded Python assertion still reads 36 (unchanged since before `cover set` landed, per the 2026-07-30 entry above) — now three behind the real 38; left as-is, same live code/test mismatch, not a wiki drift.
- **The colour invariant holds, audited.** Every rice's colours derive from [[Stylix]] plus the livery base16 palette: the stylix facet bakes one base16 scheme from `aoide.livery.base16` (or synthesizes it from the four palette anchors when a song sets no explicit base16 block), and the live Quickshell notes derive from the same `aoide.livery` value, so the baked and live halves cannot disagree. The known departures from "everything reads `notes.*`" are verified deliberate, not drift: the `*Preview.qml` files are standalone screenshot scaffolds that hardcode palette values to simulate `notes.*` off the live tree; `AoideBar.qml`'s `#000000` is structural staff-ink, not a themed surface. A small number of `#14141a` literals remain unconverted to a note role — noted, not touched this pass.
- **A song's substance is already plain files.** A rice's `livery.json`, `widgets/*.qml`, and `design/` folder are ordinary files read by `aoide`, Quickshell, and `hyprctl` — none of them require Nix to exist or apply. Nix's remaining jobs on a rice are generating `hyprland.conf`, theming every non-Quickshell app via Stylix, deploying the QML tree, and choosing the boot default via `aoide.song`.
- Four open questions from this pass are carried as Open Threads above rather than written into any page: per-slot host presence (no toggle exists — presence is anchor existence), the proposed `palette-usage.md`, boot-time seeding of `stage/livery.json` from the selected song, and a `commands/` registry + `graph.rs` domain split sketched for `pkgs/aoide`.
- Grounded by reading the repo directly (`modules/facets/quickshell/default.nix`, `modules/facets/quickshell/qml/slots.md`, `modules/nucleus/options.nix`, `modules/facets/compositor/default.nix`, `pkgs/aoide/src/hypr.rs`, `pkgs/aoide/src/dispatch.rs`, the standalone Node note package's `src/schema.js`), not a commit hash — this work sits in the working tree.
- Pages updated: [[Widget-Maker]], [[livery]], [[Self-Ricing]], [[aoide-cli]], `Overview.md`, [[Full-Architecture]], [[Feature-Set]], `ingest/index.md`. No pages added or removed; `SCHEMA.md`'s manifest is unchanged.

## [2026-07-31] update | the Conductor's model tag reaches sub-agents; the transcript lookup gains a second path

- **A sub-agent row's model tag was gated on carrying a title.** `ConductorGadget.qml` only showed the small model tag for a row with `hasTitle`; a `Task` sub-agent rarely carries one (its display name already falls back to the agent type), so the tag never appeared even once its model was known. The visibility rule now also shows the tag for a sub-agent row once `model` is non-empty.
- **The underlying data gap: `find_subagent_transcript` only knew one keying regime.** A sub-agent node starts keyed `sub:<tool_use_id>`; an async `Agent`-tool node re-keys to `sub:<agent_id>` once the agent starts, and its transcript file is named `agent-<agent_id>.jsonl` directly. The lookup only scanned `subagents/*.meta.json` for a matching `toolUseId` — a search that never matches the re-keyed form — so `say`/`model` stayed null for every async `Agent`-tool sub-agent. It now tries the direct `agent-<id>.jsonl` path first (cheap, unambiguous) and falls back to the `.meta.json` scan for a node not yet re-keyed.
- Documented in [[Widget-Bridge-Contract]]: `model` added to the `sessions.json` field table, the transcript-refresh paragraph now names both lookup paths, and the Conductor-temple paragraph states the row-visibility rule.
- `pkgs/aoide/tests/fixtures/seed.sh`'s comments still said `aoide baton` — missed by the 2026-07-30 baton→conductor rename sweep, which verified only `*.rs` files. Corrected; test-fixture-only, no other pages reference it.

## [2026-07-31] rename | `AOIDE-DEV-HANDOFF.md` → `AOIDE-DEV.md` reconciled in wiki bookkeeping
`references/AOIDE-DEV-HANDOFF.md` was renamed to `references/AOIDE-DEV.md` outside this pass (the orchestrator's own ledger, out of scope to edit — see the standing instruction not to touch `AOIDE-DEV*.md`). `ingest/index.md` already pointed at the new name; `SCHEMA.md` (the "read the whole wiki" prose, the shape diagram, and the Notes manifest) and three backtick-quoted path mentions ([[aoide-cli]], [[Melete]], `protocol/OPERATIONS/Assertion.md`) still named the old file. Repointed all five to `AOIDE-DEV.md` — bookkeeping only, no content in the referenced file touched. No pages added or removed.

## [2026-07-31] update | Grimoire chrome — spine ribbon removed, entry text inset, a treble clef added

Three further touches to `AoideLauncher.qml`'s chrome, on top of the rectangular-book rebuild recorded above: the gold `stripRibbon` artifact at the spine-top is removed; the entry text is inset so the ruled manuscript lines cross under the inner blue illuminated border rather than stopping short of it; an angled gold treble clef sits at the book's top-left. No functional change; no content page asserts launcher chrome detail, so no page edits.

## [2026-08-01] ingest | self-registering commands registry, the graph/ domain split, boot-time stage seeding, and the run/qml/ deploy fix

Four related landings against `pkgs/aoide` and the quickshell facet, closing three Open Threads at once (a fourth, the `sessionGraph` owner-registry entry with no QML body, stays open — unrelated to this pass).

- **The command layer is a self-registering `commands/` registry.** `pkgs/aoide/src/registry.rs` holds the `Command`/`Registry` types plus `cmd!`/`arg!`/`flag!` macros; each command group under `pkgs/aoide/src/commands/` (`meta`, `rice`, `cover`, `stubs`, `graph`, `infra`) owns a `register(&mut Registry)` function, and `commands/mod.rs::all()` assembles them in the historical `schema --json` order. Rice/cover handler bodies live in their own command module; `commands/graph.rs` is thin registrations pointing at `crate::graph::*`. `dispatch()` (`pkgs/aoide/src/dispatch.rs`) shrinks to a `registry.get(path)` lookup plus the unchanged audit/gate tail (~130 lines); `schema --json`, the MCP tool list, and the CLI path table all derive from the one registry. The old hand-maintained `schema.rs` command table is deleted; a `command_paths_match_the_golden_snapshot` unit test in `registry.rs` replaces the prior magic command-count assertion. Explicit aggregation (`commands/mod.rs::all()`'s function calls), no new crate dependency — modeled on Hermes-agent's self-registering tool registry and Claude Code's discrete-tools-behind-a-thin-dispatch shape (both named in the prior Open Thread). Command surface unchanged: still 38 leaves.
- **`graph.rs` is a `graph/` domain module.** `pkgs/aoide/src/graph.rs` is now a thin re-export root over `graph/{model,doc,common,verbs,window,session_store,conduct,send}.rs` (+ a shared `testutil`), mirroring the existing `conductor.rs` + `conductor/` split. The public `crate::graph::*` surface `dispatch`/`reap`/`conductor`/`shellbridge` reach is unchanged.
- **`stage/livery.json` is seeded from the active song on activation.** `modules/facets/quickshell/default.nix`'s `home.activation.aoideSeedStage` writes the active song's committed `song/songbook/<song>/livery.json` (with the `"song"` field injected, matching what `rice preview` stages) into `song/stage/livery.json` on every activation, write-temp-then-rename. A host that boots without ever previewing now carries a correct live stage twin from the baked default. Documented in [[Codebase]] (Runtime contracts), [[livery]], and [[Self-Ricing]]; already reflected in `CONTRACTS.md` §4.
- **The QML deploy moves out of the repo root.** The prior `home.file` symlink tree at `~/Aoide/qml` is gone; `home.activation.aoideDeployQml` rsyncs (`-a --delete`) the built config tree into the gitignored `~/Aoide/run/qml/`, and `aoide-quickshell.service` reads `run/qml/shell.qml`. Widget source stays at `modules/facets/quickshell/qml/`; the repo root carries no `qml/` directory (the stale untracked copy is removed). Documented in [[Quickshell]] and [[Full-Architecture]].

Grounded by reading the repo directly at commits `62121a4` (commands registry), `16b65d6` (graph/ split), `99a8447` (stage seed), and `252c3d4` (run/qml deploy) — no `source:` field added (source is the repo, not a `references/` doc).

Pages updated: [[aoide-cli]] (command-tree section rewritten around the registry), [[Codebase]] (graph-domain test paragraph, stage-file seeding note), [[Self-Ricing]] (status paragraph), [[livery]] (stage seeding note), [[Quickshell]] (deploy mechanism rewritten, two mentions), [[Full-Architecture]] (deploy mention), `ingest/index.md` (aoide-cli, Codebase, Quickshell glosses). No pages added or removed; `SCHEMA.md` manifest unchanged (same file set, `updated:` bumped on the six touched pages).

Closed above in `## Open Threads`: "a `commands/` registry + a `graph.rs` domain split are sketched for `pkgs/aoide`, not started" and "`stage/livery.json` is not seeded from the selected song on boot" (both dated 2026-07-31) — both struck through and marked closed. The QML-deploy-target thread (2026-07-26, "QML deploy target vs source") and its 2026-07-31 diagnosis follow-up ("the QML deploy target still symlinks into the repo root") are likewise closed. Left open: the host-side per-slot `enable` toggle, `palette-usage.md`, and the `sessionGraph` owner-registry-with-no-body thread — none of the four landings here touch any of them.

## [2026-08-01] ingest | Package-Layout (target blueprint, not yet built)

Mirrored `docs/architecture/PACKAGE-LAYOUT.md` — a blueprint (not yet built) for splitting `pkgs/aoide` into pi-style single-charter crates — into the wiki as `concepts/Package-Layout.md`. The source is explicit that `pkgs/aoide` is one crate today; the page states that plainly and frames every crate/tree/table as the target, not present structure, per [[Assertion]]'s "documenting the unbuilt" clause (status labels: carve-out / skeleton / seed→build / elevate, matching the source's own per-crate status column).

- Minted `concepts/Package-Layout.md`: the pi→aoide crate mapping, the target `crates/` tree, the per-crate charter table, a detail section on the steward (`steward` crate — system-management agent, conductor-driven, `canon` as memory of design primitives, `audit` self-checking against canon + CONTRACTS.md), the phased migration (0–8), and the open-questions close.
- **Naming decided mid-pass (hybrid scheme):** the source doc's agent crate is `agent` → `steward` (the only rename; every other crate keeps its blueprint name — plain for universal concerns, aoide vocabulary kept for aoide-unique ones). Applied throughout `concepts/Package-Layout.md` (mapping table, target tree, charter table, the steward-detail heading, phase 7) and every place the crate was named elsewhere: `concepts/Codebase.md`, `Overview.md`, `ingest/index.md`. The page's "Open questions" section now states naming as **DECIDED**, with the rejected alternatives (fully-literal `agent`, the stagecraft scheme, the deep-pantheon scheme — the latter colliding with the live [[Mneme]]/[[Melete]] MCP servers) and the two questions still open (`storage` backend, `audit`'s home).
- Wired in: `Overview.md` (Concepts list), `ingest/index.md`, `SCHEMA.md` (manifest entry; Tags gained `blueprint` and `crate`). Linked from [[Full-Architecture]] (tree-map `pkgs/` line + Related) and [[Codebase]] (a new paragraph after the crate/test discussion, stating the one-crate-today fact + a pointer, plus Related) — both are where the built repo's architecture is otherwise discussed. Backlinked from [[Agent-Interface]] ("one schema, N doors" section gains a sentence on how the `protocol` crate would make the invariant structural, plus Related); `updated:` bumped on Agent-Interface.
- No source contradicted wiki content — the blueprint is additive; nothing existing claimed `pkgs/aoide` was already split. No pages removed; `SCHEMA.md` manifest: 71 → 72 files.

## [2026-08-03] update | agent-profile seam + kimi CLI integration (working tree, uncommitted)

The dev workstream landed the multi-harness seam in `pkgs/aoide` (uncommitted working tree — described here as the current state of the code on disk; the live system generation still runs the pre-seam binary until the next gated switch). Grounded by reading `pkgs/aoide/crates/protocol/src/agents.rs`, `pkgs/aoide/src/commands/hooks.rs`, `pkgs/aoide/crates/conduct/src/graph/{send,session_store,window}.rs`, `pkgs/aoide/crates/conduct/src/reap.rs`, and `pkgs/aoide/crates/storage/src/session.rs` directly.

- **The `protocol::agents` `AgentProfile` seam** — every harness-specific fact (hook event map, permission vocab, sub-agent tools, model ceilings, payload normalizer, transcript locate/tail/extractors, hook-settings path+format) behind one lookup table; today `claude` + `kimi`, all former claude-hardcoding in the hook door / transcript refresh / window listener / reaper / session default rewired to dispatch through it.
- **The kimi door** — `aoide graph session hook --agent <name>` (default claude; unknown → structured `unknown-agent`); `PermissionRequest` is kimi's needs-input signal, kimi-only events (`PermissionResult`, `Interrupt`, the compact/failure events) are ok no-ops; payload normalization maps `prompt` block arrays / `tool_call_id` / `agent_name` onto the canonical fields; transcript reads `~/.kimi-code/sessions/wd_*/<id>/{state.json,agents/*/wire.jsonl}` honoring `KIMI_CODE_HOME`; 0.31.1 gaps recorded (no `SubagentStop`, no `Stop` on Esc, no sub-node transcript probe).
- **`aoide hooks install <agent> [--capture]`** (48 commands total now) — idempotent settings merge: TOML `[[hooks]]` text-append into `${KIMI_CODE_HOME:-~/.kimi-code}/config.toml` (never a parse-rewrite), JSON merge into `~/.claude/settings.json`; `--capture` tees raw payloads to `~/Aoide/state/<agent>-hooks.jsonl` as a distinct idempotency key.
- **The conductable-host eviction fix** — `is_agent_kind` returns false for `conductable` records (a conducted PTY is a HOST, never an agent duplicate) and registration-time same-window eviction spares the new record's `parentSessionId`; wrapper-of-agent (`aoide conduct -- kimi`) records are no longer evicted/retired.
- **Operator facts** — kimi's TUI submits on `\r` not `\n` (`graph send --submit` types but does not submit a kimi target); kimi model aliases are provider-prefixed (`-m kimi-code/kimi-for-coding`).

Pages updated: [[Agent-Hooking]] (profile-seam section + Kimi Code recipe + generalized hook-door framing), [[Agent-Interface]] (hooked-agents section replaces the claude-only "Primary agent" one; `hooks install` + `--agent`), [[Terminal-Commander]] (hook-source bullet generalized), [[Session-Graph]] (dedup paragraph gains the conductable-host exemption + profile-dispatched transcript probe; `graph send` bullet with the `\r` fact), [[Conductor-Channel]] (`--submit` kimi caveat), [[Widget-Bridge-Contract]] (`say`/`model`/`awaiting`/transcript/sub-agent/dedup passages generalized to per-profile), [[aoide-cli]] (48-leaf tree, `hooks install` group, `--agent` on `session hook`), `ingest/index.md` (aoide-cli + Agent-Hooking glosses). Also `docs/architecture/aoide-report.html`: a `pending` changelog entry for the workstream + the stale 47→48 command count in three places. No pages added or removed; `SCHEMA.md` manifest unchanged; `protocol/AOIDE-DEV.md` untouched per the standing instruction (the orchestrator's §7 ledger already carries this work).

## [2026-08-12] fix | reap/prune subagent-cascade unified — orphaned Task-node ghosts closed (working tree, uncommitted)

Two divergent cleanup paths existed for a session ending: a clean `graph session end` (`do_session_end_inner`, `session_store.rs`) correctly cascade-removed `kind:"subagent"` descendants of the ended session, but `prune_done` (`doc.rs` — shared by `graph prune`, `graph reap`'s liveness sweep, and the same-window agent-eviction path) only cleared a surviving child's dangling `parentSessionId` instead of cascading its removal. A `Task`/`Agent`-tool sub-agent node carries no `pid` and no `windowAddress`, so its owning session's end is its *only* possible cleanup path — `is_session_dead` can structurally never fire for one. A top-level session that died **abnormally** (killed terminal, crash — caught by the reap sweep rather than exiting through a clean session-end) left its sub-agent children permanently un-reapable: state stuck `working` forever, invisible to every liveness signal. Confirmed live: two such orphans sat in `song/stage/sessions.json` for 11-12 days, rendering under the Conductor gadget's synthetic `(unanchored)` group (empty `cwd`) — cleared by hand (`aoide graph session end --id <id>` ×2 + `aoide graph prune`, current deployed binary) as a separate remediation from the code fix below.

- **The fix** (`pkgs/aoide/crates/conduct/src/graph/doc.rs`, `session_store.rs`): extracted the walk `do_session_end_inner` already did into a shared `doomed_subagent_descendants(sessions, roots)` helper — fixed-point loop over `parent_session_id`, handles subagent-of-subagent nesting in one call. `prune_done` now cascades this into its removal set before filtering, so all three of its callers inherit correct cascade behavior in one fix; `do_session_end_inner` calls the same helper instead of its own previously-divergent inline copy — one source of truth instead of two. Two regression tests added (`prune_cascades_subagent_descendants_of_a_done_session` in `doc.rs`, `reap_cascades_an_orphaned_subagent_when_its_top_level_parent_is_reaped` in `session_store.rs`); full `graph::` suite (77 tests) passing; reviewed clean (independent pass, no defects, correctness verified line-by-line).
- **Checked against the open ledger flag "Sessions untrack after a rebuild" (`protocol/AOIDE-DEV.md` §7)** — not the same root cause. That flag is top-level sessions erroneously disappearing (over-pruning, hypothesized causes: reaper sweeping during a restart's hook-silence window, or lost hook events staling `sessions.json`). This bug is the opposite direction: sub-agent children under-cleaned, surviving forever instead of being dropped. Left the ledger entry untouched; flagging the distinction here rather than editing it.
- Grounded by reading `pkgs/aoide/crates/conduct/src/graph/doc.rs`, `session_store.rs`, and `reap.rs` directly, plus the working-tree diff. Described here as the current state of the code on disk (uncommitted working tree) — the live systemd reaper runs this fixed logic already (remediation used the current deployed binary); the change is not yet in git history.

Pages updated: [[Session-Graph]] — the `graph prune` bullet gains the cascade clause; the "Liveness reaping" section's dead-sessions paragraph drops the now-inaccurate "orphaned `parentSessionId` links cleared" language and gains a new "Sub-agent cascade" paragraph describing the unified, always-cascading current behavior across all three `prune_done` callers. No pages added or removed; `SCHEMA.md` manifest unchanged.

## [2026-08-13] refactor | livery merge — the standalone note engine ported native (working tree, uncommitted)

The dev workstream landed the livery merge (LIVERY-MERGE.md, Phases 1–3): the standalone Node note package (wrapping Style Dictionary) is ported natively into `crates/song/src/livery/` (schema · resolve · four emitters behind one `Emitter` trait), and the whole token-layer surface carries the one name **livery** — the `aoide.livery` option (a `mkRenamedOptionModule` transition alias keeps the pre-merge option path evaluating through the window), the songbook `livery.json`, and the live stage file `stage/livery.json` (canonical; mirrored under the pre-merge name during the compat window, the mirror dropped when the transition closes). The standalone Node package is deleted; the Node toolchain leaves the core; `aoide livery emit|resolve|lint` carries the old binary's surface (51 command leaves in `schema --json`). Schema shape unchanged — still v0, no version bump; the update playbook records the migration.

- **The entity page is `entities/livery.md`** (a `git mv` from its pre-merge path, history preserved), written as the livery entity — no continuity alias is kept; naming is normalized tree-wide, so nothing links by an earlier spelling. The naming rationale lives in [[Lexicon]] ("Why the seam is a livery").
- **Swept all 33 wiki pages** for the merge: stage-file names, `aoide.livery`, `[[livery]]` wikilinks, native-engine descriptions (`rice lint` no longer delegates/shells out), the standalone note package's overlay attr removed, its flake check dropped, `nodejs` devShell drop. The `LiveryState` QML singleton's file rename is deferred per the merge plan §7 — only the stage path it watches changed in this phase.
- **Command counts refreshed to the in-tree golden snapshot (51 leaves; 40 real / 11 stubs)** across [[aoide-cli]], [[Full-Architecture]], `Overview.md` — the "48/44/38" figures were already behind the working tree.
- **`ingest/log.md` dated entries are naming-normalized** (khoa's 2026-08-13 ruling: the pre-merge engine name exists nowhere in the tree) — every entry keeps its facts (dates, commits, what landed); only the token layer's one name, livery, is spelled anywhere.
- **`SCHEMA.md`**: the manifest lists the entity under its post-merge path `entities/livery.md`; the tag set carries `livery`. `updated:` bumped on the renamed entity page only.
- **Docs outside the wiki**: `CONTRACTS.md` (§1/§4/§5 + the stage-file compat note), `AGENTS.md` (#5), `README.md`, `docs/architecture/PACKAGE-LAYOUT.md`, `docs/BUILD.md`, `docs/architecture/aoide-report.html` (+ one changelog ledger line), `song/songbook/update-playbook.md` (+ the migration note).

Pages updated: [[livery]] (renamed entity), SCHEMA.md, Overview.md, `ingest/index.md`, [[Full-Architecture]], [[Codebase]], [[Lexicon]], [[Snowflake-Anatomy]], [[Package-Layout]], [[Song-Anatomy]], [[Song-Vocabulary]], [[Ricing-Protocol]], [[Self-Ricing]], [[aoide-cli]], [[Quickshell]], [[Stylix]], [[shellbridge]], [[Hyprland]], [[Widget-Maker]], [[Widget-Bridge-Contract]], [[Feature-Set]], [[Gadget-Dock]], [[Desktop-Architecture]], [[Session-Graph]], [[Terminal-Commander]], [[Conductor-3D-DAG]], [[Agent-Interface]], [[Fork-and-Run]], [[Governance]], `protocol/AOIDE-DEV.md` (mentions only — §7 flags untouched in structure), `protocol/OPERATIONS/{Assertion,Wikilinks}.md`. No pages added or removed; manifest file count unchanged.

## [2026-08-13] refactor | livery merge Phase 4 — transition window closed (working tree, uncommitted)

The dev workstream executed Phase 4 of the livery merge (LIVERY-MERGE.md): the transition window is closed and the compat scaffolding is gone.

- **Mirror write dropped** — `aoide rice preview` no longer writes the legacy stage mirror (both the write and its error arm are gone), and the quickshell facet's `aoideSeedStage` seed script writes only `song/stage/livery.json`. The legacy mirror file (gitignored runtime state) is deleted.
- **Fallback reads dropped** — the conductor's `stage_notes_path` returns `dir/livery.json` unconditionally (no existence probe); the staged no-arg reads in `rice.rs` `resolve_rice_notes` and `commands/livery.rs` `resolve_notes` retarget to `livery.json`; `conductor/src/ui.rs` status row reads `livery.json`; the QML singleton's legacy FileView fallback is removed.
- **Option alias dropped** — the `lib.mkRenamedOptionModule` transition alias is deleted from `modules/nucleus/options.nix`; the pre-merge option namespace no longer evaluates.
- **QML file rename** — the QML state singleton is renamed `LiveryState.qml` (git mv), including the two code instantiations (`shell.qml`, `ConductorPreview.qml`) and every comment reference across the QML tree.
- **Comment sweep** — stale pre-merge-name mentions updated to `livery` across `song/song.md`, the sonata/default design docs, `CONTRACTS.md`, `docs/BUILD.md`, the wiki ([[livery]] transition-window lines to past tense, [[Quickshell]], [[Full-Architecture]], [[Widget-Maker]], `AOIDE-DEV.md` §7 flag → [landed + switched], `.obsidian/workspace.json`), `modules/nucleus/shellbridge.nix`, `modules/dendrites/hyprland.nix`, and `hosts/yomi-strix/default.nix`. `schema --json` changes by exactly one line (the `livery.lint` summary drops its parenthetical).
- **Dated entries below are naming-normalized** — facts untouched (dates, commits, what landed); the token layer is spelled livery throughout, per khoa's 2026-08-13 ruling.

Gated: `cargo test --workspace` green; yomi-strix toplevel build green; `nix flake check` incl. vm-boot green; `qs -p shell.qml` loads (LiveryState resolves). Committed and switched 2026-08-13, per house rule 2.

## [2026-08-14] update | `rice stage`/`rice compose` renames + the staging/declarative mode toggle (commit `e1b24b1`)

Two CLI renames plus one new feature, all landed and workspace-test-verified in `e1b24b1` (352 tests green, `nix build .#aoide` green — grounded by reading `crates/song/src/commands/{rice,mode}.rs`, `crates/storage/src/mode.rs`, and the commit diff directly, not re-verified live on yomi-strix, which still runs the pre-commit binary).

- **`rice preview` → `rice stage`; `rice mint` → `rice compose`.** Full renames, not aliases — the old spellings are unknown commands now, same as a typo. The CLI also drops its one pre-existing alias (`rice new` → `rice mint`): **no CLI-internal aliases** is now a standing contract-level rule (one spelling per command), stated as such in [[aoide-cli]]'s conventions section rather than as a one-off rename note.
- **`stage/mode.json` — the staging/declarative mode toggle.** A new gitignored stage-file (same category as `stage/design.json`) records whether `rice stage`/`cover set` may write live (`staging`) or must refuse (`declarative`, the default — absent file reads as declarative). `rice mode status`/`stage [<name>]`/`declarative [<name>]` are three new command leaves. The enforcement is two entrypoint guards (`handle_rice_stage_entry` in `rice.rs`, `handle_cover_set_entry` in `cover.rs`), not a background reconciler — `rice stage`/`cover set` are the only writers of those two stage files anywhere in the codebase, so guarding both entrypoints is a complete guarantee; `aoided` stays a one-shot skeleton with no event loop either way. `rice design enter`/`exit` (the separate, pre-existing design-mode marker) is unaffected — it calls the same underlying staging logic guard-free, and the two markers are independent today (not a decided relationship, just the current state).
- **Command tree: 51 → 54 leaves** (the three `rice mode` commands; the two renames don't change the count).

Pages updated: [[Self-Ricing]] (status paragraph, the rice-loop diagram, "Minting a song" → "Composing a song", new "Staging vs Declarative Mode" section), [[aoide-cli]] (module list, leaf table + new `rice mode` row, `rice compose`/`cover set` prose, the no-aliases rule, leaf count), [[Full-Architecture]] (stub/real callout, the rice-loop prose + diagram, the command-count paragraph), [[Codebase]] (stage-file bullet, the stub-vs-real callout, plus a separate unrelated staleness note below), [[livery]] (zero-drift paragraph, the geometry-tier live-apply paragraph), [[Widget-Maker]] (hot-swap paragraph), [[Package-Layout]] (two `management`-crate table cells), [[Fork-and-Run]] (onboarding step 9), [[Song-Anatomy]] (stage-file table row), `Overview.md` (aoide-cli gloss), `ingest/index.md` ([[Self-Ricing]] and [[aoide-cli]] glosses), [[Stylix]] (the unbuilt `--gallery` flag's status line), [[Melete]] (the extension-engine loop's verb). `protocol/AOIDE-DEV.md`: §3 mechanics table + a new "staging can be locked" bullet, the cover-derivation bug flag, the per-song-widgets open item, and the §8 quick-reference row named directly in the brief. This `## Open Threads` section: the 2026-07-26 "post-skeleton build backlog" thread's `rice preview` line struck closed (real since, renamed).

**Found but not fixed (flagged, not this pass's scope):** `references/AOIDE-HANDOFF.md` still says `rice preview` throughout — left untouched as original design-contract source material, same as every other historical reference. `concepts/desktop/Feature-Set.md` still says "38-command surface" (twice) — stale independent of this rename, predates it by several command-count bumps. `CONTRACTS.md` (repo root, not the wiki) still says `rice preview` in five places — outside wiki scope. `SCHEMA.md`'s own prose and Notes manifest still claim `AOIDE-DEV.md` lives at `references/AOIDE-DEV.md`; the file is actually at `protocol/AOIDE-DEV.md` (the 2026-07-31 rename-reconciliation entry above already missed this location drift) — a structural fix orthogonal to today's brief, not made here. `lib/vmTest.nix`'s `cmd_count == 51` assertion (code, not wiki) is now doubly stale against the real 54 — the existing [[AOIDE-DEV]] §7 flag for this class of bug had itself drifted to stale numbers (28 vs 36); its numbers are corrected in place here (51 vs 54) since that's ledger upkeep, but the code fix itself is not made. No pages added or removed; `SCHEMA.md` manifest unchanged (same file set); its `snapshot:` date is not bumped (predates this pass, unrelated staleness).

## [2026-08-14] refactor | `rice design` cut outright — added nothing over `rice mode stage` (working tree, uncommitted)

Confirmed premise: `rice design enter`/`exit` never did anything mechanically beyond `rice mode stage <song>` — both called the same `handle_rice_stage` staging logic; `enter` additionally wrote a second marker (`stage/design.json`) whose extra fields (`intent`, `sources`, `carriedSlots`) were written but read by nothing anywhere in the codebase (`carriedSlots` was always `[]` — the widget-carry/`sync` phases that would have populated it were never built). Cut clean, no deprecated alias left behind (standing no-internal-aliases rule): `crates/song/src/commands/design.rs` and `crates/storage/src/design.rs` deleted outright (including their tests); `design::register` dropped from `crates/cli/src/commands/mod.rs::all()`; the three `rice.design.*` entries dropped from `registry.rs`'s golden command-path snapshot; dangling doc-refs to `crate::design::load_design_marker`/`DesignMarker` in `crates/storage/src/mode.rs` and `crates/song/src/commands/{mode,rice}.rs` rewritten. Command tree: 54 → 51 leaves.

**Terminology note (do not confuse):** "rice design **memory**" — `song/songbook/<name>/design/intent.md`, the songbook's design-rationale folder ([[Song-Anatomy]], [[Gadget-Dock]], `rice compose`'s scaffolding) — is a completely different, unaffected concept from the "design **mode**" verb group removed here. Only the mode/marker died; design-memory content and its wiki pages are untouched.

Pages updated: `CONTRACTS.md` (`song/stage/design.json` §4 entry deleted), [[Self-Ricing]] (the `stage/design.json` mention + the marker-independence paragraph removed from "Staging vs Declarative Mode"), [[aoide-cli]] (command table row dropped, leaf count 54 → 51), `ingest/index.md` ([[aoide-cli]] gloss), `docs/architecture/PACKAGE-LAYOUT.md` (the `song` crate's `commands/` line and maps-from cell). `lib/vmTest.nix`'s `cmd_count` tripwire: bump-history comment gains the previously-unrecorded `rice mode` +3 (51 → 54) and this pass's design −3 (54 → 51); the asserted count itself lands back at 51 — unchanged in value from its already-stale prior assertion, but now for the right reason. `AGENTS.md`'s stale `rice adopt` mention (pre-dating this session's `rice adopt` → `rice declare` rename) corrected to `rice declare` in the same pass. Gate: `cargo test --workspace` green, `cargo build --workspace` warning-free, `nix build .#aoide` green.

## [2026-08-14] feat | `rice draft` lands nested under its songbook — plus `rice gen` cut outright (working tree, uncommitted)

Two changes landed together in the same pass: the new `rice draft` group (planned to follow the `rice design` removal above), and an owner correction on its storage shape mid-build that also took `rice gen` with it.

**`rice draft {save,list,stage,drop}`** — durable scratch snapshots of the live stage, for iterating on more than one variant of a song without declaring any of them. Storage shape is **nested under the song it varies**: `song/songbook/<song>/drafts/<name>/{livery.json[,cover.json]}`, not a flat `song/drafts/<name>/` (the first-drafted shape, corrected before landing — a draft is fundamentally a variation of an ALREADY COMPOSED song, so it belongs inside that song's own directory, not a separate global namespace; this falls out of the existing flow for free since a song can only ever be staged, and therefore have a resolvable "current song," once it already exists under `songbook/`). `<song>` is never a caller-supplied argument to `save`/`stage`/`drop` — all three resolve it off the CURRENTLY staged song (`stage/livery.json`'s own `"song"` field, the same resolution `rice mode stage`'s no-arg path already used, now `pub(crate)` as `commands/mode.rs::current_staged_song` so `commands/draft.rs` reuses it rather than re-deriving). `rice draft list [<song>]` walks every song's drafts with no arg, or scopes to one. `mode.json` gains a `draft` field (already present in the `ModeMarker` struct from the mode-toggle work, previously unused): it always names "the draft whose content is CURRENTLY sitting in the stage," kept true across every write path — `rice draft save`/`stage` set it (only while staging), `rice draft drop` clears it if the dropped draft was loaded, and `rice stage`/`rice mode stage`/`rice mode declarative` all clear it when they overwrite the stage from the songbook instead. One behavior change ships alongside: bare `rice mode declarative` (no name) no longer freezes the stage as-is — it now mirrors `rice mode stage`'s auto-resolve and re-pins from the resolved song's committed notes before locking (discarding unsaved live edits, `rice draft save` first to keep them — stated in the CLI's own success message); a genuinely unresolvable stage still falls back to the old bare-lock no-op. Gitignored (`song/songbook/*/drafts/`) and banned from nix-eval reads — `lib/checks.nix`'s `noSongRead` gained a regex match (`.*/song/songbook/[^/]+/drafts/.*`) since the runtime dir now nests at a variable depth a flat infix can't name, scoped tightly to just the `drafts/` subfolder (the songbook entry itself stays legitimately walkable). Command tree: 51 → 55 leaves.

**`rice gen` cut outright** (not stub-tidied) — a speculative prompt/wallpaper rice generator, scoped early in the project but never built and with no design behind it; kept as a permanent not-implemented stub would have been a promise nobody was building toward. `stubs::register_rice_gen` and its call site deleted; `rice.gen` dropped from the registry golden snapshot. The real loop is `rice compose <name> [--from <song>]` (scaffold) → `rice mode stage <name>` (go live) → edit → `rice lint` → `rice draft save <draft-name>` (iterate, not yet declared) → `rice declare <name>` (commit, still a stub, untouched by this pass). Command tree: 55 → 54 leaves (net effect of both changes together: 51 → 54).

**Terminology note (do not confuse):** none needed here — `rice gen` shared no concept with anything surviving; nothing else in the codebase referred to it beyond narrating the (never-built) planned loop.

Pages updated: `CONTRACTS.md` (new `song/stage/mode.json` §4 entry — filling a pre-existing gap, since the mode-toggle feature landed earlier this session without one — and a new `song/songbook/<song>/drafts/<name>/` entry), [[Self-Ricing]] (status paragraph, the whole "Rice Loop" diagram rewritten, a new "Drafts" section with the round-trip, the declarative-mode-discards-edits behavior note, "Coverage Tiers" section CUT — it was entirely about `rice gen`'s planned `--full` tier and had no anchor once `gen` was gone, "Adopt, Select, Replay" renamed "Declare, Select, Replay" for internal consistency with the rewritten loop), [[aoide-cli]] (module list, new `rice draft` table row + prose block, `rice mode declarative`'s prose updated for the re-pin behavior change, leaf count 51 → 54, the `rice.gen` MCP-tool-name example swapped for `rice.lint`, `rice adopt` → `rice declare` in the gated-commands line), [[Full-Architecture]] (stub/real callout, the rice-loop diagram rewritten, the command-count paragraph recomputed real 44/stub 10, `adopt-only` → `declare-only`), [[Codebase]] (vm-boot assertion count, the stub-count paragraph), [[Song-Anatomy]] (the "before every gen" phrasing), [[Ricing-Protocol]] (the base16-key creation-status paragraph, the Related-section gloss), `protocol/AOIDE-DEV.md` (the dev-vs-rice-agent callout, and the stale `lib/vmTest.nix` command-count §7 flag closed now that the code is actually fixed), `Overview.md` and `ingest/index.md` (aoide-cli/Self-Ricing glosses), `AGENTS.md` (the rice-loop headline rewritten, house rule 3's "read before you write" reworded off `rice gen`), `README.md` (stub-verb list). `lib/vmTest.nix`'s `cmd_count` tripwire itself (code, not wiki) updated to 54 with the bump-history comment carrying both changes. `song/song.md`, `song/songbook/default/rice.nix`, `song/songbook/default/design/intent.md` (comment/prose mentions of `rice gen` reworded to the real `rice compose` mechanism — each file's own dated Iteration Log / history sections, where present, left untouched).

**Found but not fixed (flagged, not this pass's scope):** `references/AOIDE-HANDOFF.md` and this file's own older dated entries still say `rice preview`/`rice gen` in places — left untouched as historical/frozen source material, same convention as every prior pass. `concepts/desktop/Full-Architecture.md`, `references/AOIDE-HANDOFF.md`, and `concepts/desktop/Controls.md` still say `rice adopt` in a few spots unrelated to this pass's edits (a pre-existing staleness from the `rice adopt` → `rice declare` rename earlier this session, not touched here since it's outside both this pass's and the design-removal pass's stated scope — flagged, not guessed away).

Gated: `cargo test --workspace` green (all crates, including the flaky-but-pre-existing `aoide-song` env-lock race noted separately — unrelated to this pass), `cargo build --workspace` warning-free, `nix build .#aoide` green, `nix eval` sanity-checks against the new `noSongRead` regex, and the full 6-step round-trip from the spec verified live against the built `aoide` binary with a scratch `AOIDE_STAGE_DIR`/songbook.

## [2026-08-14] refactor | drafts reached by SYMLINK ROUTING, not mtime-guessing — `rice mode draft` replaces the auto-prefer heuristic (working tree, uncommitted)

Simplification from the owner, superseding the auto-prefer-latest-draft mechanism from the immediately preceding pass: "just make a separate draft mode and things will be routed to the draft" — routing, not guessing.

**`RiceMode` becomes three-way**: `Staging | Declarative | Draft` (`crates/storage/src/mode.rs`). `Draft` is a distinct mode, not a flag riding on `Staging`. `ModeMarker.draft` is `Some(name)` **if and only if** `mode == Draft` — simpler than the old invariant ("`draft` names whatever's currently in the stage, even under `Staging`"), which is gone along with the mtime-comparison heuristic that needed it.

**The foundational fix, first:** `crates/storage/src/fs.rs::atomic_write` writes a temp file then `rename()`s it into place — POSIX `rename()` replaces whatever directory entry sits at the destination, it does NOT dereference a symlink there and write through it. Without a fix, the first write after routing `stage/livery.json` into a draft would silently replace the routing symlink with a plain file. Fixed generally (not draft-specific — every stage-file writer routes through this one function): before renaming, `atomic_write` now checks `symlink_metadata` on its target; if it's a symlink, resolves where it points (`read_link`, resolved against the target's own parent when the link is relative) and renames the temp into THAT path instead, leaving the symlink itself untouched. Three new tests: writes through a symlink twice (proving it survives, not just a one-shot fix), a relative-symlink-target case matching `rice mode draft`'s own shape, and a plain-file regression check.

**Mechanism:** `rice mode draft <name>` (new, `crates/song/src/commands/mode.rs::handle_mode_draft`) resolves the current song ([`current_staged_song`], same auto-resolve `rice mode stage` uses — no separate `<song>` arg), forks the draft from whatever's currently in the stage if the name is new (reusing `crates/song/src/commands/draft.rs::fork_stage_into`, the same write `rice draft save` performs — not reimplemented), removes whatever currently sits at `stage/livery.json` (a real file or an old symlink to a DIFFERENT draft), and symlinks it to the draft's own `livery.json`. From then on, every writer — `rice stage`, a hand-edit, Quickshell's own FileView reload — transparently lands in the draft, because `atomic_write` is now symlink-transparent. `cover.json` is explicitly OUT of scope: only `livery.json` is routed.

**Command surface:**
- `rice mode stage [<name>]` and bare `rice stage [<name>]` had the auto-prefer-latest-draft heuristic (and `latest_draft_for`/`load_draft_into_stage`) deleted outright. Both now ALWAYS mean plain declared content — zero draft-awareness. Leaving `Draft` mode is `rice mode stage`/`rice mode declarative`'s job: both now call a new `teardown_draft_symlink` FIRST (after resolving which song is active, since that resolution itself reads through the symlink — order matters), so their declared-content write lands in a real file rather than transparently through into whatever draft the symlink still pointed at. `handle_rice_stage_entry` (`commands/rice.rs`) got a related fix: while `mode == Draft`, it now leaves `mode.json` COMPLETELY untouched on a successful stage (previously it always wrote `song`/cleared `draft`) — a raw `rice stage <name>` call while routed still writes through the symlink into the draft (zero symlink-awareness, per the mechanism above), so mutating the marker's `song`/`draft` fields here, independent of the actual routing, risked contradicting the "`draft` is `Some` iff `mode == Draft`" invariant if this handler's song ever diverged from the routed draft's own song. Its doc comments (and one test's framing) also dropped the now-moot "ignores drafts" language the owner flagged — reframed as plain "no name = current song," the same convenience `rice mode stage` already documents for its own no-arg form; that auto-resolve itself (the owner's own in-flight addition, landed just before this pass) is unchanged and independently useful.
- `rice draft stage <name>` (the old copy-based verb: read the draft, write it into the stage as a one-shot snapshot) is DELETED — registry entry, handler, tests. Fully superseded by `rice mode draft`'s routing; keeping both would be two spellings of "go live with this draft" (no-internal-aliases rule).
- `rice draft save <name>` KEPT, narrowed to an explicit, mode-independent fork: snapshot whatever's currently live (reading transparently through a routing symlink if one is active) into a new-or-updated draft, without switching modes — useful for preserving a second variant while still working in a first one. Never touches `mode.json` anymore (the old "if staging, set the marker's draft field" logic is gone — that semantics now belongs exclusively to `rice mode draft`). Its write-only core is `fork_stage_into`, reused by `rice mode draft`'s fork-on-first-entry step.
- `rice draft list [<song>]` unchanged in code — its existing `marker.song == song && marker.draft == name` "currently loaded" check was already exactly right for the simpler invariant, nothing to fix.
- `rice draft drop <name>` now REFUSES (`draft-is-live`) if `<name>` is the draft `Draft` mode currently has the stage routed to, rather than silently also tearing down the routing and falling back to `Staging` — the owner left this choice to us: refuse is the pick, since a `drop` that also silently changes your mode and re-pins the stage would be the more surprising behavior; `rice mode stage`/`rice mode declarative` are the already-documented way to leave `Draft` mode first.

Command tree: 54 → 54 (net zero: `rice draft stage` removed, `rice mode draft` added).

**Terminology note:** "rice design **memory**" (`song/songbook/<name>/design/intent.md`) remains completely unrelated and untouched, same as every prior pass in this thread.

Pages updated: `CONTRACTS.md` (`song/stage/mode.json` rewritten for the three-way enum, `song/songbook/<song>/drafts/<name>/` rewritten for routing-not-copying, `song/stage/livery.json`'s own entry gains an additive symlink note), [[Self-Ricing]] (the Rice Loop diagram, the Drafts section fully rewritten and retitled "reached by ROUTING not copying," the Staging vs Declarative Mode section expanded to the three-way toggle — header text kept stable so existing `[[Self-Ricing#Staging vs Declarative Mode]]` links across the wiki don't break), [[aoide-cli]] (command table row counts, the `rice mode`/`rice draft` prose blocks), `AGENTS.md` (the rice-loop headline, the staging/declarative callout retitled), `ingest/index.md` ([[Self-Ricing]] gloss). `lib/vmTest.nix`'s `cmd_count` tripwire: bump-history comment gains the net-zero `rice draft stage` → `rice mode draft` swap; the asserted count itself stays 54. **Also fixed in passing:** [[Snowflake-Anatomy]] had a `[[Self-Ricing#Adopt, Select, Replay]]` link left dangling by this thread's own earlier rename of that section to "Declare, Select, Replay" — caught during this pass's link audit and repointed.

## [2026-08-14] retire | `song/songbook/default/` is gone — `sonata` becomes the shipped standard song (working tree, uncommitted)

The song formerly named `default` — upstream's shipped, guaranteed-present baseline (`aoide.song`'s default value) — is retired outright and the name `sonata` (already the actively-performed song on yomi-strix) takes over that role. This is a rename of the *role*, not a merge of two songs: `default`'s files are deleted, not folded into `sonata`'s.

**Code/config:** `modules/nucleus/options.nix` (`aoide.song`'s `mkOption` default + description), `lib/vmTest.nix` (the vm-boot check), `hosts/yomi-strix/default.nix` (comment reworded — its explicit `aoide.song = "sonata";` now duplicates the default, kept anyway as good practice, no longer framed as a non-default example), `pkgs/aoide/crates/cli/src/guide.rs`, `pkgs/aoide/crates/song/src/commands/rice.rs` (`rice compose`'s implicit `--from` default + its `flag!` description string — five test fixtures that built a `songbook/default` dir and relied on the old implicit fallback repointed to `songbook/sonata`, two content assertions (`"inherited from `default`"`, `data["from"]`) updated to match; `compose_missing_from_song_is_error` needed no change, it never named a song). `CONTRACTS.md` §5 gets an inline `**Renamed (2026-08-14)**` note (matching the file's own undated `**Additive**` callout style — CONTRACTS.md has no prior dated-amendment convention to match more closely).

**Salvage before delete:** `design/pantheon.md` (the cross-cutting Pantheon wireframe/glyph grammar `default` owned, live-cross-referenced from six wiki pages plus sonata's own `greek-grammar.md`/`intent.md`) moved, not deleted, to `docs/Aoide-Wiki/references/pantheon/pantheon-grammar.md` (the dir that already held its source-still screenshots) with a new header marking it historical reference for a retired song. `design/intent.md`'s recorded aesthetic (Catppuccin Mocha rationale, the Windows-7-sidebar/ASCII gadget-dock aesthetic, its iteration log) distilled into a new `song/songbook/learnings.md` entry — the file itself is gone; full history is `git log --follow -- song/songbook/default/`. Every live cross-reference to the old `pantheon.md` path (or to `default` as the shipped/guaranteed-present song) repointed: `SCHEMA.md`, `Song-Vocabulary.md`, `Fork-and-Run.md`, `Full-Architecture.md` (incl. its merge-only framing, see below), `Lexicon.md`, `Conductor-3D-DAG.md`, `Song-Anatomy.md` (incl. its merge-only framing), `Ricing-Protocol.md`, `Self-Ricing.md`, `AOIDE-DEV.md` (its "[planned] Default song with Pantheon thematics" open item retitled `[retired]` — the song it planned to dress no longer exists), `song/song.md`, `docs/BUILD.md`, `song/songbook/update-playbook.md`, and sonata's own `rice.nix`/`design/{intent.md,greek-grammar.md}`. `docs/Aoide-Wiki/references/AOIDE-HANDOFF.md` and `docs/architecture/LIVERY-MERGE.md` deliberately left untouched (append-only/frozen history).

**The merge-safety claim rewritten, not just renamed:** [[Self-Ricing]]'s "Shipped Defaults Are Immutable" section retitled "The Shipped Baseline Is Guarded, Not Frozen" — the original claim ("upstream ships a never-iterated fallback; evolution only lands as new folders") is exactly what this change overturns, since `sonata` is now both shipped AND actively iterated. Restated as the standing rule, not a regression: `sonata` is upstream-owned and evolving, like any other upstream-owned tree (nucleus, facets) — upstream MAY update it. Every OTHER song, composed via `rice compose` under any other name, is fork-owned and upstream never touches it — an absolute, unchanged guarantee. What actually prevents silent clobbering was never path-freezing: `rice compose` without `--force` refuses existing songs, nothing writes into a song unprompted, `rice declare` is User-gated. Same reframing applied everywhere the merge-only claim appeared: [[Song-Anatomy]]'s blockquote + repo-layout tree, [[Full-Architecture]], `song/song.md`'s tree comment.

Gated: `cargo test -p aoide-song rice::` green (25/25, incl. the five repointed fixtures) ahead of the full-workspace gate below.

Gated: `cargo build --workspace` warning-free, `cargo test --workspace` green (117 tests in `aoide-song` alone, incl. new symlink-routing coverage in `mode.rs`/`draft.rs`/`rice.rs` and the `atomic_write` symlink tests in `aoide-storage`), `nix build .#aoide` green (staged new/untracked files first), and the owner's exact round-trip re-verified live against the built binary end to end — including the in-place-edit-lands-directly-in-the-draft step, the `rice draft drop`-refuses-the-live-draft case, and a raw `rice stage` writing through an active routing symlink while leaving the marker untouched.

## [2026-08-14] docs | greek-grammar retcon — §4 widget-map + motif vocabulary cut, dependent docs/comments reconciled (working tree, uncommitted)

The owner hand-edited `song/songbook/sonata/design/greek-grammar.md` directly, cutting it 578 → 108 lines: gone are the intro's Pantheon-divergence framing, the entire "Five architectural motifs" subsection (columns, the meander/Greek-key, entablature & pediment, stylobate — the box-drawing vocabulary `║ ▌ ▐ │ ╎ ‖`), and the whole §4 "Widget-by-widget map" (the `GadgetFrame` entablature/pantheon-temples "two families" split and their convergence amendment). Explicit instruction: this is a **retcon**, not a further edit — the surviving 108 lines (§1 order-marks + musical-motif coexistence, §2 the state-glyph contract, §3 the role→hue palette) are the new, complete, sole truth. Nothing cut was restored, summarized, or re-derived; every dependent doc/comment that had cited the cut vocabulary or §4 as sonata's design authority was reconciled to no longer do so.

**Rewritten (cited the deleted vocabulary/§4 as sonata's own documented grammar):** `song/songbook/sonata/design/intent.md` — the header `**Grammar:**` line (dropped "(columns, meanders, pediments)"), the "Current surface elements" intro (dropped the "greek-grammar reskin"/§4-blueprint framing, points at each songbook widget's own file header instead), and a bracketed `[2026-08-14: …]` annotation appended to the 2026-07-29 "RE-KEYED to the Greek register" log entry — following that same file's own existing precedent (its 2026-07-29 glass-alpha entry already carries a bracketed 2026-08-14 note from the earlier Pantheon-retirement pass) rather than rewriting historical log prose in place. `docs/Aoide-Wiki/concepts/Package-Layout.md` and `docs/architecture/PACKAGE-LAYOUT.md` — both dropped "the Greek-key meander" from the `canon` design-primitives example list (verified via grep: no file under `modules/facets/` draws a meander anywhere; the motif exists nowhere live, not even facet-side).

**Reworded (QML comments that cited greek-grammar.md/§4 as the source of a shared cross-cutting rule, not describing their own file's real chrome):** `song/songbook/sonata/widgets/notifications.qml` (dropped the `§4 "The four temples"` citation, points at each temple file's own header instead — the underlying claim, that this widget joins the shared marble-stele family, stays, since it's still true); `song/songbook/sonata/widgets/calendar.qml` (a librarian-pass TODO that said "carry X into greek-grammar.md §4" — reworded since there's no §4 left to carry anything into; the "Π dormancy into §1" half of that TODO is resolved by this same pass, see below).

**Left alone (describes the file's own real, current chrome, not a cross-cutting mandate):** `song/songbook/sonata/widgets/bar.qml`'s header line "the old architectural grammar (the Pantheon entablature …) is DISCARDED wholesale" — this contrasts bar.qml's own manuscript-staff redesign against the already-independently-retired `default` song's Pantheon grammar, never cites greek-grammar.md by name or section. `docs/Aoide-Wiki/references/pantheon/pantheon-grammar.md` and `docs/Aoide-Wiki/protocol/AOIDE-DEV.md` — both point at greek-grammar.md only as "sonata draws its own, deliberately divergent grammar," never restate the cut vocabulary, so neither dangles.

**"Current aoide standards" accuracy pass (§1–3 verified live, not assumed) — two findings, both fixed in `greek-grammar.md` itself:** (1) §1's "only the bar-popout marks are LIVE — `Λ Ξ Π Σ`" claim no longer holds: reading `song/songbook/sonata/widgets/bar.qml`'s actual popout wiring (plus its own comments) shows `Λ` (volume.level) and `Π` (calendar.sheet) moved to bare `StelePopout`/`SteleLayerPopout` hosting this session (no `GadgetFrame`, hence no pediment to carry a mark — bar.qml's own comment says the `Π` mark "goes dormant with the bay, like `Α Β Γ Δ Θ Ω` before it"), and `Σ` (nowplaying.score) has no bar-popout wiring at all today. Only `Ξ` (battery.gauge, still hosted via `BarPopout`/`GadgetFrame`) is actually live — §1 rewritten to say so. (2) §2's "lifted VERBATIM from `theme.rs`" claim had two real drifts against `pkgs/aoide/crates/conductor/src/theme.rs`: the path itself was stale (`src/conductor/` pre-dates this session's crate-split, real path is `crates/conductor/src/`), and the `stopped` glyph in the table (`𝄼` U+1D13C) did not byte-match the Rust's actual `StateClass::Stopped => "𝄁"` (U+1D101) — confirmed via codepoint extraction, not eyeballing. Both fixed in `greek-grammar.md`.

**Left alone, flagged, no scope-creep:** intent.md's own iteration-log prose still says the old `src/conductor/theme.rs` path in its 2026-07-29 entry — left as historical record (same append-only convention as its neighboring entries), not fixed in place; the crate-split rename is orthogonal to this retcon and already documented elsewhere ([[Package-Layout]]). No file under `pkgs/aoide/` touched (out of scope by instruction). Nothing committed — left uncommitted for review.

## [2026-08-14] rename + reframe | Fork-and-Run retired → Clone-and-Run — the install model no longer requires a fork (working tree, uncommitted)

The deployment model is reframed: **your clone is your instance**. A plain `git clone` of upstream is the whole install — your dendrites (`modules/dendrites/`, additive by construction) and your songs (`song/songbook/`, your writable domain) are ordinary local commits on that clone, and `aoide update` merging upstream works identically. A remote fork is now **optional** — for backup, fleet sync, or contributing back — not a prerequisite. Docs/governance reframing only: no code path ever enforced forking (`onboard`/`update` remain exit-64 stubs, re-specified in place).

**The page:** `concepts/governance/Fork-and-Run.md` → `concepts/governance/Clone-and-Run.md` (`git mv`, history preserved) and rewritten: "Why a Fork" → "Why Shared History" (songs travel with the *clone*; the remote-fork bullet demoted to optional), install drops the fork-first step (`git clone <upstream> ~/Aoide` + `aoide onboard`), onboard step 3 reworded (the clone already tracks upstream as `origin`; a personal remote is an offer, not a requirement), Self-Update unchanged in substance.

**The sweep (`[[Fork-and-Run]]` → `[[Clone-and-Run]]` plus "fork"→"clone" model-mentions):** `SCHEMA.md` (manifest path + governance-folder gloss), `Overview.md` (lead + concept gloss), `ingest/index.md` (gloss), `Lexicon.md` (the snowflake row: "your fork" → "your clone"; "committed to the fork"), `Snowflake-Anatomy.md` ("committed to the fork" + related link), `Gadget-Dock.md` ("a fork can add its own gadgets"), `Widget-Maker.md` ("one reproducible fork"), `Feature-Set.md` ("a fresh AoideOS fork"), `Codebase.md` (×2), `Full-Architecture.md` ("fork-owned"), `Package-Layout.md` ("forked onto"), `Song-Anatomy.md` ("fork-owned"), `Song-Vocabulary.md` ("committed to the fork"), `Self-Ricing.md` ("pulls the fork"), related-link lists in `Governance.md`/`Wiki-Protocol.md`/`Agent-Interface.md`/`Song-Vocabulary.md`/`entities/dxflake.md`. Non-wiki: `README.md` (lead, install §4 — fork-first step dropped, updates §5 links), `flake.nix` (description), `song/song.md` ("fork-owned" tree comment), `docs/BUILD.md` (`aoide.user` row), `docs/architecture/PACKAGE-LAYOUT.md` (×3), `docs/architecture/aoide-report.html` (repo copy; live claude.ai artifact republish owed — flagged in `AOIDE-DEV.md` §7). Code strings that name the model: `modules/nucleus/options.nix` (`aoide.user` description), `modules/nucleus/packages.nix` + `modules/facets/quickshell/default.nix` (comments), `pkgs/aoide/crates/cli/src/commands/stubs.rs` (`onboard` schema summary "register the fork" → "register the clone" — verified the string is not quoted in `CONTRACTS.md` or `aoide-cli.md`; command count unchanged).

**Deliberately untouched:** `references/AOIDE-HANDOFF.md` (frozen historical contract — it records the old model *as* history), `references/AOIDE-VS-LANGCHAIN-HANDOFF.md` (incidental verb), this log's own older entries (append-only), and the entire draft-fork vocabulary (`rice draft save`'s "fork the stage", `fork_stage_into`, sudo/PTy process-fork comments) — a different word sense entirely.

## [2026-08-14] add | shelved host skeletons — `hosts/_{desktop,laptop,server,mac}` templates (working tree)

Four example host skeletons land under `hosts/`, carrying the `_` shelving prefix so nothing walks, registers, or evals them — yomi-strix stays the living reference and the only entry in `nixosConfigurations`. `_desktop`/`_laptop` are the full-desktop shape (all three facets + hyprland + conveniences; laptop adds stock `powerManagement`); `_server` is the headless profile (core only — no facets, kitty shelved, melete/mneme commented-but-guarded); `_mac` is explicitly forward-looking and non-evaluable — it documents the darwin class seam it waits for (nix-darwin input, `mkHost` class arg + darwinModules, launchd twin for the nucleus services, `hosts/common`'s NixOS-only options needing a class split, `pkgs/aoide` aarch64-darwin), per the PACKAGE-LAYOUT "NixOS optional" direction, and blocks the darwin-incompatible baseline host-side (`aoide.fonts`/`kitty` off — both defaulted on by common — plus a keep-off list for the systemd/wayland/pipewire dendrites), deliberately NOT importing `../common`. Each header carries its own adopt recipe (copy → register one flake line → set hostName/user → rebuild). Docs touched: `flake.nix` registration comment, `README.md` install step 2, [[Clone-and-Run]] onboard step 1. Verified: nixfmt + `nix-instantiate --parse` on all four, flake eval unchanged (`[ "yomi-strix" ]`).

## [2026-08-14] add | `networkmanager` dendrite — the applet package, nothing more (working tree)

New `modules/dendrites/networkmanager.nix` (shape v0, `aoide.networkmanager.enable`): ships `pkgs.networkmanagerapplet` and nothing else — the user steered it to "just a package." The package provides both `nm-connection-editor` (the actually-useful surface: the GUI network editor, reachable from the Grimoire launcher) and `nm-applet` (the tray indicator — inert today since the Quickshell shell has no system tray anywhere: checked `modules/facets/quickshell/qml/`, `run/qml/`, and `song/` for a SystemTray before deciding against any autostart service). NetworkManager itself stays venue plumbing — the host's own `networking.networkmanager.enable`, which the dendrite merely assumes. Enabled on `yomi-strix` and the `_desktop`/`_laptop` skeletons; `_server`/`_mac` skip it (headless / GTK-on-darwin). Verified: attr confirmed against the locked nixpkgs (`networkmanagerapplet`, mainProgram `nm-applet`), nixfmt clean, `config.aoide.networkmanager.enable` evals `true`; toplevel build gate in flight.

## [2026-08-19] ingest | design philosophy — "everything is a plugin" (`e9a1f89`)

`CONTRACTS.md` gains a `§0. Design philosophy — everything is a plugin`, mirrored as house rule 7 in `AGENTS.md`/`aoide guide` (`pkgs/aoide/crates/cli/src/guide.rs`) and as the hard line in sonata's `design/widget-structure.md`. The statement: a capability enters Aoide by *existing* at a conventional path, declares what it needs by *name*, and is removable without a trace — never an import-list edit, never reaching into another module, never an effect with no inverse. Cited, not adopted: **Cordis**, *A Programming Paradigm for Spatiotemporal Composability* (Shi, Zhang & Cui; preprint 2026-08-13, `github.com/cordiverse/paper`), the plugin kernel under DeepSeek Harness — supplying the **spatial** (declare deps by name, never import an implementation) / **temporal** (effects are revertible; removal unwinds what it installed) composability vocabulary. The corollary: Quickshell PAINTS and is never where a capability lives — state/policy/IPC/system access sit behind an agnostic bridge reachable with only a shell; a new API lands as a bridge first, QML picks it up second.

New page `concepts/Plugin-Architecture.md` — the philosophy's home, chosen over folding it into [[Full-Architecture]] or [[Widget-Maker]] because it is cited across both the nix-discovery side and the Quickshell corollary, and CONTRACTS.md itself calls it "the rule every contract is downstream of," which reads as a first-class concept, not a subsection of either. Cross-linked into [[Overview]] (Concepts list), [[Full-Architecture]] (thesis + control-plane mentions, Related), [[Codebase]] (`lib/walk.nix` section, Related), [[Widget-Maker]] (new "The hard line: a widget is a render surface" section, Related), [[Widget-Bridge-Contract]] ("The rules a widget is built by" intro, Related), [[Desktop-Architecture]] (the "no MCP in QML" rule, Related). Wired into `ingest/index.md` and the `SCHEMA.md` manifest + `plugin` tag.

## [2026-08-19] refactor | sonata facet→songbook migration, Phases 1–5 — Phase 6 (the facet switch) NOT landed (`c083d2b`, `b7f5615`, `4d08843`, `8e02f7b`, `6b459af`)

Fourteen widget bodies now live at `song/songbook/sonata/widgets/` rather than only `modules/facets/quickshell/qml/`: the pantheon gadgets (conductor, terminals, meters, power, usage — Phase 2), the dock host (Phase 3), the wallpaper layer + its picker (Phase 4, plus a Phase-4.5 visual pass on the picker in `8e02f7b`), and the four preview harnesses repointed to load sonata's own score via `Loader.setSource` instead of instantiating the facet gadget directly (Phase 5). **Phase 6 — repointing `shell.qml` from the facet originals (`AoidePanel`, `AoideWallpaper`, `AoideWallpaperPicker`) to a `WidgetSlot`/`SurfaceSlot` anchor — has not happened and needs a User rebuild**: `shell.qml` still instantiates the facet originals directly today, so the live desktop renders the pre-migration bodies; the eight new songbook twins (dock, conductor, terminals, meters, power, usage, wallpaper, wallpaper-picker) are carried but unanchored, same inert-not-error posture as any unwired slot.

Wiki updated to state this boundary precisely, not as if the switch landed: [[Gadget-Dock]] gains a "Status: songbook migration landed, facet switch pending a rebuild" note (Related: [[Song-Anatomy]]); [[Song-Anatomy]]'s `songbook/<name>/` table gains a fourteen-body count with the wired/unwired split (Related: [[Gadget-Dock]]). `updated:` bumped on both pages.

## [2026-08-19] rename | the `notes` → `livery` property rename, wiki sweep (`3d2b073`, `1bb6372`, `3e6e7be`)

The QML injected-prop contract every song widget declares (`WidgetSlot.qml` injects `{"livery": …, "bridge": …}`; all fourteen sonata widgets declare `required property var livery`) and the CLI's user-facing `livery lint/resolve/emit` summary strings (`crates/song/src/commands/livery.rs`) both moved from `notes` to `livery` — the same rename [[livery]]'s page already carries at the *concept* level from 2026-07-28; this pass is the literal property/CLI-text catch-up. `3d2b073` itself already swept four wiki pages ([[Full-Architecture]], [[Widget-Maker]], [[Quickshell]], `protocol/AOIDE-DEV.md`). This pass found and fixed the remainder: `entities/livery.md`'s "Verbs" section still described the CLI as validating a "note container"/printing a "note set"/defaulting to "the staged notes" — reworded to "livery file"/"livery set"/"the staged livery" to match the CLI's own current summary text (verified against `livery.rs`'s diff in `3d2b073`, which made the identical substitution in the Rust source); `ingest/index.md`'s [[shellbridge]] gloss said the stage files carry "notes, sessions … " — corrected to "livery, sessions …".

**Deliberately left untouched** (the musical sense, not the property): [[Lexicon]]'s "liner" gloss, [[Quickshell]]'s "note-glyph"/note-theming UI vocabulary, [[Widget-Bridge-Contract]]'s "smaller notes joined … beam" beaming description, [[Content-Pipeline]]'s "design notes" dogfood section, `entities/livery.md`'s own `aliases: [aoide-notes, Notes, notes package, note engine]` (resolves historical `[[Notes]]` mentions in this log), and every dated entry in this log that legitimately says "notes" as the name that was true on that date — falsifying a record to match a later rename would be the wrong repair, same reasoning `3e6e7be`'s own commit message gives for leaving sonata's `hazards.md`/`intent.md` alone.

## [2026-08-19] refactor | A2A bearer-token admission gate (`23f0a0e`)

`message/send` was origin-blind on the Spawn path and loopback-trusting unconditionally on Inject — behind any reverse proxy or tunnel (`ssh -R`, a tailscale funnel, cloudflared, nginx) the server sees the proxy's own loopback address for every caller, so a remote attacker inherited that trust and, on Spawn, could launch the operator's configured agent with zero gate beyond the command being non-empty. Closed with `aoide.a2a.tokenFile`: `classify_origin` → `PeerOrigin::{Loopback,Remote,Unknown}`; `classify_token` → `TokenState::{Absent,Valid,Invalid}`; `spawn_authorized(token_configured, token_state)` refuses a spawn with `-32005` unless a configured token validates; `effective_origin` demotes a bad-token caller — loopback included — to `Unknown` before `should_deliver_now` sees it; per-peer identification moves off address too via `Peer.tokenFile` (`state/peers.json`, `peer add --token-file`) and `aoide_storage::peer_store::{token_bytes_eq,is_autogated_peer_token}` (length-independent compare). Empty `tokenFile` (the default) is byte-identical to pre-amendment behavior, pinned by dedicated regression tests. `CONTRACTS.md` §6 carries the full amendment text (dated 2026-08-19, alongside the 2026-08-14 non-loopback pending-gate amendment it revises).

Wiki: [[A2A-Door]]'s "Security and governance" section rewritten — the old "loopback is unconditional" bullet is gone, replaced with the token mechanism, Spawn's new gate, and the loopback-coupling rule; [[Peer-Federation]]'s "Security — the non-loopback pending-gate amendment" section updated to match (the `Loopback` bullet now carries the "only while no server-wide token is configured" condition, and autogate is now the OR of address-match and per-peer-token-match), plus its registry section/CLI surface gain the `tokenFile`/`--token-file` field; `entities/aoide-cli.md`'s `peer add` bullet gains the flag and a pointer to the security section. `ingest/index.md` glosses updated for both pages. `updated:` bumped on all four.

**Backlog note:** this wiki's last log entry before this pass was dated 2026-08-14 — a large backlog of commits landed between then and now (`git log` shows dozens) remains unlogged and un-ingested; out of scope for this pass by explicit instruction, flagged here so it is not silently forgotten.

## [2026-08-19] add | dev CLI reference — `references/cli/` (83 commands)

Minted `references/cli/` — the dev-facing command I/O reference the shape reserves `references/` for: a hub ([[references/cli/Index|cli/Index]]) plus six group pages ([[references/cli/Rice-and-Livery|Rice-and-Livery]] 22 verbs, [[references/cli/Graph-and-Conduct|Graph-and-Conduct]] 17, [[references/cli/Screen-Verbs|Screen-Verbs]] 14, [[references/cli/Doors-and-Peers|Doors-and-Peers]] 15, [[references/cli/Content-and-Hooks|Content-and-Hooks]] 7, [[references/cli/Meta-and-Upkeep|Meta-and-Upkeep]] 8), each command carrying signature, files read, files written, and where output pipes to — verified against the Rust source (`pkgs/aoide/crates/`) and `aoide schema --json`, not prose memory. Full coverage of the 83-leaf schema confirmed by a skim review; stubs (`rice declare`/`transpose`, the five `content` verbs, `make`/`update`/`onboard`) documented as contract surface only. The pages flag two source-vs-binary discrepancies (the installed binary predates the `--token-file` flags) and mark the few claims unverifiable in source (`rice transpose`'s `palette/` dir, the content registry path, who drains `stage/pending.json`) instead of inventing. SCHEMA manifest + index updated.

## [2026-08-20] add | Loop-Protocol — the harness-agnostic agent loop spec (#52 Phase 1)

New `concepts/orchestration/Loop-Protocol.md`, minted from the #52 decision
record (Fable advisor, 2026-08-20): the loop is specified over Aoide's
existing session primitives (`graph spawn`/`send`/`pending`/`view`/logs/
transcripts), not a new verb — every consequential judgment in a multi-role
run (ship vs send-back, coaching, killing an agent) stays with the agent
exercising it. The page states the tier concept (one fresh-context unit per
planner/executor/reviewer role; a harness's internal subagents and a `graph
spawn` session are two bindings of that one concept, neither ranked over the
other), the two-rung R1-tiered/R2-single-agent degradation ladder and the
invariant that `graph spawn` makes R1 universally reachable, the binding rule
(judgment per brief, with long-running-executor/short-scoped-work guidance
and three conditions that force `graph spawn`), review integrity (the
reviewer is never the executor's own context; grading discipline travels
with the brief), the R2 degradation-announcement rule, and each registered
harness's rung status (claude and kimi bind through either internal
subagents or `graph spawn`; pi holds no rung, linked to
[[Conductor-Channel]]'s headless section for the reason rather than
restating it). Links out to [[Conductor-Channel]], [[Session-Graph]],
[[Agent-Hooking]], [[Terminal-Commander]] without duplicating any of their
owned content (the autogate gate ladder, hook mechanics, or the CLI tier
map).

Wired into `SCHEMA.md` (manifest entry under `concepts/orchestration/`,
snapshot bumped to 2026-08-20) and `ingest/index.md` (one gloss line under
Concepts, beside [[Conductor-Channel]]).

**Flag:** the queued installable onboarding skill (agent-read dir, wired via
hooks/commands) folds this page's spec into itself as part of collapsing the
AGENTS.md/guide.rs duplication — not done in this pass.

## [2026-08-20] fix | per-profile `graph send --submit` keystroke — the kimi `\r` footgun closed (#52 Phase 2)

`AgentProfile` (`protocol/src/agents.rs`) gained `submit_key`: `\n` for claude
and pi, `\r` for kimi (ground-truthed on a live screen — Conductor-Channel's
`graph send` entry). `graph send`'s delivery path (`conduct/src/graph/
send.rs::session_send`) now appends the TARGET session's own `submit_key`,
resolved from its registered agent via `profile_for_agent` (promoted
`pub(in crate::graph)` from `permit.rs`, the same resolver `graph permit`
already used — no second lookup of the profile table); an unregistered or
empty agent falls back to claude's `\n`, same as that resolver always has.
Ordering is unchanged and load-bearing: the submit byte still appends to the
ORIGINAL text before the sender-provenance prefix is prepended.

Closes the manual workaround the wiki previously documented: a `graph send
--submit` against a kimi target used to type the line without submitting it,
requiring a separate `\r` send. `Conductor-Channel.md`, `Session-Graph.md`,
`Agent-Hooking.md`, and `references/cli/Graph-and-Conduct.md` are corrected —
the byte is now resolved automatically, not a caller's manual step.
`CONTRACTS.md` §4 gained one sentence at the `pending.json` submit entry:
the queued flag means "submit the line", the concrete keystroke resolves at
delivery time.

**Flag:** `AOIDE-DEV.md:281-282` still describes the old `\n`-only /
separate-`\r`-send behavior and is now stale — left untouched
(orchestrator-owned file per the brief); orchestrator to correct.

## [2026-08-20] fix | kimi's permission-prompt keys, verified on a live screen (#52 follow-up)

`KIMI_PROFILE.permission_keys` (`protocol/src/agents.rs`) was `None` —
kimi's own permission prompt had never been read on a live screen, so
`graph permit` refused to summon it rather than guess. A headless
conducted kimi driven to a real shell-permission prompt settled it: `▶ 1.
Approve once / 2. Approve for this session / 3. Reject / 4. Reject with
feedback`, and injecting the bare byte `1` (no trailing `\r`) fired
approval immediately — the digit alone chooses AND confirms, same as
claude's. `permission_keys` is now `Some({approve: "1", deny: "3"})`,
matching claude's shape (never `"2"`, the session-wide allow-all; `"4"` is
reject-with-feedback, not the bare deny). `permit.rs`'s
`only_claude_carries_verified_permission_keys_today` test — name and
assertion both stale — is renamed
`claude_and_kimi_carry_verified_permission_keys_pi_does_not` and now pins
kimi's keys instead of asserting they're absent; pi's stay `None` (no live
probe yet). `references/cli/Graph-and-Conduct.md`'s `graph permit` entry
corrected to match — no other page asserted kimi refuses a summons.

## [2026-08-23] ingest | Workstream SECRETS (pkgs/aoide/crates/secrets/{README,AGENTS}.md, CONTRACTS.md's Secrets wire subsection)

- 1 concept page created: [[Secrets-Broker]] — identity + release-to-client
  flow, policy (`consumers[]`/`requireTotp`/`automation`/`remote`), the
  parked-TOTP-resolve lifecycle (`pending`/`approve`/`dismiss`), `secrets
  watch`/`--popup`, the `file`/`age` backends and `secrets migrate`, and
  deployment (`aoide-secrets-serve`, `ProtectHome=true`, the events feed).
- Pages updated: `Overview.md` (Concepts list), `entities/aoide-cli.md`
  (the `aoide_secrets` registry entry, the leaf table's missing `secrets`
  row, a new `secrets` paragraph, the stale `48 leaves`/`63 leaves` count
  corrected to the live `secrets migrate`-inclusive total), `concepts/
  Full-Architecture.md` (the command-tree breakdown gained `secrets`, the
  systemd-units paragraph gained the broker's own SYSTEM service), `concepts/
  governance/Governance.md` (the secrets broker mirrors into the same audit
  log; its gate is parking, not the rebuild gate), `references/cli/Index.md`
  (the Gating bullet gained `secrets exec`'s park-instead-of-`gated:true`
  shape), `ingest/index.md` (new gloss + the `aoide-cli` gloss's stale
  `83-command tree` figure dropped, its own count never having matched
  either binary's real total).
- Subjects deferred to mentions, not their own page: TOTP/RFC 6238,
  `secrets put`'s overwrite-warning flow, and the age-identity lazy-mint —
  folded into [[Secrets-Broker]]'s own sections rather than split out.
- Not touched (drift spotted, out of this pass's scope): `aoide-cli.md`'s
  and `Full-Architecture.md`'s command counts were already short by an
  `inbox` (3-verb) and a `who` (1-verb) group predating this pass — flagged
  in both pages' own prose rather than silently absorbed into the corrected
  secrets-inclusive totals; `references/fleshing-out-aoide-ricing.md`
  asserts "aoide implements no authentication and holds no secret" in two
  places, now superseded by this broker — not a wiki content page under
  this pass's naming convention (a `references/` design doc), left
  unedited; `Full-Architecture.md`'s own tree diagram still calls
  `pkgs/aoide` "one crate today", stale against the landed pi-style crate
  split (secrets included) — unrelated to this pass, left unedited.

## [2026-08-25] refactor | de-slop sweep 1

- Pages touched: `Overview.md` (rewrite — the Aoide-vs-AoideOS distinction
  now stated once, tightly; every concept/entity annotation cut to one line
  ≤20 words), `ingest/index.md` (rewrite — every "answers:" run-on cut to
  one line ≤25 words), `SCHEMA.md` (trim + manifest drift fixes),
  `references/cli/Index.md` (keep + verify; intro now names both binaries),
  `concepts/governance/Wiki-Protocol.md` (trim — the dated "## Decision
  (2026-07-25)" header folded into present indicative; the date lives here
  in the log), `concepts/governance/Governance.md`, `Rebuild-Gate.md`
  (polkit mechanism detail kept intact), `Clone-and-Run.md` (trims).
- Verified against `aoide schema --json` / `lyra schema --json` and the repo:
  aoide 68 commands / lyra 42; the gated trio (`content approve`, `update`,
  `rice declare`); exit codes 0/1/2/64; the content/make/update/onboard and
  `rice declare`/`transpose` stubs; four core gadgets in `AoidePanel.qml`;
  `aoide.a2a.enable`/`aoide.a2a.spawnAgent` present, `aoide.rebuild` absent.
- Claims corrected: Overview's "48-command tree" → 68 (schema-verified);
  `SCHEMA.md` manifest — `references/AOIDE-DEV.md` → `protocol/AOIDE-DEV.md`,
  added `references/AOIDE-VS-LANGCHAIN-HANDOFF.md`,
  `references/fleshing-out-aoide-ricing.md`,
  `references/pantheon/pantheon-grammar.md`; the "Read the whole thing"
  section's `references/` roster fixed the same way and its
  retired-`default`-song grammar paragraph rewritten in present indicative;
  the index's Package-Layout gloss no longer calls the landed crate split
  unbuilt; the index's `aoide-cli` gloss (which listed paint verbs as
  aoide's) replaced by the registry/exit-code summary.
- Out-of-scope drift spotted, not fixed: `concepts/desktop/Widget-Maker.md`
  still says "seven gadgets" (four core at HEAD);
  `concepts/desktop/Feature-Set.md` and `concepts/Full-Architecture.md`
  still cite the 48-command surface; the manifest still lacks
  `concepts/desktop/Controls.md`, `concepts/orchestration/Screen-Control.md`,
  and `concepts/orchestration/Secrets-Broker.md`; repo `AGENTS.md` still
  counts aoide at 48 commands.

## [2026-08-25] refactor | de-slop sweep 2

- Pages touched (all `references/cli/`, KEEP+VERIFY — headnote trims only;
  verb-block shapes untouched): `Graph-and-Conduct.md` (525→528),
  `Rice-and-Livery.md` (521→521), `Screen-Verbs.md` (376→375),
  `Doors-and-Peers.md` (396→397), `Content-and-Hooks.md` (173→172),
  `Meta-and-Upkeep.md` (202→202).
- Verified against `aoide schema --json` / `lyra schema --json` and the
  repo: every verb roster on all six pages (21 graph/conduct verbs, 22
  rice/cover/livery verbs, 14 screen verbs, 16 door/peer verbs, 7
  content/herald/hooks verbs, 7 meta/upkeep verbs); all flags, args,
  gated/implemented status, and exit-code maps; all cited handler file
  paths; the `claude|kimi|pi` agent roster and per-agent submit
  keystrokes; the ~12 s reap timer; MCP `2024-11-05` and AgentCard
  `0.3.0`; a2a bind/port defaults and hardening caps; `PEER_CACHE_TTL_SECS
  = 300`; herald `LEDGER_CAP = 20`; the name regex
  `^[a-z0-9][a-z0-9-]*$`; `usage`'s env vars, OAuth endpoint, and pricing
  fallback; `soundcheck`'s C1–C3 finding classes.
- Claims corrected: `graph send` gained the schema's `--to <name>` flag
  (mutually exclusive with `--id`; `peer/<query>` remote targeting, never
  queued locally) in signature and Notes; the shellbridge handler path
  moved to `conduct/src/commands/shellbridge.rs::handle_shellbridge`
  (P-A2 binary split); dunst herald feed is a single catch-all rule, not
  multiple `skip_display` rules; "the USER gates this step" → "the User".
- Slop removed: headnote contrast framing and history asides ("not the
  user rebuild gate", "previously a write-only dead drop", "historical
  `None`", "former Node engine", "stubs today", "is currently
  flag/env-only", "future work"); one marketing phrase ("eyes-and-hands
  surface"); caps emphasis; multi-claim intro sentences split.
- Claims left unverified (left as-written): exact numeric flag bounds on
  `screen point`/`diff`; env-var value sets (`AOIDE_CONDUCT_AUTOGATE` et
  al.); permit-card field claims; a2a task-state map and JSON-RPC error
  assignments; herald ledger replace-branch semantics; `graph reap`
  staleness constants; deep behavioral Notes inside verb blocks (no
  schema surface contradicts them).
- Out-of-scope drift spotted, not fixed: repo working tree has
  uncommitted `peer add --bearer-secret` / outbound Bearer support (the
  page matches HEAD today; revisit on commit); schema summaries for
  `rice stage` and `rice take list` carry stale/history framing in
  source; `lyra/src/registry.rs:63` test comment cites an 87/48-path
  golden count stale against 68/42; `quickshell.rs:3` doc comment uses a
  real first name; `graph/` crate dir holds `who.rs`/`testutil.rs`
  unlisted in the page's handler roster.

## [2026-08-25] refactor | de-slop sweep 3

- Pages touched: `entities/aoide-cli.md` (277→271, TRIM+VERIFY),
  `entities/aoided.md` (45, KEEP — verified, zero edits per the plan's
  explicit verdict), `entities/shellbridge.md` (136→139, TRIM),
  `entities/livery.md` (205→192, TRIM+VERIFY — naming-lore section cut,
  pointed at `concepts/Lexicon.md`), `entities/Agent-Hooking.md`
  (263→274, TRIM+VERIFY), `entities/Quickshell.md` (219→257,
  TRIM+VERIFY). Three pages grew: verified corrections restored facts the
  prior text got wrong or omitted, and length targets are filler
  ceilings, not deletion quotas (rule 16).
- Headline finding: `entities/Quickshell.md`'s "nine declared, eight with
  a live body" surface-registry claim is wrong at HEAD. Checked
  `modules/facets/quickshell/default.nix`'s `aoide.surfaces` registry
  against `shell.qml` and every `song/songbook/*/widgets/` tree: nine
  surfaces are declared (bar, notifications, launcher, osd, lockscreen,
  greeter, wallpaper, agentWidgets, sessionGraph) but only **five** carry
  a live QML body (bar, notifications — via dunst-as-daemon +
  `herald`/`herald-center`, launcher, wallpaper, agentWidgets). `osd`,
  `lockscreen`, `greeter` are registry-only Stylix stand-down
  declarations with no facet or song QML anywhere in the repo (no
  `AoideOsd`/`AoideLockscreen`/`AoideGreeter` file exists, and none ever
  did per `git log -p`); `sessionGraph` was already correctly noted as
  bodyless. The page's Implementation/Launcher sections were also
  rewritten: there is no `AoideLauncher.qml`/`AoideNotifications.qml`
  facet file — `shell.qml` loads `launcher`/`powermenu`/`herald` through
  `SurfaceSlot` from `song/songbook/sonata/widgets/{launcher,herald}.qml`
  (SurfaceSlot/WidgetSlot's baseline-fallback chain), and the "Notification
  card" section's `song/songbook/sonata/widgets/notifications.qml` path
  was wrong — the real file is `herald.qml`, `notifications.qml` doesn't
  exist. Also fixed: `lyra quickshell reload`'s cited handler path
  (`crates/song/src/commands/shell.rs` → `crates/song/src/commands/
  quickshell.rs`, the real file). Removed the stale `**Status:** …no
  actions/inline-reply support…` line, which the page's own Notification
  card section already contradicted (real approve/deny action buttons
  are documented there). `Overview.md` and `ingest/index.md` both still
  carry the old "nine declared, eight live" phrasing — out-of-scope,
  reported below, not fixed.
- Second finding: `entities/Agent-Hooking.md`'s agent-profile roster was
  stale. `pkgs/aoide/crates/protocol/src/agents.rs` registers three
  profiles (`CLAUDE_PROFILE`, `KIMI_PROFILE`, `PI_PROFILE` — confirmed
  also by `pkgs/aoide/crates/protocol/AGENTS.md`'s own "beyond
  `claude`/`kimi`/`pi`" line), not two. Fixed `known_agents()`'s cited
  roster, the hook-door subheading, the `hook_settings` bullet (added
  pi's `~/.pi/agent/extensions/aoide-pi-session.ts`, declarative), and
  a dangling `(hooks install pi, below)` pointer to a Pi recipe
  subsection that doesn't exist on the page (dropped "below" rather than
  authoring a new subsection — out of scope for a de-slop pass, reported
  below). Also cut the dated "Live-proven 2026-08-20, the three-harness
  probe" status-brag framing (S5/rule 2) down to present-indicative
  behavior, and collapsed the nine-times-repeated identical claude hook
  JSON block to one representative entry plus a one-line note.
- Verified against `aoide schema --json` / `lyra schema --json` and the
  repo (installed binaries report 68/42 commands, matching sweep 1):
  `aoide-cli.md`'s full command-tree table (64 leaves this page tracks +
  `inbox`/`who` = 68), the 20-strong `graph` group, `content approve`/
  `update`/`rice declare` gated flags, `graph focus`'s five failure
  reasons (`session-not-found`/`no-window-address`/`hyprctl-unavailable`/
  `hyprctl-failed`/`window-not-found`, verbatim in
  `crates/conduct/src/graph/window.rs`), the exit-code map, and the
  `schema --json` top-level shape (`aoide`/`commands`/`schemaVersion`/
  `stageNotesVersion`). `livery.md`'s geometry-tier defaults
  (`8`/`6`/`2`/`0`/`true`/`8`/`3`, matching `compositor/default.nix`
  exactly), `SCHEMA_VERSION = "0"`, the four emitters
  (`stage`/`hyprctl`/`osc`/`file`), and the `lyra livery
  lint`/`resolve`/`emit` verb shapes. `shellbridge.md`'s socket path,
  `RuntimeDirectory=aoide`, and the `path = [ pkgs.hyprland ]` unit
  option on both `shellbridge` and `aoide-graph-reap`. `Quickshell.md`'s
  systemd unit settings (`ConditionPathExists`, `StartLimitIntervalSec =
  60`, `StartLimitBurst = 5`, `RestartSec = 3`) and `AoideIpc.qml`'s
  reload contract.
- Repo-ahead-of-installed-binaries note (per the sweep brief): repo HEAD
  (commit `4ce2c96`, already committed, not a working-tree diff) adds
  `--bearer-secret` to both `a2a serve` (inbound bearer, resolved via
  [[Secrets-Broker]]) and `peer add` (outbound bearer to that peer) —
  the installed `aoide`/`lyra` binaries' `schema --json` lack the flag.
  `entities/aoide-cli.md` documents both, sourced from repo, per the
  brief's "repo wins" instruction.
- Claims left unverified (left as-written): `Agent-Hooking.md`'s kimi
  transcript-layout internals (field names, model-ceiling table) and the
  `autogate-sibling` gate-label mechanics — no schema surface
  contradicts them, and re-deriving them needs a live kimi transcript,
  out of scope for this pass.
- Slop removed: S1 em-dash appositive chains (`aoide-cli.md`'s registry
  paragraph, `Agent-Hooking.md`'s hook-event-map sentence), S4 naming
  lore (`livery.md`'s "Why a livery" essay, now one line pointing at
  [[Lexicon]]), S5 status brag (`Agent-Hooking.md`'s dated probe
  writeup, `Quickshell.md`'s stale NotificationServer status line), S7
  marketing register (`aoide-cli.md`'s "An API that happens to be
  typeable" quote and the Hermes-agent/Claude-Code registry comparison),
  S9-style index run-on left in place since `ingest/index.md` is out of
  scope this sweep.
- Out-of-scope drift spotted, not fixed: `Overview.md` line 49 and
  `ingest/index.md` line 46 both still say "nine surfaces declared,
  eight with a live QML body" for Quickshell — now wrong, corrected only
  on `entities/Quickshell.md` itself (out of the six-page scope for this
  sweep); `Agent-Hooking.md` has no `### Pi` per-agent recipe subsection
  though the `PI_PROFILE` and `hooks install pi` are real and referenced
  elsewhere on the page; `AoideIpc.qml`'s own code comment cites `aoide
  shell reload` where the real registered command is `lyra quickshell
  reload` (stale in-repo comment, not wiki content); repo `AGENTS.md`
  still counts aoide at 48 commands (carried from sweep 1, still
  unfixed).

## [2026-08-25] refactor | de-slop sweep 4

- Pages touched: `entities/Melete.md` (119→114, TRIM), `entities/Mneme.md`
  (97→95, TRIM), `entities/Hyprland.md` (29→29, KEEP — one fact fixed),
  `entities/Stylix.md` (KEEP — verified, zero edits), `entities/dxflake.md`
  (KEEP — verified, zero edits), `concepts/desktop/Controls.md` (95→103,
  KEEP+VERIFY), `concepts/song/Ricing-Protocol.md` (140→124, TRIM+VERIFY),
  `concepts/song/Song-Vocabulary.md` (KEEP — verified, zero edits).
- Melete.md / Mneme.md: the muse-lore paragraph in each collapsed to one
  clause + a [[Lexicon]] link (S3 catechism rule) — Lexicon.md is now the
  sole home for the full three-Boeotian-Muses table. "Integrates"/"door"
  restated without the bold-emphasis inflation; an em-dash appositive
  chain in Melete.md's adapter-allow-list paragraph split into two
  sentences; a rule-3-flavored pre-emptive parenthetical ("as if Melete
  were 'my shell, over MCP'") cut to a direct statement.
- Controls.md (KEEP+VERIFY): verified every keybind against
  `modules/dendrites/hyprland.nix` and `modules/dendrites/bash.nix`.
  Corrections: (1) keybinds live in the `hyprland` dendrite, not the
  compositor facet as the page claimed — the compositor facet owns only
  livery-derived appearance; (2) launcher/dock are Hyprland global
  shortcuts (`aoide:launcher`/`aoide:dock`) with no CLI verb involved,
  not `aoide shell launcher toggle` as written; (3) lock moved from
  `SUPER+L` to `SUPER+ESCAPE` (freeing `L` for `movefocus`/`movewindow`
  right, giving full H/J/K/L parity on those two rows) — `SUPER+ESCAPE`
  execs `aoide shell lock`, and no `shell` command group exists in either
  binary's registry, so this is a genuine dead keybind, reported not
  repointed; (4) two undocumented binds added: `SUPER+W` (wallpaper
  picker) and `SUPER+C` (clipboard history); (5) the bar cell interaction
  table and its file pointer were stale — the bar moved from
  `modules/facets/quickshell/qml/AoideBar.qml` to
  `song/songbook/sonata/widgets/bar.qml` (facet→song migration), the
  clock cell sits LEFT not center, volume click now opens the audio
  colonnade (mute moved into it) rather than muting directly, and
  battery/network split into hover-only vs click-only rather than
  sharing one "hover popout" description. `adrebuild`/`adtest`/etc. and
  the shell QoL aliases matched `bash.nix` exactly — zero changes there.
- Ricing-Protocol.md (TRIM+VERIFY): every verb checked against
  `pkgs/aoide/crates/song/src/commands/{rice,mode,draft,cover,livery}.rs`
  and `crates/lyra/src/commands/stubs.rs` — `rice compose`/`rice
  stage`/`rice lint`/`rice mode {status,stage,declarative,draft}`/`rice
  draft {save,list,drop}`/`cover set`/`livery {emit,resolve,lint}` are
  real; `rice declare`/`rice transpose` are registered but
  `implemented: false` (stubs); `rice gen`/`preview`/`adopt`/`mint`/`new`
  do not exist anywhere in the registry, matching the page's existing
  "cut outright" claim almost verbatim against the source comment in
  `rice.rs`. No retired-verb claim survived because none was live to
  begin with. Trimmed the opening scope blockquote (S11 empty framing),
  an em-dash appositive chain, and an aphorism ("'Close' beats
  'identical-on-paper but wrong on screen.'") that added no checkable
  fact. The `wireCyan`/`violet`/`holoBlue`/`glitchPink` role-palette claim
  in the vision-check section was verified live against
  `LiveryState.qml` and current gadget QML — still real, unchanged.
- Hyprland.md (KEEP): one claim failed verification — "consuming
  `aoide.livery` … and touches nothing else" is incomplete; the
  compositor facet's own header states it also reads `aoide.arrangement`
  (house rule 5's full whitelist). Fixed to name both. `github:hyprwm/
  Hyprland` as the flake input, and the `yomi-strix` host lineage,
  verified against `flake.nix`/`hosts/yomi-strix/` and left as-written.
- Stylix.md / dxflake.md / Song-Vocabulary.md: every checkable claim
  verified against the repo (surface-ownership assertion in
  `lib/checks.nix`, the `hyprwm/Hyprland` input, `lib/mkHost.nix`'s
  walker + song-guard pattern matching `dxflake`'s "hosts know dendrites,
  dendrites never know hosts" line verbatim, the `noSongRead` boundary
  covering only `stage/`/`auditions/`) — nothing failed, zero edits made
  to any of the three pages. dxflake.md's claims about the external
  `dxcently/dxflake` repo's own internals (aggregation role names, the
  walker's exact filter) are outside this repo and were left as-written,
  unverified (prior-art citation, not a Aoide-repo fact).
- Claims left unverified: none outright — every command/path/flag claim
  touched this sweep resolved against the repo one way or the other.
- Out-of-scope drift spotted, not fixed: `concepts/Lexicon.md` still
  spells out "integrated (not vendored)" for both Melete and Mneme
  (lines 19–20) even though this sweep's plan designates
  `entities/Melete.md`/`entities/Mneme.md` as that phrase's sole home —
  Lexicon.md was not in scope to edit; rice verbs live in the `lyra`
  crate per the binary split (P-A5) but no `lyra-cli` entity page exists
  in the wiki, so Ricing-Protocol.md's `[[aoide-cli]]` link for `rice
  gen`'s history points at a page that no longer owns that command
  group; `modules/dendrites/hyprland.nix`'s own in-repo comments claim
  `aoide shell launcher`/`aoide shell dock` are "unimplemented stub[s]"
  when no such command is registered as a stub or otherwise anywhere in
  either binary's registry (a stale code comment, not wiki content, but
  the root cause of the Controls.md launcher/dock drift this sweep
  fixed).
