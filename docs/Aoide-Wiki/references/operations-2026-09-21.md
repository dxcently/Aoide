# Operations ledger — 2026-09-21

## ThinkChiyo native connection and verification

The User configured native Windows OpenSSH on ThinkChiyo. The daemon listened
on port 22, but its Private-only firewall rule did not permit this connection.
The User added `Aoide-SSH-From-Yomi`, allowing TCP 22 from `192.168.1.158`, then
authorized Yomi's public key in the Windows administrator key file. Key login
as `thinkchiyo\dxcen` is verified. No password or private key was transferred.

Yomi's SSH alias `ThinkChiyo` resolves to `192.168.1.205` with user `dxcen`.
This is verified shell access; no app handoff or saved remote Aoide project
is claimed. Existing projects were not modified.

Verification uses Eidolon/Ollama `deepseek-v4.1-flash` executors and a separate
reviewer. Native tools already installed include Rust 1.98.1/MSVC, Git and
Codex. The scratch harness lives at
`C:\Users\dxcen\aoide-native-proof-20260921\harness`; it builds offline and
uses byte copies of the actual protocol modules, with no WSL or mocks.

### Initial native result

The first run executed 29 tests: 24 passed and 5 failed. All 17 feed tests
passed, including protected ACL readback, rejection without truncation,
append/cap behavior and file replacement. Three executable-test expectations
were wrong: platform path separators, case-sensitive path-string comparisons,
and a `.cmd` sibling expectation contradicting the `.exe` convention. Two
additional failures came from the poisoned test lock after the first panic.

### Corrected native result

An independent reviewer accepted the test-only corrections after verifying
that the production region of `bin.rs` was byte-identical. The reviewer copied
the corrected test source to ThinkChiyo, verified its SHA-256 against Yomi,
and ran `cargo test --offline --lib -- --test-threads=1`: all 29 native tests
passed with exit 0. The Linux protocol suite independently passed all 160
tests. No ignored tests or poison recovery were added.

Corrected `bin.rs` SHA-256:
`708f4fb5f866d2ab78ed769010898e8cf2b34110af561c3532ef3b4aaf396fb7`.
Review evidence: `/tmp/aoide-native-e2e/thinkchiyo-bin-review-report.md`.

### Two-process feed probe

Both child processes signal readiness before the parent releases them.
Each appends 1,000 short JSON records through `FeedWriter` with mode `0600`
and an effectively unbounded cap. The native run completed with exactly
2,000 valid, unique records, zero missing/duplicate/blank lines and successful
child exits. This is bounded host evidence, not universal atomicity proof.

The feed source matches commit `0526b70`:

| Source | SHA-256 |
| --- | --- |
| `protocol/src/feed.rs` | `8f440f917681a8f6770fad0f5954f07014e7853f61afedba20dd81dcb717e048` |
| `protocol/src/feed_windows.rs` | `aecf3f0c65635a2c40a4162499047050a73502936cc61dc295712ea7c5c97fa9` |

Scratch evidence on Yomi: `/tmp/aoide-native-e2e/thinkchiyo-report.md`,
`thinkchiyo/cargo-test-native.log`, `thinkchiyo/append_probe.frozen.rs`, and
`thinkchiyo/cargo-append-gated-native.log`. The gated log SHA-256 is
`deac16aa3ca17f43d3488b924260b1d3d6c410fb810aaf09e256c13ef2048737`.

The full native core port, daemon deployment and HTTPS implementation remain
unfinished. These results cover the protocol modules only.
