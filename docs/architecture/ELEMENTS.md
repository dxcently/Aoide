# ELEMENTS — one contract per element, the rice loop off the QML island

The design authority for the elements workstream (task #121): making
non-QML programs — waybar, dunst, a compositor, anything with a config
file — first-class rice targets with the same EDIT/SAVE/DRAFT loop the
QML widgets already have. Everything in "Settled decisions" was
User-decided through the 2026-08-28 design grill; executors do not
relitigate it. Where this document and a brief conflict, this document
wins.

## The problem

Today the rice loop reaches exactly two kinds of surface: QML (staged
via `run/qml` + the stage twin, hot-reloaded by Quickshell's FileView)
and the compositor's live keywords (`hyprctl keyword`, applied by
`rice stage`). Everything else on a desktop — the bar a user actually
runs when it isn't Quickshell's, the notification daemon's config, a
launcher's theme file — is either Stylix-derived (rebuild-gated,
templated by someone else's derivation) or simply outside the loop.
A song cannot carry a waybar config; `rice stage` cannot restage one;
capturing an existing rice from a foreign flake means transcribing
config text by hand. The `livery emit file` template engine already
renders `{{palette.bg}}` into any text file — pure, proven — but its
effectful half ("write the rendered text into some live config") was
explicitly deferred (`song/src/livery/emit/file.rs`, "the deferred
management seam"). This lane completes that seam.

## Settled decisions

1. **Order: substrate → authoring → venue.** Non-QML element rices
   first (the runtime substrate), capture-as-rice second (the
   authoring tool), foreign-flake drop-in polish last (the venue).
   The design center is the EDIT/SAVE/DRAFT loop for element rices.
2. **Runtime mirrors `run/qml`.** The facet renders the declared
   song's element configs into `$AOIDE_ROOT/run/elements/<element>/`;
   units and exec-onces point THERE — never store symlinks in
   `~/.config`. `lyra rice stage` overwrites the staged configs and
   RESTARTS the element; a rebuild re-seeds (stage-beats-declared
   rank, the existing convention). ONE uniform contract per element:
   a config dir + a restart (unit restart, or kill+exec for exec-once
   elements). A per-element OPTIONAL `reload` field (waybar's
   SIGUSR2, `hyprctl reload`) is a later optimization, never
   required. Capability is TIERED per element: hyprland keeps its
   live `hyprctl keyword` seam; niri or anything else is
   restart/rebuild-only until someone adds a descriptor line. No
   bespoke hot-reload engineering per element class.
3. **A song grows `elements/<name>/`** — raw config files
   byte-for-byte plus ONE small descriptor per element (which files
   land where under `run/elements/`, how the element starts, the
   optional reload/restart overrides). The element set flows through
   `aoide.arrangement` (the existing "which TYPES a song brings into
   existence" namespace) and each element declares `aoide.surfaces`
   ownership — waybar owning `bar` makes the Quickshell facet and
   Stylix stand down through the EXISTING
   `surfaceToStylixTargets`/mkForce-false seam. House rule 5's
   whitelist (livery/arrangement/surfaces) does NOT grow.
4. **Capture is verbatim.** Byte-for-byte, frozen into nix as files
   REFERENCED from nix (home-manager/xdg-style seeding from the
   store copy of the song dir) — never config text transcribed
   through an LLM's context. Agents drive capture: the AGENT decides
   which elements exist, which files matter, how each starts; the
   helper verb `lyra rice capture` does only the mechanical freeze.
   Captured configs start with hardcoded colors BY DESIGN; a later
   mint/tokenize pass (designed here, NOT built in this lane)
   rewrites colors into `{{palette.*}}` placeholders, promoting the
   element to the hot-recolor tier.
5. **Three recolor tiers** (table below): live, hot, rebuild-gated.
   Stylix stays the right tool for GTK/Qt; each tokenization removes
   an element from the rebuild gate.
6. **Drop-in is flake-input shaped.** A foreign flake (dxflake)
   imports Aoide's modules, keeps its OWN layout, sets `aoide.*` in
   host files — chiyo already proves the shape. Zero-ceremony folder
   creation (compose/capture auto-create); NO onboard verb;
   foreign-flake wiring guidance goes into `aoide guide` and docs.
   Aoide-with-lyra ships nix on non-NixOS hosts (standalone
   home-manager), so the Stylix/declarative tier exists everywhere
   lyra runs; core stays nix-FREE (hard constraint).

## The element contract

An element is a program with a config directory and a way to be
restarted. That is the whole contract, and it is uniform:

- **Config dir**: `$AOIDE_ROOT/run/elements/<element>/` — the ONLY
  place the running program reads config from. The unit or exec line
  the facet generates points there; nothing under `~/.config` is
  managed, symlinked, or fought over.
- **Restart**: derived from how the element starts. A unit-run
  element restarts via `systemctl --user restart
  aoide-element-<element>.service`; an exec-once element restarts by
  exact-name kill of its process plus a fresh detached spawn of its
  own exec line. `reload`, when a descriptor carries one, is
  preferred over restart; `restart` in the descriptor overrides the
  derivation entirely. All of it is best-effort and non-fatal — the
  config write is the source of truth, the process poke rides on top
  (the same discipline `rice stage` already holds for hyprctl).

Everything-is-a-plugin holds by construction: an element enters by a
directory existing at `song/songbook/<song>/elements/<name>/`, needs
no import-list edit anywhere, and is removable without trace by
deleting that directory. The `_`-prefix shelving convention applies
(`elements/_waybar/` is skipped everywhere a `_widgets/` slot would
be). The delete-every-qml test passes: with every facet gone,
`lyra element seed` and `lyra rice stage` still drive
`run/elements/` from a bare shell.

## The descriptor

One file per element: `song/songbook/<song>/elements/<element>/element.json`.

```json
{
  "v": 0,
  "element": "waybar",
  "files": [
    { "src": "config.jsonc", "dest": "config", "template": false },
    { "src": "style.css", "template": false }
  ],
  "surfaces": ["bar"],
  "run": { "exec": "waybar -c {run}/config -s {run}/style.css", "via": "unit" },
  "reload": "pkill -SIGUSR2 -x waybar"
}
```

Field rules (pinned in CONTRACTS §5's "Elements" subsection in the
same commit that lands the parser):

- `v` — descriptor version, `0`. Unknown versions refuse with a
  taught error.
- `element` — must equal the directory name and pass the same
  `^[a-z0-9][a-z0-9-]*$` shape `rice stage` already enforces on song
  names (it is joined into paths and process names).
- `files` — the ordered manifest. `src` is relative to the element
  dir; `dest` is relative to `run/elements/<element>/` and defaults
  to `src`. Both are validated against traversal (no `..`, no
  absolute, no escaping their root). `template` defaults to `false`
  (verbatim byte copy); `true` renders the file through the EXISTING
  `livery::emit::file::render` grammar (`{{group.key}}`, group ∈
  palette|base16|bar|notif|window) against the resolved livery. The
  `template` flag is the mint seam: the future tokenize pass flips
  it and rewrites the bytes, and nothing else in the pipeline
  changes.
- `surfaces` — optional list of surface names this element claims
  (`["bar"]`). The elements facet folds these into `aoide.surfaces`
  with `owner = "<element>"`; Stylix and Quickshell stand down for
  them (below).
- `run.exec` — the full command line, with the literal token `{run}`
  substituted at generation time with the absolute
  `run/elements/<element>` path (never expanded at runtime; units
  and exec-once lines both get a finished string).
- `run.via` — `"unit"` (the facet generates
  `aoide-element-<element>.service`, a systemd user unit with that
  ExecStart) or `"exec-once"` (the facet emits a compositor
  exec-once line). Exactly these two; anything else refuses.
- `reload` — optional command; when present, `rice stage` runs it
  instead of the restart after a changed write. This is the tiering
  line from decision 2: hyprland-class elements get their live seam
  by a descriptor line, not by per-class engineering.
- `restart` — optional command overriding the derived restart
  entirely. Absent for almost everything.

## Runtime layout

```
$AOIDE_ROOT/run/elements/            (sibling of run/qml — storage
  waybar/                             fs.rs gains run_elements_dir(),
    config                            resolving exactly as
    style.css                         run_qml_dir() does)
```

Two writers, one rank rule — the same stage-beats-declared convention
`run/qml` and the stage twin already hold. Between rebuilds, `rice
stage` overwrites freely (the sketch); a rebuild reasserts the
declared song's store truth (the truth), exactly as the quickshell
facet's `rsync -a --delete` reasserts `run/qml`.

## Flows

**Stage — the edit loop (this lane's design center):**

```
lyra rice stage <song>
  ├─ livery.json → song/stage/livery.json          (existing, atomic)
  ├─ hyprctl keyword batch                          (existing, live tier)
  ├─ widget QML → run/qml + IPC reload              (existing)
  └─ NEW: for each songbook/<song>/elements/<e>/element.json
       ├─ verbatim files copied, template files rendered
       │    against the STAGED livery → run/elements/<e>/
       │    (atomic per file; a render error fails that element,
       │     leaves its old config in place, reports structured)
       └─ anything changed? → reload if declared, else restart
            (best-effort, per-element status in the Outcome)
```

**Declared — the rebuild:**

```
nixos-rebuild / home-manager switch          (user-gated, house rule 2)
  eval: elements facet readDirs the declared song's elements/
    ├─ aoide.arrangement.elements.<e> = { surfaces, via }   (the TYPE set)
    ├─ aoide.surfaces.<s>.owner = "<e>"                     (claims)
    └─ units / exec-once lines generated, ExecStart → run/elements/<e>/
  activation: lyra element seed <song>
    renders the STORE copy of the song's elements against the
    declared livery → run/elements/   (re-seeds; then EVERY element
    unit is try-restarted unconditionally — the house ruling that
    activation always brings the rice elements back up, same as the
    quickshell facet's aoideRestartRice hook; changed-unit-only
    restarts leave a wedged element invisible to every rebuild)
```

**Capture — the mechanical freeze:**

```
lyra rice capture <song> <element> --config <path>...
                  [--unit <u> | --exec <cmd>] [--surface <s>]...
  ├─ auto-creates song/songbook/<song>/elements/<element>/   (zero ceremony;
  │    the song dir itself auto-creates too, compose-style)
  ├─ copies each --config byte-for-byte, RESOLVING symlinks first
  │    (a ~/.config store symlink freezes as the real bytes it
  │     points at — the rendered ground truth, per decision 4)
  ├─ scaffolds element.json: files manifest (dest = basename),
  │    run from --unit/--exec, surfaces from --surface
  └─ prints what the agent still owns: dest layout, the {run}
       rewrite of hardcoded paths in the exec line, reload, whether
       the capture is complete
```

The agent decides everything above the mechanics — which elements
exist, which files matter, how each starts. Capture never guesses,
never transcribes, never rewrites file contents.

## Recolor tiers

| Tier | Mechanism | Latency | Members |
| --- | --- | --- | --- |
| live | stage-file watch (QML FileView) · `hyprctl keyword` · OSC | instant | Quickshell widgets, hyprland borders/geometry, terminals |
| hot | `{{palette.*}}` templates rendered at stage + element reload/restart | seconds | tokenized elements (`template: true` files) |
| rebuild-gated | Stylix auto-targets the song doesn't own · untokenized captures | one switch | GTK/Qt (Stylix stays the right tool), fresh captures |

Every capture is born rebuild-gated (hardcoded colors by design);
each tokenization pass flips files to `template: true` and promotes
that element to hot. Nothing in this lane builds the tokenizer — the
per-file flag is its complete seam.

## Surface stand-down

`aoide.surfaces` (nucleus options.nix, `attrsOf { owner }`) is
already the one registry; Stylix already derives its target-disable
set from "owner is not stylix" (`surfaceToStylixTargets` +
`presentDisables`, facets/stylix). Two small moves complete the
picture, and the whitelist does not grow:

- The quickshell facet's nine owner declarations become
  `lib.mkDefault "quickshell"`, so an element's normal-priority claim
  wins without an override dance; `checks.surface-ownership` still
  sees exactly one owner per surface, and two elements claiming the
  same surface is the module system's ordinary conflict error —
  correct, loud, at eval.
- Quickshell's rendering of a surface it lost gates on
  `aoide.surfaces.<s>.owner == "quickshell"` (the bar first — the
  waybar case; other surfaces follow the same line as elements claim
  them).

## Phases

Serialized, Sonnet exec + different-Sonnet review each, cargo field
exclusive per phase, per-crate tests only (never `--workspace`), no
rustfmt, pathspec commits. Golden count changes ride the same commit
with the full count-site checklist (git show 9c2d05c). Docs land in
the same commit as the seams they describe.

- **L-E1 — descriptor + renderer (cargo, M).** New
  `song/src/elements/` module: serde descriptor (v0), name/path
  validation, `{run}` substitution, the render pipeline (verbatim
  copy + `emit::file::render` for templated files), and
  `run_elements_dir()` in `storage/src/fs.rs` beside
  `run_qml_dir()`. Plumbing verb `lyra element seed <song>` (golden
  +1) doing a full render into `run/elements/` from the songbook —
  the shell-reachable bridge the facet will call. CONTRACTS §5
  "Elements" subsection in the same commit. Tests: descriptor
  parse/refuse vectors, byte-identity for verbatim files, template
  render, unknown-placeholder structured error, traversal refusal,
  `_`-shelving skip.
- **L-E2 — stage integration + restart contract (cargo, M).**
  `rice stage` renders the song's elements after the widget sync;
  restart/reload execution: unit → `systemctl --user restart`,
  exec-once → exact-name kill + detached respawn, `reload` preferred,
  `restart` override honored, all best-effort with per-element
  status in the Outcome (the hyprctl discipline). Files:
  `song/src/commands/rice.rs`, `song/src/elements/`. Tests:
  changed-detection (unchanged element is not poked), restart
  derivation table (unit/exec-once/reload/override), render failure
  leaves the old config and reports.
- **L-E3 — the elements facet (nix, L).** `modules/facets/elements/`
  (default.nix + README + AGENTS): readDir the declared song's
  `elements/`, parse descriptors at eval, populate
  `aoide.arrangement.elements` (new option under the existing
  arrangement namespace, declared in nucleus options.nix) and the
  `aoide.surfaces` claims; generate `aoide-element-<name>.service`
  units and compositor exec-once lines pointing at
  `run/elements/`; activation hook runs `lyra element seed` against
  the declared song (store-referenced files, decision 4). Gate: a
  host with no `elements/` dirs evals byte-identical to today
  (drv-identity check — pass the worktree as a path literal); a
  fixture song with a waybar element produces the unit, the claims,
  and the seeded tree.
- **L-E4 — surface stand-down (nix, S).** Quickshell facet owners →
  `mkDefault`; bar render gated on ownership;
  `checks.surface-ownership` untouched and passing. Gate: fixture
  with waybar claiming `bar` — Stylix disables the mapped targets,
  quickshell keeps its other eight surfaces; fixture without
  elements — drv-identical to today.
- **L-E5 — `lyra rice capture` (cargo, M).** The freeze verb (golden
  +1): copy-with-symlink-resolve, scaffold, `--unit`/`--exec`/
  `--surface` flags, auto-create of the song and element dirs,
  idempotent re-capture (same files → same bytes, manifest merged
  not duplicated). Files: `song/src/commands/capture.rs` +
  registration in lyra's registry. Tests: byte-for-byte through a
  symlink, scaffold shape vectors, name validation, re-capture
  idempotence.
- **L-E6 — the waybar venue proof (nix + live, M).** Capture
  dxflake's waybar into a song for real: `rice capture` against the
  rendered `~/.config` ground truth, agent-edited descriptor
  (`{run}` paths, `reload` = SIGUSR2), declared through the facet on
  the User's rig. The rebuild is user-run (house rule 2). Live
  gates: `rice stage` restyles the running waybar without a rebuild;
  a switch re-seeds `run/elements/` and the unit points nowhere near
  `~/.config`; Stylix's bar-adjacent targets are down. Learnings
  append to `song/songbook/learnings.md`.
- **L-E7 — drop-in guidance (docs + S cargo touch).** Foreign-flake
  wiring guidance into `aoide guide` (lyra's guide.rs) and the wiki
  (`concepts/song/` page for elements; the KIMI librarian owns wiki
  prose — this phase supplies the facts): flake-input import, host
  `aoide.*` settings, standalone home-manager on non-NixOS, where
  capture puts things when there is no repo checkout (the L-C3
  share/-templates path). No onboard verb.

Live gates at the end of the lane: the L-E6 trio on the User's rig —
a staged restyle of a running non-QML bar with no rebuild, a rebuild
re-seeding the same tree, and the ownership stand-down visible in
the built system.

## Risks

- **Exec-once kill-by-name can hit an unrelated process.** The
  derived restart pkills the exec's basename with exact match (`-x`);
  a user running a second, unmanaged instance of the same program
  loses it. Documented on the descriptor; `restart` override is the
  escape.
- **Restart-on-stage is disruptive by design.** The bar blinks on
  every staged save. That IS the contract (decision 2); the `reload`
  field is the per-element softening, added one descriptor line at a
  time.
- **Template collisions with literal `{{` in configs.** A CSS or
  JSONC file containing literal `{{` only matters once its file is
  flipped to `template: true`; the render then errors loudly
  (unknown/malformed placeholder is a structured error, never a
  silent no-op). Verbatim files are immune. The mint pass owns
  escaping when it is built.
- **mkDefault flip regressing existing hosts.** L-E4's drv-identity
  gate on a no-elements fixture is exactly for this; run it before
  believing any red result (the path-literal trap is on record).
- **Eval-time descriptor parsing in nix duplicates the parser.** The
  facet needs `files`/`surfaces`/`run` at eval (units, claims) while
  cargo owns the full semantics. Held narrow: nix reads only the
  three fields it generates from, treats the descriptor as data, and
  every content decision (render, validate, refuse) stays in the one
  cargo parser that `lyra element seed` runs at activation — a bad
  descriptor fails the seed with the cargo error, not a nix guess.
- **Unit restart double-fire on switch.** Home-manager's own
  restart-on-change and the seed's freshly written configs can both
  poke a unit in one activation. Harmless (idempotent restart), but
  the executor should confirm ordering (seed runs after
  writeBoundary, before service reloads) rather than assume.

## Kill-list

- No mint/tokenize implementation — the per-file `template` flag is
  the complete, designed seam; the pass itself is a later lane.
- GTK/Qt stay Stylix. No element ever wraps a GTK theme.
- No onboard verb; no scaffolding ceremony beyond what compose and
  capture auto-create (User ruling, decision 6).
- No per-element-class hot-reload engineering, no inotify/watch
  daemon for elements, no niri live seam — one descriptor line
  (`reload`) is the entire tiering mechanism.
- No fourth facet-readable namespace, no new `aoide.*` whitelist
  entry — arrangement and surfaces carry everything (house rule 5).
- No `~/.config` management, ever — not even a symlink. The config
  dir contract is `run/elements/` or nothing.
- Core (`aoide`/`aoided`) stays nix-free and element-free: every
  crate change in this lane lands in the song/storage/lyra side of
  the two-binary split.
