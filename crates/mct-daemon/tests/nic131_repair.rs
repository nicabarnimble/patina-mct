//! NIC-131 repair regressions. Production daemons, supported adapters,
//! synthetic keys. Each test asserts the intended behavior after the repair:
//! a canonically revoked peer is refused whatever happens to the config copy,
//! a stale publication cannot finish last, and two governed Mothers complete
//! a receiver-issued proof exchange and a resident call. Tests that need a
//! fault or a barrier use the `nic131-witness` hooks and compile only with it.
#![cfg(unix)]

use base64::{
    Engine as _, engine::general_purpose::STANDARD, engine::general_purpose::URL_SAFE_NO_PAD,
};
use mct_daemon::MctRuntimeStateStore;
use mct_iroh::{
    MotherIrohEndpoint, MotherIrohEndpointConfig, MotherIrohEndpointTicket,
    endpoint_id_for_secret_key_hex,
};
use mct_kernel::admin_key::WebAuthnP256DayKeyV1;
use mct_kernel::{
    AuthorityContextSnapshot, CallId, CallOrigin, CallerIdentity, ConnectionSide,
    GrantsAuthorityIdentity, HelloOutcome, HelloPolicy, IrohConnectionPresentation, MCT_CALL_ALPN,
    MCT_HELLO_ALPN, MctCall, MctCallPayloadHandle, MctCallProtocolAuthority, MctCallProtocolReply,
    MctCallProtocolRequest, MctHelloRequest, MctHelloResponse, MctNodeId,
    MctPeerBindingPresentation, ObservationId, OperationTarget, PathClass, PayloadMetadata,
    PeerBindingId, ProtocolRequestId, SpanId, Timestamp, TraceContext, TraceId, VisionId,
};
use mct_observation::private_fs;
use p256::ecdsa::signature::Signer as _;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::io::{Read as _, Write as _};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

const VISION: &str = "vision-local";
const P_SECRET: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
const Q_SECRET: &str = "a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4";
const ECHO_NAMESPACE: &str = "patina:demo";
const ECHO_INTERFACE: &str = "control@0.1.0";
const DIR_NAMESPACE: &str = "patina:nic131";
const DIR_INTERFACE: &str = "dir@0.1.0";
const REMOTE_NAMESPACE: &str = "patina:nic131-remote";
const REMOTE_INTERFACE: &str = "control@0.1.0";

// ---------------------------------------------------------------- process

fn daemon() -> PathBuf {
    std::env::var_os("NIC131_DAEMON")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_mct-daemon")))
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

fn reply_status(output: &Output) -> Value {
    json_stdout(output)
        .and_then(|value| value.get("status").cloned())
        .unwrap_or(Value::Null)
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

fn private_tempdir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn random_secret() -> [u8; 32] {
    let mut secret = [0_u8; 32];
    getrandom::fill(&mut secret).expect("OS randomness for disposable test identity");
    secret
}

fn random_secret_hex() -> String {
    const_hex::encode(random_secret())
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
        "{method} {path} HTTP/1.1\r\nHost: local\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
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
    boot: usize,
    env: Vec<(String, String)>,
    process: Option<MotherProc>,
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
            boot: 0,
            env: Vec::new(),
            process: None,
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

    fn prepare_command(&mut self, scope: &str, route: &str, body: &Value) -> Value {
        self.prepare(
            &json!({"action":"command","scope":scope,"route":route,"body":body.to_string()}),
            "ops",
        )
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
        let prepared = self.prepare_command(scope, route, body);
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

    fn log_path(&self) -> PathBuf {
        self.work.join(format!("mother-{}.log", self.boot))
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.log_path()).unwrap_or_default()
    }

    #[cfg(feature = "nic131-witness")]
    fn all_logs(&self) -> String {
        (1..=self.boot)
            .map(|boot| {
                std::fs::read_to_string(self.work.join(format!("mother-{boot}.log")))
                    .unwrap_or_default()
            })
            .collect()
    }

    fn start(&mut self) -> Result<(), String> {
        assert!(self.process.is_none());
        self.boot += 1;
        let log = std::fs::File::create(self.log_path()).unwrap();
        let mut command = Command::new(daemon());
        command
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
            .args(["--vision", VISION])
            .args(["--bind", "127.0.0.1:0"])
            .args(["--max-connections", "8"])
            .current_dir(&self.work)
            .env(
                "NIC131_CLOCK_FILE",
                self.work.join("clock.txt").to_str().unwrap(),
            )
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log);
        for (key, value) in &self.env {
            command.env(key, value);
        }
        self.process = Some(MotherProc(command.spawn().unwrap()));
        self.wait_ready()
    }

    fn stop(&mut self) {
        self.process = None;
    }

    fn restart(&mut self) -> Result<(), String> {
        self.stop();
        self.start()
    }

    fn wait_ready(&self) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            if self.socket.exists()
                && uds(&self.socket, "GET", "/status", None).1["readiness"] == "ready"
                && !self.ticket_line().is_empty()
            {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(self.log().chars().rev().take(3000).collect());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn ticket_line(&self) -> String {
        self.log()
            .lines()
            .find_map(|line| line.strip_prefix("ticket="))
            .unwrap_or("")
            .to_owned()
    }

    fn ticket_value(&self) -> Value {
        serde_json::from_str(&self.ticket_line()).unwrap_or(Value::Null)
    }

    fn ticket(&self) -> MotherIrohEndpointTicket {
        MotherIrohEndpointTicket::from_json(&self.ticket_line()).unwrap()
    }

    fn send_iroh(&mut self, request: &Value) -> Output {
        let ticket = self.work.join("resident-ticket.json");
        put(&ticket, &self.ticket_value());
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
            45,
        )
    }

    fn signed_command(&mut self, scope: &str, route: &str, body: &Value) -> Value {
        let prepared = self.prepare_command(scope, route, body);
        self.sign_day(&prepared)
    }

    /// One signed command over `mct/admin/1`; returns the reply status.
    fn govern(&mut self, scope: &str, route: &str, body: &Value) -> Value {
        let signed = self.signed_command(scope, route, body);
        let output = self.send_iroh(&signed);
        json!({"status": reply_status(&output), "exit": output.status.code(), "stderr": text(&output.stderr)})
    }

    fn config_value(&self) -> Value {
        serde_json::from_slice(&std::fs::read(&self.config).unwrap_or_default())
            .unwrap_or(Value::Null)
    }

    fn peer_entry(&self, peer: &str) -> Value {
        self.config_value()["peers"][peer].clone()
    }

    fn binding_state(&self, peer: &str) -> Value {
        self.peer_entry(peer)["binding_state"].clone()
    }

    fn receiver_authority(&self) -> Value {
        uds(&self.socket, "GET", "/receiver-authority", None).1
    }

    fn ledger_lines(&self) -> Vec<Value> {
        std::fs::read_to_string(self.root.join("observations.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    /// Committed passkey commands in ledger order, with the generation each made.
    fn admin_facts(&self) -> Vec<Value> {
        self.ledger_lines()
            .into_iter()
            .filter(|entry| {
                entry["observation"]["safe_message"] == "passkey administration committed"
            })
            .map(|entry| {
                let detail = entry["observation"]["detail_ref"].as_str().unwrap_or("");
                let route = [
                    "/peers/revoke",
                    "/peers/add",
                    "/toys/grant-directory-read",
                    "/toys/revoke",
                    "/children/approve",
                    "/artifacts/stage",
                ]
                .into_iter()
                .find(|route| detail.contains(route))
                .unwrap_or("other");
                let named: Vec<&str> = [
                    "peer-p",
                    "peer-q",
                    "peer-p2",
                    "peer-e",
                    "peer-heal",
                    "dr-p2",
                ]
                .into_iter()
                .filter(|needle| mentions(detail, needle))
                .collect();
                json!({
                    "local_sequence": entry["local_sequence"],
                    "generation": entry["observation"]["grants_revision"],
                    "route": route,
                    "mentions": named,
                })
            })
            .collect()
    }

    fn runs_for_trace(&self, trace: &str) -> Vec<Value> {
        let Ok(state) = MctRuntimeStateStore::open(self.root.join("state.sqlite")) else {
            return Vec::new();
        };
        state
            .list_runs(10_000)
            .unwrap_or_default()
            .into_iter()
            .filter(|run| run.trace_id.as_str() == trace)
            .map(|run| {
                json!({
                    "child": run.child_name,
                    "state": serde_json::to_value(run.state).unwrap_or(Value::Null),
                    "call_id": run.call_id.as_str(),
                })
            })
            .collect()
    }

    fn completed_runs_of(&self, child: &str) -> usize {
        let Ok(state) = MctRuntimeStateStore::open(self.root.join("state.sqlite")) else {
            return 0;
        };
        state
            .list_runs(10_000)
            .unwrap_or_default()
            .into_iter()
            .filter(|run| {
                run.child_name.as_deref() == Some(child)
                    && serde_json::to_value(run.state).ok() == Some(json!("completed"))
            })
            .count()
    }

    fn ledger_for_trace(&self, trace: &str) -> Vec<Value> {
        self.ledger_lines()
            .into_iter()
            .filter(|entry| entry["observation"]["trace"]["trace_id"] == trace)
            .map(|entry| {
                json!({
                    "kind": entry["observation"]["kind"],
                    "outcome": entry["observation"]["outcome"],
                    "safe_message": entry["observation"]["safe_message"],
                })
            })
            .collect()
    }
}

/// True when `needle` occurs as a whole identifier (not as a prefix of a longer one).
fn mentions(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(index, _)| {
        haystack[index + needle.len()..]
            .chars()
            .next()
            .is_none_or(|next| !(next.is_ascii_alphanumeric() || next == '-' || next == '_'))
    })
}

fn presented(mother: &Mother, peer: &str) -> Presented {
    Presented::from_receiver_entry(&mother.peer_entry(peer), &format!("binding-{peer}"))
}

// ---------------------------------------------------------------- children

const ECHO_MANIFEST: &str = "[child]\nname = \"resident-echo\"\nversion = \"0.1.0\"\nkind = \"child\"\nrole = \"test\"\n\n[child.ingress]\nmode = \"wit-only\"\n\n[child.artifact]\nwasm = \"resident-echo.wasm\"\n\n[child.contract]\ndefault = \"patina:demo/control@0.1.0.run\"\nallow = [\"patina:demo/control@0.1.0.run\"]\n\n[needs]\ntoys = []\n";

fn echo_wasm() -> Vec<u8> {
    let path = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/mct-test-echo-0.1.0/mct-test-echo.wasm"
    ));
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Imports only `wasi:filesystem` types and preopens, so the resident must
/// admit the governed directory-read Toy before it can start the run. It never
/// calls the import; its export returns 11.
fn dir_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"(component
  (type (;0;) (instance (export (;0;) "descriptor" (type (sub resource)))))
  (import "wasi:filesystem/types@0.2.3" (instance (;0;) (type 0)))
  (alias export 0 "descriptor" (type (;1;)))
  (type (;2;)
    (instance
      (alias outer 1 1 (type (;0;)))
      (type (;1;) (own 0))
      (type (;2;) (tuple 1 string))
      (type (;3;) (list 2))
      (type (;4;) (func (result 3)))
      (export (;0;) "get-directories" (func (type 4)))))
  (import "wasi:filesystem/preopens@0.2.3" (instance (;1;) (type 2)))
  (core module $m (func (export "run") (result i32) i32.const 11))
  (core instance $i (instantiate $m))
  (func $run (result s32) (canon lift (core func $i "run")))
  (instance $dir (export "run" (func $run)))
  (export "patina:nic131/dir@0.1.0" (instance $dir)))"#,
    )
    .unwrap()
}

fn dir_manifest() -> String {
    "[child]\nname = \"resident-dir\"\nversion = \"0.1.0\"\nkind = \"child\"\nrole = \"test\"\n\n[child.ingress]\nmode = \"wit-only\"\n\n[child.artifact]\nwasm = \"resident-dir.wasm\"\n\n[child.contract]\ndefault = \"patina:nic131/dir@0.1.0.run\"\nallow = [\"patina:nic131/dir@0.1.0.run\"]\n\n[needs]\ntoys = []\n".into()
}

fn remote_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"(component
  (core module $m (func (export "run") (result i32) i32.const 23))
  (core instance $i (instantiate $m))
  (func $run (result s32) (canon lift (core func $i "run")))
  (instance $c (export "run" (func $run)))
  (export "patina:nic131-remote/control@0.1.0" (instance $c)))"#,
    )
    .unwrap()
}

fn remote_manifest() -> String {
    "[child]\nname = \"resident-remote\"\nversion = \"0.1.0\"\nkind = \"child\"\nrole = \"test\"\n\n[child.ingress]\nmode = \"wit-only\"\n\n[child.artifact]\nwasm = \"resident-remote.wasm\"\n\n[child.contract]\ndefault = \"patina:nic131-remote/control@0.1.0.run\"\nallow = [\"patina:nic131-remote/control@0.1.0.run\"]\n\n[needs]\ntoys = []\n".into()
}

/// Signed offline staging and approval through the governed adapters.
fn stage_and_approve(mother: &mut Mother, name: &str, wasm: &[u8], manifest: &str) {
    let source = mother.work.join(format!("source-{name}"));
    std::fs::create_dir_all(&source).unwrap();
    let wasm_file = format!("{name}.wasm");
    std::fs::write(source.join(&wasm_file), wasm).unwrap();
    std::fs::write(source.join("child.toml"), manifest).unwrap();
    let staged = mother.command_offline(
        "artifact",
        "/artifacts/stage",
        &json!({
            "source_root": source,
            "manifest_path": "child.toml",
            "component_path": wasm_file,
            "claimed_child_name": name,
            "claimed_artifact_version": "0.1.0",
            "expected_digest": format!("blake3:{}", blake3::hash(wasm).to_hex()),
            "expected_manifest_digest": format!("sha256:{:x}", Sha256::digest(manifest.as_bytes())),
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
    mother.command_offline(
        "approve_child",
        "/children/approve",
        &json!({
            "child_name": name,
            "artifact_id": artifact_id,
            "artifact_version": "0.1.0",
            "manifest_hash": manifest_hash,
            "vision_id": VISION,
            "policy_revision": 1
        }),
    );
}

fn peer_body(peer: &str, endpoint_id: &str, ticket: &Value, expires_at: &str) -> Value {
    json!({
        "peer_node_id": peer,
        "binding_id": format!("binding-{peer}"),
        "endpoint_id": endpoint_id,
        "vision_id": VISION,
        "ticket": ticket,
        "policy_revision": 1,
        "expires_at": expires_at
    })
}

fn grant_body(grant_id: &str, caller: &str, root: &str) -> Value {
    json!({
        "grant_id": grant_id,
        "child_name": "resident-dir",
        "canonical_root": root,
        "caller_node_id": caller,
        "expires_at": plus_secs(3600)
    })
}

// ---------------------------------------------------------------- peer client

/// A synthetic peer speaking the public `mct/hello/0` and `mct/call/0` client
/// API. It holds no Mother; the receiving Mother is the production daemon.
struct PeerClient {
    runtime: tokio::runtime::Runtime,
    endpoint: Option<MotherIrohEndpoint>,
    node: String,
    endpoint_id: String,
    sequence: std::sync::atomic::AtomicU32,
}

struct Presented {
    binding_id: String,
    signature_ref: Option<String>,
    policy_revision: Option<u64>,
    expires_at: Option<String>,
}

impl Presented {
    /// The receiver-issued binding as the receiver's own copy records it.
    fn from_receiver_entry(entry: &Value, fallback_binding: &str) -> Self {
        Self {
            binding_id: entry["binding_id"]
                .as_str()
                .unwrap_or(fallback_binding)
                .to_owned(),
            signature_ref: entry["binding_signature_ref"].as_str().map(str::to_owned),
            policy_revision: entry["policy_revision"].as_u64(),
            expires_at: entry["expires_at"].as_str().map(str::to_owned),
        }
    }
}

impl PeerClient {
    fn endpoint_id_for(secret: &str) -> String {
        endpoint_id_for_secret_key_hex(secret)
            .unwrap()
            .as_str()
            .to_owned()
    }

    fn new(secret: &str, node: &str) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let config = MotherIrohEndpointConfig::local_mct()
            .with_secret_key_hex(secret)
            .with_bind_addr("127.0.0.1:0".parse().unwrap());
        let endpoint = runtime.block_on(MotherIrohEndpoint::bind(config)).unwrap();
        let endpoint_id = endpoint.snapshot().endpoint_id.as_str().to_owned();
        assert_eq!(endpoint_id, Self::endpoint_id_for(secret));
        Self {
            runtime,
            endpoint: Some(endpoint),
            node: node.to_owned(),
            endpoint_id,
            sequence: std::sync::atomic::AtomicU32::new(0),
        }
    }

    fn next(&self) -> u32 {
        self.sequence
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1
    }

    fn hello(
        &self,
        ticket: &MotherIrohEndpointTicket,
        presented: &Presented,
        vision: &str,
        trace: &str,
    ) -> (Value, Option<MctHelloResponse>) {
        let endpoint_id = mct_kernel::EndpointIdText::new(self.endpoint_id.as_str()).unwrap();
        let request = MctHelloRequest {
            hello_id: format!("hello-{}-{}", self.node, self.next()),
            received_over: IrohConnectionPresentation {
                endpoint_id: endpoint_id.clone(),
                alpn: MCT_HELLO_ALPN.into(),
                connection_side: ConnectionSide::Outgoing,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            requested_protocol: HelloPolicy::default().protocol,
            requested_vision_id: Some(VisionId::new(vision).unwrap()),
            requested_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
            presented_binding: MctPeerBindingPresentation {
                binding_id: Some(PeerBindingId::new(presented.binding_id.as_str()).unwrap()),
                endpoint_id,
                mct_node_id: Some(MctNodeId::new(self.node.as_str()).unwrap()),
                vision_id: Some(VisionId::new(vision).unwrap()),
                policy_revision: presented.policy_revision,
                allowed_alpns_claim: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
                signature_ref: presented.signature_ref.clone(),
                expires_at: presented
                    .expires_at
                    .as_deref()
                    .map(|stamp| Timestamp::new(stamp).unwrap()),
            },
            capability_view: None,
            local_policy_revision_seen: Some(1),
            trace_id: TraceId::new(trace).unwrap(),
            received_observation_id: ObservationId::new(format!("obs-{trace}")).unwrap(),
        };
        let endpoint = self.endpoint.as_ref().unwrap();
        match self.runtime.block_on(endpoint.send_hello(ticket, &request)) {
            Ok(response) => (
                json!({
                    "trace": trace,
                    "outcome": serde_json::to_value(response.hello_outcome).unwrap(),
                    "safe_message": response.safe_message,
                    "receiving_grants_authority": response.receiving_grants_authority,
                }),
                Some(response),
            ),
            Err(error) => (
                json!({"trace": trace, "transport_error": format!("{error:#}")}),
                None,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn call(
        &self,
        ticket: &MotherIrohEndpointTicket,
        hello_decision: &mct_kernel::DecisionId,
        binding_id: &str,
        expectation: &GrantsAuthorityIdentity,
        target: (&str, &str),
        trace: &str,
    ) -> (Value, Option<MctCallProtocolReply>) {
        let endpoint_id = mct_kernel::EndpointIdText::new(self.endpoint_id.as_str()).unwrap();
        let id = format!("{trace}-{}", self.next());
        let payload = b"[]".to_vec();
        let digest = blake3::hash(&payload).to_hex().to_string();
        let request = MctCallProtocolRequest {
            protocol_request_id: ProtocolRequestId::new(format!("proto-{id}")).unwrap(),
            authority: MctCallProtocolAuthority {
                hello_decision_id: hello_decision.clone(),
                peer_binding_id: PeerBindingId::new(binding_id).unwrap(),
                vision_id: VisionId::new(VISION).unwrap(),
                accepted_alpn: MCT_CALL_ALPN.into(),
                endpoint_id: endpoint_id.clone(),
                policy_revision: 1,
                expected_receiver_grants_authority: expectation.clone(),
            },
            received_over: IrohConnectionPresentation {
                endpoint_id,
                alpn: MCT_CALL_ALPN.into(),
                connection_side: ConnectionSide::Outgoing,
                path_class: PathClass::Direct,
                relay_url: None,
                presented_capability_ref: None,
            },
            call: MctCall {
                call_id: CallId::new(format!("call-{id}")).unwrap(),
                caller: CallerIdentity {
                    node_id: MctNodeId::new(self.node.as_str()).unwrap(),
                    user_id: None,
                    vision_id: VisionId::new(VISION).unwrap(),
                    project_id: None,
                },
                target: OperationTarget {
                    namespace: target.0.into(),
                    interface_name: target.1.into(),
                    function_name: "run".into(),
                },
                payload_metadata: PayloadMetadata {
                    data_classification: "public".into(),
                    size_bytes: payload.len() as u64,
                    contains_secret_scoped_material: false,
                },
                authority_context: AuthorityContextSnapshot {
                    policy_revision: 1,
                    expected_receiver_grants_authority: expectation.clone(),
                    vision_policy_revision: 1,
                },
                deadline: Timestamp::new(plus_secs(120)).unwrap(),
                trace_context: TraceContext {
                    trace_id: TraceId::new(trace).unwrap(),
                    span_id: SpanId::new(format!("span-{id}")).unwrap(),
                },
                origin: CallOrigin::Iroh,
            },
            payload: MctCallPayloadHandle::InlinePayload {
                inline_payload_ref: format!("payload-{id}"),
                content_type: "application/json".into(),
                size_bytes: payload.len() as u64,
                blake3_digest_hex: digest,
            },
            idempotency_key: Some(format!("key-{id}")),
            received_observation_id: ObservationId::new(format!("obs-{id}")).unwrap(),
        };
        let endpoint = self.endpoint.as_ref().unwrap();
        match self
            .runtime
            .block_on(endpoint.send_call_with_inline_payload(ticket, &request, payload))
        {
            Ok(reply) => (
                json!({
                    "trace": trace,
                    "expected_generation": expectation.generation,
                    "reply_outcome": serde_json::to_value(reply.reply.reply_outcome).unwrap(),
                    "reason": serde_json::to_value(&reply.reply.reason).unwrap(),
                    "safe_message": reply.reply.safe_message,
                    "route_taken": serde_json::to_value(&reply.reply.route_taken).unwrap(),
                }),
                Some(reply.reply),
            ),
            Err(error) => (
                json!({"trace": trace, "transport_error": format!("{error:#}")}),
                None,
            ),
        }
    }
}

impl Drop for PeerClient {
    fn drop(&mut self) {
        if let Some(mut endpoint) = self.endpoint.take() {
            self.runtime.block_on(endpoint.close());
        }
    }
}

fn admitted(hello: &Option<MctHelloResponse>) -> bool {
    hello
        .as_ref()
        .is_some_and(|response| response.hello_outcome == HelloOutcome::Admitted)
}

fn success(reply: &Option<MctCallProtocolReply>) -> bool {
    reply.as_ref().is_some_and(|reply| {
        serde_json::to_value(reply.reply_outcome).ok() == Some(json!("success"))
    })
}

/// One fresh hello followed, when admitted, by a call on that hello.
#[allow(clippy::too_many_arguments)]
fn probe(
    mother: &Mother,
    client: &PeerClient,
    presented: &Presented,
    vision: &str,
    expectation: Option<&GrantsAuthorityIdentity>,
    target: (&str, &str),
    trace: &str,
) -> (Value, bool, bool) {
    let ticket = mother.ticket();
    let (hello_fact, hello) = client.hello(&ticket, presented, vision, &format!("{trace}-hello"));
    let hello_admitted = admitted(&hello);
    let (call_fact, call_ok) = match hello.as_ref() {
        Some(response) if hello_admitted => {
            let current = response.receiving_grants_authority.clone();
            let expectation = expectation.cloned().or(current);
            match expectation {
                Some(expectation) => {
                    let (fact, reply) = client.call(
                        &ticket,
                        &response.decision_id,
                        &presented.binding_id,
                        &expectation,
                        target,
                        trace,
                    );
                    (fact, success(&reply))
                }
                None => (
                    json!({"skipped": "admitted hello carried no receiver authority"}),
                    false,
                ),
            }
        }
        _ => (json!({"skipped": "hello not admitted"}), false),
    };
    let fact = json!({
        "hello": hello_fact,
        "hello_ledger": mother.ledger_for_trace(&format!("{trace}-hello")),
        "call": call_fact,
        "call_ledger": mother.ledger_for_trace(trace),
        "runs": mother.runs_for_trace(trace),
    });
    (fact, hello_admitted, call_ok)
}

fn runs_completed(fact: &Value) -> usize {
    fact["runs"]
        .as_array()
        .map(|runs| {
            runs.iter()
                .filter(|run| run["state"] == "completed")
                .count()
        })
        .unwrap_or(0)
}

#[cfg(feature = "nic131-witness")]
fn send_in_background(mother: &mut Mother, request: &Value) -> std::thread::JoinHandle<Output> {
    let ticket = mother.work.join("resident-ticket.json");
    put(&ticket, &mother.ticket_value());
    let request = mother.file(request);
    std::thread::spawn(move || {
        run(
            &[
                "passkey-admin".into(),
                "send-iroh".into(),
                "--ticket".into(),
                ticket.to_str().unwrap().into(),
                "--request".into(),
                request,
            ],
            45,
        )
    })
}

#[cfg(feature = "nic131-witness")]
fn wait_for_hold(gate: &Path, timeout: Duration) -> Option<Value> {
    wait_for_marker(gate, "paused-", timeout)
}

/// Waits for the gate's marker file with this prefix: the held thread wrote it
/// after its boundary and before it blocked.
#[cfg(feature = "nic131-witness")]
fn wait_for_marker(gate: &Path, prefix: &str, timeout: Duration) -> Option<Value> {
    let deadline = Instant::now() + timeout;
    loop {
        let found = std::fs::read_dir(gate).ok().and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .find(|entry| entry.file_name().to_string_lossy().starts_with(prefix))
        });
        if let Some(entry) = found {
            return std::fs::read_to_string(entry.path())
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok());
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn hello_outcome(fact: &Value) -> &str {
    fact["hello"]["outcome"].as_str().unwrap_or("none")
}

fn resident_call(mother: &Mother, target: (&str, &str), label: &str) -> Value {
    let authority = mother.receiver_authority();
    let payload = b"[]";
    let id = format!("{}-{label}", mother.name);
    let (status, reply) = uds(
        &mother.socket,
        "POST",
        "/calls",
        Some(&json!({
            "protocol_request_id": format!("proto-{id}"),
            "call_id": id,
            "target": {"namespace": target.0, "interface_name": target.1, "function_name": "run"},
            "payload_metadata": {"data_classification": "public", "size_bytes": payload.len(), "contains_secret_scoped_material": false},
            "authority_context": {"policy_revision": 1, "expected_receiver_grants_authority": authority, "vision_policy_revision": 1},
            "deadline": plus_secs(120),
            "trace_context": {"trace_id": format!("trace-{id}"), "span_id": format!("span-{id}")},
            "payload": {"payload_kind": "inline_payload", "inline_payload_ref": format!("payload-{id}"), "content_type": "application/json", "size_bytes": payload.len(), "blake3_digest_hex": blake3::hash(payload).to_hex().to_string()},
            "inline_payload_base64": STANDARD.encode(payload),
            "idempotency_key": format!("key-{id}")
        })),
    );
    json!({
        "http": status,
        "outcome": reply.get("outcome").cloned().unwrap_or(Value::Null),
        "reason": reply.get("reason").cloned().unwrap_or(Value::Null),
        "safe_message": reply.get("safe_message").cloned().unwrap_or(Value::Null),
        "route_taken": reply.get("route_taken").cloned().unwrap_or(Value::Null),
        "caller_ledger": mother.ledger_for_trace(&format!("trace-{id}")),
    })
}

fn surfaces(mother: &Mother, peer: &str) -> usize {
    MctRuntimeStateStore::open(mother.root.join("state.sqlite"))
        .ok()
        .and_then(|state| {
            state
                .remote_callable_surfaces(
                    &MctNodeId::new(peer).unwrap(),
                    &VisionId::new(VISION).unwrap(),
                )
                .ok()
        })
        .map(|surfaces| surfaces.len())
        .unwrap_or(0)
}

#[cfg(feature = "nic131-witness")]
fn read_trace(gate: &Path) -> Vec<Value> {
    std::fs::read_to_string(gate.join("trace.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

// ---------------------------------------------------------------- shared setup

/// A governed Mother with `resident-echo` and `resident-dir` approved, a
/// directory grant naming peer-p, and peer-p admitted. Two runtime workers and
/// the witness gate directory, so held spans never starve the serve loop.
struct Governed {
    _dir: tempfile::TempDir,
    mother: Mother,
    #[cfg_attr(not(feature = "nic131-witness"), allow(dead_code))]
    gate: PathBuf,
    p: PeerClient,
    q: PeerClient,
}

fn governed() -> Governed {
    let dir = private_tempdir();
    let gate = dir.path().join("gate");
    private_fs::ensure_directory(&gate).unwrap();
    let mut mother = Mother::new(dir.path(), "mother-a", [7; 32]);
    mother.env = vec![
        ("NIC131_GATE_DIR".into(), gate.to_str().unwrap().into()),
        ("NIC131_GATE_MAX_HOLD_MS".into(), "25000".into()),
        ("TOKIO_WORKER_THREADS".into(), "2".into()),
    ];
    let root = dir.path().join("grant-root");
    private_fs::ensure_directory(&root).unwrap();
    let root = format!("file://{}", std::fs::canonicalize(&root).unwrap().display());
    mother.bootstrap();
    stage_and_approve(&mut mother, "resident-echo", &echo_wasm(), ECHO_MANIFEST);
    stage_and_approve(&mut mother, "resident-dir", &dir_wasm(), &dir_manifest());
    mother.command_offline(
        "watch_grant",
        "/toys/grant-directory-read",
        &grant_body("dr-p", "peer-p", &root),
    );
    mother.command_offline(
        "peer_add",
        "/peers/add",
        &peer_body(
            "peer-p",
            &PeerClient::endpoint_id_for(P_SECRET),
            &Value::Null,
            &plus_secs(3600),
        ),
    );
    std::fs::write(mother.work.join("clock.txt"), "").unwrap();
    mother
        .start()
        .unwrap_or_else(|log| panic!("governed Mother not ready: {log}"));
    Governed {
        _dir: dir,
        mother,
        gate,
        p: PeerClient::new(P_SECRET, "peer-p"),
        q: PeerClient::new(Q_SECRET, "peer-q"),
    }
}

fn echo() -> (&'static str, &'static str) {
    (ECHO_NAMESPACE, ECHO_INTERFACE)
}

fn dir_target() -> (&'static str, &'static str) {
    (DIR_NAMESPACE, DIR_INTERFACE)
}

/// Before the revoke, peer-p's hello is admitted and both Children run.
/// Returns the admitted hello, the old expectation it carries, and peer-p's
/// presented binding.
fn admitted_before_revoke(g: &Governed) -> (MctHelloResponse, GrantsAuthorityIdentity, Presented) {
    let (fact, hello_ok, call_ok) = probe(
        &g.mother,
        &g.p,
        &presented(&g.mother, "peer-p"),
        VISION,
        None,
        echo(),
        "before-p-echo",
    );
    assert!(hello_ok && call_ok && runs_completed(&fact) == 1, "{fact}");
    let (fact, _, dir_ok) = probe(
        &g.mother,
        &g.p,
        &presented(&g.mother, "peer-p"),
        VISION,
        None,
        dir_target(),
        "before-p-dir",
    );
    assert!(dir_ok && runs_completed(&fact) == 1, "{fact}");
    let before = presented(&g.mother, "peer-p");
    let (fact, hello) =
        g.p.hello(&g.mother.ticket(), &before, VISION, "before-revoke-hello");
    let hello = hello.unwrap_or_else(|| panic!("{fact}"));
    assert_eq!(hello.hello_outcome, HelloOutcome::Admitted, "{fact}");
    let old = hello.receiving_grants_authority.clone().unwrap();
    (hello, old, before)
}

/// Fresh supported traffic from the canonically revoked peer. Every probe must
/// be refused, nothing may run, and the unknown-peer control must reach an
/// evaluation so the refusals are not a read failure.
fn assert_revoked_peer_refused(
    g: &Governed,
    label: &str,
    before: &(MctHelloResponse, GrantsAuthorityIdentity, Presented),
) {
    let (hello, old, presented_before) = before;
    let current: GrantsAuthorityIdentity =
        serde_json::from_value(g.mother.receiver_authority()).unwrap();
    assert!(
        current.generation > old.generation,
        "{label}: revoke must advance the receiver generation"
    );
    let (unknown, _, _) = probe(
        &g.mother,
        &g.q,
        &Presented {
            binding_id: "binding-peer-q".into(),
            signature_ref: None,
            policy_revision: Some(1),
            expires_at: None,
        },
        VISION,
        None,
        echo(),
        &format!("{label}-q-unknown"),
    );
    assert_eq!(hello_outcome(&unknown), "denied", "{label}: {unknown}");
    for (target, trace) in [(echo(), "p-echo"), (dir_target(), "p-dir")] {
        let trace = format!("{label}-{trace}");
        let (fact, hello_ok, call_ok) = probe(
            &g.mother,
            &g.p,
            &presented(&g.mother, "peer-p"),
            VISION,
            None,
            target,
            &trace,
        );
        assert!(
            !hello_ok,
            "{label}: fresh hello from the revoked peer was admitted: {fact}"
        );
        assert!(!call_ok && runs_completed(&fact) == 0, "{label}: {fact}");
        assert!(
            g.mother.runs_for_trace(&trace).is_empty(),
            "{label}: {fact}"
        );
    }
    for (expectation, name) in [(current.clone(), "current"), (old.clone(), "old")] {
        let trace = format!("{label}-p-on-old-hello-{name}");
        let (fact, reply) = g.p.call(
            &g.mother.ticket(),
            &hello.decision_id,
            &presented_before.binding_id,
            &expectation,
            echo(),
            &trace,
        );
        assert!(
            !success(&reply),
            "{label}: call on the pre-revoke hello ran: {fact}"
        );
        assert!(
            g.mother.runs_for_trace(&trace).is_empty(),
            "{label}: {fact}"
        );
    }
}

fn revoke_p(mother: &mut Mother) -> Value {
    mother.govern(
        "peer_revoke",
        "/peers/revoke",
        &json!({"peer_node_id": "peer-p"}),
    )
}

fn ledger_revoked_p(mother: &Mother) -> bool {
    mother.admin_facts().iter().any(|fact| {
        fact["route"] == "/peers/revoke"
            && fact["mentions"]
                .as_array()
                .is_some_and(|names| names.contains(&json!("peer-p")))
    })
}

// ---------------------------------------------------------------- inbound

/// Control: with nothing held or failing, the revoked peer is refused.
#[test]
fn nic131_repair_fault_free_revoke_refuses_the_revoked_peer() {
    let mut g = governed();
    let before = admitted_before_revoke(&g);
    let reply = revoke_p(&mut g.mother);
    assert_eq!(reply["status"], "completed", "{reply}");
    assert!(ledger_revoked_p(&g.mother));
    assert_eq!(g.mother.binding_state("peer-p"), "revoked");
    assert_revoked_peer_refused(&g, "fault-free", &before);
}

/// The revoke commits; its publication is held inside its existing span, so
/// the copy still says admitted while fresh traffic arrives.
#[cfg(feature = "nic131-witness")]
#[test]
fn nic131_repair_revoked_peer_refused_while_publication_is_held() {
    let mut g = governed();
    let before = admitted_before_revoke(&g);
    std::fs::write(g.gate.join("pause-next"), "").unwrap();
    let signed = g.mother.signed_command(
        "peer_revoke",
        "/peers/revoke",
        &json!({"peer_node_id": "peer-p"}),
    );
    let sender = send_in_background(&mut g.mother, &signed);
    let held = wait_for_hold(&g.gate, Duration::from_secs(20));
    assert!(held.is_some(), "the revoke's publication was not held");
    assert!(
        ledger_revoked_p(&g.mother),
        "the revoke must commit before publication"
    );
    assert_eq!(
        g.mother.binding_state("peer-p"),
        "admitted",
        "the fault must be active: the copy still says admitted"
    );
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert_revoked_peer_refused(&g, "held", &before)
    }));
    std::fs::write(g.gate.join("release"), "").unwrap();
    let reply = sender.join().unwrap();
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
    assert_eq!(reply_status(&reply), "completed", "{}", text(&reply.stderr));
    assert_eq!(g.mother.binding_state("peer-p"), "revoked");
}

/// The revoke commits; its publication write fails while reads keep working.
#[cfg(feature = "nic131-witness")]
#[test]
fn nic131_repair_revoked_peer_refused_after_write_only_publication_failure() {
    let mut g = governed();
    let before = admitted_before_revoke(&g);
    std::fs::write(g.gate.join("fail-next"), "").unwrap();
    let reply = revoke_p(&mut g.mother);
    assert_eq!(reply["status"], "completed", "{reply}");
    assert!(ledger_revoked_p(&g.mother));
    assert!(
        g.mother
            .all_logs()
            .contains("governed config projection failed"),
        "the injected write failure must be logged"
    );
    assert_eq!(
        g.mother.binding_state("peer-p"),
        "admitted",
        "the fault must be active: the copy still says admitted"
    );
    assert_revoked_peer_refused(&g, "write-failed", &before);
    let later = g.mother.govern(
        "peer_add",
        "/peers/add",
        &peer_body("peer-heal", "endpoint-heal", &Value::Null, &plus_secs(3600)),
    );
    assert_eq!(later["status"], "completed", "{later}");
    assert_eq!(g.mother.binding_state("peer-p"), "revoked");
}

/// A post-commit clock failure makes the reply Indeterminate; the canonical
/// change still publishes and the revoked peer is refused.
#[cfg(feature = "nic131-witness")]
#[test]
fn nic131_repair_indeterminate_revoke_publishes_and_refuses_the_revoked_peer() {
    let mut g = governed();
    let before = admitted_before_revoke(&g);
    let prepared = g.mother.prepare_command(
        "peer_revoke",
        "/peers/revoke",
        &json!({"peer_node_id": "peer-p"}),
    );
    let issued_at = prepared["intent"]["issued_at"].as_str().unwrap().to_owned();
    let earlier = issued_at
        .parse::<jiff::Timestamp>()
        .unwrap()
        .checked_sub(jiff::SignedDuration::from_secs(1))
        .unwrap();
    let clock = g.mother.work.join("clock.txt");
    std::fs::write(&clock, format!("{issued_at}\n{earlier}\n")).unwrap();
    let signed = g.mother.sign_day(&prepared);
    let reply = g.mother.send_iroh(&signed);
    assert!(
        std::fs::read_to_string(&clock).unwrap().trim().is_empty(),
        "the witness clock must be consumed"
    );
    assert_eq!(
        reply_status(&reply),
        "indeterminate",
        "{}",
        text(&reply.stderr)
    );
    assert!(ledger_revoked_p(&g.mother));
    assert_eq!(
        g.mother.binding_state("peer-p"),
        "revoked",
        "a committed change must publish even when the reply is Indeterminate"
    );
    assert_revoked_peer_refused(&g, "indeterminate", &before);
    let retry = revoke_p(&mut g.mother);
    assert_eq!(retry["status"], "rejected", "{retry}");
}

// ---------------------------------------------------------------- publication order

/// A completes and captures snapshot R; its publication is held. B revokes
/// peer-p against the now-current authority and commits. A is released only
/// after B's commit is in the ledger. The copy must end at B's state.
#[cfg(feature = "nic131-witness")]
#[test]
fn nic131_repair_stale_publication_cannot_finish_after_a_newer_one() {
    let mut g = governed();
    let a_signed = g.mother.signed_command(
        "peer_add",
        "/peers/add",
        &peer_body("peer-q", "endpoint-q", &Value::Null, &plus_secs(3600)),
    );
    std::fs::write(g.gate.join("pause-next"), "").unwrap();
    let a = send_in_background(&mut g.mother, &a_signed);
    let held = wait_for_hold(&g.gate, Duration::from_secs(20)).expect("A was not held");
    let b_signed = g.mother.signed_command(
        "peer_revoke",
        "/peers/revoke",
        &json!({"peer_node_id": "peer-p"}),
    );
    let b = send_in_background(&mut g.mother, &b_signed);
    // Barrier on the real boundary: B's revoke is committed to the ledger.
    let deadline = Instant::now() + Duration::from_secs(20);
    while !ledger_revoked_p(&g.mother) {
        assert!(Instant::now() < deadline, "B never committed");
        std::thread::sleep(Duration::from_millis(10));
    }
    std::fs::write(g.gate.join("release"), "").unwrap();
    let a = a.join().unwrap();
    let b = b.join().unwrap();
    let trace = read_trace(&g.gate);
    println!(
        "NIC131_REPAIR_TRACE {}",
        json!({"held": held, "trace": trace})
    );
    assert_eq!(reply_status(&a), "completed", "{}", text(&a.stderr));
    assert_eq!(reply_status(&b), "completed", "{}", text(&b.stderr));
    assert_eq!(g.mother.binding_state("peer-q"), "admitted");
    assert_eq!(
        g.mother.binding_state("peer-p"),
        "revoked",
        "the older snapshot finished last: {trace:?}"
    );
    let last = trace
        .iter()
        .rev()
        .find(|event| event["event"] == "exit" && event["published"] == true)
        .and_then(|exit| {
            trace
                .iter()
                .find(|event| event["event"] == "enter" && event["seq"] == exit["seq"])
        })
        .cloned()
        .unwrap_or(Value::Null);
    assert_eq!(last["snapshot_peers"]["peer-p"], "revoked", "{trace:?}");
}

// ---------------------------------------------------------------- governed peer call

fn remote() -> (&'static str, &'static str) {
    (REMOTE_NAMESPACE, REMOTE_INTERFACE)
}

/// The receiver's own issued binding for `peer`, read through its supported
/// `peers list --json` surface.
fn issued_proof(receiver: &Mother, peer: &str) -> Value {
    let output = run(
        &[
            "peers".into(),
            "list".into(),
            "--json".into(),
            "--config".into(),
            receiver.config.to_str().unwrap().into(),
        ],
        30,
    );
    assert!(output.status.success(), "{}", text(&output.stderr));
    let peers: Value = serde_json::from_slice(&output.stdout).unwrap();
    let entry = &peers[peer];
    assert!(entry["binding_signature_ref"].is_string(), "{peers}");
    json!({
        "binding_id": entry["binding_id"],
        "policy_revision": entry["policy_revision"],
        "signature_ref": entry["binding_signature_ref"],
        "expires_at": entry["expires_at"],
    })
}

fn admit_with_proof(caller: &mut Mother, receiver_ticket: &Value, proof: Option<&Value>) -> Value {
    let mut body = peer_body(
        "mother-b",
        receiver_ticket["endpoint_id"].as_str().unwrap(),
        receiver_ticket,
        &plus_secs(3600),
    );
    if let Some(proof) = proof {
        body["outbound_proof"] = proof.clone();
    }
    caller.govern("peer_add", "/peers/add", &body)
}

fn wait_for_surfaces(mother: &Mother, peer: &str) -> usize {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let count = surfaces(mother, peer);
        if count > 0 || Instant::now() >= deadline {
            return count;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn nic131_repair_governed_peers_exchange_receiver_issued_proof_and_call() {
    let dir = private_tempdir();
    let mut origin = Mother::new(dir.path(), "mother-a", random_secret());
    let mut receiver = Mother::new(dir.path(), "mother-b", random_secret());
    origin.bootstrap();
    receiver.bootstrap();
    stage_and_approve(&mut origin, "resident-echo", &echo_wasm(), ECHO_MANIFEST);
    stage_and_approve(
        &mut receiver,
        "resident-remote",
        &remote_wasm(),
        &remote_manifest(),
    );
    origin.start().unwrap();
    receiver.start().unwrap();
    let a_ticket = origin.ticket_value();
    let b_ticket = receiver.ticket_value();
    let admitted = receiver.govern(
        "peer_add",
        "/peers/add",
        &peer_body(
            "mother-a",
            a_ticket["endpoint_id"].as_str().unwrap(),
            &a_ticket,
            &plus_secs(3600),
        ),
    );
    assert_eq!(admitted["status"], "completed", "{admitted}");
    let runs = |receiver: &Mother| receiver.completed_runs_of("resident-remote");

    // Absent proof: admitted, but no route and no execution.
    let absent = admit_with_proof(&mut origin, &b_ticket, None);
    assert_eq!(absent["status"], "completed", "{absent}");
    origin.restart().unwrap();
    let absent_call = resident_call(&origin, remote(), "absent-proof");
    assert_ne!(absent_call["outcome"], "completed", "{absent_call}");
    assert_eq!(runs(&receiver), 0);

    // Unknown target: no route either.
    let unknown_target_call =
        resident_call(&origin, ("patina:nic131-none", "control@0.1.0"), "unknown");
    assert_ne!(
        unknown_target_call["outcome"], "completed",
        "{unknown_target_call}"
    );

    // Unknown peer: the live receiver refuses hello before any Child execution.
    let unknown_peer = PeerClient::new(&random_secret_hex(), "mother-never-admitted");
    let unknown_presented = Presented {
        binding_id: "binding-mother-never-admitted".into(),
        signature_ref: None,
        policy_revision: Some(1),
        expires_at: Some(plus_secs(3600)),
    };
    let (unknown_peer_fact, unknown_admitted, unknown_call_ok) = probe(
        &receiver,
        &unknown_peer,
        &unknown_presented,
        VISION,
        None,
        remote(),
        "unknown-peer",
    );
    assert!(!unknown_admitted && !unknown_call_ok, "{unknown_peer_fact}");
    assert_eq!(runs(&receiver), 0, "unknown peer executed a Child");

    // Supported exchange: the receiver issues, the origin's person records it.
    let proof = issued_proof(&receiver, "mother-a");
    let recorded = admit_with_proof(&mut origin, &b_ticket, Some(&proof));
    assert_eq!(recorded["status"], "completed", "{recorded}");
    let stored = origin.peer_entry("mother-b")["outbound_binding"].clone();
    assert_eq!(
        stored["signature_ref"], proof["signature_ref"],
        "receiver-issued provenance"
    );
    origin.restart().unwrap();
    assert!(
        wait_for_surfaces(&origin, "mother-b") > 0,
        "no surface learned with the proof"
    );
    let valid_call = resident_call(&origin, remote(), "valid-proof");
    assert_eq!(valid_call["outcome"], "completed", "{valid_call}");
    assert_eq!(
        valid_call["route_taken"]["node_id"], "mother-b",
        "{valid_call}"
    );
    assert_eq!(runs(&receiver), 1);

    // Wrong proof: one changed character in the receiver's signature.
    let mut wrong = proof.clone();
    let signature = proof["signature_ref"].as_str().unwrap();
    let last = signature.chars().last().unwrap();
    let flipped = if last == '0' { '1' } else { '0' };
    wrong["signature_ref"] = json!(format!("{}{flipped}", &signature[..signature.len() - 1]));
    let replaced = admit_with_proof(&mut origin, &b_ticket, Some(&wrong));
    assert_eq!(replaced["status"], "completed", "{replaced}");
    let wrong_proof_call = resident_call(&origin, remote(), "wrong-proof");
    assert_ne!(
        wrong_proof_call["outcome"], "completed",
        "{wrong_proof_call}"
    );
    assert_eq!(runs(&receiver), 1, "invalid proof executed a Child");

    // Valid again, then the receiver revokes the origin.
    let restored = admit_with_proof(&mut origin, &b_ticket, Some(&proof));
    assert_eq!(restored["status"], "completed", "{restored}");
    let valid_again_call = resident_call(&origin, remote(), "valid-again");
    assert_eq!(
        valid_again_call["outcome"], "completed",
        "{valid_again_call}"
    );
    assert_eq!(runs(&receiver), 2);
    let revoked = receiver.govern(
        "peer_revoke",
        "/peers/revoke",
        &json!({"peer_node_id": "mother-a"}),
    );
    assert_eq!(revoked["status"], "completed", "{revoked}");
    let revoked_call = resident_call(&origin, remote(), "revoked-by-receiver");
    assert_ne!(revoked_call["outcome"], "completed", "{revoked_call}");
    assert_eq!(runs(&receiver), 2, "revoked peer executed a Child");

    println!(
        "NIC131_CONTAINER_SMOKE_RECEIPT {}",
        json!({
            "schema": "nic131-container-smoke/v1",
            "readiness": {"mother_a": "ready", "mother_b": "ready"},
            "valid_proof": {"outcome": valid_call["outcome"], "remote_completed_runs": 1},
            "unknown_peer": {"hello_admitted": unknown_admitted, "call_succeeded": unknown_call_ok, "remote_completed_runs": 0},
            "invalid_proof": {"outcome": wrong_proof_call["outcome"], "remote_completed_runs_after": 1},
            "revoked_peer": {"outcome": revoked_call["outcome"], "remote_completed_runs_after": 2},
            "final_remote_completed_runs": runs(&receiver),
            "proof_issued_by_receiver": stored["signature_ref"] == proof["signature_ref"],
            "transport": "iroh-loopback-relay-disabled"
        })
    );
}

// ---------------------------------------------------------------- split reads
//
// Hello and call each read the peer bindings and then the receiver authority,
// with other work between. The gate holds the evaluator after its binding
// read and before its receiver-authority read; a revoke commits in between.

#[cfg(feature = "nic131-witness")]
fn binding_reads(gate: &Path) -> Vec<Value> {
    read_trace(gate)
        .into_iter()
        .filter(|event| event["event"] == "binding_read")
        .collect()
}

#[cfg(feature = "nic131-witness")]
fn binding_state_in(event: &Value, peer: &str) -> Value {
    event["outcome"]["bindings"]
        .as_array()
        .and_then(|bindings| bindings.iter().find(|binding| binding["peer"] == peer))
        .map(|binding| binding["state"].clone())
        .unwrap_or(Value::Null)
}

#[cfg(feature = "nic131-witness")]
fn release_binding(gate: &Path) {
    std::fs::write(gate.join("binding-release"), "").unwrap();
}

#[cfg(feature = "nic131-witness")]
fn clear_markers(gate: &Path, prefix: &str) {
    for entry in std::fs::read_dir(gate).unwrap().filter_map(Result::ok) {
        if entry.file_name().to_string_lossy().starts_with(prefix) {
            std::fs::remove_file(entry.path()).unwrap();
        }
    }
}

/// A hello whose binding read saw peer-p admitted must not come back admitted
/// when the revoke committed before its receiver-authority read.
#[cfg(feature = "nic131-witness")]
#[test]
fn nic131_repair_revoke_between_hello_binding_and_receiver_reads_refuses_admission() {
    let mut g = governed();
    let (_, old, _) = admitted_before_revoke(&g);
    let ticket = g.mother.ticket();

    // Control: the same hold with no revoke still admits.
    std::fs::write(g.gate.join("binding-pause-next"), "").unwrap();
    let presented_p = presented(&g.mother, "peer-p");
    let (control, control_hello) = std::thread::scope(|scope| {
        let hello = scope.spawn(|| {
            g.p.hello(&ticket, &presented_p, VISION, "split-hello-control")
        });
        assert!(
            wait_for_marker(&g.gate, "binding-paused-", Duration::from_secs(20)).is_some(),
            "the control hello's binding read was not held"
        );
        release_binding(&g.gate);
        hello.join().unwrap()
    });
    assert!(admitted(&control_hello), "control hello: {control}");
    clear_markers(&g.gate, "binding-paused-");

    // The revoke commits between the hello's two reads.
    std::fs::write(g.gate.join("binding-pause-next"), "").unwrap();
    let (fact, hello, revoke) = std::thread::scope(|scope| {
        let hello = scope.spawn(|| g.p.hello(&ticket, &presented_p, VISION, "split-hello"));
        assert!(
            wait_for_marker(&g.gate, "binding-paused-", Duration::from_secs(20)).is_some(),
            "the hello's binding read was not held"
        );
        let read = binding_reads(&g.gate).last().cloned().unwrap();
        assert_eq!(
            binding_state_in(&read, "peer-p"),
            "admitted",
            "the held binding read must have seen peer-p admitted: {read}"
        );
        let revoke = revoke_p(&mut g.mother);
        release_binding(&g.gate);
        let (fact, hello) = hello.join().unwrap();
        (fact, hello, revoke)
    });
    assert_eq!(revoke["status"], "completed", "{revoke}");
    assert!(ledger_revoked_p(&g.mother));
    let current: GrantsAuthorityIdentity =
        serde_json::from_value(g.mother.receiver_authority()).unwrap();
    assert!(current.generation > old.generation);
    println!(
        "NIC131_SPLIT_HELLO {}",
        json!({"hello": fact, "current": current, "pre_revoke": old})
    );
    // Execution: whatever the hello said, a call on it runs nothing.
    if let Some(response) = hello.as_ref().filter(|_| admitted(&hello)) {
        let identity = response.receiving_grants_authority.clone().unwrap();
        let (call, reply) = g.p.call(
            &ticket,
            &response.decision_id,
            &presented_p.binding_id,
            &identity,
            echo(),
            "split-hello-call",
        );
        assert!(!success(&reply), "{call}");
        assert!(
            g.mother.runs_for_trace("split-hello-call").is_empty(),
            "{call}"
        );
    }
    // Admission: the hello is refused.
    assert!(
        !admitted(&hello),
        "the hello read peer-p admitted, the revoke committed, and the hello still came back admitted with generation {:?}: {fact}",
        hello
            .as_ref()
            .and_then(|h| h.receiving_grants_authority.as_ref())
            .map(|a| a.generation)
    );
}

/// A call whose binding read saw peer-p admitted is refused when the revoke
/// committed before its receiver-authority read: the expectation it carries
/// names the pre-revoke generation.
#[cfg(feature = "nic131-witness")]
#[test]
fn nic131_repair_revoke_between_call_binding_and_receiver_reads_refuses_dispatch() {
    let mut g = governed();
    let (hello, old, presented_p) = admitted_before_revoke(&g);
    let ticket = g.mother.ticket();
    let echo_runs = |mother: &Mother| mother.completed_runs_of("resident-echo");

    // Control: the same hold with no revoke still runs the Child once.
    std::fs::write(g.gate.join("binding-pause-next"), "").unwrap();
    let before_control = echo_runs(&g.mother);
    let (control, control_reply) = std::thread::scope(|scope| {
        let call = scope.spawn(|| {
            g.p.call(
                &ticket,
                &hello.decision_id,
                &presented_p.binding_id,
                &old,
                echo(),
                "split-call-control",
            )
        });
        assert!(wait_for_marker(&g.gate, "binding-paused-", Duration::from_secs(20)).is_some());
        release_binding(&g.gate);
        call.join().unwrap()
    });
    assert!(success(&control_reply), "control call: {control}");
    assert_eq!(echo_runs(&g.mother), before_control + 1);
    clear_markers(&g.gate, "binding-paused-");

    // The revoke commits between the call's two reads.
    std::fs::write(g.gate.join("binding-pause-next"), "").unwrap();
    let before = echo_runs(&g.mother);
    let (fact, reply, revoke) = std::thread::scope(|scope| {
        let call = scope.spawn(|| {
            g.p.call(
                &ticket,
                &hello.decision_id,
                &presented_p.binding_id,
                &old,
                echo(),
                "split-call",
            )
        });
        assert!(wait_for_marker(&g.gate, "binding-paused-", Duration::from_secs(20)).is_some());
        let read = binding_reads(&g.gate).last().cloned().unwrap();
        assert_eq!(binding_state_in(&read, "peer-p"), "admitted", "{read}");
        let revoke = revoke_p(&mut g.mother);
        release_binding(&g.gate);
        let (fact, reply) = call.join().unwrap();
        (fact, reply, revoke)
    });
    assert_eq!(revoke["status"], "completed", "{revoke}");
    assert!(ledger_revoked_p(&g.mother));
    let ledger = g.mother.ledger_for_trace("split-call");
    println!(
        "NIC131_SPLIT_CALL {}",
        json!({"call": fact, "ledger": ledger})
    );
    assert!(!success(&reply), "{fact}");
    assert_eq!(
        reply.as_ref().map(|reply| reply.safe_message.as_str()),
        Some("receiver authority changed; refresh hello"),
        "the refusal must be the receiver-generation check: {fact}"
    );
    assert!(g.mother.runs_for_trace("split-call").is_empty(), "{fact}");
    assert_eq!(echo_runs(&g.mother), before, "no Child ran");

    // Stale-expectation control after the revoke, with no hold.
    let (fact, reply) = g.p.call(
        &ticket,
        &hello.decision_id,
        &presented_p.binding_id,
        &old,
        echo(),
        "after-revoke-stale-call",
    );
    assert!(!success(&reply), "{fact}");
    assert_eq!(echo_runs(&g.mother), before);
}

// ---------------------------------------------------------------- writer faults

/// The writer takes the armed fault when it receives its next command. An
/// unknown peer's hello sends one (its governed binding read) and is recorded
/// as the trigger, not as a probe.
#[cfg(feature = "nic131-witness")]
fn trigger_writer_fault(g: &Governed, label: &str) -> Value {
    g.q.hello(
        &g.mother.ticket(),
        &Presented {
            binding_id: "binding-peer-q".into(),
            signature_ref: None,
            policy_revision: Some(1),
            expires_at: None,
        },
        VISION,
        &format!("{label}-trigger"),
    )
    .0
}

#[cfg(feature = "nic131-witness")]
fn assert_writer_fault_refuses(
    g: &Governed,
    label: &str,
    error_fragment: &str,
    before: &(MctHelloResponse, GrantsAuthorityIdentity, Presented),
) {
    let (hello, old, presented_p) = before;
    let echo_before = g.mother.completed_runs_of("resident-echo");
    let dir_before = g.mother.completed_runs_of("resident-dir");
    let reads_before = binding_reads(&g.gate).len();
    let ticket = g.mother.ticket();
    let probes = [
        (
            g.p.hello(&ticket, presented_p, VISION, &format!("{label}-p-hello"))
                .0,
            "p-hello",
        ),
        (
            g.q.hello(
                &ticket,
                &Presented {
                    binding_id: "binding-peer-q".into(),
                    signature_ref: None,
                    policy_revision: Some(1),
                    expires_at: None,
                },
                VISION,
                &format!("{label}-q-hello"),
            )
            .0,
            "q-hello",
        ),
        (
            g.p.call(
                &ticket,
                &hello.decision_id,
                &presented_p.binding_id,
                old,
                echo(),
                &format!("{label}-p-echo"),
            )
            .0,
            "p-echo",
        ),
        (
            g.p.call(
                &ticket,
                &hello.decision_id,
                &presented_p.binding_id,
                old,
                dir_target(),
                &format!("{label}-p-dir"),
            )
            .0,
            "p-dir",
        ),
    ];
    let reads: Vec<Value> = binding_reads(&g.gate)
        .into_iter()
        .skip(reads_before)
        .collect();
    let status = uds(&g.mother.socket, "GET", "/status", None).1;
    println!(
        "NIC131_WRITER_FAULT {}",
        json!({"label": label, "probes": probes.iter().map(|(fact, name)| json!({"probe": name, "fact": fact})).collect::<Vec<_>>(), "binding_reads": reads, "status": status})
    );
    for (fact, name) in &probes {
        assert!(
            fact["transport_error"].is_string(),
            "{label} {name}: a failed binding load ends the connection before any evaluation: {fact}"
        );
    }
    assert_eq!(
        reads.len(),
        probes.len(),
        "{label}: each probe made one binding read: {reads:?}"
    );
    for read in &reads {
        let error = read["outcome"]["error"].as_str().unwrap_or("");
        assert!(
            error.contains("read governed peers") && error.contains(error_fragment),
            "{label}: classification must be a failed governed binding load, not config fallback: {read}"
        );
    }
    for trace in ["p-echo", "p-dir"] {
        assert!(
            g.mother
                .runs_for_trace(&format!("{label}-{trace}"))
                .is_empty()
        );
    }
    assert_eq!(g.mother.completed_runs_of("resident-echo"), echo_before);
    assert_eq!(g.mother.completed_runs_of("resident-dir"), dir_before);
}

/// The writer's own fence flag is set: fresh hellos and calls are refused at
/// the governed binding read, with nothing run and no config-only fallback.
#[cfg(feature = "nic131-witness")]
#[test]
fn nic131_repair_fenced_writer_refuses_fresh_hello_and_call() {
    let g = governed();
    // Positive control: the same peer's hello and both Children work first.
    let before = admitted_before_revoke(&g);
    std::fs::write(g.gate.join("writer-fence-next"), "").unwrap();
    let trigger = trigger_writer_fault(&g, "fenced");
    let status = uds(&g.mother.socket, "GET", "/status", None).1;
    println!(
        "NIC131_FENCE_TRIGGER {}",
        json!({"trigger": trigger, "status": status})
    );
    assert!(
        read_trace(&g.gate)
            .iter()
            .any(|event| event["event"] == "writer_fenced"),
        "the writer did not take the fence"
    );
    assert_eq!(status["readiness"], "not_ready", "{status}");
    assert_eq!(
        status["safe_message"], "resident observation writer fenced",
        "{status}"
    );
    assert_writer_fault_refuses(&g, "fenced", "fenced", &before);
}

/// The writer task ends, as a dead writer would: fresh hellos and calls are
/// refused at the governed binding read. A stopped runtime is a different
/// class: no binding read happens at all.
#[cfg(feature = "nic131-witness")]
#[test]
fn nic131_repair_stopped_writer_refuses_fresh_hello_and_call() {
    let mut g = governed();
    let before = admitted_before_revoke(&g);
    std::fs::write(g.gate.join("writer-stop-next"), "").unwrap();
    let trigger = trigger_writer_fault(&g, "stopped");
    println!("NIC131_STOP_TRIGGER {}", json!({"trigger": trigger}));
    assert!(
        read_trace(&g.gate)
            .iter()
            .any(|event| event["event"] == "writer_stopped"),
        "the writer did not stop"
    );
    assert_writer_fault_refuses(&g, "stopped", "governed peer read", &before);

    // Stopped runtime: the process is gone, so no binding read happens.
    let reads_before = binding_reads(&g.gate).len();
    let presented_p = presented(&g.mother, "peer-p");
    let ticket = g.mother.ticket();
    g.mother.stop();
    let (fact, hello) =
        g.p.hello(&ticket, &presented_p, VISION, "stopped-runtime-hello");
    assert!(
        hello.is_none() && fact["transport_error"].is_string(),
        "{fact}"
    );
    assert_eq!(
        binding_reads(&g.gate).len(),
        reads_before,
        "a stopped runtime reads nothing"
    );
}

/// A call admitted by evaluation is held before the resident pipeline's
/// dispatch-time receiver read; a revoke commits there. Dispatch refuses it.
#[cfg(feature = "nic131-witness")]
#[test]
fn nic131_repair_revoke_between_call_evaluation_and_dispatch_refuses_execution() {
    let mut g = governed();
    let (hello, old, presented_p) = admitted_before_revoke(&g);
    let ticket = g.mother.ticket();
    let echo_runs = |mother: &Mother| mother.completed_runs_of("resident-echo");

    // Control: the same hold with no revoke runs the Child once.
    std::fs::write(g.gate.join("dispatch-pause-next"), "").unwrap();
    let before_control = echo_runs(&g.mother);
    let (control, control_reply) = std::thread::scope(|scope| {
        let call = scope.spawn(|| {
            g.p.call(
                &ticket,
                &hello.decision_id,
                &presented_p.binding_id,
                &old,
                echo(),
                "dispatch-control",
            )
        });
        assert!(wait_for_marker(&g.gate, "dispatch-paused-", Duration::from_secs(20)).is_some());
        std::fs::write(g.gate.join("dispatch-release"), "").unwrap();
        call.join().unwrap()
    });
    assert!(success(&control_reply), "control call: {control}");
    assert_eq!(echo_runs(&g.mother), before_control + 1);
    clear_markers(&g.gate, "dispatch-paused-");

    std::fs::write(g.gate.join("dispatch-pause-next"), "").unwrap();
    let before = echo_runs(&g.mother);
    let (fact, reply, revoke) = std::thread::scope(|scope| {
        let call = scope.spawn(|| {
            g.p.call(
                &ticket,
                &hello.decision_id,
                &presented_p.binding_id,
                &old,
                echo(),
                "dispatch-split",
            )
        });
        assert!(wait_for_marker(&g.gate, "dispatch-paused-", Duration::from_secs(20)).is_some());
        // Evaluation already admitted this call: its authority fact is in the ledger.
        let admitted_facts = g.mother.ledger_for_trace("dispatch-split");
        let revoke = revoke_p(&mut g.mother);
        std::fs::write(g.gate.join("dispatch-release"), "").unwrap();
        let (fact, reply) = call.join().unwrap();
        println!(
            "NIC131_DISPATCH_SPLIT {}",
            json!({"facts_while_held": admitted_facts, "call": fact})
        );
        (fact, reply, revoke)
    });
    assert_eq!(revoke["status"], "completed", "{revoke}");
    assert!(ledger_revoked_p(&g.mother));
    assert!(!success(&reply), "{fact}");
    assert_eq!(
        reply.as_ref().map(|reply| reply.safe_message.as_str()),
        Some("receiver authority changed; refresh hello"),
        "the refusal must be the dispatch-time receiver check: {fact}"
    );
    assert!(
        g.mother.runs_for_trace("dispatch-split").is_empty(),
        "{fact}"
    );
    assert_eq!(echo_runs(&g.mother), before, "no Child ran");
}
