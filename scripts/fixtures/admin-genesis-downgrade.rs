// Copy into fe1e0c9's mct-observation/tests/; use an isolated target directory.
// MCT_NEW_GENESIS_PREFIX names admin_genesis/canonical-prefix.json on this branch.
#[test]
fn prior_binary_rejects_strict_genesis_instead_of_treating_it_as_legacy() {
    let bytes = std::fs::read(std::env::var("MCT_NEW_GENESIS_PREFIX").unwrap()).unwrap();
    let entries: Vec<mct_observation::MctObservationLedgerEntry> =
        serde_json::from_slice(&bytes).unwrap();
    let error = mct_observation::replay_authority_entries(&entries).unwrap_err();
    assert!(
        matches!(error,
        mct_observation::AuthorityReplayError::UnknownFactKind { ref fact_kind, .. }
            if fact_kind == "admin_genesis"),
        "{error}"
    );
}
