# pkgs/lyra-shell — the shell source

The QML tree, the resolved icon assets and the preview fixture sets that a
lyra shell is generated FROM. This package ships the source; it does not ship
a shell. Nothing here runs, and no config is named here.

```
pkgs/lyra-shell/              →   ${pkgs.lyra-shell}/share/lyra/
  qml/        29 .qml + slots.md      qml/
  icons/      catalog.json, selection.json, iconoir/, LICENSE-*   icons/
  preview/fixtures/{one,many,…}/      preview/fixtures/…
```

One copy of the tree, two consumers:

| consumer | what it takes | how |
|---|---|---|
| the lyra lane's build (today's consumer) | `qml/` | `cp -r ${pkgs.lyra-shell}/share/lyra/qml/. "$out/qml/"`, then the manifest/registry/surfaces and the songs' widgets land beside it |
| `lyra preview` (`pkgs/aoide/crates/lyra/src/commands/preview.rs`) | `qml/`, `icons/`, `preview/fixtures/` | joins `aoide_storage::fs::LYRA_SHELL_SRC` onto the checkout (`$AOIDE_FLAKE_ROOT`, default `~/.aoide/Aoide`) — a preview root is copies of the checkout's own files, never symlinks into it |

The Rust side reads the CHECKOUT, not this store path: `lyra preview` runs on
a host that has the checkout, and its whole point is that the designer edits
the checkout's file. The one string both sides agree on is
`aoide_storage::fs::LYRA_SHELL_SRC` (`"pkgs/lyra-shell"`), which is also the
directory this package is built from — rename the directory and both follow.

## What this package is not

- **Not a shell.** `lyra` is the binary; a shell is a directory of QML plus
  whatever a song contributes. Which directory runs is `aoide.quickshell.config`
  (a nucleus fact), set by the lane that brings a session up.
- **Not a place to add QML for a host.** A host's own paint lands in its song;
  a capability that needs to be reachable from a terminal lands as a bridge
  first (root `AGENTS.md` house rule 7 — delete every `.qml`, is it still
  reachable?).
- **Not named `lyra`.** nixpkgs already carries an unrelated `lyra`, and
  `packages.lyra` is the core flake's rice output
  (`docs/architecture/PACKAGE-LAYOUT.md`, "Two binaries"). `lyra-shell` is the
  shell source; that name means nothing else.

Consumed as `pkgs.lyra-shell` through `lib/pkgs.nix`'s overlay, and published
as `packages.<system>.lyra-shell` by the same walk — there is no second build
and no hand-list.
