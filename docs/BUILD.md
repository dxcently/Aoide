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
- `modules/facets/` — render surfaces, read `aoide.livery` only.

**Shelving opt-out:** any path containing `/_` is skipped. Prefix a
work-in-progress file (`_wip.nix`) or dir (`_scratch/`) with `_` to hide it.

---

## The option contract (what you read)

Declared in `modules/nucleus/options.nix`, evaluated per host. Read these; write
none of them except your own dendrite/facet flags.

| Option                         | Type                         | Notes |
| ------------------------------ | ---------------------------- | ----- |
| `aoide.enable`                 | bool                         | framework master switch |
| `aoide.song`                   | str (default `"sonata"`)     | the song this host performs; names a `song/songbook/<name>/` (or the shipped standard) |
| `aoide.user`                   | str (default `"khoa"`)       | owner of the `~/Aoide` clone |
| `aoide.livery.palette.{bg,fg,accent,urgent}` | hex        | v0 palette (base16) |
| `aoide.livery.bar.{bg,fg,accent}` | nullOr hex                | component override; null → palette |
| `aoide.livery.notif.{bg,fg,urgent}` | nullOr hex              | component override; null → palette |
| `aoide.livery.window.{border,borderInactive}` | nullOr hex     | component override; null → palette |
| `aoide.surfaces.<name>.owner`  | str                          | surface-ownership registry |
| `aoide.mcp.enable`             | bool (default false)         | MCP façade toggle |
| `aoide.auditLog`               | str (default `/home/<user>/Aoide/log`) | single audit log |

See `CONTRACTS.md §1` for the full note schema and fallback rules.

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

A facet renders appearance. It reads **only** `aoide.livery`, and if it owns a
surface it declares that in `aoide.surfaces`. Apply component fallbacks yourself.

```nix
# modules/facets/quickshell/default.nix
{ config, lib, ... }:
let
  t = config.aoide.livery;
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
`concepts/Notes`).

**Never read `song/` runtime paths at build time.** `checks.no-song-read` fails
eval if a module under a `song/` runtime dir is discovered. `stage/` is live
state, never load-bearing for the nix build.

---

## Authoring a song (replayable rice)

A **song** is a committed, host-agnostic rice. Any host in the fleet performs it
by naming it — the score adapts to that host's specifics and its enabled
facet/dendrite set. **The venue (host) decides its instruments; the song
carries only the notes.**

Drop a folder under `song/songbook/<name>/` — `lib/mkHost.nix` walks it in
like a dendrite, so there is no import list to edit. The song's `rice.nix`
self-gates on `aoide.song`:

```nix
# song/songbook/moonlight/rice.nix
{ lib, config, ... }:
{
  config = lib.mkIf (config.aoide.song == "moonlight") {
    aoide.livery.palette = { bg = "#0b1021"; fg = "#c8d3f5"; accent = "#82aaff"; urgent = "#ff757f"; };
    # component tier (bar/notif/window) — null falls back to palette
  };
}
```

Replay it on any host with **one line** in `hosts/<host>/default.nix`:
`aoide.song = "moonlight";`. Naming no song performs song `"sonata"` — the
shipped standard (`song/songbook/sonata/rice.nix`).

**Host-agnostic rules (CONTRACTS.md §5):** a song sets ONLY `aoide.livery` (and,
later, cover/chime refs inside `song/`). It NEVER sets host options (monitors,
hardware, services) and NEVER enables facets/dendrites — those are the venue's.
Note values are literal nix; a song never reads `song/` runtime paths. The
`song/songbook/**` tree is versioned score (not a runtime dir), so walking it
does not violate `checks.no-song-read`. `checks.song-shape` asserts each walked
songbook path is a `rice.nix`.

---

## Package handoffs — exactly what each agent drops in

`pkgs/` is walked, exactly like `modules/` and `song/songbook/`. Drop
`pkgs/<name>/default.nix` (a `callPackage`-able derivation, standard nixpkgs
args) and `lib/pkgs.nix` self-registers it into the flake `packages` output, the
host + vm overlays, and a `pkg-<name>` check — all from one source. **Adding a
package is one file; the "only Wave 0 edits `flake.nix` and `lib/`" promise now
HOLDS for packages too** (it did not before — the four packages used to be
hand-listed in `flake.nix` and duplicated in both `lib/` overlays).

`_`-prefix a package dir to shelve it (same as the module walker); a name must
not shadow a nixpkgs attribute (the overlay guard `throw`s on an accidental
clash — a deliberate shadow is an `intentionalShadows` exemption in
`lib/pkgs.nix`); a non-standard build arg goes through `lib/pkgs.nix`'s
documented `//` escape hatch. See `CONTRACTS.md §2`.

The current packages replace their `default.nix` **in place** — keep the
file path and the `callPackage` signature; the walker never needs the file list.

### Agent A — the livery engine (native, `crates/song/src/livery/`)

The standalone Node note engine was folded into the song crate by the livery
merge (LIVERY-MERGE.md); it is now native Rust, one module
tree inside `crates/song`:

- `livery/schema.rs` — the authoritative v0 validator (the `rice lint` schema;
  ported from `schema.js` with the exact error strings preserved).
- `livery/resolve.rs` — the flat resolver: `{group.key}` alias deref
  (cycle-guarded) + the component null → palette fallback.
- `livery/emit/{stage,hyprctl,osc,file}.rs` — the four pure emitters behind
  one `Emitter` trait + registry; a new backend is one file + one registry
  line. The stage backend writes `song/stage/livery.json` (Quickshell; atomic
  write — see `CONTRACTS.md §4`).
- Commands: `lyra livery lint|resolve|emit <target>` (the original standalone
  CLI's surface, native); `lyra rice lint` calls `livery::lint` directly — no
  binary locate, no PATH shell-out.

Build against **note schema v0** (`CONTRACTS.md §1`): palette is
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

- Add files only under `modules/facets/` and `modules/dendrites/`.
- Flip flags in `hosts/yomi-strix/default.nix` (one line each).
- Never edit `flake.nix` or `lib/`. If you need a new flake input, that is a
  Wave-0 change — request it; do not add it yourself.

---

## Bare `graph`, `project`, and `session` — projects + sessions as a DAG

The Terminal-Commander roster, upgraded from a flat list to a DAG: registered
**projects** are anchor nodes, agent **sessions** hang under them (anchored by
cwd — longest path-prefix wins, so nested projects anchor correctly), and
sessions nest under the session that spawned them (`parentSessionId`,
CONTRACTS.md §4). Sessions matching no project group under a synthetic
`(unanchored)` root. All state lives in `song/stage/` (gitignored runtime;
atomic writes only); missing stage files read as empty registries.

| Command                            | Does |
| ---------------------------------- | ---- |
| `aoide graph [--focus <id>]`       | Unicode tree render (`◆` projects, `●` sessions, `▶` marks the focused node); `--json` emits the graph document |
| `aoide project add <name> [<path>]` | register/update an anchor root in `song/stage/projects.json` (idempotent); `path` defaults to the cwd, so a session started there anchors to it |
| `aoide project remove <name>`      | unregister (ok + no-op if absent) |
| `aoide project list`               | list the registered anchor roots |
| `aoide graph link <child> <parent>`| set the spawned-by edge on the child session (rejects self-links and cycles; a not-yet-registered parent is recorded with a warning) |
| `aoide session prune`              | drop `done` sessions + their hook records (orphaned children keep running with their `parentSessionId` cleared); the blessed manual resync — restages `song/stage/graph.json` for Quickshell hot-reload the same as every other mutating graph command |

Session live state is the latest hook phase from `hooks.json` when present
(falling back to the roster `state`). Like every command, the group takes and
emits `--json` with structured errors and reports exactly what changed. Stage
shapes (`projects.json`, `graph.json`, the additive `parentSessionId` field on
session records) are contract §4.

---

## Checks you must keep green

`lib/checks.nix` rides as flake `checks`:

- `surface-ownership` — every `aoide.surfaces.<name>` names a non-empty owner.
- `no-song-read` — no discovered module lives under a `song/` **runtime** dir
  (`song/songbook/**` is versioned score, legitimately walked).
- `song-shape` — every walked `song/songbook/**` path is a `rice.nix`
  (host-agnostic song discipline; CONTRACTS.md §5).
- `phantom-commands` — every backticked `aoide …`/`lyra …` spelling in
  `AGENTS.md` + `docs/agent/*.md` resolves against the binaries' `schema
  --json`, built from the checked-out source (no doc may teach a command the
  registry no longer carries).
- `pkg-<name>` — one auto-generated check per discovered package builds it
  (currently `pkg-aoide`, `pkg-melete`, `pkg-mneme`). Generated
  by `lib/pkgs.nix`, so a new `pkgs/<name>/` gains its check with no edit here.

Run `nix flake check` before every commit.
