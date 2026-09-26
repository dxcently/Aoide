# Aoide HTTPS mesh API

Status: proposed design. No sealing, roster, per-mesh grant, HTTPS listener,
credential or cutover exists or is authorized. The library-profile gate is reduced
to confirming the `age` crate profile and pinning the canonical `ctx` and roster
signature encodings; a follow-up review remains before implementation. MAIL.md
carries the same design for the mail workstream and owns the slice order. Where
this document and PAIRING.md or CONTRACTS.md conflict, those describe what is built
and win until the amendments listed below land.

**Sealing is mandatory.** From P-SEAL on, every letter that leaves its node is
sealed at mint, on every transport. No plaintext profile and no per-node toggle
exist. The one plaintext path left is the SSH direct lane to a peer that has not
yet published an age binding (Migration and coexistence); no relay, hub or HTTPS
hop ever carries plaintext.

## Why this exists

An Aoide node must be reachable from networks where no VPN runs and only HTTPS on
443 gets out, such as a Windows laptop on a locked-down network. A user may also
belong to several meshes — their own machines, an organization, a club — each with
its own relays, transports and trust.

Doors stay **loopback-only**: no Aoide door ever binds a routable address. So no
home node accepts inbound HTTPS. Every node connects OUT to a relay, and **the relay
is the first HTTPS slice**, not a late phase. Direct HTTPS edges exist only as a
later optional mode for a node that legitimately exposes a URL, such as a VPS.

## Objective and boundary

An Aoide-native HTTPS API for mail, authorized mesh observation, and separately
granted control, without SSH accounts. SSH remains supported. This is a design for
the application API, the trust model and the payload-protection boundary, not new
TLS, cryptography or RPC machinery. It reuses the existing mail store, identities,
command services, authorization and audit. The network adapter stays separate from
the local daemon (AOIDED.md: aoided is a local socket service). The adapter binds
loopback, and whatever fronts it (a tunnel, a reverse proxy) is a transport hop,
never a door.

**Grants, not shell.** HTTPS exposes only a node's grants in a mesh (`read`,
`spawn`, `message`). It never exposes a shell, a generic remote command string, or
unrestricted registry dispatch. A roster node defaults to `message` only; `read`
and `spawn` are added on its roster line by the operator. A paired node's grants
are set by hand with `aoide node allow`.

## Transports and relays

A relay is declared **per mesh**: in the roster (`relays = [...]`) for a roster
mesh, in `[mesh.<name>] relays` for a pair mesh (MAIL.md §Transit). A node's address
selects the transport by URL scheme. The transport is a per-node option and is not
baked into the design:

| Address | Transport | Examples |
|---|---|---|
| `ssh://[user@]host[:port]` | SSH tunnel to the loopback door (today's transport) | LAN boxes |
| `https://host` | HTTPS to a loopback adapter behind something that owns 443 | a Cloudflare Tunnel hostname, a VPS with public 443, a tailnet hostname. Aoide treats all three identically. |
| `poll` | none inbound: the node has no address. It connects out to its mesh's relay, deposits, and polls for its own letters. | a laptop on a locked network |

A hub's outbox entry for a `poll` node is `hold`-flavored. The hub never dials it,
and only the node's own `mailPoll` drains it (MAIL.md P-M3). A hub may receive over
one transport and forward over another. Both hops carry the same sealed container.

The home mesh as a roster mesh. Every machine's config names only the operator key
it trusts; everything else comes from the signed roster:

```toml
# config.toml, on every machine of the home mesh
[mesh.home]
operator = "ed25519:…"                         # the home mesh's operator key (see Rosters)

[mesh.club.pins]                               # optional; end-to-end TLS only
evo-hub = "sha256:…"
```

```toml
# rosters/home.toml, on the operator's machine; signed and sent to every node on it
mesh    = "home"
version = 12                                   # written by `aoide mesh roster sign`
relays  = ["sakaki"]

[nodes]
sakaki     = { key = "ed25519:…", age = "…", address = "https://aoide.necoconeco.net", grant = ["message", "read"] }
yomi-strix = { key = "ed25519:…", age = "…", address = "ssh://khoa@192.168.1.158", grant = ["message", "read", "spawn"] }
thinkchiyo = { key = "ed25519:…", age = "…", address = "poll" }
```

Hostnames are illustrative. In the home mesh, sakaki is the hub behind its existing
Cloudflare Tunnel, which dials out and forwards to a loopback listener. Other meshes
may use a VPS or a tailnet hub.

### Degenerate topologies

| Topology | Behaviour |
|---|---|
| One host | This layer is not needed. Conducting and local `self/<name>` mail run with no mesh, relay or HTTPS. The HTTPS mesh is an add-on, never a prerequisite. |
| One relay for a mesh, relay down | The relay is a single point of failure for **availability, not secrecy**. Letters wait in each sender's outbox and retry, so nothing is lost. LAN peers with a direct `ssh://` edge still deliver. |
| One relay for a mesh, relay compromised | Because letters are sealed and signed, it can drop, delay and observe metadata (who wrote to whom, size, time). It can never read or forge a letter, and never alter a roster. |
| Several relays (`relays = ["a", "b"]`) | For availability. The sender takes the first relay, in declaration order, that it trusts in the mesh, can reach by address, and is not `down` (MAIL.md routing step 2). A relay that is unreachable but not declared `down` is still chosen, and the letter waits for it. Failing over automatically on unreachability is open. |
| Relay that is also a node (sakaki) | The hub is a role. Its own mail files locally, like any destination's. |

## Trust per mesh

A mesh is a routing zone **and** a trust scope. A grant is given in one mesh and
holds only there: a machine trusted in one mesh gains nothing in another, even
when both meshes contain the same third machine. A mesh is one of two kinds:

- **Roster mesh.** One operator key signs the mesh's node list, grants and relays
  (Rosters, below). The User's own fleet, an organization's machines, or a club
  with an admin.
- **Pair mesh.** Trust is the pairwise records made in it (PAIRING.md). Links
  between machines of different owners who share no operator.

Rules:

- **Every signed request names the mesh it acts in**, inside the per-request
  signature. The door checks the caller's grant in that mesh and in no other: the
  caller's roster line for a roster mesh, the paired record's grant in that mesh
  for a pair mesh. For mail, the request's mesh must equal the envelope's `mesh`.
  The remote sub-agent rules (register §32: any-frame read, spawn, the
  remote-parent inject match) read the grant in the request's mesh.
- **Local narrowing only.** A node's own `aoide node allow <node> <cap> off --mesh
  <m>` narrows what its door grants in that mesh, and wins over the roster.
  Nothing local widens a roster grant.
- **One node, one identity key, in every mesh.** A node may sit in several meshes
  with different grants in each. Trust is per mesh; identity is not.
- **A pairing names its mesh.** `aoide pair … --mesh <name>`, required when this
  node knows more than one mesh. The record's grant is its grant in that mesh.
- **Migration.** Grants are global per node today (`nodes.json` `allows`, register
  §11). The slice that lands per-mesh trust moves every existing paired record into
  the User's home mesh (`[pairing] homeMesh`, default `home`). Its `allows` becomes
  its grant there, unchanged, so every trusted peer keeps exactly what it had. A
  request that names no mesh (a peer not yet upgraded) is evaluated in the home
  mesh only, and only against a migrated paired record. It never matches a roster
  mesh.
- **Rooting a mesh that holds paired records.** When the User runs `aoide mesh
  roster init home`, the roster becomes the home mesh's trust. `init` lists the
  mesh's paired records: each of the User's own machines is added to the roster,
  and any other owner's machine is re-paired into a pair mesh. A paired record left
  in a roster mesh grants nothing there and is reported by `aoide mesh`.

## Rosters: one operator's machines

The use case is one person setting up many machines: an organization's admin, or
the User's own fleet. The roster replaces per-pair ceremonies between those
machines. It is shaped like agenix: public keys in one file, one signer.

**Setup, in order:**

1. **Each machine generates its own keys** on first setup: the Ed25519 identity key
   and the age X25519 key (Keys, below). `aoide onboard` ends by printing the
   machine's **node line**, which `aoide identity` also prints at any time:

   ```text
   thinkchiyo = { key = "ed25519:<hex>", age = "<binding>" }   # SHA256:<fingerprint>
   ```

   The line carries the identity public key and the self-signed age binding. The
   fingerprint is for the operator's eye. Nothing private is printed.
2. **The operator roots the mesh once.** `aoide mesh roster init <mesh>` mints the
   mesh's operator key on the operator's machine, writes an empty roster source,
   and prints the **operator line**, `operator = "ed25519:<hex>"`, with its
   fingerprint.
3. **The operator adds each machine**: paste its node line under `[nodes]`, add its
   `address` and, when it needs more than `message`, its `grant`, then run `aoide
   mesh roster sign <mesh>`.
4. **Each machine trusts the operator key once**, by one of:
   - the operator line in its config, `[mesh.<name>] operator = "…"`, rendered by
     Nix or written by hand;
   - `aoide mesh join <mesh> --operator <key>`, which records the same key in state
     on a host whose config is not hand-editable;
   - `aoide mesh join <mesh> <operator-node>`, one LAN ceremony against the
     operator's machine that carries the operator key and the current signed
     roster. It rides pairing's ceremony and its local-network guard, never a
     relay, and it creates no pairwise record.

   The first roster reaches a machine through the LAN join, or by file with `aoide
   mesh roster accept <file>`. Later versions arrive as letters. From then on the
   machine trusts every roster node with the roster's grants.

**The file.** TOML, edited by the operator; the example is under Transports and
relays. Keys:

- `mesh` — the mesh name.
- `version` — written by `sign`, strictly increasing.
- `relays` — relay node names, in preference order.
- `[nodes]` — one line per node:
  - `key` — the identity public key;
  - `age` — the node's self-signed age binding;
  - `address` — `ssh://…`, `https://…` or `poll`; default `poll`;
  - `grant` — capabilities; default `["message"]`.
- Optional `[status]` (`hold`, `down`) and `[gates]`, with MAIL.md's meanings.

The source lives at `$AOIDE_ROOT/rosters/<mesh>.toml` by default; `--file` points
anywhere, such as the operator's Nix repository, since it holds public keys only.
For a roster mesh the roster IS the mesh declaration: config's `[mesh.<name>]`
carries only `operator` and optional `pins`. `nodes`, `grant`, `relays`, `status`
or `gates` beside `operator` is a load-time error, so there is one source.

**Signing.** `aoide mesh roster sign <mesh>` validates (name grammar, capability
vocabulary, every binding verifies under the key on its own line, no key on two
lines), writes the next `version` into the file, and writes the detached signature
`<mesh>.toml.sig`:

```text
roster_sig = operator Ed25519 over canonical("aoide/roster", mesh, version, sha256(file bytes))
```

It then spools the signed pair to every node on the roster. The operator key signs
rosters and board takeover records (MAIL.md §Boards, "Takeover") and nothing else;
it is never a node's identity key, even on the machine that is both.

**Accepting.** Every node keeps the roster in force at
`state/mesh/<mesh>/roster.toml` with its `.sig`, and the highest version seen per
(mesh, operator key) at rest. A node accepting a roster:

1. Verifies the signature under the operator key it trusts for that mesh, before
   parsing anything. No trusted key for that mesh is `unknown-operator`.
2. Requires the parsed `mesh` and `version` to equal the signed ones.
3. Refuses a version not above the high-water mark (`stale-roster`), so a replayed
   older roster never returns.
4. Refuses every roster for a mesh whose config operator line and state record
   disagree (`operator-mismatch`) until the User resolves it.

A version that changes an existing node's identity `key` is applied, and `aoide mesh`
reports the re-keyed node, because a new key on an old name is what a stolen
operator key would sign.

**Carriage.** A signed roster travels like mail: a `roster` letter, sealed to each
node and origin-signed by the operator's machine, through the ordinary outbox,
relays and poll. Its authority is the operator signature, never the carrier. A relay
can drop or delay a roster; it can never alter or forge one. A node verifies the
enclosed roster first and then the letter's origin against it, so a machine can
accept its first roster by letter from an origin it did not yet know.

**Grants.** A node's roster line is what every other node on the roster grants it,
at its own door, in this mesh. Local narrowing (above) is the only per-host
exception.

**Revocation.** Deleting a line and re-signing is revocation. Each node applies the
new version on receipt: its door refuses the removed key in that mesh, its router
stops routing to it, and every board of that mesh with a member on the removed
node rotates to a new epoch (MAIL.md §Boards). A node that has not yet received the
new roster still trusts the removed machine. The revocation reaches it through its
relay like any letter. Rosters carry no expiry, which is an open decision. A board
whose owner node is removed is frozen until the operator takes it over.

**The operator key and re-rooting.** The operator key is one Ed25519 key per mesh,
stored `0600` at `state/operator/<mesh>.key` on the operator's machine. It is
never on a relay, never printed, never in the Nix store. Losing it does not stop the
mesh: every node keeps the last roster it verified, so only changes stop (add,
remove, grant, relay). `aoide mesh roster reroot <mesh>` mints a new operator key
and signs the current roster source with it. The cost:

- every machine must trust the new key the way it trusted the first: one config line
  or one LAN join per machine;
- nothing else changes: no node re-keys, nothing is re-paired, no board re-keys,
  and filed mail is untouched.

Trust is replaced, not added. A node that trusts the new key refuses rosters under
the old one, and its high-water mark restarts under the new key. If the old key was
**compromised** rather than lost, its holder can sign rosters that every untouched
machine accepts until it is re-rooted. The exposure window is the time it takes to
touch every machine; a successor key committed in advance would shorten it (open).

**`sameOperator`.** The roster answers the parked question: one operator is one
roster signer. Nodes on one roster need no pairing between them, so a converge has
nothing to skip. `mesh.<name>.sameOperator` retires with P-ROSTER and is never
acted on before then. `aoide mesh pair` stays the converge for pair meshes.

**Pairing stays for different owners.** A link between machines with no common
operator, such as the User's box and a friend's box, is a per-pair LAN pairing made
in a pair mesh. There is no crowd pairing mode (one code shown to a room, confirmed
by a typed fingerprint step). Many machines of one owner are a roster, and many
owners under one admin are a roster that admin signs.

**SSH keys (register §31).** A roster grant is an Aoide grant checked at the door.
It never grants, limits or reads SSH access. An unrestricted key in
`authorized_keys` is a full shell and bypasses every grant in every mesh, so trust
per mesh holds on a host only once every mesh SSH key there is restricted to the
door entry or removed. Inside a roster mesh whose nodes reach each other through a
relay, an `ssh://` edge between two machines is optional, and its key can be
removed rather than restricted.

## Connections and trust

**TLS may terminate at an intermediary.** Cloudflare terminates TLS and can read
plaintext HTTPS. The design follows from that:

- **Sealing is load-bearing, not optional.** No relay or proxy ever holds a key that
  opens a letter or a board post.
- **Node authentication lives in the Aoide layer.** Every request on every transport
  carries the existing per-request Ed25519 signature over method, path, timestamp,
  nonce, body digest and the mesh the request acts in. The receiver resolves that
  signature to a node through that mesh's roster or paired records. A request's
  observed source address is never an input: behind a tunnel, every request looks
  like loopback. TLS is additional and never replaces payload protection.
  Revocation and scope changes affect active streams as well as new requests.
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

1. Verify the signed request, and the caller's grant in the mesh it names.
2. Recompute `ctx` from the outer fields.
3. Verify the outer origin signature over `ctx ‖ ct` under the key that `origin.key`
   names, resolved through the roster or paired records of `originMesh`.
4. Check the zone on the outer `mesh`.
5. Dedup against previously validated and admitted records.

Then it branches once:

**Hop branch: spool, never open.** Append this hop's chained hop signature and
re-spool toward the destination. A hub never opens a letter and never needs a
recipient key. It cannot read, edit or forge.

**Destination branch: open, match, file.** This branch runs only when self is the
destination. Steps, in order:

1. Open `ct` with the age identity for `to.age`: this node's own age key for a
   letter, the board's epoch identity for a post.
2. Require the inner `ctx` to equal the recomputed outer `ctx`, byte for byte (see
   Container).
3. Recompute the inner `msgid` and require it to equal the container `msgid`.
4. Require `header.from.node == origin.node`, `header.to.node == to.node` and
   `header.originMesh == ctx.originMesh`.
5. Require `origin.key` to be the single key on record for that node.
6. Verify the inner envelope signature.
7. Walk the full hop chain.
8. Check membership: the origin in the mesh; for a post, the origin a member node of
   the board at that epoch.
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
   identity key at pairing or through the roster, and verifies the binding BEFORE
   encrypting. The generation high-water mark and revocation rules below apply. A
   binding carried by a relay, or a fingerprint signed by the origin, does not prevent
   substitution. The recipient's own signature under a key pinned by the trust root
   does.
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

- **`to.age` names what the payload is sealed to.** For a letter it is the
  destination node's age key. For a board post it is the board's epoch recipient,
  and `ctx` also carries the board id and the epoch number, so a post cannot be
  replayed onto another board or epoch. One post is sealed once. Each member node's
  copy differs only in `to.node`, which `ctx` covers.
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
  the library-profile gate before any P-SEAL test. `suite` is one registered
  identifier that names the age format version and recipient type (for example, age
  v1 with X25519), so a downgrade cannot be expressed by mixing components.
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
entries. A flat per-hop signature over `msgid ‖ node ‖ at ‖ mesh`, with the `transit`
list outside every signature, would let a later hop delete or reorder earlier
entries. Each hop instead signs its place in a chain and whom it hands the letter to:

```text
hop_sig_n = sign_n(canonical(msgid, prev_n, node_n, next_n, at_n, mesh_n))
prev_1    = msgid
prev_n    = sha256(canonical bytes of entry n-1, its sig included)
```

- Fields are length-prefixed, not joined with `0x00`.
- `next` is the node this hop hands the letter to.
- The destination walks the chain from `msgid`. It verifies each signature under that
  hop's key in the mesh the hop carried the letter in, and requires
  `entry[j].next == entry[j+1].node`. It also requires the last entry's `next` to be
  self.
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
| Relay carrying a roster | Drops, delays or replays a roster letter | Cannot alter or forge a roster (operator signature). An older version is refused (`stale-roster`). A dropped revocation is delayed, never cancelled. |
| Node trusted in one mesh | Has a valid identity and its grants there | Gains nothing in another mesh. Reads only what is addressed to it. A mismatch between outer and inner claims, or between the outer and inner `ctx`, refuses. |
| Node removed from a roster | Its keys, and everything it already received | Refused at every door holding the new version. Opens no post of an epoch after its boards rotate. Keeps what it already filed. |
| Stolen operator key | Signs rosters and board takeovers for that mesh | Every machine trusting that key accepts its rosters until re-rooted. It can seize the mesh's boards: take them over, and re-key a member node's roster line to keys it holds so every later wrap reaches it. It opens no epoch wrapped before the re-key. Every takeover and every re-keyed line is marked on each member node and audited. The operator key is the mesh's root and is guarded as one. |
| Member removed from a board | The epoch identities it held | Opens posts of those epochs, never a later one. |
| Offline copy of a relay's disk or logs | Ciphertext, metadata, hop chain, board membership records | No plaintext and no mailbox names. Transit logs record no mailboxes. |
| Offline copy of the destination's disk | Decrypted mailbase after filing, unwrapped epoch identities | Plaintext, by design. |
| Later compromise of a node's age key | The static private key | Opens past ciphertext sealed to that key, and every epoch identity wrapped to it. Static X25519 recipients give no forward secrecy. |
| Same-uid local process | Keys (0600) and stores | Not addressed. Total compromise of that node. |

**What transit still sees:** the outer version and purpose, `msgid`, the
`origin.key` fingerprint and `origin.node`, the destination node, the recipient age
key fingerprint, `originMesh` and the current `mesh`, `suite`, and the age header,
which shows the recipient stanza count and type. It also sees the ciphertext length
(age STREAM hides no length), the arrival time, and the node names in the hop chain.
For a board post it also sees the board id, the epoch, and the board's member nodes
(its membership records are signed, not sealed). Inside `ct` are the subject, body,
To/Cc mailbox names, `threadId`/`replyTo`, `from.name`, the inner envelope version and
the inner `ctx`; a board's member mailboxes and title ride sealed too. Padding is an
open decision. Without padding, the ciphertext length approximates the letter length.

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
  signature is not one for this purpose, because it binds only the method, the mesh
  and the body digest.
- **Letters are sealed per recipient node.** Each unique recipient gets its own
  sealed copy, matching MAIL.md's independently signed per-recipient envelope.
- **Boards use epoch recipients, with age alone.** Each board epoch has its own age
  X25519 identity. Posts are sealed to its recipient, and the identity is wrapped
  (age-sealed) to each member node's age key: the agenix recipient model, with no
  new construction. A member removed at epoch n never receives epoch n+1's identity.
  An epoch key is shared by the board's member nodes by design; it is not a host key.
  Within an epoch there is no FS or PCS, and any member can seal to the epoch, so
  only the Ed25519 layers and the membership check bind a post's author. MLS
  (RFC 9420) is the vetted upgrade if FS/PCS is required, and is not chosen.
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
in an Outcome. They live in runtime state (`~/.aoide/state/identity/ed25519.key`,
`~/.aoide/state/identity/age.key`), never in the Nix store or the repo. Rosters and
config hold only public keys, so no secret-management layer (agenix, sops) is
involved. The operator key (Rosters) and board epoch identities (MAIL.md §Boards) are
the only other private keys, with the same storage rules.

**Trust entry: two ways a peer's keys become trusted.**

| | Pairing | Roster |
|---|---|---|
| For | Two machines of different owners | One operator's machines: the User's fleet, an organization, a club with an admin |
| Where | Same LAN or network only, over a direct link. Never through a relay or the HTTPS adapter, which carries no pairing methods. | Anywhere: the roster is signed. The operator key is trusted once per machine, by config line, `mesh join --operator`, or one LAN join. |
| Proof | Typed codes on both sides over the pubkey and nonce round trip (PAIRING.md; enrollment windows, register §5) | The operator's signature over the roster, under the key the machine trusts |
| Carries | Identity keys plus each side's signed age binding. One ceremony yields both keys. | Each node's identity key and signed age binding, its address and grants, and the relays |
| Scope | The pair mesh the pairing names | The roster's mesh |
| Default grant | As typed (`--allow`) | `message` only. `read` and `spawn` are added on the node's line. |

- **"Same LAN" is a guard, not a proof.** No cryptography can show locality. An SSH
  tunnel makes any peer look local, and the loopback door is itself reached through a
  hop. The guard, for pairing and for the LAN join: the dialed peer address (direct,
  or the SSH hop's host) lies in a subnet the host declares local, and the ceremony
  never traverses a relay.
- **In a roster mesh the roster is the only trust.** A paired record in that mesh for
  a key the roster lists is inert and reported. A key the roster does not list is not
  trusted in that mesh, whatever a pairing says.
- **Rendering the operator line.** Nix hosts render `[mesh.<name>] operator` from a
  future Aoide module option. Non-Nix hosts (native Windows, WSL) write the same line
  by hand or run `aoide mesh join`.
- **Rotation.** A new age key needs no re-pairing and no roster re-sign, only a new
  signed binding. A new identity key needs a new pairing, or a new node line and a
  re-sign.

**The binding** is self-signed and keyed by the identity-key fingerprint, never by a
name, because a node in two meshes may have two names and one key. It carries:

- the age public key and its fingerprint
- the purpose (mail sealing)
- the accepted suites
- a monotonic `generation` with `rotatedAt`
- a validity window

Its rules:

- **Carriage.** A node self-publishes its signed binding through the authenticated
  door (`aoide/binding`, a signed read of the node's own current binding). The
  pairing ceremony and the roster also carry it. A verifier accepts the binding only
  when the signer is the exact identity key the binding names. It stores the binding
  beside the paired record or the roster line. Any other carriage (a relay, a mesh
  peer, a cached copy) is untrusted and is accepted only after independently
  verifying the recipient's own signature. A relay is never the authority. The root
  of trust stays the pairing record or the roster, consistent with "one node, one
  key" and with drift reporting. The roster's copy is a floor: a newer
  self-published binding with a higher generation supersedes it without a re-sign.
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
  reported, never dropped silently. Epoch identities a node has already unwrapped are
  kept in its own state and survive the rotation; a wrap still in flight follows the
  same grace rule as any letter.
- **Already-filed mail survives key loss.** `base.jsonl` holds the opened envelope in
  plaintext, so filed letters and posts are unaffected. Only copies that are sealed
  but not filed (outbox, in transit, spooled) become unreadable. Losing the identity
  signing key means re-pairing the node or giving it a new roster line. There is no
  escrow and no recovery authority.

## Guarantees and at-rest honesty

**v1 claims:**

- confidentiality of letters and board posts against relays, TLS-terminating fronts
  and on-path observers
- integrity and origin authenticity
- roster authenticity and freshness
- refusal of replay, tamper, chain truncation or reordering, and downgrade

**v1 does NOT claim** forward secrecy or post-compromise security. age X25519
recipients are static keys, so a later compromise of a node's age key opens its past
mail and every board epoch wrapped to it. FS/PCS is a **clearly labelled optional
future decision**. Neither the User nor v1 has accepted it as a guarantee. If it is
required, the selection moves to MLS or a ratchet BEFORE P-SEAL.

Decrypted letters and posts live in plaintext in `state/mail/base.jsonl`, as today.
The outbox, every hub's spool and every board log on a relay hold sealed containers,
and keys are 0600. No at-rest secrecy is claimed.

**No secret content in logs, split by half:**

- The destination audit records the door name, status, origin and destination node
  and mailbox, `msgid`, size and outcome. It never records letter text, the subject,
  container bytes, keys or secrets.
- A transit hop records node names, `msgid`, size and outcome only, and **never
  mailbox names**. A hub cannot know them and must not claim them.

Refusals name their reason: `downgrade-refused`, `unverified-origin`, `bad-msgid`,
`wrong-recipient`, `context-mismatch`, `addressing-mismatch`, `broken-chain`,
`stale-binding`, `key-retired`, `not-in-mesh`, `unknown-operator`, `stale-roster`,
`operator-mismatch`, `not-a-member`, `stale-epoch`, `not-owner`, `stale-takeover`.

## API resources and endpoints

The endpoints below are semantic groups. Their spellings describe the contract and are
not final URLs. The wire contract is versioned, and unknown versions are refused.
There is no key-registry endpoint: bindings come from the node's own `aoide/binding`
read, the pairing and the roster. There is no roster endpoint either: a roster
travels as a letter. Boards need no endpoint of their own: posts, membership records
and wraps all move as letters. Pairing methods are never served here. Read access is
not control access, and a session listing is not transcript access.

| Endpoint (semantic) | Operations and limits |
|---|---|
| `mail/deposit` | Deposit one sealed container and get back `accepted`, `duplicate` or `refused(reason)`. Also carries receipts, rosters, wraps and board posts; there is no second ack method. |
| `mail/poll` | Returns the caller's own spooled containers. The caller's identity must BE the node being polled. This is the only inbound path for a `poll` node. |
| `mail/ack` | Semantic grouping for the receipt flow. One receipt per acked `msgid` while pending. A later redelivery after the first delivery may re-spool one (MAIL.md's actual rule). |
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
before H2 or H4 is implemented.** Until then, no front that terminates TLS may sit in
front of an adapter where `control` is reachable, and such a front is never silently
exempted. No such implementation exists now.

## Boards and live state

Boards are MAIL.md's (§Boards): every group conversation, from a two-member direct
board to a project's or an organization's, with a key per epoch wrapped to its
member nodes. Their posts, membership records, wraps and takeover records travel as
sealed letters, so HTTPS adds nothing for them beyond the mail endpoints, and a relay
holding a board's log holds ciphertext only.

Live state is not a board. `state` and `events` serve host-authoritative records,
such as `host/<name>/sessions` for session lifecycle and `host/<name>/activity` for
activity, as event streams:

- **Each host is authoritative for its own process state.** Records carry stable
  host-qualified IDs, the origin identity, a schema version, and an ordered origin
  sequence and epoch. Cross-host order never comes from wall clocks.
- **Verify before accepting or forwarding.** Check the origin and the subscriber's
  grant in the mesh against the signed binding, never against a claimed name.
  Prevent duplicates with stable IDs and bounded forwarding.
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
  or post bytes.
- Control actions use explicit capabilities and the existing admission gates.

## Migration and coexistence

**Sealing is independent of transport** and lands first, at P-SEAL, on the SSH direct
lane, before the roster, HTTPS and transit. By the time a hub carries anything, over
SSH or HTTPS, every letter is already sealed, so no hub ever reads mail. HTTPS is then
just another transport for sealed letters.

- **Per-peer upgrade on the SSH direct lane.** From P-SEAL on, a node seals every
  letter to a destination whose signed binding it holds, and never sends that
  destination plaintext again. A destination that has published no binding yet (not
  upgraded) still receives the plaintext v1 envelope over the direct SSH lane only,
  never through a relay, a hub or HTTPS. No cutover removes SSH.
- **Mixed transports interoperate through a hub.** A `poll` node on HTTPS and an
  `ssh://` node exchange letters through a hub that speaks both. A node that cannot
  seal cannot receive through transit.
- **No plaintext fallback exists** for a sealed destination, in either direction.
- **Grants move into meshes** as Trust per mesh describes: every existing paired
  record moves into the home mesh with its grant unchanged.

**Configuration** selects:

- the loopback listener and endpoint identity
- per mesh: the operator key (roster mesh) or the node declaration (pair mesh)
- optional pins
- credentials, and local narrowing of grants
- limits
- the **accepted-suite list**, carried in the self-signed binding

Ordinary configuration works without Nix. Optional Nix modules declare services,
render the operator line, and reference credential files, but never put secrets in
the store. Renewal, pin rotation, key rotation, roster updates and revocation must
also work offline. This document enables no listener, credential or cutover.

## Required amendments elsewhere

This design lands only with these changes, each in the same commit as the code it
governs (house rule 8):

| Where | Amendment |
|---|---|
| CONTRACTS.md §4 `config.toml` | `[mesh.<name>] operator`; a roster mesh's section carries only `operator` and `pins`; `[pairing] homeMesh`; `sameOperator` retires; the rule that one node name may not appear in two meshes is dropped, so a node may be declared in several meshes (its key the same in each). |
| CONTRACTS.md §4 state files | `state/identity/age.key`, `state/operator/<mesh>.key`, `state/mesh/<mesh>/` (roster in force, its signature, the high-water mark), `state/boards/`. |
| CONTRACTS.md §6 | Signed requests carry the mesh they act in. `mailDeposit`/`mailPoll` take the sealed container; admission reads the caller's grant in that mesh. `aoide/binding`. The refusal reasons above. |
| CONTRACTS.md §7 `nodes.json` | `allows` becomes a grant per mesh, and existing records migrate into the home mesh. |
| CONTRACTS.md mesh address grammar | Node addresses accept `https://host` and `poll` beside `ssh://`. |
| PAIRING.md | The roster is the second trust entry beside the ceremony, for one operator's machines. The kill-list's "no mesh-level object" names the roster as its one exception: a signed object, never a transitive relay of pairwise trust. A pairing names its mesh. The ceremony carries the signed age binding and gains the local-network guard. `aoide mesh join <mesh> <operator-node>` rides the ceremony. The HTTPS adapter serves no pairing method. The "Mesh declaration" section describes a pair mesh, and `sameOperator` leaves it. |
| `storage/src/mail.rs` doc comments (P-SEAL) | "Sealed" there (the module header, `mint_outbound_letter`, the two filing seams and one test message) means origin-signed. They say "signed" once sealing means age encryption. |

## Phases and acceptance tests

The beta path runs in this order. MAIL.md §Phases holds each mail-side slice's scope
and its mail-side tests; the tests below are this design's acceptance evidence for
the same slices. They are required, not suggestions.

1. **P-M3: hold and poll** (in progress). MAIL.md.
2. **P-SEAL: sealing and key bindings**, on the SSH direct lane, the only transport
   the slice has.
3. **P-ROSTER: the roster and trust per mesh.**
4. **H1: the mail-only HTTPS adapter on the relay.** The loopback HTTPS adapter on
   the relay, fronted by a tunnel, a VPS or a tailnet; `https://` and `poll`
   addresses; signed requests on every call; deposit, poll and receipts for letters
   already sealed at P-SEAL; hold-flavored spooling for `poll` nodes. No node accepts
   inbound connections except the relay's loopback adapter. Until P-M4, a `poll` node
   exchanges letters with the relay node itself only.
5. **P-M4: transit with the `next` hop chain.** MAIL.md.
6. **P-BOARD: boards.** MAIL.md.

After the beta path, each only after its own review:

7. **H2: state snapshots and WSS events**, after the reviewed end-to-end design.
8. **H3: direct HTTPS edges (optional).** For a node that legitimately exposes a URL,
   such as a VPS, with optional pins. It is never a routable Aoide door on a home
   node.
9. **H4: typed control actions**, using existing policy and audit only, and only after
   the same reviewed end-to-end design.

**Native Windows (register §28) must run a `poll` node.** H1's acceptance includes a
native Windows `poll` node with no SSH account or binary, and no WSL. Whatever the
CORE-POSIX.md matrix marks Unix-only on that node's path is a prerequisite of H1,
not a follow-up. That path is: identity and age keys at rest, the mailbase, outbox
and stage lock, signed HTTPS requests, roster verification, and the deposit, poll
and ack loop. Where aoided does not run, the poll is driven by hand or by the OS
scheduler (`aoide mail poll`).

**P-SEAL tests:**

- **Ciphertext on the wire:** a capture of the SSH direct lane and the sender's
  outbox contain no letter bytes, subject, body or mailbox names for a destination
  that holds a binding.
- **Per-peer upgrade:** a destination with no binding receives plaintext over the
  direct SSH lane only; once its binding is learned, it never receives plaintext
  again.
- **No unnecessary open:** an unverified origin is refused, and a previously admitted
  duplicate returns `duplicate`. Neither case opens anything.
- **Context binding:** a container re-signed under another key and `ctx` is refused at
  the destination (`context-mismatch`). A mismatch between the inner and outer `ctx`
  in any field refuses before any inner field is used.
- **Addressing mismatch:** a legitimately trusted node whose inner header is addressed
  elsewhere is refused and never filed.
- **Wrong recipient:** a container for B opened with C's age key fails.
- **Tamper:** a flipped byte in `ct`, an outer `ctx` field, `to.age` or `msgid` is
  refused and audited.
- **Replay and retry:**
  - A resent capture, and a retry after a lost response, yield `duplicate` with no
    second filing.
  - A crash between base and seen re-accepts exactly once.
  - A same-`msgid` collision with different immutable bytes, within the same origin
    and destination, is refused.
  - A filed duplicate recovers a lost receipt without duplicating a receipt that is
    already pending.
- **Bindings:**
  - A binding signed by any key but the one it names is refused.
  - A new age binding is accepted without re-pairing.
  - A pairing carries both sides' bindings.
- **Revocation and rotation:**
  - A replayed superseded binding is refused for current use.
  - A binding whose generation is below the high-water mark cannot become current.
    Mail already sealed to it while it was current still opens within the grace
    window.
  - A known revocation rejects immediately.
  - An expired or unknown binding always parks and is reported.
  - `key-retired` reaches the origin's retry loop.
- **Crash and recovery:** a torn tail is truncated, a resend is byte-identical, and no
  receipt is duplicated. Already-filed letters survive loss of the age key.
- **Downgrade:** mixed suite components or an older accepted-suite entry is refused
  with a taught error and an audit line.

**P-ROSTER tests:**

- **Signature and freshness:**
  - A roster signed by a key the node does not trust for that mesh is refused
    (`unknown-operator`).
  - An older or equal version is refused (`stale-roster`).
  - A flipped byte in a roster carried through a relay is refused.
  - A config operator line and a state record that disagree refuse every roster for
    that mesh (`operator-mismatch`).
  - A version that re-keys an existing node is applied and reported by `aoide mesh`.
- **Setup:** the node line `aoide onboard` prints, pasted and signed, admits that
  node at every other node with `message` only, and nothing is paired.
- **Bootstrap:** a machine that trusts only the operator key accepts its first roster
  by `mesh roster accept` and by LAN join, and a later version by letter from an
  origin it learns from that roster.
- **Revocation:** a removed line refuses the node on the first request after the new
  version is received, and routing stops using it.
- **Trust per mesh:**
  - A node granted `spawn` in mesh A is refused a spawn in a request naming mesh B.
  - Local `node allow … off --mesh` narrows a roster grant; nothing local widens one.
  - A paired record in a roster mesh for a listed key is inert and reported.
- **Migration:** every existing paired record lands in the home mesh with its grant
  byte-identical; trusted peers keep working; a request naming no mesh matches only
  a migrated home-mesh record.
- **Re-root:** after the new key is trusted, rosters under the old key are refused and
  the high-water mark restarts; a node that has not been re-rooted keeps working on
  its last roster.
- **LAN only:** a LAN join or pairing attempt through a relay or the HTTPS adapter is
  refused.
- **`sameOperator`:** a mesh still declaring it loads with a note and acts on
  nothing.

**H1 tests:**

- **Ciphertext at the relay:** a wire capture of the HTTPS hop, and the relay's spool,
  log and audit, contain no letter bytes, subject, body or mailbox names.
- **Outbound only:** during a full send, poll and ack round trip, no home node holds a
  listening socket on a routable address.
- **Signed requests everywhere:** through the tunnel, an unsigned request, an
  unknown-key request, a request naming a mesh the caller is not in, and a replayed
  nonce are all refused. The observed loopback source grants nothing.
- **Receipts:** one per acked `msgid` while pending. A later redelivery after delivery
  may re-spool one. No receipt chain.
- **Windows and Linux, no SSH:** send, poll, fetch and ack work end to end with the
  relay node, with a `poll` node on each OS that has no SSH account or binary.
- **Downgrade:** a plaintext request is refused with a taught error and an audit
  line.
- **Invalid credential, a bad pin where one is declared, and an unauthorized inbox**
  are rejected, as in the plaintext profile's negative tests.

**P-M4 tests (relay side; the hop-chain and zone tests are MAIL.md P-M4's):**

- **Ciphertext transit:** the hub's transit entry, spool, log and audit hold the
  sealed container and routing metadata only.
- **Tamper through a relay:** a flipped byte in `mesh` or any hop signature, from an
  authorized or an unauthorized relay, is refused and audited.
- **Context binding at a relay:** a relay that re-signs another origin's `ct` under its
  own key and `ctx` is refused at the destination (`context-mismatch`).
- **Retry over another route:** the same immutable container over a different route
  dedups rather than colliding (the comparison excludes `mesh` and `transit`).
- **No preclaim:** a different origin cannot preclaim a `msgid` at a relay.
- **No forged receipt:** a hub cannot make an origin stop retrying.

**P-BOARD tests** are MAIL.md P-BOARD's, plus: a relay holding a board log holds
ciphertext and membership records only, and its audit names no mailbox. A takeover
record relayed by a hub verifies only under the operator key, and a hub cannot alter
its `seq` or new owner.

## Open design decisions

- **Forward secrecy:** age does not provide it, for letters or board epochs. Adopt MLS
  or a ratchet before P-SEAL if the User requires FS/PCS.
- **Padding policy** for ciphertext length.
- **Roster expiry:** whether a roster carries a validity window, so that a relay
  dropping a revocation delays it by a bounded time rather than indefinitely.
- **A successor key committed in the roster**, so a compromised operator key can be
  replaced without touching every machine.
- **Cross-mesh origin verification through a gate:** a destination in mesh B must hold
  the origin's key from mesh A, and mesh B's trust does not carry it. Where that key
  comes from is open; a gate never supplies it.
- **Read visibility per mesh:** what `read` returns (sessions, frames, the graph
  summary) is not yet partitioned by mesh. A node with `read` in any mesh sees the
  whole node.
- **Relay operator trust:** the shape of the Cloudflare Access policy (service token
  versus mTLS, per mesh or per node).
- **Relay failover:** whether a sender moves to the next declared relay when the
  first is unreachable but not `down`. MAIL.md's router picks the first eligible
  relay only.
- **Observation scopes, cursor and retention limits** for `state` and `events`.

## References

- [Mail](MAIL.md): envelope bytes, store, outbox, transit, boards, security model.
  [Pairing](PAIRING.md): identity, signed wire authentication, the ceremony,
  transport. [Mail-only external access](MAIL-ONLY-ACCESS.md).
  [Daemon boundary](AOIDED.md). [Core portability](CORE-POSIX.md): Linux and native
  Windows.
- [age v1 (C2SP)](https://github.com/C2SP/C2SP/blob/main/age.md): X25519 recipients,
  ChaCha20-Poly1305 STREAM payload, header MAC, no sender authentication, no
  associated data.
- [agenix](https://github.com/ryantm/agenix): the recipient model rosters and board
  epochs follow — public keys listed in one file, a secret sealed to each.
- [RFC 9180 (HPKE)](https://www.rfc-editor.org/rfc/rfc9180.txt): the alternative that
  was considered. §9.1.1 signs `(enc, ct)` for KCI resistance. §9.7.4 notes no forward
  secrecy against recipient compromise, which holds equally for static age recipients.
- [RFC 9420 (MLS)](https://www.rfc-editor.org/rfc/rfc9420.txt): asynchronous group
  keying with FS and PCS, an untrusted Delivery Service, epochs and Remove.
- [Double Ratchet](https://signal.org/docs/specifications/doubleratchet/): FS/PCS for
  pairwise sessions. §8.2 names what a key compromise still defeats.
- [RFC 8446 (TLS 1.3)](https://www.rfc-editor.org/info/rfc8446/): transport
  confidentiality and authentication, not a substitute for payload protection.
