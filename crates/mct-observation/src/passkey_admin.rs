//! Canonical passkey genesis, day-key leases, revocation and BeforeEffect admission.
use super::*;
use mct_kernel::admin_key::{PasskeyAdminActionV1, PasskeyAdminStateV1, WebAuthnP256DayKeyV1};
use mct_kernel::admin_sig::{
    DayKeyAdminProofV1, PasskeyAdminRequestV1, evaluate_passkey_admin_transition,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasskeyAdminFactV1 {
    pub request: PasskeyAdminRequestV1,
    pub decided_at: Timestamp,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasskeyAdminCanonicalV1 {
    pub genesis: PasskeyAdminFactV1,
    pub state: PasskeyAdminStateV1,
}

/// Host-only setup assertion, not deserializable request data. The producer must
/// independently register/pin the credential and classify disk before opening a
/// new writer. Neither the request nor legacy grants can supply these facts.
pub struct PasskeyGenesisSetupV1 {
    mother_node_id: String,
    ledger_id: String,
    root: WebAuthnP256DayKeyV1,
    inventory_hash: String,
}
fn denied(detail: impl ToString) -> ObservationLedgerError {
    ObservationLedgerError::AuthorityReplay {
        detail: detail.to_string(),
    }
}
impl PasskeyGenesisSetupV1 {
    pub fn from_registered_passkey(
        mother_node_id: String,
        ledger_id: String,
        root: WebAuthnP256DayKeyV1,
        startup: &AuthorityTenureStartupEvidenceV1,
    ) -> Result<Self> {
        root.validate().map_err(denied)?;
        if startup.startup_class != AuthorityStartupClassV1::Virgin
            || startup.expected_predecessor != AuthorityEpochPredecessorV1::NoneForVirgin
            || startup.expected_prior_authority.is_some()
            || startup.expected_authority_state_hash
                != authority_state_hash(&AuthorityStateV1::default())?
            || startup.operator_gate_decision_id.is_some()
            || startup.authenticated_principal_ref.is_some()
            || !digest(&startup.inventory_hash)
        {
            return Err(denied(
                "passkey genesis requires independent virgin startup",
            ));
        }
        Ok(Self {
            mother_node_id,
            ledger_id,
            root,
            inventory_hash: startup.inventory_hash.clone(),
        })
    }

    fn validate_request(&self, request: &PasskeyAdminRequestV1) -> Result<()> {
        let i = &request.intent;
        let PasskeyAdminActionV1::Bootstrap {
            root,
            inventory_hash,
        } = &i.action
        else {
            return Err(denied("not genesis"));
        };
        if i.mother_node_id != self.mother_node_id
            || i.ledger_id != self.ledger_id
            || root != &self.root
            || inventory_hash != &self.inventory_hash
        {
            return Err(denied("genesis disagrees with trusted registration/setup"));
        }
        Ok(())
    }
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn now() -> Timestamp {
    Timestamp::new(jiff::Timestamp::now().to_string()).expect("system time")
}
impl PasskeyAdminFactV1 {
    fn id(&self) -> Result<String> {
        Ok(format!(
            "obs:passkey-admin:{}",
            blake3::hash(&canonical_json_bytes(&self.request)?).to_hex()
        ))
    }
    fn identity(&self) -> Result<GrantsAuthorityIdentityV1> {
        Ok(GrantsAuthorityIdentityV1 {
            mother_node_id: self.request.intent.mother_node_id.clone(),
            authority_epoch: self.request.intent.epoch.clone(),
            generation: self
                .request
                .intent
                .generation
                .checked_add(1)
                .ok_or_else(|| denied("generation overflow"))?,
            source_authority_observation_id: self.id()?,
        })
    }
    fn observation(&self) -> Result<MctObservation> {
        let id = self.id()?;
        Ok(MctObservation {
            observation_id: ObservationId::new(id.clone()).expect("fixed id"),
            observed_at: self.decided_at.clone(),
            kind: ObservationKind::OperatorActionRecorded,
            source_plane: SourcePlane::Kernel,
            trace: ObservationTraceRef {
                trace_id: TraceId::new(format!("trace:{id}")).expect("fixed trace"),
                span_id: None,
                parent_span_id: None,
                external_trace_id: None,
            },
            call_id: None,
            decision_id: None,
            subject_id: Some(self.request.intent.mother_node_id.clone()),
            resource_id: Some(self.request.intent.ledger_id.clone()),
            policy_revision: None,
            grants_revision: Some(self.identity()?.generation),
            outcome: ObservationOutcome::Allowed,
            visibility: ObservationVisibility::NodeOperator,
            safe_message: "passkey administration committed".into(),
            detail_ref: Some(encode_authority_fact("passkey_admin", &id, self)?),
        })
    }
}
fn transition(
    replay: &AuthorityReplayV1,
    fact: &PasskeyAdminFactV1,
) -> Result<PasskeyAdminCanonicalV1> {
    let i = &fact.request.intent;
    let prior = replay.state.passkey_admin.as_deref();
    if let Some(prior) = prior {
        let current = replay
            .current_authority
            .as_ref()
            .ok_or_else(|| denied("missing authority"))?;
        if i.mother_node_id != current.mother_node_id
            || i.epoch != current.authority_epoch
            || i.generation != current.generation
            || i.authority_observation_id != current.source_authority_observation_id
        {
            return Err(denied("stale or wrong authority snapshot"));
        }
        return Ok(PasskeyAdminCanonicalV1 {
            genesis: prior.genesis.clone(),
            state: evaluate_passkey_admin_transition(
                Some(&prior.state),
                &fact.request,
                &fact.decided_at,
            )
            .map_err(denied)?,
        });
    }
    let PasskeyAdminActionV1::Bootstrap { inventory_hash, .. } = &i.action else {
        return Err(denied("no passkey genesis"));
    };
    if replay.state != AuthorityStateV1::default()
        || replay.current_authority.is_some()
        || replay.canonical_fact_count != 0
        || i.generation != 0
        || !i.authority_observation_id.is_empty()
        || !i
            .epoch
            .strip_prefix(AUTHORITY_EPOCH_PREFIX_V1)
            .is_some_and(digest)
        || !digest(inventory_hash)
    {
        return Err(denied(
            "legacy authority cannot promote into passkey genesis",
        ));
    }
    Ok(PasskeyAdminCanonicalV1 {
        genesis: fact.clone(),
        state: evaluate_passkey_admin_transition(None, &fact.request, &fact.decided_at)
            .map_err(denied)?,
    })
}
pub(super) fn replay_fact(
    replay: &mut AuthorityReplayV1,
    used_epochs: &mut BTreeSet<String>,
    previous: Option<(u64, &str)>,
    entry: &MctObservationLedgerEntry,
    fact: PasskeyAdminFactV1,
) -> std::result::Result<(), AuthorityReplayError> {
    let fail = |e: ObservationLedgerError| authority_incoherent(e.to_string());
    if entry.mother_node_id != fact.request.intent.mother_node_id
        || entry.ledger_id != fact.request.intent.ledger_id
        || entry.durability_class != DurabilityClass::BeforeEffect
        || entry.appended_at != fact.decided_at.as_str()
        || entry.observation != fact.observation().map_err(fail)?
    {
        return Err(authority_incoherent("passkey fact envelope mismatch"));
    }
    if replay.state.passkey_admin.is_none() {
        if previous.is_some()
            || entry.previous_entry_hash.is_some()
            || entry.local_sequence != 0
            || !used_epochs.is_empty()
        {
            return Err(authority_incoherent(
                "passkey genesis must be the first physical frame",
            ));
        }
        used_epochs.insert(fact.request.intent.epoch.clone());
    }
    let next = transition(replay, &fact).map_err(fail)?;
    replay.current_authority = Some(fact.identity().map_err(fail)?);
    replay.state.passkey_admin = Some(Box::new(next));
    replay.canonical_fact_count += 1;
    Ok(())
}

impl JsonlObservationLedger {
    /// Rejects invalid genesis before creating storage. Existing files are never
    /// reopened here; I/O failures after admission retain the normal recovery gate.
    pub fn create_passkey_genesis(
        path: impl AsRef<Path>,
        setup: &PasskeyGenesisSetupV1,
        request: PasskeyAdminRequestV1,
    ) -> Result<MctObservationLedgerEntry> {
        Self::create_passkey_genesis_at(path.as_ref(), setup, request, now())
    }

    pub(super) fn create_passkey_genesis_at(
        path: &Path,
        setup: &PasskeyGenesisSetupV1,
        request: PasskeyAdminRequestV1,
        time: Timestamp,
    ) -> Result<MctObservationLedgerEntry> {
        setup.validate_request(&request)?;
        let fact = PasskeyAdminFactV1 {
            request,
            decided_at: time,
        };
        transition(&empty_authority_replay(), &fact)?;
        let file = private_fs::create_new(path).map_err(|source| ObservationLedgerError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let mut writer = open_file_with_recovery_hook(
            path.to_path_buf(),
            &setup.ledger_id,
            &setup.mother_node_id,
            file,
            true,
            |_| Ok(()),
        )?;
        // Keep the same admission time: a second clock read could reject an
        // admitted request after file creation and strand an empty ledger again.
        writer.execute_passkey_genesis_at(setup, fact.request, fact.decided_at)
    }

    pub fn execute_passkey_genesis(
        &mut self,
        setup: &PasskeyGenesisSetupV1,
        request: PasskeyAdminRequestV1,
    ) -> Result<MctObservationLedgerEntry> {
        self.execute_passkey_genesis_at(setup, request, now())
    }
    pub(super) fn execute_passkey_genesis_at(
        &mut self,
        setup: &PasskeyGenesisSetupV1,
        request: PasskeyAdminRequestV1,
        time: Timestamp,
    ) -> Result<MctObservationLedgerEntry> {
        setup.validate_request(&request)?;
        if self.is_poisoned()
            || self.mother_node_id != setup.mother_node_id
            || self.ledger_id != setup.ledger_id
        {
            return Err(denied("genesis disagrees with trusted registration/setup"));
        }
        if let Some(prior) = &self.cached_replay()?.state.passkey_admin {
            if prior.genesis.request == request {
                return self
                    .entries()?
                    .into_iter()
                    .next()
                    .ok_or_else(|| denied("missing birth receipt"));
            }
            return Err(denied("conflicting passkey genesis"));
        }
        if !self.created_new
            || self.next_sequence != 0
            || self.authority_tenure.is_some()
            || self.recovery_status.is_some()
        {
            return Err(denied("nonvirgin ledger cannot acquire passkey provenance"));
        }
        self.commit_passkey_at(request, time)
    }
    /// Issues or revokes canonical leases. Commands require the scoped effect API.
    pub fn execute_passkey_admin(
        &mut self,
        request: PasskeyAdminRequestV1,
    ) -> Result<MctObservationLedgerEntry> {
        self.execute_passkey_admin_at(request, now())
    }
    pub(super) fn execute_passkey_admin_at(
        &mut self,
        request: PasskeyAdminRequestV1,
        time: Timestamp,
    ) -> Result<MctObservationLedgerEntry> {
        if !matches!(
            request.intent.action,
            PasskeyAdminActionV1::IssueDayKey { .. } | PasskeyAdminActionV1::RevokeDayKey { .. }
        ) {
            return Err(denied("wrong administrative API"));
        }
        self.commit_passkey_at(request, time)
    }
    fn commit_passkey_at(
        &mut self,
        request: PasskeyAdminRequestV1,
        time: Timestamp,
    ) -> Result<MctObservationLedgerEntry> {
        if self.is_poisoned()
            || request.intent.mother_node_id != self.mother_node_id
            || request.intent.ledger_id != self.ledger_id
        {
            return Err(denied("writer unavailable or wrong target"));
        }
        let fact = PasskeyAdminFactV1 {
            request,
            decided_at: time,
        };
        transition(self.cached_replay()?, &fact)?;
        fact.identity()?;
        self.append_canonical_before_effect(fact.observation()?, fact.decided_at.to_string())
    }
    /// Host adapter supplies its own route/scope, not labels copied from the wire.
    /// Writer ownership serializes revocation with admission/effect. Complete-frame
    /// acknowledgement loss consumes the nonce on recovery but never runs effect.
    pub fn execute_passkey_command<T>(
        &mut self,
        request: PasskeyAdminRequestV1,
        route: &str,
        scope: AdminKeyScopeV1,
        effect: impl FnOnce(&str) -> Result<T>,
    ) -> Result<T> {
        self.execute_passkey_command_with_clock(request, route, scope, now, effect)
    }
    pub(super) fn execute_passkey_command_with_clock<T>(
        &mut self,
        request: PasskeyAdminRequestV1,
        route: &str,
        scope: AdminKeyScopeV1,
        mut clock: impl FnMut() -> Timestamp,
        effect: impl FnOnce(&str) -> Result<T>,
    ) -> Result<T> {
        let PasskeyAdminActionV1::Command {
            scope: signed_scope,
            route: signed_route,
            body,
        } = &request.intent.action
        else {
            return Err(denied("not a scoped command"));
        };
        if *signed_scope != scope || signed_route != route {
            return Err(denied("route/scope substitution"));
        }
        let body = body.clone();
        self.commit_passkey_at(request.clone(), clock())?;
        let time = clock();
        request.intent.validate(&time).map_err(denied)?;
        let state = &self
            .cached_replay()?
            .state
            .passkey_admin
            .as_ref()
            .ok_or_else(|| denied("lost passkey provenance"))?
            .state;
        let parsed: jiff::Timestamp = time.as_str().parse().map_err(denied)?;
        let last: jiff::Timestamp = state.last_decided_at.as_str().parse().map_err(denied)?;
        if parsed < last {
            return Err(denied("clock rollback after admission"));
        }
        if let DayKeyAdminProofV1::Ed25519DayKeyV1 { grant_id, .. } = &request.proof
            && !state
                .day_keys
                .get(grant_id)
                .is_some_and(|key| key.authorizes(scope, &time))
        {
            return Err(denied("lease expired or revoked before effect"));
        }
        effect(&body)
    }
}
