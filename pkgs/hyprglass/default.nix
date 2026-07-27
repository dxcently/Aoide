# pkgs/hyprglass/default.nix — Liquid Glass decoration plugin for Hyprland.
#
# https://github.com/hyprnux/hyprglass — layered glass effects (gaussian
# blur, edge refraction, fresnel glow, adaptive brightness) on translucent
# windows AND layer surfaces. Aoide points it at the quickshell surfaces
# (aoide-bar / aoide-dock namespaces) from the compositor facet: the "gloss"
# layer of the rice's Aero glass, on top of Hyprland's own blur.
#
# Version discipline: a Hyprland plugin is ABI-locked to the compositor
# commit it was built against. hyprglass ships an explicit pin table
# (hyprpm.toml): hyprland 0.56.0 (36b2e0cf) → hyprglass v0.7.0 (c96940a).
# This derivation pins that vetted pair; bump BOTH together when nixpkgs'
# hyprland moves (check hyprpm.toml upstream for the new mapping).
#
# Discovered by lib/pkgs.nix (the packages walker) → flake package
# `hyprglass` + host overlay attr `pkgs.hyprglass` (no nixpkgs collision).
{
  lib,
  fetchFromGitHub,
  hyprlandPlugins,
}:
hyprlandPlugins.mkHyprlandPlugin {
  pluginName = "hyprglass";
  version = "0.7.0";

  src = fetchFromGitHub {
    owner = "hyprnux";
    repo = "hyprglass";
    rev = "c96940a86e6c5c9290dacb9fde204e4172186a96";
    hash = "sha256-u+Rk8l7oidgxQsVWOYuQCmnvGqRpuU07k8wIk/0PTyE=";
  };

  # Upstream Makefile: pkg-config hyprland/pixman/libdrm (provided by
  # mkHyprlandPlugin's dep set), emits hyprglass.so at the repo root.
  buildPhase = ''
    runHook preBuild
    make -j"$NIX_BUILD_CORES"
    runHook postBuild
  '';

  installPhase = ''
    runHook preInstall
    install -Dm644 hyprglass.so "$out/lib/libhyprglass.so"
    runHook postInstall
  '';

  meta = {
    description = "Liquid Glass decoration effects for Hyprland windows and layer surfaces";
    homepage = "https://github.com/hyprnux/hyprglass";
    license = lib.licenses.bsd3;
    platforms = lib.platforms.linux;
  };
}
