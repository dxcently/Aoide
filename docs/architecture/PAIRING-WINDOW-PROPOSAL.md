# Pairing windows — implementation proposal

Status: User-approved interaction direction; protocol details require review before implementation. This proposal does not describe shipped behavior. PAIRING.md remains the implemented protocol reference until the redesign lands.

## Interaction

The receiving host opens enrollment, chooses the permissions it grants, and shows a code. The requesting host selects that receiver and enters the receiver's code. No code is transferred back by the operator. Both hosts report the resulting identity, permissions and connection status.

Proposed commands:

```sh
aoide pair open
aoide pair open --for 10m
aoide pair close
```

`open` without a duration accepts one successful pairing, then closes. It still has a finite expiry; the concrete default must be chosen during protocol review. `--for` keeps enrollment open for multiple devices until the fixed deadline. A successful pairing rotates the code. Success never extends the deadline. Closing or expiry invalidates outstanding enrollment credentials and prevents further trust commits under that window; already committed peers retain their grants.

```text
Receiver                                Requestor
Open enrollment; choose grants
Show code; advertise temporarily ------> Select receiver; enter code
                          <-----------> Authenticate exact pairing exchange
Commit authorized peer    <-----------> Confirm completion
Rotate code or close                    Show paired identity and access
```

The existing read-only default remains. Message and spawn permissions require explicit selection. Each endpoint controls the permissions it grants; the receiver's selected grant cannot silently widen the requestor's grant. Reconnecting an existing peer does not re-grant revoked capabilities.

## Discovery and lifetime

Enrollment temporarily makes the receiver discoverable and willing to accept new pairings. Stopping enrollment restores the prior discovery policy; it does not shut down the daemon, trusted peer connections, or ordinary services. An already enabled persistent beacon may remain visible without accepting new enrollment.

The receiver must be reachable through an existing configured transport. Opening enrollment must not silently expose a listener on a wider interface, change firewall policy, or bypass SSH host verification. Explicit addresses and existing SSH routes remain available when discovery is unavailable.

The panel and CLI show remaining time, effective grants, newly paired peers, and failures. One persistent panel advances through the flow. Protocol progress and unrelated requests continue while a UI is open. CLI and UI use the same daemon-owned state and gates.

## Authentication review gate

The current two-code commitment/reveal protocol cannot be reduced by deleting a prompt. A receiver-generated code entered on a requester changes the authentication construction. Review must choose a vetted construction suitable for a short shared secret, such as an established PAKE where appropriate, or demonstrate an equally suitable existing protocol. Do not invent a code-to-signature construction or transmit a short code as an ordinary bearer credential.

Bind authentication and completion to both instance public keys, fresh protocol state, the intended receiver, and the specific enrollment window. Review offline guessing, online guessing, interception, replay, reflection, concurrent request races, and partial completion. Discovery names are untrusted labels. Possession of the code conveys the receiver's preselected enrollment authority, not mesh-wide authority.

Each displayed code authorizes at most one successful enrollment. Concurrent uses must commit at most one peer before rotation. Attempt budgets must not reset merely by opening a new request; account for abuse and denial-of-service tradeoffs. Never put secrets in audit logs, beacon metadata, URLs, or process arguments. Exact limits and code representation remain protocol-review decisions.

Define crash/restart, expiry during a handshake, retry after lost completion, and close-versus-commit races. A process restart must not accidentally reopen enrollment or reset its deadline. Report a partially completed exchange honestly and provide idempotent reconciliation without granting additional access.

## Scale and automation

Timed enrollment serves manually added batches. It does not mean that every discovered device is accepted. Every new device must authenticate under a current code and its bounded authority.

One operator's machines are not enrolled through windows. They share the mesh's signed roster ([HTTPS-MESH-API.md](HTTPS-MESH-API.md), "Rosters"): each machine's public keys are listed in one file that the mesh's operator key signs, and each machine trusts that operator key once, by configuration or one LAN join. The roster keeps per-node grants, revocation (a removed line, re-signed) and audit identity, and it has its own design and slice. This implementation slice adds no enrollment authority, relay, or transitive trust.

Enrollment windows serve machines of different owners, which share no operator. Trust between them stays pairwise and is made in a pair mesh: a full mesh of N such machines has N(N-1)/2 relationships, so declare the required edges rather than implicitly connecting every host. Known peers reconnect automatically without enrollment.

## Delivery phases

The enrollment-window model replaces the old interactive pairing ceremony; it is not an optional second ceremony. After the new protocol passes review and verification, remove obsolete two-code/reply-code paths, superseded prompts and dialogs, command branches, state fields, tests, and documentation. Inspect each dependency before removal: retain identity keys, existing verified peer records, grants, authenticated transport, and any security primitive still required by the reviewed protocol. Do not remove commitment/reveal or another primitive merely because its previous UI disappears.

Existing trusted peers continue to reconnect. Define migration or explicit invalidation of pending old-ceremony requests. Unsupported old peers receive a clear upgrade-required response for new enrollment; do not silently fall back to the old ceremony or maintain parallel enrollment modes. Historical records may remain as history, but active documentation and examples teach only the replacement once shipped. Compatibility checks are not a second implementation of the old protocol.

The implementation plan includes a concrete deletion inventory: old item, replacement or reason it is unnecessary, callers/state affected, and verification that removal leaves existing trusted connections intact. Temporary development coexistence must not become a shipped configuration switch.

1. Inventory current CLI/UI/wire state and write a threat-model and protocol decision with compatibility behavior. Do not silently downgrade old peers to a weaker ceremony.
2. Implement daemon-owned enrollment lifecycle, deadlines, close semantics, atomic code consumption/rotation, and headless CLI using the reviewed protocol.
3. Add temporary discovery and one persistent Lyra panel over the same API. Reuse current visual conventions; avoid an independent UI policy engine.
4. Verify two real hosts: single device, timed batch, wrong code, expired code, replay, parallel enrollment, network loss, restart, close races, prior grants and normal reconnection. Visually review the panel and verify CLI-only operation.
5. Update PAIRING.md, Pairing-Ceremony.md, command schema, contracts and owning-directory docs alongside implementation; then build and release with explicit provenance.

## Acceptance

- Exactly one operator code transfer, receiver to requester.
- Single-device mode closes after success; timed mode rotates codes without extending its deadline.
- Expiry/close stop new trust without interrupting trusted connections.
- Beacon visibility alone never grants access; unknown peers cannot spawn or message without the corresponding explicit grant.
- No secret leakage through logs/discovery and no automatic widening of network exposure.
- All state transitions and security tests are verified before the UI or docs label the new ceremony available.
