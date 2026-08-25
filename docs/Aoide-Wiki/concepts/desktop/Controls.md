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

Keybinds are host-invariant behaviour, so they live in
`modules/dendrites/hyprland.nix`, not the compositor facet (which owns only
livery-derived appearance — gaps, borders, layerrules). `SUPER` is the
modifier.

| Keybind | Action |
|---|---|
| `SUPER + SPACE` | Toggle the launcher — a Hyprland global shortcut (`global, aoide:launcher`) reaching the QML surface directly; no CLI verb. |
| `SUPER + W` | Toggle the wallpaper picker (`global, aoide:wallpaper`); picking a cover shells `aoide cover set <path>`. |
| `SUPER + G` | Toggle the gadget dock (`global, aoide:dock`) — see [[Gadget-Dock]]. |
| `SUPER + C` | Toggle clipboard history — opens the launcher on its clipboard chapter (`global, aoide:clipboard`). |
| `SUPER + ESCAPE` | Lock the screen — `exec, aoide shell lock`; no `shell` command group exists in either binary's registry, so this binding is dead (report only, not repointed). |
| `SUPER SHIFT + P` | Bound to `lyra rice preview` in `modules/dendrites/hyprland.nix` — a retired verb, so the keybind is a no-op until repointed to `rice stage` (flagged in [[AOIDE-DEV]] §7). |
| `SUPER SHIFT + A` | Bound to `lyra rice adopt` in `modules/dendrites/hyprland.nix` — a retired verb, so the keybind is a no-op until repointed to `rice declare` (a stub — `implemented: false`; flagged in [[AOIDE-DEV]] §7). |
| `SUPER + RETURN` | Open a terminal (kitty). |
| `SUPER + Q` | Close the active window. |
| `SUPER + V` / `SUPER + F` | Toggle floating / fullscreen. |
| `SUPER + arrows` (or `H/J/K/L`) | Move focus — full parity between arrows and the vim set (lock moved off `L` to `ESCAPE`, freeing it for `movefocus, r`). |
| `SUPER SHIFT + arrows` (or `H/J/K/L`) | Move the window — same full parity. |
| `SUPER ALT + arrows` (or `H/J/K`) | Resize the window (repeats while held) — `L` is unbound here; ALT+right is arrow-only. |
| `SUPER + 1–0` | Switch to workspace 1–10. |
| `SUPER SHIFT + 1–0` | Move window to workspace 1–10. |
| `ALT + Tab` | Previous workspace. |
| `SUPER + X` / `SUPER + Z` | Toggle special workspace `magic` / `scratch` (`SHIFT` = move window there). |
| `SUPER + leftdrag` / `rightdrag` | Move / resize window with the mouse. |

> Launcher/dock/wallpaper/clipboard route entirely through Hyprland global
> shortcuts consumed by QML — no CLI verb, no inbound socket. Lock is the one
> holdout still exec'ing a CLI form (`aoide shell lock`) that resolves to
> nothing in the command registry.

### Bar cell interactions

`song/songbook/sonata/widgets/bar.qml` (the bar moved out of the facet into
the song; see [[Song-Anatomy]]):

| Cell | Interaction |
|---|---|
| Sessions (`✎N`, left) | Click → toggle the gadget dock. |
| Clock + date (left) | Click → toggle the calendar popout, if the active song authors one. |
| Volume / mic (right) | Scroll = adjust, click = toggle the audio colonnade, hover = a cue readout (no separate slider). |
| Battery (right) | Hover → time-remaining + charge-bar popout; no click. |
| Network (right) | Click → toggle the DIKTYON network stele; no hover popout. |
| Tray (right) | Click → toggle the held-icons popout. |

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
