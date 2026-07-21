//! Standing call-trigger authority and pure occurrence admission decisions.
//!
//! This module contains no clock, storage, scheduler, or call execution. Adapters
//! supply current records and known occurrences; the kernel validates scope,
//! derives stable identities, and applies the closed missed-fire/overlap laws.

use crate::{
    CallId, CallTriggerAuthorityId, CallTriggerFiringId, CallTriggerOccurrenceId,
    CallTriggerPendingOccurrenceId, CallerIdentity, MctCallPayloadHandle, MctKernelError,
    MctKernelResult, MctNodeId, ObservationId, OperationTarget, Timestamp, VisionId,
    error::ensure_non_blank,
};
use serde::{Deserialize, Serialize};

/// Minimum interval admitted by the v1 temporal trigger contract.
pub const MCT_TRIGGER_MIN_INTERVAL_MS: u64 = 100;

/// Source class for a standing trigger record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallTriggerClass {
    /// Occurrences are derived from an anchored fixed interval.
    Temporal,
    /// Occurrences are supplied by an independently authorized event adapter.
    Event,
}

/// Explicit behavior for occurrences known to have been missed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissedFirePolicy {
    /// Record the represented set and create no call.
    #[default]
    Skip,
    /// Admit one deterministic representative of the complete known set.
    CoalesceOne,
    /// Admit ordered individual representatives up to a supplied bound.
    FireLateBounded,
}

/// Explicit behavior when the same trigger already has an active target call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlapPolicy {
    /// Suppress the occurrence and create no pending work.
    #[default]
    Refuse,
    /// Retain at most one pending representative.
    CoalesceOne,
    /// Retain ordered pending representatives up to the supplied bound.
    QueueBounded,
}

/// Lifecycle state of one immutable trigger record revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallTriggerAuthorityState {
    /// This is the current revision and may be evaluated.
    Active,
    /// An authenticated owner revoked this trigger id.
    Revoked,
    /// A later immutable revision replaced this revision.
    Superseded,
}

/// Exact source shape bound into one trigger authority revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CallTriggerSource {
    /// Anchored fixed-interval temporal source.
    Temporal {
        /// RFC3339 anchor used for nominal occurrence arithmetic.
        anchor_at: Timestamp,
        /// Fixed interval in milliseconds.
        interval_ms: u64,
    },
    /// Future independently authorized Mother-side event source.
    Event {
        /// Opaque digest/reference to the event source scope.
        event_source_ref: String,
    },
}

impl CallTriggerSource {
    /// Returns the closed class represented by this source.
    pub const fn class(&self) -> CallTriggerClass {
        match self {
            Self::Temporal { .. } => CallTriggerClass::Temporal,
            Self::Event { .. } => CallTriggerClass::Event,
        }
    }

    /// Validates the source-local shape and lower interval bound.
    pub fn validate(&self) -> MctKernelResult<()> {
        match self {
            Self::Temporal { interval_ms, .. } if *interval_ms < MCT_TRIGGER_MIN_INTERVAL_MS => {
                Err(MctKernelError::InvalidConstraint {
                    record: "CallTriggerSource",
                    field: "interval_ms",
                    reason: "below minimum",
                })
            }
            Self::Temporal { .. } => Ok(()),
            Self::Event { event_source_ref } => {
                ensure_non_blank("CallTriggerSource", "event_source_ref", event_source_ref)
            }
        }
    }
}

/// Immutable authority-bearing trigger record revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallTriggerAuthority {
    /// Stable standing-record identifier.
    pub trigger_authority_id: CallTriggerAuthorityId,
    /// Mother identity on which the record may fire.
    pub mother_node_id: MctNodeId,
    /// Vision in which the record may fire.
    pub vision_id: VisionId,
    /// Authenticated canonical caller copied into fresh calls.
    pub canonical_caller: CallerIdentity,
    /// One exact WIT operation target.
    pub target: OperationTarget,
    /// Immutable local content-addressed payload descriptor.
    pub payload_constraint: MctCallPayloadHandle,
    /// Exact temporal or event source.
    pub trigger_source: CallTriggerSource,
    /// Digest of the canonical source shape.
    pub trigger_source_ref: String,
    /// Missed-fire behavior; omission at JSON edges defaults to skip.
    #[serde(default)]
    pub missed_fire_policy: MissedFirePolicy,
    /// Overlap behavior; omission at JSON edges defaults to refuse.
    #[serde(default)]
    pub overlap_policy: OverlapPolicy,
    /// Authenticated issuer reference, normally `os-uid:<uid>`.
    pub issuer_principal_ref: String,
    /// Immutable record revision starting at one.
    pub record_revision: u64,
    /// Policy revision used to evaluate the record.
    pub policy_revision: u64,
    /// Inclusive beginning of the authority window.
    pub starts_at: Timestamp,
    /// Exclusive end of the authority window.
    pub expires_at: Timestamp,
    /// Current lifecycle state of this revision.
    pub authority_state: CallTriggerAuthorityState,
    /// Durable authority observation that precedes activation.
    pub authority_observation_id: ObservationId,
    /// BLAKE3 digest over all preceding canonical authority fields.
    pub canonical_record_digest: String,
}

#[derive(Serialize)]
struct CallTriggerAuthorityDigestMaterial<'a> {
    trigger_authority_id: &'a CallTriggerAuthorityId,
    mother_node_id: &'a MctNodeId,
    vision_id: &'a VisionId,
    canonical_caller: &'a CallerIdentity,
    target: &'a OperationTarget,
    payload_constraint: &'a MctCallPayloadHandle,
    trigger_source: &'a CallTriggerSource,
    trigger_source_ref: &'a str,
    missed_fire_policy: MissedFirePolicy,
    overlap_policy: OverlapPolicy,
    issuer_principal_ref: &'a str,
    record_revision: u64,
    policy_revision: u64,
    starts_at: &'a Timestamp,
    expires_at: &'a Timestamp,
    authority_state: CallTriggerAuthorityState,
    authority_observation_id: &'a ObservationId,
}

impl CallTriggerAuthority {
    /// Computes the canonical digest expected for this exact record revision.
    pub fn expected_record_digest(&self) -> String {
        let material = CallTriggerAuthorityDigestMaterial {
            trigger_authority_id: &self.trigger_authority_id,
            mother_node_id: &self.mother_node_id,
            vision_id: &self.vision_id,
            canonical_caller: &self.canonical_caller,
            target: &self.target,
            payload_constraint: &self.payload_constraint,
            trigger_source: &self.trigger_source,
            trigger_source_ref: &self.trigger_source_ref,
            missed_fire_policy: self.missed_fire_policy,
            overlap_policy: self.overlap_policy,
            issuer_principal_ref: &self.issuer_principal_ref,
            record_revision: self.record_revision,
            policy_revision: self.policy_revision,
            starts_at: &self.starts_at,
            expires_at: &self.expires_at,
            authority_state: self.authority_state,
            authority_observation_id: &self.authority_observation_id,
        };
        let bytes =
            serde_json::to_vec(&material).expect("closed trigger digest material must serialize");
        format!("blake3:{}", blake3::hash(&bytes).to_hex())
    }

    /// Replaces the digest with the canonical value for the current fields.
    pub fn seal(mut self) -> Self {
        self.canonical_record_digest = self.expected_record_digest();
        self
    }

    /// Validates closed scope, source, payload, validity, and digest invariants.
    pub fn validate(&self) -> MctKernelResult<()> {
        self.canonical_caller.validate()?;
        self.target.validate()?;
        self.payload_constraint.validate()?;
        self.trigger_source.validate()?;
        ensure_non_blank(
            "CallTriggerAuthority",
            "trigger_source_ref",
            &self.trigger_source_ref,
        )?;
        ensure_non_blank(
            "CallTriggerAuthority",
            "issuer_principal_ref",
            &self.issuer_principal_ref,
        )?;
        if self.record_revision == 0 {
            return Err(MctKernelError::InvalidConstraint {
                record: "CallTriggerAuthority",
                field: "record_revision",
                reason: "must start at one",
            });
        }
        if self.policy_revision == 0 {
            return Err(MctKernelError::InvalidConstraint {
                record: "CallTriggerAuthority",
                field: "policy_revision",
                reason: "must start at one",
            });
        }
        if self.starts_at >= self.expires_at {
            return Err(MctKernelError::InvalidConstraint {
                record: "CallTriggerAuthority",
                field: "expires_at",
                reason: "must follow starts_at",
            });
        }
        if self.canonical_caller.node_id != self.mother_node_id
            || self.canonical_caller.vision_id != self.vision_id
        {
            return Err(MctKernelError::InvalidConstraint {
                record: "CallTriggerAuthority",
                field: "canonical_caller",
                reason: "must match Mother and Vision scope",
            });
        }
        if let CallTriggerSource::Temporal { anchor_at, .. } = &self.trigger_source
            && anchor_at >= &self.expires_at
        {
            return Err(MctKernelError::InvalidConstraint {
                record: "CallTriggerAuthority",
                field: "anchor_at",
                reason: "must admit an occurrence before expiry",
            });
        }
        if !matches!(
            self.payload_constraint,
            MctCallPayloadHandle::ContentAddressedBlob { .. } | MctCallPayloadHandle::Empty
        ) {
            return Err(MctKernelError::InvalidConstraint {
                record: "CallTriggerAuthority",
                field: "payload_constraint",
                reason: "must be local content-addressed or empty",
            });
        }
        if self.canonical_record_digest != self.expected_record_digest() {
            return Err(MctKernelError::InvalidConstraint {
                record: "CallTriggerAuthority",
                field: "canonical_record_digest",
                reason: "digest mismatch",
            });
        }
        Ok(())
    }

    /// Returns true only when this revision is active at the supplied instant.
    pub fn is_current_at(&self, now: &Timestamp) -> bool {
        self.authority_state == CallTriggerAuthorityState::Active
            && self.starts_at <= *now
            && *now < self.expires_at
            && self.validate().is_ok()
    }
}

/// Exact countable occurrence set represented by one decision or candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallTriggerRepresentedSet {
    /// First occurrence in deterministic source order.
    pub first_occurrence_id: CallTriggerOccurrenceId,
    /// Last occurrence in deterministic source order.
    pub last_occurrence_id: CallTriggerOccurrenceId,
    /// Number represented, including first and last.
    pub count: u64,
    /// Stable digest reference for the represented set.
    pub represented_set_ref: String,
}

/// One known occurrence supplied to missed-fire evaluation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownCallTriggerOccurrence {
    /// Deterministic occurrence identity.
    pub occurrence_id: CallTriggerOccurrenceId,
    /// Nominal temporal time or event receipt ordering time.
    pub nominal_at: Timestamp,
}

/// Candidate that may proceed to overlap evaluation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallTriggerOccurrenceCandidate {
    /// Identity used by firing/pending/call derivations.
    pub occurrence_id: CallTriggerOccurrenceId,
    /// Complete exact set represented by this candidate.
    pub represented_set: CallTriggerRepresentedSet,
}

/// Terminal pre-overlap disposition of a known occurrence set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallTriggerTerminalDispositionKind {
    /// Missed occurrences were intentionally skipped.
    Skipped,
    /// Current authority or overlap refused the set.
    Suppressed,
    /// A named capacity bound refused the set.
    CapacityRefused,
}

/// Terminal disposition and exact represented occurrence set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallTriggerTerminalDisposition {
    /// Closed terminal class.
    pub kind: CallTriggerTerminalDispositionKind,
    /// Complete exact set that can never be reconstructed as missed.
    pub represented_set: CallTriggerRepresentedSet,
}

/// Pure missed-fire decision before overlap or capacity admission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallTriggerMissedFireDecision {
    /// Ordered candidates passed to overlap evaluation.
    pub candidates: Vec<CallTriggerOccurrenceCandidate>,
    /// Ordered terminal sets that produce no candidate.
    pub terminal: Vec<CallTriggerTerminalDisposition>,
}

/// Why a candidate entered durable pending state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallTriggerPendingReason {
    /// One representative is retained and expanded by later overlaps.
    OverlapCoalesced,
    /// Individual representatives retain deterministic queue order.
    OverlapQueued,
}

/// Pure overlap decision for one candidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case", deny_unknown_fields)]
pub enum CallTriggerOverlapDecision {
    /// No active call exists; proceed to active-capacity admission.
    FireNow,
    /// Refuse terminally because another call is active.
    Suppressed,
    /// Append one new deterministic pending item.
    Pending {
        /// Pending reason implied by the selected overlap policy.
        reason: CallTriggerPendingReason,
    },
    /// Merge this set into the existing coalesced pending item.
    CoalescedInto {
        /// Existing deterministic pending identity.
        pending_occurrence_id: CallTriggerPendingOccurrenceId,
    },
    /// The per-record pending bound refused this candidate.
    CapacityRefused,
}

fn hash_segments(domain: &str, segments: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    for segment in std::iter::once(domain).chain(segments.iter().copied()) {
        let bytes = segment.as_bytes();
        hasher.update(&(bytes.len() as u64).to_le_bytes());
        hasher.update(bytes);
    }
    hasher.finalize().to_hex().to_string()
}

/// Derives one temporal occurrence identity from exact record and nominal time.
pub fn derive_temporal_occurrence_identity(
    trigger_authority_id: &str,
    record_revision: u64,
    nominal_at: &str,
) -> CallTriggerOccurrenceId {
    CallTriggerOccurrenceId::new(format!(
        "occurrence:{}",
        hash_segments(
            "mct-trigger-occurrence-v1",
            &[
                trigger_authority_id,
                &record_revision.to_string(),
                "temporal",
                nominal_at,
            ],
        )
    ))
    .expect("generated occurrence identity must be non-empty")
}

/// Derives a represented-set reference from exact ordered occurrence bounds.
pub fn derive_represented_set_ref(
    trigger_authority_id: &str,
    record_revision: u64,
    first: &CallTriggerOccurrenceId,
    last: &CallTriggerOccurrenceId,
    count: u64,
) -> String {
    format!(
        "represented-set:{}",
        hash_segments(
            "mct-trigger-represented-set-v1",
            &[
                trigger_authority_id,
                &record_revision.to_string(),
                first.as_str(),
                last.as_str(),
                &count.to_string(),
            ],
        )
    )
}

/// Builds an exact represented set from deterministic bounds and count.
pub fn trigger_represented_set_from_bounds(
    trigger_authority_id: &CallTriggerAuthorityId,
    record_revision: u64,
    first_occurrence_id: CallTriggerOccurrenceId,
    last_occurrence_id: CallTriggerOccurrenceId,
    count: u64,
) -> MctKernelResult<CallTriggerRepresentedSet> {
    if count == 0 {
        return Err(MctKernelError::InvalidConstraint {
            record: "CallTriggerRepresentedSet",
            field: "count",
            reason: "must be positive",
        });
    }
    Ok(CallTriggerRepresentedSet {
        represented_set_ref: derive_represented_set_ref(
            trigger_authority_id.as_str(),
            record_revision,
            &first_occurrence_id,
            &last_occurrence_id,
            count,
        ),
        first_occurrence_id,
        last_occurrence_id,
        count,
    })
}

/// Derives one representative occurrence identity for an exact coalesced set.
pub fn derive_coalesced_occurrence_identity(
    represented_set: &CallTriggerRepresentedSet,
) -> CallTriggerOccurrenceId {
    CallTriggerOccurrenceId::new(format!(
        "occurrence-coalesced:{}",
        hash_segments(
            "mct-trigger-coalesced-occurrence-v1",
            &[represented_set.represented_set_ref.as_str()],
        )
    ))
    .expect("generated coalesced occurrence identity must be non-empty")
}

/// Derives the stable firing identity for one occurrence candidate.
pub fn derive_trigger_firing_identity(
    occurrence_id: &CallTriggerOccurrenceId,
) -> CallTriggerFiringId {
    CallTriggerFiringId::new(format!("firing:{occurrence_id}"))
        .expect("generated firing identity must be non-empty")
}

/// Derives the stable semantic call identity for one trigger occurrence.
pub fn derive_trigger_call_identity(occurrence_id: &CallTriggerOccurrenceId) -> CallId {
    CallId::new(format!("call-trigger:{occurrence_id}"))
        .expect("generated trigger call identity must be non-empty")
}

/// Derives the stable pending identity for one trigger occurrence.
pub fn derive_trigger_pending_identity(
    occurrence_id: &CallTriggerOccurrenceId,
) -> CallTriggerPendingOccurrenceId {
    CallTriggerPendingOccurrenceId::new(format!("pending:{occurrence_id}"))
        .expect("generated pending identity must be non-empty")
}

/// Derives the request idempotency key for exact record/revision/occurrence scope.
pub fn derive_trigger_idempotency_key(
    trigger_authority_id: &CallTriggerAuthorityId,
    record_revision: u64,
    occurrence_id: &CallTriggerOccurrenceId,
) -> String {
    format!(
        "trigger-v1:{}",
        hash_segments(
            "mct-trigger-idempotency-v1",
            &[
                trigger_authority_id.as_str(),
                &record_revision.to_string(),
                occurrence_id.as_str(),
            ],
        )
    )
}

fn represented_set(
    trigger_authority_id: &CallTriggerAuthorityId,
    record_revision: u64,
    known: &[KnownCallTriggerOccurrence],
) -> Option<CallTriggerRepresentedSet> {
    let first = known.first()?.occurrence_id.clone();
    let last = known.last()?.occurrence_id.clone();
    let count = known.len() as u64;
    Some(CallTriggerRepresentedSet {
        represented_set_ref: derive_represented_set_ref(
            trigger_authority_id.as_str(),
            record_revision,
            &first,
            &last,
            count,
        ),
        first_occurrence_id: first,
        last_occurrence_id: last,
        count,
    })
}

/// Applies the explicit missed-fire policy to an ordered known occurrence set.
///
/// The adapter must supply deterministic source order. Event gaps must not be
/// fabricated as known occurrences.
pub fn evaluate_missed_fire_policy(
    trigger_authority_id: &CallTriggerAuthorityId,
    record_revision: u64,
    policy: MissedFirePolicy,
    known: &[KnownCallTriggerOccurrence],
    fire_late_limit: usize,
) -> CallTriggerMissedFireDecision {
    if known.is_empty() {
        return CallTriggerMissedFireDecision {
            candidates: Vec::new(),
            terminal: Vec::new(),
        };
    }

    match policy {
        MissedFirePolicy::Skip => CallTriggerMissedFireDecision {
            candidates: Vec::new(),
            terminal: vec![CallTriggerTerminalDisposition {
                kind: CallTriggerTerminalDispositionKind::Skipped,
                represented_set: represented_set(trigger_authority_id, record_revision, known)
                    .expect("known occurrence set is non-empty"),
            }],
        },
        MissedFirePolicy::CoalesceOne => {
            let represented_set = represented_set(trigger_authority_id, record_revision, known)
                .expect("known occurrence set is non-empty");
            let occurrence_id = derive_coalesced_occurrence_identity(&represented_set);
            CallTriggerMissedFireDecision {
                candidates: vec![CallTriggerOccurrenceCandidate {
                    occurrence_id,
                    represented_set,
                }],
                terminal: Vec::new(),
            }
        }
        MissedFirePolicy::FireLateBounded => {
            let admitted = known.len().min(fire_late_limit);
            let candidates = known[..admitted]
                .iter()
                .map(|occurrence| CallTriggerOccurrenceCandidate {
                    occurrence_id: occurrence.occurrence_id.clone(),
                    represented_set: represented_set(
                        trigger_authority_id,
                        record_revision,
                        std::slice::from_ref(occurrence),
                    )
                    .expect("single occurrence set is non-empty"),
                })
                .collect();
            let terminal =
                represented_set(trigger_authority_id, record_revision, &known[admitted..])
                    .map(|represented_set| CallTriggerTerminalDisposition {
                        kind: CallTriggerTerminalDispositionKind::CapacityRefused,
                        represented_set,
                    })
                    .into_iter()
                    .collect();
            CallTriggerMissedFireDecision {
                candidates,
                terminal,
            }
        }
    }
}

/// Applies overlap policy after missed-fire evaluation.
pub fn evaluate_overlap_policy(
    policy: OverlapPolicy,
    has_active_call: bool,
    pending_count: usize,
    pending_limit: usize,
    existing_coalesced_pending: Option<CallTriggerPendingOccurrenceId>,
) -> CallTriggerOverlapDecision {
    if !has_active_call {
        return CallTriggerOverlapDecision::FireNow;
    }
    match policy {
        OverlapPolicy::Refuse => CallTriggerOverlapDecision::Suppressed,
        OverlapPolicy::CoalesceOne => {
            if let Some(pending_occurrence_id) = existing_coalesced_pending {
                CallTriggerOverlapDecision::CoalescedInto {
                    pending_occurrence_id,
                }
            } else if pending_count < pending_limit {
                CallTriggerOverlapDecision::Pending {
                    reason: CallTriggerPendingReason::OverlapCoalesced,
                }
            } else {
                CallTriggerOverlapDecision::CapacityRefused
            }
        }
        OverlapPolicy::QueueBounded if pending_count < pending_limit => {
            CallTriggerOverlapDecision::Pending {
                reason: CallTriggerPendingReason::OverlapQueued,
            }
        }
        OverlapPolicy::QueueBounded => CallTriggerOverlapDecision::CapacityRefused,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProjectId, UserId};

    fn known(id: &str, nominal_at: &str) -> KnownCallTriggerOccurrence {
        KnownCallTriggerOccurrence {
            occurrence_id: CallTriggerOccurrenceId::new(id).unwrap(),
            nominal_at: Timestamp::new(nominal_at).unwrap(),
        }
    }

    fn authority() -> CallTriggerAuthority {
        CallTriggerAuthority {
            trigger_authority_id: CallTriggerAuthorityId::new("trigger-a").unwrap(),
            mother_node_id: MctNodeId::new("node-a").unwrap(),
            vision_id: VisionId::new("vision-a").unwrap(),
            canonical_caller: CallerIdentity {
                node_id: MctNodeId::new("node-a").unwrap(),
                user_id: Some(UserId::new("uid:501").unwrap()),
                vision_id: VisionId::new("vision-a").unwrap(),
                project_id: Some(ProjectId::new("project-a").unwrap()),
            },
            target: OperationTarget::new("patina:watch", "control@0.1.0", "scan-now").unwrap(),
            payload_constraint: MctCallPayloadHandle::Empty,
            trigger_source: CallTriggerSource::Temporal {
                anchor_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
                interval_ms: 1_000,
            },
            trigger_source_ref: "blake3:source".into(),
            missed_fire_policy: MissedFirePolicy::Skip,
            overlap_policy: OverlapPolicy::Refuse,
            issuer_principal_ref: "os-uid:501".into(),
            record_revision: 1,
            policy_revision: 1,
            starts_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
            expires_at: Timestamp::new("2026-07-21T13:00:00Z").unwrap(),
            authority_state: CallTriggerAuthorityState::Active,
            authority_observation_id: ObservationId::new("obs-trigger-a-1").unwrap(),
            canonical_record_digest: String::new(),
        }
        .seal()
    }

    #[test]
    fn trigger_firing_identities_are_record_revision_and_occurrence_scoped() {
        let first = derive_temporal_occurrence_identity("trigger-a", 1, "2026-07-21T12:00:00Z");
        let retry = derive_temporal_occurrence_identity("trigger-a", 1, "2026-07-21T12:00:00Z");
        let other_record =
            derive_temporal_occurrence_identity("trigger-b", 1, "2026-07-21T12:00:00Z");
        let other_revision =
            derive_temporal_occurrence_identity("trigger-a", 2, "2026-07-21T12:00:00Z");
        assert_eq!(first, retry);
        assert_ne!(first, other_record);
        assert_ne!(first, other_revision);
        assert_ne!(
            derive_trigger_idempotency_key(
                &CallTriggerAuthorityId::new("trigger-a").unwrap(),
                1,
                &first,
            ),
            derive_trigger_idempotency_key(
                &CallTriggerAuthorityId::new("trigger-b").unwrap(),
                1,
                &first,
            )
        );
    }

    #[test]
    fn trigger_default_policies_are_skip_and_refuse() {
        assert_eq!(MissedFirePolicy::default(), MissedFirePolicy::Skip);
        assert_eq!(OverlapPolicy::default(), OverlapPolicy::Refuse);
        let decoded: serde_json::Value =
            serde_json::from_str(r#"{"missed_fire_policy":"skip","overlap_policy":"refuse"}"#)
                .unwrap();
        assert_eq!(decoded["missed_fire_policy"], "skip");
    }

    #[test]
    fn trigger_authority_validation_is_closed_and_bounded() {
        let record = authority();
        record.validate().unwrap();
        assert!(record.is_current_at(&Timestamp::new("2026-07-21T12:30:00Z").unwrap()));

        let mut too_fast = record.clone();
        too_fast.trigger_source = CallTriggerSource::Temporal {
            anchor_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
            interval_ms: MCT_TRIGGER_MIN_INTERVAL_MS - 1,
        };
        too_fast = too_fast.seal();
        assert!(matches!(
            too_fast.validate(),
            Err(MctKernelError::InvalidConstraint {
                field: "interval_ms",
                ..
            })
        ));

        let mut tampered = record;
        tampered.target.function_name = "other".into();
        assert!(matches!(
            tampered.validate(),
            Err(MctKernelError::InvalidConstraint {
                field: "canonical_record_digest",
                ..
            })
        ));
    }

    #[test]
    fn trigger_missed_fire_policies_are_bounded_deterministic_and_countable() {
        let id = CallTriggerAuthorityId::new("trigger-a").unwrap();
        let occurrences = vec![
            known("occurrence-1", "2026-07-21T12:00:01Z"),
            known("occurrence-2", "2026-07-21T12:00:02Z"),
            known("occurrence-3", "2026-07-21T12:00:03Z"),
        ];
        let skipped = evaluate_missed_fire_policy(&id, 1, MissedFirePolicy::Skip, &occurrences, 2);
        assert!(skipped.candidates.is_empty());
        assert_eq!(skipped.terminal[0].represented_set.count, 3);

        let coalesced =
            evaluate_missed_fire_policy(&id, 1, MissedFirePolicy::CoalesceOne, &occurrences, 2);
        assert_eq!(coalesced.candidates.len(), 1);
        assert_eq!(coalesced.candidates[0].represented_set.count, 3);
        assert_eq!(
            coalesced,
            evaluate_missed_fire_policy(&id, 1, MissedFirePolicy::CoalesceOne, &occurrences, 2,)
        );

        let late =
            evaluate_missed_fire_policy(&id, 1, MissedFirePolicy::FireLateBounded, &occurrences, 2);
        assert_eq!(late.candidates.len(), 2);
        assert_eq!(late.terminal[0].represented_set.count, 1);
        assert_eq!(
            late.terminal[0].kind,
            CallTriggerTerminalDispositionKind::CapacityRefused
        );
    }

    #[test]
    fn trigger_overlap_policies_preserve_one_active_call_and_order() {
        assert_eq!(
            evaluate_overlap_policy(OverlapPolicy::Refuse, true, 0, 16, None),
            CallTriggerOverlapDecision::Suppressed
        );
        assert_eq!(
            evaluate_overlap_policy(OverlapPolicy::CoalesceOne, true, 0, 16, None),
            CallTriggerOverlapDecision::Pending {
                reason: CallTriggerPendingReason::OverlapCoalesced
            }
        );
        let pending = CallTriggerPendingOccurrenceId::new("pending-a").unwrap();
        assert_eq!(
            evaluate_overlap_policy(
                OverlapPolicy::CoalesceOne,
                true,
                1,
                16,
                Some(pending.clone())
            ),
            CallTriggerOverlapDecision::CoalescedInto {
                pending_occurrence_id: pending
            }
        );
        assert_eq!(
            evaluate_overlap_policy(OverlapPolicy::QueueBounded, true, 16, 16, None),
            CallTriggerOverlapDecision::CapacityRefused
        );
        assert_eq!(
            evaluate_overlap_policy(OverlapPolicy::QueueBounded, false, 16, 16, None),
            CallTriggerOverlapDecision::FireNow
        );
    }
}
