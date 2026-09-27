# AGENTS.md — modules/nucleus

Points up to `modules/AGENTS.md` for the cross-module invariants (aggregate
discipline, `_`-prefix shelving, the closed read whitelist) — this file
covers only what's specific to nucleus.

## Invariants

- **Discovered unconditionally — no `mkIf` guard.** Unlike a dendrite, a
  nucleus file is not opt-in per host; it applies everywhere. A new nucleus
  file is core plumbing, not a feature toggle.
- **`options.nix` is the one file every other module builds against.**
  Widening the read whitelist (`aoide.livery`/`aoide.arrangement`/
  `aoide.surfaces`) is a decision that touches root `AGENTS.md` house rule 5
  too — don't add a fourth namespace without updating both.
- **Core options live in the core flake; nucleus declares only the paint
  half.** `enable`/`root`/`checkout`/`auditLog`/`terminal`/`user`/
  `sessionTarget` are
  `pkgs/aoide/module/options.nix`'s contract now, pulled in by the nucleus LANE
  (`lib/aoideos.nix`'s `nucleusModule`) — the one place that closes over Aoide's
  own flake inputs, since neither a file here nor an `imports` list can read a
  module argument that `_module.args` supplies. A new CORE
  option (portable, cargo-buildable, no `modules/`/`song/` reach) is added
  there, never here; a new PAINT option (livery, arrangement, surfaces, an
  enable fact, a lane toggle) is added here, never there. A core door toggle whose unit
  has not migrated yet (`mcp`, `a2a`, `usage`, `secrets`, `pairing`) stays
  here beside its unit — core, not paint, just not yet relocated.
- **Nucleus reads FACTS, never a lane's option.** The enable facts
  (`aoide.{quickshell,lyra,stylix,compositor,greeter}.enable`, each
  defaulting `false`, and `aoide.quickshell.config`, defaulting `null`) are
  declared here; the lane that owns one sets it with `mkDefault`. A nucleus file that needs to
  know "is there a shell / a session here" reads the fact — never
  `aoide.<name>.enable`, never a lane's own option. `aoide.sessionTarget` is the
  same rule from the other side: it is core-declared
  (`pkgs/aoide/module/options.nix`) and a painting lane sets it, so the unit
  that anchors on it need not know which lane did.
- **Nix authors runtime config; it never owns it (`config.nix`, task #135
  P-C).** A CORE command's configuration lives in the portable runtime file
  (`$AOIDE_ROOT/config.toml`, CONTRACTS.md §4) because core is
  cargo-buildable on any Linux — `config.nix` renders that file to a
  read-only store path and POINTS `AOIDE_CONFIG` at it, whole-file or
  nothing. Don't add a NixOS option for a new core setting: add a
  `config::SCHEMA` key in `aoide-storage` and let `aoide.config.settings`
  carry it like every other key. And don't grow partial management (nix
  owning one section, the CLI another) — two writers on one document is the
  split-brain the point-don't-copy design exists to avoid.
- **Changes here land by ordered lane, not by rice edit** (root `AGENTS.md`
  house rule 1 — a rice agent writes only `song/songbook/<song>/`; everything
  outside `song/`, this directory included, changes only on a lane the User
  ordered, with that lane's scope named). A rice agent proposing a nucleus
  change writes the diff and gets it reviewed/merged the normal way, same as
  any other core-structure change.
- **A user unit gets no polkit session.** Anything spawned from a
  `systemd.user.services.*` here lands outside `session-N.scope`, so polkit
  resolves no session for it and `allow_active` never fires — the action
  falls through to `allow_any` and is refused for want of interactive auth.
  A privileged action reached that way needs an explicit
  `security.polkit.extraConfig` rule keyed on `config.aoide.user`, declared
  in the same file as the unit (`modules/dendrites/lyra/shellbridge.nix` and
  its power actions).
  `pkcheck --action-id <id> --process <pid>` is the check: exit 0 is
  authorized, exit 2 is the refusal.

## Extension points

- **A new piece of core plumbing** (a new always-on service, a new option
  namespace) is a new file here, following the existing pattern: a header
  comment stating scope and what it reads, `mkOption`/`mkEnableOption` as
  needed, plus one line in `default.nix`.

## Docs update required in the same commit

- This `README.md` when a new nucleus file or option namespace is added.
- `CONTRACTS.md` — the livery schema when `options.nix`'s livery contract
  changes shape, §0 when the enable facts or `aoide.quickshell.config` do.
