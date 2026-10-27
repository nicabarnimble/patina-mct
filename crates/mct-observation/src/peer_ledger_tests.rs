use super::*;
use std::os::unix::fs::PermissionsExt;

fn root() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
fn time(s: &str) -> Timestamp {
    Timestamp::new(s).unwrap()
}
fn request(id: &str, change: AuthorityChangeV1, at: &Timestamp) -> AuthorityMutationRequestV1 {
    AuthorityMutationRequestV1 {
        mutation_id: id.into(),
        changes: vec![change],
        decided_at: at.to_string(),
        grant_shaping_sources: vec![GrantShapingSourceV1::OperatorDecision {
            decision_id: "test-owned-ticket-issuer".into(),
            authenticated_principal_ref: "fixture-operator".into(),
            command_kind: GrantShapingCommandKindV1::GrantChange,
        }],
    }
}
fn issue(ledger: &mut JsonlObservationLedger, scoped: bool) {
    let result = ledger.execute_authority_mutation(
        request(
            "issue",
            AuthorityChangeV1::PairingTicketIssued {
                ticket_hash: "ab1234".into(),
                peer_endpoint_id: scoped.then(|| "peer-a".into()),
                window_starts_at: scoped.then(|| time("2026-08-06T12:00:00Z")),
                window_expires_at: scoped.then(|| time("2026-08-06T13:00:00Z")),
                signature: None,
            },
            &time("2026-08-06T11:00:00Z"),
        ),
        |_| Ok(None),
    );
    assert!(
        matches!(
            result,
            AuthorityMutationResultV1::Committed { .. }
                | AuthorityMutationResultV1::CommittedProjectionPending { .. }
        ),
        "{result:?}"
    );
}
fn consume(at: &Timestamp) -> AuthorityMutationRequestV1 {
    request(
        "consume",
        AuthorityChangeV1::PairingTicketConsumed {
            ticket_hash: "ab1234".into(),
            admission: Some(PairingTicketAdmissionV1 {
                authenticated_peer_endpoint_id: "peer-a".into(),
                admitted_at: at.clone(),
            }),
            signature: None,
        },
        at,
    )
}
fn rejected(result: AuthorityMutationResultV1) {
    assert!(
        matches!(
            result,
            AuthorityMutationResultV1::RejectedBeforeCommit { .. }
        ),
        "{result:?}"
    );
}

#[test]
fn l11_pairing_requires_authenticated_target_mother_window_and_single_use() {
    let root = root();
    let path = root.path().canonicalize().unwrap().join("ledger");
    let mut ledger = JsonlObservationLedger::open_authority(&path, "ledger", "mother").unwrap();
    issue(&mut ledger, true);
    let before = std::fs::read(&path).unwrap();
    let now = time("2026-08-06T12:30:00Z");
    let peer = mct_kernel::EndpointIdText::new("peer-a").unwrap();
    rejected(ledger.execute_authority_mutation(consume(&now), |_| panic!("must not project")));
    for (endpoint, at) in [
        ("other-peer", "2026-08-06T12:30:00Z"),
        ("peer-a", "2026-08-06T11:59:59Z"),
        ("peer-a", "2026-08-06T13:00:00Z"),
        ("peer-a", "2026-08-06T14:00:00Z"),
    ] {
        let at = time(at);
        rejected(ledger.execute_pairing_ticket_admission(
            consume(&at),
            &mct_kernel::EndpointIdText::new(endpoint).unwrap(),
            &at,
            |_| panic!("must not project"),
        ));
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    rejected(ledger.execute_pairing_ticket_admission(
        consume(&now),
        &peer,
        &time("2026-08-06T13:00:00Z"),
        |_| panic!("caller time cannot substitute for Mother time"),
    ));
    let mut mixed = consume(&now);
    mixed.changes.push(AuthorityChangeV1::ToyGrantRemove {
        grant_id: "unrelated".into(),
    });
    rejected(
        ledger.execute_pairing_ticket_admission(mixed, &peer, &now, |_| {
            panic!("ticket consumption cannot smuggle another authority change")
        }),
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    // Equal instants with different RFC3339 offsets are valid; end is exclusive.
    let now = time("2026-08-06T14:00:00+02:00");
    let result = ledger.execute_pairing_ticket_admission(consume(&now), &peer, &now, |_| Ok(None));
    assert!(
        matches!(
            result,
            AuthorityMutationResultV1::Committed { .. }
                | AuthorityMutationResultV1::CommittedProjectionPending { .. }
        ),
        "{result:?}"
    );
    let consumed_bytes = std::fs::read(&path).unwrap();
    rejected(
        ledger.execute_pairing_ticket_admission(consume(&now), &peer, &now, |_| {
            panic!("replay must not admit")
        }),
    );
    assert_eq!(std::fs::read(&path).unwrap(), consumed_bytes);
    drop(ledger);
    let mut reopened = JsonlObservationLedger::open_authority(&path, "ledger", "mother").unwrap();
    rejected(
        reopened.execute_pairing_ticket_admission(consume(&now), &peer, &now, |_| {
            panic!("restart must not admit")
        }),
    );
}

#[test]
fn l11_historical_unscoped_tickets_are_readable_but_inert_for_admission() {
    let root = root();
    let path = root.path().canonicalize().unwrap().join("ledger");
    let mut ledger = JsonlObservationLedger::open_authority(&path, "ledger", "mother").unwrap();
    issue(&mut ledger, false);
    let now = time("2026-08-06T12:30:00Z");
    let before = std::fs::read(&path).unwrap();
    rejected(ledger.execute_pairing_ticket_admission(
        consume(&now),
        &mct_kernel::EndpointIdText::new("peer-a").unwrap(),
        &now,
        |_| panic!("missing scope"),
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert!(
        ledger
            .verified_replay()
            .unwrap()
            .state
            .pairing_tickets
            .contains_key("ab1234")
    );
}

#[test]
fn l11_complete_unacknowledged_consumption_is_not_admitted_twice() {
    let root = root();
    let path = root.path().canonicalize().unwrap().join("ledger");
    let mut ledger = JsonlObservationLedger::open_authority(&path, "ledger", "mother").unwrap();
    issue(&mut ledger, true);
    let now = time("2026-08-06T12:30:00Z");
    let peer = mct_kernel::EndpointIdText::new("peer-a").unwrap();
    ledger.inject_append_fault_after_for_test(0, TestAppendFault::CompleteFrameAfterDurabilityAck);
    let result = ledger.execute_pairing_ticket_admission(consume(&now), &peer, &now, |_| {
        panic!("uncertainty denies effect")
    });
    assert!(
        matches!(result, AuthorityMutationResultV1::CommitUnknown { .. }),
        "{result:?}"
    );
    drop(ledger);
    let mut reopened = JsonlObservationLedger::open_authority(&path, "ledger", "mother").unwrap();
    rejected(
        reopened.execute_pairing_ticket_admission(consume(&now), &peer, &now, |_| {
            panic!("canonical consumption survives lost ack")
        }),
    );
}

fn ordinary() -> MctObservation {
    MctObservation::informational(
        ObservationId::new("ordinary").unwrap(),
        time("2026-08-06T12:00:00Z"),
        ObservationKind::OperatorActionRecorded,
        TraceId::new("trace").unwrap(),
        "ordinary",
    )
}
#[test]
fn l12_ordinary_single_and_batched_appends_cannot_write_authority_namespaces() {
    let root = root();
    let path = root.path().canonicalize().unwrap().join("ledger");
    let mut ledger = JsonlObservationLedger::open_authority(&path, "ledger", "mother").unwrap();
    let before = std::fs::read(&path).unwrap();
    let head = ledger.verified_head().unwrap();
    // Even an exactly valid canonical fact cannot enter through ordinary/buffered append.
    let valid_authority_fact = ledger.entries().unwrap()[0].observation.clone();
    assert!(matches!(
        ledger.append(
            valid_authority_fact,
            "2026-08-06T12:00:00Z",
            DurabilityClass::Buffered,
            ExportStatus::NotRequired
        ),
        Err(ObservationLedgerError::ReservedAuthorityDetail)
    ));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    for prefix in [
        "mct-authority-fact-v1:",
        "mct-authority-fact-v99:",
        "mct-authority-epoch-v1:",
        "ledger-tail-recovery-v1:",
    ] {
        let mut forged = ordinary();
        forged.detail_ref = Some(format!("{prefix}{{bad}}"));
        assert!(matches!(
            ledger.append_before_effect(forged.clone(), "2026-08-06T12:00:00Z"),
            Err(ObservationLedgerError::ReservedAuthorityDetail)
        ));
        assert!(
            ledger
                .append_durable_batch(
                    [ordinary(), forged.clone()],
                    "2026-08-06T12:00:00Z",
                    DurabilityClass::BeforeEffect,
                    ExportStatus::NotRequired
                )
                .is_err()
        );
        assert!(
            ledger
                .append_batch_before_effect([forged], "2026-08-06T12:00:00Z")
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(ledger.verified_head().unwrap(), head);
        assert!(ledger.verified_replay().is_ok());
    }
    let authority = ledger.verified_replay().unwrap().current_authority.clone();
    let mut forged = ordinary();
    forged.detail_ref = Some("mct-authority-fact-v1:{}".into());
    let failure = ledger
        .append_batch_before_effect([ordinary(), forged], "2026-08-06T12:00:00Z")
        .unwrap_err();
    assert!(
        matches!(failure, ObservationLedgerError::BatchPartiallyCommitted { outcome }
        if outcome.acknowledged_committed_prefix.len() == 1 && !outcome.commit_unknown)
    );
    assert_eq!(
        ledger.verified_replay().unwrap().current_authority,
        authority
    );
    ledger
        .append_before_effect(ordinary(), "2026-08-06T12:00:00Z")
        .unwrap();
    issue(&mut ledger, true); // typed canonical writer still works
    drop(ledger);
    assert!(JsonlObservationLedger::open_authority(path, "ledger", "mother").is_ok());
}
