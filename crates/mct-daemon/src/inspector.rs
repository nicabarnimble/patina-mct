use mct_kernel::{CallId, MctObservation};
use mct_observation::MctObservationLedgerEntry;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctInspectorObservationQuery {
    pub call_id: Option<CallId>,
    pub subject_id: Option<String>,
    pub resource_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctInspectorObservationView {
    pub observations: Vec<MctObservationLedgerEntry>,
    pub returned: usize,
    pub total_matched: usize,
    pub newest_first: bool,
}

pub fn inspect_observation_entries(
    entries: &[MctObservationLedgerEntry],
    query: &MctInspectorObservationQuery,
) -> MctInspectorObservationView {
    let mut matched = entries
        .iter()
        .rev()
        .filter(|entry| observation_matches(&entry.observation, query))
        .cloned()
        .collect::<Vec<_>>();
    let total_matched = matched.len();
    if let Some(limit) = query.limit {
        matched.truncate(limit);
    }
    MctInspectorObservationView {
        returned: matched.len(),
        total_matched,
        observations: matched,
        newest_first: true,
    }
}

fn observation_matches(observation: &MctObservation, query: &MctInspectorObservationQuery) -> bool {
    query
        .call_id
        .as_ref()
        .is_none_or(|call_id| observation.call_id.as_ref() == Some(call_id))
        && query
            .subject_id
            .as_ref()
            .is_none_or(|subject_id| observation.subject_id.as_ref() == Some(subject_id))
        && query
            .resource_id
            .as_ref()
            .is_none_or(|resource_id| observation.resource_id.as_ref() == Some(resource_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mct_kernel::{
        CallId, MctObservation, ObservationId, ObservationKind, ObservationOutcome,
        ObservationTraceRef, ObservationVisibility, SourcePlane, SpanId, Timestamp, TraceId,
    };
    use mct_observation::{DurabilityClass, ExportStatus};

    fn observation(
        observation_id: &str,
        call_id: &str,
        subject_id: &str,
        resource_id: &str,
    ) -> MctObservation {
        MctObservation {
            observation_id: ObservationId::new(observation_id)
                .expect("string ID literal/generated value must be non-empty"),
            observed_at: Timestamp::new("2026-05-31T00:00:00Z").unwrap(),
            kind: ObservationKind::RuntimeExecutionCompleted,
            source_plane: SourcePlane::Adapter,
            trace: ObservationTraceRef {
                trace_id: TraceId::new("trace-inspector")
                    .expect("string ID literal/generated value must be non-empty"),
                span_id: Some(
                    SpanId::new("span-inspector")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                parent_span_id: None,
                external_trace_id: None,
            },
            call_id: Some(
                CallId::new(call_id).expect("string ID literal/generated value must be non-empty"),
            ),
            decision_id: None,
            subject_id: Some(subject_id.into()),
            resource_id: Some(resource_id.into()),
            policy_revision: Some(1),
            grants_revision: Some(1),
            outcome: ObservationOutcome::Completed,
            visibility: ObservationVisibility::InternalOnly,
            safe_message: "completed".into(),
            detail_ref: None,
        }
    }

    fn entry(sequence: u64, observation: MctObservation) -> MctObservationLedgerEntry {
        MctObservationLedgerEntry {
            ledger_id: "ledger-test".into(),
            mother_node_id: "node-test".into(),
            local_sequence: sequence,
            observation,
            previous_entry_hash: None,
            entry_hash: format!("hash-{sequence}"),
            appended_at: sequence.to_string(),
            durability_class: DurabilityClass::BeforeEffect,
            export_status: ExportStatus::NotRequired,
        }
    }

    #[test]
    fn inspector_filters_observations_by_call_child_and_peer() {
        let entries = vec![
            entry(0, observation("obs-0", "call-a", "child-a", "peer-a")),
            entry(1, observation("obs-1", "call-b", "child-b", "peer-a")),
            entry(2, observation("obs-2", "call-a", "child-a", "peer-b")),
        ];

        let by_call = inspect_observation_entries(
            &entries,
            &MctInspectorObservationQuery {
                call_id: Some(
                    CallId::new("call-a")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                ..Default::default()
            },
        );
        assert_eq!(by_call.total_matched, 2);
        assert_eq!(by_call.observations[0].local_sequence, 2);

        let by_child_peer = inspect_observation_entries(
            &entries,
            &MctInspectorObservationQuery {
                subject_id: Some("child-a".into()),
                resource_id: Some("peer-a".into()),
                ..Default::default()
            },
        );
        assert_eq!(by_child_peer.returned, 1);
        assert_eq!(by_child_peer.observations[0].local_sequence, 0);
    }

    #[test]
    fn inspector_limits_recent_observations_without_view_buffers() {
        let entries = vec![
            entry(0, observation("obs-0", "call-a", "child-a", "peer-a")),
            entry(1, observation("obs-1", "call-a", "child-a", "peer-a")),
            entry(2, observation("obs-2", "call-a", "child-a", "peer-a")),
        ];

        let view = inspect_observation_entries(
            &entries,
            &MctInspectorObservationQuery {
                call_id: Some(
                    CallId::new("call-a")
                        .expect("string ID literal/generated value must be non-empty"),
                ),
                limit: Some(2),
                ..Default::default()
            },
        );

        assert!(view.newest_first);
        assert_eq!(view.total_matched, 3);
        assert_eq!(view.returned, 2);
        assert_eq!(view.observations[0].local_sequence, 2);
        assert_eq!(view.observations[1].local_sequence, 1);
    }
}
