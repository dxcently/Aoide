# sonata's `launcher` slot — the SUPER+Space launcher, hosted by shell.qml's
# `SurfaceSlot { slot: "launcher" }` (slots.md's wired table, extras
# `clipboard`/`ledger`). `kind` absent → null: shell-anchored, never a
# declared registry entry. `clipboard` and `ledger` are shell QtObjects
# (`AoideClipboard`, `GrimoireLedger`), not sibling slots, so they are not
# `dependsOn` entries.
_: {
  file = "launcher.qml";
}
