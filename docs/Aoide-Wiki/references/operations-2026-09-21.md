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

## ThinkChiyo to Yomi app connection preparation

The User requested an SSH connection from ThinkChiyo. A dedicated Ed25519
key was generated on ThinkChiyo as `.ssh/id_ed25519_aoide_yomi`; its private
half remains there. Its public key was appended to Yomi's authorized keys.
Yomi's actual host public key was installed in ThinkChiyo's known-hosts file.

ThinkChiyo's `.ssh/config` now contains `Host Yomi`, targeting
`khoa@192.168.1.158` with that identity. `ssh Yomi hostname` from Windows
returns `yomi-strix`. Codex is available in the remote login shell and reports
an existing ChatGPT login. The remaining app action is to enable `Yomi` under
SSH connections and select `/home/khoa/Aoide`; no app handoff is claimed.

## Managed wrapper consumer checkpoint, 0.0.25

The User requested a checkpoint of the completed work and a patch-version bump.
The managed wrapper accepts any executable with its normal headless arguments;
process tracking, captured output, instructions, child mail and completion
reporting do not require a harness trace adapter. Exact prompts, reasoning and
tool-event streaming are optional. Linux is verified; native Windows supervision
remains unfinished.

Parent consumption found and corrected four integration defects: the native
Eidolon sweep removed conduct-owned wrappers, live mail omitted letter bodies,
timeouts displayed as ordinary stops, and a watcher started before the mailbase
existed missed its first letter. Initial attachment also catches letters filed
between the opening frame and attachment without replaying consumed mail.

Two real Ollama `deepseek-v4.1-flash` tasks ran under private Aoide daemons using
the new Eidolon producer. The final run lasted 75 seconds, retained its wrapper
through native presence scans, rendered the first unread letter before the
child read it, and completed with exit code 0. Two observer snapshots preserved
the same unread letter; only the child's read drained it. The daemon filed the
completion report and retained the finished record. Separate consumer probes
covered detached runs, exit code 7 and a two-second timeout. The CLI uses its
normal error exit status while preserving the child's exact exit code in the
record. No Codex desktop wake is claimed: the report had no armed reader.

The conduct suite passed 858 tests before the final attachment correction;
the focused view suite then passed 10 tests, including the new startup-race
case. The workspace all-target check passed. The exact source system build
before the version bump passed. Evidence is retained locally under
`/tmp/aoide-wrapper-consumer-20260921/`; private test daemons were stopped.

Eidolon upstream `d9ff7002877469054e1ca29acae059da18e030f7` was built with harnox
`cbff17589bc4dab5d43472104d47589b899b5c15`. Fresh run, resume, read-only export
and real-provider tests passed. Its reader failed on a copy of an older agent
journal at bitcode offset 420248, while the previous reader succeeded without
changing the copy. The default producer was therefore restored to the previous
binary. The new producer remains available explicitly as
`~/.local/bin/eidolon-current-20260921`; the Aoide Nix package is a launcher and
does not itself upgrade that external binary. Legacy journal compatibility is
still required before making the candidate the default.

HTTPS remains a design for end-to-end encrypted, signed immutable letters;
SSH remains the current transport. The native Windows core port and AoideOS
migration to the documented dxflake composition remain open lanes.

The versioned 0.0.25 system build passed at
`/nix/store/kjqdvrfynymcyhq9pnapc4ghvxm8g0pn-nixos-system-yomi-strix-26.11.20260907.dc5d91f`.
Its package checks include 859 passing conduct tests. A final consumer test used
that built release to wrap a silent `sleep` process under the unregistered
`generic-headless` label: it remained visible with empty output, then recorded
exit 0 and filed its completion report. This proves basic availability does not
require a harness profile or a structured output stream. No system activation
was performed.

At 08:19:28 EDT the live user service was observed running the built 0.0.25
`aoided`, and `/run/current-system` pointed to the tested `kjqdvr…` system.
The parent agent did not initiate the activation. The corrected wrapper is now
available through the live daemon as well as the private consumer fixtures.

Independent Eidolon/DeepSeek review returned SHIP for the final Linux consumer
corrections, including the parent's attachment-race delta. The reviewer also
reproduced retained wrapper ownership, native-child ancestry, native-presence
cleanup and exactly one completion report with a private synthetic presence
socket. A separate bounded source review returned SHIP. Neither verdict claims
Windows completion or producer legacy-journal compatibility.
