// Copy into 7e5bc9d's mct-observation/tests/ and use an isolated target directory.
// MCT_NEW_ISSUANCE_PREFIX names this branch's admin_issuance canonical-prefix.json.
#[test]
fn prior_binary_rejects_issuance_instead_of_ignoring_its_ceiling() {
    let bytes = std::fs::read(std::env::var("MCT_NEW_ISSUANCE_PREFIX").unwrap()).unwrap();
    let entries: Vec<mct_observation::MctObservationLedgerEntry> =
        serde_json::from_slice(&bytes).unwrap();
    let error = mct_observation::replay_authority_entries(&entries).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unknown variant `admin_key_issue`"),
        "{error}"
    );
}
