//! Explicit signed administration. Never falls back to UID/offline legacy mutation.
use anyhow::{Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use mct_kernel::{Timestamp, admin_key::*, admin_sig::*};
use mct_observation::{
    JsonlObservationLedger, PasskeyGenesisSetupV1, load_verified_ledger_replay, private_fs,
};
use std::{
    collections::BTreeMap,
    io::Read as _,
    path::{Path, PathBuf},
};

fn read_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T> {
    let mut bytes = Vec::new();
    private_fs::read_file(Path::new(path))?
        .take(96 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 96 * 1024 {
        bail!("administrative input exceeds 96 KiB");
    }
    Ok(serde_json::from_slice(&bytes)?)
}
fn nonce() -> Result<String> {
    let mut random = [0; 32];
    getrandom::fill(&mut random).map_err(|e| anyhow!("randomness unavailable: {e}"))?;
    Ok(const_hex::encode(random))
}
fn stamp(t: jiff::Timestamp) -> Timestamp {
    Timestamp::new(t.to_string()).expect("system timestamp")
}
fn options(args: Vec<String>) -> Result<(String, BTreeMap<String, String>)> {
    let mut iter = args.into_iter();
    let command = iter.next().ok_or_else(|| {
        anyhow!("expected prepare-genesis, bootstrap, prepare, apply, day-keygen or sign-day")
    })?;
    let allowed: &[&str] = match command.as_str() {
        "day-keygen" => &["--secret-output"],
        "sign-day" => &["--intent", "--grant", "--secret"],
        "prepare-genesis" => &["--root", "--mother", "--ledger-id", "--registered-root"],
        "bootstrap" => &[
            "--root",
            "--mother",
            "--ledger-id",
            "--registered-root",
            "--request",
        ],
        "prepare" => &["--root", "--mother", "--ledger-id", "--action", "--signer"],
        "apply" => &["--root", "--mother", "--ledger-id", "--request"],
        _ => bail!("unknown passkey-admin command"),
    };
    let mut flags = BTreeMap::new();
    while let Some(flag) = iter.next() {
        if !allowed.contains(&flag.as_str()) {
            bail!("unknown option {flag}");
        }
        let value = iter
            .next()
            .ok_or_else(|| anyhow!("missing value for {flag}"))?;
        if flags.insert(flag.clone(), value).is_some() {
            bail!("duplicate option {flag}");
        }
    }
    for flag in allowed {
        if !flags.contains_key(*flag) {
            bail!("missing {flag}");
        }
    }
    Ok((command, flags))
}
pub(super) fn run_passkey_admin(args: Vec<String>) -> Result<()> {
    let (command, flags) = options(args)?;
    if command == "day-keygen" {
        let path = Path::new(&flags["--secret-output"]);
        if path.try_exists()? {
            bail!("refusing to replace an existing day-key secret");
        }
        let mut seed = [0; 32];
        getrandom::fill(&mut seed).map_err(|e| anyhow!("randomness unavailable: {e}"))?;
        let key = ed25519_dalek::SigningKey::from_bytes(&seed);
        private_fs::write_atomic(
            path,
            serde_json::to_string(&const_hex::encode(seed))?.as_bytes(),
        )?;
        println!(
            "{}",
            serde_json::to_string(
                &AdminKeyPublicHalf::ed25519_from_bytes(key.verifying_key().as_bytes())
                    .map_err(|e| anyhow!(e))?
            )?
        );
        return Ok(());
    }
    if command == "sign-day" {
        use ed25519_dalek::Signer as _;
        let i: PasskeyAdminIntentV1 = read_json(&flags["--intent"])?;
        let secret: String = read_json(&flags["--secret"])?;
        let seed: [u8; 32] = const_hex::decode(secret)
            .map_err(|_| anyhow!("invalid day-key secret encoding"))?
            .try_into()
            .map_err(|_| anyhow!("invalid day-key secret"))?;
        let key = ed25519_dalek::SigningKey::from_bytes(&seed);
        let id = &flags["--grant"];
        let signature = key.sign(&day_key_signing_message(&i, &format!("ed25519:{id}"))?);
        let request = PasskeyAdminRequestV1 {
            intent: i,
            proof: DayKeyAdminProofV1::Ed25519DayKeyV1 {
                grant_id: id.clone(),
                signature: const_hex::encode(signature.to_bytes()),
            },
        };
        println!("{}", serde_json::to_string_pretty(&request)?);
        return Ok(());
    }
    let root = PathBuf::from(&flags["--root"]);
    let mother = &flags["--mother"];
    let ledger_id = &flags["--ledger-id"];
    let paths = mct_daemon::MctStartupPaths::supervised(&root, root.join("supervisor.plist"));
    if command == "prepare-genesis" || command == "bootstrap" {
        // Pin is a separate operator-installed file, not extracted from request.
        let registered: WebAuthnP256DayKeyV1 = read_json(&flags["--registered-root"])?;
        registered.validate()?;
        let startup = mct_daemon::classify_authority_startup(&paths, ledger_id, mother)?;
        let evidence = startup.tenure_evidence(None)?;
        let setup = PasskeyGenesisSetupV1::from_registered_passkey(
            mother.clone(),
            ledger_id.clone(),
            registered.clone(),
            &evidence,
        )?;
        if command == "prepare-genesis" {
            let time = jiff::Timestamp::now();
            let intent = PasskeyAdminIntentV1 {
                mother_node_id: mother.clone(),
                ledger_id: ledger_id.clone(),
                epoch: format!("{}{}", mct_observation::AUTHORITY_EPOCH_PREFIX_V1, nonce()?),
                generation: 0,
                authority_observation_id: String::new(),
                nonce: nonce()?,
                issued_at: stamp(time),
                expires_at: stamp(time.checked_add(std::time::Duration::from_secs(300))?),
                action: PasskeyAdminActionV1::Bootstrap {
                    root: registered.clone(),
                    inventory_hash: evidence.inventory_hash,
                },
            };
            print_prepared(intent, &registered, "root")?;
        } else {
            let request = read_json(&flags["--request"])?;
            let mut writer = JsonlObservationLedger::open(&paths.ledger, ledger_id, mother)?;
            let entry = writer.execute_passkey_genesis(&setup, request)?;
            println!("{}", serde_json::to_string_pretty(&entry)?);
        }
        return Ok(());
    }
    if command == "prepare" {
        let verified = load_verified_ledger_replay(&paths.ledger, ledger_id, mother)?;
        let state = verified
            .replay
            .state
            .passkey_admin
            .as_ref()
            .ok_or_else(|| anyhow!("no passkey genesis; legacy authority cannot promote"))?;
        let authority = verified
            .replay
            .current_authority
            .as_ref()
            .ok_or_else(|| anyhow!("no authority"))?;
        let time = jiff::Timestamp::now();
        let action = read_json(&flags["--action"])?;
        if matches!(action, PasskeyAdminActionV1::Bootstrap { .. }) {
            bail!("genesis cannot be replaced");
        }
        let intent = PasskeyAdminIntentV1 {
            mother_node_id: mother.clone(),
            ledger_id: ledger_id.clone(),
            epoch: authority.authority_epoch.clone(),
            generation: authority.generation,
            authority_observation_id: authority.source_authority_observation_id.clone(),
            nonce: nonce()?,
            issued_at: stamp(time),
            expires_at: stamp(time.checked_add(std::time::Duration::from_secs(300))?),
            action,
        };
        print_prepared(intent, &state.state.root, &flags["--signer"])?;
        return Ok(());
    }
    let request: PasskeyAdminRequestV1 = read_json(&flags["--request"])?;
    // Do not begin a new epoch between preparing and consuming the signed intent.
    // The exclusive writer and canonical snapshot serialize these one-shot calls.
    let mut writer = JsonlObservationLedger::open(&paths.ledger, ledger_id, mother)?;
    if matches!(request.intent.action, PasskeyAdminActionV1::Command { .. }) {
        let status = serde_json::to_string_pretty(&writer.verified_replay()?.state.passkey_admin)?;
        let status = writer.execute_passkey_command(
            request,
            "/status",
            AdminKeyScopeV1::Status,
            |body| {
                if !body.is_empty() {
                    return Err(mct_observation::ObservationLedgerError::AuthorityReplay {
                        detail: "status body must be empty".into(),
                    });
                }
                Ok(status)
            },
        )?;
        println!("{status}");
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(&writer.execute_passkey_admin(request)?)?
        );
    }
    Ok(())
}
fn print_prepared(
    intent: PasskeyAdminIntentV1,
    root: &WebAuthnP256DayKeyV1,
    signer: &str,
) -> Result<()> {
    let result = if signer == "root" {
        serde_json::json!({"challenge":webauthn_admin_challenge(&intent, &root.credential_id)?, "root":root, "intent":intent})
    } else {
        serde_json::json!({"signing_message":URL_SAFE_NO_PAD.encode(day_key_signing_message(&intent, &format!("ed25519:{signer}"))?), "intent":intent})
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn passkey_cli_requires_explicit_trust_and_rejects_unknown_duplicate_options() {
        assert!(
            options(vec![
                "bootstrap".into(),
                "--request".into(),
                "caller.json".into()
            ])
            .is_err()
        );
        assert!(options(vec!["apply".into(), "--uid".into(), "0".into()]).is_err());
        assert!(
            options(vec![
                "day-keygen".into(),
                "--secret-output".into(),
                "a".into(),
                "--secret-output".into(),
                "b".into()
            ])
            .is_err()
        );
    }
}
