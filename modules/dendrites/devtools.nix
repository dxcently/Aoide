# modules/dendrites/devtools.nix — the dxflake CLI dev-tool toolbox.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.devtools.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/common/default.nix).
#
# What this dendrite does (ported from dxflake modules/nucleus/packages.nix):
#   - Installs the dev-tool package set system-wide, keeping dxflake's section
#     layout. This is where bash.nix's `lg` (lazygit) and `crc` (claude --rc)
#     aliases get their binaries.
#   - Sets nixpkgs.config.allowUnfree — claude-code, ngrok (and unrar) are
#     unfree; dxflake set this in its nucleus packages.nix, Aoide keeps it
#     scoped to the dendrite that needs it.
#
# Adapted vs dxflake:
#   - bash/starship/fastfetch are SKIPPED — their own dendrites own them.
#   - git is SKIPPED — the nucleus carries it; nh is SKIPPED — the nh
#     dendrite's programs.nh installs it.
#   - The whole "Hardware & System Administration" section is OMITTED
#     (gptfdisk/lshw/usbutils/lm_sensors/v4l-utils/clinfo/poppler/socat —
#     sysadmin, not dev tools; sops/age belong to dxflake's secrets stack,
#     which Aoide doesn't have).
#   - dxflake's openldap overlay is NOT ported (a dxflake-local build fix).
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.devtools.enable = lib.mkEnableOption "the CLI dev-tool toolbox (dxflake package set: lazygit, claude-code, ripgrep, …)";

  config = lib.mkIf config.aoide.devtools.enable {
    # claude-code / ngrok / unrar are unfree. Scoped here: the dendrite that
    # needs unfree carries the switch (narrowest scope wins).
    nixpkgs.config.allowUnfree = true;

    environment.systemPackages = with pkgs; [
      # ── Shell & Terminal ──
      bat # cat clone with syntax highlighting
      fzf # command-line fuzzy finder
      htop # interactive process viewer
      cowsay # configurable speaking ASCII cow

      # ── File System & Archives ──
      unrar # extract RAR archives
      unzip # extract ZIP archives
      unar # universal unarchiver
      fd # fast, user-friendly find alternative
      file # determine file type via magic bytes
      xdg-utils # XDG MIME and desktop integration tools

      # ── Media CLI ──
      ffmpeg # audio/video encoding framework
      yt-dlp # feature-rich youtube-dl fork

      # ── Development & Engineering ──
      vim # vi-compatible modal text editor
      neovide # GPU-accelerated Neovim GUI
      lazygit # terminal UI for git
      ripgrep # recursive regex search (rg)
      jq # command-line JSON processor
      curl # transfer data with URLs
      wget # non-interactive network downloader
      claude-code # agentic AI coding assistant
      # Shelved: heavyweight one-project tools from the dxflake set — not
      # sensible baseline defaults. Re-enable by uncommenting (same convention
      # as neovim.nix's shelved languages).
      # godot # 2D/3D cross-platform game engine
      # arduino-ide # IDE for Arduino microcontrollers
      # jupyter # interactive computational notebooks
      typst # markup-based document typesetting
      tinymist # Typst language server
      nixfmt # formatter for Nix source code
      nix-tree # browse Nix derivation closures
      ngrok # expose local servers via secure tunnels
      nodejs # cross-platform JavaScript runtime
    ];
  };
}
