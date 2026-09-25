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
# entries stringify.
pkgs=$(nixeval --raw "$CFG.environment.systemPackages" --apply \
  'ps: builtins.concatStringsSep "\n" (builtins.sort builtins.lessThan (map (p: if builtins.isAttrs p then p.name or "unnamed" else builtins.toString p) ps))')
list 'G1 environment.systemPackages' "$pkgs"

# G2 — the three service namespaces: system, system-user, and home-manager's
# own user units (HM writes these as files, so they are NOT the same set as
# systemd.user.services).
system_services=$(nixeval --raw "$CFG.systemd.services" --apply "$ATTR_NAMES")
list 'G2 systemd.services' "$system_services"
user_services=$(nixeval --raw "$CFG.systemd.user.services" --apply "$ATTR_NAMES")
list 'G2 systemd.user.services' "$user_services"
hm_user_services=$(nixeval --raw "$CFG.home-manager.users" --apply "$(hm_leaf systemd.user.services)")
list 'G2 home-manager systemd.user.services' "$hm_user_services"

# G3 — home-manager activation entries (the switch's whole side-effect surface).
hm_activation=$(nixeval --raw "$CFG.home-manager.users" --apply "$(hm_leaf home.activation)")
list 'G3 home-manager home.activation' "$hm_activation"

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
