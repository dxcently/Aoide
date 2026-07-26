# pkgs/tokens/default.nix — the design-token package (Node / Style Dictionary).
#
# ┌─ WAVE-0 PLACEHOLDER ─────────────────────────────────────────────────────┐
# │ Trivial derivation so `nix flake check` evals & builds green with an      │
# │ empty package dir. AGENT A replaces the body with the real Node package   │
# │ that wraps Style Dictionary (resolver + schema lint + live emitters).     │
# │ See docs/BUILD.md. Keep the callPackage signature stable.                 │
# └──────────────────────────────────────────────────────────────────────────┘
#
# Contract Agent A must honour:
#   * Stays `pkgs/tokens/default.nix`, `callPackage`-able.
#   * `pname = "aoide-tokens"`.
#   * Exposes the resolver + schema-lint (called by `rice lint`) and the three
#     live-side emitters: stage/tokens.json, hyprctl dispatcher, terminal OSC
#     (see concepts/Design-Tokens). Palette tier is base16-closed; v0 component
#     tier is bar.*/notif.*/window.* (matches modules/nucleus/options.nix).
{
  lib,
  runCommand,
  # ── Agent A: add the real build inputs, e.g. ──
  # buildNpmPackage, nodejs, style-dictionary,
  ...
}:
runCommand "aoide-tokens-0.0.0-placeholder"
  {
    pname = "aoide-tokens";
    version = "0.0.0-placeholder";
    meta.description = "Aoide design-token package (Wave-0 placeholder; Agent A replaces).";
  }
  ''
    mkdir -p "$out/share/aoide-tokens"
    cat > "$out/share/aoide-tokens/README" <<'EOF'
    Wave-0 placeholder for the Aoide token package.
    Agent A: replace pkgs/tokens/default.nix per docs/BUILD.md.
    EOF
  ''
