# Selective Nix composition

This is the architecture for dxflake and, after external-consumer proof,
AoideOS. It specifies the agreed configuration interface and the implementation
plan. dxflake implements the interface below: `lib/composition.nix` is the
constructor, its four hosts select through it, and its selection and template
suites test it. AoideOS has not migrated; for that tree the examples here are
still target contracts, not drop-in modules.

## Scope and rollout

dxflake is the first live consumer of portable Aoide and Lyra exports. Finish the
current fixes and reach a coherent checkpoint, prove consumption on dxflake,
then migrate AoideOS away from its walkers. Additive upstream package/module
exports can precede that migration. Osaka and Sakaki use dxflake; Yomi uses
AoideOS. System activation remains user-controlled.

Aoide core remains usable without Nix, Lyra, Mneme, or Melete. This design is a
deployment and composition interface, not a new dependency of the core binary.

## Vocabulary and boundaries

| Term | Meaning |
|---|---|
| Package | A built program or other store artifact; installing one does not necessarily configure a service |
| Module | Configuration evaluated by a particular module system, such as NixOS, nix-darwin, or Home Manager |
| Dendrite | An independently selectable capability and its supported platform/user lanes |
| Provider | A selectable implementation of a capability, such as Mako for notifications |
| Lane | A module for a particular evaluator: `nixos`, `darwin`, or `homeManager` |
| Aggregation | A named group owning its own directory under `modules/aggregations/`: a data body naming, per scope, its dendrite members, its provider choices, and the preferences that ride with them |
| Provider selector | `aggregation.<name>.<dendrite>.provider`: the choice an aggregation exposes on its own interface for a provider-bearing member it groups |
| Scope | Which half of a group's membership is being resolved: `"system"` for the host, `"home"` for one user |
| Override record | `modules/overrides/<name>.nix`: a fix scoped to a capability rather than to a host. Names its target dendrites, optionally the hosts it is confined to, and carries an overlay and/or deferred lane modules |
| Registry | `modules/default.nix`: plain data, the catalogue plus the discovered aggregations and override records. Not a module, and it declares no options |
| Catalogue | The named capability paths in the registry, one line per dendrite |
| Nucleus | Only the minimal common foundation for the selected platform |
| Song | A rice composition: palette, widgets/assets, layout, and supported configuration/dependency references |

Use **aggregation** — singular, one namespace for both scopes: `aggregation.<name>.enable`
on a host and `users.<u>.aggregation.<name>.enable` on a person. Not `aggregations`,
and not an interchangeable second namespace called profiles.
Use **provider**, not implementation, in configuration. Nix modules still use
their ordinary `programs.*`, `services.*`, and package options internally.

## Target tree

```text
flake.nix
hosts/
  osaka/default.nix
  osaka/hardware.nix
  sakaki/default.nix
users/
  khoa.nix
  guest.nix
modules/
  default.nix                 the registry: catalogue, aggregations, override records
  nucleus/default.nix         minimal platform foundation
  aggregations/
    default.nix               shallow discovery: name = path, no body imported
    base/default.nix          one group: description, membership, provider choices
    desktop/default.nix
    gaming/default.nix
    shell/default.nix
  overrides/
    default.nix               shallow discovery: every `*.nix` beside it is a record
  dendrites/
    notifications/
      default.nix             provider paths
      mako.nix                supported lanes
      dunst.nix
    compositor/
      default.nix
      hyprland.nix
      niri.nix
    openai.nix                 local dxflake integration
    kimi-cli.nix
    cloudflared.nix
    aoide.nix                  public upstream integration
    lyra.nix
song/                         mirrors ~/.aoide/song/ byte for byte
  songbook/
    palettes/                 optional shared palettes, arbitrary filenames
    sonata/
      default.nix
      palette.nix
      widgets/
      design/
  covers/                     shared cover art, beside the rices, any rice wears any cover
  stage/                      runtime only, gitignored: what lyra renders and programs watch
pkgs/                         custom package definitions only
```

Groups live under `modules/aggregations/`, one directory each.
`modules/aggregations/default.nix` performs shallow discovery: `readDir` of its
own directory, keep every immediate child directory that holds a `default.nix`,
produce `name = path`. It never imports a body. There is no marker file, no
recursive walker, and no unconditional import of every aggregation. Discovery is
one level deep on purpose: an implementation must never become reachable by
dropping a file in a directory, which is why the catalogue names dendrites; an
aggregation body is inert data whose only effect is the gate it answers, so its
directory name is enough. A discovered directory is not an enabled group —
nothing happens until a host or one of its users selects it by name.

Capability and provider registries stay under `modules/dendrites/`. Every
`default.nix` there is a capability implementation or a provider registry, and
there is no `modules/dendrites/default.nix` collector at all. The earlier rule
telling two kinds of `default.nix` apart under that tree is obsolete: the groups
moved out, and nothing under `modules/dendrites/` is a selection declaration.

The dendrite root is flat. A single-implementation capability is one file at
the root and the catalogue name is the file name, so `desktop-hardware` is
`desktop-hardware.nix`. A directory under `modules/dendrites/` means the
capability needs more than one file: a provider registry with its providers,
or a script or asset the module wraps (`fastfetch/` with its logo,
`cheatsheet/` with its yad script). Grouping is never a directory's job — a
`desktop/` or `gaming/` folder duplicates what the aggregation already states
and drifts from it. The one grouping folder allowed is a compositor's own:
`hyprland/` holds exactly the members written or compiled against Hyprland,
the ones the `compositor` aggregation names, and nothing else.

A shell script a dendrite runs is a store path beside it — `writeShellApplication`
over a real `.sh` file, with every tool it calls in `runtimeInputs` so a
missing tool is a build error — never a `~/<checkout>/scripts/...` string.
A dendrite that needs another's program reaches it by name on PATH, never by
path into its directory; a host that does not select the provider gets an
inert key.

The same composition shape applies to AoideOS when it migrates. Public Lyra
runtime QML/bridges belong to its package; consumer-owned widget source belongs
with the songbook. A consumer does not recreate AoideOS's private facets tree.
Large lane implementations may be extracted to `home.nix`, `nixos.nix`, or
`darwin.nix` within their dendrite. Empty lane files are unnecessary.

## Selection before platform evaluation

`mkDefault` controls definition priority. It cannot select imports after a
platform module graph has already been assembled. Likewise, `mkIf enable`
does not keep an imported module's declarations outside that graph.

The constructor therefore resolves a small selection configuration first using
ordinary `lib.evalModules`, then assembles platform imports. No recursive
filesystem walker is used; the shallow aggregation discovery above is the only
filesystem read in the tree.

Selection itself runs in two steps, because the host interface nests provider
choices under the aggregation that owns them, and those option names come from
the aggregation's own body:

- **Gate step.** Every discovered aggregation declares only `enable`; the rest
  of its attribute set is freeform and ignored. This step answers exactly one
  question: which aggregations did this host, or one of its users, select?
- **Select step.** Those bodies, and only those, are imported. They declare
  their real nested provider options and write their membership.

Both steps are ordinary `lib.evalModules` over a schema that knows nothing about
NixOS. Because an aggregation body is data, it has no way to enable another
aggregation: the gate step's answer is the select step's answer, and no
recursive-dependency machinery is needed or wanted.

```text
registry (catalogue + discovered aggregations) + host selection modules
                              |
      gate step: every aggregation declares only `enable`, nothing more
                              |
     which aggregations did this host or one of its users select?
                              |
      select step: import THOSE bodies; they declare their nested provider
      selectors and write their membership
                              |
        enabled names + resolved providers + per-user home selections
                              |
          import selected dendrite implementations and providers only
                              |
     match override records against what this host resolved
                              |
           NixOS / Darwin modules       per-user HM modules
                              |
                  ordinary platform evaluation
                              |
                  packages, services, configuration
```

Selection options cannot depend on the resulting NixOS/HM configuration: that
would reintroduce a circular import decision. Platform settings are deferred
modules until selection is complete. Constructors are small explicit functions
using Nix module APIs; they are not a second module language or custom loader.

The evaluation boundary, stated exactly. `modules/aggregations/default.nix`
names directories without importing them. An aggregation body is imported if and
only if the host or one of its users selected it — the union of both, so a body
selected only by a user is also present, gated off, in the host scope. A
dendrite implementation and a provider file are imported only in the platform
pass, and only if selection kept them. Override records are the documented
exception: every host imports every record file, because matching means reading
which dendrites it targets, and only the functions it carries stay uncalled —
see Override records. Catalogue path values and selected registries may be read;
fetching or hashing a flake source can still include other files. Root lock resolution is also a separate boundary.

## Catalogue and dendrites

`modules/default.nix` is the registry: plain data, read before any module graph
exists. It is not a module and declares no options. It grows by one named path
per capability:

```nix
# modules/default.nix
{
  catalogue = {
    notifications = ./dendrites/notifications;
    compositor = ./dendrites/compositor;
    openai = ./dendrites/openai.nix;
    kimi-cli = ./dendrites/kimi-cli.nix;
    cloudflared = ./dendrites/cloudflared.nix;
    aoide = ./dendrites/aoide.nix;
    lyra = ./dendrites/lyra.nix;
  };

  # Names and paths only; no body is imported here.
  aggregations = import ./aggregations;
}
```

A catalogue value is a file when the capability has one implementation, and a
directory whose `default.nix` lists provider paths when it has several. The
constructor takes `registry` plus `hostModules` and generates the selection
schema from the registry: `dendrites.<name>.enable` and `.provider` per
catalogue name, `aggregation.<name>` per discovered group.

The selection schema exposes each name with `enable` (default false) and an
optional `provider`. Resolve provider validity only for enabled capabilities,
so option generation does not import every provider implementation. Unknown
dendrite and aggregation names fail selection validation as options that do not
exist, naming the file that asked for them. An enabled multi-provider dendrite
needs a provider from the aggregation that owns it or from the host;
missing/unknown choices report the capability and available names instead of
silently choosing one.

```nix
# notifications/default.nix: an ordinary attribute set
{
  providers = {
    mako = ./mako.nix;
    dunst = ./dunst.nix;
  };
}
```

```nix
# notifications/mako.nix: small lanes can remain in one file
{
  homeManager = { ... }: {
    services.mako.enable = true;
  };
}
```

A dendrite with one implementation needs no artificial provider layer:

```nix
# openai.nix
{
  nixos = { pkgs, ... }: {
    environment.systemPackages = [ pkgs.codex ];
  };
  homeManager = { pkgs, ... }: {
    home.packages = [ pkgs.codex ];
  };
}
```

The example assumes `pkgs.codex` exists in the chosen package set. ChatGPT's
desktop package is included only on supported platforms with an available
package definition. dxflake owns this dendrite like Kimi; it does not require
`aoide.openai.enable` or an Aoide service just to install OpenAI applications.

A provider is exclusive within one selection scope. A compositor capability
chooses Hyprland or Niri; a desktop aggregation composes compositor,
notifications, launcher, wallpaper, and other independent capabilities.
If multiple implementations must coexist, model their separate capability or
user scopes explicitly rather than silently turning `provider` into a list.

## Aggregations and overrides

A group owns one directory, and its `default.nix` is data. The body declares no
options, carries no gate, and takes no arguments. It names its members by
catalogue name, carries both halves of its membership together with the
preferences that belong with them, and imports nothing:

```nix
# modules/aggregations/workstation/default.nix
{
  # Shown on the generated `aggregation.workstation.enable` option.
  description = "A graphical workstation.";

  # The half that answers for the host.
  system = {
    # Single-implementation members: catalogue names, nothing else.
    members = [
      "hyprland-packages"
      "lyra"
    ];

    # Provider-bearing members. The key is the catalogue name; the value is the
    # shared default, and `null` means the group has no opinion.
    providers.compositor = null;

    # A preference of the group rather than of any one member: an ordinary
    # NixOS module, evaluated only in the platform pass.
    nixos = {
      boot.kernel.sysctl."vm.max_map_count" = 2147483642;
    };
  };

  # The half that answers for a person.
  home = {
    members = [
      "rofi"
      "waybar"
    ];

    # A shared default this group carries; a user may name the other one.
    providers.notifications = "mako";

    homeManager =
      { lib, ... }:
      {
        programs.kitty.font.size = lib.mkDefault 12;
      };
  };
}
```

The constructor supplies the gate and the scope split, once, in the only place a
body is wrapped: `aggregation.<name>.enable` on a host reads the `system` half,
the same option under `users.<u>` reads the `home` half. One file, one option
name, two answers. Both halves are optional, and an absent half is a real
answer, not a placeholder. `scope` is no longer part of aggregation authoring;
it remains an implementation detail of the constructor. There is no collector
file and no import line to add: discovery finds the directory.

Provider choices nest under the aggregation that owns the dendrite. Each key of
a body's `providers` attribute set generates a `<dendrite>.provider` option on
that aggregation's own interface in that scope, so a host states what it runs in
one place:

```nix
# on the host: the group owns compositor, so the choice is stated there
aggregation.workstation = {
  enable = true;
  compositor.provider = "hyprland";
};

# the same group's home half, for one person, overriding its shared default
users.khoa.aggregation.workstation = {
  enable = true;
  notifications.provider = "dunst";
};
```

Those names are existing dendrites, not a new capability layer: `compositor` is
the same catalogue name a host can select directly, and the provider list itself
still lives once, in `modules/dendrites/compositor/default.nix`. A dendrite with
one implementation gets no provider option at all. `null` in a body means no
shared default and every selecting host must choose — a host that forgets is
named in the error along with the providers that exist; a string is a shared
default a host may override. One source of truth: the aggregation's selector is
what writes the normalized `dendrites.<name>.provider`.

The top-level `dendrites.<name>.provider` remains available and outranks the
aggregation. It is the escape hatch — a capability no group speaks for, or one
answered against its group's choice on a single host — not the ordinary path.

Two aggregations selecting the same dendrite in the same scope do not
instantiate it twice. Membership and provider are `mkDefault`, so identical
selections merge into one selection; two groups that name different providers
for the same dendrite in the same scope collide with an error naming
`dendrites.<name>.provider` and both values. Import order never decides.

Membership is resolved in the two selection steps; deferred platform preferences
under a half's `nixos` or `homeManager` are evaluated only in the corresponding
selected lane.

- Membership and intentional shared preferences use `mkDefault`.
- Implementation settings use ordinary definitions; preserve upstream defaults
  where they already express the desired behavior.
- A host's ordinary selection overrides a group default, including
  `dendrites.<name>.enable = false` against a group that wants the member.
- Conflicting equal-priority group defaults produce an error; import order does
  not secretly pick a winner.
- `mkForce` is a deliberate exception, not routine host boilerplate.
- Shared broken-package exceptions live in the group that needs them, applied
  only to its selected consumers. Extract a file only when substantial or
  independently reused.
- A group never imports the capability files it selects. It names them, and the
  platform pass imports whichever survived selection.

## Override records

Some fixes belong to a capability rather than to a host: a package whose
upstream build broke, a setting every machine that runs the thing needs. Writing
it into each host drifts; writing it into the dendrite confuses what the thing
*is* with a patch log, and the fix then has to be found and removed by hand when
upstream lands. A record says what it is about and lets the constructor decide
where it lands.

A record is one file under `modules/overrides/`, discovered the same shallow
way as an aggregation — every `*.nix` file beside `modules/overrides/default.nix`
is one, with no catalogue line and no collector:

```nix
# modules/overrides/browser.nix
{
  dendrites = [ "browser" ];        # catalogue names — required
  hosts = [ "osaka" "sakaki" ];     # optional; omit for every host that selected one
  overlay = _final: prev: { … };    # host package set
  nixos = { lib, ... }: { … };      # deferred platform module
  homeManager = { … };              # rides only the users who selected a target
}
```

**Matching.** A record applies to a host when its host filter admits that host
*and* any dendrite it names was selected there. Selection means the union of the
host's own selection and its users' home selections, because `useGlobalPkgs`
means a home lane draws from the host's package set and there is no separate
home one to patch — so a capability only a *user* selected is enough to match,
and that is a deliberate answer, not an oversight. The `homeManager` half is the
exception: it rides only the users whose own home selection hit a target, never
every user on a matched host.

**What a record does not do.** It never selects anything. A record naming a
capability nobody chose is a record that does not apply, not a capability that
gets installed. It is also not a capability itself: a patch does not become a
dendrite, an aggregation, or a new provider variant.

**Application.** A record applies at most once, however many of its targets were
selected. Matched records apply in record-name order, so the result does not
depend on the filesystem. An `overlay` is an ordinary Nix overlay on the host
package set — later overlays see earlier ones as `prev` and win on the same
attribute, which is the whole of the precedence story; there is no overlap
detection beyond it, and none is claimed. A record outranks everything the
constructor imported on its behalf, and the host's own platform module still
outranks the record. Within a lane the ordinary merge rules hold: a plain
definition that conflicts with a dendrite's own is an error, `mkDefault` marks a
preference, and `mkForce` is the deliberate exception.

**Diagnostics.** Unknown fields, targets that are not catalogue names, and host
names that are not in the flake's host list all fail — on *every* host, not only
where the record would have applied, so a typo cannot hide on the machines it
would have missed. A record carrying none of `overlay`, `nixos` or `homeManager`
is an error too: it is a typo, not an intention.

**The evaluation boundary here is weaker than selection's, and the difference is
the point.** An aggregation body is never imported unless selected; a record
file *is* imported on every host, because matching means reading which dendrites
it targets. What an unmatched host never spends is the work: `overlay`, `nixos`
and `homeManager` are functions and nothing calls them. Authors must therefore
keep imports, fetches and package computation inside those functions — metadata
that computes defeats this, and the tests prove only the function bodies. Do not
state or imply that an unmatched record is unread.

`darwin` is deliberately absent from the record schema: there is no darwin
constructor to apply it, and a field silently dropped is worse than one that
does not exist. It is added with the constructor, not before it.

## Hosts and shared users

System selection and per-user selection are separate. A user selects the same
interface for its home lane: aggregations, the provider choices those
aggregations own, and individual dendrites. This replaces the earlier
illustrative plain list of home dendrite names, which could not express per-user
providers or default-priority overrides.

```nix
# hosts/osaka/default.nix
{
  aggregation = {
    base.enable = true;
    desktop.enable = true;
    gaming.enable = true;
    shell = {
      enable = true;
      compositor.provider = "hyprland";
    };
  };

  # What no group speaks for, and this machine's exceptions.
  dendrites = {
    openai.enable = true;
    gpu = {
      enable = true;
      provider = "amd";
    };
  };

  users.khoa = {
    definition = ../../users/khoa.nix;
    homeManager.enable = true;
    # The person, not the machine: these groups contribute home lanes.
    aggregation = {
      base.enable = true;
      desktop.enable = true;
      shell.enable = true;
    };
    dendrites.pi-coding-agent.enable = true;
    homeManager.config = { pkgs, ... }: {
      home.packages = [ pkgs.ripgrep ];
    };
  };
  users.guest = {
    definition = ../../users/guest.nix;
    homeManager.enable = false;
  };

  nixos = { pkgs, ... }: {
    imports = [ ./hardware.nix ];
    networking.hostName = "osaka";
    environment.systemPackages = [ pkgs.filezilla ];
  };
}
```

Shared user definitions carry account lanes (`nixos`/`darwin`) and optional
`homeManager` preferences. Hosts attach those definitions; there are no copied
`hosts/<host>/users/khoa.nix` identities. A shared aggregation can attach a user
to several hosts when that membership is intentional. Adding another user is
one shared definition plus attachment to the relevant hosts or aggregation.

Selecting a system dendrite imports its system lane; selecting it under a user
imports its home lane. Neither automatically installs the other. Components
requiring both declare that relationship through aggregation selections, with
platform assertions for actual prerequisites. No hidden dependency walk.
The implementation must not automatically install both package lanes for the
same selection; explicitly selecting both remains possible and visible.

Host files show roles and exceptions; a derived inventory can expand the
resolved dendrites, providers, users, lanes, and source definitions for review.
It is generated from selection, never another manually maintained registry.

## Platform and package boundaries

| Consumer | Evaluated configuration |
|---|---|
| NixOS without HM | Nucleus NixOS lane, selected system lanes, account lanes, host NixOS settings |
| NixOS with HM | Above plus explicitly attached users' home lanes |
| nix-darwin | Darwin lanes and account configuration; optional compatible HM lanes |
| Standalone HM | Shared home preferences and selected home lanes; no system account creation |
| Plain nixpkgs/package consumer | Exported packages; no deployment module or service configuration |

An explicitly selected unsupported lane fails with a useful error. Missing
lanes are not filled with empty modules or silently skipped. HM-disabled hosts
do not import HM modules; requesting a home selection while HM is disabled is
a configuration error. A standalone HM constructor supplies its system/package
set and username/home-directory details explicitly.

Package choices use the consumer's channel by default. Integrated HM shares the
host package set when configured with `useGlobalPkgs`; standalone HM receives
its own selected set. Ordinary individual packages go directly in
`environment.systemPackages` or `home.packages`. `pkgs/` is for custom builds,
not a duplicate of nixpkgs's package catalogue.

External flakes are consumed through public packages, modules, and documented
library exports. No `inputs.aoide` private-tree imports or input rewrites are
needed. `follows` is explicit only where requested. A different provider module
revision uses a distinct pinned input and a selected catalogue/provider path;
changing a package version uses the module's package option or a selected
package override. These are distinct operations.

Selective evaluation does not guarantee independent locking: unrelated root
inputs can still fail during lock/fetch operations. Separate flake roots are
needed only when independent lock failure domains are required. Darwin package
and runtime support must be proved separately from exporting a Darwin lane.

## Lyra and songbook

Rice authoring is pure Nix plus owned QML/assets. Runtime JSON is generated.
`palette.nix` is a conventional local name; shared palette files may have any
name. Livery's palette contract does not require Stylix; Stylix is an optional
selected integration.

```text
shared palette + song widgets + composition + dependency references
                              |
                 selected available Nix-built bundles
                              |
          declared default / temporary stage / editable draft
                              |
             Lyra runtime and supported reload backends
```

Songs live under `song/songbook/<name>/`, and `song/covers/` sits beside the
songbook, not inside a rice: an image is not a look, and any rice may wear any
cover. The repository's `song/` mirrors the runtime `~/.aoide/song/` exactly;
`song/stage/` is the one directory that exists only at runtime, gitignored,
where lyra renders what programs watch and hot-reload. The wallpaper manager is
independent of individual rices. Each user has independent state.
Lyra ships reusable QML components and named bridges (including optional Aoide
integration); consumers customize widgets in their songbook without copying the
entire runtime. Importing Lyra's public module supplies capabilities without
installing every upstream song.

Several compatible bundles may be installed for immediate staging. Returning
to declared restores the declared bundle without deleting draft source.
Declaring promotes authored source to a songbook entry, not generated runtime
files. Host selection of that default is a separate source change.

Bound QML colors may hotload. Hyprland configuration switching uses an explicit
reload backend and managed configuration scope; host-owned settings are not
silently overwritten. Dependency availability does not mean every setting is
reloadable. Missing dependencies require an admitted deployment; unsupported
changes and partial reload failures must be reported. Full configuration
hotloading, draft persistence, and runtime path migration require their own
implementation proof, not just this Nix layout.

## Editing cost

| Change | Required edits |
|---|---|
| New dendrite | Its file or directory, one catalogue entry, selecting host, user, or group |
| New provider | Provider file and that dendrite's provider registry; selection where wanted |
| New group | Its directory `default.nix` — description, membership halves, provider choices. Discovery finds it; there is no import line anywhere |
| New supported lane | Dendrite/provider and consumers selecting that lane |
| Shared preference or package fix | Owning group's `default.nix`, in the matching half (`system` or `home`) |
| Capability-wide fix | One record under `modules/overrides/`. Discovery finds it; it applies to the hosts that selected a target |
| Host exception | Host selection or ordinary platform setting |
| New shared user | Shared user definition and intended attachments |
| New rice | Song source and relevant available/default selection |
| External version pin | Input/lock and affected provider/package selection |

## Build plan and acceptance gates

| Phase | Work | Required evidence |
|---|---|---|
| 1. Selection prototype | Catalogue, enable/provider options, aggregation bodies, deferred lanes, constructor | Disabled throwing implementation is never evaluated; unknown/missing provider and wrong lane have clear errors |
| 2. Vertical consumer slice | One local app dendrite, notification providers, one shared user; public Aoide/Lyra consumption | Host overrides group provider; system-only and HM variants evaluate; no private upstream paths |
| 2b. Override records | Capability-scoped records: discovery, matching, overlay and deferred lane application | Targeted and omitted host filters; unmatched throwing overlay and module uncalled; duplicate targets apply once; record schema typos fail on every host |
| 3. dxflake reconciliation | Move existing configuration without losing membership; restore agreed enable interface and shared users | Before/after package/service/user inventory; Osaka and Sakaki builds; existing pin failures resolved explicitly |
| 4. Live portability | User-admitted deployments and cross-host operation | Aoide tracking/conduct/mail and Lyra selected-song/runtime behavior on dxflake, with failures recorded |
| 5. AoideOS migration | Replace walkers, move source paths and docs together | Yomi build and user-admitted runtime proof; no accidental all-song/all-module loading |
| 6. Additional consumers | Standalone HM, Darwin where supported, package-only installation | Independent evaluation/build checks and honest platform support matrix |

Recorded on dxflake. All four hosts — chiyo, osaka, sakaki and yomi-strix —
evaluate to byte-identical system derivations before and after the migration.
`tests/selection/run.sh` passes 39 of 39, including cases proving that an
aggregation body which throws on import stays unread when nothing selects it,
that an unselected provider file stays unread, and that an unmatched override
record whose overlay and module both throw is never called — with the
complement, a matched record whose overlay really is the throwing one, so the
first is not passing vacuously. `tests/templates/run.sh` passes 28 of 28: it
assembles a whole tree out of `templates/`, resolves two hosts against the real
constructor, checks that the files nobody selected stayed unread, and runs the
override template's `nixos` half through the real NixOS module system so "real
options" is checked rather than claimed. The override mechanism costs nothing
where no record exists: all four hosts stay byte-identical with it in place.
This is evaluation evidence, not runtime activation proof; phases 4 and 5 still
owe that. Phase 2's "no private upstream paths" clause is
also still open: dxflake reaches into the Aoide input's own tree for modules and
songs until the matching public exports exist, and names that seam in its
`flake.nix`.

Prototype tests also cover conflicting aggregation defaults, false overriding
default true, two users with different providers, HM absent, and selected
external dependency isolation. Do not infer runtime proof from evaluation.

The constructor's executable schema — user account fields, diagnostics, and
public output names — is settled by dxflake's implementation and the suites
above, and the example syntax in this document is the syntax they exercise.
AoideOS migration work is audited against this target, not accepted merely
because it removed walkers.

## Related documents

- [Composition, Lyra, and muse triad](../Aoide-Wiki/references/aoide-composable-system-and-muse-triad.md): broader runtime and integration context.
- [Task register](TASK-REGISTER.md): execution history and evidence; not a substitute for this design.
- [Package layout](PACKAGE-LAYOUT.md): Aoide/Lyra binary boundary.
- dxflake `templates/`: one copyable example per authoring role — dendrite, provider, provider registry, aggregation, host, user, nucleus module, package — with `templates/README.md` mapping role to destination. `tests/templates/run.sh` builds a tree out of them, so they cannot drift from the constructor.

Mneme/Melete integration decisions remain in their dedicated proposals. Neither
is required for an agent to understand or operate this tree: lean root agent
instructions point to directory READMEs and this architecture, and leaf modules
carry their local contracts.
