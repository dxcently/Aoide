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

### [2026-08-29] open: command-count drift — core 82 vs 80, lyra 43 vs 48
`concepts/cli/CLI-Reference.md`, `concepts/cli/Meta-and-Upkeep.md`, and
`concepts/Package-Layout.md` still say core holds 82 commands. The P-PV2
pairing-surface collapse (`peer.invite`/`peer.pair.request` retired into one
smart-target `peer.pair`; `peer.pair.pending` renamed `peer.pending`) landed
it at 80 (`pkgs/aoide/crates/cli/src/registry.rs`'s golden-snapshot
comment is ground truth); `entities/aoide-cli.md` already reflects 80, those
three pages don't. `concepts/Codebase.md`'s vm-boot tripwire figure (82,
[[AOIDE-DEV]] §7) is unverified against the actual hardcoded assertion in
`lib/vmTest.nix`. Separately, `entities/lyra.md`'s
command-surface table and `concepts/Full-Architecture.md`'s itemized lyra
paragraph (in "The control plane") are both frozen at 43 — the `onboard`
checkpoint (`pkgs/aoide/crates/lyra/src/registry.rs`'s golden comment) —
never updated through `secrets ask` (44), `element seed` (45), `pair ask`
(46), `pair confirm`'s revert (47), or `quickshell healthcheck` (48, see the
entry below). `Package-Layout.md`'s lyra breakdown ("41 real, 2 stub") also
misstates the stub count: `rice transpose` is lyra's one remaining stub per
`entities/aoide-cli.md`. Both tables need a full itemized reconciliation
against the current registries, not a bare-number patch — left as found,
not fixed, while landing the quickshell-healthcheck sweep below.

**Closed 2026-08-29** (see the command-count reconciliation entry below):
every core-82 and lyra-43 citation swept to 80/48 across `CLI-Reference.md`,
`Meta-and-Upkeep.md`, `Package-Layout.md`, `Codebase.md` (both the vm-boot
tripwire figure and the per-crate golden-test figure, cross-checked against
the actual `assert cmd_count == 80` in `lib/vmTest.nix`), `Doors-and-Peers.md`,
and `Full-Architecture.md`. `entities/lyra.md`'s command-surface table and
`Full-Architecture.md`'s itemized lyra paragraph both gained the five rows
that took lyra 43 → 48 (`secrets ask`, `element seed`, `pair ask`, `pair
confirm`, `quickshell healthcheck`) and now sum to 48. `Package-Layout.md`'s
stub count is fixed to "47 real, 1 stub" (`rice transpose` only — `rice
declare` is real, gated code, not a stub) and its `song`/`lyra` crate rows
gained the same five commands.

### [2026-08-29] open: `who`/`session undying` naming drift under the count fix
Found while re-verifying `aoide schema --json` for the command-count sweep
above — a naming drift, not a count drift, so left unfixed here. Live
`aoide schema --json` has no `who` command and no `session undying`
command; `lib/vmTest.nix`'s own bump-history comment explains why:
"Session-surface redesign (command-defrag lane X): `session.undying`
REMOVED, absorbed into `session.grant`'s positional `<kind>` grammar;
`session.grant` ADDED; `who` REMOVED, folded into bare `session`/`session
--hosts`." The bare-total counts these pages state (80 for core, 73 real)
are unaffected — the redesign is a net wash — but `entities/aoide-cli.md`
(command tree + the "further commands" enumeration), `concepts/cli/
Doors-and-Peers.md` (documents `aoide who` in full, with its own flags and
output shape), `concepts/Package-Layout.md`'s `conduct` crate row, and
`concepts/Full-Architecture.md`'s core command paragraph all still name
`who` as a live command and never mention `session grant`. `concepts/
Package-Layout.md`'s `client` crate row is separately stale in the same
family: it lists `a2a.agent.*` (4) — a family `lib/vmTest.nix` records as
deleted outright — and `peer.*` (5), where the live `peer` group is 15
paths. None of this was in scope for a count-only pass; a content sweep
against the actual `session grant`/`peer`/`who` surface is queued here.

### [2026-09-07] open: the mesh speaks two wire vocabularies
yomi-strix runs 0.0.22, which sends `X-Aoide-Node` and stamps session
origins `node:<name>`; osaka (0.0.21), sakaki (0.0.13) and chiyo answer the
old `X-Aoide-Peer`. Cross-box calls out of yomi fail at the header until
each box switches. Reading a pre-switch session record is already covered —
`aoide_storage::attest::is_node_origin` accepts both prefixes, and the
secrets broker's origin gate depends on that — but the request header has
no such shim by design; a flag day was the ruling. Close when every box has
switched, and retire the legacy arm then. sakaki's switch is the TOTP-gated
cross-host path, which is not built; chiyo has been dark since 08-28.

### [2026-09-07] open: an audit record's `untrusted_data` is unbounded
`append_audit` clamps `AuditRecord.message` to 512 bytes, so a command
whose printed output is also its outcome message can no longer copy a
whole letter into `~/.aoide/log`. The sibling field is not clamped: a
notification record carries its forwarded app title and body into the
same log verbatim, at whatever length the sending app chose. The house
rule that forwarded text is data, never a command, is upheld — nothing
interprets it — but an unbounded second copy of it lands in a log with
no pruning, which is the same defect the message clamp closed. Whether
`untrusted_data` takes the same 512-byte clamp, a larger one, or a
per-field bound set where the record is built is undecided.

### [2026-09-07] open: the door's audit calls a refusal a success
MAIL.md §Wire rules that admission is a JSON-RPC error and everything
after it an outcome, and justifies the split this way: an admission
failure returned as a 200 result "would audit as `ok` through the door's
own error heuristic, and an audit that files a refusal as a success is
worse than a blunt error code." Steps 2 onward are ruled into results,
and two of them — `bad-msgid` and `unverified-origin` — are refusals.
The principle therefore condemns the shape the same section prescribes.
The A2A door's generic per-request heuristic keys on a JSON-RPC `error`
member, so a refused deposit, now a 200 carrying
`{"result":{"status":"refused"}}`, logs `ok` — against the heuristic's
own stated intent of recording the logical outcome rather than the HTTP
line. `mail_deposit` writes its own audit line above the match,
labelling both outcomes `invalid`, so nothing is unrecorded; the log
instead holds two lines for one event under one command string,
disagreeing. Undecided: whether the generic heuristic learns the refused
shape, whether a handler reports its logical verdict out of band so the
door labels from the method rather than the payload, or whether MAIL.md
is the thing that changes and these two outcomes return to being errors.

### [2026-09-08] open: the ChatGPT payload is fetched from a mutable URL
`modules/dendrites/openai.nix` overrides the `chatgpt-desktop-linux`
flake's pinned source because the hash upstream records no longer
matches what the URL serves. The dendrite fetches the current DMG and
rewrites the install phase's reference to the stale derivation,
discarding and reattaching string context so the old path does not
follow the substitution into the closure. This works as long as the URL
serves exactly one payload; the next silent replacement breaks the build
again, with a hash mismatch that names upstream's fetch rather than
ours. Close when upstream re-pins, or when the payload is mirrored to
something content-addressed. Evaluated, not built — the substitution is
unproven against a real fetch.

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
- **`concepts/Notes.md` retired and merged into [[livery (rename to lyra)]]** (khoa steer: "notes should be livery in the concepts"). Both pages already asserted that "notes" and "livery" are *one thing, not a values-vs-engine split* — so keeping a `Notes` concept beside a `livery` entity re-created exactly the split the pages disclaimed. The token layer now has **one page**. Folded into `entities/livery.md`: the score/performance seam framing, "every facet consumes livery and nothing else", the two-fan-outs/one-source diagram + the zero-drift guarantee, the tier-structure table, prior-art (wrap-don't-rewrite), the provisional-v0 note, and Stylix overlap resolution.
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
- **A geometry tier joins the palette and component tiers.** `aoide.livery.geometry` (`modules/nucleus/options.nix`) carries `gapsOut`, `gapsIn`, `borderSize`, `rounding`, `blurEnabled`, `blurSize`, `blurPasses` — every field `nullOr`. The compositor facet reads the tier directly and falls back field-by-field to its existing opinionated defaults (`8`/`6`/`2`/`0`/`true`/`8`/`3`) for anything unset, so a song that sets no geometry produces the same `hyprland.conf` as one that omits the block. The tier sits outside the standalone Node note package's own schema — `livery lint` never validates it; it rides `stage/livery.json` unvalidated, at the same additive-optional `schemaVersion "0"` posture as the base16 block. Documented in [[livery (rename to lyra)]] and [[Self-Ricing]].
- **`rice preview` now applies geometry and border colours to the running compositor.** A new `pkgs/aoide/src/hypr.rs` builds one `hyprctl --batch` `keyword` list, in a fixed order (gaps → border size → border colours → rounding → blur), emitting a keyword only for a field that actually resolves — an unset geometry field is skipped, not defaulted, so the call never fights a host's baked config or a user's own out-of-band tweak. The call is a no-op off Hyprland (guarded on `HYPRLAND_INSTANCE_SIGNATURE`) and never fails the preview outcome; it never runs `hyprctl reload`, since every field it touches is live-settable via `keyword` and a reload would re-read the baked config from disk, discarding anything else live on the compositor. This sharpens the rice loop's truth/sketch split: `aoide.song` selecting a song and rebuilding is the truth (bakes `hyprland.conf`, themes every nix-manageable app via [[Stylix]], deploys the song's widgets, sets the boot default); `rice preview` is the sketch — a live compositor call plus a `stage/livery.json` write, no rebuild.
- **`aoide rice mint <name>` scaffolds a new committed song directly.** (`--from <song>` · `--force` · `--json`; alias `rice new`; `gated: false`.) It writes `song/songbook/<name>/rice.nix` (a self-gating `lib.mkIf (config.aoide.song == "<name>")` block copying the `palette`/`window`/`geometry` tiers from `--from`, defaulting to `default` — the only `.nix` file the scaffold writes, so the folder satisfies the `song-shape` check unaided), a `livery.json` mirror of the same values, an honest-empty `design/intent.md` that points at the widget-slot catalog and the update playbook rather than fabricating design rationale, and `widgets/.gitkeep`. `name` and `--from` are validated against a strict `^[a-z0-9][a-z0-9-]*$` pattern (rejecting path traversal); rendering a copied livery value into `rice.nix` escapes `$` and quotes attribute keys, so a value containing `${…}` cannot round-trip into live Nix interpolation. Being an ordinary schema command puts it in `schema --json` and the MCP tool list, so an agent bootstraps a song through the same door a human would. The command tree grows to **38 leaves**. Documented in [[aoide-cli]] and [[Self-Ricing]]; command counts refreshed across `Overview.md`, `ingest/index.md`, [[Full-Architecture]] (master map + I/O table), and [[Feature-Set]] (two "N-command surface" mentions). `lib/vmTest.nix`'s own hardcoded Python assertion still reads 36 (unchanged since before `cover set` landed, per the 2026-07-30 entry above) — now three behind the real 38; left as-is, same live code/test mismatch, not a wiki drift.
- **The colour invariant holds, audited.** Every rice's colours derive from [[Stylix]] plus the livery base16 palette: the stylix facet bakes one base16 scheme from `aoide.livery.base16` (or synthesizes it from the four palette anchors when a song sets no explicit base16 block), and the live Quickshell notes derive from the same `aoide.livery` value, so the baked and live halves cannot disagree. The known departures from "everything reads `notes.*`" are verified deliberate, not drift: the `*Preview.qml` files are standalone screenshot scaffolds that hardcode palette values to simulate `notes.*` off the live tree; `AoideBar.qml`'s `#000000` is structural staff-ink, not a themed surface. A small number of `#14141a` literals remain unconverted to a note role — noted, not touched this pass.
- **A song's substance is already plain files.** A rice's `livery.json`, `widgets/*.qml`, and `design/` folder are ordinary files read by `aoide`, Quickshell, and `hyprctl` — none of them require Nix to exist or apply. Nix's remaining jobs on a rice are generating `hyprland.conf`, theming every non-Quickshell app via Stylix, deploying the QML tree, and choosing the boot default via `aoide.song`.
- Four open questions from this pass are carried as Open Threads above rather than written into any page: per-slot host presence (no toggle exists — presence is anchor existence), the proposed `palette-usage.md`, boot-time seeding of `stage/livery.json` from the selected song, and a `commands/` registry + `graph.rs` domain split sketched for `pkgs/aoide`.
- Grounded by reading the repo directly (`modules/facets/quickshell/default.nix`, `modules/facets/quickshell/qml/slots.md`, `modules/nucleus/options.nix`, `modules/facets/compositor/default.nix`, `pkgs/aoide/src/hypr.rs`, `pkgs/aoide/src/dispatch.rs`, the standalone Node note package's `src/schema.js`), not a commit hash — this work sits in the working tree.
- Pages updated: [[Widget-Maker]], [[livery (rename to lyra)]], [[Self-Ricing]], [[aoide-cli]], `Overview.md`, [[Full-Architecture]], [[Feature-Set]], `ingest/index.md`. No pages added or removed; `SCHEMA.md`'s manifest is unchanged.

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
- **`stage/livery.json` is seeded from the active song on activation.** `modules/facets/quickshell/default.nix`'s `home.activation.aoideSeedStage` writes the active song's committed `song/songbook/<song>/livery.json` (with the `"song"` field injected, matching what `rice preview` stages) into `song/stage/livery.json` on every activation, write-temp-then-rename. A host that boots without ever previewing now carries a correct live stage twin from the baked default. Documented in [[Codebase]] (Runtime contracts), [[livery (rename to lyra)]], and [[Self-Ricing]]; already reflected in `CONTRACTS.md` §4.
- **The QML deploy moves out of the repo root.** The prior `home.file` symlink tree at `~/Aoide/qml` is gone; `home.activation.aoideDeployQml` rsyncs (`-a --delete`) the built config tree into the gitignored `~/Aoide/run/qml/`, and `aoide-quickshell.service` reads `run/qml/shell.qml`. Widget source stays at `modules/facets/quickshell/qml/`; the repo root carries no `qml/` directory (the stale untracked copy is removed). Documented in [[Quickshell]] and [[Full-Architecture]].

Grounded by reading the repo directly at commits `62121a4` (commands registry), `16b65d6` (graph/ split), `99a8447` (stage seed), and `252c3d4` (run/qml deploy) — no `source:` field added (source is the repo, not a `references/` doc).

Pages updated: [[aoide-cli]] (command-tree section rewritten around the registry), [[Codebase]] (graph-domain test paragraph, stage-file seeding note), [[Self-Ricing]] (status paragraph), [[livery (rename to lyra)]] (stage seeding note), [[Quickshell]] (deploy mechanism rewritten, two mentions), [[Full-Architecture]] (deploy mention), `ingest/index.md` (aoide-cli, Codebase, Quickshell glosses). No pages added or removed; `SCHEMA.md` manifest unchanged (same file set, `updated:` bumped on the six touched pages).

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

Pages updated: [[livery (rename to lyra)]] (renamed entity), SCHEMA.md, Overview.md, `ingest/index.md`, [[Full-Architecture]], [[Codebase]], [[Lexicon]], [[Snowflake-Anatomy]], [[Package-Layout]], [[Song-Anatomy]], [[Song-Vocabulary]], [[Ricing-Protocol]], [[Self-Ricing]], [[aoide-cli]], [[Quickshell]], [[Stylix]], [[shellbridge]], [[Hyprland]], [[Widget-Maker]], [[Widget-Bridge-Contract]], [[Feature-Set]], [[Gadget-Dock]], [[Desktop-Architecture]], [[Session-Graph]], [[Terminal-Commander]], [[Conductor-3D-DAG]], [[Agent-Interface]], [[Fork-and-Run]], [[Governance]], `protocol/AOIDE-DEV.md` (mentions only — §7 flags untouched in structure), `protocol/OPERATIONS/{Assertion,Wikilinks}.md`. No pages added or removed; manifest file count unchanged.

## [2026-08-13] refactor | livery merge Phase 4 — transition window closed (working tree, uncommitted)

The dev workstream executed Phase 4 of the livery merge (LIVERY-MERGE.md): the transition window is closed and the compat scaffolding is gone.

- **Mirror write dropped** — `aoide rice preview` no longer writes the legacy stage mirror (both the write and its error arm are gone), and the quickshell facet's `aoideSeedStage` seed script writes only `song/stage/livery.json`. The legacy mirror file (gitignored runtime state) is deleted.
- **Fallback reads dropped** — the conductor's `stage_notes_path` returns `dir/livery.json` unconditionally (no existence probe); the staged no-arg reads in `rice.rs` `resolve_rice_notes` and `commands/livery.rs` `resolve_notes` retarget to `livery.json`; `conductor/src/ui.rs` status row reads `livery.json`; the QML singleton's legacy FileView fallback is removed.
- **Option alias dropped** — the `lib.mkRenamedOptionModule` transition alias is deleted from `modules/nucleus/options.nix`; the pre-merge option namespace no longer evaluates.
- **QML file rename** — the QML state singleton is renamed `LiveryState.qml` (git mv), including the two code instantiations (`shell.qml`, `ConductorPreview.qml`) and every comment reference across the QML tree.
- **Comment sweep** — stale pre-merge-name mentions updated to `livery` across `song/song.md`, the sonata/default design docs, `CONTRACTS.md`, `docs/BUILD.md`, the wiki ([[livery (rename to lyra)]] transition-window lines to past tense, [[Quickshell]], [[Full-Architecture]], [[Widget-Maker]], `AOIDE-DEV.md` §7 flag → [landed + switched], `.obsidian/workspace.json`), `modules/nucleus/shellbridge.nix`, `modules/dendrites/hyprland.nix`, and `hosts/yomi-strix/default.nix`. `schema --json` changes by exactly one line (the `livery.lint` summary drops its parenthetical).
- **Dated entries below are naming-normalized** — facts untouched (dates, commits, what landed); the token layer is spelled livery throughout, per khoa's 2026-08-13 ruling.

Gated: `cargo test --workspace` green; yomi-strix toplevel build green; `nix flake check` incl. vm-boot green; `qs -p shell.qml` loads (LiveryState resolves). Committed and switched 2026-08-13, per house rule 2.

## [2026-08-14] update | `rice stage`/`rice compose` renames + the staging/declarative mode toggle (commit `e1b24b1`)

Two CLI renames plus one new feature, all landed and workspace-test-verified in `e1b24b1` (352 tests green, `nix build .#aoide` green — grounded by reading `crates/song/src/commands/{rice,mode}.rs`, `crates/storage/src/mode.rs`, and the commit diff directly, not re-verified live on yomi-strix, which still runs the pre-commit binary).

- **`rice preview` → `rice stage`; `rice mint` → `rice compose`.** Full renames, not aliases — the old spellings are unknown commands now, same as a typo. The CLI also drops its one pre-existing alias (`rice new` → `rice mint`): **no CLI-internal aliases** is now a standing contract-level rule (one spelling per command), stated as such in [[aoide-cli]]'s conventions section rather than as a one-off rename note.
- **`stage/mode.json` — the staging/declarative mode toggle.** A new gitignored stage-file (same category as `stage/design.json`) records whether `rice stage`/`cover set` may write live (`staging`) or must refuse (`declarative`, the default — absent file reads as declarative). `rice mode status`/`stage [<name>]`/`declarative [<name>]` are three new command leaves. The enforcement is two entrypoint guards (`handle_rice_stage_entry` in `rice.rs`, `handle_cover_set_entry` in `cover.rs`), not a background reconciler — `rice stage`/`cover set` are the only writers of those two stage files anywhere in the codebase, so guarding both entrypoints is a complete guarantee; `aoided` stays a one-shot skeleton with no event loop either way. `rice design enter`/`exit` (the separate, pre-existing design-mode marker) is unaffected — it calls the same underlying staging logic guard-free, and the two markers are independent today (not a decided relationship, just the current state).
- **Command tree: 51 → 54 leaves** (the three `rice mode` commands; the two renames don't change the count).

Pages updated: [[Self-Ricing]] (status paragraph, the rice-loop diagram, "Minting a song" → "Composing a song", new "Staging vs Declarative Mode" section), [[aoide-cli]] (module list, leaf table + new `rice mode` row, `rice compose`/`cover set` prose, the no-aliases rule, leaf count), [[Full-Architecture]] (stub/real callout, the rice-loop prose + diagram, the command-count paragraph), [[Codebase]] (stage-file bullet, the stub-vs-real callout, plus a separate unrelated staleness note below), [[livery (rename to lyra)]] (zero-drift paragraph, the geometry-tier live-apply paragraph), [[Widget-Maker]] (hot-swap paragraph), [[Package-Layout]] (two `management`-crate table cells), [[Fork-and-Run]] (onboarding step 9), [[Song-Anatomy]] (stage-file table row), `Overview.md` (aoide-cli gloss), `ingest/index.md` ([[Self-Ricing]] and [[aoide-cli]] glosses), [[Stylix]] (the unbuilt `--gallery` flag's status line), [[Melete]] (the extension-engine loop's verb). `protocol/AOIDE-DEV.md`: §3 mechanics table + a new "staging can be locked" bullet, the cover-derivation bug flag, the per-song-widgets open item, and the §8 quick-reference row named directly in the brief. This `## Open Threads` section: the 2026-07-26 "post-skeleton build backlog" thread's `rice preview` line struck closed (real since, renamed).

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

The QML injected-prop contract every song widget declares (`WidgetSlot.qml` injects `{"livery": …, "bridge": …}`; all fourteen sonata widgets declare `required property var livery`) and the CLI's user-facing `livery lint/resolve/emit` summary strings (`crates/song/src/commands/livery.rs`) both moved from `notes` to `livery` — the same rename [[livery (rename to lyra)]]'s page already carries at the *concept* level from 2026-07-28; this pass is the literal property/CLI-text catch-up. `3d2b073` itself already swept four wiki pages ([[Full-Architecture]], [[Widget-Maker]], [[Quickshell]], `protocol/AOIDE-DEV.md`). This pass found and fixed the remainder: `entities/livery.md`'s "Verbs" section still described the CLI as validating a "note container"/printing a "note set"/defaulting to "the staged notes" — reworded to "livery file"/"livery set"/"the staged livery" to match the CLI's own current summary text (verified against `livery.rs`'s diff in `3d2b073`, which made the identical substitution in the Rust source); `ingest/index.md`'s [[shellbridge]] gloss said the stage files carry "notes, sessions … " — corrected to "livery, sessions …".

**Deliberately left untouched** (the musical sense, not the property): [[Lexicon]]'s "liner" gloss, [[Quickshell]]'s "note-glyph"/note-theming UI vocabulary, [[Widget-Bridge-Contract]]'s "smaller notes joined … beam" beaming description, [[Content-Pipeline]]'s "design notes" dogfood section, `entities/livery.md`'s own `aliases: [aoide-notes, Notes, notes package, note engine]` (resolves historical `[[Notes]]` mentions in this log), and every dated entry in this log that legitimately says "notes" as the name that was true on that date — falsifying a record to match a later rename would be the wrong repair, same reasoning `3e6e7be`'s own commit message gives for leaving sonata's `hazards.md`/`intent.md` alone.

## [2026-08-19] refactor | A2A bearer-token admission gate (`23f0a0e`)

`message/send` was origin-blind on the Spawn path and loopback-trusting unconditionally on Inject — behind any reverse proxy or tunnel (`ssh -R`, a tailscale funnel, cloudflared, nginx) the server sees the proxy's own loopback address for every caller, so a remote attacker inherited that trust and, on Spawn, could launch the operator's configured agent with zero gate beyond the command being non-empty. Closed with `aoide.a2a.tokenFile`: `classify_origin` → `PeerOrigin::{Loopback,Remote,Unknown}`; `classify_token` → `TokenState::{Absent,Valid,Invalid}`; `spawn_authorized(token_configured, token_state)` refuses a spawn with `-32005` unless a configured token validates; `effective_origin` demotes a bad-token caller — loopback included — to `Unknown` before `should_deliver_now` sees it; per-peer identification moves off address too via `Peer.tokenFile` (`state/peers.json`, `peer add --token-file`) and `aoide_storage::peer_store::{token_bytes_eq,is_autogated_peer_token}` (length-independent compare). Empty `tokenFile` (the default) is byte-identical to pre-amendment behavior, pinned by dedicated regression tests. `CONTRACTS.md` §6 carries the full amendment text (dated 2026-08-19, alongside the 2026-08-14 non-loopback pending-gate amendment it revises).

Wiki: [[A2A-Door]]'s "Security and governance" section rewritten — the old "loopback is unconditional" bullet is gone, replaced with the token mechanism, Spawn's new gate, and the loopback-coupling rule; [[Peer-Federation]]'s "Security — the non-loopback pending-gate amendment" section updated to match (the `Loopback` bullet now carries the "only while no server-wide token is configured" condition, and autogate is now the OR of address-match and per-peer-token-match), plus its registry section/CLI surface gain the `tokenFile`/`--token-file` field; `entities/aoide-cli.md`'s `peer add` bullet gains the flag and a pointer to the security section. `ingest/index.md` glosses updated for both pages. `updated:` bumped on all four.

**Backlog note:** this wiki's last log entry before this pass was dated 2026-08-14 — a large backlog of commits landed between then and now (`git log` shows dozens) remains unlogged and un-ingested; out of scope for this pass by explicit instruction, flagged here so it is not silently forgotten.

## [2026-08-19] add | dev CLI reference — `references/cli/` (83 commands)

Minted `references/cli/` — the dev-facing command I/O reference the shape reserves `references/` for: a hub ([[concepts/cli/Index|cli/Index]]) plus six group pages ([[Rice-and-Livery|Rice-and-Livery]] 22 verbs, [[Graph-and-Conduct|Graph-and-Conduct]] 17, [[Screen-Verbs|Screen-Verbs]] 14, [[Doors-and-Peers|Doors-and-Peers]] 15, [[Content-and-Hooks|Content-and-Hooks]] 7, [[Meta-and-Upkeep|Meta-and-Upkeep]] 8), each command carrying signature, files read, files written, and where output pipes to — verified against the Rust source (`pkgs/aoide/crates/`) and `aoide schema --json`, not prose memory. Full coverage of the 83-leaf schema confirmed by a skim review; stubs (`rice declare`/`transpose`, the five `content` verbs, `make`/`update`/`onboard`) documented as contract surface only. The pages flag two source-vs-binary discrepancies (the installed binary predates the `--token-file` flags) and mark the few claims unverifiable in source (`rice transpose`'s `palette/` dir, the content registry path, who drains `stage/pending.json`) instead of inventing. SCHEMA manifest + index updated.

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

## [2026-08-25] refactor | de-slop sweep 5

De-slopped the six orchestration concept pages
(`concepts/orchestration/{Session-Graph,Conductor-Channel,
Terminal-Commander,Agent-Interface,Loop-Protocol,Content-Pipeline}.md`),
architectural depth kept — mechanisms/invariants/failure modes stay, only
filler goes.

- Session-Graph.md (TRIM+VERIFY): cut the S5 status-brag opener line
  ("Verified green…") that also carried a stale implementation path
  (`pkgs/aoide/src/graph.rs`; the real location is
  `pkgs/aoide/crates/conduct/src/graph/` + `.../src/reap.rs`). Verb roster
  was missing `permit` (`graph permit` — a herald permission-card verb, real
  per `commands/graph.rs`) — added, with a one-line description. Verified
  the liveness reaper against `crates/conduct/src/reap.rs` at HEAD: it now
  fires on THREE independent signals (window gone / pid gone / sustained
  staleness past a state-dependent band — 72h at rest, 7 days mid-turn, 2h
  for a stranded subagent), not the two the page described; added the third
  signal and the window-owner-map veto that makes it safe. Fixed the
  same-window duplicate keeper-rank order — the page said "the one with a
  real on-disk transcript" wins; the code (`superseded_agent_duplicates`)
  ranks newest `startedAt` first, transcript second. Fixed the
  registration-time eviction carve-out — the page said it spares only the
  new record's own `parentSessionId`; per `pkgs/aoide/crates/conduct/
  CLAUDE.md` and `CONTRACTS.md` §4 (task #89) it now spares the record's
  WHOLE lineage (every ancestor and descendant via `parentSessionId`), and
  a nested headless session is windowless-by-lineage so it can no longer be
  mistaken for the enclosing terminal's duplicate. Added the `hookAncestry`
  automatic-parenting fact (CONTRACTS.md §4) to the graph model and
  contracts sections, and folded it into the "Open seams" note on
  spawn-time parenting. Trimmed S1/S6 in the intro and viewer section.
  267 lines (was 246) — net growth is the three corrected/added facts above,
  not filler; every added line is load-bearing per the VERIFY note in the
  sweep brief.
- Conductor-Channel.md (TRIM+VERIFY): split every line over 250 chars (the
  worst was 1231) into normal prose width without cutting substance; removed
  the scattered inline "(SHIPPED)"/"(SHIPPED, 2026-08-20)" status tags in
  favor of the page's single top-of-page **Status:** line (S8) and present
  indicative throughout. Verified `AOIDE_CONDUCT_AUTOGATE`/
  `AOIDE_CONDUCT_SIBLING_AUTOGATE`/`AOIDE_NO_CONDUCT` and the kitty-wrapper
  branch order against `pkgs/aoide/crates/conduct/src/graph/send.rs` and
  `modules/dendrites/kitty.nix` — all real, unchanged. Added the `--to
  <name>` flag on `graph send` (local id/tail4/petname or `peer/<query>`,
  folding a remote send through A2A) — present in `commands/graph.rs`'s
  registered summary but absent from the page; linked to [[Peer-Federation]]
  rather than duplicating its contract. 142 → 308 lines; the growth is
  entirely line-wrap of previously-unwrapped giant lines, not new prose.
- Terminal-Commander.md (TRIM): softened marketing framing ("flagship
  shipped widget" → "shipped widget", "cleanest exemplar" → "exemplifies")
  and collapsed a doubled [[Lexicon]] cross-ref (S6) in the herdr paragraph.
  Fixed a verified factual error: the A2A task-state string is
  `auth-required` (kebab-case, per `crates/server/src/a2a.rs`'s
  `a2a_task_state` and its own test assertions), not `AUTH_REQUIRED` as the
  page had it.
- Agent-Interface.md (TRIM+VERIFY): verified the four-tier guidance ladder
  against the repo root `AGENTS.md` — matches; compressed the page's
  restated copy of the tier list to a pointer (S12), keeping only the
  wiki-side Tier-3-connector elaboration AGENTS.md doesn't carry. Fixed a
  real drift: "One implementation, three doors" described the `protocol`
  crate as a "target blueprint" that "would hold" the registry/schema/door
  types (future tense) — `pkgs/aoide/crates/protocol/` already exists with
  exactly that content (`registry.rs`, `door.rs`, confirmed against
  `concepts/Package-Layout.md`'s own crate table, which already lists
  `protocol` as `landed`). Rewrote to present indicative. Normalized
  `protocol::agents` → `aoide_protocol::agents` (the real crate name).
- Loop-Protocol.md (TRIM): already close to the style standard; split two
  S1 em-dash-chain sentences (the "Framing" intro's three-item dash list,
  and the R1-universally-reachable sentence) into a lead sentence + trailing
  list / two sentences. No factual claim changed — `graph spawn`/`graph
  send`/`graph pending`/`graph view` all re-verified live in the registry.
- Content-Pipeline.md (KEEP): re-verified all five verbs (`content
  register/propose/approve/ingest/query`) against
  `pkgs/aoide/crates/cli/src/commands/stubs.rs` — `implemented: false` on
  all five, confirming the page's "exit-64 stub" status claim exactly.
  Nothing failed verification; zero edits made.
- Claims left unverified: none outright — every command/path/flag/behavior
  claim touched this sweep resolved against the repo one way or the other
  (per the brief's note that the graph/conduct area is mid-fix, `reap.rs`
  and the eviction carve-out were re-read fresh at HEAD rather than reused
  from an earlier pass).
- Out-of-scope drift spotted, not fixed: `concepts/Peer-Federation.md` (not
  in this sweep's page list) documents `peer add`/`peer pull`/`peer status`
  but not the `graph send --to peer/<query>` cross-instance send path that
  `commands/graph.rs` already registers — a gap for a future pass on that
  page. `concepts/A2A-Door.md` was not re-verified this sweep for the
  `auth-required` task-state string now fixed here in Terminal-Commander.md
  — worth a matching check there. The Session-Graph reaper's newly-verified
  depth (pre-boot-ghost detection, orphaned-subagent sweep, orphan-socket
  sweep, and the transient-read grace during a compositor reload/restart)
  goes beyond what this trim pass folded in — `crates/conduct/src/reap.rs`'s
  module doc comment has the full shape if a future ingest wants it.

## [2026-08-25] refactor | de-slop sweep 6

De-slopped the five orchestration-B concept pages
(`concepts/orchestration/{A2A-Door,Peer-Federation,Secrets-Broker,
Screen-Control,Conductor-3D-DAG}.md`), architectural depth kept — mechanisms,
invariants, failure modes, and wire contracts survive verbatim in meaning,
only filler goes.

- A2A-Door.md (TRIM+VERIFY): verified and documented task #84's inbound
  bearer mechanism — `a2a serve --bearer-secret <name>` /
  `AOIDE_A2A_BEARER_SECRET` resolves a secret through the local
  [[Secrets-Broker]] as consumer `a2a-door`, fresh per connection (never
  cached, unlike the pre-existing `tokenFile`), fails CLOSED with a
  per-call sentinel on a broker-resolve failure, and takes precedence over
  `tokenFile` when set (`crates/server/src/a2a.rs`'s `resolve_bearer_secret`/
  `resolve_inbound_bearer`/`resolve_failure_sentinel`). No nix option wires
  it yet — CLI flag/env-var only. Added the matching outbound half —
  `peer add --bearer-secret <name>` resolved as consumer `a2a-client`
  (`crates/client/src/commands.rs`'s `resolve_peer_bearer`) — as a short
  cross-link to [[Peer-Federation]] rather than duplicating its contract.
  Re-verified the A2A task-state strings are lowercase-kebab
  (`auth-required`, `input-required`) against `a2a_task_state` and its test
  assertions — already correct on this page (sweep 5 fixed the sibling
  Terminal-Commander.md instance and flagged this page as unchecked; now
  checked, no fix needed). Split two S1 em-dash/comma-chain sentences (the
  `a2a serve` server description, the `message/stream` bullet) to one claim
  each. 166 → 182 lines; growth is the two verified bearer-mechanism
  additions, not filler.
- Peer-Federation.md (TRIM+VERIFY): added the `graph send --to
  peer/<query>` cross-instance send path sweep 5 flagged as missing here —
  verified against `commands/graph.rs`'s registered `--to` flag,
  `graph/send.rs`, and `storage/src/addr.rs`'s tier-5 `peer/<rest>`
  resolution; new "Sending across the fold" section, cross-linked to
  [[Conductor-Channel]] and [[A2A-Door]]. Added the `bearerSecret` field to
  the `state/peers.json` registry section (task #84's outbound half,
  `peer_store::Peer.bearer_secret` — the previously-undocumented opposite
  number to the existing `tokenFile` field) and the `--bearer-secret` flag
  to the CLI-surface line. Verified `CONTRACTS.md` §7 (2026-08-14) and the
  `peer_connectivity.rs` integration test both exist as cited. Split one S1
  em-dash-chain sentence (the integration-test paragraph, five clauses on
  one dash) into two. 205 → 225 lines.
- Secrets-Broker.md (TRIM+VERIFY): verified every numeric/name constant this
  page states against `crates/secrets` at HEAD — `DEFAULT_PARK_TIMEOUT_SECS`
  = 300 (`park.rs`), `DEFAULT_PARK_CAP` = 32 (`park.rs`),
  `DEFAULT_BACKEND_TIMEOUT_SECS` = 10 (`backend.rs`), `DEFAULT_BACKEND` =
  `"age"` (`commands.rs`), `LOCKOUT_SECS` = 10 /
  `POPUP_KILL_LOCKOUT_SECS` = 15, the 1s→60s zenity spawn backoff, and the
  five broker event kinds `released`/`parked`/`completed`/`dismissed`/
  `expired` (`watch.rs`'s `emit_event` doc) — all already correct, no
  drift. Fixed one clarity bug: the event-kind list punctuated `completed`'s
  "(a successful `approve`)" gloss as a stray sixth list item instead of a
  parenthetical on `completed`; reflowed to match the other four glossed
  kinds' parenthetical form. Frontmatter had no `updated:` field; added one.
  236 → 238 lines.
- Screen-Control.md (TRIM+VERIFY): verified the fourteen-verb roster exactly
  against `crates/screen/src/commands.rs`'s registrations (`info`/`shot`/
  `ocr`/`diff`/`send` + nine `point` subverbs: `move`/`click`/`drag`/
  `hover`/`scroll`/`idle`/`save`/`restore`/`text`) and that the crate
  registers under `lyra`, not `aoide` (`crates/lyra/src/commands/mod.rs`),
  matching the page's `lyra screen` framing. `CONTRACTS.md` §8 exists as
  cited. No drift, no slop pattern found; zero edits.
- Conductor-3D-DAG.md (KEEP): verified no `spatial.rs` or any 3D-projector
  module exists anywhere under `pkgs/aoide/crates` — the page's
  "Status: PLANNED … Not yet implemented" line remains accurate. Zero edits.
- Claims left unverified: none — every claim this sweep touched or
  spot-checked (CONTRACTS.md §6/§7 section numbers, `modules/nucleus/
  secrets.nix`'s `ProtectHome`/`members` option, the secrets AGENTS.md
  self-asserted-consumer note) resolved against the repo at HEAD
  (2f23210 at sweep start).
- Out-of-scope drift spotted, not fixed: `aoide.a2a.bearerSecret` (the new
  inbound broker-bearer mechanism) has no nix option yet — it is reachable
  only via `--bearer-secret`/`AOIDE_A2A_BEARER_SECRET`, unlike every other
  `a2a serve` setting, which is nix-configured; a future nix-module pass
  could close that gap or the wiki could note it as a deliberate CLI-only
  surface. `concepts/Peer-Federation.md`'s "Security" section still
  describes only the pre-#84 `tokenFile` autogate match; the new
  `bearerSecret` field is now documented in the registry section above it
  but not folded into that section's own autogate-match prose — a small
  follow-up, not done here to stay inside this sweep's page list.

## [2026-08-25] refactor | de-slop sweep 7

De-slopped the five deepest architecture pages (`concepts/{Full-Architecture,
Codebase,Package-Layout,Plugin-Architecture,Snowflake-Anatomy}.md`);
mechanism depth (diagrams, tables, invariants) kept everywhere, only filler
and stale facts went. This sweep's VERIFY pass surfaced far more drift than
usual — the crate split (7 → 13 crates) and the two-binary split had never
been folded into `Codebase.md`, and several counts had drifted since their
last verification.

- Full-Architecture.md (TRIM+VERIFY): 441 → 439 lines. Cut the status-brag
  italic lead and the redundant "built, switched, and logged-into walking
  skeleton" framing (S5/S2/S10) from the Status section. Rewrote the
  "Binary note" out of predates-the-split framing (Assertion clause 1) into
  present-tense ownership language. Verified and fixed: aoide's command
  total was stated as 48/64 in three places, actual is **68** (60 real + 8
  stub, confirmed against `crates/cli/src/registry.rs`'s golden snapshot
  AND `lib/vmTest.nix`'s `cmd_count == 68` drift-tripwire assertion — the
  page previously undercounted by not folding in the `who`/`inbox.*`
  groups); lyra's `rice` group was stated as 16-verb, actual is
  **18-verb** (confirmed against `crates/lyra/src/registry.rs`'s golden
  snapshot). Fixed the repo-tree line claiming `pkgs/aoide` is "one crate
  today" — it is a 13-crate workspace over two binaries (verified `ls
  pkgs/aoide/crates/`). Fixed the dendrite tree line: 20 named entries,
  actual is **27** (`modules/dendrites/` now also carries audio,
  claude-code, clipboard, dunst, kimi-code, networkmanager,
  pi-coding-agent). Fixed the CONTRACTS.md line: "five versioned contracts"
  is stale, actual is §0 (philosophy) + §1–8 (eight versioned contracts,
  including A2A door/peer federation/screen capture, added since). Fixed
  the systemd unit list: added the `aoide-graph-reap` liveness-reap timer
  (missing entirely) and expanded `song/stage/*.json` from the 5 files
  named to the full set (livery, mode, sessions, hooks, projects, graph,
  cover, herald, pending). Fixed the facets-read-whitelist line: it named
  only `aoide.livery`, missing `aoide.arrangement` (verified against
  `modules/nucleus/options.nix` and house rule 5). **Fixed a real
  falsehood**: "the repo is deliberately local-only for now: no git
  remote" — `git remote -v` shows `origin` configured
  (`git@github.com:dxcently/Aoide.git`); rewrote to state the repo tracks
  a remote and dropped the now-unverifiable Melete-fleet-registration
  consequent claim.
- Codebase.md (TRIM+VERIFY): 336 → 354 lines — net growth because VERIFY
  turned up more missing facts than there was filler to cut; every added
  line is a verified correction, not padding. Cut the status-brag italic
  lead (S5/S2). Fixed the package list: `{aoide, melete, mneme}` was stale,
  actual is **`{aoide, hyprglass, kimi-code, melete, mneme}`** (verified
  `ls pkgs/`). Added the three `lib/` files the page never mentioned
  (`songbook.nix`, `livery.nix`, `song.nix` — verified against their own
  header comments). Fixed the vm-boot assertion count from "60 commands"
  to **68** (matches `lib/vmTest.nix`'s literal `cmd_count == 68`). Added
  the missing option-contract entries (`aoide.arrangement`,
  `aoide.a2a.enable`, `aoide.usage.enable`, `aoide.secrets.enable`).
  Rewrote the systemd unit table to add the four units it was missing
  entirely (`aoide-graph-reap`, `aoide-a2a`, `aoide-usage`+timer,
  `aoide-secrets-serve`) — verified against every `systemd.user.services.*`/
  `systemd.services.*` declaration in `modules/nucleus/*.nix`. Expanded the
  stage-file list from 6 to the verified 9 documented files (added
  `cover.json`, `herald.json`, `pending.json`, each glossed from
  `CONTRACTS.md` §4). Fixed the CONTRACTS.md section count (same fix as
  Full-Architecture, above). **Fixed a latent safety bug in the
  documentation itself**: the build-and-verify block told a reader to run
  bare `cargo test`, which is `cargo test --workspace` in a workspace root
  and is exactly the command `pkgs/aoide/crates/AGENTS.md` says
  deadlocks on this machine (`aoide-conduct`/`aoide-server` bind real
  sockets) — changed the example to `cargo test -p aoide-cli` with a
  one-line explanation. Rewrote the whole "one crate today" / stale
  `pkgs/aoide/src/graph.rs` paragraph to the landed 13-crate reality,
  pointing to [[Package-Layout]] instead of duplicating its table. Fixed
  the `aoide graph` subcommand count from 15 to **20** (added `permit`,
  `pending list/approve/deny`, missing from the enumerated list). Fixed
  the dendrite count/list (20 → 27, same fix as Full-Architecture). Fixed
  one Assertion-clause-2 violation ("no longer in the QML tree" → present
  indicative).
- Package-Layout.md (TRIM+VERIFY): 248 → 170 lines. This page was the
  most out of date of the five — it described the crate split as "seven
  crates carved out" with `cli` explicitly stated to NOT be a separate
  crate, no mention of `lyra`/`screen`/`secrets`/`test-support`/`upkeep`
  anywhere. Verified the actual crate roster against `ls
  pkgs/aoide/crates/` and each crate's own `Cargo.toml` `name = `: **13
  crates** (`protocol`, `storage`, `conduct`, `client`, `server`, `song`,
  `conductor`, `cli`, `test-support`, `upkeep`, `lyra`, `screen`,
  `secrets`) — the brief's suggested list matched exactly. Rewrote the
  per-crate charter table with verified command-ownership (cross-checked
  each crate's `commands::register` call against both `registry.rs` golden
  snapshots, hand-summed to confirm the 68/42 totals) instead of the stale
  "maps-from (today)" file-archaeology column, which was history, not
  present architecture. Added the "Two binaries" section (previously
  entirely absent from this page despite being the single most consequential
  fact about the current layout) condensed from `docs/architecture/
  PACKAGE-LAYOUT.md`'s own "Two binaries" section, including the four
  named charter exceptions (shellbridge/herald files staying in `conduct`,
  `storage::takes`/`storage::mode` staying in `storage`). Compressed the
  blow-by-blow landed-phase commit history (Assertion clause 2 — history
  belongs in this log, not page prose) to one line; kept the DEFER
  reasoning for `steward`/`management`/`evals` under present-indicative
  Status labels, since those are decided-but-unbuilt designs, not history.
  Dropped the "naming — DECIDED" open question (it isn't open) and the
  redundant repeat of the crate mapping table's rationale.
- Snowflake-Anatomy.md (TRIM): 68 → 68 lines. Cut the "the metaphor is not
  decorative" snowflake-morphology justification essay (S4) — it is
  byte-for-byte the same reasoning already in `Lexicon.md`'s "The frozen
  family" section; replaced with one clause + a `[[Lexicon#The frozen
  family — snowflake morphology]]` link. Split one dense paragraph
  chaining 3+ wikilinks per clause (S6) into shorter sentences. Fixed the
  same `aoide.arrangement` omission as the other pages ("each reads only
  the `aoide.livery` option" → livery **and** arrangement). Fixed the
  repo-layout tree's `pkgs/` line ("aoide CLI (Rust)" → the actual 5
  walker-discovered packages).
- Plugin-Architecture.md (KEEP): 96 lines, zero edits. Verified its two
  load-bearing claims against the repo: `lib/walk.nix` walking
  `song/songbook/*/rice.nix` (confirmed — `lib/mkHost.nix` calls the same
  `walk` function on `../song/songbook`) and the closed facet read-list
  `aoide.livery`/`aoide.arrangement` (confirmed against
  `modules/nucleus/options.nix` and `modules/AGENTS.md`). No slop pattern
  found, no drift found.
- Claims left unverified (cargo is off-limits this sweep — another agent
  owns the build directory, and a hygiene batch is actively editing
  conduct/secrets test files): the exact unit-test counts stated in
  Codebase.md's prose ("63 unit tests," etc. — left as-is, not
  independently re-derivable without running `cargo test`).
- Out-of-scope drift spotted, not fixed: `pkgs/aoide/crates/lyra/AGENTS.md`
  itself states "cli's 48" commands in its own invariants section — stale
  against the actual 68, inside the REPO, not the wiki (a bug this sweep
  cannot fix per its file scope). `docs/architecture/PACKAGE-LAYOUT.md`
  (the repo doc `Package-Layout.md` mirrors) has no mention of the
  `secrets` crate anywhere, and mentions `screen` only inside prose, not
  as a workspace member — that repo doc itself is behind its own tree.
  `entities/aoide-cli.md` (not in this sweep's page list) almost certainly
  repeats several of the same stale counts (48/64 commands, single-crate
  framing) fixed here in Full-Architecture.md and Codebase.md — a strong
  candidate for the next sweep. `concepts/Session-Graph.md` (also outside
  this sweep) should be checked for the same "one crate"/command-count
  drift class. `song/stage/grimoire.json` exists on disk
  (`modules/facets/quickshell/qml/GrimoireLedger.qml` reads it) but has no
  `CONTRACTS.md` §4 entry and is not mentioned on any page touched this
  sweep — left out of the stage-file lists added above rather than guessed
  at.

## [2026-08-25] refactor | de-slop sweep 8

- Widget-Maker.md (REWRITE): 246 → 94 lines. Worst-offender page for
  marketing register (S7): cut the "headline capability", "the real
  product is", "**that is the leverage**" framing from the intro and the
  "Why Aoide can do this" section, replacing with a flat statement of what
  the mechanism does. Kept the make-a-widget loop diagram and the hard-line
  rule verbatim, per the plan. Compressed the staging-engine and
  declared-widget-registry sections (the page's real payload) by cutting
  redundant restatement rather than facts — e.g. collapsing the
  "song-switch case" vs "content-edit case" split into one paragraph now
  that the new-song-still-needs-a-rebuild fact is already stated once
  earlier on the page. Fixed a real drift while verifying against
  `modules/facets/quickshell/qml/slots.md`: the page claimed "Five slots
  are wired today: calendar, notifications, bar … powermenu and launcher"
  — `notifications` was retired 2026-08-16 and superseded by `herald`/
  `herald-center` (confirmed in `slots.md`'s own table); corrected to "six
  slots… calendar, herald-center, bar … herald, powermenu, launcher",
  matching Song-Anatomy.md's already-correct count. Fixed "used to need a
  manual restart" (Assertion clause 2) to present-tense "needs no restart".
  Cut the stale "Gadget-Dock's seven gadgets… are the shipped proof the
  pattern works" status-brag/count (S5); Gadget-Dock's own page counts six
  baseline gadgets, not seven — reworded to drop the invented number
  entirely rather than swap in an unverified one.
- Feature-Set.md (TRIM): 194 → 192 lines. Fixed the stale "48-command
  surface" claim (two occurrences) to 68, verified via `aoide schema
  --json | jq '.commands | length'` (68) against the installed binary,
  which matches repo HEAD (`git log -1` == working tree, no uncommitted
  changes to the CLI crates). Stripped S2 bullet-bolding across four
  feature-group sections (Outbound/Inbound/Surfaced-as/Default,
  Tailscale/Cloudflare/Fleet-management, Agent-schedules/System-timers/
  Widget/Governance, Watches/Shows/Jump, In/Out/Control/Toggle) — none were
  contract-term first-definitions, all were label-bolding on ordinary
  bullets. Cut the "**A specialized widget maker.**"/"the *point* of
  Aoide" marketing paragraph (S7), keeping its one factual clause. Verified
  the `lyra rice` verb-implementedness matrix row
  (lint/stage/compose/draft/mode/take/back real, declare/transpose exit-64)
  against `lyra schema --json` — exact match, no edit needed.
- Gadget-Dock.md (TRIM+VERIFY): 126 → 126 lines. Verified the keybind
  claim against the repo and found a real drift: the page said **SUPER+P**
  for the dock toggle; `modules/dendrites/hyprland.nix:158` binds `SUPER,
  G, global, aoide:dock` — SUPER+G is what the compositor actually runs.
  (The QML side's own comments in `AoidePanel.qml`, `shell.qml`, and
  `song/songbook/sonata/widgets/bar.qml` all say SUPER+P too — stale
  repo-side comments, flagged below, not fixed here.) Corrected to
  SUPER+G. Re-verified the "four core gadgets + opt-in Usage stele" count
  against `AoidePanel.qml` (Conductor/Terminals/Meters/Power + Usage,
  gadgetW=360 block) — unchanged from the prior sweep's finding, no edit
  needed. No other slop pattern found; page was already tight.
- Widget-Bridge-Contract.md (TRIM+VERIFY): 244 → 243 lines. Field contract
  table and hook→state-machine section kept intact in meaning, per the
  plan. Verified the canonical state vocabulary (`working`/`awaiting`/
  `stopped`/`idle`/`done`) and the legacy shim (`running`→working,
  `blocked`→awaiting, `waiting`→idle) against
  `pkgs/aoide/crates/conduct/src/graph/model.rs`'s own test module — exact
  match. Condensed the closing landed-features "Status:" paragraph (S5-
  adjacent: read as a status list more than a single status line) into one
  sentence without dropping any named capability. Fixed one contrast-
  framing sentence ("undisturbed by a `nixos switch` … not by a service
  flag") to a positive statement (Assertion clause 2).
- Desktop-Architecture.md (KEEP, verify, edit only what fails): 65 → 65
  lines. Verified against source: Hyprland-only-multiplexer claim, the
  `SUPER+SPACE` launcher bind (`hyprland.nix:137`), the `aoide.dunst`
  dendrite option name (`modules/dendrites/dunst.nix:82`), and the
  `[[Controls]]` related-link target (page exists) — all correct, no edit.
  Found and fixed one real internal contradiction: the surface table's
  "Notification daemon | Native `org.freedesktop.Notifications`
  implementation" row directly contradicted the very next paragraph on the
  same page ("dunst holds the notification-delivery role") — Quickshell's
  own `NotificationServer` was retired 2026-08-16 (`slots.md`); dunst owns
  the bus name now, Quickshell only draws the herald popup/ledger from its
  history. Row corrected to name the actual surface (Herald) and its real
  relationship to dunst.
- Self-Ricing.md (TRIM+VERIFY): 320 → 317 lines. Lifecycle diagram and the
  three-mode (`Staging`/`Declarative`/`Draft`) machine kept intact in
  meaning, per the plan. Verified the verb roster against `lyra schema
  --json`: `compose`/`stage`/`mode draft`/`mode declarative`/`declare` all
  present as named; `declare`/`transpose` confirmed `implemented: false`
  (exit 64); no `gen`/`preview`/`mint`/`adopt`/`new` subcommand exists
  under `rice` — the page's own "There is no `rice gen`" claim confirmed
  true. Fixes: retitled "Self-Ricing — the Headline Feature" (S7 marketing
  register in the H1 itself) to "Self-Ricing — the Rice Loop"; deleted a
  pure-history paragraph about the removed `rice draft stage` verb
  (Assertion clause 2 — "There used to be a fourth verb… it's gone"); fixed
  a "was never X in the first place — it's Y" contrast-framing sentence
  (Assertion clause 2) in the Shipped-Baseline section to a positive
  statement; tightened a speculative "may no longer require a restart …
  unconfirmed" hedge into a plain present-tense fact plus one flagged
  open question; removed a real-name attribution ("khoa 2026-08-14") from
  a sentence about the cut `rice gen` prototype, per house rule 12 — the
  surrounding history narrative was cut too (Assertion clause 2), not just
  the name. Trimmed the Declare/Select/Replay section's restatement of the
  songbook walker-registration mechanism (already stated in this page's
  own Rice Loop section and canonically owned by Song-Anatomy.md) down to
  a cross-reference. Did not reach the ~250-line target: nearly the entire
  page is the mode-machine/drafts-routing mechanism the plan explicitly
  protects (symlink semantics, the round-trip demo, the reap sweep), and
  rule 16 (never cut a fact to hit a number) took precedence over the line
  target once the marketing/history/hedge material was gone.
- Song-Anatomy.md (TRIM): 155 → 152 lines. Verified the "sonata's widgets/
  holds fourteen bodies" count (14 named files) and the "six wired to a
  live host anchor" count (`calendar`, `herald`, `herald-center`,
  `powermenu`, `launcher`, `bar`) against `modules/facets/quickshell/qml/
  slots.md`'s own slot table — both exact matches, this page already had
  the correct post-`notifications`-retirement figures (unlike the stale
  Widget-Maker.md claim fixed above). Fixed one "no longer" construction
  (Assertion clause 2) describing the retired `default` song's Pantheon
  grammar to a present-tense "lives only as historical reference…, not in
  any live song's design folder". Compressed the shipped-standard-song
  blockquote's guarantee explanation (S3 catechism repetition with
  Self-Ricing.md's own "Shipped Baseline" section) to one clause plus a
  `[[Self-Ricing#The Shipped Baseline Is Guarded, Not Frozen]]` link.
- Lexicon.md (TRIM): 132 → 102 lines. Kept the `<!-- narrative -->`-marked
  "Why a vocabulary at all" section untouched, per the plan. Compressed
  "Why the seam is a livery" from ~34 lines (prose essay + a four-row
  recap table restating the same content) to ~10: the token-vs-livery
  definition, the two-axes placement, the `aoide.arrangement` sibling —
  dropped the "name keeps reading true past the rename" table entirely as
  the redundant "heraldry essay" the plan called out. Trimmed the Greek-
  Muses section's "integrated (not vendored)" paragraph to one clause per
  house rule 7 (that catechism's home is `entities/Melete.md`/
  `entities/Mneme.md`). Every table and one rationale paragraph per word
  family preserved, per the plan.
- Claims left unverified: Self-Ricing.md's "unconfirmed against a live
  instance" flag on whether `lyra quickshell reload` also re-reads
  `manifest.json` for brand-new widget files — left as an open technical
  question, not resolved (would need a live Quickshell instance to test,
  out of scope for a wiki sweep).
- Out-of-scope drift spotted, not fixed: `modules/facets/quickshell/
  qml/AoidePanel.qml` (comment), `modules/facets/quickshell/qml/shell.qml`
  (comment), and `song/songbook/sonata/widgets/bar.qml` (comment) all say
  "SUPER+P" for the dock toggle — stale against `hyprland.nix`'s actual
  `SUPER, G` bind; these are repo-side QML comments, not wiki pages, and
  three separate files would need the same fix. `entities/aoide-cli.md`
  (flagged in sweep 7 too, still unfixed, not in this sweep's page list)
  likely still carries the stale 48/64-command framing fixed in
  Feature-Set.md here. `concepts/Session-Graph.md` (also flagged in sweep
  7, also not in this sweep's list) was not checked this pass either.

## [2026-08-25] refactor | de-slop sweep 9

- `protocol/AOIDE-DEV.md` (TRIM, final sweep): 535 → 528 lines. Deliberately
  the most conservative pass of the nine — the page's §7 "Open flags — live
  ledger" is a quoted, cross-referenced record and was left completely
  untouched (verified: `diff` of the section's exact line range between HEAD
  and the edited file is empty). Trimmed only the manual sections around it
  (§1–6, §8, Related): compressed the header's "Canonical framing"
  blockquote from an 11-line em-dash-chained/bold-inflated restatement of
  Aoide-vs-AoideOS (S1, S2, S3 — the catechism's home page is `Overview.md`,
  confirmed by grep) to 5 lines pointing at [[Overview]], keeping only the
  one fact `Overview.md` doesn't carry (the bidirectional `melete aoide …`
  passthrough); reflowed the "Dev agent vs rice agent" blockquote's stray
  mid-sentence colon-break. Fixed three real-name slips inside the
  trimmable zone — "khoa asks" / "khoa's request" / "khoa looks first" — to
  "the User" per house rule 6, leaving every `khoa` mention inside the
  frozen §7 range and the two technical config facts (the literal unix
  username in the §3 build-recipe header, the literal git-push identity in
  §5) untouched, since those name the actual required value rather than
  narrating who decided something. Fixed three Assertion-clause-1 "now"/
  "old" constructions in §3's live-deploy paragraph ("`lyra quickshell
  reload`, which now replaces the old `systemctl --user restart` step" →
  present-tense with the old command kept as a stated fallback; "is now
  UNNECESSARY" → "is unnecessary") and cut one pure contrast-framing tail
  ("happens on the render, not the diff" → "happens on the render") without
  touching the paragraph's genuine NOT-warning (the `run/qml/` vs
  `modules/facets/quickshell/qml/` deploy-path disambiguation stays, since
  that's a hazard warning agents actually trip on, not rhetorical framing).
  Retitled the "Living report" subheading and its opening sentence out of a
  repeated three-times "X, not chat/not repeated/doesn't replace" contrast
  pattern (S8) into one positive statement plus a single non-repeating
  clause each. `updated:` stamped 2026-08-25 in frontmatter (previously
  unset). Left `[[Loop-Protocol]]`, `[[Ricing-Protocol]]`, `[[Self-Ricing]]`,
  and every other wikilink and command/code block byte-exact per the plan;
  the §3 recipe block and its three sibling code fences were not opened for
  editing.
- Drift found, not fixed (outside the one-minute verify budget): §7's own
  two live command-count entries already contradict each other pre-sweep
  (`cmd_count == 60` in the struck-through closed entry vs `cmd_count == 83`
  vs `aoide schema --json` reporting 87 in the entry below it) — this is
  §7 content, frozen for this sweep by the brief's own constraint, not
  edited or restruck.
- No other drift found in the trimmable zone: the page carries no other
  count/verb claims outside §7 to check (grepped for digits excluding
  dates; every hit outside §7 was a section-number cross-reference, a
  house-rule number, or a code snippet).
- Sole page touched. `docs/Aoide-Wiki/protocol/OPERATIONS/*`, `PROTOCOL.md`,
  `SHAPE.md`, and `_template` were not opened, per the brief's scope.

## [2026-08-25] refactor | cli-section expansion + section reorg

- New pages minted: `concepts/cli/Conductor-TUI.md` (the `aoide conductor`
  interactive terminal — seven panels, keys, dispatch, sourced from
  `pkgs/aoide/crates/conductor/{README,AGENTS}.md` and `src/{lib,app,
  commands}.rs`; the plan's own outline mis-grouped `h`/`l`/`L` under
  PROJECTS — verified source shows they belong to SESSIONS'
  `handle_dag_key`, corrected in the written page) and
  `concepts/cli/Secrets-Verbs.md` (the `aoide secrets` credential door's 16
  verbs, sourced from `pkgs/aoide/crates/secrets/{README,AGENTS}.md`).
- Four moves, `git mv`, history preserved: `entities/Agent-Hooking.md` →
  `concepts/orchestration/Agent-Hooking.md` (a mechanism, not a named
  thing — `Indexing.md`'s entity/concept split), `concepts/orchestration/
  Terminal-Commander.md` → `concepts/desktop/Terminal-Commander.md` (a
  shipped Quickshell widget, `desktop/`'s domain), `concepts/cli/Index.md`
  → `concepts/cli/CLI-Reference.md` (`Index` collided with `ingest/
  index.md`'s basename, forcing every inbound link into path form against
  `Wikilinks.md`'s bare-name rule), `entities/livery (rename to lyra).md`
  → `entities/livery.md` (repairs `Wikilinks.md:12`'s own `[[livery]]`
  example, which resolved to nothing before this move).
- The livery/lyra split: SPLIT resolution — `entities/livery.md` restored
  (the palette/design-token engine, native `lyra livery lint|resolve|
  emit`) and a new `entities/lyra.md` minted (the paint binary itself,
  mirroring `entities/aoide-cli.md` section-for-section, sourced from
  `pkgs/aoide/crates/lyra/README.md` and `docs/architecture/
  PACKAGE-LAYOUT.md`'s "Two binaries" section). 44 occurrences of the
  marker string swept across 19 files via `sed`, plus three surgical edits
  inside `ingest/log.md`'s own Open Threads section (the two ranges left
  deliberately untouched: this file's other dated entries, and
  `protocol/AOIDE-DEV.md`'s §7 ledger, never opened this pass).
- Coverage closed: `### aoide who` (full register entry — live per-peer
  probe, fallback to cache, `<filter>` narrows display only), `--bearer-
  secret` added to `### aoide a2a serve` (inbound, consumer `a2a-door`,
  resolved fresh, fails closed) and `### aoide peer add` (outbound,
  consumer `a2a-client`, mirror direction), `### aoide conductor` shrunk to
  signature + pointer with the stale "five panels/keys 1–5" replaced by
  the verified seven/1–7, `### aoide events tail` (new — see the
  command-count correction below), `### aoide inbox list/read/clear` added
  to `Graph-and-Conduct.md` (verified against `pkgs/aoide/crates/storage/
  src/{inbox,commands}.rs`; the crate note — inbox lives in `storage`, not
  `conduct` — added to that page's scope paragraph), and `lyra guide`/
  `lyra schema` cross-reference lines added to `Meta-and-Upkeep.md`'s
  `guide`/`schema` sections.
- Mid-flight correction, independently verified (not taken on the
  orchestrator's word): commit `86bb6e2` landed `aoide events tail`
  between this plan's authoring and its execution, confirmed via `git
  show 86bb6e2 --stat`, the golden snapshot in `pkgs/aoide/crates/cli/src/
  registry.rs` (69 paths, `events.tail` present), and `pkgs/aoide/crates/
  lyra/AGENTS.md`'s own independent line ("lyra's golden is 42 paths, not
  a subset check against cli's 69"). Every command-count claim this pass
  touched was written or corrected to 69 (real 61 + stub 8), not the
  plan's original 68 (real 60 + stub 8): `entities/aoide-cli.md`,
  `concepts/cli/CLI-Reference.md`, `concepts/Full-Architecture.md` (three
  spots: the binary-note prose, the AGENT INTERFACE diagram box, and the
  real/stub breakdown, which now also lists `events tail` among the real
  verbs). `lyra` stays 42, unchanged, confirmed unaffected. `events tail`
  was placed in `Doors-and-Peers.md` rather than `Graph-and-Conduct.md` or
  `Meta-and-Upkeep.md`: it registers in the same crate/file
  (`server/src/commands.rs`, alongside `daemon`) as this page's other
  entries, matches the page's own scope statement (the doors onto aoided's
  policy skeleton), and shares `secrets watch`'s CLI-only-blocking-
  foreground-follow shape that page's Secrets-Verbs sibling already
  documents.
- Two out-of-section one-line fixes (plus two opportunistic 68→69 count
  fixes at the same file, already open for the mandated edit):
  `concepts/Full-Architecture.md` "3-verb `lyra` group" → "3-verb `livery`
  group" (the token-engine group; `lyra` is the binary, not the group
  name) and `concepts/orchestration/Peer-Federation.md` "`graph who`" →
  "`aoide who`" (the verb is top-level, not under `graph`).
- Four considered-and-rejected moves, not re-proposed: grouping the six
  `concepts/` root pages into `concepts/architecture/` (would strand
  `Lexicon.md` alone to fix a two-line `SCHEMA.md` staleness — fixed the
  shape block instead); moving `concepts/orchestration/Screen-Control.md`
  to `desktop/` (it acts *on* the desktop, `desktop/` holds surfaces
  themselves); moving `concepts/Package-Layout.md` (the concept page is
  the wiki's summary of `docs/architecture/PACKAGE-LAYOUT.md`, not a
  duplicate, and correctly cross-cutting); retiring `concepts/
  orchestration/Conductor-3D-DAG.md` (out of scope for a reorg; its
  "specified, not implemented" status line is `Assertion.md`-legal).
- Drift found and NOT fixed, outside this pass's scope: the 68→69 ripple
  in `concepts/desktop/Feature-Set.md`, `concepts/Codebase.md`, and
  `docs/architecture/PACKAGE-LAYOUT.md` (repo-side, not a wiki page) still
  read the pre-`events tail` count — none were already open for another
  edit in this pass. `concepts/Full-Architecture.md` lines ~99–153 (the
  ASCII data-flow diagram plus its subsystem table and "lyra seam" prose)
  use `lyra`/`aoide.lyra`/`stage/lyra.json` throughout to name what
  verified source (`modules/AGENTS.md`, `modules/nucleus/README.md`,
  `pkgs/aoide/crates/song/src/livery/`) confirms is actually `livery`/
  `aoide.livery`/`stage/livery.json` — `aoide.lyra` is a real, distinct
  option (`modules/nucleus/packages.nix`, gates installing the `lyra`
  package) that this diagram is conflating with the palette engine. This
  is a pre-existing, multi-line ASCII-art and table defect, not the
  single-line fix this plan scoped; flagged here rather than silently
  rewritten mid-diagram without a dedicated pass. `entities/livery.md`'s
  note-era prose beyond the dead aliases line was not audited beyond what
  the plan already scoped.
- Manifest rewritten in `SCHEMA.md` from `find . -name '*.md' -not -path
  './.obsidian/*' | LC_ALL=C sort` (74 files, was 68 — nine `concepts/cli/*`
  entries added, `entities/Agent-Hooking.md`/`entities/livery (rename to
  lyra).md`/`references/cli/Index.md` renamed in place rather than added,
  `entities/lyra.md` and `concepts/desktop/Controls.md`/`concepts/
  orchestration/Screen-Control.md`/`concepts/orchestration/
  Secrets-Broker.md` newly present, the four `references/cli/*.md` entries
  removed since that tree moved to `concepts/cli/` in an earlier commit);
  `snapshot:` restamped 2026-08-25; `SCHEMA.md`'s §"Read the whole thing"
  and §"The shape" repointed from `references/…cli/` to `concepts/cli/`,
  and the `concepts/ (root)` shape line extended with `Package-Layout` and
  `Plugin-Architecture`. Tags line extended with 17 additions surfaced by
  the new/edited pages' own frontmatter (`a2a`, `aliases`, `computer-use`,
  `hooks`, `keybinds`, `lyra`, `notification`, `paint`, `peer`, `pointer`,
  `reference`, `schema`, `screen`, `secrets`, `totp`, `upkeep`, `vision`).

## [2026-08-25] rename | concepts/cli/Screen-Verbs.md → Screen-Commands.md, Secrets-Verbs.md → Secrets-Commands.md

- Repo-wide "verb" → "command" terminology sweep (task #94): both pages
  `git mv`'d, history preserved, and every "verb"/"verbs" occurrence in the
  wiki's in-scope zones (`concepts/`, `entities/`, `SCHEMA.md`,
  `ingest/index.md`, `protocol/AOIDE-DEV.md`, `protocol/OPERATIONS/
  Assertion.md`) swapped to "command"/"commands" — 41 files, 183
  occurrences.
- Nine inbound wikilinks repointed: `concepts/cli/CLI-Reference.md` (×3),
  `concepts/orchestration/Secrets-Broker.md`, `entities/lyra.md` (×2),
  `entities/aoide-cli.md` (×2), `ingest/index.md`.
- `SCHEMA.md`'s Notes manifest updated in place for both renamed filenames;
  `snapshot:` was already stamped today from an earlier pass, left as is.
- Excluded, per the wiki protocol: `references/**` (raw source material,
  never edited), `ingest/log.md`'s own dated history above this entry
  (four historical mentions of the old filenames at lines 743, 894, 1628,
  1681 — they name the file as it was), and `.obsidian/workspace.json`
  (editor state, regenerated by Obsidian).

## [2026-08-27] refactor | Style joins the protocol's OPERATIONS rule set

- New `protocol/OPERATIONS/Style.md`: the prose register for every content
  and protocol page — fact-density tests, banned filler constructions
  (throat-clearing, contrast rhetoric, symmetric slogan lists, empty
  intensifiers, marketing vocabulary, restating summaries, false tension),
  and the "What stays" depth exemption. Codifies the de-slop register the
  sweep-1..9 passes applied from an ephemeral brief; the rule now lives in
  the protocol so any agent editing the wiki inherits it.
- Wired at the existing Assertion seams: [[Lint]] gains check 10 (Style
  violations) with a grep-able first pass and a report section; SHAPE.md's
  Rules list and SCHEMA.md's rule roster + "never do" list + Notes manifest
  all cite [[Style]].
- Self-description test: walked SCHEMA.md, SHAPE.md, Lint.md, Assertion.md
  against the new rule; protocol pages remain imperative-mood conformant.

## [2026-08-28] ingest | pairing ceremony page + federation security rewrite

- New page `concepts/orchestration/Pairing-Ceremony.md` (the pairing lane,
  task #93 / P-P5 close): the commit-then-reveal ceremony over
  `aoide/pairRequest`/`pairReveal`/`pairApprove`, the A-commits/B-reveals
  wire asymmetry and the B-commits-first record asymmetry, the locally
  derived SAS (`aoide_storage::pairing::derive_sas`, pinned vectors
  740-729/847-405), the five `peer pair` commands including `watch
  [--popup|--json]` (zenity `--question` only), the gate-classed events
  feed (`pair-parked`/`pair-revealed`/`pair-awaiting-confirm`, payload
  `id`/`name`/`originAddr`/`url`/`direction` only), the park cap (32) and
  4-hour expiry, and the legacy-escape rungs.
- [[Peer-Federation]]: the Security section rewritten as "Security —
  pairing is the verification path" (the rung ladder, Signature-only
  Spawn, signature-outranks-loopback, `origin: "peer:<name>"` audit
  stamping); the previous autogate/token-only framing read as the whole
  security story and is superseded. Its CLI-surface list gains `pair
  request|pending|approve|reject|watch`, `allow`, `discover`, `invite`,
  `spawn`; the in-page anchor on the registry section's `autogate`
  paragraph repointed to the new heading.
- Cross-links: [[A2A-Door]] and [[Peer-Transport]] gain a
  [[Pairing-Ceremony]] Related entry (Peer-Transport's notes the `via`
  marker is recorded by the ceremony).
- `concepts/cli/Doors-and-Peers.md` gains a `peer pair
  request/pending/approve/reject/watch` section in its house
  reads/writes/output shape. `entities/aoide-cli.md`'s command-count
  paragraph updated: 75 leaves at HEAD per `cli/src/registry.rs`'s golden
  snapshot, the pair ceremony now enumerated as 5 commands. The installed
  binary's `schema --json` still reports 74 (older than HEAD); the tree's
  own golden test pins 75 including `peer.pair.watch`.
- Registration: SCHEMA.md Notes manifest and `ingest/index.md` catalog
  entries added; `updated:` bumped on Peer-Federation, A2A-Door,
  Peer-Transport, Doors-and-Peers, aoide-cli, and the index.

## [2026-08-28] lint | aoide.song default removed — six pages reconciled

`aoide.song` is `types.nullOr types.str`, default `null`
(`modules/nucleus/options.nix`, commits 0b65c49 + ae18b98). Naming no song
performs no song: with the quickshell facet enabled and `aoide.song = null`
the host gets no paint config and no shell service — not an empty surface;
the unit carries `ConditionPathExists` on the runtime `shell.qml` so a
runtime-composed song starts it without a rebuild. Hosts wanting paint name
the song explicitly (`aoide.song = "sonata";`, as yomi-strix does).
Canonical wording: CONTRACTS.md, "Naming no song performs no song".

Six stale claims of the old `"sonata"` default corrected in place:
`concepts/song/Song-Anatomy.md` (shipped-standard blockquote),
`concepts/song/Self-Ricing.md` (Shipped-Baseline section; Declare/Select/
Replay selector line), `concepts/Snowflake-Anatomy.md` (song
self-registration paragraph), `concepts/Codebase.md` (option-contract
bullet), `concepts/Full-Architecture.md` (song-replay paragraph). The
`rice compose --from` default of `"sonata"` (Self-Ricing.md diagram +
pipeline line, Rice-and-Livery.md Reads bullet) is a separate, unchanged
CLI default and was left alone, as were all `aoide.song = "sonata";`
example lines — explicit naming is the current mechanism.

Lint grep pass over the five touched pages: no new violations in the edited
lines; pre-existing candidates on untouched lines (e.g. Codebase.md:20
"robust", Self-Ricing.md:53 "would") left for a user-reviewed lint sweep.

## [2026-08-28] lint | runtime root ~/.aoide + shipped templates — lane #107 sweep

Verified and finished the L-C5 sweep (lyra-carrier lane, task #107),
resuming a run that died mid-sweep with 23 pages edited and no log entry.
Every runtime tree — `song/stage/`, `state/` (+ `state/stage/`), `run/qml/`,
the composed `song/songbook/<name>/`, the audit log — hangs off ONE root,
`$AOIDE_ROOT` (absolute-path-wins, default `~/.aoide`), nix-free; `~/Aoide`
is purely the dev git checkout, reached through `$AOIDE_FLAKE_ROOT` (default
`~/Aoide`). `pkgs/lyra-songbook` bakes the committed `song/songbook/` tree
plus prebaked `manifest.json`/`registry.json` into `share/lyra/songbook`, so
a repo-less host still `rice compose --from`s a shipped song and
regenerates the widget registry/manifest nix-free (three-layer merge: baked
baseline, surviving host-songbook entries overlaid, the staged song's own
scan patched in last). `rice declare` is real: a gated, byte-diff copy of
the composed song from the runtime songbook into the checkout's
`song/songbook/<name>/` — no `git add`, no rebuild. `entities/dxflake.md`
documents chiyo as the AoideOS carrier (dxflake's full paint stack, rev-
pinned via `git+file:///home/khoa/Aoide?rev=…`; Aoide's own tree carries no
`hosts/chiyo`).

All 23 pages the prior run touched verified against HEAD (`d510235`,
`CONTRACTS.md` §4, `aoide_storage::fs`, `aoide_protocol::audit`,
`aoide-song`'s README/AGENTS, `crates/lyra/src/commands/stubs.rs`,
`dxflake/hosts/chiyo/default.nix`) — all held up factually and against
Style/Assertion; no half-finished edits found. Two more pages the sweep had
missed, caught by the `~/Aoide`/`Aoide/state`/`Aoide/log` grep and fixed in
place: `concepts/cli/Doors-and-Peers.md` (shared state/stage/audit-log
path resolution) and `concepts/cli/Secrets-Commands.md` (the mirrored
audit-log path in the TOTP-park paragraph). `Overview.md` and
`entities/livery.md` — both on the brief's expected-edit list — carry no
`~/Aoide`-as-runtime-root claim and needed nothing. The `~/Aoide/state/
<agent>-hooks.jsonl` mentions left standing in `concepts/cli/
Content-and-Hooks.md`, `entities/aoide-cli.md`, and `concepts/orchestration/
Agent-Interface.md` are correct as written: `hooks install --capture`'s tee
wrap (`conduct::commands::hooks::door_command`) still hardcodes that literal
path, untouched by the L-C2 migration.

`updated:` bumped to 2026-08-27 on every page actually edited this pass
(the 23 plus the two caught above); `entities/aoide-cli.md` was already
`2026-08-28` at HEAD, ahead of this pass, and left as is. Lint grep run
over every changed page's added lines only (`git diff` `+`-line scan): one
candidate (`Secrets-Broker.md`'s "would silently be blocked") is inherited,
unchanged-in-kind phrasing describing `ProtectHome=true`'s real behavior,
not new debt from this pass — left as rationale, not routed.

Pages touched this pass: concepts/Codebase.md, concepts/Full-Architecture.md,
concepts/Snowflake-Anatomy.md, concepts/cli/CLI-Reference.md,
concepts/cli/Content-and-Hooks.md, concepts/cli/Graph-and-Conduct.md,
concepts/cli/Meta-and-Upkeep.md, concepts/cli/Rice-and-Livery.md,
concepts/cli/Screen-Commands.md, concepts/cli/Doors-and-Peers.md,
concepts/cli/Secrets-Commands.md, concepts/governance/Clone-and-Run.md,
concepts/governance/Governance.md, concepts/governance/Rebuild-Gate.md,
concepts/orchestration/Conductor-Channel.md,
concepts/orchestration/Secrets-Broker.md, concepts/song/Self-Ricing.md,
concepts/song/Song-Anatomy.md, concepts/song/Song-Vocabulary.md,
entities/Quickshell.md, entities/aoide-cli.md, entities/aoided.md,
entities/dxflake.md, entities/lyra.md, entities/shellbridge.md.


## [2026-08-28] refactor | identity lane #63 sweep — sealed credentials, kernel-attested senders, origin gate, key-resolved peers

Brings every page touching session identity, origin/provenance, the
secrets broker's gates, peer federation trust, and command counts current
with LANE IDENTITY (#63, P-ID0–P-ID5, landed through `cab9b12`). Ground
truth: `CONTRACTS.md`'s identity sections, `docs/architecture/PAIRING.md`,
and the HEAD registry golden — 82 commands, `secrets allow-remote-origin`
landed; pages saying 81 were stale.

Session-Graph gains the lane's home section ("Session identity — origin and
the sealed credential"): write-once door-stamped `origin` (attribution,
never a gate), the ephemeral in-memory seal keypair and its
`ping`/`sealPubkeyHex` verification channel, the fresh-starttime pid-reuse
defense, the Yama-plus-liveness trust root (OQ1-A), and the lane's
accounting — six enforced phases and the five named open items
(consumer-name authentication; OQ1-B with its daemon-impersonation and
sweep-relaunder residuals; the cross-uid attestation channel; an
authenticated registration path; per-surface dispatch-door gates).
Conductor-Channel and Graph-and-Conduct restate the send gate on a
kernel-attested sender (`/proc` ancestry to a seal-verified session;
`AOIDE_SESSION_ID` demoted to attribution) plus the control socket's
`SO_PEERCRED` self-injection refusal. Secrets-Broker gains the third policy
axis (`allowRemoteOrigin`, caller provenance — distinct from `remote`
transport and `automation` code), the kernel-facts attestation chain, and
the exact boundary (unidentified callers never refused; dormant in the
packaged cross-uid deployment); Secrets-Commands gains the new admin verb
section (16 → 17). Peer-Federation's Signature rung now states
key-not-name resolution, the claimed-vs-resolved attribution-drift rule,
the collision tiebreak and union-of-grants consequence, and the
`-32007`/`-32008`/`-32009` vocabulary; A2A-Door and Doors-and-Peers flip
Spawn to the pairing gate (`-32006` taught refusals, the bearer demoted to
the read arms); Pairing-Ceremony records the `state/identity/` layout and
the ephemeral seal-key split; Peer-Transport's framing is corrected to
signature-is-identity. The shellbridge/aoided entities gain the cross-uid
peercred floors and the daemon's sealing role; Widget-Bridge-Contract's
field table gains the `origin`/`seal` rows. Count bumps 81 → 82 (74 → 75
real) in Full-Architecture, Package-Layout, Codebase, CLI-Reference,
Meta-and-Upkeep ("core's 80" → 82), aoide-cli (59 → 60 tracked leaves),
and the index's secrets/A2A glosses.

Pages touched: concepts/orchestration/Conductor-Channel.md,
concepts/orchestration/Session-Graph.md,
concepts/orchestration/Peer-Federation.md,
concepts/orchestration/A2A-Door.md,
concepts/orchestration/Pairing-Ceremony.md,
concepts/orchestration/Peer-Transport.md,
concepts/orchestration/Secrets-Broker.md,
concepts/cli/Graph-and-Conduct.md, concepts/cli/Secrets-Commands.md,
concepts/cli/Doors-and-Peers.md, concepts/cli/CLI-Reference.md,
concepts/cli/Meta-and-Upkeep.md, concepts/Full-Architecture.md,
concepts/Package-Layout.md, concepts/Codebase.md,
concepts/desktop/Widget-Bridge-Contract.md, entities/shellbridge.md,
entities/aoided.md, entities/aoide-cli.md, ingest/index.md.

## [2026-08-29] refactor | quickshell placeholder-screen watchdog

Brings the wiki current for the live watchdog closing the
`aoide-quickshell.service` placeholder-screen lockup
(`4c8a93a`/`27eb159`/`42475e4`): `Restart=on-failure` cannot catch a Qt
wayland QPA fallback that leaves the process `active` but painting nothing,
so `aoide-quickshell-healthcheck.timer` (~15s, `lyra quickshell
healthcheck` → `pkgs/aoide/crates/song/src/health.rs`) and
`home.activation.aoideVerifyRice` (rebuild-time confirmation, 5s after
`aoideRestartRice`) watch for it live instead.

`entities/Quickshell.md`'s "Session service & resilience" section gains the
watchdog as a fourth layer: the two-signal detection (the journal's
placeholder line scoped to the unit's own `ActiveEnterTimestamp`, plus a
live `hyprctl layers -j` zero-`aoide-*`-surface reading), the
whole-system-not-per-monitor surface count and why, the 0s/15s/60s/300s/900s
retry ladder that floors at 900s without ever stopping, and the
once-per-episode notification's dependence on the journal (dunst's
`skip_display` means the
toast cannot render while the shell it reports on is stuck).
`concepts/cli/Meta-and-Upkeep.md` gains the `lyra quickshell healthcheck`
command doc section, mirroring `lyra quickshell reload`'s shape.

lyra's golden command-path count moves 47 → 48 (`quickshell.healthcheck`,
real); core's stays 80 — `quickshell` has been a lyra-only family since the
P-A5 binary split. Bumped in `concepts/cli/Meta-and-Upkeep.md`,
`concepts/cli/CLI-Reference.md`, `concepts/Full-Architecture.md` (two
mentions), `Overview.md`, and `ingest/index.md` — all bare totals. The
itemized lyra breakdowns in `entities/lyra.md` and `Full-Architecture.md`'s
control-plane section were already stale before this pass (frozen at 43)
and stay untouched this pass — see the Open Thread above.

Pages touched: entities/Quickshell.md, concepts/cli/Meta-and-Upkeep.md,
concepts/cli/CLI-Reference.md, concepts/Full-Architecture.md, Overview.md,
ingest/index.md.

## [2026-08-29] refactor | command-count reconciliation (core 82→80, lyra 43→48)

Follow-up to commit a084fdd, which bumped some command-count citations and
deliberately left others — the "command-count drift" Open Thread above.
Ground truth re-verified directly against the live binaries this pass:
`aoide schema --json | jq '(.data.commands // .commands)|length'` → 80 (73
`implemented: true`, 7 `implemented: false` — the `content`
register/propose/ingest/query/approve group, `make`, `update`); `lyra
schema --json` → 48 (47 real, 1 stub — `rice transpose` alone; `rice
declare` is real, gated code). Both figures match their respective golden
snapshots (`pkgs/aoide/crates/cli/src/registry.rs`,
`pkgs/aoide/crates/lyra/src/registry.rs`'s `assert_eq!(got.len(), 48)`) and
`lib/vmTest.nix`'s `assert cmd_count == 80`.

Bare-number citations swept from 82→80 (core) or 43→48 (lyra):
`concepts/cli/CLI-Reference.md` (both — its "82 command paths (75 real, 7
stubs)" line was a core-count error the original a084fdd pass and its own
Open Thread had missed entirely), `concepts/cli/Meta-and-Upkeep.md`
(core), `concepts/cli/Doors-and-Peers.md` (lyra), `concepts/Codebase.md`
(core, two places — the vm-boot tripwire figure and the per-crate golden-
test figure, both cross-checked against source rather than taken on
citation), `concepts/Full-Architecture.md` (lyra, the Self-Ricing status
row's stray `declare`/`transpose` double-stub claim fixed to `transpose`
alone in the same edit).

Itemized reconciliation (the hard part a084fdd left standing): `entities/
lyra.md`'s command-surface table and `concepts/Full-Architecture.md`'s
itemized lyra paragraph were both frozen at 43, five commands short of the
live 48. Five rows added to each, verified against `lyra schema --json`
rather than assumed from the Open Thread's list: `element seed` (song
crate, `run/elements/` render), `secrets ask` and the 2-command `pair`
group (`ask`/`confirm` — both register in lyra's own crate, not `song`),
and `quickshell healthcheck` folded into what was a 1-leaf `quickshell
reload` row, now a 2-leaf `quickshell` group. `concepts/Package-Layout.md`'s
per-crate table gained the same five commands (`element.seed` and
`quickshell.*` on the `song` row; `secrets.ask`/`pair.ask`/`pair.confirm`
on the `lyra` row) and its stub tally corrected from "41 real, 2 stub" to
"47 real, 1 stub" — the prior "2 stub" count wrongly carried `rice declare`
alongside `rice transpose`; `declare` is real (it byte-diff-copies a
composed song into the checkout, gated but not a stub).

`concepts/Full-Architecture.md`'s aoide-side paragraph (`**80 commands** —
real (73): ...`) was independently verified against the schema's
`implemented` field and left untouched — its enumeration already sums
correctly once `who` is set aside (see the new Open Thread below, opened
during this same verification: `who` and `session undying` are stale
command names carried in several pages, a naming drift the live schema
confirms via `lib/vmTest.nix`'s own bump-history comment, distinct from
the count drift this entry fixes and out of scope for a count-only pass).

Pages touched: entities/lyra.md, concepts/cli/CLI-Reference.md,
concepts/cli/Meta-and-Upkeep.md, concepts/cli/Doors-and-Peers.md,
concepts/Package-Layout.md, concepts/Codebase.md,
concepts/Full-Architecture.md, ingest/log.md (this entry, plus closing the
command-count-drift Open Thread and opening the who/session-grant one).

## [2026-09-03] refactor | pairing docs de-staled — mutual two-code ceremony

The pairing ceremony rebuilt at task #21 (`e17f3d1`+4, 2026-09-02) into a
mutual two-code exchange — both legs a typed CodeGate, never a bare
yes/no or an Approve/Reject — and the whole `peer pair`/`peer pending`/
`peer invite` family folded into bare `aoide pair` at task #135 P3. Several
pages still asserted the old single-code, one-sided-confirm shape as
current; this pass reconciles them against `aoide schema --json`/`lyra
schema --json` (2026-09-03) rather than re-deriving from prose.

`README.md` gains a "Pairing" `###` subsection under Features, immediately
after Conducting (pairing is how conducting reaches another host): the
five-step ceremony named by seat, and what it buys — a verified peer
record's `allows` set, the `peer list`/`peer pull`/`peer spawn` roster,
and the ssh `via` hop for a loopback-only door.
`docs/architecture/aoide-report.html`'s L2 enrolment block: two SVG
labels (`peer pair` → `aoide pair`), three prose paragraphs rewritten to
the two-code shape (the ceremony description, the command surface, the
popup), and one site beyond the brief's own list — the `lyra schema
--json` inventory paragraph still named lyra's second pairing dialog
`pair confirm`, renamed `pair show` at the same rebuild — fixed to
match. `concepts/orchestration/Pairing-Ceremony.md`: the ceremony
diagram's outbound-confirm step, the "commit asymmetry" section (was
describing A's confirm as a skippable self-check against a code it
already printed — it is now B's second, different reply code, gated the
same as B's own leg), a new paragraph on `derive_reply_sas` under "The
SAS", the popup paragraph, and the `--yes` bullet.
`concepts/cli/Doors-and-Peers.md`'s `aoide pair` Output bullet: the
CLI-level outbound confirm and the popup paragraph, both still
describing a bare `y`/`N`/Approve-Reject. `docs/architecture/PAIRING.md`:
one site, the P-P5 phase bullet's present-tense-readable "`aoide peer
pair watch`... `--popup` is zenity only, no lyra fallback" —
historicized and cross-referenced to P-PV3, where that fallback was
actually built; every other `peer pair` mention on that page narrates a
phase already marked dead by a later bullet and stays untouched.

Left alone, checked: `CONTRACTS.md:4844`, `pkgs/aoide/crates/lyra/
README.md:111`, `pkgs/aoide/crates/lyra/AGENTS.md:94` all mention `lyra
pair confirm`/`pair confirm`, but each is explicit lineage narration
("P-PV3 ... repurposed and renamed", "this reverses P-PV3's own outbound
confirm dialog") — correct as written, not touched (CONTRACTS is a
versioned interface doc regardless of verdict). `aoide-report.html`'s
paragraph on the live yomi-strix/sakaki/chiyo/osaka mesh, immediately
after the L2 block, states the registry as verified live the same
morning and keeps that sentence; only its one mid-paragraph `peer pair
approve` naming moved to the approver's own `aoide pair <id>`. Three further pages carried the same rename: `entities/lyra.md`'s lyra
command table, `concepts/Full-Architecture.md`'s 2-command `pair` group,
and `concepts/Package-Layout.md`'s lyra command-path column all named
`pair.confirm`, a path the lyra golden (`crates/lyra/src/registry.rs:130`)
spells `pair.show`; lyra.md additionally described `pair ask` as inbound
only, where it collects on either leg. All three corrected.

Pages touched: README.md, docs/architecture/aoide-report.html,
docs/architecture/PAIRING.md, concepts/orchestration/Pairing-Ceremony.md,
concepts/cli/Doors-and-Peers.md, concepts/Full-Architecture.md,
concepts/Package-Layout.md, entities/lyra.md, ingest/log.md (this entry).

## [2026-09-07] rename | peer → node, one noun for a mesh member

`peer` and `node` both named a mesh member, split by a distinction nobody
could state twice the same way — edge endpoint against declared member.
The User ruled the synonym gone and ordered the rename ahead of the mail
workstream, so the mail design would be written in the surviving noun
rather than translated into it later.

Two lanes ran in parallel over disjoint paths. The code lane (`a30d25f`,
114 files) is a word-boundary rename across Rust identifiers, the ten CLI
paths `node.add` through `node.status` (net command count unchanged at
81), the `X-Aoide-Node` request header, `node:<name>` session origins, the
on-disk `state/nodes.json`, `state/node-cache/` and the node-pairing
files, the `[mesh.<name>].nodes` config key, and the prose in CONTRACTS.md
and the crate docs. Five files moved with `git mv`. The wiki lane
(`622c6b6`, 25 files) took the pages, renaming three of them —
`Doors-and-Nodes.md`, `Node-Federation.md`, `Node-Transport.md` — with
every wikilink to them updated in the same commit.

Three things deliberately keep the old word. The kernel and std socket
sense is untouched: `SO_PEERCRED`, `peer_cred`, `peer_uid`, `peer_addr`,
and prose meaning the far end of a socket. `pair`/`pairing` is a ceremony,
not a member. `references/P2P-Board-Protocols.md` surveys FidoNet, Usenet,
SSB and NNCP in their own vocabulary and stays that way.

`PeerOrigin` became `ConnOrigin` rather than `NodeOrigin`: it classifies a
connection as loopback, remote or unknown, and calling a loopback caller a
node would assert mesh membership the type never checks. Graph-vertex
collisions in `conduct/src/graph/**`, where `node` already meant a DAG
vertex, resolve as `mesh_node` — the compiler surfaced two beyond the
predicted one, both same-scope shadowing.

Compatibility is two shims and no more. `aoide_storage::attest::
is_node_origin` accepts `node:` and legacy `peer:`, and all three readers
— conduct's resurrect and registration gates, the secrets broker's origin
gate — go through it, because a pre-switch session record on disk would
otherwise walk past `allowRemoteOrigin`. `NodeRegistry`'s `nodes` field
carries `#[serde(alias = "peers")]`. `fs::migrate_root_once` renames the
four state paths on first open. The mesh config key gained no alias:
nothing had it deployed.

Review caught a real over-reach. The code lane had renamed the socket
credential vocabulary as well, so `cross_uid_gate`'s refusals and the
broker's `SO_PEERCRED` documentation read as claims about mesh members;
`b86ac1f` reverts that. That revert then over-corrected one line back —
`allowRemoteOrigin` gates a session a remote *mesh member* created, which
is the renamed sense — fixed in `91dfcf9`.

Version 0.0.21 → 0.0.22, the wire change being what the version is for.
yomi-strix switched to it the same day; the state files migrated on first
open and `aoide node list` reads the registry back. The rest of the mesh
still answers the old header — see the open thread above.

Pages touched: `concepts/cli/Doors-and-Peers.md` →
`concepts/cli/Doors-and-Nodes.md`, `concepts/orchestration/
Peer-Federation.md` → `Node-Federation.md`, `concepts/orchestration/
Peer-Transport.md` → `Node-Transport.md`, `concepts/orchestration/
{A2A-Door,Pairing-Ceremony,Session-Graph,Conductor-Channel,Agent-Interface,
Secrets-Broker}.md`, `concepts/cli/{Graph-and-Conduct,Secrets-Commands,
Screen-Commands,CLI-Reference,Conductor-TUI}.md`, `concepts/
{Full-Architecture,Package-Layout,Codebase}.md`, `entities/{aoide-cli,
aoided,lyra}.md`, `SCHEMA.md`, `references/{fleshing-out-aoide-ricing,
audit-report,AOIDE-HANDOFF}.md`, `protocol/dev/DEV.md`, `ingest/index.md`,
`ingest/log.md` (this entry and the open thread above).

## [2026-09-07] update | MAIL's P-M1 forks are ruled

`docs/architecture/MAIL.md` carried the settled store-and-forward design
but left fourteen points either silent or contradicted by the code. The
plan tier read the design against the repo and named them; the architect
ruled all fourteen before any executor saw a brief (`3d9fc21`).

The load-bearing ones. `try_stage_lock` blocks on `LOCK_EX` like its
fail-open sibling and returns `Err` only when the lock cannot be taken at
all — `LOCK_NB` would have turned ordinary contention between the timer,
the door and the CLI into failures, which inverts the guarantee it exists
for. Only a write truncates a torn tail; readers skip it, as
`ledger::read_ledger` already does, because truncating on read means
writing on every open. A local receipt is signed by the box identity,
minting the keypair if the box has never paired, since an unverifiable
entry in a store whose premise is verification is worse than a mint. `self`
resolves to the box's own node name when the envelope is minted, so a
locally filed letter is byte-identical to the same letter on the wire and
no `msgid` is ever computed over a literal that means something else
elsewhere. The retiring per-entry read flag and the reserved `context`
passthrough are both dropped rather than carried: cursors are a high-water
mark with no honest seed from scattered read bits, and the canonical header
is a closed eight fields.

Also fixed: pruning is `--older-than <Nd|Nh>`, the grammar already in the
repo; every mail mutation writes one audit line naming the command and the
msgid or count, never the text or the name, and reads write none; bare
`aoide mail` ships its names half in P-M1 and its caller's-own half moves
to P-M5 with the doorbell, which is where the reader binding lands.

## [2026-09-07] add | codex dendrite

`modules/dendrites/codex.nix` (`5accea6`) is the fourth agent-CLI
dendrite beside claude-code, kimi-code and pi-coding-agent, on the same
per-tool shape rather than bundled into devtools. Apache licensed, so no
unfree flag, and packaged in nixpkgs, so nothing is vendored. It installs
the binary and nothing else: codex is the hookless harness, carrying no
`AgentProfile` row and no hook-settings install, and reaches the session
graph through `aoide conduct --agent codex` instead. Shipped off; yomi
enables it (`2d2b528`). The flake inputs advanced in the same commit as
the dendrite file, independently of it — the previous nixpkgs already
carried codex.

Pages touched: `ingest/log.md` (this entry).

## [2026-09-07] add | the mailbase replaces the inbox

P-M1 is built (`9d873df`, 21 files). `state/inbox.json` — one rewritten
JSON array, a per-entry read flag, and a reserved `context` nothing ever
set — is gone. In its place `storage/src/mail.rs` keeps three files:
`base.jsonl` append-only and immutable, `cursors.json` holding a
per-name high-water seq and its readers, and `seen.jsonl`, a dedup
memory deliberately outliving the entries it remembers so a pruned
message cannot be re-accepted. Every mutation runs under the stage lock,
and a writer that cannot take the lock now fails rather than proceeding.

Each entry carries a self-signed envelope: eight canonical header fields
joined by NUL, an ed25519 signature over the header and the text, and a
msgid that is the hash of all three. `self` resolves to the box's own
node name at mint time, so a locally filed letter is byte-identical to
the same letter arriving over the wire, and no msgid is ever computed
over a literal that means something else elsewhere. The two writer seams
that used to file into the inbox — conduct's local delivery and the
server's A2A inject — now call one three-argument `file_receipt`, with
the same best-effort tolerance: a filing failure never turns an
already-delivered message into a reported failure. Six `mail` commands
replace the three retired `inbox` ones, and the golden command list
moves 81 to 84. The legacy file migrates on first store open, inside the
same critical section, and is renamed rather than deleted.

A same-day fix (`facd6c1`) closes two holes the store's own premise
opened. `append_audit` now clamps the message it writes to 512 bytes on
a character boundary, because the generic per-dispatch audit call passes
a command's outcome message through as its payload — and for `mail show`
and `mail read`, that message *is* the rendered letter. The mailbase
promises `mail rm --older-than` is the only pruning, which an unbounded
second copy in `~/.aoide/log` broke. The audit log records that an
operation happened, not what it printed. The clamp sits inside
`append_audit`, so every door and every future caller inherits it. The
migration also reads the seen set once before its loop and skips a row
already in it: a crash between filing row N and the end-of-loop rename
left the legacy file in place, and the retry duplicated rows 1..N under
fresh sequence numbers.

Mail writes no audit code of its own. Every CLI dispatch is already
audited by the dispatcher, and each mutating command's outcome message
already names its msgid, seq, or count — an existing convention, not a
gap. The wire half, the outbox, the `mailDeposit` method and the
`message` capability are P-M2 and later; `mail show`'s rendering is
basic here and hardens in P-M5, which is also where bare `aoide mail`
grows its caller's-own half.

Pages touched: `ingest/log.md` (this entry).

## [2026-09-07] fix | the mailbase's cursors were never per reader

MAIL.md contradicted itself and P-M1 built the wrong half. Principle 7
rules per-reader cursors in the NNTP `.newsrc` shape, and the doorbell
latch waits on "that reader's cursor" — but the Store section's file
listing gave `{ "<name>": { "seq", "readers": [...] } }`: one
high-water mark per mailbox name, with the readers merely listed
beside it. The executor built the listing, faithfully, and both the
orchestrator's read and a full review pass checked the code against the
same wrong line.

The consequence was silent mail loss, live on this box. Two agents
reading one name consumed each other's letters, and a read from any
terminal without a session id advanced the mark for everybody.

Cursors are now `{ "<name>": { "<reader>": { "seq": n } } }`, and the
page says what a reader is, which it never did. A reader is the
conducting session id, falling back to the mailbox name itself when
there is none, so a stray read from an unconducted terminal moves a
pseudo-reader and never a live agent's mark. Two unconducted terminals
still share that pseudo-reader — the honest floor, since nothing
distinguishes them. A respawned session is a new reader and sees the
name from the beginning, the way an NNTP client with no `.newsrc`
does: re-delivery is the safe direction where silent loss is not.

The live `cursors.json` already carried the flat shape, so it migrates
on first open under the same lock the inbox migration uses. Every
listed reader inherits the name's old seq; a name with no listed reader
keeps its mark under the name. A name is old-shape only when its `seq`
is a bare number, which stays true even for a reader named `seq`, so a
second pass is a clean no-op and needs no sentinel file. Nobody
re-reads what they had already read.

The defect was found by a Codex session reading the store against the
doc and reporting it as mail. The finding was verified in the code
before anything moved; the letter's other content was not acted on,
because `mail send --from` is attribution and not authentication, and
mail is inert data by ruling.

## [2026-09-07] update | P-M2's design questions, answered before dispatch

Writing the P-M2 brief surfaced four more places the design read two
ways. Each is settled in MAIL.md and PAIRING.md rather than left for an
executor to guess.

`message` is an explicit grant and never a default. Decision 5's
heading said deposits are ungated for paired nodes while its body
required the node to hold the capability, so a freshly paired node
could be read as either able or unable to deposit. It cannot: `message`
widens the way `spawn` does, because opening a mail link is the moment
another node may fill this box's keep-all mailbase, and it is also when
the bearer-token fix falls due. PAIRING.md's closed capability
vocabulary gains `message` and states the same default rule from the
pairing side.

`.bsy` is taken non-blocking, the way BSO means it: a drain that finds
a link busy skips it. The phase's test line said two drains
"serialize", which on a twelve-second cadence would queue tick after
tick behind one stalled ssh dial and starve every other link.

An outbox entry retires when its delivery is confirmed, and what
confirmation means depends on the entry. A letter needs the
destination-signed ack. A receipt needs only the deposit outcome, since
an ack is itself what confirmation looks like and acks are never acked
— read literally, the old rule left every ack in the spool forever.
Acks stay spooled, so one survives a dead link rather than vanishing on
it.

Admission is a JSON-RPC error and the envelope's fate is a result. The
refusal vocabulary never covered a caller that lacks `message`, and
returning that as a successful result would file it as `ok` through the
door's own error heuristic. An audit that records a refusal as a
success is worse than a blunt error code.

Pages touched: `ingest/log.md` (both entries).

## [2026-09-07] fix | a fact about the world, written down once

Three defects found in one session share a single shape, and the shape is
worth a name: a fact about the world captured at write time and never
re-derived at read time. The mail cursor held one high-water mark per
mailbox and served it to every reader. `aoide graph` served the
`conductable` flag it was handed at session registration. The A2A door's
`has_socket` tested that a stored string was non-empty. Each was true when
written and none of them re-checked.

The socket pair matter together because they share a cause outside the
code. `shellbridge.service` owns `/run/user/<uid>/aoide` through systemd's
`RuntimeDirectory`, whose default `RuntimeDirectoryPreserve=no` deletes the
directory on every stop — so a rebuild silently unlinks every live session's
control socket while `sessions.json` still names it. A session keeps its
open descriptor on the now-unlinked inode and cannot be reached through the
path; no command rebinds it, and restarting the unit does not repair it.

Both fixes derive the report at read time and repair nothing. `aoide graph`
gained `is_conductable_now` in `graph/doc.rs`, the single boundary its JSON,
its Unicode tree and its federation wire response all funnel through, so one
check covers three renderings. The A2A door's check went into
`session_ref_lookup`, the one impure boundary `decide_send_action` reads
through, leaving that decision pure and disk-free — the six tests that
construct a `SessionRef` directly pass unedited, which is what proves it.
The stored flag and path are never migrated or cleared: the record says what
was registered, the report says what is true now, and conflating those is
the defect itself.

The A2A case was the worse of the two. `graph` merely printed a hopeful
line; the door told a remote node that delivery was happening down a socket
nothing could open.

The underlying unit change is not ours to make. `modules/nucleus` moves by
upstream merge, and whether the answer is `RuntimeDirectoryPreserve=yes` or
the deeper one — that a paint-side unit should not own core's socket
directory at all, which inverts the core/paint split — is the user's ruling.

Pages touched: `ingest/log.md`.

## [2026-09-07] find | two trees, one score

`lyra reload` and the rebuild both write `run/qml`, from different sources,
with nothing arbitrating them.

A switch builds `run/qml` from the repository's `song/songbook/*/widgets/`
into the store and rsyncs it down with `--delete`, so immediately after a
switch the deployed tree is the committed score. `lyra reload`'s sync beat
copies from `songbook_dir(song)/widgets`, and that resolves under
`$AOIDE_ROOT` — the runtime songbook, seeded once by `rice compose` and
re-synced from the repository by nothing. A switch therefore moves the
deployed tree forward and the next reload walks it back to whatever vintage
the runtime copy happens to be. Found live: the runtime tree was five days
stale, and an ordinary reload reverted battery percentages and widget
widths that had shipped in the meantime.

`reload`'s own design notes claim the command removed the tree-direction
trap. It removed it within the agent loop — edit, reload, look, no separate
save and no separate sync — and left it standing across the rebuild
boundary, which the loop never looks at.

Two smaller things fall out of the same place. The reload's snapshot beat
runs after its sync, deliberately, so that a deterministic re-derivation
does not register as drift on every call; the consequence is that the first
reload after a switch snapshots the already-reverted state, and the
switch-deployed state was never a take at all, so `rice back` cannot reach
it. And the staging and draft arms report `ok` even when the shell reload
returns `Failed`, so an agent reading the status believes its edit is on
screen when the message beside it says the IPC call failed. `NotRunning` is
right to stay `ok` — no live shell is the ordinary headless case, not an
error — but a failure is not.

The fix waits on a ruling about which tree is the score. Adding a staleness
guard would preserve both trees and decide nothing.

Pages touched: `ingest/log.md`.

## [2026-09-07] refactor | Mneme shared-memory proposal

Moved the proposal from the repository-root `references/` directory into
`references/mneme-shared-memory-and-aoide.md` in this wiki, and registered it in
the catalog and notes manifest. The source remains a proposal; its system
comparison identifies the reviewed local revisions rather than claiming the
same features are deployed or present on upstream main.

The draft defines separate persistent agent identities, versioned profiles,
selected skills, scoped memories, and task-specific context. Shared project
knowledge stays canonical; cross-agent sharing uses explicit grants or
publication into shared collections. Runtime tool permissions remain with
Aoide and the harness. The first milestone includes isolation and handoff
acceptance checks. The initial proposal was drafted on 2026-09-07; its change
record is maintained here.

Pages touched: references/mneme-shared-memory-and-aoide.md, SCHEMA.md,
ingest/index.md, ingest/log.md. The misplaced repository-root copy was removed.

## [2026-09-07] build | mail reaches another box

`aoide mail send` now crosses to a directly paired node. Four things had
to arrive together and none of them works alone: a `message` capability
joining the closed pairing vocabulary, an `aoide/mailDeposit` method on
the one A2A door, a `state/outbox/<node>/` spool with per-link backoff,
and the commands that drive them. A deposit is admitted or refused by
policy, and a well-formed envelope's fate is reported separately from
whether its sender was allowed to speak at all.

The spool is BSO's, deliberately. A link's `.bsy` is taken
non-blocking, so a drain that finds a link busy skips it rather than
queueing behind a stalled dial and starving every other link. An entry
retires when its delivery is confirmed, and what confirmation means
depends on the entry: a letter waits for the destination's signed ack, a
receipt needs only its own deposit outcome, because an ack is what
confirmation looks like and acks are never acked.

Two defects followed, and both were the specification's rather than the
code's. The first: the door returned `bad-msgid` and `unverified-origin`
as JSON-RPC errors, where MAIL.md rules that only admission is an error
and everything after it an outcome. CONTRACTS.md documented the error
shape too, so code and contract agreed with each other and both
disagreed with the design authority. Correcting the door alone would
have been half a fix — the drain classified only a JSON-RPC `error`
member as a refusal, so a peer answering the specified refusal *result*
read as delivered, and a letter's entry then sat spooled forever waiting
on an ack that was never coming. This box could not have talked to a box
that implemented the specification faithfully, including a later version
of itself.

The second was smaller and the same shape. The corrected drain matches a
response's status carefully — accepted and duplicate mean delivered,
refused and anything unrecognised mean the entry did not land — and then
resolved a missing status field to the literal "accepted", walking past
the match entirely. For a receipt, whose delivered arm removes the
outbox record outright, a malformed reply with no status at all
destroyed the local record on a response that confirmed nothing. A
missing field is weaker evidence than an unrecognised one, and the
unrecognised one already parked.

The pattern across all three of this phase's defects is that a
specification can be wrong in a way no review tier catches, because
every tier is checking the code against the spec. The refusal shape was
found only because a reviewer read MAIL.md and the code as two
independent claims about the same thing rather than as source and
implementation.

Pages touched: `ingest/log.md`.

## [2026-09-08] rename | the codex dendrite becomes openai

`modules/dendrites/codex.nix` installed `pkgs.codex` and stopped there.
The tool it named grew a second half — a desktop app — and a toggle
called `codex` could not honestly gate both, so `openai.nix` replaces it
and `aoide.openai.enable` gates the pair (`6551c1c`). The CLI installs
as before; ChatGPT Desktop arrives through the `chatgpt-desktop-linux`
flake, whose NixOS module the dendrite imports and whose launcher
receives the same `pkgs.codex` path. yomi's enable line follows the
rename. Nothing is lost: `pkgs.codex` installs either way.

The lock carries the new input and its `flake-utils`/`systems`
transitives, and moves nixpkgs to `dc5d91f` — which is the revision
generation 193 was already built from. The lock is catching up to the
running system rather than advancing past it.

Two things the old file carried and the new one does not: the dendrite
shape v0 header (CONTRACTS.md §2) and the note that codex is the
hookless harness, reaching the session graph through `aoide conduct
--agent codex` rather than a profile row. The second fact survives in
`concepts/orchestration/Agent-Hooking.md` §3, which the old comment
cited; the first is a convention the sibling dendrites still keep.

Verified by evaluation only. `nix eval` of the host toplevel resolves
with no warning from this module; the ChatGPT payload has not been
built, and its fetch override is an open thread above.

Pages touched: `modules/dendrites/README.md`,
`docs/architecture/CODEX-INTEGRATION.md`, `ingest/log.md` (this entry).

## [2026-09-08] refactor | the mneme proposal stops asserting its own design

The draft logged on 2026-09-07 read as settled architecture: one
authoritative store, a directory tree, a division of labour in the
present tense — while being a proposal nobody had accepted. A reader
could not separate the sentences reporting code from the sentences
reporting a wish, and the diagrams decided questions the prose left
open. `183200a` fixes the posture rather than the content.

Four labels now run through the page. CURRENT is behavior found in a
named source snapshot; REQUIRED is a constraint the integration must
hold; PROPOSED is the design being put forward; OPEN is a choice the
document does not make. Every table column and both mermaid charts
carry one, and the directory tree — the worst offender — becomes a
chart of logical references that states in as many words that it
implies no directory move.

Scope narrows to match: shared personas and memory, with the managed
database and replica work split out as an extension the initial
integration does not require. Melete independence becomes a stated
requirement rather than an assumption — shared personas and memory must
work with Melete absent, it being an optional client with a prior
implementation and not a daemon anything else depends on.

That prior implementation is documented from source instead of guessed
at: `wiki/personalities/<name>{.md,/index.md}`, memory at
`<melete_home>/memory/personas/<slug>.md`, `memory_op`'s conditional
writes and process-local append lock, and the lossy slug that lets a
rename select a different file and two distinct names collide on one.
Those are the facts a migration has to survive, so they belong on the
page before the migration is designed.

Snapshots cited: Mneme `5512299`, Melete `7619d40`, Aoide `8df8e60`.

Pages touched: `references/mneme-shared-memory-and-aoide.md`,
`ingest/log.md` (this entry).

## [2026-09-09] feat | the doorbell's latch lands before the doorbell

P-M5a splits in two, one executor each, never concurrent. This is
`P-M5a-1`: the durable state and the safety floor a later ring can be
built on with nothing left to design mid-implementation. `storage::mail`
gains the doorbell's own latch — `rung` on each reader's cursor mark —
and the functions a ringer will call to read and move it:
`ring_targets`, `stamp_rung`, `armed_names_for_reader`, `enrol_reader`,
plus `arms(kind)`, the one place that decides which entry kinds ring. No
line is injected anywhere in this slice and no new command exists yet —
`mail ring` and the socket write it drives are `P-M5a-2`'s.

Two refusals land alongside the latch. A mailbox name is now validated
against the node-name grammar at filing (`file_letter`,
`mint_outbound_letter`) — refused before anything is written, never
clamped; a letter already on disk under an older, off-grammar name
stays filed and readable exactly as before. And `aoide send` stops
trusting a stored socket path whose file is gone: the same
`is_conductable_now` check the session graph already used to REPORT
reachability now gates the SEND path too, so a runtime-directory rebuild
that silently orphans a socket path is refused with a message that says
so, rather than attempted and left to time out.

The concurrency and readiness rulings behind the eventual ring — one
critical section, the readiness signal a target must show, which entry
kinds arm a reader — are answered in the architecture brief this slice
descends from, not repeated here: they constrain code this slice does
not yet contain.

Pages touched: `docs/architecture/MAIL.md`, `CONTRACTS.md`,
`pkgs/aoide/crates/storage/README.md`,
`pkgs/aoide/crates/storage/AGENTS.md`,
`pkgs/aoide/crates/conduct/AGENTS.md`, `ingest/log.md` (this entry).

## [2026-09-10] feat | the doorbell rings

`P-M5a-2`, the other half of the split `P-M5a-1` opened: `conduct::
graph::doorbell` is the actual ringer. Filing an arming letter now
writes a fixed nudge line into the socket of every armed reader whose
wrap is headless and whose hook-fed agent child is sitting at the
prompt — raw injection, no gate, no prefix, no rename, submitted with
the CHILD's own harness key rather than the wrap's, then latched until
that reader's own read catches up. A name no one has read yet enrols
every conducted session whose petname matches before it rings, but only
when nothing is enrolled already — a real recorded reader is never
second-guessed by a coincidence. The whole select-inject-stamp sequence
for one name runs under one dedicated `.ring.lock` file, held across the
real socket write and the submit-keystroke delay — never the ordinary
mailbase stage lock, which stays scoped to brief in-memory writes as it
always has.

Three triggers reach the same ringer: `mail ring --for <name>` by hand,
a remote letter landing over `aoide/mailDeposit` (rung in-process by the
door itself, a receipt never rings), and a reader's own Stop hook
replaying whatever is still armed the moment its turn ends — turning a
deferred ring, mid-turn, into a delivered one without a second trigger
ever needing to arrive. `aoide-client` sits below `aoide-conduct` in the
crate graph and cannot call the ringer directly, so a self-filed
letter's own `mail send` forwards `mail ring` through the resident
daemon instead; no daemon reachable is a reported `"no-daemon"`, never
an error, since the filing itself already succeeded.

One residual is labeled, not solved: an interactive composer —
someone's own terminal, conductable but not headless — is never
auto-submitted into, by design, until a control-layer guard for that
case exists. That guard is `P-M5a-3`, deliberately not this slice.

Verified: the crate-local test suites (`aoide-storage`, `aoide-conduct`,
`aoide-server`, `aoide-client`, `aoide-cli`) and `aoide schema --json`
listing `mail ring`, all in an isolated environment. **Not verified
here: a live headless agent actually being woken by a real ring** — that
run belongs to whoever operates a live headless session, not to this
slice's own isolated test environment, and is called out as undone
rather than assumed.

Pages touched: `docs/architecture/MAIL.md`, `CONTRACTS.md`,
`pkgs/aoide/crates/conduct/README.md`,
`pkgs/aoide/crates/conduct/AGENTS.md`,
`pkgs/aoide/crates/server/README.md`,
`pkgs/aoide/crates/server/AGENTS.md`,
`pkgs/aoide/crates/client/README.md`,
`pkgs/aoide/crates/storage/README.md`,
`pkgs/aoide/crates/storage/AGENTS.md`, `ingest/log.md` (this entry).

## [2026-09-10] fix | every ring executes inside aoided

`P-M5a-2c`, correcting `P-M5a-2` (the previous entry): the architecture
owner rejected the idea that any process linking `aoide-conduct` may
ring in-process under `.ring.lock` alone. The ruling is narrower and
harder: **the resident daemon is the policy and audit boundary for
every ring**, not the lock file, and a lock only ever serializes, it
never authorizes.

A ring now executes only inside `aoided` — an invocation whose door is
`Door::Daemon`. `mail ring` reached from any other door (the CLI, MCP)
forwards the request through `aoide_client::daemon::daemon_dispatch`
instead of ringing locally; a name is still validated before that
forward, so a bad name is refused with no daemon round trip and no name
bytes in the message. With no daemon reachable, nothing is written to
any socket and the caller sees `Outcome::error(cmd, "no daemon
reachable — nothing rung")` with `data.ring == "no-daemon"`. A reader's
own Stop hook replays a deferred ring only when that hook is itself
being handled under `Door::Daemon`; the hook's local no-daemon fallback
replays nothing; the latch stays armed for the next daemon-handled
trigger, and the hook's own outcome is unchanged either way. The A2A
deposit arm (`aoide-server`) no longer rings at all — a remotely
deposited letter still arms its readers exactly like a locally-filed
one, but is rung only by the next daemon-side trigger for that name,
until a later slice (`P-M5b-2`) gives that door its own forward path to
the daemon.

`.ring.lock` itself is untouched: it still wraps the whole
select-inject-stamp sequence, still never the ordinary stage lock. Only
its documentation changes — it is the daemon's own serializer for
concurrent rings (and a restart's brief process overlap), never a
second policy boundary standing in for the daemon.

Verified: the crate-local test suites (`aoide-conduct`, `aoide-server`,
`aoide-client`, `aoide-cli`) and `cargo check --workspace
--all-targets`, all in an isolated environment. Not verified here: a
live headless agent actually being woken by a real ring through this
corrected path — that run belongs to whoever operates a live headless
session, not to this slice's own isolated test environment.

Pages touched: `docs/architecture/MAIL.md`, `CONTRACTS.md`,
`pkgs/aoide/crates/conduct/README.md`,
`pkgs/aoide/crates/conduct/AGENTS.md`,
`pkgs/aoide/crates/server/README.md`,
`pkgs/aoide/crates/server/AGENTS.md`,
`pkgs/aoide/crates/storage/AGENTS.md`, `ingest/log.md` (this entry).

## [2026-09-10] feat | a project spans several roots

A project stops being one directory and becomes a set of anchor roots.
`Project.path` stays the first root, always; a new additive `roots` field
(`storage/src/records.rs`) carries the rest, off the wire when empty, so
every pre-existing one-root project round-trips byte-identical. Every
reader enumerates through `Project::roots()` (path first, then `roots`,
deduplicated) rather than the raw fields — a hand-edited record whose
`path` disagrees with `roots[0]` is read as given, never silently
rewritten.

`project add NAME PATH…` grows that set (or registers a new name); it
never replaces what is already there. `--new` refuses a name that already
exists instead of silently adding a root to it — the guard against a
typo'd name joining the wrong project. `project edit NAME PATH…` is a NEW
command, the exact-replacement editor: it swaps a project's whole root
list for the one given, atomically, and never touches the name or
`autoResume` — `add`/`remove` stay the only ways a project appears or
disappears. `project remove NAME [PATH]` now takes an optional root:
dropping one root promotes the next remaining one into `path`; dropping
the last root (or calling with no `PATH`) drops the whole project,
unchanged from before. `project add`/`project edit` validate EVERY given
path before mutating anything — one bad path in a multi-path call writes
nothing.

`anchor_for` (`conduct/src/graph/model.rs`) now matches the longest root
across EVERY root of EVERY project, not just each project's first — so a
project's own second root anchors sessions exactly like its first, and
nested projects still resolve to the deepest match. `graph.json`'s project
node gains an always-present `roots` array (`CONTRACTS.md` §4 — the
resolved document, no reader ever falls back to `path`); the Unicode tree
and the conductor's PROJECTS panel both grow one indented/dim line per
extra root, one selectable row per project regardless of root count. The
SESSION panel's DAG group header deliberately keeps showing only a
project's first root — extra roots stay the PROJECTS panel's job alone.

Rejected: repeating `path` inside `roots` (would have doubled the primary
root on every read); a `--replace`/`--set` mode bolted onto `add` instead
of its own `edit` command (the exact-replace and grow-by-appending
semantics are different enough to earn separate names); a variadic marker
on the registry's `Arg` schema (the existing leftover-positional handling
already passes extra args through untouched, so nothing there needed to
change); adding extra-root rows to the SESSION panel's DAG header
(duplicating them there would add rows `dag_sel` does not index).

Verified: `aoide-storage` (400 tests), `aoide-conductor` (77 tests),
`aoide-cli` (41 lib tests + integration suites), all green, plus `cargo
check --workspace --all-targets`. Not verified here: `aoide-conduct`'s own
test suite — a concurrent, unrelated, uncommitted hunk elsewhere in this
crate (`who::SessionView` gained a `project` field whose two test-only
call sites, `graph/node_list.rs` and `graph/grant.rs`, were not yet
updated to match) keeps that one crate's test target from compiling,
independent of anything in this entry; also not verified: a live `aoided`/
Quickshell run against the new `roots` key (additive, so a reader that
ignores unknown keys is unaffected, but unproven live).

Pages touched: `CONTRACTS.md`, `docs/BUILD.md`,
`docs/Aoide-Wiki/concepts/orchestration/Session-Graph.md`,
`docs/Aoide-Wiki/concepts/cli/Graph-and-Conduct.md`,
`docs/Aoide-Wiki/concepts/cli/Conductor-TUI.md`,
`pkgs/aoide/crates/storage/README.md`,
`pkgs/aoide/crates/storage/AGENTS.md`,
`pkgs/aoide/crates/conduct/README.md`,
`pkgs/aoide/crates/conduct/AGENTS.md`,
`pkgs/aoide/crates/conductor/README.md`,
`pkgs/aoide/crates/conductor/AGENTS.md`, `ingest/log.md` (this entry).

## [2026-09-10] feat | acknowledged session actions over the shellbridge socket

Every other shellbridge socket command is fire-and-forget: Quickshell writes
a line and never learns whether it landed. `sessionaction` is the one
exception — a closed, five-action whitelist (`undying`, `project`, `kill`,
`createproject`, `editproject`) that re-execs the matching `aoide session
project|kill|grant undying` or `aoide project add|edit` through aoided with
`--json`, and writes exactly one JSON reply line back on the same
connection before closing it, so a click on the song-side session menu gets
a real yes/no.

The whitelist is closed, not a translator: `session_action_args` is the ONE
place that turns a wire action into an argv, and every element of every
argv is validated before it exists — `safe_action_value` (session ids,
project/create/edit names) refuses empty, `-`-prefixed, whitespaced, or
control-charactered strings; `safe_action_path` is a separate, looser rule
for path arguments, since a real path can legitimately contain spaces and
an argv element is passed to `Command::arg` whole, never through a shell.
No generic exec, no value the bridge did not itself validate, ever. The
bridge never pre-checks what the CLI already refuses — `project add --new`
owns "this name exists," not this file — so `createproject` (two
invocations, add-then-assign) stops at the first failure and reports the
partial state honestly rather than rolling back; a half-created project is
a real project, not something to silently unwind.

A confirmed live blocker forced a second change: the accept loop used to
handle connections serially, so ANY idle, persistent client — exactly the
shape of Quickshell's own shared fire-and-forget socket — blocked every
later connection forever. `serve` now spawns one thread per connection
(peercred admission still runs before the spawn); the shared QML socket
stops holding its own link open after each flush, since an idle client is
what caused the wedge, not a virtue. No read timeout was added — idling
between human gestures is legitimate. The one existing read-modify-write
this surfaced, `herald.json`'s ledger edit, previously relied on serial
accept for its safety; it is now wrapped in the crate's existing
stage-lock convention instead, kept deliberately off every CLI re-exec
path to avoid a lock-across-a-child-process-wait deadlock.

Not run here: a live desktop click through an actual Quickshell process —
the QML side (`ShellBridge.qml`'s `sessionAction`/`actionFactory`) is
written from the documented Socket/SplitParser types, not from working
precedent elsewhere in this repo, and is not cargo-checked. Also not run:
`project add --new` / `project edit` end-to-end through this bridge — both
landed in the tree (`feat(project): a project spans several roots`,
`f7eeb87`) during this same session, after this slice's argv shapes were
already written against their registered flag/positional signatures
(verified by reading `commands/graph.rs`'s registration, not by executing
a freshly built binary against live state); a live run is the only thing
that confirms the two agree in practice.

Pages touched: `pkgs/aoide/crates/conduct/src/shellbridge.rs`,
`modules/facets/quickshell/qml/ShellBridge.qml`,
`docs/Aoide-Wiki/concepts/cli/Doors-and-Nodes.md`,
`docs/Aoide-Wiki/concepts/desktop/Widget-Bridge-Contract.md`, `ingest/log.md`
(this entry). `pkgs/aoide/crates/conduct/README.md` and `AGENTS.md` also
carry this slice's doc paragraphs, but landed already committed under
`f7eeb87` (a same-file race in a documented shared-file arrangement, not a
change made by this commit) rather than here.

## [2026-09-10] fix | review fixes for acknowledged session actions

Root's live review of the shellbridge `sessionaction` command (`b62988f`)
found four points; `5f00da9` (`fix(shellbridge): review fixes for
acknowledged actions`, shellbridge.rs only) fixes them and this docs pass
brings the crate docs, the QML protocol comment, and the wiki to the same
fact. The `project` field must be a JSON string: `""` is the clear
request, and a missing key, `null`, or a non-string value is refused by
the whitelist outright, never read as an implicit clear. Session ids and
names are two different checks now — `safe_session_id` (no whitespace)
and `safe_action_value` (ordinary spaces allowed, so "My Project" is a
legal name; empty, `-`-prefixed, and control-charactered values still
refused). The reply forwards the CLI outcome's `data` verbatim under
`"data"` when the envelope has one, so a kill can name the terminal and
pid it stops. The `MAX_ACTION_PATHS` cap on `createproject`/`editproject`
is gone with no replacement; the wire line's own length is the only bound.

Verified: independent review of `b62988f`+`5f00da9` graded the code LAND
(whitelist closed, no generic exec, audit lines carry no argument values,
herald lock off every re-exec path, threaded accept sound) and the docs
FIX-THEN-LAND — this entry is that fix. `cargo test -p aoide-conduct`
572 passed under `TMPDIR=/tmp`; the two socket-binding tests need that
short temp path (a `sockaddr_un` limit under nix develop's long
`TMPDIR`), the same gotcha the server suite already carries.

Pages touched: `pkgs/aoide/crates/conduct/README.md`,
`pkgs/aoide/crates/conduct/AGENTS.md`,
`modules/facets/quickshell/qml/ShellBridge.qml`,
`docs/Aoide-Wiki/concepts/cli/Doors-and-Nodes.md`, `ingest/log.md` (this
entry).

## [2026-09-10] fix | roots are complete and mutations are daemon-owned

Two review fixes to "a project spans several roots" (f7eeb87), landed as
18e5dd5 with this docs pass following.

**Roots serialized complete.** `Project.roots` (`storage/src/records.rs`)
was extras-only on the wire (`skip_serializing_if`, holding the second
root onward), so every writer had to reconstruct `path` + `roots` together
and a reader trusting the raw field saw part of the set. `roots` is now
the FULL ordered root list, `path` mirrored at `roots[0]`, always written
by `project add`/`project edit`/`project remove` — even a one-root project
carries `"roots":["/path"]`. `Project::roots()` is unchanged
(path-first-then-roots, deduplicated) and still tolerates a legacy record
with no `roots` field or a hand-edited one whose `roots[0]` disagrees with
`path`, which is what makes the wire change additive for records on disk.

**Daemon-owned atomic mutations.** `project add`/`project edit`/`project
remove` (`conduct/src/graph/manage.rs`) ran their whole mutation locally
whichever door the invocation came through — the one command family in
`graph/` that never forwarded to `aoided`, unlike `session.project`/
`session.kill`'s `local_daemon` precedent (`graph/actions.rs`). All three
now route through that shape: a `Door::Cli` caller forwards through
`aoide_client::daemon::daemon_dispatch` and gets a real error when no
daemon answers; a `Door::Daemon` caller takes the local path (the
reentrancy guard `daemon_dispatch` already provides). The local mutation
moved into three inner helpers (`add_roots`, `remove_roots`, `edit_roots`),
each run inside ONE `aoide_storage::fs::with_stage_lock` hold end to end,
closing the lost-update race where two concurrent `project add --new`
calls for one name could both observe an empty registry and both win
(`add_roots_new_races_two_threads_for_one_name_exactly_one_wins`). Names
with spaces were already legal and stay so.

Two surfaces now need a live `aoided` for project registration: `aoide
onboard`'s `register_clone` step (`cli/src/commands/onboard.rs`) forwards
the top-level door, so a first-run onboard with no daemon records a
failure note on its project step and still exits 0 — OPEN, ruling
requested, not worked around; and the conductor TUI's `App::dispatch`
always stamps `Door::Cli`, so a project keystroke needs the daemon —
`cli/tests/conductor_integration.rs` proves that path with a fake daemon
thread answering through the real `dispatch` under `Door::Daemon`.

Independent review (FIX-THEN-LAND) also found: CONTRACTS.md's
`projects.json` example still lacked `roots` (fixed here); the
`conduct/AGENTS.md` project bullet still said "rest → roots" and carried
no daemon-ownership invariant (fixed here); `resurrect_one` discards
`assign_project`'s result, so a revived session whose ledger project was
since removed silently falls back to cwd anchoring (LOW, open).

Verified on 18e5dd5: `aoide-storage` 400, `aoide-conduct` 572 (the two
socket-binding tests need `env TMPDIR=/tmp` under `nix develop`),
`aoide-conductor` 77, `aoide-cli` 41 lib + integration suites, `cargo
check --workspace --all-targets` clean. The reviewer re-ran storage (400)
and could not build conduct because the concurrent slice-C executor held
`send.rs`/`identity.rs` mid-edit.

Pages touched: `CONTRACTS.md`,
`docs/Aoide-Wiki/concepts/cli/Graph-and-Conduct.md`,
`docs/Aoide-Wiki/concepts/orchestration/Session-Graph.md`,
`pkgs/aoide/crates/storage/README.md`,
`pkgs/aoide/crates/storage/AGENTS.md`,
`pkgs/aoide/crates/conduct/AGENTS.md`, `ingest/log.md` (this entry).

## [2026-09-10] fix | kill resolves a card to its attested terminal

`session kill --id ID` (`conduct/src/graph/actions.rs`) worked only on a
record that already WAS a conducted wrap (`conductable == Some(true)` +
a dedicated pid) — but the desktop menu passes the NATIVE harness record's
id (`SessionMenu.qml`'s `bridge.sessionAction(record.sessionId, …)`), never
a conductable one, so every "Kill process" click refused. Fixed at two
seams (slice C of the session-QoL assignment, 57154ba).

**Hook-time attested lineage.** `send.rs`'s `session_hook` now carries the
hook process's own real pid across the daemon hop (`HOOK_PID_FLAG`, beside
the existing `STDIN_PAYLOAD_FLAG` — `aoided`'s own pid, read daemon-side,
is useless ancestry evidence; its parent is `systemd --user`). A new
`identity::attested_wrap` (the conducted-ancestor-only sibling of
`attested_sender`) walks that pid's real `/proc` ancestry for a sealed,
`conductable` ancestor, and `session_store::stamp_attested_parent`
re-stamps `parentSessionId` from it — change-only on difference (never
write-once like `hookAncestry`), cycle-guarded, re-staging `graph.json`
since (unlike `hookAncestry`) this field is rendered. `hook_ensure_session`
runs this UNCONDITIONALLY, before its exists/fresh split, so an
ALREADY-registered record gets re-parented too — the actual bug: the old
exists-branch only ever refreshed `pid` and returned. A fresh record's
parent becomes `attested.or(env_parent)` — kernel truth over the ordinary
`AOIDE_SESSION_ID` env fallback, one resolution, no double write.

**Kill-time resolution.** `kill_target` now walks `parentSessionId` from
the requested id (itself the first candidate), up to 32 hops,
cycle-guarded, to the nearest record that is a dedicated conducted
process — the target. The seal is checked exactly once, by
`terminate_verified` against the resolved target, never inside the walk
(a stale seal surfaces its own refusal, never a misleading ancestry
message). The shared-pid refusal now excludes the whole walked chain
(a wrap and its own hook-fed descendants sharing one pid is expected, not
a collision). `session_kill`'s data carries `target`/`pid` alongside the
requested `sessionId`; its message names the terminal being terminated
when it differs from the requested id. One resolution serves both doors —
the shellbridge `sessionaction` kill action re-execs `aoide session kill
--id ID` unchanged.

No new stage field, no new command/flag, no golden change.

Verified: `cargo test -p aoide-conduct` 585 passed, `cargo test -p
aoide-storage` 400 passed, `cargo check --workspace --all-targets` clean.
Not provable in-crate: the composed positive path (hook → attested wrap →
re-stamp) needs a live daemon — this crate's fixtures guarantee a dead
`AOIDE_DAEMON_SOCKET` by design (`isolated_mail_root`), so only the two
seams are proven directly (`identity::attested_wrap`'s walk from a real
spawned child; `session_store::stamp_attested_parent`'s change-only/cycle/
restage semantics against a stage fixture) plus the fail-closed no-daemon
path end to end through two real hooks. The live acceptance still owed:
register a claude session under a conducted wrap, `--resume` it under a
DIFFERENT wrap (or nest a hook-fed child under a wrap), then `aoide
session kill --id <the native card's id>` and confirm the reply's
`target`/`pid` name the current wrap's terminal, that terminal's process
actually receives SIGTERM, and no unrelated live session sharing that pid
gets refused as shared. `CONTRACTS.md`'s `parentSessionId` clause is
stale about its writers independent of this slice — flagged, not fixed.

Pages touched: `pkgs/aoide/crates/conduct/README.md`,
`pkgs/aoide/crates/conduct/AGENTS.md`, `ingest/log.md` (this entry).

## [2026-09-10] fix | the attested walk runs on per-turn hooks only

Two review findings on 57154ba's `hook_ensure_session`, both fixed in
`send.rs` alone (8bfdd24).

**The walk ran on every hook, including the two hottest.**
`hook_ensure_session` ran the attested-wrap walk (a daemon ping for the
seal pubkey, a full sessions.json load, up to a 64-hop `/proc` walk)
unconditionally on all six hook arms, PreToolUse/PostToolUse included. A
terminal's wrap only ever changes across a process restart or a `--resume`
onto a new wrap, never mid-tool-call, and the next per-turn hook catches
it. `hook_ensure_session` now takes `attest: Option<i32>`; `None` skips
the walk before the resolver is called. The `Phase` arm's registration
self-heal and `PhaseIfRunning` (once per turn) pass `Some(hook_pid)`;
`ToolStart`/`ToolEnd`/`SubRekey`/`SubEnsure` pass `None` and keep the pid
refresh / fresh registration unmodified. Inside aoided the seal-key fetch
dials aoided's own socket; the accept loop is thread-per-connection, so
that self-ping completes rather than deadlocking.

**`HOOK_PID_FLAG`'s doc read as if the carried pid were trustworthy by
construction.** It is trusted verbatim from the dispatch socket; a
same-uid process can name any pid. Its doc now carries the same honest
accounting `cross_uid_gate` (`server/src/daemon.rs`) and
`daemon_seal_pubkey_hex` (`client/src/daemon.rs`) already give this
socket: not a channel a same-uid attacker is locked out of, and not a
privilege escalation — `terminate_verified` re-verifies the resolved kill
target's own seal, and same-uid can already signal any process — but a
lied-about pid can still misdirect the re-parenting walk. The flag is
reachable only over the local socket: MCP maps registry-declared flags
only and A2A builds a fixed flag map.

**Injection seam.** `hook_ensure_session_with` is the parameterized body,
the `deliver_local`/`deliver_local_with` seam restated for
`real_attested_wrap`. Tests inject a fixed resolver and prove the composed
positive path (Start-shaped and per-turn hooks re-stamp a stale parent,
a tool hook never calls the resolver, the fresh branch prefers the
attested wrap over the env parent) that 57154ba could only prove
fail-closed.

Open after this entry: the `SessionStart` arm itself registers through
`do_session_start` with the env-derived parent and never runs the walk,
so a resumed record stays stale-parented until its first per-turn hook —
a kill clicked inside that window resolves through the old wrap.

Verified: `cargo test -p aoide-conduct` test result: ok. 587 passed; 0
failed; 0 ignored; 0 measured; 0 filtered out; finished in 45.32s. `cargo
check --workspace --all-targets` Finished `dev` profile.

Pages touched: `pkgs/aoide/crates/conduct/README.md`,
`pkgs/aoide/crates/conduct/AGENTS.md`, `ingest/log.md` (this entry).

## [2026-09-10] fix | SessionStart re-parents a resumed record from attested evidence

Closes the gap the previous entry left open (2bd596b): `HookAction::Start`
in `send.rs`'s `hook_for_profile_gated` registered a resumed session
through `do_session_start` with only the env-derived parent (empty on the
daemon-routed path), never the attested one — a `--resume` under a new
wrap left the record stale-parented until its first per-turn hook, and a
`session kill` clicked inside that window resolved through the OLD wrap.

The Start arm now resolves `real_attested_wrap(hook_pid)` the same way the
per-turn hooks do, through the pure helper `start_parent(attested,
env_parent)` (`attested.or(env_parent)`). The resolved parent feeds both
`windowless_by_lineage_from_parent` and `do_session_start`, otherwise
unchanged. The re-parent runs through `do_session_start`'s own upsert
(`aoide_storage::session::upsert_session`), not `stamp_attested_parent`;
`do_session_start_inner`'s existing `would_cycle` guard covers it.

Two tests: `start_parent_prefers_attested_over_env_and_falls_back` pins
the resolution order; `do_session_start_reparents_an_existing_record_
with_a_stale_parent` drives `do_session_start` on a stale-parented
existing record and confirms the upsert re-stamps it.

Verified: `cargo test -p aoide-conduct` test result: ok. 589 passed; 0
failed; 0 ignored; 0 measured; 0 filtered out; finished in 45.34s. `cargo
check --workspace --all-targets` Finished `dev` profile. A final
independent review of 57154ba + 8bfdd24 + 2bd596b together follows.

Pages touched: `pkgs/aoide/crates/conduct/README.md`,
`pkgs/aoide/crates/conduct/AGENTS.md`, `ingest/log.md` (this entry).

## [2026-09-10] fix | the clone registers its project before any daemon exists

`project add/edit/remove` went daemon-owned in 18e5dd5 (`local_daemon` in
`conduct/src/graph/manage.rs`), which silently broke `aoide onboard`'s
clone registration on the documented first-run path (clone, `cd`, `aoide
onboard` — no daemon running yet): `register_clone` called `project_add`,
got `aoided must be running for project management` back as an ordinary
error, and only pushed the message into a notes vec — onboard printed it
and still exited 0 with the project never registered. Fixed in 13ed07e.

`register_bootstrap_project` in `manage.rs` is the onboarding-only entry
that calls the locked `add_roots` mutation directly, bypassing
`local_daemon`; re-exported through `conduct/src/graph.rs`'s existing
`pub use` list; `register_clone` calls it instead of `project_add` and
drops its unused door parameter. A bootstrap failure is no longer
swallowed: the note carries the prefix `project registration failed: `
and `handle_onboard` returns an error (`data.projectRegistered: false`)
instead of an unconditional ok, while the rest of onboarding (songbook
seeding, hook wiring, lyra delegation) still runs since each is useful on
its own. No new command or flag; golden unchanged. Ruling requested from
the orchestrating session across six letters with no answer; the default
was taken and flagged.

Tests: `register_bootstrap_project_writes_under_the_lock_with_cli_equivalent_validation`
(`manage.rs`) and `register_clone_registers_the_project_with_no_daemon_reachable`
(`cli/src/commands/onboard.rs`, `AOIDE_DAEMON_SOCKET` pinned to a dead
path under a unique stage — the exact regression scenario).

Verified: `cargo test -p aoide-conduct` test result: ok. 590 passed; 0
failed. `cargo test -p aoide-cli` test result: ok. 42 passed; 0 failed
(unit) plus the four integration binaries all ok. `cargo check
--workspace --all-targets` clean.

Pages touched: `CONTRACTS.md`,
`docs/Aoide-Wiki/concepts/cli/Graph-and-Conduct.md`,
`pkgs/aoide/crates/conduct/AGENTS.md`, `ingest/log.md` (this entry).

## [2026-09-10] fix | a dead enrolment never blocks the petname fallback

P-M5c-1, the first slice of the interactive doorbell (brief
`p-m5c-brief.md`, adapter-independent), landed as 979eb9d. Live failure
it fixes: a mailbox whose enrolled readers were conducted wraps that have
since died never rang anyone, because the petname fallback ran only when
`enrolled == 0` and the dead enrolments kept the count above zero
(`claude-mail` on this host had exactly that shape).

`storage::mail::RingTargets.enrolled` changed from a `usize` count to
`Vec<String>` of reader keys (the count is `.len()`); `ring_targets` stays
a pure read with no liveness argument, the pseudo-reader exclusion
unchanged. `conduct::graph::doorbell::ring_locked` loads the session
roster once and gates the petname fallback on whether ANY enrolled key
resolves to a record that `is_conductable_now`; a latched but live reader
still blocks it, a dead one (no record, or a record whose control socket
is gone) no longer does. The target walk is unchanged: a dead armed
reader still walks and still reports `unknown`/`not-conductable`.

One existing test moved by fixture, not assertion:
`a_latched_reader_blocks_the_petname_fallback` enrolled `wrap-1` with no
session record, which the new gate correctly reads as dead; the fixture
now binds `headless_wrap("wrap-1")` and the test is renamed
`a_live_latched_reader_still_blocks_the_petname_fallback`. New tests:
storage `ring_targets_names_every_enrolled_reader_not_just_a_count`,
`the_pseudo_reader_is_absent_from_the_enrolled_names`; conduct
`a_stale_enrolment_with_no_session_record_does_not_block_the_petname_fallback`,
`a_stale_enrolment_whose_socket_is_gone_does_not_block_the_petname_fallback`.
`a_recorded_reader_whose_socket_file_is_gone_is_skipped_and_stays_armed`
stays green untouched.

Verified: `cargo test -p aoide-storage` test result: ok. 402 passed; 0
failed. `cargo test -p aoide-conduct` test result: ok. 592 passed; 0
failed. `cargo check --workspace --all-targets` clean. Staged only;
nothing deployed — the live `claude-mail` mailbox stays wedged until a
deployed aoided runs this code.

Pages touched: `pkgs/aoide/crates/storage/README.md`,
`docs/architecture/MAIL.md`, `pkgs/aoide/crates/conduct/AGENTS.md`,
`ingest/log.md` (this entry).

## [2026-09-10] fix | kill refuses subagents and ended hosts; a resume outside any wrap drops its stale parent

Three kill-safety cases codex-integration asked to see tested (P-QOL-C4).
`conduct::graph::actions::kill_target` now checks every hop, the requested
record first, before testing it for wrap-ness: a `sub:`-prefixed record
never resolves and the walk never climbs through one (a subagent shares
its executor's process; refusal `SUBAGENT_SHARES_EXECUTOR`), and an
ancestor hop already `done` refuses with `HOST_HAS_ENDED` while the
requested record keeps its own "session has already ended". The 32-hop
cap, the cycle guard and the chain-excluding shared-pid check are
unchanged. `session_store::clear_stale_parent` (sibling of
`stamp_attested_parent`, change-only, restage on write) is called by the
`HookAction::Start` arm the moment neither the attested wrap nor
`AOIDE_SESSION_ID` resolves: outside any wrap there is no host, so a
resumed record stops pointing at the terminal of its previous run and a
later kill refuses (`NO_DEDICATED_PROCESS`) instead of reaching it.
`kill_target` and `NO_DEDICATED_PROCESS` widened to `pub(super)` so the
hook-path tests can assert the kill outcome directly.

Kill-scope-before-click needs no backend change: the record already
carries `parentSessionId` and `conductable`, so the desktop menu can label
the item "Kill terminal ‹host›" from data it has. That is a song widget
edit, not made here.

New tests: actions `kill_target_refuses_a_subagent_id`,
`kill_target_never_walks_through_a_subagent`,
`kill_target_refuses_an_ended_host`; send
`a_resume_outside_any_wrap_clears_the_stale_parent_so_kill_refuses`,
`a_resume_inside_a_wrap_keeps_the_env_parent` (both drive the real
`SessionStart` hook path through `hook_from_str`). Reviewer flag,
accepted as doctrine: an explicit `session start --parent` on a hook-fed
id is also cleared by a later resume with no evidence of a host.

Verified: `cargo test -p aoide-conduct` test result: ok. 597 passed; 0
failed (executor and independent reviewer both). `cargo check
--workspace --all-targets` clean. Staged only; the deployed aoided
(0.0.22) predates this branch.

Pages touched: `pkgs/aoide/crates/conduct/AGENTS.md`,
`pkgs/aoide/crates/conduct/README.md`, `ingest/log.md` (this entry).

## [2026-09-10] feat | the kill item names its scope before the click

The session menu (`song/songbook/sonata/widgets/SessionMenu.qml`) no
longer reads "Kill process" for every card. `conductor.qml`'s
`openSessionMenu` resolves the card's immediate `parentSessionId` in the
roster it already holds and passes it to `open(rec, x, y, host)`. The
label follows `conduct::kill_target`'s own order and claims no scope it
cannot know: a `sub:` record shows a disabled "Kill (subagent)"; a card
that is itself a conductable wrap with a pid reads "Kill process"; a card
whose immediate parent is a conductable wrap with a pid and not `done`
reads "Kill terminal ‹petname|title|id›"; anything deeper or unknown
reads "Kill…" with the hint that aoide resolves the host on click and
refuses if there is none. Each case carries a one-line hint in the
Undying hint's style. The post-click message is unchanged: the backend
names the target. No bridge or backend change (house rule 7: the data
the label needs was already on the bridge).

Verified: `qmllint` is not in the devshell; brace balance checked on both
files and the three label functions exercised under node against seven
fixtures covering the four cases. The ended-host clause was added by the
orchestrator on review. Staged, not deployed.

Pages touched: `song/songbook/sonata/design/intent.md`, `ingest/log.md`
(this entry).

## [2026-09-10] docs | the task register moves into the repository

`docs/architecture/TASK-REGISTER.md` is the canonical shared register
codex-integration asked for (seq 158/168): six workstreams with status,
owner, dependencies, evidence and next step, plus the carried backlog
with verified status. Corrections applied from seq 168: session QoL is
implementation-and-build complete with live acceptance pending, not
released; the Lyra/AoideOS architecture migration is a first-class
phased workstream (portable core exports → explicit default.nix and
walker removal → songbook/runtime split → draft/stage/declared lifecycle
with hotload → external-consumer proof → host migration), with the docs
and glossary acceptance riding each phase; pairing windows and the canvas
are queued behind it. Root's `PAIRING-WINDOW-PROPOSAL.md` is committed
alongside by explicit pathspec, unchanged. The private session memory now
links to this file instead of holding the register.

Pages touched: `docs/architecture/TASK-REGISTER.md` (new),
`docs/architecture/PAIRING-WINDOW-PROPOSAL.md` (root's, committed as
written), `ingest/log.md` (this entry).

## [2026-09-10] feat | desktop Codex threads get a record shape and a pure reconciler

P-CX-1, the first slice of the desktop Codex/ChatGPT association
(scratchpad brief `p-codex-desktop-brief.md`, Opus). New
`conduct::graph::codex_app` carries `CodexThread { id, cwd, pid }` and
`reconcile_codex_app_threads`, a pure core mirroring
`window::reconcile_untracked_terminals` rule for rule: upsert in place,
remove what is no longer desired, change-only writes. It is keyed by the
Codex thread's own native id, writes a fixed `agent:"codex"` /
`kind:"app"` / `state:"idle"` identity on every upsert, never overwrites
a record another kind already owns, and leaves `windowAddress` /
`workspace` for the existing window sweep. `kind:"app"` is a new value
meaning "a task inside an app aoide does not conduct": `is_agent_kind` is
false for it, so N threads sharing one app window never count as agent
duplicates and a shared app-server pid never stands as proof one thread
is alive. No I/O and no call site yet (two dead-code warnings until
P-CX-2 wires discovery, accepted by review; no `allow` added, the crate
has no such precedent).

Discovery (P-CX-2) was redesigned on the User's requirement that
detection be one repeatable process across operating systems: thread
liveness by a non-blocking `flock` on Codex's lock file (verified held on
this box, the file is empty), ownership by one parsed
`ps -axo pid=,ppid=,command=` table keyed on the app-server argv, `/proc`
only as a Linux tie-break when several app-servers run, other unix enrol
without pid or window as owner-ambiguous, Windows taught-unsupported.

Tests: `a_live_thread_becomes_one_record_keyed_by_its_native_id`,
`two_threads_of_one_app_are_two_records`,
`a_thread_whose_lock_is_gone_loses_its_record`,
`a_record_a_tracked_session_already_owns_is_never_overwritten`,
`an_app_record_is_never_agent_kind_so_dedup_and_staleness_skip_it`,
`an_app_record_never_publishes_a_state_other_than_idle`. Verified:
`cargo test -p aoide-conduct` test result: ok. 603 passed; 0 failed
(executor and independent reviewer); `cargo check --workspace
--all-targets` clean. Reviewed FIX (docs missing from the commit) → docs
landed here in the same commit → LAND.

Pages touched: `pkgs/aoide/crates/conduct/README.md`,
`pkgs/aoide/crates/conduct/AGENTS.md`, `CONTRACTS.md` (§4 `kind` gains
`app`), `ingest/log.md` (this entry).

## [2026-09-10] feat | desktop Codex threads are found by one portable process

P-CX-2, the discovery slice of the desktop Codex/ChatGPT association
(scratchpad brief `p-codex-desktop-brief.md` §4, Opus; Sonnet executor,
independent Sonnet review). `conduct::graph::codex_app` gains the I/O
that feeds P-CX-1's pure reconciler, written to the User's rule that
detection be one repeatable process across operating systems:

- Liveness is a non-blocking `flock` probe on Codex's own
  `~/.codex/thread-writer-locks/<thread_id>.lock` (`lock_is_held`): a
  held lock is a live thread, an unheld one is not, a missing file is
  never created, and a probe that wins the lock releases it before the
  descriptor closes. `/proc` is never consulted for liveness.
- Ownership is one parsed `ps -axo pid=,ppid=,command=` table
  (`process_table` / `parse_process_table`, the same flags on Linux, BSD
  and macOS), read only when at least one lock is held. An app-server is
  an entry whose argv0 basename is `codex` with a bare `app-server`
  token (`codex_app_servers`); a `codex` TUI, an Electron zygote/GPU
  child and a space-containing argument all parse and none match.
- `lock_holder`: one app-server owns every live lock; none leaves the
  pid unset; two or more fall to `holder_via_proc_fd`, a Linux-only
  `/proc/<pid>/fd` tie-break, and elsewhere the thread enrols with no
  pid and no window under one audit line, "codex app-server owner
  ambiguous (<n> servers)". `CodexThread.pid` is `Option<u32>` for
  that case. `cfg(not(unix))` compiles to taught-unsupported
  (`UNSUPPORTED_PLATFORM`), never a second discovery path.
- The thread cwd comes from the rollout header
  (`sessions/**/rollout-*-<thread_id>.jsonl`, first line,
  `payload.cwd`), read once per newly seen thread id, never per tick.
- `codex_home` (CODEX_HOME, else `$HOME/.codex`) is lifted here and
  reused by `reap::refresh_codex_titles` through one `pub(crate) use`
  in `graph.rs`, the `hyprctl_clients` precedent.
- `sync_codex_app_threads` mirrors `sync_untracked_terminal_windows`:
  gather outside the stage lock, reconcile under it, restage on change.
  Still no call site: P-CX-3 wires the daemon tick and the Hyprland
  listener and the `codex-app-unsupported` refusals.

Tests (14 new beside P-CX-1's 6):
`an_ambiguous_owner_enrols_the_thread_with_no_pid_and_no_window`,
`an_electron_table_yields_exactly_one_app_server`,
`a_zygote_or_gpu_child_is_never_an_app_server`,
`a_codex_tui_argv_is_never_an_app_server`,
`a_macos_table_yields_the_same_one_app_server`,
`a_command_argument_containing_a_space_still_parses_its_pid_and_ppid`,
`two_app_servers_leave_the_holder_unresolved_without_the_linux_tiebreak`,
`a_lock_another_fd_holds_reads_as_live`,
`an_unheld_lock_is_not_a_live_thread`,
`a_probe_never_creates_a_missing_lock_file`,
`codex_home_prefers_the_configured_root`,
`a_rollout_header_yields_the_thread_cwd`,
`no_codex_home_does_no_work_and_writes_nothing`,
`the_unsupported_platform_string_names_flock_and_the_process_table`,
plus, from review,
`a_known_thread_keeps_its_cwd_without_a_walk_while_a_new_one_still_walks`.
Verified: `cargo test -p aoide-conduct` test result: ok. 618 passed; 0 failed
(the `codex_app` filter alone: 21 passed; 0 failed); `cargo check
--workspace --all-targets` clean apart from the expected dead_code
warnings for the not-yet-called sync. Reviewed FIX (the rollout walk
ran every tick for known threads; a stale `graph.rs` comment; the audit
tag) → fixed → LAND. Review flag carried to the register: with one
app-server running, every held lock is attributed to it without a
per-lock check, which is the brief's §3 rule but differs from its open
question §6.1; the User decides.

Pages touched: `pkgs/aoide/crates/conduct/README.md`,
`pkgs/aoide/crates/conduct/AGENTS.md`, `AGENTS.md` (root: the portable
capability clause), `ingest/log.md` (this entry).

## [2026-09-10] feat | desktop Codex threads reach the roster and refuse by name

P-CX-3, the last in-crate slice of the desktop Codex/ChatGPT association
(scratchpad brief `p-codex-desktop-brief.md` §4; Sonnet executor,
independent Sonnet review). `sync_codex_app_threads` gains its call sites
and the two reaches that need a transport or a lifecycle are taught to
refuse:

- The reaper's tick calls it immediately before the live-agent title
  refresh, outside every stage lock (it takes its own), so a thread
  enrolled this pass is titled in the same pass; a true return adds one
  line, "reconciled desktop codex threads", to the sweep's change ledger
  because an enrolment or a dropped thread is a roster change. This is
  the PRIMARY call site: it runs wherever `aoided` runs.
- `run_hypr_window_listener` calls it beside every untracked-terminal
  sync (startup, reconnect, the timeout tick, Appeared, Closed), ahead of
  `resolve_pending_session_windows` so the new record is stamped with its
  window in the same pass. Promptness only, on a host with a listener.
- `send` (and through the same `deliver_local_with` gate `--to` and A2A
  inject) refuses a `kind:"app"` record before the conductability check
  with reason `codex-app-unsupported` — the desktop app owns the thread's
  server and aoide has no channel to it — never `not-conductable`, which
  implies a retry. `kill_target` refuses an app hop at the top of its walk
  beside the subagent check with "the desktop app owns this thread's
  process; close the thread in the app instead". Resurrect skips such a
  record structurally: no codex profile, no restore, so no resume argv.
- A third test floor in `lib.rs` pins `CODEX_HOME` for the whole crate:
  the first run of the wired reaper test picked up this box's real live
  thread through the ambient `$HOME/.codex`. Discovery still reads only
  the lock, the process table and the rollout header, never writes, but a
  test must never see the real desktop.

Tests:
`a_codex_app_record_refuses_a_send_as_unsupported_not_not_conductable`,
`the_unsupported_refusal_names_the_app_as_the_server_owner`,
`killing_a_codex_app_record_refuses_without_touching_the_app`. Verified:
`cargo test -p aoide-conduct` test result: ok. 621 passed; 0 failed
(executor and independent reviewer); `cargo check --workspace --all-targets`
clean, the dead_code
warnings for the sync are gone (one remains, `UNSUPPORTED_PLATFORM`, whose
value no non-unix arm cites yet). Reviewed LAND.

Live acceptance is the User's gate and is not claimed: with the desktop app
open on two threads, `aoide session` should show two codex cards under
their own projects, focusing either raises the app window, closing a
thread drops its card within a tick, quitting the app drops all of them.

Pages touched: `pkgs/aoide/crates/conduct/README.md` (the two call
sites), `pkgs/aoide/crates/conduct/AGENTS.md` (the refuse-by-name
invariant), `docs/architecture/CODEX-INTEGRATION.md` (phase 1: association
and identity met in code, lifecycle/hooks/transport explicitly not),
`ingest/log.md` (this entry).

## [2026-09-10] proof | build 2 passes the isolated session-QoL acceptance

The pre-activation acceptance of the combined build
(`/nix/store/8qbyy7xynz3i9hpa7jlm8xh1j4b8ci2z-nixos-system-yomi-strix-…`)
ran against its `aoide`/`aoided`/`lyra` store binaries in a throwaway root
(`AOIDE_ROOT`, `AOIDE_STATE_DIR`, `AOIDE_STAGE_DIR`, `AOIDE_AUDIT_LOG`,
`XDG_RUNTIME_DIR`, `AOIDE_DAEMON_SOCKET` all under `/tmp/aoide-accept-iso`),
with the live socket inodes recorded before and after and found identical.
Two `conduct`-wrapped `sleep` fixtures enrolled; `project add`/`list`/
`edit`/`remove` and `session project --id … --project …` behaved per
`schema --json`; the shellbridge `sessionaction` wire answered a project
change with `ok:true`, an unknown session with `ok:false`, a malformed
action with silence (the menu's timeout case) and a kill with SIGTERM
that left the sleep dead and the record gone after reap; the CLI
`session kill` did the same and refuses a reaped or unknown id with
"session is not registered locally". Two earlier runs failed at daemon
start through the brief's own shell wrapper (an exported function under
`setsid` sees an empty root variable), not through the build. Not
activated, not released: the live roster check waits on the User.

Pages touched: `docs/architecture/TASK-REGISTER.md`, `ingest/log.md`
(this entry).

## [2026-09-10] feat | the core flake exports `overlays.default`

Task 4 phase (a) slice 1 (`p-lyra-migration-a-brief.md` §2.1-2.3).
`pkgs/aoide/flake.nix` gains `overlays.default` beside `packages`;
`lib/mkHost.nix` and `tests/vm-boot.nix` read it instead of each carrying
a hand-copied injection lambda. Proof: through the root flake,
`.#nixosConfigurations.yomi-strix.pkgs.aoide.outPath` and
`.#packages.x86_64-linux.aoide.outPath` are byte-identical, and
`nix flake check --no-build ./pkgs/aoide` passes. Flag: the standalone
`./pkgs/aoide#packages.x86_64-linux.aoide` resolves to a different store
path because `pkgs/aoide/flake.lock` pins a nixpkgs the root's `follows`
overrides — pre-existing drift, not this change. The KVM vm-boot check
was not run. Reviewed FIX→fixed (a stale quotation of the old Wave-0 rule
in `docs/BUILD.md`), landed.

Pages touched: `docs/architecture/PACKAGE-LAYOUT.md` (new "Flake outputs"
section), `docs/BUILD.md` (the Wave-0 rule corrected in two places),
`ingest/log.md` (this entry).

## [2026-09-10] feat | a desktop Codex thread enrols only on positive ownership evidence

P-CX-2b, root ruling seq 180: the "one app-server owns every held lock"
rule is gone. `lock_holder` returns the app-server whose own
`/proc/<pid>/fd` table holds the lock, or nothing — one server or many —
and `codex_app_threads` skips a held lock with no proven owner, so a
terminal `codex` thread never becomes a `kind:"app"` record and never
focuses the desktop window; `CodexThread.pid` is `u32` (a thread without
an owner is not a thread), the lock path is canonicalized before the fd
compare, the "owner ambiguous" audit line is deleted, and `resolved_cwd`
carries the once-per-new-id walk. Tests: one server with no fd owns
nothing; the fd holder is the owner (through a symlinked parent too); the
mixed CLI+desktop held-lock proof drives `lock_holder` and the reconcile
with two throwaway locks, leaving the tracked CLI record byte-identical.
Live evidence (read-only): the one held lock is open in exactly one
process, the `codex … app-server` child of the Electron main. conduct 623
green; reviewed LAND. Flag: `cargo clippy -D warnings` fails crate-wide
under the dev shell's clippy 0.1.97 on pre-existing lints (none in
`codex_app.rs`); a clippy-clean pass is backlog.

Pages touched: `pkgs/aoide/crates/conduct/AGENTS.md` (invariant "a desktop
thread enrols only on positive ownership evidence"),
`pkgs/aoide/crates/conduct/README.md` (`graph/codex_app.rs` bullet),
`docs/architecture/TASK-REGISTER.md` (§2), `ingest/log.md` (this entry).

## [2026-09-10] feat | `packages.lyra` names the paint binary

Task 4 phase (a) slice 2 (`p-lyra-migration-a-brief.md` §2.2-2.4), on
slice 1's `overlays.default`. `pkgs/aoide/flake.nix` gains
`packages.<sys>.lyra = aoide.rice`, and the root flake re-exports it so
`.#lyra` resolves — a consumer names `lyra` instead of writing the
`aoide^out,rice` selector. Proof: `.#packages.x86_64-linux.lyra.outPath`
and `.#packages.x86_64-linux.aoide.rice.outPath` are byte-identical;
`nix build .#lyra` installs `bin/lyra`; `nix flake check --no-build` on
the core flake passes; the fmt and nix-lint checks pass; the yomi-strix
toplevel still evaluates. Flag: the standalone
`./pkgs/aoide#packages.x86_64-linux.lyra` resolves elsewhere — the same
nixpkgs lock drift the slice 1 entry records. Reviewed LAND (one comment
line citation corrected).

Pages touched: `docs/architecture/PACKAGE-LAYOUT.md` (the `lyra` row, the
`packages.<sys>.lyra` contract line, the "deliberately not exported"
list), `ingest/log.md` (this entry).
