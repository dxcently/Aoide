#!/usr/bin/env bash
# The consumer fixture, evaluated: a stranger's flake whose ONLY contact with
# Strillix is the root flake's exported surface.
#
#   ./tests/consumer/run.sh                 # this checkout's working tree
#   ./tests/consumer/run.sh git+file:///abs/dir?rev=<sha>   # a frozen commit
#
# Two proofs, in that order:
#
#   1. The reach-in grep. `flake.nix` and its neighbours must name NO path into
#      Aoide's source tree (`<aoide> + "/…"`) and no Aoide input threading
#      (`aoide.inputs.<x>`). This is the migration contract's own words: a
#      consumer consumes Aoide ONLY through public flake outputs.
#   2. The evaluation. `flake.nix`'s `outputs` is applied to the REF's own
#      inputs (so a commit ref really freezes the tree under test, the same
#      discipline tests/quickshell-seam uses), and the resulting hosts are read.
#      `consumer` performs from its OWN rices: it hands over the nucleus module (the option
#      contract) and selects its own `nucleus` group (the services), strillix, the shell and stylix; it performs one rice and builds in a second
#      that BORROWS the first's widgets; and the rice machinery a real host gets
#      (deployed config, seeded stage, seeded machine rices, templates on the
#      units) is there. `aoide-rices` performs Aoide's own `sonata` through the
#      `ricesRoot` export, and its strillix lane is handed that same directory.
#      `strillix-only` is the desktop with aoide off: it evaluates, defines no
#      aoided unit, and neither its bridge nor its migration call carries
#      AOIDE_CORE_BIN.
set -uo pipefail
cd "$(dirname "$0")" || exit 1
root=$(cd ../.. && pwd)

ref=${1:-path:$root}

pass=0; fail=0
check() { # name  expected  actual
  if [ "$2" = "$3" ]; then
    printf '%-52s PASS\n' "$1"; pass=$((pass+1))
  else
    printf '%-52s FAIL (want %s, got %s)\n' "$1" "$2" "$3"; fail=$((fail+1))
  fi
}

printf '%-52s %s\n' "CHECK" "RESULT"
printf '%s\n' "--------------------------------------------------------------------"

# ── 1. No reach-in ──────────────────────────────────────────────────────────
# Three shapes are banned, and each is caught by its own control below:
#   - `<aoide> + "/…"`            a path built onto the aoide input, through an
#                                 attribute chain (`aoide.outPath`,
#                                 `aoide.inputs.aoide`);
#   - `aoide.inputs.<x>`          Aoide's own inputs threaded by hand;
#   - `getFlake(…)`               the ref re-imported so a path can be read off
#                                 it, the shape that dodges both greps above.
# A name the grep cannot follow (`let p = aoide.outPath; in p + "/modules"`) is
# a reach-in this grep does NOT catch: the controls below are the honest
# statement of what it does reach, and the evaluation proof is the real answer.
# `aoide.lib.…` and `aoide.nixosModules.…` are the shape being REQUIRED, and
# neither mentions a path or a ref. Both greps run over
# every file in this directory, so a new fixture file cannot opt out.
reachin=$(grep -rnE 'aoide(\.[A-Za-z]+)* *\+ *"/|aoide\.inputs\.|getFlake' --include='*.nix' . || true)
if [ -z "$reachin" ]; then
  printf '%-52s PASS\n' "no path into Aoide, no input threading, no getFlake"; pass=$((pass+1))
else
  printf '%-52s FAIL\n' "no path into Aoide, no input threading, no getFlake"
  printf '%s\n' "$reachin" | sed 's/^/    | /'
  fail=$((fail+1))
fi

# The positive controls for that grep: it must be able to fire on EACH spelling
# it bans. A pattern that matches nothing anywhere is not evidence.
control() { # label  sample-line
  if grep -qE 'aoide(\.[A-Za-z]+)* *\+ *"/|aoide\.inputs\.|getFlake' <(printf '%s\n' "$2"); then
    printf '%-52s PASS\n' "$1"; pass=$((pass+1))
  else
    printf '%-52s FAIL\n' "$1"; fail=$((fail+1))
  fi
}
control "the reach-in grep can match: bare input" \
  'x = aoide + "/modules/default.nix";'
control "the reach-in grep can match: .outPath" \
  'x = aoide.outPath + "/modules/default.nix";'
control "the reach-in grep can match: a chained attr" \
  'x = aoide.inputs.aoide.outPath + "/rices";'
control "the reach-in grep can match: input threading" \
  'x = aoide.inputs.nixpkgs.legacyPackages.${system};'
control "the reach-in grep can match: getFlake" \
  'x = (builtins.getFlake "path:/home/khoa/Aoide").outPath + "/modules";'

# ── 2. The consumer's host ──────────────────────────────────────────────────
tmp=$(mktemp -d) || exit 1
trap 'rm -rf "$tmp"' EXIT

expr="
  let
    flake = builtins.getFlake \"$ref\";
    out = (import ./flake.nix).outputs {
      nixpkgs = flake.inputs.nixpkgs;
      home-manager = flake.inputs.home-manager;
      aoide = flake;
    };
    cfg = out.nixosConfigurations.consumer.config;
    home = cfg.home-manager.users.\${cfg.aoide.user};
    envOf = units: unit: let u = units.\${unit} or { }; in u.serviceConfig.Environment or u.Service.Environment or [ ];
    aoideRices = out.nixosConfigurations.aoide-rices.config;
    # ── The front door (CONTRACTS.md §2, PACKAGE-LAYOUT) ──────────────────────
    # The stubbed host mixes value slots: the consumer's own bar and Aoide's dock
    # part. Its shell is read from the home it is built into, and its components
    # and the source of each filled slot from the record and the inventory.
    stubbedCfg = out.nixosConfigurations.stubbed.config;
    stubbedHome = stubbedCfg.home-manager.users.\${stubbedCfg.aoide.user};
    consumerCfg = out.nixosConfigurations.consumer.config;
    stubbed = {
      drv = stubbedCfg.system.build.toplevel.drvPath;
      components = builtins.mapAttrs (_: c: c.provider) stubbedCfg.strillix.components;
      shellUnits = builtins.length (
        builtins.filter (n: n == \"aoide-quickshell\") (builtins.attrNames (stubbedHome.systemd.user.services or { }))
      );
      bridgeUnits = builtins.length (
        builtins.filter (n: n == \"shellbridge\") (builtins.attrNames stubbedCfg.systemd.user.services)
      );
      aggregations = out.inventory.stubbed.aggregation;
    };
    consumerComponents = builtins.mapAttrs (_: c: c.provider) consumerCfg.strillix.components;
    # The parts are the ten strillix providers, each a file that exists; the rices
    # are the committed rices with a rice.nix.
    parts = {
      names = builtins.attrNames flake.parts;
      allFiles = builtins.all builtins.pathExists (builtins.attrValues flake.parts);
    };
    rices = builtins.attrNames flake.rices;
    # Merging Aoide's rices into a registry the hook over the SAME rices
    # already registers is a name collision the hook refuses. The hook is handed
    # a registry that holds the rices, and a bare registry is the control.
    hookOver = registry:
      (flake.lib.rices {
        lib = flake.inputs.nixpkgs.lib;
        ricesRoot = flake.ricesRoot;
      }).hook { inherit registry; };
    collision = {
      merged = (builtins.tryEval (hookOver { aggregations = flake.rices; }).registry).success;
      bare = (builtins.tryEval (hookOver { aggregations = { }; }).registry).success;
    };
    # Aoide's rices and the consumer's own, merged BEFORE anything reads them:
    # one call answers the directory and the registry over it.
    mergedRices = flake.lib.rices.merge {
      lib = flake.inputs.nixpkgs.lib;
      pkgs = flake.inputs.nixpkgs.legacyPackages.x86_64-linux;
      books = [
        flake.ricesRoot
        ./rices-merge
      ];
    };
    mergedCfg = out.nixosConfigurations.merged.config;
    onlyCfg = out.nixosConfigurations.strillix-only.config;
    onlyHome = onlyCfg.home-manager.users.fixture;
  in {
    aoideRices = {
      drv = aoideRices.system.build.toplevel.drvPath;
      rice = aoideRices.strillix.rice;
      builtIn = aoideRices.strillix.rices.builtIn;
      bg = aoideRices.strillix.theme.palette.bg;
      ricesIsExport = toString out.nixosConfigurations.aoide-rices._module.args.ricesRoot == toString flake.ricesRoot;
    };
    merged = {
      drv = mergedCfg.system.build.toplevel.drvPath;
      rice = mergedCfg.strillix.rice;
      builtIn = mergedCfg.strillix.rices.builtIn;
      bg = mergedCfg.strillix.theme.palette.bg;
      borrowed = mergedRices.riceMeta.encore.manifest == mergedRices.riceMeta.sonata.manifest && mergedRices.riceMeta.encore.manifest != { };
      names = mergedRices.discoveredNames;
      ricesIsMerged = toString out.nixosConfigurations.merged._module.args.ricesRoot == toString mergedRices.dir;
    };
    drv = out.nixosConfigurations.consumer.config.system.build.toplevel.drvPath;
    rice = cfg.strillix.rice;
    builtIn = cfg.strillix.rices.builtIn;
    bg = cfg.strillix.theme.palette.bg;
    strillix = cfg.strillix.enable;
    stylix = cfg.strillix.stylix.enable;
    quickshell = cfg.strillix.quickshell.enable;
    config = cfg.strillix.quickshell.config;
    activation = builtins.attrNames (home.home.activation or { });
    hmServices = builtins.attrNames (home.systemd.user.services or { });
    systemdUserServices = builtins.attrNames cfg.systemd.user.services;
    templates = map (e: builtins.replaceStrings [ \"STRILLIX_TEMPLATES=\" ] [ \"\" ] e) (
      builtins.filter (e: builtins.match \"STRILLIX_TEMPLATES=.*\" e != null)
        (envOf cfg.systemd.user.services \"shellbridge\")
    );
    aoidedUnit = builtins.hasAttr \"aoided\" cfg.systemd.user.services;
    aoidedTemplates = builtins.length (
      builtins.filter (e: builtins.match \"STRILLIX_TEMPLATES=.*\" e != null) (envOf cfg.systemd.user.services \"aoided\")
    );
    strillixOnPath = builtins.length (builtins.filter (p: (p.name or \"\") == \"strillix\") cfg.environment.systemPackages);
    strillixOnlyPath = builtins.length (builtins.filter (p: (p.name or \"\") == \"strillix\") onlyCfg.environment.systemPackages);
    front = {
      inherit stubbed consumerComponents parts rices collision;
    };
    strillixOnly = {
      drv = onlyCfg.system.build.toplevel.drvPath;
      aoide = onlyCfg.aoide.enable;
      strillix = onlyCfg.strillix.enable;
      aoided = builtins.hasAttr \"aoided\" onlyCfg.systemd.user.services;
      components = builtins.mapAttrs (_: c: c.provider) onlyCfg.strillix.components;
      shellUnits = builtins.length (
        builtins.filter (n: n == \"aoide-quickshell\") (builtins.attrNames (onlyHome.systemd.user.services or { }))
      );
      bridgeUnits = builtins.length (
        builtins.filter (n: n == \"shellbridge\") (builtins.attrNames onlyCfg.systemd.user.services)
      );
      bridgeCoreBin = builtins.length (
        builtins.filter (e: builtins.match \"AOIDE_CORE_BIN=.*\" e != null) (envOf onlyCfg.systemd.user.services \"shellbridge\")
      );
      consumerBridgeCoreBin = builtins.length (
        builtins.filter (e: builtins.match \"AOIDE_CORE_BIN=.*\" e != null) (envOf cfg.systemd.user.services \"shellbridge\")
      );
      migrateCoreBin = builtins.length (
        flake.inputs.nixpkgs.lib.splitString \"AOIDE_CORE_BIN=\" onlyHome.home.activation.aoideMigrateRoots.data
      ) - 1;
      consumerMigrateCoreBin = builtins.length (
        flake.inputs.nixpkgs.lib.splitString \"AOIDE_CORE_BIN=\" home.home.activation.aoideMigrateRoots.data
      ) - 1;
    };
  }
"

if ! out=$(nix eval --impure --json --show-trace --expr "$expr" 2>"$tmp/eval.err"); then
  printf '%-52s FAIL\n' "the consumer host evaluates"
  grep -v '^ *$' "$tmp/eval.err" | tail -14 | sed 's/^/    | /'
  fail=$((fail+1))
  printf '%s\n' "--------------------------------------------------------------------"
  printf '%d passed, %d failed\n' "$pass" "$fail"
  exit 1
fi
printf '%-52s PASS\n' "the consumer host evaluates"; pass=$((pass+1))

o() { printf '%s' "$out" | jq -r "$1"; }
cfg=$(o .config)

check "performs the declared rice"            "solo" "$(o .rice)"
check "builds in declared + available"        "duet solo" "$(o '.builtIn | join(" ")')"
check "the available rice's theme is inert"  "#14161c" "$(o .bg)"
check "the strillix lane is on"                   "true" "$(o .strillix)"
check "the stylix lane is on"                 "true" "$(o .stylix)"
check "the shell lane is on"                  "true" "$(o .quickshell)"
check "the shell runs the deployed config"    "/home/fixture/.aoide/run/qml" "$(o .config)"
check "the rice's seed activation is there"   "1" \
  "$(o '[.activation[] | select(test("^aoideSeedRices$"))] | length')"
check "the shell's unit is there"             "1" \
  "$(o '[.hmServices[] | select(test("^aoide-quickshell$"))] | length')"
check "the bridge's unit is there"            "1" \
  "$(o '[.systemdUserServices[] | select(test("^shellbridge$"))] | length')"
check "the bridge unit carries the templates"  "1" "$(o '.templates | length')"
check "…and it names the shipped directory"   "true" \
  "$(o '(.templates | unique | length) == 1 and (.templates[0] | endswith("-strillix-rices-templates/share/strillix/rices"))')"
check "the consumer's aoided unit is there"   "true" "$(o .aoidedUnit)"
check "aoided's unit carries no templates"    "0" "$(o .aoidedTemplates)"
check "the host PATH carries one strillix"    "1" "$(o .strillixOnPath)"

# ── 2b. The installed strillix, run with its variable unset ───────────────────
# The strillix on the host PATH is built and run with no home and no exported
# templates. `rice refresh --check` lists what a refresh would bring in, and it
# writes nothing to the rices. It still names the shipped rices. The bare core
# binary is the control: with no default it finds none. An exported value wins
# over the default.
host_expr="
let
flake = builtins.getFlake \"$ref\";
out = (import ./flake.nix).outputs {
nixpkgs = flake.inputs.nixpkgs;
home-manager = flake.inputs.home-manager;
aoide = flake;
};
cfg = out.nixosConfigurations.consumer.config;
in
builtins.head (builtins.filter (p: (p.name or \"\") == \"strillix\") cfg.environment.systemPackages)
"
core_expr="
let
flake = builtins.getFlake \"$ref\";
out = (import ./flake.nix).outputs {
nixpkgs = flake.inputs.nixpkgs;
home-manager = flake.inputs.home-manager;
aoide = flake;
};
in
out.nixosConfigurations.consumer.pkgs.aoide.strillix
"
host_pkg=$(nix build --no-link --print-out-paths --impure --expr "$host_expr" 2>"$tmp/host.err") || host_pkg=""
core_pkg=$(nix build --no-link --print-out-paths --impure --expr "$core_expr" 2>"$tmp/core.err") || core_pkg=""
check "the host PATH strillix builds"         "true" "$([ -n "$host_pkg" ] && echo true || echo false)"
check "the bare core strillix builds"         "true" "$([ -n "$core_pkg" ] && echo true || echo false)"

mkdir -p "$tmp/aoide" "$tmp/strillix"
clean() { env -u STRILLIX_TEMPLATES HOME=/homeless-shelter AOIDE_ROOT="$tmp/aoide" STRILLIX_ROOT="$tmp/strillix" "$@"; }
names() { jq -r '[.data.rices[]?.rice] | sort | join(" ")' 2>/dev/null; }
check "strx links to the installed strillix"  "strillix" "$(readlink "$host_pkg/bin/strx" 2>/dev/null)"
check "the installed strillix lists its rices" "duet solo" \
  "$(clean "$host_pkg/bin/strillix" rice refresh --check --json 2>/dev/null | names)"
bare_rc=0; clean "$core_pkg/bin/strillix" rice refresh --check >/dev/null 2>&1 || bare_rc=$?
check "control: the bare core strillix finds none" "1" "$bare_rc"
check "control: an exported value wins over it" "" \
  "$(env STRILLIX_TEMPLATES="$tmp/nowhere" HOME=/homeless-shelter AOIDE_ROOT="$tmp/aoide" STRILLIX_ROOT="$tmp/strillix" "$host_pkg/bin/strillix" rice refresh --check --json 2>/dev/null | names)"

check "front: the stubbed host fills bar and dock"  '{"bar":"stub","dock":"strillix"}' \
  "$(o '.front.stubbed.components | tojson')"
check "front: its shell is one unit"                 "1" "$(o .front.stubbed.shellUnits)"
check "front: its bridge is one unit"                "1" "$(o .front.stubbed.bridgeUnits)"
check "front: the stubbed host selects its aggregation" "nucleus,stubbed" \
  "$(o '.front.stubbed.aggregations | join(",")')"
check "front: the consumer fills ten slots, all strillix" "10 strillix" \
  "$(o '"\(.front.consumerComponents | length) \(.front.consumerComponents | [.[]] | unique | join(","))"')"
check "front: parts name the ten components"         \
  "bar,calendar,dock,greeter,launcher,lock,notifications,powermenu,switcher,wallpaper" \
  "$(o '.front.parts.names | join(",")')"
check "front: every part is a file"                  "true" "$(o .front.parts.allFiles)"
check "front: rices name the committed rice rices"   "true" \
  "$(o '.front.rices | index("sonata") != null')"
check "front: a rice merged into its own hook clashes" "false" "$(o .front.collision.merged)"
check "front: the same hook with no rice merged holds" "true" "$(o .front.collision.bare)"

# ── 3. The merged rices ──────────────────────────────────────────────────
# One call gives the directory and the registry over it; a host performs a
# consumer rice that borrows an Aoide rice across the two books.
check "merged: performs the consumer's encore"       "encore" "$(o .merged.rice)"
check "merged: builds in the Aoide rice it borrows"  "encore sonata" "$(o '.merged.builtIn | join(" ")')"
check "merged: encore wears its own theme"          "#1a1424" "$(o .merged.bg)"
check "merged: encore's widgets are sonata's"        "true" "$(o .merged.borrowed)"
check "merged: the registry names both books' rices" "true" \
  "$(o '.merged.names | (index("sonata") != null) and (index("encore") != null) and (index("barcarolle") != null)')"

# A name two books define is refused, and the refusal names the rice and both
# books. Run on its own: a throw cannot be read out of the JSON above.
clash_expr="
  let
    flake = builtins.getFlake \"$ref\";
  in
    (flake.lib.rices.merge {
      lib = flake.inputs.nixpkgs.lib;
      pkgs = flake.inputs.nixpkgs.legacyPackages.x86_64-linux;
      books = [ flake.ricesRoot ./rices-clash ];
    }).dir
"
if clash_out=$(nix eval --impure --raw --expr "$clash_expr" 2>"$tmp/clash.err"); then
  clash_rc=0
else
  clash_rc=$?
fi
clash_msg=$(cat "$tmp/clash.err")
check "merged: a rice two books define is refused"   "1" "$([ "$clash_rc" -ne 0 ] && echo 1 || echo 0)"
check "merged: the refusal names the rice"           "true" \
  "$(printf '%s' "$clash_msg" | grep -q "'sonata' is in" && echo true || echo false)"
check "merged: the refusal names Aoide's book"        "true" \
  "$(printf '%s' "$clash_msg" | grep -q 'rices' && echo true || echo false)"
check "merged: the refusal names the consumer's book" "true" \
  "$(printf '%s' "$clash_msg" | grep -q 'rices-clash' && echo true || echo false)"

printf '%-52s %s\n' "consumer toplevel drvPath" "$(o .drv)"

check "aoide-rices performs Aoide's sonata"           "sonata" "$(o .aoideRices.rice)"
check "…and builds in sonata alone"                 "sonata" "$(o '.aoideRices.builtIn | join(" ")')"
check "…and wears sonata's theme"                  "#f2ebde" "$(o .aoideRices.bg)"
check "…its strillix lane reads the exported rices"  "true" "$(o .aoideRices.ricesIsExport)"
check "merged: its strillix lane reads the merged directory" "true" "$(o .merged.ricesIsMerged)"

printf '%-52s %s\n' "aoide-rices toplevel drvPath" "$(o .aoideRices.drv)"
printf '%-52s %s\n' "merged toplevel drvPath" "$(o .merged.drv)"

# ── 4. Strillix with Aoide off ──────────────────────────────────────────────
# The desktop aggregation alone, with no core under it: the strillix lane
# evaluates, no aoided unit is defined, and the bridge and the migration call
# carry no AOIDE_CORE_BIN.
check "strillix-only: aoide is off"                  "false" "$(o .strillixOnly.aoide)"
check "strillix-only: the strillix lane is on"       "true" "$(o .strillixOnly.strillix)"
check "strillix-only: no aoided unit is defined"     "false" "$(o .strillixOnly.aoided)"
check "strillix-only: the desktop fills ten slots, all strillix" "10 strillix" \
  "$(o '"\(.strillixOnly.components | length) \(.strillixOnly.components | [.[]] | unique | join(","))"')"
check "strillix-only: the shell's unit is there"     "1" "$(o .strillixOnly.shellUnits)"
check "strillix-only: the bridge's unit is there"    "1" "$(o .strillixOnly.bridgeUnits)"
check "strillix-only: the bridge carries no AOIDE_CORE_BIN" "0" "$(o .strillixOnly.bridgeCoreBin)"
check "strillix-only: the host PATH carries one strillix" "1" "$(o .strillixOnlyPath)"
check "control: the consumer's bridge carries AOIDE_CORE_BIN" "1" "$(o .strillixOnly.consumerBridgeCoreBin)"
check "strillix-only: the migration call carries no AOIDE_CORE_BIN" "0" "$(o .strillixOnly.migrateCoreBin)"
check "control: the consumer's migration call carries AOIDE_CORE_BIN" "1" "$(o .strillixOnly.consumerMigrateCoreBin)"
printf '%-52s %s\n' "strillix-only toplevel drvPath" "$(o .strillixOnly.drv)"

printf '%s\n' "--------------------------------------------------------------------"
printf '%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
