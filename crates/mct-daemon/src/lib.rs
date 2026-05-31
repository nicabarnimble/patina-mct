//! MCT daemon composition layer.
//!
//! The daemon composes the kernel, observation ledger, and adapters. Authority
//! remains in `mct-kernel`; external effects remain in adapter crates.

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
