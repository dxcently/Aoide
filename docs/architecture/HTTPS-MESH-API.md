# Aoide HTTPS mesh API

Status: proposed design. No HTTPS implementation, listener, credential or cutover
exists or is authorized. The library-profile gate is reduced to confirming the
`age` crate profile and pinning the canonical `ctx` encoding; a follow-up review
remains before implementation. Where this document and MAIL.md, PAIRING.md or
CONTRACTS.md conflict, those win until the amendments listed below land.

**Sealing is mandatory on this lane**: every letter is sealed. No plaintext profile
and no per-node toggle exist for HTTPS. The existing loopback/SSH direct-edge lane
stays the separate, explicitly plaintext v1 path.

## Why this exists

An Aoide node must be reachable from networks where no VPN runs and only HTTPS on
443 gets out, such as a Windows laptop on a locked-down network. A user may also
belong to several meshes, each with its own relays and transports.

Doors stay **loopback-only**: no Aoide door ever binds a routable address. So no
home node accepts inbound HTTPS. Every node connects OUT to a relay, and **the relay
is the first slice**, not a late phase. Direct HTTPS edges exist only as a later
optional mode for a node that legitimately exposes a URL, such as a VPS.

## Objective and boundary

An Aoide-native HTTPS API for mail, authorized mesh observation, and separately
granted control, without SSH accounts. SSH remains supported. This is a design for
the application API and the payload-protection boundary, not new TLS, cryptography
or RPC machinery. It reuses the existing mail store, identities, command services,
authorization and audit. The network adapter stays separate from the local daemon
(AOIDED.md: aoided is a local socket service). The adapter binds loopback, and
whatever fronts it (a tunnel, a reverse proxy) is a transport hop, never a door.

**Grants, not shell.** HTTPS exposes only per-node grants (`read`, `spawn`,
`message`). It never exposes a shell, a generic remote command string, or
unrestricted registry dispatch. A declared machine defaults to `message` only.
`read` and `spawn` are added per node by hand (`aoide node allow`).

## Transports and relays

A relay is declared **per mesh** (MAIL.md `[mesh.<name>] relays = [...]`). A node's
declared address selects the transport by URL scheme. The transport is a per-node
option and is not baked into the design:

| Address | Transport | Examples |
|---|---|---|
| `ssh://[user@]host[:port]` | SSH tunnel to the loopback door (today's transport) | LAN boxes |
| `https://host` | HTTPS to a loopback adapter behind something that owns 443 | a Cloudflare Tunnel hostname, a VPS with public 443, a tailnet hostname. Aoide treats all three identically. |
| `poll` | none inbound: the node has no address. It connects out to its mesh's relay, deposits, and polls for its own letters. | a laptop on a locked network |

A hub's outbox entry for a `poll` node is `hold`-flavored. The hub never dials it,
and only the node's own `mailPoll` drains it (MAIL.md P-M3). A hub may receive over
one transport and forward over another. Both hops carry the same sealed container.

```toml
[mesh.home]
grant  = ["message"]
relays = ["sakaki"]

[mesh.home.nodes]
sakaki     = "https://aoide.necoconeco.net"   # loopback adapter behind sakaki's Cloudflare Tunnel
yomi-strix = "ssh://khoa@192.168.1.158"
thinkchiyo = "poll"

[mesh.home.keys]                               # ed25519 identity pubkeys (see Keys)
thinkchiyo = "…"

[mesh.club.pins]                               # optional; end-to-end TLS only
evo-hub = "sha256:…"
```

Hostnames are illustrative. In the home mesh, sakaki is the hub behind its existing
Cloudflare Tunnel, which dials out and forwards to a loopback listener. Other meshes
may use a VPS or a tailnet hub.

### Degenerate topologies

| Topology | Behaviour |
|---|---|
| One host | This layer is not needed. Conducting and local `self/<name>` mail run with no mesh, relay or HTTPS. The HTTPS mesh is an add-on, never a prerequisite. |
| One relay for a mesh, relay down | The relay is a single point of failure for **availability, not secrecy**. Letters wait in each sender's outbox and retry, so nothing is lost. LAN peers with a direct `ssh://` edge still deliver. |
| One relay for a mesh, relay compromised | Because letters are sealed and signed, it can drop, delay and observe metadata (who wrote to whom, size, time). It can never read or forge. |
| Several relays (`relays = ["a", "b"]`) | For availability. The sender takes the first relay, in declaration order, that is a direct, verified, not-`down` edge (MAIL.md routing step 2). A relay that is unreachable but not declared `down` is still chosen, and the letter waits for it. Failing over automatically on unreachability is open. |
| Relay that is also a node (sakaki) | The hub is a role. Its own mail files locally, like any destination's. |

## Connections and trust

**TLS may terminate at an intermediary.** Cloudflare terminates TLS and can read
plaintext HTTPS. The design follows from that:

- **Sealing is load-bearing, not optional.** No relay or proxy ever holds a key that
  opens a letter.
- **Node authentication lives in the Aoide layer.** Every request on every transport
  carries the existing per-request Ed25519 signature over method, path, timestamp,
  nonce and body digest. The receiver resolves that signature to a node through the
  paired record or the declared key table. A request's observed source address is
  never an input: behind a tunnel, every request looks like loopback. TLS is
  additional and never replaces payload protection. Revocation and scope changes
  affect active streams as well as new requests.
- **Certificate pinning is optional and per node.** It applies only where TLS is
  end to end (a VPS, a tailnet) and is declared in `[mesh.<name>.pins]`. The design
  never assumes a pin through Cloudflare, where the edge certificate belongs to the
  intermediary. Pinning is a declared trust mode, never a skip-verification flag.
- **Defense in depth for a tunnel hostname.** The IP stays hidden, but the hostname
  is public. Put Cloudflare Access (service token or mTLS) in front of it, on top of
  signed requests, so unauthenticated traffic dies at the edge. Access is a filter.
  It is not the trust root.
- No automatic NAT traversal. Outbound-to-relay is the traversal.

Sealing protects content between authorized Aoide endpoints. It does not isolate
processes that run under the same uid.

## Two verification halves

The container splits MAIL.md's one `mailDeposit` policy into a hop half that needs no
key and a destination half that needs one. Both end at the same daemon policy and
audit boundary, with the authenticated origin preserved.

**Shared verification, then branch.** Every receiver, hop or destination, first runs
the same steps with no key:

1. Verify the signed request.
2. Recompute `ctx` from the outer fields.
3. Verify the outer origin signature over `ctx ‖ ct` under the key that `origin.key`
   names, resolved through the paired record or the declared key table.
4. Check the zone on the outer `mesh`.
5. Dedup against previously validated and admitted records.

Then it branches once:

**Hop branch: spool, never open.** Append this hop's chained hop signature and
re-spool toward the destination. A hub never opens a letter and never needs a
recipient key. It cannot read, edit or forge.

**Destination branch: open, match, file.** This branch runs only when self is the
destination. Steps, in order:

1. Open `ct` with the age identity for `to.age`.
2. Require the inner `ctx` to equal the recomputed outer `ctx`, byte for byte (see
   Container).
3. Recompute the inner `msgid` and require it to equal the container `msgid`.
4. Require `header.from.node == origin.node`, `header.to.node == to.node` and
   `header.originMesh == ctx.originMesh`.
5. Require `origin.key` to be the single key on record for that node.
6. Verify the inner envelope signature.
7. Walk the full hop chain.
8. Check membership.
9. File the letter.
10. Send the receipt and ring the doorbell.

This branch never runs the hop spool step, because a destination that misfiled would
otherwise relay content addressed elsewhere. A mismatch refuses
(`context-mismatch`, `addressing-mismatch`, `broken-chain`). The letter is never filed
and never re-spooled.

**Outer signature and dedup are decided before anything is opened.** The outer
signature needs no plaintext and no recipient private key. An unverified origin is
refused. A previously admitted duplicate returns `duplicate` without opening. For a
filed letter, the destination recovers receipt delivery from the durable filed
record under the existing pending-receipt rule. Dedup never records a malformed or
unverified container.

The dedup lookup key is the authenticated `(origin.key, to.node, msgid)` tuple. A
relay cannot verify the inner hash, so one origin must not be able to reserve another
origin's message ID. Within that tuple, the comparison is the **digest of the
immutable container fields**: everything `ctx` covers, plus `ct` and `sig`. The digest
deliberately **excludes the hop-mutable `mesh` and `transit`**. Retrying the same
letter over a different route is therefore a duplicate of the same letter, not a
collision. A `msgid` that reappears with different immutable bytes is a collision and
refuses.

## Chosen requirements (not negotiable)

1. **Sealed payload.** No intermediate holds a key that opens the letter. This covers
   hubs on every transport and any TLS-terminating front.
2. **Vetted primitives only.** Use published, analyzed formats. The only composition
   is the documented signature-and-`ctx` binding below, and that binding is subject
   to review.
3. **Separate encryption keys.** The Ed25519 identity key signs and never encrypts.
   The age X25519 key is generated independently. Never derive the age key from the
   identity seed, and never use age's `ssh-ed25519` recipient mode.
4. **The recipient signs its own binding.** A sender accepts an age key only inside a
   binding signed by the recipient's own Ed25519 identity key. The sender pinned that
   identity key at pairing or through a two-sided declaration, and verifies the
   binding BEFORE encrypting. The generation high-water mark and revocation rules
   below apply. A binding carried by a relay, or a fingerprint signed by the origin,
   does not prevent substitution. The recipient's own signature under a key pinned by
   the trust root does.
5. **No plaintext fallback.** No non-sealed HTTPS profile exists.
6. **A relay never learns plaintext.** It may route, drop or delay. It may append a
   signature and do nothing else.

## Container

The outer object is new, and the inner envelope is untouched. This is design notation,
not a frozen wire format. The library-profile gate fixes the encoding.

```text
outer = { v, purpose, msgid, generation, origin { node, key },
          to { node, age }, originMesh, mesh, suite, ct,
          sig, transit [...] }
ctx   = canonical(v, purpose, msgid, origin.node, origin.key, to.node,
                  to.age, originMesh, suite, generation)
pt    = canonical(ctx, inner envelope bytes)
ct    = age-encrypt(pt, recipient = to.age)      -- the whole age file, header included
sig   = origin Ed25519 over canonical("aoide/mail-outer", ctx, ct)
```

- **`ctx` is carried twice and must agree** (NEEDS REVIEW before implementation).
  age has no associated-data input, so nothing in `ct` itself binds the ciphertext to
  its context. The sender puts `ctx` inside the sealed plaintext and signs
  `ctx ‖ ct` outside. The ordering is fixed:
  1. The sender computes `ctx` from the outer fields.
  2. The sender seals `pt = canonical(ctx, envelope)` to `to.age`.
  3. The sender signs `(ctx, ct)`.
  4. Every verifier recomputes `ctx` from the outer fields and never takes it from the
     wire.
  5. Every verifier checks `sig` against the recomputed `ctx` before opening.
  6. Only the destination opens. It then requires the inner `ctx` bytes to equal the
     recomputed outer `ctx`. It treats no inner field as meaningful until that check
     and the inner envelope signature both pass.

  The outer signature proves that the holder of `origin.key` vouched for these
  context bytes and this ciphertext. The inner copy proves that the party who sealed
  the plaintext meant the same context. So a relay that re-signs someone else's `ct`
  under its own key and context is caught when the letter opens. This check replaces
  what an AEAD's AAD would have given.
- The `ctx` encoding is a **domain-separated, length-delimited canonical string**: a
  purpose label plus length-prefixed fields, following the MAIL.md canonical
  discipline. Naive concatenation is refused as ambiguous. The encoding is pinned at
  the library-profile gate before any H1 test. `suite` is one registered identifier
  that names the age format version and recipient type (for example, age v1 with
  X25519), so a downgrade cannot be expressed by mixing components.
- **`originMesh` is immutable inside `ctx`. `mesh` is hop-mutable outside it.** Only a
  declared gate rewrites `mesh`, and the rewrite is verifiable **only** through that
  gate's hop signature, which is MAIL.md's rule.
- `purpose` and the method are bound into `ctx`, so a mail container cannot be
  replayed as another operation.
- **Immutable fields:** content, subject, from, to, destination, envelope version and
  key bindings are fixed inside `ct` and `ctx`. A changed byte breaks the age payload
  tag, `sig`, the inner `ctx` equality or the recomputed `msgid`. The equality checks
  catch two intact claims that disagree. No relay has edit authority, and a relay
  spool holds sealed containers only.
- **Only an edit by the origin endpoint mints a new message.** A correction, a resend
  with different content, or a forward is a NEW signed container with a new `msgid`.
  An accepted letter is never revised in place.

### Hop-signature chain

A relay may append its own signature. It may never alter the letter or remove earlier
entries. MAIL.md's per-hop signature over `msgid ‖ node ‖ at ‖ mesh`, with the
`transit` list outside every signature, would let a later hop delete or reorder
earlier entries. The chain replaces it:

```text
hop_sig_n = sign_n(canonical(msgid, prev_n, node_n, next_n, at_n, mesh_n))
prev_1    = msgid
prev_n    = sha256(canonical bytes of entry n-1, its sig included)
```

- Fields are length-prefixed, not joined with `0x00`.
- `next` is the node this hop hands the letter to.
- The destination walks the chain from `msgid`. It verifies each signature under that
  hop's paired or declared key and requires `entry[j].next == entry[j+1].node`. It
  also requires the last entry's `next` to be self.
- The chain defeats **cut-and-reappend**: a hop that truncated the chain to entry `j`
  and re-appended itself would verify under a flat per-hop rule. Here it breaks the
  `prev` or `next` link.
- A hop can still drop the tail, which drops the letter. No destination-signed ack
  then reaches the origin, and the outbox reports the letter as undelivered.
- The destination verifies the full chain. Per-hop zone checks at each door are
  unchanged.

## Threat model

Out of scope: anonymity, traffic analysis beyond the metadata below, post-compromise
security, and protection once an endpoint is compromised.

| Adversary | Capability | Outcome |
|---|---|---|
| On-path observer or TLS-terminating front (for example Cloudflare) | Sees ciphertext, outer metadata and request signatures. Can drop, delay, reorder, replay. | No plaintext and no signing key. A replayed request nonce is refused. An admitted duplicate letter takes the receipt-recovery path without being opened. |
| Anyone who finds a tunnel hostname | Can send arbitrary HTTPS | Access refuses the request at the edge. Behind Access, an unsigned or unknown-key request is refused before any handler runs. |
| Unpermitted relay | Holds containers only | Cannot read, mutate, re-seal, re-sign, or forge a receipt. |
| Authorized relay or hub (transit, `message`) | Verifies, spools, appends a chained hop signature, drops or delays | Routing authority is not edit authority. It cannot open, alter, forge, or cut and re-append the chain. |
| Misbehaving paired or declared node | Has a valid identity and its own grants | Reads only what is addressed to it. A mismatch between outer and inner claims, or between the outer and inner `ctx`, refuses. |
| Offline copy of a relay's disk or logs | Ciphertext, metadata, hop chain | No plaintext. Transit logs record no mailboxes. |
| Offline copy of the destination's disk | Decrypted mailbase after filing | Plaintext, by design. |
| Later compromise of a node's age key | The static private key | Opens past ciphertext sealed to that key. Static X25519 recipients give no forward secrecy. |
| Same-uid local process | Keys (0600) and stores | Not addressed. Total compromise of that node. |

**What transit still sees:** the outer version and purpose, `msgid`, the
`origin.key` fingerprint and `origin.node`, the destination node, the recipient age
key fingerprint, `originMesh` and the current `mesh`, `suite`, and the age header,
which shows the recipient stanza count and type. It also sees the ciphertext length
(age STREAM hides no length), the arrival time, and the node names in the hop chain.
Inside `ct` are the subject, body, To/Cc mailbox names, `threadId`/`replyTo`,
`from.name`, the inner envelope version and the inner `ctx`. Padding is an open
decision. Without padding, the ciphertext length approximates the letter length.

## Cryptographic selection

- **Chosen: age** (the age-encryption.org/v1 format, C2SP spec) through the maintained
  Rust `age` crate. It uses X25519 recipients and a ChaCha20-Poly1305 STREAM payload
  in 64 KiB chunks. Chunking suits large attachments, and it needs no session state
  for store-and-forward. Given the key, a letter can be opened by hand with the stock
  age tooling, which helps recovery and debugging. One ciphertext can carry several
  recipients.
- **Sender authentication is Aoide's job.** age has no sender authentication: anyone
  who knows a recipient key can encrypt to it. Two Ed25519 layers cover this. The
  inner MAIL.md envelope signature proves who wrote the text. The outer origin
  signature over `ctx ‖ ct` lets relays verify the origin without opening.
- **Associated data is Aoide's job.** age has no associated-data (AAD) input. The
  `ctx`-inside-and-outside binding described in Container supplies it.
- **Replay and downgrade are door requirements.** age provides neither. They are
  covered by unopened dedup, the nonce guard, and the accepted-suite list carried
  inside the self-signed binding. That list is an authenticated input. The request
  signature is not one for this purpose, because it binds only the method and the
  body digest.
- **Multi-recipient mail and areas:** mail starts with pairwise sealing. Each unique
  recipient gets its own sealed copy, matching MAIL.md's independently signed
  per-recipient envelope. age can seal one ciphertext to several recipients. Any
  recipient of such a file knows its file key, though, so only the Ed25519 layers
  bind its origin. Group keying for areas is open (MLS, RFC 9420, is the vetted
  option with FS and PCS).
- **Considered, not chosen: HPKE (RFC 9180).** It has native AAD and a single-shot
  shape. age won on multi-recipient support, streaming for large payloads, and
  letters that can be opened by hand. The cost is no native AAD, which the `ctx`
  binding compensates for.
- **Deferred:** the Double Ratchet, for interactive lanes where per-message FS is
  worth keeping ratchet state (Signal spec: FS/PCS, §8.2 caveats).
- **Rejected:**
  - home-grown crypto
  - reusing the Ed25519 key for encryption
  - converting the identity seed into an X25519 key
  - age's `ssh-ed25519` recipient mode
  - plaintext under TLS
  - unauthenticated suite negotiation

## Keys: generation, trust entry, binding, rotation, revocation

**Each host generates its own keys:** an Ed25519 identity key and an age X25519 key.
Private keys never leave the host. They are stored 0600, never printed, and never put
in an Outcome. The identity lives in runtime state (`~/.aoide/state/identity/
ed25519.key`), never in the Nix store or the repo. Declarations hold only public
keys, so no secret-management layer (agenix, sops) is involved.

**Trust entry: two ways a peer's keys become trusted, with the same result.**

| | Pairing | Declaration |
|---|---|---|
| Where | Same LAN or network only, over a direct link. Never through a relay or the HTTPS adapter, which carries no pairing methods. | Anywhere. `[mesh.<name>.keys]` in `aoide.toml` |
| Proof | Typed codes on both sides over the pubkey and nonce round trip (PAIRING.md) | Each host's own config names the other's Ed25519 public key |
| Carries | Identity keys plus each side's signed age binding. One ceremony yields both keys. | Identity keys. The age binding is then self-published and verified under the declared key. |
| Default grant | As typed (`--allow`) | `message` only. `read` and `spawn` are added per node by hand. |

- **"Same LAN" is a guard, not a proof.** No cryptography can show locality. An SSH
  tunnel makes any peer look local, and the loopback door is itself reached through a
  hop. The guard: the dialed peer address (direct, or the SSH hop's host) lies in a
  subnet the host declares local, and the ceremony never traverses a relay.
- **Declared trust is symmetric.** A peer is trusted only when each host's own config
  names the other's public key. Each door admits only keys its own config names, so a
  one-sided declaration produces refusals, never a half-trusted edge. Each side's own
  config vouches for the other side. That is what separates declaration from the
  rejected `sameOperator` shortcut, in which one side's y/N stood in for the other
  side's typed code.
- **A paired key wins over a declared one.** A mismatch between them is drift.
- **Rendering the table.** Nix hosts can render it from a future Aoide module option.
  Non-Nix hosts (native Windows, WSL) write the same lines by hand or paste what
  `aoide mesh` prints.
- **Rotation.** A new age key needs no re-pairing, only a new signed binding. A new
  identity key needs a new pairing or declaration.

**The binding** is self-signed and keyed by the identity-key fingerprint, never by a
name, because a node declared in two meshes has two names and one key. It carries:

- the age public key and its fingerprint
- the purpose (mail sealing)
- the accepted suites
- a monotonic `generation` with `rotatedAt`
- a validity window

Its rules:

- **Channel.** A node self-publishes its signed binding through the authenticated
  door, or delivers it inside the pairing ceremony. A verifier accepts the binding
  only when the signer is the exact identity key the binding names. It stores the
  binding beside the paired or declared record. Any other carriage (a relay, a mesh
  peer, a cached copy) is untrusted and is accepted only after independently
  verifying the recipient's own signature. A relay is never the authority. The root
  of trust stays the pairing record or the two-sided declaration, consistent with
  "one node, one key, one grant" and with drift reporting.
- **Freshness (learning and publishing).** Generations increase strictly per identity
  key. The highest generation seen is kept durably at rest, and the generation in use
  is bound into `ctx`. A lower generation cannot be learned or published as current
  (`stale-binding`). Without this, a replayed superseded binding would be
  byte-for-byte indistinguishable from the current one, and revocation would not
  stick. This high-water mark governs **which binding is current**. It is separate
  from the destination's retired-key grace window below, which governs **opening
  ciphertext that is already sealed**. A binding refused as stale does not, by itself,
  invalidate a letter sealed before it.
- **Offline truth.** Revoking instantly on a peer that cannot be reached is
  impossible, and this design does not claim it. With no fresh binding, a node seals
  to the last verified binding within its validity window. Once a binding is
  **expired or unknown, the letter always parks and is reported**. No configurable
  setting may seal to such a binding. A known compromise or revocation rejects
  immediately.
- **Retired keys.** A superseded private age key is kept only for a bounded grace
  window, to open mail sealed to it while it was current, and is then deleted. After
  that window, sealed but undelivered mail addressed to the retired key is refused
  (`key-retired`), and the outcome reaches the origin's retry loop. Queue expiry is
  reported, never dropped silently.
- **Already-filed mail survives key loss.** `base.jsonl` holds the opened envelope in
  plaintext, so filed letters are unaffected. Only copies that are sealed but not
  filed (outbox, in transit, spooled) become unreadable. Losing the identity signing
  key means re-pairing or re-declaring the node. There is no escrow and no recovery
  authority.

## Guarantees and at-rest honesty

**v1 claims:**

- confidentiality against relays, TLS-terminating fronts and on-path observers
- integrity and origin authenticity
- refusal of replay, tamper, chain truncation or reordering, and downgrade

**v1 does NOT claim** forward secrecy or post-compromise security. age X25519
recipients are static keys, so a later compromise of a node's age key opens its past
mail. FS/PCS is a **clearly labelled optional future decision**. Neither the User nor
v1 has accepted it as a guarantee. If it is required, the selection moves to MLS or a
ratchet BEFORE the first slice.

Decrypted letters live in plaintext in `state/mail/base.jsonl`, as today. The outbox
and every hub's spool hold sealed containers, and keys are 0600. No at-rest secrecy
is claimed.

**No secret content in logs, split by half:**

- The destination audit records the door name, status, origin and destination node
  and mailbox, `msgid`, size and outcome. It never records letter text, the subject,
  container bytes, keys or secrets.
- A transit hop records node names, `msgid`, size and outcome only, and **never
  mailbox names**. A hub cannot know them and must not claim them.

Refusals name their reason: `downgrade-refused`, `unverified-origin`, `bad-msgid`,
`wrong-recipient`, `context-mismatch`, `addressing-mismatch`, `broken-chain`,
`stale-binding`, `key-retired`.

## API resources and endpoints

The endpoints below are semantic groups. Their spellings describe the contract and are
not final URLs. The wire contract is versioned, and unknown versions are refused.
There is no key-registry endpoint: bindings come from the self-published record and
the pairing or declaration trust root. Pairing methods are never served here. Read
access is not control access, and roster access is not transcript access.

| Endpoint (semantic) | Operations and limits |
|---|---|
| `mail/deposit` | Deposit one sealed container and get back `accepted`, `duplicate` or `refused(reason)`. Also carries receipts; there is no second ack method. |
| `mail/poll` | Returns the caller's own spooled containers. The caller's identity must BE the node being polled. This is the only inbound path for a `poll` node. |
| `mail/ack` | Semantic grouping for the receipt flow. One receipt per acked `msgid` while pending. A later redelivery after the first delivery may re-spool one (MAIL.md's actual rule). |
| `areas` | Discover authorized areas, subscribe or unsubscribe, bounded history |
| `state` | Authorized snapshot of hosts, projects, sessions, terminals, agents and relationships |
| `events` | WSS subscription to permitted changes after a cursor; a bounded, recoverable stream |
| `presence` | Expiring host connection and heartbeat state, distinct from process state |
| `control` | Explicit typed actions through the existing gates, only after read and mail are proven |
| `credentials` | Owner-managed revocation. Never implicitly granted to observers. |

The first external profile is the restricted correspondent in
[MAIL-ONLY-ACCESS.md](MAIL-ONLY-ACCESS.md), sealed under the same rules when remote.

## State, events and control

The argument that TLS-terminating fronts can read HTTPS, which justifies sealing mail,
applies unchanged to `state`, `events` (including WSS responses) and `control`.
**Requests and responses on those endpoints must have a reviewed end-to-end design
before H2 or H5 is implemented.** Until then, no front that terminates TLS may sit in
front of an adapter where `control` is reachable, and such a front is never silently
exempted. No such implementation exists now.

## Shared areas and live state

Nodes receive records for the areas they may carry. Examples: `project/dxflake` for
durable correspondence, `host/<name>/sessions` for session lifecycle, and
`host/<name>/activity` for activity. The subscription model is borrowed from
Echomail.

- **Each host is authoritative for its own process state.** Records carry stable
  host-qualified IDs, the origin identity, a schema version, and an ordered origin
  sequence and epoch. Cross-host order never comes from wall clocks.
- **Verify before accepting or forwarding.** Check the origin and area membership
  against the signed binding, never against a claimed name. Prevent duplicates with
  stable IDs and bounded forwarding.
- **Sealing and removal.** Area records are sealed to subscribers. Removing a
  subscriber stops future distribution and rotates the group key where one exists. It
  cannot revoke plaintext already received.
- **Snapshots and reconnects.** A cursor-tied snapshot precedes live changes, with no
  race between them. On reconnect, retained events are replayed. If the cursor is too
  old, a fresh snapshot is required.
- **Back-pressure.** Queues and slow subscribers are bounded. Durable mail is never
  dropped silently.

## Delivery semantics

HTTPS success identifies what the server accepted or filed. It does not mean an agent
acted.

- Keep the queued, filed and fetched states distinct, and keep idempotency
  identifiers stable. A retry after a lost response must not duplicate work or
  receipts.
- The sealed container is fixed at mint time, so a retry is byte-identical and the
  `msgid` stays stable.
- WSS is a transport stream, not durable storage. Recovery comes from cursors and
  backend state.
- Neither TLS nor sealing prevents application retries. Validation, dedup and the
  nonce guard do.
- Doorbells are fixed nudges that the recipient controls, and they carry no letter
  bytes.
- Control actions use explicit capabilities and the existing admission gates.

## Migration and coexistence

**Sealing is independent of transport** and lands with transit (MAIL.md P-M4), before
any HTTPS work. Once transit exists, a hub carries sealed letters over SSH first and
over HTTPS later, so no hub reads mail. HTTPS is then just another transport for
letters that are already sealed.

- **The SSH direct-edge lane is unchanged.** The plaintext v1 direct-edge mail path
  over the loopback/SSH tunnel stays separate, and no cutover removes SSH.
- **Mixed transports interoperate through a hub.** A `poll` node on HTTPS and an
  `ssh://` node exchange letters through a hub that speaks both. A node that cannot
  seal cannot receive through transit.
- **No plaintext fallback exists** in either direction.

**Configuration** selects:

- the loopback listener and endpoint identity
- the per-node address (scheme)
- optional pins
- credentials and grants
- limits
- the **accepted-suite list**, carried in the self-signed binding

Ordinary configuration works without Nix. Optional Nix modules declare services,
render the public key table, and reference credential files, but never put secrets in
the store. Renewal, pin rotation, key rotation and revocation must also work offline.
This document enables no listener, credential or cutover.

## Required amendments elsewhere

This design lands only with these changes, each in the same commit as the code it
governs (house rule 8):

| Document | Amendment |
|---|---|
| MAIL.md §Wire | Steps 2 and 4 become destination-branch statements, with the hop branch beside them. |
| MAIL.md §The envelope, §Transit | The chained hop signature replaces `msgid ‖ 0x00 ‖ node ‖ 0x00 ‖ at ‖ 0x00 ‖ mesh`. The destination verifies the full chain, not only the last element. |
| MAIL.md rules 10, 12 | A transit node can delay or drop, and can never read or forge. The transit entry at a hub holds the sealed container and routing metadata only. |
| MAIL.md P-M4 | Scope gains age sealing, the binding and its channel, and the chain. |
| MAIL.md §Transit keys | `[mesh.<name>.keys]` establishes a symmetric declared edge (default grant `message`), not only origin verification for nodes that are not direct edges. A `pins` table is added. |
| CONTRACTS.md mesh address grammar | Node addresses accept `https://host` and `poll` beside `ssh://`. |
| PAIRING.md | Declaration becomes a second trust entry beside the ceremony. Pairing gains the local-network guard and carries the signed age binding. The HTTPS adapter serves no pairing method. |

## Phases and acceptance tests

Prerequisites in MAIL.md, in order: **P-M3** (hold and `mailPoll`) → **P-M4**
(transit, the hop chain, and age sealing over SSH) → the HTTPS phases below.

1. **H0: inventory.** Current door, event feed, grants, daemon calls, mail envelope
   and outbox. No new behaviour.
2. **H1: HTTPS relay (outbound-only nodes, one hub per mesh).** Includes:
   - the loopback HTTPS adapter on the hub, fronted by a tunnel, a VPS or a tailnet
   - `https://` and `poll` addresses
   - signed requests on every call
   - deposit and poll over HTTPS for letters already sealed at P-M4
   - hold-flavored spooling for `poll` nodes
   - receipts back over the same path

   No node accepts inbound connections except the hub's loopback adapter.
3. **H2: state snapshots and WSS events**, only after the reviewed end-to-end design.
4. **H3: authorized shared areas.** Membership, duplicates and loops, origin
   verification, removal semantics.
5. **H4: direct HTTPS edges (optional).** For a node that legitimately exposes a URL,
   such as a VPS, with optional pins. It is never a routable Aoide door on a home
   node.
6. **H5: typed control actions**, using existing policy and audit only, and only after
   the same reviewed end-to-end design.

Acceptance evidence for P-M4 sealing and H1. These are required tests, not
suggestions:

- **Ciphertext transit:** inspect a wire capture on both the SSH and HTTPS hops, plus
  the hub's spool, transit entry, log and audit. None may contain letter bytes, the
  subject, the body or mailbox names. Transit logs carry node names and outcome only.
- **Outbound only:** during a full send, poll and ack round trip, no home node holds a
  listening socket on a routable address.
- **Signed requests everywhere:** through the tunnel, an unsigned request, an
  unknown-key request and a replayed nonce are all refused. The observed loopback
  source grants nothing.
- **No unnecessary open:** an unverified origin is refused, and a previously admitted
  duplicate returns `duplicate`. Neither case opens anything.
- **Context binding:** a relay that re-signs another origin's `ct` under its own key
  and `ctx` is refused at the destination (`context-mismatch`). A mismatch between the
  inner and outer `ctx` in any field refuses before any inner field is used.
- **Addressing mismatch:** a legitimately trusted node whose inner header is addressed
  elsewhere is refused, never filed, never re-spooled.
- **Wrong recipient:** a container for B opened with C's age key fails.
- **Tamper, with authorized and unauthorized relays:** a flipped byte in `ct`, an
  outer `ctx` field, `to.age`, `msgid`, `mesh` or any hop signature is refused and
  audited.
- **Hop chain:**
  - a removed middle entry fails
  - swapped entries fail
  - truncating to entry `j` and re-appending fails
  - a chain whose last `next` is not the destination fails
  - a chain truncated by dropping the tail verifies but yields no ack, and the outbox
    reports the letter as undelivered
- **Zone:** a relabel by a node that is not a gate is `zone-violation`. An accepted
  gate rewrite verifies through the gate's hop signature.
- **Replay and retry:**
  - A resent capture, and a retry after a lost response, yield `duplicate` with no
    second filing.
  - A retry of the same immutable container over a different route dedups rather
    than colliding (the comparison excludes `mesh` and `transit`).
  - A crash between base and seen re-accepts exactly once.
  - A same-`msgid` collision with different immutable bytes, within the same origin
    and destination, is refused.
  - A different origin cannot preclaim the ID at a relay.
  - A filed duplicate recovers a lost receipt without duplicating a receipt that is
    already pending.
- **Trust entry:**
  - A one-sided declaration is refused in both directions.
  - A two-sided declaration admits `message` only.
  - A declared key that disagrees with a paired key is drift, and the paired key wins.
  - A pairing attempt through a relay or the HTTPS adapter is refused.
  - A new age binding is accepted without re-pairing.
- **Revocation and rotation:**
  - A replayed superseded binding is refused for current use.
  - A binding whose generation is below the high-water mark cannot become current.
    Mail already sealed to it while it was current still opens within the grace
    window.
  - A known revocation rejects immediately.
  - An expired or unknown binding always parks and is reported.
  - `key-retired` reaches the origin's retry loop.
- **Receipts:** one per acked `msgid` while pending. A later redelivery after delivery
  may re-spool one. No receipt chain.
- **Crash and recovery:** a torn tail is truncated, a resend is byte-identical, and no
  receipt is duplicated. Already-filed letters survive loss of the age key.
- **Windows and Linux, no SSH:** send, poll, fetch and ack work end to end, with a
  `poll` node on each OS that has no SSH account or binary, through an HTTPS hub.
- **Downgrade:** a plaintext request, mixed suite components, or an older
  accepted-suite entry is refused with a taught error and an audit line.
- **Invalid credential, a bad pin where one is declared, and an unauthorized inbox**
  are rejected, as in the plaintext profile's negative tests.

## Open design decisions

- **Forward secrecy:** age does not provide it. Adopt MLS or a ratchet before H1 if the
  User requires FS/PCS.
- **Padding policy** for ciphertext length.
- **Mesh model**, among three options:
  - meshes as routing zones only (today: trust is flat per node)
  - trust boundaries per mesh (grants and visibility per mesh)
  - separate identities per mesh

  The leaning is trust per mesh for any mesh that contains machines the User does not
  own.
- **Group keying for shared areas:** a joiner cannot open letters sealed before it
  joined. Options are pairwise fan-out, age multi-recipient (origin bound only by
  signatures), or MLS. The concept also needs one noun: board, room, area or channel.
- **User sign-off on symmetric declared trust.** It reopens the parked `sameOperator`
  question (PAIRING.md), so declaration does not land without the User's explicit
  approval.
- **Relay operator trust:** the shape of the Cloudflare Access policy (service token
  versus mTLS, per mesh or per node).
- **Whether P-M4 seals direct-edge letters too:** sealing at mint time avoids
  re-sealing when a route changes. The alternative seals only letters routed through a
  hub.
- **Relay failover:** whether a sender moves to the next declared relay when the
  first is unreachable but not `down`. MAIL.md's router picks the first eligible
  relay only.
- **Area grants and observation scopes; cursor and retention limits.**

## References

- [Mail](MAIL.md): envelope bytes, store, outbox, transit, security model.
  [Pairing](PAIRING.md): identity, signed wire authentication, the ceremony,
  transport. [Mail-only external access](MAIL-ONLY-ACCESS.md).
  [Daemon boundary](AOIDED.md). [Core portability](CORE-POSIX.md): Linux and native
  Windows.
- [age v1 (C2SP)](https://github.com/C2SP/C2SP/blob/main/age.md): X25519 recipients,
  ChaCha20-Poly1305 STREAM payload, header MAC, no sender authentication, no
  associated data.
- [RFC 9180 (HPKE)](https://www.rfc-editor.org/rfc/rfc9180.txt): the alternative that
  was considered. §9.1.1 signs `(enc, ct)` for KCI resistance. §9.7.4 notes no forward
  secrecy against recipient compromise, which holds equally for static age recipients.
- [RFC 9420 (MLS)](https://www.rfc-editor.org/rfc/rfc9420.txt): asynchronous group
  keying with FS and PCS, an untrusted Delivery Service, epochs and Remove.
- [Double Ratchet](https://signal.org/docs/specifications/doubleratchet/): FS/PCS for
  pairwise sessions. §8.2 names what a key compromise still defeats.
- [RFC 8446 (TLS 1.3)](https://www.rfc-editor.org/info/rfc8446/): transport
  confidentiality and authentication, not a substitute for payload protection.
