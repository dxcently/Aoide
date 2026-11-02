---
type: concept
created: 2026-07-25
tags: [aoide, governance, policy]
source: "[[references/AOIDE-HANDOFF]]"
updated: 2026-08-27
---

# Governance — Gates, Contracts, Audit

## Rebuild gate

Every NixOS rebuild is **user-gated**: the agent proposes, the user admits, git records the result. The agent commits locally or to a branch. The user's push of a deploy branch is the admission, and every host that deploys from that branch through comin rebuilds from the push; dxflake's `main` is one. The agent pushes a deploy branch only when the user explicitly asks for that push, and any other `switch` is the user's own, under their own `sudo`. **Status:** a specified, opt-in `aoide.rebuild` capability grants a passwordless, narrowly-scoped path without loosening the approval gate; it is not yet declared in the option contract. See [[Rebuild-Gate]] for the full mechanism.

## Single policy surface

Policy, lint, and audit all live in [[aoided]] core. All three doors — the CLI,
the MCP façade, and the [[A2A-Door]] — inherit the same gate and write to the
same audit log at `$AOIDE_ROOT/log` (default `~/.aoide/log`, the
`aoide.auditLog` option), each dispatch tagged with the door it came
through (`Door::Cli` / `Door::Mcp` / `Door::A2a`). One audit path for every
interface: divergence between the doors is structurally impossible.

The [[Secrets-Broker]] mirrors its own resolve/put/approve/dismiss
decisions into the same log, tagged `Secret`, broker-side only — a value
itself never rides an audit line, only the grant/deny outcome and the
names involved. Its gate is a distinct mechanism from the rebuild gate
below: a codeless TOTP resolve PARKS a connection open until a separate
`secrets approve`/`dismiss` call or a timeout, rather than admitting at
rebuild time.

## Admission across the doors

The gate meets the interactive doors as a per-operation prompt (the agent
proposes, the user admits). The [[A2A-Door]] cannot: a JSON-RPC
request/response cannot block on a human clicking "approve", so its admission
moves entirely to rebuild time. Enabling the door and setting its spawn
target are rebuild-gated nix options (`aoide.a2a.enable` /
`aoide.a2a.spawnAgent`), so turning them on is the user's admission, made once.
Per request the door is bounded — a spawn runs only the configured agent,
and a forwarded message is data, never executed — and every inject, spawn, and
error is audited as `Door::A2a`, loopback by default.

## Contractual core stability

Core interfaces are versioned contracts, not conventions:

| Interface | Stability guarantee |
|---|---|
| theme schema | Versioned in `CONTRACTS.md` |
| Dendrite shape | Versioned in `CONTRACTS.md` |
| `aoide schema --json` output | Versioned in `CONTRACTS.md` |
| Stage file formats | Versioned in `CONTRACTS.md` |

The flake's `checks` fail a merge that breaks any of these. `aoide update` detects contract bumps during upstream merge and routes them through the update playbook before the rebuild discovers them — today the command is a schema-real, exit-64 stub (arg-parsing and audit trail exist; the merge/detection logic does not yet run).

## Merge hygiene

Provenance governs ownership. Merge-base divergence lint inside `aoide update` detects when a personal branch has edited inherited upstream files, backed by a commit-hook warning at write time; neither is implemented yet (no such hook is installed in `.git/hooks/`, and `update` itself is the stub above). Path guards (directory-level access controls) are not used: they create false security without solving the problem.

## No background self-updaters

Neither Aoide nor Melete updates itself. All updates — framework merges, flake.lock bumps, theme-schema migrations — are proposed by the agent as commits and applied only through the gated rebuild: the user's push of a deploy branch, or the user's own `switch`. A host's comin deploys only what the user pushed. This is house policy, not a configuration option.

## Related

- [[aoided]]
- [[Clone-and-Run]]
- [[Agent-Interface]]
- [[A2A-Door]]
- [[Secrets-Broker]]
- [[Content-Pipeline]]
