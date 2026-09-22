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
#   - Writes ~/.config/eidolon/config.toml: `default_model`, `[claude_cli]`,
#     and the `[mneme]` vault wiring. Eidolon's own no-config fallback model is
#     `mock`; the
#     mere presence of that table flips it to the already-authenticated
#     `claude` binary (aoide.claude-code.enable), so no secret store or token
#     file is needed to get a real turn — see ~/eidolon's
#     crates/cli/src/config.rs (`default_model`). `default_model` then names
#     the model a new session actually starts on, ahead of that fallback
#     (which is the CLI's own `sonnet`) — here the Ollama-hosted DeepSeek V4.1
#     Flash the daily driver had settled on, so switching to it is not a
#     per-launch ritual.
#
#     Note the file is a home-manager `home.file`, hence read-only in the
#     store: changing the default model is a change here plus a rebuild, not
#     an edit to ~/.config/eidolon/config.toml.
#
#     `fallback_model` is the second name behind that one: a launch whose
#     model does not resolve on this machine (key never stored, endpoint
#     moved) opens on `sonnet` — the Claude CLI, on the subscription — instead
#     of refusing. It is *resolution* only: a provider that answers and then
#     errors mid-turn still surfaces that error, and `space m` is how a
#     running session changes its mind. `persona = "Rook"` is the voice every
#     session starts in, resolved against the vault below; a session that pins
#     its own voice — or a reopened log that already carries one — is never
#     re-dressed. Both keys are additions in ~/eidolon
#     (crates/cli/src/config.rs, `fallback_model`/`persona`): a binary older
#     than them ignores the lines silently rather than failing, which is why
#     they can land before the install does.
#
#     The `[mneme]` stanza points at the vault on sakaki. Its passphrase is
#     deliberately NOT written here: ~/.config/eidolon/mneme.pass (0600) is
#     the operator's out-of-band file, read and trimmed by Eidolon at call
#     time, so the credential never enters the Nix store. With the vault
#     wired, personas come alive — Rook is wiki/personalities/Rook ♜.md, worn
#     per session with `eidolon tui --persona Rook` or `:persona Rook`.
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
      default_model = "ollama:deepseek-v4.1-flash"
      fallback_model = "sonnet"
      persona = "Rook"

      [claude_cli]

      # The vault (Mneme). Personas and notes are read from here; the
      # passphrase is the operator's out-of-band file, not a Nix value.
      [mneme]
      mcp_url = "https://mneme.necoconeco.net/mcp"
      passphrase_file = "~/.config/eidolon/mneme.pass"
    '';
  };
}
