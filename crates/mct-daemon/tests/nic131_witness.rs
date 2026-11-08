//! NIC-131 witness. Drives the production daemon and `project_governed_config`.
//! Classifies source-derived candidates. It does not change their outcome to pass.
use base64::{
    Engine as _, engine::general_purpose::STANDARD, engine::general_purpose::URL_SAFE_NO_PAD,
};
use mct_kernel::admin_key::WebAuthnP256DayKeyV1;
use mct_kernel::{EndpointIdText, MctNodeId, PeerBindingId, Timestamp, VisionId};
use mct_observation::private_fs;
use mct_observation::{GovernedChildStateV1, GovernedPeerV1, GovernedRuntimeV1};
use p256::ecdsa::signature::Signer as _;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::io::{Read as _, Write as _};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

fn daemon() -> PathBuf {
    std::env::var_os("NIC131_DAEMON")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_mct-daemon")))
}

fn echo_wasm() -> Vec<u8> {
    let path = std::env::var_os("NIC131_ECHO_WASM")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/mct-test-echo-0.1.0/mct-test-echo.wasm"
            ))
        });
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

fn publish(report: &Value) {
    println!("NIC131_REPORT {report}");
    if let Some(dir) = std::env::var_os("NIC131_REPORT_DIR") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let name = report["candidate"].as_str().unwrap_or("witness");
        std::fs::write(
            dir.join(format!("{name}.json")),
            serde_json::to_vec_pretty(report).unwrap(),
        )
        .unwrap();
    }
}

fn verdict(candidate: &str, kind: &str, reason: &str, observations: Value) -> Value {
    json!({
        "candidate": candidate,
        "verdict": kind,
        "reason": reason,
        "observations": observations,
        "pin": "abc9c30465533622338ed0a9d47f0949291af640",
    })
}

fn assert_classified(report: &Value) {
    assert!(
        matches!(
            report["verdict"].as_str(),
            Some("confirmed" | "refuted" | "inconclusive")
        ),
        "{report}"
    );
    assert!(
        report["reason"]
            .as_str()
            .is_some_and(|reason| !reason.is_empty())
    );
}

fn run(args: &[String], timeout_secs: u64) -> Output {
    Command::new("timeout")
        .arg("--signal=KILL")
        .arg(timeout_secs.to_string())
        .arg(daemon())
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("spawn {}: {error}", daemon().display()))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn json_stdout(output: &Output) -> Option<Value> {
    serde_json::from_slice(&output.stdout).ok()
}

fn plus_secs(secs: i64) -> String {
    jiff::Timestamp::now()
        .checked_add(jiff::SignedDuration::from_secs(secs))
        .unwrap()
        .to_string()
}

fn put(path: &Path, value: &impl serde::Serialize) {
    private_fs::write_atomic(path, &serde_json::to_vec(value).unwrap()).unwrap();
}

fn sign_root(prepared: &Value, secret: &[u8; 32]) -> Value {
    let key = p256::ecdsa::SigningKey::from_slice(secret).unwrap();
    let mut auth = Sha256::digest(b"admin.example.test").to_vec();
    auth.push(5);
    auth.extend([0; 4]);
    let client = serde_json::to_vec(&json!({
        "type": "webauthn.get",
        "origin": "https://admin.example.test",
        "challenge": prepared["challenge"],
        "crossOrigin": false
    }))
    .unwrap();
    let mut signed = auth.clone();
    signed.extend(Sha256::digest(&client));
    let signature: p256::ecdsa::Signature = key.sign(&signed);
    json!({
        "intent": prepared["intent"],
        "proof": {
            "scheme": "webauthn-p256-day-key/v1",
            "credential_id": prepared["root"]["credential_id"],
            "authenticator_data": URL_SAFE_NO_PAD.encode(auth),
            "client_data_json": URL_SAFE_NO_PAD.encode(client),
            "signature": URL_SAFE_NO_PAD.encode(signature.to_der().as_bytes())
        }
    })
}

struct MotherProc(Child);
impl Drop for MotherProc {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Mother {
    name: String,
    root: PathBuf,
    work: PathBuf,
    config: PathBuf,
    socket: PathBuf,
    secret: [u8; 32],
    nonce: usize,
}

impl Mother {
    fn new(dir: &Path, name: &str, secret: [u8; 32]) -> Self {
        let root = dir.join(name);
        let work = dir.join(format!("{name}-work"));
        let config_dir = dir.join(format!("{name}-cfg"));
        private_fs::ensure_directory(&root).unwrap();
        private_fs::ensure_directory(&work).unwrap();
        private_fs::ensure_directory(&config_dir).unwrap();
        Self {
            name: name.to_owned(),
            config: config_dir.join("config.json"),
            socket: root.join("control.sock"),
            root,
            work,
            secret,
            nonce: 0,
        }
    }

    fn file(&mut self, value: &Value) -> String {
        self.nonce += 1;
        let path = self.work.join(format!("request-{}.json", self.nonce));
        put(&path, value);
        path.to_str().unwrap().to_owned()
    }

    fn common(&self) -> Vec<String> {
        vec![
            "--root".into(),
            self.root.to_str().unwrap().into(),
            "--mother".into(),
            self.name.clone(),
            "--ledger-id".into(),
            "ledger-local".into(),
        ]
    }

    fn passkey(&mut self, extra: &[String]) -> Output {
        let mut args = vec!["passkey-admin".into()];
        args.extend(extra.iter().cloned());
        args.extend(self.common());
        run(&args, 60)
    }

    fn passkey_ok(&mut self, extra: &[String]) -> Value {
        let output = self.passkey(extra);
        assert!(
            output.status.success(),
            "{} {extra:?}\n{}",
            self.name,
            text(&output.stderr)
        );
        json_stdout(&output).unwrap_or(Value::Null)
    }

    fn prepare(&mut self, action: &Value, signer: &str) -> Value {
        let action = self.file(action);
        self.passkey_ok(&[
            "prepare".into(),
            "--action".into(),
            action,
            "--signer".into(),
            signer.into(),
        ])
    }

    fn sign_day(&mut self, prepared: &Value) -> Value {
        let intent = self.file(&prepared["intent"]);
        let secret = self.work.join("ops-secret.json");
        let output = run(
            &[
                "passkey-admin".into(),
                "sign-day".into(),
                "--intent".into(),
                intent,
                "--grant".into(),
                "ops".into(),
                "--secret".into(),
                secret.to_str().unwrap().into(),
            ],
            30,
        );
        assert!(output.status.success(), "{}", text(&output.stderr));
        json_stdout(&output).unwrap()
    }

    fn apply_offline(&mut self, request: &Value) -> Value {
        let request = self.file(request);
        self.passkey_ok(&["apply".into(), "--request".into(), request])
    }

    fn command_offline(&mut self, scope: &str, route: &str, body: &Value) -> Value {
        let prepared = self.prepare(
            &json!({"action":"command","scope":scope,"route":route,"body":body.to_string()}),
            "ops",
        );
        let signed = self.sign_day(&prepared);
        self.apply_offline(&signed)
    }

    fn bootstrap(&mut self) {
        let pin = self.work.join("pin.json");
        let key = p256::ecdsa::SigningKey::from_slice(&self.secret).unwrap();
        put(
            &pin,
            &WebAuthnP256DayKeyV1 {
                credential_id: URL_SAFE_NO_PAD.encode(self.secret),
                public_key_sec1: const_hex::encode(key.verifying_key().to_sec1_bytes()),
                rp_id: "admin.example.test".into(),
                origin: "https://admin.example.test".into(),
            },
        );
        let pin = pin.to_str().unwrap().to_owned();
        let prepared = self.passkey_ok(&[
            "prepare-genesis".into(),
            "--registered-root".into(),
            pin.clone(),
        ]);
        let signed = self.file(&sign_root(&prepared, &self.secret));
        self.passkey_ok(&[
            "bootstrap".into(),
            "--registered-root".into(),
            pin,
            "--request".into(),
            signed,
        ]);
        let secret = self.work.join("ops-secret.json");
        let public_half = run(
            &[
                "passkey-admin".into(),
                "day-keygen".into(),
                "--secret-output".into(),
                secret.to_str().unwrap().into(),
            ],
            30,
        );
        assert!(
            public_half.status.success(),
            "{}",
            text(&public_half.stderr)
        );
        let public_half = json_stdout(&public_half).unwrap();
        let prepared = self.prepare(
            &json!({
                "action": "issue_day_key",
                "grant_id": "ops",
                "public_half": public_half,
                "scopes": ["approve_child", "artifact", "peer_add", "peer_revoke", "revoke_child", "watch_grant", "watch_revoke"],
                "expires_at": plus_secs(3600)
            }),
            "root",
        );
        self.apply_offline(&sign_root(&prepared, &self.secret));
    }

    fn serve(&self) -> MotherProc {
        let log = std::fs::File::create(self.work.join("mother.log")).unwrap();
        MotherProc(
            Command::new(daemon())
                .arg("serve")
                .args([
                    "--identity",
                    self.root.join("identity/iroh-secret.hex").to_str().unwrap(),
                ])
                .args(["--config", self.config.to_str().unwrap()])
                .args([
                    "--children-dir",
                    self.root.join("children").to_str().unwrap(),
                ])
                .args(["--state", self.root.join("state.sqlite").to_str().unwrap()])
                .args([
                    "--ledger",
                    self.root.join("observations.jsonl").to_str().unwrap(),
                ])
                .args(["--uds", self.socket.to_str().unwrap()])
                .args(["--node-id", self.name.as_str()])
                .args(["--vision", "vision-local"])
                .args(["--bind", "127.0.0.1:0"])
                .args(["--max-connections", "4"])
                .current_dir(&self.work)
                .env(
                    "NIC131_CLOCK_FILE",
                    self.work.join("clock.txt").to_str().unwrap(),
                )
                .stdin(Stdio::null())
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .spawn()
                .unwrap(),
        )
    }

    fn wait_ready(&self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            if self.socket.exists()
                && uds(&self.socket, "GET", "/status", None).1["readiness"] == "ready"
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(
                    std::fs::read_to_string(self.work.join("mother.log")).unwrap_or_default()
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn ticket(&self) -> Value {
        let log = std::fs::read_to_string(self.work.join("mother.log")).unwrap_or_default();
        let line = log
            .lines()
            .find_map(|line| line.strip_prefix("ticket="))
            .unwrap_or("");
        serde_json::from_str(line).unwrap_or(Value::Null)
    }

    fn send_iroh(&mut self, request: &Value) -> Output {
        let ticket = self.work.join("resident-ticket.json");
        put(&ticket, &self.ticket());
        let request = self.file(request);
        run(
            &[
                "passkey-admin".into(),
                "send-iroh".into(),
                "--ticket".into(),
                ticket.to_str().unwrap().into(),
                "--request".into(),
                request,
            ],
            40,
        )
    }

    fn signed_command(&mut self, scope: &str, route: &str, body: &Value) -> Value {
        let prepared = self.prepare(
            &json!({"action":"command","scope":scope,"route":route,"body":body.to_string()}),
            "ops",
        );
        self.sign_day(&prepared)
    }

    fn config(&self) -> Value {
        serde_json::from_slice(&std::fs::read(&self.config).unwrap_or_default())
            .unwrap_or(Value::Null)
    }
}

fn uds(socket: &Path, method: &str, path: &str, body: Option<&Value>) -> (u16, Value) {
    let body = body.map_or_else(Vec::new, |value| serde_json::to_vec(value).unwrap());
    let mut stream = match UnixStream::connect(socket) {
        Ok(stream) => stream,
        Err(error) => return (0, json!({"connect": error.to_string()})),
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(60)))
        .unwrap();
    let _ = write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: local\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(&body);
    let mut response = String::new();
    let _ = stream.read_to_string(&mut response);
    let status = response
        .split_whitespace()
        .nth(1)
        .and_then(|status| status.parse().ok())
        .unwrap_or(0);
    let body = response.split_once("\r\n\r\n").map_or("", |(_, body)| body);
    (
        status,
        serde_json::from_str(body).unwrap_or(Value::String(body.to_owned())),
    )
}

fn peer_body(peer: &str, endpoint_id: &str, ticket: &Value) -> Value {
    json!({
        "peer_node_id": peer,
        "binding_id": format!("binding-{peer}"),
        "endpoint_id": endpoint_id,
        "vision_id": "vision-local",
        "ticket": ticket,
        "policy_revision": 1,
        "expires_at": plus_secs(3600)
    })
}

fn stage_echo(mother: &mut Mother) -> String {
    let source = mother.work.join("source");
    std::fs::create_dir_all(&source).unwrap();
    let wasm = echo_wasm();
    let manifest = "[child]\nname = \"resident-echo\"\nversion = \"0.1.0\"\nkind = \"child\"\nrole = \"test\"\n\n[child.ingress]\nmode = \"wit-only\"\n\n[child.artifact]\nwasm = \"resident-echo.wasm\"\n\n[child.contract]\ndefault = \"patina:demo/control@0.1.0.run\"\nallow = [\"patina:demo/control@0.1.0.run\"]\n\n[needs]\ntoys = []\n";
    std::fs::write(source.join("resident-echo.wasm"), &wasm).unwrap();
    std::fs::write(source.join("child.toml"), manifest).unwrap();
    let digest = blake3::hash(&wasm).to_hex().to_string();
    let manifest_hash = format!("sha256:{:x}", Sha256::digest(manifest.as_bytes()));
    let staged = mother.command_offline(
        "artifact",
        "/artifacts/stage",
        &json!({
            "source_root": source,
            "manifest_path": "child.toml",
            "component_path": "resident-echo.wasm",
            "claimed_child_name": "resident-echo",
            "claimed_artifact_version": "0.1.0",
            "expected_digest": format!("blake3:{digest}"),
            "expected_manifest_digest": manifest_hash,
            "standing_source_authority_id": null,
            "claimed_publisher": null,
            "require_source_sidecars": false,
            "children_dir": mother.root.join("children"),
            "state_path": mother.root.join("state.sqlite")
        }),
    );
    assert_eq!(staged["verification_outcome"], "verified", "{staged}");
    let artifact_id = staged["artifact_id"].as_str().unwrap().to_owned();
    let installed = mother
        .root
        .join("children/artifacts/sha256")
        .join(artifact_id.trim_start_matches("sha256:"))
        .join("child.toml");
    let manifest_hash = format!(
        "sha256:{:x}",
        Sha256::digest(std::fs::read(installed).unwrap())
    );
    let approved = mother.command_offline(
        "approve_child",
        "/children/approve",
        &json!({
            "child_name": "resident-echo",
            "artifact_id": artifact_id,
            "artifact_version": "0.1.0",
            "manifest_hash": manifest_hash,
            "vision_id": "vision-local",
            "policy_revision": 1
        }),
    );
    assert_eq!(approved["children"], 1, "{approved}");
    artifact_id
}

fn local_call(socket: &Path, authority: &Value, call_id: &str) -> (u16, Value) {
    let payload = b"[]";
    uds(
        socket,
        "POST",
        "/calls",
        Some(&json!({
            "protocol_request_id": format!("proto-{call_id}"),
            "call_id": call_id,
            "target": {"namespace": "patina:demo", "interface_name": "control@0.1.0", "function_name": "run"},
            "payload_metadata": {"data_classification": "public", "size_bytes": payload.len(), "contains_secret_scoped_material": false},
            "authority_context": {"policy_revision": 1, "expected_receiver_grants_authority": authority, "vision_policy_revision": 1},
            "deadline": plus_secs(120),
            "trace_context": {"trace_id": format!("trace-{call_id}"), "span_id": format!("span-{call_id}")},
            "payload": {"payload_kind": "inline_payload", "inline_payload_ref": format!("payload-{call_id}"), "content_type": "application/json", "size_bytes": payload.len(), "blake3_digest_hex": blake3::hash(payload).to_hex().to_string()},
            "inline_payload_base64": STANDARD.encode(payload)
        })),
    )
}

fn proof_present(peer: &Value) -> bool {
    match peer.get("outbound_binding") {
        None | Some(Value::Null) => false,
        Some(_) => true,
    }
}

fn runtime_peer(name: &str, state: GovernedChildStateV1) -> GovernedRuntimeV1 {
    let mut runtime = GovernedRuntimeV1::default();
    let peer = GovernedPeerV1 {
        peer_node_id: MctNodeId::new(name).unwrap(),
        binding_id: PeerBindingId::new(format!("binding-{name}")).unwrap(),
        endpoint_id: EndpointIdText::new("endpoint-p").unwrap(),
        vision_id: VisionId::new("vision-local").unwrap(),
        ticket: None,
        policy_revision: 1,
        expires_at: Timestamp::new(plus_secs(3600)).unwrap(),
        state,
        decided_by: "ops".into(),
        decided_at: Timestamp::new(plus_secs(-30)).unwrap(),
    };
    runtime.peers.insert(name.to_owned(), peer);
    runtime
}

fn binding_state(path: &Path, peer: &str) -> String {
    let config: Value =
        serde_json::from_slice(&std::fs::read(path).unwrap_or_default()).unwrap_or(Value::Null);
    config["peers"][peer]["binding_state"]
        .as_str()
        .unwrap_or("")
        .to_owned()
}

#[test]
fn nic131_g1b_older_snapshot_overwrites_newer_publication() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let path = dir.path().join("config.json");
    private_fs::write_atomic(&path, b"{}\n").expect("seed config");
    let newer = runtime_peer("peer-p", GovernedChildStateV1::Revoked);
    let older = runtime_peer("peer-p", GovernedChildStateV1::Approved);
    let newer_result = mct_daemon::project_governed_config(&path, &newer);
    let older_result = mct_daemon::project_governed_config(&path, &older);
    let after_stale = binding_state(&path, "peer-p");
    let fresh = dir.path().join("fresh.json");
    private_fs::write_atomic(&fresh, b"{}\n").expect("seed fresh config");
    let _ = mct_daemon::project_governed_config(&fresh, &older);
    let _ = mct_daemon::project_governed_config(&fresh, &newer);
    let after_current = binding_state(&fresh, "peer-p");
    let observations = json!({
        "newer_publish_ok": newer_result.is_ok(),
        "older_publish_ok": older_result.is_ok(),
        "newer_publish_error": newer_result.as_ref().err().map(|error| format!("{error:#}")),
        "older_publish_error": older_result.as_ref().err().map(|error| format!("{error:#}")),
        "state_when_older_snapshot_finishes_last": after_stale,
        "state_when_newer_snapshot_finishes_last": after_current,
        "outbound_binding_after_stale_overwrite": {
            "present": proof_present(&serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap_or_default()).unwrap_or(Value::Null)["peers"]["peer-p"])
        },
        "schedule": "publish revoked R+1, then publish the earlier admitted snapshot R through project_governed_config. This is the publisher's generation behavior, not a reproduced scheduler interleaving."
    });
    let report = if newer_result.is_err() || older_result.is_err() {
        verdict(
            "G1b",
            "inconclusive",
            "project_governed_config returned an error, so the ordering was not published",
            observations,
        )
    } else if after_stale == "admitted" && after_current == "revoked" {
        verdict(
            "G1b",
            "confirmed",
            "the publisher has no generation fence: a later call with the older admitted snapshot overwrites a revoked publication, and the reverse order publishes revoked",
            observations,
        )
    } else if after_stale == "revoked" {
        verdict(
            "G1b",
            "refuted",
            "publishing the older admitted snapshot after the revoked snapshot left the peer revoked",
            observations,
        )
    } else {
        verdict(
            "G1b",
            "inconclusive",
            "the published binding states were neither the stale-overwrite pair nor a generation refusal",
            observations,
        )
    };
    publish(&report);
    assert_classified(&report);
}

#[test]
fn nic131_g1_failed_projection_leaves_stale_inbound_binding() {
    let dir = tempfile::tempdir().unwrap();
    let mut mother = Mother::new(dir.path(), "mother-a", [7; 32]);
    mother.bootstrap();
    mother.command_offline(
        "peer_add",
        "/peers/add",
        &peer_body("peer-p", "endpoint-p", &Value::Null),
    );
    let _proc = mother.serve();
    if let Err(log) = mother.wait_ready() {
        let report = verdict(
            "G1",
            "inconclusive",
            "the governed Mother did not become ready, so the projection fault was not injected",
            json!({"log_tail": log.chars().rev().take(2000).collect::<String>()}),
        );
        publish(&report);
        assert_classified(&report);
        return;
    }
    let before = mother.config()["peers"]["peer-p"]["binding_state"].clone();
    let cfg_dir = mother.config.parent().unwrap();
    std::fs::set_permissions(cfg_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let restore = cfg_dir.to_path_buf();
    struct Restore(PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }
    let _restore = Restore(restore);
    let signed = mother.signed_command(
        "peer_revoke",
        "/peers/revoke",
        &json!({"peer_node_id": "peer-p"}),
    );
    let reply = mother.send_iroh(&signed);
    let after = mother.config()["peers"]["peer-p"]["binding_state"].clone();
    let log = std::fs::read_to_string(mother.work.join("mother.log")).unwrap_or_default();
    let projection_failed = log.contains("governed config projection failed");
    let reply_status = json_stdout(&reply)
        .and_then(|value| value.get("status").cloned())
        .unwrap_or(Value::Null);
    let observations = json!({
        "binding_before": before,
        "binding_after": after,
        "reply_status": reply_status,
        "send_exit": reply.status.code(),
        "projection_failed_logged": projection_failed,
        "stderr": text(&reply.stderr),
    });
    let report = if before != "admitted" {
        verdict(
            "G1",
            "inconclusive",
            "startup projection did not leave an admitted peer to go stale",
            observations,
        )
    } else if projection_failed && after == "admitted" && reply_status == "completed" {
        verdict(
            "G1",
            "confirmed",
            "the revoke reply completed, projection logged a failure, and the config binding stayed admitted",
            observations,
        )
    } else if projection_failed && after != "admitted" {
        verdict(
            "G1",
            "refuted",
            "projection logged a failure and the visible binding did not stay admitted",
            observations,
        )
    } else {
        verdict(
            "G1",
            "inconclusive",
            "the directory permission did not produce a logged projection failure against an admitted binding",
            observations,
        )
    };
    publish(&report);
    assert_classified(&report);
}

#[test]
fn nic131_g1c_postcommit_indeterminate_skips_projection_and_refuses_retry() {
    let dir = tempfile::tempdir().unwrap();
    let mut mother = Mother::new(dir.path(), "mother-a", [8; 32]);
    mother.bootstrap();
    mother.command_offline(
        "peer_add",
        "/peers/add",
        &peer_body("peer-p", "endpoint-p", &Value::Null),
    );
    let clock = mother.work.join("clock.txt");
    std::fs::write(&clock, "").unwrap();
    let _proc = mother.serve();
    if let Err(log) = mother.wait_ready() {
        let report = verdict(
            "G1c",
            "inconclusive",
            "the governed Mother did not become ready",
            json!({"log_tail": log.chars().rev().take(2000).collect::<String>()}),
        );
        publish(&report);
        assert_classified(&report);
        return;
    }
    let prepared = mother.prepare(
        &json!({"action":"command","scope":"peer_revoke","route":"/peers/revoke","body": json!({"peer_node_id":"peer-p"}).to_string()}),
        "ops",
    );
    let issued_at = prepared["intent"]["issued_at"].as_str().unwrap().to_owned();
    let issued = issued_at.parse::<jiff::Timestamp>().unwrap();
    let earlier = issued
        .checked_sub(jiff::SignedDuration::from_secs(1))
        .unwrap();
    std::fs::write(&clock, format!("{issued_at}\n{earlier}\n")).unwrap();
    let signed = mother.sign_day(&prepared);
    let first = mother.send_iroh(&signed);
    let clock_left = std::fs::read_to_string(&clock).unwrap_or_default();
    let after_first = mother.config()["peers"]["peer-p"]["binding_state"].clone();
    let ledger =
        std::fs::read_to_string(mother.root.join("observations.jsonl")).unwrap_or_default();
    let fact_appended = ledger.contains("/peers/revoke");
    std::fs::write(&clock, "").unwrap();
    let retry_prepared = mother.prepare(
        &json!({"action":"command","scope":"peer_revoke","route":"/peers/revoke","body": json!({"peer_node_id":"peer-p"}).to_string()}),
        "ops",
    );
    let retry_signed = mother.sign_day(&retry_prepared);
    let retry = mother.send_iroh(&retry_signed);
    let after_retry = mother.config()["peers"]["peer-p"]["binding_state"].clone();
    let first_status = json_stdout(&first)
        .and_then(|value| value.get("status").cloned())
        .unwrap_or(Value::Null);
    let retry_status = json_stdout(&retry)
        .and_then(|value| value.get("status").cloned())
        .unwrap_or(Value::Null);
    let observations = json!({
        "clock_lines_remaining_after_first_command": clock_left.lines().filter(|line| !line.is_empty()).count(),
        "first_reply_status": first_status,
        "first_stderr": text(&first.stderr),
        "binding_after_first": after_first,
        "revoke_fact_in_ledger": fact_appended,
        "retry_reply_status": retry_status,
        "retry_stderr": text(&retry.stderr),
        "binding_after_retry": after_retry,
    });
    let clock_consumed = clock_left.lines().all(|line| line.trim().is_empty());
    let report = if !clock_consumed {
        verdict(
            "G1c",
            "inconclusive",
            "the daemon did not consume the injected clock, so this binary is not running the witness clock hook",
            observations,
        )
    } else if first_status == "indeterminate"
        && after_first == "admitted"
        && fact_appended
        && retry_status == "rejected"
        && after_retry == "admitted"
    {
        verdict(
            "G1c",
            "confirmed",
            "the revoke fact was appended, the reply was indeterminate, config stayed admitted, and the retry was rejected",
            observations,
        )
    } else if first_status == "completed" && after_first == "revoked" {
        verdict(
            "G1c",
            "refuted",
            "the injected post-commit clock still produced a completed reply and a revoked projection",
            observations,
        )
    } else {
        verdict(
            "G1c",
            "inconclusive",
            "the clock was consumed but the reply, ledger, projection, and retry did not match one classified schedule",
            observations,
        )
    };
    publish(&report);
    assert_classified(&report);
}

#[test]
fn nic131_g13_governed_foreign_proof_witness() {
    let dir = tempfile::tempdir().unwrap();
    let mut origin = Mother::new(dir.path(), "mother-a", [7; 32]);
    let mut peer = Mother::new(dir.path(), "mother-b", [9; 32]);
    origin.bootstrap();
    peer.bootstrap();
    stage_echo(&mut origin);
    let _origin_proc = origin.serve();
    let _peer_proc = peer.serve();
    if let Err(log) = origin.wait_ready().and(peer.wait_ready()) {
        let report = verdict(
            "G13",
            "inconclusive",
            "one governed Mother did not become ready",
            json!({"log_tail": log.chars().rev().take(2000).collect::<String>()}),
        );
        publish(&report);
        assert_classified(&report);
        return;
    }
    let origin_ticket = origin.ticket();
    let peer_ticket = peer.ticket();
    let peer_endpoint = peer_ticket["endpoint_id"].as_str().unwrap_or("").to_owned();
    let origin_endpoint = origin_ticket["endpoint_id"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    let add_peer = origin.signed_command(
        "peer_add",
        "/peers/add",
        &peer_body("mother-b", &peer_endpoint, &peer_ticket),
    );
    let add_origin = peer.signed_command(
        "peer_add",
        "/peers/add",
        &peer_body("mother-a", &origin_endpoint, &origin_ticket),
    );
    let add_peer_reply = origin.send_iroh(&add_peer);
    let add_origin_reply = peer.send_iroh(&add_origin);
    let config = origin.config();
    let recorded = config["peers"]["mother-b"].clone();
    let (_, authority) = uds(&origin.socket, "GET", "/receiver-authority", None);
    let (call_status, local) = local_call(&origin.socket, &authority, "g13-local");
    let unknown = run(
        &[
            "iroh".into(),
            "call-peer".into(),
            "--bind".into(),
            "127.0.0.1:0".into(),
            origin
                .root
                .join("identity/iroh-secret.hex")
                .to_str()
                .unwrap()
                .into(),
            "peer-never-added".into(),
            "patina:demo".into(),
            "control@0.1.0".into(),
            "run".into(),
            "--config".into(),
            origin.config.to_str().unwrap().into(),
            "--children-dir".into(),
            origin.root.join("children").to_str().unwrap().into(),
            "--state".into(),
            origin.root.join("state.sqlite").to_str().unwrap().into(),
            "--ledger".into(),
            origin
                .root
                .join("observations.jsonl")
                .to_str()
                .unwrap()
                .into(),
        ],
        30,
    );
    let (proof_status, proof_body) = uds(
        &origin.socket,
        "POST",
        "/peers/proof",
        Some(&json!({"peer_node_id": "mother-b"})),
    );
    let cli_proof = run(
        &[
            "peers".into(),
            "set-outbound-proof".into(),
            "mother-b".into(),
            "binding-mother-b".into(),
            "--signature-ref".into(),
            "synthetic-not-a-foreign-proof".into(),
            "--expires-at".into(),
            plus_secs(600),
            "--config".into(),
            origin.config.to_str().unwrap().into(),
            "--ledger".into(),
            origin
                .root
                .join("observations.jsonl")
                .to_str()
                .unwrap()
                .into(),
            "--uds".into(),
            origin.socket.to_str().unwrap().into(),
        ],
        30,
    );
    let offline_proof = run(
        &[
            "peers".into(),
            "set-outbound-proof".into(),
            "mother-b".into(),
            "binding-mother-b".into(),
            "--signature-ref".into(),
            "synthetic-not-a-foreign-proof".into(),
            "--expires-at".into(),
            plus_secs(600),
            "--config".into(),
            origin.config.to_str().unwrap().into(),
            "--ledger".into(),
            origin
                .root
                .join("observations.jsonl")
                .to_str()
                .unwrap()
                .into(),
            "--uds".into(),
            dir.path().join("missing.sock").to_str().unwrap().into(),
        ],
        30,
    );
    let call = run(
        &[
            "iroh".into(),
            "call-peer".into(),
            "--bind".into(),
            "127.0.0.1:0".into(),
            origin
                .root
                .join("identity/iroh-secret.hex")
                .to_str()
                .unwrap()
                .into(),
            "mother-b".into(),
            "patina:demo".into(),
            "control@0.1.0".into(),
            "run".into(),
            "--config".into(),
            origin.config.to_str().unwrap().into(),
            "--children-dir".into(),
            origin.root.join("children").to_str().unwrap().into(),
            "--state".into(),
            origin.root.join("state.sqlite").to_str().unwrap().into(),
            "--ledger".into(),
            origin
                .root
                .join("observations.jsonl")
                .to_str()
                .unwrap()
                .into(),
        ],
        40,
    );
    let after = origin.config()["peers"]["mother-b"].clone();
    let peer_log = std::fs::read_to_string(peer.work.join("mother.log")).unwrap_or_default();
    let remote_started = peer_log.contains("trace-cli-iroh-call-peer");
    let call_error = format!("{}{}", text(&call.stderr), text(&call.stdout));
    let unknown_error = format!("{}{}", text(&unknown.stderr), text(&unknown.stdout));
    let observations = json!({
        "harness": "one test process starts two production mct-daemon serve processes on 127.0.0.1 with relay left disabled. No public in-address-space governed Mother constructor exists.",
        "peer_add_origin_to_peer_status": json_stdout(&add_peer_reply).and_then(|value| value.get("status").cloned()).unwrap_or(Value::Null),
        "peer_add_peer_to_origin_status": json_stdout(&add_origin_reply).and_then(|value| value.get("status").cloned()).unwrap_or(Value::Null),
        "local_admission_binding_state": recorded.get("binding_state").cloned().unwrap_or(Value::Null),
        "local_signature_present": recorded.get("binding_signature_ref").and_then(Value::as_str).is_some(),
        "foreign_proof_present_after_admission": proof_present(&recorded),
        "ticket_endpoint_matches_peer": recorded.get("ticket").and_then(|ticket| ticket.get("endpoint_id")).and_then(Value::as_str) == Some(peer_endpoint.as_str()),
        "local_call_http": call_status,
        "local_call_outcome": local.get("outcome").cloned().unwrap_or(Value::Null),
        "unknown_peer_error": unknown_error,
        "uds_proof_status": proof_status,
        "uds_proof_body": proof_body,
        "cli_proof_error": text(&cli_proof.stderr),
        "offline_proof_error": text(&offline_proof.stderr),
        "foreign_proof_present_after_writers": proof_present(&after),
        "outbound_call_exit": call.status.code(),
        "outbound_first_error": call_error,
        "peer_log_contains_call_trace": remote_started,
    });
    let admitted = recorded.get("binding_state").and_then(Value::as_str) == Some("admitted");
    let local_ok =
        call_status == 200 && local.get("outcome").and_then(Value::as_str) == Some("completed");
    let unknown_earlier = unknown_error.contains("not found");
    let writers_refused = proof_body.get("reason").and_then(Value::as_str)
        == Some("governed_unix_socket_mutation_refused")
        && text(&cli_proof.stderr).contains("governed")
        && text(&offline_proof.stderr).contains("governed_offline_mutation_refused");
    let missing_proof = call_error.contains("no outbound binding proof");
    let report = if !admitted || !local_ok || peer_endpoint.is_empty() {
        verdict(
            "G13",
            "inconclusive",
            "local admission or the local call positive control did not succeed, or the peer ticket had no endpoint",
            observations,
        )
    } else if proof_present(&after) && remote_started {
        verdict(
            "G13",
            "refuted",
            "a supported path stored an outbound proof and the peer log shows the outbound call trace",
            observations,
        )
    } else if proof_present(&after) {
        verdict(
            "G13",
            "refuted",
            "a supported writer populated outbound_binding on the governed Mother",
            observations,
        )
    } else if missing_proof && writers_refused && unknown_earlier && !remote_started {
        verdict(
            "G13",
            "confirmed",
            "local admission and a local call succeeded, an unknown peer failed earlier, both proof writers were refused, and the admitted peer's first outbound refusal was the missing foreign proof before any peer runtime trace",
            observations,
        )
    } else {
        verdict(
            "G13",
            "inconclusive",
            "the mothers ran, but the first refusal or the controls did not separate missing foreign proof from an earlier failure",
            observations,
        )
    };
    publish(&report);
    assert_classified(&report);
}
