# modules/dendrites/strillix/runtime.nix — the strillix runtime's system half (the
# Quickshell surface strillix paints).
#
# Renders the complete Aoide shell surface using Quickshell (QML runtime).
# The system half, imported BY PATH by the component providers (AGENTS.md). Its
# home half is `runtime-home.nix` beside it, which each provider imports through
# `habit.home`, so the same home body runs under NixOS (home-manager) and under a
# standalone home (habit's mkHome). Nothing here names the home half.
# Surfaces owned: osd, agentWidgets, sessionGraph; each component provider
# beside this file owns its own, and writes its `strillix.components` entry, which
# this file publishes as `components.json` at the top of the deployed tree.
#
# Reading discipline (CONTRACTS.md §1): a paint dendrite reads the dress, the
# structure, the surfaces and identity — nothing else (root AGENTS.md house
# rule 5). Here: `strillix.theme` (palette + component tiers), `strillix.surfaces`
# (the registry it declares its own surfaces in), `strillix.layout`, the
# core scalars (`aoide.root`, `aoide.checkout`, `aoide.user`),
# its identity (`strillix.hostConfig`), the rice selection (`strillix.rice`) and
# the derived fact `strillix.rices.builtIn` (the rices this host builds in — what this lane's
# deployed manifest, widget copy, installed packages, shipped templates and seed
# are built FROM) and its own fact.
#   - Component-tier fallback (null → palette) applied locally, never pushed
#     back into the option system.
#   - NEVER reads rice/ runtime paths at build time (checks.rice-shape and
#     checks.rice-runtime-untracked are the structural half).
#
# Communication discipline (entities/Quickshell):
#   - QML reads state files from $STRILLIX_ROOT/stage/ at runtime (hot-reload).
#   - QML issues commands via the shellbridge unix socket (shellbridge.nix
#     beside this file).
#   - QML never speaks MCP or any agent protocol.
{
  config,
  lib,
  pkgs,
  aoideInputs,
  ricesRoot,
  ...
}:
let
  # Committed rices live here (CONTRACTS.md §5) — versioned score, read at
  # build time (nothing here reads a rice/ RUNTIME dir; the check that used
  # to assert that scanned module PATHS and is gone — what replaced it is
  # `checks.rice-shape`'s `strayNixFiles` and `checks.rice-runtime-untracked`).
  #
  # Handed in as a module argument by the same hook that sets
  # `strillix.rices.builtIn` (lib/strillix.nix, and every consumer's copy of
  # that wiring): a consumer's rices may be its own tree's or Aoide's
  # exported `ricesRoot`, so this lane must not name a path inside
  # Aoide's.

  # ── What this host BUILDS IN ────────────────────────────────────────────
  # `habit.rice.declared ∪ habit.rice.available` in its module, derived by the
  # constructor's hook into the fact `strillix.rices.builtIn`. Everything below
  # the deployed manifest and registry, the widget bodies, the installed
  # packages, the shipped templates and the machine-rices seed all run over
  # these names, never over every committed rice.
  #
  # A rice that is NOT built in is still stageable from the machine's own
  # rices (`strillix rice stage <n>`) — the runtime keeps a wider manifest
  # than this deployed tree carries, which is exactly the seam
  # `builtin.json`'s `packages` exists to check (§7.5).
  ricesLib = import ../../../lib/rices.nix {
    inherit lib ricesRoot;
    discovered = config._module.args.riceDirs;
  };

  # The option doc list the shipped `aoide-options.json` carries
  # (`lib/options.nix`) — `strillix onboard`'s offline source, and the reason
  # the shipped `strillix-rices` needs an argument `callPackage` cannot
  # fill. Derived here rather than handed down: this lane is what builds
  # `pkgs.strillix-rices` for the host, and the value needs `aoideInputs`,
  # which is a module argument. Lazily forced — a host that never reads
  # `pkgs.strillix-rices` never evaluates it.
  aoideOptions = import ../../../lib/options.nix {
    inherit lib aoideInputs;
    pkgs = aoideInputs.nixpkgs.legacyPackages.${pkgs.stdenv.hostPlatform.system};
  };

  builtIn = config.strillix.rices.builtIn;

  # The installed strillix (package.nix): the one binary this dendrite installs and runs.
  aoide = aoideInputs.aoide.packages.${pkgs.stdenv.hostPlatform.system}.default;
  strillixRices = pkgs.strillix-rices;
  strillixPkg = import ./package.nix { inherit pkgs aoide strillixRices; };
  # `~/Aoide` onto `aoide.root` (default `~/.aoide`).
  qmlDir = "${config.aoide.root}/run/qml";
in
{
  # shellbridge is this capability's other half (strillix owns the bridge,
  # docs/architecture/PACKAGE-LAYOUT.md): the file gates on the facts it reads, so
  # importing it beside this lane is the whole wiring. The shell process is the
  # quickshell lane's system half, imported by its path so a host that also
  # selects `quickshell` gets the one module.
  imports = [
    ./shellbridge.nix
    ../quickshell/system.nix
  ];

  # ── Config ─────────────────────────────────────────────────────────────────
  # Importing the runtime is the switch; it sets the read-only fact
  # `strillix.enable` (declared in modules/nucleus/options.nix) to true.
  config = {
    strillix.enable = true;

    # name -> rice directory, the rices's registry. The rice hook sets it
    # (lib/rices.nix's `riceDirs`) so a merged rices's rices come from
    # the input books; a host that wires only `rices` gets the directory's
    # own discovery, at the lower priority the hook's value overrides.
    _module.args.riceDirs =
      lib.mkDefault
        (import ../../../lib/rices.nix { inherit lib ricesRoot; }).riceDirs;

    # ── What this lane brings into existence ────────────────────────────────
    # One thing, and it is a FACT rather than this lane's own option: the
    # directory the shell runs. Quickshell itself is the `quickshell` lane's
    # (package and the `aoide-quickshell` service); no module reads another
    # module (root AGENTS.md house rule 5), so this lane NAMES the config it
    # deploys into and the shell lane reads that name. Strillix paints a rice —
    # deploy the tree, seed the stage, restart the rice — and the shell lane
    # is what knows how to run a shell on the result.
    #
    # The shell lane spells `<dir>/shell.qml` for quickshell's entry point,
    # so `$AOIDE_ROOT/run/qml` here is the same command line as the day the
    # service lived in this file.
    strillix.quickshell.config = qmlDir;

    # ── The templates this host ships ───────────────────────────────────────
    # `pkgs.strillix-rices` is the baseline `STRILLIX_TEMPLATES`: what
    # `strillix rice compose` and the repo-less manifest regeneration fall back
    # to. Its default is the WHOLE committed rices (the flake's own
    # `packages` output); a host overrides it with what it actually built
    # in, so a `_server` ships an empty templates dir and yomi ships
    # sonata's folder and no other. `builtin.json` records the selection
    # itself — `{ declared, rices, packages }` — which is what runtime
    # staging checks a non-built-in rice's needs against before refusing to
    # stage it (the machine owns its rices; the baseline is the fallback).
    #
    # An overlay rather than a reference: the readers of the templates dir
    # resolve `pkgs.strillix-rices` by name (the installed strillix's default in
    # package.nix, shellbridge's `STRILLIX_TEMPLATES`), and only the overlay
    # gives them this host's built-in set.
    nixpkgs.overlays = [
      (_final: prev: {
        # A FRESH `callPackage`, never `.override` on whatever the walker's
        # overlay left in `prev`: application order across the base
        # (lib/strillix.nix) and this lane is not a contract, so
        # `prev.strillix-rices` may not exist yet. A fresh call also names
        # every argument here rather than inheriting them — including
        # `aoideOptions`, which this lane derives itself because the
        # walker's own `extra` (lib/pkgs.nix) is that overlay's business,
        # not this lane's. Both overlays may define the name; the walker's
        # step-aside makes whichever is applied second yield, so this
        # lane's answer wins in either order.
        strillix-rices = prev.callPackage ../../../pkgs/strillix-rices {
          rices = builtIn;
          builtin = {
            declared = config.strillix.rice;
            rices = builtIn;
            packages = ricesLib.packagesFor builtIn;
          };
          inherit aoideOptions ricesRoot;
          inherit (ricesLib) riceDirs;
        };
      })
    ];

    # ── Surface-ownership registry ──────────────────────────────────────────
    # The Quickshell-owned surfaces no component slot answers for; each
    # component provider beside this file declares its own (README.md).
    # Stylix reads this registry and stands down for these surfaces
    # (concepts/Notes).
    strillix.surfaces = {
      osd.owner = "quickshell";
      agentWidgets.owner = "quickshell";
      sessionGraph.owner = "quickshell";
    };

    # ── What the shell needs to speak to hardware ───────────────────────────
    # The Quickshell PACKAGE and its service belong to the `quickshell` lane
    # (it is the lane that knows how to run a shell, with or without strillix);
    # these two live here because it is THIS lane's widget that shells out to
    # them.
    #
    #   pkgs.pavucontrol — the colonnade's `[ mixer ]` tag launches it.
    #   pkgs.pulseaudio  — for `pactl` only; pipewire-pulse remains the
    #                      server. The bluetooth bay writes the A2DP ↔
    #                      headset profile with `pactl set-card-profile`
    #                      (neither Quickshell.Bluetooth nor
    #                      Quickshell.Services.Pipewire exposes a card
    #                      profile — checked against both modules'
    #                      compiled qmltypes).
    #
    # Sonata's `_widgets/bar.nix` declares those same two names (the record
    # is what a rice says it needs), so the hard-coded pair and
    # `packagesFor builtIn` agree today — `lib.unique` is what makes that a
    # checked coincidence rather than an assumption: the union is a set, not
    # a concatenation, and a rice declaring a package the lane already
    # carries adds nothing.
    environment.systemPackages = lib.unique (
      [
        # The `strillix` binary (package.nix: P-A8's separate output, with the
        # shipped rices named by default), installed here because this dendrite is
        # its owner. A host that never selects it has no `strillix` on PATH, and
        # core never installs it.
        strillixPkg
        pkgs.pavucontrol
        pkgs.pulseaudio # for `pactl` only; pipewire-pulse remains the server
      ]
      # …plus what the built-in rices' own widgets declare. `composeRice`
      # reads each `_widgets/` record's `packages` (lib/rice.nix); until this
      # slice nothing installed them. A shelf-less rice declares none —
      # there is no record to declare them in.
      ++ map (name: pkgs.${name}) (ricesLib.packagesFor builtIn)
    );

    # ── Login shells: Strillix's own root and host file ──────────────────────
    # `STRILLIX_ROOT` and `STRILLIX_HOST_CONFIG` reach `aoide.user`'s shells only,
    # for the same reason core's `AOIDE_ROOT` does: a system-wide export would
    # hand khoa's root to root. The shipped rices need no login variable: the
    # `strillix` on PATH names them itself (package.nix).
    environment.extraInit = ''
      if [ "$(id -un)" = ${lib.escapeShellArg config.aoide.user} ]; then
        export STRILLIX_ROOT=${lib.escapeShellArg config.strillix.root}
        export STRILLIX_HOST_CONFIG=${lib.escapeShellArg config.strillix.hostConfig}
      fi
    '';

  };
}
