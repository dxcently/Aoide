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
# layer-temp-fbo-native-alloc.patch: Hyprland writes into any mid-pass
# framebuffer in native window coordinates — the viewport and
# outputProjection are both m_pixelSize — but the layer-surface temp FBO
# was allocated at m_transformedSize (the portrait-swapped logical frame).
# On a 90/270-transformed monitor every write past the swapped width was
# silently clipped by the buffer edge, in both sampleAndRedirect (the
# alloc and its clear-box intersection) and compositeAndRestore (the mask
# UV divisors). Allocating at m_pixelSize instead makes the buffer match
# the frame its content is actually written in. Window decorations don't
# hit this path (no mask/temp-FBO redirect), which is why only layer
# surfaces (dock/launcher/powermenu) showed it. Identity on transform 0 and
# 180 (and their FLIPPED variants), where m_pixelSize == m_transformedSize.
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

  patches = [ ./layer-temp-fbo-native-alloc.patch ];

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
