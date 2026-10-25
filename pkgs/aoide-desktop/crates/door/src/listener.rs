//! The listener check: before a byte is written, the client finds the kernel's
//! row for its own connection in `/proc/net/tcp` (`tcp6` for an IPv6 door). The
//! row's local address is the door's and its remote address is this
//! connection's local address, so the row is the accepted socket itself, and
//! nothing can change hands between the check and the request. Its owner, the
//! row's eighth column, must be this process's effective uid.
//!
//! What it proves: the listener that answered this connection runs as this
//! user. It does not prove the process is `eidolon`, and a same-uid listener
//! passes it (ruling 29).
//!
//! A V4 door's row is in `tcp`, since Eidolon binds one loopback address and its
//! accepted socket is an IPv4 socket. A dual-stack listener (an IPv6 socket that
//! also takes IPv4) has its accepted socket in `tcp6`, which a V4 door's check
//! does not read, so its connection finds no row and is refused. That is
//! fail-closed.

use std::fs;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::thread;
use std::time::Duration;

use crate::error::DoorError;
use crate::token::euid;

/// The row can appear a moment after `connect` returns on loopback, so the
/// table is read again, this many times, before "no row" is the answer.
const TRIES: u32 = 50;
const PAUSE: Duration = Duration::from_millis(10);

/// Refuses unless the socket that answers `client` on `door` is owned by this user.
pub(crate) fn check(door: SocketAddr, client: SocketAddr) -> Result<(), DoorError> {
    let table = match door {
        SocketAddr::V4(_) => "/proc/net/tcp",
        SocketAddr::V6(_) => "/proc/net/tcp6",
    };
    let mut owner = None;
    for attempt in 0..TRIES {
        owner = owner_in(&fs::read_to_string(table)?, door, client);
        if owner.is_some() || attempt + 1 == TRIES {
            break;
        }
        thread::sleep(PAUSE);
    }
    decide(door, owner, euid()?)
}

/// The verdict on a row's owner: this user's listener passes. No row, and any
/// other uid (root included), is refused as `Unowned`.
pub(crate) fn decide(door: SocketAddr, owner: Option<u32>, me: u32) -> Result<(), DoorError> {
    match owner {
        Some(uid) if uid == me => Ok(()),
        uid => Err(DoorError::Unowned { addr: door, uid }),
    }
}

/// The owner of the row whose local address is `door` and whose remote address
/// is `client`.
fn owner_in(table: &str, door: SocketAddr, client: SocketAddr) -> Option<u32> {
    table.lines().skip(1).find_map(|line| {
        let columns: Vec<&str> = line.split_whitespace().collect();
        let local = parse_addr(columns.get(1)?)?;
        let remote = parse_addr(columns.get(2)?)?;
        if local != door || remote != client {
            return None;
        }
        columns.get(7)?.parse().ok()
    })
}

/// One `ADDR:PORT` column. The kernel prints each 32-bit word of the address as
/// `%08X` of the word in host byte order, and the port as `%04X`, so a word's
/// native bytes are the address's octets in order.
fn parse_addr(column: &str) -> Option<SocketAddr> {
    let (ip, port) = column.split_once(':')?;
    let port = u16::from_str_radix(port, 16).ok()?;
    let mut octets = Vec::with_capacity(16);
    for word in ip.as_bytes().chunks(8) {
        let word = u32::from_str_radix(std::str::from_utf8(word).ok()?, 16).ok()?;
        octets.extend_from_slice(&word.to_ne_bytes());
    }
    let ip: IpAddr = match octets.len() {
        4 => Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3]).into(),
        16 => Ipv6Addr::from(<[u8; 16]>::try_from(octets.as_slice()).ok()?).into(),
        _ => return None,
    };
    Some(SocketAddr::new(ip, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    const DOOR4: &str = "127.0.0.1:41394";
    const CLIENT4: &str = "127.0.0.1:50000";
    const DOOR6: &str = "[::1]:41394";
    const CLIENT6: &str = "[::1]:50000";

    /// The kernel's column text for the two addresses, as /proc prints them on x86_64.
    const DOOR4_COLUMN: &str = "0100007F:A1B2";
    const CLIENT4_COLUMN: &str = "0100007F:C350";
    const DOOR6_COLUMN: &str = "00000000000000000000000001000000:A1B2";
    const CLIENT6_COLUMN: &str = "00000000000000000000000001000000:C350";

    /// One row of /proc/net/tcp with `uid` in its eighth column.
    fn row(local: &str, remote: &str, uid: u32) -> String {
        format!("   0: {local} {remote} 01 00000000:00000000 00:00000000 00000000 {uid:>5}        0 1 0 10 0\n")
    }

    /// The table as the kernel prints it: a header line, then the rows.
    fn table(rows: &[String]) -> String {
        let header = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n";
        format!("{header}{}", rows.concat())
    }

    #[test]
    fn the_row_for_this_connection_gives_its_owner() {
        let other_client = row(DOOR4_COLUMN, "0100007F:D000", 0);
        let ours = row(DOOR4_COLUMN, CLIENT4_COLUMN, 1000);
        let text = table(&[other_client, ours]);
        assert_eq!(owner_in(&text, DOOR4.parse().unwrap(), CLIENT4.parse().unwrap()), Some(1000));
    }

    #[test]
    fn a_row_owned_by_another_uid_is_found_as_that_uid() {
        let text = table(&[row(DOOR4_COLUMN, CLIENT4_COLUMN, 0)]);
        assert_eq!(owner_in(&text, DOOR4.parse().unwrap(), CLIENT4.parse().unwrap()), Some(0));
    }

    #[test]
    fn no_row_for_this_connection_gives_none() {
        let text = table(&[row(DOOR4_COLUMN, "0100007F:D000", 1000)]);
        assert_eq!(owner_in(&text, DOOR4.parse().unwrap(), CLIENT4.parse().unwrap()), None);
    }

    #[test]
    fn an_ipv6_row_matches_its_words_in_host_order() {
        let text = table(&[row(DOOR6_COLUMN, CLIENT6_COLUMN, 1000)]);
        assert_eq!(owner_in(&text, DOOR6.parse().unwrap(), CLIENT6.parse().unwrap()), Some(1000));
    }

    #[test]
    fn columns_parse_to_the_address_they_name() {
        assert_eq!(parse_addr(DOOR4_COLUMN), Some("127.0.0.1:41394".parse().unwrap()));
        assert_eq!(parse_addr("0100A8C0:0050"), Some("192.168.0.1:80".parse().unwrap()));
        assert_eq!(parse_addr(DOOR6_COLUMN), Some("[::1]:41394".parse().unwrap()));
    }

    #[test]
    fn a_column_that_names_no_address_parses_to_none() {
        assert_eq!(parse_addr("0100007F"), None, "no port");
        assert_eq!(parse_addr("ZZZZZZZZ:0001"), None, "not hex");
        assert_eq!(parse_addr("0100007F00:0001"), None, "a word short of 32 bits");
    }

    #[test]
    fn this_process_owns_the_listener_it_connects_to() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let door = listener.local_addr().unwrap();
        let client = std::net::TcpStream::connect(door).unwrap();
        assert!(check(door, client.local_addr().unwrap()).is_ok());
    }

    /// The verdict is this user's listener and nothing else: no row, and any other
    /// uid with root's among them, is refused, and the door's address is named.
    #[test]
    fn decide_passes_only_this_users_listener() {
        let door: SocketAddr = DOOR4.parse().unwrap();
        assert!(decide(door, Some(1000), 1000).is_ok());
    }

    #[test]
    fn decide_refuses_no_row_and_any_other_uid_root_included() {
        let door: SocketAddr = DOOR4.parse().unwrap();
        for owner in [None, Some(0), Some(1001)] {
            match decide(door, owner, 1000) {
                Err(DoorError::Unowned { addr, uid }) => assert_eq!((addr, uid), (door, owner)),
                other => panic!("{owner:?}: {other:?}"),
            }
        }
    }
}
