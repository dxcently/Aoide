---
type: reference
created: 2026-08-30
updated: 2026-08-30
tags: [aoide, plugin, nix, architecture, philosophy, protocol]
---

# Principles — what to build, and what shape it takes

Two doctrines govern every design decision in Aoide. This page states them
as instructions you can check a design against. Dependency of [[DEV]].

`CONTRACTS.md` §0 in the repo holds the full statement and its citation;
`AGENTS.md` house rule 7 holds the binding form. This page is the working
distillation, not a replacement for either.

---

## Everything is a plugin

**A capability enters by existing at a conventional path, declaring what it
needs by name, and being removable without a trace. Nothing enters by being
added to a list.**

The repo already runs this way: the walkers discover `modules/dendrites/*`,
`modules/facets/*`, `pkgs/*` and `song/songbook/*/rice.nix` by walking the
tree. **Adding a capability is a new folder, never an edit to an import
list.**

### Instructions

- **Place it, do not register it.** Put the new thing at the conventional
  path. If an author must edit a registry to be seen, the design is wrong.
- **Self-gate.** A module switches itself on from its own `enable` or its
  own declaration. Never switch a module on from outside it.
- **Depend by name, never by import.** Ask for a service, a slot, a stage
  file, or a schema entry. Never import another module's implementation.
- **Read only what you are given.** A facet reads the closed namespace
  whitelist and nothing else. No module reads another module.
- **Give every effect an inverse.** A change that cannot be backed out is
  not finished.
- **When a design choice is open, take the one that can be deleted.**

### Forbidden, concretely

- a registry an author must edit to be seen
- a module reaching into another module
- a capability that exists only inside one consumer
- an effect with no inverse

### The corollary: surfaces paint, they are never the capability

QML picks up an agnostic bridge by name. State, policy, IPC and system
access live behind something reachable **with only a shell**.

**A new API lands as a bridge first; the surface picks it up second.** Never
the reverse, never only in the surface.

> **The test:** delete every `.qml` in the repo. Is this capability still
> reachable from a terminal? **No means it is in the wrong place.**

A surface may read, arrange, animate, and draw. It may not own the only
copy of a fact.

---

## Nix's thesis, applied above the nix layer

Aoide is nix-shaped in its reasoning even where no nix runs — core is
cargo-buildable on any Linux and shells out to no nix tool. The thesis is
**declarative, additive, atomically reversible**, and it governs the Rust
as much as the modules.

### Instructions

- **Declare the end state; do not script the steps.** Say what exists, not
  how to make it exist. A procedure that installs is a step; a declaration
  that something is so is a state, and a state can be diffed, checked, and
  reverted.
- **Be additive.** New capability, new file. Never edit in place what
  another author owns; never mutate a shared list to make room for
  yourself.
- **Make every artifact immutable, and switch the pointer.** Produce the
  new thing, then point at it. Never mutate the live one. The previous one
  surviving *is* the rollback.
- **Name every dependency.** What you need, you ask for by name. No ambient
  state, no implicit ordering, no reaching for what happens to be present.
- **Same inputs, same outputs.** Nothing that claims to be reproducible may
  read the clock, the network, or unset state.
- **Write state atomically.** Compose the whole new value, then replace in
  one step. A half-written state file is a corrupted one.
- **Compose small units; do not configure large ones.** Two things that
  combine beat one thing with a switch. A knob nobody will turn is debt.
- **Immutability is provenance.** When a file's source must be
  unambiguous, make it unwritable rather than stamping it with a marker
  field. A marker can lie or go stale; a read-only path cannot.

---

## The two axes, as working tests

Taken from Cordis, the plugin kernel these doctrines are cited from. Use
them as questions against a design, not as vocabulary.

**Spatial — can it be composed?**

> Does this component declare its dependencies by name and wait for them,
> or does it import an implementation?

**Temporal — can it be unwound?**

> If this component is removed, does everything it installed unwind with
> it?

A design that fails either is not finished, whatever it does when it runs.

---

## When the doctrines are in tension

They rarely conflict, and when they appear to, the conflict is usually a
missing seam rather than a real trade-off. Resolve in this order:

1. **Reversibility wins.** An irreversible effect is the one defect neither
   doctrine tolerates.
2. **Then removability.** Prefer the design whose deletion is cleanest.
3. **Then reach.** Prefer the design that works with only a shell, on a
   machine with no nix and no desktop.

If a design needs an exception to any of this, the exception is a decision
worth writing down. State it where the code lives, in one line, as current
fact.
