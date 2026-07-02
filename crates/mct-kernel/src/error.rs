use thiserror::Error;

/// Public type alias `MctKernelResult` for kernel callers.
pub type MctKernelResult<T> = std::result::Result<T, MctKernelError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Closed domain enum `InvalidFieldReason` used by the MCT kernel.
pub enum InvalidFieldReason {
    /// Public `Empty` item.
    Empty,
    /// Public `Blank` item.
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
/// Closed domain enum `MctKernelError` used by the MCT kernel.
pub enum MctKernelError {
    #[error("invalid {record}.{field}: {reason}")]
    /// Public `InvalidField` item.
    InvalidField {
        /// Field `str` of this domain record.
        record: &'static str,
        /// Field `str` of this domain record.
        field: &'static str,
        /// Field `InvalidFieldReason` of this domain record.
        reason: InvalidFieldReason,
    },

    #[error("invalid Timestamp '{value}': {reason}")]
    /// Public `InvalidTimestamp` item.
    InvalidTimestamp {
        /// Timestamp string that failed validation.
        value: String,
        /// Validation failure reason.
        reason: String,
    },

    #[error(
        "MCT call payload metadata size {call_size_bytes} does not match payload handle size {handle_size_bytes}"
    )]
    /// Public `PayloadSizeMismatch` item.
    PayloadSizeMismatch {
        /// Field `u64` of this domain record.
        call_size_bytes: u64,
        /// Field `u64` of this domain record.
        handle_size_bytes: u64,
    },

    #[error("failed to encode MCT call protocol JSON edge value: {source}")]
    /// Public `EncodeCallProtocolJson` item.
    EncodeCallProtocolJson {
        #[source]
        /// Field `Error` of this domain record.
        source: serde_json::Error,
    },

    #[error("failed to decode MCT call protocol JSON edge value: {source}")]
    /// Public `DecodeCallProtocolJson` item.
    DecodeCallProtocolJson {
        #[source]
        /// Field `Error` of this domain record.
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
