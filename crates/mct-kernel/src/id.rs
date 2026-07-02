use crate::{MctKernelError, MctKernelResult};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
};

macro_rules! string_id {
    ($name:ident) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self::new(value)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self::new(value)
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

/// RFC3339 timestamp at the domain boundary, ordered by chronological instant.
#[derive(Clone, Debug)]
pub struct Timestamp {
    value: String,
    epoch_nanoseconds: i128,
}

impl Timestamp {
    pub fn new(value: impl Into<String>) -> MctKernelResult<Self> {
        let value = value.into();
        let parsed = value.parse::<jiff::Timestamp>().map_err(|source| {
            MctKernelError::InvalidTimestamp {
                value: value.clone(),
                source,
            }
        })?;
        Ok(Self {
            value,
            epoch_nanoseconds: parsed.as_nanosecond(),
        })
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl From<&str> for Timestamp {
    fn from(value: &str) -> Self {
        Self::new(value).expect("valid RFC3339 timestamp literal")
    }
}

impl From<String> for Timestamp {
    fn from(value: String) -> Self {
        Self::new(value).expect("valid RFC3339 timestamp string")
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
        let id = CallId::from("call-1");
        let encoded = serde_json::to_string(&id).unwrap();
        assert_eq!(encoded, "\"call-1\"");
        let decoded: CallId = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.as_str(), "call-1");
    }

    #[test]
    fn timestamps_order_chronologically_across_subsecond_precision() {
        let same_instant_short = Timestamp::from("2026-05-31T00:00:00.1Z");
        let same_instant_padded = Timestamp::from("2026-05-31T00:00:00.10Z");
        let later = Timestamp::from("2026-05-31T00:00:00.100001Z");

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
