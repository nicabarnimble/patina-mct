//! Ledger-backed retry fencing. A reservation means dispatch may have begun,
//! not that a Child was authorized or an external effect completed.

use anyhow::{Result, bail, ensure};
use mct_kernel::{
    MctIdempotencyFingerprint, MctObservation, ObservationId, ObservationKind, ObservationOutcome,
    ObservationVisibility, SourcePlane, Timestamp,
};
use mct_observation::{DurabilityClass, MctObservationLedgerEntry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const PREFIX: &str = "mct-call-dispatch-v1:";

/// Digested authenticated caller/key and semantic request, with Mother time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallDispatchClaimV1 {
    pub key_digest: String,
    pub fingerprint_digest: String,
    pub accepted_at: Timestamp,
    pub expires_at: Timestamp,
}

impl CallDispatchClaimV1 {
    pub fn new(
        caller_scope: &str,
        key: &str,
        fingerprint: &MctIdempotencyFingerprint,
        accepted_at: Timestamp,
        expires_at: Timestamp,
    ) -> Result<Self> {
        let digest = |values: &[&str]| -> Result<String> {
            Ok(blake3::hash(&serde_json::to_vec(values)?)
                .to_hex()
                .to_string())
        };
        let claim = Self {
            key_digest: digest(&["mct-dispatch-key/v1", caller_scope, key])?,
            fingerprint_digest: digest(&[
                "mct-dispatch-fingerprint/v1",
                &fingerprint.target,
                fingerprint.call_id.as_str(),
                &fingerprint.payload_digest,
            ])?,
            accepted_at,
            expires_at,
        };
        claim.validate()?;
        Ok(claim)
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            valid_digest(&self.key_digest) && valid_digest(&self.fingerprint_digest),
            "invalid dispatch digest"
        );
        let accepted = self.accepted_at.as_str().parse::<jiff::Timestamp>()?;
        let expires = self.expires_at.as_str().parse::<jiff::Timestamp>()?;
        ensure!(
            expires
                == accepted.checked_add(jiff::SignedDuration::from_secs(
                    crate::MCT_IDEMPOTENCY_TTL_SECONDS
                ))?,
            "invalid dispatch lifetime"
        );
        Ok(())
    }
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CallDispatchReservationV1 {
    pub claim: CallDispatchClaimV1,
    /// Sequence of the reservation, never a caller-supplied identity.
    pub attempt_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallDispatchCheckV1 {
    Fresh,
    Existing,
    FingerprintMismatch,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum DispatchEvent {
    Reserved {
        reservation: CallDispatchReservationV1,
    },
    Released {
        key_digest: String,
        attempt_sequence: u64,
    },
}

/// Disposable index rebuilt only from a validated canonical ledger.
#[derive(Default)]
pub struct CallDispatchIndexV1 {
    reservations: BTreeMap<String, CallDispatchReservationV1>,
}

impl CallDispatchIndexV1 {
    pub fn check(&mut self, claim: &CallDispatchClaimV1) -> Result<CallDispatchCheckV1> {
        claim.validate()?;
        let now = claim.accepted_at.as_str().parse::<jiff::Timestamp>()?;
        self.reservations.retain(|_, record| {
            record
                .claim
                .expires_at
                .as_str()
                .parse::<jiff::Timestamp>()
                .is_ok_and(|expiry| expiry > now)
        });
        Ok(match self.reservations.get(&claim.key_digest) {
            None => CallDispatchCheckV1::Fresh,
            Some(record) if record.claim.fingerprint_digest == claim.fingerprint_digest => {
                CallDispatchCheckV1::Existing
            }
            Some(_) => CallDispatchCheckV1::FingerprintMismatch,
        })
    }

    pub fn contains(&self, reservation: &CallDispatchReservationV1) -> bool {
        self.reservations.get(&reservation.claim.key_digest) == Some(reservation)
    }

    /// The entry's framing, sequence, identity and hash must already be verified.
    /// Unknown versions and incoherent events refuse startup, not just replay.
    pub fn replay_entry(&mut self, entry: &MctObservationLedgerEntry) -> Result<()> {
        let Some(event) = decode_event(entry)? else {
            return Ok(());
        };
        match event {
            DispatchEvent::Reserved { reservation } => {
                if let Some(prior) = self.reservations.get(&reservation.claim.key_digest) {
                    ensure!(
                        prior.claim.expires_at.as_str().parse::<jiff::Timestamp>()?
                            <= reservation
                                .claim
                                .accepted_at
                                .as_str()
                                .parse::<jiff::Timestamp>()?,
                        "overlapping dispatch reservations"
                    );
                }
                self.reservations
                    .insert(reservation.claim.key_digest.clone(), reservation);
            }
            DispatchEvent::Released {
                key_digest,
                attempt_sequence,
            } => {
                ensure!(
                    self.reservations
                        .get(&key_digest)
                        .is_some_and(|record| record.attempt_sequence == attempt_sequence),
                    "dispatch release does not name the current attempt"
                );
                self.reservations.remove(&key_digest);
            }
        }
        Ok(())
    }
}

fn decode_event(entry: &MctObservationLedgerEntry) -> Result<Option<DispatchEvent>> {
    let Some(detail) = entry.observation.detail_ref.as_deref() else {
        return Ok(None);
    };
    if !detail.starts_with("mct-call-dispatch-") {
        return Ok(None);
    }
    let Some(json) = detail.strip_prefix(PREFIX) else {
        bail!("unknown dispatch schema");
    };
    ensure!(json.len() <= 4096, "dispatch fact too large");
    // Do not echo untrusted strings from serde errors into diagnostics.
    let event: DispatchEvent =
        serde_json::from_str(json).map_err(|_| anyhow::anyhow!("malformed dispatch fact"))?;
    let obs = &entry.observation;
    ensure!(
        entry.durability_class == DurabilityClass::BeforeEffect
            && obs.source_plane == SourcePlane::Kernel
            && obs.visibility == ObservationVisibility::InternalOnly
            && obs.call_id.is_some(),
        "invalid dispatch observation envelope"
    );
    let action = match &event {
        DispatchEvent::Reserved { reservation } => {
            reservation.claim.validate()?;
            ensure!(
                reservation.attempt_sequence == entry.local_sequence
                    && obs.observed_at == reservation.claim.accepted_at
                    && obs.kind == ObservationKind::CallConstructed
                    && obs.outcome == ObservationOutcome::Started,
                "invalid dispatch reservation envelope"
            );
            "reserved"
        }
        DispatchEvent::Released {
            key_digest,
            attempt_sequence,
        } => {
            ensure!(
                valid_digest(key_digest)
                    && *attempt_sequence > 0
                    && *attempt_sequence < entry.local_sequence
                    && obs.kind == ObservationKind::CallDenied
                    && obs.outcome == ObservationOutcome::Denied,
                "invalid dispatch release envelope"
            );
            "released"
        }
    };
    ensure!(
        entry.local_sequence > 0
            && obs.observation_id.as_str()
                == format!("obs:dispatch-{action}:{}", entry.local_sequence),
        "invalid dispatch observation identity"
    );
    Ok(Some(event))
}

pub fn dispatch_reservation_observation(
    mut template: MctObservation,
    claim: CallDispatchClaimV1,
    sequence: u64,
) -> Result<(MctObservation, CallDispatchReservationV1)> {
    claim.validate()?;
    ensure!(
        template.call_id.is_some() && sequence > 0,
        "invalid dispatch reservation template"
    );
    let reservation = CallDispatchReservationV1 {
        claim,
        attempt_sequence: sequence,
    };
    template.observed_at = reservation.claim.accepted_at.clone();
    template.observation_id = ObservationId::new(format!("obs:dispatch-reserved:{sequence}"))?;
    template.kind = ObservationKind::CallConstructed;
    template.outcome = ObservationOutcome::Started;
    template.source_plane = SourcePlane::Kernel;
    template.visibility = ObservationVisibility::InternalOnly;
    template.safe_message = "keyed dispatch reserved; execution may follow".into();
    template.detail_ref = Some(format!(
        "{PREFIX}{}",
        serde_json::to_string(&DispatchEvent::Reserved {
            reservation: reservation.clone()
        })?
    ));
    Ok((template, reservation))
}

pub fn dispatch_release_observation(
    mut template: MctObservation,
    reservation: &CallDispatchReservationV1,
    sequence: u64,
) -> Result<MctObservation> {
    ensure!(
        template.call_id.is_some()
            && reservation.attempt_sequence > 0
            && reservation.attempt_sequence < sequence,
        "invalid dispatch release template"
    );
    template.observation_id = ObservationId::new(format!("obs:dispatch-released:{sequence}"))?;
    template.kind = ObservationKind::CallDenied;
    template.outcome = ObservationOutcome::Denied;
    template.source_plane = SourcePlane::Kernel;
    template.visibility = ObservationVisibility::InternalOnly;
    template.safe_message = "keyed dispatch released before effect".into();
    template.detail_ref = Some(format!(
        "{PREFIX}{}",
        serde_json::to_string(&DispatchEvent::Released {
            key_digest: reservation.claim.key_digest.clone(),
            attempt_sequence: reservation.attempt_sequence,
        })?
    ));
    Ok(template)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mct_kernel::{CallId, ObservationTraceRef, TraceId};
    use mct_observation::{ExportStatus, JsonlObservationLedger};

    fn claim(scope: &str, key: &str, call: &str, seconds: i64) -> CallDispatchClaimV1 {
        let now = "2026-10-19T00:00:00Z"
            .parse::<jiff::Timestamp>()
            .unwrap()
            .checked_add(jiff::SignedDuration::from_secs(seconds))
            .unwrap();
        CallDispatchClaimV1::new(
            scope,
            key,
            &MctIdempotencyFingerprint {
                target: "example:worker/run@1.0.0#run".into(),
                call_id: CallId::new(call).unwrap(),
                payload_digest: "empty".into(),
            },
            Timestamp::new(now.to_string()).unwrap(),
            Timestamp::new(
                now.checked_add(jiff::SignedDuration::from_secs(
                    crate::MCT_IDEMPOTENCY_TTL_SECONDS,
                ))
                .unwrap()
                .to_string(),
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn template() -> MctObservation {
        MctObservation {
            observation_id: ObservationId::new("template").unwrap(),
            observed_at: Timestamp::new("2026-10-19T00:00:00Z").unwrap(),
            kind: ObservationKind::CallConstructed,
            source_plane: SourcePlane::Kernel,
            trace: ObservationTraceRef {
                trace_id: TraceId::new("trace-dispatch").unwrap(),
                span_id: None,
                parent_span_id: None,
                external_trace_id: None,
            },
            call_id: Some(CallId::new("call-a").unwrap()),
            decision_id: None,
            subject_id: None,
            resource_id: None,
            policy_revision: Some(1),
            grants_revision: Some(1),
            outcome: ObservationOutcome::Started,
            visibility: ObservationVisibility::InternalOnly,
            safe_message: String::new(),
            detail_ref: None,
        }
    }

    fn entry(observation: MctObservation, sequence: u64) -> MctObservationLedgerEntry {
        MctObservationLedgerEntry {
            ledger_id: "test".into(),
            mother_node_id: "test".into(),
            local_sequence: sequence,
            observation,
            previous_entry_hash: None,
            entry_hash: String::new(),
            appended_at: "2026-10-19T00:00:00Z".into(),
            durability_class: DurabilityClass::BeforeEffect,
            export_status: ExportStatus::NotRequired,
        }
    }

    #[test]
    fn call_dispatch_fuzz_seed_reaches_the_semantic_parser() {
        let entry: MctObservationLedgerEntry = serde_json::from_slice(include_bytes!(
            "../../../fuzz/corpus/call_dispatch/reserved.json"
        ))
        .unwrap();
        assert!(matches!(
            decode_event(&entry).unwrap(),
            Some(DispatchEvent::Reserved { .. })
        ));
        let mut index = CallDispatchIndexV1::default();
        index.replay_entry(&entry).unwrap();
        let release: MctObservationLedgerEntry = serde_json::from_slice(include_bytes!(
            "../../../fuzz/corpus/call_dispatch/released.json"
        ))
        .unwrap();
        assert!(matches!(
            decode_event(&release).unwrap(),
            Some(DispatchEvent::Released { .. })
        ));
        index.replay_entry(&release).unwrap();
    }

    #[test]
    fn call_dispatch_scope_fingerprint_expiry_and_exact_release() {
        let original = claim("caller-a", "private-key", "call-a", 0);
        let (obs, reservation) =
            dispatch_reservation_observation(template(), original.clone(), 1).unwrap();
        let first = entry(obs, 1);
        let mut index = CallDispatchIndexV1::default();
        index.replay_entry(&first).unwrap();
        assert_eq!(
            index.check(&original).unwrap(),
            CallDispatchCheckV1::Existing
        );
        assert_eq!(
            index
                .check(&claim("caller-b", "private-key", "call-a", 0))
                .unwrap(),
            CallDispatchCheckV1::Fresh
        );
        assert_eq!(
            index
                .check(&claim("caller-a", "private-key", "other-call", 0))
                .unwrap(),
            CallDispatchCheckV1::FingerprintMismatch
        );
        assert!(index.replay_entry(&first).is_err());
        let mut wrong = reservation.clone();
        wrong.attempt_sequence = 2;
        let wrong_release = entry(
            dispatch_release_observation(template(), &wrong, 3).unwrap(),
            3,
        );
        assert!(index.replay_entry(&wrong_release).is_err());
        assert!(index.contains(&reservation));
        let release = entry(
            dispatch_release_observation(template(), &reservation, 2).unwrap(),
            2,
        );
        index.replay_entry(&release).unwrap();
        assert_eq!(index.check(&original).unwrap(), CallDispatchCheckV1::Fresh);
        assert!(index.replay_entry(&release).is_err());
        index.replay_entry(&first).unwrap();
        assert_eq!(
            index
                .check(&claim("caller-a", "private-key", "call-a", 719))
                .unwrap(),
            CallDispatchCheckV1::Existing
        );
        assert_eq!(
            index
                .check(&claim("caller-a", "private-key", "call-a", 720))
                .unwrap(),
            CallDispatchCheckV1::Fresh
        );
        assert!(
            !serde_json::to_string(&first)
                .unwrap()
                .contains("private-key")
        );
    }

    #[test]
    fn call_dispatch_parser_rejects_malformed_unknown_and_wrong_envelopes() {
        let (obs, _) =
            dispatch_reservation_observation(template(), claim("a", "key", "call-a", 0), 1)
                .unwrap();
        let good = entry(obs, 1);
        let mut bad = good.clone();
        bad.durability_class = DurabilityClass::Buffered;
        assert!(decode_event(&bad).is_err());
        let mut bad = good.clone();
        bad.local_sequence = 2;
        assert!(decode_event(&bad).is_err());
        let mut bad = good.clone();
        bad.observation.kind = ObservationKind::ChildInvoked;
        assert!(decode_event(&bad).is_err());
        let mut bad = good.clone();
        bad.observation.call_id = None;
        assert!(decode_event(&bad).is_err());
        for detail in [
            "mct-call-dispatch-v2:{}".to_owned(),
            format!("{PREFIX}{{}}"),
            format!("{PREFIX}{}", "x".repeat(4097)),
            format!("{PREFIX}{{\"kind\":\"unknown\"}}"),
        ] {
            let mut bad = good.clone();
            bad.observation.detail_ref = Some(detail);
            assert!(decode_event(&bad).is_err());
        }
        let mut event: serde_json::Value = serde_json::from_str(
            good.observation
                .detail_ref
                .as_ref()
                .unwrap()
                .strip_prefix(PREFIX)
                .unwrap(),
        )
        .unwrap();
        event["unexpected"] = true.into();
        let mut bad = good.clone();
        bad.observation.detail_ref = Some(format!("{PREFIX}{event}"));
        assert!(decode_event(&bad).is_err());
        let mut bad_claim = claim("a", "key", "call-a", 0);
        bad_claim.expires_at = bad_claim.accepted_at.clone();
        assert!(bad_claim.validate().is_err());
        bad.observation.detail_ref = Some("unrelated-historical-fact".into());
        assert!(decode_event(&bad).unwrap().is_none());
    }

    #[test]
    fn call_dispatch_complete_unacknowledged_fact_and_release_replay() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ledger.jsonl");
        let original = claim("a", "key", "call-a", 0);
        let mut ledger = JsonlObservationLedger::open(&path, "test", "test").unwrap();
        ledger
            .append_before_effect(template(), "2026-10-19T00:00:00Z")
            .unwrap();
        let (obs, reservation) =
            dispatch_reservation_observation(template(), original.clone(), 1).unwrap();
        ledger
            .append_before_effect(obs, "2026-10-19T00:00:00Z")
            .unwrap();
        // Lose the caller acknowledgement and every runtime projection.
        drop(ledger);
        let mut ledger = JsonlObservationLedger::open(&path, "test", "test").unwrap();
        let mut index = CallDispatchIndexV1::default();
        for entry in ledger.iter_entries() {
            index.replay_entry(&entry.unwrap()).unwrap();
        }
        assert_eq!(
            index.check(&original).unwrap(),
            CallDispatchCheckV1::Existing
        );
        ledger
            .append_before_effect(
                dispatch_release_observation(template(), &reservation, 2).unwrap(),
                "2026-10-19T00:00:01Z",
            )
            .unwrap();
        drop(ledger);
        let ledger = JsonlObservationLedger::open(&path, "test", "test").unwrap();
        let mut index = CallDispatchIndexV1::default();
        for entry in ledger.iter_entries() {
            index.replay_entry(&entry.unwrap()).unwrap();
        }
        assert_eq!(index.check(&original).unwrap(), CallDispatchCheckV1::Fresh);
    }

    #[test]
    fn call_dispatch_torn_reservation_is_not_committed_but_torn_release_keeps_fence() {
        use std::io::Write as _;
        for tear_release in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("ledger.jsonl");
            let original = claim("a", "key", "call-a", 0);
            let (obs, reservation) =
                dispatch_reservation_observation(template(), original.clone(), 1).unwrap();
            let mut ledger = JsonlObservationLedger::open(&path, "test", "test").unwrap();
            ledger
                .append_before_effect(template(), "2026-10-19T00:00:00Z")
                .unwrap();
            if tear_release {
                ledger
                    .append_before_effect(obs, "2026-10-19T00:00:00Z")
                    .unwrap();
            }
            drop(ledger);
            let torn = if tear_release {
                dispatch_release_observation(template(), &reservation, 2).unwrap()
            } else {
                dispatch_reservation_observation(template(), original.clone(), 1)
                    .unwrap()
                    .0
            };
            let bytes = serde_json::to_vec(&entry(torn, if tear_release { 2 } else { 1 })).unwrap();
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .unwrap();
            file.write_all(&bytes[..bytes.len() / 2]).unwrap();
            file.sync_all().unwrap();
            drop(file);
            let ledger = JsonlObservationLedger::open(&path, "test", "test").unwrap();
            assert!(ledger.recovery_status().is_some());
            let mut index = CallDispatchIndexV1::default();
            for entry in ledger.iter_entries() {
                index.replay_entry(&entry.unwrap()).unwrap();
            }
            assert_eq!(
                index.check(&original).unwrap(),
                if tear_release {
                    CallDispatchCheckV1::Existing
                } else {
                    CallDispatchCheckV1::Fresh
                }
            );
        }
    }
}

#[cfg(feature = "fuzzing")]
pub fn fuzz_call_dispatch(data: &[u8]) {
    if data.len() > 16384 {
        return;
    }
    if let Ok(entry) = serde_json::from_slice::<MctObservationLedgerEntry>(data) {
        let mut index = CallDispatchIndexV1::default();
        let _ = index.replay_entry(&entry);
        let _ = index.replay_entry(&entry);
    }
}
