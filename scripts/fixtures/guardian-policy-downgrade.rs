// Copy into fe579eb's mct-observation/tests/ for the old-reader compatibility proof.
// MCT_NEW_POLICY_PREFIX names this branch's committed policy-prefix.json corpus.
#[test]
fn prior_binary_rejects_new_policy_facts_instead_of_resurrecting_the_old_roster() {
    let bytes = std::fs::read(std::env::var("MCT_NEW_POLICY_PREFIX").unwrap()).unwrap();
    let entries: Vec<mct_observation::MctObservationLedgerEntry> =
        serde_json::from_slice(&bytes).unwrap();
    let error = mct_observation::replay_authority_entries(&entries).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unknown variant `propose_policy`"),
        "{error}"
    );
}
