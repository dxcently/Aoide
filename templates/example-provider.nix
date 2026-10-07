# example-provider.nix — one implementation of a multi-implementation capability.
#
# Copy to:  modules/dendrites/<capability>/<provider>.nix
# Then:     add `<provider> = ./<provider>.nix;` to that directory's
#           default.nix (example-default-provider-registry.nix). Nothing goes in
#           the catalogue: the catalogue names the capability, the registry
#           names its providers.
# Replace:  <provider> and the settings.
#
# A provider file has exactly the shape of a dendrite: a plain module, whose
# `habit.home` is its home half. Providers of one capability may differ in what
# they set — one that is home-only simply writes no system settings, and one
# that is system-only writes no `habit.home`.
#
# A provider is exclusive within a scope: one implementation answers for the
# host, one answers for each user. Two aggregations that want the same
# capability on the same terms merge into one selection; two that name different
# providers for it collide on `habit.dendrites.<capability>.provider` with both
# values in the error.
{
  habit.home =
    { pkgs, ... }:
    {
      services.mako = {
        enable = true;
        settings.default-timeout = 5000;
      };
      home.packages = [ pkgs.libnotify ];
    };
}
