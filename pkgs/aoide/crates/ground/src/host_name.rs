//! This host's name as the OS reports it.

/// The OS hostname, or `None` on any failure (truncated/non-UTF8/errno) —
/// best-effort, never a panic. Unix asks `gethostname(2)`; Windows asks
/// `GetComputerNameExW` for the DNS hostname, the same name a Unix host
/// reports for itself (and not the NetBIOS name, which is the truncated,
/// uppercase form).
///
/// Public because it has a SECOND consumer with the same need and the same
/// no-fork rule: the secrets broker's enrollment labels an `otpauth://` URI
/// with this host, and an enrollment must not spell the hostname differently
/// from every other surface.
#[cfg(unix)]
pub fn os_hostname() -> Option<String> {
    let mut buf = vec![0u8; 256];
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if rc != 0 {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let s = String::from_utf8_lossy(&buf[..end]).trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// The Windows arm — see the Unix arm's doc above for the shared contract.
/// The two-call `GetComputerNameExW` shape (the required length first, then
/// the value, with the length it wrote checked against what it was given) is
/// what keeps a name too long for the first buffer from coming back
/// truncated-but-plausible: a truncated name would be a DIFFERENT host.
#[cfg(windows)]
pub fn os_hostname() -> Option<String> {
    use windows_sys::Win32::System::SystemInformation::{ComputerNameDnsHostname, GetComputerNameExW};
    let mut buf = vec![0u16; 256];
    let mut len = buf.len() as u32;
    if unsafe { GetComputerNameExW(ComputerNameDnsHostname, buf.as_mut_ptr(), &mut len) } == 0 {
        return None;
    }
    let len = len as usize;
    if len == 0 || len > buf.len() {
        return None;
    }
    let s = String::from_utf16_lossy(&buf[..len]).trim().to_string();
    (!s.is_empty()).then_some(s)
}
