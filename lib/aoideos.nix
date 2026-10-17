# lib/aoideos.nix — AoideOS's host constructor.
#
# Two things, and they answer to each other: `hostNames` is the machines this
# flake holds, and `mkHost name` assembles the per-host outputs for one of them.
# It is the only place that knows both halves of the flake's shape at once —
# where the module tree is, where the hosts are, and which input supplies
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
# (habit's composition, `inputs.habit`): the host module's `habit.*` keys are read
# in an ordinary `evalModules` pass that knows nothing about NixOS, and the
# platform import list is assembled from the result. Nothing below imports a
# dendrite file, and a capability the host did not select is never read.
#
# The songs are one hook. `lib/songbook.nix`'s `hook` answers habit's three
# song hooks: each committed `rice.nix` is an aggregation body (the registry
# half), `habit.song.*` is the selection (the selection-module half), and the
# checks plus the facts the paint lanes read are the platform half. Habit names
# no song; this is the only site that wires the hook; `lib/songbook.nix` is the
# only site that says what it means.
{
  inputs,
  lib,
  system ? "x86_64-linux",
  username ? "khoa",
}:
let
  composition = inputs.habit.lib.composition { inherit lib; };

  # The committed songbook this flake's hosts' songs live in. Named ONCE here,
  # as the caller's answer to `lib/songbook.nix`'s own default (`songbook ?
  # ../song/songbook` — the same directory): the selection validates a host
  # against it, the hook below hands the same value to the lane that paints the
  # built-in songs, and the flake exports this same value as `songbookRoot`, so
  # a consumer performing Aoide's songs hands it to that same argument, where a
  # consumer with its own songbook hands its own directory. `lib/songbook.nix` itself is deliberately NOT touched for this:
  # it is COPIED into `pkgs/lyra-songbook` (`share/lyra/nix/songbook.nix`, the
  # shipped generator), so a single added line there moves every host's
  # templates path — and every session variable and unit `Environment` that
  # interpolates it.
  songbookRoot = ../song/songbook;
  songbook = import ./songbook.nix {
    inherit lib;
    songbook = songbookRoot;
  };

  # The song hook, over the registry this flake's hosts share: each committed
  # song is an aggregation, the declared one is selected, and its checks run
  # in the constructor's gate pass. `users` is the one person this flake's
  # hosts name (`username`); a host that performs a song attaches that user's
  # home (`habit.users.<u>.home.enable`), and nothing else about songs is named here.
  hook = songbook.hook {
    registry = import ../modules;
    users = [ username ];
  };

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
  # the selected modules come after it and one may replace a name it provides.
  overlay = (import ./pkgs.nix { inherit lib; }).overlay {
    stock = inputs.nixpkgs.legacyPackages.${system};
  };

  # The nucleus module (the option contract), as a CONSUMER takes it — and the
  # ONE place Aoide's own
  # flake inputs reach a module evaluation.
  #
  # What a module reads is the module argument `aoideInputs`, and there are
  # exactly two doors for it, because the module system asks for arguments in
  # two different moments:
  #
  #   - INSIDE `config` (every module's reads except the imports below), through
  #     `_module.args`, which is `raw`: exactly ONE module defines it, and this
  #     is that module. Both this file's `mkHost` and the flake's
  #     `nixosModules.nucleus` output are that same value.
  #   - While an `imports` LIST is being resolved, through specialArgs — and
  #     only specialArgs. An `imports` list is what the module list is built
  #     from, and a name provided by `_module.args` is read out of `config`,
  #     which is computed FROM that module list. Nixpkgs names the wall itself
  #     when it happens ("argument `x' is not externally provided, so querying
  #     `_module.args` instead, requiring `config`" → infinite recursion). So
  #     the two upstream modules a module used to name in its own `imports` are
  #     imported HERE, by value, where `inputs` is a LEXICAL value of this file
  #     and costs no argument at all: the core's module (was
  #     `modules/nucleus/options.nix`) and stylix's (was
  #     `modules/dendrites/stylix.nix`).
  #   - The home half gets a third door for the same reason: home-manager
  #     evaluates its own module system, and `extraSpecialArgs` IS its
  #     specialArgs (external, available during its own module collection), so
  #     `aoideInputs.nvf` works in an HM `imports` list the way
  #     `aoideInputs.quickshell` works in a NixOS one. `sharedModules` would put
  #     the name back in `config` and hit the wall above.
  #
  # `options ? home-manager` is asked INSIDE this module's `config`, never at
  # module-application time: a guard forced in the top-level attrset asks the
  # same question the wrong way (the error above). A host that never imports
  # home-manager's NixOS module gets no option named that from here.
  nucleusModule =
    { options, ... }:
    let
      imports = [
        ../modules/nucleus
        inputs.aoide.nixosModules.default
      ]
      ++ lib.optional (inputs ? stylix) inputs.stylix.nixosModules.stylix;
    in
    {
      inherit imports;
      config = {
        _module.args.aoideInputs = inputs;
      }
      // lib.optionalAttrs (options ? home-manager) {
        home-manager.extraSpecialArgs.aoideInputs = inputs;
      };
    };
in
{
  inherit hostNames songbookRoot;

  # The nucleus module as a consumer takes it — the same module value
  # `nixosModules.nucleus` exports (see the let-block above).
  inherit nucleusModule;

  mkHost =
    name:
    composition.mkNixosHost {
      inherit (inputs) nixpkgs;
      inherit system;

      hostName = name;
      # The hosts this flake holds, for the one thing the constructor
      # needs the list for: an override record confined to unknown host names is
      # a typo, and it fails by name instead of applying nowhere.
      knownHosts = hostNames;

      inherit (hook) registry selectionModules extraModulesFor;
      host = ../hosts/${name};
      homeManagerModule = inputs.home-manager.nixosModules.home-manager;
      overlays = [ overlay ];

      # The nucleus module: the option contract every host carries, selected by no
      # one. The services are the `nucleus` aggregation's, which a host selects.
      extraModules = [ nucleusModule ];

      # `system` is threaded to the modules rather than to `nixosSystem`: a host
      # module states its own platform (`nixpkgs.hostPlatform`), because the
      # constructor assembles the module list and lets the platform come from
      # the modules it assembled. `host` and `username` ride
      # along because modules and test fixtures have always had them — and
      # `inputs` does NOT: every module reads Aoide's own inputs as `aoideInputs`
      # (`nucleusModule` above), so threading them through specialArgs would be
      # a second, drifting answer to a question already answered once.
      specialArgs = { inherit username; };
    };
}
