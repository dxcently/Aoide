# AGENTS.md — invariants for `modules/aggregations/`

Points up to `modules/AGENTS.md` (catalogue, dendrite shape, flat option style) and
root `AGENTS.md` (house rules 1, 5, 7). This file holds only what is local to a
grouping body.

## A body is data, and only data

An aggregation body declares **no options** and carries **no gate**. It has no
`config`, no `lib.mkIf`, no way to enable another aggregation: its whole effect
is the membership the constructor reads. `description` at the top, and `members`, `providers` and `module` in each of
`system` and `home`, are the keys the constructor understands; anything else is
refused by name, which is why nothing else belongs here.

## Members are catalogue names, and only catalogue names

`members = [ "bash" "git" ]` — names from `modules/default.nix`, never a path and
never a file. A grouping change therefore never moves a dendrite, and a member
that has no catalogue line is unreachable (house rule 7's shelving).

Two bodies MAY name the same member: membership merges (`mkDefault`), and two
groups choosing different providers for one member collide on
`dendrites.<member>.provider` — an error naming the member, never import order
picking a winner. What a body may not do is reach for another group's story:
`desktop` and `aoideos` are deliberately disjoint (the session's platform vs the
thing that paints it), and the host writes how they compose.

## The guard is the host's, not yours

Membership is `mkDefault`, and so is every provider default. A host always wins:
`dendrites.<name>.enable = false` drops a member, and
`aggregation.<group>.<member>.provider = "…"` overrides a group's choice. Never
raise a body's own settings out of `mkDefault` — that takes the decision away
from the machine that has to live with it.

## What needs a docs update in the same commit

- This directory's `README.md` when a body's charter changes, a member moves
  between bodies, or a new key in the body shape is honoured by the constructor.
- `modules/README.md` / `modules/AGENTS.md` when the registry's shape moves
  (the `aggregations = import ./aggregations;` line, or the discovery's depth).
- `modules/default.nix` — one catalogue line per new member. A body and its
  catalogue line are the two halves of the same addition.
