{
  description = "Aoide tracks and conducts terminal and agent sessions — collaborating across agents and hosts, human in the loop.";

  # ── Inputs ─────────────────────────────────────────────────────────────────
  # Pre-declared for all waves. Later waves ADD files in their own dirs and
  # never touch this file. Prefer nixpkgs versions of tools where possible;
  # these inputs are wired but the eval is kept robust to empty layers.
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    stylix = {
      url = "github:danth/stylix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    quickshell = {
      url = "github:quickshell-mirror/quickshell";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    hyprland.url = "github:hyprwm/Hyprland";

    # nvf (Neovim-Flake) — the neovim dendrite's config framework (dxflake
    # form, verbatim). Inputs can only live here; the dendrite reaches it via
    # specialArgs (lib/mkHost.nix threads `inputs` into home-manager too).
    nvf = {
      url = "github:notashelf/nvf";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # The aoide core (CLI + daemon) as a self-flaked package — topology (b) of
    # AOIDE-DEV §7: pkgs/aoide owns a nixpkgs-only flake of its own and is
    # consumed here as a path input, NOT discovered by the packages walker
    # (lib/pkgs.nix skips any package dir carrying its own flake.nix).
    aoide = {
      url = "path:./pkgs/aoide";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  # ── Outputs ────────────────────────────────────────────────────────────────
  # All outputs the later waves need are pre-declared here and are tolerant of
  # the currently-empty layers. Plain (non-flake-parts) style, matching the
  # dxflake prior art the walker adopts.
  outputs =
    { self, nixpkgs, ... }@inputs:
    let
      systems = [ "x86_64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems f;
      inherit (nixpkgs) lib;

      # Per-host assembly (walker + host + home-manager + stylix).
      mkHost = import ./lib/mkHost.nix {
        inherit inputs lib;
        system = "x86_64-linux";
        username = "khoa";
      };

      # The walker, for checks that reason over the module tree.
      walk = import ./lib/walk.nix { inherit lib; };

      # The packages walker — auto-discovers pkgs/<name>/default.nix. One source
      # feeds the `packages` output, the auto-generated `pkg-<name>` checks, and
      # the host + vm overlays (lib/mkHost.nix, lib/vmTest.nix).
      pkgsWalk = import ./lib/pkgs.nix { inherit lib; };
    in
    {
      # ── NixOS configurations ───────────────────────────────────────────────
      # One line per host. Shelved skeletons live at hosts/_{desktop,laptop,
      # server,mac} — copy one to hosts/<name>/, register it here, done (the
      # `_mac` one is forward-looking: it needs the darwin class seam in
      # lib/mkHost.nix first). yomi-strix remains the living reference host.
      nixosConfigurations = {
        yomi-strix = mkHost "yomi-strix";
      };

      # ── Packages ───────────────────────────────────────────────────────────
      # Auto-discovered by lib/pkgs.nix: every `pkgs/<name>/default.nix` (not
      # `_`-shelved, and without its own flake.nix) self-registers here as
      # `callPackage ./pkgs/<name>`. Adding a package is one new folder — this
      # file never changes. `aoide` is the exception: it is self-flaked
      # (pkgs/aoide/flake.nix) and skipped by the walker — it arrives as the
      # `aoide` input, and `default` = that input's package.
      # See docs/BUILD.md and CONTRACTS.md §2.
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          discovered = pkgsWalk.discover pkgs;
          aoide = inputs.aoide.packages.${system}.default;
        in
        discovered
        // {
          inherit aoide;
          default = aoide;
        }
      );

      # ── Songbook manifest/registry (C4/W3) ──────────────────────────────────
      # The SAME generator `modules/facets/quickshell/default.nix`'s
      # `quickshellConfig` derivation uses at build time
      # (`lib/songbook.nix`), exposed as a lean flake output so
      # `pkgs/aoide/crates/song/src/widgets.rs` can shell out to `nix eval
      # --json .#songbookManifest` at RUNTIME and regenerate
      # `manifest.json`/`registry.json` whole — the hot-sync half of `rice
      # stage`, no rebuild. Deliberately system-independent (`nixpkgs.lib` is
      # itself system-independent — no `legacyPackages.${system}` needed) and
      # deliberately NOT `self.nixosConfigurations.*` or anything under it:
      # evaluating this attribute must never force a host's full module
      # system (`config.aoide.*`), only `lib` + `song/songbook/` +
      # `lib/song.nix` (nix is lazy — the songs loop above is a SEPARATE
      # thunk this output shares no dependency with beyond `lib`).
      songbookManifest =
        let
          sb = import ./lib/songbook.nix { inherit lib; };
        in
        {
          manifest = sb.manifestAttrs;
          registry = sb.registryAttrs;
        };

      # ── aoide.* option derivation (P-I3, onboarding lane) ───────────────────
      # Every `aoide.*` option declared across modules/{nucleus,facets,
      # dendrites}, narrowed to {name, description, default} for `lyra
      # onboard`'s `aoide.nix` generator (docs/architecture/ONBOARD.md "The
      # vars-file generator"; lib/options.nix does the evalModules walk).
      # Deliberately NOT per-system like `packages`/`checks` below —
      # `songbookManifest` above sets the precedent (system-independent where
      # nothing system-specific is needed) and this repo targets
      # `x86_64-linux` only (`systems` above), so lyra shells a single static
      # flake ref (`nix eval --json <checkout>#aoideOptions`) with no system
      # attrpath to get right.
      aoideOptions = import ./lib/options.nix {
        inherit lib inputs;
        pkgs = nixpkgs.legacyPackages.x86_64-linux;
      };

      # ── Checks ─────────────────────────────────────────────────────────────
      # The contractual coupling discipline (lib/checks.nix). They pass
      # trivially now (no facets declare surface owners yet) and become real as
      # Wave-1 facets populate `aoide.surfaces`. Also builds EVERY package as
      # `pkg-<name>` — one per DISCOVERED package (lib/pkgs.nix; whatever
      # currently lives under pkgs/ — no fixed list here to go stale) plus
      # `pkg-aoide` explicit from the `aoide` input, since the core is
      # self-flaked — so `nix flake check` exercises the packaging contract
      # for the whole set with no coverage gap. `fmt` and `discovery` enforce
      # the formatter and the pkgs/ discovery rule respectively — see
      # lib/checks.nix's header.
      checks = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          checks = import ./lib/checks.nix { inherit lib pkgs; };
          hostCfg = self.nixosConfigurations.yomi-strix.config;
          # pkg-<name> per DISCOVERED package: the check IS the built package.
          # Reads pkgsWalk.discover directly (not self.packages) so the alias
          # `default` yields no redundant `pkg-default`. `aoide` is self-flaked
          # and skipped by the walker, so `pkg-aoide` is explicit — the input's
          # package, the same derivation `packages.aoide` and the overlays use.
          pkgChecks =
            lib.mapAttrs' (name: drv: lib.nameValuePair "pkg-${name}" drv) (pkgsWalk.discover pkgs)
            // {
              pkg-aoide = inputs.aoide.packages.${system}.default;
            };
        in
        pkgChecks
        // {
          surface-ownership = checks.surfaceOwnership (hostCfg.aoide.surfaces or { });
          no-song-read = checks.noSongRead (walk ./modules);
          # Committed songs self-register from song/songbook (walked into each
          # host by lib/mkHost.nix); song-shape asserts each is a rice.nix only.
          song-shape = checks.songShape (walk ./song/songbook);
          # Formatter enforcement — flake.nix:173 declares `formatter = nixfmt`;
          # nothing ran it before this check existed. Scoped to `self`, the
          # git-filtered committed tree (see lib/checks.nix's Check 4).
          fmt = checks.fmt self;
          # pkgs/ discovery completeness — see lib/pkgs.nix's `strayEntries`
          # and lib/checks.nix's Check 5.
          discovery = checks.discovery pkgsWalk.strayEntries;
          # No phantom commands in the agent docs — every backticked
          # `aoide …`/`lyra …` spelling in AGENTS.md + docs/agent/*.md
          # resolves against the binaries built from this source (see
          # lib/checks.nix's Check 6).
          phantom-commands = checks.phantomCommands self inputs.aoide.packages.${system}.default;
          # VM boot test — boots the Aoide desktop config headless and asserts
          # the stack comes up (multi-user.target, aoide on PATH,
          # greetd enabled, aoided + shellbridge user services active, graph
          # commands pass).  Requires KVM on the build host.
          vm-boot = import ./lib/vmTest.nix { inherit pkgs inputs lib; };
        }
      );

      # ── Dev shell ──────────────────────────────────────────────────────────
      # Rust (cargo/rustc) + nix tools. This is the build
      # surface the CLI/daemon agents develop in.
      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            name = "aoide-dev";
            packages = with pkgs; [
              # Rust — aoide CLI + aoided daemon
              cargo
              rustc
              rustfmt
              clippy
              rust-analyzer
              # Nix tooling
              nixfmt
              nil
              deadnix
              statix
            ];
          };
        }
      );

      # ── Formatter ──────────────────────────────────────────────────────────
      formatter = forAllSystems (system: nixpkgs.legacyPackages.${system}.nixfmt);
    };
}
