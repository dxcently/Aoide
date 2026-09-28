# modules/dendrites/wallpaper/default.nix — the wallpaper capability's
# provider registry (CONTRACTS.md §2 v1).
#
# A capability with alternatives is a registry of provider NAMES to provider
# FILES and nothing else: the constructor (`lib/composition.nix`) reads this
# record before any module graph exists and imports exactly the provider the
# selecting host asked for, so an unselected alternative is never read. The
# catalogue names THIS directory (`wallpaper = ./dendrites/wallpaper;`) and
# nothing names a provider file but this line — one place each.
#
# WHO PAINTS A SONG'S WALLPAPER is a host's choice, the way its compositor is.
# `quickshell` is the default: the shell's own `aoide-wallpaper` layer draws the
# song's default AND the user's picks. `skwd-wall` hands picks to the external
# engine of that name, which paints them on its own surface while the shell's
# layer stays mapped and paints nothing. Both providers name themselves in the
# fact `aoide.wallpaper.provider`, which the runtime reads (CONTRACTS.md §4).
#
# Adding an alternative is one file plus one line here; removing it is
# deleting both, with no other edit anywhere in the tree.
{
  providers = {
    quickshell = ./quickshell.nix;
    skwd-wall = ./skwd-wall.nix;
  };
}
