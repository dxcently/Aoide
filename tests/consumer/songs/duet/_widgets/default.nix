# tests/consumer/songs/duet/_widgets/default.nix — the borrowing song's shelf,
# which authors nothing of its own: the documented cross-song idiom, applied by
# a CONSUMER's song rather than by Aoide's.
#
# `borrow "solo"` hands back solo's already-resolved `slot -> record` map (owner
# bound by solo's own roll-up), so duet's manifest is solo's, and the built-in
# closure pulls solo in as the lender. duet has no `widgets/` directory: a
# borrowed record's body lives under its OWNER's `widgets/`, which is exactly
# what the songbook's owner-relative existence check resolves.
{ borrow, ... }:
borrow "solo"
