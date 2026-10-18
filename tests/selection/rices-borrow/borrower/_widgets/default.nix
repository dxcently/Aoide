# tests/selection/rices-borrow/borrower/_widgets/default.nix — the composition
# itself, borrowed by NAME: every slot this rice dresses is the lender's body.
#
# `borrower` has no `widgets/` directory of its own, which is the point — its
# manifest's records all carry `owner = "lender"`, and the built-in set has to
# carry the lender for those records to resolve on a host that performs this rice.
{ borrow, ... }: borrow "lender"
