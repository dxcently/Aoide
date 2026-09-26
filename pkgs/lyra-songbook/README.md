# pkgs/lyra-songbook

The shipped score templates: what `lyra` falls back to on a machine with no Aoide
checkout, and what `aoideSeedSongbook` copies a machine's own songbook from.

## What it is

`$out/share/lyra/songbook/` — the song folders this instance ships, plus
`manifest.json`, `registry.json` and (when a host selected songs)
`builtin.json`. `aoide_storage::fs::song_templates_dir` finds it through
`AOIDE_SONG_TEMPLATES` or the exe-sibling `../share/lyra/songbook`, and only
`aoide-song` reads it, so it is wired onto the units whose process actually execs
`lyra`.

## Two knobs

| argument | default | what it does |
|---|---|---|
| `songs` | `null` | `null` ships every discovered song; a list ships those folders |
| `builtin` | `null` | `{ declared, songs, packages }`, written to `builtin.json`; `null` writes none |

The flake's own `packages.lyra-songbook` uses the defaults. The `lyra` lane
overrides both per host (`lib/songbook.nix`'s `builtIn`), which is what makes the
package the byte-level meaning of "built in": a host that performs one song ships
one song's folders and records the selection in `builtin.json`.

## What a derivation reads — the part that is not tidiness

Each shipped song enters the build as **its own store path**
(`builtins.path { path = <song>; name = "lyra-song-<song>"; }`), never as a
subdirectory of the whole `song/songbook` tree, and the baked
`manifest.json`/`registry.json` are `builtins.toFile` over the shipped set only.

That is a contract with every host, not a micro-optimisation. This package's
store path is a STRING in a host's `environment.sessionVariables`
(`AOIDE_SONG_TEMPLATES`) and in its `aoideSeedSongbook` activation entry, so both
the `etc` and `activate` derivations of a NixOS system name it — which means
anything that rehashes this derivation moves the host's **toplevel drvPath**
even when no configuration changed. While the build read the songbook directory,
that was true of every song in the repo: editing a song a host does not even
build in moved that host's system. Now it is true only of the songs it ships.

## Consumers

- a NixOS host, through the `lyra` lane's override (above);
- the flake's `packages.lyra-songbook`, for a consumer that wants the whole
  songbook without a host;
- `lyra rice compose` / the runtime manifest regeneration
  (`crates/song/src/widgets.rs`), which fall back to it on a repo-less machine.
