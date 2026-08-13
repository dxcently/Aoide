---
type: concept
created: 2026-07-25
updated: 2026-08-13
tags: [aoide, naming, rice]
source: "[[references/AOIDE-HANDOFF]]"
---

# Song Vocabulary — the Performed Half

The naming thesis, engraved in the design: architecture is frozen music. The nix layer is the score — crystalline, immutable (see [[Snowflake-Anatomy]]) — and the running desktop is the performance. A rice is a song the system sings.

Song vocabulary names the performed half. Snowflake vocabulary names the frozen half. livery is where they meet: values frozen into the crystal, sounded at runtime.

## The Song Map

Every term maps to a literal path inside `song/` (which lives at `~/Aoide/song`; onboard links `~/song` → `~/Aoide/song`).

| Music term | Meaning | `song/` path |
|---|---|---|
| key | palette | `songbook/<song>/palette/` |
| melody | semantic tier — survives transposition | (tier within livery) |
| arrangement | component tier | (tier within livery) |
| instruments | facets — quickshell, compositor, stylix | `modules/facets/` (in the nix tree) |
| song | rice | `songbook/<song>/` |
| design | per-song design wiki | `songbook/<song>/design/` |
| songbook | per-song homes + cross-cutting memory | `songbook/` |
| cover | wallpaper | `song/covers/` |
| chimes | notification + system sounds | `songbook/<song>/sounds/` |
| stage | live preview state (gitignored) | `stage/` |
| auditions | propose gate (gitignored) | `auditions/` |
| the standard | shipped default song | `songbook/default/` |
| rehearsal | preview (`stage/livery.json`, hot-reload) | — |
| recording | adopt — durable, committed, rebuilt | — |
| venue | host — its own specifics and enabled instruments | `hosts/<host>/` |
| replay | perform an existing song at a new venue | `aoide.song = "<name>";` in host config |

Gitignored runtime dirs (`stage/`, `auditions/`, `catalog/`, `index/`, `log/`) hold ephemera only. Everything else is versioned.

## Arrangement — What a Full Rice Covers

A full-orchestration rice addresses all six dimensions:

| Dimension | What it covers |
|---|---|
| key | Palette (base16 color scheme) |
| voice | Typography — fonts, scale, nerd glyphs |
| geometry | Gaps, radius, borders, blur, animations |
| instruments | Quickshell shell surfaces (bar, notifs, launcher, OSD, lockscreen, greeter, wallpaper layer, widgets); compositor (hyprctl, live); [[Stylix]] fan-out (terminal + prompt, GTK/Qt + icons + cursor, editors, browser, boot) |
| chimes | Notification and system sounds |
| cover | Wallpaper |

Everything app-side is a [[Stylix]] target — `rice.nix` feeds the scheme once and it fans out to every nix-manageable app. Everything shell-side is a [[Quickshell]] widget — one runtime reading `stage/livery.json` means nearly the full arrangement hot-reloads at rehearsal.

## Replay — any song, any host

A song is a score, not a room. The same committed song can be performed at any venue — any host in the fleet. The venue supplies its own acoustics (host specifics: hardware, monitors, systemd environment) and its own available instruments (which facets and dendrites are enabled). The score does not change; only what the venue can sound differs.

Replaying a committed song on another host is a single declaration in that host's `hosts/<host>/default.nix`:

```nix
aoide.song = "sonata";
```

Songs self-register like dendrites: `lib/mkHost.nix` walks `song/songbook/` alongside `modules/`. Each song's `rice.nix` guards itself with `lib.mkIf (config.aoide.song == "<name>")`, so only one song activates per host. Committing a song to the fork makes it fleet-available — every host that pulls can perform it.

**The separation of concerns (the point of replay):** the song carries only livery — palette, component tiers, and its own covers and chimes. It never sets host options, hardware configuration, or which facets and dendrites are enabled. Those remain host responsibilities. A host lacking an instrument simply does not sound that part; coverage degrades gracefully through the [[Self-Ricing]] coverage tiers. Host-agnosticism is a documented song-shape convention in `CONTRACTS.md`.

The `noSongRead` check guards only the runtime dirs (`stage/`, `auditions/`). Committed `song/songbook/**` is versioned score — it is legitimately read at eval and safe for hosts to reference.

**Transpose vs replay:** transposing a song replays it in a different key (new palette, same venue). Replaying at a new venue uses the same key but lets a different host's instruments sound it.

## Related

- [[Self-Ricing]]
- [[livery]]
- [[Stylix]]
- [[Snowflake-Anatomy]]
- [[Fork-and-Run]]
- [[Ricing-Protocol|Ricing Protocol]] — the creation/application split and the light/dark vision-check
