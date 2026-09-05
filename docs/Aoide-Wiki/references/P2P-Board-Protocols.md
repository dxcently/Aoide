# P2P Board Protocols — how four store-and-forward systems answer the seven questions

Reference research for the aoide inbox/board design session: a 4-node LAN mesh
with a declared topology in a config file, mutual cryptographic pairing, and
ssh hops between boxes, choosing semantics for identity/addressing, dedup,
propagation, loop prevention, read-state ownership, retention, and
offline-peer handling.

Four systems, primary sources only, fetched 2026-09-04 and quoted from the
spec text itself: FidoNet (FTSC standards at ftsc.org), Usenet/NNTP (RFC
3977/4644/5536/5537 at rfc-editor.org), Secure Scuttlebutt (the SSBC protocol
guide), and NNCP (the nncpgo.org manual). FidoNet's FTS documents are
unnumbered prose in places; those are cited by the document's own section
headings.

The pattern the whole survey reduces to: **all four converge on the same
skeleton — a stable per-node identity, a per-link outbound queue on disk,
flooding along declared links, receiver-side dedup on a globally unique
message identity, and read-state kept off the wire entirely.** They differ
only in where the identity comes from (assigned number vs keypair), what the
dedup key is (sender-stamped ID vs content hash vs author+sequence), and
whether loop prevention is done by the sender (don't send where it's been) or
the receiver (drop what I've seen).

---

## 1. FidoNet — echomail (flooded conferences) + netmail (routed mail)

Sources:
- FTS-0001.016, "A Basic FidoNet Technical Standard" — <http://ftsc.org/docs/fts-0001.016>
- FTS-0004.001, "EchoMail Specification" — <http://ftsc.org/docs/fts-0004.001>
- FTS-0009.001, "MSGID / REPLY — A standard for unique message identifiers" — <http://ftsc.org/docs/fts-0009.001>
- FTS-5000.005, "The Distribution Nodelist" — <http://ftsc.org/docs/fts-5000.005>
- FTS-5001.006, "Nodelist flags and userflags" — <http://ftsc.org/docs/fts-5001.006>
- FTS-5005.003, "Advanced BinkleyTerm Style Outbound" (BSO) — <http://ftsc.org/docs/fts-5005.003>

### Identity / addressing

- Addresses are hierarchical numbers, not keys: `zone:net/node[.point]`. The
  stored-message header carries `origNode/destNode/origNet/destNet` with
  optional `origZone/destZone/origPoint/destPoint` fields (FTS-0001 §A.1
  "Application Layer Data Definition : a Stored Message"). Zone and point
  addressing ride in `^A` control lines when the binary fields are absent:
  "TOPT <pt no> — destination point address, FMPT <pt no> — origin point
  address, INTL <dest z:n/n> <orig z:n/n> — used for inter-zone address"
  (FTS-0001 §A.1, "Current ^A keyword assignments").
- The address is *assigned* by the network hierarchy the nodelist declares
  (zone → region → net/host → hub → node → point), not self-generated
  (FTS-5000 §5.3, Field 1/Field 2). No cryptography anywhere in the address.

### Dedup

- Two independent mechanisms. First, sender-side suppression: the SEEN-BY
  trailer lists `net/node` pairs and "the net/node numbers correspond to the
  net/node numbers of the systems having already received the message. In
  this way a message is never sent to a system twice" (FTS-0004, "Conference
  Mail Message Control Information", item 4 "Seen-by Lines"). SEEN-BY is
  2D — no zone component — and is REQUIRED.
- Second, receiver-side identity: `^AMSGID: origaddr serialno`, "a serial
  number unique to that message on that system … no two messages from a given
  system may have the same serial number within three years" (FTS-0009,
  "MSGID"). MSGID exists because SEEN-BY provably does not prevent all
  duplicates (below); tossers keep a database of recent MSGIDs and drop
  matches. FTS-0004 itself predates MSGID and offers no receiver-side dedup
  beyond SEEN-BY.

### Propagation

- Echomail is flooding along per-conference declared links: "2. This message
  is 'Exported' … to each system 'linked' to the conference … 3. Each of the
  receiving systems 'Import' the message … 4. The receiving systems then
  'Export' these messages … to each of their conference links. 5. Return to
  step 3." (FTS-0004, "How It Works"). The conference is named by the first
  line `AREA:CONFERENCE` (FTS-0004, "Conference Mail Message Control
  Information", item 1).
- Netmail is point-to-point with optional host routing: "FidoNet does not
  necessarily send a message directly to its destination. To reduce the
  number of network connections, mail to a subset of the nodelist may be
  routed to one node for further distribution" — file attaches must go
  direct; plain messages "should be routed through the inbound host of the
  destination node's subnet as specified by a FidoNet format nodelist"
  (FTS-0001 §E.2 "Transport Layer Protocol : Routing"). Store-and-forward
  transit state is carried in message attribute bits: `InTransit`, `Sent`,
  `KillSent`, `HoldForPickup`, `Crash` (FTS-0001 §A.1, AttributeWord).

### Loop prevention

- SEEN-BY is the mechanism, and it is *incomplete by the spec's own
  admission*: "Duplicates will occur in any topology that forms a closed
  polygon that is not fully connected", with a worked A-B-C-D square where D
  receives the message twice from B and C (FTS-0004, "Conference Topology").
  Conversely the spec lists "fully connected polygons with a few vertices"
  (triangle, "fully connected square") as safe topologies.
- The `^APATH:` line records only "those systems having actually processed
  the message" (vs SEEN-BY's "all systems the message has been sent to") and
  is explicitly diagnostic, not preventive — "the path line can be used to
  quickly locate the source of duplicate messages" (FTS-0004, "PATH Lines"
  and "Why a PATH line?").

### Read-state ownership

- Every node imports messages into its own message base; read-state lives
  there as store-side flags on the local copy: the stored-message header has
  a `timesRead` field and a `Recd` (received/read) attribute bit (FTS-0001
  §A.1). Nothing about read-state ever crosses a link; the "store" holding
  the flag is the reader's own machine.

### Retention

- Not specified by any FTS document consulted. Message-base purging is a
  local tosser/BBS policy; the specs define only transport and format. The
  closest spec-level statement is FTS-0009's dedup horizon: serial numbers
  must not repeat "within three years" — an identity-uniqueness window, not a
  storage mandate.

### Offline handling

- The per-link outbound queue is a directory of files named after the remote
  address: "The name of flow file is formed from network and node number of
  the remote system, expressed as 4 hexadecimal digits each … information
  concerning node 104/36 is stored in flow and control files with name
  '00680024'", with other zones in `outbound.zzz` directories and points in
  `<nff>.pnt` subdirectories (FTS-5005 §2 "Definitions"). Mail for an
  unreachable node is simply files that stay in that directory.
- *When* the queue may be flushed is the flow-file flavour: Immediate
  (`iut/ilo`, "must try to poll a remote system without taking in
  consideration any external and internal restrictions"), Continuous
  (`cut/clo`, ignores external restrictions, "assuming that the remote
  system carries a CM flag"), Direct (`dut/dlo`), Normal (`out/flo`, "may be
  rerouted by specific programs"), and Hold (`hut/hlo`) — "a flow file with
  such flavour instructs the mailer to wait for a poll from the remote
  system" (FTS-5005 §3.2 "Flavours of flow file"). Interrupted sessions
  restart the flow file "from the beginning"; files are deleted only after
  successful transmission (FTS-5005 §3.1).
- Availability is *declared*, in the nodelist: `CM` = "Node accepts mail 24
  hours a day" (FTS-5001 §5.1); a non-CM node is only guaranteed reachable
  during Zone Mail Hour — "the ability to send to and receive from any
  FidoNet listed node during the Zone Mail Hour … is considered mandatory"
  (FTS-0001 §A.2). Nodelist keywords declare outage states: `Hold` — "Mail
  may be sent to such nodes, routed via its Hub, Host or Coordinator, or held
  locally by the originating node until the Hold keyword is removed"; `Down`
  — "Mail may NOT be sent or routed to it" (FTS-5000 §5.3, Field 1).
- Local per-link call throttling uses control files beside the queue: `.bsy`
  (link busy/locked, required), `.csy` (call in progress), `.hld` (contains
  "the expiration of the hold period expressed in UNIX-time" — do not call
  until then), `.try` (last-attempt record) (FTS-5005 §5.1-5.4).

### The nodelist as declared topology

- "For direct communication, it is necessary that the originating node knows
  in advance the exact method and capabilities of the destination node …
  For routing and management, the stated hierarchy must also be known. All
  this information is collected into a file called the 'nodelist' … In
  essence, the nodelist defines Fidonet." (FTS-5000 §1 "Introduction").
  Entries are 7+ comma-separated fields: keyword, number, name, location,
  sysop, phone, baud, then capability flags (FTS-5000 §5.3). It is published
  as a full file plus weekly diffs (§4, §6).

---

## 2. Usenet / NNTP — flood-fill among peers, local stores, thin readers

Sources:
- RFC 3977, "Network News Transfer Protocol (NNTP)" — <https://www.rfc-editor.org/rfc/rfc3977>
- RFC 4644, "NNTP Extension for Streaming Feeds" — <https://www.rfc-editor.org/rfc/rfc4644>
- RFC 5536, "Netnews Article Format" — <https://www.rfc-editor.org/rfc/rfc5536>
- RFC 5537, "Netnews Architecture and Protocols" — <https://www.rfc-editor.org/rfc/rfc5537>

### Identity / addressing

- The article's global identity is its Message-ID, stamped once at injection:
  "Each article MUST have a unique message-id; two articles offered by an
  NNTP server MUST NOT have the same message-id" (RFC 3977 §3.6); RFC 5536
  §3.1.3 tightens the syntax (`<id-left@id-right>`, max 250 octets) and makes
  uniqueness global "across all protocols using such message identifiers".
  The ID is opaque — chosen by the injecting agent, not derived from content,
  not cryptographic.
- Server identity in transit is the `<path-identity>`, preferably the FQDN,
  else "some other (arbitrary) name … believed to be unique and registered at
  least with all the other news servers to which that relaying agent …
  sends articles" (RFC 5537 §3.2).
- Placement addressing is the Newsgroups header (which boards), not a node
  address; articles are keyed three ways on a server: globally unique
  message-id; group + article number, which "is not required to be globally
  unique, so the same key MAY refer to different articles on different
  servers"; and arrival timestamp (RFC 3977 §6).

### Dedup

- Mandatory receiver-side history: "Relaying and serving agents therefore
  MUST keep a record of articles they have already seen and use that record
  to reject additional offers of the same article. This record is called
  the 'history' file or database." (RFC 5537 §3.3). A relaying agent "MUST
  reject any article that has already been accepted" (RFC 5537 §3.6, duty 3).
- To bound the history db: agents "MAY select a cutoff interval and reject
  any article with a date farther in the past than that cutoff … for Usenet
  … no less than seven days is conventional", after which history records
  older than the cutoff may be dropped, because the date-based reject now
  covers them (RFC 5537 §3.3). Dropping history by first-seen date instead
  requires retention "at least 24 hours longer than the cutoff interval to
  allow for articles dated in the future" (RFC 5537 §3.3).
- The wire protocol dedups *before transfer* by offering the message-id:
  IHAVE — 335 send it / 435 "Article not wanted" ("if, for example, the
  server already has a copy of it") / 436 try again later; after transfer
  235 OK / 436 retry later / 437 "Transfer rejected; do not retry"
  (RFC 3977 §6.3.2). A server "MAY acknowledge the successful transfer of
  the article (with a 235 response) but later silently discard it"
  (RFC 3977 §6.3.2.2).

### Propagation

- "Netnews normally uses a flood-fill algorithm for propagation of articles
  in which each news server offers the articles it accepts to multiple
  peers, and each news server may be offered the same article from multiple
  other news servers." (RFC 5537 §3.3). Peering links and which groups flow
  over them are bilateral configuration: an article "SHOULD NOT be relayed
  unless the sending agent has been configured to supply, and the receiving
  agent to receive, at least one of the <newsgroup-name>s" (RFC 5537 §3.6).
- IHAVE is one offer per round-trip and "MUST NOT be pipelined" (RFC 3977
  §6.3.2.1). The STREAMING extension splits offer and transfer so both
  pipeline: CHECK message-id → 238 send it / 431 try later / 438 not wanted;
  TAKETHIS message-id + article → 239 OK / 439 rejected. Clients "MAY use an
  adaptive strategy where … CHECK commands for all articles, but switches to
  using TAKETHIS without CHECK if most articles are being accepted (over 95%
  acceptance might be a reasonable metric)" (RFC 4644 §2.1, §2.4, §2.5).

### Loop prevention

- Path header, built right-to-left: every injecting/relaying/serving agent
  "MUST identify themselves, when processing an article, by prepending their
  <path-identity> to the Path header field" (RFC 5537 §3.2), with
  path-diagnostics (`!.POSTED`, `!.SEEN.x`, `!.MISMATCH.x`, `!!` verified)
  recording how each hop verified its upstream (RFC 5537 §3.2.1).
- The loop check is advisory and sender-side: "In order to avoid unnecessary
  relaying attempts, an article SHOULD NOT be relayed if the <path-identity>
  of the receiving agent (or some known alias thereof) appears as a
  <path-identity> … in its Path header field" (RFC 5537 §3.6). The MUST-level
  backstop is the history reject — loops die at the receiver even when the
  Path check fails.

### Read-state ownership

- Purely reader-side; the protocol defines no per-user read flag anywhere.
  The primitives a reading client builds its cursor on are server-local,
  per-group article numbers issued "in order of arrival timestamp"
  (RFC 3977 §6), the GROUP/LISTGROUP number ranges (§6.1), and NEWNEWS
  "wildmat date time" returning what arrived since a date (§7.4). The
  classic `.newsrc` read-ranges file is client convention, not part of any
  of these RFCs. Because group+number keys differ per server, a reader's
  cursor is only meaningful against the one server it tracks (§6).

### Retention

- Local policy, full stop. The poster may *request* expiry: "The Expires
  header field specifies a date and time when the poster deems the article to
  be no longer relevant and could usefully be removed ('expired')"
  (RFC 5536 §3.2.5) — and RFC 5537 imposes no duty on any agent to honor or
  even read it; a serving agent's final duty is simply "it stores the
  article and makes it available for reading agents" (RFC 5537 §3.7). The
  only spec-mandated time window is the dedup-history cutoff (§3.3 above).

### Offline handling

- The RFCs define only in-session behaviour: 436/431 mean "try again later"
  (RFC 3977 §6.3.2; RFC 4644 §2.4), and "a lack of response … SHOULD be
  treated the same as a 436 response" (RFC 3977 §6.3.2.2). Per-peer outbound
  queueing/backlog while a peer is down is implementation territory (news
  server spool files), not specified in RFC 3977/4644/5537.

---

## 3. Secure Scuttlebutt — signed per-author append-only logs, gossiped

Source: "Scuttlebutt Protocol Guide", Scuttlebutt Consortium —
<https://ssbc.github.io/scuttlebutt-protocol-guide/> (single page; cited by
its section headings).

### Identity / addressing

- "An identity is an Ed25519 key pair … Because identities are long and
  random, no coordination or permission is required to create a new one,
  which is essential to the network's design." Displayed as
  `@base64(pubkey).ed25519`. "Each identity has an associated feed, which is
  a list of all the messages posted by that identity. This is why the
  identity is also called a feed ID." ("Keys and identities"). Key loss is
  terminal: "If a user loses their secret key … they will need to generate a
  new identity and tell people to use their new one instead."
- There is no location in the address; sigils type the references: `@` keys,
  `%` messages, `&` blobs ("Keys and identities"). Addresses (IP/port) travel
  separately — LAN UDP broadcast every second, invite codes, or `type:pub`
  messages advertising a pub's host/port/key ("Discovery").

### Dedup

- Structural, not a separate mechanism. Every message carries `author`,
  `sequence` ("1 for the first message in a feed, 2 for the second and so
  on"), and `previous` ("Message ID of the latest message posted in the
  feed"), and the message ID is "a hash of the message including signature"
  (sha256, self-describing) ("Feeds — Message format", "Message ID"). A
  replica either already has (author, seq) or it does not; there is nothing
  to dedup because a peer only requests messages *after* its highest held
  sequence ("createHistoryStream": "sequence — Only return messages later
  than this sequence number").

### Propagation

- Gossip of whole feeds between peers who care about them: "Scuttlebutt
  clients maintain a set of feeds that they are interested in … When peers
  connect, one of the first things they do is ask each other whether the
  feeds they are interested in have any new messages" — the
  `createHistoryStream` RPC per feed, with `live` streaming as new messages
  arrive ("createHistoryStream").
- EBT replication replaces per-feed streams with an exchanged vector clock:
  "The key of each pair specifies a Scuttlebutt feed … The value … is a
  signed integer encoding a replicate flag, a receive flag and a feed
  sequence number", -1 meaning do-not-replicate; then "both peers may begin
  sending messages at will", sending updated clocks any time ("EBT
  replication — Vector Clocks"). "Request skipping" stores the remote's last
  clock and omits unchanged feeds from the next session's clock ("EBT
  replication"). Fallback to createHistoryStream when a peer lacks EBT.
- What gets replicated is set by the follow graph: a follow is itself a
  message (`type: contact, following: true`) in the follower's own feed
  ("Following"); "One implementation, Patchwork, shows messages up to 2 hops
  out by default. Messages from feeds 3 hops out are replicated to help keep
  them available for others" ("Follow graph"). Topology is therefore data in
  the logs, not a config file.

### Loop prevention

- None needed and none present. Messages don't flood as independent objects;
  a peer requests feed suffixes by (author, sequence). Signed append-only
  chains ("once a message is posted it cannot be modified", each message
  referencing `previous`) make replay/re-receipt idempotent — receiving
  message (author, seq=n) twice is the same message, verifiable by signature
  and hash chain ("Feeds — Structure", "Signature").

### Read-state ownership

- The protocol guide defines no read-state at all — no flag, no cursor, no
  RPC touches it. What it does define is a *replication* cursor per (peer,
  feed): the EBT vector-clock sequence ("The decoded sequence number defines
  the latest message held by the peer for that feed") and the
  createHistoryStream `sequence` argument. Everything above that — what a
  human has read — is application state on the reader's own replica; every
  client works against its local copy of the logs.

### Retention

- The guide's model is keep-everything: feeds are append-only, immutable,
  hash-chained back to sequence 1, and verification depends on that chain
  ("Feeds — Structure"). No expiry or deletion mechanism appears in the
  guide's replication sections; blobs (attachments, off-log content-addressed
  objects fetched via `blobs.get`) are the only separable storage ("Blobs").
  Message size is capped (8KiB), pushing bulk data to blobs.

### Offline handling

- Offline-first by construction: posting appends to the local log with no
  network involved; any later contact with any peer that wants the feed
  syncs it ("createHistoryStream", "EBT replication"). Availability for
  peers that are rarely online simultaneously is solved socially by pubs:
  "normally run on servers so that they are always online", following and
  followed back, "Pubs speak the same protocol as regular peers" ("Pubs").
  Third-party delivery is inherent: any replica within hops range carries
  your feed to peers you never meet ("Follow graph").

---

## 4. NNCP — per-neighbour encrypted spools over arbitrary transports

Source: "NNCP (Node to Node copy)" manual, Sergey Matveev — <http://www.nncpgo.org/>
(texinfo manual; cited by page name). NNCP names its own lineage: multicast
areas "can also be called echomail (like in FidoNet networks) or newsgroup
(like in Usenet networks)" (Multicast page).

### Identity / addressing

- A node is its keys. The config declares `self` (id + exchange, signing, and
  noise keypairs) and a `neigh` map of named neighbours, each with `id`,
  `exchpub`, `signpub`, optional `noisepub` for online calls (Configuration
  page). The node id is a 32-byte value, Base32-encoded on disk and in
  config; every encrypted packet header carries "Sender … node's id,
  Recipient … node's id" plus an "ed25519 signature for that packet's header"
  (Encrypted packet page).
- Reachability is per-neighbour config: `addrs` (named network addresses) and
  `via: ["foo","bar"]` — "an array of node identifiers that will be used as a
  relay to that node … packet can reach current node by transitioning through
  foo and then bar" (Configuration neighbour options page). Relaying is
  onion-style: a `trns` plain packet whose payload is the "whole encrypted
  packet we need to relay on" (Plain packet page).
- Multicast areas get their own identity: "Area consists of private/public
  Curve25519 keypairs for packets encryption, identity (BLAKE2b-256 hash of
  the public key) and possible subscribers" (Multicast page).

### Dedup

- Content-hash, at two layers. Spool packets are named by their hash: "Its
  filename is Base32 encoded Merkle Tree Hashing hash of the whole contents.
  It can be integrity checked anytime" (Spool directory page); the sync
  protocol advertises files as INFO (niceness, size, "Hash … Unique file
  identifier") (Synchronization protocol page).
- Area messages have a transfer-invariant id: "Area's message identity
  (MsgHash) is the hash of the encrypted packet header. Because the area
  packet … is relayed as-is without any modifications, that area message's
  hash will be the same on each node it reaches" (Multicast page). Dedup
  state is per-(node, area) `seen` files: "check if that message was already
  seen … check existence of SPOOL/NODE/area/AREA/MsgHash file. Skip that
  node if it exists"; one's own copy is guarded by `SPOOL/SELF/area/AREA/
  MsgHash` — a second arrival "is just an ordinary possible duplicate",
  removed (Multicast page). For unicast packets deliverable by multiple
  transports, `nncp-toss -seen` keeps `seen/<hash>` files "telling that the
  file with specified hash has already been processed before … useful when
  there are use-cases where multiple ways of packets transfer available and
  there is possibility of duplicates reception" (Spool directory page).

### Propagation

- Unicast: sender spools an encrypted packet into `SPOOL/<neighbour-id>/tx/`;
  transports move spool contents — `nncp-daemon`/`nncp-call[er]` over TCP,
  `nncp-xfer` over removable media, `nncp-bundle` over broadcast/sequential
  media; `nncp-toss` decrypts inbound queues and "relay[s] transition packets
  to other nodes" (Workflow page).
- Multicast: flood along declared per-area subscriber lists. The area config
  `subs: ["alice","bob"]` "contains a list of recipients you must relay
  incoming multicast packet on" (Configuration areas options page). The
  tosser re-wraps the same inner area packet for each subscriber except the
  one it came from, records the MsgHash, and the initial send is addressed
  to *self* so the fan-out is crash-restartable from the spool (Multicast
  page, tossing algorithm and its worked example — which is literally a
  4-node mesh: `A—B—C` with `D` connected to A and B, showing D discarding
  the second copy by MsgHash and A silently dropping its own message when
  echoed back).

### Loop prevention

- Receiver-side seen-state, plus one hop of sender-side suppression: "if
  subscriber's node is not the one we received the packet from, then create
  outgoing encrypted packet to it" and per-destination MsgHash files stop
  re-sends; a copy arriving back at a node that has the MsgHash is removed
  (Multicast page). No path record exists in the packet — loops are cut
  purely by the seen-files.

### Read-state ownership

- Not a concept in NNCP; delivery is the terminal event. Tossing hands the
  payload to the configured sink (`incoming` directory for files, `exec`
  handlers such as sendmail for messages) and the spool copy is removed
  (Workflow, Configuration areas options pages). What is offered instead is
  *delivery* state: explicit end-to-end ACK — "possibility to explicitly
  acknowledge the receipt of the encrypted packet, by generating the reply
  of ACK-type which contains the packet identifier"; the sender keeps
  outbound packets (`-keep`) and "each ACK packet will remove kept
  corresponding outbound packets, because Bob explicitly confirmed their
  receipt" (ACKnowledgements page).

### Retention

- Manual/local. Spool entries live until transferred and tossed; `seen` and
  area MsgHash files accumulate — "You have to manually remove them, when
  you do not need them (probably because they are expired)" (Spool directory
  page) — and `nncp-rm` is the policy tool, with `-older X` applying an age
  limit to `-seen`, `-area`, `-rx/-tx`, `-part`, `-nock` classes (nncp-rm
  page). Note the standing trade: deleting a seen-file re-opens the dedup
  window for that hash.

### Offline handling

- The spool *is* the offline story: "Spool directory holds encrypted packets
  received from remote nodes and queued for sending to them", one directory
  per neighbour id with `rx/` and `tx/` (Spool directory page). Nothing
  expires on its own; an unreachable neighbour's `tx/` just grows until a
  transport succeeds — including sneakernet (`nncp-xfer`) when no link ever
  comes up (Workflow page).
- Online calls are scheduled per neighbour: `calls` entries with cron
  expressions, `onlinedeadline` (hold the connection idle this long),
  `maxonlinetime`, `when-tx-exists` (call only if something is queued),
  rate limits, and `autotoss` during the session (Call configuration page).
- Transfers resume: the sync protocol runs over Noise_IK_25519_ChaChaPoly_
  BLAKE2b, advertises INFO per spooled file, and requests FREQ with "Offset
  from which remote side must transmit the file"; partly received files sit
  as `.part` (Synchronization protocol, Spool directory pages). Priority is
  per-packet niceness 1-255 — "higher priority packets, like mail messages,
  will pass first, even when lower priority packet was already been
  partially downloaded" (Niceness page).

---

## Comparison table

| Axis | FidoNet | Usenet / NNTP | Secure Scuttlebutt | NNCP |
|---|---|---|---|---|
| **Identity / addressing** | Assigned hierarchical number `zone:net/node.point` (FTS-0001 §A.1); hierarchy declared in nodelist (FTS-5000 §5.3). No crypto. | Article: opaque globally-unique Message-ID stamped at injection (RFC 3977 §3.6; RFC 5536 §3.1.3). Server: `<path-identity>` FQDN-ish name (RFC 5537 §3.2). Boards via Newsgroups header. | Ed25519 keypair IS the identity and the feed address (`@…ed25519`); self-generated, no coordination (guide, "Keys and identities"). Msg ID = sha256 of signed message. | 32-byte node id + per-node keypairs declared in config `neigh` (Configuration); packets carry sender/recipient ids, ed25519-signed header (Encrypted packet). Areas = keypair + BLAKE2b id (Multicast). |
| **Dedup** | Sender-side SEEN-BY (net/node list, "never sent to a system twice", FTS-0004 "Seen-by Lines") + receiver-side `^AMSGID` origaddr+serial unique ~3yr (FTS-0009). | Receiver MUST keep history db, MUST reject already-accepted (RFC 5537 §3.3, §3.6); wire-level pre-offer by message-id: IHAVE 435 / CHECK 438 "not wanted" (RFC 3977 §6.3.2; RFC 4644 §2.4). | Structural: (author, sequence) chain; peers request only seq > held (guide, "createHistoryStream"); nothing arrives twice to dedup. | Content hash: spool files named by MTH hash (Spool); area MsgHash invariant across hops, per-node `seen` files (Multicast); `seen/` files for multi-transport unicast (Spool). |
| **Propagation** | Echomail: flood along per-conference declared links, import→re-export (FTS-0004 "How It Works"). Netmail: point-to-point, optionally host-routed per nodelist (FTS-0001 §E.2). | Flood-fill: every server offers accepted articles to all configured peers (RFC 5537 §3.3, §3.6); offer/transfer via IHAVE or pipelined CHECK/TAKETHIS (RFC 4644 §2.1). | Gossip whole per-author logs between interested peers; EBT vector clocks, live streams; interest = follow graph, 2-3 hops (guide, "EBT", "Follow graph"). | Unicast spool-to-spool over any transport (TCP, sneakernet, broadcast) (Workflow); multicast flood along per-area `subs` lists, re-wrapped per subscriber (Multicast; CfgAreas). |
| **Loop prevention** | SEEN-BY suppresses known recipients; fails on non-fully-connected cycles by spec's own example; PATH is diagnostic only (FTS-0004 "Conference Topology", "Why a PATH line?"). MSGID drop is the backstop. | Sender SHOULD NOT relay to a peer already in Path (RFC 5537 §3.6); receiver history reject is the MUST backstop (§3.3, §3.6 duty 3). | Not needed: replication is by (author, seq) request, append-only signed chains make re-receipt idempotent (guide, "Feeds"). | Don't send back to the arrival link + per-(node, area) MsgHash seen-files; returning copies silently removed (Multicast). No path record. |
| **Read-state ownership** | Store-side flags on the *reader's own* message base: `timesRead`, `Recd` bit (FTS-0001 §A.1). Never crosses a link. | Reader-side cursor; protocol defines none. Primitives: per-server per-group arrival-ordered article numbers, NEWNEWS since-date (RFC 3977 §6, §7.4). `.newsrc` is client convention outside the RFCs. | Absent from protocol; only per-(peer, feed) *replication* sequence exists (EBT clock / createHistoryStream `sequence`). Read-state is app-local on the reader's replica. | Absent; delivery is terminal (toss → incoming/exec sink). Delivery-state exists instead: explicit ACK packets clear kept outbound copies (ACK page). |
| **Retention** | Unspecified by FTS specs; local message-base policy. MSGID uniqueness window ~3 years (FTS-0009). | Local policy. Expires header is a poster *request* only (RFC 5536 §3.2.5); no agent duty to honor it (RFC 5537 §3.7). History cutoff ≥7 days conventional (RFC 5537 §3.3). | Keep-everything: append-only immutable hash-chained feeds; no expiry in the guide; bulk data pushed to content-addressed blobs (guide, "Feeds", "Blobs"). | Manual: spool until delivered; seen/MsgHash files pruned by operator via `nncp-rm -older` (Spool; nncp-rm). Pruning seen-state re-opens the dup window. |
| **Offline handling** | Per-link on-disk outbound: hex-named flow files per remote (FTS-5005 §2); flavours Immediate/Continuous/Direct/Normal/Hold decide who initiates, Hold = wait to be polled (§3.2); nodelist declares availability (CM 24h, FTS-5001 §5.1; Hold/Down keywords, FTS-5000 §5.3; ZMH mandatory window, FTS-0001 §A.2). | In-session only: 436/431 try-again-later; silence = 436 (RFC 3977 §6.3.2). Inter-server queueing while a peer is down is implementation, not in the RFCs. | Offline-first: post locally, sync opportunistically with any interested peer; always-on pubs bridge disjoint online windows; 3-hop replication carries feeds for others (guide, "Pubs", "Follow graph"). | Per-neighbour spool waits indefinitely, any transport drains it incl. removable media (Spool; Workflow); cron-scheduled calls, `when-tx-exists`, `onlinedeadline` (Call); resumable offset-based transfer, niceness priority (Sync; Niceness). |

---

## What maps onto a 4-node declared-topology mesh

Factual observations only; the decisions belong to the design session.

- FidoNet's nodelist is a declared-topology artifact of the same kind as
  aoide's `[mesh.<name>]` declaration: a file, distributed out-of-band, that
  "defines" the network — membership, hierarchy, per-node capabilities and
  availability states (FTS-5000 §1, §5.3). NNCP's config is the same thing
  with keys in it: `neigh` + per-area `subs` are the entire topology, and a
  node cannot even process traffic for an area absent from its config
  ("You can not process multicast packets that has unknown area
  identification", CfgAreas page).
- The per-link outbound spool is the store-and-forward primitive for offline
  peers in both file-based systems, and both key it by the remote's
  identifier: BSO's `00680024.*` hex flow files per net/node (FTS-5005 §2)
  and NNCP's `SPOOL/<base32-node-id>/tx/` (Spool page). Mail for a dead box
  is just files that wait; no timer, no failure path.
- NNCP's multicast-area worked example *is* a 4-node mesh (A—B—C, D linked
  to A and B) and demonstrates the complete mechanism at that scale:
  per-subscriber relay lists, a transfer-invariant MsgHash, per-(node, area)
  seen-files, duplicate copies silently dropped, and a node relaying an area
  it cannot decrypt (Multicast page).
- fts-0004's dup-ring condition — "duplicates will occur in any topology
  that forms a closed polygon that is not fully connected" — vanishes in a
  fully connected 4-node graph, which the same spec lists among its safe
  topologies ("fully connected square", FTS-0004 "Conference Topology").
  Sender-side suppression alone is only sound under that topology
  constraint; every surveyed system that outlived its origin added a
  receiver-side identity check anyway (MSGID, history db, MsgHash).
- Two of the four make the keypair the address (SSB identity, NNCP node id
  + keys in `neigh`), which is the shape mutual pairing already produces:
  pairing = exchanging the entries a declared-neighbour map needs. The other
  two bolt trust on later or never (FidoNet phone numbers, NNTP
  path-identities with `.MISMATCH` diagnostics for unverified hops,
  RFC 5537 §3.2.1).
- NNCP's `via: ["hop1","hop2"]` per-neighbour relay arrays are declared
  static routes through mesh members, with the relayed payload opaque to the
  relay (`trns` wrapping, Plain packet page) — structurally the same shape
  as reaching a box only via an ssh hop through another box.
- Read-state never crosses the wire in any of the four systems. Where every
  node holds a full replica (FidoNet import bases, SSB logs, NNCP tossed
  delivery), "read" is a flag or app-state on the reader's own store; the
  reader-side *cursor* proper appears only in NNTP's thin-client reading,
  built on per-server arrival-ordered article numbers (RFC 3977 §6). The
  cursor-shaped thing the replicating systems do have is the replication
  cursor: SSB's per-(peer, feed) sequence in the EBT vector clock, NNCP's
  spool presence/absence.
- Dedup keys divide three ways: sender-stamped serial (MSGID, Message-ID —
  requires trusting the stamper's uniqueness), content/envelope hash (NNCP
  MsgHash, spool MTH names — self-verifying, transfer-invariant), and
  author+sequence position in a signed log (SSB — makes dedup a non-event
  but couples message identity to a total per-author order).
- Retention is local policy in all four; none propagates deletion. The one
  spec-level coupling is Usenet's: the dedup-history window and the
  date-based reject must cover each other, or pruned history re-admits old
  duplicates (RFC 5537 §3.3) — the same trade NNCP surfaces manually with
  `nncp-rm -older` on seen-files (nncp-rm, Spool pages).
- Delivery acknowledgement over store-and-forward exists only in NNCP among
  the four: keep the outbound copy until an explicit ACK packet carrying the
  packet id returns (ACK page). FidoNet's nearest analogues are per-message
  attribute bits (ReturnReceiptRequest, FTS-0001 §A.1); NNTP's 235/239 are
  hop-local, and RFC 3977 §6.3.2.2 explicitly allows acknowledge-then-drop.
- Who initiates contact is declared per link in FidoNet (BSO Hold flavour =
  "wait for a poll from the remote system", FTS-5005 §3.2; nodelist CM/ZMH)
  and per neighbour in NNCP (cron `calls`, `when-tx-exists`, or run
  `nncp-daemon` and be called) — the asymmetry a mesh has when only some
  boxes can open ssh connections to others is a first-class declared fact in
  both.
