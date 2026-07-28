---
type: concept
created: 2026-07-26
updated: 2026-07-28
tags: [aoide, governance, policy, rebuild, security]
---

# Rebuild Gate — how agents rebuild the system

How an agent turns a proposed change into a running system, and where the human sits in that path. The rebuild is the one irreversible-feeling step in the [[Self-Ricing]] loop (`adopt` → commit → **rebuild**), so it is the most carefully gated. NixOS already makes it *recoverable* — every `switch` writes a new generation and the old one stays bootable — so the gate is about **authority and intent**, not about preventing damage.

## Two things the gate controls

The gate has two orthogonal jobs; keeping them separate is what makes the design safe:

1. **Authentication** — *may this process invoke a privileged rebuild at all?*
2. **Approval** — *should this specific rebuild happen now?*

By default the two collapse into one act: the human typing their sudo password is simultaneously the authentication *and* the approval. The `aoide.rebuild` capability (below) splits them, so authentication becomes passwordless without weakening approval.

## Default behaviour (capability disabled)

Until `aoide.rebuild` is enabled, rebuilds are handled the ordinary way, matching the house rule (*you propose; the user admits; git records* — see [[Agent-Interface]] and `AGENTS.md`):

- The agent builds and rehearses freely — `nixos-rebuild build` and `nixos-rebuild dry-activate` need **no privilege**, and `nixos-rebuild test` activates without touching the boot default (a reboot reverts it), so the agent can validate its own work end to end.
- To make it stick, the agent **stops and prompts the human**, who runs the `switch` under their own `sudo` and types their password. The agent never holds the password.

This is the safe default precisely because it is human-in-the-loop. It is also the *only* mode until the capability is deliberately turned on.

yomi-strix runs the Aoide flake live, with the prior [[dxflake]] generation retained in systemd-boot as the rollback (see [[Codebase]], [[Full-Architecture]]).

## The `aoide.rebuild` capability

**Status:** specified; `modules/nucleus/options.nix` declares no `aoide.rebuild` surface today, and the compositor facet marks the polkit prompt explicit future work.

The design grants the agent a **passwordless but narrowly-scoped** path to the gated verbs:

- A **dedicated no-login agent user** with no general `sudo` rights.
- The rebuild running as a fixed **systemd oneshot unit** (`aoide-rebuild-{test,switch}.service`) whose flake path, host, and verb are baked into the unit — the agent chooses *which unit to start*, never the command line.
- A **polkit rule** letting that user `systemctl start` **those units and nothing else**, no password — the same polkit pipeline [[Governance]] records as sakaki's agent-sudo design.
- Every invocation streaming through journald into the single [[aoided]] audit log (`~/Aoide/log`).

The capability changes only **authentication**; it does not create background rebuilds. The **approval** gate persists as policy: the design auto-admits `test` (reboot-recoverable); `switch` still routes through the admit door, keeping the *no background rebuilds, no self-updaters* house rule intact ([[Governance]]). Until built, all rebuilds use the default behaviour above.

## Why passwordless-narrow beats a sudo password

For an autonomous process a sudo password is not a real boundary — it authenticates *a human at a keyboard*, which the agent is not. It collapses to one of two bad cases:

- **The agent holds the password** (in a file/env var, so it can rebuild unattended) → the password now protects nothing, and you have added an exfiltratable secret that *also* unlocks every other `sudo` command. Strictly worse.
- **A human types it each time** → that is just the default mode above; the human, not the password, is the gate.

What actually contains a compromised agent is **how narrow the granted capability is** — one pinned unit, nothing else — combined with NixOS rollback and the audit log. So the capability is passwordless by design: security comes from scope + recoverability + audit, not from a typed secret.

## Rollback

Nothing here defeats recovery. Every `switch` is a new generation; the previous one stays in the boot menu and via `nixos-rebuild --rollback`. A bad `test` reverts on reboot. The gate governs *who may act and whether they should*; the generation history guarantees any act can be undone.

## Related

- [[Governance]]
- [[aoided]]
- [[Agent-Interface]]
- [[Self-Ricing]]
- [[Snowflake-Anatomy]]
