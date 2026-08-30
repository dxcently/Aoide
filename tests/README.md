# tests/ — non-cargo testing

Test content that isn't cargo's: whole-system and whole-artifact checks that
need a Nix build to even exist. `lib/` holds build/eval machinery
(`checks.nix`, `mkHost.nix`, `pkgs.nix`, `walk.nix`); this directory holds
what those checks actually test.

- `vm-boot.nix` — headless NixOS boot test, wired as `checks.<system>.vm-boot`.
- `portability.nix` — asserts `packages.aoide-static` is genuinely free of
  nix, wired as `checks.<system>.portability` (`lib/checks.nix` Check 8).
- `distrobox.md` — manual container-based portability suite; not a gate.

A top-level `tests/` is not a Rust convention — this is not where `cargo
test` looks. It is the NixOS one (`nixos/tests/`), extended here to any
non-cargo test.

## Why Rust tests do NOT live here

`#[cfg(test)]` unit tests stay in-source: they need access to private
functions, so moving them out breaks what they test. `crates/<name>/tests/`
is cargo's own convention for integration tests — it's where `cargo test -p
<crate>` looks, and that per-crate scoping is load-bearing here: `cargo test
--workspace` deadlocks (the conduct crate binds real sockets), so every Rust
test in this repo is run crate-scoped, always. Consolidating Rust tests into
this directory would break the only test invocation that works.
