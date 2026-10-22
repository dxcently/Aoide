# Changelog

## Unreleased

### Split: Aoide is the engine, Strillix is the OS

- Paint options leave `aoide.*` under the same names: `aoide.strillix.{enable,root,home}`,
  `aoide.hostConfig`, `aoide.rice`, `aoide.rices.{builtIn,greeter}`, `aoide.theme.*`,
  `aoide.layout.*`, `aoide.surfaces`, `aoide.components.*`, `aoide.wallpaper.provider`,
  `aoide.quickshell.*` and `aoide.stylix.enable` become `strillix.*`. A hard rename with no alias: a host that sets an old name fails
  eval with an unknown option until it is renamed. `aoide.enable` no longer
  turns Strillix on; `strillix.enable` does, and Strillix runs with `aoide.enable` off.
- Two sockets. `aoided` serves `$XDG_RUNTIME_DIR/aoide/shellbridge.sock` for core's
  verbs. `strillix shellbridge --run` serves `$XDG_RUNTIME_DIR/strillix/bridge.sock`
  for the desktop's verbs. `ShellBridge.qml` routes each cmd to one socket.
- The rice event class is gone from the audit log. Rice operations are logged in
  `$STRILLIX_ROOT/log`, not `$AOIDE_ROOT/log`: a tool that reads only the core log
  no longer sees them.
- `aoide rice ...` is an unknown command. The rice commands are `strx rice ...`,
  and advice strings name `strillix rice ...`.
- dxflake's host lines that set the old names break until dxflake renames them in
  its own lane, so dxflake pins a commit from before this change until then.

### Rename: strillix, and the rice runtime leaves `$AOIDE_ROOT`

- The paint binary is `strillix`. `strx` is a symlink to it in the same package.
- Songs are rices (`rices/`), livery is theme, arrangement is layout, cover is
  wallpaper, takes are history, herald is notifications. `aoide herald push`
  is `aoide notifications push`.
- The rice runtime moves from `$AOIDE_ROOT/song/` to `$STRILLIX_ROOT` (default
  `~/.strillix`). `~/.local/state/lyra` becomes `~/.local/state/strillix`.
- habit options are `habit.rice.declared` and `habit.rice.available`, a hard
  rename with no alias. `strx rice declare --host` rewrites the host line.
- Migration: a rebuild moves the old rice state (the activation's
  `aoideMigrateRoots`). By hand, run `strx migrate --check`, then `strx migrate`.
  Until the state settles, `strillix` refuses every verb except `migrate`,
  help and version. Rice state held by both roots, or across homes, is
  refused: nothing moves, and the verb exits nonzero.

### A host's theme value outranks only that value

- A host's plain write to one `strillix.theme` value now outranks only that
  value. Before, `strillix.theme` was one submodule option, so a host write to
  any single value (`strillix.theme.geometry.terminalOpacity`, say) dropped the
  performed rice's whole theme back to the option defaults.
- A whole-tier write such as `strillix.theme.palette = lib.mkForce { … }` now
  sets only the values it names. `strillix onboard`'s generated `aoide.nix`
  lists the tiers' values but no longer the seven tier entries
  (`strillix.theme`, `.palette`, `.bar`, `.notif`, `.window`, `.geometry`,
  `.fonts`).
- `base16` and `fonts.monospace` stay whole units (a host value replaces the
  rice's whole scheme or face), and `override` stays the host's own tier
  (CONTRACTS.md §5).
