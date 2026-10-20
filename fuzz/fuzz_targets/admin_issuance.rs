#![no_main]

use mct_kernel::{AdminKeyGrantV1, Timestamp, evaluate_admin_key_issuance};
use mct_observation::{
    AdminKeyIssueRequestV1, AuthorityChangeV1, AuthorityStateV1, MctObservationLedgerEntry,
};

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    // Harness bound only; production ingress byte limits are a separate gate.
    if data.len() > 262_144 {
        return;
    }
    let _ = serde_json::from_slice::<AdminKeyIssueRequestV1>(data);
    if let Ok((issuer, recipient, time)) =
        serde_json::from_slice::<(AdminKeyGrantV1, AdminKeyGrantV1, Timestamp)>(data)
    {
        let _ = evaluate_admin_key_issuance(&issuer, &recipient, &time);
    }
    if let Ok((mut state, change)) =
        serde_json::from_slice::<(AuthorityStateV1, AuthorityChangeV1)>(data)
    {
        if matches!(change, AuthorityChangeV1::AdminKeyIssue { .. })
            && mct_observation::apply_authority_changes(&mut state, &[change]).is_ok()
        {
            assert!(mct_observation::authority_state_hash(&state).is_ok());
        }
    }
    if let Ok(entries) = serde_json::from_slice::<Vec<MctObservationLedgerEntry>>(data) {
        let _ = mct_observation::replay_authority_entries(&entries);
    }
});
