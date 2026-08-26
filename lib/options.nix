# lib/options.nix — the aoide.* option-derivation output (P-I3, onboarding
# lane, docs/architecture/ONBOARD.md "The vars-file generator"). Feeds `lyra
# onboard`'s `aoide.nix` generator: `nix eval --json <checkout>#aoideOptions`
# returns every VISIBLE, non-internal `aoide.*` option declared across
# modules/{nucleus,facets,dendrites}, narrowed to exactly what the generator
# needs to render one commented line — name, description, and the default
# already rendered as nix SOURCE TEXT (nixpkgs' own doc renderer does the
# quoting/escaping; the generator pastes it verbatim, never re-serializes a
# value itself). DERIVED from the option declarations via `lib.evalModules`
# + `lib.optionAttrSetToDocList` — no hand-list.
#
# Ladder rung (a) HELD (docs/architecture/ONBOARD.md "The vars-file
# generator" / P-I3 brief): a bare `lib.evalModules` over the walked module
# tree evaluates cleanly with no fight from `pkgs`/`config`-referencing
# option constructions (`aoide.lyra.enable`'s `config.aoide.facets.
# quickshell.enable` default, `aoide.auditLog`'s `"/home/${config.aoide.
# user}/…"` default, three `melete`/`mneme` dendrite path defaults — nine
# entries total, verified by grepping the raw doc-list output for `config\.`/
# `pkgs\.`/`/nix/store` before committing to this rung). The heavier
# `nixosSystem` fallback (rung (b)) was never needed. `pkgs` is the one thing
# a real `nixosSystem` supplies as a module arg automatically that a bare
# `evalModules` does not — supplied here via `specialArgs`, the flake's own
# `nixpkgs.legacyPackages.<system>` (same source `checks`/`packages` already
# use), per the brief's "try the standard `pkgs = import nixpkgs {…}` first"
# guidance.
#
# `_module.check = false`: modules under `modules/` set plenty of config
# OUTSIDE the `aoide.*` tree (`home-manager.users.*`, `systemd.*`, …) that a
# real host declares via the NixOS/home-manager module sets this bare eval
# never loads. Those thunks are never forced (this file only reads
# `.options.aoide`, never `.config`), but disabling the check is the
# defensive, documented reason nothing here depends on loading those module
# sets just to read option DECLARATIONS.
{
  lib,
  pkgs,
  inputs,
}:
let
  walk = import ./walk.nix { inherit lib; };
  discovered = walk ../modules;

  evaled = lib.evalModules {
    modules = discovered ++ [ { config._module.check = false; } ];
    specialArgs = {
      inherit inputs pkgs;
      username = "khoa";
      host = "aoide-options-eval";
      system = pkgs.system;
    };
  };

  docList = lib.optionAttrSetToDocList evaled.options;

  aoideOptions = builtins.filter (
    o: o.visible == true && o.internal == false && lib.hasPrefix "aoide." o.name
  ) docList;
in
map (o: {
  inherit (o) name description;
  default = o.default.text or null;
}) aoideOptions
