---
type: concept
created: 2026-10-07
updated: 2026-10-07
tags: [aoide, cli, lyra, desktop, quickshell]
---

# Apps Commands — the `lyra apps` Command Surface

The `apps` command group lists the installed desktop apps with their icons
already resolved to files, and keeps `song/stage/apps.json` current for the
shell's launcher. It is a `lyra` group ([[lyra]]): desktop entries and icon
themes exist to paint a graphical session, and a resolved icon path is a
paint input, so the capability sits with the paint binary. Delete every
`.qml` and `lyra apps list --json` still answers from a terminal
([[Widget-Bridge-Contract]]).

Handlers live in `pkgs/aoide/crates/lyra/src/commands/apps.rs`. They read
through `pkgs/aoide/crates/lyra/src/xdg/`, the workspace's one freedesktop
key-file reader: `entry.rs` (`.desktop` files and the listing filter),
`icon_theme.rs` (themes and icon lookup), `keyfile.rs` (the shared grammar).
lyra launch reads the same module; no second parser exists. The document
shape, the listing filter and the icon rules are CONTRACTS.md §4,
"`song/stage/apps.json`".

`lyra icon` is unrelated: it serves pinned Iconify glyphs
([[Widget-Preview]]), not the icons of installed apps.

## Shared facts

Every command reads the process environment through one `Env`:
`$XDG_DATA_HOME` then `$XDG_DATA_DIRS` (an empty or relative value counts as
unset), `$XDG_CONFIG_HOME`, `$HOME`, `$XDG_CURRENT_DESKTOP` split on `:`, and
the locale from the first set of `LC_ALL`, `LC_MESSAGES`, `LANG`. The first
data dir holding a desktop-file id wins, and that winning file alone decides
whether the id is listed.

The icon theme is `gtk-icon-theme-name` from
`$XDG_CONFIG_HOME/gtk-3.0/settings.ini`, used when some icon base dir holds
that theme's `index.theme`, else `hicolor`. Icons resolve at a nominal 48 px
and scale 1.

Every text field an entry carries (`name`, `genericName`, `comment`,
`keywords`, `startupWMClass`, action names) is untrusted data from a file
anyone can install. `Exec`, `Path` and `TryExec` are never emitted: launching
is by id, through lyra launch.

## lyra apps list

```
lyra apps list [--json]
```

- **Reads:** the desktop entries and icon themes on the data dirs. Never reads
  `apps.json`.
- **Writes:** nothing.
- **Output:** text is a count line, then one line per entry: the name, its id
  in parentheses, and `terminal` when the entry runs in a terminal. `--json`
  carries the `apps.json` document as `data`, built fresh.

```
lyra apps list
lyra apps list --json
```

## lyra apps show

```
lyra apps show <id> [--json]
```

- **Reads:** the same inputs, resolving one id.
- **Output:** the entry in the `apps.json` shape plus `file` (the winning
  `.desktop`), `listed` (whether the shell lists it) and `filtered` (null, or
  the filter that dropped it: `not-application`, `no-name`, `no-exec`,
  `no-display`, `only-show-in`, `not-show-in`, `try-exec`). An unlisted entry
  still shows; it is the one way to see why a launcher omits an app.
- **Refusals** (exit 1, `Fix: lyra apps list`):
  - an unknown id, with up to three closest ids;
  - a `Hidden=true` entry — `the entry is marked Hidden, which deletes it`;
  - a winning file that is unreadable, not UTF-8, over 1 MiB, or has no
    `[Desktop Entry]` group, with the reason.
- A missing `<id>` is refused by the declaration at exit 2.

```
lyra apps show org.gnome.Nautilus
```

## lyra apps publish

```
lyra apps publish [--run] [--json]
```

- **Writes:** `song/stage/apps.json` (`aoide_storage::fs::stage_dir()`, default
  `~/.aoide/song/stage/apps.json`), atomically, and only when the new document
  minus `at` differs from the file on disk minus `at`. There is no stage lock;
  the file has one writer and is replaced whole.
- **Output:** `{path, written, entries, theme}`. A second run with nothing
  changed reports `written: false`.
- **`--run`:** keeps the file current until stopped. Every 2 s it recomputes a
  fingerprint of the data dirs' canonical paths, every `*.desktop` below their
  canonical `applications/` (name, size, mtime) and the canonical
  `settings.ini`, rebuilds only when it changed, and prints one stderr line for
  a write. Every path is re-resolved on every tick and nothing holds a watch or
  an inode, so a nix profile swap is seen. A failed write is logged and the
  loop continues. `--run` never returns and is CLI-only: another door refuses it at
  exit 2.

```
lyra apps publish
lyra apps publish --run
```

## The aoide-apps unit

`aoide-apps` is a user unit in `modules/dendrites/lyra/shellbridge.nix`:
`ExecStart` is `lyra apps publish --run`, `Type=simple`, wanted by, ordered
after and part of `graphical-session.target`, `Restart=on-failure`
(`RestartSec=3s`), with `AOIDE_USER` and `AOIDE_ROOT` in its environment and
`NoNewPrivileges`. The user manager hands it `XDG_DATA_DIRS`,
`XDG_CURRENT_DESKTOP` and `LANG`; it has no session `PATH`, which is why a
relative `TryExec` is never looked up.

## Related

- [[CLI-Reference]]
- [[lyra]]
- [[Widget-Bridge-Contract]]
