//! Mother-local canonical mutation/effect-start ordering seam.
//!
//! Phase H3 intentionally gives this primitive no production consumer. Grants slices 7 and 8
//! can later adopt the same synchronous handoff at their final adapter-start seams.

use mct_observation::{
    AuthorityProjectionCursorV1, AuthorityProjectionDenyReasonV1, AuthorityProjectionExpectationV1,
    UsableAuthorityProjectionProofV1,
};
use serde::{Deserialize, Serialize};
use std::{marker::PhantomData, sync::Mutex};

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

#[derive(Debug)]
struct MotherAuthorityOrderStateV1 {
    current_expectation: Option<AuthorityProjectionExpectationV1>,
    fenced: Option<MotherAuthorityFenceReasonV1>,
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
            }),
        }
    }

    pub fn unavailable() -> Self {
        Self {
            state: Mutex::new(MotherAuthorityOrderStateV1 {
                current_expectation: None,
                fenced: Some(MotherAuthorityFenceReasonV1::RescanUnresolved),
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
        match &outcome {
            MotherAuthorityCommitOutcomeV1::Committed {
                mutation_id: committed_id,
                current_expectation,
            } if committed_id == mutation_id => {
                state.current_expectation = Some(current_expectation.clone());
            }
            MotherAuthorityCommitOutcomeV1::CommittedProjectionPending { .. } => {
                state.fenced = Some(MotherAuthorityFenceReasonV1::ProjectionLag);
            }
            MotherAuthorityCommitOutcomeV1::CommitUnknown { .. } => {
                state.fenced = Some(MotherAuthorityFenceReasonV1::CommitUnknown);
            }
            MotherAuthorityCommitOutcomeV1::WriterPoisoned { .. } => {
                state.fenced = Some(MotherAuthorityFenceReasonV1::WriterPoisoned);
            }
            MotherAuthorityCommitOutcomeV1::RejectedBeforeCommit { .. } => {}
            MotherAuthorityCommitOutcomeV1::Committed { .. } => {
                state.fenced = Some(MotherAuthorityFenceReasonV1::CommitUnknown);
            }
        }
        outcome
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
        if current != expectation {
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
    use mct_observation::{
        AuthorityProjectionHashInputV1, AuthorityProjectionStatusV1, GrantsAuthorityIdentityV1,
        authority_projection_hash,
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
}
