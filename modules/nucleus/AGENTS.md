# AGENTS.md — modules/nucleus

Points up to `modules/AGENTS.md` for the cross-module invariants (aggregate
discipline, `_`-prefix shelving, the closed read whitelist) — this file
covers only what's specific to the contract.

## Invariants

- **The contract declares and asserts; it never configures.** A file here
  declares options or checks an invariant over them, applied unconditionally on
  every host (no `mkIf` guard). A file that sets `environment.*`, `systemd.*`,
  `nix.*` or any other platform option is a dendrite, a member of the `nucleus`
  aggregation (`modules/aggregations/nucleus`), which a host can deselect. The
  test for where a thing goes: *must it be present on a host that selected
  nothing?* An option an unselected module may read: yes, here. Behaviour: no,
  a member.
- **`options.nix` is the one file every other module builds against.**
  Widening the read whitelist (`aoide.livery`/`aoide.arrangement`/
  `aoide.surfaces`) is a decision that touches root `AGENTS.md` house rule 5
  too — don't add a fourth namespace without updating both.
- **Core options live in the core flake; the contract declares only the paint
  half.** `enable`/`root`/`checkout`/`auditLog`/`terminal`/`user`/
  `sessionTarget` are `pkgs/aoide/module/options.nix`'s contract now, pulled in
  by the nucleus module (`lib/aoideos.nix`'s `nucleusModule`) — the one place
  that closes over Aoide's own flake inputs, since neither a file here nor an
  `imports` list can read a module argument that `_module.args` supplies. A new
  CORE option (portable, cargo-buildable, no `modules/`/`song/` reach) is added
  there, never here; a new PAINT option (livery, arrangement, surfaces, an
  enable fact, a lane toggle) is added here, never there. Door toggles and member
  settings are not declared here: each lives in the member that reads it
  (`aoided`: the doors; `aoide-secrets`: `members`; `aoide-config`: `settings`).
- **The contract reads FACTS, never a lane's option.** The enable facts
  (`aoide.{quickshell,strillix,stylix}.enable`, set only by their owner, each defaulting `false`,
  `aoide.quickshell.config`, defaulting `null`, and `aoide.wallpaper.provider` —
  the active provider's own name, defaulting `"quickshell"`) are declared here;
  the dendrite that owns one sets it `true`, and no host writes it. The owner is
  the file Aoide's own catalogue names for it (`options.nix`'s `mkFact`), so a
  consumer that maps `quickshell`, `stylix` or `strillix` to a file of its own is
  refused until that lookup takes the consumer's catalogue. A member dendrite or an assertion that needs to know "is there a
  shell / a session here" reads the fact — never `aoide.<name>.enable`, never a
  lane's own option. `aoide.sessionTarget` is the same rule from the other
  side: it is core-declared (`pkgs/aoide/module/options.nix`) and a painting
  lane sets it, so the unit that anchors on it need not know which lane did.
- **An option a member reads is declared in that member.**
  `aoide.config.settings` is declared in `aoide-config.nix`, `aoide.a2a.*` in
  `aoided.nix`, `aoide.secrets.members` in `aoide-secrets.nix`. The option exists
  only while its member is selected, so a host that sets one without selecting
  the member fails eval; `aoideOptions` lists it whether or not any host selects
  the member, since it evaluates the whole catalogue.
- **Changes here land by ordered lane, not by rice edit** (root `AGENTS.md`
  house rule 1 — a rice agent writes only `song/songbook/<song>/`; everything
  outside `song/`, this directory included, changes only on a lane the User
  ordered, with that lane's scope named). A rice agent proposing a contract
  change writes the diff and gets it reviewed/merged the normal way, same as
  any other core-structure change.

## Extension points

- **A new option namespace** is a new file here, following the existing
  pattern: a header comment stating scope and what it reads, `mkOption`/
  `mkEnableOption`, plus one line in `default.nix`.
- **A new always-on service** is not a file here: it is a dendrite, a
  catalogue line and one member line in `modules/aggregations/nucleus/default.nix`
  (`modules/dendrites/README.md`, "The core-service dendrites").

## Docs update required in the same commit

- This `README.md` when a new contract file or option namespace is added, or a
  member of the `nucleus` aggregation is added or removed.
- `CONTRACTS.md` — the livery schema when `options.nix`'s livery contract
  changes shape, §0 when the enable facts or `aoide.quickshell.config` do.
