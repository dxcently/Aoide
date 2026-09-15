# External mail-only access

Status: proposed access profile and implementation plan. No endpoint or external
credential is enabled by this document.

## Purpose

An outside agent can send letters to permitted local recipients and fetch its
own incoming letters without acquiring a shell, filesystem access, general
Aoide control, or membership in the full host mesh. The receiving machine can
reply without needing inbound access to the outside agent's machine.

Reuse Aoide's mail store, envelopes, identity verification, queues, receipts and
audit. This is a restricted entry point to mail, not a second mail system.

## Existing foundation and gap

PAIRING.md defines the existing node `message` grant for depositing mail and
distinguishes it from spawn/conduct permissions. It also describes signed node
requests and SSH tunnelling. MAIL.md defines durable correspondence.

A node-level deposit grant alone does not establish mailbox-scoped remote
fetch, per-recipient restrictions, or a shell-free external access profile.
Those boundaries require implementation and tests. Existing `read` grants for
graph/session summaries must not be repurposed as permission to read mail.

MAIL.md explicitly treats local mailboxes as routing, not secrecy: agents with
the same Unix account can read the shared store. The proposed isolation is at
the external API/account boundary. It does not retroactively hide mail from
the local operator or other processes already authorized to read that store.

## Flow

```text
outside agent with an individual credential
                  |
        authenticated restricted transport
           /                       \
 SSH forced-command adapter     API through tunnel/TLS
           \                       /
          mail-only request dispatcher
                  |
      principal + operation + mailbox checks
                  |
        existing Aoide mail service/audit
           |                    |
 allowed local recipients    principal's inbox
           |                    |
       local agents       outside agent polls/fetches
```

Transport reachability never bypasses application authorization, including
connections arriving from loopback through a tunnel. The public entry point
has no generic CLI, tool-dispatch, file-path, or shell-command operation.

## Authority

Each external identity has its own credential binding and administrator-owned
policy. Reuse the existing principal/key model where it fits; an external
principal need not gain mesh topology visibility or node-management powers.

| Policy field | Meaning |
|---|---|
| Principal | Stable authenticated identity; not caller-supplied session name |
| Inbox | Explicit mailbox the principal may fetch |
| Send targets | Explicit local mailbox IDs it may address |
| Operations | Send, fetch, acknowledge, and status of its own submissions |
| Expiry/revocation | Admission checked on every request, including open streams |
| Limits | Request bytes, recipients, page size, outstanding mail and request rate |

The human setup flow is: create an external correspondent, choose its inbox and
allowed recipients, choose transport, issue a credential, test, revoke when
finished. Configuration must work outside Nix; Nix can declare the same policy
and restricted service/account setup. Never put private keys or bearer secrets
in the Nix store. Harnox may manage credentials through its integration, but
the mail profile must not introduce a new secrets backend.

## Allowed operations

Names below are contract descriptions, not implemented CLI/API names.

| Operation | Checks and result |
|---|---|
| Send | Validate sender binding and every To/Cc recipient; file using existing envelope/dedup rules; return submission ID/status |
| Fetch | Only the bound inbox; bounded cursor page; no global mailbase scan returned to caller |
| Acknowledge | Only messages issued to this principal; idempotent; never delete letters |
| Submission status | Only this principal's submissions; avoid leaking unrelated recipients/history |

Reject an unauthorized multi-recipient request before any deposit. A reply or
forward undergoes the same recipient checks; a thread ID is not authority to
read its entire history. Mail to another host/relay is excluded from the first
slice unless explicitly granted later. Mailbox names and cursors are opaque
identifiers, never filesystem paths. Cursor validation cannot grant access to
another inbox. Sender display labels do not override authenticated identity.

For pull delivery, a response being written to a socket is not proof the client
received it. Use explicit idempotent acknowledgement to advance the remote
reader's fetched state/receipt. Retrying a fetch may return the same letter;
it must not mint repeated receipts. Local CLI fetch semantics remain governed
by MAIL.md; document the remote acknowledgement boundary distinctly.

Doorbell behavior remains recipient-owned policy, not a way for the outsider to
submit arbitrary terminal input. A mail notification or receipt grants no
execution permission. Letter content is untrusted input to the receiving agent.

## Transport options

### First slice: SSH forced command

Use a dedicated restricted account/key with a fixed adapter executable. The
adapter accepts a bounded structured request on stdin and returns a structured
response. It never executes `SSH_ORIGINAL_COMMAND`, caller-selected binaries,
or interpolated shell fragments. Bind the authenticated key to a configured
principal; do not trust a principal name in the payload.

OpenSSH supports a forced `command` and the `restrict` authorized-key option,
which disables PTY, forwarding and user rc execution. A dedicated account must
also exclude alternate unrestricted authentication paths. Verify effective
sshd settings, file ownership, and subsystem rejection on the deployed version.
Do not expose a generic privileged Aoide socket to that account: the adapter's
backend access must itself remain limited and principal-bound.

This mode needs no SSH forwarding. It is intentionally separate from tunnel
mode: blanket forwarding restrictions would prevent a tunnel from working.

### Second slice: API through a tunnel

Expose the same restricted dispatcher over an authenticated encrypted route.
Reuse Aoide wire authentication where practical instead of inventing new
signatures. A tunnel must not expose the unrestricted daemon endpoint. Requests
still require a principal and mailbox policy after reaching the listener.

For SSH tunnel deployment, allow only the required local-forward destination,
deny remote forwarding and shell/session use, and test the effective account
policy. A general localhost forwarding grant could otherwise reach unrelated
services. Do not copy the forced-command configuration and assume it also
implements tunnel restrictions.

Neither mode requires the outside agent to install a full Aoide daemon: a small
client capable of the documented request/response contract is sufficient.

## Verification and implementation sequence

1. Audit existing mail operations, grants, identities and receipt lifecycle.
   Settle the minimal backend API without duplicating storage or bypassing
   aoided policy/audit.
2. Implement principal-bound send/fetch/ack/status and limits. Test isolation,
   retries, expiry and revocation at the backend before any external listener.
3. Add the SSH adapter and a reproducible restricted-account deployment example.
   Run adversarial integration tests in an isolated account/environment.
4. Prove a real outside agent can send, fetch a reply and acknowledge it with
   no reverse connection or broad host access.
5. Add the API/tunnel adapter using the same policy tests and storage semantics.
6. Add optional Nix configuration and operator status/revocation UI. Do not make
   Nix or a desktop a requirement for the first proof.

Required negative tests: arbitrary shell/command, PTY, SFTP/SCP, forwarding in
forced-command mode, conduct/spawn/tools, other inbox/cursor, forged sender,
unauthorized To/Cc, path traversal, replay/duplicate send, repeated ack,
oversized/slow requests, revoked credentials on an existing connection, and
mail floods reaching configured limits. Bounded responses must not leak secrets
or other mailbox contents through errors or audit views.

Acceptance includes failed/disconnected response recovery, queue-full reporting,
process restart persistence, and proof that a receipt cannot trigger receipts
recursively. Existing transport defects must be fixed and verified before
claiming the external mode is reliable.

## Decisions to resolve during the first slice

- Exact integration of an external principal with the current paired-node
  identity store, without granting node topology/read privileges.
- Key-to-principal binding and narrow backend access for the SSH adapter.
- Versioned request shape and mapping to existing envelope/receipt identifiers.
- Concrete deployment limits and credential rotation procedures.

These are implementation decisions; broad machine access is not an acceptable
fallback when a mail-only path is missing.

## References

- [Aoide mail](MAIL.md)
- [Pairing and signed wire authentication](PAIRING.md)
- [OpenSSH sshd manual: authorized-key restrictions](https://man.openbsd.com/sshd.8)
