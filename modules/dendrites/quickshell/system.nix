# modules/dendrites/quickshell/system.nix — the shell lane's system half (the
# Quickshell runtime and its one service).
#
# Two things and nothing else: the quickshell PACKAGE, and the
# `aoide-quickshell` user service, started from the config directory a host (or
# another lane) has named. This lane renders no QML, deploys nothing, and knows
# no rice: it starts the shell on whatever `strillix.quickshell.config` points at.
#
# A plain module with no `habit` key, imported only BY PATH: by this
# directory's `default.nix` (the catalogue entry) and by any lane that brings
# the shell with it, so every importer lands on the one module. Its home half
# is `home.nix` beside it, imported into each user's home (README.md).
#
# The seam (CONTRACTS.md §0). Facts, never lanes — no module reads another
# module (root AGENTS.md house rule 5):
#   - SET here: `strillix.quickshell.enable` — "a shell surface exists on this
#     host", a read-only fact true exactly when this file is imported.
#   - READ here: `strillix.quickshell.config : nullOr str` — the directory the
#     shell runs — which the `strillix` lane sets to its deployed runtime root
#     (`$AOIDE_ROOT/run/qml`) and a host may set to any config directory of its
#     own. strillix sets it, this lane reads it; neither names the other's option.
#
# The two allowed shapes of quickshell WITHOUT strillix:
#   * own config — a host that brings its own directory sets
#     `strillix.quickshell.config = "/home/<u>/.config/quickshell"` (or a store
#     path) in the host module; this service runs from it.
#   * bare — `config` stays null (the default): the package is installed and no
#     service runs. No shell, no graphical-session anchor, nothing to fail.
#
# Reading discipline (house rule 5): the dress and the core scalars this lane
# genuinely uses — `strillix.theme.wallpaper` (the shell's baked wallpaper env, so a
# wallpapered rice survives a rebuild) and `aoide.root` (the runtime root the
# QML reads its stage files from). Nothing else, and no `rice/` path at all:
# what this lane starts is a directory, not a rice.
{
  config,
  lib,
  pkgs,
  aoideInputs,
  ...
}:
let
  # The Quickshell binary from the pre-declared flake input (flake.nix). The
  # upstream package lives there, not in this flake's own pkgs/ walk.
  quickshellPkg = aoideInputs.quickshell.packages.${pkgs.stdenv.hostPlatform.system}.default;

  # The directory the shell runs. `null` is the bare shape above.
  configDir = config.strillix.quickshell.config;

in
{
  config = {
    strillix.quickshell.enable = true;
    strillix.quickshell.nixos = true;

    # ── Quickshell package ──────────────────────────────────────────────────
    environment.systemPackages = [ quickshellPkg ];

    # ── Session anchor ──────────────────────────────────────────────────────
    # `aoide.sessionTarget` is the anchor rather than a fact: the core
    # `aoided` unit is `wantedBy` the target named here, and PartOf ties its
    # lifetime to it. A host with a shell names the graphical session, so the
    # daemon comes up with the compositor instead of at boot — a manually
    # started daemon left on `default.target` takes an immediate stop through
    # PartOf, and BindsTo drags the a2a/mcp doors down with it (found live on
    # sakaki). Not `mkDefault`: exactly one answer is right on this host, and
    # a second one is a conflict worth failing on.
    #
    # A graphical session exists whether or not strillix paints it, which is why
    # this rides the CONFIG fact and not a rice: a host running its own
    # quickshell config is a painting host too. The bare shape names no
    # config and so anchors nothing — package only, no session claim.
    aoide.sessionTarget = lib.mkIf (configDir != null) "graphical-session.target";

  };
}
