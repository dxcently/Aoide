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
  # line — e.g. `aoide.song = "sonata";` swaps the whole notes fan-out with
  # zero other edits (song/repertoire/moonlight/). Default = the shipped standard.
  # "hero": the dusk-plum key drawn from the hero cover itself.
  aoide.song = "sonata";

  # Wave-1 facets — the whole desktop, one line each.
  aoide.facets.quickshell.enable = true;
  aoide.facets.compositor.enable = true;
  aoide.facets.stylix.enable = true;

  # Screen capture — two callers, two dendrites (see each module header):
  # hyprshot+satty for the human (SUPER+S), grim/slurp for agents ("vision").
  aoide.screenshot.enable = true;
  aoide.vision.enable = true;

  # Shipped dendrites (off unless wanted; aoide.mcp.enable stays false — house policy).
  aoide.obsidian.enable = false;
}
