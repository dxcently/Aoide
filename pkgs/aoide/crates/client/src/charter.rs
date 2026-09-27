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
    r.insert(cmd!(
        path: ["mesh", "charter", "show"],
        summary: "Print the charter IN FORCE at this node and its status: the version, the operator key and its durable fingerprint, where this node's trust in that key is written (config line, state record, both, or neither), whether the key is decidable (a config/state disagreement is `operator-mismatch`, and then nothing about the mesh is honoured), the high-water version applied under that key, every node line the charter carries, and the re-keyed nodes the last applied version changed — a charter applied by an unattended poll has to be visible afterwards. Omitted <mesh>: every mesh with a charter on disk.",
        args: [arg!("mesh", "string", false, "The mesh whose charter to print. Omitted: every mesh with a charter in force here.")],
        flags: [],
        gated: false,
        implemented: true,
        handler: handle_charter_show,
        examples: ["mesh charter show", "mesh charter show home", "mesh charter show home --json"],
    ));
    r.insert(cmd!(
        path: ["mesh", "join"],
        summary: "Enter a mesh by trusting its operator key — the one trust-entry step every machine but the operator's own performs. Two ways: `--operator <key>` records the operator line in STATE (never in config.toml, so a machine that rolls back to a pre-P-CHARTER build can still read its own config), or a LAN address runs the one local-network ceremony that fetches the operator key AND the charter in force from the operator's machine, prints the key's fingerprint and every node line for the operator to compare out of band, and then records the key and applies the charter. A relay, the HTTPS adapter, a tailnet and an ssh tunnel are all refused: the guard admits a private or link-local peer address and never loopback.",
        args: [
            arg!("mesh", "string", true, "The mesh to join."),
            arg!("host", "string", false, "The operator's machine on the local network — a bare host/IP (`sakaki`, `192.168.1.158`), or host:port; the door port defaults to AOIDE_A2A_PORT, else 8710. Omitted: --operator is required instead."),
        ],
        flags: [
            flag!("operator", "string", "The operator line the operator's machine printed (`ed25519:<hex>`) — the non-LAN path, no dial at all. Refused when the machine already trusts a different key for this mesh."),
            flag!("yes", "bool", "Skip the fingerprint confirm on the LAN arm. The fingerprint is the whole trust step there (this is a first-use ceremony), so skipping it commits to a key nobody compared — the same stance `pair --yes` takes on pairing's codes."),
        ],
        gated: false,
        implemented: true,
        handler: handle_charter_join,
        examples: ["mesh join home --operator ed25519:…", "mesh join home sakaki", "mesh join home 192.168.1.158 --yes --json"],
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
    const USAGE: &str = "usage: aoide mesh join <mesh> (--operator ed25519:<hex> | <host>[:port]) [--yes] [--json]";
    let Some(mesh) = inv.args.first().map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        return Outcome::usage(cmd, USAGE);
    };
    let host_arg = inv.args.get(1).map(|s| s.trim()).filter(|s| !s.is_empty());
    let operator_flag = inv.flags.get("operator").cloned().filter(|s| !s.is_empty());
    if host_arg.is_some() == operator_flag.is_some() {
        return Outcome::usage(
            cmd,
            format!("{USAGE} — name exactly one source of trust: `--operator <key>` or the operator's LAN address"),
        );
    }

    // The LAN arm. The address is checked HERE, on the dial side, because the
    // ceremony is a local-network one by definition: a relay, the HTTPS
    // adapter, a tailnet and an ssh tunnel are all refused before anything is
    // posted, so a join never travels further than the LAN it is meant for.
    let (operator_key, charter_pair) = match (&operator_flag, host_arg) {
        (Some(field), _) => match charter::trust_operator(mesh, field) {
            Ok(key) => (key, None),
            Err(e) => return Outcome::error(cmd, e).with_data(json!({ "reason": "trust-refused", "mesh": mesh })),
        },
        (None, Some(host)) => {
            let (addr, port) = split_host_port(host);
            let resolved = match addr.parse::<std::net::IpAddr>() {
                Ok(ip) => ip.to_string(),
                Err(_) => match resolve_host_to_ip(&addr) {
                    Some(ip) => ip.to_string(),
                    None => {
                        return Outcome::error(
                            cmd,
                            format!("`{host}` does not resolve to an address — name the operator's machine by IP, or use `--operator <key>`"),
                        )
                        .with_data(json!({ "reason": "unresolved-host", "host": host }))
                    }
                },
            };
            if !charter::is_local_network(&resolved) {
                return Outcome::error(
                    cmd,
                    format!(
                        "`{host}` resolves to {resolved}, which is not a local-network address — a LAN join is admitted only over a private or link-local one \
                         (never loopback: a relayed or tunneled request arrives from loopback). Take the mesh without a LAN: `aoide mesh join {mesh} --operator <key>`, \
                         or `aoide mesh charter accept <file>` with the file the operator hands you"
                    ),
                )
                .with_data(json!({ "reason": "not-local-network", "host": host, "address": resolved }));
            }
            let url = format!("http://{resolved}:{port}/");
            let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "aoide/charterFetch", "params": { "mesh": mesh } });
            let (code, body_text) = match crate::commands::post_json_via(&url, None, "", &body.to_string(), None, &[], 15) {
                Ok(v) => v,
                Err(e) => {
                    return Outcome::error(cmd, format!("asking {url} for mesh `{mesh}`: {e}"))
                        .with_data(json!({ "reason": "unreachable", "url": url }))
                }
            };
            if code != 200 {
                return Outcome::error(cmd, format!("asking {url} for mesh `{mesh}`: HTTP {code} with body {body_text}"))
                    .with_data(json!({ "reason": "fetch-http-error", "url": url, "httpCode": code }));
            }
            let parsed: serde_json::Value = match serde_json::from_str(&body_text) {
                Ok(v) => v,
                Err(e) => {
                    return Outcome::error(cmd, format!("asking {url} for mesh `{mesh}`: unparseable response: {e}"))
                        .with_data(json!({ "reason": "unparseable", "url": url }))
                }
            };
            if let Some(err) = parsed.get("error") {
                return Outcome::error(cmd, format!("`{}` refused the join: {err}", host))
                    .with_data(json!({ "reason": "refused", "host": host, "error": err }));
            }
            let result = &parsed["result"];
            let key = result["operator"].as_str().unwrap_or_default().to_string();
            let bytes = match base64_decode(result["charter"].as_str().unwrap_or_default()) {
                Ok(b) => b,
                Err(e) => return Outcome::error(cmd, format!("the charter body from `{host}` is not decodable: {e}")),
            };
            let sig = match base64_decode(result["sig"].as_str().unwrap_or_default()) {
                Ok(b) => b,
                Err(e) => return Outcome::error(cmd, format!("the charter signature from `{host}` is not decodable: {e}")),
            };
            let version = result["version"].as_u64().unwrap_or(0);
            // The fingerprint is the trust step: printed for the operator to
            // compare with what the operator's machine shows, before anything
            // is recorded. This is a first-use ceremony, and its authority is
            // that comparison — never the transport, never the LAN guard.
            let fingerprint = charter::fingerprint_of_key(&key);
            if !inv.flag_present("yes") {
                if !confirm_join(mesh, &fingerprint, version) {
                    return Outcome::ok(cmd, "not confirmed — nothing recorded".to_string())
                        .with_data(json!({ "confirmed": false, "mesh": mesh }));
                }
            }
            (key, Some((bytes, sig)))
        }
        (None, None) => return Outcome::usage(cmd, USAGE),
    };

    // Record the trust FIRST and only then apply: `accept` reads the operator
    // key it must verify under, so a charter applied before its key is
    // recorded would be refused `unknown-operator` by this very machine.
    let field = format!("ed25519:{operator_key}");
    let trusted = match charter::trust_operator(mesh, &field) {
        Ok(k) => k,
        Err(e) => return Outcome::error(cmd, e).with_data(json!({ "reason": "trust-refused", "mesh": mesh })),
    };

    let mut applied = serde_json::Value::Null;
    let mut message = format!(
        "joined mesh `{mesh}` — this machine now trusts operator {} (recorded in state, never in config.toml)",
        charter::fingerprint_of_key(&trusted)
    );
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

/// The LAN arm's one human gate: the operator key's fingerprint, printed for
/// the operator to compare with what their own machine shows. Deliberately its
/// own five lines rather than a shared prompt helper — this crate's prompts
/// are each shaped by what their own arm asks, and `--yes` skips exactly this
/// one question and no other.
fn confirm_join(mesh: &str, fingerprint: &str, version: u64) -> bool {
    eprint!(
        "join mesh `{mesh}` with operator key {fingerprint} (charter v{version})? \
         Compare it with the fingerprint the operator's machine printed. [y/N] "
    );
    let _ = std::io::Write::flush(&mut std::io::stderr());
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line).unwrap_or(0);
    read > 0 && matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
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
                continue;
            }
            return Outcome::error(
                cmd,
                format!(
                    "no charter in force for mesh `{mesh}` at this node — its state is {}/charter.toml, \
                     written by `aoide mesh charter accept <file>` or by a charter letter",
                    charter::mesh_state_dir(mesh).display()
                ),
            )
            .with_data(json!({ "reason": "no-charter", "mesh": mesh }));
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
        return Outcome::ok(
            cmd,
            "no charter in force at this node — root one with `aoide mesh charter init <mesh>`, or take one with `aoide mesh charter accept <file>`",
        )
        .with_data(json!({ "meshes": [] }));
    }
    let mut lines = Vec::new();
    for row in &rows {
        let mesh = row["mesh"].as_str().unwrap_or("");
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
}
