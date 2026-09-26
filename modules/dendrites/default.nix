# modules/dendrites/default.nix — every opt-in capability this tree ships.
#
# One line per file, in directory order. A new dendrite is a new file plus
# one line here; deleting both removes it without a trace. A `_`-prefixed
# file is never listed — that is what the prefix means.
{
  imports = [
    ./audio.nix
    ./bash.nix
    ./btop.nix
    ./claude-code.nix
    ./cli.nix
    ./clipboard.nix
    ./devtools.nix
    ./dunst.nix
    ./eidolon.nix
    ./fastfetch
    ./firefox.nix
    ./fonts.nix
    ./git.nix
    ./hyprland.nix
    ./inference.nix
    ./kimi-code.nix
    ./kitty.nix
    ./mcfly.nix
    ./melete.nix
    ./mneme.nix
    ./neovim.nix
    ./networkmanager.nix
    ./nh.nix
    ./obsidian.nix
    ./openai.nix
    ./pi-coding-agent.nix
    ./qbittorrent.nix
    ./screenshot.nix
    ./starship.nix
    ./vision.nix
    ./yazi.nix
  ];
}
