//! Random bytes, one contract on both hosts: fill the caller's buffer from the
//! operating system's CSPRNG, or return the error. Nothing here keeps a pool,
//! a seed, or a fallback — a caller that cannot get OS randomness must fail,
//! never quietly get something weaker (`pkgs/aoide/crates/AGENTS.md`: a
//! missing capability is never an empty success).
//!
//! Unix: `/dev/urandom`, read directly. `/dev/urandom` never blocks once the
//! kernel CSPRNG is seeded, which is true well before userspace runs — the
//! standard justification for reading it rather than `/dev/random`. Native
//! Windows: `BCryptGenRandom` with `BCRYPT_USE_SYSTEM_PREFERRED_RNG`, the CNG
//! entry point that asks the system's own preferred RNG and holds no provider
//! handle of its own. Two spellings, one question asked of the host — the same
//! seam shape `owner_only`/`win_proc`/`win_unix` hold, and it lives here
//! because the one consumer below it (`aoide-secrets`' `enroll`) already
//! depends on this crate.
use std::io;

/// Fill `buf` entirely, or return the host's error without having promised
/// any part of it. A zero-length buffer is a successful no-op on both arms.
#[cfg(unix)]
pub fn fill(buf: &mut [u8]) -> io::Result<()> {
    use std::io::Read;
    std::fs::File::open("/dev/urandom")?.read_exact(buf)
}

#[cfg(windows)]
pub fn fill(buf: &mut [u8]) -> io::Result<()> {
    use windows_sys::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
    let len = u32::try_from(buf.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "buffer larger than 4 GiB"))?;
    // With BCRYPT_USE_SYSTEM_PREFERRED_RNG the algorithm-handle argument is
    // ignored, so a null handle is the documented way to ask for the system
    // RNG rather than opening one provider.
    let status = unsafe {
        BCryptGenRandom(std::ptr::null_mut(), buf.as_mut_ptr(), len, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
    };
    if status < 0 {
        Err(io::Error::from_raw_os_error(status))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    /// Native Windows only: the Unix arm is `/dev/urandom`, read by the
    /// `aoide-secrets` enrollment tests this seam exists for. What needs its
    /// own assertion is the arm this crate introduces — a real fill that
    /// differs across calls, and the zero-length no-op a caller may rely on.
    #[cfg(windows)]
    #[test]
    fn cng_fills_a_buffer_with_bytes_that_differ_across_calls() {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        super::fill(&mut a).expect("ask this host's RNG");
        super::fill(&mut b).expect("ask this host's RNG");
        assert_ne!(a, b, "32 bytes from the system RNG that repeat are not random");
        assert_ne!(a, [0u8; 32], "an unwritten buffer is not a fill");
        super::fill(&mut []).expect("a zero-length fill is a no-op, not an error");
    }
}
