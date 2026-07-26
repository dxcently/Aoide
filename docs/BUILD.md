# docs/BUILD.md — building on the Wave-0 foundation

Wave 0 established the flake, the walker, the option contract, the hosts, and
the packaging placeholders. **Only Wave 0 edits `flake.nix` and `lib/`.** Every
later wave ADDS files in its own directory; nothing below requires touching
`flake.nix` or `lib/` again.

Verify at any point:

```
cd ~/Aoide
nix flake check 2>&1 | tail -20                       # must stay green
nix eval .#nixosConfigurations.yomi-strix.config.system.build.toplevel.drvPath
```

---

## The walker (how discovery works)

`lib/walk.nix` hands every `.nix` file under `modules/` to every host. There is
no import list. To add a module, drop a file in the right layer:

- `modules/nucleus/` — core, applies unconditionally (no `mkIf`).
- `modules/dendrites/` — opt-in features, guarded on a flag.
- `modules/facets/` — render surfaces, read `aoide.tokens` only.
- `modules/rime/` — rice engine + shipped default rices.

**Shelving opt-out:** any path containing `/_` is skipped. Prefix a
work-in-progress file (`_wip.nix`) or dir (`_scratch/`) with `_` to hide it.

---

## The option contract (what you read)

Declared in `modules/nucleus/options.nix`, evaluated per host. Read these; write
none of them except your own dendrite/facet flags.

| Option                         | Type                         | Notes |
| ------------------------------ | ---------------------------- | ----- |
| `aoide.enable`                 | bool                         | framework master switch |
| `aoide.user`                   | str (default `"khoa"`)       | owner of the `~/Aoide` fork |
| `aoide.tokens.palette.{bg,fg,accent,urgent}` | hex        | v0 palette (base16) |
| `aoide.tokens.bar.{bg,fg,accent}` | nullOr hex                | component override; null → palette |
| `aoide.tokens.notif.{bg,fg,urgent}` | nullOr hex              | component override; null → palette |
| `aoide.tokens.window.{border,borderInactive}` | nullOr hex     | component override; null → palette |
| `aoide.surfaces.<name>.owner`  | str                          | surface-ownership registry |
| `aoide.mcp.enable`             | bool (default false)         | MCP façade toggle |
| `aoide.auditLog`               | str (default `/home/<user>/Aoide/log`) | single audit log |

See `CONTRACTS.md §1` for the full token schema and fallback rules.

---

## Authoring a dendrite (Wave 1 — features)

Guard `config` on your own flag. Carry your own dependencies (narrowest scope
wins). Read no other module.

```nix
# modules/dendrites/firefox.nix
{ config, lib, ... }:
{
  options.aoide.firefox.enable = lib.mkEnableOption "firefox";
  config = lib.mkIf config.aoide.firefox.enable {
    programs.firefox.enable = true;
  };
}
```

Enable it with one line in `hosts/yomi-strix/default.nix`:
`aoide.firefox.enable = true;`. `hosts/` knows dendrites; dendrites never know
hosts.

---

## Authoring a facet (Wave 1 — render surfaces)

A facet renders appearance. It reads **only** `aoide.tokens`, and if it owns a
surface it declares that in `aoide.surfaces`. Apply component fallbacks yourself.

```nix
# modules/facets/quickshell/default.nix
{ config, lib, ... }:
let
  t = config.aoide.tokens;
  # component-tier fallback: null → palette (see CONTRACTS.md §1)
  barBg = if t.bar.bg != null then t.bar.bg else t.palette.bg;
in
{
  config = lib.mkIf config.aoide.enable {
    # Declare surface ownership — Stylix reads this and stands down for `bar`.
    aoide.surfaces.bar.owner = "quickshell";
    # … render the bar using barBg / t.palette.* …
  };
}
```

The `checks.surface-ownership` assertion fails eval if a declared surface has no
owner; the Stylix facet must read `config.aoide.surfaces` and disable its own
derivation for any surface already owned (overlap resolution,
`concepts/Design-Tokens`).

**Never read `song/` runtime paths at build time.** `checks.no-song-read` fails
eval if a module under a `song/` runtime dir is discovered. `stage/` is live
state, never load-bearing for the nix build.

---

## Package handoffs — exactly what each agent drops in

Both packages are `callPackage ./pkgs/<name> { }` in `flake.nix`. They are
Wave-0 placeholders (trivial `runCommand` derivations that build green). Replace
the `default.nix` **in place** — keep the file path and the `callPackage`
signature so `flake.nix` never changes.

### Agent A — token package (`pkgs/tokens/`) — Node / Style Dictionary

Provide:

- `pkgs/tokens/default.nix` — a `callPackage`-able derivation
  (`buildNpmPackage { pname = "aoide-tokens"; … }`). Add `nodejs` /
  `style-dictionary` as build inputs there; do **not** edit `flake.nix`.
- `pkgs/tokens/package.json`, lockfile, and source — the resolver (tiered:
  palette → semantic → component), the `rice lint` schema validator, and the
  three live-side emitters:
  - `song/stage/tokens.json` (Quickshell; atomic write — see `CONTRACTS.md §4`),
  - `hyprctl` dispatcher (compositor properties),
  - terminal OSC sequences (color injection).
- Wrap Style Dictionary and the W3C design-tokens format; do not reimplement a
  resolver. Build against **token schema v0** (`CONTRACTS.md §1`): palette is
  base16-closed; component tier is `bar.*` / `notif.*` / `window.*`.

### Agent B — CLI + daemon (`pkgs/aoide/`) — Rust

Provide:

- `pkgs/aoide/default.nix` — `rustPlatform.buildRustPackage { pname = "aoide"; … }`,
  `callPackage`-able. Must install a binary named `aoide` (`meta.mainProgram`).
- `pkgs/aoide/Cargo.toml`, `Cargo.lock`, `src/` — the CLI trunk + `aoided`.
- Implement the CLI contract: `aoide guide`, `aoide schema --json`
  (`CONTRACTS.md §3`), `--json` on every command, structured exit codes, the
  rice loop (`gen`/`lint`/`preview`/`adopt`), and `aoide mcp serve --stdio`
  (façade generated from the schema). Policy, lint, and the single audit log
  (`aoide.auditLog`) live in `aoided`; the rebuild is user-gated.

### Wave-1 facet/module agents (C, D)

- Add files only under `modules/facets/` and `modules/dendrites/`
  (+ `modules/rime/` for the rice engine and shipped default rices).
- Flip flags in `hosts/yomi-strix/default.nix` (one line each).
- Never edit `flake.nix` or `lib/`. If you need a new flake input, that is a
  Wave-0 change — request it; do not add it yourself.

---

## Checks you must keep green

`lib/checks.nix` rides as flake `checks`:

- `surface-ownership` — every `aoide.surfaces.<name>` names a non-empty owner.
- `no-song-read` — no discovered module lives under a `song/` runtime dir.
- `pkg-aoide`, `pkg-aoide-tokens` — the two packages build (exercises the
  packaging contract).

Run `nix flake check` before every commit.
