//! Real CLI processes, real ES256/Ed25519 signatures, private files and restart.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use mct_kernel::admin_key::WebAuthnP256DayKeyV1;
use mct_observation::private_fs;
use p256::ecdsa::signature::Signer as _;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::{
    path::Path,
    process::{Command, Output},
};

fn call(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mct-daemon"))
        .arg("passkey-admin")
        .args(args)
        .output()
        .unwrap()
}
fn ok(args: &[&str]) -> Value {
    let out = call(args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn put(path: &Path, value: &impl serde::Serialize) {
    private_fs::write_atomic(path, &serde_json::to_vec(value).unwrap()).unwrap();
}
fn sign_root(prepared: Value, key: &p256::ecdsa::SigningKey) -> Value {
    let mut auth = Sha256::digest(b"admin.example.test").to_vec();
    auth.push(5);
    auth.extend([0; 4]);
    let client = serde_json::to_vec(&json!({"type":"webauthn.get","origin":"https://admin.example.test","challenge":prepared["challenge"],"crossOrigin":false})).unwrap();
    let mut signed = auth.clone();
    signed.extend(Sha256::digest(&client));
    let sig: p256::ecdsa::Signature = key.sign(&signed);
    json!({"intent":prepared["intent"],"proof":{"scheme":"webauthn-p256-day-key/v1","credential_id":prepared["root"]["credential_id"],"authenticator_data":URL_SAFE_NO_PAD.encode(auth),"client_data_json":URL_SAFE_NO_PAD.encode(client),"signature":URL_SAFE_NO_PAD.encode(sig.to_der().as_bytes())}})
}

#[test]
fn passkey_cli_bootstrap_issue_sign_status_revoke_and_legacy_denial() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("mother");
    private_fs::ensure_directory(&root).unwrap();
    let pin = d.path().join("pin.json");
    let request = d.path().join("request.json");
    let action = d.path().join("action.json");
    let secret = d.path().join("secret.json");
    let intent = d.path().join("intent.json");
    let key = p256::ecdsa::SigningKey::from_slice(&[7; 32]).unwrap();
    let registered = WebAuthnP256DayKeyV1 {
        credential_id: URL_SAFE_NO_PAD.encode([8; 32]),
        public_key_sec1: const_hex::encode(key.verifying_key().to_sec1_bytes()),
        rp_id: "admin.example.test".into(),
        origin: "https://admin.example.test".into(),
    };
    put(&pin, &registered);
    let common = [
        "--root",
        root.to_str().unwrap(),
        "--mother",
        "mother-a",
        "--ledger-id",
        "ledger-a",
    ];
    let root_args = ["--registered-root", pin.to_str().unwrap()];
    let request_args = ["--request", request.to_str().unwrap()];
    let prepared = ok(&[&["prepare-genesis"][..], &common, &root_args].concat());
    put(&request, &sign_root(prepared, &key));
    ok(&[&["bootstrap"][..], &common, &root_args, &request_args].concat());
    let paths = mct_daemon::MctStartupPaths::supervised(&root, root.join("supervisor.plist"));
    assert!(mct_daemon::classify_authority_startup(&paths, "ledger-a", "mother-a").is_err());
    let ledger = mct_observation::JsonlObservationLedger::open_read_only(
        &paths.ledger,
        "ledger-a",
        "mother-a",
    )
    .unwrap();
    let store = mct_daemon::MctRuntimeStateStore::open(d.path().join("projection.sqlite")).unwrap();
    assert!(
        store
            .rebuild_authority_projection(&ledger.entries().unwrap())
            .is_err()
    );
    let public = ok(&["day-keygen", "--secret-output", secret.to_str().unwrap()]);
    assert!(
        !call(&["day-keygen", "--secret-output", secret.to_str().unwrap()])
            .status
            .success()
    );
    let expiry = jiff::Timestamp::now()
        .checked_add(std::time::Duration::from_secs(3600))
        .unwrap()
        .to_string();
    put(
        &action,
        &json!({"action":"issue_day_key","grant_id":"day","public_half":public,"scopes":["status"],"expires_at":expiry}),
    );
    let prepared = ok(&[
        &["prepare"][..],
        &common,
        &["--action", action.to_str().unwrap(), "--signer", "root"],
    ]
    .concat());
    put(&request, &sign_root(prepared, &key));
    ok(&[&["apply"][..], &common, &request_args].concat());
    for revoked in [false, true] {
        put(
            &action,
            &json!({"action":"command","scope":"status","route":"/status","body":""}),
        );
        let prepared = ok(&[
            &["prepare"][..],
            &common,
            &["--action", action.to_str().unwrap(), "--signer", "day"],
        ]
        .concat());
        put(&intent, &prepared["intent"]);
        let signed = ok(&[
            "sign-day",
            "--intent",
            intent.to_str().unwrap(),
            "--grant",
            "day",
            "--secret",
            secret.to_str().unwrap(),
        ]);
        put(&request, &signed);
        let result = call(&[&["apply"][..], &common, &request_args].concat());
        assert_eq!(
            result.status.success(),
            !revoked,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        if !revoked {
            put(
                &action,
                &json!({"action":"revoke_day_key","grant_id":"day"}),
            );
            let prepared = ok(&[
                &["prepare"][..],
                &common,
                &["--action", action.to_str().unwrap(), "--signer", "root"],
            ]
            .concat());
            put(&request, &sign_root(prepared, &key));
            ok(&[&["apply"][..], &common, &request_args].concat());
        }
    }
}
