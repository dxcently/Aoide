//! `aoide mesh charter` — the operator's four commands (P-CHARTER,
//! `docs/architecture/HTTPS-MESH-API.md` "Charters", MAIL.md's P-CHARTER
//! slice).
//!
//! One mesh, one operator key. `init` roots a mesh on the operator's machine
//! and writes an empty source; the operator pastes each machine's node line
//! under `[nodes]` and runs `sign`, which validates, bumps the version, writes
//! `<file>.sig`, applies the result here and spools the signed pair to every
//! node on the charter; every other node trusts the operator key once (the
//! `[mesh.<name>] operator` line in config) and then takes `accept`, by hand
//! or by letter.
//!
//! **Every piece of the mechanism lives in `aoide_storage::charter`**: the
//! document and its validation, the five accept steps, the operator key at
//! rest, the high-water mark, the spool. These handlers parse an invocation,
//! call one of them, and render — the only work they do themselves is the
//! best-effort drain of what `sign` spooled and the audit line a re-key
//! deserves.
//!
//! `aoide mesh`'s own drift report and `mesh pair` stay where they were
//! (`crate::mesh`); `mesh join` and the door's grants-per-mesh are the
//! trust-per-mesh slice's.

use aoide_protocol::audit::{audit, default_audit_log, Door, EventClass};
use aoide_protocol::output::Outcome;
use aoide_protocol::registry::{arg, cmd, flag, Registry};
use aoide_protocol::Invocation;
use aoide_storage::charter;
use serde_json::json;
use std::path::{Path, PathBuf};

pub fn register(r: &mut Registry) {
    r.insert(cmd!(
        path: ["mesh", "charter", "init"],
        summary: "Root a mesh on the operator's machine: mint its operator key (state/operator/<mesh>.key, 0600, never printed) and write an empty charter source at $AOIDE_ROOT/charters/<mesh>.toml. Prints the operator line to put in every machine's config. Never overwrites an existing source.",
        args: [arg!("mesh", "string", true, "The mesh to root.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_charter_init,
        examples: ["mesh charter init home"],
    ));
    r.insert(cmd!(
        path: ["mesh", "charter", "sign"],
        summary: "Validate the charter source (name grammar, capability vocabulary, addresses, relays, and every node's age binding under the key on its own line), write the next version into it IN PLACE, sign those exact bytes with the mesh's operator key, write <file>.sig beside it, apply it here, and spool the signed pair to every node on the charter. This is the last write to a charter file: any later edit invalidates the signature.",
        args: [arg!("mesh", "string", true, "The mesh whose source to sign.")],
        flags: [flag!("file", "string", "Sign this source instead of $AOIDE_ROOT/charters/<mesh>.toml; its signature goes to <file>.sig. Use it for a source kept in a Nix repository, which holds public keys only.")],
        gated: false,
        implemented: true,
        handler: handle_charter_sign,
        examples: ["mesh charter sign home", "mesh charter sign home --file ./home.toml"],
    ));
    r.insert(cmd!(
        path: ["mesh", "charter", "accept"],
        summary: "Apply a signed charter carried by hand: <file> plus its detached <file>.sig. Verifies the digest, the operator key this node trusts for that mesh, the signature, and the version against the high-water mark, before anything is written. Refuses with one of unknown-operator, stale-charter, charter-tampered or operator-mismatch.",
        args: [arg!("file", "string", true, "The charter source; its signature must be at <file>.sig.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_charter_accept,
        examples: ["mesh charter accept ./home.toml"],
    ));
    r.insert(cmd!(
        path: ["mesh", "charter", "reroot"],
        summary: "Replace a lost or compromised operator key: mint a new one, record it as this machine's root, and sign the current source under it. Every other machine must then trust the new key the way it trusted the first — one config line, or its own join — and each keeps working on its last charter until it does.",
        args: [arg!("mesh", "string", true, "The mesh to re-root.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_charter_reroot,
        examples: ["mesh charter reroot home"],
    ));
}

fn handle_charter_init(inv: &Invocation) -> Outcome {
    let cmd = "mesh.charter.init";
    let Some(mesh) = mesh_arg(inv) else {
        return Outcome::usage(cmd, "usage: aoide mesh charter init <mesh>");
    };
    match charter::init(mesh) {
        Ok(init) => Outcome::ok(
            cmd,
            format!(
                "mesh `{}` is rooted — put this line in `[mesh.{}]` on EVERY machine, then paste each machine's node line under `[nodes]` in {} and run `aoide mesh charter sign {}`:\n  {}\n  # operator fingerprint {}",
                init.mesh,
                init.mesh,
                init.path.display(),
                init.mesh,
                charter::operator_line(&init.operator),
                init.fingerprint,
            ),
        )
        .changed(vec![
            init.path.to_string_lossy().into_owned(),
            init.key_path.to_string_lossy().into_owned(),
            init.trust_path.to_string_lossy().into_owned(),
        ])
        .with_data(json!({
            "mesh": init.mesh,
            "operator": init.operator,
            "fingerprint": init.fingerprint,
            "file": init.path.to_string_lossy(),
        })),
        Err(e) => Outcome::error(cmd, e).with_data(json!({ "reason": "init-failed" })),
    }
}

fn handle_charter_sign(inv: &Invocation) -> Outcome {
    let cmd = "mesh.charter.sign";
    let Some(mesh) = mesh_arg(inv) else {
        return Outcome::usage(cmd, "usage: aoide mesh charter sign <mesh> [--file <path>]");
    };
    let file = inv
        .flags
        .get("file")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    match charter::sign(mesh, file.as_deref()) {
        Ok(signed) => {
            let delivery = drain_spooled(&signed.spooled);
            Outcome::ok(
                cmd,
                format!(
                    "signed {} v{} (operator {}) — applied here, spooled to {} node(s){}",
                    signed.mesh,
                    signed.version,
                    signed.fingerprint,
                    signed.spooled.len(),
                    rekey_note(&signed.rekeyed),
                ),
            )
            .changed(vec![
                signed.path.to_string_lossy().into_owned(),
                signed.sig_path.to_string_lossy().into_owned(),
                charter::in_force_path(&signed.mesh).to_string_lossy().into_owned(),
                charter::trust_path(&signed.mesh).to_string_lossy().into_owned(),
            ])
            .with_data(json!({
                "mesh": signed.mesh,
                "version": signed.version,
                "operator": signed.operator,
                "fingerprint": signed.fingerprint,
                "file": signed.path.to_string_lossy(),
                "sig": signed.sig_path.to_string_lossy(),
                "spooled": signed.spooled,
                "delivery": delivery,
                "rekeyed": signed.rekeyed,
            }))
        }
        Err(e) => Outcome::error(cmd, e).with_data(json!({ "reason": "sign-failed" })),
    }
}

fn handle_charter_accept(inv: &Invocation) -> Outcome {
    let cmd = "mesh.charter.accept";
    let Some(file) = inv.args.first().map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        return Outcome::usage(cmd, "usage: aoide mesh charter accept <file>");
    };
    let path = Path::new(file);
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            return Outcome::error(cmd, format!("{}: {e}", path.display()))
                .with_data(json!({ "reason": "unreadable-charter" }));
        }
    };
    let sig_path = charter::sig_path_for(path);
    let sig = match std::fs::read(&sig_path) {
        Ok(sig) => sig,
        Err(e) => {
            return Outcome::error(
                cmd,
                format!(
                    "{}: {e} — a signed charter travels as a PAIR; the detached signature must sit beside it",
                    sig_path.display()
                ),
            )
            .with_data(json!({ "reason": "unreadable-signature" }));
        }
    };
    match charter::accept(&bytes, &sig) {
        Ok(accepted) => {
            for rekeyed in &accepted.rekeyed {
                let _ = audit(
                    &default_audit_log(),
                    Door::Cli,
                    EventClass::Audit,
                    "mesh.charter.accept",
                    "rekeyed",
                    &format!(
                        "{} v{} re-keyed `{}`: {} -> {}",
                        accepted.charter.mesh, accepted.charter.version, rekeyed.node, rekeyed.from, rekeyed.to
                    ),
                );
            }
            let operator = charter::fingerprint_of_key(&accepted.operator);
            Outcome::ok(
                cmd,
                format!(
                    "mesh `{}` v{} in force (operator {}){}",
                    accepted.charter.mesh,
                    accepted.charter.version,
                    operator,
                    rekey_note(&accepted.rekeyed),
                ),
            )
            .changed(vec![
                charter::in_force_path(&accepted.charter.mesh).to_string_lossy().into_owned(),
                charter::in_force_sig_path(&accepted.charter.mesh).to_string_lossy().into_owned(),
                charter::trust_path(&accepted.charter.mesh).to_string_lossy().into_owned(),
            ])
            .with_data(json!({
                "mesh": accepted.charter.mesh,
                "version": accepted.charter.version,
                "operator": accepted.operator,
                "fingerprint": operator,
                "rekeyed": accepted.rekeyed,
            }))
        }
        Err(r) => Outcome::error(cmd, format!("{}: {}", r.reason, r.detail)).with_data(json!({
            "reason": r.reason,
            "detail": r.detail,
        })),
    }
}

fn handle_charter_reroot(inv: &Invocation) -> Outcome {
    let cmd = "mesh.charter.reroot";
    let Some(mesh) = mesh_arg(inv) else {
        return Outcome::usage(cmd, "usage: aoide mesh charter reroot <mesh>");
    };
    match charter::reroot(mesh) {
        Ok(signed) => {
            let delivery = drain_spooled(&signed.spooled);
            Outcome::ok(
                cmd,
                format!(
                    "mesh `{}` re-rooted and signed v{} under the new key. Every other machine keeps working on its last charter until it takes the new key the way it took the first — its own trust-entry step: the same line in `[mesh.{}]`, or `aoide mesh join {} --operator <key>` on a host whose config is not hand-editable (the trust-per-mesh slice's; it is the ONLY way to record a key, and a re-rooted machine whose config line alone changes stops on `operator-mismatch` until the record follows). Never edit `state/mesh/{}/trust.json` — that is where the version mark lives.\n  {}\n  # operator fingerprint {}",
                    signed.mesh,
                    signed.version,
                    signed.mesh,
                    signed.mesh,
                    signed.mesh,
                    charter::operator_line(&signed.operator),
                    signed.fingerprint,
                ),
            )
            .changed(vec![
                charter::operator_key_path(&signed.mesh).to_string_lossy().into_owned(),
                signed.path.to_string_lossy().into_owned(),
                signed.sig_path.to_string_lossy().into_owned(),
                charter::trust_path(&signed.mesh).to_string_lossy().into_owned(),
            ])
            .with_data(json!({
                "mesh": signed.mesh,
                "version": signed.version,
                "operator": signed.operator,
                "fingerprint": signed.fingerprint,
                "spooled": signed.spooled,
                "delivery": delivery,
            }))
        }
        Err(e) => Outcome::error(cmd, e).with_data(json!({ "reason": "reroot-failed" })),
    }
}

/// The mesh positional, or `None` for a usage answer.
fn mesh_arg(inv: &Invocation) -> Option<&str> {
    inv.args.first().map(|s| s.trim()).filter(|s| !s.is_empty())
}

/// Drain what `sign`/`reroot` spooled, one node at a time, best-effort. The
/// command has already reported the WRITE; a node that cannot be dialed is
/// `mail outbox`'s story, exactly as it is for `mail send` (spec item 8) —
/// and the entry stays spooled, because the charter reaching that node is not
/// this command's promise to keep.
fn drain_spooled(spooled: &[String]) -> Vec<serde_json::Value> {
    spooled
        .iter()
        .map(|node| match crate::mail_wire::drain_node(node) {
            Ok(()) => json!({ "node": node, "drained": true }),
            Err(e) => json!({ "node": node, "drained": false, "error": e }),
        })
        .collect()
}

/// The one sentence a re-key deserves wherever a charter is applied or signed.
fn rekey_note(rekeyed: &[charter::Rekeyed]) -> String {
    if rekeyed.is_empty() {
        return String::new();
    }
    let names: Vec<&str> = rekeyed.iter().map(|r| r.node.as_str()).collect();
    format!(
        " — RE-KEYED: {} (a new key on an old name is what a stolen operator key would sign)",
        names.join(", ")
    )
}
