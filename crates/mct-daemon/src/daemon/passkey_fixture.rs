//! A real passkey-rooted (D-58) ledger for governed-profile tests: software
//! ES256 root, Ed25519 day key, and the production admin adapter.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use mct_kernel::AdminKeyScopeV1;
use mct_kernel::admin_key::{
    AdminKeyPublicHalf, PasskeyAdminActionV1, PasskeyAdminIntentV1, WebAuthnP256DayKeyV1,
};
use mct_kernel::admin_sig::{
    DayKeyAdminProofV1, PasskeyAdminRequestV1, day_key_signing_message, webauthn_admin_challenge,
};
use mct_observation::{
    AUTHORITY_EPOCH_PREFIX_V1, AuthorityEpochPredecessorV1, AuthorityStartupClassV1,
    AuthorityStateV1, AuthorityTenureStartupEvidenceV1, JsonlObservationLedger,
    PasskeyGenesisSetupV1, authority_state_hash,
};
use p256::ecdsa::signature::Signer as _;
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};

pub(crate) const OPS_KEY: &str = "ops";

fn root_key() -> p256::ecdsa::SigningKey {
    p256::ecdsa::SigningKey::from_slice(&[7; 32]).unwrap()
}

fn day_key() -> ed25519_dalek::SigningKey {
    ed25519_dalek::SigningKey::from_bytes(&[9; 32])
}

fn root() -> WebAuthnP256DayKeyV1 {
    WebAuthnP256DayKeyV1 {
        credential_id: URL_SAFE_NO_PAD.encode([8; 32]),
        public_key_sec1: const_hex::encode(root_key().verifying_key().to_sec1_bytes()),
        rp_id: "admin.example.test".into(),
        origin: "https://admin.example.test".into(),
    }
}

fn stamp(offset_secs: u64) -> mct_kernel::Timestamp {
    let now = jiff::Timestamp::now()
        .checked_add(std::time::Duration::from_secs(offset_secs))
        .unwrap();
    mct_kernel::Timestamp::new(now.to_string()).unwrap()
}

fn root_signed(intent: PasskeyAdminIntentV1) -> PasskeyAdminRequestV1 {
    let root = root();
    let mut auth = Sha256::digest(root.rp_id.as_bytes()).to_vec();
    auth.push(0x1d);
    auth.extend([0; 4]);
    let client = serde_json::to_vec(&serde_json::json!({
        "type": "webauthn.get",
        "challenge": webauthn_admin_challenge(&intent, &root.credential_id).unwrap(),
        "origin": root.origin,
        "crossOrigin": false
    }))
    .unwrap();
    let mut signed = auth.clone();
    signed.extend(Sha256::digest(&client));
    let signature: p256::ecdsa::Signature = root_key().sign(&signed);
    PasskeyAdminRequestV1 {
        intent,
        proof: DayKeyAdminProofV1::WebAuthnP256DayKeyV1 {
            credential_id: root.credential_id,
            authenticator_data: URL_SAFE_NO_PAD.encode(auth),
            client_data_json: URL_SAFE_NO_PAD.encode(client),
            signature: URL_SAFE_NO_PAD.encode(signature.to_der().as_bytes()),
        },
    }
}

fn day_key_signed(intent: PasskeyAdminIntentV1, grant_id: &str) -> PasskeyAdminRequestV1 {
    use ed25519_dalek::Signer as _;
    let message = day_key_signing_message(&intent, &format!("ed25519:{grant_id}")).unwrap();
    PasskeyAdminRequestV1 {
        intent,
        proof: DayKeyAdminProofV1::Ed25519DayKeyV1 {
            grant_id: grant_id.into(),
            signature: const_hex::encode(day_key().sign(&message).to_bytes()),
        },
    }
}

/// Passkey genesis for `local-mct` / `ledger-local`, plus one day key holding
/// the four governed scopes.
pub(crate) struct GovernedLedger {
    path: PathBuf,
    nonce: u64,
}

impl GovernedLedger {
    pub(crate) fn create(path: &Path) -> Self {
        let startup = AuthorityTenureStartupEvidenceV1 {
            startup_class: AuthorityStartupClassV1::Virgin,
            expected_predecessor: AuthorityEpochPredecessorV1::NoneForVirgin,
            expected_prior_authority: None,
            expected_authority_state_hash: authority_state_hash(&AuthorityStateV1::default())
                .unwrap(),
            inventory_hash: "ab".repeat(32),
            operator_gate_decision_id: None,
            authenticated_principal_ref: None,
        };
        let setup = PasskeyGenesisSetupV1::from_registered_passkey(
            "local-mct".into(),
            "ledger-local".into(),
            root(),
            &startup,
        )
        .unwrap();
        let birth = root_signed(PasskeyAdminIntentV1 {
            mother_node_id: "local-mct".into(),
            ledger_id: "ledger-local".into(),
            epoch: format!("{AUTHORITY_EPOCH_PREFIX_V1}{}", "ac".repeat(32)),
            generation: 0,
            authority_observation_id: String::new(),
            nonce: format!("{:064x}", 0),
            issued_at: stamp(0),
            expires_at: stamp(240),
            action: PasskeyAdminActionV1::Bootstrap {
                root: root(),
                inventory_hash: startup.inventory_hash,
            },
        });
        JsonlObservationLedger::create_passkey_genesis(path, &setup, birth).unwrap();
        let mut governed = Self {
            path: path.to_path_buf(),
            nonce: 0,
        };
        let request = governed.root_request(PasskeyAdminActionV1::IssueDayKey {
            grant_id: OPS_KEY.into(),
            public_half: AdminKeyPublicHalf::ed25519_from_bytes(
                day_key().verifying_key().as_bytes(),
            )
            .unwrap(),
            scopes: mct_observation::GOVERNED_ROUTES
                .iter()
                .map(|(_, scope)| *scope)
                .collect(),
            expires_at: stamp(3_600),
        });
        governed.apply(request).unwrap();
        governed
    }

    fn intent(&mut self, action: PasskeyAdminActionV1) -> PasskeyAdminIntentV1 {
        self.nonce += 1;
        let ledger =
            JsonlObservationLedger::open_read_only(&self.path, "ledger-local", "local-mct")
                .unwrap();
        let replay = mct_observation::replay_authority_entries(&ledger.entries().unwrap()).unwrap();
        let authority = replay.current_authority.unwrap();
        PasskeyAdminIntentV1 {
            mother_node_id: "local-mct".into(),
            ledger_id: "ledger-local".into(),
            epoch: authority.authority_epoch,
            generation: authority.generation,
            authority_observation_id: authority.source_authority_observation_id,
            nonce: format!("{:064x}", self.nonce),
            issued_at: stamp(0),
            expires_at: stamp(240),
            action,
        }
    }

    fn root_request(&mut self, action: PasskeyAdminActionV1) -> PasskeyAdminRequestV1 {
        root_signed(self.intent(action))
    }

    /// Signs with the `ops` day key and applies through the production adapter.
    pub(crate) fn command(
        &mut self,
        route: &str,
        scope: AdminKeyScopeV1,
        body: serde_json::Value,
    ) -> anyhow::Result<serde_json::Value> {
        let request = day_key_signed(
            self.intent(PasskeyAdminActionV1::Command {
                scope,
                route: route.into(),
                body: body.to_string(),
            }),
            OPS_KEY,
        );
        self.apply(request)
    }

    fn apply(&mut self, request: PasskeyAdminRequestV1) -> anyhow::Result<serde_json::Value> {
        let mut ledger = JsonlObservationLedger::open(&self.path, "ledger-local", "local-mct")?;
        mct_daemon::execute_passkey_administration(&mut ledger, request)
    }
}
