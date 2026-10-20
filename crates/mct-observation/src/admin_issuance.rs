//! Canonical D-56 issuance. Storage API only; authenticated runtime is still gated.
use super::*;
use mct_kernel::evaluate_admin_key_issuance;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminKeyIssueIntentV1 {
    pub issuance_id: String,
    pub ledger_id: String,
    pub expected_authority: AuthorityStateReferenceV1,
    pub issuer_grant_id: String,
    pub grant: AdminKeyGrantV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminKeyIssueRequestV1 {
    pub intent: AdminKeyIssueIntentV1,
    pub signature: AdminSignatureV1,
}

/// Reconstructable provenance, not a source of present issuer authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminKeyIssuanceRecordV1 {
    pub intent: AdminKeyIssueIntentV1,
    pub issuer_at_acceptance: AdminKeyGrantV1,
    pub decided_at: Timestamp,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
}

pub(super) fn command_bytes(request: &AdminKeyIssueRequestV1) -> Result<Vec<u8>> {
    canonical_json_bytes(&("mct-admin-key-issue/v1", &request.intent))
}

pub(super) fn validate_context(
    used_grant_ids: &BTreeSet<String>,
    changes: &[AuthorityChangeV1],
    authority: &GrantsAuthorityIdentityV1,
    ledger_id: &str,
    mutation_id: &str,
    decision_time: &str,
) -> std::result::Result<(), String> {
    for change in changes {
        if let AuthorityChangeV1::AdminKeyIssue {
            request,
            decided_at,
        } = change
            && (used_grant_ids.contains(&request.intent.grant.grant_id)
                || request.intent.expected_authority.grants_authority != *authority
                || request.intent.ledger_id != ledger_id
                || request.intent.issuance_id != mutation_id
                || decided_at.as_str() != decision_time)
        {
            return Err(
                "issuance does not name the current target, epoch, generation and command".into(),
            );
        }
    }
    Ok(())
}

pub(super) fn validate_state(state: &AuthorityStateV1) -> std::result::Result<(), String> {
    for (id, record) in &state.admin_issuance {
        if id != &record.intent.grant.grant_id
            || record.intent.issuer_grant_id != record.issuer_at_acceptance.grant_id
        {
            return Err("issuance provenance identity mismatch".into());
        }
        let current = state
            .admin_key_grants
            .get(id)
            .ok_or("issued grant missing from canonical state")?;
        let mut original = current.clone();
        original.grant_state = AdminKeyGrantStateV1::Active;
        if original != record.intent.grant {
            return Err("issued grant disagrees with provenance".into());
        }
        evaluate_admin_key_issuance(
            &record.issuer_at_acceptance,
            &record.intent.grant,
            &record.decided_at,
        )
        .map_err(|e| e.to_string())?;
    }
    for grant in state.admin_key_grants.values() {
        if grant.issuance_ceiling.is_some() && !state.admin_issuance.contains_key(&grant.grant_id) {
            return Err("ceiling lacks a canonical issuance fact".into());
        }
    }
    Ok(())
}

pub(super) fn apply(
    state: &mut AuthorityStateV1,
    request: &AdminKeyIssueRequestV1,
    decided_at: &Timestamp,
) -> std::result::Result<(), AuthorityReplayError> {
    let deny = |detail: &str| authority_incoherent(detail);
    let intent = &request.intent;
    let prior_time = state
        .admin_issuance
        .values()
        .map(|record| &record.decided_at)
        .chain(state.admin_genesis.iter().map(|birth| &birth.decided_at))
        .chain(
            state
                .guardian_recovery
                .iter()
                .map(|recovery| &recovery.clock.wall_time),
        )
        .max();
    if prior_time.is_some_and(|floor| decided_at < floor) {
        return Err(deny(
            "issuance decision clock regressed behind canonical authority",
        ));
    }
    // Bound caller-created IDs, not inherited canonical identifiers: generated
    // root IDs can exceed a binding ID's 128-byte bound. Issuer/ledger identity
    // is matched exactly to canonical state/context instead.
    if !valid_id(&intent.issuance_id)
        || !valid_id(&intent.grant.grant_id)
        || intent
            .grant
            .grant_id
            .starts_with(guardian_recovery::RECOVERED_ROOT_GRANT_PREFIX)
        || request.signature.signer_grant_id != intent.issuer_grant_id
        || state.admin_key_grants.contains_key(&intent.grant.grant_id)
        || state.admin_issuance.contains_key(&intent.grant.grant_id)
        || state
            .guardian_recovery
            .as_ref()
            .is_some_and(|r| r.maintenance)
    {
        return Err(deny(
            "invalid issuance identity, existing grant, signer mismatch or recovery maintenance",
        ));
    }
    if authority_state_hash(state).map_err(|e| deny(&e.to_string()))?
        != intent.expected_authority.authority_state_hash
    {
        return Err(deny("issuance expected authority snapshot is stale"));
    }
    let issuer = state
        .admin_key_grants
        .get(&intent.issuer_grant_id)
        .ok_or_else(|| deny("issuer is not canonical"))?;
    evaluate_admin_key_issuance(issuer, &intent.grant, decided_at)
        .map_err(|e| deny(&e.to_string()))?;
    if issuer.ml_dsa65_public_key.is_none()
        || intent.grant.ml_dsa65_public_key.is_none()
        || request.signature.signature.len() != mct_kernel::ADMIN_HYBRID_SIGNATURE_LEN * 2
    {
        return Err(deny(
            "new issuance requires complete hybrid credentials and signature",
        ));
    }
    let recipient = &intent.grant;
    let matches_root = |key: &mct_kernel::recovery::RecoveryPublicKeyV1| {
        key.ed25519 == recipient.public_half
            || Some(&key.ml_dsa65) == recipient.ml_dsa65_public_key.as_ref()
    };
    if state.admin_key_grants.values().any(|g| {
        g.class == AdminKeyGrantClassV1::Recovery
            && (g.public_half == recipient.public_half
                || g.ml_dsa65_public_key
                    .as_ref()
                    .is_some_and(|key| Some(key) == recipient.ml_dsa65_public_key.as_ref()))
    }) || state
        .guardian_recovery
        .as_ref()
        .is_some_and(|r| r.retired_root_keys.iter().any(matches_root))
    {
        return Err(deny("root credentials cannot be issued as day keys"));
    }
    let change = AuthorityChangeV1::AdminKeyIssue {
        request: Box::new(request.clone()),
        decided_at: decided_at.clone(),
    };
    // Standalone apply is a proposed-state helper, not a writer capability. Still
    // require the signature; the canonical writer additionally consumes nonces.
    verify_admin_change_signatures(
        state,
        &BTreeSet::new(),
        &intent.expected_authority.grants_authority.mother_node_id,
        &intent.ledger_id,
        &[change],
    )
    .map_err(|e| deny(&e))?;
    let record = AdminKeyIssuanceRecordV1 {
        intent: intent.clone(),
        issuer_at_acceptance: issuer.clone(),
        decided_at: decided_at.clone(),
    };
    let mut proposed = state.clone();
    apply_admin_key_grant_put(&mut proposed, recipient)?;
    proposed
        .admin_issuance
        .insert(recipient.grant_id.clone(), record);
    authority_state_hash(&proposed).map_err(|e| deny(&e.to_string()))?;
    *state = proposed;
    Ok(())
}

impl JsonlObservationLedger {
    /// Commit one exact issuer-authorized grant. Not exposed through old ingress.
    pub fn execute_admin_key_issuance(
        &mut self,
        request: AdminKeyIssueRequestV1,
    ) -> AuthorityMutationResultV1 {
        self.execute_admin_key_issuance_at(
            request,
            Timestamp::new(jiff::Timestamp::now().to_string()).expect("system timestamp"),
        )
    }

    // Deterministic Mother-time seam, private outside this storage crate.
    pub(super) fn execute_admin_key_issuance_at(
        &mut self,
        request: AdminKeyIssueRequestV1,
        now: Timestamp,
    ) -> AuthorityMutationResultV1 {
        let id = request.intent.issuance_id.clone();
        let Ok(replay) = self.cached_replay() else {
            return rejected_mutation(id, AuthorityMutationRejectionReasonV1::PriorStateMismatch);
        };
        let (decided_at, sources) = if let Some(existing) = replay.mutations.get(&id) {
            match existing.fact.changes.as_slice() {
                [
                    AuthorityChangeV1::AdminKeyIssue {
                        request: prior,
                        decided_at,
                    },
                ] if prior.as_ref() == &request => (
                    decided_at.clone(),
                    existing.fact.grant_shaping_sources.clone(),
                ),
                _ => {
                    return rejected_mutation(
                        id,
                        AuthorityMutationRejectionReasonV1::MutationIdConflict,
                    );
                }
            }
        } else {
            let Some(issuer) = replay
                .state
                .admin_key_grants
                .get(&request.intent.issuer_grant_id)
            else {
                return rejected_mutation(id, AuthorityMutationRejectionReasonV1::InvalidRequest);
            };
            (
                now,
                vec![GrantShapingSourceV1::AdminKey {
                    public_half: issuer.public_half,
                }],
            )
        };
        self.execute_authority_mutation_inner(
            AuthorityMutationRequestV1 {
                mutation_id: id,
                decided_at: decided_at.to_string(),
                grant_shaping_sources: sources,
                changes: vec![AuthorityChangeV1::AdminKeyIssue {
                    request: Box::new(request),
                    decided_at,
                }],
            },
            |_| Ok(None),
            true,
        )
    }
}
