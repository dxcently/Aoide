# modules/aggregations

Memberships. An aggregation is a named group of catalogue entries a host
selects with ONE line — `aggregation.base.enable = true;` — plus whatever that
group has to say about the platform. It names no file: a member is a catalogue
name, so a grouping change never moves a dendrite.

## What it is

`modules/aggregations/default.nix` discovers every immediate child directory
holding a `default.nix`; the directory's name is the aggregation's name. A body
is inert DATA — it declares no options and carries no gate, and
habit's composition wraps it in one, and refuses a body with a top-level key
other than `description`, `system` or `home`, or a half with a key other than
`members`, `providers`, `nixos` or `homeManager`. Members are applied with `mkDefault`, so
a host can take a group and drop one member
(`dendrites.kitty.enable = false`) without giving up the rest.

```nix
{
  description = "…";              # what the group is for; a host reads it nowhere
  system = {                      # the system scope; `home` is the home scope
    members = [ "bash" "git" ];   # catalogue names, nothing else
    providers.compositor = "hyprland";  # the group's default provider; a host may override
    nixos = { lib, ... }: { … };  # a module, deferred to the platform pass
  };
}
```

The four bodies today are `base` (the command line every host carries,
including `aoide.enable` and the stateVersion the deleted `hosts/common` used to
hold), `desktop` (a graphical session's platform: audio, clipboard,
notifications, screenshots, network, a browser), `agents` (the coding agents a
workstation runs) and `aoideos` (the desktop itself: compositor, greeter,
hyprland, quickshell, lyra, songbook, stylix).

## The seams

- **Discovery** — the directory name, one level deep, no body imported. The
  constructor imports only the bodies a host or one of its users selected, so an
  unselected group is never read. `tests/selection` proves that with a body
  that throws on import.
- **Membership** — `members` is catalogue names. A member the catalogue does not
  hold is an unknown option, named with the file that asked for it.
- **Provider defaults** — `providers.<member> = "<provider>"` gives the group's
  default, which the host overrides at `aggregation.<group>.<member>.provider`.
- **The platform half** — `nixos` / `homeManager`, a module or a module
  function, evaluated only when the group is selected. This is where a group's
  own settings live; a group that has none omits the key (`agents`).

## The home scope

`home` is declared and read by the constructor, but every Aoide lane is currently
system-scope: each body writes Home Manager from NixOS. A home grouping arrives
when a real home lane does — the shape is here, the bodies are not invented
ahead of it.
