# lib/aoideos.nix — AoideOS's host constructor.
#
# Two things, and they answer to each other: `hostNames` is the machines this
# flake holds, and `mkHost name` assembles the per-host outputs for one of them.
# It is the only place that knows both halves of the flake's shape at once —
# where the module tree is, where the host records are, and which input supplies
# Home Manager — so `flake.nix` stays a list of the flake's own outputs and
# nothing else.
#
# What it does NOT do: name a host. `hostNames` is DISCOVERED — every immediate
# child directory of `hosts/` that holds a `default.nix`, minus the `_`-prefixed
# shelved ones — and it is the ONE site that reads `hosts/`. `flake.nix` builds
# its `nixosConfigurations`/`inventory` from this list and `mkHost` takes the
# same list as `knownHosts`, so "which machines exist" can never come from two
# answers that disagree.
#
# Selection happens before this file is reached, inside `mkNixosHost`
# (`lib/composition.nix`): the host record is evaluated in an ordinary
# `evalModules` pass that knows nothing about NixOS, and the platform import list
# is assembled from the result. Nothing below imports a dendrite file, and a
# capability the host did not select is never read.
#
# The songs are the two hooks. `selectionModules` puts `song.declared` /
# `song.available` on the host record, so the gate pass can read a selection the
# constructor has never heard of; `extraModulesFor` turns that selection into
# the platform modules — the built-in songs' `rice.nix` files, and the
# `aoide.song` / `aoide.songbook.builtIn` facts the paint lanes read. Both are
# generic hooks (`lib/composition.nix` names no song), and this is the only site
# that wires them; `lib/songbook.nix` is the only site that says what they mean.
{
  inputs,
  lib,
  system ? "x86_64-linux",
  username ? "khoa",
}:
let
  composition = import ./composition.nix { inherit lib; };

  songbook = import ./songbook.nix { inherit lib; };

  # Every immediate child directory of `hosts/` holding a `default.nix`, minus
  # the `_`-prefixed shelved ones.
  hostNames = builtins.attrNames (
    lib.filterAttrs (
      name: type:
      type == "directory"
      && !lib.hasPrefix "_" name
      && builtins.pathExists (../hosts + "/${name}/default.nix")
    ) (builtins.readDir ../hosts)
  );

  # The flake's own packages (`pkgs/aoide`'s outputs and the walker's
  # `pkgs/<name>` dirs) reach a host's package set through this overlay — the
  # same source `flake.nix`'s `packages` output and the `pkg-<name>` checks
  # read, so there is one build and no second list. It is a BASE, handed to the
  # constructor as an `overlays` entry rather than as an extra module, because
  # the lanes come after it and a lane may replace a name it provides.
  overlay = (import ./pkgs.nix { inherit lib; }).overlay {
    stock = inputs.nixpkgs.legacyPackages.${system};
  };
in
{
  inherit hostNames;

  mkHost =
    name:
    composition.mkNixosHost {
      inherit (inputs) nixpkgs;
      inherit system;

      hostName = name;
      # The host records this flake holds, for the one thing the constructor
      # needs the list for: an override record confined to unknown host names is
      # a typo, and it fails by name instead of applying nowhere.
      knownHosts = hostNames;

      registry = import ../modules;
      nucleus = ../modules/nucleus;
      hostModules = [ ../hosts/${name} ];
      homeManagerModule = inputs.home-manager.nixosModules.home-manager;
      overlays = [ overlay ];

      # The song fields a host record may set, and nothing else about songs.
      selectionModules = [ songbook.selectionModule ];

      # What that selection MEANS: first the checks (a song name the songbook
      # does not hold, a song folder with no `rice.nix`, a song with no
      # performer — all refused here, before the platform list is assembled),
      # then the built-in songs' `rice.nix` files (declared ∪ available, and
      # nothing else), then the facts they are read through. A host that names no
      # song contributes no song module and sets `aoide.song` null, which every
      # `rice.nix` self-gate reads as "not me".
      extraModulesFor =
        sel:
        let
          song = songbook.check {
            inherit (sel) song;
            lyra = sel.dendrites.lyra.enable;
          };
        in
        songbook.songModules song
        ++ [
          {
            aoide.song = song.declared;
            aoide.songbook.builtIn = songbook.builtIn song;
          }
        ];

      # `system` is threaded to the modules rather than to `nixosSystem`: a host
      # record states its own platform (`nixpkgs.hostPlatform` in its `nixos`
      # half), because the constructor assembles the module list and lets the
      # platform come from the modules it assembled. `host` and `username` ride
      # along because modules and test fixtures have always had them.
      specialArgs = { inherit inputs username; };
    };
}
