//! MCT daemon composition layer.
//!
//! The daemon composes the kernel, observation ledger, and adapters. Authority
//! remains in `mct-kernel`; external effects remain in adapter crates.

#![forbid(unsafe_code)]

mod children;
mod config;
mod control;
#[cfg(test)]
mod fake;
mod lifecycle;
mod process;
mod state;
mod status;
mod supervisor;
mod toy;
mod wasm;

pub use children::{
    MctChildFileDigest, MctChildIngressMode, MctChildInstanceState, MctChildIntegrityMode,
    MctChildLoadFailure, MctChildLoadOptions, MctChildLoadReport, MctChildRegistry, MctLoadedChild,
    component_artifact_from_loaded_child, load_children_from_dir, operation_id_from_target,
};
pub use config::{
    MctConfigChildAuthorityProjection, MctDaemonConfig, MctDaemonConfigStore,
    MctOperatorChildScope, MctPeerAddressBookEntry, MctStoredChildApproval,
    MctStoredChildAssignment, default_config_path, unix_timestamp_string,
};
pub use control::{
    MctDaemonLocalControlFacts, MctDaemonLocalControlRequest, MctDaemonLocalControlResponse,
    handle_local_control_request,
};
pub use lifecycle::{
    MctChildReloadReport, MctChildWarmupReport, reload_configured_child, warmup_configured_child,
};
pub use process::{
    MctProcessChildError, MctProcessChildHarness, MctProcessChildInvocationIds,
    MctProcessChildInvocationReport,
};
pub use state::{
    MctCompositionRunRecord, MctMetricPoint, MctRegistrySourceRecord, MctRuntimeRunRecord,
    MctRuntimeRunState, MctRuntimeStateStore, MctRuntimeStateSummary, default_state_path,
};
pub use status::{MctDaemonHealth, MctDaemonReadiness, MctDaemonStatus, daemon_status};
pub use supervisor::{
    MctProcessSpawnConfig, MctProcessSupervisor, MctProcessSupervisorError,
    MctProcessSupervisorEvent, MctProcessSupervisorRecoveryReport, MctSupervisedProcessState,
    MctSupervisedProcessStatus,
};
pub use toy::{
    MctToyAdapterOutcome, MctToyAdapterRegistry, MctToyBackend, MctToyCallIds, MctToyCallReport,
};
pub use wasm::{
    MctWasmComponentInvocationIds, MctWasmComponentInvocationReport, MctWasmComponentRuntime,
    MctWasmComponentRuntimeError,
};

/// Returns the crate version for health and smoke tests.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{run_fake_echo_slice, run_fake_end_to_end_status_slice};
    use mct_iroh::{MotherIrohEndpointLifecycle, MotherIrohEndpointSnapshot, MotherIrohRelayMode};
    use mct_kernel::*;
    use mct_observation::JsonlObservationLedger;

    fn iroh_snapshot(lifecycle: MotherIrohEndpointLifecycle) -> MotherIrohEndpointSnapshot {
        MotherIrohEndpointSnapshot {
            endpoint_id: EndpointIdText::from("endpoint-daemon"),
            lifecycle,
            accepted_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            direct_addresses: vec!["127.0.0.1:0".into()],
            relay_urls: Vec::new(),
            relay_mode: MotherIrohRelayMode::Disabled,
        }
    }

    #[test]
    fn exposes_version() {
        assert_eq!(super::version(), "0.1.0");
    }

    #[test]
    fn daemon_reports_health_and_readiness() {
        let ready = daemon_status(Some(iroh_snapshot(MotherIrohEndpointLifecycle::Bound)));
        assert_eq!(ready.version, "0.1.0");
        assert_eq!(ready.health, MctDaemonHealth::Healthy);
        assert_eq!(ready.readiness, MctDaemonReadiness::Ready);
        assert_eq!(ready.safe_message, "ready");

        let closed = daemon_status(Some(iroh_snapshot(MotherIrohEndpointLifecycle::Closed)));
        assert_eq!(closed.readiness, MctDaemonReadiness::NotReady);
        assert_eq!(closed.safe_message, "iroh endpoint not ready");

        let missing = daemon_status(None);
        assert_eq!(missing.readiness, MctDaemonReadiness::NotReady);
        assert!(missing.iroh_endpoint.is_none());
    }

    #[test]
    fn local_control_status_request_reports_daemon_status() {
        let response = handle_local_control_request(
            MctDaemonLocalControlRequest::Status,
            MctDaemonLocalControlFacts::new(Some(iroh_snapshot(
                MotherIrohEndpointLifecycle::Bound,
            ))),
        );

        let MctDaemonLocalControlResponse::Status(status) = response;
        assert_eq!(status.readiness, MctDaemonReadiness::Ready);
        assert_eq!(status.safe_message, "ready");
    }

    #[test]
    fn fake_end_to_end_status_reports_runtime_spine() {
        let dir = tempfile::tempdir().unwrap();
        let ledger_path = dir.path().join("observations.jsonl");
        let report = run_fake_end_to_end_status_slice(
            &ledger_path,
            iroh_snapshot(MotherIrohEndpointLifecycle::Bound),
        )
        .unwrap();

        assert_eq!(report.daemon.readiness, MctDaemonReadiness::Ready);
        assert_eq!(report.echo.result.outcome, ResultOutcome::Success);
        assert_eq!(report.echo.trace_observation_count, 6);
        assert_eq!(report.call_observation_count, 4);
    }

    #[test]
    fn fake_echo_slice_records_trace_and_result() {
        let dir = tempfile::tempdir().unwrap();
        let ledger_path = dir.path().join("observations.jsonl");
        let report = run_fake_echo_slice(&ledger_path).unwrap();
        assert!(report.hello.is_admitted());
        assert!(report.call.is_accepted_for_routing());
        assert_eq!(report.result.outcome, ResultOutcome::Success);
        assert_eq!(
            report.reply.reply_outcome,
            CallProtocolReplyOutcome::Success
        );
        assert_eq!(report.trace_observation_count, 6);

        let ledger = JsonlObservationLedger::open(&ledger_path, "ledger-dev", "mother-a").unwrap();
        let call_entries = ledger.by_call(&CallId::from("call-fake-echo")).unwrap();
        assert_eq!(call_entries.len(), 4);
    }
}
