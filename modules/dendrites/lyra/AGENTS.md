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
  path is applied twice. `default.nix` is the catalogue's renderer and imports
  neither half: a host that selects `lyra` alone gets the rice seams and no
  shell, and the shell comes only with a `vibeos` provider.
- **The home half lives in `aoide.lyra.home`.** It is an internal
  `deferredModule` that `runtime.nix` sets, closing over the system config
  (`config.aoide.root`, the built-in set, the derivations in its `let`), and
  `runtime-home.nix` imports through `osConfig`. Home Manager's `pkgs` is the
  system's (habit sets `useGlobalPkgs`), so the lane's overlays reach it.
- **`runtime.nix` sets `aoide.lyra.enable` and `aoide.quickshell.config`,** and
  brings the shell by importing `../quickshell/system.nix`; `runtime-home.nix`
  imports `../quickshell/home.nix` beside `aoide.lyra.home`. `mkFact` accepts
  both facts because each is written from its owner's directory.

- **The renderer writes `hyprland.conf` by one line and reads no runtime
  fact.** Its `extraConfig` is one `source` line at `mkAfter`, after the
  look (500) and the behaviour (1000). It reads `rice.hyprland.*` (its own
  options), Home Manager's `wayland.windowManager.hyprland` state (`enable`,
  `finalPackage`, `configType`, `importantPrefixes`, `sourceFirst`), and
  `osConfig.aoide.root`. It never sets `aoide.lyra.enable`: that fact means
  "the vibeos runtime is present", and only `runtime.nix` can say so.
- **Render order is fixed: the song's `hyprland.conf` first, the Nix settings
  last.** A setting in `rice.hyprland.settings` therefore overrides the song's
  own line for the same key, and a host's `rice.hyprland.host` is sourced after
  both. Swapping the order silently changes which value wins.
- **The state link is relinked only when it is absent or names the twin.** A
  staged link is another writer's current song and must survive a rebuild
  (house rule 10), so the activation never overwrites one. Only the stage
  writers (`rice stage` and its siblings, in `commands/rice.rs`) retarget a
  link that is not the twin.
- **The import of the runtime pair is unconditional.** A provider imports
  `./runtime.nix` and `./runtime-home.nix` on every host that selects it; no
  `mkIf` gates the import, because the module system dedups by path and a
  gated import would let two providers disagree on whether the runtime exists.
- **A component provider writes only its own slot.** It sets
  `aoide.components.<its slot>` and the surfaces that component owns, never
  another slot's entry or a surface the runtime keeps (`osd`, `agentWidgets`,
  `sessionGraph`). What the runtime always needs stays in `runtime.nix`: the
  declared lock dress is staging data the seed writes atomically with the
  rest, so it stays there while the lockers and PAM live in `lock.nix`.
- **The greeter does not import the runtime.** It runs in greetd on its own
  tree; importing the pair would put a session shell on a greeter-only host.

## Extension points

- A new component is one provider file here plus one catalogue slot line
  (`<slot> = { providers.vibeos = ./dendrites/lyra/<slot>.nix; };`), and the
  file imports the runtime pair BY PATH, as above.
- A new lane that needs the runtime imports the pair by path, as above.
- A new rice seam is one more `rice.hyprland.*` option in `default.nix`, and
  it belongs in the rendered file's place in the order above.
- A new unit or activation step of the runtime's home half goes in
  `aoide.lyra.home` in `runtime.nix`, not in a new `habit.home`.

## Docs in the same commit

A change to what the runtime pair imports, sets or publishes, or to which
slot a provider here fills and what it owns, updates this file, `README.md`,
and CONTRACTS.md §0/§2/§4 where they name it. A change to the renderer's render
order, its state link, or its declared twin updates CONTRACTS.md §4 (the three
rice files) and the storage and song READMEs that write them.
