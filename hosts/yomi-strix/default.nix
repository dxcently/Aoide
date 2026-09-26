# hosts/yomi-strix/default.nix — the reference host.
#
# A host record: what this machine SELECTS (aggregations, lone capabilities,
# people), then this machine's own platform settings in `nixos`, deferred until
# selection is complete. It names no module file — nothing outside
# `modules/default.nix` does.
#
# Nothing here is registered anywhere: `flake.nix` discovers `hosts/` one level
# deep, so adding a machine is a new directory and never an edit to a list.
{
  # ── Aggregations ───────────────────────────────────────────────────────────
  # base is the floor; desktop is the session's platform; agents is the coding
  # agents a workstation runs; aoideos is the desktop itself, songs included.
  aggregation.base.enable = true;
  aggregation.desktop.enable = true;
  aggregation.agents.enable = true;
  aggregation.aoideos.enable = true;

  # ── Lone capabilities ──────────────────────────────────────────────────────
  # What no aggregation speaks for. `qbittorrent` is here because no group
  # claims it; `obsidian` in the same way; `inference` is a server a host either
  # hosts or does not.
  dendrites.obsidian.enable = true;
  dendrites.qbittorrent.enable = true;
  dendrites.inference.enable = true;

  # ── Songs ──────────────────────────────────────────────────────────────────
  # `declared` is the song this host PERFORMS — it becomes the fact
  # `aoide.song`, which every `rice.nix` self-gates on. `available` is what it
  # builds in to STAGE without a rebuild but does not perform.
  #
  # yomi builds in sonata and nothing else: the shipped standard, the one song
  # this machine has performed since the beginning. The other committed rices
  # are reached from the machine's own songbook (`aoide rice stage <n>`), which
  # this host's songbook already holds — that is the point of the split.
  song.declared = "sonata";
  song.available = [ ];

  # ── People ─────────────────────────────────────────────────────────────────
  # The definition is shared between machines and attached, never copied.
  users.khoa = {
    definition = ../../users/khoa.nix;
    homeManager.enable = true;
  };

  # ── This machine ───────────────────────────────────────────────────────────
  # An ordinary NixOS module: hardware, the platform, and the `aoide.*` knobs
  # that genuinely vary per machine. Nothing here can influence selection.
  nixos =
    { lib, pkgs, ... }:
    {
      # Guarded: import ./hardware.nix only when a scan is committed, so the
      # flake still evaluates on a machine that has not run
      # `nixos-generate-config` yet.
      imports = lib.optional (builtins.pathExists ./hardware.nix) ./hardware.nix;

      # The platform is the record's own statement: the constructor assembles
      # the module list and lets the modules name the platform, rather than
      # handing `nixosSystem` a `system` argument.
      nixpkgs.hostPlatform = "x86_64-linux";

      networking.hostName = "yomi-strix";
      networking.networkmanager.enable = true;

      time.timeZone = "America/New_York";

      boot.kernelPackages = pkgs.linuxPackages_latest;
      boot.kernelParams = [
        "amdgpu.gttsize=24576" # MiB (24 GiB) of system RAM the GPU may map
        "ttm.pages_limit=6291456" # 24 GiB in 4 KiB pages, matches gttsize
      ];

      services.xserver.videoDrivers = [ "amdgpu" ];
      # `enable` is the `desktop` aggregation's (mkDefault true); the 32-bit
      # userspace is this machine's own answer, so it is stated here.
      hardware.graphics.enable32Bit = true;

      zramSwap.enable = true;

      # ── Aoide ──────────────────────────────────────────────────────────────
      # `aoide.enable` and `aoide.mcp.enable` are the `base` aggregation's, and
      # `aoide.song` is DERIVED from the record's `song.declared` by the
      # constructor's hook — none of the three is a line here.
      aoide.user = "khoa";

      aoide.hyprland.monitors = [ "HDMI-A-1,1920x1080@60,0x0,1,transform,3" ];
      aoide.hyprland.scrollingMonitor = "HDMI-A-1";

      aoide.usage.enable = true;

      aoide.a2a.enable = true;
      aoide.a2a.discoveryAdvertise = true;
      aoide.a2a.pairingPopup = true;

      aoide.secrets.enable = true;
      aoide.secrets.members = [ "khoa" ];

      # ── SSH ────────────────────────────────────────────────────────────────
      services.openssh = {
        enable = true;
        settings = {
          PermitRootLogin = "no";
          PasswordAuthentication = false;
          KbdInteractiveAuthentication = false;
        };
      };
      # Keys on the account `users/khoa.nix` creates.
      users.users.khoa.openssh.authorizedKeys.keys = [
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEyoERlxyi80OB0h+nw1NKO7Ki5gBfUCv8ufo5D8b8Kk sakaki-to-yomi-strix"
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICSHb3e535b2U/hWEmIsFC2j99SmEayq3HS/IH1c61Aw osaka-to-yomi-strix"
      ];
    };
}
