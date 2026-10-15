# Two-mesh acceptance: letters across a shared gate, end boxes on 443 only

The procedure that proves two charter meshes exchange mail through one gate,
over HTTPS, with each end box reaching nothing but that gate. Design:
[MAIL.md](MAIL.md) (Transit, the four steps),
[HTTPS-MESH-API.md](HTTPS-MESH-API.md) ("Charters", "Transports and relays").
The single-mesh live runbook is
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
`ip` and `openssl` on `PATH`. It skips with a printed reason when one is
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

Steps that read the gate's own box are skipped there and print a skip line:
A1 (the gate's charter), A2b, A4, A9a, A9b and A9c. Two checks inside A5n and
A7 (the gate's outbox, and the gate's front and adapter from a fence) are left
out without a line. The rest run unchanged.

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
| A7 | From inside each fence the gate answers. 1.1.1.1:443, UDP 8.8.8.8:53, TCP port 22 (on 1.1.1.1 and on the gate's address), and the gate's front and adapter ports on 127.0.0.1 do not. The host reaches the front and the adapter, so the refusals are the fence's. |
| A8 | With no CA bundle, https fails at verification (exit 60), nothing is filed, and a send stays spooled. |
| A9b | A letter to a mesh with no gate is refused `no-route` at the gate. The sender hands it to its relay first. |
| A9c | labbox holds no record of the gate, so the charter alone names the relay. With the relay down, the poll reports `unreachable`, never `polled`, and files nothing. |
| A9a | When home drops its gate into lab, the gate's route for lab flips from a hop to a refusal. The refusal reads `not-a-member`, not `one-sided-gate`; the test logs that as a finding. |

## Bootstrap seeds

One step writes node records by hand. Each write is marked `BOOTSTRAP` in the
test, and each is a cross-mesh seal:

- homebox learns labbox's age binding (`seed_cross_mesh`). Home's charter does
  not declare labbox, so the binding cannot come from a charter homebox holds.
- labbox learns homebox's age binding for the reply (`labbox_takes_home`), for
  the same reason: lab's charter does not declare homebox.

Both stand in for the unbuilt cross-mesh origin gate. A seed goes when a seal
reads the binding from the charter itself; the assertions it supports stay.

A poll node needs no seed. It asks its relay from the signed charter alone, with
no record of the relay in its state.
