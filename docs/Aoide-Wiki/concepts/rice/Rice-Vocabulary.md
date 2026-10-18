---
type: concept
created: 2026-07-25
updated: 2026-08-27
tags: [aoide, naming, rice]
source: "[[references/AOIDE-HANDOFF]]"
---

# Rice Vocabulary — the Performed Half

The naming thesis, engraved in the design: architecture is frozen music. The nix layer is the score — crystalline, immutable (see [[Snowflake-Anatomy]]) — and the running desktop is the performance. A rice is a song the system sings.

Rice vocabulary names the performed half. Snowflake vocabulary names the frozen half. theme is where they meet: values frozen into the crystal, sounded at runtime.

## The Rice Map

Every term maps to a literal path — committed score under the dev checkout's
`$AOIDE_FLAKE_ROOT/rice/` (default `~/Aoide/rice`; onboard links `~/rice` →
the checkout's `rice/`), runtime state under `$STRILLIX_ROOT/` (default
`~/.aoide/rice`).

| Music term | Meaning | `rice/` path |
|---|---|---|
| key | palette | `rices/<rice>/palette/` |
| melody | semantic tier — survives transposition | (tier within theme) |
| component tier | bar / notif / window overrides | (tier within theme) |
| instruments | paint lanes — quickshell, compositor, stylix, greeter, strillix | `modules/dendrites/` (in the nix tree) |
| rice | rice | `rices/<rice>/` |
| design | per-rice design wiki | `rices/<rice>/design/` |
| rices | per-rice homes + cross-cutting memory | `rices/` |
| wallpaper | wallpaper | `/wallpapers/` |
| chimes | notification + system sounds | `rices/<rice>/sounds/` |
| stage | live preview state (runtime) | `$STRILLIX_ROOT/stage/` |
| the standard | shipped standard rice | `rices/sonata/` |
| rehearsal | preview (`stage/theme.json`, hot-reload) | — |
| recording | adopt — durable, committed, rebuilt | — |
| venue | host — its own specifics and enabled instruments | `hosts/<host>/` |
| replay | perform an existing rice at a new venue | `aoide.rice = "<name>";` in host config |

The runtime tree off `$AOIDE_ROOT` (`/stage/`, the composed `rices/`
with its per-rice `drafts/`, `state/`, `run/qml/`, `log`) holds ephemera only.
Everything in the checkout's `rice/` is versioned score.

## Full-Orchestration Coverage

A full-orchestration rice addresses all six dimensions:

| Dimension | What it covers |
|---|---|
| key | Palette (base16 color scheme) |
| voice | Typography — fonts, scale, nerd glyphs |
| geometry | Gaps, radius, borders, blur, animations |
| instruments | Quickshell shell surfaces (bar, notifs, launcher, OSD, lockscreen, greeter, wallpaper layer, widgets); compositor (hyprctl, live); [[Stylix]] fan-out (terminal + prompt, GTK/Qt + icons + cursor, editors, browser, boot) |
| chimes | Notification and system sounds |
| wallpaper | Wallpaper |

Everything app-side is a [[Stylix]] target — `rice.nix` feeds the scheme once and it fans out to every nix-manageable app. Everything shell-side is a [[Quickshell]] widget — one runtime reading `stage/theme.json` means nearly the full rice hot-reloads at rehearsal.

## Replay — any rice, any host

A rice is a score, not a room. The same committed rice can be performed at any venue — any host in the fleet. The venue supplies its own acoustics (host specifics: hardware, monitors, systemd environment) and its own available instruments (which dendrites are enabled). The score does not change; only what the venue can sound differs.

Replaying a committed rice on another host is a single declaration in that host's `hosts/<host>/default.nix`:

```nix
aoide.rice = "sonata";
```

Rices are SELECTED, not walked: the host module names what it performs
(`habit.rice.declared`) and what it keeps built in to stage without a rebuild
(`habit.rice.available`), and `lib/strillix.nix` wires exactly those rices in. Each rice's
`rice.nix` is a body that habit imports only when its rice is performed, so only
one rice activates per host. Committing a rice to the clone makes it available to
select — a host performs it once its record names it, and can stage one it never
built in when that rice's nix and packages are present.

**The separation of concerns (the point of replay):** the rice carries only theme — palette, component tiers, and its own wallpapers and chimes. It never sets host options, hardware configuration, or which dendrites are enabled. Those remain host responsibilities. A host lacking an instrument simply does not sound that part; coverage degrades gracefully through the [[Self-Ricing]] coverage tiers. Host-agnosticism is a documented rice-shape convention in `CONTRACTS.md`.

The `rice-runtime-untracked` check fails the tree if any `rice/` runtime dir (`/stage/` · `rice/auditions/` · `/declared/`) exists in the source at all — pure flake eval reads only tracked files, and `.gitignore` covers those three, so committing or leaving one behind is what the check catches. `rice declare` never carries a draft (or `history/`) into the checkout, and a draft copied there by hand lands under a gitignored `rices/*/drafts/` and is never tracked. Committed `rices/**` is versioned score — it is legitimately read at eval and safe for hosts to reference.

**Transpose vs replay:** transposing a rice replays it in a different key (new palette, same venue). Replaying at a new venue uses the same key but lets a different host's instruments sound it.

## Related

- [[Self-Ricing]]
- [[theme]]
- [[Stylix]]
- [[Snowflake-Anatomy]]
- [[Clone-and-Run]]
- [[Ricing-Protocol|Ricing Protocol]] — the creation/application split and the light/dark vision-check
