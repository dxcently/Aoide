#!/usr/bin/env bash
# tests/thin-flake/run.sh — a thin flake: a consumer whose whole flake is Strillix's
# `mkFlake` over its own root. Usage: run.sh <aoide flake ref>
# It evaluates only; nothing is built or switched.
#
# The fixture's registry marks each `{ byClass = …; }` form once: a file, an inline record
# that names only one of its keys, an inline record that names both, and an inline
# provider target. Every module is checked where habit puts it. The refusals are checked
# by their message: a file that is not a byClass record, an inline record that has no
# keys, is not an attrset, or has a key other than `nixos` or `homeManager`, a raw marker
# inside a provider-set file (mkFlake leaves that unrendered by design), a provider set
# whose `providers` is a path (habit refuses it by its own message), and a host that
# leaves `strillix.hostConfig` unset. The flake's other arguments (`registries`,
# `extraModules`, `overlays`), the thin host's own rices book and its
# `strillix.hostConfig` reach the host too; the checks after the dendrites read what each
# one set.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
aoide=${1:?usage: run.sh <aoide flake ref>}
fails=0
# The stderr of the last evaluation. A failed check prints its error lines: nix's lock-file
# warning comes first in that stream, so the head would show only the warning.
errs=$(mktemp)
trap 'rm -f "$errs"' EXIT

# ev <host> <apply>: a NixOS host's config, reduced by the apply expression, as JSON.
ev() {
  nix eval --json --no-write-lock-file --override-input aoide "$aoide" \
    "path:$here#nixosConfigurations.$1.config" --apply "$2" 2>"$errs"
}

# inv <apply>: the thin host's inventory, reduced by the apply expression, as JSON.
inv() {
  nix eval --json --no-write-lock-file --override-input aoide "$aoide" \
    "path:$here#inventory.thin" --apply "$1" 2>"$errs"
}

# hostev <host> <apply>: a NixOS host's own attributes, reduced by the apply expression, as JSON.
# Its `config` carries neither `_module` nor the package set the overlays built, so those are
# read from the host itself.
hostev() {
  nix eval --json --no-write-lock-file --override-input aoide "$aoide" \
    "path:$here#nixosConfigurations.$1" --apply "$2" 2>"$errs"
}

# refusal <name> <host> <attr> <pattern>...: the host's evaluation must fail, and its
# stderr must hold every pattern, so a refusal that names nothing is a failing check.
refusal() {
  local name=$1 host=$2 attr=$3 got status=0 pat missing=""
  shift 3
  got=$(nix eval --raw --no-write-lock-file --override-input aoide "$aoide" \
    "path:$here#nixosConfigurations.$host.config.$attr" 2>&1) || status=$?
  for pat in "$@"; do
    grep -qF -- "$pat" <<<"$got" || missing="$missing [$pat]"
  done
  if [ "$status" -ne 0 ] && [ -z "$missing" ]; then
    echo "ok   $name"
  else
    echo "FAIL $name: exit $status, missing$missing"
    grep -m 4 -F error <<<"$got" | sed 's/^/     | /'
    fails=$((fails + 1))
  fi
}

check() {
  local name=$1 want=$2 got=$3
  if [ "$got" = "$want" ]; then
    echo "ok   $name"
  else
    echo "FAIL $name: want $want, got $got"
    grep -m 4 -F error "$errs" | sed 's/^/     | /'
    fails=$((fails + 1))
  fi
}

# ── the thin host ───────────────────────────────────────────────────────────
# The host evaluates: forcing its toplevel's derivation runs every module and assertion.
# `never` (a catalogue line) and `trap` (a provider set) are in the registry and no host selects
# them. The thin host evaluating at all is the proof that neither was read: each one throws.
drv=$(nix eval --raw --no-write-lock-file --override-input aoide "$aoide" \
  "path:$here#nixosConfigurations.thin.config.system.build.toplevel.drvPath" 2>"$errs")
check "the thin host evaluates to a system derivation" "true" \
  "$([[ $drv == /nix/store/*-nixos-system-thin-*.drv ]] && echo true || echo false)"

# a file with both modules (`both`): each lands in its own place.
check "both: the NixOS module is in /etc" '"system"' \
  "$(ev thin 'c: c.environment.etc."thin-flake/both".text')"
check "both: the Home Manager module is in the user's home" '"home"' \
  "$(ev thin 'c: c.home-manager.users.thin.home.file."thin-flake-both".text')"
# an inline record that names only `homeManager` (`inline`): a Home Manager module alone.
check "inline: its Home Manager module is in the user's home" '"inline"' \
  "$(ev thin 'c: c.home-manager.users.thin.home.file."thin-flake-inline".text')"
# an inline record that names only `nixos` (`sysonly`): a NixOS module, and nothing in the home.
check "sysonly: the NixOS module is in /etc" '"sysonly"' \
  "$(ev thin 'c: c.environment.etc."thin-flake/sysonly".text')"
check "sysonly: nothing lands in the user's home" "false" \
  "$(ev thin 'c: builtins.elem "thin-flake-sysonly" (builtins.attrNames c.home-manager.users.thin.home.file)')"
# an inline provider target (`picked`, provider `one`): the chosen target's modules apply.
check "picked, provider one: its NixOS module is in /etc" '"picked-one"' \
  "$(ev thin 'c: c.environment.etc."thin-flake/picked".text')"
check "picked is answered by provider one" '"one"' \
  "$(inv 'i: i.dendrites.picked.provider')"

# the `registries` argument: the `extra` source's entry is selected by the thin host, so its
# module is in /etc.
check "registries: the extra source's module is in /etc" '"registries"' \
  "$(ev thin 'c: c.environment.etc."thin-flake/extra".text')"
# the `extraModules` argument: a module every host takes, so the thin host has it too.
check "extraModules: the module is in /etc" '"extraModules"' \
  "$(ev thin 'c: c.environment.etc."thin-flake/extra-module".text')"
# the `overlays` argument: the overlay's attribute is in the host's package set, and is `hello`.
check "overlays: pkgs.thinFlakeMarker is hello" '"hello"' \
  "$(hostev thin 'h: h.pkgs.thinFlakeMarker.pname')"
# the rices book: the thin host's own `rices/` is merged with Strillix's, and the home half reads
# the same merged directory its system half reads.
check "rices: the merged book is a store path ending in -aoide-rices-merged, and the home ricesRoot is that book" \
  '{"merged":true,"same":true}' \
  "$(hostev thin 'h: { merged = builtins.match ".*-aoide-rices-merged" (toString h._module.args.ricesRoot) != null; same = toString h.config.home-manager.extraSpecialArgs.ricesRoot == toString h._module.args.ricesRoot; }')"

# `strillix.hostConfig`: this host sets its own (`thin`), and the value it set reads back.
check "thin sets strillix.hostConfig, and it reads back as set" \
  '"/srv/thin-flake/hosts/thin/default.nix"' \
  "$(ev thin 'c: c.strillix.hostConfig')"
check "inventory.thin names exactly the selected dendrites" '["both","extra","inline","picked","sysonly"]' \
  "$(inv 'i: builtins.attrNames i.dendrites')"

# ── refusals, each by its message ───────────────────────────────────────────
# a file that is a plain module, not a byClass record (`bad`): refused by name.
refusal "bad: a file that is not a byClass record is refused by name" \
  bad system.build.toplevel.drvPath "bad.nix is not a byClass record"
# a raw marker inside a provider-set file (`deep`, provider `one`): habit reads it as a module,
# and the error names the unknown `byClass` option and the provider's value, `deep.one`.
refusal "deep: a raw marker inside a provider-set file is refused, naming byClass and deep.one" \
  deep system.build.toplevel.drvPath "byClass" "deep.one"
# an inline record with no keys (`emptyByClass`): refused by name, with the reason.
refusal "empty-by-class: an inline record with no keys is refused, naming the entry" \
  empty-by-class system.build.toplevel.drvPath \
  "catalogue entry 'emptyByClass' (inline byClass) is not a byClass record" "found no keys"
# an inline record that is not an attrset (`intByClass`): refused by name, with its type.
refusal "int-by-class: an inline record that is not an attrset is refused, naming the entry" \
  int-by-class system.build.toplevel.drvPath \
  "catalogue entry 'intByClass' (inline byClass) is not a byClass record" "found a value of type int"
# an inline record whose only key is `darwin` (`darwinByClass`): no record takes that key, so it
# is refused by name, with the key.
refusal "darwin-by-class: an inline record with a darwin key is refused, naming the entry and the key" \
  darwin-by-class system.build.toplevel.drvPath \
  "catalogue entry 'darwinByClass' (inline byClass) is not a byClass record" "found key(s) darwin"
# a provider set whose `providers` is a path (`notSet`): habit refuses it with its own message,
# so the renderer passed the path on unrendered.
refusal "not-set: a providers value that is not a set is refused by habit's own message" \
  not-set system.build.toplevel.drvPath \
  '`providers` is a path, not an attrset of provider targets'
# a host from this tree that sets no `strillix.hostConfig` (`bare`): reading it fails by name.
refusal "bare: an unset strillix.hostConfig is refused, naming the option" \
  bare strillix.hostConfig "strillix.hostConfig is not set for this host" "so it has no default"

echo "$fails failure(s)"
[ "$fails" -eq 0 ]
