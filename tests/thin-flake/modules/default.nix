# tests/thin-flake/modules/default.nix — the fixture's registry: plain data, shaped like
# Strillix's own catalogue. A `{ byClass = …; }` entry (a line, or an inline provider
# target) is rendered by mkFlake (lib/by-class.nix); a path is imported only when a host
# selects it. `deep` is a provider-set FILE: a marker inside it is deliberately not
# rendered (deep/default.nix says so).
{
  catalogue = {
    both = {
      byClass = ./dendrites/both.nix;
    };
    inline = {
      byClass = {
        homeManager = ./dendrites/inline-home.nix;
      };
    };
    sysonly = {
      byClass = {
        nixos = ./dendrites/sysonly.nix;
      };
    };
    # Never selected by any host, so never imported: its file throws if read, and the
    # thin host evaluating at all is the proof that it was not read.
    never = {
      byClass = ./dendrites/never.nix;
    };
    # Selected only by the `bad` host: a plain module where a byClass record belongs.
    bad = {
      byClass = ./dendrites/bad.nix;
    };
    picked = {
      providers = {
        one = {
          byClass = {
            nixos = ./dendrites/picked-one.nix;
          };
        };
      };
    };
    deep = ./dendrites/deep;
    trap = {
      providers = import ./trap.nix;
    };
    # Selected only by the `not-set` host: a provider set whose `providers` is a path, not a set
    # of targets. habit refuses it by its own message, so the renderer must pass it on unrendered.
    notSet = {
      providers = ./notset.nix;
    };
    # Inline records no host can take as they are, each selected by a host of its own that
    # exists to be refused: an empty record, a record that is not an attrset, and a record
    # with a `darwin` key, which no record takes.
    emptyByClass = {
      byClass = { };
    };
    intByClass = {
      byClass = 1;
    };
    darwinByClass = {
      byClass = {
        darwin = { };
      };
    };
  };
}
