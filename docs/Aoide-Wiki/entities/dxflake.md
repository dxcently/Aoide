---
type: entity
created: 2026-07-25
updated: 2026-07-28
tags: [aoide, nix, flake, dxflake, auto-discovery]
source: "[[references/AOIDE-HANDOFF]]"
---

# dxflake

A dendritic auto-discovery NixOS flake at github.com/dxcently/dxflake. It serves as both prior art and Aoide's adoption target for the nucleus + dendrite import pattern.

**Walker mechanism.** `flake.nix` defines `mkHost`, which passes every `.nix` file under `modules/` to every host via `nixpkgs.lib.filesystem.listFilesRecursive`. A filter drops any path containing `/_`, which is the shelving opt-out: prefix a filename with `_` to hide it from discovery without deleting it. There is no imports list to maintain.

**Gating.** Discovery imports a file; gating decides whether it activates. `modules/nucleus/` contains no `mkIf` guard — it applies unconditionally on every host (boot, users, network, ssh, sops, tailscale, base packages). `modules/dendrites/` files wrap their `config` in `lib.mkIf` on either a per-feature flag (`dx.<name>.enable`) or a role flag from `modules/aggregations.nix`. The aggregation roles are: `dx.aggregations.{desktop,hyprland,gaming,server}`.

**Host configuration.** A host's `default.nix` imports only its `./hardware.nix` and then flips `dx.*` flags. It never imports module files directly. The separation is strict: hosts know dendrites; dendrites never know hosts.

`yomi-strix` — Aoide's own host — derives from dxflake's own `yomi-strix`, trimmed to essentials (`hosts/yomi-strix/`); it builds under Aoide's *own* flake (`flake.nix`'s `mkHost`), not dxflake's. Aoide's `lib/mkHost.nix` reuses the same walker pattern; see [[Snowflake-Anatomy]] for how Aoide names the equivalent layers (nucleus, dendrites, facets).

## Related

- [[Snowflake-Anatomy]]
- [[Fork-and-Run]]
