# tests/quickshell-seam/fixtures.nix — the seam's eval hosts, and the readings
# run.sh checks them against.
#
# Everything under test is read out of `${flake}`: the constructor, the
# catalogue, nucleus and the three host records all come from the ref's own
# source, so a flakeref really freezes the tree. (A fixture imported from the
# working directory would keep testing the tree you are standing in, whatever
# ref was passed — a frozen ref has to name the commit whose files get read, or
# it proves nothing about that commit.) The ref's source is the only input;
# nothing here stubs the seam itself.
{
  flake,
  system ? "x86_64-linux",
}:
let
  lib = flake.inputs.nixpkgs.lib;
  inputs = flake.inputs;

  # The ref's source root. A commit ref cannot see uncommitted work; `path:`
  # (the runner's default) sees exactly what is on disk.
  src = flake.outPath;

  composition = inputs.habit.lib.composition { inherit lib; };

  # The nucleus lane as the ref itself builds it (`lib/aoideos.nix`) — the same
  # value the flake exports as `nixosModules.nucleus`, and the ONE module that
  # defines `_module.args.aoideInputs`. Read out of the ref rather than
  # reconstructed here, so the fixtures cannot drift from what a host really
  # gets; `specialArgs` therefore carries no `inputs` at all.
  aoideos = import (src + "/lib/aoideos.nix") {
    inherit inputs lib system;
    username = "khoa";
  };

  host =
    name:
    (composition.mkNixosHost {
      nixpkgs = inputs.nixpkgs;
      hostName = name;
      registry = import (src + "/modules");
      hostModules = [ (src + "/tests/quickshell-seam/hosts/${name}.nix") ];
      nucleus = aoideos.nucleusModule;
      homeManagerModule = inputs.home-manager.nixosModules.home-manager;
      specialArgs = {
        username = "khoa";
      };
      # `mkNixosHost` hands `system` to the modules but not to `nixosSystem`
      # itself, so a constructor-built host at this point in the tree states its
      # platform in its own module — the S7 host record carries it, and this
      # harness supplies it here rather than inventing one in the fixtures.
      extraModules = [ { nixpkgs.hostPlatform = system; } ];
      knownHosts = [ name ];
      inherit system;
    }).system;

  # What the seam is, in readings: which directory the shell runs, whether ONE
  # `aoide-quickshell` unit exists and what it says, and which of the things a
  # lyra host would have brought are absent. Named explicitly rather than
  # counted, so a green run says what it checked.
  #
  # `systemServices` is the SYSTEM-level namespace and `systemdUserServices` is
  # the NixOS-level USER one — two different namespaces, and shellbridge is
  # declared in the second (`modules/dendrites/lyra/shellbridge.nix`), so a
  # check that reads only the first would pass on a host that has the bridge.
  #
  # `templates` is the units' OWN `AOIDE_SONG_TEMPLATES` (the staging path's
  # one session-sourced variable — `fs::song_templates_dir`): declared on the
  # unit, not inherited from the systemd user manager's login environment, so a
  # unit restarted after a switch stages from THIS build's songbook. The two
  # module systems spell it differently — NixOS user units put it under
  # `serviceConfig.Environment`, home-manager units under `Service.Environment`
  # — so both are read here rather than assumed.
  unitEnv =
    units: unit:
    let
      u = units.${unit} or { };
    in
    u.serviceConfig.Environment or u.Service.Environment or [ ];

  templatesOf =
    env:
    map (lib.removePrefix "AOIDE_SONG_TEMPLATES=") (
      lib.filter (lib.hasPrefix "AOIDE_SONG_TEMPLATES=") env
    );

  read =
    name:
    let
      cfg = (host name).config;
      home = cfg.home-manager.users.${cfg.aoide.user};
      userUnits = home.systemd.user.services or { };
      systemUnits = cfg.systemd.user.services;
    in
    {
      config = cfg.aoide.quickshell.config;
      song = cfg.aoide.song;
      lyra = cfg.aoide.lyra.enable;
      sessionTarget = cfg.aoide.sessionTarget;
      packages = map (p: baseNameOf p.outPath) cfg.environment.systemPackages;
      userServices = builtins.attrNames userUnits;
      userTimers = builtins.attrNames (home.systemd.user.timers or { });
      systemServices = builtins.attrNames cfg.systemd.services;
      systemdUserServices = builtins.attrNames cfg.systemd.user.services;
      activation = builtins.attrNames (home.home.activation or { });
      templates =
        templatesOf (unitEnv userUnits "aoide-quickshell")
        ++ templatesOf (unitEnv systemUnits "aoided")
        ++ templatesOf (unitEnv systemUnits "shellbridge");
      service =
        if userUnits ? aoide-quickshell then
          {
            condition = userUnits.aoide-quickshell.Unit.ConditionPathExists;
            execStart = userUnits.aoide-quickshell.Service.ExecStart;
          }
        else
          null;
      # What is actually IN the directory the shell runs — the entry point and
      # nothing else is the claim, so a `songs/` or `widgets/` tree landing
      # there (what lyra deploys) would flip this. Null when no directory was
      # named: the bare shape has nothing to read.
      configEntries =
        if cfg.aoide.quickshell.config != null then
          builtins.attrNames (builtins.readDir cfg.aoide.quickshell.config)
        else
          null;
    };
in
{
  only = read "test-quickshell-only";
  bare = read "test-quickshell-bare";

  # The third eval — the one that must FAIL. Forcing the toplevel is what a
  # rebuild does, and `system.build.toplevel` is where nixpkgs runs the
  # platform's assertions (`lib/asserts.nix`'s `checkAssertWarn`), so this
  # attribute is the failure a user would meet, not a probe for one.
  songWithoutLyra = (host "test-song-without-lyra").config.system.build.toplevel.drvPath;

  # The positive control for the absence checks: yomi runs lyra, so it HAS the
  # shellbridge unit, the rice binary and the templates declaration the two
  # lyra-less fixtures must not have. Read off the ref's own
  # `nixosConfigurations`, so it is the real host and not a fourth fixture.
  yomi =
    let
      cfg = flake.nixosConfigurations.yomi-strix.config;
      home = cfg.home-manager.users.${cfg.aoide.user};
      # Every unit that stages, or whose spawned children can: the daemon that
      # spawns sessions, the shell whose QML execs `lyra`, and the bridge that
      # stages in-process. One entry per unit, so a check can name the one that
      # went missing.
      templates = {
        aoided = templatesOf (unitEnv cfg.systemd.user.services "aoided");
        shellbridge = templatesOf (unitEnv cfg.systemd.user.services "shellbridge");
        quickshell = templatesOf (unitEnv (home.systemd.user.services or { }) "aoide-quickshell");
      };
    in
    {
      systemdUserServices = builtins.attrNames cfg.systemd.user.services;
      packages = map (p: baseNameOf p.outPath) cfg.environment.systemPackages;
      inherit templates;
      # All three must agree on ONE directory: a unit left with a different
      # answer is the drift this reading exists to catch.
      templatesAgree = builtins.length (lib.unique (lib.concatLists (lib.attrValues templates))) == 1;
    };
}
