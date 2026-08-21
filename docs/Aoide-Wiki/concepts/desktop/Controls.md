---
type: concept
created: 2026-08-14
tags: [aoide, desktop, keybinds, aliases, cli]
---

# Controls — Keybindings & Shell Aliases

The day-to-day surface: the rebuild aliases, the compositor keybinds, the bar's
click/scroll/hover interactions, and the shell quality-of-life aliases. For the
full `aoide` command tree see [[aoide-cli]]; for what each surface *is*
architecturally see [[Desktop-Architecture]] and [[Gadget-Dock]].

## The `ad*` rebuild family

Shell aliases from the `bash` dendrite (`modules/dendrites/bash.nix`), pointed
at the Aoide flake at `/home/khoa/Aoide`. They wrap [nh](https://github.com/viperML/nh).

| Alias | Runs | What it does |
|---|---|---|
| `ad` | `cd /home/khoa/Aoide` | Jump to the flake dir. |
| `adrebuild` | `nh os switch` | Build + activate + set as boot default (the everyday rebuild). |
| `adupdate` | `nh os switch --update` | Same, but bump flake inputs first (`flake.lock`). |
| `adboot` | `nh os boot` | Build + set as boot default, activate on next reboot. |
| `adtest` | `nh os test` | Build + activate **without** touching the boot default (reverts on reboot). |
| `adbuild` | `nh os build` | Build only — no activation, no privilege needed. |
| `adrollback` | `nh os rollback` | Roll back to the previous generation. |
| `adcheck` | `nix flake check` | Run the flake's `checks` (coupling, song-shape, packages, VM boot). |
| `adgens` | `nh os info` | List system generations. |
| `adclean` | `nh clean all` | Garbage-collect old generations + store paths. |

Typical loop: `adbuild` to validate → `adtest` to try it live → `adrebuild` to
make it stick. The rebuild itself is the **user-gated** step — see
[[Rebuild-Gate]].

## Desktop keybinds

Compositor keybinds live in `modules/facets/compositor/default.nix`. `SUPER`
is the modifier.

| Keybind | Action |
|---|---|
| `SUPER + SPACE` | Toggle the launcher (`aoide shell launcher toggle`). |
| `SUPER + G` | Toggle the gadget dock — see [[Gadget-Dock]]. |
| `SUPER + L` | Lock the screen (`aoide shell lock`). |
| `SUPER SHIFT + P` | Bound to `lyra rice preview` in `modules/dendrites/hyprland.nix` — a retired verb, so the keybind is a no-op until repointed to `rice stage` (flagged in [[AOIDE-DEV]] §7). |
| `SUPER SHIFT + A` | Bound to `lyra rice adopt` in `modules/dendrites/hyprland.nix` — a retired verb, so the keybind is a no-op until repointed to `rice declare` (flagged in [[AOIDE-DEV]] §7). |
| `SUPER + RETURN` | Open a terminal (kitty). |
| `SUPER + Q` | Close the active window. |
| `SUPER + V` / `SUPER + F` | Toggle floating / fullscreen. |
| `SUPER + arrows` (or `H/J/K`) | Move focus (arrows: full set; H/J/K: vim left/down/up — right is arrow-only, `SUPER + L` stays lock). |
| `SUPER SHIFT + arrows` (or `H/J/K`) | Move the window. |
| `SUPER ALT + arrows` (or `H/J/K`) | Resize the window (repeats while held). |
| `SUPER + 1–0` | Switch to workspace 1–10. |
| `SUPER SHIFT + 1–0` | Move window to workspace 1–10. |
| `ALT + Tab` | Previous workspace. |
| `SUPER + X` / `SUPER + Z` | Toggle special workspace `magic` / `scratch` (`SHIFT` = move window there). |
| `SUPER + leftdrag` / `rightdrag` | Move / resize window with the mouse. |

> The `aoide shell *` verbs the launcher/lock keybinds call are not yet in the
> command schema — a documented open seam (the bridge path is stubbed). The
> hot-edge and pure-QML paths work today regardless.

### Bar cell interactions

`modules/facets/quickshell/qml/AoideBar.qml`:

| Cell | Interaction |
|---|---|
| Clock (center) | Click → anchored calendar month-grid popup. |
| Volume (right) | Scroll = adjust, click = mute, hover = slider. |
| Sessions (`✎`, left) | Click → toggle the gadget dock. |
| Battery / network | Hover popout; note-glyph icons. |

## Shell QoL aliases (`bash` dendrite)

| Alias | Runs |
|---|---|
| `v`, `nv` | `nvim` |
| `lg` | `lazygit` |
| `crc` | `claude --rc` |
| `y` | `yazi` cd-wrapper (chdir to where yazi last browsed) |
| `ls` / `ll` / `la` / `lt` | `lsd` / `lsd -l` / `lsd -la` / `lsd --tree` |
| `..` | `cd ..` |
| `reboot` / `shutdown` / `poweroff` | `systemctl reboot` / `poweroff` / `poweroff` |
| `sleep` / `hibernate` | `systemctl suspend` / `hibernate` |
| `lock` | `hyprlock` |

## Related

- [[aoide-cli]]
- [[Desktop-Architecture]]
- [[Gadget-Dock]]
- [[Rebuild-Gate]]
- [[Self-Ricing]]
