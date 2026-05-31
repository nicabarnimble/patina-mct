//! Iroh adapter boundary for MCT peer protocols.
//!
//! This crate will own Mother-owned Iroh endpoint lifecycle and MCT ALPN protocol
//! effects. It must translate Iroh facts into `mct-kernel` domain records rather
//! than making Iroh transport identity into MCT authority.

#![forbid(unsafe_code)]

/// Returns the crate version for health and smoke tests.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    #[test]
    fn exposes_version() {
        assert_eq!(super::version(), "0.1.0");
    }
}
