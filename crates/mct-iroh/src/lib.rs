//! Iroh adapter boundary for MCT peer protocols.
//!
//! This crate owns Mother-owned Iroh endpoint lifecycle and MCT ALPN protocol
//! effects. It translates Iroh facts into `mct-kernel` domain records rather
//! than making Iroh transport identity into MCT authority.

#![forbid(unsafe_code)]

mod endpoint;
#[cfg(test)]
mod observation;
#[cfg(test)]
mod test_support;

pub use endpoint::{
    MotherIrohEndpoint, MotherIrohEndpointLifecycle, MotherIrohEndpointSnapshot,
    MotherIrohRelayMode,
};

/// Returns the crate version for health and smoke tests.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoint::mct_alpns;
    use crate::observation::local_denied_peer_adapter_observations;
    use crate::test_support::{run_local_iroh_echo_roundtrip, run_unknown_peer_denial_roundtrip};
    use mct_kernel::*;

    #[test]
    fn exposes_version() {
        assert_eq!(super::version(), "0.1.0");
    }

    #[tokio::test]
    async fn mother_owned_endpoint_starts_and_closes() {
        let mut endpoint = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let bound = endpoint.snapshot();

        assert_eq!(bound.lifecycle, MotherIrohEndpointLifecycle::Bound);
        assert_eq!(bound.relay_mode, MotherIrohRelayMode::Disabled);
        assert!(!bound.endpoint_id.as_str().is_empty());
        assert_eq!(bound.accepted_alpns, mct_alpns());
        assert!(bound.relay_urls.is_empty());

        endpoint.close().await;
        let closed = endpoint.snapshot();
        assert_eq!(closed.lifecycle, MotherIrohEndpointLifecycle::Closed);
        assert_eq!(closed.endpoint_id, bound.endpoint_id);
        assert_eq!(closed.accepted_alpns, bound.accepted_alpns);

        endpoint.close().await;
        assert_eq!(
            endpoint.snapshot().lifecycle,
            MotherIrohEndpointLifecycle::Closed
        );
    }

    #[tokio::test]
    async fn local_iroh_completes_mct_hello_then_call() {
        let report = run_local_iroh_echo_roundtrip().await.unwrap();
        assert_eq!(report.hello_response.hello_outcome, HelloOutcome::Admitted);
        assert_eq!(
            report.call_reply.reply_outcome,
            CallProtocolReplyOutcome::Success
        );
        assert_eq!(
            report.call_reply.result_ref,
            Some(ResultRef::from("result-iroh-echo"))
        );
    }

    #[tokio::test]
    async fn iroh_adapter_observations_cover_endpoint_and_protocol_events() {
        let mut endpoint = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let bound = endpoint.snapshot();
        endpoint.close().await;
        let closed = endpoint.snapshot();
        let report = run_unknown_peer_denial_roundtrip().await.unwrap();

        let observations = local_denied_peer_adapter_observations(
            &bound,
            &closed,
            &report,
            TraceId::from("trace-obs"),
        );

        let kinds = observations
            .iter()
            .map(|observation| observation.kind)
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                ObservationKind::AdapterEffectStarted,
                ObservationKind::PeerHelloReceived,
                ObservationKind::PeerRejected,
                ObservationKind::PeerCallReceived,
                ObservationKind::PeerCallReplied,
                ObservationKind::AdapterEffectCompleted,
            ]
        );
        assert!(
            observations
                .iter()
                .all(|observation| observation.source_plane == SourcePlane::Adapter)
        );
        assert_eq!(
            observations[2].decision_id,
            Some(report.hello_evaluation.decision_id)
        );
        assert_eq!(observations[3].call_id, report.call_evaluation.call_id);
        assert_eq!(
            observations[4].decision_id,
            Some(report.call_evaluation.decision_id)
        );
        assert_eq!(observations[4].outcome, ObservationOutcome::Denied);
        assert_eq!(
            observations[0].subject_id,
            Some(bound.endpoint_id.to_string())
        );
        assert_eq!(
            observations[5].subject_id,
            Some(closed.endpoint_id.to_string())
        );
    }

    #[tokio::test]
    async fn unknown_peer_is_denied_before_call() {
        let report = run_unknown_peer_denial_roundtrip().await.unwrap();

        assert_eq!(report.hello_response.hello_outcome, HelloOutcome::Denied);
        assert_eq!(report.hello_response.safe_message, "not authorized");
        assert_eq!(report.hello_evaluation.reason, HelloReason::MissingBinding);
        assert_eq!(report.hello_evaluation.selected_binding_id, None);
        assert_eq!(
            report.hello_evaluation.observation_id,
            ObservationId::from("obs-iroh-hello-decision")
        );

        assert_eq!(
            report.call_reply.reply_outcome,
            CallProtocolReplyOutcome::Denied
        );
        assert_eq!(report.call_reply.result_ref, None);
        assert_eq!(report.call_evaluation.outcome, CallProtocolOutcome::Denied);
        assert_eq!(
            report.call_evaluation.reason,
            CallProtocolReason::HelloNotAdmitted
        );
        assert_eq!(report.call_evaluation.route_decision_id, None);
        assert_eq!(
            report.call_evaluation.observation_id,
            ObservationId::from("obs-iroh-call-decision")
        );
    }
}
