# Aoide — Log

## Open Threads

### [2026-07-25] open: minor subjects deferred as mentions
Deferred (not yet page-worthy): den (dropped prior art), Style Dictionary, the W3C design-tokens format, Magi. Promote to pages if they accrue independent claims. (Melete and Mneme promoted to entity pages 2026-07-26; the herdr agent-guide pattern captured as the [[Terminal-Commander]] concept 2026-07-26.)

### [2026-07-25] open: drachma schema v1
Provisional v0 in use; design-system v1 supersedes and the update playbook migrates. Close when v1 lands (see [[Notes]]). (v0 grew two optional tiers 2026-07-27 without a version bump: `palette.hot` and the all-or-nothing `base16` block, both drachma-validated — still v0.)

### [2026-07-26] open: post-skeleton build backlog
The walking skeleton (commit f3ceadf, see [[Codebase]]) is complete and verified; the deeper build is queued. Deliberately local for now — the repo has no git remote (user's call), so Melete fleet registration and its code-task flow wait until one exists. The backlog, roughly in order:
- Functional rice loop: real `rice gen` (palette from prompt/wallpaper), `rice preview` writing `song/stage/drachma.json` through [[aoide-notes]], `rice adopt` with the gated rebuild, `rice transpose`.
- `aoide onboard` (first-boot flow) and `aoide update` (merge + contract-bump detection).
- Content pipeline verbs made real (`content register→query`, quarantine, liner dogfood).
- `aoide make` — the [[Widget-Maker]] loop.
- Quickshell NotificationServer spike (actions + inline reply); greetd IPC in AoideGreeter.
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
- **QML deploy target vs source** — the quickshell facet's home-manager activation deploys the QML tree to `~/Aoide/qml/` at the repo root, so on the live box the deployed copy now sits **untracked in the fork's working tree**. The relationship between source (`modules/facets/quickshell/qml/`) and deploy target needs a decision: commit the deployed tree, point activation elsewhere, or symlink.
- **First-boot desktop verification pending** — the switch is live, but nobody has logged into the greetd → Hyprland → Quickshell session in this state; the surfaces (bar, dock-popup hot edge, launcher) are unproven on real glass.

### [2026-07-27→28] closed (mostly): page ingest + lint pending for 9e2eb61..8f062df
The 2026-07-27 log entry below is **log-only**: 37 commits (the port tail, the rice going live, drachma, baton-ratatui, the hero song, the four Pantheon rounds, the session write door) are summarized in the log but NOT yet worked into the pages. Pending: concepts/entities updates (at least [[Codebase]], [[Session-Graph]], [[Gadget-Dock]], [[Quickshell]], [[aoide-cli]], [[drachma]], [[shellbridge]], [[Self-Ricing]], [[Notes]]), `ingest/index.md` glosses, and a lint pass that settles the manifest — `design/Pantheon-Grammar.md`, `design/Baton-3D-DAG.md`, `references/pantheon/**` (13 stills), and `references/dxflake-rice-screenshot.png` are on disk but unmanifested (known drift; `design/` may also need a SCHEMA.md shape decision — it is a new top-level directory).
**Closed 2026-07-28** (see the refactor/index/lint entries below): the `design/` shape decision is made — `design/` is now a manifested kind folder (SCHEMA shape + Notes manifest + index `## Design` section), the four design pages carry frontmatter, and the pantheon `.png` stills stay assets (not notes), so they are intentionally unmanifested. drachma/sonata/identity/count facts swept across the pages; [[Song-Anatomy]] minted. Residual (still owed): a *deeper* re-ingest of the 37-commit tail into [[Codebase]]/[[Quickshell]]/[[Gadget-Dock]] beyond the surgical fact-fixes done here, and the design/songbook content migration into `song/` (its own `[refactor]` flag in [[references/AOIDE-DEV-HANDOFF]] §7).

### [2026-07-27] open: user-hand gates (the day's landings wait on khoa)
- **Merge + gated switch** — everything from c782689..8f062df runs live only via the worktree QML drop-in + hot-reloaded stage files; the flake side (screenshot/vision dendrites, hyprglass, hero song, base16 terminals, session verbs on PATH) needs merge to main + [[Rebuild-Gate]] switch. Post-switch cleanup: the temporary systemd drop-in and the `result-aoide` fallback path inside `.claude/settings.json`.
- **`.claude/settings.json`** (session-registration hooks) — written and live from disk, but committing it (and copying it to the main checkout) is classifier-reserved for the user's hand.
- **Wiki is not a git repo** — all wiki changes are unversioned saves; `git init` is the user's call.
- **Mixed commit 3d03d95** — optional manual split (`git reset --mixed 8567ff1`), user's hand only.
- **Baton 3D DAG** — planned in `design/Baton-3D-DAG.md`; implementation awaits the user's green light.

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
- `stage/tokens.json` → `stage/drachma.json`; per-song `tokens.json` → `drachma.json`; `aoide.tokens` option → `aoide.drachma`; `token package` → `note package`; `token schema v0` → `drachma schema v0`.
- KEPT W3C design-tokens format and Style Dictionary by their external names; Notes.md states the mapping: notes are Aoide's design-token layer, the container stays W3C design-tokens.
- Frontmatter tag `tokens` → `notes` in Full-Architecture and Notes.md; SCHEMA Tags updated accordingly.
- SCHEMA manifest: `concepts/Design-Tokens.md` → `concepts/Notes.md`; file count unchanged (40).
- Open Thread "token schema v1" → "drachma schema v1"; wikilink updated to `[[Notes]]`.

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
- Minted 3 pages: `concepts/Codebase.md` (how the built repo composes — flake inputs/outputs, `lib/walk.nix` walker + `/_` shelving, `lib/mkHost.nix` walked-tree + host + HM + stylix + the `pkgs.{aoide,aoide-notes}` overlay, `lib/checks.nix` surface-ownership + no-song-read, the `modules/nucleus/options.nix` option contract, the systemd user-unit map, socket + stage-file contracts, repo-file roles, build/verify recipe, real-vs-stubbed status); `entities/aoide-cli.md` (the `aoide` Rust binary — the 19-command tree with real/stub/gated breakdown, `--json` + exit-code conventions 0/1/2/64, `schema --json` as single source of truth + `stageNotesVersion`, MCP one-schema-two-doors, `AOIDE_NOTES_BIN`→PATH location, the `aoided` second binary); `entities/aoide-notes.md` (the Node note engine wrapping Style Dictionary — lint/resolve/emit, the three emitters with atomic stage write, drachma schema v0, `packages.aoide-notes` + overlay attr).
- Updated 6 existing pages with implementation facts (short additions, not rewrites): `entities/aoided.md` (unit name, `AOIDE_AUDIT_LOG`/`AOIDE_USER` seams, JSON-lines audit, propose-only gate), `entities/shellbridge.md` (`aoide shellbridge --run`, socket path, sessions/hooks stage files + v0 shapes), `entities/Quickshell.md` (DrachmaState/ShellBridge singletons, widget files, install to ~/Aoide/qml via HM, hyprland.conf owned by HM — compositor mkBefore / quickshell mkAfter), `entities/Stylix.md` (stand-down on both module layers filtered by option existence; provisional base16 synthesis), `entities/Melete.md` (the `aoide-melete-adapter` unit + `AOIDE_ADAPTER_SUBSCRIBE` allow-list + metadata-only notification boundary), `concepts/Snowflake-Anatomy.md` (walker + overlay now implemented in `lib/`).
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
- **Baton + toolchain** (1664018, 45633f2, 774c43d, 531704e, 0117a79): the conductor's TUI ported to ratatui/crossterm (DAG panel, roster lens); `aoide rice lint/preview` made real, `graph view` restages after mutations, honest `--help`/unknown-flag handling (43 tests); `pkgs/` walker self-registration; the note engine renamed **drachma**.
- **The first song** (4459c7b, 6f6443f, cf43447, 8567ff1): `hero` — dusk-plum palette from `song/covers/hero.webp`, wallpaper reads `stage/cover.json`; **hyprglass** plugin on the bar/dock surfaces; square-corner BW window key; a real all-or-nothing **base16 tier** in the nucleus options → stylix (terminals speak "pantheon bw").
- **The Pantheon design loop, rounds 1–4** (b960332, ab484ea, 279c0e8, 3d03d95, a3e92fd, 8f062df — run as reviewed agent rounds against `references/pantheon/`): wireframe depth stacks over the aero glass → neon dominance (one blaze against a dim field, drawn kinked leaders, glyph unification ♪𝄐𝄽𝄂) → tear-off floating gadgets with trace → the multicolor field (base16 semantic roles wireCyan/holoBlue/violet/glitchPink through drachma + DrachmaState), vanishing-point depth directions (all offset stacks lean toward screen centre), orchestration callout vocabulary (`gadgets.case`, `baton.control`, `terminals.roster`, `dag.trace`, …), workspace numbers at true screen centre, the 𝄞 clef sized to fit the strip (pop-out apron tried and rolled back), rose accents purged from the bar. Full grammar: `design/Pantheon-Grammar.md` (agent-maintained, new).
- **The session write door** (fa600ae): `aoide graph session start/phase/end/hook` — the missing writer for `sessions.json`/`hooks.json` (the shellbridge socket remains a skeleton; these CLI verbs are the interim registrar). The `hook` verb reads a Claude Code hook payload from stdin and never exits non-zero; `.claude/settings.json` hooks (SessionStart/UserPromptSubmit/Stop/SessionEnd) wire every harness session through it. **This closed the user-visible defect "agents are invisible to the widgets/baton"** — the dock now renders the real session DAG. 53 Rust tests.
- **Known blemish**: 3d03d95 is a mixed commit (a shared-index `git add && git commit` swept a concurrent agent's staged round-3 files into the bar-v4 commit; gates re-run green; splitting requires the user's hand — classifier blocks history rewrites in the shared worktree).
- **Wiki file additions this session** (unmanifested until the next lint): `design/Pantheon-Grammar.md`, `design/Baton-3D-DAG.md` (the planned ratatui 3D wireframe DAG renderer — spatial projector, braille canvas, ANSI-frame widget embed; implementation awaits the user's green light), `references/pantheon/` (5 stills + 8 art-direction stills), `references/dxflake-rice-screenshot.png`; `entities/shellbridge.md` + `entities/aoide-cli.md` edited in place (session verbs documented, socket noted still-future).
- **Status**: everything above is live on yomi-strix ONLY via the worktree QML drop-in + hot-reloaded stage files; the flake side (dendrites, hyprglass, hero song, base16 terminals, session verbs on PATH) waits on the user's merge + gated switch.

## [2026-07-28] refactor | design protocol → songbook model + Song-Anatomy
- **Reworded the design protocol to point at a songbook under `song/`** (khoa steer: per-song design memory is the song agent's domain, not the dev wiki). Added a framing block to `design/Ricing-Protocol.md` and `design/Pantheon-Grammar.md`: the dev wiki documents architecture/protocol; per-song and cross-cutting *design memory* (palette rationales, opacity numbers, the visual grammar) belongs to the song agent in `song/songbook/` (cross-cutting) + `song/repertoire/<name>/liner/` (per-song). Existing content is reframed as "what the songbook records, mirrored here," not deleted; noted the songbook is sparse/aspirational today (`song/songbook/` is a placeholder). Kept the load-bearing protocol (creation/application split, the vision-check) and QML constants in the wiki.
- **Minted [[Song-Anatomy]]** (`concepts/Song-Anatomy.md`) — the performed-half sibling of [[Snowflake-Anatomy]]: the role of every `song/` subfolder verified against the real tree (`repertoire/<name>/` {rice.nix, drachma.json, liner/} · `songbook/` · `covers/` · `keys/` · `chimes/` committed; `stage/` · `backstage/` · `auditions/` gitignored runtime), committed-vs-runtime, who writes each, and the six `stage/` files (drachma/sessions/hooks/projects/graph/cover.json). Songs on disk: `sonata` (cream Alma-Tadema light key, selected) + `hero` (dusk-plum).
- **Handoff flag added** — `[refactor · khoa]` in [[references/AOIDE-DEV-HANDOFF]] §7: design/songbook content should migrate into `song/`; protocol reworded to point there; content migration pending. Also closed the §7 `[decision] notes vs drachma` flag (drachma won, shipped — commit 0117a79).

## [2026-07-28] index | design/ + Song-Anatomy + references manifested
- `ingest/index.md`: added [[Song-Anatomy]] under Concepts; added a `## Design` section (Ricing-Protocol, Pantheon-Grammar, Conductor-Channel, Baton-3D-DAG) and a `## References` section (AOIDE-HANDOFF, AOIDE-DEV-HANDOFF).
- `SCHEMA.md`: added `design/` to the shape diagram + the read-whole-thing path; added `concepts/Song-Anatomy.md` and the four `design/*.md` pages to the Notes manifest; snapshot bumped 2026-07-26 → 2026-07-28; tag set extended (baton, conductor, dag, design, glyph, orchestration, pantheon, protocol, pty, song, tui). Pantheon `.png` stills stay assets, not manifested notes.
- Frontmatter backfilled on `design/Pantheon-Grammar.md` (had none) and `design/Baton-3D-DAG.md` (had none) with `type: design`.

## [2026-07-28] lint | full wiki — drachma/sonata/identity/counts sweep
- **Songs**: fixed the light key attribution — the cream Alma-Tadema *Unconscious Rivals* key is **`sonata`** (currently selected on yomi-strix), not `hero` (which reverted to its dusk-plum key); `moonlight` is retired (reserved for a planned dark `moonlight-sonata`). Corrected `design/Ricing-Protocol.md`, `design/Pantheon-Grammar.md` (round 5), `entities/Stylix.md`, and the `aoide.song = "moonlight"` replay examples in `concepts/Song-Vocabulary.md` + `concepts/Self-Ricing.md` (→ `sonata`), plus `concepts/Full-Architecture.md` (fixture + tree map).
- **drachma, not notes**: `## Note schema v0` → `## The drachma schema v0` (`entities/drachma.md`, `concepts/Full-Architecture.md`); "Note schema" → "drachma schema" (`concepts/Governance.md`); "the note engine" → "the design-token mint"/"the drachma engine" (`Overview.md`, `ingest/index.md`); `design/Pantheon-Grammar.md` `notes.paletteAccent` → `drachma.paletteAccent`.
- **Identity**: "bundled" → "integrated (not vendored)" for Melete/Mneme (`concepts/Lexicon.md`, `ingest/index.md`). Overview/Widget-Maker framing (AoideOS = distribution/widget-maker; Aoide = shell-only core) already correct — left intact.
- **Command count**: 27 → **28** (verified: the vm-boot check asserts exactly 28; README = 28; `aoide baton` is command 28) in `Overview.md`, `ingest/index.md`, `concepts/Full-Architecture.md` (×2), `concepts/Codebase.md` (×2, with the graph-group delta kept as history).
- **Feature status**: click-to-jump, the window→session socket2 listener, and the holoBlue workspace-hover-preview are already documented (feature agent) across [[Terminal-Commander]]/[[shellbridge]]/[[Session-Graph]] — verified current, not duplicated.
- **Wikilinks**: full sweep — no live broken links. Remaining unresolved forms are OPERATIONS pedagogical examples, immutable dated-log history mentions (`[[Design-Tokens]]`), and `[[aoide-notes]]` (resolves via the `drachma` alias). Kitty opacity corrected to the re-tuned 0.86 (was frozen at 0.60 in Pantheon round 5).
