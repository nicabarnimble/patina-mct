//! Scheme v1 admin signatures: Ed25519 AND ML-DSA-65.
//!
//! The hybrid matches `patina-wasi-pi` `crates/mother/src/sig.rs` at `c436eaf`:
//! tag `0x48`, the Ed25519 half signs `context ‖ msg`, the ML-DSA-65 half signs
//! `msg` under that FIPS 204 context, and both halves must verify. The context
//! string is MCT's admin context, not `patina-sig/ed25519+mldsa65/v1` and not
//! the peer-binding context, so an admin proof cannot pass as either of those.
//!
//! Mother verifies. The signing key has no serde form, so a raw secret cannot
//! land in a fact. Transport stays plain Ed25519; iroh never sees the ML-DSA half.

use crate::admin_key::AdminKeyPublicHalf;
use ed25519_dalek::{Signer as _, Verifier as _};
use libcrux_ml_dsa::ml_dsa_65 as mldsa65;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// FIPS 204 context and Ed25519 domain prefix for MCT admin signatures.
pub const ADMIN_SIG_CONTEXT: &[u8] = b"mct-admin-sig/ed25519+mldsa65/v1";
/// Domain tag at the start of every signed admin message.
pub const ADMIN_SIGNED_MESSAGE_DOMAIN: &[u8] = b"mct-admin-signed-change";
/// Scheme name, bound into the signed bytes with [`ADMIN_SCHEME_VERSION`].
pub const ADMIN_SCHEME_NAME: &str = "ed25519+mldsa65";
/// Scheme version. A later version would be a new tag, not a guess from length.
pub const ADMIN_SCHEME_VERSION: &str = "v1";
/// Scheme version tag, first byte of the encoded signature. Same tag as wasi-pi v1.
pub const ADMIN_SIG_V1_TAG: u8 = 0x48;
/// Encoded ML-DSA-65 verifying key length.
pub const ML_DSA65_PUBLIC_KEY_LEN: usize = 1952;
/// Encoded ML-DSA-65 signature length.
pub const ML_DSA65_SIGNATURE_LEN: usize = 3309;
/// `0x48 ‖ ed25519 (64) ‖ ML-DSA-65 (3309)`.
pub const ADMIN_HYBRID_SIGNATURE_LEN: usize = 1 + 64 + ML_DSA65_SIGNATURE_LEN;

/// Why an admin signature or ML-DSA key is not accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AdminSignatureError {
    /// Not hex, or wrong length for the scheme.
    #[error("admin signature is malformed")]
    Malformed,
    /// ML-DSA-65 public key is not 1952 bytes.
    #[error("ml-dsa-65 public key is malformed")]
    MalformedPublicKey,
    /// Ed25519 half did not verify.
    #[error("admin signature ed25519 half does not verify")]
    Ed25519Invalid,
    /// ML-DSA-65 half did not verify.
    #[error("admin signature ml-dsa-65 half does not verify")]
    MlDsaInvalid,
}

/// ML-DSA-65 verifying key stored on an admin grant, hex on the wire.
///
/// This is the public half only. It cannot represent a seed or an expanded secret.
#[derive(Clone, PartialEq, Eq)]
pub struct MlDsa65PublicKey(Vec<u8>);

impl std::fmt::Debug for MlDsa65PublicKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "MlDsa65PublicKey({}…)", hex_encode(&self.0[..8]))
    }
}

impl MlDsa65PublicKey {
    /// Parses an encoded ML-DSA-65 verifying key.
    ///
    /// # Errors
    ///
    /// Returns an error unless `bytes` is exactly 1952 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AdminSignatureError> {
        if bytes.len() != ML_DSA65_PUBLIC_KEY_LEN {
            return Err(AdminSignatureError::MalformedPublicKey);
        }
        Ok(Self(bytes.to_vec()))
    }

    /// Parses lowercase or uppercase hex.
    ///
    /// # Errors
    ///
    /// Returns an error for non-hex text or a wrong length.
    pub fn from_hex(hex: &str) -> Result<Self, AdminSignatureError> {
        Self::from_bytes(&hex_decode(hex).ok_or(AdminSignatureError::MalformedPublicKey)?)
    }

    /// Encoded key bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Lowercase hex.
    pub fn to_hex(&self) -> String {
        hex_encode(&self.0)
    }

    fn verifying_key(&self) -> mldsa65::MLDSA65VerificationKey {
        mldsa65::MLDSA65VerificationKey::new(
            self.0
                .as_slice()
                .try_into()
                .expect("length checked at construction"),
        )
    }
}

impl Serialize for MlDsa65PublicKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for MlDsa65PublicKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::from_hex(&text).map_err(D::Error::custom)
    }
}

/// Scheme recorded on a signed admin fact. Replay verifies by this tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdminSignatureSchemeV1 {
    /// Ed25519 AND ML-DSA-65, version v1.
    #[serde(rename = "ed25519+mldsa65/v1")]
    Ed25519MlDsa65V1,
}

impl AdminSignatureSchemeV1 {
    /// Wire name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Ed25519MlDsa65V1 => "ed25519+mldsa65/v1",
        }
    }

    /// Scheme version bound into the signed bytes.
    pub fn version(self) -> &'static str {
        match self {
            Self::Ed25519MlDsa65V1 => ADMIN_SCHEME_VERSION,
        }
    }
}

/// Signature carried on one admin authority change.
///
/// The signature covers the command plus the signer grant. `nonce` is strictly
/// increasing per signer grant; replay rejects a reuse.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminSignatureV1 {
    /// Scheme of `signature`. Replay uses this, not a global switch.
    pub scheme: AdminSignatureSchemeV1,
    /// Grant whose Ed25519 and ML-DSA-65 keys verify this signature.
    pub signer_grant_id: String,
    /// Counter for this signer grant. A second use of the same value is rejected.
    pub nonce: u64,
    /// Hex of `0x48 ‖ ed25519 sig ‖ ML-DSA-65 sig`.
    pub signature: String,
}

/// Bytes an admin signs for one change.
///
/// Length-prefixed: domain tag, scheme version, Mother identity, ledger id,
/// nonce, the signer grant (both public keys), and the command.
pub fn admin_signed_message(parts: AdminSignedMessageParts<'_>) -> Vec<u8> {
    let mut out = Vec::new();
    for part in [
        ADMIN_SIGNED_MESSAGE_DOMAIN,
        parts.scheme.version().as_bytes(),
        parts.mother_node_id.as_bytes(),
        parts.ledger_id.as_bytes(),
        &parts.nonce.to_be_bytes(),
        parts.signer_grant_canonical,
        parts.command_canonical,
    ] {
        out.extend_from_slice(&(part.len() as u64).to_be_bytes());
        out.extend_from_slice(part);
    }
    out
}

/// Inputs to [`admin_signed_message`].
pub struct AdminSignedMessageParts<'a> {
    /// Scheme version bound into the message.
    pub scheme: AdminSignatureSchemeV1,
    /// Mother this command is for.
    pub mother_node_id: &'a str,
    /// Ledger this command is for.
    pub ledger_id: &'a str,
    /// Counter or nonce for the signer grant.
    pub nonce: u64,
    /// Canonical JSON of the signer grant, including both public keys.
    pub signer_grant_canonical: &'a [u8],
    /// Canonical JSON of the command, with the signature field omitted.
    pub command_canonical: &'a [u8],
}

/// Admin-side hybrid key. It has no serde form, so it cannot land in a fact.
pub struct AdminHybridSigningKey {
    ed: ed25519_dalek::SigningKey,
    pq: mldsa65::MLDSA65KeyPair,
}

impl std::fmt::Debug for AdminHybridSigningKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("AdminHybridSigningKey(<redacted>)")
    }
}

impl AdminHybridSigningKey {
    /// Fresh key from the OS RNG.
    ///
    /// # Panics
    ///
    /// Panics when the OS RNG is unavailable.
    pub fn generate() -> Self {
        Self::from_seeds(&os_seed(), &os_seed())
    }

    /// Key from an Ed25519 seed and an ML-DSA-65 seed.
    pub fn from_seeds(ed25519_seed: &[u8; 32], ml_dsa_seed: &[u8; 32]) -> Self {
        Self {
            ed: ed25519_dalek::SigningKey::from_bytes(ed25519_seed),
            pq: mldsa65::generate_key_pair(*ml_dsa_seed),
        }
    }

    /// Ed25519 public half, as stored on the grant. Transport stays this key.
    pub fn public_half(&self) -> AdminKeyPublicHalf {
        AdminKeyPublicHalf::ed25519_from_bytes(self.ed.verifying_key().as_bytes())
            .expect("dalek key is canonical")
    }

    /// ML-DSA-65 public key, stored on the grant beside the Ed25519 half.
    pub fn ml_dsa65_public_key(&self) -> MlDsa65PublicKey {
        MlDsa65PublicKey(self.pq.verification_key.as_slice().to_vec())
    }

    /// Hybrid signature bytes over `msg`.
    ///
    /// Ed25519 signs `ADMIN_SIG_CONTEXT ‖ msg`. ML-DSA-65 signs `msg` under the
    /// FIPS 204 context `ADMIN_SIG_CONTEXT`.
    pub fn sign(&self, msg: &[u8]) -> Vec<u8> {
        let mut prefixed = ADMIN_SIG_CONTEXT.to_vec();
        prefixed.extend_from_slice(msg);
        let mut out = Vec::with_capacity(ADMIN_HYBRID_SIGNATURE_LEN);
        out.push(ADMIN_SIG_V1_TAG);
        out.extend_from_slice(&self.ed.sign(&prefixed).to_bytes());
        // Deterministic FIPS 204 variant: rnd is 32 zero bytes.
        let pq = mldsa65::sign(&self.pq.signing_key, msg, ADMIN_SIG_CONTEXT, [0u8; 32])
            .expect("context is shorter than 255 bytes");
        out.extend_from_slice(pq.as_slice());
        out
    }

    /// Hex of [`Self::sign`].
    pub fn sign_hex(&self, msg: &[u8]) -> String {
        hex_encode(&self.sign(msg))
    }
}

/// Verifies a v1 hybrid signature. Both halves must verify.
///
/// # Errors
///
/// Returns why the signature is refused.
pub fn verify_admin_hybrid_signature(
    ed25519: &AdminKeyPublicHalf,
    ml_dsa65: &MlDsa65PublicKey,
    msg: &[u8],
    signature_hex: &str,
) -> Result<(), AdminSignatureError> {
    let bytes = hex_decode(signature_hex).ok_or(AdminSignatureError::Malformed)?;
    if bytes.len() != ADMIN_HYBRID_SIGNATURE_LEN || bytes[0] != ADMIN_SIG_V1_TAG {
        return Err(AdminSignatureError::Malformed);
    }
    let ed_sig: [u8; 64] = bytes[1..65].try_into().expect("length checked");
    let mut prefixed = ADMIN_SIG_CONTEXT.to_vec();
    prefixed.extend_from_slice(msg);
    ed25519_dalek::VerifyingKey::from_bytes(&ed25519.to_ed25519_bytes())
        .map_err(|_| AdminSignatureError::Ed25519Invalid)?
        .verify(&prefixed, &ed25519_dalek::Signature::from_bytes(&ed_sig))
        .map_err(|_| AdminSignatureError::Ed25519Invalid)?;
    let pq_sig = mldsa65::MLDSA65Signature::new(bytes[65..].try_into().expect("length checked"));
    if mldsa65::verify(&ml_dsa65.verifying_key(), msg, ADMIN_SIG_CONTEXT, &pq_sig).is_err() {
        return Err(AdminSignatureError::MlDsaInvalid);
    }
    Ok(())
}

fn os_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).expect("os rng");
    seed
}

fn hex_encode(bytes: &[u8]) -> String {
    const_hex::encode(bytes)
}

/// Strict lowercase-or-uppercase hex. A `0x` / `0X` prefix is refused.
fn hex_decode(text: &str) -> Option<Vec<u8>> {
    if text.starts_with("0x") || text.starts_with("0X") {
        return None;
    }
    const_hex::decode(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(nonce: u64) -> Vec<u8> {
        admin_signed_message(AdminSignedMessageParts {
            scheme: AdminSignatureSchemeV1::Ed25519MlDsa65V1,
            mother_node_id: "mother-a",
            ledger_id: "ledger-a",
            nonce,
            signer_grant_canonical: br#"{"grant_id":"g"}"#,
            command_canonical: br#"{"change_kind":"admin_key_grant_revoke","grant_id":"x"}"#,
        })
    }

    #[test]
    fn hybrid_sign_and_verify_roundtrip_with_v1_lengths() {
        let key = AdminHybridSigningKey::generate();
        let msg = message(1);
        let sig = key.sign_hex(&msg);
        assert_eq!(sig.len(), ADMIN_HYBRID_SIGNATURE_LEN * 2);
        assert_eq!(key.ml_dsa65_public_key().as_bytes().len(), 1952);
        assert_eq!(key.sign(&msg).len(), ADMIN_HYBRID_SIGNATURE_LEN);
        verify_admin_hybrid_signature(&key.public_half(), &key.ml_dsa65_public_key(), &msg, &sig)
            .unwrap();
    }

    #[test]
    fn tampering_either_half_or_the_message_is_rejected() {
        let key = AdminHybridSigningKey::generate();
        let (ed, pq) = (key.public_half(), key.ml_dsa65_public_key());
        let msg = message(1);
        let sig = key.sign_hex(&msg);
        let flip = |index: usize| {
            let mut bytes = hex_decode(&sig).unwrap();
            bytes[index] ^= 1;
            hex_encode(&bytes)
        };
        assert_eq!(
            verify_admin_hybrid_signature(&ed, &pq, &msg, &flip(10)),
            Err(AdminSignatureError::Ed25519Invalid)
        );
        assert_eq!(
            verify_admin_hybrid_signature(&ed, &pq, &msg, &flip(200)),
            Err(AdminSignatureError::MlDsaInvalid)
        );
        assert!(verify_admin_hybrid_signature(&ed, &pq, &message(2), &sig).is_err());
        assert_eq!(
            verify_admin_hybrid_signature(&ed, &pq, &msg, &sig[..128]),
            Err(AdminSignatureError::Malformed)
        );
        let other = AdminHybridSigningKey::generate();
        assert_eq!(
            verify_admin_hybrid_signature(&ed, &other.ml_dsa65_public_key(), &msg, &sig),
            Err(AdminSignatureError::MlDsaInvalid)
        );
    }

    #[test]
    fn signature_under_the_wasi_pi_context_is_rejected() {
        let ed_seed = os_seed();
        let pq_seed = os_seed();
        let key = AdminHybridSigningKey::from_seeds(&ed_seed, &pq_seed);
        let msg = message(1);
        // Same construction as patina-wasi-pi sig.rs, with that crate's domain.
        let foreign: &[u8] = b"patina-sig/ed25519+mldsa65/v1";
        let mut prefixed = foreign.to_vec();
        prefixed.extend_from_slice(&msg);
        let mut bytes = vec![ADMIN_SIG_V1_TAG];
        bytes.extend_from_slice(&key.ed.sign(&prefixed).to_bytes());
        let pq = mldsa65::sign(&key.pq.signing_key, &msg, foreign, [0u8; 32]).unwrap();
        bytes.extend_from_slice(pq.as_slice());
        assert_eq!(
            verify_admin_hybrid_signature(
                &key.public_half(),
                &key.ml_dsa65_public_key(),
                &msg,
                &hex_encode(&bytes)
            ),
            Err(AdminSignatureError::Ed25519Invalid)
        );
        let bare = key.ed.sign(&msg).to_bytes();
        let mut downgraded = vec![ADMIN_SIG_V1_TAG];
        downgraded.extend_from_slice(&bare);
        downgraded.extend_from_slice(pq.as_slice());
        assert!(
            verify_admin_hybrid_signature(
                &key.public_half(),
                &key.ml_dsa65_public_key(),
                &msg,
                &hex_encode(&downgraded)
            )
            .is_err()
        );
    }

    #[test]
    fn signed_message_binds_domain_version_mother_and_nonce() {
        let msg = message(7);
        let domain = ADMIN_SIGNED_MESSAGE_DOMAIN;
        assert!(msg.windows(domain.len()).any(|window| window == domain));
        let version = ADMIN_SCHEME_VERSION.as_bytes();
        assert!(msg.windows(version.len()).any(|window| window == version));
        assert!(msg.windows(8).any(|window| window == b"mother-a"));
        assert!(msg.windows(8).any(|window| window == 7u64.to_be_bytes()));
        assert_ne!(message(7), message(8));
    }

    /// libcrux output matches RustCrypto `ml-dsa` 0.1.1, the previous signer.
    #[test]
    fn libcrux_matches_rustcrypto_ml_dsa_bytes() {
        use ml_dsa::{EncodedVerifyingKey, MlDsa65};
        for i in 0u8..8 {
            let (ed_seed, pq_seed) = ([i; 32], [i.wrapping_mul(37).wrapping_add(1); 32]);
            let key = AdminHybridSigningKey::from_seeds(&ed_seed, &pq_seed);
            let msg = message(u64::from(i) + 1);
            let rc = ml_dsa::SigningKey::<MlDsa65>::from_seed(&pq_seed.into());
            let rc_pk = rc.expanded_key().verifying_key().encode();
            assert_eq!(key.ml_dsa65_public_key().as_bytes(), rc_pk.as_slice());
            let rc_sig = rc
                .expanded_key()
                .sign_deterministic(&msg, ADMIN_SIG_CONTEXT)
                .unwrap();
            let sig = key.sign(&msg);
            assert_eq!(&sig[65..], rc_sig.encode().as_slice());
            let vk = ml_dsa::VerifyingKey::<MlDsa65>::decode(
                &EncodedVerifyingKey::<MlDsa65>::try_from(rc_pk.as_slice()).unwrap(),
            );
            let lc_sig = ml_dsa::Signature::<MlDsa65>::try_from(&sig[65..]).unwrap();
            assert!(vk.verify_with_context(&msg, ADMIN_SIG_CONTEXT, &lc_sig));
        }
    }

    #[test]
    fn hex_codec_rejects_0x_prefix_and_keeps_plain_hex() {
        assert_eq!(hex_encode(&[0x00, 0xab, 0xff]), "00abff");
        assert_eq!(hex_decode("00abFF"), Some(vec![0x00, 0xab, 0xff]));
        assert_eq!(hex_decode(""), Some(vec![]));
        assert_eq!(hex_decode("abc"), None);
        assert_eq!(hex_decode("zz"), None);
        assert_eq!(hex_decode("0xab"), None);
        assert_eq!(hex_decode("0Xab"), None);
        assert_eq!(hex_decode(" ab"), None);
    }

    #[test]
    fn ml_dsa_key_rejects_wrong_length_and_signing_key_debug_has_no_secret() {
        assert!(MlDsa65PublicKey::from_hex(&"ab".repeat(32)).is_err());
        assert!(MlDsa65PublicKey::from_hex("zz").is_err());
        let key = AdminHybridSigningKey::generate();
        let debug = format!("{key:?}");
        assert_eq!(debug, "AdminHybridSigningKey(<redacted>)");
        assert!(!debug.contains(&hex_encode(&key.ed.to_bytes())));
    }
}
