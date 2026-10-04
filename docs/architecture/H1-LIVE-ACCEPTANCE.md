# H1 live acceptance: letters through the HTTPS relay

The procedure that proves the mail adapter on a real relay, from a node that
has no route to the home network except HTTPS 443. Design:
[HTTPS-MESH-API.md](HTTPS-MESH-API.md) (the deployment shape under
"Transports and relays", the H1 phase), [MAIL.md](MAIL.md) (Wire, Transit,
Status). Contracts: CONTRACTS.md §6 "`aoide mail serve`".

```text
osaka (operator, LAN) ---ssh, admin only---> sakaki (relay)
                                              cloudflared (TLS) -> reverse proxy :8080 -> aoide mail serve 127.0.0.1:8712
yomi-strix (other network, HTTPS 443 only) ---HTTPS---> https://aoide.necoconeco.net
```

`$AOIDE_ROOT` is each box's Aoide root (default `~/.aoide`). Steps that run
on sakaki are run over ssh from osaka; `~/.local/bin` is not on the ssh `PATH`,
so where `aoide` does not resolve there, call it by its full path
(`~/.local/bin/aoide`). Node names are what `aoide mesh` prints on each box: `osaka`, `sakaki`,
`yomi-strix`. Mesh `home` is a charter mesh. Every command is run as the box's
Aoide user. `$M` is a marker word chosen per run, never a word a mailbox or
a node already uses:

```sh
M=h1-$(date +%s)
```

## 0. Preconditions

On sakaki (the relay unit and ingress are the dxflake slice's):

```sh
systemctl --user is-active aoided aoide-mail-adapter      # active, active
ss -ltn | grep -E ':(8710|8712)\b'                        # 127.0.0.1 only, never 0.0.0.0 or [::]
curl -sS http://127.0.0.1:8712/.well-known/agent-card.json
```

Expected: the card carries three keys, `name`, `protocolVersion`, `url`, and
nothing else (CONTRACTS.md §6: the stripped card). From any machine:

```sh
curl -sS https://aoide.necoconeco.net/.well-known/agent-card.json     # the same three keys
```

sakaki's A2A door (8710) must be bearer-gated, or off, before any public
ingress goes live: a caller behind cloudflared or a proxy arrives from loopback
and loopback is otherwise trusted (CONTRACTS.md §6, the bearer-token amendment).
Either:

```sh
systemctl --user is-active aoide-a2a                      # inactive: the door is off
systemctl --user cat aoide-a2a | grep -E 'AOIDE_A2A_(BEARER_SECRET|TOKEN_FILE)='
```

prints a non-empty `AOIDE_A2A_BEARER_SECRET=` (preferred; `aoide.a2a.bearerSecret`)
or `AOIDE_A2A_TOKEN_FILE=` (`aoide.a2a.tokenFile`). If the door is active and
both are empty, set one and rebuild before going further.

The reverse proxy's route for the hostname targets `127.0.0.1:8712` with the
path forwarded unchanged. `grep -rn 8710` over the proxy and tunnel config
returns nothing: the door is not an ingress target.

On osaka (the operator's machine):

```sh
aoide mesh charter show home        # a version in force, this box is the signer
aoide mesh                          # sakaki, yomi-strix, osaka listed, charter version column set
```

If `home` is not a charter mesh yet, root it first
(HTTPS-MESH-API.md "Charters", setup steps 1 to 4) and come back.

## 1. Yomi's public side (runtime state, not Nix)

None of this is a Nix option: core is configured by `config.toml`, charters
and state, and `aoide.config.settings` is whole-file-or-nothing
(`modules/nucleus/config.nix`). yomi-strix's Nix is unchanged.

1. osaka: edit `$AOIDE_ROOT/charters/home.toml`. Change only these `address`
   values and leave every `key`, `age` and `grant` as printed:

   ```toml
   [nodes]
   sakaki     = { key = "ed25519:…", age = '…', address = "https://aoide.necoconeco.net", grant = ["message", "read"] }
   yomi-strix = { key = "ed25519:…", age = '…', address = "poll", grant = ["message", "read", "spawn"] }
   osaka      = { key = "ed25519:…", age = '…', address = "poll", grant = […unchanged…] }
   ```

   `relays = ["sakaki"]` stays. `osaka = "poll"` is what makes yomi route to
   osaka through the relay: a node whose address is `ssh://` is attempted
   directly (MAIL.md Transit, step 1), and from the other network that
   attempt can only fail. Then sign:

   ```sh
   aoide mesh charter sign home
   ```

   Expected: a new version, one `charter` letter spooled per other node. yomi
   shows `NOT DIALLED` or `poll-only`: it is reached by its own ask.
2. osaka: carry the signed pair to yomi by file (a charter reaches a node off
   the LAN by file or by letter, HTTPS-MESH-API.md "Charters" step 4):

   ```sh
   scp $AOIDE_ROOT/charters/home.toml $AOIDE_ROOT/charters/home.toml.sig yomi-strix:/tmp/
   ```

   Any file carrier does: the signature is what is trusted, not the carrier.
3. yomi: apply it.

   ```sh
   aoide mesh charter accept /tmp/home.toml
   aoide mesh charter show home        # the version signed in step 1
   ```

   `unknown-operator` means yomi does not trust the operator key yet:
   `aoide mesh join home --operator ed25519:<hex osaka printed at charter init>`
   and accept again. This records the key in state. The alternative is a
   config line, and it carries a rollback hazard (CONTRACTS.md §4: a
   pre-P-CHARTER binary refuses `operator` as an unknown field):

   ```toml
   # $AOIDE_ROOT/config.toml on yomi-strix, only if the join above is refused as unreadable
   [mesh.home]
   operator = "ed25519:<64 lowercase hex>"
   ```

4. yomi: repoint the relay's VERIFIED record at its HTTPS address. A poll node
   dials a record (`mail_wire::poll_node`), a request is signed only for a
   verified record (`commands::sign_headers_for_node`), and the pairing that
   verifies is LAN-only. yomi was paired with sakaki on the LAN, so the
   existing record is moved, once, keeping its key and grants:

   ```sh
   aoide node address sakaki https://aoide.necoconeco.net
   jq '.nodes[] | select(.name=="sakaki") | {name,url,verified,grants}' $AOIDE_ROOT/state/nodes.json
   ```

   Expected: `url` is the HTTPS address, `verified` is `true`, `grants.home`
   contains `message`. The command clears `via`: an `https://` url together
   with a `via` is refused at the dial seam (`node_store::transport_conflict`).
   If `node address` answers `unknown-node`, or `verified` is `false`, there is
   no paired record to repoint: stop and report; pairing cannot be done across
   this network.
5. yomi: the ask is a timer, because a node with nothing to send never dials
   (MAIL.md Wire). A transient user timer, removed in section 9:

   ```sh
   systemd-run --user --unit=aoide-h1-poll --setenv=PATH="$PATH" \
     --on-active=5s --on-unit-active=30s "$(command -v aoide)" mail poll sakaki
   ```

6. osaka also asks its relay, by hand in the steps below (`aoide mail poll
   sakaki`), because osaka is `poll` in the charter.

Check, on yomi:

```sh
aoide mail route osaka/conductor
```

Expected: the path `yomi-strix -> sakaki -> osaka`, step 1 failing with
"asks its relay, not `yomi-strix`", step 2 picking `sakaki` at
`https://aoide.necoconeco.net`. Nothing is sent.

## 2. A refusal that means the sender cannot sign (stop on it)

The first deposit through the relay is T1 below. A hop reached off a charter
line is signed with this box's key (`mail_wire::dial_node`, `Node::declared`).
If a deposit still parks with a reason containing "SIGNED request from a paired
node", the hop was dialled with no key behind it: the letters' mesh is not a
charter mesh that lists the relay, and no record exists. Do not retry and do
not edit records around it: report it with `aoide mail outbox --json` from the
sending box, naming the mesh the entry rides.

A signed request whose key the receiving door's registry and in-force charter do
not carry is refused `-32007`; an `ssh://` charter hop, which used to go out
unsigned, meets that rule now.

## 3. T1: osaka to yomi, delivered, with a receipt back

osaka:

```sh
aoide mail send --to yomi-strix/conductor --json -- "$M osaka-to-yomi"
```

Expected: status ok, `data.next` is `sakaki`, a `msgid`. Then:

```sh
aoide mail outbox sakaki --json
```

Expected: one entry for that `msgid`, `delivery.status` `accepted` once the
deposit to `https://aoide.necoconeco.net` answered `accepted`.

sakaki (over ssh from osaka):

```sh
aoide mail outbox yomi-strix --json     # the sealed container, held for yomi's own ask
grep '"type":"transit"' $AOIDE_ROOT/state/mail/base.jsonl | tail -n 3   # a transit line for the msgid, no mailbox, no text
```

yomi (wait for the timer or ask now):

```sh
aoide mail poll sakaki
aoide mail read --for conductor
```

Expected: `polled 1 node(s): 1 envelope(s) filed`, then the text
`$M osaka-to-yomi`. yomi mints the destination's receipt, spools it toward
sakaki, and aoided's drain deposits it.

osaka (the receipt waits at the relay for osaka's ask):

```sh
aoide mail poll sakaki
aoide mail outbox sakaki --json
```

Expected: `1 envelope(s) filed` (the receipt), then the T1 entry reads
`delivery.status` `delivered`. Not `accepted`: delivered means a
destination-signed receipt is in osaka's own mailbase (MAIL.md Status).

## 4. T2: yomi to osaka, the same way

yomi:

```sh
aoide mail send --to osaka/conductor --json -- "$M yomi-to-osaka"
```

Expected: `data.next` is `sakaki`, the deposit goes out over HTTPS 443 (no
other port is open on this network).

sakaki: `aoide mail outbox osaka --json` shows the container held for osaka.

osaka:

```sh
aoide mail poll sakaki
aoide mail read --for conductor
```

Expected: the text `$M yomi-to-osaka`. Then on yomi, after its next timer
tick (or `aoide mail poll sakaki`): `aoide mail outbox sakaki --json` reads
`delivered`.

## 5. T3: an unsigned request is refused

From any machine, yomi included:

```sh
curl -sS https://aoide.necoconeco.net/ -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"aoide/mailPoll","params":{"node":"yomi-strix"}}'
```

Expected: HTTP 200 with a JSON-RPC `error`, code `-32010`, message starting
`mail poll refused:` and naming a verified, per-request SIGNED request
(`a2a.rs::poll_refusal`). No `result`, no envelope. The same shape for
`aoide/mailDeposit` with `"params":{"container":{}}`: `mail deposit refused`.
sakaki:

```sh
grep -c 'unauthorized' $AOIDE_ROOT/log        # increased by the attempts above
```

## 6. T4: a door method through the hostname is refused

```sh
for m in message/send tasks/get message/stream aoide/pairRequest aoide/graphSummary; do
  curl -sS https://aoide.necoconeco.net/ -H 'content-type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$m\",\"params\":{}}"; echo
done
```

Expected for every line: `"error":{"code":-32601,"message":"method not found: <method>"}`.
The method is not in the listener's table at all (CONTRACTS.md §6), so no
`task`, no `spawn`, no pairing state is created. sakaki:

```sh
grep 'mail-adapter.refused' $AOIDE_ROOT/log | tail -n 5
```

Expected: one `a2a.mail-adapter.refused` line per method, each with the detail
`HTTP 200 from loopback via mail-adapter`. The hostname reaches the adapter
and nothing else: there is no hostname that reaches the door.

## 7. T5: the relay stores only sealed bytes

Send one more letter with a body that cannot occur anywhere else, from osaka:

```sh
aoide mail send --to yomi-strix/conductor -- "$M plaintext-canary"
```

On sakaki, after it is deposited (and again after yomi has taken it):

```sh
cd $AOIDE_ROOT
grep -rl "$M" state log 2>/dev/null                      # no output
journalctl --user -u aoide-mail-adapter --no-pager | grep -c "$M"      # 0
grep -rl 'conductor' state/outbox state/mail 2>/dev/null # no output: no mailbox name
jq -r '.container | {purpose, msgid, mesh, to: .to.node, ct_bytes: (.ct|length)}' state/outbox/yomi-strix/*.json
```

Expected: the marker and the mailbox name appear nowhere; the container shows
`ct` as an opaque string and only routing facts beside it (node names, msgid,
mesh, size). A `type=transit` line in `state/mail/base.jsonl` holds metadata
and no body (MAIL.md Store). After yomi acks, the entry leaves the spool: the
`jq` glob matches nothing. The relay sees who wrote to whom, when and how
large, and that is its documented ceiling.

## 8. T6: `down` and `hold`

Each declaration is edited into `charters/home.toml` on osaka and signed with
`aoide mesh charter sign home`. Every sign bumps the version.

**Down (kept, refused, released).** First queue a letter yomi has not taken:
stop the timer (`systemctl --user stop aoide-h1-poll.timer` on yomi), then on osaka:

```sh
aoide mail send --to yomi-strix/conductor -- "$M queued-before-down"
```

Declare it down and sign:

```toml
[status]
yomi-strix = "down"
```

Expected on osaka:

```sh
aoide mail route yomi-strix/conductor    # no-route, and the sentence says declared `down`, no relay asked
aoide mail send --to yomi-strix/conductor -- "$M after-down"   # refused no-route at send time, nothing spooled
```

On sakaki the queued container is still in `state/outbox/yomi-strix/`: a
letter already queued for a `down` node is KEPT. On yomi, `aoide mail poll
sakaki` is refused by the relay naming `down` (the caller's own `down` is
refused at the door; yomi still holds the earlier charter and does not know).
Then remove the `[status]` table, sign, and restart yomi's timer:

```sh
systemctl --user start aoide-h1-poll.timer   # on yomi: the timer stopped above runs again
aoide mail poll sakaki          # on yomi
aoide mail read --for conductor # on yomi: "$M queued-before-down" arrives, the same msgid, never re-minted
```

Expected: the kept letter arrives, and the `after-down` letter was never
sent.

**Hold.** `hold` changes which side initiates, never whether a letter moves
(MAIL.md Status). Declare the relay held and sign:

```toml
[status]
sakaki = "hold"
```

Expected on osaka:

```sh
aoide mail route yomi-strix/conductor    # step 2: `sakaki` holds it at `https://aoide.necoconeco.net` -- it leaves when `sakaki` asks for it
aoide mail send --to yomi-strix/conductor -- "$M held"
aoide mail outbox sakaki --json          # the entry is hold-flavored; the deposit is never attempted
```

and sakaki's log shows no `mailDeposit` from osaka for it. The entry leaves
only by sakaki's own mailPoll of osaka, which a `poll` osaka never receives:
this is the specified behaviour, and so the cleanup is by hand. Remove the
`[status]` table, sign, then on osaka list and drop the held entries (the
`held` letter and the charter letter signed under the declaration):

```sh
aoide mail outbox sakaki
aoide mail outbox rm <msgid>             # each held entry
```

The declaration that no longer holds is read by the next send: a fresh
`aoide mail send` is deposited normally again.

## 9. Teardown

yomi:

```sh
systemctl --user stop aoide-h1-poll.timer aoide-h1-poll.service
```

Leave sakaki's record as repointed (it is the working HTTPS record), or point
it back with `aoide node address sakaki <the former address>`. osaka: restore the charter's
`address` lines and sign, if the LAN transport is wanted back. Record the run
in the session ledger with each step's command output.
