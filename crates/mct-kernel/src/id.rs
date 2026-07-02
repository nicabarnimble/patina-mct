use serde::{Deserialize, Serialize};
use std::fmt;

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

/// Allium uses `Timestamp`; v0 keeps it as an RFC3339-ish string at the domain boundary.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(String);

impl Timestamp {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for Timestamp {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for Timestamp {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
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
        let earlier = Timestamp::from("2026-05-31T00:00:00.09Z");
        let later = Timestamp::from("2026-05-31T00:00:00.100Z");

        assert!(earlier < later);
    }

    #[test]
    fn epoch_second_strings_are_rejected_as_timestamps() {
        let decoded = serde_json::from_str::<Timestamp>("\"1772323200\"");

        assert!(decoded.is_err());
    }
}
