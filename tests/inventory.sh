#!/usr/bin/env bash
# tests/inventory.sh — the per-host evaluation inventory for the NIX-COMPOSITION
# phase-5 acceptance gates (docs/architecture/NIX-COMPOSITION.md, TASK-REGISTER
# §4). Prints the seven measures G1-G7 for ONE nixosConfiguration as stable,
# diffable plain text.
#
#   tests/inventory.sh <flakeref> <host>
#   tests/inventory.sh .# yomi-strix
#   tests/inventory.sh path:/home/khoa/worktrees/aoide-p5 yomi-strix
#
# The flakeref convention: any `nix eval` ref whose `#`-root is a checkout of
# this flake — `.` (cwd is the checkout), `.#`, `path:/abs/dir`, or
# `git+file:///abs/dir?rev=<full-sha>` for a frozen commit. The host measures
# read `<flakeref>#nixosConfigurations.<host>.config`; G6/G7 read the
# flake-level `aoideOptions`/`songbookManifest` through the same ref (they are
# host-independent by construction — no per-host variant exists).
#
# EVALUATION EVIDENCE, NOT ACTIVATION PROOF. Nothing here builds, switches, or
# activates anything: every measure is one `nix eval`, capped at 580s, so this
# says what the host's configuration EVALUATES to. A gate going green here is
# necessary, never sufficient — the runtime half is the user's admitted
# `nixos-rebuild` switch and the `lyra quickshell healthcheck` / `aoide doctor`
# checks (TASK-REGISTER §4). Reading `drvPath`s proves the derivation graph
# did not move; it does not prove the built system behaves.
#
# Reads nothing outside the given flake, writes nothing but stdout. One
# `nix eval` per measure, deliberately: each measure is independently
# reproducible. The flakeref is NOT echoed, so a before/after pair (parent
# commit vs working tree, phase-5's per-slice gate) diffs cleanly. Each slice
# puts both columns in its commit body.
set -euo pipefail

FLAKE=${1:?usage: tests/inventory.sh <flakeref> <host>}
HOST=${2:?usage: tests/inventory.sh <flakeref> <host>}
# `.#` is the common spelling of "this ref, root attrpath" — drop the
# separator the caller may have already written; every measure re-adds it.
FLAKE=${FLAKE%#}
CFG="$FLAKE#nixosConfigurations.$HOST.config"
TIMEOUT=580

nixeval() { timeout -s INT "$TIMEOUT" nix eval "$@"; }

count() { printf '%s\n' "$1" | grep -c . || true; }
list() { printf '## %s (%s)\n%s\n' "$1" "$(count "$2")" "$2"; }
scalar() { printf '## %s\n%s\n' "$1" "$2"; }

# One line per name, sorted — the sort happens inside nix so ordering never
# depends on the shell's locale.
ATTR_NAMES='as: builtins.concatStringsSep "\n" (builtins.sort builtins.lessThan (builtins.attrNames as))'
# Every home-manager user's <leaf> names, flattened and sorted.
hm_leaf() { printf 'us: builtins.concatStringsSep "\\n" (builtins.sort builtins.lessThan (builtins.concatLists (map (u: builtins.attrNames u.%s) (builtins.attrValues us))))' "$1"; }

printf '# aoide evaluation inventory — nixosConfigurations.%s\n' "$HOST"

# G1 — environment.systemPackages: the sorted `name` multiset (versions
# included; duplicates kept, that is the point of a multiset). Non-derivation
# entries stringify. An entry with no `name` keeps its outPath basename instead
# of collapsing into one "unnamed" line, so two nameless entries stay two.
pkgs=$(nixeval --raw "$CFG.environment.systemPackages" --apply \
  'ps: builtins.concatStringsSep "\n" (builtins.sort builtins.lessThan (map (p: if builtins.isAttrs p then p.name or (builtins.baseNameOf (toString p)) else builtins.toString p) ps))')
list 'G1 environment.systemPackages' "$pkgs"

# G2 — the unit namespaces a slice can add or drop: system, system-user, and
# home-manager's own user units (HM writes these as files, so they are NOT the
# same set as systemd.user.services). Timers, sockets and paths are measured
# beside services because a desktop distro's slices add them, and a unit kind
# no measure reads is a delta only the opaque drvPath would show.
system_services=$(nixeval --raw "$CFG.systemd.services" --apply "$ATTR_NAMES")
list 'G2 systemd.services' "$system_services"
system_timers=$(nixeval --raw "$CFG.systemd.timers" --apply "$ATTR_NAMES")
list 'G2 systemd.timers' "$system_timers"
system_sockets=$(nixeval --raw "$CFG.systemd.sockets" --apply "$ATTR_NAMES")
list 'G2 systemd.sockets' "$system_sockets"
system_paths=$(nixeval --raw "$CFG.systemd.paths" --apply "$ATTR_NAMES")
list 'G2 systemd.paths' "$system_paths"
user_services=$(nixeval --raw "$CFG.systemd.user.services" --apply "$ATTR_NAMES")
list 'G2 systemd.user.services' "$user_services"
user_timers=$(nixeval --raw "$CFG.systemd.user.timers" --apply "$ATTR_NAMES")
list 'G2 systemd.user.timers' "$user_timers"
user_sockets=$(nixeval --raw "$CFG.systemd.user.sockets" --apply "$ATTR_NAMES")
list 'G2 systemd.user.sockets' "$user_sockets"
user_paths=$(nixeval --raw "$CFG.systemd.user.paths" --apply "$ATTR_NAMES")
list 'G2 systemd.user.paths' "$user_paths"
hm_user_services=$(nixeval --raw "$CFG.home-manager.users" --apply "$(hm_leaf systemd.user.services)")
list 'G2 home-manager systemd.user.services' "$hm_user_services"

# G2 — tmpfiles rules, in DECLARED order: a later rule wins where two touch the
# same path, so sorting them (as the name measures above do) would hide a
# precedence change. One rule per line, so the count is the list length.
tmpfiles=$(nixeval --raw "$CFG.systemd.tmpfiles.rules" --apply 'rs: builtins.concatStringsSep "\n" rs')
list 'G2 systemd.tmpfiles.rules' "$tmpfiles"

# G3 — home-manager activation entries (the switch's whole side-effect surface),
# plus the two file trees an activation entry may only seed: every target path
# HM writes into the home, which a slice that moves a file changes.
hm_activation=$(nixeval --raw "$CFG.home-manager.users" --apply "$(hm_leaf home.activation)")
list 'G3 home-manager home.activation' "$hm_activation"
hm_home_file=$(nixeval --raw "$CFG.home-manager.users" --apply "$(hm_leaf home.file)")
list 'G3 home-manager home.file targets' "$hm_home_file"
hm_xdg_config=$(nixeval --raw "$CFG.home-manager.users" --apply "$(hm_leaf xdg.configFile)")
list 'G3 home-manager xdg.configFile targets' "$hm_xdg_config"

# G4 — normal users and the extra groups each is in (`users.groups` names the
# groups themselves; membership is extraGroups).
users=$(nixeval --raw "$CFG.users.users" --apply \
  'us: builtins.concatStringsSep "\n" (map (u: "${u.name}: [" + builtins.concatStringsSep " " (builtins.sort builtins.lessThan u.extraGroups) + "]") (builtins.sort (a: b: a.name < b.name) (builtins.filter (u: u.isNormalUser) (builtins.attrValues us))))')
list 'G4 normal users (name: [extraGroups])' "$users"

# G5 — the toplevel derivation. The P0 gate: byte-identical across a slice that
# changes no Nix.
toplevel=$(nixeval --raw "$CFG.system.build.toplevel.drvPath")
scalar 'G5 system.build.toplevel.drvPath' "$toplevel"

# G6 — every aoide.* option `lyra onboard` can generate (count + names).
aoide_options=$(nixeval --raw "$FLAKE#aoideOptions" --apply 'os: builtins.concatStringsSep "\n" (builtins.sort builtins.lessThan (map (o: o.name) os))')
list 'G6 aoideOptions' "$aoide_options"

# G7 — the songbook manifest/registry, hashed as `nix eval --json` emits it.
manifest_sha=$(nixeval --json "$FLAKE#songbookManifest" | sha256sum | cut -d' ' -f1)
scalar 'G7 songbookManifest sha256 (nix eval --json | sha256sum)' "$manifest_sha"
