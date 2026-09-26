# modules/dendrites/default.nix — every opt-in capability this tree ships.
#
# One line per file, in directory order. A dendrite file is a lane record
# (`{ body; nixos; }`, CONTRACTS.md §2): `modules/default.nix`'s catalogue
# names it, the constructor (`lib/composition.nix`) imports the `nixos` lane of
# the ones a host selected, and this aggregate imports each `body` so that a
# host taking the whole tree still sees every `aoide.<name>.*` option and its
# own guard.
#
# A new dendrite is a new file plus one line here and one in the catalogue;
# deleting both removes it without a trace. A `_`-prefixed file is never
# listed — that is what the prefix means.
{
  imports = [
    (import ./audio.nix).body
    (import ./bash.nix).body
    (import ./btop.nix).body
    (import ./claude-code.nix).body
    (import ./cli.nix).body
    (import ./clipboard.nix).body
    (import ./devtools.nix).body
    (import ./dunst.nix).body
    (import ./eidolon.nix).body
    (import ./fastfetch).body
    (import ./firefox.nix).body
    (import ./fonts.nix).body
    (import ./git.nix).body
    (import ./hyprland.nix).body
    (import ./inference.nix).body
    (import ./kimi-code.nix).body
    (import ./kitty.nix).body
    (import ./mcfly.nix).body
    (import ./melete.nix).body
    (import ./mneme.nix).body
    (import ./neovim.nix).body
    (import ./networkmanager.nix).body
    (import ./nh.nix).body
    (import ./obsidian.nix).body
    (import ./openai.nix).body
    (import ./pi-coding-agent.nix).body
    (import ./qbittorrent.nix).body
    (import ./screenshot.nix).body
    (import ./starship.nix).body
    (import ./vision.nix).body
    (import ./yazi.nix).body
  ];
}
