# modules/dendrites/compositor/default.nix — the compositor capability's
# provider registry (CONTRACTS.md §2).
#
# A capability with alternatives is a registry of provider NAMES to provider
# FILES and nothing else: the constructor (habit's composition) reads this
# entry when the capability is enabled and imports exactly the provider the
# selecting host asked for, so an unselected alternative is never read. The
# catalogue names THIS directory (`compositor = ./dendrites/compositor;`) and
# nothing names a provider file but this line — one place each.
#
# `hyprland` is the only compositor today. Its two halves live beside this
# file: `hyprland/default.nix` is the LOOK (livery-derived appearance and
# session plumbing, guarded on the `aoide.compositor.enable` fact) and
# `hyprland/behaviour.nix` is the BEHAVIOUR (keybinds, input, tiling, window
# rules — its own catalogue entry, `hyprland`, because `aoide.hyprland.*`
# stays a host-facing option).
#
# Adding an alternative is one file plus one line here; removing it is
# deleting both, with no other edit anywhere in the tree.
{
  providers = {
    hyprland = ./hyprland;
  };
}
