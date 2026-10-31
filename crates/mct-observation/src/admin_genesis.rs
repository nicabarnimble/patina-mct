//! D-57 fresh origin. This storage boundary is not a live bootstrap service.
use super::*;
use mct_kernel::recovery::RecoveryPublicKeyV1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminGenesisIntentV1 {
    pub bootstrap_id: String,
    pub mother_node_id: String,
    pub ledger_id: String,
    pub birth_epoch: String,
    pub inventory_hash: String,
    pub root: AdminKeyGrantV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminGenesisRequestV1 {
    pub intent: AdminGenesisIntentV1,
    pub signature: AdminSignatureV1,
}

/// Birth provenance, never a substitute for current root authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminGenesisFactV1 {
    pub request: AdminGenesisRequestV1,
    pub decided_at: Timestamp,
}

/// Trusted adapter input, deliberately not a wire/deserializable type.
///
/// This is an explicit trust boundary, not proof that the supplied inventory was
/// collected correctly. The producer must obtain pin and startup evidence from
/// trusted setup/disk, never from request fields. There is deliberately no
/// production constructor until that trusted producer and lifecycle are ready.
pub struct AdminGenesisSetupV1 {
    mother_node_id: String,
    ledger_id: String,
    pinned_root: RecoveryPublicKeyV1,
    inventory_hash: String,
}

fn failure(detail: &str) -> ObservationLedgerError {
    ObservationLedgerError::AuthorityReplay {
        detail: detail.into(),
    }
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
}

impl AdminGenesisSetupV1 {
    /// Accept independently collected trusted setup, not caller expectations.
    #[cfg(test)]
    pub(super) fn from_verified_virgin_startup(
        mother_node_id: String,
        ledger_id: String,
        pinned_root: RecoveryPublicKeyV1,
        startup: &AuthorityTenureStartupEvidenceV1,
    ) -> Result<Self> {
        if mother_node_id.trim().is_empty()
            || ledger_id.trim().is_empty()
            || startup.startup_class != AuthorityStartupClassV1::Virgin
            || startup.expected_predecessor != AuthorityEpochPredecessorV1::NoneForVirgin
            || startup.expected_prior_authority.is_some()
            || startup.expected_authority_state_hash
                != authority_state_hash(&AuthorityStateV1::default())?
            || startup.operator_gate_decision_id.is_some()
            || startup.authenticated_principal_ref.is_some()
            || !digest(&startup.inventory_hash)
        {
            return Err(failure("genesis requires explicit verified virgin setup"));
        }
        Ok(Self {
            mother_node_id,
            ledger_id,
            pinned_root,
            inventory_hash: startup.inventory_hash.clone(),
        })
    }
}

impl AdminGenesisRequestV1 {
    pub fn signing_message(&self) -> Result<Vec<u8>> {
        let intent = &self.intent;
        let grant = canonical_json_bytes(&intent.root)?;
        let command = canonical_json_bytes(&("mct-admin-genesis/v1", intent))?;
        Ok(admin_signed_message(AdminSignedMessageParts {
            scheme: AdminSignatureSchemeV1::Ed25519MlDsa65V1,
            mother_node_id: &intent.mother_node_id,
            ledger_id: &intent.ledger_id,
            nonce: 0,
            signer_grant_canonical: &grant,
            command_canonical: &command,
        }))
    }

    fn validate(&self) -> Result<()> {
        let i = &self.intent;
        let root = &i.root;
        let AdminKeyPublicHalf::Ed25519(ed) = root.public_half;
        if !id(&i.bootstrap_id)
            || !id(&root.grant_id)
            || root
                .grant_id
                .starts_with(guardian_recovery::RECOVERED_ROOT_GRANT_PREFIX)
            || i.mother_node_id.trim().is_empty()
            || i.ledger_id.trim().is_empty()
            || !i
                .birth_epoch
                .strip_prefix(AUTHORITY_EPOCH_PREFIX_V1)
                .is_some_and(digest)
            || !digest(&i.inventory_hash)
            || admin_key_grant_shape_error(root).is_some()
            || root.class != AdminKeyGrantClassV1::Recovery
            || root.grant_state != AdminKeyGrantStateV1::Active
            || ed.verifying_key().is_weak()
            || self.signature.signer_grant_id != root.grant_id
            || self.signature.nonce != 0
            || self.signature.scheme != AdminSignatureSchemeV1::Ed25519MlDsa65V1
            || self.signature.signature.len() != mct_kernel::ADMIN_HYBRID_SIGNATURE_LEN * 2
        {
            return Err(failure("malformed strict administrative genesis"));
        }
        let pq = root
            .ml_dsa65_public_key
            .as_ref()
            .ok_or_else(|| failure("genesis requires both public keys"))?;
        verify_admin_hybrid_signature(
            &root.public_half,
            pq,
            &self.signing_message()?,
            &self.signature.signature,
        )
        .map_err(|_| failure("genesis hybrid signature denied"))
    }
}

impl AdminGenesisFactV1 {
    fn fact_id(&self) -> String {
        format!("obs:admin-genesis:{}", self.request.intent.bootstrap_id)
    }
    fn identity(&self) -> GrantsAuthorityIdentityV1 {
        GrantsAuthorityIdentityV1 {
            mother_node_id: self.request.intent.mother_node_id.clone(),
            authority_epoch: self.request.intent.birth_epoch.clone(),
            generation: 1,
            source_authority_observation_id: self.fact_id(),
        }
    }
    pub(super) fn observation(&self) -> Result<MctObservation> {
        Ok(MctObservation {
            observation_id: ObservationId::new(self.fact_id()).expect("nonempty genesis id"),
            observed_at: self.decided_at.clone(),
            kind: ObservationKind::OperatorActionRecorded,
            source_plane: SourcePlane::Kernel,
            trace: ObservationTraceRef {
                trace_id: TraceId::new(format!(
                    "trace:admin-genesis:{}",
                    self.request.intent.bootstrap_id
                ))
                .expect("nonempty trace"),
                span_id: None,
                parent_span_id: None,
                external_trace_id: None,
            },
            call_id: None,
            decision_id: None,
            subject_id: Some(self.request.intent.mother_node_id.clone()),
            resource_id: Some(self.request.intent.ledger_id.clone()),
            policy_revision: None,
            grants_revision: Some(1),
            outcome: ObservationOutcome::Allowed,
            visibility: ObservationVisibility::NodeOperator,
            safe_message: "strict administrative genesis committed".into(),
            detail_ref: Some(encode_authority_fact(
                "admin_genesis",
                &self.fact_id(),
                self,
            )?),
        })
    }
}

pub(super) fn validate_state(state: &AuthorityStateV1) -> Result<()> {
    if let Some(genesis) = &state.admin_genesis {
        genesis.request.validate()?;
        let current = state
            .admin_key_grants
            .values()
            .find(|grant| {
                grant.class == AdminKeyGrantClassV1::Recovery
                    && grant.grant_state == AdminKeyGrantStateV1::Active
            })
            .ok_or_else(|| failure("strict profile has no active root"))?;
        let recovered = state
            .guardian_recovery
            .as_ref()
            .is_some_and(|r| r.binding.root_generation > 1);
        if !recovered && current != &genesis.request.intent.root {
            return Err(failure(
                "strict root differs from genesis without guardian replacement",
            ));
        }
        if state.guardian_recovery.as_ref().is_some_and(|r| {
            r.binding.mother_node_id.as_str() != genesis.request.intent.mother_node_id
                || r.binding.ledger_id != genesis.request.intent.ledger_id
        }) {
            return Err(failure(
                "guardian binding and strict origin target disagree",
            ));
        }
    }
    Ok(())
}

pub(super) fn replay_genesis(
    replay: &mut AuthorityReplayV1,
    used_epochs: &mut BTreeSet<String>,
    previous: Option<(u64, &str)>,
    entry: &MctObservationLedgerEntry,
    fact: AdminGenesisFactV1,
) -> std::result::Result<(), AuthorityReplayError> {
    let deny = |detail: &str| authority_incoherent(detail);
    fact.request.validate().map_err(|e| deny(&e.to_string()))?;
    if previous.is_some()
        || entry.previous_entry_hash.is_some()
        || entry.local_sequence != 0
        || replay.current_authority.is_some()
        || replay.state != AuthorityStateV1::default()
        || replay.canonical_fact_count != 0
        || !used_epochs.is_empty()
        || entry.mother_node_id != fact.request.intent.mother_node_id
        || entry.ledger_id != fact.request.intent.ledger_id
        || entry.durability_class != DurabilityClass::BeforeEffect
        || entry.appended_at != fact.decided_at.as_str()
        || entry.observation != fact.observation().map_err(|e| deny(&e.to_string()))?
    {
        return Err(deny(
            "strict genesis must be the exact first canonical frame",
        ));
    }
    replay.current_authority = Some(fact.identity());
    used_epochs.insert(fact.request.intent.birth_epoch.clone());
    replay
        .admin_signature_nonces
        .insert((fact.request.intent.root.grant_id.clone(), 0));
    replay.state.admin_key_grants.insert(
        fact.request.intent.root.grant_id.clone(),
        fact.request.intent.root.clone(),
    );
    replay.state.admin_genesis = Some(Box::new(fact));
    replay.canonical_fact_count += 1;
    Ok(())
}

impl JsonlObservationLedger {
    /// Local trusted-setup storage boundary. No network/UID bootstrap adapter is enabled.
    /// Returns a historical receipt on exact retry, never permission to reinitialize.
    pub fn execute_admin_genesis(
        &mut self,
        setup: &AdminGenesisSetupV1,
        request: AdminGenesisRequestV1,
    ) -> Result<MctObservationLedgerEntry> {
        self.execute_admin_genesis_at(
            setup,
            request,
            Timestamp::new(jiff::Timestamp::now().to_string()).expect("system timestamp"),
        )
    }

    // Writer-owned clock in production; deterministic storage test seam.
    pub(super) fn execute_admin_genesis_at(
        &mut self,
        setup: &AdminGenesisSetupV1,
        request: AdminGenesisRequestV1,
        now: Timestamp,
    ) -> Result<MctObservationLedgerEntry> {
        if self.is_poisoned() {
            return Err(failure("genesis writer is poisoned"));
        }
        request.validate()?;
        let i = &request.intent;
        if self.mother_node_id != setup.mother_node_id
            || self.ledger_id != setup.ledger_id
            || i.mother_node_id != setup.mother_node_id
            || i.ledger_id != setup.ledger_id
            || i.inventory_hash != setup.inventory_hash
            || i.root.public_half != setup.pinned_root.ed25519
            || i.root.ml_dsa65_public_key.as_ref() != Some(&setup.pinned_root.ml_dsa65)
        {
            return Err(failure("genesis does not match independently pinned setup"));
        }
        let replay = self.cached_replay()?;
        if let Some(prior) = &replay.state.admin_genesis {
            if prior.request != request {
                return Err(failure("genesis conflicts with committed birth"));
            }
            return self
                .entries()?
                .into_iter()
                .next()
                .ok_or_else(|| failure("genesis receipt missing"));
        }
        if !self.created_new
            || self.next_sequence != 0
            || self.authority_tenure.is_some()
            || replay.state != AuthorityStateV1::default()
            || replay.current_authority.is_some()
            || self.recovery_status.is_some()
        {
            return Err(failure("nonvirgin history cannot acquire strict genesis"));
        }
        let fact = AdminGenesisFactV1 {
            request,
            decided_at: now,
        };
        self.append_canonical_before_effect(fact.observation()?, fact.decided_at.to_string())
    }
}
