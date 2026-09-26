# modules/default.nix — the registry: plain data, not a module.
#
# `catalogue` names every dendrite once, by the name a host, a user or an
# aggregation selects it with, and points at the file (or directory) that
# answers it. Nothing here is imported: `lib/composition.nix` reads this
# record before any module graph exists and imports only what selection kept —
# a name with no line is unreachable, and a capability is shelved by dropping
# its line without deleting its file.
#
# `aggregations` and `overrides` are discovered one level deep beside the
# catalogue — names and paths only, no body imported — and are empty until
# `modules/aggregations/` and `modules/overrides/` land.
{
  catalogue = {
    audio = ./dendrites/audio.nix;
    bash = ./dendrites/bash.nix;
    btop = ./dendrites/btop.nix;
    claude-code = ./dendrites/claude-code.nix;
    cli = ./dendrites/cli.nix;
    clipboard = ./dendrites/clipboard.nix;
    devtools = ./dendrites/devtools.nix;
    dunst = ./dendrites/dunst.nix;
    eidolon = ./dendrites/eidolon.nix;
    fastfetch = ./dendrites/fastfetch;
    firefox = ./dendrites/firefox.nix;
    fonts = ./dendrites/fonts.nix;
    git = ./dendrites/git.nix;
    hyprland = ./dendrites/hyprland.nix;
    inference = ./dendrites/inference.nix;
    kimi-code = ./dendrites/kimi-code.nix;
    kitty = ./dendrites/kitty.nix;
    mcfly = ./dendrites/mcfly.nix;
    melete = ./dendrites/melete.nix;
    mneme = ./dendrites/mneme.nix;
    neovim = ./dendrites/neovim.nix;
    networkmanager = ./dendrites/networkmanager.nix;
    nh = ./dendrites/nh.nix;
    obsidian = ./dendrites/obsidian.nix;
    openai = ./dendrites/openai.nix;
    pi-coding-agent = ./dendrites/pi-coding-agent.nix;
    qbittorrent = ./dendrites/qbittorrent.nix;
    screenshot = ./dendrites/screenshot.nix;
    starship = ./dendrites/starship.nix;
    vision = ./dendrites/vision.nix;
    yazi = ./dendrites/yazi.nix;
  };

  aggregations = { };
  overrides = { };
}
