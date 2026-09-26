# modules/overrides

Capability-scoped fixes. An override record is where a repair lives when it
belongs to a CAPABILITY rather than to a host — a package whose upstream build
broke, a setting every machine that runs the thing wants — so the fix travels
with the capability instead of being copied into each host file.

## What it is

`modules/overrides/default.nix` discovers every `*.nix` file beside it; the file
name (minus `.nix`) is the record's name, and it is the only place the file is
named. A record says which dendrites it is about, optionally which hosts it is
confined to, and carries one or more of `overlay`, `nixos`, `homeManager`:

```nix
{
  dendrites = [ "lyra" ];       # required: the capabilities this fix is about
  hosts = [ "yomi-strix" ];     # optional: confine it; absent = every host
  overlay = _final: _prev: { … };   # at least one carrier is required
}
```

It does not SELECT anything. A record targeting a capability nobody chose simply
never applies.

## The seams

- **Matching** — the constructor applies a record to the hosts that (a) its host
  filter admits and (b) selected a dendrite it targets, for the system OR by one
  of its users (with `useGlobalPkgs` a home lane draws from the host's own
  package set, so there is no separate home one to fix). `homeManager` rides a
  USER's lane only when that user's own home selection hits a target.
- **Order** — record name, so the list does not depend on the filesystem.
  Overlays then compose the ordinary Nix way, each seeing the previous one as
  `prev`.
- **Precedence** — a record outranks everything the constructor imported on its
  behalf; the host's own module still outranks the record.
- **The boundary is weaker than selection's, and this is the honest statement of
  it:** every host READS every record, because matching means reading what it
  targets. What stays unevaluated is the work — `overlay` and the lane modules
  are functions, and an unmatched record's functions are never called. Keep
  imports and package computation inside those functions; metadata that computes
  defeats the boundary.

Typos fail loudly rather than applying to nothing: an unknown field, a target
that is not a catalogue name, a host name no record holds, or a record carrying
nothing at all is an error naming the record and its file.

## Today

The directory is EMPTY of records — no Aoide dendrite patches another one yet —
so every host resolves to no overlay, no module and no match. `tests/selection`
drives the whole matching contract against a fixture set; `tests/templates`
exercises this discovery file itself.
