{
  description = "Aoide — a dendritic auto-discovery NixOS framework: an agent-wearable desktop you fork and run.";

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
  };

  # ── Outputs ────────────────────────────────────────────────────────────────
  # All outputs the later waves need are pre-declared here and are tolerant of
  # the currently-empty layers. Plain (non-flake-parts) style, matching the
  # dxflake prior art the walker adopts.
  outputs =
    { self, nixpkgs, ... }@inputs:
    let
      systems = [ "x86_64-linux" ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
      lib = nixpkgs.lib;

      # Per-host assembly (walker + host + home-manager + stylix).
      mkHost = import ./lib/mkHost.nix {
        inherit inputs lib;
        system = "x86_64-linux";
        username = "khoa";
      };

      # The walker, for checks that reason over the module tree.
      walk = import ./lib/walk.nix { inherit lib; };
    in
    {
      # ── NixOS configurations ───────────────────────────────────────────────
      nixosConfigurations = {
        yomi-strix = mkHost "yomi-strix";
      };

      # ── Packages ───────────────────────────────────────────────────────────
      # `callPackage ./pkgs/<name>` — both are Wave-0 placeholders that build
      # green today; Agents A & B replace their default.nix in place, so this
      # file never changes. See docs/BUILD.md.
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          aoide = pkgs.callPackage ./pkgs/aoide { };
          aoide-notes = pkgs.callPackage ./pkgs/notes { };
          # Melete AI harness + Mneme vault MCP server. Ported from dxflake;
          # both are launcher wrappers over runtime-deployed binaries (their
          # authenticated/out-of-band fetch is lifted to a runtime seam owned by
          # modules/dendrites/{melete,mneme}.nix). See pkgs/{melete,mneme}.
          melete = pkgs.callPackage ./pkgs/melete { };
          mneme = pkgs.callPackage ./pkgs/mneme { };
          default = self.packages.${system}.aoide;
        }
      );

      # ── Checks ─────────────────────────────────────────────────────────────
      # The contractual coupling discipline (lib/checks.nix). They pass
      # trivially now (no facets declare surface owners yet) and become real as
      # Wave-1 facets populate `aoide.surfaces`. Also builds the placeholder
      # packages so `nix flake check` exercises the packaging contract.
      checks = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          checks = import ./lib/checks.nix { inherit lib pkgs; };
          hostCfg = self.nixosConfigurations.yomi-strix.config;
        in
        {
          surface-ownership = checks.surfaceOwnership (hostCfg.aoide.surfaces or { });
          no-song-read = checks.noSongRead (walk ./modules);
          # Committed songs self-register from song/repertoire (walked into each
          # host by lib/mkHost.nix); song-shape asserts each is a rice.nix only.
          song-shape = checks.songShape (walk ./song/repertoire);
          pkg-aoide = self.packages.${system}.aoide;
          pkg-aoide-notes = self.packages.${system}.aoide-notes;
          # VM boot test — boots the Aoide desktop config headless and asserts
          # the stack comes up (multi-user.target, aoide + aoide-notes on PATH,
          # greetd enabled, aoided + shellbridge user services active, graph
          # commands pass).  Requires KVM on the build host.
          vm-boot = import ./lib/vmTest.nix { inherit pkgs inputs lib; };
        }
      );

      # ── Dev shell ──────────────────────────────────────────────────────────
      # Rust (cargo/rustc) + Node toolchains + nix tools. This is the build
      # surface Agents A (Node/notes) and B (Rust/CLI) develop in.
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
              # Node — notes package (wraps Style Dictionary)
              nodejs
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
