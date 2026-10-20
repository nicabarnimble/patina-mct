use super::*;
use crate::{AdminHybridSigningKey, admin_sig::ADMIN_HYBRID_SIGNATURE_LEN};

fn key(seed: u8) -> AdminHybridSigningKey {
    AdminHybridSigningKey::from_seeds(&[seed; 32], &[seed.wrapping_add(64); 32])
}
fn public(key: &AdminHybridSigningKey) -> RecoveryPublicKeyV1 {
    RecoveryPublicKeyV1 {
        ed25519: key.public_half(),
        ml_dsa65: key.ml_dsa65_public_key(),
    }
}
fn sign(key: &AdminHybridSigningKey, bytes: &[u8]) -> RecoverySignatureV1 {
    key.sign_hex(bytes).try_into().unwrap()
}
fn time(value: &str) -> Timestamp {
    Timestamp::new(value).unwrap()
}
fn node(value: &str) -> MctNodeId {
    MctNodeId::new(value).unwrap()
}
fn genesis(key: &AdminHybridSigningKey) -> VoiceGenesisV1 {
    let public_key = public(key);
    let possession = sign(key, &VoiceGenesisV1::signing_message(&public_key).unwrap());
    VoiceGenesisV1 {
        public_key,
        possession,
    }
}

struct Fixture {
    policy: GuardianRecoveryPolicyV1,
    request: RecoveryRequestV1,
    signers: [AdminHybridSigningKey; 3],
    replacement: AdminHybridSigningKey,
}
impl Fixture {
    fn new() -> Self {
        let voice_id = genesis(&key(1)).verify().unwrap();
        let target = OwnerBindingV1 {
            mother_node_id: node("mother-a"),
            ledger_id: "ledger-a".into(),
            epoch_id: "epoch-a".into(),
            binding_id: "owner-a".into(),
            voice_id,
            root_generation: 3,
            root_key: public(&key(2)),
        };
        let signers = [key(3), key(4), key(5)];
        let guardians = std::array::from_fn(|index| RecoveryGuardianV1 {
            mother_node_id: node(&format!("mother-{}", index + 1)),
            owner_binding_id: format!("owner-{}", index + 1),
            owner_binding_generation: index as u64 + 1,
            voice_id,
            signing_key: public(&signers[index]),
        });
        let policy = GuardianRecoveryPolicyV1 {
            generation: 7,
            target: target.clone(),
            guardians,
        };
        let replacement = key(6);
        let intent = RecoveryIntentV1 {
            request_id: "request-1".into(),
            target,
            policy_generation: policy.generation,
            policy_digest: policy.digest().unwrap(),
            replacement_key: public(&replacement),
            not_before: time("2026-10-19T00:00:00Z"),
            expires_at: time("2026-10-27T00:00:00Z"),
        };
        let replacement_possession = sign(&replacement, &intent.signing_message().unwrap());
        Self {
            policy,
            request: RecoveryRequestV1 {
                intent,
                replacement_possession,
            },
            signers,
            replacement,
        }
    }
    fn approvals(&self, purpose: &RecoveryApprovalPurposeV1) -> Vec<GuardianRecoveryApprovalV1> {
        self.signers
            .iter()
            .enumerate()
            .map(|(index, signer)| {
                let guardian_node_id = self.policy.guardians[index].mother_node_id.clone();
                let issued_at = time("2026-10-19T00:00:00Z");
                let expires_at = time(&format!("2026-10-19T00:0{}:00Z", index + 7));
                let message = GuardianApprovalMessageV1 {
                    policy: &self.policy,
                    intent: &self.request.intent,
                    purpose,
                    guardian_node_id: &guardian_node_id,
                    issued_at: &issued_at,
                    expires_at: &expires_at,
                }
                .signing_message()
                .unwrap();
                GuardianRecoveryApprovalV1 {
                    guardian_node_id,
                    issued_at,
                    expires_at,
                    signature: sign(signer, &message),
                }
            })
            .collect()
    }
    fn verify(
        &self,
        purpose: &RecoveryApprovalPurposeV1,
        approvals: &[GuardianRecoveryApprovalV1],
    ) -> Result<RecoveryQuorumEvidenceV1, RecoveryError> {
        verify_recovery_quorum(
            &self.policy.target,
            &self.policy,
            &self.request,
            purpose,
            approvals,
            &time("2026-10-19T00:01:00Z"),
        )
    }
    fn resign_replacement(&mut self) {
        self.request.replacement_possession = sign(
            &self.replacement,
            &self.request.intent.signing_message().unwrap(),
        );
    }
}

#[test]
fn voice_genesis_is_stable_and_binds_both_key_halves() {
    let first = genesis(&key(1));
    let id = first.verify().unwrap();
    assert_eq!(id, genesis(&key(1)).verify().unwrap());
    assert_ne!(id, genesis(&key(2)).verify().unwrap());
    for classical in [true, false] {
        let mut altered = first.clone();
        if classical {
            altered.public_key.ed25519 = public(&key(2)).ed25519;
        } else {
            altered.public_key.ml_dsa65 = public(&key(2)).ml_dsa65;
        }
        assert_eq!(altered.verify(), Err(RecoveryError::SignatureInvalid));
    }
}

#[test]
fn voice_genesis_rejects_signatures_from_other_domains() {
    let signer = key(1);
    let mut record = genesis(&signer);
    record.possession = sign(&signer, b"mct-admin-signed-change");
    assert_eq!(record.verify(), Err(RecoveryError::SignatureInvalid));
    record.possession = sign(&signer, &serde_json::to_vec(&record.public_key).unwrap());
    assert_eq!(record.verify(), Err(RecoveryError::SignatureInvalid));
}

#[test]
fn digest_identifiers_reject_labels_wrong_domains_uppercase_and_bad_lengths() {
    let voice = genesis(&key(1)).verify().unwrap();
    let encoded = serde_json::to_string(&voice).unwrap();
    assert_eq!(serde_json::from_str::<VoiceIdV1>(&encoded).unwrap(), voice);
    assert!(serde_json::from_str::<RecoveryDigestV1>(&encoded).is_err());
    for bad in [
        "default".to_string(),
        format!("mct-voice-v1:{}", "A".repeat(64)),
        format!("mct-voice-v1:{}", "a".repeat(63)),
        format!("mct-voice-v2:{}", "a".repeat(64)),
    ] {
        assert!(serde_json::from_value::<VoiceIdV1>(bad.into()).is_err());
    }
}

#[test]
fn weak_ed25519_genesis_or_policy_keys_are_rejected() {
    let mut bytes = [0; 32];
    bytes[0] = 1;
    let weak = AdminKeyPublicHalf::ed25519_from_bytes(&bytes).unwrap();
    let mut g = genesis(&key(1));
    g.public_key.ed25519 = weak;
    assert_eq!(g.verify(), Err(RecoveryError::Malformed));
    let mut f = Fixture::new();
    f.policy.guardians[0].signing_key.ed25519 = weak;
    assert_eq!(f.policy.validate(), Err(RecoveryError::Malformed));
}

#[test]
fn two_of_three_and_all_three_return_bounded_exact_evidence() {
    let f = Fixture::new();
    let purpose = RecoveryApprovalPurposeV1::Initiate;
    let approvals = f.approvals(&purpose);
    for subset in [&approvals[..2], &approvals[1..], &approvals[..]] {
        let result = f.verify(&purpose, subset).unwrap();
        assert_eq!(result.intent_digest(), f.request.intent.digest().unwrap());
        assert_eq!(result.policy_digest(), f.policy.digest().unwrap());
        assert_eq!(result.purpose(), &purpose);
        assert_eq!(result.guardians().len(), subset.len());
        assert_eq!(
            result.valid_until(),
            &subset.iter().map(|a| a.expires_at.clone()).min().unwrap()
        );
    }
}

#[test]
fn insufficient_and_oversized_approval_sets_deny() {
    let f = Fixture::new();
    let purpose = RecoveryApprovalPurposeV1::Initiate;
    let mut approvals = f.approvals(&purpose);
    assert_eq!(
        f.verify(&purpose, &[]).unwrap_err(),
        RecoveryError::QuorumInvalid
    );
    assert_eq!(
        f.verify(&purpose, &approvals[..1]).unwrap_err(),
        RecoveryError::QuorumInvalid
    );
    approvals.push(approvals[0].clone());
    assert_eq!(
        f.verify(&purpose, &approvals).unwrap_err(),
        RecoveryError::QuorumInvalid
    );
}

#[test]
fn duplicate_approvals_cannot_count_twice() {
    let f = Fixture::new();
    let purpose = RecoveryApprovalPurposeV1::Initiate;
    let approvals = f.approvals(&purpose);
    assert_eq!(
        f.verify(&purpose, &[approvals[0].clone(), approvals[0].clone()])
            .unwrap_err(),
        RecoveryError::DuplicatePrincipal
    );
}

#[test]
fn invalid_extra_approval_is_not_ignored_after_a_valid_quorum() {
    let f = Fixture::new();
    let purpose = RecoveryApprovalPurposeV1::Initiate;
    let mut approvals = f.approvals(&purpose);
    approvals[2].guardian_node_id = node("ordinary-peer");
    assert_eq!(
        f.verify(&purpose, &approvals).unwrap_err(),
        RecoveryError::UnappointedGuardian
    );
}

#[test]
fn same_voice_is_required_for_every_appointed_guardian() {
    let mut f = Fixture::new();
    f.policy.guardians[2].voice_id = genesis(&key(20)).verify().unwrap();
    assert_eq!(f.policy.validate(), Err(RecoveryError::VoiceMismatch));
}

#[test]
fn policy_rejects_duplicate_nodes_either_key_half_and_self_guardianship() {
    let f = Fixture::new();
    for mutation in 0..5 {
        let mut p = f.policy.clone();
        match mutation {
            0 => p.guardians[1].mother_node_id = p.guardians[0].mother_node_id.clone(),
            1 => p.guardians[1].signing_key.ed25519 = p.guardians[0].signing_key.ed25519,
            2 => p.guardians[1].signing_key.ml_dsa65 = p.guardians[0].signing_key.ml_dsa65.clone(),
            3 => p.guardians[0].mother_node_id = p.target.mother_node_id.clone(),
            _ => p.guardians[0].signing_key = p.target.root_key.clone(),
        }
        assert_eq!(
            p.validate(),
            Err(RecoveryError::DuplicatePrincipal),
            "mutation {mutation}"
        );
    }
}

#[test]
fn policy_roster_order_is_canonical_and_generations_are_positive() {
    let f = Fixture::new();
    let mut p = f.policy.clone();
    p.guardians.swap(0, 1);
    assert_eq!(p.validate(), Err(RecoveryError::PolicyInvalid));
    p = f.policy.clone();
    p.generation = 0;
    assert_eq!(p.validate(), Err(RecoveryError::PolicyInvalid));
    p = f.policy.clone();
    p.guardians[0].owner_binding_generation = 0;
    assert_eq!(p.validate(), Err(RecoveryError::PolicyInvalid));
    p = f.policy.clone();
    p.target.root_generation = 0;
    assert_eq!(p.validate(), Err(RecoveryError::Malformed));
}

#[test]
fn stale_canonical_root_policy_and_guardian_generation_deny() {
    let f = Fixture::new();
    let purpose = RecoveryApprovalPurposeV1::Initiate;
    let approvals = f.approvals(&purpose);
    let mut current = f.policy.target.clone();
    current.root_generation += 1;
    assert_eq!(
        verify_recovery_quorum(
            &current,
            &f.policy,
            &f.request,
            &purpose,
            &approvals,
            &time("2026-10-19T00:01:00Z")
        )
        .unwrap_err(),
        RecoveryError::StaleAuthority
    );
    for mutation in 0..2 {
        let mut policy = f.policy.clone();
        if mutation == 0 {
            policy.generation += 1;
        } else {
            policy.guardians[0].owner_binding_generation += 1;
        }
        assert_eq!(
            verify_recovery_quorum(
                &policy.target,
                &policy,
                &f.request,
                &purpose,
                &approvals,
                &time("2026-10-19T00:01:00Z")
            )
            .unwrap_err(),
            RecoveryError::StaleAuthority
        );
    }
}

#[test]
fn request_cannot_change_target_mother_ledger_epoch_binding_or_voice() {
    let f = Fixture::new();
    let purpose = RecoveryApprovalPurposeV1::Initiate;
    let approvals = f.approvals(&purpose);
    for mutation in 0..5 {
        let mut request = f.request.clone();
        match mutation {
            0 => request.intent.target.mother_node_id = node("other-mother"),
            1 => request.intent.target.ledger_id = "other-ledger".into(),
            2 => request.intent.target.epoch_id = "other-epoch".into(),
            3 => request.intent.target.binding_id = "other-binding".into(),
            _ => request.intent.target.voice_id = genesis(&key(22)).verify().unwrap(),
        }
        assert_eq!(
            verify_recovery_quorum(
                &f.policy.target,
                &f.policy,
                &request,
                &purpose,
                &approvals,
                &time("2026-10-19T00:01:00Z")
            )
            .unwrap_err(),
            RecoveryError::StaleAuthority
        );
    }
}

#[test]
fn altered_request_id_or_replacement_requires_new_guardian_signatures() {
    for mutation in 0..2 {
        let mut f = Fixture::new();
        let purpose = RecoveryApprovalPurposeV1::Initiate;
        let approvals = f.approvals(&purpose);
        if mutation == 0 {
            f.request.intent.request_id = "request-2".into();
        } else {
            f.replacement = key(24);
            f.request.intent.replacement_key = public(&f.replacement);
        }
        f.resign_replacement();
        assert_eq!(
            f.verify(&purpose, &approvals).unwrap_err(),
            RecoveryError::SignatureInvalid
        );
    }
}

#[test]
fn old_root_or_guardian_key_half_cannot_be_reused_as_replacement() {
    let f = Fixture::new();
    let purpose = RecoveryApprovalPurposeV1::Initiate;
    let approvals = f.approvals(&purpose);
    for replacement in [
        &f.policy.target.root_key,
        &f.policy.guardians[0].signing_key,
    ] {
        for classical in [true, false] {
            let mut request = f.request.clone();
            if classical {
                request.intent.replacement_key.ed25519 = replacement.ed25519;
            } else {
                request.intent.replacement_key.ml_dsa65 = replacement.ml_dsa65.clone();
            }
            assert_eq!(
                verify_recovery_quorum(
                    &f.policy.target,
                    &f.policy,
                    &request,
                    &purpose,
                    &approvals,
                    &time("2026-10-19T00:01:00Z")
                )
                .unwrap_err(),
                RecoveryError::ReplacementNotFresh
            );
        }
    }
}

#[test]
fn each_signature_half_is_required_on_replacement_and_guardian_proofs() {
    let f = Fixture::new();
    let purpose = RecoveryApprovalPurposeV1::Initiate;
    for index in [1, 65] {
        let flip = |sig: &RecoverySignatureV1| {
            let mut bytes = const_hex::decode(sig.as_str()).unwrap();
            bytes[index] ^= 1;
            RecoverySignatureV1::try_from(const_hex::encode(bytes)).unwrap()
        };
        let approvals = f.approvals(&purpose);
        let mut request = f.request.clone();
        request.replacement_possession = flip(&request.replacement_possession);
        assert_eq!(
            verify_recovery_quorum(
                &f.policy.target,
                &f.policy,
                &request,
                &purpose,
                &approvals,
                &time("2026-10-19T00:01:00Z")
            )
            .unwrap_err(),
            RecoveryError::SignatureInvalid
        );
        let mut tampered = approvals.clone();
        tampered[0].signature = flip(&tampered[0].signature);
        assert_eq!(
            f.verify(&purpose, &tampered).unwrap_err(),
            RecoveryError::SignatureInvalid
        );
    }
}

#[test]
fn initiate_cancel_complete_and_completion_challenges_are_not_interchangeable() {
    let f = Fixture::new();
    let purposes = [
        RecoveryApprovalPurposeV1::Initiate,
        RecoveryApprovalPurposeV1::Cancel,
        RecoveryApprovalPurposeV1::Complete {
            challenge: "challenge-1".into(),
        },
        RecoveryApprovalPurposeV1::Complete {
            challenge: "challenge-2".into(),
        },
    ];
    for signed in &purposes {
        let approvals = f.approvals(signed);
        for expected in &purposes {
            let result = f.verify(expected, &approvals);
            if signed == expected {
                result.unwrap();
            } else {
                assert_eq!(result.unwrap_err(), RecoveryError::SignatureInvalid);
            }
        }
    }
}

#[test]
fn changing_guardian_identity_or_approval_expiry_invalidates_signature() {
    let f = Fixture::new();
    let purpose = RecoveryApprovalPurposeV1::Initiate;
    let mut approvals = f.approvals(&purpose);
    approvals[0].guardian_node_id = approvals[2].guardian_node_id.clone();
    assert_eq!(
        f.verify(&purpose, &approvals[..2]).unwrap_err(),
        RecoveryError::SignatureInvalid
    );
    approvals = f.approvals(&purpose);
    approvals[0].expires_at = time("2026-10-19T00:06:00Z");
    assert_eq!(
        f.verify(&purpose, &approvals[..2]).unwrap_err(),
        RecoveryError::SignatureInvalid
    );
}

#[test]
fn approval_expiry_is_exclusive_and_future_issuance_denies() {
    let f = Fixture::new();
    let purpose = RecoveryApprovalPurposeV1::Initiate;
    let approvals = f.approvals(&purpose);
    for now in [
        "2026-10-18T23:59:59Z",
        "2026-10-19T00:07:00Z",
        "2026-10-27T00:00:00Z",
    ] {
        assert_eq!(
            verify_recovery_quorum(
                &f.policy.target,
                &f.policy,
                &f.request,
                &purpose,
                &approvals[..2],
                &time(now)
            )
            .unwrap_err(),
            RecoveryError::TimeInvalid
        );
    }
    let mut future = approvals.clone();
    future[0].issued_at = time("2026-10-19T00:02:00Z");
    assert_eq!(
        f.verify(&purpose, &future).unwrap_err(),
        RecoveryError::TimeInvalid
    );
}

#[test]
fn approval_lifetime_cannot_exceed_ten_minutes_even_when_signed() {
    let f = Fixture::new();
    let purpose = RecoveryApprovalPurposeV1::Initiate;
    let mut approvals = f.approvals(&purpose);
    approvals[0].expires_at = time("2026-10-19T00:10:00.001Z");
    assert_eq!(
        f.verify(&purpose, &approvals).unwrap_err(),
        RecoveryError::TimeInvalid
    );
    let message = GuardianApprovalMessageV1 {
        policy: &f.policy,
        intent: &f.request.intent,
        purpose: &purpose,
        guardian_node_id: &approvals[0].guardian_node_id,
        issued_at: &approvals[0].issued_at,
        expires_at: &time("2026-10-19T00:10:00Z"),
    }
    .signing_message();
    assert!(message.is_ok());
}

#[test]
fn intent_validity_is_bounded_and_timestamp_overflow_denies() {
    let mut f = Fixture::new();
    f.request.intent.expires_at = time("2026-10-27T00:00:00.001Z");
    assert_eq!(
        f.request.intent.signing_message(),
        Err(RecoveryError::TimeInvalid)
    );
    f.request.intent.expires_at = f.request.intent.not_before.clone();
    assert_eq!(
        f.request.intent.signing_message(),
        Err(RecoveryError::TimeInvalid)
    );
    f.request.intent.not_before = time("9999-12-30T00:00:00Z");
    f.request.intent.expires_at = time("9999-12-30T00:00:01Z");
    assert_eq!(
        f.request.intent.signing_message(),
        Err(RecoveryError::TimeInvalid)
    );
}

#[test]
fn malformed_empty_control_and_oversized_identifiers_deny() {
    let f = Fixture::new();
    for bad in [
        "".into(),
        " ".into(),
        "x\ny".into(),
        "x".repeat(129),
        "../owner".into(),
        "owner😀".into(),
    ] {
        let mut intent = f.request.intent.clone();
        intent.request_id = bad;
        assert_eq!(intent.signing_message(), Err(RecoveryError::Malformed));
    }
    let approvals = f.approvals(&RecoveryApprovalPurposeV1::Initiate);
    assert_eq!(
        f.verify(
            &RecoveryApprovalPurposeV1::Complete {
                challenge: String::new()
            },
            &approvals
        )
        .unwrap_err(),
        RecoveryError::Malformed
    );
}

#[test]
fn signature_codec_is_bounded_normalized_and_rejects_downgrades() {
    let sig = sign(&key(1), b"message");
    assert_eq!(
        RecoverySignatureV1::try_from(sig.as_str().to_uppercase()).unwrap(),
        sig
    );
    for malformed in [
        String::new(),
        "ab".repeat(64),
        "ab".repeat(ADMIN_HYBRID_SIGNATURE_LEN + 1),
        format!("0x{}", sig.as_str()),
        format!("00{}", &sig.as_str()[2..]),
        "zz".repeat(ADMIN_HYBRID_SIGNATURE_LEN),
    ] {
        assert_eq!(
            RecoverySignatureV1::try_from(malformed),
            Err(RecoveryError::Malformed)
        );
    }
}

#[test]
fn strict_json_rejects_unknown_fields_missing_halves_and_wrong_roster_size() {
    let f = Fixture::new();
    let encoded = serde_json::to_vec(&f.request).unwrap();
    assert_eq!(
        serde_json::from_slice::<RecoveryRequestV1>(&encoded).unwrap(),
        f.request
    );
    let mut value = serde_json::to_value(&f.request).unwrap();
    value["allow_unsigned"] = true.into();
    assert!(serde_json::from_value::<RecoveryRequestV1>(value).is_err());
    let mut value = serde_json::to_value(&f.request).unwrap();
    value["intent"]["target"]["trusted"] = true.into();
    assert!(serde_json::from_value::<RecoveryRequestV1>(value).is_err());
    let mut value = serde_json::to_value(&f.request).unwrap();
    value["intent"]["replacement_key"]
        .as_object_mut()
        .unwrap()
        .remove("ml_dsa65");
    assert!(serde_json::from_value::<RecoveryRequestV1>(value).is_err());
    let mut value = serde_json::to_value(&f.policy).unwrap();
    value["guardians"].as_array_mut().unwrap().pop();
    assert!(serde_json::from_value::<GuardianRecoveryPolicyV1>(value).is_err());
    assert!(
        serde_json::from_str::<RecoveryApprovalPurposeV1>(
            r#"{"purpose":"initiate","challenge":"ignored"}"#
        )
        .is_err()
    );
}

#[test]
fn length_prefixed_domains_do_not_alias_and_reordered_approvals_are_equivalent() {
    assert_ne!(
        internal::transcript("ab", &"c").unwrap(),
        internal::transcript("a", &"bc").unwrap()
    );
    let f = Fixture::new();
    let purpose = RecoveryApprovalPurposeV1::Initiate;
    let mut approvals = f.approvals(&purpose);
    let first = f.verify(&purpose, &approvals).unwrap();
    approvals.reverse();
    let second = f.verify(&purpose, &approvals).unwrap();
    assert_eq!(first.guardians(), second.guardians());
    assert_eq!(first.valid_until(), second.valid_until());
}
