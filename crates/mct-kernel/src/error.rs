use thiserror::Error;

/// Kernel result type that preserves typed authority and validation failures.
pub type MctKernelResult<T> = std::result::Result<T, MctKernelError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Reason a string field failed the kernel's non-blank invariant.
pub enum InvalidFieldReason {
    /// The field contained no bytes.
    Empty,
    /// The field contained only whitespace after trimming.
    Blank,
}

impl std::fmt::Display for InvalidFieldReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("empty"),
            Self::Blank => f.write_str("blank"),
        }
    }
}

#[derive(Debug, Error)]
#[non_exhaustive]
/// Typed kernel boundary errors for invalid records and JSON edge values.
///
/// Authority denials are represented as decisions, not errors. This enum is
/// reserved for malformed domain data, invalid timestamps, payload consistency
/// failures, and serialization faults at the protocol edge.
pub enum MctKernelError {
    #[error("invalid {record}.{field}: {reason}")]
    /// A required string field was empty or blank.
    InvalidField {
        /// Name of the kernel record being validated.
        record: &'static str,
        /// Field within the record that violated the invariant.
        field: &'static str,
        /// Whether the value was empty or whitespace-only.
        reason: InvalidFieldReason,
    },

    #[error("invalid Timestamp '{value}': {reason}")]
    /// Timestamp text was not a valid RFC3339 instant accepted by the kernel.
    InvalidTimestamp {
        /// Timestamp string that failed validation.
        value: String,
        /// Parser or range failure reason.
        reason: String,
    },

    #[error(
        "MCT call payload metadata size {call_size_bytes} does not match payload handle size {handle_size_bytes}"
    )]
    /// Call metadata and payload handle disagreed about payload size.
    PayloadSizeMismatch {
        /// Size declared in [`crate::MctCall::payload_metadata`].
        call_size_bytes: u64,
        /// Size carried by the protocol payload handle.
        handle_size_bytes: u64,
    },

    #[error("failed to encode MCT call protocol JSON edge value: {source}")]
    /// Serialization failed while emitting a validated call protocol edge value.
    EncodeCallProtocolJson {
        #[source]
        /// Serde source error preserved for diagnostics.
        source: serde_json::Error,
    },

    #[error("failed to decode MCT call protocol JSON edge value: {source}")]
    /// Deserialization failed before a call protocol edge value could be validated.
    DecodeCallProtocolJson {
        #[source]
        /// Serde source error preserved for diagnostics.
        source: serde_json::Error,
    },
}

pub(crate) fn ensure_non_blank(
    record: &'static str,
    field: &'static str,
    value: &str,
) -> MctKernelResult<()> {
    if value.is_empty() {
        return Err(MctKernelError::InvalidField {
            record,
            field,
            reason: InvalidFieldReason::Empty,
        });
    }

    if value.trim().is_empty() {
        return Err(MctKernelError::InvalidField {
            record,
            field,
            reason: InvalidFieldReason::Blank,
        });
    }

    Ok(())
}
