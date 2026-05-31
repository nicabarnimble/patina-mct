//! Append-only observation ledger support for MCT.
//!
//! Runtime truth starts from `MctObservation` facts defined by `mct-kernel`.
//! Storage details stay in this crate and do not leak into the kernel.

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
