//! A6, the token read (plan §3.7). Each check takes the runtime directory as a
//! parameter, so no test sets `XDG_RUNTIME_DIR`. Production passes the
//! variable's value once, through [`runtime_from_env`].
//!
//! No libc FFI here: the effective uid comes from `/proc/self/status`, the
//! open is `OpenOptions` with `O_NOFOLLOW` (std adds `O_CLOEXEC`), and the
//! fstat is `File::metadata`.

use std::ffi::OsStr;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use crate::error::DoorError;

/// The longest token file read. A longer one is refused, never truncated.
const TOKEN_CAP: u64 = 4096;

/// The door's token. Its `Debug` and `Display` print no token.
pub struct Token(String);

impl Token {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// The runtime directory from `XDG_RUNTIME_DIR`'s value. Unset, empty or
/// relative is refused, as the base directory spec says: there is no temp-dir
/// fallback.
pub fn runtime_from_env(value: Option<&OsStr>) -> Result<PathBuf, DoorError> {
    match value {
        Some(value) if !value.is_empty() && Path::new(value).is_absolute() => Ok(PathBuf::from(value)),
        _ => Err(DoorError::RuntimeUnset),
    }
}

/// The token file of the door bound on `port`: `<runtime>/eidolon/web-<port>.token`.
pub fn token_path(runtime: &Path, port: u16) -> PathBuf {
    runtime.join("eidolon").join(format!("web-{port}.token"))
}

/// The runtime dir must be this user's, at mode 0700, and not a link.
pub fn check_runtime(runtime: &Path) -> Result<(), DoorError> {
    private_dir(runtime, euid()?)
}

/// The token of the door bound on `port`, read under A6's checks. The file
/// must be exactly `<runtime>/eidolon/web-<port>.token`.
pub(crate) fn read_token(runtime: &Path, token_file: &Path, port: u16) -> Result<Token, DoorError> {
    check_runtime(runtime)?;
    let uid = euid()?;
    private_dir(&runtime.join("eidolon"), uid)?;
    let expected = token_path(runtime, port);
    if token_file != expected.as_path() {
        return Err(refused(token_file, "not the token path of the door bound on this port"));
    }
    // O_NONBLOCK: a FIFO opens without waiting for a writer, and the fstat below refuses it.
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(token_file)
        .map_err(|e| refused(token_file, &format!("cannot open it without following links: {e}")))?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.uid() != uid || (meta.mode() & 0o077) != 0 || meta.nlink() != 1 {
        return Err(refused(token_file, "not a regular file of this user, mode 0600, with one link"));
    }
    let mut bytes = Vec::new();
    (&mut file).take(TOKEN_CAP + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > TOKEN_CAP {
        return Err(refused(token_file, "larger than 4 KiB"));
    }
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_graphic) {
        return Err(refused(token_file, "not printable ASCII with no space or newline"));
    }
    String::from_utf8(bytes).map(Token).map_err(|_| refused(token_file, "not text"))
}

fn refused(path: &Path, why: &str) -> DoorError {
    DoorError::Refused { path: path.to_path_buf(), why: why.to_string() }
}

/// A directory this user owns, at mode 0700, and not a link.
fn private_dir(path: &Path, uid: u32) -> Result<(), DoorError> {
    let meta = fs::symlink_metadata(path).map_err(|e| refused(path, &format!("cannot lstat it: {e}")))?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(refused(path, "not a directory, or a symlink to one"));
    }
    if meta.uid() != uid || (meta.mode() & 0o777) != 0o700 {
        return Err(refused(path, "not owned by this user at mode 0700"));
    }
    Ok(())
}

/// The effective uid, from the `Uid:` line of `/proc/self/status`.
pub(crate) fn euid() -> io::Result<u32> {
    let status = fs::read_to_string("/proc/self/status")?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|ids| ids.split_whitespace().nth(1))
        .and_then(|effective| effective.parse().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no effective uid in /proc/self/status"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    /// A runtime directory the test owns, removed with everything in it.
    struct Runtime(PathBuf);

    impl Runtime {
        fn new(tag: &str) -> Runtime {
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!("aoide-door-{tag}-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            chmod(&root, 0o700);
            Runtime(root)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn eidolon(&self) -> PathBuf {
            let dir = self.0.join("eidolon");
            fs::create_dir_all(&dir).unwrap();
            chmod(&dir, 0o700);
            dir
        }

        fn token(&self, port: u16) -> PathBuf {
            self.eidolon().join(format!("web-{port}.token"))
        }

        fn write_token(&self, port: u16, body: &[u8], mode: u32) -> PathBuf {
            let path = self.token(port);
            fs::write(&path, body).unwrap();
            chmod(&path, mode);
            path
        }
    }

    impl Drop for Runtime {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn chmod(path: &Path, mode: u32) {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn refused(result: Result<Token, DoorError>) -> bool {
        matches!(result, Err(DoorError::Refused { .. }))
    }

    #[test]
    fn an_unset_or_empty_runtime_dir_is_refused() {
        assert!(matches!(runtime_from_env(None), Err(DoorError::RuntimeUnset)));
        assert!(matches!(runtime_from_env(Some(OsStr::new(""))), Err(DoorError::RuntimeUnset)));
        assert_eq!(
            runtime_from_env(Some(OsStr::new("/run/user/1000"))).unwrap(),
            Path::new("/run/user/1000")
        );
    }

    #[test]
    fn a_relative_runtime_dir_is_refused_as_unset() {
        assert!(matches!(runtime_from_env(Some(OsStr::new("run/user/1000"))), Err(DoorError::RuntimeUnset)));
    }

    #[test]
    fn a_good_token_reads_back_exactly() {
        let rt = Runtime::new("good");
        let file = rt.write_token(4477, b"s3cret-TOKEN_1", 0o600);
        assert_eq!(read_token(rt.path(), &file, 4477).unwrap().as_str(), "s3cret-TOKEN_1");
    }

    #[test]
    fn a_runtime_dir_not_at_0700_is_refused() {
        let rt = Runtime::new("xdg-mode");
        let file = rt.write_token(1, b"t", 0o600);
        chmod(rt.path(), 0o755);
        assert!(refused(read_token(rt.path(), &file, 1)));
    }

    #[test]
    fn a_runtime_dir_that_is_a_symlink_is_refused() {
        let rt = Runtime::new("xdg-link");
        let real = rt.path().join("real");
        fs::create_dir_all(real.join("eidolon")).unwrap();
        chmod(&real, 0o700);
        chmod(&real.join("eidolon"), 0o700);
        fs::write(real.join("eidolon/web-2.token"), b"t").unwrap();
        chmod(&real.join("eidolon/web-2.token"), 0o600);
        let link = rt.path().join("xdg");
        symlink(&real, &link).unwrap();
        assert!(refused(read_token(&link, &link.join("eidolon/web-2.token"), 2)));
    }

    #[test]
    fn an_eidolon_dir_not_at_0700_is_refused() {
        let rt = Runtime::new("eid-mode");
        let file = rt.write_token(3, b"t", 0o600);
        chmod(&rt.path().join("eidolon"), 0o755);
        assert!(refused(read_token(rt.path(), &file, 3)));
    }

    #[test]
    fn a_token_not_at_0600_is_refused() {
        let rt = Runtime::new("tok-mode");
        let file = rt.write_token(4, b"t", 0o644);
        assert!(refused(read_token(rt.path(), &file, 4)));
    }

    #[test]
    fn a_symlinked_token_is_refused() {
        let rt = Runtime::new("tok-link");
        let target = rt.path().join("elsewhere.token");
        fs::write(&target, b"t").unwrap();
        chmod(&target, 0o600);
        let file = rt.token(5);
        symlink(&target, &file).unwrap();
        assert!(refused(read_token(rt.path(), &file, 5)));
    }

    #[test]
    fn a_token_with_a_second_hard_link_is_refused() {
        let rt = Runtime::new("nlink");
        let file = rt.write_token(6, b"t", 0o600);
        fs::hard_link(&file, rt.path().join("eidolon").join("second")).unwrap();
        assert!(refused(read_token(rt.path(), &file, 6)));
    }

    #[test]
    fn a_token_path_other_than_the_expected_one_is_refused() {
        let rt = Runtime::new("path");
        let file = rt.write_token(7, b"t", 0o600);
        // The door bound port 8, so `web-8.token` is the only file it may name.
        assert!(refused(read_token(rt.path(), &file, 8)));
    }

    #[test]
    fn the_effective_uid_is_the_owner_of_a_file_this_process_makes() {
        let rt = Runtime::new("euid");
        let file = rt.write_token(9, b"t", 0o600);
        assert_eq!(euid().unwrap(), fs::metadata(&file).unwrap().uid());
    }

    #[test]
    fn a_token_with_a_space_or_a_newline_is_refused() {
        let bodies: [&[u8]; 3] = [b"abc\n", b"ab c", b""];
        for body in bodies {
            let rt = Runtime::new("charset");
            let file = rt.write_token(10, body, 0o600);
            assert!(refused(read_token(rt.path(), &file, 10)), "{body:?}");
        }
    }

    #[test]
    fn a_token_file_over_4_kib_is_refused_not_truncated() {
        let rt = Runtime::new("big");
        let file = rt.write_token(11, &[b'a'; 4097], 0o600);
        assert!(refused(read_token(rt.path(), &file, 11)));
    }

    #[test]
    fn a_token_prints_no_token() {
        let token = Token("s3cret".into());
        assert!(!format!("{token:?}{token}").contains("s3cret"));
    }
}
