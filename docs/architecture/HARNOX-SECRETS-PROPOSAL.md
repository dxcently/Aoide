# Harnox-backed Aoide secrets

Status: review draft, not implemented or a migration authorization. Proposal for Fable and the Harnox maintainers.

## Evidence boundary

Reviewed Harnox master bb68a3a505454afdf01e2caa48b9a496ec0f2093 (Cargo version 0.3.4) through an authenticated read-only checkout; Aoide source at 3f396282a133b9a668cd1b46f7f81c001e87cb11. Local Mneme 5512299 uses Harnox oauth-server pinned to v0.1.0. Local Melete 7619d40 still carries its own secrets implementation; that checkout is not evidence of Melete's latest deployed integration. No actual secret files were read, no secrets migrated, and no runtime security tests were executed for this draft.

Harnox sources: src/secrets.rs, src/lib.rs, Cargo.toml. Aoide sources: pkgs/aoide/crates/secrets/{README.md,AGENTS.md,src/broker.rs,src/policy.rs,src/client.rs,src/backend.rs}. The source and current deployment must be distinguished throughout implementation.

## Recommendation

Harnox is required for Aoide's optional secrets-management capability, not for core Aoide. Secrets support is first-class through Aoide's normal commands and deployment configuration. A secrets-enabled build directly depends on pinned upstream Harnox; a build without secrets support must still operate normally and report secret operations as unavailable rather than silently select a fallback store. Implementation must verify both build configurations.

Use Harnox's existing secrets feature as Aoide's custody implementation. Keep Aoide responsible for authenticated callers, per-operation authorization, approvals, audited delivery, and its shell-facing interface. Do not turn Harnox into an Aoide daemon or require Mneme/Melete to run for Aoide to obtain a local credential.

This replaces duplicated storage where semantics match. It is not a claim that Harnox already supplies a complete secrets broker. Importing one library into three services shares implementation, not a running store, permissions, or secret values.

```text
Agent / service / operator
          |
Aoide broker: authenticate -> authorize -> approve -> audit
          |
Harnox SecretStore: encrypted custody / rotation / issued-token verification
          |
Approved credential use in trusted code
          |
External service OR explicitly authorized process delivery

Lyra: approval UI and name-only events
Mneme: optional references to policy/docs; no credentials in memory or embeddings
Melete: another Harnox consumer, not Aoide's required intermediary
```

## Built versus planned

| Capability | Aoide implementation | Harnox implementation |
|---|---|---|
| Storage | file and age stores, backend get/set/has command templates | XChaCha20-Poly1305 encrypted file plus separate keyfile; Rust API |
| Retrieve/use | Local socket resolve, then client-side child environment injection | external_value returns Zeroizing<String> to trusted Rust consumer; not exposed as a tool |
| Issued tokens | Not the broker's principal model | issue returns token once, persists hash, verify checks it |
| Permissions | Broker policies, admin operations, consumer labels | service/grants metadata; external_value does not enforce these fields |
| Approval | TOTP enrollment, replay protection, pending/approve/dismiss, watch/UI integration | No equivalent approval queue or TOTP broker in the reviewed secrets module |
| Caller identity | Peer credentials and session-origin attestation code, with deployment gaps | Supplied by embedding application; secrets API takes no caller identity |
| Audit | Broker-side name-only events | Caller is instructed to audit injection; library is not the audit authority |
| Remote use | remote flag reserved; no broker network delivery/replication implementation | No independent secret transport/server in the reviewed library |
| Deployment | Separate broker service/uid and Unix socket, plus CLI | Embedded feature-gated crate, not a service |

Aoide's documented sequence includes V1 pure logic, V2 daemon/socket/CLI, V3 TOTP, V4 deployment/write/backend work, followed by automation, origin gating, parked approvals and watch surfaces. V5 cross-host broker replication is unbuilt; a stored remote flag does not make it operational. The integration should not revive replication merely because several applications share Harnox.

## Security mismatches that must be explicit

1. Aoide's consumer and automation names are self-asserted. Socket membership is currently the meaningful boundary; these labels do not establish separate hostile-agent identities.
2. Aoide's documented packaged cross-uid broker cannot access the operator's daemon/roster for origin attestation. Unknown callers are admitted by the existing origin policy. Moving storage to Harnox does not fix this.
3. Harnox's external_value(name) checks no grants or service binding. Aoide must authorize before calling it; do not maintain a second competing grant list in Harnox metadata.
4. Harnox documents no tool/script read path. Aoide secrets exec intentionally gives a child the secret and currently transfers it over the broker socket. That child can disclose it; environmental injection is not equivalent to custody-only HTTP injection. Maintainers must approve the intended integration contract or this use case remains on its existing backend.
5. Harnox serializes writes with an in-process mutex and explicitly assumes one writer process. Pointing Aoide, Melete and Mneme at the same encrypted files would violate that assumption. Use an Aoide-owned store initially. A shared service would be a separate design, not a filesystem shortcut.
6. Harnox external_value/has collapse load errors into missing/false. Aoide needs to distinguish absent values from corruption, missing key and I/O failure; it must not silently fall back to a different credential store.
7. Ciphertext and key coexist on a live host. Neither storage model protects secrets from a fully compromised owner/root. Harnox zeroizes selected buffers; this is not proof that every decrypted String copy is wiped.
8. Aoide value writes need their own authorization review: handle_put logs peer uid but put_gate takes no identity and does not apply consumer/TOTP authorization. Policy-admin uid gating does not cover that value-write path. CLI-only command registration is not a boundary against a direct socket caller.
9. Aoide audit writes are best effort, not durable-before-release guarantees. Existing peer-credential and ancestry code is Linux-specific; Nix-independent does not mean cross-platform identity support.

## Minimal integration shape

Depend only on a reviewed, pinned Harnox release with the secrets feature. Do not enable full, oauth-server, llm, or provider-auth for this change. Verify reproducible Cargo/Nix source fetching: Harnox is private, so publication/access for Aoide consumers must be resolved without embedding credentials in the flake or build inputs.

The broker owns SecretStore::at(an explicit broker-owned directory). A small typed Rust storage adapter calls existing Harnox methods; it does not shell out through a fake Harnox get executable or invent another protocol. Existing Aoide gates run before every read/write. Existing Aoide audit events describe the operation without values.

Harnox owns encrypted value storage and issued-token representation. Aoide owns secret-access policy in one place. Harnox grants metadata must not be presented as an independently enforced authority; choose and document a single mapping if it is used for display.

For isolated local services, retain the separate broker privilege boundary. Resolve caller identity across that boundary through a narrowly authenticated mechanism before claiming remote-origin or per-agent isolation. A failed identity lookup must not silently become a trusted local identity. Exact attestation transport and anonymous-service policy need a focused Aoide decision, not an unrelated Harnox feature request.

No change to Mneme memory retrieval is required. Memory can hold a secret reference/name if appropriate, never the credential. Persona identity and mesh membership do not confer secret access.

## Small upstream questions

- Is Aoide's authorized socket/process-environment delivery an acceptable trusted Rust consumer of external_value, or does Harnox intentionally support custody-only operations? Do not weaken the contract implicitly.
- Can Harnox expose an additive fallible accessor such as Result<Option<Zeroizing<String>>> (and a fallible existence check), preserving existing convenience callers? This distinguishes missing from damaged storage without Aoide reimplementing decryption.
- Confirm the single-writer assumption and recommended store ownership. Cross-process file locking is unnecessary if stores remain separate; do not add it speculatively.
- Which release and distribution method should Aoide pin so an external Nix consumer can build it?

No request for Harnox-specific Aoide node types, Aoide policy options, a custom OAuth grant, or a new secrets daemon is part of this proposal.

## Replacement and deletion

| Area | Treatment |
|---|---|
| Built-in file/age value storage and provisioning | Candidate removal after explicit migration and proof; do not delete existing values during cutover |
| Backend shell-template execution | Remove for migrated built-in storage; review whether any actual external-store consumer still needs it |
| Store-specific age CLI deployment dependencies | Remove only when no retained backend uses them |
| Aoide broker socket, caller checks, TOTP/replay, approvals, exec delivery, audit | Retain unless separately replaced by a reviewed equivalent; Harnox does not provide these |
| Handwritten SHA1/HMAC/base32/TOTP code | Not replaced by Harnox secrets; separate dependency review if retiring it |
| Harnox OAuth/provider/model modules | Do not add merely because the crate offers them |
| Future replication | Not implemented in this slice; no dual-authority or replicant-store topology |

Prefer one final custody implementation over permanent duplicated stores. During migration, identify all configured backends and consumers, migrate values privately, preserve policy and pending-request semantics, verify under the broker uid, switch the configured store, and retain a rollback snapshot until acceptance. Never print plaintext or put it in task logs, mail, process arguments or Nix store artifacts. Rollback must account for writes after cutover; it is not simply restoring an old ciphertext file.

## Phases and proof

1. Maintainer review: decide delivery semantics, error API and distribution. Inventory deployed consumers/backends without reading values into agent context.
2. Isolated adapter proof with synthetic secrets: set/use/delete, absent/corrupt/missing-key distinction, overwrite policy, issued versus external kinds, concurrent writes in the one owning process, and no accidental grant widening.
3. Aoide authorization proof: authenticated consumer/origin binding in actual cross-uid deployment, denied/unknown/remote callers, TOTP replay and parking, immediate revoke, no secret-bearing output/events. Do not describe these as fixed by storage replacement.
4. Implement CLI/deployment migration and docs; verify ordinary shell-only Aoide works with neither Mneme nor Melete running. Test failure/rollback and Cargo/Nix builds.
5. Operator-reviewed cutover, verify real service use without revealing values, then remove superseded code and update the task register. No automatic production secret migration is authorized by this draft.

## Decisions needed

The main choice is whether Aoide must retain generic secrets exec/socket delivery, move selected integrations to custody-only operations, or ultimately retire generic delivery. The least disruptive path retains current behavior only with an explicit Harnox contract agreement. Shared library adoption is a small code seam; replacing the entire broker, sharing one store across processes, or introducing a remote secrets authority is a substantially different project.
