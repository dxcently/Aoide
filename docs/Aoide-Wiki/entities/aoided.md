---
type: entity
created: 2026-07-25
tags: [aoide, daemon, orchestrator, policy]
source: "[[references/AOIDE-HANDOFF]]"
---

# aoided

The orchestrator daemon at the core of the Aoide framework. It emits a neutral event stream that thin per-agent adapters consume — the melete-adapter, for example, translates events into job dispatches. Subscriptions are default-deny per event class, which prevents OSD noise from burning agent runs and keeps forwarded notification text from reaching an agent as commands.

Policy and lint enforcement live in aoided, not in individual adapters or the CLI. A single audit log at `~/Aoide/log` captures all operations from both the CLI and MCP doors — neither door bypasses the gate or writes a separate log.

The gated rebuild pipeline resides here. Rebuilds are User-gated (polkit pipeline pattern): the agent proposes, the User admits, git records. `aoide update` follows the same pattern — it fetches upstream, merges framework paths, runs checks, then proposes the rebuild rather than applying it immediately.

Security boundary: forwarded notification text is treated as untrusted input. Adapters wrap it as data; an app title must never reach the agent as an instruction.

## Implementation (walking skeleton, commit f3ceadf)

`aoided` ships as the second binary of the [[aoide-cli]] crate — `aoide daemon`
and the standalone `aoided` reach the same code path. It runs as the `aoided`
systemd **user** service (keyed into `graphical-session.target`), configured
through two environment seams: `AOIDE_AUDIT_LOG` (the single audit-log path, from
`aoide.auditLog`) and `AOIDE_USER`. The daemon resolves the log in order:
`$AOIDE_AUDIT_LOG` → `/home/$AOIDE_USER/Aoide/log` → `$HOME`-derived.

The skeleton wires the real policy-surface code paths: the audit log is
**JSON-lines** (one record per append, tagged with door / event class / status),
appended for every dispatch through either door. The neutral event stream is a
real default-deny-per-class type (classes: audit, gate, rice, content,
notification) — a forwarded notification is denied unless `Notification` is
explicitly subscribed, and even then it is carried as opaque `untrusted_data`,
never executed. The user rebuild gate is **propose-only**: `propose()` records
the proposal un-admitted; there is deliberately no `admit()` reachable by an
agent, so the daemon can never auto-admit a rebuild.

## Related

- [[Desktop-Architecture]]
- [[shellbridge]]
- [[Governance]]
- [[Agent-Interface]]
- [[aoide-cli]]
- [[Codebase]]
