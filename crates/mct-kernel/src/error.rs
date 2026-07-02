use thiserror::Error;

pub type MctKernelResult<T> = std::result::Result<T, MctKernelError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvalidFieldReason {
    Empty,
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
pub enum MctKernelError {
    #[error("invalid {record}.{field}: {reason}")]
    InvalidField {
        record: &'static str,
        field: &'static str,
        reason: InvalidFieldReason,
    },

    #[error("invalid Timestamp '{value}': {source}")]
    InvalidTimestamp {
        value: String,
        #[source]
        source: jiff::Error,
    },

    #[error("invalid MCT call payload handle for {payload_kind}: missing {field}")]
    PayloadHandleMissingField {
        payload_kind: &'static str,
        field: &'static str,
    },

    #[error("invalid MCT call payload handle for {payload_kind}: unexpected {field}")]
    PayloadHandleUnexpectedField {
        payload_kind: &'static str,
        field: &'static str,
    },

    #[error("empty MCT call payload handle has non-zero size {size_bytes}")]
    EmptyPayloadHasNonZeroSize { size_bytes: u64 },

    #[error(
        "MCT call payload metadata size {call_size_bytes} does not match payload handle size {handle_size_bytes}"
    )]
    PayloadSizeMismatch {
        call_size_bytes: u64,
        handle_size_bytes: u64,
    },

    #[error("failed to encode MCT call protocol JSON edge value: {source}")]
    EncodeCallProtocolJson {
        #[source]
        source: serde_json::Error,
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
