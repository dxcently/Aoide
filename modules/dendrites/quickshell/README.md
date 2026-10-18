# modules/dendrites/quickshell

The shell lane: the Quickshell package and the one `aoide-quickshell` user
service, started on whatever directory `aoide.quickshell.config` names. It
renders no QML, deploys nothing and knows no rice — bring your own QML, or let
a lane (`strillix`) name its deployed tree.

```
modules/dendrites/quickshell/
├── default.nix   the catalogue entry:
│                 { imports = [ ./system.nix ]; habit.home.imports = [ ./home.nix ]; }
├── system.nix    the package, the session anchor, the `aoide.quickshell.enable`
│                 fact, and the home half published as `aoide.quickshell.home`
└── home.nix      imports `osConfig.aoide.quickshell.home` into each user's home:
                  the `aoide-quickshell` unit
```

## Named seams

- **`aoide.quickshell.config`** (read): the directory the shell runs.
  `null` is the bare shape — package installed, no service, no session anchor.
  A host sets it to its own config directory, or `strillix` sets it to
  `$AOIDE_ROOT/run/qml`.
- **`aoide.quickshell.enable`** (set): the fact that a shell surface exists.
- **`aoide.sessionTarget`** (set when a config is named):
  `graphical-session.target`, so `aoided` comes up with the session.
- **`aoide.quickshell.home`** (internal): the home half as a module of the
  system config, so it closes over `aoide.quickshell.config`, `aoide.root`
  and `aoide.theme.wallpaper`.

## How it composes

Selecting `quickshell` imports both halves. `strillix`'s runtime imports the same
two paths (`../quickshell/system.nix`, `../quickshell/home.nix`), so a host
with both lanes has one module of each and one `aoide-quickshell` unit. The
seam with `strillix` and the two strillix-less shapes are charted in
`../README.md` ("The shell lane").
