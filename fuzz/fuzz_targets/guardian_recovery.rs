#![no_main]

use mct_kernel::recovery::{GuardianEnrollmentV1, GuardianRecoveryCommandV1};
use mct_observation::{AuthorityChangeV1, AuthorityStateV1, MctObservationLedgerEntry};

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    // Harness bound, NOT a claim about production ingress limits.
    if data.len() > 524_288 {
        return;
    }
    if let Ok((mut state, change)) =
        serde_json::from_slice::<(AuthorityStateV1, AuthorityChangeV1)>(data)
    {
        let _ = mct_observation::authority_state_hash(&state);
        let _ = mct_observation::apply_authority_changes(&mut state, &[change]);
        let _ = mct_observation::authority_state_hash(&state);
    }
    if let Ok(entries) = serde_json::from_slice::<Vec<MctObservationLedgerEntry>>(data) {
        // Semantic authority replay, not a replacement for physical frame validation.
        let _ = mct_observation::replay_authority_entries(&entries);
    }
    let _ = serde_json::from_slice::<GuardianRecoveryCommandV1>(data);
    if let Ok(enrollment) = serde_json::from_slice::<GuardianEnrollmentV1>(data) {
        let _ = enrollment.verify();
    }
});
