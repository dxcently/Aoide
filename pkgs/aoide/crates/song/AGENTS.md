# AGENTS.md — aoide-song

## Invariants

- **This crate is lyra-only and stays nix-independent itself.** `widgets.rs`
  is the one place the workspace spawns `nix-instantiate` — it's here because
  `song` is `lyra`'s domain, but nothing else in this crate may shell out to
  Nix; the ricing engine must apply a song on generic Linux too (root
  `AGENTS.md`, "Nix-independence").
- **`widgets::plan_stage` is the gate, and it runs BEFORE the first write.**
  `rice stage`/`rice mode` call it ahead of the livery write and hand its
  answer to both syncs; a refusal stages NOTHING. Don't move it back inside
  `sync_song_widgets`/`sync_song_registry`, and don't add a write path that
  precedes it — the two refusal texts are the contract
  (`refusal_no_nix`, `refusal_rebuild_needed`).
- **Nothing in the runtime reads a checkout for the songbook.** The generator
  is the SHIPPED file (`<templates>/../nix/manifest.nix`), its argument is the
  MACHINE's songbook (`fs::songbook_root`), and the spawn's
  `ErrorKind::NotFound` IS the no-nix answer — never a `which` probe, never a
  `flake_root()` check, never a `#<flake output>` argv. `$AOIDE_FLAKE_ROOT`
  survives for `rice declare`'s commit-in step only.
- **§7.5's three cases, in order, and case 1 must stay nix-free.** Built in
  with no DIFFERING machine copy (absent counts, and so does a match over what
  the seed SHIPS — `MACHINE_RUNTIME_DIRS`, the machine's own top-level
  `takes/`/`drafts/` (NOT `elements/`, which is song-authored input) — are NOT
  differences in the song, or a snapshot would disable staging on a host with
  no nix and report a built-in song as not built in) stages from the baked
  baseline; everything else is the generator; no nix makes everything else a
  refusal.
- **`MACHINE_RUNTIME_DIRS` names top-level directories, in both consumers.**
  `trees_equal` filters the list at the song folder's root call only, and
  `copy_tree_atomic`'s `skip` matches the root's entries by name only (before
  the file-type branch, so a symlinked or file-typed `takes` is skipped too)
  and passes `&[]` down; a song's own `widgets/takes/` is content, never
  scratch. `lyra rice declare` is the second consumer: the checkout carries
  the song, not the machine's undo history and scratch, so it copies through
  `copy_tree_atomic` with the list and keeps no tree copy of its own. A name
  added to the list changes both §7.5's comparison and what a declare leaves
  behind.
- **A staged song's LENDERS come with it.** `widget_owners` reads the owners its
  manifest entry names and `sync_song_widgets` carries each lender's `widgets/`
  into `run/qml/songs/<owner>/` — a borrowed slot resolves to
  `songs/<owner>/<file>`, so carrying only the borrower leaves a dead slot with
  no error anywhere. A lender with no `widgets/` in the songbook is an ERROR
  naming it, never a skip.
- **The gate runs in the CALLER, before the caller's first write.** Every
  staging caller (`rice stage`, `rice mode stage`, `rice back` — whose first
  write is its drift snapshot — `reload`) calls `plan_stage` before writing
  anything, and an in-crate test pins the `back` case (§9(d) only covers
  `stage`). Don't move the gate down into a sync.
- **A transition that swaps `stage/livery.json`'s type or target plans first and
  writes `mode.json` last.** `rice mode stage`/`rice mode declarative` run the
  template seed and `plan_rice_stage` before `teardown_draft_symlink`, so a song
  that cannot be staged leaves the draft routed and the marker `draft`; the
  shell's livery watch holds an inode and re-arms only on a `mode.json` write
  (CONTRACTS.md §4). A new transition owes the same order.
- **`baseline_songbook` only ever resolves a NO-`_widgets/`-shelf song's own
  entry — it must skip the patch, not guess, when `name` has one.** Borrowed
  ownership resolves only in `composeSong`, and for a built-in song the baked
  baseline already carries that answer; patching it with a scan would DROP
  every borrowed slot. Don't extend `scan_own_entry` to attempt shelf
  resolution.
- **`baseline_songbook` is a THREE-layer merge, not
  baseline-plus-current-song — don't collapse it back to two.** (1) the
  templates dir's baked `manifest.json`/`registry.json`, authoritative for
  every shipped, read-only song; (2) `overlay_surviving_entries` copies the
  EXISTING on-disk file's entries for any song that still has a directory in
  the host songbook on top of that baseline — without this, staging song B
  right after song A silently drops A's entry (A is a runtime composition,
  in neither the frozen baseline nor B's own scan), and `StagingEngine.qml`
  falls back to resolving A's widgets against some OTHER song's slot with no
  error anywhere (the exact regression a review caught live); (3) `name`'s
  own entry, from a fresh scan whenever `name` HAS a directory in the host
  songbook, never the baked file OR the overlay, even for a song that is
  itself shipped in the templates — this is what
  self-heals the STAGED song on every call, mirroring the real `nix eval`
  path's own posture for that one song, while layer 2 is what preserves
  every OTHER still-live song across calls (the nix path doesn't need a
  layer 2 at all — its eval is already total). A song whose songbook
  directory is removed is NOT overlaid — that is the prune, not a bug; don't
  add a "keep it anyway" fallback that would leave an immortal stale key.
  The inverse is also fixed: a shipped song with NO host-songbook directory
  (re-pinned from the declared twin by `rice mode declarative` before any
  seed — `rice stage` has no twin fallback and needs the runtime copy) keeps
  its BAKED entry — layer 3 has nothing to scan there, and an empty patch
  deleted sonata from osaka's manifest and blanked every surface. Directory
  present → scan wins; directory absent → baseline stands.
- **`livery`/`live` stay dependency-free leaves.** `compose`/`cover` are the
  ones allowed to pull in `aoide-storage` (Phase 5b); don't push a storage
  dependency down into `livery`/`live` without re-deriving why that
  boundary existed.
- **A cover carries its song — don't add a rule that reads "the previous
  song".** Every writer of `stage/cover.json` stamps the staged song into it
  (`cover::stage_pick`/`stage_default`), and the ONE rule is read-side: a
  cover applies while its `song` equals the song staged now, and is ignored
  otherwise (`AoideWallpaper.qml`; CONTRACTS.md §4). `cover::stage_for_song`
  keeps a standing pick iff `pick.song == <song being staged>` and otherwise
  writes that song's own default, so `rice stage`, the `rice mode` re-pins,
  `cover set --clear` and the lane's activation seed never need to know what
  was staged a moment ago — the seed touches `cover.json` not at all.
- **Who PAINTS a pick is the HOST's choice — a provider, never a song's
  field.** The fact is `aoide.wallpaper.provider` (nucleus), read at runtime by
  `wallpaper_provider::provider()` off the one word the `lyra` lane publishes at
  `song/stage/wallpaper-provider`; absent means the shell's own layer, which is
  the default and needs no call. `wallpaper_provider::action()` is the whole
  decision — an external provider shows the staged pick while one applies to the
  staged song and the step-aside image otherwise, every other provider gets
  nothing — and `sync()` is the ONE function that runs a provider, best-effort and
  inert under this crate's test build. Call it from every write that changes
  what should show (`cover set`, `--clear`, `rice stage`, `rice back`) and NEVER
  from `--from-skwd`: that door only RECORDS, because the engine's own
  post-processing hook re-enters it and the engine cannot tell an Aoide apply
  from the user's own pick (an unbounded loop, measured).
- **Staging/draft/declarative-mode gating lives in `commands`, not here.**
  `rice stage`/`cover set` refuse outside an unlocked mode — that gate is a
  `commands` concern layered over these pure/near-pure engine modules. The
  record door (`--from-skwd`) is the one exemption: it writes down what an
  external provider is already showing.
- **`commands::rice::handle_rice_stage` is Staging's write path, NEVER
  Draft's — don't call it from a Draft-mode code path.** It reads the
  RUNTIME songbook, with `venue.json` laid over the declared song (see the
  bullet below), and writes the result into `stage/livery.json`. In
  `Staging` mode that file is a plain file, so this is exactly "re-derive
  declared content" — correct by design. While routed into a `Draft`,
  `stage/livery.json` is a SYMLINK into the draft's own file
  (`commands/mode.rs`'s own doc), and `atomic_write` is symlink-transparent
  — calling `handle_rice_stage` there would silently overwrite the draft
  with plain committed truth, destroying the edits draft mode exists to
  hold. `commands/reload.rs`'s own `sync_draft_in_place` is the guard this
  bit live once (`lyra reload` design, settled 2026-08-31) — a Draft-mode
  caller needing the widget-sync/hyprctl-apply tail reuses THAT, or the bare
  `crate::live`/`crate::widgets` primitives directly, never
  `handle_rice_stage`.
- **`commands::rice::stage_terminal_colors` is the ONE writer of
  `stage/terminal-colors.conf`, and every path that rewrites the staged
  livery calls it.** Today: `handle_rice_stage` (so `rice stage`, `rice mode
  stage` and `rice mode declarative`'s re-pin), `rice back`'s restore and
  `lyra reload`'s `sync_draft_in_place`. A new path that writes
  `stage/livery.json` calls it too, off the same notes, or the terminals
  keep the previous song. It adds no mode gate of its own and is never
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
  note or not** (`livery::emit::kitty::synthesised_base16`, the Stylix
  lane's `synthesisedScheme` twin — change both together): a reload reads it
  as config, and a missing slot would show the bake's colour under the
  staged song. **It carries a `background_opacity` line only for a song with
  an opinion** (`live::terminal_opacity`): no opinion writes no line, so
  kitty falls through to the declared fragment and then the host's own
  `background_opacity`, and no cargo constant copies that bake.
  `geometry.terminalOpacity` is linted in `livery::schema` as a plain
  number in [0, 1] or null, through `schema::terminal_opacity_value`, the
  same predicate the hot path uses to find an opinion.
- **`polarity` is a LINTED top-level field and no emitter carries it**
  (`livery::schema::POLARITY_VALUES`, exactly `"light"`/`"dark"`; absent or
  null is "no opinion"). It is the baked fan-out's register — the stylix lane
  reads `aoide.livery.polarity` off the option, never off a stage file — so it
  must stay out of `Resolved` and out of every backend's output. The goldens
  pin that as a contract: `tests/fixtures/valid-polarity.json` is `valid.json`
  plus `"polarity": "dark"`, and
  `polarity_is_lint_only_and_moves_no_emitted_byte` asserts every emitter
  produces byte-identical output with and without it. `rice compose` copies the
  field into a scaffolded `rice.nix` (`aoide.livery.polarity`), defaulting to
  `schema::POLARITY_DEFAULT` when the source notes carry none.
- **A song with no `blurEnabled` opinion restores the baked hyprglass
  switches** (`live::HYPRGLASS_BAKED`, both on), so every
  `geometry_keywords` call carries the two hyprglass keywords — which
  `live::apply_live` partitions out (`live::partition_keywords`) and sends as
  their OWN second `hyprctl --batch`, so a host without the plugin loses its
  glass batch alone and the borders/gaps/blur batch is never entangled with
  it. The predicate is a `contains("plugin:hyprglass:")` on the EMITTED
  keyword, which carries the `keyword ` prefix — matching the bare plugin
  name by prefix silently puts both in the core batch, and
  `every_glass_keyword_the_emitter_produces_lands_in_the_glass_batch` is the
  test that catches it. Keep the
  constant equal to what the compositor lane bakes for a song with NO
  `blurEnabled` opinion — the lane's block takes both keys from that same
  field, so the bake follows the song. `decoration:blur:*`
  keeps the plain no-opinion rule (no keyword). **This crate's own unit
  tests never run `hyprctl` and never reload the live shell** (`cfg!(test)`
  in `live::apply_live`, after the `HYPRLAND_INSTANCE_SIGNATURE` check, and
  at the head of `ipc::quickshell_ipc_reload`): `cfg!` is evaluated when
  *this crate* is compiled, so a `lyra`/CLI integration test, or anything
  else linking this library, still reaches the compositor and the shell —
  and with a batch in every stage, such a handler test run from a Hyprland
  terminal would flip the operator's live glass and borders, and a reload
  rebuilds the whole running scene.
- **`song/declared/livery.json` (the declared twin) and
  `song/declared/venue.json` (the venue's slots and geometry), CONTRACTS.md §4,
  are READ-ONLY for this crate — only the nix side writes them.** The lyra
  lane's activation seed (`home.activation.aoideSeedStage`) publishes them: the
  twin is the declared song's committed notes with the venue's
  `aoide.livery.override` applied and the host's geometry laid over the song's
  own, `"song"` injected, keys sorted; `venue.json` is `venueDelta`, only the
  slots that override recolours and the geometry the host sets, `{}` with
  neither. `commands::rice::notes_source` reads the twin for the re-pin
  (`handle_rice_stage_without_cover`, `rice mode declarative` alone), and
  `commands::rice::declared_song` exposes its `"song"` field
  (`rice mode declarative`'s no-`<name>` resolve uses it, ahead of
  `current_staged_song`). `handle_rice_stage` reads the RUNTIME songbook for
  every song and, for the declared one, `commands::rice::overlay_venue` lays
  `venue.json` over the parsed notes: a colour tier the notes lack is
  skipped, `geometry` is created (the venue sets that value, it does not
  recolour it). **The declared-song test is `"song"` EQUALITY against the name being staged — never a mode, never a mtime,
  never "the twin exists so use it".** The twin describes exactly one song;
  staging any other applies no venue, and re-pinning any other derives from
  that song's own runtime notes. A host that never activated the lane has no
  twin and no `venue.json`, and every reader falls back to the runtime
  songbook — absent is the ordinary no-venue-override case, never an error;
  an unparseable `venue.json` is the one taught refusal, raised before any
  write. Never write these paths from Rust: `rice stage` is a runtime writer
  of the STAGE, and a second Rust writer would race the lane's seed and could
  never compute the override tier the nix evaluator owns — the overlay is a
  merge of data the evaluator handed over, never a second copy of the
  recolour rule.
- **`commands::rice::seed_songbook_from_templates` (task #41) is called
  from the STAGING ENTRY POINTS, never from inside `handle_rice_stage`
  itself.** `handle_rice_stage_entry` (`rice stage <name>`) and
  `commands::mode::handle_mode_stage` (`rice mode stage <name>` — the
  realistic fresh-host first call, since it unlocks AND stages in one go
  where bare `rice stage` just refuses while still locked) each call it
  before handing off to `handle_rice_stage`, the shared sync core that only
  ever READS the songbook. Don't move the seed inside `handle_rice_stage`
  itself — that would also fire it from `rice mode declarative <name>`'s
  re-pin, turning a lock command into a write-adjacent one for no reason.
  `lyra reload`'s staging arm doesn't need it either: it only ever runs
  once `mode.json`'s `song` field is set, which only happens after one of
  the two seed-checking entry points already resolved that song
  successfully — reload calling `handle_rice_stage` directly is therefore
  never a gap. The check itself is dir-level and existence-only
  (`songbook_dir(name).exists()`) — never a per-file fill; a caller adding
  a THIRD staging entry point must call the same function, not reimplement
  the check.
- **`aoide_storage::takes`' functions take `draft: Option<&str>`, not
  `&str` — `None` means staging-mode (`songbook/<song>/takes/`), `Some`
  means a routed draft (`songbook/<song>/drafts/<name>/takes/`).**
  `commands/take.rs::resolve_scope` is the one place that resolves which
  scope applies, off `mode.json` — every take/back/prune command in that
  file threads its `Option<&str>` straight through rather than re-deriving
  it. `TakeRecord` also carries `widgets: Value` (the song's widget bodies
  at mint time, `widgets.rs::snapshot_widget_bodies`) — captured on every
  take in both scopes, but never restored by `rice back` (see the
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
  compared against the song's DECLARED surface set, then a journal
  placeholder-screen line to decide whether a restart is the known cure, not
  a pid/window-address probe), different crate (`lyra`-only, vs `conduct`'s
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
  published `run/qml/songs/surfaces.json`, and `shell_has_zero_layers` is
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
  with `published_surfaces` the single impure reader — that split is what
  makes the judgement unit-testable and is why an absent or malformed
  published file must read as "no expectation declared", never as an
  unhealthy desktop. **An EMPTY published declaration is the same case,
  folded by `asserted_expectation`, not by the parser**: the lane publishes
  the file on every host, so `{"surfaces": {}}` is what every non-declaring
  song ships, and it must reach `shell_has_zero_layers` — routing it to
  `surfaces_fall_short` gives a check with nothing to fail, which reads a
  blank desktop as healthy on exactly the hosts that never opted in.
- **`elements::seed_tree` takes explicit paths and touches no global
  state — `elements::seed_song` is the only env-resolving wrapper around
  it.** Every other elements test exercises `seed_tree`/`render_files`/
  `write_files` directly against a tmp dir, no `AOIDE_STAGE_DIR`/
  `env_lock` needed; only `commands::elements`'s own handler test and
  `seed_song` itself need the env rig. Don't fold `seed_song`'s
  `songbook_dir`/`run_elements_dir` resolution back into `seed_tree` — that
  would make the whole-songbook walker untestable without a stage override,
  the same split `widgets.rs`'s `copy_tree_atomic`/`sync_song_widgets`
  already holds.
- **An element render error must never partially write that element.**
  `render_files` renders every declared file into memory FIRST and returns
  the first error without writing anything; `write_files` runs only once
  `render_files` returned `Ok` for the WHOLE element. Don't merge these two
  passes into one read-render-write-per-file loop — that would leave a
  half-updated `run/elements/<name>/` on a mid-list render failure, exactly
  what ELEMENTS.md's "leaves its old config in place" rules out.
- **Element/song name validation reuses `compose::valid_song_name` —
  don't fork the regex.** `element.json`'s `element` field holds to the
  identical `^[a-z0-9][a-z0-9-]*$` shape `rice compose` already enforces on
  song names; a second copy of that check would drift the moment one of
  them changes.

## Extension points

- **A new `rice`/`livery`/`cover`/`element` command** adds a `cmd!`/
  `register` entry in `commands/`, wired into `lyra`'s `commands::all()`
  only.
- **A new emitter target** (stage/hyprctl/osc/file/kitty exist today) extends
  `livery::emit`, keeping the schema-validate → resolve → emit pipeline
  shape.
- **A new element-descriptor field** extends `elements::Descriptor`/
  `FileEntry`/`RunSpec` (serde, additive) and `parse_descriptor`'s
  validation — `docs/architecture/ELEMENTS.md`'s "The descriptor" section is
  the field-rule authority; `CONTRACTS.md §5`'s "Elements" subsection is
  pinned in the same commit as any change to those rules.

## Docs update required in the same commit

- This `README.md` when a new module or CLI command group is added.
- `docs/architecture/PACKAGE-LAYOUT.md`'s "song rices portably" note if the
  nix-independence boundary shifts.
- `CONTRACTS.md §5`'s "Elements" subsection when a descriptor field rule
  changes.
- `CONTRACTS.md §4`'s "shipped score templates" paragraph when a new
  consumer of `fs::song_templates_dir` is added — `rice compose --from`,
  the widget registry/manifest regeneration, and `seed_songbook_from_templates`
  are the three today.
- `pkgs/aoide/crates/AGENTS.md` for cross-crate invariants — not restated
  here.
