# Two-mesh acceptance: letters across a shared gate, end boxes on 443 only

The procedure that proves two charter meshes exchange mail through one gate,
over HTTPS, with each end box reaching nothing but that gate. Design:
[MAIL.md](MAIL.md) (Transit, the four steps), [HTTPS-MESH-API.md](HTTPS-MESH-API.md)
("Charters", "Transports and relays"). The single-mesh live runbook is
[H1-LIVE-ACCEPTANCE.md](H1-LIVE-ACCEPTANCE.md).

```text
home (charter)                          lab (charter)
  homebox  (poll) --- fence ---+          +--- fence --- labbox (poll)
                               |          |
                               v          v
                        gate: one identity, in both charters
                        aoide mail serve behind a TLS front on :443
```

A box is `poll`: it asks the gate for its mail and never takes an inbound
connection. Each end box runs every `aoide` command inside a fence: a fresh
user and network namespace whose only route is `127.0.0.1:443`, carried out
over a unix socket to a host bridge. Nothing else leaves the namespace.

## Run it on one Linux box

The harness is `crates/cli/tests/two_mesh_https.rs`, ignored by default:

```sh
cargo test -p aoide-cli --test two_mesh_https -- --ignored --nocapture
```

It needs unprivileged user namespaces (`unshare -rn`), and `socat`, `curl`,
`ssh`, `ip` and `openssl` on `PATH`. It skips with a printed reason when one is
missing. A passing run removes its roots; a failing run keeps them and prints
the path, with every command's stdout and stderr under `runs/`.

## Run it against a live gate

Set all three variables and the harness starts no gate of its own. Its end boxes
then dial the gate you name:

| Variable | Holds |
|---|---|
| `AOIDE_TWO_MESH_GATE_URL` | `https://<ip-literal>:<port>/`. An IP literal, because the namespace has no name resolution. |
| `AOIDE_TWO_MESH_GATE_CA` | The PEM bundle the end boxes trust for that gate. |
| `AOIDE_TWO_MESH_GATE_LINE` | A file holding the gate's node line, named `gate`, with no address. `aoide identity --json` on the gate prints it as `data.nodeLine`. |

Steps that read the gate's own state (A2b, A4, A9a, A9c) print a skip line,
since that state is not on this box. The rest run unchanged.

## What each assertion proves

| ID | Proves |
|---|---|
| A1 | Both charters verify on each box that holds them, at the signed version. The gate carries one key in both meshes. |
| A2 | An end box routes to the other mesh's box through the gate as its relay (step 2). The gate crosses the letter into the destination mesh (step 3). |
| A3 | A letter from one mesh is accepted by the gate, not parked at the sender. |
| A4 | The gate holds the letter for the destination, held and in the destination's mesh. |
| A5n | Without the origin's charter the destination refuses the letter `unverified-origin`, and the gate keeps it. |
| A5 | With the origin's charter the destination files exactly that letter, origin verified. |
| A6 | A reply travels back the same way and is filed. |
| A6b | The receipt for the first letter returns and retires it from the sender's outbox. **Fails until cross-mesh receipts route.** |
| A7 | From inside each fence the gate answers and 1.1.1.1:443, UDP 8.8.8.8:53, ssh, and the gate's other ports do not. The host reaches the same targets, so the failures are the fence's. |
| A8 | With no CA bundle, https fails at verification (exit 60), nothing is filed, and a send stays spooled. |
| A9b | A letter to a mesh with no gate is refused `no-route` at the gate. The sender hands it to its relay first. |
| A9c | A poll with the relay record present and the relay down reports `unreachable`, never `polled`. |
| A9a | When home drops its gate into lab, the gate's route for lab flips from a hop to a refusal. |

## Bootstrap seeds

Two steps write node records by hand, because the design does not yet pair them
for the case. Each is marked `BOOTSTRAP` in the test:

- charter-poll: a poll node asks its relay from a key-bearing record in `nodes.json`.
- cross-mesh seal: a letter to another mesh is sealed with the destination's
  binding, which the origin learns from the destination's charter line.

Remove a seed when the design does the same from the charter alone; the
assertions it supports stay as they are.
