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
use crate::teach;
use aoide_protocol::output::{Fix, Kind, Outcome};
use aoide_protocol::registry::{arg, cmd, flag, Registry};
use aoide_protocol::Invocation;
use aoide_storage::charter;
use serde_json::json;
use std::io::BufRead;
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
        brief: "Root a mesh here: mint its operator key and an empty charter.",
    ));
    r.insert(cmd!(
        path: ["mesh", "charter", "sign"],
        summary: "Validate the charter source (name grammar, capability vocabulary, addresses, relays, and every node's age binding under the key on its own line), write the next version into it IN PLACE, sign those exact bytes with the mesh's operator key, write <file>.sig beside it, apply it here, and spool the signed pair to every node on the charter. This is the last write to a charter file: any later edit invalidates the signature.",
        args: [arg!("mesh", "string", true, "The mesh whose source to sign.")],
        flags: [flag!("file", "string", "Sign this source instead of $AOIDE_ROOT/charters/<mesh>.toml; its signature goes to <file>.sig. Use it for a source kept in a Nix repository, which holds public keys only.", value: "path")],
        gated: false,
        implemented: true,
        handler: handle_charter_sign,
        examples: ["mesh charter sign home", "mesh charter sign home --file ./home.toml"],
        brief: "Sign the charter source, apply it here and spool it to every node.",
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
        brief: "Apply a signed charter file carried by hand.",
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
        brief: "Replace a lost or leaked operator key and re-sign the charter.",
    ));
    r.insert(cmd!(
        path: ["mesh", "charter", "show"],
        summary: "Print the charter IN FORCE at this node and its status: the version, the operator key and its durable fingerprint, where this node's trust in that key is written (config line, state record, both, or neither), whether the key is decidable (a config/state disagreement is `operator-mismatch`, and then nothing about the mesh is honoured), the high-water version applied under that key, every node line the charter carries, and the re-keyed nodes the last applied version changed — a charter applied by an unattended poll has to be visible afterwards. Omitted <mesh>: every mesh with a charter on disk.",
        args: [arg!("mesh", "string", false, "The mesh whose charter to print. Omitted: every mesh with a charter in force here.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_charter_show,
        examples: ["mesh charter show", "mesh charter show home", "mesh charter show home --json"],
        brief: "Print the charter in force here, its operator and its status.",
    ));
    r.insert(cmd!(
        path: ["mesh", "join"],
        summary: "Enter a mesh by trusting its operator key — the one trust-entry step every machine but the operator's own performs. Two ways: `--operator <key>` records the operator line in STATE (never in config.toml, so a machine that rolls back to a pre-P-CHARTER build can still read its own config), or a LAN address runs the one local-network ceremony that fetches the operator key AND the charter in force from the operator's machine, prints the key's fingerprint and every node line for the operator to compare out of band, and then records the key and applies the charter. A relay, the HTTPS adapter, a tailnet and an ssh tunnel are all refused: the guard admits a private or link-local peer address and never loopback.",
        args: [
            arg!("mesh", "string", true, "The mesh to join."),
            arg!("host", "string", false, "The operator's machine on the local network — a bare host/IP (`sakaki`, `192.168.1.158`), or host:port; the door port defaults to AOIDE_A2A_PORT, else 8710. Omitted: --operator is required instead."),
        ],
        flags: [
            flag!("operator", "string", "The operator line the operator's machine printed (`ed25519:<hex>`) — the non-LAN path, no dial at all. Refused when the machine already trusts a different key for this mesh, unless --replace says the change is the operator's.", value: "ed25519-key"),
            flag!("replace", "bool", "The key this mesh's operator key is CHANGING to, deliberately: a re-root (`aoide mesh charter reroot`) on the operator's machine plus this flag on every other machine is the whole recovery from a leaked or lost operator key. It records the new key and CARRIES THE VERSION HIGH-WATER ACROSS, so the first charter the new key signs must still beat every version this mesh has applied — a replaced node cannot be walked back to an older version by a fresh key. Never a default: without it, a different key is refused, because replacing a mesh's root is the operator's decision and never a typo's. On the LAN arm a replacement ALWAYS asks for the fingerprint comparison, --yes included: that flag skips a first-time confirm, never a destructive one."),
            flag!("yes", "bool", "Skip the fingerprint confirm on the LAN arm. The fingerprint is the whole trust step there (this is a first-use ceremony), so skipping it commits to a key nobody compared — the same stance `pair --yes` takes on pairing's codes."),
        ],
        gated: false,
        implemented: true,
        handler: handle_charter_join,
        examples: ["mesh join home --operator ed25519:…", "mesh join home sakaki", "mesh join home 192.168.1.158 --yes --json"],
        brief: "Trust a mesh's operator key, by key or from the operator over the LAN.",
    ));
}

/// `aoide mesh join` — the trust-entry step, both arms. **No pairwise record is
/// ever written**: a join adds no `state/nodes.json` entry, because the
/// charter IS the mesh's node list, and the door reads a caller's line out of
/// it (`docs/architecture/HTTPS-MESH-API.md` "Charters": "it rides pairing's
/// ceremony and its local-network guard, never a relay, and it creates no
/// pairwise record").
fn handle_charter_join(inv: &Invocation) -> Outcome {
    let cmd = "mesh.join";
    let mesh = inv.args[0].trim();
    let host_arg = inv.args.get(1).map(|s| s.trim()).filter(|s| !s.is_empty());
    let operator_flag = inv.flags.get("operator").cloned().filter(|s| !s.is_empty());
    if host_arg.is_some() == operator_flag.is_some() {
        let both = host_arg.is_some();
        return Outcome::refuse(
            cmd,
            Kind::Usage,
            if both { "a join takes a LAN address or `--operator`, not both" } else { "a join needs a source of trust" },
            "trust enters a mesh one of two ways: the operator's key typed with `--operator`, or fetched from the operator's machine on the local network",
            Fix::Run(if both {
                teach::edited(inv, |i| {
                    i.args.truncate(1);
                })
            } else {
                teach::edited(inv, |i| {
                    i.flags.insert("operator".to_string(), "ed25519:<hex>".to_string());
                })
            }),
        );
    }

    // The LAN arm. The address is checked HERE, on the dial side, because the
    // ceremony is a local-network one by definition: a relay, the HTTPS
    // adapter, a tailnet and an ssh tunnel are all refused before anything is
    // posted, so a join never travels further than the LAN it is meant for.
    let replace = inv.flag_present("replace");
    let (operator_key, replaced, seen, charter_pair) = match (&operator_flag, host_arg) {
        // The `--operator` arm has nothing to compare: the typed flag IS the
        // decision, so it passes no `seen` and skips the change check (the
        // write is still locked, so two joins cannot interleave).
        (Some(field), _) => match charter::trust_operator_with(mesh, field, replace) {
            Ok((key, replaced)) => (key, replaced, None, None),
            Err(e) => return trust_refused(cmd, mesh, e),
        },
        (None, Some(host)) => {
            let (addr, port) = split_host_port(host);
            let resolved = match addr.parse::<std::net::IpAddr>() {
                Ok(ip) => ip.to_string(),
                Err(_) => match resolve_host_to_ip(&addr) {
                    Some(ip) => ip.to_string(),
                    None => {
                        return Outcome::refuse(
                            cmd,
                            Kind::Refused,
                            format!("`{host}` does not resolve to an address"),
                            "the LAN join checks the address before dialling, and this name has none here",
                            Fix::Run(teach::edited(inv, |i| {
                                i.args.truncate(1);
                                i.flags.insert("operator".to_string(), "ed25519:<hex>".to_string());
                            })),
                        )
                        .with_fields(json!({ "reason": "unresolved-host", "host": host }))
                    }
                },
            };
            if !charter::is_local_network(&resolved) {
                return Outcome::refuse(
                    cmd,
                    Kind::Refused,
                    format!("`{host}` resolves to {resolved}, which is not a local-network address"),
                    "a LAN join is admitted only over a private or link-local address, never loopback, because a relayed or tunneled request arrives from loopback; take the mesh without a LAN with `--operator`, or `aoide mesh charter accept <file>` with the file the operator hands you",
                    Fix::Run(teach::edited(inv, |i| {
                        i.args.truncate(1);
                        i.flags.insert("operator".to_string(), "ed25519:<hex>".to_string());
                    })),
                )
                .with_fields(json!({ "reason": "not-local-network", "host": host, "address": resolved }));
            }
            let url = format!("http://{resolved}:{port}/");
            let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/charterFetch", "params": { "mesh": mesh } });
            teach::progress(inv, &format!("asking {url} for mesh `{mesh}` (up to 15s)…"));
            let rerun = teach::line(inv);
            let (code, body_text) = match crate::commands::post_json_via(&url, None, "", &body.to_string(), None, &[], 15) {
                Ok(v) => v,
                Err(e) => return teach::far_door(cmd, &format!("asking for mesh `{mesh}`"), &url, &e, "unreachable", &rerun),
            };
            if code != 200 {
                return teach::far_door(cmd, &format!("asking for mesh `{mesh}`"), &url, &format!("HTTP {code} with body {body_text}"), "fetch-http-error", &rerun)
                    .with_fields(json!({ "httpCode": code }));
            }
            let parsed: serde_json::Value = match serde_json::from_str(&body_text) {
                Ok(v) => v,
                Err(e) => return teach::far_door(cmd, &format!("reading the answer for mesh `{mesh}`"), &url, &e.to_string(), "unparseable", &rerun),
            };
            if let Some(err) = parsed.get("error") {
                return Outcome::refuse(
                    cmd,
                    Kind::Refused,
                    format!("`{host}` refused the join"),
                    err.get("message").and_then(|m| m.as_str()).map(str::to_string).unwrap_or_else(|| err.to_string()),
                    Fix::Run(format!("aoide mesh charter show {mesh}")),
                )
                .with_fields(json!({ "reason": "refused", "host": host, "error": err }));
            }
            let result = &parsed["result"];
            let key = result["operator"].as_str().unwrap_or_default().to_string();
            let bytes = match base64_decode(result["charter"].as_str().unwrap_or_default()) {
                Ok(b) => b,
                Err(e) => return teach::far_door(cmd, "reading the charter body", &url, &e, "undecodable", &rerun),
            };
            let sig = match base64_decode(result["sig"].as_str().unwrap_or_default()) {
                Ok(b) => b,
                Err(e) => return teach::far_door(cmd, "reading the charter signature", &url, &e, "undecodable", &rerun),
            };
            let version = result["version"].as_u64().unwrap_or(0);
            // The fingerprint is the trust step: printed for the operator to
            // compare with what the operator's machine shows, before anything
            // is recorded. This is a first-use ceremony, and its authority is
            // that comparison — never the transport, never the LAN guard.
            let fingerprint = charter::fingerprint_of_key(&key);
            // **A REPLACEMENT is always confirmed** (re-review N4): `--yes`
            // skips a first-use comparison, where the worst outcome is taking a
            // key nobody checked, but when this machine ALREADY trusts a
            // different key the same command DESTROYS that record. The
            // fingerprint is the only trust step on this arm, so replacing a
            // mesh's root with a key nobody compared is the one thing the flag
            // may not skip.
            let will_replace = replace
                && charter::load_trust(mesh)
                    .ok()
                    .flatten()
                    .is_some_and(|trust| !trust.operator.is_empty() && trust.operator != key);
            // The record as the human saw it — handed to the write, which
            // re-reads it under the mesh's lock and refuses if it moved
            // (confirm finding 3: the decision and the write must see the same
            // record). `Some("")` is "no record when I looked".
            let seen = charter::load_trust(mesh).ok().flatten().map(|t| t.operator).unwrap_or_default();
            if join_needs_confirm(inv.flag_present("yes"), will_replace) {
                match confirm_join(mesh, &fingerprint, version, will_replace) {
                    JoinConfirm::Confirmed => {}
                    JoinConfirm::Declined => {
                        // A required comparison that is DECLINED is a refusal:
                        // non-zero, with what it means. An optional one keeps
                        // the old "nothing recorded" success.
                        if will_replace {
                            return Outcome::refuse(
                                cmd,
                                Kind::Refused,
                                format!("joining mesh `{mesh}` with `--replace` was declined"),
                                "it REPLACES the operator key this machine already trusts, so the fingerprint comparison is required and was declined; nothing was written",
                                Fix::Run(teach::line(inv)),
                            )
                            .with_fields(json!({ "reason": "confirm-declined", "mesh": mesh, "willReplace": will_replace }));
                        }
                        return Outcome::ok(cmd, "not confirmed — nothing recorded".to_string())
                            .with_data(json!({ "confirmed": false, "mesh": mesh }));
                    }
                    JoinConfirm::NoTty => {
                        if will_replace {
                            return Outcome::refuse(
                                cmd,
                                Kind::Refused,
                                format!("joining mesh `{mesh}` with `--replace` needs the fingerprint compared by a person"),
                                format!("this is not an interactive terminal; compare {fingerprint} with the operator's machine where you can, or name the new key outright; nothing was written"),
                                Fix::Run(format!("aoide mesh join {mesh} --operator ed25519:<hex> --replace")),
                            )
                            .with_fields(json!({ "reason": "confirm-needs-tty", "mesh": mesh, "willReplace": true }));
                        }
                        return Outcome::ok(cmd, "not confirmed — nothing interactive, nothing recorded".to_string())
                            .with_data(json!({ "confirmed": false, "mesh": mesh }));
                    }
                }
            }
            (key, None, Some(seen), Some((bytes, sig)))
        }
        (None, None) => unreachable!("a join names exactly one source of trust, checked above"),
    };

    // Record the trust FIRST and only then apply: `accept` reads the operator
    // key it must verify under, so a charter applied before its key is
    // recorded would be refused `unknown-operator` by this very machine. On
    // the non-LAN arm this already happened (`trust_operator_with` above); the
    // LAN arm reaches it here, AFTER the operator compared the fingerprint, so
    // a replacement is never recorded before a human saw it — and it passes the
    // record the human SAW, because the write re-reads it under the mesh's lock
    // and refuses if it moved since (confirm finding 3).
    let field = format!("ed25519:{operator_key}");
    let (trusted, replaced_here) = match charter::trust_operator_confirmed(mesh, &field, replace, seen.as_deref()) {
        Ok((k, replaced)) => (k, replaced),
        Err(e) => return trust_refused(cmd, mesh, e),
    };
    let replaced = replaced.or(replaced_here);

    // Replacing a mesh's root is the event an operator has to be able to find
    // afterwards, so both fingerprints are audited — the one that was trusted
    // and the one that now is.
    if let Some(old) = &replaced {
        let _ = audit(
            &default_audit_log(),
            Door::Cli,
            EventClass::Audit,
            "mesh.join",
            "operator-replaced",
            &format!(
                "mesh `{mesh}`: operator {} replaced by {} (the version high-water carried across)",
                charter::fingerprint_of_key(old),
                charter::fingerprint_of_key(&trusted)
            ),
        );
    }

    let mut applied = serde_json::Value::Null;
    let mut message = format!(
        "joined mesh `{mesh}` — this machine now trusts operator {} (recorded in state, never in config.toml)",
        charter::fingerprint_of_key(&trusted)
    );
    if let Some(old) = &replaced {
        message.push_str(&format!(
            "\nREPLACED operator {} — the version high-water carried across, so the first charter the new key signs must still beat every version applied here",
            charter::fingerprint_of_key(old)
        ));
    }
    if let Some((bytes, sig)) = charter_pair {
        match charter::accept(&bytes, &sig) {
            Ok(accepted) => {
                message.push_str(&format!(
                    "\ncharter v{} applied here ({} node(s){})",
                    accepted.charter.version,
                    accepted.charter.nodes.len(),
                    rekey_note(&accepted.rekeyed),
                ));
                applied = json!({
                    "version": accepted.charter.version,
                    "nodes": accepted.charter.nodes.len(),
                    "rekeyed": accepted.rekeyed.iter().map(|r| json!({ "node": r.node, "from": r.from, "to": r.to })).collect::<Vec<_>>(),
                });
            }
            Err(refusal) => {
                message.push_str(&format!(
                    "\nthe operator key is recorded, but the charter it offered was refused ({}): {}",
                    refusal.reason, refusal.detail
                ));
            }
        }
    }
    Outcome::ok(cmd, message).with_data(json!({
        "mesh": mesh,
        "operator": trusted,
        "fingerprint": charter::fingerprint_of_key(&trusted),
        "charter": applied,
    }))
}

/// The trust record refused the key (exit 1): a different key is already trusted
/// for the mesh, or the key is no key. `why` is the record's own words.
fn trust_refused(cmd: &str, mesh: &str, why: String) -> Outcome {
    Outcome::refuse(
        cmd,
        Kind::Refused,
        format!("this machine did not take the operator key for mesh `{mesh}`"),
        why,
        Fix::Run(format!("aoide mesh charter show {mesh}")),
    )
    .with_fields(json!({ "reason": "trust-refused", "mesh": mesh }))
}

/// **Does this join need the operator's own comparison?** `--yes` skips it for
/// a FIRST-TIME trust (the worst outcome is taking a key nobody checked), but
/// never for a replacement: when this machine already trusts a different key
/// for the mesh, the same command destroys that record, and on the LAN arm the
/// fingerprint is the only trust step there is. Pure, so the rule is provable
/// without a tty.
fn join_needs_confirm(yes: bool, will_replace: bool) -> bool {
    will_replace || !yes
}

/// What the LAN arm's comparison came back with. **A required comparison on a
/// non-terminal is its own answer** (confirm finding 3): the first cut returned
/// `Ok "not confirmed"` (exit 0) for a scripted re-root, which reads as success
/// to a caller that only checks the status — a taught error is what a scripted
/// refusal owes its caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JoinConfirm {
    Confirmed,
    Declined,
    /// stdin is not an interactive terminal, so nobody could compare.
    NoTty,
}

/// The LAN arm's one human gate: the operator key's fingerprint, printed for
/// the operator to compare with what their own machine shows. Deliberately its
/// own six lines rather than a shared prompt helper — this crate's prompts
/// are each shaped by what their own arm asks, and `--yes` skips exactly this
/// one question and no other.
fn confirm_join(mesh: &str, fingerprint: &str, version: u64, will_replace: bool) -> JoinConfirm {
    use std::io::IsTerminal;
    let replacing = if will_replace {
        " This REPLACES the operator key this machine already trusts — compare it carefully."
    } else {
        ""
    };
    eprint!(
        "join mesh `{mesh}` with operator key {fingerprint} (charter v{version})? \
         Compare it with the fingerprint the operator's machine printed.{replacing} [y/N] "
    );
    let _ = std::io::Write::flush(&mut std::io::stderr());
    if !std::io::stdin().is_terminal() {
        return JoinConfirm::NoTty;
    }
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line).unwrap_or(0);
    if read > 0 && matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        JoinConfirm::Confirmed
    } else {
        JoinConfirm::Declined
    }
}

/// `host`, `host:port`, `[v6]:port` or a bare address → (address text, port).
/// The port defaults to `AOIDE_A2A_PORT`, else 8710, exactly as every other
/// LAN dial in this crate does.
fn split_host_port(host: &str) -> (String, u16) {
    let default_port = crate::commands::default_a2a_port();
    if let Some(rest) = host.strip_prefix('[') {
        if let Some((addr, tail)) = rest.split_once(']') {
            let port = tail.strip_prefix(':').and_then(|p| p.parse().ok()).unwrap_or(default_port);
            return (addr.to_string(), port);
        }
    }
    match host.rsplit_once(':') {
        Some((addr, port)) if !addr.contains(':') && port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => {
            (addr.to_string(), port.parse().unwrap_or(default_port))
        }
        _ => (host.to_string(), default_port),
    }
}

/// Resolve a hostname to one address, without pulling a resolver dependency:
/// `getent ahostsv4`/`ahostsv6` where they exist (glibc and musl both ship
/// `getent`), and no answer at all otherwise — the join then tells the operator
/// to name the machine by IP rather than guessing. Deliberately NOT a second
/// transport: it is one `getent` call feeding the ADDRESS CHECK, and the dial
/// itself still goes to the literal address this returns.
fn resolve_host_to_ip(host: &str) -> Option<String> {
    for family in ["ahostsv4", "ahostsv6"] {
        let out = std::process::Command::new("getent").arg(family).arg(host).output().ok()?;
        if !out.status.success() {
            continue;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(first) = text.split_whitespace().next() {
            if first.parse::<std::net::IpAddr>().is_ok() {
                return Some(first.to_string());
            }
        }
    }
    None
}

/// `aoide/charterFetch`'s decoder — the exact inverse of the door's
/// `base64_bytes`, total (a malformed body is an `Err` the join prints, never
/// a panic, and never a partially-trusted blob).
fn base64_decode(text: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    for (i, c) in text.trim().bytes().enumerate() {
        if c == b'=' {
            break;
        }
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'\n' | b'\r' => continue,
            _ => return Err(format!("not base64 at byte {i}: {}", c as char)),
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

/// `aoide mesh charter show [<mesh>]` — the charter in force and its status.
fn handle_charter_show(inv: &Invocation) -> Outcome {
    let cmd = "mesh.charter.show";
    let named = inv.args.first().map(|s| s.trim()).filter(|s| !s.is_empty()).map(str::to_string);
    let meshes: Vec<String> = match &named {
        Some(m) => vec![m.clone()],
        None => charter::meshes_with_state(),
    };
    let mut rows = Vec::new();
    for mesh in &meshes {
        let Some(in_force) = charter::in_force_charter(mesh) else {
            if named.is_none() {
                // **A charter-SHAPED mesh with no readable document is
                // reported, not skipped** (re-review, the F2 table's last row):
                // the mesh state exists (an operator key recorded by a `join`
                // that has not accepted anything, or a document that no longer
                // parses), the door refuses every request in it, and `mesh
                // --json` lists it — so this surface must not go quiet about
                // the very mesh an operator is trying to diagnose.
                if !charter::charter_shaped(mesh) {
                    continue;
                }
                // A row shaped like its siblings (confirm finding 5): a typed
                // `inForce` discriminator rather than a prose `note` the
                // renderer keys on, `trust` inside the SAME enumeration the
                // others use, and the `paths` block every other row carries.
                let trust = charter::load_trust(mesh).ok().flatten().unwrap_or_default();
                let declared_key = charter::config_operator(mesh).ok().flatten();
                let state_key = Some(trust.operator.clone()).filter(|k| !k.is_empty());
                rows.push(json!({
                    "mesh": mesh,
                    "inForce": false,
                    "version": 0,
                    "operatorKey": trust.operator,
                    "operator": charter::fingerprint_of_key(&trust.operator),
                    "trust": match (&declared_key, &state_key) {
                        (Some(_), Some(_)) => "both",
                        (Some(_), None) => "config",
                        (None, Some(_)) => "state",
                        (None, None) => "none",
                    },
                    "honoured": false,
                    "highWater": 0,
                    "relays": [],
                    "nodes": [],
                    "rekeyed": [],
                    "paths": {
                        "charter": charter::in_force_path(mesh).to_string_lossy(),
                        "trust": charter::trust_path(mesh).to_string_lossy(),
                        "source": charter::source_path(mesh).to_string_lossy(),
                    },
                }));
                continue;
            }
            let held = charter::meshes_with_state();
            if !held.is_empty() && aoide_protocol::suggest::closest(mesh, held.iter().map(String::as_str), 1).first().is_some() {
                return teach::unknown_name(inv, cmd, "mesh", teach::Slot::Arg(0), mesh, &held, "meshes with a charter here", "aoide mesh charter show", Fix::Run("aoide mesh charter show".to_string()));
            }
            return no_charter(cmd, mesh, Some(charter::mesh_state_dir(mesh).display().to_string()));
        };
        let trust = charter::load_trust(mesh).ok().flatten().unwrap_or_default();
        let declared = charter::config_operator(mesh).ok().flatten();
        let state_key = Some(trust.operator.clone()).filter(|k| !k.is_empty());
        let governed = charter::trusted_operator(mesh);
        let mut node_rows: Vec<serde_json::Value> = Vec::new();
        for (name, line) in &in_force.nodes {
            node_rows.push(json!({
                "name": name,
                "key": line.key,
                "fingerprint": charter::fingerprint_of_key(&line.key),
                "address": line.address,
                "grant": line.grant,
            }));
        }
        rows.push(json!({
            "mesh": mesh,
            "version": in_force.version,
            "operatorKey": trust.operator,
            "operator": charter::fingerprint_of_key(&trust.operator),
            "trust": match (&declared, &state_key) {
                (Some(_), Some(_)) => "both",
                (Some(_), None) => "config",
                (None, Some(_)) => "state",
                (None, None) => "none",
            },
            "honoured": governed.is_ok(),
            "highWater": trust.versions.get(&trust.operator).copied().unwrap_or(0),
            "relays": in_force.relays,
            "nodes": node_rows,
            "rekeyed": trust.rekeyed,
            "paths": {
                "charter": charter::in_force_path(mesh).to_string_lossy(),
                "trust": charter::trust_path(mesh).to_string_lossy(),
                "source": charter::source_path(mesh).to_string_lossy(),
            },
        }));
    }
    if rows.is_empty() {
        return no_charter(cmd, &aoide_storage::config::home_mesh(), None);
    }
    let mut lines = Vec::new();
    for row in &rows {
        let mesh = row["mesh"].as_str().unwrap_or("");
        if row["inForce"].as_bool() == Some(false) {
            lines.push(format!(
                "charter {} — NO charter document in force (operator {} ({}) recorded, none readable) — every request in this mesh is refused until it is resolved",
                mesh,
                row["operator"],
                row["operatorKey"],
            ));
            continue;
        }
        lines.push(format!(
            "charter {} v{} — operator {} ({}) — trust: {}, {} — high-water v{}",
            mesh,
            row["version"],
            row["operator"],
            row["operatorKey"],
            row["trust"],
            if row["honoured"].as_bool().unwrap_or(false) {
                "honoured here".to_string()
            } else {
                "NOT HONOURED (the operator key is undecidable — resolve `operator-mismatch`)".to_string()
            },
            row["highWater"],
        ));
        if let Some(relays) = row["relays"].as_array().filter(|r| !r.is_empty()) {
            lines.push(format!(
                "  relays: {}",
                relays.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
        for node in row["nodes"].as_array().into_iter().flatten() {
            lines.push(format!(
                "  {}: {}  address {}  grant {}",
                node["name"],
                node["fingerprint"],
                node["address"],
                node["grant"].as_array().map(|g| g.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(",")).unwrap_or_default(),
            ));
        }
        for r in row["rekeyed"].as_array().into_iter().flatten() {
            lines.push(format!(
                "  RE-KEYED {}: {} -> {} (v{}, {})",
                r["node"], r["from"], r["to"], r["version"], r["at"]
            ));
        }
    }
    Outcome::ok(cmd, lines.join("\n")).with_data(json!({ "meshes": rows }))
}

/// No charter is in force here (exit 1): a world state, not a usage mistake.
/// The fix takes a charter by hand; joining the mesh is the other way in.
fn no_charter(cmd: &str, mesh: &str, state_dir: Option<String>) -> Outcome {
    let where_ = match &state_dir {
        Some(dir) => format!("its state would be `{dir}/charter.toml`, written by `charter accept` or by a charter letter"),
        None => "no mesh has a charter in force at this node".to_string(),
    };
    Outcome::refuse(
        cmd,
        Kind::Refused,
        format!("no charter in force for mesh `{mesh}` at this node"),
        format!("{where_}; a machine takes one from the operator's signed file, or joins the mesh with `aoide mesh join {mesh} --operator ed25519:<hex>`"),
        Fix::Run(format!("aoide mesh charter accept ./{mesh}.toml")),
    )
    .with_fields(json!({ "reason": "no-charter", "mesh": mesh, "meshes": [] }))
}

fn handle_charter_init(inv: &Invocation) -> Outcome {
    let cmd = "mesh.charter.init";
    let mesh = inv.args[0].trim();
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
        Err(e) => Outcome::refuse(
            cmd,
            Kind::Refused,
            format!("could not root mesh `{mesh}`"),
            e,
            Fix::Run(format!("aoide mesh charter show {mesh}")),
        )
        .with_fields(json!({ "reason": "init-failed" })),
    }
}

/// A mesh named for `sign`/`reroot` that this machine holds no state for, when one
/// that does is a typo away (exit 1). A mesh with no state and no near name is
/// left to the operation's own refusal, which says what is missing.
fn near_mesh(inv: &Invocation, cmd: &str, typed: &str) -> Option<Outcome> {
    let held = charter::meshes_with_state();
    if held.iter().any(|m| m == typed) {
        return None;
    }
    aoide_protocol::suggest::closest(typed, held.iter().map(String::as_str), 1).first()?;
    Some(teach::unknown_name(inv, cmd, "mesh", teach::Slot::Arg(0), typed, &held, "meshes with state here", "aoide mesh charter show", Fix::Run("aoide mesh charter show".to_string())))
}

fn handle_charter_sign(inv: &Invocation) -> Outcome {
    let cmd = "mesh.charter.sign";
    let mesh = inv.args[0].trim();
    if let Some(out) = near_mesh(inv, cmd, mesh) {
        return out;
    }
    let file = inv
        .flags
        .get("file")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    match charter::sign(mesh, file.as_deref()) {
        Ok(signed) => {
            let delivery = drain_spooled(&signed.mesh, &signed.spooled);
            Outcome::ok(
                cmd,
                format!(
                    "signed {} v{} (operator {}) — applied here, spooled to {} node(s){}",
                    signed.mesh,
                    signed.version,
                    signed.fingerprint,
                    signed.spooled.len(),
                    rekey_note(&signed.rekeyed),
                ) + &render_spooled(&delivery),
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
        Err(e) => Outcome::refuse(
            cmd,
            Kind::Refused,
            format!("could not sign the charter for mesh `{mesh}`"),
            e,
            Fix::Run(format!("aoide mesh charter show {mesh}")),
        )
        .with_fields(json!({ "reason": "sign-failed" })),
    }
}

fn handle_charter_accept(inv: &Invocation) -> Outcome {
    let cmd = "mesh.charter.accept";
    let file = inv.args[0].trim();
    let path = Path::new(file);
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => {
            let c = aoide_protocol::output::io_cause("read", path, &e);
            return Outcome::refuse(
                cmd,
                Kind::Refused,
                "the charter file cannot be read",
                c.why,
                Fix::Set("the path of the charter file the operator handed you".to_string()),
            )
            .with_fields(json!({ "reason": "unreadable-charter" }))
            .with_detail(c.detail);
        }
    };
    let sig_path = charter::sig_path_for(path);
    let sig = match std::fs::read(&sig_path) {
        Ok(sig) => sig,
        Err(e) => {
            let c = aoide_protocol::output::io_cause("read", &sig_path, &e);
            return Outcome::refuse(
                cmd,
                Kind::Refused,
                "the charter's signature cannot be read",
                format!("{}; a signed charter travels as a pair, and the detached signature must sit beside it", c.why),
                Fix::Set(format!("`{}` beside the charter file", sig_path.display())),
            )
            .with_fields(json!({ "reason": "unreadable-signature" }))
            .with_detail(c.detail);
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
        Err(r) => {
            let fix = match r.reason.as_str() {
                "stale-charter" => Fix::Wait("for the operator to sign a newer version of the charter".to_string()),
                "charter-tampered" => Fix::Wait("for the operator to send the charter and its signature again".to_string()),
                _ => Fix::Run("aoide mesh charter show".to_string()),
            };
            Outcome::refuse(cmd, Kind::Refused, format!("the charter was refused: {}", r.reason), r.detail.clone(), fix)
                .with_fields(json!({ "reason": r.reason, "detail": r.detail }))
        }
    }
}

fn handle_charter_reroot(inv: &Invocation) -> Outcome {
    let cmd = "mesh.charter.reroot";
    let mesh = inv.args[0].trim();
    if let Some(out) = near_mesh(inv, cmd, mesh) {
        return out;
    }
    match charter::reroot(mesh) {
        Ok(signed) => {
            let delivery = drain_spooled(&signed.mesh, &signed.spooled);
            Outcome::ok(
                cmd,
                format!(
                    "mesh `{}` re-rooted and signed v{} under the new key. Every other machine keeps working on its last charter until it takes the new key the way it took the first — its own trust-entry step: the same line in `[mesh.{}]`, or, on a host whose config is not hand-editable, `aoide mesh join {} --operator <new key> --replace` (`--replace` is what tells that command the change is the OPERATOR'S, and it carries this mesh's version high-water across, so the new key's first charter must still beat every version applied here; without it that command refuses a different key, and a config line changed ALONE stops on `operator-mismatch` until the record follows). Never edit `state/mesh/{}/trust.json` — that is where the version mark lives.\n  {}\n  # operator fingerprint {}",
                    signed.mesh,
                    signed.version,
                    signed.mesh,
                    signed.mesh,
                    signed.mesh,
                    charter::operator_line(&signed.operator),
                    signed.fingerprint,
                ) + &render_spooled(&delivery),
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
        Err(e) => Outcome::refuse(
            cmd,
            Kind::Refused,
            format!("could not re-root mesh `{mesh}`"),
            e,
            Fix::Run(format!("aoide mesh charter show {mesh}")),
        )
        .with_fields(json!({ "reason": "reroot-failed" })),
    }
}

/// Drain what `sign`/`reroot` spooled, one node at a time, best-effort, and
/// report each one HONESTLY.
///
/// **A charter node this box holds no record for is not a drained node**
/// (review N2). `drain_node` returns `Ok(())` for a name it has no
/// `state/nodes.json` record for — correctly, since it has nothing to dial —
/// and the first cut of this function mapped that `Ok` to `drained: true`,
/// telling the operator a letter had gone out to a machine that was never
/// contacted. A charter `address` IS a dial target now (`dial_node` turns it
/// into one, a pairing or a declaration alike), so the case left here is a name
/// NO declaration addresses and NO record names. So the
/// entry STAYS in the spool and this says so: `drained: false`, with a reason
/// naming the two ways it will leave — a pairing that gives the node a record,
/// or a declaration that addresses it.
///
/// A node WITH a record is unchanged: dialled, and its own `Err` reported —
/// unless the mesh being signed declares it `down`, which is its own answer
/// before any record is consulted ([`declaration_forbids_dial`]): never dialled,
/// and the
/// entry KEPT in the spool, since `down` stops SENDING and never confiscates.
fn drain_spooled(mesh: &str, spooled: &[String]) -> Vec<serde_json::Value> {
    let known: std::collections::BTreeMap<String, bool> = aoide_storage::node_store::load_nodes()
        .into_iter()
        .map(|node| (node.name.clone(), node.never_dialled()))
        .collect();
    spooled
        .iter()
        .map(|node| {
            // **A mesh whose declaration cannot be read is never dialled, and
            // that is its own answer** — a `down` node is a node the mesh DID
            // declare, and saying so about a mesh nobody can read would be a
            // false word. Nothing is dialled either way (fail closed).
            if crate::mail_wire::declaration_unreadable(mesh) {
                return json!({
                    "node": node,
                    "drained": false,
                    "reason": "declaration-unreadable",
                    "detail": "not dialled: this mesh's declaration cannot be read here, and nothing dials a node its own mesh cannot vouch for",
                });
            }
            // **A `down` node is never dialled, and the entry stays.** Asked
            // before the record half below: `down` is a fact about the mesh's
            // declaration, and a node with no record is still `down`.
            if crate::mail_wire::declaration_forbids_dial(mesh, node) {
                return json!({
                    "node": node,
                    "drained": false,
                    "reason": "down",
                    "detail": "not dialled: down — the declaration says this box stops sending to it, so nothing was dialled and the entry is kept in the spool until that changes",
                });
            }
            let Some(poll_only) = known.get(node) else {
                return json!({
                    "node": node,
                    "drained": false,
                    "reason": "no-record",
                    "detail": "this box holds no node record for it, so nothing was dialled — the entry waits in the spool until it is paired (giving it a record) or a declaration addresses it",
                });
            };
            // **A `poll` node is never dialled either** (confirm finding 6):
            // `drain_node` returns `Ok(())` for it exactly as it does for an
            // unknown name, so mapping that to `drained: true` told the same
            // lie one arm over. Its held entry leaves through the FAR end's own
            // `aoide mail poll`, which is worth saying out loud.
            if *poll_only {
                return json!({
                    "node": node,
                    "drained": false,
                    "reason": "poll-only",
                    "detail": "this node's address is `poll`, so this box never dials it — the entry waits for the far end's own `aoide mail poll`",
                });
            }
            match crate::mail_wire::drain_node(node) {
                Ok(()) => json!({ "node": node, "drained": true }),
                Err(e) => json!({ "node": node, "drained": false, "error": e }),
            }
        })
        .collect()
}

/// The human rendering of [`drain_spooled`]'s rows — one line per node, and
/// `not dialled` for the record-less case rather than silence (review N2: the
/// command's message has to say what its `--json` says).
fn render_spooled(rows: &[serde_json::Value]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let mut lines = vec![format!("\nspooled to {} node(s):", rows.len())];
    for row in rows {
        let node = row["node"].as_str().unwrap_or("");
        if row["drained"].as_bool() == Some(true) {
            lines.push(format!("  {node}: drained now"));
        } else if row["reason"].as_str() == Some("no-record") {
            lines.push(format!(
                "  {node}: NOT DIALLED — no node record here (the entry waits in the spool until a pairing or a declaration addresses it)"
            ));
        } else if row["reason"].as_str() == Some("declaration-unreadable") {
            lines.push(format!(
                "  {node}: NOT DIALLED — this mesh's declaration cannot be read here (nothing dials a node its own mesh cannot vouch for)"
            ));
        } else if row["reason"].as_str() == Some("down") {
            lines.push(format!(
                "  {node}: NOT DIALLED — declared `down` (the entry waits in the spool until the declaration changes)"
            ));
        } else if row["reason"].as_str() == Some("poll-only") {
            lines.push(format!(
                "  {node}: NOT DIALLED — a `poll` node is never dialled (the entry waits for its own `aoide mail poll`)"
            ));
        } else {
            lines.push(format!(
                "  {node}: queued — {}",
                row["error"].as_str().unwrap_or("the drain did not complete")
            ));
        }
    }
    lines.join("\n")
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

#[cfg(test)]
mod tests {
    use super::*;
    use aoide_protocol::Door;

    /// Sandboxes the handler's two reads (`AOIDE_ROOT` for the charter state,
    /// `AOIDE_STATE_DIR` for the node registry) at one scratch dir — the same
    /// shape `crate::mesh`'s own tests use.
    fn with_root<T>(tag: &str, f: impl FnOnce(&Path) -> T) -> T {
        let _guard = crate::env_lock().lock().unwrap_or_else(|e| e.into_inner());
        let _saver = aoide_test_support::EnvSaver::capture(&["AOIDE_ROOT", "AOIDE_STATE_DIR", "AOIDE_CONFIG"]);
        let dir = aoide_test_support::unique_tmp(&format!("charter-show-{tag}"));
        std::env::set_var("AOIDE_ROOT", &dir);
        std::env::set_var("AOIDE_STATE_DIR", &dir);
        std::env::remove_var("AOIDE_CONFIG");
        let out = f(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        out
    }

    fn inv(args: &[&str]) -> Invocation {
        Invocation {
            path: ["mesh", "charter", "show"].iter().map(|s| s.to_string()).collect(),
            args: args.iter().map(|s| s.to_string()).collect(),
            flags: Default::default(),
            door: Door::Cli,
        }
    }

    /// **A replacement always asks** (re-review N4): `--yes` skips a
    /// first-time comparison, never a destructive one. The predicate is pure so
    /// the rule is provable without a tty.
    #[test]
    fn a_join_confirms_always_when_it_would_replace_and_otherwise_only_without_yes() {
        assert!(join_needs_confirm(false, false), "no --yes: first-use still asks");
        assert!(!join_needs_confirm(true, false), "--yes skips a first-use comparison");
        assert!(
            join_needs_confirm(true, true),
            "--yes may NOT skip the comparison when the join REPLACES the operator key this machine trusts"
        );
        assert!(join_needs_confirm(false, true));
    }

    /// **The read side of the charter** (P-CHARTER, surfaces slice):
    /// `mesh charter show` prints the charter in force with its status — the
    /// version, the operator key and fingerprint, where this node's trust is
    /// written, the high-water mark, and every node line — and answers a
    /// taught `no-charter` for a mesh that has none, rather than inventing a
    /// status for a charter nobody signed.
    #[test]
    fn charter_show_prints_the_charter_in_force_and_refuses_an_unknown_mesh() {
        with_root("in-force", |_dir| {
            let init = charter::init("home").unwrap();
            let line = charter::node_line().unwrap();
            let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{line}\n");
            std::fs::write(charter::source_path("home"), &src).unwrap();
            charter::sign("home", None).unwrap();

            // No argument: every mesh with a charter on disk.
            let out = handle_charter_show(&inv(&[]));
            assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{out:?}");
            let row = &out.data.as_ref().unwrap()["meshes"][0];
            assert_eq!(row["mesh"], "home");
            assert_eq!(row["version"], 1);
            assert_eq!(row["operatorKey"], init.operator);
            assert_eq!(row["operator"], init.fingerprint);
            assert_eq!(row["trust"], "state", "`init` recorded the key in state, and no config line exists");
            assert_eq!(row["honoured"], true);
            assert_eq!(row["highWater"], 1);
            assert_eq!(row["nodes"][0]["grant"][0], "message", "no grant key on a line means `message`");
            assert!(out.message.contains("high-water v1"), "{}", out.message);
            assert!(out.message.contains(&init.fingerprint), "{}", out.message);

            // A mesh with no charter is a taught refusal, not an empty report.
            let missing = handle_charter_show(&inv(&["away"]));
            assert_eq!(missing.status, aoide_protocol::output::Status::Error, "{missing:?}");
            assert_eq!(missing.data.as_ref().unwrap()["reason"], "no-charter");
        });
    }

    /// **No charter here is the world's answer, not a usage mistake**: exit 1,
    /// with a fix that takes one by hand, for a named mesh and for a bare `show`.
    #[test]
    fn show_with_no_charter_is_a_refused_world_state_with_a_fix_that_runs() {
        with_root("show-none", |_dir| {
            for args in [&["home"][..], &[][..]] {
                let out = handle_charter_show(&inv(args));
                assert_eq!(out.status, aoide_protocol::output::Status::Error, "{out:?}");
                assert_eq!(out.render(false).1, 1);
                let refusal = &out.data.as_ref().unwrap()["refusal"];
                assert_eq!(refusal["kind"], "refused");
                assert_eq!(refusal["fix"]["run"], "aoide mesh charter accept ./home.toml");
                assert!(refusal["why"].as_str().unwrap().contains("aoide mesh join home --operator"), "{refusal:?}");
            }
        });
    }

    /// A mesh typed a letter off a mesh this machine holds is named, not guessed.
    #[test]
    fn a_mistyped_mesh_for_sign_names_the_one_that_exists() {
        with_root("sign-typo", |_dir| {
            charter::init("home").unwrap();
            let mut typed = inv(&["hom"]);
            typed.path = ["mesh", "charter", "sign"].iter().map(|s| s.to_string()).collect();
            let out = handle_charter_sign(&typed);
            assert_eq!(out.status, aoide_protocol::output::Status::Error, "{out:?}");
            assert_eq!(out.data.as_ref().unwrap()["refusal"]["fix"]["run"], "aoide mesh charter sign home");
        });
    }

    /// **A charter-SHAPED mesh with no readable document is reported, and its
    /// row is shaped like its siblings** (confirm finding 5): the 1197c88 row
    /// shipped untested, with a prose `note` the renderer keyed on and a fifth
    /// value in `trust` — so any future row carrying a `note` would silently
    /// take that branch. The discriminator is now the typed `inForce: false`,
    /// `trust` is the same enumeration the other rows use, and the row carries
    /// the `paths` block every other one does.
    #[test]
    fn a_shaped_mesh_with_no_document_is_reported_as_its_own_row() {
        with_root("show-shaped-no-doc", |_dir| {
            // A trust record and NO charter document: the `join`-before-accept
            // shape (and what a corrupt document leaves behind).
            aoide_storage::charter::trust_operator("home", &format!("ed25519:{}", "cd".repeat(32))).unwrap();
            assert!(charter::charter_shaped("home"));
            assert!(charter::in_force_charter("home").is_none(), "no document");

            let out = handle_charter_show(&inv(&[]));
            assert_eq!(out.status, aoide_protocol::output::Status::Ok, "{out:?}");
            let row = &out.data.as_ref().unwrap()["meshes"][0];
            assert_eq!(row["mesh"], "home");
            assert_eq!(row["inForce"], false);
            assert_eq!(row["version"], 0);
            assert_eq!(row["trust"], "state", "the same enumeration the other rows use, not a prose pointer");
            assert_eq!(row["honoured"], false);
            assert!(row.get("note").is_none(), "no prose discriminator: {row}");
            assert!(row["paths"]["trust"].is_string(), "and the paths block every sibling carries: {row}");
            assert!(out.message.contains("NO charter document in force"), "{}", out.message);
            assert!(out.message.contains("refused"), "and says what it means: {}", out.message);
        });
    }

    /// **A `down` node is NOT dialled, and its entry is KEPT.** This arm comes
    /// before the record half on purpose: a node with no `nodes.json` record can
    /// be `down` too, and "not dialled, declared `down`" is a different fact from
    /// "no record here" — the operator acting on the first must change a
    /// declaration, on the second must pair.
    #[test]
    fn a_down_node_is_reported_as_not_dialled() {
        with_root("drain-down", |_dir| {
            let _ = charter::init("home").unwrap();
            let me = aoide_storage::display::local_node_name();
            let line = charter::node_line().unwrap();
            let src = format!(
                "mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{line}\n\n[status]\n{me} = \"down\"\n"
            );
            std::fs::write(charter::source_path("home"), &src).unwrap();
            charter::sign("home", None).unwrap();

            let rows = drain_spooled("home", &[me.clone()]);
            assert_eq!(rows[0]["node"], me);
            assert_eq!(rows[0]["drained"], false, "nothing was dialled: {rows:?}");
            assert_eq!(rows[0]["reason"], "down", "{rows:?}");
            assert!(
                rows[0]["detail"].as_str().unwrap().contains("not dialled: down"),
                "and the answer is the one the task asks for: {rows:?}"
            );
            let text = render_spooled(&rows);
            assert!(text.contains("NOT DIALLED") && text.contains("down"), "{text}");
        });
    }

    /// **A charter node this box has no record for is NOT reported as
    /// drained** (re-review N2). `drain_node` returns `Ok(())` for a name it
    /// cannot dial — correctly, there is nothing to dial — and the sign
    /// command used to map that to `drained: true`, telling the operator a
    /// letter had gone to a machine that was never contacted. A charter
    /// `address` IS a route now, so what is left for this case is a name no
    /// declaration addresses and no record names: the honest answer is
    /// "not dialled, the entry waits".
    #[test]
    fn a_recordless_charter_node_is_reported_as_not_dialled() {
        with_root("drain-no-record", |_dir| {
            let init = charter::init("home").unwrap();
            let line = charter::node_line().unwrap();
            let src = format!("mesh = \"home\"\nversion = 0\nrelays = []\n\n[nodes]\n{line}\n");
            std::fs::write(charter::source_path("home"), &src).unwrap();
            // SIGNED, so the mesh is readable and the case this test is about —
            // a name no record, and no declaration, names — is the one answered.
            charter::sign("home", None).unwrap();
            // `sign` spools one `charter` letter per OTHER charter node — and
            // there is none here, so drive the reporter directly with a name
            // this box holds no record for (the LAN/`--operator` join case).
            let _ = init;
            let rows = drain_spooled("home", &["laptop".to_string()]);
            assert_eq!(rows[0]["node"], "laptop");
            assert_eq!(
                rows[0]["drained"],
                false,
                "nothing was dialled, so nothing may claim to be drained: {rows:?}"
            );
            assert_eq!(rows[0]["reason"], "no-record");
            assert!(
                rows[0]["detail"].as_str().unwrap().contains("paired"),
                "and the reason names how it leaves: {rows:?}"
            );
            let text = render_spooled(&rows);
            assert!(text.contains("NOT DIALLED"), "{text}");
            assert!(text.contains("pairing"), "{text}");

            // A node WITH a record keeps the old answer: nothing to send, but
            // the drain is a real (empty) pass over a real record.
            let (kp, _) = aoide_storage::identity::load_or_mint().unwrap();
            let mut nodes = aoide_storage::node_store::load_nodes();
            aoide_storage::node_store::upsert_paired_node(
                &mut nodes,
                "peerbox",
                "http://127.0.0.1:1/",
                &kp.info().pubkey_hex,
                &aoide_storage::time::now_iso_utc(),
                &["message".to_string()],
                "home",
            );
            aoide_storage::node_store::save_nodes(&nodes).unwrap();
            let rows = drain_spooled("home", &["peerbox".to_string()]);
            assert_eq!(rows[0]["drained"], true, "a dialled node still answers for itself: {rows:?}");

            // And a node WITH a record whose address is `poll` is its own
            // answer (confirm finding 6): `drain_node` no-ops on it exactly as
            // it does for an unknown name, and claiming `drained: true` there
            // was the same lie one arm over.
            let mut nodes = aoide_storage::node_store::load_nodes();
            let mut poll_only = aoide_storage::node_store::Node {
                url: aoide_storage::charter::DEFAULT_ADDRESS.to_string(),
                ..aoide_storage::node_store::Node {
                    name: "laptop".to_string(),
                    url: String::new(),
                    autogate: false,
                    token_file: None,
                    bearer_secret: None,
                    hub: false,
                    pubkey: Some(kp.info().pubkey_hex.clone()),
                    verified: true,
                    grants: aoide_storage::node_store::Grants::new(),
                    narrowed: aoide_storage::node_store::Grants::new(),
                    via: None,
                    added_at: aoide_storage::time::now_iso_utc(),
                }
            };
            poll_only.grants = aoide_storage::node_store::grants_in("home", &["message"]);
            nodes.push(poll_only);
            aoide_storage::node_store::save_nodes(&nodes).unwrap();
            let rows = drain_spooled("home", &["laptop".to_string()]);
            assert_eq!(rows[0]["drained"], false, "{rows:?}");
            assert_eq!(rows[0]["reason"], "poll-only");
            assert!(
                rows[0]["detail"].as_str().unwrap().contains("never dials it"),
                "{rows:?}"
            );
            let text = render_spooled(&rows);
            assert!(text.contains("a `poll` node is never dialled"), "{text}");
        });
    }
}
