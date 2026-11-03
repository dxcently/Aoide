# tests/boundary — where Aoide and Strillix name each other

Aoide (core: `aoide`, `aoided`, `aoide-desktop`) and Strillix (the OS: the
modules, the paint, `strillix`) share this repo and are kept from naming each
other. `checks.<system>.boundary` counts every place one names the other and
holds the count against `crossings.golden`, the crossings that stand. A
crossing the golden lacks fails the build. A crossing the golden holds that is
gone fails it too, until the golden is tightened, so each step of progress
stays locked in. `lib/checks.nix` (Check 14) only invokes the scan; the scan
and the golden live here.

A third side, the neutral crates (`pkgs/aoide/crates/ground/`), is held to the
same line from both ends: it names neither product and depends on no crate of
either side. The golden holds no row for it, so any row a neutral crate would
need is a failure to fix, never a crossing to list. An edge into a neutral
crate is free.

- `scan.pl` — `check` and `tighten`. Its header holds the whole classification.
- `crossings.golden` — the rows, sorted, one per line.

```
perl tests/boundary/scan.pl check   . tests/boundary/crossings.golden
perl tests/boundary/scan.pl tighten . tests/boundary/crossings.golden
nix build .#checks.x86_64-linux.boundary
```

## Sides

A file's side is decided by its path and nothing else. Aoide is `pkgs/aoide/**`
without the `ground`, `strillix`, `strillix-rice` and `screen` crates,
`pkgs/aoide-desktop/**`, and the nine core files of `modules/dendrites/`
(`aoided.nix`, the `aoide-*` ones, `melete*.nix`, `mneme.nix`, `tailscale.nix`).
Strillix is those three crates, `pkgs/strillix-shell/**`,
`pkgs/strillix-rices/**`, the rest of `modules/`, six files of `lib/` and
`rices/**`. The neutral side is `pkgs/aoide/crates/ground/`. Everything else —
`flake.nix`, the other `lib/` files, `tests/`, `docs/`, `hosts/`, `users/` — is
unscanned. On every side `*.md` files, binary files and symlinks are skipped,
and the files counted are the tracked ones, the files a flake build sees.

## Rows

```
name <side> <path> <count>
dep <from-crate> <to-crate> <section>
```

A `name` row says `<count>` lines of `<path>` name the other product: an
Aoide-side file names Strillix on a line matching `/strillix|\bstrx\b/i`, a
Strillix-side file names Aoide on a line matching `/aoide/i`, a neutral-side
file names either on a line matching `/aoide|strillix|\bstrx\b/i`, and a
full-line comment does not count. A `dep` row is a Cargo dependency edge from a
crate on one side to a crate whose directory is on the other, or from a neutral
crate to a crate on either side; an edge into a neutral crate has no row. Edges
are read from `[dependencies]`, `[dev-dependencies]` and `[build-dependencies]`
and their target-specific forms. Crates are named by their `[package]` names.

## Reading a failure

- **An unlisted crossing.** The rows are printed exactly as the golden would
  hold them: a row the golden lacks, or one whose count is higher. Take the
  crossing out. If it is deliberate, add the row to the golden by hand and
  give the reason in the commit message; `tighten` never adds rows.
- **A neutral row.** `name neutral <path>`, or a `dep` row whose
  `<from-crate>` is a neutral crate. Take the product's name or the dependency
  out of the neutral crate (a test fixture gets a neutral word, a test helper
  is written in the crate). Never add the row.
- **A stale golden.** A golden row's crossing is gone, or fewer lines name the
  other product than before. The check prints the complete tightened golden,
  ready to paste, and only when no crossing is unlisted; run `tighten`
  instead of pasting.
- **A line the scan cannot read.** A dependency line in a `Cargo.toml` that
  the scan's regexes do not parse (anything but a version string, an inline
  `{ ... }` table, a dotted key or a `[dependencies.<name>]` table) fails the
  scan naming the line. Rewrite the line, or teach `scan.pl` the form.
- **No golden.** `check` fails; `tighten` writes the first one.

## Lowering the golden

A lane that removes a crossing runs `tighten` from the repo root and commits
the smaller golden in the same commit as the change. `tighten` refuses, and
writes nothing, while any crossing is unlisted, so it only ever shrinks the
golden. A file that moves carries its crossings by hand: add the rows at the
path it moves to, and `tighten` drops the old ones. When two lanes' goldens
conflict at a merge, take either side and run `tighten`: it recomputes from the
merged tree.

## Limits

The scan counts lines, not meaning: trading one crossing line for another in
the same file keeps the count. A Rust line that opens with `*` (a dereference)
reads as a comment. Comments and `*.md` files are not counted on any side, so a
neutral crate's prose stays free of the two names by review, not by this scan.
