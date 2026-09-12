# modules/dendrites/eidolon.nix — the Eidolon coding harness (~/eidolon, a
# Rust interactive coding harness built to replace pi for daily use).
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.eidolon.enable (default false — shipped but off).
#   - Carries its own dependencies (pkgs/eidolon, a launcher — see its header
#     for why); reads only config.aoide.user plus its own options.
#   - Enable with one line in hosts/ (see hosts/yomi-strix/default.nix).
#
# What this dendrite does:
#   - Installs `eidolon` (pkgs.eidolon, the launcher) system-wide.
#   - Writes ~/.config/eidolon/config.toml with just `[claude_cli]`. Eidolon's
#     own no-config fallback model is `mock`; the mere presence of this table
#     flips it to the already-authenticated `claude` binary
#     (aoide.claude-code.enable), so no secret store or token file is needed
#     to get a real turn — see ~/eidolon's crates/cli/src/config.rs
#     (`default_model`).
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.eidolon.enable = lib.mkEnableOption "the Eidolon coding harness";

  config = lib.mkIf config.aoide.eidolon.enable {
    environment.systemPackages = [ pkgs.eidolon ];

    home-manager.users.${config.aoide.user}.home.file.".config/eidolon/config.toml".text = ''
      [claude_cli]
    '';
  };
}
