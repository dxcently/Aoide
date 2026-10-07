# users

The people. One definition per person, attached by every machine that carries
them — `habit.users.khoa.definition = ../../users/khoa.nix;` — never copied per host.
There is no per-machine copy of a person, so a change to an account or to a home
floor lands once.

## The definition

A user definition is a plain module, wrapped like a dendrite, with two halves:

```nix
{ lib, ... }:
{
  users.users.khoa = { … };                              # creates the account
  habit.home = { home.stateVersion = "25.11"; };         # the home floor
}
```

- **The module's own settings are the account.** They apply to every host that
  attaches the user, Home Manager or not.
- **`habit.home` is optional.** It is the floor every machine's Home Manager
  starts from for this person; the capabilities that user selected are imported
  ALONGSIDE it, and that machine's own `habit.users.<name>.home.config` lands on
  top.
- **`home.enable = false` on the host side** creates the account and imports no
  Home Manager module at all. Asking for a home capability with Home Manager off
  is a configuration error, not a quiet no-op.

## What lives where

| Belongs to the person | Belongs to the machine |
|---|---|
| the account (`isNormalUser`, groups, shell) | `home.enable` |
| the home floor (`home.stateVersion`, shared programs) | extra groups that host needs |
| anything true on EVERY machine they use | `habit.users.<name>.home.config` — this host only |

The split is the same one `hosts/README.md` draws: the definition is shared, the
attachment is per machine. A second copy of a person is the bug this directory
exists to prevent.

## Today

`khoa.nix` carries the account and Home Manager's compat pin — the two things
the deleted `hosts/common` held for every Aoide box.
