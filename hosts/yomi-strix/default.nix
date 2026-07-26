# hosts/yomi-strix/default.nix — the reference host.
#
# Flags only, plus a GUARDED hardware import. A host never imports module files
# directly; it flips `aoide.*` flags and pulls its own ./hardware.nix. The
# hardware import is guarded so eval works even before a real hardware scan
# exists (Wave 0 ships a placeholder stub; a real host regenerates it).
{ lib, ... }:
{
  imports =
    [ ../common ]
    # Guarded: import ./hardware.nix only if the file exists, so the flake
    # still evaluates on a machine without a committed hardware scan.
    ++ lib.optional (builtins.pathExists ./hardware.nix) ./hardware.nix;

  networking.hostName = "yomi-strix";

  # Aoide flags for this box. Facets/dendrites (Wave 1) read these; this line
  # is the entire host-side wiring for the desktop.
  aoide.enable = true;
  aoide.user = "khoa";

  # Wave-1 facet/aggregation flags land here as one line each, e.g.:
  #   aoide.facets.quickshell.enable = true;
  #   aoide.facets.compositor.enable = true;
  #   aoide.facets.stylix.enable     = true;
}
