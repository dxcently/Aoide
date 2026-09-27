# modules/default.nix — the catalogue: plain data, not a module.
#
# `catalogue` names every dendrite once, by the name a host, a user or an
# aggregation selects it with, and points at the file (or directory) that
# answers it. It is the ONE place a dendrite file is named (`AGENTS.md` house
# rule 7): `modules/dendrites/default.nix` derives its imports from it, so one
# line here is both what makes a dendrite selectable and what puts it in the
# full tree — a name with no line is unreachable, and a capability is shelved by
# dropping its line without deleting its file.
#
# Nothing here joins a module graph: `lib/composition.nix` reads this record
# before any module graph exists and imports only what selection kept.
#
# `aggregations` and `overrides` are read one level deep beside the catalogue —
# names and paths only, no body imported — by their own directories' discovery
# files. An aggregation is inert data whose only effect is the gate it answers;
# an override is read by every host, because matching means reading what it
# targets, and what an unmatched one never costs is its work.
{
  catalogue = {
    audio = ./dendrites/audio.nix;
    bash = ./dendrites/bash.nix;
    btop = ./dendrites/btop.nix;
    claude-code = ./dendrites/claude-code.nix;
    cli = ./dendrites/cli.nix;
    clipboard = ./dendrites/clipboard.nix;
    # The compositor capability: a provider registry, not a lane record. Its
    # `hyprland` provider is the LOOK; the BEHAVIOUR is the `hyprland` entry
    # below, which keeps that name because `aoide.hyprland.*` stays a
    # host-facing option.
    compositor = ./dendrites/compositor;
    devtools = ./dendrites/devtools.nix;
    dunst = ./dendrites/dunst.nix;
    eidolon = ./dendrites/eidolon.nix;
    fastfetch = ./dendrites/fastfetch;
    firefox = ./dendrites/firefox.nix;
    fonts = ./dendrites/fonts.nix;
    git = ./dendrites/git.nix;
    greeter = ./dendrites/greeter.nix;
    hyprland = ./dendrites/compositor/hyprland/behaviour.nix;
    inference = ./dendrites/inference.nix;
    kimi-code = ./dendrites/kimi-code.nix;
    kitty = ./dendrites/kitty.nix;
    lyra = ./dendrites/lyra;
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
    quickshell = ./dendrites/quickshell.nix;
    screenshot = ./dendrites/screenshot.nix;
    starship = ./dendrites/starship.nix;
    stylix = ./dendrites/stylix.nix;
    vision = ./dendrites/vision.nix;
    yazi = ./dendrites/yazi.nix;
  };

  aggregations = import ./aggregations;
  overrides = import ./overrides;
}
