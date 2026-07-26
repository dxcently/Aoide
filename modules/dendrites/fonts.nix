# modules/dendrites/fonts.nix — the system font set (glyph coverage).
#
# Dendrite shape v0 (CONTRACTS.md §2):
#   - Guarded on aoide.fonts.enable (default false — shipped but off).
#   - Carries its own dependencies; reads no other module.
#   - Enable with one line in hosts/ (see hosts/common/default.nix).
#
# What this dendrite does (ported from dxflake modules/dendrites/fonts.nix):
#   - Installs the system-wide font set. Aoide's desktop look leans on
#     musical-notation glyphs everywhere (𝄞 𝅘𝅥 𝄽 …); those live outside the
#     BMP and are NOT in a typical monospace font. Coverage rationale:
#       * symbola + noto-fonts carry the Musical Symbols block (U+1D100…),
#         so notation renders instead of tofu.
#       * noto-fonts-cjk-sans + the two azuki fonts cover CJK / kana.
#       * noto-fonts-color-emoji carries colour emoji.
#       * material-icons / font-awesome / fira-code-symbols carry UI glyphs.
#       * nerd-fonts.lekton is the desktop's primary face (stylix facet points
#         monospace/sans/serif at "Lekton Nerd Font Mono"); the other nerd
#         fonts are alternates.
#   - The two azuki fonts build from pkgs/azuki-font{,-b} (fetchzip from
#     azukifont.com — no local asset, so callPackage is enough).
#
# Unfree note: corefonts is unfree. devtools.nix also sets
# nixpkgs.config.allowUnfree = true inside its own mkIf, but a host could enable
# fonts WITHOUT devtools, so this dendrite carries the switch too. Both settings
# are mkIf-scoped to the same value (true), which the module system merges
# cleanly (agreeing definitions never conflict) — verified by eval.
{
  config,
  lib,
  pkgs,
  ...
}:
{
  options.aoide.fonts.enable = lib.mkEnableOption "the system font set (broad glyph coverage: musical notation, CJK, emoji, nerd fonts)";

  config = lib.mkIf config.aoide.fonts.enable {
    # corefonts is unfree. Scoped here so a host can enable fonts without also
    # pulling the devtools dendrite (which sets the same switch for its own
    # unfree packages). Agreeing mkIf definitions merge cleanly.
    nixpkgs.config.allowUnfree = true;

    fonts.packages = with pkgs; [
      corefonts # Microsoft core web fonts (unfree)
      noto-fonts # broad Unicode coverage incl. Musical Symbols
      noto-fonts-cjk-sans # CJK sans coverage
      noto-fonts-color-emoji # colour emoji
      material-icons # Material Design UI glyphs
      font-awesome # Font Awesome UI glyphs
      fira-code-symbols # programming-ligature symbols
      symbola # symbol coverage incl. musical notation (U+1D100…)
      nerd-fonts.jetbrains-mono # nerd-patched JetBrains Mono
      nerd-fonts.comic-shanns-mono # nerd-patched Comic Shanns Mono
      nerd-fonts.shure-tech-mono # nerd-patched Share Tech Mono
      nerd-fonts.lekton # primary desktop face (stylix points here)
      (pkgs.callPackage ../../pkgs/azuki-font-b { }) # azuki kana font (B weight)
      (pkgs.callPackage ../../pkgs/azuki-font { }) # azuki kana font
    ];
  };
}
