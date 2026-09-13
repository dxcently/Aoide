# pkgs/iconify-data/default.nix — pinned Iconify icon collections, data only.
#
# `design/p-icon-brief.md` §B/§C6: nixpkgs carries no `@iconify/json` package
# (`nix search nixpkgs iconify` is empty), so this is a hashed `fetchurl` of
# each collection's npm tarball — the same shape `pkgs/kimi-code/default.nix`
# already uses for a plain public download, no auth, no self-flaked input.
# `nix build` of THIS derivation is the only place in the whole `icon`
# workstream that ever touches the network (root AGENTS.md: `lyra icon
# resolve` itself reads only this derivation's `$out`, never a URL).
#
# Two collections, pinned exactly as `design/p-icon-brief.md` §B verified them
# (npm tarball SRI hash, decompressed `icons.json` byte count both re-verified
# against a fresh download at authoring time):
#   @iconify-json/iconoir@1.2.11 — Iconoir 7.11.0, MIT, 1671 icons, 663723 B
#   @iconify-json/ph@1.2.2       — Phosphor 2.1.1, MIT, 9072 icons, 4566288 B
#
# Neither tarball carries a LICENSE file (only `info.json`'s
# `license{title,spdx,url}`), so the two upstream texts are vendored
# separately at `modules/facets/quickshell/icons/LICENSE-{iconoir,ph}` (the
# facet directory that actually ships selected icons — §C2) and copied into
# this derivation's `$out` from there, rather than fetched a second time.
#
# `lib/pkgs.nix`'s walker auto-discovers any `pkgs/<name>/default.nix` with no
# sibling `flake.nix` (confirmed against the `pkgs/eidolon`/`pkgs/kimi-code`
# precedent) — no line anywhere else in the tree names this package.
{
  lib,
  stdenvNoCC,
  fetchurl,
}:

let
  iconoir = fetchurl {
    url = "https://registry.npmjs.org/@iconify-json/iconoir/-/iconoir-1.2.11.tgz";
    hash = "sha256-jl07+9VaqO84ePDC8kjPcf6ZgiFy/QeZDjKrf+emix4=";
  };
  ph = fetchurl {
    url = "https://registry.npmjs.org/@iconify-json/ph/-/ph-1.2.2.tgz";
    hash = "sha256-46WCoEfZgp/H6IQsZg2AZeWY0baLtYy2jnWh8eWmFMw=";
  };
  licenseIconoir = ../../modules/facets/quickshell/icons/LICENSE-iconoir;
  licensePh = ../../modules/facets/quickshell/icons/LICENSE-ph;
in
stdenvNoCC.mkDerivation {
  pname = "iconify-data";
  version = "iconoir-1.2.11+ph-1.2.2";

  dontUnpack = true;
  dontConfigure = true;
  dontBuild = true;

  # Each npm tarball unpacks to one `package/` directory carrying
  # `icons.json` (the IconifyJSON body+alias data `lyra icon` reads),
  # `info.json` (IconifyInfo: name/version/author/license/total), and
  # `metadata.json` (category -> name list, iconoir only carries this one —
  # `ph` also ships it, but neither is required by the resolver, which
  # treats a missing `metadata.json` as an empty category map).
  installPhase = ''
    runHook preInstall

    mkdir -p "$out/share/iconify/iconoir" "$out/share/iconify/ph"

    tar -xzf ${iconoir} -C "$out/share/iconify/iconoir" --strip-components=1 \
      package/icons.json package/info.json package/metadata.json
    tar -xzf ${ph} -C "$out/share/iconify/ph" --strip-components=1 \
      package/icons.json package/info.json package/metadata.json

    install -Dm444 ${licenseIconoir} "$out/share/iconify/iconoir/LICENSE"
    install -Dm444 ${licensePh} "$out/share/iconify/ph/LICENSE"

    runHook postInstall
  '';

  meta = {
    description = "Pinned Iconify icon collections (Iconoir + Phosphor) for `lyra icon` — data only, no code";
    homepage = "https://iconify.design";
    license = lib.licenses.mit;
    platforms = lib.platforms.all;
  };
}
