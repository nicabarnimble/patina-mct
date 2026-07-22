use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use mct_daemon::{
    ChildInvocationProvenance, DEFAULT_WASM_MEMORY_LIMIT_BYTES, MCT_BLOB_MAX_BYTES,
    MCT_IDEMPOTENCY_MAX_ENTRIES_PER_CALLER, MCT_IDEMPOTENCY_TTL_SECONDS, MCT_SECRETS_TOY_ID,
    MctArtifactAcquisitionReport, MctArtifactAttemptContext, MctArtifactStageRequest,
    MctChildIntegrityMode, MctChildLoadOptions, MctCompositionPlan, MctCompositionRunRecord,
    MctCompositionStep, MctConfigChildAuthorityProjection, MctControlPlaneResponse,
    MctControlPlaneSnapshot, MctControlPlaneSnapshotError, MctControlPlaneSnapshotResult,
    MctDaemonConfigStore, MctDaemonHealth, MctDaemonReadiness, MctDaemonReleaseAcquisitionReport,
    MctDaemonReleaseAcquisitionRequest, MctDaemonStatus, MctIdempotencyReservation, MctLoadedChild,
    MctLocalBlobStoreError, MctLocalNodeIdentity, MctOperatorChildScope, MctOperatorNodeScope,
    MctOutboundPeerBindingPresentation, MctPeerAddressBookEntry, MctProcessChildHarness,
    MctProcessChildInvocationIds, MctRecordedCallReply, MctRemoteCallableSurfaceRecord,
    MctRemoteSurfaceRefresh, MctResidentStatus, MctRuntimeStateStore, MctStandingSourceLedgerProof,
    MctToyAdapterRegistry, MctToyBackend, MctTriggerFiringRecord, MctTriggerOccurrenceDisposition,
    MctTriggerOccurrenceRecord, MctTriggerPendingOccurrenceRecord, MctUdsControlCallHandler,
    MctUdsControlCallPreflight, MctUdsPeerCredentials, MctVerifiedDaemonRelease, MctWasiHostConfig,
    MctWasiPreopen, MctWasiPreopenAccess, MctWasmComponentInvocationIds, MctWasmComponentRuntime,
    MctWasmHostConfig, MctWitHostImportAdapters, MctWitKeyvalueHostAdapter,
    MctWitMessagingHostAdapter, MctWitProducedMessage, MctWitToyHostAdapter,
    MctWitWatchCallOutWireEvent, MctWitWatchMessageAdmission,
    acquire_operator_file_daemon_release_offline,
    acquire_operator_file_daemon_release_with_observer,
    build_federation_capability_view_with_children, build_metrics_snapshot,
    component_artifact_from_loaded_child, current_timestamp, current_timestamp_string,
    daemon_status, daemon_status_with_resident, default_config_path, default_state_path,
    hello_capability_view_from_federation_view, install_verified_child_package,
    load_children_from_dir, local_blob_store_for_state_path, mct_secrets_toy_contract,
    new_artifact_attempt_context, outbound_peer_binding_for_local, plan_daemon_release_source,
    record_composition_plan, reload_configured_child, serve_http_control_once_with_snapshot_result,
    stage_artifact_with_context_and_observer, sync_child_registry_source,
    verify_standing_source_ledger_correlation, warmup_configured_child,
};
use mct_iroh::{
    MCT_RESULT_INLINE_PAYLOAD_MAX_BYTES, MctIrohCallHandlerResult, MctIrohCallPayloadReply,
    MctIrohConcurrentServeConfig, MctIrohObservationBatch, MctIrohObservationDurability,
    MctIrohObservationSink, MctIrohServeEvent, MctIrohServeState, MctIrohServedProtocol,
    MctPeerBindingSignatureVerification, MotherIrohEndpoint, MotherIrohEndpointConfig,
    MotherIrohEndpointError, MotherIrohEndpointSnapshot, MotherIrohEndpointTicket,
    MotherIrohRelayMode, endpoint_id_for_secret_key_hex, generate_node_secret_key_hex,
    load_or_create_node_secret_key_hex, verify_peer_binding_signature_ref,
    write_new_node_secret_key_file,
};
use mct_kernel::*;
use mct_observation::{DurabilityClass, ExportStatus, JsonlObservationLedger};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{net::TcpListener, sync::broadcast};

#[cfg(unix)]
use tokio::net::UnixListener;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        print_help();
        return Ok(());
    }

    match args.remove(0).as_str() {
        "version" => println!("mct-daemon {}", mct_daemon::version()),
        "status" => run_status(args)?,
        "upgrade" => run_upgrade(args)?,
        "install" => run_install(args)?,
        "uninstall" => run_uninstall(args)?,
        "start" => run_start(args)?,
        "stop" => run_stop(args)?,
        "restart" => run_restart(args)?,
        "serve" => run_serve(args).await?,
        "artifacts" => run_artifacts(args)?,
        "children" => run_children(args)?,
        "control" => run_control(args).await?,
        "process" => run_process(args)?,
        "peers" => run_peers(args)?,
        "state" => run_state(args)?,
        "runs" => run_runs(args)?,
        "slate" => run_slate(args)?,
        "toys" => run_toys(args)?,
        "triggers" => run_triggers(args)?,
        "watch" => run_watch(args)?,
        "wasm" => run_wasm(args)?,
        "federation" => run_federation(args)?,
        "iroh" => run_iroh(args).await?,
        "jvm" => run_jvm(args).await?,
        "metrics" => run_metrics(args)?,
        "pando" => run_pando(args)?,
        "registry" => run_registry(args)?,
        "help" | "--help" | "-h" => print_help(),
        other => bail!("unknown command '{other}'"),
    }

    Ok(())
}

#[path = "daemon/cli_artifacts.rs"]
mod cli_artifacts;
use cli_artifacts::*;

#[path = "daemon/cli_runtime.rs"]
mod cli_runtime;
use cli_runtime::*;

#[path = "daemon/resident/mod.rs"]
mod resident;
use resident::*;

#[path = "daemon/control.rs"]
mod control;
use control::*;

#[path = "daemon/triggers.rs"]
mod triggers;
use triggers::*;
#[path = "daemon/watch.rs"]
mod watch;
use watch::*;

#[path = "daemon/cli_admin.rs"]
mod cli_admin;
use cli_admin::*;

#[path = "daemon/supervisor_lifecycle.rs"]
mod supervisor_lifecycle;
use supervisor_lifecycle::*;

#[path = "daemon/upgrade.rs"]
mod upgrade;
use upgrade::*;

#[path = "daemon/ingress.rs"]
mod ingress;
use ingress::*;

fn take_flag(args: &mut Vec<String>, flag: &str) -> bool {
    if let Some(index) = args.iter().position(|arg| arg == flag) {
        args.remove(index);
        true
    } else {
        false
    }
}

fn take_option(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.remove(index);
    if index < args.len() {
        Some(args.remove(index))
    } else {
        None
    }
}

fn default_children_dir() -> PathBuf {
    PathBuf::from(".mct").join("children")
}

fn default_identity_path() -> PathBuf {
    PathBuf::from(".mct")
        .join("identity")
        .join("iroh-secret.hex")
}

fn help_text() -> String {
    format!(
        "mct-daemon {version}\n\nCommands:\n  status [--uds socket-path] [--json]\n  upgrade <artifact-ref> [--root absolute-path] [--expected-digest sha256:hex] [--approve-artifact sha256:hex] [--json]\n  install [--root absolute-path] [--executable absolute-path] [--replace] [--json]\n  uninstall [--root absolute-path] [--json]\n  start [--root absolute-path] [--json]\n  stop [--root absolute-path] [--json]\n  restart [--root absolute-path] [--json]\n  serve [--identity path] [--config path] [--children-dir path] [--state path] [--ledger path] [--max-connections n] [--relay-default] [--http addr | --uds socket-path]\n  artifacts stage <source-root> --manifest relative-path --component relative-path --child name --version version [--expected-digest blake3:hex] [--children-dir path] [--state path] [--ledger path] [--uds socket-path] [--json]\n  artifacts acquire <package-dir> (--operator-pointed | --source-authority id) --child name --version version [--expected-digest blake3:hex] [--children-dir path] [--state path] [--ledger path] [--uds socket-path] [--json]\n  artifacts show <sha256:hex> [--state path] [--json]\n  artifacts acquisitions [--artifact sha256:hex] [--state path] [--json]\n  artifacts sources create|revoke|list ...\n  control serve-http [addr] [--state path]\n  control serve-uds [socket-path] [--state path]\n  federation view [--config path] [--state path] [--children-dir path] [--json]\n  metrics snapshot [--state path] [--json]\n  pando record <composition-id> [step-id,call-id,runtime,child,decision ...] [--state path] [--ledger path] [--uds socket-path] [--json]\n  children load [children-dir] [--strict-integrity] [--json]\n  process call <executable> [payload-json] [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  children approve <child-name> [children-dir] --artifact <sha256:digest> [--config path] [--state path] [--ledger path] [--uds socket-path] [--strict-integrity]\n  children revoke <child-name> [--config path] [--ledger path] [--uds socket-path]\n  children approvals [--config path] [--json]\n  children warmup <child-name> [--children-dir path] [--config path] [--ledger path] [--state path] [--json]\n  children reload <child-name> [--children-dir path] [--config path] [--ledger path] [--state path] [--json]\n  peers add <peer-node-id> <binding-id> <endpoint-id> <vision-id> [ticket-file] [--signature-ref proof] --expires-at ts [--config path] [--ledger path] [--uds socket-path]\n  peers list [--config path] [--json]\n  peers set-outbound-proof <peer-node-id> <binding-id> --signature-ref proof --expires-at ts [--config path] [--ledger path] [--uds socket-path]\n  peers revoke <peer-node-id> [--config path] [--ledger path] [--uds socket-path]\n  peers remove <peer-node-id> [--config path] [--ledger path] [--uds socket-path]\n  state summary [--state path] [--json]\n  runs list [--state path] [--json] [--limit n]\n  slate list-work --project-root path [--status status] [--kind kind] [--children-dir path] [--config path] [--state path] [--ledger path]\n  toys authorize-slate <child-name> <project-root> [--children-dir path] [--config path] [--state path] [--ledger path] [--uds socket-path] [--json]\n  toys authorize-secret <child-name> <secret-name> [--children-dir path] [--config path] [--state path] [--ledger path] [--uds socket-path] [--json]\n  toys grant-watch <child-name> <canonical-root> --scope-id id --traversal recursive --events created,modified,deleted --max-events-per-batch n --coalescing none|last-per-path --starts-at ts --expires-at ts [--scope-mode constrained|explicit-broad] [--children-dir path] [--config path] [--state path] [--uds socket-path] [--json]\n  toys revoke-watch <scope-id> --expected-revision n [--config path] [--state path] [--uds socket-path] [--json]\n  toys grant-directory-read <child-name> <canonical-root> --expires-at ts [--children-dir path] [--config path] [--state path] [--uds socket-path] [--json]\n  toys grant-keyvalue <child-name> <bucket-name> --expires-at ts [--children-dir path] [--config path] [--state path] [--uds socket-path] [--json]\n  toys grant-observability <child-name> [--logging] [--measure] --expires-at ts [--children-dir path] [--config path] [--state path] [--uds socket-path] [--json]\n  watch scopes show <scope-id> [--state path] [--json]\n  watch scopes list [--state path] [--json]\n  triggers create <trigger-id> --target operation --payload-json json --anchor-at ts --interval-ms n --starts-at ts --expires-at ts [--missed-fire-policy skip|coalesce-one|fire-late-bounded] [--overlap-policy refuse|coalesce-one|queue-bounded] [--config path] [--state path] [--ledger path] [--uds socket-path] [--json]\n  triggers revise <trigger-id> --expected-revision n <complete create scope> ...\n  triggers revoke <trigger-id> --expected-revision n [--config path] [--state path] [--ledger path] [--uds socket-path] [--json]\n  triggers show <trigger-id> [--state path] [--json]\n  triggers list [--state path] [--json]\n  wasm call <component-file> <export-name> [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  wasm call-wit <child-name> <operation-id> <args-json> [--project-root path] [--guest-project /project] [--git-repo path] [--children-dir path] [--config path] [--ledger path] [--state path]\n  iroh identity [identity-file] [--config path] [--ledger path] [--uds socket-path]\n  iroh serve [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> [children-dir] --expires-at ts [--ledger path] [--state path]\n  iroh serve-process [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> <executable> --child <child-name> --expires-at ts [--children-dir path] [--config path] [--ledger path] [--state path]\n  iroh call [--relay-default] <identity-file> <peer-ticket-file> <binding-id> <local-node-id> <vision-id> [namespace interface function] [--signature-ref proof] [--ledger path]\n  iroh call-peer [--relay-default] <identity-file> <peer-node-id> [namespace interface function] [--config path] [--children-dir path] [--state path] [--ledger path]\n  jvm call-json <operation-id> <args-json> [--children-dir path] [--config path] [--state path] [--ledger path]",
        version = mct_daemon::version()
    )
}

fn print_help() {
    println!("{}", help_text());
}

#[cfg(test)]
#[path = "authority_test_fixture.rs"]
mod authority_test_fixture;
