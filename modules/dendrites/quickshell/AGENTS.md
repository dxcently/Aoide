# AGENTS.md — modules/dendrites/quickshell

Points up to `modules/dendrites/AGENTS.md`; this file covers only the shell
lane.

## Invariants

- **`system.nix` and `home.nix` carry no `habit` key and are imported only BY
  PATH** — by `default.nix` and by any lane that brings the shell
  (`lyra/runtime.nix`, `lyra/runtime-home.nix`). Never inline or let-bind
  either, and never import `default.nix` from another lane: a catalogue entry
  is applied under its catalogue key, so it would be applied twice.
- **The home import is `habit.home.imports = [ ./home.nix ]`,** never
  `habit.home = ./home.nix`, which habit turns into a module with no key.
- **The home half is `aoide.quickshell.home`,** an internal `deferredModule`
  set in `system.nix` and imported by `home.nix` through `osConfig`. A change
  to the unit is a change to that value.
- **No song and no lyra.** The service is gated on `aoide.quickshell.config`,
  never on `aoide.song`; the one lyra read is the `aoide.lyra.enable` fact
  that puts `AOIDE_SONG_TEMPLATES` on the unit.

## Docs in the same commit

A change to the halves' shape, the facts this lane sets, or the unit updates
this file, `README.md`, `../README.md` and CONTRACTS.md §0/§2.
