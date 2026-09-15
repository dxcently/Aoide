# Aoide HTTPS mesh API

Status: architecture proposal, deferred. The user explicitly places this behind
current work. Do not start implementation, change listeners, issue credentials,
or replace transport as a side effect of this plan.

## Objective and boundary

Provide an Aoide-native HTTPS API for mail, authorized mesh observation, and
separately granted control. Work on LAN, closed networks and internet routes
without requiring SSH accounts. SSH remains supported; live synchronization is
not dependent on replacing it.

Design the application API, not new TLS, cryptography or general RPC machinery.
Reuse the existing mail store, session/project identities, command services,
authorization and audit. Select maintained HTTP/TLS libraries during the
implementation review rather than prescribing an unverified dependency now.

The network adapter remains separate from the local daemon. AOIDED.md describes
aoided as a local socket service; adding HTTPS does not silently turn it into a
public listener. Extend or replace the network-door adapter behind explicit
deployment configuration. Every operation reaches the same daemon policy and
audit boundary with its authenticated origin preserved. No generic remote
command-string or unrestricted registry-dispatch endpoint.

```text
CLI / agent / Conductor / optional web client
                       |
              HTTPS requests + WSS stream
                       |
       network door: TLS, identity, limits, validation
                       |
          principal-bound local daemon operations
              /                         \
    existing mail/receipts       session/project projections
              \                         /
                   existing audit
```

## Connections and trust

- Direct reachable peers use HTTPS for requests and WSS for ongoing events.
- LAN and offline networks use configured trust: private CA certificates or
  explicitly verified peer certificate/key bindings. Public internet service
  and public DNS are not prerequisites. Validate endpoint names/IPs normally;
  pinning is a designed trust mode, not a skip-verification flag.
- Pairing establishes peer identity, credential and scopes; transport
  encryption does not replace application authentication.
- Prefer reuse of existing signed-peer authentication if suitable. Otherwise
  individual revocable tokens are a candidate, not a shared mesh password.
  Settle one coherent binding rather than layering redundant credentials.
- Revocation and scope changes affect active streams as well as new requests.
- No automatic NAT traversal in the first slice. A later relay allows outbound
  connections from both machines; direct and relay routes retain message IDs.

TLS protects a connection. A TLS-terminating relay/proxy can read plaintext
unless payloads also use end-to-end encryption. E2E means between authorized
Aoide endpoints, not isolation between processes sharing a local Unix account.
Do not claim relay confidentiality until a vetted message/group encryption
scheme, peer verification and area-membership key rotation are implemented.
Removing a subscriber cannot revoke already received plaintext.

## API resources

These are semantic resource groups, not finalized URL or command spellings.
Version the wire contract and negotiate supported features before use.

| Resource | Operations and limits |
|---|---|
| Mail | Send to granted recipients, fetch authorized inbox, acknowledge, inspect own delivery status |
| Areas | Discover authorized areas, subscribe/unsubscribe, retrieve bounded history |
| State | Authorized snapshot of hosts, projects, sessions, terminals, agents and relationships |
| Events | Subscribe to permitted changes after a cursor; bounded recoverable stream |
| Presence | Expiring host connection/heartbeat state, distinct from process state |
| Control | Explicit typed actions through existing gates, added after read/mail proof |
| Credentials | Owner-managed pairing/revocation; never granted implicitly to observers |

Read access is not control access. Roster access is not transcript access.
Prompts, responses, tool activity, usage, file metadata and logs each retain
their authorization and availability boundaries; secret values are excluded.
An unavailable source field is reported as unavailable, not fabricated.

The first external access profile is the restricted correspondent in
[MAIL-ONLY-ACCESS.md](MAIL-ONLY-ACCESS.md). An outsider need not receive mesh
topology or host administration access merely to exchange letters.

## Shared areas and live state

Borrow Echomail's subscription and distribution model: participating nodes
receive records for areas they are authorized to carry. This is not an FTN
wire-format compatibility commitment and not a reason to store every heartbeat
as a permanent letter.

```text
project/dxflake          durable correspondence
host/osaka/sessions     structured lifecycle changes
host/sakaki/activity    authorized activity changes
          |
     subscribed observer
          |
     local mesh projection → Conductor
```

Each host is authoritative for its own local process state. Use stable
host-qualified object IDs, origin identity, schema version and ordered origin
sequence/epoch. Do not infer cross-host causal order from wall clocks alone.
Verify origin and area membership before accepting or forwarding events.
Prevent duplicates/loops by stable IDs and bounded forwarding, not by reminting
messages at each hop. Exact routing and subscription storage are phase decisions.

Initial subscription needs a snapshot tied to a cursor, followed by changes
after that cursor without a race. On reconnect, replay retained events; if the
cursor is too old or history changed, explicitly require a fresh snapshot.
Deletion/end events remove stale objects. Heartbeats expire: disconnected
means last-known state, not proof that every remote process ended.

Bound queues, retention, event size and slow subscribers. Use backpressure and
snapshot recovery for state; do not silently drop durable mail. Keep sensitive
logs/transcripts on demand rather than flooding every observer by default.
The observer may track all authorized machines without being their mandatory
relay or a new authority for their process state.

## Delivery semantics

HTTPS success identifies what the server actually accepted or filed, not that
an agent acted. Preserve queued/filed/fetched distinctions and stable
idempotency identifiers. Retry after a lost response must not duplicate work
or receipts. WSS is a transport stream, not durable storage; persistent cursors
and backend state provide recovery.

TLS alone does not prevent application retries/replays. Follow existing
envelope validation and deduplication rules. Doorbells are recipient-controlled
fixed nudges; mail content cannot become arbitrary terminal input through this
API. Control actions use explicit capabilities and existing admission gates.

## Deployment and configuration

Network access is opt-in. Configure listener address, endpoint identity,
certificate/trust source, credentials and grants, and connection/storage limits.
Direct TLS is the baseline architectural model; a reverse proxy is an optional
deployment adapter with a protected backend and preserved authentication.
Do not treat loopback or proxy-supplied identity headers as inherently trusted.

Ordinary configuration works without Nix. Optional Nix modules declare services
and credential-file references, never secrets in the store. Harnox integration
can supply custody through the existing secrets boundary; this API does not
create another secrets broker. Certificate renewal, pin rotation and token
revocation must be operable on offline networks as well as connected ones.

## Deferred implementation sequence

1. Finish current higher-priority fixes and agreed checkpoints. Inventory the
   current network door, event feed, grants and daemon calls before coding.
2. Specify versioned request/error schemas, identity binding, scope enforcement,
   limits and snapshot/cursor behavior. Record compatibility with SSH transport.
3. Prove authenticated HTTPS mail between two machines with SSH unavailable:
   normal send/fetch, offline queue, reconnect, duplicate response recovery,
   invalid certificate/credential, revocation and unauthorized inbox rejection.
4. Add state snapshots and WSS events; prove gap recovery, host restart,
   slow-consumer bounds and honest offline state. Connect Conductor to the same
   projection, not a separate UI-only data path.
5. Add authorized shared-area distribution. Prove membership, duplicate/loop
   handling and origin verification. Keep control grants separate.
6. Consider relay and E2E payload protection as an explicit subsequent phase.
   Prove recipient/key membership changes and reconnect behavior before use.
7. Add typed control actions only through existing policy, audit and permission
   tests. Provide optional deployment/UI configuration after headless proof.

No cutover removes SSH until parity and recovery are demonstrated. A successful
HTTP request is not proof of live mesh consistency or isolation.

## Open design decisions

- Existing signed-peer identity versus scoped token binding at the HTTPS door.
- TLS termination/library and certificate provisioning for each deployment.
- Area grants and per-field observation scopes mapped to existing capabilities.
- Snapshot/cursor persistence and retention limits in the existing event model.
- Relay ownership and vetted E2E scheme, if relay confidentiality is required.

## References

- [Mail](MAIL.md), [pairing](PAIRING.md), [local daemon boundary](AOIDED.md).
- [Mail-only external access](MAIL-ONLY-ACCESS.md).
- [Maximus Echomail documentation](https://maximusng-bbs.github.io/maximus/fidonet-echomail/): subscribed-area distribution reference.
- [TLS specification](https://www.rfc-editor.org/info/rfc8446/): transport confidentiality and authentication; follow current maintained implementations and standards when implementing.
