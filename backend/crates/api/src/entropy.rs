//! Keys, nonces, tokens and OTP draws read the OS directly (getrandom via `SysRng`), as the old `OsRng` did.

use rand::{RngExt, TryRng, rand_core::UnwrapErr, rngs::SysRng};

/// Fills `dst` from the OS. Panics if the OS has no entropy, as `OsRng` did.
pub fn fill(dst: &mut [u8]) {
    SysRng.try_fill_bytes(dst).expect("OS entropy unavailable");
}

/// Uniform value in `0..n` from the OS, without modulo bias.
pub fn below(n: usize) -> usize {
    UnwrapErr(SysRng).random_range(0..n)
}
