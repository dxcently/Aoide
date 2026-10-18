# strillix-rice

Aoide's ricing/design engine: the native theme engine (schema validation,
`{group.key}` deref + component fallback, stage/hyprctl/osc/file/kitty/hyprlock
emitters), `rice compose`'s scaffolding/rendering, wallpaper-art derivation, the
Hyprland geometry keyword live-apply, the staged terminal colours, and the
element render pipeline (non-QML rice targets — waybar, dunst, anything with
a config file). Paint-side — ships in `strillix`, not core.

## Named seams (what it exposes)

- `theme` — the design-token engine: schema, resolve, emit. The schema also
  validates top-level fields NO backend carries (CONTRACTS.md §1's `polarity`,
  a baked-only register the stylix lane reads off the option; §5's `widgets`
  block), so a document may legitimately hold more than the resolved set —
  `tests/theme_goldens.rs`'s polarity fixture is the contract.
- `live` — computes + (best-effort) applies the Hyprland geometry/border
  keyword list a staged notes document implies, over a `hyprctl reload
  config-only` that puts the host's bake back first (`blurEnabled` also
  switches hyprglass's two live enable keys; a field with no opinion sends
  nothing and shows the bake),
  renders the staged terminal colour
  file (`terminal_colors`, through the `kitty` emitter, with a
  `background_opacity` line only when the rice has an opinion:
  `terminal_opacity`) and reloads every open kitty's own config over its
  control socket (`reload_kitty`: `kitty @ load-config` with no path, then
  `ls`, per `$XDG_RUNTIME_DIR/kitty-<pid>` socket, bounded, best-effort). `commands::rice::stage_terminal_colors` is
  the one writer of `stage/terminal-colors.conf` (CONTRACTS.md §4), shared by
  `rice stage`, the `rice mode` re-pins, `rice back` and `strx reload`'s
  draft sync. The lock screen follows the same way: `lockscreen_fragment`
  renders the staged rice's hyprlock dress, and
  `commands::rice::stage_lockscreen` writes `stage/hyprlock.conf` beside the
  terminal file, for a rice that ships a `lockscreen.<ext>` (`wallpaper`).
  The rice state link (`~/.local/state/strillix/rice.conf`, CONTRACTS.md §4) is
  retargeted by `commands::rice::stage_rice_conf` before the live apply: to the
  rice's own `hyprland.conf`, or to the empty staging file when it has none.
  The declarative arm of `strx reload` (`reload.rs`) reloads the compositor's
  config through `live::reload_config`, and no rice file otherwise.
- `compose` — the pure `rice compose` scaffolding engine: the rice body
  (`rice.nix`, CONTRACTS §5 v1) and the `hyprland.conf` header it starts from.
- `wallpaper` — wallpaper-art derivation + resolution, and `stage/wallpaper.json`'s
  writers, which stamp the staged rice into every file they write
  (`stage_for_rice`, `stage_rice_default`, `stage_pick`, `staged_rice`;
  CONTRACTS.md §4's wallpaper.json entry). The rule that a wallpaper applies only
  while its `rice` matches the staged rice is read-side — one rule, in
  `AoideWallpaper.qml`. `pick_tiles` decides whether a still pick tiles
  (`wallpaper set --position`, else a `_tile` file name); `stage_pick` writes the
  answer as `tile`/`position`, and the reader paints `tile` under that same
  read-side rule.
- `wallpaper_provider` — the wallpaper provider's bridge: `provider()` (the
  host's provider, off the word the `strillix` lane publishes at
  `/stage/wallpaper-provider`; absent means the shell's own layer),
  `applying_pick()` (a pick that applies to the staged rice, `(kind, identity)`),
  `action()` (the whole decision, pure: `Apply` / `StandAside` / `Nothing`) and
  `sync()` (best-effort, never fatal, inert under this crate's own test build
  like `live::apply_live`). `strx wallpaper sync` runs it, and every write that
  changes what should show calls it; `step_aside` tries the engine's `clear`
  verb first and falls back to the transparent step-aside image
  (`AOIDE_SKWD_WALL_STANDIN`), so swapping the two is one function.
- `elements` — the element descriptor + render pipeline
  (docs/architecture/ELEMENTS.md): parses/validates a rice's
  `elements/<name>/element.json` (v0 — name shape, directory-name match,
  `files[].src`/`dest` traversal, `run.via` ∈ {unit, exec-once}), then
  renders every declared file into `run/elements/<name>/` — verbatim byte
  copy for `template: false`, `theme::emit::file::render` for
  `template: true`. `seed_tree` is the pure whole-rices walker (explicit
  paths, no env, `_`-prefix dirs shelved); `seed_rice` is the thin wrapper
  resolving `rice_dir`/`run_elements_dir` and the rice's own committed
  theme for it. A render error fails only that one element and leaves its
  existing config untouched — every file for an element renders into memory
  first, and nothing is written until all of them succeed.
- `widgets` — the one place a rices is EVALUATED at runtime (strillix-only;
  core must stay nix-independent — `docs/architecture/PACKAGE-LAYOUT.md`,
  "Nix-independence"). `plan_stage` is §7.5's gate AND the manifest/registry a
  stage writes, in one decision: a rice that is BUILT IN with no differing
  machine copy stages straight from the shipped templates' prebaked
  `manifest.json`/`registry.json` (`fs::rice_templates_dir`), with no `nix` at
  all — "no differing copy" compares content over what the seed SHIPS, so the
  machine's own `history/` and `drafts/` are not differences in the
  rice (`elements/` is NOT in that list: it is rice-authored, so a difference
  there is a real one); anything else is regenerated by the SHIPPED generator
  (`<templates>/../nix/manifest.nix`, `pkgs/strillix-rices`) through one plain
  `nix-instantiate --eval --strict --json` over the MACHINE's own rices
  (`fs::rices_root`). No flake, no network, no checkout: nothing names
  `fs::flake_root` any more. A refusal (not built in and no `nix` on the host;
  or a machine rice needing packages `builtin.json.packages` does not carry)
  stages NOTHING — no theme, no body, no manifest — and the gate runs in the
  caller before its first write, `rice back` included. The body carry follows
  the manifest's own ownership: the staged rice AND every LENDER its entry
  names (`widget_owners`), because a borrowed slot resolves to
  `rices/<owner>/<file>`.
  `snapshot_widget_bodies`
  (`strx reload` design, settled 2026-08-31) captures the same
  `rices/<rice>/widgets/` tree `sync_rice_widgets` copies FROM — never
  `run/qml`'s deployed copy — as the take store's own widget-body payload;
  read-only, no restore counterpart (`rice back` still never touches widget
  bodies — they stay git's substrate).
- `ipc`, `lint` — the rice IPC surface (the Quickshell reload trigger and the
  `aoide-quickshell.service` MainPID probe the healthcheck shares) and
  `rice lint`.
- `health` — the `quickshell healthcheck` watchdog (`src/health.rs`, whose
  module header is the argument). It first reads the unit's state through
  `not_running_state`: an installed unit that is `inactive` or `failed`,
  whatever its load state, is `NotRunning`, reported and never restarted. Its bad-state predicate is
  `surfaces_fall_short` against the declaration published to
  `run/qml/rices/surfaces.json` by the strillix lane's build
  (`CONTRACTS.md §5`, `aoide.layout.surfaces`), read through
  `aoide_storage::fs::run_qml_dir` and folded by `asserted_expectation`: an
  absent, unreadable, or EMPTY declaration (the lane publishes the file on
  every host, so every non-declaring rice ships `{"surfaces": {}}`) means no
  expectation is declared and `shell_has_zero_layers` — the older total
  `aoide-*` count — decides exactly as before. A declared set is first
  intersected with the slots of `run/qml/components.json` (`CONTRACTS.md
  §4`), because the shell instantiates only the selected components: a rice
  that declares a `dock` surface on a host without the dock component does
  not report it lost. An absent or non-object `components.json` intersects
  nothing. A total count cannot see a
  PARTIAL loss (the wallpaper recovering while the bar and dock stay bound
  to a dead output is the incident that motivated the declared set), and a
  declared set cannot exist on a host where another shell owns a surface,
  so both predicates stay. The judgement functions are pure (`&Value` in,
  data out); only `published_surfaces` touches the filesystem.
- `checkout_gate` — the no-archive gate of `strx rice declare <rice>`:
  `check_folder` answers git-missing, not-a-repo, or dirty (the `git status
  --porcelain` lines under the rice folder), and `dirty_lines` is the warning
  `--host` reads. Git history is the archive, so a copy over an uncommitted
  edit is refused, and the refusal names the folder. `is_old_rice_shape` is the
  copy's second refusal: a staged `rice.nix` in the old function-module shape
  (its first brace followed by `:`) is refused before anything is copied.
- `host_declare` — `strx rice declare <rice> --host`'s engine
  (`declare_on_host`, called from strillix's `rice declare` handler): rewrites the
  one live `habit.rice.declared` line in `aoide_storage::fs::host_config()`
  to a rice this build's `builtin.json` carries with its own shipped
  `rice.nix`, refusing a draft, an unknown or lending-only rice, zero or
  several lines, and a file edited between the read and the rename, and
  reports `stagedDiffers` through
  `commands::refresh::staged_differs` (the machine copy's theme and widgets
  hashed against the shipped copy's). CONTRACTS.md, "Declaring a rice from
  the machine", is the contract.
- `commands` — this crate's CLI commands: `rice *`, `theme *`, `wallpaper set`,
  `wallpaper sync`, `element seed`, `rice history`/`take.*`/`rice back`,
  `quickshell healthcheck`,
  and `reload` (`strx reload` design, settled 2026-08-31 — the one
  mode-aware iteration command; absorbed `quickshell reload` outright).
  `rice compose --from <rice>` resolves its source via
  `commands::rice::resolve_from_notes_path` (L-C3, task #107):
  `rice_dir(from)` first, else `<templates>/<from>/theme.json`
  (`fs::rice_templates_dir`) — a repo-less host still has something to copy
  from. Compose TO always writes the host rices under the runtime root,
  never the templates dir. `commands::rice::seed_rices_from_templates`
  (task #41) reuses the SAME `fs::rice_templates_dir` resolver for a
  different write: the first `rice stage <name>`/`rice mode stage <name>`
  for a SHIPPED rice the runtime `rice_dir(name)` lacks entirely copies
  that rice's whole template tree in — once, dir-level never-clobber (a
  rices dir with anything in it, even partially, is left alone), so
  idempotent by construction; the copy is `commands::refresh::refresh_rice`'s
  absent-folder arm, so it also writes the rice's record
  (`fs::rice_record(name)`, outside the rice folder). `strx rice refresh
  [<name>] [--check] [--json]` is the same function on a folder that
  exists: per file, a file the machine never edited takes the shipped
  change, an edited one is kept (`commands/refresh.rs` holds the rule). It never follows a machine-side symlink, never
  touches `history/`/`drafts/` modes, and never sweeps temp files.
  Both staging entry points call the seeder before
  `handle_rice_stage` (the sync) ever reads the rices; `rice mode
  declarative`'s re-pin and `strx reload`'s staging arm reuse
  `handle_rice_stage` directly and need nothing extra, since by then the
  rice already resolved through one of the two entry points. `element seed
  <rice>` (L-E1) is the shell-reachable
  bridge to `crate::elements::seed_rice` — the render pipeline's only
  caller today; `rice stage` (L-E2) and the elements lane's activation
  hook (L-E3) call the same function later, not a fork of it.

## What it consumes

`aoide-protocol`, `aoide-storage` (`aoide_storage::fs::strillix_dir`/
`flake_root`/`rice_templates_dir`/`rice_dir`/`rice_notes`/
`declared_notes`/`declared_venue`/`run_elements_dir`), plus `aoide-test-support` as a
dev-dependency.
`theme`/`live` are otherwise dependency-free leaves.

`declared_notes` (`/declared/theme.json`, CONTRACTS.md §4) is the
DECLARED rice's own notes, venue override applied and the host's geometry laid
over the rice's own — published by the strillix lane's activation seed, read-only
here.
`commands::rice::notes_source` reads it for `rice mode declarative`'s re-pin
whenever its `"rice"` field equals the name being re-pinned (never otherwise),
because declarative mode is declared truth;
`commands::rice::declared_rice` is the same field exposed to `rice mode
declarative`'s no-`<name>` resolve and to the staging writers, which derive
every rice from `rice_notes` (the runtime rices, where a rice is edited)
and, for the declared rice, lay `declared_venue` over it
(`commands::rice::overlay_venue`: `/declared/venue.json`, the slots the
venue recolours and the geometry it sets; absent is the identity, unparseable
is a taught refusal). A host with no twin falls back to `rice_notes`
unchanged.

## How it composes

Only `strillix` depends on it. **Rices portably**: applying a rice can't require
Stylix/NixOS-module plumbing, so this crate itself stays free of that
assumption even though its sole consumer (`strillix`) is nix-dependent.
