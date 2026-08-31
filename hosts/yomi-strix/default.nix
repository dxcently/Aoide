# hosts/yomi-strix/default.nix — the reference host.
#
# Flags + venue specifics, plus a GUARDED hardware import. A host never
# imports module files directly; it flips `aoide.*` flags, states its own
# hardware picks (the venue decides its instruments), and pulls ./hardware.nix.
# First iteration: real profile ported from dxflake (the template), trimmed to
# what boots this box and runs the desktop.
{ lib, pkgs, ... }:
{
  imports = [
    ../common
  ]
  # Guarded: import ./hardware.nix only if the file exists, so the flake
  # still evaluates on a machine without a committed hardware scan.
  ++ lib.optional (builtins.pathExists ./hardware.nix) ./hardware.nix;

  networking.hostName = "yomi-strix";
  networking.networkmanager.enable = true;

  # Venue clock — the box sat on UTC with no zone set. Pin US Eastern to match
  # the dxflake reference rig; timesyncd (default-on) keeps it NTP-synced.
  time.timeZone = "America/New_York";

  # ── Venue specifics (dxflake-templated, essentials only) ──────────────────
  # Strix Halo (Ryzen AI Max) is new silicon — ride the latest kernel for the
  # freshest amdgpu. gttsize/ttm let the iGPU borrow a large slice of the
  # unified 32 GB pool (24 GiB GPU-mappable, ~8 GiB left for CPU/OS).
  boot.kernelPackages = pkgs.linuxPackages_latest;
  boot.kernelParams = [
    "amdgpu.gttsize=24576" # MiB (24 GiB) of system RAM the GPU may map
    "ttm.pages_limit=6291456" # 24 GiB in 4 KiB pages, matches gttsize
  ];

  # RDNA 3.5 iGPU: kernel driver + userspace graphics for the Hyprland facet.
  services.xserver.videoDrivers = [ "amdgpu" ];
  hardware.graphics = {
    enable = true;
    enable32Bit = true;
  };

  # 32 GB is modest while the iGPU eats RAM — compressed in-RAM swap cushion.
  zramSwap.enable = true;

  # Aoide flags for this box. Facets/dendrites (Wave 1) read these; this line
  # is the entire host-side wiring for the desktop.
  aoide.enable = true;
  aoide.user = "khoa";

  # The song this host performs. Replay any committed song on ANY host with one
  # line — e.g. `aoide.song = "moonlight";` swaps the whole livery fan-out with
  # zero other edits (song/songbook/<name>/). REQUIRED, not decorative:
  # `aoide.song` defaults to null, and a host that names no song deploys no
  # QML and runs no shell service — the paint facets only activate once a
  # song is named. `sonata` is the shipped standard, the guaranteed-present
  # baseline this host opts into by name. "sonata": the light glass key drawn
  # from its own cover.
  aoide.song = "sonata";

  # Wave-1 facets — the whole desktop, one line each.
  aoide.facets.quickshell.enable = true;
  aoide.facets.compositor.enable = true;
  aoide.facets.stylix.enable = true;

  # Host-invariant Hyprland behaviour (keybinds, input, layout, window rules).
  # Paired with the compositor facet above: that one owns the look, this one
  # owns everything a re-rice must not touch.
  aoide.hyprland.enable = true;

  # The Samsung C24F390 hangs in PORTRAIT. Its EDID still reports the panel's
  # native landscape geometry (520x290mm), so the rotation has to be declared:
  # transform 3 is counter-clockwise, giving an effective 1080x1920. Flip the
  # 3 to a 1 for clockwise — that single token is the whole change, and either
  # direction can be tried live first with
  #   hyprctl keyword monitor HDMI-A-1,1920x1080@60,0x0,1,transform,1
  aoide.hyprland.monitors = [ "HDMI-A-1,1920x1080@60,0x0,1,transform,3" ];

  # Scrolling columns suit the tall geometry; every other output keeps the
  # global dwindle default, so this survives plugging a landscape monitor
  # back in.
  aoide.hyprland.scrollingMonitor = "HDMI-A-1";

  # Screen capture — two callers, two dendrites (see each module header):
  # hyprshot+satty for the human (SUPER+S), grim/slurp for agents ("vision").
  aoide.screenshot.enable = true;
  aoide.vision.enable = true;

  # Text clipboard history provider (cliphist + wl-clipboard) with QML picker.
  aoide.clipboard.enable = true;

  # Notification daemon — dunst owns org.freedesktop.Notifications but draws
  # nothing; it feeds `aoide herald push`, and the Quickshell herald draws the
  # popup and the dock ledger.
  aoide.dunst.enable = true;

  # Audio backend (PipeWire + WirePlumber) — real volume control for the bar.
  aoide.audio.enable = true;

  # claude.ai usage ledger — the dock's Usage stele reads state/usage.json,
  # and this flag is the only thing that keeps it fed; without the poller the
  # gadget draws whatever the last hand-run left and marks itself stale. The
  # live half spends this account's own OAuth token, which is why the option
  # ships off and a host opts in by name.
  aoide.usage.enable = true;

  # NetworkManager applet (nm-connection-editor + nm-applet; NM itself is the
  # venue's own `networking.networkmanager.enable` above).
  aoide.networkmanager.enable = true;

  # A2A door resident (was hand-started in every earlier live test) — this box
  # and sakaki are mutual peers for the live who/send mesh checks.
  aoide.a2a.enable = true;

  # Inbound ssh, keys-only — the doors are loopback-bound by policy, so an
  # ssh tunnel is the ONLY transport a peer can ride to reach this box's
  # far-door; without sshd here, sakaki's tunnel leg (its yomi-strix peer
  # record dials 127.0.0.1:18711) can never come up. Password auth stays off:
  # the enrolled peer keys below are the entire guest list.
  services.openssh = {
    enable = true;
    settings = {
      PermitRootLogin = "no";
      PasswordAuthentication = false;
      KbdInteractiveAuthentication = false;
    };
  };
  users.users.khoa.openssh.authorizedKeys.keys = [
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIEyoERlxyi80OB0h+nw1NKO7Ki5gBfUCv8ufo5D8b8Kk sakaki-to-yomi-strix"
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICSHb3e535b2U/hWEmIsFC2j99SmEayq3HS/IH1c61Aw osaka-to-yomi-strix"
  ];

  # LAN discovery: this box both announces itself and runs `peer discover`.
  # Advertising is what opens UDP 8711 (aoided.nix wires the firewall off this
  # flag), and the port has to be open to HEAR beacons as well as send them —
  # a NixOS default-deny firewall drops even this host's own multicast
  # loopback copy when it arrives on a real interface (task #98). Discovery
  # grants nothing on its own: pairing remains the only thing that writes a
  # peer record.
  aoide.a2a.discoveryAdvertise = true;

  # Secrets broker (workstream #58, P-V4 deployment): own uid, socket-only
  # door. The operator joins the access group; enrollment happens only when
  # the User says connect.
  aoide.secrets.enable = true;
  aoide.secrets.members = [ "khoa" ];

  # Shipped dendrites (off unless wanted; aoide.mcp.enable stays false — house policy).
  aoide.obsidian.enable = true;
  aoide.firefox.enable = true;
  aoide.claude-code.enable = true;
  aoide.kimi-code.enable = true;
  aoide.pi-coding-agent.enable = true;
}
