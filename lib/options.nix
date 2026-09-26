# lib/options.nix — the aoide.* option-derivation output (P-I3, onboarding
# lane, docs/architecture/ONBOARD.md "The vars-file generator"). Feeds `lyra
# onboard`'s `aoide.nix` generator: `nix eval --json <checkout>#aoideOptions`
# returns every VISIBLE, non-internal `aoide.*` option declared across
# modules/nucleus and the dendrite bodies the registry catalogues,
# narrowed to exactly what the generator needs to render one commented line —
# name, description, and the default already rendered as nix SOURCE TEXT
# (nixpkgs' own doc renderer does the quoting/escaping; the generator pastes
# it verbatim, never re-serializes a value itself). DERIVED from the option
# declarations via `lib.evalModules`
# + `lib.optionAttrSetToDocList` — no hand-list. The emitted JSON deliberately
# carries only {name, description, default} — no `type` field, since nothing
# downstream consumes one; a future consumer adds it then, not speculatively.
#
# Ladder rung (a) HELD (docs/architecture/ONBOARD.md "The vars-file
# generator" / P-I3 brief): a bare `lib.evalModules` over the module tree
# evaluates cleanly with no fight from `pkgs`/`config`-referencing
# option constructions (ten `default` fields reference `config` — the core
# half's `aoide.root`/`checkout`/`auditLog`, with `"/home/${config.aoide.
# user}/…"` and `${config.aoide.root}` paths, plus the `melete`/`mneme`
# dendrite path defaults — and none references `pkgs` or a store path;
# verified by grepping the raw doc-list output for `config\.`/`pkgs\.`/
# `/nix/store` before committing to this rung). The heavier
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
  registry = import ../modules;

  # A dendrite file is a lane record (CONTRACTS.md §2): the option
  # declarations and the guard live on `body`, and the lane is the module the
  # constructor imports for a host that selected it. Only `body` is read here —
  # a lane that nothing selected must not be evaluated to render a doc line.
  #
  # A capability with alternatives is a provider registry instead
  # (`{ providers.<p> = <path>; }`, e.g. `compositor`), and each provider is a
  # lane record of its own; the doc list needs every alternative's options, so
  # every provider body is read. Same rule as
  # `modules/dendrites/default.nix`'s whole-tree derivation.
  bodyOf =
    path:
    let
      entry = import path;
    in
    if entry ? body then [ entry.body ] else map (p: (import p).body) (lib.attrValues entry.providers);

  bodies = lib.concatMap bodyOf (lib.attrValues registry.catalogue);

  evaled = lib.evalModules {
    modules = bodies ++ [
      ../modules/nucleus
      { config._module.check = false; }
    ];
    specialArgs = {
      inherit inputs pkgs;
      inherit (pkgs) system;
      username = "khoa";
      host = "aoide-options-eval";
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
