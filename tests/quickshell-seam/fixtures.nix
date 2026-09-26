# tests/quickshell-seam/fixtures.nix — the seam's eval hosts, and the readings
# run.sh checks them against.
#
# Each host is assembled the way a real one is: the real catalogue, the real
# nucleus, the real constructor (`lib/composition.nix`), a Home Manager module.
# The only fixture-shaped parts are the three host records under `./hosts`, the
# fixture person, and the fixture config directory — nothing here stubs the
# seam itself, which is the whole point: a stubbed seam proves nothing.
#
# `flake` is the checkout under test (`builtins.getFlake`), so the tree is read
# once and every input comes from its own lock.
{ flake, system ? "x86_64-linux" }:
let
  lib = flake.inputs.nixpkgs.lib;
  inputs = flake.inputs;

  composition = import ../../lib/composition.nix { inherit lib; };

  host =
    name:
    (composition.mkNixosHost {
      nixpkgs = inputs.nixpkgs;
      hostName = name;
      registry = import ../../modules;
      hostModules = [ ./hosts/${name}.nix ];
      nucleus = ../../modules/nucleus;
      homeManagerModule = inputs.home-manager.nixosModules.home-manager;
      specialArgs = { inherit inputs; username = "khoa"; };
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
  read =
    name:
    let
      cfg = (host name).config;
      home = cfg.home-manager.users.${cfg.aoide.user};
      userUnits = home.systemd.user.services or { };
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
      activation = builtins.attrNames (home.home.activation or { });
      service =
        if userUnits ? aoide-quickshell then
          {
            condition = userUnits.aoide-quickshell.Unit.ConditionPathExists;
            execStart = userUnits.aoide-quickshell.Service.ExecStart;
          }
        else
          null;
      songsDir =
        cfg.aoide.quickshell.config != null
        && builtins.pathExists (cfg.aoide.quickshell.config + "/songs");
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
}
