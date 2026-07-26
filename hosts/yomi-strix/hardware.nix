# hosts/yomi-strix/hardware.nix — PLACEHOLDER hardware stub.
#
# GUARDED PLACEHOLDER: enough for `config.system.build.toplevel` to EVALUATE.
# It is NOT a real hardware scan and will NOT boot. A real deployment
# regenerates this with `nixos-generate-config --show-hardware-config` (or
# nixos-anywhere's --generate-hardware-config). Wave 0 keeps this minimal so
# the walking skeleton evals green; no `nixos-rebuild switch` is performed.
{ lib, ... }:
{
  # Minimal boot + root fs so toplevel evaluates. Real values come from the scan.
  boot.loader.grub.enable = lib.mkDefault true;
  boot.loader.grub.device = lib.mkDefault "nodev";

  fileSystems."/" = lib.mkDefault {
    device = "/dev/disk/by-label/nixos";
    fsType = "ext4";
  };

  # Placeholder CPU/microcode-neutral defaults; no firmware assumptions.
  boot.initrd.availableKernelModules = lib.mkDefault [ "nvme" "xhci_pci" "ahci" "sd_mod" ];
  boot.kernelModules = lib.mkDefault [ "kvm-amd" ];
}
