# modules/dendrites/eidolon.nix — the Eidolon coding harness (~/eidolon, a
# Rust interactive coding harness built to replace pi for daily use).
#
# Dendrite shape (CONTRACTS.md §2):
#   - Selection is the switch: no enable flag; the body runs when habit selects it.
#   - Carries its own dependencies (pkgs/eidolon, a launcher — see its header
#     for why); reads only config.aoide.user plus its own options.
#   - Select with one line in hosts/ (see hosts/yomi-strix/default.nix).
#
# What this dendrite does:
#   - Installs `eidolon` (pkgs.eidolon, the launcher) system-wide.
#   - Writes ~/.config/eidolon/config.toml: `default_model`, `[claude_cli]`,
#     and the `[mneme]` vault wiring. Eidolon's own no-config fallback model is
#     `mock`; the
#     mere presence of that table flips it to the already-authenticated
#     `claude` binary (the claude-code dendrite, when selected), so no secret store or token
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
#     `default_persona = "10 ♜ AGENT/rook/AGENTS.md"` is the voice every
#     session a launch *creates* opens in: the pin resolved against the vault
#     below, where that path is Rook — a path, not a bare name, because the
#     vault keeps its agents under `10 ♜ AGENT/<agent>/` and carries no
#     `wiki/personalities/` entry to pin by. A launch that names its own voice
#     (`--persona`, `:persona`), or a reopened log that already carries one,
#     is never re-dressed; a pin that resolves to nothing is reported on
#     stderr rather than worn silently.
#
#     `fallback_model` names no field in the binary — crates/cli/src/config.rs
#     carries `default_persona`, never a bare `persona`, and no
#     `fallback_model` at all — so that line is inert, and the substitution a
#     launch does perform when a journaled model no longer resolves is
#     `default_model`, announced as `[model] … opening this session on …`.
#     Unknown keys are ignored rather than refused, which is how a line can
#     land here before the install does.
#
#     The `[mneme]` stanza points at the vault on sakaki. Its passphrase is
#     deliberately NOT written here: ~/.config/eidolon/mneme.pass (0600) is
#     the operator's out-of-band file, read and trimmed by Eidolon at call
#     time, so the credential never enters the Nix store. Two notes ride a
#     pin — the persona, and the memory beside it — and the memory folder is
#     `wiki/agent-memory/` in the binary (`PERSONA_MEMORY_FOLDER`,
#     crates/core/src/persona.rs), so a vault that keeps its memory elsewhere
#     wears the persona alone.

{
  config,
  lib,
  pkgs,
  ...
}:
{

  config = {
    environment.systemPackages = [ pkgs.eidolon ];

    habit.home.home.file.".config/eidolon/config.toml".text = ''
      default_model = "ollama:deepseek-v4.1-flash"
      fallback_model = "sonnet"
      default_persona = "10 ♜ AGENT/rook/AGENTS.md"

      [claude_cli]

      # The vault (Mneme). Personas and notes are read from here; the
      # passphrase is the operator's out-of-band file, not a Nix value.
      [mneme]
      mcp_url = "https://mneme.necoconeco.net/mcp"
      passphrase_file = "~/.config/eidolon/mneme.pass"
    '';
  };
}
