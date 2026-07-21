//! MCT daemon composition layer.
//!
//! The daemon composes the kernel, observation ledger, and adapters. Authority
//! remains in `mct-kernel`; external effects remain in adapter crates.

#![forbid(unsafe_code)]

mod acquisition;
#[cfg(test)]
mod authority_test_fixture;
mod blob_store;
mod children;
mod composition;
mod config;
mod control;
mod cycle;
#[cfg(test)]
mod fake;
mod federation;
mod inspector;
mod lifecycle;
mod metrics;
mod process;
mod registry;
mod state;
mod status;
mod supervisor;
mod toy;
mod wasm;
mod wit_values;

pub use acquisition::{
    MCT_CHILD_MANIFEST_MAX_BYTES, MCT_COMPONENT_ARTIFACT_MAX_BYTES,
    MCT_FILESYSTEM_ACQUISITION_ADAPTER, MctArtifactAcquisitionReport, MctArtifactAttemptContext,
    MctArtifactStageRequest, new_artifact_attempt_context, stage_artifact_with_context,
    stage_artifact_with_context_and_observer, stage_operator_pointed_artifact,
};
pub use blob_store::{
    MCT_BLOB_MAX_BYTES, MctLocalBlobStore, MctLocalBlobStoreError, content_addressed_blob_handle,
    ingest_blob_from_path, local_blob_store_for_state_path,
};
pub use children::{
    MctChildFileDigest, MctChildIngressMode, MctChildInstanceState, MctChildIntegrityMode,
    MctChildLoadFailure, MctChildLoadOptions, MctChildLoadReport, MctChildRegistry, MctLoadedChild,
    component_artifact_from_loaded_child, load_children_from_dir, operation_id_from_target,
};
pub use composition::{
    MctCompositionPlan, MctCompositionStep, MctPandoActivationCommand,
    MctPandoActivationEvaluation, MctPandoActivationPlan, MctPandoChild, MctPandoCommand,
    MctPandoCommandArg, MctPandoComposition, MctPandoDiagnostic, MctPandoDiagnosticKind,
    MctPandoLifecycleStatus, MctPandoManifest, MctPandoRegistry, MctPandoRegistryEntry,
    MctPandoSection, MctPandoWiring, MctPandoWiringEndpoint, build_pando_activation_plan,
    build_pando_registry, parse_pando_manifest_path, parse_pando_manifest_str,
    record_composition_plan,
};
pub use config::{
    MctConfigChildAuthorityProjection, MctDaemonConfig, MctDaemonConfigStore, MctLocalNodeIdentity,
    MctOperatorChildScope, MctOperatorNodeScope, MctOutboundPeerBindingPresentation,
    MctPeerAddressBookEntry, MctPeerAuthorityProjection, MctStoredChildApproval,
    MctStoredChildAssignment, current_timestamp, current_timestamp_string, default_config_path,
    outbound_peer_binding_for_local,
};
pub use control::{
    MctControlPlaneAuthPolicy, MctControlPlaneResponse, MctControlPlaneSnapshot,
    MctControlPlaneSnapshotError, MctControlPlaneSnapshotResult, MctDaemonLocalControlFacts,
    MctDaemonLocalControlRequest, MctDaemonLocalControlResponse, handle_control_plane_path,
    handle_control_plane_path_result_with_auth, handle_control_plane_path_with_auth,
    handle_local_control_request, serve_http_control_once, serve_http_control_once_with_auth,
    serve_http_control_once_with_snapshot_result,
};
#[cfg(unix)]
pub use control::{
    MctUdsControlCallHandler, MctUdsControlCallPreflight, MctUdsControlMutationHandler,
    MctUdsPeerCredentials, serve_uds_control_once, serve_uds_control_once_with_auth,
    serve_uds_control_once_with_handlers, serve_uds_control_once_with_snapshot_result,
    serve_uds_control_once_with_snapshot_result_and_blob_store,
    serve_uds_control_once_with_snapshot_result_blob_store_and_mutations,
    serve_uds_control_stream_with_handlers,
};
pub use cycle::{
    MctChildTaskCycleReport, MctDrainedEvent, MctTaskCycleChild, run_child_task_cycle,
};
pub use federation::{
    MctFederationCallableSurfaceView, MctFederationCapabilityView, MctFederationPeerView,
    build_federation_capability_view, build_federation_capability_view_with_children,
    hello_capability_view_from_federation_view,
};
pub use inspector::{
    MctInspectorObservationQuery, MctInspectorObservationView, inspect_observation_entries,
};
pub use lifecycle::{
    MctChildReloadError, MctChildReloadReport, MctChildWarmupReport, reload_configured_child,
    warmup_configured_child,
};
pub use metrics::{MctMetricsSnapshot, build_metrics_snapshot};
pub use process::{
    MctProcessChildError, MctProcessChildHarness, MctProcessChildInvocationIds,
    MctProcessChildInvocationReport,
};
pub use registry::{
    MctChildPackageInstallReport, MctRegistrySyncReport, install_verified_child_package,
    sync_child_registry_source,
};
pub use state::{
    ChildInvocationProvenance, MCT_IDEMPOTENCY_MAX_ENTRIES_PER_CALLER, MCT_IDEMPOTENCY_TTL_SECONDS,
    MctArtifactPackageRecord, MctCompositionRunRecord, MctIdempotencyReservation, MctMetricPoint,
    MctQueuedTaskRecord, MctRecordedCallReply, MctRegistrySourceRecord,
    MctRemoteCallableSurfaceRecord, MctRemoteSurfaceRefresh, MctRuntimeRunRecord,
    MctRuntimeRunState, MctRuntimeStateStore, MctRuntimeStateSummary, MctTaskIntentRecord,
    MctTaskStatus, MctTriggerFiringRecord, MctTriggerOccurrenceDisposition,
    MctTriggerOccurrenceRecord, MctTriggerPendingOccurrenceRecord, default_state_path,
};
pub use status::{
    MctDaemonHealth, MctDaemonReadiness, MctDaemonStatus, MctResidentStatus, daemon_status,
    daemon_status_with_resident,
};
pub use supervisor::{
    MctProcessSpawnConfig, MctProcessSupervisor, MctProcessSupervisorError,
    MctProcessSupervisorEvent, MctProcessSupervisorRecoveryReport, MctSupervisedProcessState,
    MctSupervisedProcessStatus,
};
pub use toy::{
    MCT_SECRETS_TOY_ID, MctToyAdapterOutcome, MctToyAdapterRegistry, MctToyBackend, MctToyCallIds,
    MctToyCallReport, mct_secrets_toy_contract,
};
pub use wasm::{
    DEFAULT_WASM_MEMORY_LIMIT_BYTES, MctWasiHostConfig, MctWasiPreopen, MctWasiPreopenAccess,
    MctWasmComponentDiagnosticIds, MctWasmComponentInvocationIds, MctWasmComponentInvocationReport,
    MctWasmComponentRuntime, MctWasmComponentRuntimeError, MctWasmComponentToyInvocation,
    MctWasmHostConfig, MctWasmToyHostImport, MctWitComponentInvocationReport,
    MctWitHostImportAdapters, MctWitResolvedOperation, MctWitToyHostAdapter,
    resolve_wit_operation_target, wasm_component_runtime_error_observation,
    wit_operation_id_from_target,
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
            endpoint_id: EndpointIdText::new("endpoint-daemon")
                .expect("string ID literal/generated value must be non-empty"),
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

        let ledger =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-dev", "mother-a").unwrap();
        let call_entries = ledger
            .by_call(
                &CallId::new("call-fake-echo")
                    .expect("string ID literal/generated value must be non-empty"),
            )
            .unwrap();
        assert_eq!(call_entries.len(), 4);
    }
}
