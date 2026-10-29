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
