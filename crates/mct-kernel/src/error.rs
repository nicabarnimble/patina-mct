use thiserror::Error;

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
    InvalidField {
        record: &'static str,
        field: &'static str,
        reason: InvalidFieldReason,
    },

    #[error("invalid Timestamp '{value}': {reason}")]
    /// Timestamp text was not a valid RFC3339 instant accepted by the kernel.
    InvalidTimestamp { value: String, reason: String },

    #[error(
        "MCT call payload metadata size {call_size_bytes} does not match payload handle size {handle_size_bytes}"
    )]
    PayloadSizeMismatch {
        call_size_bytes: u64,
        handle_size_bytes: u64,
    },

    #[error("invalid {record}.{field}: {reason}")]
    /// A closed trigger or watch authority constraint was malformed.
    InvalidConstraint {
        record: &'static str,
        field: &'static str,
        /// Stable caller-safe description of the failed constraint.
        reason: &'static str,
    },

    #[error("failed to encode MCT call protocol JSON edge value: {source}")]
    EncodeCallProtocolJson {
        #[source]
        source: serde_json::Error,
    },

    #[error("invalid {record}.route_taken for {outcome} reply outcome")]
    /// A caller-safe reply carried a route projection for an outcome that must hide it.
    InvalidRouteTakenPresence {
        record: &'static str,
        outcome: &'static str,
    },

    #[error("invalid {record}.reason for {outcome} reply outcome")]
    /// A caller-safe reply carried a reason on a success outcome.
    InvalidReplyReasonPresence {
        record: &'static str,
        outcome: &'static str,
    },

    #[error("failed to decode MCT call protocol JSON edge value: {source}")]
    DecodeCallProtocolJson {
        #[source]
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
