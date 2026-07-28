# modules/dendrites/cli.nix — general command-line utilities.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.cli.enable (default false — shipped but off; defaulted
#     ON fleet-wide in hosts/common/default.nix).
#   - Carries its own dependencies; reads no other module.
#
# Charter (why this exists as a sibling of devtools.nix):
#   General-purpose CLI utilities that are NOT dev-specific — shell helpers,
#   archive/file/network/media command-line tools. Scoped by FORM (a small CLI
#   utility), not by fuzzy domain, so it never becomes a media/archives dumping
#   ground. Anything that is a standalone app or service (a media PLAYER, a
#   backup system) gets its OWN specifically-named dendrite; it does not land
#   here. A tool graduates out to its own dendrite the moment a host needs to
#   toggle it independently (cheap + additive — see [[Snowflake-Anatomy]]).
#
# Split from devtools.nix: devtools keeps the genuinely dev-specific set
# (neovide, lazygit, claude-code, nixfmt, nix-tree, nodejs, ngrok); the general
# utilities live here. With both defaulted-on in common, the aggregate is
# unchanged from the old single devtools set.
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.cli.enable = lib.mkEnableOption "general CLI utilities (fzf, fd, ripgrep, jq, ffmpeg, …)";

  config = lib.mkIf config.aoide.cli.enable {
    # unrar is unfree. Scoped here: the dendrite that needs unfree carries the
    # switch (narrowest scope wins).
    nixpkgs.config.allowUnfree = true;

    environment.systemPackages = with pkgs; [
      # ── Shell & Terminal ──
      fzf # command-line fuzzy finder
      htop # interactive process viewer

      # ── File System & Archives ──
      unrar # extract RAR archives
      unzip # extract ZIP archives
      unar # universal unarchiver
      fd # fast, user-friendly find alternative
      file # determine file type via magic bytes
      xdg-utils # XDG MIME and desktop integration tools

      # ── Media CLI ──
      ffmpeg # audio/video encoding framework

      # ── Search & Text ──
      ripgrep # recursive regex search (rg)
      jq # command-line JSON processor
      vim # vi-compatible modal text editor

      # ── Network ──
      curl # transfer data with URLs
      wget # non-interactive network downloader
    ];
  };
}
