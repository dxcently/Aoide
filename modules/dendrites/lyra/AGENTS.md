# AGENTS.md — modules/dendrites/lyra

Points up to `modules/dendrites/AGENTS.md` for what every dendrite holds; this
file covers only the lyra lane.

## Invariants

- **`runtime.nix` and `runtime-home.nix` are the ONE runtime pair.** Every
  lane that brings the vibeos runtime imports these two paths and nothing
  else of it: `imports = [ ./runtime.nix ]` (or `../lyra/runtime.nix`) and
  `habit.home.imports = [ ./runtime-home.nix ]`. The module system lands a
  path once however many lanes import it; an inline copy or a let-bound
  attrset of either is a second module that declares `aoide.lyra.home` twice.
- **The home import is a list, never `habit.home = ./runtime-home.nix`.** habit
  unifies a bare path into a module with no key, so two lanes that wrote it
  that way would apply the home half twice.
- **Neither half carries a `habit` key, and neither is a catalogue entry.**
  habit reads `habit.home` only on a selected module's top level, so a
  `habit.home` inside `runtime.nix` fails; and a catalogue entry is applied
  under its catalogue key, not its path, so a catalogue file imported again by
  path is applied twice. `default.nix` is the catalogue's wrapper and holds
  nothing but the two imports.
- **The home half lives in `aoide.lyra.home`.** It is an internal
  `deferredModule` that `runtime.nix` sets, closing over the system config
  (`config.aoide.root`, the built-in set, the derivations in its `let`), and
  `runtime-home.nix` imports through `osConfig`. Home Manager's `pkgs` is the
  system's (habit sets `useGlobalPkgs`), so the lane's overlays reach it.
- **`runtime.nix` sets `aoide.lyra.enable` and `aoide.quickshell.config`,** and
  brings the shell by importing `../quickshell/system.nix`; `runtime-home.nix`
  imports `../quickshell/home.nix` beside `aoide.lyra.home`. `mkFact` accepts
  both facts because each is written from its owner's directory.

## Extension points

- A new lane that needs the runtime imports the pair by path, as above.
- A new unit or activation step of the runtime's home half goes in
  `aoide.lyra.home` in `runtime.nix`, not in a new `habit.home`.

## Docs in the same commit

A change to what the runtime pair imports, sets or publishes updates this
file, `README.md`, and CONTRACTS.md §0/§2 where they name it.
