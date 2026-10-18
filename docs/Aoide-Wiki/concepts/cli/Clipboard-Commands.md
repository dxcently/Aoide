---
type: concept
created: 2026-10-15
updated: 2026-10-15
tags: [aoide, cli, strillix, desktop, clipboard]
---

# Clipboard Commands — the `strx clipboard` Command Surface

The `clipboard` command group removes entries from the clipboard history:
one by id, or all of them. It is a `strillix` group ([[strillix]]): the history is
cliphist's record of a Wayland session's clipboard, and the thumbnails it
also removes are the launcher's paint cache. The launcher's clipboard backend
(`pkgs/strillix-shell/qml/AoideClipboard.qml`) runs these commands; it never runs
`cliphist delete` or `cliphist wipe` itself. Delete every `.qml` and
`strx clipboard clear` still empties the history from a terminal
([[Widget-Bridge-Contract]]).

Handlers live in `pkgs/aoide/crates/strillix/src/commands/clipboard.rs`.

## Shared facts

Both commands run the `cliphist` on `PATH`, with the caller's environment, so
they act on the same database `cliphist list` reads (its default is
`~/.cache/cliphist/db`). The `clipboard` dendrite
(`modules/dendrites/clipboard.nix`) installs cliphist; a host running its own
watcher brings its own.

Thumbnails are `$XDG_CACHE_HOME/aoide-clipboard/previews/<id>`, with
`~/.cache` when `XDG_CACHE_HOME` is empty or relative: the directory
`AoideClipboard.qml` decodes image entries into.

Only an id reaches cliphist. The history's text is read only to learn which
ids exist, and never reaches the output.

Every refusal carries a stable `data.reason` beside the taught
`data.refusal`:

| `reason` | kind | exit | when |
|---|---|---|---|
| `bad-id` | usage | 2 | the id is not a whole number that fits a u64 (signs, spaces, decimals refused) |
| `no-such-entry` | refused | 1 | cliphist holds no entry with that id |
| `cliphist-missing` | refused | 1 | no `cliphist` on `PATH`; fix sets `habit.dendrites.clipboard.enable = true` |
| `cliphist-failed` | failed | 1 | cliphist exited non-zero; its stderr is `data.detail` |
| `preview-remove-failed` | failed | 1 | the history changed but a thumbnail could not be removed |

## strx clipboard delete

```
strx clipboard delete <id> [--json]
```

- **Reads:** `cliphist list`, for its ids.
- **Writes:** writes `<id>\t` to `cliphist delete`'s stdin, then removes
  `previews/<id>` when present. A leading zero names the same entry
  (`007` is `7`).
- **Output:** `deleted clipboard entry <id>`. `--json` data:
  `{"id": "<id>", "previewRemoved": <bool>}`, the id in canonical form.

```
strx clipboard delete 42
strx clipboard delete 42 --json
```

## strx clipboard clear

```
strx clipboard clear [--json]
```

- **Writes:** runs `cliphist wipe`, then removes the whole `previews`
  directory. Clearing an empty history succeeds. It asks nothing: a
  confirmation belongs to the surface that offers it.
- **Output:** `cleared the clipboard history`. `--json` data:
  `{"previewsRemoved": <n>}`, the number of entries the `previews` directory
  held.

```
strx clipboard clear
strx clipboard clear --json
```

## The QML side

`AoideClipboard` (the launcher slot's `clipboard` extra) exposes
`deleteById(id)`, which returns `false` without running anything for a
non-numeric id, and `clear()`. Each runs `strx clipboard … --json`, puts
strillix's message in `actionError` when it fails (`""` otherwise), and then
calls `refresh()`.

## Related

- [[CLI-Reference]]
- [[strillix]]
- [[Widget-Bridge-Contract]]
