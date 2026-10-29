# pkgs/aoide/module

The core's NixOS deployment bundle, exported as `nixosModules.default`
from `pkgs/aoide/flake.nix`. Lives inside the core flake (not the root
flake) so a consumer outside this repo gets the option contract and the
package build from the same one-line input.

## Named seams

- `default.nix` — composes the bundle: imports the siblings below and
  applies `overlays.default` (the same export `packages.<sys>.aoide`
  names), so a consumer that imports this module needs no separate
  overlay line of their own.
- `options.nix` — THE core `aoide.*` option contract: `enable`, `root`,
  `checkout`, `auditLog`, `terminal`, `user`, `sessionTarget`. Every
  unit (`modules/dendrites/{aoided,aoide-secrets,aoide-config}.nix`,
  `modules/dendrites/strillix/shellbridge.nix`) reads these options; this file only
  declares them.
- `aoided.nix` — the `aoided` systemd user service itself: the tmpfiles
  rules for the core state tree, its `RuntimeDirectory` (`$XDG_RUNTIME_DIR/aoide/`,
  which holds the control socket and the core shellbridge socket), and the core
  session variables (`AOIDE_TERMINAL`, `AOIDE_ROOT`, `AOIDE_FLAKE_ROOT`), portable
  and nixpkgs-only. The unit's `wantedBy`/`after` anchor to
  `aoide.sessionTarget` — the seam a paint-dependent value enters
  through, since this file may not read a lane's option directly. The lane
  that brings a graphical session up sets that option (the lane that ships
  the session's shell today), so this bundle never learns which lane it was —
  and that anchor decides only when the unit STARTS: there is no `partOf`, so
  a desktop that dies takes the paint down and leaves the daemon, its audit
  log, its gate and its doors running.
  `modules/dendrites/aoided.nix` carries every door (mcp/a2a/pair-watch), the
  discovery firewall carve, and the usage poller, all deployment on top of this
  unit. The strillix dendrite writes nothing onto it: the installed strillix
  defaults `STRILLIX_TEMPLATES` to its shipped rices itself.
- `reap.nix` — the liveness reaper: the `aoide-graph-reap` oneshot service
  (`aoide session reap`) and its timer (about every 12 s). It is anchored to
  `aoide.sessionTarget` the way `aoided` is, and carries no compositor: a window
  that closed reaches it through the door (`session window closed`), not through
  a Hyprland read.

## How a consumer imports it

```nix
{
  imports = [ inputs.aoide.nixosModules.default ];
}
```

One line pulls in the option contract, the `aoided` unit, and the
overlay together — no separate `nixpkgs.overlays` entry, no second
`aoide.*` declaration to keep in sync. `modules/nucleus/options.nix` is this repo's own consumer. A standalone Home
Manager user imports the same file through `homeModules.nucleus`, which reads it
by input path (`${inputs.aoide}/module/options.nix`), not through a flake output:
`module/` is outside the aoide package's source, so the package does not move.

A stranger outside this repo pins the same five lines — no `modules/`
import, no `inputs.quickshell`/`hyprland`/`nvf`:

```nix
inputs.aoide.url = "path:/path/to/pkgs/aoide";
inputs.aoide.inputs.nixpkgs.follows = "nixpkgs";
modules = [ aoide.nixosModules.default ];
aoide.enable = true;
aoide.user = "stranger";
```

## This directory is outside the package `src`

`pkgs/aoide/default.nix`'s `src` filter drops the top-level `module/`
directory, so a file added or edited here never moves the `aoide`
derivation's store path or reruns its cargo test phase — the package
is the crate tree; this module ships beside it.
