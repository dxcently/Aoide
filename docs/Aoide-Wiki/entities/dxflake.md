---
type: entity
created: 2026-07-25
updated: 2026-08-27
tags: [aoide, nix, flake, dxflake, auto-discovery]
source: "[[references/AOIDE-HANDOFF]]"
---

# dxflake

A dendritic auto-discovery NixOS flake at github.com/dxcently/dxflake. It serves as both prior art and Aoide's adoption target for the nucleus + dendrite import pattern.

**Walker mechanism.** `flake.nix` defines `mkHost`, which passes every `.nix` file under `modules/` to every host via `nixpkgs.lib.filesystem.listFilesRecursive`. A filter drops any path containing `/_`, which is the shelving opt-out: prefix a filename with `_` to hide it from discovery without deleting it. There is no imports list to maintain.

**Gating.** Discovery imports a file; gating decides whether it activates. `modules/nucleus/` contains no `mkIf` guard — it applies unconditionally on every host (boot, users, network, ssh, sops, tailscale, base packages). `modules/dendrites/` files wrap their `config` in `lib.mkIf` on either a per-feature flag (`dx.<name>.enable`) or a role flag from `modules/aggregations.nix`. The aggregation roles are: `dx.aggregations.{desktop,hyprland,gaming,server}`.

**Host configuration.** A host's `default.nix` imports only its `./hardware.nix` and then flips `dx.*` flags. It never imports module files directly. The separation is strict: hosts know dendrites; dendrites never know hosts.

`yomi-strix` — Aoide's own host — derives from dxflake's own `yomi-strix`, trimmed to essentials (`hosts/yomi-strix/`); it builds under Aoide's *own* flake, not dxflake's. Aoide keeps dxflake's gating shape (a core with no guard, opt-in branches that gate themselves) but names its modules instead of walking them: see [[Snowflake-Anatomy]] for the four directories and the catalogue, and what a paint lane may read.

**chiyo, the AoideOS carrier.** dxflake's host `chiyo` runs Aoide's full paint stack on top of dxflake; Aoide's own tree carries no `hosts/chiyo`. dxflake consumes Aoide as a flake input (`git+file:///home/khoa/Aoide`, rev-pinned to a committed HEAD) and walks the input's `modules/` and `song/songbook/` into every host's module list, so chiyo flips the `aoide.*` options directly — **as of dxflake's pin (`e5b460e`) these are** `aoide.song = "sonata"`, the three paint facets (`aoide.facets.{quickshell,compositor,stylix}.enable`), `aoide.lyra.enable`, and the dendrites `aoide.{hyprland,clipboard,screenshot,dunst}.enable`. The Aoide core (binaries, aoided, A2A) enters through dxflake's own `dx.aoide.enable` dendrite. dxflake keeps what does not collide — `./hardware.nix`, `ly` login and pipewire via `dx.aggregations.desktop` (with `services.greetd.enable = lib.mkForce false` against the shell lane's stub) — and turns off `dx.aggregations.hyprland` and `dx.stylix.enable` so only one stack writes the leaf options the two stacks share. The `aoide.facets.*` option names are dxflake's PIN's vocabulary, retired in Aoide's own tree; a later dxflake rev migrates onto the export surface ([[Package-Layout]]'s "AoideOS's export surface").

## Related

- [[Snowflake-Anatomy]]
- [[Clone-and-Run]]
