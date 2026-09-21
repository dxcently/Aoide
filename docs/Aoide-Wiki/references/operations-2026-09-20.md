# Operations ledger — 2026-09-20

## User decisions

- Execution and independent review use Eidolon with Ollama
  `deepseek-v4.1-flash`; Codex coordinates, inspects and commits.
- Linux and Windows are the primary core targets. macOS comes later;
  POSIX-compatible shared code should make that integration easier.
- Native Windows versus WSL remains unanswered. The current Unix-only
  dependencies do not establish native Windows support.
- JEV oversight covers both brief drift and completion evidence. CPU-only
  consumer hardware is a required acceptance target; performance budgets
  and measurements remain open.

## Three-host letters

All six directed probe routes delivered their original signed letters, and
return receipts were observed on all three hosts.

| Sender | Recipient | Probe message ID |
| --- | --- | --- |
| Yomi | Osaka | `4c7c49c912f1f7bb0891187708de695310623bdaadf9484eef9527ca07dbacce` |
| Yomi | Sakaki | `c8768d9a18290913db5b1d0ed62d788b4a5c33f27f0cdbb2200d040a0e1665be` |
| Osaka | Yomi | `6db667d587974a04df8096ae5ac9ef0f276165c1931473ea04c572f80fce1051` |
| Osaka | Sakaki | `6054d01ea9fc76a1b5093464f7a69dabc1ddbd137c3b8d1a56a17cb4ae6b8d9d` |
| Sakaki | Yomi | `a5db46a820d50cc742eac942be9a9fa6d5d362f8510eb770d98ef6cfaaa80f45` |
| Sakaki | Osaka | `f39d375a7f262a5f42a63676c66769c6d6571b9e24f365be1f7d23c7ceb87ac7` |

Osaka and Sakaki now reach Yomi through Wi-Fi `192.168.1.158`, preserving
the existing `192.168.1.175` host-key identity. Sakaki's existing public key
was authorized on Osaka and selected in Sakaki's SSH configuration.
Modified SSH files have `.aoide-ops-20260920.bak` backups. Private keys,
Aoide identities and node grants were not transferred or replaced.
Automatic retries delivered the original probes; no forced queue flush.

Evidence is in `/tmp/aoide-ops-20260920/mail-{yomi,osaka,sakaki}.json` and
`mail-results.md`. The Wi-Fi address is a runtime dependency; Ethernet
reachability and declaration of these SSH settings remain open.

## Landed code and verification

| Commit | Change | Evidence |
| --- | --- | --- |
| `c28cbda` | Pairing audit text withholds ceremony codes | Independent review; four audit regressions |
| `572ff20` | Broker-backed, value-free `secrets status` | 441 unit + 7 e2e tests; absent-broker smoke |
| `9308be9` | Shared POSIX liveness probe; source guards for Linux peer credentials | Storage 421, client 294, conduct 815 tests |
| `5584220` | Socket-address capacity and offset derive from the target layout; NUL refusal | Secrets 445 unit + 7 e2e tests; independent review |
| `57b958b` | TUI pairing, config and secret controls; version `0.0.24` | Conductor 191 tests; independent review; CLI 46 unit + 23 integration tests |

Six CLI integration tests requiring live discovery/pairing/SSH setup remain
ignored. Cargo ran per crate through the devshell, serialized by a shared
lock; no workspace-wide test command was used.

The final TUI smoke used an isolated empty state and absent broker socket.
Review, Status and Mesh opened and responded through a real PTY; quit exited
zero and restored the terminal. Pairing/TOTP rows and masked-input render
tests use synthetic fixtures. No real pairing or secret-release ceremony
was performed. Review fixes include asynchronous broker reads and mutations,
numeric request timestamps, modal mouse handling, honest error states, and
serialization of the TUI's competing node writes.

## Remaining boundaries

JEV/VV readiness documents and 16 synthetic prose-brief evaluation cases were
prepared and independently reviewed. The data validator accepts the fixture
and rejects invalid references, invalid verdicts, and prose-only completion
evidence. These are preparation artifacts, not trained models or production
labels; see [AOIDE-VV-JEV](../../architecture/AOIDE-VV-JEV.md).

- Core is **not yet POSIX-compliant**. The capability matrix is
  [CORE-POSIX](../../architecture/CORE-POSIX.md). Non-Linux peer identity
  still closes the daemon dispatch door; process identity, runtime paths,
  locks and other host assumptions remain.
- The FreeBSD cross-check lacked target `core`/`std` libraries; no non-Linux
  runtime proof or Windows build was obtained.
- JEV/VV runtime integrations and CPU measurements remain unbuilt. The
  readiness artifacts distinguish design constraints from runnable code.
- SSH-to-HTTPS remains the deferred proposal in
  [HTTPS-MESH-API](../../architecture/HTTPS-MESH-API.md); these transport
  repairs do not implement that migration.
- Mail export remains queued. No rebuild, deployment, daemon restart or
  push was performed; installed hosts do not acquire `0.0.24` from these
  local commits.
