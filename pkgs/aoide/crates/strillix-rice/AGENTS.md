# AGENTS.md — strillix-rice

## Invariants

- **This crate is strillix-only and stays nix-independent itself.** `widgets.rs`
  is the one place the workspace spawns `nix-instantiate` — it's here because
  `rice` is `strillix`'s domain, but nothing else in this crate may shell out to
  Nix; the ricing engine must apply a rice on generic Linux too (root
  `AGENTS.md`, "Nix-independence").
- **`widgets::plan_stage` is the gate, and it runs BEFORE the first write.**
  `rice stage`/`rice mode` call it ahead of the theme write and hand its
  answer to both syncs; a refusal stages NOTHING. Don't move it back inside
  `sync_rice_widgets`/`sync_rice_registry`, and don't add a write path that
  precedes it — the two refusal texts are the contract
  (`refusal_no_nix`, `refusal_rebuild_needed`).
- **Nothing in the runtime reads a checkout for the rices.** The generator
  is the SHIPPED file (`<templates>/../nix/manifest.nix`), its argument is the
  MACHINE's rices (`fs::rices_root`), and the spawn's
  `ErrorKind::NotFound` IS the no-nix answer — never a `which` probe, never a
  `flake_root()` check, never a `#<flake output>` argv. `$AOIDE_FLAKE_ROOT`
  survives for `rice declare`'s commit-in step, and for `host_config()`'s
  default under `rice declare --host`, only.
- **`host_declare` rewrites one string and never guesses.** Exactly one live
  line in nixfmt's shapes (`value_in`, on a line `Lexer` says starts in code,
  not in a `/* */` comment or a string), or a refusal naming the file and the
  count; the bytes between the quotes and nothing else; a temp in the file's
  own directory renamed over it with the mode kept, refused instead
  (`host-config-changed`) when the file's `Stamp` moved since the read, and
  the directory synced after; no git, no rebuild. The `Lexer` is a lexer, not
  a parser: widening it toward Nix semantics belongs in `nix eval`, not here.
  The declarable set is `read_builtin`'s `rices` whose `rice.nix` ships in the
  templates, and nothing wider — the machine's rices is not the build's. A new refusal keeps a `reason` in
  `data` beside the taught refusal, because the shellbridge's `ricedeclare`
  and the bar's balloon read it. The `--host` flag stays CLI-door-only
  (strillix's registration) unless the user opens it.
- **§7.5's three cases, in order, and case 1 must stay nix-free.** Built in
  with no DIFFERING machine copy (absent counts, and so does a match over what
  the seed SHIPS — `MACHINE_RUNTIME_DIRS`, the machine's own top-level
  `history/`/`drafts/` (NOT `elements/`, which is rice-authored input) — are NOT
  differences in the rice, or a snapshot would disable staging on a host with
  no nix and report a built-in rice as not built in) stages from the baked
  baseline; everything else is the generator; no nix makes everything else a
  refusal.
- **`MACHINE_RUNTIME_DIRS` names top-level directories, in both consumers.**
  `trees_equal` filters the list at the rice folder's root call only, and
  `copy_tree_atomic`'s `skip` matches the root's entries by name only (before
  the file-type branch, so a symlinked or file-typed `history` is skipped too)
  and passes `&[]` down; a rice's own `widgets/history/` is content, never
  scratch. `strx rice declare` is the second consumer: the checkout carries
  the rice, not the machine's undo history and scratch, so it copies through
  `copy_tree_atomic` with the list and keeps no tree copy of its own. The copy
  runs only after `checkout_gate::check_folder` finds git clean under that
  rice folder, and `is_old_rice_shape` (the staged `rice.nix` must be a v1
  body, not a `{ … }:` function header) before any byte moves; a new copy path
  takes both guards, and `--host` takes
  `dirty_lines` for its warning and never a refusal. A name
  added to the list changes both §7.5's comparison and what a declare leaves
  behind.
- **A staged rice's LENDERS come with it.** `widget_owners` reads the owners its
  manifest entry names and `sync_rice_widgets` carries each lender's `widgets/`
  into `run/qml/rices/<owner>/` — a borrowed slot resolves to
  `rices/<owner>/<file>`, so carrying only the borrower leaves a dead slot with
  no error anywhere. A lender with no `widgets/` in the rices is an ERROR
  naming it, never a skip.
- **The gate runs in the CALLER, before the caller's first write.** Every
  staging caller (`rice stage`, `rice mode stage`, `rice back` — whose first
  write is its drift snapshot — `reload`) calls `plan_stage` before writing
  anything, and an in-crate test pins the `back` case (§9(d) only covers
  `stage`). Don't move the gate down into a sync.
- **A transition that swaps `stage/theme.json`'s type or target plans first and
  writes `mode.json` last.** `rice mode stage`/`rice mode declarative` run the
  template seed and `plan_rice_stage` before `teardown_draft_symlink`, so a rice
  that cannot be staged leaves the draft routed and the marker `draft`; the
  shell's theme watch holds an inode and re-arms only on a `mode.json` write
  (CONTRACTS.md §4). A new transition owes the same order.
- **`baseline_rices` only ever resolves a NO-`_widgets/`-shelf rice's own
  entry — it must skip the patch, not guess, when `name` has one.** Borrowed
  ownership resolves only in `composeRice`, and for a built-in rice the baked
  baseline already carries that answer; patching it with a scan would DROP
  every borrowed slot. Don't extend `scan_own_entry` to attempt shelf
  resolution.
- **`baseline_rices` is a THREE-layer merge, not
  baseline-plus-current-rice — don't collapse it back to two.** (1) the
  templates dir's baked `manifest.json`/`registry.json`, authoritative for
  every shipped, read-only rice; (2) `overlay_surviving_entries` copies the
  EXISTING on-disk file's entries for any rice that still has a directory in
  the host rices on top of that baseline — without this, staging rice B
  right after rice A silently drops A's entry (A is a runtime composition,
  in neither the frozen baseline nor B's own scan), and `StagingEngine.qml`
  falls back to resolving A's widgets against some OTHER rice's slot with no
  error anywhere (the exact regression a review caught live); (3) `name`'s
  own entry, from a fresh scan whenever `name` HAS a directory in the host
  rices, never the baked file OR the overlay, even for a rice that is
  itself shipped in the templates — this is what
  self-heals the STAGED rice on every call, mirroring the real `nix eval`
  path's own posture for that one rice, while layer 2 is what preserves
  every OTHER still-live rice across calls (the nix path doesn't need a
  layer 2 at all — its eval is already total). A rice whose rices
  directory is removed is NOT overlaid — that is the prune, not a bug; don't
  add a "keep it anyway" fallback that would leave an immortal stale key.
  The inverse is also fixed: a shipped rice with NO host-rices directory
  (re-pinned from the declared twin by `rice mode declarative` before any
  seed — `rice stage` has no twin fallback and needs the runtime copy) keeps
  its BAKED entry — layer 3 has nothing to scan there, and an empty patch
  deleted sonata from osaka's manifest and blanked every surface. Directory
  present → scan wins; directory absent → baseline stands.
- **`theme`/`live` stay dependency-free leaves.** `compose`/`wallpaper` are the
  ones allowed to pull in `aoide-storage` (Phase 5b); don't push a storage
  dependency down into `theme`/`live` without re-deriving why that
  boundary existed.
- **A wallpaper carries its rice — don't add a rule that reads "the previous
  rice".** Every writer of `stage/wallpaper.json` stamps the staged rice into it
  (`wallpaper::stage_pick`/`stage_default`), and the ONE rule is read-side: a
  wallpaper applies while its `rice` equals the rice staged now, and is ignored
  otherwise (`AoideWallpaper.qml`; CONTRACTS.md §4). `wallpaper::stage_for_rice`
  keeps a standing pick iff `pick.rice == <rice being staged>` and otherwise
  writes that rice's own default, so `rice stage`, the `rice mode` re-pins,
  `wallpaper set --clear` and the lane's activation seed never need to know what
  was staged a moment ago — the seed touches `wallpaper.json` not at all.
- **`wallpaper::pick_tiles` is the one tile rule, and `stage_pick` is its one
  writer.** `tile` and `position` are decided for a still by `wallpaper set`
  alone: the record door (`--from-skwd`) and the rice defaults never carry
  them, and every copy (an entry, a draft, a kept pick) carries them verbatim
  without recomputing. Nothing reading the PICK re-derives the `_tile` name
  rule; it takes `tile` as written (CONTRACTS.md §4).
- **Who PAINTS a pick is the HOST's choice — a provider, never a rice's
  field.** The fact is `aoide.wallpaper.provider` (nucleus), read at runtime by
  `wallpaper_provider::provider()` off the one word the `strillix` lane publishes at
  `/stage/wallpaper-provider`; absent means the shell's own layer, which is
  the default and needs no call. `wallpaper_provider::action()` is the whole
  decision — an external provider shows the staged pick while one applies to the
  staged rice and the step-aside image otherwise, every other provider gets
  nothing — and `sync()` is the ONE function that runs a provider, best-effort and
  inert under this crate's test build. Call it from every write that changes
  what should show (`wallpaper set`, `--clear`, `rice stage`, `rice back`) and NEVER
  from `--from-skwd`: that door only RECORDS, because the engine's own
  post-processing hook re-enters it and the engine cannot tell an Aoide apply
  from the user's own pick (an unbounded loop, measured).
- **Staging/draft/declarative-mode gating lives in `commands`, not here.**
  `rice stage`/`wallpaper set` refuse outside an unlocked mode — that gate is a
  `commands` concern layered over these pure/near-pure engine modules. The
  record door (`--from-skwd`) is the one exemption: it writes down what an
  external provider is already showing.
- **`commands::rice::handle_rice_stage` is Staging's write path, NEVER
  Draft's — don't call it from a Draft-mode code path.** It reads the
  RUNTIME rices, with `venue.json` laid over the declared rice (see the
  bullet below), and writes the result into `stage/theme.json`. In
  `Staging` mode that file is a plain file, so this is exactly "re-derive
  declared content" — correct by design. While routed into a `Draft`,
  `stage/theme.json` is a SYMLINK into the draft's own file
  (`commands/mode.rs`'s own doc), and `atomic_write` is symlink-transparent
  — calling `handle_rice_stage` there would silently overwrite the draft
  with plain committed truth, destroying the edits draft mode exists to
  hold. `commands/reload.rs`'s own `sync_draft_in_place` is the guard this
  bit live once (`strx reload` design, settled 2026-08-31) — a Draft-mode
  caller needing the widget-sync/hyprctl-apply tail reuses THAT, or the bare
  `crate::live`/`crate::widgets` primitives directly, never
  `handle_rice_stage`.
- **`commands::rice::stage_rice_conf` is the ONE writer of the rice state
  link, and it runs before `apply_live`.** The compositor's reload reads the
  link, so the link must already name the new target when the reload fires.
  The target is `rice_target_for`: read-only, decided in `plan_rice_stage`, so a
  refused plan links nothing. A stage links the rice's own runtime
  `hyprland.conf`; a re-pin (`rice mode declarative`) links the declared twin;
  either falls back to the empty file. `sync_draft_in_place` calls it too, with
  the rice's own target, never the draft's. Never fatal, reported as
  `riceConf` in the envelope, and idempotent: an already-correct link is
  `unchanged`. Tests cannot observe the order against `apply_live` (inert under
  `cfg!(test)`); the order is the code's position, and a change that moves
  `stage_rice_conf` after `apply_live` breaks the reload's view of the rice.
- **`reload.rs`'s declarative arm reloads the compositor's config, and
  nothing else of the rice.** `live::reload_config` runs `hyprctl reload
  config-only`, guarded on `HYPRLAND_INSTANCE_SIGNATURE` and inert under
  `cfg!(test)` like `apply_live`. It exists because a retargeted link's new
  target is not guaranteed to be followed by Hyprland's own watcher.
- **`commands::rice::stage_terminal_colors` is the ONE writer of
  `stage/terminal-colors.conf`, and every path that rewrites the staged
  theme calls it.** Today: `handle_rice_stage` (so `rice stage`, `rice mode
  stage` and `rice mode declarative`'s re-pin), `rice back`'s restore and
  `strx reload`'s `sync_draft_in_place`. A new path that writes
  `stage/theme.json` calls it too, off the same notes, or the terminals
  keep the previous rice. It adds no mode gate of its own and is never
  fatal. Open windows are reached ONLY through kitty's control socket
  (`live::reload_kitty`: `load-config` with NO path, so kitty re-reads its
  own kitty.conf and every include; a path would replace the whole config
  with that one file), never a raw OSC write into a pty — that interleaves
  with the program's own output and gets eaten. Sockets are found by the
  `kitty-<pid>` name the kitty dendrite's `listen_on` gives them, a
  directory listing: no `/proc` or process-table discovery. **A test build
  never reloads** (`#[cfg(test)]` in `stage_terminal_colors`): handler tests
  run against the real `$XDG_RUNTIME_DIR`, and a reload from one would
  redraw the operator's own terminals. Test the reload through
  `live::reload_kitty` with a stand-in `kitty` on `PATH`, as `live.rs` does.
  Every colour is hex-linted in the `kitty` emitter before it is written:
  the file is an `include` in kitty.conf, so a value carrying a newline
  would be a config directive. **The file carries every colour slot, base16
  note or not** (`theme::emit::kitty::synthesised_base16`, the Stylix
  lane's `synthesisedScheme` twin — change both together): a reload reads it
  as config, and a missing slot would show the bake's colour under the
  staged rice. **It carries a `background_opacity` line only for a rice with
  an opinion** (`live::terminal_opacity`): no opinion writes no line, so
  kitty falls through to the declared fragment and then the host's own
  `background_opacity`, and no cargo constant copies that bake.
  `geometry.terminalOpacity` is linted in `theme::schema` as a plain
  number in [0, 1] or null, through `schema::terminal_opacity_value`, the
  same predicate the hot path uses to find an opinion.
- **`commands::rice::stage_lockscreen` is the ONE writer of
  `stage/hyprlock.conf` and is called wherever `stage_terminal_colors` is**
  (`handle_rice_stage`, `rice back`, `strx reload`'s `sync_draft_in_place`,
  after the §7.5 gate there). Only a rice that ships `lockscreen.<ext>`
  (`wallpaper::lockscreen_image`) dresses the lock: the `hyprlock` emitter writes
  all ten variables with it and comments alone without it, and a rice with no
  picture still rewrites the file, which is what clears an earlier staged
  rice's dress. A dressed file is complete, so a staged rice's picture never
  mixes with the previous one's colours. The picture comes from the RUNTIME
  rices (`shellbridge::rice_dir`), never the checkout or a nix eval. No
  value carries `#` (a hyprlang comment): colours are `rgb(rrggbb)`, and the
  path is refused if it is relative or holds a control character, `#`, `$`,
  `{`, `}` or `\`. Never fatal, no mode gate of its own, and no push: hyprlock
  reads its config at every lock. The file's includer rules live in
  CONTRACTS.md §4, not here; the emitter's fail-closed facts name hyprlock
  0.9.6 and hyprlang 0.6.8 and are re-checked on a bump.
- **`polarity` is a LINTED top-level field and no emitter carries it**
  (`theme::schema::POLARITY_VALUES`, exactly `"light"`/`"dark"`; absent or
  null is "no opinion"). It is the baked fan-out's register — the stylix lane
  reads `aoide.theme.polarity` off the option, never off a stage file — so it
  must stay out of `Resolved` and out of every backend's output. The goldens
  pin that as a contract: `tests/fixtures/valid-polarity.json` is `valid.json`
  plus `"polarity": "dark"`, and
  `polarity_is_lint_only_and_moves_no_emitted_byte` asserts every emitter
  produces byte-identical output with and without it. `rice compose` copies the
  field into a scaffolded `rice.nix` (`aoide.theme.polarity`), defaulting to
  `schema::POLARITY_DEFAULT` when the source notes carry none.
- **`compose::render_rice_nix` emits a rice body (CONTRACTS §5, v1).** The
  scaffold is an attrset with a `system` half (the ten `strillix` providers,
  `STRILLIX_SLOTS`, and a `module` of theme) and a `home` half (`strillix`, and
  `rice.hyprland.conf = ./hyprland.conf`). `rice compose` writes the
  `hyprland.conf` of `--from` when it ships one, else `render_hyprland_conf`'s
  header with no keys. Any change to the body shape is a §5 change: the test
  `compose_*` in `commands/rice.rs` pins the emitted names.
- **`live::apply_live` resets, then overlays.** It runs `hyprctl reload
  config-only` as its own best-effort call, then the keyword batches
  (`live::hyprctl_calls` is the pure order), so a field a rice has no
  opinion about shows the HOST's baked value, whatever the previous rice or an
  out-of-band `hyprctl keyword` set. Do not name a baked value in this crate
  (a constant is right on one host and wrong on another): only the compositor
  knows its host's. Every caller gets the reset — `rice stage`,
  `rice mode stage`/`declarative`, `rice back`, `strx reload`'s draft sync. A
  rice with no `blurEnabled` opinion therefore sends no `decoration:blur:*`
  and no hyprglass keyword. The two hyprglass keywords, when sent, are
  partitioned out (`live::partition_keywords`) as their OWN second `hyprctl
  --batch`, so a host without the plugin loses its glass batch alone. The
  predicate is a `contains("plugin:hyprglass:")` on the EMITTED keyword, which
  carries the `keyword ` prefix — matching the bare plugin name by prefix
  silently puts both in the core batch, and
  `every_glass_keyword_the_emitter_produces_lands_in_the_glass_batch` is the
  test that catches it. **This crate's own unit
  tests never run `hyprctl` and never reload the live shell** (`cfg!(test)`
  in `live::apply_live`, after the `HYPRLAND_INSTANCE_SIGNATURE` check, and
  at the head of `ipc::quickshell_ipc_reload`): `cfg!` is evaluated when
  *this crate* is compiled, so a `strillix`/CLI integration test, or anything
  else linking this library, still reaches the compositor and the shell —
  and with a config reload in every stage, such a handler test run from a
  Hyprland terminal would reset the operator's live compositor and flip its
  glass and borders, and a shell reload rebuilds the whole running scene.
- **`/declared/theme.json` (the declared twin) and
  `/declared/venue.json` (the venue's slots and geometry), CONTRACTS.md §4,
  are READ-ONLY for this crate — only the nix side writes them.** The strillix
  lane's activation seed (`home.activation.aoideSeedStage`) publishes them: the
  twin is the declared rice's committed notes with the venue's
  `aoide.theme.override` applied and the host's geometry laid over the rice's
  own, `"rice"` injected, keys sorted; `venue.json` is `venueDelta`, only the
  slots that override recolours and the geometry the host sets, `{}` with
  neither. `commands::rice::notes_source` reads the twin for the re-pin
  (`plan_rice_stage(inv, true)`, `rice mode declarative` alone), and
  `commands::rice::declared_rice` exposes its `"rice"` field
  (`rice mode declarative`'s no-`<name>` resolve uses it, ahead of
  `current_staged_rice`). `handle_rice_stage` reads the RUNTIME rices for
  every rice and, for the declared one, `commands::rice::overlay_venue` lays
  `venue.json` over the parsed notes: a colour tier the notes lack is
  skipped, `geometry` is created (the venue sets that value, it does not
  recolour it). **The declared-rice test is `"rice"` EQUALITY against the name being staged — never a mode, never a mtime,
  never "the twin exists so use it".** The twin describes exactly one rice;
  staging any other applies no venue, and re-pinning any other derives from
  that rice's own runtime notes. A host that never activated the lane has no
  twin and no `venue.json`, and every reader falls back to the runtime
  rices — absent is the ordinary no-venue-override case, never an error;
  an unparseable `venue.json` is the one taught refusal, raised before any
  write. Never write these paths from Rust: `rice stage` is a runtime writer
  of the STAGE, and a second Rust writer would race the lane's seed and could
  never compute the override tier the nix evaluator owns — the overlay is a
  merge of data the evaluator handed over, never a second copy of the
  recolour rule.
- **`commands::rice::seed_rices_from_templates` (task #41) is called
  from the STAGING ENTRY POINTS, never from inside `handle_rice_stage`
  itself.** `handle_rice_stage_entry` (`rice stage <name>`) and
  `commands::mode::handle_mode_stage` (`rice mode stage <name>` — the
  realistic fresh-host first call, since it unlocks AND stages in one go
  where bare `rice stage` just refuses while still locked) each call it
  before handing off to `handle_rice_stage`, the shared sync core that only
  ever READS the rices. Don't move the seed inside `handle_rice_stage`
  itself — that would also fire it from `rice mode declarative <name>`'s
  re-pin, turning a lock command into a write-adjacent one for no reason.
  `strx reload`'s staging arm doesn't need it either: it only ever runs
  once `mode.json`'s `rice` field is set, which only happens after one of
  the two seed-checking entry points already resolved that rice
  successfully — reload calling `handle_rice_stage` directly is therefore
  never a gap. The check itself is dir-level and existence-only
  (`rice_dir(name).exists()`) — never a per-file fill; a caller adding
  a THIRD staging entry point must call the same function, not reimplement
  the check.
- **`commands/refresh.rs` is the only writer of a built-in rice's record
  (`fs::rice_record(rice)`: `{schemaVersion, rice, source, files:
  {relpath: sha256}}`) and the only place the three-way rule lives.** Per
  file, S = shipped, M = machine, R = recorded: M = S is in-sync (record S);
  M differs and R = M is stale (replace, record S); M differs and R != M is
  edited, and M differs with no R is unrecorded (both kept); S present and M
  absent is new (add) without an R and machine-deleted (not re-added) with
  one; S absent and R = M is gone (delete, prune empty parents, never the
  rice root, drop R), S absent and R != M is kept and R dropped, S absent
  with no R is machine-only and untouched. Top-level `history/`/`drafts/`
  (`widgets::MACHINE_RUNTIME_DIRS`) and machine-side symlinks are never
  touched; a symlink in the shipped copy is an error; a symlinked rice
  folder is skipped. The record lives OUTSIDE the rice folder so the §7.5
  `trees_equal` gate, `rice declare`, `rice list`, `rice history` and the
  widget sync never see it; it is written last and atomically, under
  `with_stage_lock`. `seed_rices_from_templates` goes through
  `refresh_rice` so a stage-time seed records too. Changing the rule, the
  record shape or the report states updates this entry and the README in the
  same commit. Machine symlinks anywhere on a path are never followed: every
  component from the rice root to a file's parent is checked before a write,
  and a blocked file is kept and reported. Mode fixes never reach
  `history/`/`drafts/`. Refresh never sweeps temp files: it writes through its
  own dotfile temp, not `atomic_write_bytes` (whose stale-temp sweep would
  delete a machine-only `*.tmp.<pid>` file). Names in `builtin.json` pass
  `valid_rice_name` or are reported skipped. The stage-time seed now refuses
  a shipped copy containing a symlink (the old plain copy carried it over)
  and reports nothing seeded when refresh skipped the folder.
- **`aoide_storage::history`' functions take `draft: Option<&str>`, not
  `&str` — `None` means staging-mode (`rices/<rice>/history/`), `Some`
  means a routed draft (`rices/<rice>/drafts/<name>/history/`).**
  `commands/history.rs::resolve_scope` is the one place that resolves which
  scope applies, off `mode.json` — every history/back/prune command in that
  file threads its `Option<&str>` straight through rather than re-deriving
  it. `HistoryRecord` also carries `widgets: Value` (the rice's widget bodies
  at mint time, `widgets.rs::snapshot_widget_bodies`) — captured on every
  entry in both scopes, but never restored by `rice back` (see the
  `handle_rice_stage` bullet above: widget bodies stay git's substrate,
  deliberately out of the revert path).
- **`health.rs` is a second liveness mechanism, deliberately — not a
  violation of `aoide-conduct`'s "don't add a second liveness mechanism"
  rule.** That rule (`pkgs/aoide/crates/conduct/AGENTS.md`) guards ONE
  domain: a killed terminal's PROCESS liveness, owned entirely by `reap`.
  `health.rs` watches a different failure class with nothing in common but
  the word "liveness" — a quickshell process that is very much alive (no
  crash, no exit) but has silently lost its Wayland output and rendered onto
  Qt's internal placeholder screen. Different subject (screen attachment,
  not a session), different predicate (a live `hyprctl layers` reading
  compared against the rice's DECLARED surface set, then a journal
  placeholder-screen line to decide whether a restart is the known cure, not
  a pid/window-address probe), different crate (`strillix`-only, vs `conduct`'s
  core-only `reap`). Don't fold this into `reap` or generalize `reap` to
  cover it — the two mechanisms check unrelated things on unrelated
  subjects, and merging them would only muddy both.
- **`health.rs` never reads an installed, stopped unit as healthy, and never
  restarts one.** `not_running_state` is the pure judgement over
  `systemctl show` text: any load state but `not-found` plus
  `inactive`/`failed` is `NotRunning`; not-found, unparseable and transient
  states are no opinion and fall through to the surface checks
  (`deactivating` returns early). `NotRunning` is report-only because the
  watchdog cannot tell a deliberate stop from a `StartLimit*` park, an unmet
  condition or a unit that was never started.
- **`health.rs`'s bad-state predicate is `surfaces_fall_short` against the
  published `run/qml/rices/surfaces.json`, and `shell_has_zero_layers` is
  only the fallback for a host that published nothing.** The two are not
  interchangeable and both must stay: a total count cannot see a PARTIAL
  loss (the wallpaper recovering while the bar and dock stay bound to a dead
  output is exactly the incident that motivated the declared set), while a
  declared set cannot exist on a host where another shell owns a surface.
  `run_healthcheck`'s tail — the journal gate, the marker, the retry ladder,
  the flapping notification — is shared by both paths and must stay that
  way. **`surfaces_fall_short` must never return `true` when the expectation
  is empty, when `layers`/`monitors` is unreadable, or when
  `real_monitor_count` is 0** — the last is the load-bearing one: with no
  real output there is nowhere to paint, a restart reproduces the
  placeholder state, and standing down is what keeps a blackout from turning
  the watchdog into a restart loop. Hyprland's synthesized `FALLBACK` output
  is excluded from BOTH the demand and the coverage; counting it in one and
  not the other is an off-by-one that fires on every blackout. Keep the
  predicate functions pure (`&Value` in, data out, no filesystem, no shell)
  with `published_surfaces` and `published_components` the only impure
  readers — that split is what makes the judgement unit-testable and is why
  an absent or malformed published file must read as "no expectation
  declared" (or, for `components.json`, "intersect nothing"), never as an
  unhealthy desktop. **The declared set is intersected with the selected
  components before it is asserted** (`asserted_expectation`): the shell
  never instantiates an unselected component, so its surface is never
  expected; a declaration the intersection empties is `Some(empty)` and
  never falls short, while a declaration that was empty to begin with still
  reaches `shell_has_zero_layers`. **An EMPTY published declaration is the same case,
  folded by `asserted_expectation`, not by the parser**: the lane publishes
  the file on every host, so `{"surfaces": {}}` is what every non-declaring
  rice ships, and it must reach `shell_has_zero_layers` — routing it to
  `surfaces_fall_short` gives a check with nothing to fail, which reads a
  blank desktop as healthy on exactly the hosts that never opted in.
- **`elements::seed_tree` takes explicit paths and touches no global
  state — `elements::seed_rice` is the only env-resolving wrapper around
  it.** Every other elements test exercises `seed_tree`/`render_files`/
  `write_files` directly against a tmp dir, no `AOIDE_STAGE_DIR`/
  `env_lock` needed; only `commands::elements`'s own handler test and
  `seed_rice` itself need the env rig. Don't fold `seed_rice`'s
  `rice_dir`/`run_elements_dir` resolution back into `seed_tree` — that
  would make the whole-rices walker untestable without a stage override,
  the same split `widgets.rs`'s `copy_tree_atomic`/`sync_rice_widgets`
  already holds.
- **An element render error must never partially write that element.**
  `render_files` renders every declared file into memory FIRST and returns
  the first error without writing anything; `write_files` runs only once
  `render_files` returned `Ok` for the WHOLE element. Don't merge these two
  passes into one read-render-write-per-file loop — that would leave a
  half-updated `run/elements/<name>/` on a mid-list render failure, exactly
  what ELEMENTS.md's "leaves its old config in place" rules out.
- **Element/rice name validation reuses `compose::valid_rice_name` —
  don't fork the regex.** `element.json`'s `element` field holds to the
  identical `^[a-z0-9][a-z0-9-]*$` shape `rice compose` already enforces on
  rice names; a second copy of that check would drift the moment one of
  them changes.

- **The rice stage is `paths::stage_dir()`, always `$STRILLIX_ROOT/stage`.** `paths` reads `strillix_root()` and nothing else for rice locations: `AOIDE_STAGE_DIR` names the conducting stage only and never relocates a rice path. A test that moves the rice tree pins `STRILLIX_ROOT` (the `paths` tests use `pin_roots`), never the stage override.
- **The rice lock is `lock::with_stage_lock`, on `stage_dir()`.** It is `aoide_storage::fs::with_lock_in(&stage_dir(), f)`; core's `with_stage_lock` locks the conducting stage. The two lock files never guard the same state, and `lock.rs`'s tests prove the split and that each domain still serialises its own writers. Do not route a rice writer through core's lock, or the reverse.
- **The rice move is explicit.** `migrate::migrate_rice_root` runs only from `strx migrate` and from `rice_root_at_start` under `STRILLIX_MIGRATE=1` (the home activation sets it). `rice_root_at_start` with the flag unset moves no rice state and prints one line naming `strx migrate` when old rice state is due. Never call `migrate_rice_root` from a library getter, a core binary or a plain start. `migrate_strillix_state` (`~/.local/state/lyra` to `~/.local/state/strillix`) runs inside `migrate_rice_root`, so it moves under the same flag and in the same activation: Hyprland sources the link from there until the rebuild rewrites its config.
- **The rice move waits for core's legacy state while an aoide binary resolves.** `migrate_rice_root` returns an error, moving nothing, while `aoide_protocol::bin::core_present()` (the check the protocol door makes when it says aoide is installed) holds and `~/Aoide/state` holds state that `migrate_root_once` would move. With no aoide binary the move runs and leaves that state where it is. `legacy_state_pending` checks three things: the legacy `state/` holds state, the root's own `state/` is absent or empty, and `$AOIDE_STATE_DIR` is unset. The legacy audit log is not part of the check, because the aoided units export `AOIDE_AUDIT_LOG`, which keeps `migrate_root_once` from moving it in that process. The wait is pinned by `the_rice_move_waits_while_core_has_not_moved_its_legacy_state` and `the_rice_move_runs_without_aoide_and_leaves_core_legacy_state_in_place` in `migrate.rs`, and by `strillix/tests/core_state.rs`.
- **`rice_status` judges every rice source before anything moves.** The old `~/Aoide/song/stage`, the aoide-root `song/` tree and the strillix root are read together. Rice state in both roots is `Both`; sources that would carry rice state across homes are `Refused`. `migrate_rice_root` returns an error for either, with nothing moved. Two roots named by absolute overrides are an explicit choice (`rice_move_allowed`) and may cross homes; a default root may not.
- **An emptied `song/` becomes a marker file.** `mark_rice_tree_moved` replaces an empty `$AOIDE_ROOT/song` with a regular file, so an old binary that opens `song/stage` fails with ENOTDIR instead of recreating a second live tree. A leftover keeps the directory and is narrated; the marker is never written over it.
- **Theme and wallpaper names move once, without clobbering.** `livery.json` becomes `theme.json` (stage, declared twin, each rice and draft), `cover.json` becomes `wallpaper.json`, `covers/` becomes `wallpapers/`, and each rice's `cover/` becomes `wallpaper/`. A file already at its new name is never overwritten.
- **`rice_conf_link` is the first writer outside `$AOIDE_ROOT`.** It names `<home>/.local/state/strillix/rice.conf` (`<home>` = `aoide_home()`), because the renderer's Nix activation hardcodes that path, so the two halves must name one file. The one way to retarget it is `aoide_storage::fs::atomic_symlink` (a temp link renamed over it), never remove-then-link: a reader must see the old target or the new one. A test that exercises it sets `HOME` under the temp dir.
- **`declared_rice_conf` is `$STRILLIX_ROOT/declared/rice.conf`, never under the stage override.** `empty_rice_conf` is `stage_dir()`'s `empty-hyprland.conf`, so it follows the rice stage and never the conducting override.

## Extension points

- **A new `rice`/`theme`/`wallpaper`/`element` command** adds a `cmd!`/
  `register` entry in `commands/`, wired into `strillix`'s `commands::all()`
  only.
- **A new emitter target** (stage/hyprctl/osc/file/kitty exist today) extends
  `theme::emit`, keeping the schema-validate → resolve → emit pipeline
  shape.
- **A new element-descriptor field** extends `elements::Descriptor`/
  `FileEntry`/`RunSpec` (serde, additive) and `parse_descriptor`'s
  validation — `docs/architecture/ELEMENTS.md`'s "The descriptor" section is
  the field-rule authority; `CONTRACTS.md §5`'s "Elements" subsection is
  pinned in the same commit as any change to those rules.

## Docs update required in the same commit

- This `README.md` when a new module or CLI command group is added.
- `docs/architecture/PACKAGE-LAYOUT.md`'s "rice rices portably" note if the
  nix-independence boundary shifts.
- `CONTRACTS.md §5`'s "Elements" subsection when a descriptor field rule
  changes.
- `CONTRACTS.md §4`'s "shipped score templates" paragraph when a new
  consumer of `fs::rice_templates_dir` is added — `rice compose --from`,
  the widget registry/manifest regeneration, and `seed_rices_from_templates`
  are the three today.
- `pkgs/aoide/crates/AGENTS.md` for cross-crate invariants — not restated
  here.
