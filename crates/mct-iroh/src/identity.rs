use crate::endpoint::{MotherIrohEndpointError, MotherIrohEndpointResult};
use iroh::{PublicKey, SecretKey, Signature};
use mct_kernel::{EndpointIdText, MctPeerBinding};
use serde::Serialize;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{fs::OpenOptions, io::Write, path::Path, str::FromStr};

pub const MCT_PEER_BINDING_SIGNATURE_PREFIX: &str = "mct-ed25519-binding-v1:";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MctPeerBindingSignatureVerification {
    Valid,
    Missing,
    Malformed,
    Invalid,
}

pub fn load_or_create_node_secret_key_hex(
    path: impl AsRef<Path>,
) -> MotherIrohEndpointResult<String> {
    let path = path.as_ref();
    if path.exists() {
        let content = std::fs::read_to_string(path).map_err(|source| {
            MotherIrohEndpointError::IdentityFile {
                path: path.to_path_buf(),
                source,
            }
        })?;
        let secret_key_hex = content.trim().to_string();
        secret_key_from_hex(&secret_key_hex)?;
        return Ok(secret_key_hex);
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| {
            MotherIrohEndpointError::IdentityFile {
                path: parent.to_path_buf(),
                source,
            }
        })?;
    }
    let secret_key_hex = generate_node_secret_key_hex();
    write_new_node_secret_key_file(path, &secret_key_hex)?;
    Ok(secret_key_hex)
}

pub fn generate_node_secret_key_hex() -> String {
    secret_key_to_hex(&SecretKey::generate())
}

pub fn write_new_node_secret_key_file(
    path: &Path,
    secret_key_hex: &str,
) -> MotherIrohEndpointResult<()> {
    secret_key_from_hex(secret_key_hex)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| {
            MotherIrohEndpointError::IdentityFile {
                path: parent.to_path_buf(),
                source,
            }
        })?;
    }
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);

    let mut file = options
        .open(path)
        .map_err(|source| MotherIrohEndpointError::IdentityFile {
            path: path.to_path_buf(),
            source,
        })?;
    writeln!(file, "{secret_key_hex}").map_err(|source| MotherIrohEndpointError::IdentityFile {
        path: path.to_path_buf(),
        source,
    })
}

pub fn endpoint_id_for_secret_key_hex(
    secret_key_hex: &str,
) -> MotherIrohEndpointResult<EndpointIdText> {
    Ok(
        EndpointIdText::new(secret_key_from_hex(secret_key_hex)?.public().to_string())
            .expect("string ID literal/generated value must be non-empty"),
    )
}

pub fn sign_peer_binding_signature_ref(
    issuer_secret_key_hex: &str,
    binding: &MctPeerBinding,
    issuer_endpoint_id: &EndpointIdText,
) -> MotherIrohEndpointResult<String> {
    let secret_key = secret_key_from_hex(issuer_secret_key_hex)?;
    let derived_endpoint_id = EndpointIdText::new(secret_key.public().to_string())
        .expect("string ID literal/generated value must be non-empty");
    if &derived_endpoint_id != issuer_endpoint_id {
        return Err(MotherIrohEndpointError::InvalidSecretKey {
            reason: format!(
                "issuer endpoint mismatch: secret key is for {derived_endpoint_id}, binding issuer endpoint is {issuer_endpoint_id}"
            ),
        });
    }
    let signature = secret_key.sign(&peer_binding_signature_message(
        binding,
        issuer_endpoint_id,
    )?);
    Ok(format!(
        "{MCT_PEER_BINDING_SIGNATURE_PREFIX}{}",
        encode_hex(&signature.to_bytes())
    ))
}

pub fn verify_peer_binding_signature_ref(
    signature_ref: Option<&str>,
    binding: &MctPeerBinding,
    issuer_endpoint_id: &EndpointIdText,
) -> MctPeerBindingSignatureVerification {
    let Some(signature_ref) = signature_ref else {
        return MctPeerBindingSignatureVerification::Missing;
    };
    let Some(signature_hex) = signature_ref.strip_prefix(MCT_PEER_BINDING_SIGNATURE_PREFIX) else {
        return MctPeerBindingSignatureVerification::Malformed;
    };
    let Ok(signature_bytes) = decode_64_hex(signature_hex) else {
        return MctPeerBindingSignatureVerification::Malformed;
    };
    let signature = Signature::from_bytes(&signature_bytes);
    let Ok(public_key) = PublicKey::from_str(issuer_endpoint_id.as_str()) else {
        return MctPeerBindingSignatureVerification::Malformed;
    };
    let Ok(message) = peer_binding_signature_message(binding, issuer_endpoint_id) else {
        return MctPeerBindingSignatureVerification::Malformed;
    };
    if public_key.verify(&message, &signature).is_ok() {
        MctPeerBindingSignatureVerification::Valid
    } else {
        MctPeerBindingSignatureVerification::Invalid
    }
}

#[derive(Serialize)]
struct PeerBindingSignaturePayload<'a> {
    context: &'static str,
    binding_id: &'a str,
    issuer_node_id: &'a str,
    issuer_endpoint_id: &'a str,
    peer_node_id: &'a str,
    peer_endpoint_id: &'a str,
    vision_id: &'a str,
    allowed_alpns: &'a [String],
    policy_revision: u64,
    expires_at: &'a str,
}

fn peer_binding_signature_message(
    binding: &MctPeerBinding,
    issuer_endpoint_id: &EndpointIdText,
) -> MotherIrohEndpointResult<Vec<u8>> {
    let payload = PeerBindingSignaturePayload {
        context: "mct-peer-binding-signature-v1",
        binding_id: binding.binding_id.as_str(),
        issuer_node_id: binding.issuer_node_id.as_str(),
        issuer_endpoint_id: issuer_endpoint_id.as_str(),
        peer_node_id: binding.scope.mct_node_id.as_str(),
        peer_endpoint_id: binding.iroh_endpoint_id.as_str(),
        vision_id: binding.scope.vision_id.as_str(),
        allowed_alpns: &binding.scope.allowed_alpns,
        policy_revision: binding.policy_revision,
        expires_at: binding.expires_at.as_str(),
    };
    serde_json::to_vec(&payload).map_err(|source| MotherIrohEndpointError::ProtocolJson {
        action: "encode peer binding signature payload",
        source,
    })
}

pub(crate) fn secret_key_from_hex(secret_key_hex: &str) -> MotherIrohEndpointResult<SecretKey> {
    let bytes = decode_32_hex(secret_key_hex.trim())?;
    Ok(SecretKey::from_bytes(&bytes))
}

fn secret_key_to_hex(secret_key: &SecretKey) -> String {
    encode_hex(&secret_key.to_bytes())
}

fn decode_32_hex(value: &str) -> MotherIrohEndpointResult<[u8; 32]> {
    if value.len() != 64 {
        return Err(MotherIrohEndpointError::InvalidSecretKey {
            reason: format!("expected 64 lowercase hex characters, got {}", value.len()),
        });
    }
    let mut bytes = [0_u8; 32];
    decode_hex_into(value, &mut bytes)?;
    Ok(bytes)
}

fn decode_64_hex(value: &str) -> MotherIrohEndpointResult<[u8; 64]> {
    if value.len() != 128 {
        return Err(MotherIrohEndpointError::InvalidSecretKey {
            reason: format!("expected 128 lowercase hex characters, got {}", value.len()),
        });
    }
    let mut bytes = [0_u8; 64];
    decode_hex_into(value, &mut bytes)?;
    Ok(bytes)
}

fn decode_hex_into(value: &str, bytes: &mut [u8]) -> MotherIrohEndpointResult<()> {
    for (index, chunk) in value.as_bytes().chunks(2).enumerate() {
        let high = decode_hex_nibble(chunk[0])?;
        let low = decode_hex_nibble(chunk[1])?;
        bytes[index] = (high << 4) | low;
    }
    Ok(())
}

fn decode_hex_nibble(byte: u8) -> MotherIrohEndpointResult<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(MotherIrohEndpointError::InvalidSecretKey {
            reason: "secret key must be lowercase hex".into(),
        }),
    }
}

pub(crate) fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mct_kernel::{
        BindingState, MCT_CALL_ALPN, MCT_HELLO_ALPN, MctNodeId, MctPeerBindingScope, ObservationId,
        PeerBindingId, Timestamp, VisionId,
    };

    fn binding(peer_endpoint_id: EndpointIdText) -> MctPeerBinding {
        MctPeerBinding {
            binding_id: PeerBindingId::new("binding-signed")
                .expect("string ID literal/generated value must be non-empty"),
            iroh_endpoint_id: peer_endpoint_id,
            scope: MctPeerBindingScope {
                mct_node_id: MctNodeId::new("peer-node")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-a")
                    .expect("string ID literal/generated value must be non-empty"),
                allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
                data_scope: None,
                observation_scope: None,
            },
            issuer_node_id: MctNodeId::new("issuer-node")
                .expect("string ID literal/generated value must be non-empty"),
            policy_revision: 7,
            binding_state: BindingState::Admitted,
            issued_at: Timestamp::new("2026-07-09T00:00:00Z").unwrap(),
            expires_at: Timestamp::new("2026-07-09T00:05:00Z").unwrap(),
            created_by_observation_id: ObservationId::new("obs-binding-signed")
                .expect("string ID literal/generated value must be non-empty"),
            superseded_by_observation_id: None,
        }
    }

    #[test]
    fn peer_binding_signature_ref_roundtrips_and_fails_on_tamper() {
        let issuer_secret = SecretKey::generate();
        let issuer_secret_hex = secret_key_to_hex(&issuer_secret);
        let issuer_endpoint_id = endpoint_id_for_secret_key_hex(&issuer_secret_hex).unwrap();
        let peer_endpoint_id = EndpointIdText::new(SecretKey::generate().public().to_string())
            .expect("string ID literal/generated value must be non-empty");
        let binding = binding(peer_endpoint_id);

        let signature_ref =
            sign_peer_binding_signature_ref(&issuer_secret_hex, &binding, &issuer_endpoint_id)
                .unwrap();

        assert_eq!(
            verify_peer_binding_signature_ref(
                Some(signature_ref.as_str()),
                &binding,
                &issuer_endpoint_id
            ),
            MctPeerBindingSignatureVerification::Valid
        );
        assert_eq!(
            verify_peer_binding_signature_ref(None, &binding, &issuer_endpoint_id),
            MctPeerBindingSignatureVerification::Missing
        );
        assert_eq!(
            verify_peer_binding_signature_ref(Some("not-a-proof"), &binding, &issuer_endpoint_id),
            MctPeerBindingSignatureVerification::Malformed
        );

        let mut tampered = binding.clone();
        tampered.policy_revision += 1;
        assert_eq!(
            verify_peer_binding_signature_ref(
                Some(signature_ref.as_str()),
                &tampered,
                &issuer_endpoint_id
            ),
            MctPeerBindingSignatureVerification::Invalid
        );
    }
}
