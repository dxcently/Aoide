# AGENTS.md — modules/dendrites

Points up to `modules/AGENTS.md` for cross-module invariants (the registry,
aggregate discipline, `_`-prefix shelving, flags-default-off) — this file
covers only what's specific to dendrites.

## Invariants

- **One file, one plain module.** A dendrite file is an ordinary NixOS module
  (CONTRACTS.md §2). It declares `aoide.<name>.*`, sets the flag
  `lib.mkDefault true` and guards the rest of its config with
  `aoide.<name>.enable`, all in the same file. What it sets under
  `habit.home` is its home half; everything else is its system half.
  The constructor splits the two when it wraps the module.
- **A paint dendrite guards on a FACT, not on an option of its own.** The paint
  lanes (`compositor`, `greeter`, `stylix`, `quickshell`, `lyra`, `wallpaper`)
  read the fact `modules/nucleus/options.nix` declares; the lane sets it
  `mkDefault true` — or, for a capability whose alternatives are named rather
  than enabled (`wallpaper`), `mkDefault "<its own name>"`. Never declare
  `aoide.<that name>.enable` a second time.
- **A dependency rides the guard that needs it, never an unconditional
  `imports`.** `lib/options.nix` evaluates every dendrite in a bare
  `evalModules` to render `aoideOptions`, so a module that pulls in a
  third-party NixOS module unconditionally forces option trees a bare eval has
  not declared (the `stylix` dendrite is the worked example: its `imports`
  sit behind the presence check). The module declares and guards; what it
  needs to run stays under the guard.
- **A condition does not cover a home half's `imports` or `options`.** habit
  reads those before any condition is forced, and refuses a condition over a
  `habit.home` that carries either. Put a home half's `imports` in a
  `habit.home` of its own beside the guarded one (`neovim.nix`), or write them
  as `config` (`lyra/default.nix`).
- **One enable toggle, default off.** `aoide.<name>.enable = false` is the
  shape every dendrite follows — shipped but inert until a host opts in.
- **Carries its own dependencies; reads no other module.** Not another
  dendrite, not another half's internals — only `config.aoide.<name>.*` the file
  declares itself, plus stock options.
- **Draws through a bridge, never directly.** A dendrite that produces
  something visible (a notification, a status line) hands data to a
  render surface via a CLI command or stage file (root `AGENTS.md`
  corollary: Quickshell paints, it is never the capability) — it does not
  embed its own rendering.
- **The paint test decides QML placement, not convenience.** A file stays in
  `pkgs/lyra-shell/qml/` only if it's song-blind, song-plural, and a
  bridge/mechanism (CONTRACTS.md §0, "The paint test"). A shared visual
  component that fails any leg belongs in a song's `widgets/` instead — a paint
  lane is not a component library.
- **Nothing in a paint lane reads a `song/` RUNTIME path at build time** —
  `checks.song-shape` and `checks.song-runtime-untracked` enforce this
  structurally, not just by convention.
  Committed songbook score (`song/songbook/**`) is not a runtime path.
- **The activation seed goes through `lib/livery.nix`, never the raw committed
  file.** `stageLivery` patches the active song's committed `livery.json` with
  `stagePatch` (the same `aoide.livery.override` rule `resolve` applies for
  stylix/the compositor) before it is jq-stamped with `song` and written to
  `song/stage/livery.json` — `checks.livery-fanout` is the gate. The seed
  RENAMES that file over the stage entry and never writes through it, so a
  drafted song's own file is untouched, and it is declared truth on every
  activation: the stage twin stays that until the session's `lyra reload`
  (the lane's `aoide-rice-reload` unit) puts back the staged or drafted song
  `stage/mode.json` names. Nix never reads the mode, so the seed stays one
  unconditional write. The one run
  writes the same bytes to `song/declared/livery.json` too (CONTRACTS.md §4):
  the declared twin that `rice mode declarative` re-pins from. It also publishes
  `song/declared/venue.json` — `venueDelta`, the slots the same `stagePatch`
  rule recolours and the geometry the host sets, `{}` with neither — which
  staging lays over the runtime copy of the declared song, and
  `checks.livery-fanout` guards it. The `lyra` lane is the ONLY writer of
  both: `rice stage` and the other runtime writers read them and must never
  write them, since only the nix evaluator can compute the override tier.
- **A surface takes its size from its CONTENT; content never sizes itself
  from the SCREEN.** A layer anchors only the edges it genuinely occupies
  and lets `implicitWidth`/`implicitHeight` follow what it draws (the
  herald anchors bottom+right and sizes off its own card stack; the bar
  anchors top+left+right at a fixed height). A full-screen overlay is
  legitimate for a modal (launcher, powermenu), but its CONTENT is then
  naturally sized and centred — never anchored to the layer's own edges,
  and never scaled off `screen.height`. A card keeps its own aspect
  ratio; a panel derives from what it stacks, capped, not from a screen
  fraction. **Every geometry constant is a claim about a screen you have
  not seen** — a portrait panel, a second monitor, a different scale.
  Surface-sized content hides its own breakage: on the screen it was
  written for, the derived number sits near what the content wanted, so
  it reads as correct until the geometry changes and every
  height-derived value inflates while every width-derived one clips.
  Verify a new surface at more than one aspect before landing it.
- **The compositor is LOOK + session plumbing only.** Host-invariant
  behavior (keybinds, input devices, tiling layout, misc window rules)
  belongs in `compositor/hyprland/behaviour.nix` so a re-rice can't disturb it.
- **`hosts/` knows dendrites; dendrites never know hosts.** No
  host-conditional logic inside a dendrite file itself.

## Extension points

- **A new dendrite**: a new plain-module file here, and one line in
  `modules/default.nix`'s catalogue — the only place its file is named. Nothing
  else changes. Author it as `_name.nix` while work-in-progress — see
  `_example.nix`, the checked-in template.
- **A capability with alternatives**: a directory here whose `default.nix` is
  `{ providers.<p> = <path>; }`; the catalogue entry is the DIRECTORY, and each
  provider file is a plain module of its own.
- **Splitting a tool out of a bundled dendrite** (e.g. `devtools.nix`) is
  warranted the moment a host needs to toggle it independently — see
  `claude-code.nix`'s header for the precedent.

## Docs update required in the same commit

- This `README.md` when the set of dendrites shifts materially (a new
  category of capability, not every single addition).
- A paint lane's own `README.md` when its scope, surfaces owned, or read set
  changes.
- `CONTRACTS.md` §2 when the dendrite shape itself moves; `CONTRACTS.md §0`
  when the read whitelist itself changes.
- `modules/AGENTS.md` is the layer above for the aggregate/shelving
  mechanics themselves — not restated here.
