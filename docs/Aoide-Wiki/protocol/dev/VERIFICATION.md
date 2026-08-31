---
type: reference
created: 2026-08-30
updated: 2026-08-30
tags: [aoide, verification, testing, nix, git, protocol]
---

# Verification and Repo Discipline

What counts as proof, and how work lands. Harness-agnostic. Dependency of
[[DEV]].

---

## Build, load, show

```
# eval + build (validates all nix)
nix build .#nixosConfigurations.<host>.config.system.build.toplevel --no-link

# activate — THE USER'S GATE. Only when they have admitted it.
sudo nix-env -p /nix/var/nix/profiles/system --set <toplevel>
sudo <toplevel>/bin/switch-to-configuration switch

# desktop-only reloads — no switch needed
hyprctl reload                                     # compositor rules
systemctl --user restart aoide-quickshell.service  # bar / dock / gadgets
lyra rice stage <song>                             # livery hot-reload
qs -p modules/facets/quickshell/qml/shell.qml      # QML parse check

# show — capture through lyra, never bare grim
lyra screen shot                                   # full layout + JSON sidecar
lyra screen shot --region "0,0 1080x36"            # one rect
lyra screen shot --session <id>                    # a conducted session's window
lyra screen info                                   # monitors, clients, layer surfaces
```

- **The switch is the User's gate.** Propose and prepare the build; they
  admit it. **Never hold or use a password, even when handed one.** Say the
  gate exists once, then move on.
- **Smallest reload that proves the change.** QML → restart quickshell;
  compositor rule → `hyprctl reload`; anything nix-baked (Stylix, base16,
  terminal config) → needs a rebuild. Staged livery values hot-reload.
- **Staging can be locked.** `rice stage` refuses while the rice mode
  reports `declarative`. Unlock with `lyra rice mode stage [<song>]`, lock
  back with `lyra rice mode declarative [<song>]`.
- **Never hand-edit** `song/stage/livery.json` or the draft targets it
  symlinks to — that bypasses the lock check and the draft routing.
- **Never declare a visual change done without looking at it.** Capture it,
  read it back, judge it yourself, then show the User. `lyra screen shot` is
  the capture path, not bare `grim`: it writes a JSON sidecar beside every
  image — origin, scale, and the whole desktop at that instant (cursor, every
  client and layer surface) — so a coordinate read off the image inverts back
  to screen space through `screen point --from-shot`, and `--session <id>`
  frames a conducted agent's own window. Aoide is the orchestration core;
  shelling past its own surface leaves that surface unproven.

---

## The gates

| gate | covers |
|---|---|
| `nix flake check` | the committed tree — formatting, lint, coupling, packages, boot, portability |
| `aoide soundcheck` | the working tree — repo slop, orphans, build clutter; report-only, never repairs |
| `cargo test -p <crate>` | the crate you touched |

**`cargo` exists only inside the devshell.** It is on no system or user
profile path, so a bare `cargo test` reports `No such file or directory` and
reads like a broken toolchain:

```
nix develop --command bash -c 'cd pkgs/aoide && TMPDIR=/tmp cargo test -p <crate>'
```

**Flake checks see unstaged edits to TRACKED files, and are blind to
UNTRACKED ones.** A new `.nix` file is invisible to the whole check suite
until it is `git add`ed, so a green on new work means nothing until
`git status` is clean of untracked sources. This is the single easiest way
to certify a change that was never examined.

**A gate nobody runs is decoration.** A check can sit red for days without
anyone noticing. If a gate is not wired into something that fires, treat it
as absent.

---

## Proofs

**Prove a refactor inert by derivation identity.** Compare
`nixosConfigurations.<host>.config.system.build.toplevel.drvPath` before
and after, evaluating the "before" in a detached worktree rather than by
mutating the live one.

- **Pass the worktree as a path literal, never an interpolated string.** A
  string leaks the temp path into the derivation and manufactures a fake
  difference.
- **Some files can never pass this test.** A package whose `src` is its own
  directory (`lib.cleanSource ./.`) contains its own recipe, so any byte
  edit to that recipe — a comment typo included — moves the hash. That is a
  property, not a defect. Bisect to attribute the delta before believing a
  red result.

**Prove a gate can go red.** Plant a violation in a throwaway worktree and
watch it fail, naming file and line. A gate never seen failing is theatre.

**Prove a config file reaches the sandbox.** A linter's config discovered
from the working directory is not necessarily inside the build's `src`.
Diff the derivation's own source path to confirm.

**Read `PoisonError` as a cascade, not a cause.** Find the first failure.

---

## Never run

- **`cargo fmt` / `rustfmt`.** No Rust format gate exists here, HEAD is
  deliberately not format-clean, and a run manufactures hundreds of churn
  lines. (`nixfmt` on `.nix` files *is* required — a check enforces it.)
- **`cargo test --workspace`.** It deadlocks: the conduct crate binds real
  sockets. Scope to the crates you changed, with `TMPDIR=/tmp`.
- **`git add -A`, `git reset`, `git checkout`, `git restore`, `git stash`**
  in the shared worktree. Other sessions hold the same index.

---

## Before landing

- The gates above are green for what you touched.
- You read the diff yourself. An agent's report is evidence, never the
  verdict.
- New files are tracked, or the checks never saw them.
- Docs for the directory you changed are in the same commit.

Branching, commit scope, pathspec discipline, and push cadence live in
[[CRAFT]].
