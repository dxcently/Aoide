---
type: entity
created: 2026-07-25
updated: 2026-08-28
tags: [aoide, daemon, orchestrator, policy]
source: "[[references/AOIDE-HANDOFF]]"
---

# aoided

The orchestrator daemon at the core of the Aoide framework. It emits a neutral event stream that thin per-agent adapters consume — the melete-adapter, for example, translates events into job dispatches. Subscriptions are default-deny per event class, which prevents OSD noise from burning agent runs and keeps forwarded notification text from reaching an agent as commands.

Policy and lint enforcement live in aoided, not in individual adapters or the CLI. A single audit log at `$AOIDE_ROOT/log` (default `~/.aoide/log`) captures all operations from every door — the CLI, the MCP façade, and the [[A2A-Door]] (each tagged `Door::Cli`/`Door::Mcp`/`Door::A2a`) — and no door bypasses the gate or writes a separate log.

The gated rebuild pipeline resides here. Rebuilds are User-gated (polkit pipeline pattern): the agent proposes, the User admits, git records. `aoide update` follows the same pattern — it fetches upstream, merges framework paths, runs checks, then proposes the rebuild rather than applying it immediately.

Security boundary: forwarded notification text is treated as untrusted input. Adapters wrap it as data; an app title must never reach the agent as an instruction.

## Implementation

`aoided` ships as the second binary of the [[aoide-cli]] crate — `aoide daemon`
and the standalone `aoided` reach the same code path. It runs as the `aoided`
systemd **user** service (keyed into `graphical-session.target`), configured
through four environment seams: `AOIDE_AUDIT_LOG` (the single audit-log path, from
`aoide.auditLog`), `AOIDE_ROOT` (the runtime root, from `aoide.root`),
`AOIDE_FLAKE_ROOT` (the dev checkout), and `AOIDE_USER`. The daemon resolves
the log in order: `--audit-log` flag → `$AOIDE_AUDIT_LOG` → `$AOIDE_ROOT/log`
(when `$AOIDE_ROOT` is absolute) → `<home>/.aoide/log`, where home is
`$AOIDE_USER` → `/home/<user>`, else `$HOME`.

The real policy-surface code paths: the audit log is **JSON-lines** (one
record per append, tagged with door / event class / status), appended for
every dispatch through either door. The neutral event stream is a real
default-deny-per-class type (classes: audit, gate, rice, content,
notification) — a forwarded notification is denied unless `Notification` is
explicitly subscribed, and even then it is carried as opaque `untrusted_data`,
never executed. The user rebuild gate is **propose-only**: `propose()` records
the proposal un-admitted; there is deliberately no `admit()` reachable by an
agent, so the daemon can never auto-admit a rebuild.

The daemon is the session-identity authority (identity lane #63,
[[Session-Graph]]). It mints an ephemeral ed25519 keypair once per process,
held in memory only — deliberately separate from `state/identity/`'s
on-disk node-wire key, which any same-uid reader could sign with — and
seals every live, pid-carrying session record with it: at `session start`
dispatch, and within one ~1s tick for records registered directly (the
`seal_unsealed_live_sessions` sweep). Verification asks the daemon itself:
its `ping` reply's `sealPubkeyHex` field is the only channel a verifier
fetches the current seal pubkey over. The daemon's own dispatch socket
reads `SO_PEERCRED` on accept and refuses a cross-uid connector outright —
the same fail-closed cross-uid floor shellbridge's socket holds (P-ID3), no
same-uid guarantee under the lane's answered threat model (OQ1-A).

## Related

- [[Desktop-Architecture]]
- [[shellbridge]]
- [[Session-Graph]] — the sealed session credential this daemon mints
- [[Governance]]
- [[Agent-Interface]]
- [[aoide-cli]]
- [[Codebase]]
