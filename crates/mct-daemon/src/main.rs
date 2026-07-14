use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use mct_daemon::{
    ChildInvocationProvenance, DEFAULT_WASM_MEMORY_LIMIT_BYTES, MCT_BLOB_MAX_BYTES,
    MCT_IDEMPOTENCY_MAX_ENTRIES_PER_CALLER, MCT_IDEMPOTENCY_TTL_SECONDS, MCT_SECRETS_TOY_ID,
    MctChildIntegrityMode, MctChildLoadOptions, MctChildPackageInstallReport, MctCompositionPlan,
    MctCompositionRunRecord, MctCompositionStep, MctConfigChildAuthorityProjection,
    MctControlPlaneResponse, MctControlPlaneSnapshot, MctControlPlaneSnapshotError,
    MctControlPlaneSnapshotResult, MctDaemonConfigStore, MctDaemonHealth, MctDaemonReadiness,
    MctDaemonStatus, MctIdempotencyReservation, MctLocalBlobStoreError, MctLocalNodeIdentity,
    MctOperatorChildScope, MctOperatorNodeScope, MctOutboundPeerBindingPresentation,
    MctPeerAddressBookEntry, MctProcessChildHarness, MctProcessChildInvocationIds,
    MctRecordedCallReply, MctRegistrySyncReport, MctRemoteCallableSurfaceRecord,
    MctRemoteSurfaceRefresh, MctResidentStatus, MctRuntimeStateStore, MctToyAdapterRegistry,
    MctToyBackend, MctUdsControlCallHandler, MctUdsControlCallPreflight, MctUdsPeerCredentials,
    MctWasiHostConfig, MctWasiPreopen, MctWasiPreopenAccess, MctWasmComponentInvocationIds,
    MctWasmComponentRuntime, MctWasmHostConfig, MctWitHostImportAdapters, MctWitToyHostAdapter,
    build_federation_capability_view_with_children, build_metrics_snapshot, current_timestamp,
    daemon_status, daemon_status_with_resident, default_config_path, default_state_path,
    hello_capability_view_from_federation_view, install_verified_child_package,
    load_children_from_dir, local_blob_store_for_state_path, mct_secrets_toy_contract,
    outbound_peer_binding_for_local, record_composition_plan, reload_configured_child,
    serve_http_control_once_with_snapshot_result, sync_child_registry_source,
    warmup_configured_child,
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
        "serve" => run_serve(args).await?,
        "children" => run_children(args)?,
        "control" => run_control(args).await?,
        "process" => run_process(args)?,
        "peers" => run_peers(args)?,
        "state" => run_state(args)?,
        "runs" => run_runs(args)?,
        "slate" => run_slate(args)?,
        "toys" => run_toys(args)?,
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

#[path = "daemon/cli_runtime.rs"]
mod cli_runtime;
use cli_runtime::*;

#[path = "daemon/resident/mod.rs"]
mod resident;
use resident::*;

#[path = "daemon/control.rs"]
mod control;
use control::*;

#[path = "daemon/cli_admin.rs"]
mod cli_admin;
use cli_admin::*;

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

fn print_help() {
    println!(
        "mct-daemon {version}\n\nCommands:\n  status [--uds socket-path] [--json]\n  serve [--identity path] [--config path] [--children-dir path] [--state path] [--ledger path] [--max-connections n] [--relay-default] [--http addr | --uds socket-path]\n  control serve-http [addr] [--state path]\n  control serve-uds [socket-path] [--state path]\n  registry install <verified-package-dir> [--children-dir path] [--state path] [--ledger path] [--uds socket-path] [--replace] [--json]\n  registry sync <source-id> [children-dir] [--state path] [--ledger path] [--uds socket-path] [--strict-integrity] [--json]\n  federation view [--config path] [--state path] [--children-dir path] [--json]\n  metrics snapshot [--state path] [--json]\n  pando record <composition-id> [step-id,call-id,runtime,child,decision ...] [--state path] [--ledger path] [--uds socket-path] [--json]\n  children load [children-dir] [--strict-integrity] [--json]\n  process call <executable> [payload-json] [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  children approve <child-name> [children-dir] [--config path] [--ledger path] [--uds socket-path] [--strict-integrity]\n  children revoke <child-name> [--config path] [--ledger path] [--uds socket-path]\n  children approvals [--config path] [--json]\n  children warmup <child-name> [--children-dir path] [--config path] [--ledger path] [--state path] [--json]\n  children reload <child-name> [--children-dir path] [--config path] [--ledger path] [--state path] [--json]\n  peers add <peer-node-id> <binding-id> <endpoint-id> <vision-id> [ticket-file] [--signature-ref proof] --expires-at ts [--config path] [--ledger path] [--uds socket-path]\n  peers list [--config path] [--json]\n  peers set-outbound-proof <peer-node-id> <binding-id> --signature-ref proof --expires-at ts [--config path] [--ledger path] [--uds socket-path]\n  peers revoke <peer-node-id> [--config path] [--ledger path] [--uds socket-path]\n  peers remove <peer-node-id> [--config path] [--ledger path] [--uds socket-path]\n  state summary [--state path] [--json]\n  runs list [--state path] [--json] [--limit n]\n  slate list-work --project-root path [--status status] [--kind kind] [--children-dir path] [--config path] [--state path] [--ledger path]\n  toys authorize-slate <child-name> <project-root> [--children-dir path] [--config path] [--state path] [--ledger path] [--uds socket-path] [--json]\n  toys authorize-secret <child-name> <secret-name> [--children-dir path] [--config path] [--state path] [--ledger path] [--uds socket-path] [--json]\n  wasm call <component-file> <export-name> [namespace interface function] --child <child-name> [--children-dir path] [--config path] [--ledger path] [--state path]\n  wasm call-wit <child-name> <operation-id> <args-json> [--project-root path] [--guest-project /project] [--git-repo path] [--children-dir path] [--config path] [--ledger path] [--state path]\n  iroh identity [identity-file] [--config path] [--ledger path] [--uds socket-path]\n  iroh serve [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> [children-dir] --expires-at ts [--ledger path] [--state path]\n  iroh serve-process [--relay-default] <identity-file> <binding-id> <peer-endpoint-id> <peer-node-id> <vision-id> <executable> --child <child-name> --expires-at ts [--children-dir path] [--config path] [--ledger path] [--state path]\n  iroh call [--relay-default] <identity-file> <peer-ticket-file> <binding-id> <local-node-id> <vision-id> [namespace interface function] [--signature-ref proof] [--ledger path]\n  iroh call-peer [--relay-default] <identity-file> <peer-node-id> [namespace interface function] [--config path] [--children-dir path] [--state path] [--ledger path]\n  jvm call-json <operation-id> <args-json> [--children-dir path] [--config path] [--state path] [--ledger path]",
        version = mct_daemon::version()
    );
}

#[cfg(test)]
#[path = "authority_test_fixture.rs"]
mod authority_test_fixture;
