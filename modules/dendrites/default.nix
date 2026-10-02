# modules/dendrites/default.nix — the whole-tree aggregate.
#
# Its imports are DERIVED from `modules/default.nix`'s catalogue: that record is
# the one place a dendrite file is named (root `AGENTS.md` house rule 7), so
# this file names no dendrite at all. Adding one is one catalogue line and
# nothing here changes; deleting the line shelves it, file untouched.
#
# A dendrite file is a lane record (`{ body; nixos; }`, CONTRACTS.md §2).
# Taking this aggregate merges every `body`, so a host that takes the whole tree
# still sees each `aoide.<name>.*` option and its own guard; a host the
# constructor (habit's composition) assembles imports only the `nixos` lane of
# what it selected. The two are mutually exclusive in one module list — an
# aggregate `body` and a selected lane's own `body` are the same declarations
# twice, which nixpkgs throws on rather than merging.
#
# This aggregate is on its way out: it is kept alive by the two readers that
# still want a whole tree without a host — `lib/options.nix`'s `aoideOptions`
# derivation (every body's declarations, one bare eval) and `flake.nix`'s
# `fmt`/`nix-lint` scope, which walk the committed tree rather than import it.
# No host takes it any more: hosts select through the catalogue and the
# constructor imports only what they selected (S7). It goes when those two
# readers stop needing it — not before (S10's export surface is where that
# lands, if it does).
let
  # A catalogue entry is either a lane record or a provider registry
  # (`{ providers.<p> = <path>; }`) — a capability with alternatives, each
  # provider a lane record of its own. Taking the whole tree means taking every
  # alternative: the constructor is where exactly one is chosen, and there is no
  # default provider to guess. Each provider body self-gates on its own fact, so
  # merging them all changes nothing a host did not already enable.
  bodiesOf =
    path:
    let
      entry = import path;
    in
    if entry ? body then
      [ entry.body ]
    else
      map (p: (import p).body) (builtins.attrValues entry.providers);
in
{
  imports = builtins.concatMap bodiesOf (builtins.attrValues (import ../default.nix).catalogue);
}
