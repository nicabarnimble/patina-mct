//! NIC-126: one resident Mother boots on a passkey-rooted ledger, runs a Child
//! approved only by signed passkey commands, and refuses Unix-socket mutation.
use base64::{
    Engine as _, engine::general_purpose::STANDARD, engine::general_purpose::URL_SAFE_NO_PAD,
};
use mct_kernel::admin_key::WebAuthnP256DayKeyV1;
use mct_observation::private_fs;
use p256::ecdsa::signature::Signer as _;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::io::{Read as _, Write as _};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mct-daemon"))
        .args(args)
        .output()
        .unwrap()
}
fn ok(args: &[&str]) -> Value {
    let out = cli(args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn put(path: &Path, value: &impl serde::Serialize) {
    private_fs::write_atomic(path, &serde_json::to_vec(value).unwrap()).unwrap();
}
fn root_key() -> p256::ecdsa::SigningKey {
    p256::ecdsa::SigningKey::from_slice(&[7; 32]).unwrap()
}
fn sign_root(prepared: Value) -> Value {
    let mut auth = Sha256::digest(b"admin.example.test").to_vec();
    auth.push(5);
    auth.extend([0; 4]);
    let client = serde_json::to_vec(&json!({"type":"webauthn.get","origin":"https://admin.example.test","challenge":prepared["challenge"],"crossOrigin":false})).unwrap();
    let mut signed = auth.clone();
    signed.extend(Sha256::digest(&client));
    let sig: p256::ecdsa::Signature = root_key().sign(&signed);
    json!({"intent":prepared["intent"],"proof":{"scheme":"webauthn-p256-day-key/v1","credential_id":prepared["root"]["credential_id"],"authenticator_data":URL_SAFE_NO_PAD.encode(auth),"client_data_json":URL_SAFE_NO_PAD.encode(client),"signature":URL_SAFE_NO_PAD.encode(sig.to_der().as_bytes())}})
}
fn plus_secs(secs: i64) -> String {
    jiff::Timestamp::now()
        .checked_add(jiff::SignedDuration::from_secs(secs))
        .unwrap()
        .to_string()
}
fn uds(socket: &Path, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let body = body.map_or_else(Vec::new, |value| serde_json::to_vec(value).unwrap());
    let mut stream = UnixStream::connect(socket).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: local\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .unwrap();
    stream.write_all(&body).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let status = response.split_whitespace().nth(1).unwrap().parse().unwrap();
    let body = response.split_once("\r\n\r\n").map_or("", |(_, body)| body);
    (status, serde_json::from_str(body).unwrap_or(Value::Null))
}

struct Mother(Child);
impl Drop for Mother {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Governed {
    root: PathBuf,
    work: PathBuf,
    nonce: usize,
}
impl Governed {
    fn common(&self) -> Vec<String> {
        [
            "--root",
            self.root.to_str().unwrap(),
            "--mother",
            "local-mct",
            "--ledger-id",
            "ledger-local",
        ]
        .map(str::to_owned)
        .to_vec()
    }
    fn file(&mut self, value: &Value) -> String {
        self.nonce += 1;
        let path = self.work.join(format!("request-{}.json", self.nonce));
        put(&path, value);
        path.to_str().unwrap().to_owned()
    }
    fn apply(&mut self, request: &Value) -> Value {
        let request = self.file(request);
        let args = [
            &["passkey-admin".to_owned(), "apply".to_owned()][..],
            &self.common(),
            &["--request".to_owned(), request],
        ]
        .concat();
        ok(&args.iter().map(String::as_str).collect::<Vec<_>>())
    }
    fn prepare(&mut self, action: &Value, signer: &str) -> Value {
        let action = self.file(action);
        let args = [
            &["passkey-admin".to_owned(), "prepare".to_owned()][..],
            &self.common(),
            &[
                "--action".to_owned(),
                action,
                "--signer".to_owned(),
                signer.to_owned(),
            ],
        ]
        .concat();
        ok(&args.iter().map(String::as_str).collect::<Vec<_>>())
    }
    fn command(&mut self, scope: &str, route: &str, body: &Value) -> Value {
        let action =
            json!({"action":"command","scope":scope,"route":route,"body":body.to_string()});
        let prepared = self.prepare(&action, "ops");
        let intent = self.file(&prepared["intent"]);
        let secret = self.work.join("ops-secret.json");
        let signed = ok(&[
            "passkey-admin",
            "sign-day",
            "--intent",
            &intent,
            "--grant",
            "ops",
            "--secret",
            secret.to_str().unwrap(),
        ]);
        self.apply(&signed)
    }
}

fn write_child_source(dir: &Path) -> (String, String) {
    std::fs::create_dir_all(dir).unwrap();
    let wasm = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/mct-test-echo-0.1.0/mct-test-echo.wasm"
    ))
    .unwrap();
    let manifest = "[child]\nname = \"resident-echo\"\nversion = \"0.1.0\"\nkind = \"child\"\nrole = \"test\"\n\n[child.ingress]\nmode = \"wit-only\"\n\n[child.artifact]\nwasm = \"resident-echo.wasm\"\n\n[child.contract]\ndefault = \"patina:demo/control@0.1.0.run\"\nallow = [\"patina:demo/control@0.1.0.run\"]\n\n[needs]\ntoys = []\n";
    std::fs::write(dir.join("resident-echo.wasm"), &wasm).unwrap();
    std::fs::write(dir.join("child.toml"), manifest).unwrap();
    (
        blake3::hash(&wasm).to_hex().to_string(),
        format!("sha256:{:x}", Sha256::digest(manifest.as_bytes())),
    )
}

fn call(socket: &Path, authority: &Value, call_id: &str) -> Value {
    let payload = b"[]";
    let (status, reply) = uds(
        socket,
        "POST",
        "/calls",
        Some(&json!({
            "protocol_request_id": format!("proto-{call_id}"), "call_id": call_id,
            "target": {"namespace": "patina:demo", "interface_name": "control@0.1.0", "function_name": "run"},
            "payload_metadata": {"data_classification": "public", "size_bytes": payload.len(), "contains_secret_scoped_material": false},
            "authority_context": {"policy_revision": 1, "expected_receiver_grants_authority": authority, "vision_policy_revision": 1},
            "deadline": plus_secs(120),
            "trace_context": {"trace_id": format!("trace-{call_id}"), "span_id": format!("span-{call_id}")},
            "payload": {"payload_kind": "inline_payload", "inline_payload_ref": format!("payload-{call_id}"), "content_type": "application/json", "size_bytes": payload.len(), "blake3_digest_hex": blake3::hash(payload).to_hex().to_string()},
            "inline_payload_base64": STANDARD.encode(payload)
        })),
    );
    assert_eq!(status, 200, "{reply:#}");
    reply
}

#[test]
fn done_c2_governed_mother_boots_runs_signed_child_and_refuses_unix_socket_mutation() {
    let dir = tempfile::tempdir().unwrap();
    let work = dir.path().join("work");
    let root = dir.path().join("root");
    private_fs::ensure_directory(&work).unwrap();
    private_fs::ensure_directory(&root).unwrap();
    let mut governed = Governed {
        root: root.clone(),
        work: work.clone(),
        nonce: 0,
    };

    let pin = work.join("pin.json");
    put(
        &pin,
        &WebAuthnP256DayKeyV1 {
            credential_id: URL_SAFE_NO_PAD.encode([8; 32]),
            public_key_sec1: const_hex::encode(root_key().verifying_key().to_sec1_bytes()),
            rp_id: "admin.example.test".into(),
            origin: "https://admin.example.test".into(),
        },
    );
    let pin_args = [
        "--registered-root".to_owned(),
        pin.to_str().unwrap().to_owned(),
    ];
    let genesis = [
        &["passkey-admin".to_owned(), "prepare-genesis".to_owned()][..],
        &governed.common(),
        &pin_args,
    ]
    .concat();
    let prepared = ok(&genesis.iter().map(String::as_str).collect::<Vec<_>>());
    let signed = governed.file(&sign_root(prepared));
    let bootstrap = [
        &["passkey-admin".to_owned(), "bootstrap".to_owned()][..],
        &governed.common(),
        &pin_args,
        &["--request".to_owned(), signed],
    ]
    .concat();
    ok(&bootstrap.iter().map(String::as_str).collect::<Vec<_>>());

    let secret = work.join("ops-secret.json");
    let public_half = ok(&[
        "passkey-admin",
        "day-keygen",
        "--secret-output",
        secret.to_str().unwrap(),
    ]);
    let issue = json!({"action":"issue_day_key","grant_id":"ops","public_half":public_half,
        "scopes":["approve_child","artifact","peer_add","peer_revoke","revoke_child","watch_grant","watch_revoke"],
        "expires_at": plus_secs(3600)});
    let prepared = governed.prepare(&issue, "root");
    governed.apply(&sign_root(prepared));

    let children = root.join("children");
    let source = work.join("source");
    let (blake3_digest, manifest_hash) = write_child_source(&source);
    let staged = governed.command("artifact", "/artifacts/stage", &json!({
        "source_root": source, "manifest_path": "child.toml", "component_path": "resident-echo.wasm",
        "claimed_child_name": "resident-echo", "claimed_artifact_version": "0.1.0",
        "expected_digest": format!("blake3:{blake3_digest}"), "expected_manifest_digest": manifest_hash,
        "standing_source_authority_id": null, "claimed_publisher": null, "require_source_sidecars": false,
        "children_dir": children, "state_path": root.join("state.sqlite")
    }));
    assert_eq!(staged["verification_outcome"], "verified", "{staged:#}");
    let artifact_id = staged["artifact_id"].as_str().unwrap().to_owned();
    // Approval pins the installed manifest, which staging canonicalizes.
    let installed = children
        .join("artifacts/sha256")
        .join(artifact_id.trim_start_matches("sha256:"))
        .join("child.toml");
    let manifest_hash = format!(
        "sha256:{:x}",
        Sha256::digest(std::fs::read(installed).unwrap())
    );
    governed.command(
        "peer_add",
        "/peers/add",
        &json!({
            "peer_node_id": "mother-b", "binding_id": "binding-b", "endpoint_id": "endpoint-b",
            "vision_id": "vision-local", "policy_revision": 1, "expires_at": plus_secs(3600)
        }),
    );
    let approved = governed.command(
        "approve_child",
        "/children/approve",
        &json!({
            "child_name": "resident-echo", "artifact_id": artifact_id, "artifact_version": "0.1.0",
            "manifest_hash": manifest_hash, "vision_id": "vision-local", "policy_revision": 1
        }),
    );
    assert_eq!(approved["children"], 1, "{approved:#}");

    let socket = root.join("control.sock");
    let log = std::fs::File::create(work.join("mother.log")).unwrap();
    let _mother = Mother(
        Command::new(env!("CARGO_BIN_EXE_mct-daemon"))
            .arg("serve")
            .args([
                "--identity",
                root.join("identity/iroh-secret.hex").to_str().unwrap(),
            ])
            .args(["--config", root.join("config.json").to_str().unwrap()])
            .args(["--children-dir", children.to_str().unwrap()])
            .args(["--state", root.join("state.sqlite").to_str().unwrap()])
            .args([
                "--ledger",
                root.join("observations.jsonl").to_str().unwrap(),
            ])
            .args(["--uds", socket.to_str().unwrap()])
            .current_dir(&work)
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if socket.exists() && uds(&socket, "GET", "/status", None).1["readiness"] == "ready" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "governed Mother never became ready:\n{}",
            std::fs::read_to_string(work.join("mother.log")).unwrap_or_default()
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    let config: Value =
        serde_json::from_slice(&std::fs::read(root.join("config.json")).unwrap()).unwrap();
    let peers = config["peers"].as_object().unwrap();
    assert_eq!(peers.len(), 1, "{config:#}");
    assert_eq!(peers["mother-b"]["binding_state"], "admitted");
    assert!(
        peers["mother-b"]["binding_signature_ref"].is_string(),
        "{config:#}"
    );
    assert_eq!(
        config["child_approvals"]["resident-echo"]["artifact_id"],
        artifact_id
    );

    let (_, authority) = uds(&socket, "GET", "/receiver-authority", None);
    let completed = call(&socket, &authority, "governed-call-1");
    assert_eq!(completed["outcome"], "completed", "{completed:#}");

    let (status, refused) = uds(
        &socket,
        "POST",
        "/children/approve",
        Some(&json!({"child_name": "resident-echo"})),
    );
    assert_eq!(status, 403, "{refused:#}");
    assert_eq!(refused["reason"], "governed_unix_socket_mutation_refused");
}
