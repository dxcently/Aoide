# A provider-set FILE holding a raw `{ byClass = …; }` marker. mkFlake does not render a
# marker inside a file (rendering would import this file for every host), so selecting
# provider `one` from here fails: habit reads the marker as a module with an unknown
# `byClass` option. A provider whose target is a marker has to be written inline on the
# registry line.
{
  providers.one = {
    byClass = {
      nixos = ./one.nix;
    };
  };
}
