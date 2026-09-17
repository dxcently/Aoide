{
  description = "conductor-state kit — one shell for every runner: python (torch, transformers) for jevlike and the RLCD-style Qwen scorer, a rust toolchain for verba-volantia (CPU), jq for the cases and the report";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      py = pkgs.python3.withPackages (p: [ p.torch p.numpy p.transformers p.safetensors ]);
    in {
      devShells.${system} = {
        default = pkgs.mkShell { packages = [ py pkgs.jq pkgs.cargo pkgs.rustc pkgs.gcc ]; };
        # the python half alone — enough for run-jevlike.sh and run-rlcd.sh
        python = pkgs.mkShell { packages = [ py pkgs.jq ]; };
      };
    };
}
