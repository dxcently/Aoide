# Changelog

## Unreleased

### Staging an edited rice, or one not built in, works again

- `strillix rice stage`, `strillix rice back` and `strillix reload` stage a built-in rice whose machine copy differs from the shipped one, and a rice that is not built in when this system has the packages it needs. Otherwise the stage is refused and asks for a rebuild. `rice back` goes through the same gate. Before, the runtime named the shipped generator's argument `rices` where the generator takes `ricesRoot`; nix refused the call, so the stage was refused, and a login or a switch left the bar on the widgets the last activation deployed.
- The `generator-runtime` flake check stages an edited copy of barcarolle with the built `strillix` and the real `nix-instantiate`.

### Thin flakes: `lib.mkFlake`

- A flake builds its machines with `aoide.lib.mkFlake { root = ./.; }`: its own `hosts/`, `modules/default.nix` and `rices/` are read by convention and come back as `nixosConfigurations` and `inventory`, so a consumer's flake needs no host loop of its own.
- `lib/strillix.nix` takes `{ inputs, lib }` and returns `mkFlake`, `ricesRoot`, `nucleusModule`, `homeNucleus` and `standaloneNucleus`. `mkHost`, `hostNames` and the file's `system` and `username` arguments are gone: a caller that imports the file by path passes `system` to `mkFlake`, and `username` through `mkFlake`'s `specialArgs`.
- A host that `mkFlake` builds from a tree other than Strillix's own sets `strillix.hostConfig` itself. It has no default there, and reading it unset fails by name.
- A catalogue line, or the target of an inline provider set, may be `{ byClass = X; }`. X is a NixOS module and a Home Manager module written together, `{ nixos = …; homeManager = …; }`, with at least one of the two and no other key. `mkFlake` renders the marker in the registry before habit reads it, and a flake without habit imports `X.nixos` or `X.homeManager` directly.

### Home half reads the host's own rices book

- The home half reads the rices directory its system half reads: the host's own book when it has one, and Strillix's otherwise. Before, the home half always read Strillix's book, so a host's own rices reached its system half and not its home.

### barcarolle's calendar opens above the clock

- The calendar sat near the top edge of every screen and ran off its right edge, because the clock stored its cell's corner in the bar window's coordinates, not the screen's, and the bar sits at the bottom. Opened from the clock, it now sits above it, its right edge on the clock's right edge and its bottom flush on the bar.

### Battery in the notification area

- barcarolle's notification area shows a battery cell on a laptop, between the Action Center flag and the network icon: the charge as a glyph, urgent orange below 15%, with a bolt while charging. A machine with no battery gets no cell, and the other icons do not move. The cell is not a button: no hover box, and a click does nothing.

### Start menu power rows act directly

- barcarolle's Start menu power rows (Log off, Restart, Shut down) send their power action at once, like Sleep and Hibernate; the separate Shut Down dialog is no longer opened from the Start menu, and the `Shut down…` ellipsis is gone.

### Login screen shows the desktop picture

- The greeter's login screen falls back to the rice's desktop picture (`strillix.theme.wallpaper`) when the face ships no logon picture. barcarolle's `barcarolle-logon.jpg`, with its painted person tile, is removed; the login screen shows `barcarolle-desktop.jpg`.
- `tests/consumer` checks that barcarolle's login-screen tree names the desktop picture and holds no logon picture.

### Bar popups do not blur their transparent regions

- The compositor sets `ignore_alpha 0.05` on the `aoide-bar` layer beside its `blur_popups` rule, so a popup's transparent area no longer renders as a dark blurred block.

### The bridge socket loop lives in aoide-protocol

- The shellbridge socket loop (the accept, the cross-uid floor, the line routing and the drop) moves from `aoide-conduct` to `aoide-protocol::shellbridge`, and peer credentials move to `aoide-protocol::peercred` (one copy, shared with `aoide-secrets`). Core's commands stay in conduct as the core socket's `front` hook. Wire behaviour, reply bytes and audit records are unchanged.

### Desktop socket drops a misrouted sessiontrace

- The desktop socket (`strillix shellbridge --run`) no longer answers a malformed `sessiontrace` line: it is audited `unparseable` and dropped like any line no verb serves. Only aoided's socket answers one, with the `bad-request` refusal.

### Greeter output

- `strillix.greeter.output` picks the monitor the Quickshell login screen shows on (e.g. `"DP-1"`); the other monitors are turned off while it runs. If that monitor is missing or disabled, nothing is turned off and the login screen spans every connected monitor. Unset, the login screen keeps its old choice, the last-connected monitor.

### Split: Aoide is the engine, Strillix is the OS

- Paint options leave `aoide.*` under the same names: `aoide.strillix.{enable,root,home}`,
  `aoide.hostConfig`, `aoide.rice`, `aoide.rices.{builtIn,greeter}`, `aoide.theme.*`,
  `aoide.layout.*`, `aoide.surfaces`, `aoide.components.*`, `aoide.wallpaper.provider`,
  `aoide.quickshell.*` and `aoide.stylix.enable` become `strillix.*`. A hard rename with no alias: a host that sets an old name fails
  eval with an unknown option until it is renamed. `aoide.enable` no longer
  turns Strillix on; `strillix.enable` does, and Strillix runs with `aoide.enable` off.
  A NixOS host whose user also imports `homeModules.nucleus` fails eval naming
  `strillix.runtime.nixos`; `homeModules.nucleus` is for standalone homes, and
  `tests/standalone-home` checks that refusal.
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
  The move also rewrites the names and paths the state files store: `song`
  becomes `rice`, `stagingSong` becomes `stagingRice`, a history entry's `take`,
  `livery` and `cover` become `entry`, `theme` and `wallpaper`, and each path
  that moved is rewritten to where its file now sits. `declared/songbook/`
  becomes `declared/rices/`, and `rices-aside/` is left as it was. A `covers`
  link an older generation placed over the real library, or no `covers` at
  all, is replaced by the `covers.backup` folder it kept, so the wallpapers
  move with the rest. If that repair fails, both stay where they were and the
  next run retries it.

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

### `rice list` reports pending edits and the next rebuild

- `strillix rice list --json` gains a top-level `nextRebuild`: the rice the
  host file's one `habit.rice.declared` line names, when it differs from
  `declared` (null otherwise). Each rice row gains `pending`, true when the
  machine copy's theme, `rice.nix`, `hyprland.conf` or widgets differ from the
  shipped copy or the build ships none, and `nixChanged`, true when `rice.nix`
  is among those. Both are null where the templates dir or a read cannot say.
- The barcarolle, cadenza and sonata rice menus draw them: a readable rice
  whose `pending` is true reads `<rice> · pending`, a dimmed `rice.nix
  changed` row sits under a rice whose `nixChanged` is true, and a dimmed
  `next rebuild: <rice>` row under the declarative row. A bridge that sends
  none of the fields leaves the menus as they were.
