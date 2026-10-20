use super::*;
use crate::{
    AdminKeyCheckAction, AdminKeyGrantCheckRequest, evaluate_admin_key_grant, recovery_grant_scopes,
};

fn time(day: u8) -> Timestamp {
    Timestamp::new(format!("2026-10-{day:02}T00:00:00Z")).unwrap()
}
fn key(index: usize) -> AdminKeyPublicHalf {
    AdminKeyPublicHalf::ed25519_from_hex(
        [
            "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a",
            "3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c",
            "fc51cd8e6218a1a38da47ed00230f0580816ed13ba3303ac5deb911548908025",
        ][index],
    )
    .unwrap()
}
fn day(scopes: Vec<Scope>, ceiling: Option<Vec<Scope>>) -> AdminKeyGrantV1 {
    AdminKeyGrantV1 {
        grant_id: "day".into(),
        public_half: key(1),
        ml_dsa65_public_key: None,
        class: Class::DayToDay,
        scopes,
        expires_at: Some(time(22)),
        grant_state: State::Active,
        issuance_ceiling: ceiling.map(|scopes| AdminIssuanceCeilingV1 { scopes }),
    }
}
fn root() -> AdminKeyGrantV1 {
    AdminKeyGrantV1 {
        grant_id: "root".into(),
        public_half: key(0),
        ml_dsa65_public_key: None,
        class: Class::Recovery,
        scopes: recovery_grant_scopes().to_vec(),
        expires_at: None,
        grant_state: State::Active,
        issuance_ceiling: None,
    }
}
fn leaf(scope: Scope) -> AdminKeyGrantV1 {
    let mut grant = day(vec![scope], None);
    grant.grant_id = "leaf".into();
    grant.public_half = key(2);
    grant.expires_at = Some(time(21));
    grant
}

#[test]
fn admin_issuance_root_issues_day_scopes_but_cannot_execute_them() {
    let issuer = root();
    let recipient = leaf(Scope::PeerAdd);
    assert_eq!(
        evaluate_admin_key_issuance(&issuer, &recipient, &time(19)),
        Ok(())
    );
    let route = evaluate_admin_key_grant(
        &AdminKeyGrantCheckRequest {
            public_half: issuer.public_half,
            action: AdminKeyCheckAction::Route(Scope::PeerAdd),
            now: time(19),
            presented_generation: 1,
            current_generation: 1,
        },
        &[issuer],
    );
    assert!(!route.is_allowed());
}
#[test]
fn admin_issuance_ceiling_does_not_grant_execution_and_need_not_match_issuer_routes() {
    let issuer = day(vec![Scope::KeyManage], Some(vec![Scope::PeerAdd]));
    assert_eq!(
        evaluate_admin_key_issuance(&issuer, &leaf(Scope::PeerAdd), &time(19)),
        Ok(())
    );
    assert!(
        !evaluate_admin_key_grant(
            &AdminKeyGrantCheckRequest {
                public_half: issuer.public_half,
                action: AdminKeyCheckAction::Route(Scope::PeerAdd),
                now: time(19),
                presented_generation: 1,
                current_generation: 1
            },
            &[issuer]
        )
        .is_allowed()
    );
}
#[test]
fn admin_issuance_route_scope_cannot_substitute_for_missing_ceiling() {
    let issuer = day(vec![Scope::KeyManage, Scope::Status], None);
    assert!(
        admin_key_grant_shape_error(&issuer).is_none(),
        "legacy record remains inspectable"
    );
    assert_eq!(
        evaluate_admin_key_issuance(&issuer, &leaf(Scope::Status), &time(19)),
        Err(AdminIssuanceError::MissingCeiling)
    );
}
#[test]
fn admin_issuance_explicit_empty_ceiling_is_revoke_only() {
    let issuer = day(vec![Scope::KeyManage], Some(vec![]));
    assert_eq!(
        evaluate_admin_key_issuance(&root(), &issuer, &time(19)),
        Ok(())
    );
    assert_eq!(
        evaluate_admin_key_issuance(&issuer, &leaf(Scope::Status), &time(19)),
        Err(AdminIssuanceError::CeilingExceeded)
    );
}
#[test]
fn admin_issuance_recipient_execution_cannot_exceed_ceiling() {
    let issuer = day(
        vec![Scope::KeyManage, Scope::Status],
        Some(vec![Scope::PeerAdd]),
    );
    assert_eq!(
        evaluate_admin_key_issuance(&issuer, &leaf(Scope::Status), &time(19)),
        Err(AdminIssuanceError::CeilingExceeded)
    );
}
#[test]
fn admin_issuance_recipient_delegation_is_independently_bounded() {
    let issuer = day(
        vec![Scope::KeyManage],
        Some(vec![Scope::KeyManage, Scope::Status]),
    );
    let mut recipient = leaf(Scope::KeyManage);
    recipient.issuance_ceiling = Some(AdminIssuanceCeilingV1 {
        scopes: vec![Scope::PeerAdd],
    });
    assert_eq!(
        evaluate_admin_key_issuance(&issuer, &recipient, &time(19)),
        Err(AdminIssuanceError::CeilingExceeded)
    );
    recipient.issuance_ceiling.as_mut().unwrap().scopes = vec![Scope::Status];
    assert_eq!(
        evaluate_admin_key_issuance(&issuer, &recipient, &time(19)),
        Ok(())
    );
}
#[test]
fn admin_issuance_new_managers_must_declare_their_ceiling() {
    assert_eq!(
        evaluate_admin_key_issuance(&root(), &leaf(Scope::KeyManage), &time(19)),
        Err(AdminIssuanceError::MissingCeiling)
    );
}
#[test]
fn admin_issuance_ceiling_shape_rejects_unsorted_duplicate_startup_and_hidden_authority() {
    for scopes in [
        vec![Scope::Status, Scope::KeyManage],
        vec![Scope::Status, Scope::Status],
        vec![Scope::StartupGate],
    ] {
        let malformed = day(vec![Scope::KeyManage], Some(scopes));
        assert!(admin_key_grant_shape_error(&malformed).is_some());
    }
    assert!(
        admin_key_grant_shape_error(&day(vec![Scope::Status], Some(vec![Scope::Status]))).is_some()
    );
    let mut malformed = root();
    malformed.issuance_ceiling = Some(AdminIssuanceCeilingV1 { scopes: vec![] });
    assert!(admin_key_grant_shape_error(&malformed).is_some());
}
#[test]
fn admin_issuance_expiry_is_mandatory_future_and_bounded_by_day_issuer() {
    let issuer = day(vec![Scope::KeyManage], Some(vec![Scope::Status]));
    let mut recipient = leaf(Scope::Status);
    for expiry in [None, Some(time(19)), Some(time(23))] {
        recipient.expires_at = expiry;
        assert!(evaluate_admin_key_issuance(&issuer, &recipient, &time(19)).is_err());
    }
    recipient.expires_at = issuer.expires_at.clone();
    assert_eq!(
        evaluate_admin_key_issuance(&issuer, &recipient, &time(19)),
        Ok(())
    );
}
#[test]
fn admin_issuance_revoked_expired_and_nonmanager_issuers_deny() {
    let valid = day(vec![Scope::KeyManage], Some(vec![Scope::Status]));
    let mut revoked = valid.clone();
    revoked.grant_state = State::Revoked;
    assert_eq!(
        evaluate_admin_key_issuance(&revoked, &leaf(Scope::Status), &time(19)),
        Err(AdminIssuanceError::InactiveIssuer)
    );
    assert_eq!(
        evaluate_admin_key_issuance(&valid, &leaf(Scope::Status), &time(22)),
        Err(AdminIssuanceError::ExpiredIssuer)
    );
    assert_eq!(
        evaluate_admin_key_issuance(
            &day(vec![Scope::Status], None),
            &leaf(Scope::Status),
            &time(19)
        ),
        Err(AdminIssuanceError::NotKeyManager)
    );
}
#[test]
fn admin_issuance_never_creates_recovery_or_reuses_root_credential_for_execution() {
    assert_eq!(
        evaluate_admin_key_issuance(&root(), &root(), &time(19)),
        Err(AdminIssuanceError::RecoveryGrantLocked)
    );
    let mut recipient = leaf(Scope::Status);
    recipient.public_half = root().public_half;
    assert_eq!(
        evaluate_admin_key_issuance(&root(), &recipient, &time(19)),
        Err(AdminIssuanceError::RecoveryCredentialReuse)
    );
    recipient.public_half = key(2);
    recipient.grant_state = State::Revoked;
    assert_eq!(
        evaluate_admin_key_issuance(&root(), &recipient, &time(19)),
        Err(AdminIssuanceError::Malformed)
    );
}
#[test]
fn admin_issuance_does_not_union_same_key_grants() {
    let first = day(vec![Scope::KeyManage], Some(vec![Scope::PeerAdd]));
    let second = day(vec![Scope::KeyManage], Some(vec![Scope::Status]));
    let mut both = leaf(Scope::Status);
    both.scopes = vec![Scope::PeerAdd, Scope::Status];
    assert_eq!(
        evaluate_admin_key_issuance(&first, &both, &time(19)),
        Err(AdminIssuanceError::CeilingExceeded)
    );
    assert_eq!(
        evaluate_admin_key_issuance(&second, &both, &time(19)),
        Err(AdminIssuanceError::CeilingExceeded)
    );
}
#[test]
fn admin_issuance_weak_recipient_key_denies() {
    let mut recipient = leaf(Scope::Status);
    let mut bytes = [0u8; 32];
    bytes[0] = 1;
    recipient.public_half = AdminKeyPublicHalf::ed25519_from_bytes(&bytes).unwrap();
    assert_eq!(
        evaluate_admin_key_issuance(&root(), &recipient, &time(19)),
        Err(AdminIssuanceError::Malformed)
    );
}
#[test]
fn admin_issuance_all_route_scope_pairs_preserve_the_ceiling_boundary() {
    let scopes = [
        Scope::ApproveChild,
        Scope::Artifact,
        Scope::AuthorityImport,
        Scope::AuthorizeSecret,
        Scope::AuthorizeSlate,
        Scope::Blob,
        Scope::KeyManage,
        Scope::Lifecycle,
        Scope::PandoRecord,
        Scope::PeerAdd,
        Scope::PeerProof,
        Scope::PeerRemove,
        Scope::PeerRevoke,
        Scope::Registry,
        Scope::RevokeChild,
        Scope::StartupGate,
        Scope::Status,
        Scope::Trigger,
        Scope::WatchGrant,
        Scope::WatchRevoke,
    ];
    for ceiling in scopes {
        let issuer = day(vec![Scope::KeyManage], Some(vec![ceiling]));
        for execution in scopes {
            let mut recipient = leaf(execution);
            if execution == Scope::KeyManage {
                recipient.issuance_ceiling = Some(AdminIssuanceCeilingV1 { scopes: Vec::new() });
            }
            assert_eq!(
                evaluate_admin_key_issuance(&issuer, &recipient, &time(19)).is_ok(),
                ceiling == execution && execution != Scope::StartupGate,
                "ceiling={ceiling:?}, execution={execution:?}"
            );
            assert_eq!(
                evaluate_admin_key_issuance(&root(), &recipient, &time(19)).is_ok(),
                execution != Scope::StartupGate
            );
        }
    }
}

#[test]
fn admin_issuance_wire_is_strict_and_legacy_omission_is_stable() {
    let legacy = day(vec![Scope::KeyManage], None);
    let bytes = serde_json::to_vec(&legacy).unwrap();
    assert!(
        !String::from_utf8(bytes.clone())
            .unwrap()
            .contains("issuance_ceiling")
    );
    assert_eq!(
        serde_json::from_slice::<AdminKeyGrantV1>(&bytes).unwrap(),
        legacy
    );
    for json in [
        r#"{"scopes":[],"unlimited":true}"#,
        r#"{"scopes":[],"scopes":[]}"#,
    ] {
        assert!(serde_json::from_str::<AdminIssuanceCeilingV1>(json).is_err());
    }
}
