# hosts

The machines. A host is ONE module: the `habit.*` keys are what this machine
selects, and everything else in it is this machine's own platform settings. `hosts/<name>/default.nix` names no module file — nothing
outside `modules/default.nix` does — and carries a `hardware.nix` beside it when
a scan has been committed.

## Discovery, not registration

`flake.nix` reads this directory one level deep: every immediate child holding a
`default.nix`, minus the `_`-prefixed shelved ones. Adding a machine is a new
directory and nothing else — no list in `flake.nix`, no line in `lib/aoideos.nix`.
A name with no directory is unreachable, which is what shelving means here.

`_desktop`, `_laptop` and `_server` are templates to copy (`cp -r hosts/_desktop
hosts/<name>`); `_mac` is the darwin-shaped skeleton, shelved until there is a
darwin constructor to answer it.

## The module

```nix
{ pkgs, ... }:
{
  habit.aggregation.base.enable = true;      # groups, one line each
  habit.dendrites.obsidian.enable = true;    # lone capabilities
  habit.users.khoa = {
    definition = ../../users/khoa.nix;       # shared, never copied
    home.enable = true;
  };
  networking.hostName = "…";                 # this machine
}
```

- **Selection first.** `habit.aggregation.*`, `habit.dendrites.*` and each user's own
  selection are resolved by habit's composition in an ordinary `evalModules`
  pass that knows nothing about NixOS; the platform import list is assembled from
  the result. A capability the host did not select is never imported.
- **Everything outside `habit` is read by the platform alone**, and nothing in
  it can influence selection — the selection pass applies the module with
  `config` and `pkgs` poisoned and reads only the literal `habit.*` keys, which
  is what keeps the two passes from chasing each other. It carries the platform
  (`nixpkgs.hostPlatform`: the constructor assembles the module list and lets the
  modules name the platform, rather than handing `nixosSystem` a `system`), the
  hardware import, the `aoide.*` knobs that genuinely vary per machine, and any
  host-only overlay.
- **A group's member is turned off the ordinary way** —
  `habit.dendrites.kitty.enable = false` outranks the group's `mkDefault`.
- **The song** is named here (`habit.song.declared = "sonata";`), and the songs
  a host may switch to are `habit.song.available`. Both are keys of the
  `habit` submodule, which `lib/aoideos.nix` adds through `selectionModules`
  (`lib/songbook.nix`), so selection resolves them with everything else.

## What a host may read

Root `AGENTS.md` house rule 5's closed list, and nothing else. A host is a
consumer of the catalogue: it names capabilities it selected and sets the options
those capabilities declare. It never imports a module file, never reaches into
another module, and never patches a dendrite — that is what an override record
(`modules/overrides/`) is for.

## The review surface

`nix eval --json .#inventory.<host>` answers what that host actually resolved:
its aggregations, every dendrite it selected with the provider answering it and
the file that answered, its users, and which override records matched. Derived
from selection in habit's composition, never maintained by hand, so it cannot
drift from what `nixosConfigurations.<host>` builds.
