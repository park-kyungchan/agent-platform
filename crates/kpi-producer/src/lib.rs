//! Bounded parent-program evidence capture. No Academy integration or attribution.
//! Callers own source authenticity, contracts, release authority and retention.
#![cfg(target_os = "linux")]

pub mod capture;
pub mod filesystem;
pub mod git;
pub mod guardrails;
pub mod journal;
pub mod model;
pub mod process;
pub mod strict;

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error(pub &'static str);
pub type Result<T> = std::result::Result<T, Error>;
impl Error {
    pub fn exit_code(self) -> u8 {
        if self.0.starts_with("journal_")
            || self.0.starts_with("evidence_")
            || matches!(self.0, "append_unknown" | "stdout" | "serialization")
        {
            4
        } else {
            2
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for Error {}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn text(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 256 && !value.chars().any(char::is_control)
}
pub fn now() -> Result<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| Error("invalid_clock"))
}
