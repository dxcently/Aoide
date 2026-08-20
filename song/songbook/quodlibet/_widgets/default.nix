# song/songbook/quodlibet/_widgets/default.nix — the composition, not a shelf.
#
# Every other song's `_widgets/default.nix` is a `readDir` roll-up over
# sibling `<slot>.nix` files that this song owns (see
# `song/songbook/sonata/_widgets/default.nix`'s header). quodlibet owns none:
# this file is the documented borrow idiom from `lib/song.nix`'s header,
# applied verbatim — plain attrset update over two PARENTS' already-resolved
# widget maps, never `mkWidget` called directly (that would hand-type another
# song's `owner` inside a third song's file, the one thing the roll-up design
# exists to prevent — see the prerequisite plan's rejected alternatives).
#
# `sonataWidgets` and `fugueWidgets` are each a fully resolved `slot -> record`
# attrset (owner already bound by their OWN roll-ups). `//` picks a winner per
# key: both parents author `bar` and `herald`, so the right-hand side
# (fugue's) wins those two, and every other key falls through from sonata
# unchanged, `owner` field and all. quodlibet therefore contributes zero
# widget records of its own — this file never types `owner = "quodlibet"`
# anywhere, which is the point.
{ lib }:
let
  sonataWidgets = import ../../sonata/_widgets { inherit lib; };
  fugueWidgets = import ../../fugue/_widgets { inherit lib; };
in
sonataWidgets // { inherit (fugueWidgets) bar herald; }
