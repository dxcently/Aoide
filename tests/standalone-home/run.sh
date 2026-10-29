#!/usr/bin/env bash
# tests/standalone-home/run.sh — a standalone Home Manager user installs the
# Strillix runtime with no NixOS host. Usage: run.sh <aoide flake ref>
# It evaluates only; nothing is built or switched.
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd)
aoide=${1:?usage: run.sh <aoide flake ref>}
fails=0

# ev <home> <apply>: the home's config, reduced by the apply expression.
ev() {
  nix eval --json --no-write-lock-file --override-input aoide "$aoide" \
    "path:$here#homeConfigurations.$1.config" --apply "$2" 2>/dev/null
}

# evf <home> <attr>: a home's own attribute, which may throw; its stderr is kept.
evf() {
  nix eval --raw --no-write-lock-file --override-input aoide "$aoide" \
    "path:$here#homeConfigurations.$1.$2" 2>&1
}

check() {
  local name=$1 want=$2 got=$3
  if [ "$got" = "$want" ]; then
    echo "ok   $name"
  else
    echo "FAIL $name: want $want, got $got"
    fails=$((fails + 1))
  fi
}

# ── the standalone user ─────────────────────────────────────────────────────
check "the bridge unit is there" "true" \
  "$(ev standalone 'c: builtins.elem "shellbridge" (builtins.attrNames c.systemd.user.services)')"
check "the bridge runs strillix shellbridge" "1" \
  "$(ev standalone 'c: builtins.length (builtins.filter (e: builtins.match ".*shellbridge --run" e != null) c.systemd.user.services.shellbridge.Service.ExecStart)')"
check "STRILLIX_ROOT is a session variable" '"/home/fixture/.strillix"' \
  "$(ev standalone 'c: c.home.sessionVariables.STRILLIX_ROOT')"
check "STRILLIX_HOST_CONFIG is the user's own path" '"/home/fixture/.config/strillix/host.nix"' \
  "$(ev standalone 'c: c.home.sessionVariables.STRILLIX_HOST_CONFIG')"
check "the bridge unit carries the templates" "1" \
  "$(ev standalone 'c: builtins.length (builtins.filter (e: builtins.match "STRILLIX_TEMPLATES=.*" e != null) c.systemd.user.services.shellbridge.Service.Environment)')"
check "the shell's unit is there" "true" \
  "$(ev standalone 'c: builtins.elem "aoide-quickshell" (builtins.attrNames c.systemd.user.services)')"
# (a) the shell unit and its QML config are forced: a missing overlay name fails here.
check "the shell unit carries the templates" "1" \
  "$(ev standalone 'c: builtins.length (builtins.filter (e: builtins.match "STRILLIX_TEMPLATES=.*" e != null) c.systemd.user.services.aoide-quickshell.Service.Environment)')"
check "the shell's ExecStart forces its QML config" "true" \
  "$(ev standalone 'c: builtins.stringLength (toString c.systemd.user.services.aoide-quickshell.Service.ExecStart) > 0')"
check "the QML config derivation is forced" "true" \
  "$(ev standalone 'c: builtins.stringLength (toString c.strillix.quickshell.config) > 0')"
# (a2) the QML config is forced through its own deploy entry: the derivation's .drv is
# in the entry's string context, so a broken callPackage (strillix-shell) fails here.
check "the QML config derivation is in the deploy context" "true" \
  "$(ev standalone 'c: builtins.any (p: builtins.match ".*-aoide-quickshell-config\\.drv" p != null) (builtins.attrNames (builtins.getContext c.home.activation.aoideDeployQml.data))')"
# (a3) a standalone home carries no aoided unit: core's daemon is NixOS-only, so nothing
# on this path anchors aoide.sessionTarget (see modules/dendrites/quickshell/home.nix).
check "a standalone home has no aoided unit" "false" \
  "$(ev standalone 'c: builtins.elem "aoided" (builtins.attrNames c.systemd.user.services)')"
# (b) the paint fact is on, set by the home half itself; the runtime mark is its own false.
check "strillix.enable is true standalone" "true" \
  "$(ev standalone 'c: c.strillix.enable')"
check "strillix.runtime.nixos is false standalone" "false" \
  "$(ev standalone 'c: c.strillix.runtime.nixos')"
# (d) the defaults follow the home's own identity.
check "aoide.user defaults to home.username" '"fixture"' \
  "$(ev standalone 'c: c.aoide.user')"
check "aoide.root defaults under home.homeDirectory" '"/home/fixture/.aoide"' \
  "$(ev standalone 'c: c.aoide.root')"
check "strillix.root defaults under home.homeDirectory" '"/home/fixture/.strillix"' \
  "$(ev standalone 'c: c.strillix.root')"
check "moved home: aoide.user follows home.username" '"guest"' \
  "$(ev moved 'c: c.aoide.user')"
check "moved home: aoide.root follows home.homeDirectory" '"/srv/guest/.aoide"' \
  "$(ev moved 'c: c.aoide.root')"
check "moved home: strillix.root follows home.homeDirectory" '"/srv/guest/.strillix"' \
  "$(ev moved 'c: c.strillix.root')"
# (f) the user's own values win over every standalone default.
check "user-set aoide.user wins" '"someone"' \
  "$(ev user-roots 'c: c.aoide.user')"
check "user-set aoide.root wins" '"/srv/aoide-root"' \
  "$(ev user-roots 'c: c.aoide.root')"
check "user-set strillix.root wins" '"/srv/strillix-root"' \
  "$(ev user-roots 'c: c.strillix.root')"

# ── negatives: each must refuse, with the named reason ──────────────────────
# (c) a host that writes the paint fact.
out=$(ev bad-enable 'c: c.strillix.enable' 2>&1 || true)
got=$(evf bad-enable config.strillix.enable)
if echo "$got" | grep -q "read-only fact"; then
  echo "ok   a host that writes strillix.enable is refused"
else
  echo "FAIL a host that writes strillix.enable: refusal not seen: ${got:0:200}"
  fails=$((fails + 1))
fi
# (g) a host that writes the runtime mark: readOnly refuses the second definition by name.
got=$(evf bad-runtime config.strillix.runtime.nixos)
if echo "$got" | grep -q "strillix.runtime.nixos' is read-only"; then
  echo "ok   a host that writes strillix.runtime.nixos is refused"
else
  echo "FAIL a host that writes strillix.runtime.nixos: refusal not seen: ${got:0:200}"
  fails=$((fails + 1))
fi
# (e) no hostConfig: the constructor refuses with the taught assertion.
got=$(evf unset activationPackage.drvPath)
if echo "$got" | grep -q "strillix.hostConfig is not set"; then
  echo "ok   no hostConfig is refused with the named assertion"
else
  echo "FAIL no hostConfig: assertion not seen: ${got:0:200}"
  fails=$((fails + 1))
fi
# (3) no nucleus module: the constructor refuses with the taught message.
got=$(evf no-nucleus activationPackage.drvPath)
if echo "$got" | grep -q "aoide.homeModules.nucleus"; then
  echo "ok   no nucleus module is refused with the named module"
else
  echo "FAIL no nucleus module: message not seen: ${got:0:200}"
  fails=$((fails + 1))
fi

echo "$fails failure(s)"
[ "$fails" -eq 0 ]
