//! Mother-local canonical mutation/effect-start ordering seam.
//!
//! Phase H3 intentionally gives this primitive no production consumer. Grants slices 7 and 8
//! can later adopt the same synchronous handoff at their final adapter-start seams.

use mct_kernel::LocalExecutionAuthoritySnapshot;
use mct_observation::{
    AuthorityProjectionCursorV1, AuthorityProjectionDenyReasonV1, AuthorityProjectionExpectationV1,
    AuthorityProjectionLedgerEvidenceV1, JsonlObservationLedger, UsableAuthorityProjectionProofV1,
    authority_state_hash, replay_authority_entries,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, marker::PhantomData, sync::Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotherAuthorityFenceReasonV1 {
    CommitUnknown,
    WriterPoisoned,
    ProjectionLag,
    RescanUnresolved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MotherAuthorityCommitOutcomeV1 {
    Committed {
        mutation_id: String,
        current_expectation: AuthorityProjectionExpectationV1,
    },
    CommittedProjectionPending {
        mutation_id: String,
    },
    CommitUnknown {
        mutation_id: String,
    },
    WriterPoisoned {
        mutation_id: String,
    },
    RejectedBeforeCommit {
        mutation_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum MotherAuthorityAdmissionDenyV1 {
    Fenced(MotherAuthorityFenceReasonV1),
    ExactAuthorityStateMismatch,
    AuthorityStateUnavailable,
    Projection(AuthorityProjectionDenyReasonV1),
    ProjectionExpectationMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "detail", rename_all = "snake_case")]
pub enum MotherAuthorityRecoveryDenyV1 {
    NotFenced,
    ExclusiveWriterUnavailable,
    FreshWriterTenureRequired,
    LedgerReplayBlocked,
    MutationResolutionUnproven,
    Projection(AuthorityProjectionDenyReasonV1),
    ProjectionExpectationMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MutationResolutionRequirementV1 {
    MustBeCommitted,
    PresentOrAbsentAfterFullRescan,
}

#[derive(Debug)]
struct MotherAuthorityOrderStateV1 {
    current_expectation: Option<AuthorityProjectionExpectationV1>,
    fenced: Option<MotherAuthorityFenceReasonV1>,
    pending_mutation_resolutions: BTreeMap<String, MutationResolutionRequirementV1>,
}

#[derive(Debug)]
pub struct MotherAuthorityOrderV1 {
    state: Mutex<MotherAuthorityOrderStateV1>,
}

/// Unforgeable, non-cloneable admission whose lifetime is tied to the held order position.
pub struct MotherAuthorityAdmissionV1<'order> {
    expectation: &'order AuthorityProjectionExpectationV1,
    _single_use: PhantomData<&'order mut ()>,
}

impl MotherAuthorityAdmissionV1<'_> {
    pub fn expectation(&self) -> &AuthorityProjectionExpectationV1 {
        self.expectation
    }
}

impl MotherAuthorityOrderV1 {
    pub fn new(initial_expectation: AuthorityProjectionExpectationV1) -> Self {
        Self {
            state: Mutex::new(MotherAuthorityOrderStateV1 {
                current_expectation: Some(initial_expectation),
                fenced: None,
                pending_mutation_resolutions: BTreeMap::new(),
            }),
        }
    }

    #[doc(hidden)]
    pub fn from_ledger(ledger: &JsonlObservationLedger) -> Self {
        authority_expectation_from_ledger(ledger).map_or_else(|_| Self::unavailable(), Self::new)
    }

    pub fn unavailable() -> Self {
        Self {
            state: Mutex::new(MotherAuthorityOrderStateV1 {
                current_expectation: None,
                fenced: Some(MotherAuthorityFenceReasonV1::RescanUnresolved),
                pending_mutation_resolutions: BTreeMap::new(),
            }),
        }
    }

    pub fn is_fenced(&self) -> bool {
        self.state
            .lock()
            .expect("Mother authority order mutex must not be poisoned")
            .fenced
            .is_some()
    }

    pub fn fence_reason(&self) -> Option<MotherAuthorityFenceReasonV1> {
        self.state
            .lock()
            .expect("Mother authority order mutex must not be poisoned")
            .fenced
    }

    pub fn commit_mutation<I>(
        &self,
        mutation_id: &str,
        intent: &I,
        commit_fn: impl FnOnce(&I) -> MotherAuthorityCommitOutcomeV1,
    ) -> MotherAuthorityCommitOutcomeV1 {
        let mut state = self
            .state
            .lock()
            .expect("Mother authority order mutex must not be poisoned");
        if state.fenced.is_some() || mutation_id.trim().is_empty() {
            return MotherAuthorityCommitOutcomeV1::RejectedBeforeCommit {
                mutation_id: mutation_id.to_owned(),
            };
        }
        let outcome = commit_fn(intent);
        let outcome_id_matches = match &outcome {
            MotherAuthorityCommitOutcomeV1::Committed {
                mutation_id: outcome_id,
                ..
            }
            | MotherAuthorityCommitOutcomeV1::CommittedProjectionPending {
                mutation_id: outcome_id,
            }
            | MotherAuthorityCommitOutcomeV1::CommitUnknown {
                mutation_id: outcome_id,
            }
            | MotherAuthorityCommitOutcomeV1::WriterPoisoned {
                mutation_id: outcome_id,
            }
            | MotherAuthorityCommitOutcomeV1::RejectedBeforeCommit {
                mutation_id: outcome_id,
            } => outcome_id == mutation_id,
        };
        match &outcome {
            MotherAuthorityCommitOutcomeV1::Committed {
                current_expectation,
                ..
            } if outcome_id_matches => {
                state.current_expectation = Some(current_expectation.clone());
            }
            MotherAuthorityCommitOutcomeV1::CommittedProjectionPending { .. }
                if outcome_id_matches =>
            {
                state.fenced = Some(MotherAuthorityFenceReasonV1::ProjectionLag);
                state.pending_mutation_resolutions.insert(
                    mutation_id.to_owned(),
                    MutationResolutionRequirementV1::MustBeCommitted,
                );
            }
            MotherAuthorityCommitOutcomeV1::CommitUnknown { .. } if outcome_id_matches => {
                state.fenced = Some(MotherAuthorityFenceReasonV1::CommitUnknown);
                state.pending_mutation_resolutions.insert(
                    mutation_id.to_owned(),
                    MutationResolutionRequirementV1::PresentOrAbsentAfterFullRescan,
                );
            }
            MotherAuthorityCommitOutcomeV1::WriterPoisoned { .. } if outcome_id_matches => {
                state.fenced = Some(MotherAuthorityFenceReasonV1::WriterPoisoned);
                state.pending_mutation_resolutions.insert(
                    mutation_id.to_owned(),
                    MutationResolutionRequirementV1::PresentOrAbsentAfterFullRescan,
                );
            }
            MotherAuthorityCommitOutcomeV1::RejectedBeforeCommit { .. } if outcome_id_matches => {}
            _ => {
                // A callback that reports a different mutation id cannot establish what happened
                // to the offered id. Preserve that offered id as uncertain and fail closed.
                state.fenced = Some(MotherAuthorityFenceReasonV1::CommitUnknown);
                state.pending_mutation_resolutions.insert(
                    mutation_id.to_owned(),
                    MutationResolutionRequirementV1::PresentOrAbsentAfterFullRescan,
                );
            }
        }
        outcome
    }

    pub fn clear_fence_after_exclusive_rescan(
        &self,
        ledger: &JsonlObservationLedger,
        state_store: &crate::MctRuntimeStateStore,
    ) -> Result<AuthorityProjectionExpectationV1, MotherAuthorityRecoveryDenyV1> {
        let mut state = self
            .state
            .lock()
            .expect("Mother authority order mutex must not be poisoned");
        if state.fenced.is_none() {
            return Err(MotherAuthorityRecoveryDenyV1::NotFenced);
        }
        let tenure = ledger
            .authority_tenure()
            .filter(|_| !ledger.is_poisoned())
            .ok_or(MotherAuthorityRecoveryDenyV1::ExclusiveWriterUnavailable)?;
        if state.current_expectation.as_ref().is_some_and(|previous| {
            previous.grants_authority.authority_epoch == tenure.fact.authority_epoch
        }) {
            return Err(MotherAuthorityRecoveryDenyV1::FreshWriterTenureRequired);
        }
        let entries = ledger
            .entries()
            .map_err(|_| MotherAuthorityRecoveryDenyV1::LedgerReplayBlocked)?;
        let replay = replay_authority_entries(&entries)
            .map_err(|_| MotherAuthorityRecoveryDenyV1::LedgerReplayBlocked)?;
        // The boundary, not its caller, retains every offered id whose outcome needs recovery.
        // A complete identity/sequence/hash-valid exclusive rescan resolves uncertainty to either
        // one replayed canonical fact or absence. Acknowledged projection-pending commitment must
        // specifically survive as a canonical fact.
        for (mutation_id, requirement) in &state.pending_mutation_resolutions {
            if *requirement == MutationResolutionRequirementV1::MustBeCommitted
                && !replay.mutations.contains_key(mutation_id)
            {
                return Err(MotherAuthorityRecoveryDenyV1::MutationResolutionUnproven);
            }
        }
        let head = entries
            .last()
            .ok_or(MotherAuthorityRecoveryDenyV1::LedgerReplayBlocked)?;
        let authority = replay
            .current_authority
            .ok_or(MotherAuthorityRecoveryDenyV1::LedgerReplayBlocked)?;
        if authority != tenure.fact.resulting_authority {
            return Err(MotherAuthorityRecoveryDenyV1::LedgerReplayBlocked);
        }
        let expectation = AuthorityProjectionExpectationV1 {
            source_mother_node_id: head.mother_node_id.clone(),
            source_ledger_id: head.ledger_id.clone(),
            through_sequence: head.local_sequence,
            through_entry_hash: head.entry_hash.clone(),
            grants_authority: authority,
            authority_state_hash: authority_state_hash(&replay.state)
                .map_err(|_| MotherAuthorityRecoveryDenyV1::LedgerReplayBlocked)?,
        };
        let proof = state_store
            .usable_authority_projection_proof(&AuthorityProjectionLedgerEvidenceV1::Validated(
                expectation.clone(),
            ))
            .map_err(|_| {
                MotherAuthorityRecoveryDenyV1::Projection(
                    AuthorityProjectionDenyReasonV1::ProjectionNotCurrent,
                )
            })?;
        let cursor = match proof {
            UsableAuthorityProjectionProofV1::Usable { cursor } => cursor,
            UsableAuthorityProjectionProofV1::Denied { reason } => {
                return Err(MotherAuthorityRecoveryDenyV1::Projection(reason));
            }
        };
        if !cursor_matches_expectation(&cursor, &expectation) {
            return Err(MotherAuthorityRecoveryDenyV1::ProjectionExpectationMismatch);
        }
        state.current_expectation = Some(expectation.clone());
        state.fenced = None;
        state.pending_mutation_resolutions.clear();
        Ok(expectation)
    }

    pub fn admit_effect<R>(
        &self,
        expectation: &AuthorityProjectionExpectationV1,
        proof_fn: impl FnOnce() -> UsableAuthorityProjectionProofV1,
        start_fn: impl for<'order> FnOnce(MotherAuthorityAdmissionV1<'order>) -> R,
    ) -> Result<R, MotherAuthorityAdmissionDenyV1> {
        let state = self
            .state
            .lock()
            .expect("Mother authority order mutex must not be poisoned");
        if let Some(reason) = state.fenced {
            return Err(MotherAuthorityAdmissionDenyV1::Fenced(reason));
        }
        let current = state
            .current_expectation
            .as_ref()
            .ok_or(MotherAuthorityAdmissionDenyV1::AuthorityStateUnavailable)?;
        if !same_authority_state(current, expectation) {
            return Err(MotherAuthorityAdmissionDenyV1::ExactAuthorityStateMismatch);
        }
        let proof = proof_fn();
        let cursor = match &proof {
            UsableAuthorityProjectionProofV1::Usable { cursor } => cursor.as_ref(),
            UsableAuthorityProjectionProofV1::Denied { reason } => {
                return Err(MotherAuthorityAdmissionDenyV1::Projection(*reason));
            }
        };
        if !cursor_matches_expectation(cursor, expectation) {
            return Err(MotherAuthorityAdmissionDenyV1::ProjectionExpectationMismatch);
        }
        let admission = MotherAuthorityAdmissionV1 {
            expectation,
            _single_use: PhantomData,
        };
        // `state` remains held through closure entry and completion. The lifetime-polymorphic,
        // non-cloneable admission cannot become a refreshable bearer for a later start.
        Ok(start_fn(admission))
    }
}

#[doc(hidden)]
pub fn authority_expectation_from_snapshot(
    snapshot: &LocalExecutionAuthoritySnapshot,
) -> AuthorityProjectionExpectationV1 {
    let grants = snapshot.canonical_grants().grants_authority();
    let projection = snapshot.projection();
    AuthorityProjectionExpectationV1 {
        source_mother_node_id: projection.source_mother_node_id().to_owned(),
        source_ledger_id: projection.source_ledger_id().to_owned(),
        through_sequence: projection.through_sequence(),
        through_entry_hash: projection.through_entry_hash().to_owned(),
        grants_authority: mct_observation::GrantsAuthorityIdentityV1 {
            mother_node_id: grants.mother_node_id().to_owned(),
            authority_epoch: grants.authority_epoch().to_owned(),
            generation: grants.generation(),
            source_authority_observation_id: grants.source_authority_observation_id().to_owned(),
        },
        authority_state_hash: projection.authority_state_hash().to_owned(),
    }
}

#[doc(hidden)]
pub fn authority_expectation_from_ledger(
    ledger: &JsonlObservationLedger,
) -> Result<AuthorityProjectionExpectationV1, MotherAuthorityRecoveryDenyV1> {
    let entries = ledger
        .entries()
        .map_err(|_| MotherAuthorityRecoveryDenyV1::LedgerReplayBlocked)?;
    let head = entries
        .last()
        .ok_or(MotherAuthorityRecoveryDenyV1::LedgerReplayBlocked)?;
    let replay = replay_authority_entries(&entries)
        .map_err(|_| MotherAuthorityRecoveryDenyV1::LedgerReplayBlocked)?;
    let grants_authority = replay
        .current_authority
        .ok_or(MotherAuthorityRecoveryDenyV1::LedgerReplayBlocked)?;
    Ok(AuthorityProjectionExpectationV1 {
        source_mother_node_id: head.mother_node_id.clone(),
        source_ledger_id: head.ledger_id.clone(),
        through_sequence: head.local_sequence,
        through_entry_hash: head.entry_hash.clone(),
        grants_authority,
        authority_state_hash: authority_state_hash(&replay.state)
            .map_err(|_| MotherAuthorityRecoveryDenyV1::LedgerReplayBlocked)?,
    })
}

fn same_authority_state(
    left: &AuthorityProjectionExpectationV1,
    right: &AuthorityProjectionExpectationV1,
) -> bool {
    left.source_mother_node_id == right.source_mother_node_id
        && left.source_ledger_id == right.source_ledger_id
        && left.grants_authority == right.grants_authority
        && left.authority_state_hash == right.authority_state_hash
}

fn cursor_matches_expectation(
    cursor: &AuthorityProjectionCursorV1,
    expectation: &AuthorityProjectionExpectationV1,
) -> bool {
    cursor.source_mother_node_id == expectation.source_mother_node_id
        && cursor.source_ledger_id == expectation.source_ledger_id
        && cursor.through_sequence == expectation.through_sequence
        && cursor.through_entry_hash == expectation.through_entry_hash
        && cursor.grants_authority == expectation.grants_authority
        && cursor.authority_state_hash == expectation.authority_state_hash
        && cursor.projection_status == mct_observation::AuthorityProjectionStatusV1::Current
}

#[cfg(test)]
mod tests {
    use super::*;
    use mct_kernel::ToyContractIdentity;
    use mct_observation::{
        AuthorityChangeV1, AuthorityMutationRequestV1, AuthorityMutationResultV1,
        AuthorityProjectionHashInputV1, AuthorityProjectionStatusV1, GrantShapingCommandKindV1,
        GrantShapingSourceV1, GrantsAuthorityIdentityV1, authority_projection_hash,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    };

    fn expectation(generation: u64) -> AuthorityProjectionExpectationV1 {
        AuthorityProjectionExpectationV1 {
            source_mother_node_id: "mother-a".into(),
            source_ledger_id: "ledger-a".into(),
            through_sequence: generation,
            through_entry_hash: format!("head-{generation}"),
            grants_authority: GrantsAuthorityIdentityV1 {
                mother_node_id: "mother-a".into(),
                authority_epoch: "epoch-a".into(),
                generation,
                source_authority_observation_id: format!("obs-authority-{generation}"),
            },
            authority_state_hash: format!("state-{generation}"),
        }
    }

    fn mutation_request(mutation_id: &str) -> AuthorityMutationRequestV1 {
        AuthorityMutationRequestV1 {
            mutation_id: mutation_id.into(),
            changes: vec![AuthorityChangeV1::ToyCatalogPut {
                toy_id: "toy-ordering".into(),
                contract: ToyContractIdentity {
                    namespace: "mct:test".into(),
                    interface_name: "ordering".into(),
                    version: "1.0.0".into(),
                    function_name: Some("run".into()),
                    resource_name: None,
                },
                authority_bearing: true,
                catalog_revision: 1,
                admitted_by_observation_id: "obs:toy-ordering".into(),
            }],
            grant_shaping_sources: vec![GrantShapingSourceV1::OperatorDecision {
                decision_id: format!("decision:{mutation_id}"),
                authenticated_principal_ref: "os-uid:501".into(),
                command_kind: GrantShapingCommandKindV1::CatalogChange,
            }],
            decided_at: "2026-08-04T00:00:00Z".into(),
        }
    }

    fn usable(expectation: &AuthorityProjectionExpectationV1) -> UsableAuthorityProjectionProofV1 {
        let mut cursor = AuthorityProjectionCursorV1 {
            schema_version: 1,
            projection_id: "authority-state-v1".into(),
            projection_kind: "authority_state".into(),
            source_mother_node_id: expectation.source_mother_node_id.clone(),
            source_ledger_id: expectation.source_ledger_id.clone(),
            through_sequence: expectation.through_sequence,
            through_observation_id: "obs-head".into(),
            through_entry_hash: expectation.through_entry_hash.clone(),
            grants_authority: expectation.grants_authority.clone(),
            authority_state_hash: expectation.authority_state_hash.clone(),
            projection_hash: String::new(),
            projection_status: AuthorityProjectionStatusV1::Current,
            updated_at: "2026-08-04T00:00:00Z".into(),
        };
        cursor.projection_hash = authority_projection_hash(&AuthorityProjectionHashInputV1 {
            source_mother_node_id: cursor.source_mother_node_id.clone(),
            source_ledger_id: cursor.source_ledger_id.clone(),
            through_sequence: cursor.through_sequence,
            through_observation_id: cursor.through_observation_id.clone(),
            through_entry_hash: cursor.through_entry_hash.clone(),
            grants_authority: cursor.grants_authority.clone(),
            authority_state_hash: cursor.authority_state_hash.clone(),
            projection_status: cursor.projection_status,
        })
        .unwrap();
        UsableAuthorityProjectionProofV1::Usable {
            cursor: Box::new(cursor),
        }
    }

    #[test]
    fn uncertainty_and_projection_lag_fence_until_exclusive_reopen_and_exact_proof() {
        let dir = tempfile::tempdir().unwrap();
        let ledger_path = dir.path().join("observations.jsonl");
        let state_path = dir.path().join("state.sqlite");
        let mut ledger = mct_observation::JsonlObservationLedger::open_authority(
            &ledger_path,
            "ledger-local",
            "local-mct",
        )
        .unwrap();
        let entries = ledger.entries().unwrap();
        let replay = mct_observation::replay_authority_entries(&entries).unwrap();
        let head = entries.last().unwrap();
        let initial = AuthorityProjectionExpectationV1 {
            source_mother_node_id: head.mother_node_id.clone(),
            source_ledger_id: head.ledger_id.clone(),
            through_sequence: head.local_sequence,
            through_entry_hash: head.entry_hash.clone(),
            grants_authority: replay.current_authority.unwrap(),
            authority_state_hash: mct_observation::authority_state_hash(&replay.state).unwrap(),
        };
        let state = crate::MctRuntimeStateStore::open(&state_path).unwrap();
        state.rebuild_authority_projection(&entries).unwrap();
        let order = MotherAuthorityOrderV1::new(initial.clone());
        order.commit_mutation("pending-1", &(), |_| {
            let committed = ledger
                .execute_authority_mutation(mutation_request("pending-1"), |_| {
                    Err("injected projection failure".into())
                });
            assert!(matches!(
                committed,
                AuthorityMutationResultV1::CommittedProjectionPending { .. }
            ));
            MotherAuthorityCommitOutcomeV1::CommittedProjectionPending {
                mutation_id: "pending-1".into(),
            }
        });
        let starts = AtomicUsize::new(0);
        for _ in 0..3 {
            assert_eq!(
                order.admit_effect(
                    &initial,
                    || usable(&initial),
                    |_| { starts.fetch_add(1, Ordering::SeqCst) }
                ),
                Err(MotherAuthorityAdmissionDenyV1::Fenced(
                    MotherAuthorityFenceReasonV1::ProjectionLag
                ))
            );
        }
        assert_eq!(starts.load(Ordering::SeqCst), 0);
        assert_eq!(
            order.clear_fence_after_exclusive_rescan(&ledger, &state),
            Err(MotherAuthorityRecoveryDenyV1::FreshWriterTenureRequired)
        );
        drop(ledger);

        let reopened = mct_observation::JsonlObservationLedger::open_authority(
            &ledger_path,
            "ledger-local",
            "local-mct",
        )
        .unwrap();
        let reopened_entries = reopened.entries().unwrap();
        state
            .rebuild_authority_projection(&reopened_entries)
            .unwrap();
        let recovered = order
            .clear_fence_after_exclusive_rescan(&reopened, &state)
            .unwrap();
        assert!(!order.is_fenced());
        assert_eq!(
            order.admit_effect(
                &recovered,
                || {
                    state
                        .usable_authority_projection_proof(
                            &AuthorityProjectionLedgerEvidenceV1::Validated(recovered.clone()),
                        )
                        .unwrap()
                },
                |_| starts.fetch_add(1, Ordering::SeqCst)
            ),
            Ok(0)
        );
        assert_eq!(starts.load(Ordering::SeqCst), 1);

        for (outcome, reason) in [
            (
                MotherAuthorityCommitOutcomeV1::CommitUnknown {
                    mutation_id: "unknown".into(),
                },
                MotherAuthorityFenceReasonV1::CommitUnknown,
            ),
            (
                MotherAuthorityCommitOutcomeV1::WriterPoisoned {
                    mutation_id: "poisoned".into(),
                },
                MotherAuthorityFenceReasonV1::WriterPoisoned,
            ),
        ] {
            let boundary = MotherAuthorityOrderV1::new(initial.clone());
            let id = match &outcome {
                MotherAuthorityCommitOutcomeV1::CommitUnknown { mutation_id }
                | MotherAuthorityCommitOutcomeV1::WriterPoisoned { mutation_id } => {
                    mutation_id.clone()
                }
                _ => unreachable!(),
            };
            boundary.commit_mutation(&id, &(), |_| outcome);
            assert_eq!(boundary.fence_reason(), Some(reason));
            for _ in 0..2 {
                assert_eq!(
                    boundary.admit_effect(
                        &initial,
                        || usable(&initial),
                        |_| { starts.fetch_add(1, Ordering::SeqCst) }
                    ),
                    Err(MotherAuthorityAdmissionDenyV1::Fenced(reason))
                );
            }
        }
        assert_eq!(
            starts.load(Ordering::SeqCst),
            1,
            "uncertainty and poisoned-writer retries must start nothing"
        );
    }

    #[test]
    fn revocation_first_denies_while_effect_start_first_runs_exactly_once() {
        let before = expectation(0);
        let after = expectation(1);
        let revocation_first = MotherAuthorityOrderV1::new(before.clone());
        let committed = revocation_first.commit_mutation("revoke-1", &(), |_| {
            MotherAuthorityCommitOutcomeV1::Committed {
                mutation_id: "revoke-1".into(),
                current_expectation: after.clone(),
            }
        });
        assert!(matches!(
            committed,
            MotherAuthorityCommitOutcomeV1::Committed { .. }
        ));
        let starts = AtomicUsize::new(0);
        let denied = revocation_first.admit_effect(
            &before,
            || usable(&before),
            |_| starts.fetch_add(1, Ordering::SeqCst),
        );
        assert_eq!(
            denied,
            Err(MotherAuthorityAdmissionDenyV1::ExactAuthorityStateMismatch)
        );
        assert_eq!(
            starts.load(Ordering::SeqCst),
            0,
            "revocation-first must start no effect"
        );

        let effect_first = Arc::new(MotherAuthorityOrderV1::new(before.clone()));
        let starts = Arc::new(AtomicUsize::new(0));
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let effect_order = Arc::clone(&effect_first);
        let effect_starts = Arc::clone(&starts);
        let effect_expectation = before.clone();
        let effect = std::thread::spawn(move || {
            effect_order
                .admit_effect(
                    &effect_expectation,
                    || usable(&effect_expectation),
                    |admission| {
                        assert_eq!(admission.expectation(), &effect_expectation);
                        effect_starts.fetch_add(1, Ordering::SeqCst);
                        started_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                    },
                )
                .unwrap();
        });
        started_rx.recv().unwrap();
        let mutation_order = Arc::clone(&effect_first);
        let committed_after = after.clone();
        let mutation = std::thread::spawn(move || {
            mutation_order.commit_mutation("revoke-2", &(), |_| {
                MotherAuthorityCommitOutcomeV1::Committed {
                    mutation_id: "revoke-2".into(),
                    current_expectation: committed_after,
                }
            })
        });
        assert_eq!(starts.load(Ordering::SeqCst), 1);
        release_tx.send(()).unwrap();
        effect.join().unwrap();
        assert!(matches!(
            mutation.join().unwrap(),
            MotherAuthorityCommitOutcomeV1::Committed { .. }
        ));
        assert_eq!(
            starts.load(Ordering::SeqCst),
            1,
            "effect-start-first must start exactly once and is not retroactively undone"
        );
    }

    /// Phase K proof 10: Phase J replacements remain and the slice-6 echo pin is retired.
    #[test]
    fn phase_j_pin_retirement_maps_each_old_seam_to_proof_or_named_residue() {
        let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let read = |relative: &str| {
            std::fs::read_to_string(crate_root.join(relative))
                .unwrap_or_else(|error| panic!("read {relative}: {error}"))
        };
        let call_protocol = read("../mct-kernel/src/call/internal.rs");
        assert!(call_protocol.contains(
            "request.call.authority_context.policy_revision < request.authority.policy_revision"
        ));
        assert!(call_protocol.contains("ExpectedReceiverAuthorityStale"));
        assert!(call_protocol.contains("ReceiverAuthorityUnavailable"));
        assert!(call_protocol.contains("expected_receiver_grants_authority"));
        assert!(call_protocol.contains("request.authority.expected_receiver_grants_authority"));
        assert!(!call_protocol.contains("request.authority.grants_revision"));
        assert!(!call_protocol.contains("< request.authority.expected_receiver_grants_authority"));

        let resident_effect = read("src/daemon/resident/execution.rs");
        assert!(resident_effect.contains("admit_effect_with_snapshot(&call, &effect_snapshot)"));
        assert!(!resident_effect.contains("current_resident_route_revisions"));
        let child_token = read("../mct-kernel/src/child.rs");
        assert!(child_token.contains(
            "authority.grants_authority() != snapshot.canonical_grants().grants_authority()"
        ));
        let toy_token = read("../mct-kernel/src/toy.rs");
        assert!(toy_token.contains("admit_effect_with_snapshot"));
        assert!(toy_token.contains("ConsumptionStateUnavailable"));
        let toy_adapter = read("src/toy.rs");
        assert!(toy_adapter.contains("authority.admit_order(&snapshot, ||"));
        let wasm_adapter = read("src/wasm.rs");
        assert!(wasm_adapter.contains("install_wasi_preopen(&mut builder"));
        assert!(read("src/process.rs").contains("authorized.admit_effect_for_call(call)"));
        assert!(wasm_adapter.contains("authorized.admit_effect_for_call(call)"));

        let peer_wire = read("../mct-kernel/src/peer/mod.rs");
        assert!(peer_wire.contains("pub struct MctHelloRequest"));
        assert!(peer_wire.contains("receiving_grants_authority"));
        assert!(!peer_wire.contains("LocalExecutionAuthoritySnapshot"));
        let cli_runtime = read("src/daemon/cli_runtime.rs");
        assert!(!cli_runtime.contains("evaluate_toy_grant_for_call"));
        let replay = read("src/daemon/resident/idempotency.rs");
        assert!(replay.contains("MctIdempotencyReason::ReplayCompleted"));
        assert!(!replay.contains("LocalExecutionAuthoritySnapshot"));

        let resident_writer = read("src/daemon/resident/observation.rs");
        assert!(resident_writer.contains("authority_order: Arc<MotherAuthorityOrderV1>"));
        assert!(resident_writer.contains("task_authority_order.commit_mutation"));
        assert!(resident_writer.contains("self.authority_order.admit_effect"));
        assert!(resident_writer.contains("admit_effect_start"));
        let offline_control = read("src/daemon/control.rs");
        assert!(offline_control.contains("authority_order.commit_mutation"));
        assert!(resident_effect.contains("ledger.admit_effect(&effect_snapshot"));
    }
}
