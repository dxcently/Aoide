# hosts/chiyo/hardware.nix — REAL hardware profile.
#
# Ported from dxflake's chiyo (nixos-generate-config output plus dxflake's own
# boot/resume additions), trimmed to essentials the same way yomi-strix's was:
# fileSystems/swap pinned by UUID, read from the LIVE box (single NVMe: ext4
# root + vfat ESP + a swap partition big enough for hibernate). Regenerate
# with `nixos-generate-config --show-hardware-config` if the disk ever
# changes.
{
  config,
  lib,
  modulesPath,
  ...
}:
{
  imports = [ (modulesPath + "/installer/scan/not-detected.nix") ];

  # ── Disk ────────────────────────────────────────────────────────────────
  fileSystems."/" = {
    device = "/dev/disk/by-uuid/3ec007cb-f29a-480a-8cfa-9830a33d6b6f";
    fsType = "ext4";
  };
  fileSystems."/boot" = {
    device = "/dev/disk/by-uuid/A124-D737";
    fsType = "vfat";
    options = [
      "fmask=0022"
      "dmask=0022"
    ];
  };
  swapDevices = [
    { device = "/dev/disk/by-uuid/507056e3-b125-424a-aa89-a62a721361a9"; }
  ];
  # Suspend-to-disk target — same partition as the swap device above (dxflake
  # carried this explicitly rather than relying on auto-detection).
  boot.resumeDevice = "/dev/nvme0n1p3";

  # ── Boot: UEFI + systemd-boot (dxflake nucleus baseline, same as yomi-strix
  # — Aoide's hosts/common does not set a bootloader, so every host's own
  # hardware.nix carries it) ─────────────────────────────────────────────────
  boot.loader = {
    systemd-boot.enable = true;
    efi.canTouchEfiVariables = true;
  };

  # ── Kernel module set (nixos-generate-config scan) ─────────────────────────
  boot.initrd.availableKernelModules = [
    "xhci_pci"
    "nvme"
    "usb_storage"
    "sd_mod"
  ];
  boot.initrd.kernelModules = [ ];
  boot.kernelModules = [ "kvm-intel" ];
  boot.extraModulePackages = [ ];

  networking.useDHCP = lib.mkDefault true;

  nixpkgs.hostPlatform = lib.mkDefault "x86_64-linux";
  hardware.cpu.intel.updateMicrocode = lib.mkDefault config.hardware.enableRedistributableFirmware;
}
