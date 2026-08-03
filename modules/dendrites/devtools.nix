# modules/dendrites/devtools.nix — the CLI dev-tool toolbox.
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.devtools.enable (default false — shipped but off;
#     defaulted ON fleet-wide in hosts/common/default.nix).
#   - Carries its own dependencies; reads no other module.
#
# Scope: genuinely DEV-specific tools only. General command-line utilities
# (fzf, fd, ripgrep, jq, ffmpeg, archives, curl/wget, vim, …) live in the
# sibling cli.nix dendrite — this file is the developer's set: editors/git UI,
# Nix tooling, a JS runtime, tunnels. This is where bash.nix's `lg` (lazygit)
# alias gets its binary. Agentic coding CLIs (claude-code,
# pi-coding-agent) each got their OWN dendrite — same reasoning as the
# cli.nix split: a tool graduates out the moment a host wants to toggle it
# independently. `crc` (claude --rc) in bash.nix now gets its binary from
# claude-code.nix instead.
#
# Adapted vs dxflake (ported from its nucleus/packages.nix):
#   - bash/starship/fastfetch/git/nh are SKIPPED — their own dendrites own them.
#   - The "Hardware & System Administration" section is OMITTED (sysadmin, not
#     dev); sops/age belong to dxflake's secrets stack, which Aoide doesn't have.
#   - dxflake's openldap overlay is NOT ported (a dxflake-local build fix).
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.devtools.enable = lib.mkEnableOption "the CLI dev-tool toolbox (lazygit, neovide, nix tooling, …)";

  config = lib.mkIf config.aoide.devtools.enable {
    # ngrok is unfree. Scoped here: the dendrite that needs unfree carries the
    # switch (narrowest scope wins).
    nixpkgs.config.allowUnfree = true;

    environment.systemPackages = with pkgs; [
      # ── Editors & Git ──
      neovide # GPU-accelerated Neovim GUI
      lazygit # terminal UI for git

      # ── Runtimes ──
      nodejs # cross-platform JavaScript runtime

      # ── Nix tooling ──
      nixfmt # formatter for Nix source code
      nix-tree # browse Nix derivation closures

      # ── Networking (dev) ──
      ngrok # expose local servers via secure tunnels
    ];
  };
}
