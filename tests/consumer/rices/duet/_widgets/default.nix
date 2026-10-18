# tests/consumer/rices/duet/_widgets/default.nix — the borrowing rice's shelf,
# which authors nothing of its own: the documented cross-rice idiom, applied by
# a CONSUMER's rice rather than by Aoide's.
#
# `borrow "solo"` hands back solo's already-resolved `slot -> record` map (owner
# bound by solo's own roll-up), so duet's manifest is solo's, and the built-in
# closure pulls solo in as the lender. duet has no `widgets/` directory: a
# borrowed record's body lives under its OWNER's `widgets/`, which is exactly
# what the rices's owner-relative existence check resolves.
{ borrow, ... }:
borrow "solo"
