//! Shared signed adapters for CLI and resident Iroh administration.
//! No UID or transport identity is accepted as administrative authority.
use anyhow::{Result, bail};
use mct_kernel::{
    AdminKeyScopeV1, admin_key::PasskeyAdminActionV1, admin_sig::PasskeyAdminRequestV1,
};
use mct_observation::JsonlObservationLedger;
use serde_json::{Value, json};

pub fn execute_passkey_administration(
    ledger: &mut JsonlObservationLedger,
    request: PasskeyAdminRequestV1,
) -> Result<Value> {
    match &request.intent.action {
        PasskeyAdminActionV1::Bootstrap { .. } => {
            bail!("bootstrap requires separate trusted setup")
        }
        PasskeyAdminActionV1::IssueDayKey { .. } | PasskeyAdminActionV1::RevokeDayKey { .. } => {
            let entry = ledger.execute_passkey_admin(request)?;
            Ok(
                json!({"sequence":entry.local_sequence,"observation_id":entry.observation.observation_id,
                "authority":ledger.verified_replay()?.current_authority}),
            )
        }
        PasskeyAdminActionV1::Command { route, body, .. } => {
            // Route policy is adapter-owned. Never use request.scope as the
            // required scope. Unknown/unmigrated routes cannot acquire authority.
            if route == "/artifacts/stage" {
                return governed_artifact_stage(ledger, request.clone(), body);
            }
            if let Some(scope) = mct_observation::governed_route_scope(route) {
                // The committed fact is the change; replay applies its body.
                let route = route.clone();
                ledger.execute_passkey_command(request, &route, scope, |_| Ok(()))?;
                let replay = ledger.verified_replay()?;
                let runtime = &replay
                    .state
                    .passkey_admin
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("missing passkey provenance"))?
                    .runtime;
                return Ok(json!({"authority":replay.current_authority,"route":route,
                    "children":runtime.children.len(),"toy_grants":runtime.toy_grants.len()}));
            }
            if route != "/status" || !body.is_empty() {
                bail!("unmigrated or malformed admin command");
            }
            ledger
                .execute_passkey_command(request, "/status", AdminKeyScopeV1::Status, |_| Ok(()))?;
            let replay = ledger.verified_replay()?;
            let state = &replay
                .state
                .passkey_admin
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("missing passkey provenance"))?
                .state;
            Ok(
                json!({"authority":replay.current_authority,"mother_node_id":state.mother_node_id,
                "ledger_id":state.ledger_id,"day_key_count":state.day_keys.len()}),
            )
        }
    }
}

/// Governed staging: a pinned, operator-pointed stage request is the signed
/// command; its admission is committed before any file is written.
fn governed_artifact_stage(
    ledger: &mut JsonlObservationLedger,
    request: PasskeyAdminRequestV1,
    body: &str,
) -> Result<Value> {
    let stage: crate::acquisition::MctArtifactStageRequest = serde_json::from_str(body)
        .map_err(|error| anyhow::anyhow!("malformed stage body: {error}"))?;
    if stage.expected_digest.is_none()
        || stage.expected_manifest_digest.is_none()
        || stage.standing_source_authority_id.is_some()
    {
        bail!("governed staging must be operator-pointed and pinned to both digests");
    }
    ledger.execute_passkey_command(
        request,
        "/artifacts/stage",
        AdminKeyScopeV1::Artifact,
        |_| Ok(()),
    )?;
    let context = crate::acquisition::new_artifact_attempt_context()?;
    let report = crate::acquisition::stage_artifact_with_context_and_observer(
        &stage,
        &context,
        None,
        |report| {
            let now = crate::current_timestamp();
            ledger.append_before_effect(
                mct_kernel::MctObservation::informational(
                    context.acquisition_observation_id.clone(),
                    now.clone(),
                    mct_kernel::ObservationKind::ArtifactVerified,
                    mct_kernel::TraceId::new(format!("trace:{}", context.acquisition_id))?,
                    format!("governed artifact staged digest={:?}", report.artifact_id),
                ),
                now.to_string(),
            )?;
            Ok(())
        },
    )?;
    Ok(serde_json::to_value(&report)?)
}
