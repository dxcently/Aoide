//! The Hyprland request socket: the client list (`j/clients`) and the workspace a window
//! reports. Every read of the request socket goes through here.

use aoide_conduct::graph::normalize_addr;
use aoide_protocol::local_socket::connect_bounded;
use serde_json::Value;
use std::path::PathBuf;

/// The Hyprland workspace id of the client whose `address` matches `want` in a
/// decoded `hyprctl clients -j` array (`workspace.id` in the client JSON).
/// Pure over the decoded JSON — unit-testable without a compositor. Address
/// comparison is `0x`/case-tolerant via [`normalize_addr`] (the stored
/// `windowAddress` and hyprctl can disagree on both). `None` when the window is
/// absent from the list or carries no numeric `workspace.id` — the caller then
/// leaves the stored `workspace` untouched (degrade gracefully, never a panic).
pub(crate) fn client_workspace_for_address(clients: &[Value], want: &str) -> Option<i64> {
    let want = normalize_addr(want);
    if want.is_empty() {
        return None;
    }
    clients.iter().find_map(|c| {
        let a = c.get("address").and_then(Value::as_str)?;
        if normalize_addr(a) != want {
            return None;
        }
        c.get("workspace")
            .and_then(|w| w.get("id"))
            .and_then(Value::as_i64)
    })
}

/// The compositor's client list, decoded into a JSON array. It asks Hyprland's
/// request socket, `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`,
/// for `j/clients` (the JSON `hyprctl clients -j` prints), and reads the reply to EOF.
/// No `hyprctl` binary and no PATH lookup: the socket is the only route, so the answer
/// does not depend on the unit's PATH. Returns `None` whenever the
/// compositor cannot be consulted authoritatively: no `HYPRLAND_INSTANCE_SIGNATURE`
/// or `XDG_RUNTIME_DIR`, no socket, a connect, request or reply that outlasts the 2 s
/// budget, or a reply that is not a JSON array. The connect, the request and the whole
/// reply share that one budget, so a listener that never accepts, or a reply that
/// trickles, cannot hold the caller past it. This is the single clients-reading seam
/// every desktop consumer shares (the window sweep, the terminal roster and the tests),
/// so they all degrade identically (never a panic, never a false result).
pub fn hyprctl_clients() -> Option<Vec<Value>> {
    use std::io::{Read, Write};
    let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    let socket = PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket.sock");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let left = || {
        deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|d| !d.is_zero())
    };
    let mut stream = connect_bounded(&socket, left()?).ok()?;
    stream.set_write_timeout(Some(left()?)).ok()?;
    stream.write_all(b"j/clients").ok()?;
    let mut reply = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        stream.set_read_timeout(Some(left()?)).ok()?;
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => reply.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
    match serde_json::from_slice(&reply) {
        Ok(Value::Array(a)) => Some(a),
        _ => None,
    }
}
