use crate::{MctKernelError, MctKernelResult};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
};

macro_rules! string_id {
    ($name:ident) => {
        #[doc = concat!("Stable string identifier for `", stringify!($name), "` values.")]
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            #[doc = concat!("Creates a `", stringify!($name), "` from a non-blank string.")]
            ///
            /// # Errors
            ///
            /// Returns an error when the supplied identifier is empty or blank.
            pub fn new(value: impl Into<String>) -> MctKernelResult<Self> {
                let value = value.into();
                crate::error::ensure_non_blank(stringify!($name), "value", &value)?;
                Ok(Self(value))
            }

            #[doc = concat!("Returns the string value of this `", stringify!($name), "`.")]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<&str> for $name {
            type Error = MctKernelError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = MctKernelError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Self::new(value).map_err(de::Error::custom)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(MctNodeId);
string_id!(VisionId);
string_id!(ProjectId);
string_id!(UserId);
string_id!(ChildId);
string_id!(CallId);
string_id!(DecisionId);
string_id!(ObservationId);
string_id!(TraceId);
string_id!(SpanId);
string_id!(PeerBindingId);
string_id!(EndpointIdText);
string_id!(ProtocolRequestId);
string_id!(ReplyId);
string_id!(ResultRef);
string_id!(AuditRef);
string_id!(ToyId);
string_id!(ToyGrantId);
string_id!(ToyGrantEvaluationId);
string_id!(AuthorizedToyCallId);
string_id!(ChildInstanceId);
string_id!(ChildAssignmentId);
string_id!(ComponentArtifactId);
string_id!(ChildApprovalId);
string_id!(ChildCallEvaluationId);
string_id!(AuthorizedChildInvocationId);
string_id!(AuthorizedRouteExecutionId);
string_id!(ArtifactSourceAuthorityId);
string_id!(ArtifactAcquisitionDecisionId);
string_id!(ArtifactAcquisitionId);
string_id!(AuthorizedArtifactAcquisitionId);
string_id!(CallTriggerAuthorityId);
string_id!(CallTriggerOccurrenceId);
string_id!(CallTriggerFiringId);
string_id!(CallTriggerPendingOccurrenceId);

/// RFC3339 timestamp at the domain boundary, ordered by chronological instant.
#[derive(Clone, Debug)]
pub struct Timestamp {
    value: String,
    epoch_nanoseconds: i128,
}

impl Timestamp {
    /// Creates a timestamp from an RFC3339 instant string.
    ///
    /// # Errors
    ///
    /// Returns an error when the value is not a valid RFC3339 instant.
    pub fn new(value: impl Into<String>) -> MctKernelResult<Self> {
        let value = value.into();
        let parsed = value.parse::<jiff::Timestamp>().map_err(|source| {
            MctKernelError::InvalidTimestamp {
                value: value.clone(),
                reason: source.to_string(),
            }
        })?;
        Ok(Self {
            value,
            epoch_nanoseconds: parsed.as_nanosecond(),
        })
    }

    /// Returns the original RFC3339 timestamp string.
    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl TryFrom<&str> for Timestamp {
    type Error = MctKernelError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for Timestamp {
    type Error = MctKernelError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl PartialEq for Timestamp {
    fn eq(&self, other: &Self) -> bool {
        self.epoch_nanoseconds == other.epoch_nanoseconds
    }
}

impl Eq for Timestamp {}

impl PartialOrd for Timestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Timestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        self.epoch_nanoseconds.cmp(&other.epoch_nanoseconds)
    }
}

impl Hash for Timestamp {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.epoch_nanoseconds.hash(state);
    }
}

impl Serialize for Timestamp {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.value)
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(de::Error::custom)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_ids_roundtrip_as_strings() {
        let id =
            CallId::new("call-1").expect("string ID literal/generated value must be non-empty");
        let encoded = serde_json::to_string(&id).unwrap();
        assert_eq!(encoded, "\"call-1\"");
        let decoded: CallId = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.as_str(), "call-1");
    }

    #[test]
    fn string_id_construction_rejects_empty_and_blank_values() {
        assert!(matches!(
            CallId::new(""),
            Err(crate::MctKernelError::InvalidField {
                record: "CallId",
                field: "value",
                reason: crate::InvalidFieldReason::Empty,
            })
        ));
        assert!(matches!(
            CallId::new("   "),
            Err(crate::MctKernelError::InvalidField {
                record: "CallId",
                field: "value",
                reason: crate::InvalidFieldReason::Blank,
            })
        ));
        assert!(serde_json::from_str::<CallId>("\"\"").is_err());
    }

    #[test]
    fn timestamps_order_chronologically_across_subsecond_precision() {
        let same_instant_short = Timestamp::new("2026-05-31T00:00:00.1Z").unwrap();
        let same_instant_padded = Timestamp::new("2026-05-31T00:00:00.10Z").unwrap();
        let later = Timestamp::new("2026-05-31T00:00:00.100001Z").unwrap();

        assert_eq!(same_instant_short, same_instant_padded);
        assert!(same_instant_padded < later);
    }

    #[test]
    fn epoch_second_strings_are_rejected_as_timestamps() {
        assert!(matches!(
            Timestamp::new("1772323200"),
            Err(crate::MctKernelError::InvalidTimestamp { .. })
        ));

        let decoded = serde_json::from_str::<Timestamp>("\"1772323200\"");
        assert!(decoded.is_err());
    }
}
