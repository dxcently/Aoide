#![cfg(feature = "fakes")]

use std::fs::{self, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use aoide_desktop_door::fake::FakeDoor;
use aoide_desktop_door::{Door, DoorError, Mode};
use aoide_desktop_shell::exec::make_fifo;

#[test]
fn a_fifo_at_the_token_path_is_refused_at_once_and_no_request_is_sent() {
    let root = std::env::temp_dir().join(format!("aoide-shell-fifo-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let runtime = root.join("run");
    fs::create_dir_all(&runtime).unwrap();
    fs::set_permissions(&runtime, Permissions::from_mode(0o700)).unwrap();
    let fake = FakeDoor::start(&runtime, "tok");
    let token = fake.token_file();
    fs::remove_file(&token).unwrap();
    make_fifo(&token).unwrap();
    let door = Door::new(fake.addr(), &runtime, &token).unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(door.say("hi", Mode::Send));
    });
    let answer = rx.recv_timeout(Duration::from_secs(5)).expect("the refusal comes within 5 s, not a wait for a writer");
    assert!(matches!(answer, Err(DoorError::Refused { .. })), "{answer:?}");
    assert!(fake.requests().is_empty(), "no request reached the door");
    let _ = fs::remove_dir_all(&root);
}
