# users

The people. One definition per person, attached by every machine that carries
them — `users.khoa.definition = ../../users/khoa.nix;` — never copied per host.
There is no per-machine copy of a person, so a change to an account or to a home
floor lands once.

## The definition

A user definition is two halves and nothing else:

```nix
{
  nixos = { lib, ... }: { users.users.khoa = { … }; };  # creates the account
  homeManager = { home.stateVersion = "25.11"; };       # the home floor
}
```

- **`nixos` is required.** A user with no `nixos` lane cannot be created, and the
  constructor says so by name rather than producing a host without them.
- **`homeManager` is optional.** It is the floor every machine's Home Manager
  lane starts from; the capabilities that user selected are imported ALONGSIDE
  it, and that machine's own `users.<name>.homeManager.config` lands on top.
- **`homeManager.enable = false` on the host side** creates the account and
  imports no Home Manager module at all. Asking for a home capability with the
  lane off is a configuration error, not a quiet no-op.

## What lives where

| Belongs to the person | Belongs to the machine |
|---|---|
| the account (`isNormalUser`, groups, shell) | `homeManager.enable` |
| the home floor (`home.stateVersion`, shared programs) | extra groups that host needs |
| anything true on EVERY machine they use | `users.<name>.homeManager.config` — this host only |

The split is the same one `hosts/README.md` draws: the definition is shared, the
attachment is per machine. A second copy of a person is the bug this directory
exists to prevent.

## Today

`khoa.nix` carries the account and Home Manager's compat pin — the two things
the deleted `hosts/common` held for every Aoide box.
