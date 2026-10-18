# hosts/yomi-strix/default.nix — the reference host.
#
# One NixOS module. Its `habit.*` keys are what this machine SELECTS
# (aggregations, lone capabilities, people); everything else is this machine's
# own platform settings, evaluated by NixOS and by nothing before it. It names
# no module file — nothing outside `modules/default.nix` does.
#
# Nothing here is registered anywhere: `flake.nix` discovers `hosts/` one level
# deep, so adding a machine is a new directory and never an edit to a list.
{ lib, pkgs, ... }:
{
  # ── Aggregations ───────────────────────────────────────────────────────────
  # nucleus is the floor; desktop is the session's platform; agents is the coding
  # agents a workstation runs; aoideos is the desktop's floor. The paint is the
  # performed song's, selected below.
  habit.aggregation.nucleus.enable = true;
  habit.aggregation.desktop.enable = true;
  habit.aggregation.agents.enable = true;
  habit.aggregation.aoideos.enable = true;

  # ── Lone capabilities ──────────────────────────────────────────────────────
  # What no aggregation speaks for. `qbittorrent` is here because no group
  # claims it; `obsidian` in the same way; `inference` is a server a host either
  # hosts or does not.
  habit.dendrites.obsidian.enable = true;
  habit.dendrites.qbittorrent.enable = true;
  habit.dendrites.inference.enable = true;
  habit.dendrites.tailscale.enable = true;
  # The secrets broker is not nucleus's: this host names it, and `aoide.secrets.members`
  # below exists only because it is selected.
  habit.dendrites.aoide-secrets.enable = true;

  # ── Who paints a wallpaper pick ────────────────────────────────────────────
  # The compositor line's twin: the performed song ships the shell's own layer
  # (`strillix`) as the setter, and this host names the external engine instead.
  # A plain write here outranks the song's default. The engine's lane is what
  # makes that real (packages, daemon, config), and the runtime reads the
  # resulting fact to decide whose picks the stage file records.
  habit.dendrites.wallpaper.provider = "skwd-wall";

  # ── Songs ──────────────────────────────────────────────────────────────────
  # `habit.rice.declared` is the song this host PERFORMS: it becomes the fact
  # `aoide.rice` and selects that song's body. `available` is what it builds in
  # to STAGE without a rebuild but does not perform.
  #
  # yomi performs barcarolle: Windows 7 Aero rebuilt on the AoideOS slots — a
  # glass superbar whose buttons are Hyprland workspaces, the Start menu and its
  # flyouts, the iMac G3 launcher, white-plate balloons, over a teal Frutiger
  # Aero wallpaper. Declaring is what bakes the half a stage cannot: Stylix
  # theming, the compositor's geometry and window borders, the assembled
  # widgets, and the boot default; `strillix reload` is what keeps the staged half
  # live between switches. The other committed rices are reached from the
  # machine's own rices (`aoide rice stage <n>`), which this host's rices
  # already holds — that is the point of the split.
  habit.rice.declared = "barcarolle";
  habit.rice.available = [ ];

  # ── People ─────────────────────────────────────────────────────────────────
  # The definition is shared between machines and attached, never copied.
  habit.users.khoa = {
    definition = ../../users/khoa.nix;
    home.enable = true;
  };

  # ── This machine ───────────────────────────────────────────────────────────
  # An ordinary NixOS module: hardware, the platform, and the `aoide.*` knobs
  # that genuinely vary per machine. Nothing here can influence selection.
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
  # The `nucleus` dendrites select the core (`aoide.enable`) and the doors
  # below exist because those dendrites are selected; `aoide.rice` is DERIVED
  # from `habit.rice.declared` by the constructor's hook. None of them is a
  # line here.
  aoide.user = "khoa";

  # Monitors: none declared, so Hyprland autodetects whatever is plugged in
  # (DP-1 landscape today) and every workspace stays on the global dwindle.
  # A rotated or scrolling panel is a host value, not a module edit:
  #   aoide.hyprland.monitors = [ "HDMI-A-1,1920x1080@60,0x0,1,transform,3" ];
  #   aoide.hyprland.scrollingMonitor = "HDMI-A-1";

  # Take subnet routes from the tailnet: off campus, owls-nest's
  # 10.1.10.236/32 route is how this machine reaches *.minerva-csc.club.
  aoide.tailscale.acceptRoutes = true;

  aoide.usage.enable = true;

  aoide.a2a.enable = true;
  aoide.a2a.discoveryAdvertise = true;
  aoide.a2a.pairingPopup = true;

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
}
