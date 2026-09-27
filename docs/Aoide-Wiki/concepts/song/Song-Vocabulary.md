---
type: concept
created: 2026-07-25
updated: 2026-08-27
tags: [aoide, naming, rice]
source: "[[references/AOIDE-HANDOFF]]"
---

# Song Vocabulary — the Performed Half

The naming thesis, engraved in the design: architecture is frozen music. The nix layer is the score — crystalline, immutable (see [[Snowflake-Anatomy]]) — and the running desktop is the performance. A rice is a song the system sings.

Song vocabulary names the performed half. Snowflake vocabulary names the frozen half. livery is where they meet: values frozen into the crystal, sounded at runtime.

## The Song Map

Every term maps to a literal path — committed score under the dev checkout's
`$AOIDE_FLAKE_ROOT/song/` (default `~/Aoide/song`; onboard links `~/song` →
the checkout's `song/`), runtime state under `$AOIDE_ROOT/song/` (default
`~/.aoide/song`).

| Music term | Meaning | `song/` path |
|---|---|---|
| key | palette | `songbook/<song>/palette/` |
| melody | semantic tier — survives transposition | (tier within livery) |
| component tier | bar / notif / window overrides | (tier within livery) |
| instruments | paint lanes — quickshell, compositor, stylix, greeter, lyra | `modules/dendrites/` (in the nix tree) |
| song | rice | `songbook/<song>/` |
| design | per-song design wiki | `songbook/<song>/design/` |
| songbook | per-song homes + cross-cutting memory | `songbook/` |
| cover | wallpaper | `song/covers/` |
| chimes | notification + system sounds | `songbook/<song>/sounds/` |
| stage | live preview state (runtime) | `$AOIDE_ROOT/song/stage/` |
| the standard | shipped standard song | `songbook/sonata/` |
| rehearsal | preview (`stage/livery.json`, hot-reload) | — |
| recording | adopt — durable, committed, rebuilt | — |
| venue | host — its own specifics and enabled instruments | `hosts/<host>/` |
| replay | perform an existing song at a new venue | `aoide.song = "<name>";` in host config |

The runtime tree off `$AOIDE_ROOT` (`song/stage/`, the composed `song/songbook/`
with its per-song `drafts/`, `state/`, `run/qml/`, `log`) holds ephemera only.
Everything in the checkout's `song/` is versioned score.

## Full-Orchestration Coverage

A full-orchestration rice addresses all six dimensions:

| Dimension | What it covers |
|---|---|
| key | Palette (base16 color scheme) |
| voice | Typography — fonts, scale, nerd glyphs |
| geometry | Gaps, radius, borders, blur, animations |
| instruments | Quickshell shell surfaces (bar, notifs, launcher, OSD, lockscreen, greeter, wallpaper layer, widgets); compositor (hyprctl, live); [[Stylix]] fan-out (terminal + prompt, GTK/Qt + icons + cursor, editors, browser, boot) |
| chimes | Notification and system sounds |
| cover | Wallpaper |

Everything app-side is a [[Stylix]] target — `rice.nix` feeds the scheme once and it fans out to every nix-manageable app. Everything shell-side is a [[Quickshell]] widget — one runtime reading `stage/livery.json` means nearly the full rice hot-reloads at rehearsal.

## Replay — any song, any host

A song is a score, not a room. The same committed song can be performed at any venue — any host in the fleet. The venue supplies its own acoustics (host specifics: hardware, monitors, systemd environment) and its own available instruments (which dendrites are enabled). The score does not change; only what the venue can sound differs.

Replaying a committed song on another host is a single declaration in that host's `hosts/<host>/default.nix`:

```nix
aoide.song = "sonata";
```

Songs are SELECTED, not walked: the host record names what it performs
(`song.declared`) and what it keeps built in to stage without a rebuild
(`song.available`), and `lib/aoideos.nix` wires exactly those songs in. Each song's
`rice.nix` guards itself with `lib.mkIf (config.aoide.song == "<name>")`, so only
one song activates per host. Committing a song to the clone makes it available to
select — a host performs it once its record names it, and can stage one it never
built in when that song's nix and packages are present.

**The separation of concerns (the point of replay):** the song carries only livery — palette, component tiers, and its own covers and chimes. It never sets host options, hardware configuration, or which dendrites are enabled. Those remain host responsibilities. A host lacking an instrument simply does not sound that part; coverage degrades gracefully through the [[Self-Ricing]] coverage tiers. Host-agnosticism is a documented song-shape convention in `CONTRACTS.md`.

The `song-runtime-untracked` check fails the tree if any `song/` runtime dir (`song/stage/` · `song/auditions/` · `song/declared/`) exists in the source at all — pure flake eval reads only tracked files, and `.gitignore` covers those three, so committing or leaving one behind is what the check catches. A `rice declare` copy of a draft into the checkout lands under a gitignored `song/songbook/*/drafts/` and is never tracked. Committed `song/songbook/**` is versioned score — it is legitimately read at eval and safe for hosts to reference.

**Transpose vs replay:** transposing a song replays it in a different key (new palette, same venue). Replaying at a new venue uses the same key but lets a different host's instruments sound it.

## Related

- [[Self-Ricing]]
- [[livery]]
- [[Stylix]]
- [[Snowflake-Anatomy]]
- [[Clone-and-Run]]
- [[Ricing-Protocol|Ricing Protocol]] — the creation/application split and the light/dark vision-check
