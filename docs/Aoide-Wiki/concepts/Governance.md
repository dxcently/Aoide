---
type: concept
created: 2026-07-25
tags: [aoide, governance, policy]
source: "[[references/AOIDE-HANDOFF]]"
updated: 2026-07-28
---

# Governance — Gates, Contracts, Audit

## Rebuild gate

Every NixOS rebuild is **user-gated**. The pattern follows sakaki's agent-sudo design (polkit pipeline): the agent proposes, the user admits, git records the result. No rebuild happens in the background or without explicit user approval. By default the agent builds and `test`-rebuilds freely, then prompts the human to `switch` under their own `sudo`. **Status:** a specified, opt-in `aoide.rebuild` capability grants a passwordless, narrowly-scoped path without loosening the approval gate; it is not yet declared in the option contract. See [[Rebuild-Gate]] for the full mechanism.

## Single policy surface

Policy, lint, and audit all live in [[aoided]] core. Both the CLI and MCP doors inherit the same gate and write to the same audit log at `~/Aoide/log`. There is no separate audit path per interface — divergence between the two doors is structurally impossible.

## Contractual core stability

Core interfaces are versioned contracts, not conventions:

| Interface | Stability guarantee |
|---|---|
| drachma schema | Versioned in `CONTRACTS.md` |
| Dendrite shape | Versioned in `CONTRACTS.md` |
| `aoide schema --json` output | Versioned in `CONTRACTS.md` |
| Stage file formats | Versioned in `CONTRACTS.md` |

The flake's `checks` fail a merge that breaks any of these. `aoide update` detects contract bumps during upstream merge and routes them through the update playbook before the rebuild discovers them — today the verb is a schema-real, exit-64 stub (arg-parsing and audit trail exist; the merge/detection logic does not yet run).

## Merge hygiene

Provenance governs ownership. Merge-base divergence lint inside `aoide update` detects when a personal branch has edited inherited upstream files, backed by a commit-hook warning at write time; neither is implemented yet (no such hook is installed in `.git/hooks/`, and `update` itself is the stub above). Path guards (directory-level access controls) are not used: they create false security without solving the problem.

## No background self-updaters

Neither Aoide nor Melete run background self-update processes. All updates — framework merges, flake.lock bumps, drachma-schema migrations — are proposed by the agent and applied only through the gated rebuild. This is house policy, not a configuration option.

## Related

- [[aoided]]
- [[Fork-and-Run]]
- [[Agent-Interface]]
- [[Content-Pipeline]]
