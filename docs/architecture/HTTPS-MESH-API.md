# Aoide HTTPS mesh API

Status: proposed design. No HTTPS implementation, listener, credential or cutover
exists or is authorized. A library-profile gate and a follow-up review remain
before implementation. Where this document and MAIL.md conflict, MAIL.md wins
until the amendment below lands.

**E2E is mandatory on this lane** — no plaintext profile and no per-node toggle for
HTTPS. The existing loopback/SSH lane stays the separate, explicitly plaintext v1
path.

## Objective and boundary

An Aoide-native HTTPS API for mail, authorized mesh observation, and separately
granted control — on LAN, closed networks and internet routes, without SSH
accounts; SSH remains supported. Design the application API and the
payload-protection boundary, not new TLS, cryptography or RPC machinery: reuse the
existing mail store, identities, command services, authorization and audit, and
select maintained libraries at the library-profile gate. The network adapter stays
separate from the local daemon (AOIDED.md: aoided is a local socket service; HTTPS
does not silently make it a public listener). No generic remote command-string or
unrestricted registry-dispatch endpoint.

## Two verification halves

The container splits MAIL.md's one `mailDeposit` policy into a hop half needing no
key and a destination half needing one. Both end at the same daemon policy and
audit boundary with the authenticated origin preserved.

**Shared verification, then branch.** Every receiver, hop or destination, first
runs the same steps with no key: signed-request verification → verify the outer
origin signature over `ctx ‖ enc ‖ ct` under the key `origin.key` names, resolved
through the paired record or the User-declared mesh key table → zone check on the
outer `mesh` → dedup to a previously validated and admitted record. Then it
branches once:

**Hop branch — spool, never open.** Re-spool toward the destination. A hub never
opens, never needs a recipient key, and cannot read, edit or forge.

**Destination branch — open, match, file.** Only when self is the destination: open
with the recipient's KEM key → recompute the inner `msgid`, require it equal the
container `msgid` → require `header.from.node == origin.node`,
`header.to.node == to.node`, `header.originMesh == ctx.originMesh` → require
`origin.key` to be the single key on record for that node → verify the inner origin
signature → membership → file → receipt/doorbell. This branch never runs the hop
spool step: a destination that misfiles would otherwise relay content addressed
elsewhere. A mismatch refuses (`addressing-mismatch`), never filed, never
re-spooled.

The outer signature needs no plaintext and no recipient private key, so **outer
signature and dedup are decided before the KEM runs**: an unverified origin is
refused; a previously admitted duplicate returns `duplicate` without opening.
For a filed letter, the destination recovers receipt delivery from the durable
filed record under the existing pending-receipt rule. Dedup never records a
malformed or unverified container. Its lookup key is the authenticated
`(origin.key, to.node, msgid)` tuple: a relay cannot verify the inner hash, so a
different origin must not reserve another origin's message ID. Within that tuple,
the comparison is the **digest of the immutable container fields** —
everything `ctx` covers plus `enc`, `ct` and `sig` — and deliberately **excludes
the hop-mutable `mesh` and `transit`**, so retrying the same letter over a different
route is a duplicate of the same letter, not a collision; a `msgid` reappearing with
different immutable bytes is a collision and refuses. This split is a MAIL.md
amendment (house rule 8): its §Wire steps 2 and 4 become destination-branch
statements, with the hop branch beside them. Until then this document describes
intended behaviour, not the shipped wire.

## Connections and trust

- Direct reachable peers use HTTPS for requests and WSS for events.
- LAN and offline networks use configured trust: private CA certificates or
  explicitly verified peer certificate/key bindings. Public internet service and
  public DNS are not prerequisites; pinning is a designed trust mode, not a
  skip-verification flag. No automatic NAT traversal in the first slice.
- Reuse the existing signed-peer authentication: a per-request Ed25519 signature
  over method, path, timestamp, nonce and body digest, resolved to the paired node
  whose stored key verifies it. It binds the method but carries no suite field;
  transport TLS is additional, never a substitute for payload protection.
  Revocation and scope changes affect active streams as well as new requests.

TLS protects a connection: a TLS-terminating relay or proxy can read plaintext
unless payloads are also end-to-end encrypted, which is why E2E is first-slice. E2E
is between authorized Aoide endpoints, not isolation between same-uid processes.

## Chosen requirements (not negotiable)

1. **Sealed payload** — no intermediate holds a key that opens the letter.
2. **Vetted primitives only** — published, analyzed protocols; no home-grown
   composition.
3. **Separate encryption keys** — the Ed25519 identity signs and never encrypts;
   KEM material is distinct (RFC 9180 §9.2.3: `ikm` and `Encap` randomness MUST NOT
   be reused elsewhere).
4. **Recipient self-signed, self-published binding** — verified under the peer's
   paired or User-declared key BEFORE encrypting. An origin signature over a
   relay-supplied fingerprint does not prevent substitution; the recipient's own
   binding does.
5. **No plaintext fallback** — no non-E2E HTTPS profile exists.
6. **Relay never learns plaintext** — it may route, drop or delay, nothing more.

## Container

The outer object is new; the inner envelope is untouched. This is design notation,
not a frozen wire format; the library-profile gate fixes the encoding.

```text
outer = { v, purpose, msgid, generation, origin { node, key },
          to { node, kem }, originMesh, mesh, hpke { suite, enc, ct },
          sig, transit [...] }
ctx   = canonical(v, purpose, msgid, origin.node, origin.key, to.node,
                  to.kem, originMesh, suite, generation)
ct    = AEAD-seal(inner envelope bytes, AAD = ctx)
sig   = origin Ed25519 over ctx ‖ enc ‖ ct
```

- `ctx` is **recomputed by each verifier from the outer fields, never trusted from
  the wire** (no transmitted blob to mismatch), and is the AAD for `ct`: it excludes
  `ct`, and `sig` covers `ctx`, `enc` and `ct`, so nothing is its own input. Its
  encoding is a **domain-separated, length-delimited canonical string** — purpose
  label plus length-prefixed fields, the MAIL.md canonical discipline; naive
  concatenation is refused as ambiguous — pinned at the library-profile gate before
  any H1 test. `suite` is one registered identifier covering DHKEM + KDF + AEAD, so
  a downgrade cannot be expressed by mixing components.
- **`originMesh` is immutable inside `ctx`; `mesh` is hop-mutable outside it**,
  rewritten only by a declared gate and verifiable **only** through that gate's hop
  signature — MAIL.md's current rule. `mesh` is not AEAD-authenticated, so a
  legitimate gate rewrite cannot break the tag.
- `purpose`/method are bound into `ctx`, so a mail container cannot be replayed as
  another operation.
- **Hop signatures** use MAIL.md's rule, `msgid ‖ 0x00 ‖ node ‖ 0x00 ‖ at ‖ 0x00 ‖
  mesh`, appended per hop. **Honest scope:** MAIL.md's door verifies only the last
  `transit` element, and only when `mesh ≠ originMesh`; this design inherits that
  scope and does **not** claim intermediate-hop tamper detection. Full-chain
  verification is an open fork needing its own amendment.
- **Immutable:** content, subject, from, to, destination, envelope version and key
  bindings are fixed inside `ct`/`ctx`; a changed byte breaks the tag, `sig` or the
  recomputed `msgid`, and the equality checks catch two intact but disagreeing
  claims. No relay has edit authority; a relay spool holds sealed containers only.
- **Only an endpoint-origin edit mints a new message** — a correction, resend with
  different content, or forward is a NEW signed container with a new `msgid`; there
  is no in-place revision of an accepted letter.

## Threat model

Out of scope: anonymity, traffic analysis beyond the metadata below,
post-compromise security, and protection once an endpoint is compromised.

| Adversary | Capability | Outcome |
|---|---|---|
| On-path observer / TLS-terminating proxy | Ciphertext, outer metadata; drop, delay, reorder, replay | No plaintext; request-nonce replay is refused, admitted duplicate letters take the receipt-recovery path without opening. |
| Unpermitted relay | Containers only | Cannot read, mutate, re-seal or forge a receipt. |
| Authorized relay/hub (transit, `message`) | Verifies, spools, appends hop signature, drops/delays | Routing authority is not edit authority; cannot open, alter or forge. |
| Misbehaving paired node | Valid identity, own grants | Reads only what is addressed to it; a mismatched outer/inner claim refuses. |
| Offline copy of a relay's disk/logs | Ciphertext, metadata, hop chain | No plaintext; transit logs no mailboxes. |
| Offline copy of the destination's disk | Decrypted mailbase after filing | Plaintext by design. |
| Later compromise of a node's KEM key | Static private key | Opens past ciphertext sealed to it: static HPKE is not forward secret (RFC 9180 §9.7.4). |
| Same-uid local process | Keys (0600) and stores | Not addressed; total on that node. |

**What transit still sees:** outer version and purpose, `msgid`, `origin.key`
fingerprint and `origin.node`, destination node, recipient KEM fingerprint,
`originMesh` and current `mesh`, suite, ciphertext length (AEAD hides no length,
RFC 9180 §9.7.6), arrival time, hop-chain node names. Inside `ct`: subject, body,
To/Cc mailbox names, `threadId`/`replyTo`, `from.name`, inner envelope version.
Padding is an open decision; without it, length approximates letter length.

## Cryptographic selection

- **Chosen, first mail slice:** HPKE (RFC 9180) — DHKEM(X25519, HKDF-SHA256),
  HKDF-SHA256, one AEAD (AES-128-GCM or ChaCha20-Poly1305); its single-shot shape
  fits store-and-forward mail with no session state.
- **Sender authentication and tamper-evidence:** an Ed25519 signature over `(ctx,
  enc, ct)` — the mitigation RFC 9180 §9.1.1 recommends against key-compromise
  impersonation. HPKE Auth mode alone is not a substitute.
- **Replay and downgrade are door requirements:** HPKE gives no replay protection
  beyond in-context ordering (§9.7.3) and no downgrade prevention (§9.7.2); covering
  both is unopened-dedup, the nonce guard, and the accepted-suite list carried inside
  the self-signed binding (an authenticated input, unlike the request signature,
  which binds only method and body digest).
- **Multi-recipient and areas:** pairwise HPKE fan-out first — each unique recipient
  gets its own sealed copy, matching MAIL.md's independently signed per-recipient
  envelope. For durable area membership at group scale, MLS (RFC 9420) is the vetted
  group option: asynchronous group keying with FS and PCS.
- **Deferred:** the Double Ratchet, for interactive lanes where per-message FS is
  worth ratchet state (Signal spec: FS/PCS, §8.2 caveats).
- **Rejected:** home-grown crypto; reusing the Ed25519 key for encryption; converting
  the identity seed into an X25519 key (a reusable-key hazard, §9.2.3); plaintext
  under TLS; unauthenticated suite negotiation.

## Keys: binding, distribution, rotation, revocation

A node mints an X25519 KEM keypair beside its Ed25519 identity, private key 0600,
never printed, never in an Outcome. The binding is **self-signed and keyed by the
identity-key fingerprint, never by a name** (a node declared in two meshes has two
names and one key), and carries the KEM public key bytes, their fingerprint,
purpose (mail E2E), accepted
suites, a monotonic `generation` with `rotatedAt`, and a validity window.

- **Channel.** A node self-publishes its signed binding through the existing
  authenticated door; a verifier accepts it only when the signer is the very identity
  key the binding names, storing it beside the paired record. Any other carriage —
  relay, mesh peer, cached copy — is untrusted and accepted only after independently
  verifying the recipient's own signature. A relay is never the authority; the root
  of trust stays the pairing record or the User-written mesh key table, consistent
  with "one node, one key, one grant" and with drift reporting.
- **Freshness (learning and publishing).** Generations increase strictly per
  identity key, the highest seen is kept durably at rest, the generation used is
  bound into `ctx`, and a lower generation refuses to be learned or published as
  current (`stale-binding`). Without this a replayed superseded binding is
  byte-indistinguishable from the current one and revocation would not stick. This
  high-water mark governs **which binding is current**; it is deliberately separate
  from the destination's retired-key grace below, which governs **opening already
  sealed ciphertext** — a binding refused as stale does not, by itself, invalidate a
  letter sealed before it.
- **Offline truth.** Instantaneous revocation to an unreachable peer is impossible
  and is not claimed. With no fresh binding a node seals to the last verified binding
  within its validity window; once a binding is **expired or unknown it always parks
  and reports** — no configurable setting may seal to it. A known compromise or
  revocation rejects immediately.
- **Retired keys.** A superseded private key is kept only for a bounded grace window
  to open mail sealed to it while it was current, then deleted; sealed-but-undelivered
  mail addressed to a retired key after that window is refused (`key-retired`) and the
  outcome reaches the origin's retry loop. Queue expiry is reported, never a silent
  drop.
- **Already-filed mail survives key loss.** `base.jsonl` holds the opened envelope in
  plaintext, so filing is unaffected; only sealed-but-unfiled copies — outbox, in
  transit, spooled — become unreadable. Losing the identity signing key re-pairs the
  node under a new name. No escrow, no recovery authority.

## Guarantees and at-rest honesty

v1 claims confidentiality against relays and on-path observers, integrity and origin
authenticity, and replay/tamper/downgrade refusal. It does NOT claim forward secrecy
or post-compromise security: RFC 9180 §9.7.4 says HPKE ciphertexts are not forward
secret against recipient compromise (they are, against sender compromise), so a later
compromise of a node's static KEM key opens its past mail. FS/PCS is a
**clearly-labelled optional future decision**, not a guarantee the User accepted or
that v1 implies; if required, the selection moves to MLS or a ratchet BEFORE the
first slice.

Decrypted letters live in `state/mail/base.jsonl` in plaintext, as today; the outbox
holds sealed containers; keys are 0600. No at-rest secrecy is claimed. **No secret
content in logs, split by half:** the destination audit records door name, status,
origin/destination node and mailbox, `msgid`, size and outcome — never letter text,
subject, container bytes, keys or secrets. A transit hop records node names, `msgid`,
size and outcome only, and **never mailbox names**: a hub cannot know them and must
not claim them. Refusals name their reason (`downgrade-refused`, `unverified-origin`,
`bad-msgid`, `wrong-recipient`, `addressing-mismatch`, `stale-binding`,
`key-retired`).

## API resources and endpoints

Semantic groups; spellings are contract descriptions, not final URLs. Version the
wire contract and refuse unknown versions. No key-registry endpoint: bindings come
from the self-published record and the pairing/nodelist trust root. Read access is
not control access; roster access is not transcript access.

| Endpoint (semantic) | Operations and limits |
|---|---|
| `mail/deposit` | Deposit one sealed container; `accepted \| duplicate \| refused(reason)`; also carries receipts — there is no second ack method |
| `mail/poll` | Caller's own spooled containers; caller identity must BE the node polled |
| `mail/ack` | Semantic grouping for the receipt flow: one receipt per acked `msgid` while pending, and a later redelivery after the first is delivered may re-spool one (MAIL.md's actual rule) |
| `areas` | Discover authorized areas, subscribe/unsubscribe, bounded history |
| `state` | Authorized snapshot of hosts, projects, sessions, terminals, agents, relationships |
| `events` | WSS subscribe to permitted changes after a cursor; bounded recoverable stream |
| `presence` | Expiring host connection/heartbeat state, distinct from process state |
| `control` | Explicit typed actions through existing gates, after read/mail proof |
| `credentials` | Owner-managed pairing/revocation; never implicitly granted to observers |

The first external profile is the restricted correspondent in
[MAIL-ONLY-ACCESS.md](MAIL-ONLY-ACCESS.md), sealed under the same rules when remote.

## State, events and control

The proxy argument that justifies sealing mail applies unchanged to `state`,
`events` (including WSS responses) and `control`: **requests and responses there
must have a reviewed E2E design before H2 or H5 is implemented**, until which the
reverse-proxy adapter is forbidden on any door where `control` is reachable, and a
terminating proxy is never silently exempted. No such implementation exists now.

## Shared areas and live state

Nodes receive records for areas they may carry (`project/dxflake` durable
correspondence, `host/<name>/sessions` lifecycle, `host/<name>/activity` activity),
borrowed from Echomail's subscription model. Each host is authoritative for its own
process state: stable host-qualified IDs, origin identity, schema version, ordered
origin sequence/epoch, never cross-host order from wall clocks. Verify origin and
area membership against the signed binding before accepting or forwarding — never a
claimed name — and prevent duplicates by stable IDs and bounded forwarding. Area
records are sealed to subscribers; removal stops future distribution and rotates the
group key where one exists, and cannot revoke plaintext already received. A
cursor-tied snapshot precedes live changes with no race; on reconnect replay retained
events and require a fresh snapshot if the cursor is too old. Bound queues and slow
subscribers; never silently drop durable mail.

## Delivery semantics

HTTPS success identifies what the server accepted or filed, not that an agent acted.
Preserve queued/filed/fetched distinctions and stable idempotency identifiers: a
retry after a lost response must not duplicate work or receipts. The sealed container
is fixed at mint time, so a retry is byte-identical and `msgid` stays stable. WSS is
a transport stream, not durable storage; cursors and backend state provide recovery.
Neither TLS nor E2E prevents application retries — validation, dedup and the nonce
guard do. Doorbells are recipient-controlled fixed nudges carrying no letter bytes,
and control actions use explicit capabilities and existing admission gates.

## Migration and coexistence

E2E is always on for the HTTPS lane: no per-node toggle and no non-E2E profile there.
The existing plaintext v1 mail path over the loopback/SSH tunnel stays explicitly
separate and unchanged; mail between an HTTPS node and a node without the capability
does not flow on this lane, with no automatic fallback either direction, and no
cutover removes SSH. Configuration selects listener, endpoint identity,
certificate/trust source, credentials and grants, limits, and the **accepted-suite
list** (carried in the self-signed binding). Ordinary configuration works without
Nix; optional Nix modules declare services and credential-file references, never
secrets in the store, and renewal, pin rotation, key rotation and revocation must
work offline too. No listener, credential or cutover is enabled by this document.

## Phases and acceptance tests

E2E sits inside the first mail phase, not a phase after relays.

1. **H0 — inventory.** Current door, event feed, grants, daemon calls, mail envelope
   and outbox. No new behaviour.
2. **H1 — sealed mail over HTTPS, direct edges (E2E mandatory).** Binding and its
   channel, sealing/opening, container and canonical `ctx`, the two halves,
   signed-request door reuse, sealed outbox entries, receipts. No relay.
3. **H2 — state snapshots and WSS events**, only after the reviewed E2E design.
4. **H3 — authorized shared areas.** Membership, duplicates/loops, origin
   verification, removal semantics.
5. **H4 — relay.** Outbound-only routing, hop chain intact, sealed end to end, no
   plaintext at the hub.
6. **H5 — typed control actions**, existing policy and audit only, and only after the
   same reviewed E2E design.

Acceptance evidence for H1 — required tests, not suggestions:

- **Ciphertext transit:** wire capture plus relay spool, log and audit inspection; no
  letter bytes, subject, body or mailbox names; transit logs carry node names and
  outcome only.
- **No unnecessary open:** an unverified origin refuses and a previously admitted
  duplicate returns `duplicate`, both with no KEM open performed.
- **Addressing mismatch:** a legitimately paired node whose inner header is addressed
  elsewhere is refused, never filed, never re-spooled.
- **Wrong recipient:** a container for B opened with C's key fails.
- **Tamper, authorized and unauthorized relays:** a flipped byte in `ct`, an outer
  `ctx` field, `to.kem`, `msgid`, `mesh` or the last hop signature refuses and
  audits; intermediate-hop tampering is not claimed detected.
- **Zone:** a non-gate relabel is `zone-violation`; an accepted gate rewrite verifies
  through the gate's hop signature.
- **Replay/retry:** a resent capture and a retry after a lost response yield
  `duplicate` with no second filing; a retry of the same immutable container over a
  different route dedups rather than collides (the comparison excludes `mesh` and
  `transit`); a base/seen crash re-accepts exactly once; a same-`msgid`
  different-immutable-bytes collision within the same origin/destination refuses.
  A different origin cannot preclaim the ID at a relay. A filed duplicate recovers
  a lost receipt without duplicating an already pending receipt.
- **Revocation/rotation:** a replayed superseded binding is refused for
  current use; a binding whose generation is below the high-water mark cannot
  become current, while mail already sealed to it during its currency still opens
  within the destination's grace window; a known revocation rejects immediately; an
  expired or unknown binding always parks and reports; `key-retired` reaches the
  origin's retry loop.
- **Receipts:** one per acked `msgid` while pending; a later redelivery after delivery
  may re-spool one; no receipt chain.
- **Crash/recovery:** torn-tail truncation, byte-identical resend, no duplicated
  receipts; already-filed letters survive KEM-key loss.
- **Windows and Linux, no SSH** (timeless target, not machine-specific progress):
  send/fetch/ack end to end with no SSH account or binary on either OS. The
  ThinkChiyo Windows host is available but its connection is currently blocked and
  not exposed here, so that leg is recorded manual/blocked.
- **Downgrade:** plaintext, mixed suite components, or an older accepted-suite entry
  refuses with a taught error and an audit line.
- **Invalid certificate/credential and unauthorized inbox** are rejected as in the
  plaintext profile's negative tests.

## Open design decisions

- AEAD choice and the maintained Rust HPKE library, with `ctx` encoding pinned at the
  profile gate.
- Padding policy for ciphertext length.
- Full-chain hop verification versus MAIL.md's last-element scope (an amendment
  either way).
- Forward secrecy: adopt MLS or a ratchet before H1 if the User requires FS/PCS.
- Group keying for areas: pairwise fan-out versus MLS.
- TLS termination/library, certificate provisioning, and the door binding.
- Area grants and observation scopes; cursor/retention limits; relay ownership.

## References

- [Mail](MAIL.md) — envelope bytes, store, outbox, transit, security model;
  [Pairing](PAIRING.md) — identity, signed wire authentication, transport;
  [Mail-only external access](MAIL-ONLY-ACCESS.md); [daemon boundary](AOIDED.md).
- [RFC 9180 — HPKE](https://www.rfc-editor.org/rfc/rfc9180.txt): §9.1.1 sign
  `(enc, ct)` for KCI resistance, §9.2.3 KEM key reuse, §9.7.2 no downgrade
  prevention, §9.7.3 replay limited to in-context ordering, §9.7.4 no forward
  secrecy against recipient compromise, §9.7.6 length is not hidden.
- [RFC 9420 — MLS](https://www.rfc-editor.org/rfc/rfc9420.txt): asynchronous group
  keying with FS and PCS, untrusted Delivery Service, epochs and Remove.
- [Double Ratchet](https://signal.org/docs/specifications/doubleratchet/): FS/PCS
  for pairwise sessions; §8.2 names what a key compromise still defeats.
- [RFC 8446 — TLS 1.3](https://www.rfc-editor.org/info/rfc8446/): transport
  confidentiality and authentication, not a payload-protection substitute.
