# pkgs/hyprglass/default.nix — Liquid Glass decoration plugin for Hyprland.
#
# https://github.com/hyprnux/hyprglass — layered glass effects (gaussian
# blur, edge refraction, fresnel glow, adaptive brightness) on translucent
# windows AND layer surfaces. Aoide points it at the quickshell surfaces
# (aoide-bar / aoide-dock namespaces) from the compositor facet: the "gloss"
# layer of the rice's Aero glass, on top of Hyprland's own blur.
#
# Version discipline: a Hyprland plugin is ABI-locked to the compositor
# commit it was built against. mkHyprlandPlugin does not pin that pairing —
# it builds hyprglass against whatever hyprland nixpkgs ships (currently
# 0.56.2), not the version hyprglass's own hyprpm.toml pin table names; the
# ABI handshake at plugin load is what actually gates compatibility, and it
# passes because both are built from the same flake input. Bump hyprglass's
# rev/hash when upstream moves; nixpkgs' hyprland moves independently of it.
#
# The pin sits above the v0.7.0 tag rather than on it, because the rotated-
# monitor layer-FBO fix (upstream PR #66) lands after it. Aoide carried that
# fix as a local patch until upstream merged the same change: the layer temp
# FBO is sized from the source framebuffer's own extents instead of the
# monitor's m_transformedSize, so the buffer matches the native frame
# Hyprland actually writes into. Both faces of the defect move together —
# the allocation with its clear-box intersection (GlassLayerSurface.cpp:178,
# :192, :201) and the mask UV divisors (:249) — which is what separates it
# from the source-rectangle-only shape that turns the smear into a hard
# vertical cliff. Identity on transform 0 and 180 and their FLIPPED
# variants, where m_pixelSize == m_transformedSize. The full account, with
# the four wrong diagnoses it took to get there, is
# docs/Aoide-Wiki/references/Glass-Stretch-on-a-Rotated-Monitor.md.
#
# This rev also carries upstream's own hyprland-0.56.2 compatibility bump,
# which is the version nixpkgs builds it against here.
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
  version = "0.7.0-unstable-2026-09-03";

  src = fetchFromGitHub {
    owner = "hyprnux";
    repo = "hyprglass";
    rev = "ee6419bd023529187875db98af4e68b3e93355a8";
    hash = "sha256-OnPDyehgqwrZWHZ0YBjZ2H5G+CFI0/MDK2I+2DH70ZY=";
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
