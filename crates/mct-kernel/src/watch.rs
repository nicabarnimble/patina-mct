//! Scoped Child watch authority, safe event evidence, and call-out identities.

use crate::{
    MctKernelError, MctKernelResult, error::ensure_non_blank, id::*, toy::AuthorizedToyCall,
};
use serde::{Deserialize, Serialize};

/// Canonical Watch Toy identifier.
pub const MCT_WATCH_TOY_ID: &str = "toy:mct:watch-observation";
/// Canonical Watch Toy resource/action contract.
pub const MCT_WATCH_TOY_ACTION: &str = "observe";
/// Maximum eligible events retained by one watch batch.
pub const MCT_WATCH_MAX_EVENTS_PER_BATCH: u32 = 128;
/// Maximum encoded watcher message accepted by the child call-out bridge.
pub const MCT_WATCH_MESSAGE_MAX_BYTES: usize = 65_536;
/// Maximum metadata pairs accepted on one watcher message.
pub const MCT_WATCH_METADATA_PAIRS_MAX: usize = 16;
/// Maximum nested Child call-out depth.
pub const MCT_CHILD_CALLOUT_MAX_DEPTH: u8 = 1;
/// Maximum key bytes in the bounded Child keyvalue adapter.
pub const MCT_KEYVALUE_KEY_MAX_BYTES: usize = 128;
/// Maximum value bytes in the bounded Child keyvalue adapter.
pub const MCT_KEYVALUE_VALUE_MAX_BYTES: usize = 262_144;
/// Maximum keys retained in one Child keyvalue bucket.
pub const MCT_KEYVALUE_MAX_KEYS_PER_BUCKET: usize = 128;
/// Maximum keys returned by one lexical list operation.
pub const MCT_KEYVALUE_LIST_PAGE_MAX: usize = 128;

/// Closed observer placement supported by a Watch scope.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchObserverShape {
    /// An authorized Child invocation observes through the Watch Toy.
    ChildToy,
}

/// Breadth classification for a canonical Watch root.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchScopeMode {
    /// A narrowly selected application root.
    Constrained,
    /// An operator-explicit broad subtree, still never machine-wide ambient access.
    ExplicitBroad,
}

/// Traversal requested by the authority record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchTraversalScope {
    /// Observe only direct entries; valid law but unsupported by the v1 adapter.
    RootOnly,
    /// Recursively observe entries beneath the exact root.
    Recursive,
}

/// Closed filesystem event classes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchEventClass {
    /// A safe in-scope subject appeared.
    Created,
    /// A prior safe in-scope subject changed.
    Modified,
    /// A prior durably observed in-scope subject disappeared.
    Deleted,
}

/// Deterministic within-batch coalescing policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchCoalescingPolicy {
    /// Retain every eligible event in canonical order.
    None,
    /// Retain the final canonical event for each safe relative path.
    LastPerPath,
}

/// Lifecycle state of one immutable Watch scope revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchObservationScopeState {
    /// The exact revision may authorize observation while otherwise current.
    Active,
    /// The revision was explicitly revoked.
    Revoked,
    /// A newer immutable revision replaced this revision.
    Superseded,
}

/// Exact Child artifact and assignment authorized to observe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchObserverRef {
    /// Stable Child package name.
    pub child_name: String,
    /// Exact acquired component artifact.
    pub artifact_id: ComponentArtifactId,
    /// Exact artifact version.
    pub artifact_version: String,
    /// Exact current Child assignment.
    pub assignment_id: ChildAssignmentId,
}

/// Immutable ledger-backed authority for one Child watch root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WatchObservationScope {
    /// Stable scope identifier across revisions.
    pub watch_scope_id: WatchObservationScopeId,
    /// Observer placement, currently only Child+Toy.
    pub observer_shape: WatchObserverShape,
    /// Exact Child artifact and assignment.
    pub observer_ref: WatchObserverRef,
    /// Explicit breadth classification.
    pub scope_mode: WatchScopeMode,
    /// Credential-free canonical absolute `file:///` URI.
    pub canonical_root_ref: String,
    /// Explicit traversal contract.
    pub traversal_scope: WatchTraversalScope,
    /// Non-empty, unique, canonical ordered event classes.
    pub event_classes: Vec<WatchEventClass>,
    /// Per-record event ceiling.
    pub max_events_per_batch: u32,
    /// Explicit deterministic coalescing behavior.
    pub coalescing_policy: WatchCoalescingPolicy,
    /// Inclusive authority start.
    pub starts_at: Timestamp,
    /// Exclusive authority expiry.
    pub expires_at: Timestamp,
    /// Immutable revision, starting at one.
    pub scope_revision: u64,
    /// Policy revision used for current evaluation.
    pub policy_revision: u64,
    /// Lifecycle state of this exact revision.
    pub authority_state: WatchObservationScopeState,
    /// Durable authority observation preceding projection.
    pub authority_observation_id: ObservationId,
    /// BLAKE3 digest over all preceding fields.
    pub canonical_record_digest: String,
}

#[derive(Serialize)]
struct WatchScopeDigestMaterial<'a> {
    watch_scope_id: &'a WatchObservationScopeId,
    observer_shape: WatchObserverShape,
    observer_ref: &'a WatchObserverRef,
    scope_mode: WatchScopeMode,
    canonical_root_ref: &'a str,
    traversal_scope: WatchTraversalScope,
    event_classes: &'a [WatchEventClass],
    max_events_per_batch: u32,
    coalescing_policy: WatchCoalescingPolicy,
    starts_at: &'a Timestamp,
    expires_at: &'a Timestamp,
    scope_revision: u64,
    policy_revision: u64,
    authority_state: WatchObservationScopeState,
    authority_observation_id: &'a ObservationId,
}

impl WatchObservationScope {
    /// Computes the canonical record digest for this exact revision.
    pub fn expected_record_digest(&self) -> String {
        let bytes = serde_json::to_vec(&WatchScopeDigestMaterial {
            watch_scope_id: &self.watch_scope_id,
            observer_shape: self.observer_shape,
            observer_ref: &self.observer_ref,
            scope_mode: self.scope_mode,
            canonical_root_ref: &self.canonical_root_ref,
            traversal_scope: self.traversal_scope,
            event_classes: &self.event_classes,
            max_events_per_batch: self.max_events_per_batch,
            coalescing_policy: self.coalescing_policy,
            starts_at: &self.starts_at,
            expires_at: &self.expires_at,
            scope_revision: self.scope_revision,
            policy_revision: self.policy_revision,
            authority_state: self.authority_state,
            authority_observation_id: &self.authority_observation_id,
        })
        .expect("closed Watch scope digest material must serialize");
        format!("blake3:{}", blake3::hash(&bytes).to_hex())
    }

    /// Replaces the digest with the canonical value for this revision.
    pub fn seal(mut self) -> Self {
        self.canonical_record_digest = self.expected_record_digest();
        self
    }

    /// Validates the closed scope shape, ordering, window, and digest.
    pub fn validate(&self) -> MctKernelResult<()> {
        ensure_non_blank(
            "WatchObserverRef",
            "child_name",
            &self.observer_ref.child_name,
        )?;
        ensure_non_blank(
            "WatchObserverRef",
            "artifact_version",
            &self.observer_ref.artifact_version,
        )?;
        validate_canonical_file_root(&self.canonical_root_ref)?;
        if self.event_classes.is_empty() {
            return invalid("event_classes", "empty set");
        }
        if self.event_classes.windows(2).any(|pair| pair[0] >= pair[1]) {
            return invalid("event_classes", "not unique canonical order");
        }
        if !(1..=MCT_WATCH_MAX_EVENTS_PER_BATCH).contains(&self.max_events_per_batch) {
            return invalid("max_events_per_batch", "outside named ceiling");
        }
        if self.starts_at >= self.expires_at {
            return invalid("expires_at", "not after starts_at");
        }
        if self.scope_revision == 0 {
            return invalid("scope_revision", "must start at one");
        }
        if self.policy_revision == 0 {
            return invalid("policy_revision", "must be non-zero");
        }
        if self.canonical_record_digest != self.expected_record_digest() {
            return invalid("canonical_record_digest", "digest mismatch");
        }
        Ok(())
    }

    /// Returns true only for an active valid revision at the supplied instant.
    pub fn is_current_at(&self, now: &Timestamp) -> bool {
        self.validate().is_ok()
            && self.authority_state == WatchObservationScopeState::Active
            && self.starts_at <= *now
            && *now < self.expires_at
    }

    /// Exact ToyGrant resource required for this revision.
    pub fn toy_resource_id(&self) -> String {
        format!(
            "watch-scope:{}:{}",
            self.watch_scope_id, self.scope_revision
        )
    }
}

/// Request for composing an already authorized Watch Toy call with current scope law.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WatchObservationSessionRequest {
    /// Exact current Child artifact and assignment projection.
    pub current_observer: WatchObserverRef,
    /// Current instant for scope and token expiry checks.
    pub now: Timestamp,
}

/// Non-clone session capability authorizing one scoped observation session.
#[derive(Debug, PartialEq, Eq)]
pub struct AuthorizedWatchObservationSession {
    authorized_toy_call: AuthorizedToyCall,
    scope: WatchObservationScope,
}

impl AuthorizedWatchObservationSession {
    /// Exact current Watch scope carried by this capability.
    pub fn scope(&self) -> &WatchObservationScope {
        &self.scope
    }

    /// Underlying non-clone Watch Toy capability.
    pub fn authorized_toy_call(&self) -> &AuthorizedToyCall {
        &self.authorized_toy_call
    }
}

/// Composes independent Watch scope and Watch Toy authority into one session.
pub fn authorize_watch_observation_session(
    authorized_toy_call: AuthorizedToyCall,
    scope: &WatchObservationScope,
    request: &WatchObservationSessionRequest,
) -> MctKernelResult<AuthorizedWatchObservationSession> {
    if authorized_toy_call.toy_id().as_str() != MCT_WATCH_TOY_ID {
        return invalid("toy_id", "not canonical Watch Toy");
    }
    if authorized_toy_call.resource_id() != Some(scope.toy_resource_id().as_str()) {
        return invalid("toy_resource", "scope revision mismatch");
    }
    if authorized_toy_call.expires_at() <= &request.now {
        return invalid("toy_expiry", "authorized Toy call expired");
    }
    if scope.observer_ref != request.current_observer {
        return invalid("observer_ref", "not current exact observer");
    }
    if !scope.is_current_at(&request.now) {
        return invalid("authority_state", "scope is not current");
    }
    Ok(AuthorizedWatchObservationSession {
        authorized_toy_call,
        scope: scope.clone(),
    })
}

/// Validates one normalized safe root-relative event path.
pub fn validate_safe_watch_relative_path(path: &str) -> MctKernelResult<()> {
    ensure_non_blank("WatchEventEvidence", "relative_path", path)?;
    if path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\\')
        || path.contains('\0')
        || path.contains("//")
    {
        return invalid("relative_path", "absolute or ambiguous separator");
    }
    for (index, segment) in path.split('/').enumerate() {
        if segment.is_empty() || segment == "." || segment == ".." {
            return invalid("relative_path", "non-canonical segment");
        }
        if index == 0 && segment.ends_with(':') {
            return invalid("relative_path", "platform prefix");
        }
    }
    Ok(())
}

/// Legacy 0.1.x compatibility validation outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyWatchCompatibilityValidation {
    /// Both legacy slots were byte-equal to one safe root-relative path.
    Matched,
    /// Unequal or unsafe slots were refused before target call construction.
    MismatchRefused,
}

/// Validates the exact legacy `patina:watch/events@0.1.x` path narrowing.
pub fn validate_legacy_watch_paths(
    operation_package_interface: &str,
    absolute_path: &str,
    relative_path: &str,
) -> MctKernelResult<LegacyWatchCompatibilityValidation> {
    if !operation_package_interface.starts_with("patina:watch/events@0.1.") {
        return invalid("legacy_interface", "not exact 0.1.x dispatch");
    }
    validate_safe_watch_relative_path(relative_path)?;
    if absolute_path != relative_path {
        return Ok(LegacyWatchCompatibilityValidation::MismatchRefused);
    }
    Ok(LegacyWatchCompatibilityValidation::Matched)
}

/// Durable evidence for one Watch scan batch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchEventBatchEvidence {
    /// Deterministic batch identifier.
    pub batch_id: WatchEventBatchId,
    /// Exact Watch scope.
    pub watch_scope_id: WatchObservationScopeId,
    /// Exact scope revision.
    pub scope_revision: u64,
    /// Monotonic per-scope sequence.
    pub sequence: u64,
    /// Watcher call causing the batch.
    pub parent_call_id: CallId,
    /// Raw source count.
    pub raw_event_count: u32,
    /// Eligible safe count.
    pub eligible_event_count: u32,
    /// Coalesced input count.
    pub coalesced_event_count: u32,
    /// Excluded input count.
    pub excluded_event_count: u32,
    /// Capacity-refused input count.
    pub capacity_refused_event_count: u32,
}

/// Safe causal evidence for one Watch event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchEventEvidence {
    /// Deterministic event identifier.
    pub event_id: WatchEventId,
    /// Containing batch.
    pub batch_id: WatchEventBatchId,
    /// Stable position in canonical batch order.
    pub batch_position: u32,
    /// Closed event class.
    pub event_class: WatchEventClass,
    /// Canonical root-relative subject.
    pub relative_path: String,
    /// Watcher call causing this event.
    pub causative_call_id: CallId,
    /// Actual trigger firing only for a Trigger parent.
    pub causative_trigger_firing_id: Option<CallTriggerFiringId>,
    /// Mother adapter evidence, always absent on this Child path.
    pub causative_adapter_observation_id: Option<ObservationId>,
}

/// Closed pre-call disposition set for Watch delivery.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatchEventDisposition {
    /// A fresh target call was planned.
    Fired,
    /// Input was represented by another deterministic event.
    Coalesced,
    /// Current law suppressed delivery before a target call.
    Suppressed,
    /// A named bound refused delivery before a target call.
    CapacityRefused,
}

/// Durable pre-call delivery disposition.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchEventDeliveryDisposition {
    /// Stable disposition identifier.
    pub disposition_id: WatchEventDeliveryDispositionId,
    /// Exact source event.
    pub event_id: WatchEventId,
    /// Closed pre-call disposition.
    pub disposition: WatchEventDisposition,
    /// Planned call, mandatory only for `fired`.
    pub planned_call_id: Option<CallId>,
    /// Legacy validation result for the exact 0.1.x edge.
    pub compatibility_validation: LegacyWatchCompatibilityValidation,
    /// Durable observation recording receipt/eligibility before effect.
    pub disposition_observation_id: ObservationId,
}

impl WatchEventDeliveryDisposition {
    /// Validates planned-call presence against the closed disposition.
    pub fn validate(&self) -> MctKernelResult<()> {
        match (self.disposition, self.planned_call_id.is_some()) {
            (WatchEventDisposition::Fired, true) => Ok(()),
            (WatchEventDisposition::Fired, false) => {
                invalid("planned_call_id", "missing for fired disposition")
            }
            (_, false) => Ok(()),
            (_, true) => invalid("planned_call_id", "present for non-fired disposition"),
        }
    }
}

/// Durable completion evidence referencing the target call's existing result.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchEventDeliveryEvidence {
    /// Stable delivery identifier.
    pub delivery_id: WatchEventDeliveryId,
    /// Fired pre-call disposition.
    pub disposition_id: WatchEventDeliveryDispositionId,
    /// Exact ordinary target call.
    pub target_call_id: CallId,
    /// Existing target result reference.
    pub target_result_ref: ResultRef,
    /// Existing target `ResultRecorded` observation.
    pub target_result_observation_id: ObservationId,
    /// True only when the durable referenced target result is successful.
    pub delivered: bool,
}

/// Derives the deterministic identity for one per-scope batch sequence.
pub fn derive_watch_batch_id(
    scope_id: &WatchObservationScopeId,
    revision: u64,
    sequence: u64,
    parent_call_id: &CallId,
) -> WatchEventBatchId {
    WatchEventBatchId::new(format!(
        "watch-batch:{}",
        digest_fields(&[
            scope_id.as_str(),
            &revision.to_string(),
            &sequence.to_string(),
            parent_call_id.as_str(),
        ])
    ))
    .expect("derived Watch batch id must be non-empty")
}

/// Derives one event identity from exact parent, batch position, and canonical event JSON.
pub fn derive_watch_callout_event_id(
    parent_call_id: &CallId,
    batch_id: &WatchEventBatchId,
    batch_position: u32,
    canonical_event_json: &str,
) -> WatchEventId {
    WatchEventId::new(format!(
        "watch-event:{}",
        digest_fields(&[
            parent_call_id.as_str(),
            batch_id.as_str(),
            &batch_position.to_string(),
            canonical_event_json,
        ])
    ))
    .expect("derived Watch event id must be non-empty")
}

/// Derives the fresh ordinary WasmHost target call identity.
pub fn derive_watch_callout_call_id(event_id: &WatchEventId) -> CallId {
    CallId::new(format!("call-wasm-host:{}", event_id.as_str()))
        .expect("derived Watch call-out call id must be non-empty")
}

/// Derives the WasmHost replay key for one exact event and target.
pub fn derive_watch_callout_idempotency_key(
    parent_call_id: &CallId,
    event_id: &WatchEventId,
    target_operation: &str,
) -> String {
    format!(
        "wasm-host-v1:{}",
        digest_fields(&[parent_call_id.as_str(), event_id.as_str(), target_operation])
    )
}

fn validate_canonical_file_root(root: &str) -> MctKernelResult<()> {
    ensure_non_blank("WatchObservationScope", "canonical_root_ref", root)?;
    let Some(path) = root.strip_prefix("file:///") else {
        return invalid("canonical_root_ref", "not canonical file URI");
    };
    if path.is_empty()
        || path.contains('@')
        || path.contains('?')
        || path.contains('#')
        || path.contains('%')
        || path.contains('\\')
        || path.contains("//")
        || path.ends_with('/')
        || path
            .split('/')
            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return invalid(
            "canonical_root_ref",
            "credentialed, broad, or non-canonical root",
        );
    }
    Ok(())
}

fn invalid<T>(field: &'static str, reason: &'static str) -> MctKernelResult<T> {
    Err(MctKernelError::InvalidConstraint {
        record: "WatchObservationScope",
        field,
        reason,
    })
}

fn digest_fields(fields: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"mct-watch-v1");
    for field in fields {
        hasher.update(&(field.len() as u64).to_be_bytes());
        hasher.update(field.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observer() -> WatchObserverRef {
        WatchObserverRef {
            child_name: "folder-watch-actor".into(),
            artifact_id: ComponentArtifactId::new(format!("sha256:{}", "a".repeat(64))).unwrap(),
            artifact_version: "0.1.0".into(),
            assignment_id: ChildAssignmentId::new("assignment-watch").unwrap(),
        }
    }

    fn scope() -> WatchObservationScope {
        WatchObservationScope {
            watch_scope_id: WatchObservationScopeId::new("scope-watch").unwrap(),
            observer_shape: WatchObserverShape::ChildToy,
            observer_ref: observer(),
            scope_mode: WatchScopeMode::Constrained,
            canonical_root_ref: "file:///tmp/watch-root".into(),
            traversal_scope: WatchTraversalScope::Recursive,
            event_classes: vec![
                WatchEventClass::Created,
                WatchEventClass::Modified,
                WatchEventClass::Deleted,
            ],
            max_events_per_batch: 32,
            coalescing_policy: WatchCoalescingPolicy::LastPerPath,
            starts_at: Timestamp::new("2026-07-21T12:00:00Z").unwrap(),
            expires_at: Timestamp::new("2026-07-21T13:00:00Z").unwrap(),
            scope_revision: 1,
            policy_revision: 1,
            authority_state: WatchObservationScopeState::Active,
            authority_observation_id: ObservationId::new("obs-watch-authority").unwrap(),
            canonical_record_digest: String::new(),
        }
        .seal()
    }

    #[test]
    fn watch_scope_validation_is_closed_bounded_and_digest_bound() {
        let valid = scope();
        valid.validate().unwrap();
        assert!(valid.is_current_at(&Timestamp::new("2026-07-21T12:30:00Z").unwrap()));

        let mut invalid_batch = valid.clone();
        invalid_batch.max_events_per_batch = MCT_WATCH_MAX_EVENTS_PER_BATCH + 1;
        invalid_batch = invalid_batch.seal();
        assert!(invalid_batch.validate().is_err());

        let mut unordered = valid.clone();
        unordered.event_classes = vec![WatchEventClass::Deleted, WatchEventClass::Created];
        unordered = unordered.seal();
        assert!(unordered.validate().is_err());

        let mut broad = valid.clone();
        broad.canonical_root_ref = "file:///".into();
        broad = broad.seal();
        assert!(broad.validate().is_err());

        let mut tampered = valid;
        tampered.observer_ref.artifact_version = "0.2.0".into();
        assert!(tampered.validate().is_err());
    }

    #[test]
    fn legacy_watch_equality_is_exact_and_path_safe() {
        assert_eq!(
            validate_legacy_watch_paths(
                "patina:watch/events@0.1.0",
                "nested/file.txt",
                "nested/file.txt"
            )
            .unwrap(),
            LegacyWatchCompatibilityValidation::Matched
        );
        assert_eq!(
            validate_legacy_watch_paths(
                "patina:watch/events@0.1.0",
                "/input/nested/file.txt",
                "nested/file.txt"
            )
            .unwrap(),
            LegacyWatchCompatibilityValidation::MismatchRefused
        );
        assert!(validate_safe_watch_relative_path("../escape").is_err());
        assert!(
            validate_legacy_watch_paths("patina:watch/events@1.0.0", "file.txt", "file.txt")
                .is_err()
        );
    }

    #[test]
    fn watch_batch_and_callout_identities_are_deterministic_and_distinct() {
        let scope_id = WatchObservationScopeId::new("scope-watch").unwrap();
        let parent = CallId::new("call-parent").unwrap();
        let first = derive_watch_batch_id(&scope_id, 1, 1, &parent);
        assert_eq!(first, derive_watch_batch_id(&scope_id, 1, 1, &parent));
        assert_ne!(first, derive_watch_batch_id(&scope_id, 1, 2, &parent));
        let event = derive_watch_callout_event_id(&parent, &first, 0, r#"{"path":"file.txt"}"#);
        assert!(
            derive_watch_callout_call_id(&event)
                .as_str()
                .starts_with("call-wasm-host:")
        );
        assert_eq!(
            derive_watch_callout_idempotency_key(&parent, &event, "patina:watch/events@0.1.0.emit"),
            derive_watch_callout_idempotency_key(&parent, &event, "patina:watch/events@0.1.0.emit")
        );
    }
}
