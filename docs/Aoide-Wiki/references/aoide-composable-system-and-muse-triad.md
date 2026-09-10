# AoideOS composition, Lyra, and the muse triad

This is the consolidated architecture and implementation plan. It distinguishes agreed requirements, verified source behavior, and proposed contracts. It does not declare the migration implemented or authorize a rebuild. The detailed [question register](mneme-aoide-open-questions.md) and [Mneme proposal](mneme-shared-memory-and-aoide.md) remain companion documents; their unresolved questions are not silently closed here.

The implementation target is **AoideOS in the Aoide repository**, with **dxflake as the first external test consumer of the exported Aoide and Lyra cores**. The Nix composition tree, aggregates, platform lanes, users, package selection, and Lyra rice model below describe the AoideOS target. Implementation belongs here; dxflake validates consumption through public packages and modules without importing private module trees. Its wider configuration migration follows that consumer test. Aoide core's portable mail, identity, and context work remains independent of the AoideOS deployment migration.

## Agreed direction

- Aoide core is a shell-accessible, headless orchestration engine. Nix and Lyra are optional.
- Lyra owns rice composition and desktop rendering. APIs and state remain reachable through bridges without QML.
- Explicit aggregates and ordinary Nix lane registries replace global module-tree discovery. Unselected dendrites stay outside a host's module graph. Reading a selected registry or hashing the repository source is distinct from importing every implementation.
- A dendrite keeps its supported lanes together. Small implementations can live inside default.nix; substantial implementations can be extracted. Unsupported lanes need no empty files.
- Shared aggregates hold feature membership and mkDefault values one layer below hosts. Hosts show selected roles and exceptions. The proposed expanded inventory is derived, not manually maintained.
- System accounts and user configurations are separate. HM is optional for server deployment. Each graphical user has independent Lyra state.
- Rice definitions and composition are pure Nix; QML/assets remain implementation source. Runtime JSON is generated output, not a separately maintained authority.
- Rice parts are replaceable independently, with their dependencies resolved. A rice covers the desktop configuration within its supported backend scope, not only paint/widgets.
- Nix prepares the selected available rice bundles. Each user session has a declared default and can stage another available bundle without creating a draft or changing that default.
- Drafts are isolated editable candidates in the same Nix/QML source format as songbook entries. Preview activates a built candidate; declare promotes its source, not runtime output.
- songbook/covers is a shared library. Songs may name preferred covers or collections. The wallpaper manager is shared across rice selections, with separate per-user state.
- Same-persona sessions under Aoide share memories and history through Mneme across harnesses. Melete is optional; agents can access Mneme directly.
- Fetching mail produces a fetched receipt, distinct from filing/delivery and action/completion. Reading a receipt must not create another receipt.

The wallpaper override rule remains a proposed mechanism: follow the active song by default, retain an explicit user selection across rice switches, and provide an explicit return to following the song.

## Responsibility map

| Component | Owns | Depends on | Does not require |
|---|---|---|---|
| Aoide / aoided | Session and project coordination, conduct, mail, policy and audit at its API boundary | Shell/process adapters; transport for remote coordination | Nix, desktop, Melete, Mneme for basic conduct |
| Nix deployment | Selected packages, service identities, configuration, secret-file references, activation | Platform evaluator and selected modules | A running Lyra shell |
| Lyra | Installed rice compositions, staging, widget and wallpaper controls | Relevant bridges; graphical backend for rendering | Melete or Mneme for ordinary desktop use |
| Mneme | Vault access and semantic retrieval today; shared persona memory/history by requirement | Authorized storage and configured embedding model when embeddings are used | Melete, Lyra |
| Melete | Accepted jobs, scheduler, worker lifecycle, app host and gateway | Runtime dependencies and configured endpoints | Lyra; Aoide for its independent operation |
| Git/source checkout | Canonical code and reviewed project documents | Repository tooling | Vector index |

Aoide does not replace Melete's scheduler. Nix declares the Melete service; systemd supervises the daemon. Melete supervises its own jobs and hosted apps. Aoide dispatches and controls those jobs through Melete's lifecycle interface. Admission and execution audits remain distinct but correlated.

## Proposed source tree

~~~text
Aoide/
├── flake.nix                       outputs and platform constructors
├── hosts/
│   ├── yomi-strix/default.nix      roles, hardware, local service values
│   ├── osaka/default.nix          roles, users, rice/package exceptions
│   └── sakaki/default.nix         server roles, storage and public routes
├── users/
│   ├── khoa.nix                   account and home lanes
│   └── guest.nix
├── modules/
│   ├── aggregates.nix            shared membership and defaults, by lane
│   ├── overrides/kitty.nix        optional shared exception, selected explicitly
│   ├── nucleus/default.nix        minimum shared contracts/foundation
│   ├── dendrites/
│   │   ├── kitty/default.nix      lane registry, small modules inline
│   │   ├── melete/default.nix
│   │   ├── mneme/default.nix
│   │   └── cloudflared/default.nix
│   └── facets/                    existing paint adapters; migration owned explicitly
├── songbook/
│   ├── covers/
│   ├── sonata/{default.nix,widgets/,design/}
│   └── nocturne/{default.nix,widgets/,design/}
└── pkgs/                          real package builds and shared overrides
~~~

This is a target tree. It does not rename existing files by assertion. In particular, song/songbook paths, rice.nix modules, packaging, runtime seed paths, and ownership documentation must migrate together. Splitting larger implementations into home.nix/nixos.nix/darwin.nix remains optional. A plain packages.nix catalogue is only warranted for shared custom choices or overrides; nixpkgs already supplies ordinary individual package entries.

A registry is an ordinary attribute set whose values are Nix modules or module paths:

~~~nix
# A dendrite's default.nix
{
  nixos = { ... }: { /* system integration */ };
  homeManager = ./home.nix;
}
~~~

Aggregates import only needed registries and select their lanes. NixOS uses nixos; HM uses homeManager whether standalone or attached to NixOS/Darwin; Darwin system settings use darwin. A registry is not itself a NixOS module. No custom recursive loader is necessary. Imports are selected independently of the module config fixed point; enable=false cannot retroactively undo an import.

The shared nucleus must contain only requirements of every selected consumer, not a hidden desktop or Melete installation. An optional dependency belongs in a selected aggregate/dendrite. Existing facets retain their closed livery/arrangement/surfaces contract until an explicit amendment moves or replaces it.

## Hosts, users, packages, and platform outputs

~~~text
Host roles ──> selected system aggregate lanes ──> system services/packages
     │
     └─ user attachments
          ├─ shared identity/preferences
          ├─ selected HM aggregate lanes
          └─ host-specific user overrides ──> per-user configuration/state
~~~

A system without HM selects only system lanes. A user can run standalone HM on a compatible existing OS. A plain package consumer can install an exported package without any module; that does not configure its services or secrets. programs.*.enable and services.*.enable belong to their evaluator, often install their own packages, and should not be duplicated behind redundant aoide.* flags.

The proposed repeated-workstation grouping can be selected by Yomi, Osaka, and Chiyo. Each keeps its own tunnel identity and hardware values. Sakaki selects a server aggregate and its own hosted-service bundles; it need not import the workstation graph. This is a proposed grouping, not current deployed membership.

Osaka can select workstation/gaming system lanes, Khoa's workstation/gaming home lanes, and a simpler Guest home. Khoa can use Nocturne and Guest Sonata while sharing cover files. System packages go in environment.systemPackages; user additions go in home.packages. The existing singleton aoide.user deployment cannot implement this merely by duplicating user names.

The shared remote-access aggregate supplies Cloudflared implementation and SSH routing defaults. Host tunnel UUIDs and credential-file references remain explicit. Sakaki's credential file is currently a module default in dxflake and must move into Sakaki's configuration. An optional web-exposure aggregate can derive tunnel web hostnames from its exposed Caddy site map, avoiding two manually synchronized lists. Gateway access policy remains separate from route existence.

| Change | Expected editing surface |
|---|---|
| Add a feature shared by three hosts | New dendrite registry and relevant shared aggregate membership |
| Add a system/user lane to a feature | That registry/implementation and selecting aggregate |
| Add a plain package to one host/user | That host/user package selection |
| Add a custom package shared across hosts | Package build/override and selected consumers or shared aggregate |
| Add a user | User registry and account/HM attachments on selected hosts |
| Change a host's hardware, tunnel, or rice preference | Host configuration |
| Add a rice to a shared installed set | Song definition/assets and selecting aggregate |
| Add an external flake dependency | Relevant input/lock plus selected consumer; not universally a one-file change |

Package selection follows the host's chosen nixpkgs by default. Integrated HM uses the host package set when useGlobalPkgs is enabled; standalone HM receives its selected set explicitly. Prefer upstream module defaults and existing package options over redundant wrappers. External flake follows declarations are explicit, never generated automatically. Shared package exceptions are ordinary modules imported only by affected hosts/aggregates; mkDefault makes a shared preference overridable, while a normal-priority assignment conflicts with a competing normal assignment.

A package override changes an executable; selecting a different module implementation happens before imports are assembled. Separate named inputs can pin different revisions. Selective module evaluation does not isolate the root lock graph: lock resolution or broad flake checks can still encounter unrelated inputs. Separate root flakes are only needed if independent dependency-lock failure domains are required.

Required outputs include exported modules, NixOS configurations, standalone HM configurations, nix-darwin configurations where supported, and packages. Adding output constructors does not port Linux-specific Rust APIs or Wayland widgets to macOS. Darwin runtime portability is a separately verified workstream.

## Lyra composition, staging, and complete configuration activation

Nix owns the authored composition. Lyra activates built configurations through supported backends. The system is declarative at the source boundary, not implemented exclusively in the Nix language: QML renders, bridges operate capabilities, and the runtime applies transitions.

| Object | Meaning and authority |
|---|---|
| Songbook entry | Declared Nix/QML/assets source, with explicit dependency references |
| Available bundle set | Selected configurations built/installed by Nix for this user/session |
| Declared selection | Configured default, with a retained built bundle for return |
| Stage mode | Temporary activation of an available rice; no draft or source change required |
| Draft | Isolated editable candidate in the same source format as a songbook entry |
| Active bundle | Generated configuration currently being applied; output rather than authoring authority |
| Runtime state | Selection markers and backend state; distinct from song source and application data |

~~~text
Host + user select aggregates, available songs and default
                           ↓
               Pure Nix composition evaluation
                 ├─ paint / cover preferences
                 ├─ widget bodies / layout / surfaces
                 ├─ supported compositor and app settings
                 └─ packages / assets / required capabilities
                           ↓
                  Available built bundles
                 Sonata · Fugue · Nocturne
                           ↓
                  Lyra activation coordinator
                 ├─ declared: configured default
                 ├─ stage: temporary available selection
                 └─ draft preview: built candidate
                           ↓
                  Supported backend transitions
~~~

Several rices can be available; one resolved composition is active per graphical session in this scope. Switching compatible prebuilt bundles needs no fresh Nix evaluation. Stage mode selects a bundle rather than copying editable source. Opening an experiment creates/opens a draft; its Nix composition and owned QML can be edited, evaluated/built, and previewed. Declaring promotes candidate source into the songbook; selecting it as the host/user default and activating system changes are distinct actions.

Returning to declared reactivates the retained declared bundle and reapplies its managed settings. It does not reset Git, delete a draft, or touch mail, sessions, application data, or service databases. Discard is a separate operation. The exact rule for refreshing the retained declared bundle after source/default changes, and whether staged selection survives login/reboot, must be specified before implementation.

Borrowed parts retain source identity until explicitly customized. A local customization becomes an owned source variant and a composition-reference change; declaration must not silently mutate a borrowed song or flatten the whole shared shell into the rice. Shared shell loader/bridge changes remain implementation work, distinct from rice authoring.

The current shared QML source lives in modules/facets/quickshell/qml; its Nix facet builds/deploys that source together with song widget bodies into run/qml. Facet refers to the deployment/render integration, not a separate collection of runtime adapters hidden outside that QML source. Generated run/qml is not the proposed editing authority.

Dependency compatibility and activation support are separate checks. Required packages/services may already exist while a changed setting still requires a process restart. Each backend needs a supported configuration schema, scope, apply/reload/restart behavior, removed-setting handling, and failure/recovery behavior. Cross-process activation is not assumed atomic. Settings delegated to Lyra have one writer: HM/Nix installs that integration instead of independently overwriting the same leaves. Host-owned settings and system-service policy remain outside a rice's delegated scope.

| Transition | Activation boundary |
|---|---|
| Quickshell livery changes | Watched values update QML bindings; hardcoded colors do not follow automatically |
| Widget body/composition changes | Supported component recreation and surface cleanup |
| Compositor settings/bindings | Only supported commands or reload operations |
| Application configuration | Supported runtime update, reload, or restart |
| Missing dependencies/system changes | Prepare an admitted deployment before claiming full activation |
| Removal of a part | Release only runtime dependencies unused by the remaining composition |

Pure Nix can emit the complete bundle and required deployment data. It does not make every application setting hot-reloadable. Prebuilt bundles can run without Nix; editing/evaluating Nix-defined candidates requires an authoring/build environment.

The wallpaper manager is independent of selected rice and picker widgets. songbook/covers holds shared images; each user has independent state. The proposed rule follows a song's preferred cover/collection unless the user selected an explicit override. Whether cover selection derives palette remains open. Removing the picker does not terminate the manager.

Verified current gaps, not claims of implemented target behavior:

- Quickshell already watches staged livery and hotloads bound colors. Full dependency-checked configuration switching is additional work.
- Quickshell and lyra-songbook package all songs; checkout staging can regenerate whole-songbook metadata. Selected installation must apply consistently to every path.
- composeSong returns package metadata, but installation hardcodes packages. Sibling validation and implicit Sonata fallback disagree.
- Missing slots can fall back instead of staying absent. Fixed shell slots and dynamic surfaces have different instantiation paths.
- Runtime registries are generated separately from host arrangement overrides; borrowing-composition staging misses modified borrowed owners.
- Current drafts isolate livery/cover but share widget bodies. Declare recursively copies the runtime song tree without promoting active draft values into Nix, filtering draft/take directories, or removing obsolete destination files.
- Existing per-user deployment, source authority, reload outcome reporting, and core runtime-directory lifetime need explicit correction.

## Melete on an AoideOS server

Yes: a headless server aggregate can deploy Aoide, Melete, and optionally Mneme without the desktop. Nix configuration can describe the daemon package, service account, working/state directories, nonsecret TOML, environment/secret-file references, endpoint policy, dependencies, source selections, and Cloudflared/Caddy routing.

Current Aoide's Melete dendrite exposes paths and a service, not full settings/app deployment. Its package is a launcher targeting ~/.local/bin/melete, while activation seeds that launcher into the same default target. With default paths and no replacement/override, this self-execs. This is a source-derived deployment defect, not a reproduced running-server failure. A real pinned package must replace this seam. Melete already supports self_update.managed_externally=true for a deployment whose updates are admitted externally.

Melete concurrently hosts its scheduler, app host, gateway, and optional MCP server. Its gateway_apps setting controls visibility; it is not the desired deployed-app inventory. Current apps store source and mutable deployment/data files together. Declarative app deployment therefore needs a source/state separation and reconciliation contract: install selected source, adopt/update through Melete's lifecycle interface, preserve app state, and define removal behavior. Do not create competing systemd supervisors for Melete-owned apps. Arbitrary services outside Melete's app runtime stay ordinary service modules.

Mutual contributions:

| Direction | Capability |
|---|---|
| Aoide → Melete | Project/persona context, bounded work requests, operator admission, session correlation |
| Melete → Aoide | Durable run IDs, scheduled/chained execution, app hosting, status/artifacts/completion events |
| Mneme → both | Authorized knowledge, persona definitions, shared memory/history, semantic retrieval |
| Either agent/job → Mneme | Attributed results and memory updates through the agreed write contract |
| Bridges → Lyra | Job/project/history/index status for optional views |

Aoide already offers Melete status, generic tool calls, and a graph snapshot. Its event adapter is a skeleton. The snapshot is not yet a merged project/task model. Integration needs distinct project, persona, Aoide session, and Melete run identifiers; stable event/dispatch IDs; reconnect and duplicate handling; and clear steering/cancellation ownership. Basic Aoide tasks and Mneme access continue without Melete.

## Mneme codebase and comment extension

The inspected Mneme source embeds Markdown sections, persists content hashes and model metadata, queues changed notes, and exposes semantic queries. It does not provide a code-symbol-aware repository index merely because code can appear inside Markdown. The existing source review is Mneme 5512299; Noah's PR #49 is discussed separately at its previously reviewed 99a5aa9 head, not treated as merged or deployed.

A bounded proposed extension:

~~~text
Selected project checkout + revision/worktree content
                      ↓
       Code-aware extraction and change tracking
         ├─ functions/types/module structure
         ├─ existing comments and documentation
         └─ source anchors and content hashes
                      ↓
       Mneme searchable derived records/index
                      ↓
 Aoide query → relevant candidates → verify current source
                      ↓
    linked annotation or reviewable source-comment patch
~~~

Each code chunk needs project/repository identity, checkout revision (and content hash for dirty files), relative path, language, symbol when available, and line/byte range. An embedding score is a relevance signal, not identity, a call graph, or proof that an old line range still applies. Use lexical/symbol search alongside semantic ranking. Re-resolve and verify anchors before navigating or editing; renamed, deleted, and stale records need explicit invalidation/reconciliation.

Git remains authoritative for code and committed comments. Mneme stores derived searchable records and linked discussions/decisions. External annotations can reference code without editing it. Source comments are proposed patches with normal review. These two products must be distinguished before a coding brief. Retrieved comments are source data, not instructions granting execution authority.

The first slice can index repository documentation with current Mneme semantics and evaluate usefulness. Code indexing then adds language-aware extraction and source anchors for one selected repository/language before broad rollout. A model suited to prose is not assumed to rank code well; assess retrieval against known queries. Define index scope, grants, deletion behavior, freshness reporting, incremental updates, and storage limits. Generated outputs, dependencies, and secret files are excluded by explicit project indexing policy.

Melete may schedule ingestion/reindexing, but the indexer and Aoide query bridge must work directly without Melete. This extension does not require adopting the managed database or replica proposal. Shared persona history likewise requires its own storage/retrieval contract; an embedding index is not the history archive.

## Dependency plan and acceptance evidence

Labels below are planning workstreams; P-M5 remains the existing mail phase identifier.

| Workstream | Needed work | Prerequisites | Acceptance evidence |
|---|---|---|---|
| O0: coordination readiness | Inspect active paths, session tracking, socket lifetime, stale MCP compatibility, outstanding remote-mail gate | Current source/roster/mail | Correct active/idle state; live socket status; writer ownership; preserved existing work |
| O1: P-M5 and fetched receipts | Reader/session binding, reaction owner, durable latch/retry, prompt readiness, receipt boundary/dedup/recovery | O0 control path; D1–D4 and F1–F4 contracts | Two idle agents exchange/fetch mail; one latched nudge; receipt is distinct; busy/restart/disconnect cases; no receipt recursion |
| N1: selected Nix composition | Lane exports, aggregates, optional HM, multiple users, constructors, convention migration | One vertical-slice architecture brief | NixOS with and without HM; independent users; unselected throwing module not evaluated; no whole-tree walker |
| N2: Melete/Mneme server deployment | Real packages, service identities, generated settings, external update ownership, secret references | N1 lane contracts for target deployment | Headless service startup; endpoint health; admitted upgrade/rollback; HM absent |
| L1: composition and stage contract | Pure-Nix authority, available/declared/staged/draft states, widget closure, manifests and source promotion | Per-user contracts from N1; A1/A2/A5 | Switch prebuilt songs without eval; hotload QML colors; return to declared; isolated draft QML; excluded slot stays absent |
| L2: wallpaper and source-tree migration | Shared covers, independent manager, user overrides, migrate CLI/build/runtime paths | L1 ownership/activation contract | Rice switching preserves explicit cover; follow mode works; two users isolated; picker removal leaves manager |
| L3: complete configuration transitions | Backend capabilities, delegated setting ownership, removed-setting cleanup, reload/restart and recovery | L1 bundle/activation contract; installed backend integrations | Compositor/app changes apply as reported; removed settings are restored; unsupported transitions request deployment; partial failures are visible/recoverable |
| K1: shared persona context | Stable identity, history representation, grants, Melete-memory migration, turn retrieval/refresh | M1–M9 decisions; reachable Mneme | Claude/Codex same persona share authorized history with provenance; other persona isolated; freshness visible |
| T1: Melete orchestration adapter | Dispatch/run correlation, event ingestion, retries, steering/cancellation boundary | Verified Melete endpoint; lifecycle contract | Duplicate request creates one intended run; reconnect restores status; Aoide does not compete with scheduler |
| T2: declarative Melete apps | Selected app sources, adoption/update/removal, state preservation, routes | N2 and Melete app lifecycle contract | Source update preserves app state; restart restores membership; removal semantics verified |
| C1: code context pilot | Anchored extraction, hybrid retrieval, invalidation, annotations vs patches | Project identity and Mneme access/index policy | Known queries find expected symbols; edits/renames/deletions handled; stale anchors cannot silently edit wrong code |
| V1: Lyra observability | Project/run/history/index/mail views through bridges | Corresponding headless APIs above | Every displayed operation works without QML; widgets report evidence and freshness |

~~~text
O0 ──> O1 (P-M5 + fetched receipts)
N1 ──> N2 ──> T2
 │       └──> deployment for T1 / K1 / C1
 └──> L1 ──┬─> L2
           └─> L3

Melete endpoint + lifecycle contract ──> T1
Mneme access + identity/history decisions ──> K1
Mneme access + project/index contract ──> C1

T1 / K1 / C1 / O1 headless APIs ──> V1 optional views
~~~

O1 is needed for unattended agent-to-agent continuation, not for manually supervised Nix design or headless Melete operation. K1 and C1 share access/provenance foundations but neither requires waiting for the other's complete implementation. Darwin runtime work and managed-database/replica adoption remain separate optional branches. The architecture can be planned in parallel; writers, Cargo work, and activation are separately serialized/gated.

## Orchestration contracts

The main orchestrator owns decomposition, mail, ownership tracking, acceptance evidence, and integration. ARCHITECT resolves a bounded phase; EXECUTE receives a concrete brief; a separate REVIEW instance derives findings from source/diff. Every brief names owned paths, prerequisites, acceptance tests, and what it got wrong. Existing unrelated packaging changes remain outside these phases. No bulk migration is assigned from this document alone.

Claude's initial assignment is read-only readiness and review of mail/control lifecycle decisions. The Nix/Lyra and Melete/Mneme architecture can be inspected independently. Implementation starts from a settled vertical slice, not from every workstream at once. Cargo use is serialized and tests are scoped to touched crates. Rebuilds remain user-admitted. Filed mail, prompt submission, fetching, and acting/completion are separate observable states.

## Source anchors

- [Current assembly and global walk](../../../lib/mkHost.nix), [module conventions](../../../modules/AGENTS.md).
- [Current declare copy](../../../pkgs/aoide/crates/lyra/src/commands/stubs.rs), [mode-aware reload](../../../pkgs/aoide/crates/song/src/commands/reload.rs).
- [Widget composition](../../../lib/song.nix), [songbook generation](../../../lib/songbook.nix), [Quickshell deployment](../../../modules/facets/quickshell/default.nix).
- [Current Melete deployment](../../../modules/dendrites/melete.nix), [launcher package](../../../pkgs/melete/default.nix), [Melete client](../../../pkgs/aoide/crates/client/src/mcp_client.rs), [adapter skeleton](../../../pkgs/aoide/crates/client/src/adapter.rs).
- [Mail design](../../architecture/MAIL.md), [orchestration protocol](../protocol/dev/ORCHESTRATION.md).
- Local source: Melete 7619d40, src/main.rs (daemon), src/config.rs (apps and managed updates), src/app_registry.rs (source/state), src/mcp_server.rs (jobs).
- Local source: Mneme 5512299, src/embedding/sections.rs (Markdown units), src/embedding/mod.rs (scope/lifecycle), src/embedding/query.rs and src/main.rs (semantic retrieval).
- [Question register](mneme-aoide-open-questions.md) retains M1–M10, S1–S6, A1–A8, D1–D4, F1–F4, O1–O3. This plan does not substitute an answer for an unresolved ID.
