//! First resident cutover unit: one existing ledger actor, signed key-management
//! and status adapters, no UID or legacy mutation adapter enabled on this ALPN.
use super::*;
use mct_daemon::{MotherAuthorityCommitOutcomeV1, MotherAuthorityOrderV1};
use mct_iroh::{MCT_ADMIN_ALPN, MctIrohAdminHandler, MctIrohAdminReply};
use mct_kernel::admin_sig::PasskeyAdminRequestV1;

pub(super) fn process_admin(
    ledger: &mut JsonlObservationLedger,
    order: &MotherAuthorityOrderV1,
    request: PasskeyAdminRequestV1,
) -> MctIrohAdminReply {
    let mutation_id = format!(
        "admin:{}:{}",
        request.proof.signer_ref(),
        request.intent.nonce
    );
    let mut reply = MctIrohAdminReply::Unavailable;
    order.commit_mutation(&mutation_id, &request, |request| {
        let prior = ledger.verified_head().ok().map(|h| h.entry_hash);
        let result = mct_daemon::execute_passkey_administration(ledger, request.clone());
        if ledger.is_poisoned() {
            reply = MctIrohAdminReply::Indeterminate;
            return MotherAuthorityCommitOutcomeV1::CommitUnknown {
                mutation_id: mutation_id.clone(),
            };
        }
        let changed = ledger.verified_head().ok().map(|h| h.entry_hash) != prior;
        reply = match result {
            Ok(result) => MctIrohAdminReply::Completed { result },
            Err(_) if changed => MctIrohAdminReply::Indeterminate,
            Err(_) => MctIrohAdminReply::Rejected,
        };
        if !changed {
            return MotherAuthorityCommitOutcomeV1::RejectedBeforeCommit {
                mutation_id: mutation_id.clone(),
            };
        }
        let expectation = ledger.verified_head().ok().and_then(|head| {
            ledger.verified_replay().ok().and_then(|replay| {
                mct_daemon::authority_projection_expectation_from_verified(&head, replay).ok()
            })
        });
        match expectation {
            Some(current_expectation) => MotherAuthorityCommitOutcomeV1::Committed {
                mutation_id: mutation_id.clone(),
                current_expectation,
            },
            None => {
                reply = MctIrohAdminReply::Indeterminate;
                MotherAuthorityCommitOutcomeV1::CommitUnknown {
                    mutation_id: mutation_id.clone(),
                }
            }
        }
    });
    reply
}

pub(crate) async fn run_passkey_admin_server(
    root: PathBuf,
    mother: String,
    ledger_id: String,
    ticket_output: PathBuf,
) -> Result<()> {
    let path = root.join("observations.jsonl");
    let ledger = ResidentLedgerWriter::open_passkey_admin(path, &ledger_id, &mother)?;
    let outcome = async {
        use std::io::Read as _;
        let identity = root.join("identity/admin-iroh-secret.hex");
        let secret = match mct_observation::private_fs::read_file(&identity) {
            Ok(file) => {
                let mut bytes = Vec::new(); file.take(129).read_to_end(&mut bytes)?;
                if bytes.len() > 128 { bail!("oversized admin endpoint identity"); }
                String::from_utf8(bytes)?.trim().to_owned()
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let secret = generate_node_secret_key_hex();
                mct_observation::private_fs::write_atomic(&identity, secret.as_bytes())?;
                secret
            }
            Err(error) => return Err(error.into()),
        };
        let mut endpoint_config = MotherIrohEndpointConfig::local_mct().with_secret_key_hex(secret);
        // No peer/call/legacy-control serving on this bounded initial mode.
        endpoint_config.accepted_alpns = vec![MCT_ADMIN_ALPN.into()];
        let mut endpoint = MotherIrohEndpoint::bind(endpoint_config).await?;
        let owner = ledger.clone();
        let mut config = MctIrohConcurrentServeConfig::new(resident_iroh_observation_sink(ledger.clone()));
        config.admin_handler = Some(MctIrohAdminHandler::new(move |request| {
            let owner = owner.clone(); async move { owner.administer(request).await }
        }));
        mct_observation::private_fs::write_atomic(&ticket_output, &serde_json::to_vec_pretty(&endpoint.ticket())?)?;
        println!("passkey admin listening; ticket={}", ticket_output.display());
        let result = {
            let serving = endpoint.serve_concurrent_with_binding_provider(
                MctIrohServeState::new(), config, current_timestamp,
                || async { Err(mct_iroh::MotherIrohEndpointError::EndpointClosed) },
                |_, _, _| async { MctIrohCallHandlerResult::denied() },
            );
            tokio::pin!(serving);
            tokio::select! { result = &mut serving => result.map_err(anyhow::Error::from), result = tokio::signal::ctrl_c() => result.map_err(anyhow::Error::from) }
        };
        endpoint.close().await;
        result
    }.await;
    ledger.close().await;
    outcome
}
