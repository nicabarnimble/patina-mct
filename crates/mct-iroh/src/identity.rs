use crate::endpoint::{MotherIrohEndpointError, MotherIrohEndpointResult};
use iroh::SecretKey;
use mct_kernel::EndpointIdText;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{fs::OpenOptions, io::Write, path::Path};

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
    let secret_key_hex = secret_key_to_hex(&SecretKey::generate());
    write_new_node_secret_key_file(path, &secret_key_hex)?;
    Ok(secret_key_hex)
}

fn write_new_node_secret_key_file(
    path: &Path,
    secret_key_hex: &str,
) -> MotherIrohEndpointResult<()> {
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
    for (index, chunk) in value.as_bytes().chunks(2).enumerate() {
        let high = decode_hex_nibble(chunk[0])?;
        let low = decode_hex_nibble(chunk[1])?;
        bytes[index] = (high << 4) | low;
    }
    Ok(bytes)
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
