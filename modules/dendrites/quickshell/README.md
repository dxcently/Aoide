# modules/dendrites/quickshell

The shell lane: the Quickshell package and the one `aoide-quickshell` user
service, started on whatever directory `strillix.quickshell.config` names. It
renders no QML, deploys nothing and knows no rice — bring your own QML, or let
a lane (`strillix`) name its deployed tree.

```
modules/dendrites/quickshell/
├── default.nix   the catalogue entry:
│                 { imports = [ ./system.nix ]; habit.home.imports = [ ./home.nix ]; }
├── system.nix    the package on NixOS, the session anchor, the
│                 `strillix.quickshell.enable` fact, `strillix.quickshell.nixos`
└── home.nix      the `strillix.quickshell.enable` fact at the home level, the
                  package on a standalone home, and the `aoide-quickshell` unit
```

## Named seams

- **`strillix.quickshell.config`** (read): the directory the shell runs.
  `null` is the bare shape — package installed, no service, no session anchor.
  A host sets it to its own config directory, or `strillix` sets it to
  `$AOIDE_ROOT/run/qml`.
- **`strillix.quickshell.enable`** (set): the fact that a shell surface exists.
- **`aoide.sessionTarget`** (set when a config is named):
  `graphical-session.target`, so `aoided` comes up with the session.
- **`strillix.quickshell.nixos`** (internal): set by `system.nix`; `home.nix`
  reads it to install the package only where no NixOS system half already did.

## How it composes

Selecting `quickshell` imports both halves. `strillix`'s runtime imports the same
two paths (`../quickshell/system.nix`, `../quickshell/home.nix`), so a host
with both lanes has one module of each and one `aoide-quickshell` unit. A
standalone home has no system half: `home.nix` alone installs the package and
runs the unit. The
seam with `strillix` and the two strillix-less shapes are charted in
`../README.md` ("The shell lane").
