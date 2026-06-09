//! MCT authority kernel domain records and decisions.
//!
//! This crate owns Mother/Child/Toy domain types. It must not expose Iroh,
//! Wasmtime, storage, telemetry, or daemon implementation types in its public API.

#![forbid(unsafe_code)]

pub mod call;
pub mod child;
pub mod id;
pub mod observation;
pub mod peer;
pub mod route;
pub mod toy;

pub use call::*;
pub use child::*;
pub use id::*;
pub use observation::*;
pub use peer::*;
pub use route::*;
pub use toy::*;

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
