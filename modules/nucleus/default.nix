# modules/nucleus/default.nix — the contract every host carries.
#
# What the dendrites and every consumer build against, and nothing that does
# work: the option tree and the platform's assertions. Services, packages and
# settings are members of the `nucleus` aggregation instead
# (`modules/aggregations/nucleus`), which a host can deselect from.
#
# One line per file, in directory order. A new file here declares an option or
# an invariant; anything that configures the platform is a dendrite.
{
  imports = [
    ./assertions.nix
    ./options.nix
  ];
}
