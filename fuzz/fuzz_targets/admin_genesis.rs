#![no_main]

use mct_observation::{AdminGenesisRequestV1, AuthorityStateV1, MctObservationLedgerEntry};

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    // Harness bound, not a production ingress limit.
    if data.len() > 262_144 {
        return;
    }
    if let Ok(request) = serde_json::from_slice::<AdminGenesisRequestV1>(data) {
        let _ = request.signing_message();
    }
    if let Ok(state) = serde_json::from_slice::<AuthorityStateV1>(data) {
        let _ = mct_observation::authority_state_hash(&state);
    }
    if let Ok(entries) = serde_json::from_slice::<Vec<MctObservationLedgerEntry>>(data) {
        if let Ok(replay) = mct_observation::replay_authority_entries(&entries) {
            assert!(mct_observation::authority_state_hash(&replay.state).is_ok());
        }
    }
});
