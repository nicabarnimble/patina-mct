//! NIC-131 proof extension. Evidence collection, not a repair.
//!
//! Every witness drives production `mct-daemon serve` processes through
//! supported adapters: offline `passkey-admin apply` before serving, signed
//! `passkey-admin send-iroh` while serving, the Unix control socket for reads
//! and resident calls, and real `mct/hello/0` and `mct/call/0` traffic from
//! synthetic peer identities. Fixed synthetic keys only. No config is edited by
//! hand and no outbound proof setter is called on a governed Mother.
//!
//! Each test publishes `NIC131_EXT_REPORT <json>` lines and, when
//! `NIC131_REPORT_DIR` is set, one file per report. Report shape
//! (`nic131-proof-extension/v1`):
//! - `prerequisites`: `{name, met, detail}`; an `inconclusive` report names at
//!   least one unmet prerequisite;
//! - `schedule`: the injection or control and the ordered steps;
//! - `facts`: observed hello, call, admission, dispatch, outcome, ledger,
//!   config and trace facts;
//! - `expected` and `actual`;
//! - `classification`: `confirmed`, `refuted`, `prevented` or `inconclusive`;
//! - `reason` and `limitations`.
//!
//! A test passes when it classified its observations. The classification is
//! the result; a passing test is not a claim that the candidate is real.
#![cfg(all(unix, feature = "nic131-witness"))]

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

const PIN: &str = "abc9c30465533622338ed0a9d47f0949291af640";
const HARNESS_BASE: &str = "7a05318054d027b33754609372853ed2e01e05df";
const SCHEMA: &str = "nic131-proof-extension/v1";
const VISION: &str = "vision-local";

const P_SECRET: &str = "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1";
const P2_SECRET: &str = "a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2";
const E_SECRET: &str = "a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3a3";
const Q_SECRET: &str = "a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4a4";

const ECHO_NAMESPACE: &str = "patina:demo";
const ECHO_INTERFACE: &str = "control@0.1.0";
const DIR_NAMESPACE: &str = "patina:nic131";
const DIR_INTERFACE: &str = "dir@0.1.0";
const REMOTE_NAMESPACE: &str = "patina:nic131-remote";
const REMOTE_INTERFACE: &str = "control@0.1.0";

// ---------------------------------------------------------------- reports

fn prerequisite(name: &str, met: bool, detail: Value) -> Value {
    json!({"name": name, "met": met, "detail": detail})
}

struct Report<'a> {
    candidate: &'a str,
    case: &'a str,
    prerequisites: Vec<Value>,
    schedule: Value,
    facts: Value,
    expected: Value,
    actual: Value,
    classification: &'a str,
    reason: String,
    limitations: Vec<&'a str>,
}

fn publish(report: Report<'_>) -> Value {
    let value = json!({
        "schema": SCHEMA,
        "candidate": report.candidate,
        "case": report.case,
        "pin": PIN,
        "harness_base": HARNESS_BASE,
        "prerequisites": report.prerequisites,
        "schedule": report.schedule,
        "facts": report.facts,
        "expected": report.expected,
        "actual": report.actual,
        "classification": report.classification,
        "reason": report.reason,
        "limitations": report.limitations,
    });
    println!("NIC131_EXT_REPORT {value}");
    if let Some(dir) = std::env::var_os("NIC131_REPORT_DIR") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("ext-{}.json", report.candidate)),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
    }
    value
}

fn assert_reported(report: &Value) {
    let classification = report["classification"].as_str().unwrap_or("");
    assert!(
        matches!(
            classification,
            "confirmed" | "refuted" | "prevented" | "inconclusive"
        ),
        "{report}"
    );
    assert!(
        report["reason"].as_str().is_some_and(|r| !r.is_empty()),
        "{report}"
    );
    if classification == "inconclusive" {
        assert!(
            report["prerequisites"]
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["met"] == false)),
            "an inconclusive report must name an unmet prerequisite: {report}"
        );
    }
}

/// First unmet prerequisite, if any.
fn unmet(prerequisites: &[Value]) -> Option<String> {
    prerequisites
        .iter()
        .find(|item| item["met"] == false)
        .map(|item| item["name"].as_str().unwrap_or("").to_owned())
}

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

fn now_text() -> String {
    jiff::Timestamp::now().to_string()
}

fn put(path: &Path, value: &impl serde::Serialize) {
    private_fs::write_atomic(path, &serde_json::to_vec(value).unwrap()).unwrap();
}

fn private_tempdir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
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
    sequence: std::cell::Cell<u32>,
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
            sequence: std::cell::Cell::new(0),
        }
    }

    fn next(&self) -> u32 {
        self.sequence.set(self.sequence.get() + 1);
        self.sequence.get()
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

// ---------------------------------------------------------------- G1 / G1c + inbound

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fault {
    /// Config directory writable; production publication.
    G1Control,
    /// Config directory mode 0500 during the revoke; publication fails.
    G1Projection,
    /// Publication is held inside its existing synchronous span while the
    /// window probes run, then released; nothing fails.
    G1HeldPublication,
    /// Witness clock returns the commit time twice; nothing fails after commit.
    G1cSteadyClock,
    /// Witness clock steps back one second after commit; post-commit check fails.
    G1cRollbackClock,
}

impl Fault {
    fn label(self) -> &'static str {
        match self {
            Fault::G1Control => "g1-control-fault-free",
            Fault::G1Projection => "g1-injected-publication-failure",
            Fault::G1HeldPublication => "g1-held-publication-window",
            Fault::G1cSteadyClock => "g1c-control-steady-clock",
            Fault::G1cRollbackClock => "g1c-injected-postcommit-clock-rollback",
        }
    }
}

struct Restore(PathBuf);
impl Drop for Restore {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
    }
}

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

fn wait_for_hold(gate: &Path, timeout: Duration) -> Option<Value> {
    let deadline = Instant::now() + timeout;
    loop {
        let found = std::fs::read_dir(gate).ok().and_then(|entries| {
            entries
                .filter_map(Result::ok)
                .find(|entry| entry.file_name().to_string_lossy().starts_with("paused-"))
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

struct Peers {
    p: PeerClient,
    p2: PeerClient,
    e: PeerClient,
    q: PeerClient,
}

/// Fresh supported inbound traffic and gate controls, run after the revoke commits.
fn window_probes(
    mother: &Mother,
    peers: &Peers,
    old_expectation: Option<&GrantsAuthorityIdentity>,
    pre_hello: &Option<MctHelloResponse>,
    pre_binding: &str,
) -> (serde_json::Map<String, Value>, Value) {
    let echo = (ECHO_NAMESPACE, ECHO_INTERFACE);
    let dirt = (DIR_NAMESPACE, DIR_INTERFACE);
    let mut window = serde_json::Map::new();
    let receiver_now = mother.receiver_authority();
    window.insert(
        "receiver_authority_from_control_socket".into(),
        receiver_now.clone(),
    );
    let (fact, fresh_admitted, fresh_call_ok) = probe(
        mother,
        &peers.p,
        &presented(mother, "peer-p"),
        VISION,
        None,
        echo,
        "window-p-echo-current",
    );
    window.insert("p_fresh_hello_and_echo_current_expectation".into(), fact);
    let (fact, _, dir_ok) = probe(
        mother,
        &peers.p,
        &presented(mother, "peer-p"),
        VISION,
        None,
        dirt,
        "window-p-dir-current",
    );
    window.insert("p_fresh_hello_and_dir_current_expectation".into(), fact);
    let old = old_expectation.map(|expectation| {
        let (fact, _, ok) = probe(
            mother,
            &peers.p,
            &presented(mother, "peer-p"),
            VISION,
            Some(expectation),
            echo,
            "window-p-echo-old",
        );
        window.insert("p_fresh_hello_and_echo_old_expectation".into(), fact);
        ok
    });
    let current: Option<GrantsAuthorityIdentity> = serde_json::from_value(receiver_now).ok();
    let on_pre = match (pre_hello.as_ref(), current) {
        (Some(response), Some(current)) if admitted(pre_hello) => {
            let (fact, reply) = peers.p.call(
                &mother.ticket(),
                &response.decision_id,
                pre_binding,
                &current,
                echo,
                "window-p-echo-on-pre-revoke-hello",
            );
            json!({
                "call": fact,
                "ok": success(&reply),
                "call_ledger": mother.ledger_for_trace("window-p-echo-on-pre-revoke-hello"),
                "runs": mother.runs_for_trace("window-p-echo-on-pre-revoke-hello"),
            })
        }
        _ => json!({"skipped": "no admitted pre-revoke hello or unreadable receiver authority"}),
    };
    window.insert(
        "p_call_on_pre_revoke_hello_current_expectation".into(),
        on_pre,
    );
    let q_presented = Presented {
        binding_id: "binding-peer-q".into(),
        signature_ref: None,
        policy_revision: Some(1),
        expires_at: None,
    };
    let (fact, _, _) = probe(
        mother,
        &peers.q,
        &q_presented,
        VISION,
        None,
        echo,
        "window-q-unknown",
    );
    window.insert("q_missing_binding".into(), fact);
    let (fact, _, _) = probe(
        mother,
        &peers.p,
        &presented(mother, "peer-p"),
        "vision-other",
        None,
        echo,
        "window-p-wrong-vision",
    );
    window.insert("p_wrong_vision".into(), fact);
    let (fact, _, _) = probe(
        mother,
        &peers.e,
        &presented(mother, "peer-e"),
        VISION,
        None,
        echo,
        "window-e-expired",
    );
    window.insert("e_expired_binding".into(), fact);
    let (fact, _, _) = probe(
        mother,
        &peers.p2,
        &presented(mother, "peer-p2"),
        VISION,
        None,
        dirt,
        "window-p2-dir-revoked-grant",
    );
    window.insert("p2_dir_revoked_grant".into(), fact);
    let (fact, _, _) = probe(
        mother,
        &peers.p2,
        &presented(mother, "peer-p2"),
        VISION,
        None,
        echo,
        "window-p2-echo",
    );
    window.insert("p2_echo".into(), fact);
    let summary = json!({
        "fresh_hello_admitted": fresh_admitted,
        "fresh_echo_call_succeeded": fresh_call_ok,
        "fresh_dir_call_succeeded": dir_ok,
        "old_expectation_call_succeeded": old,
    });
    (window, summary)
}

/// Identical synthetic preconditions for every fault: one governed Mother
/// with two runtime workers and the witness gate directory set, `resident-echo`
/// and `resident-dir` approved, directory grants for peer-p and for an
/// outsider node, and peers peer-p and peer-p2 admitted.
fn revoke_scenario(fault: Fault) -> Value {
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
        "watch_grant",
        "/toys/grant-directory-read",
        &grant_body("dr-outsider", "node-outsider", &root),
    );
    let p_endpoint = PeerClient::endpoint_id_for(P_SECRET);
    let p2_endpoint = PeerClient::endpoint_id_for(P2_SECRET);
    mother.command_offline(
        "peer_add",
        "/peers/add",
        &peer_body("peer-p", &p_endpoint, &Value::Null, &plus_secs(3600)),
    );
    mother.command_offline(
        "peer_add",
        "/peers/add",
        &peer_body("peer-p2", &p2_endpoint, &Value::Null, &plus_secs(3600)),
    );
    std::fs::write(mother.work.join("clock.txt"), "").unwrap();
    if let Err(log) = mother.start() {
        return json!({"fault": fault.label(), "ready": false, "log_tail": log});
    }
    let peers = Peers {
        p: PeerClient::new(P_SECRET, "peer-p"),
        p2: PeerClient::new(P2_SECRET, "peer-p2"),
        e: PeerClient::new(E_SECRET, "peer-e"),
        q: PeerClient::new(Q_SECRET, "peer-q"),
    };
    let echo = (ECHO_NAMESPACE, ECHO_INTERFACE);
    let dirt = (DIR_NAMESPACE, DIR_INTERFACE);

    // Baseline: every gate admits or denies for its own reason before revoke.
    let mut baseline = serde_json::Map::new();
    let mut base =
        |name: &str, mother: &Mother, client: &PeerClient, peer: &str, target, trace: &str| {
            let (fact, _, _) = probe(
                mother,
                client,
                &presented(mother, peer),
                VISION,
                None,
                target,
                trace,
            );
            baseline.insert(name.into(), fact);
        };
    base("p_echo", &mother, &peers.p, "peer-p", echo, "base-p-echo");
    base("p_dir", &mother, &peers.p, "peer-p", dirt, "base-p-dir");
    base(
        "p2_dir_only_outsider_grant",
        &mother,
        &peers.p2,
        "peer-p2",
        dirt,
        "base-p2-dir-narrow",
    );
    base(
        "p2_echo",
        &mother,
        &peers.p2,
        "peer-p2",
        echo,
        "base-p2-echo",
    );
    let granted = mother.govern(
        "watch_grant",
        "/toys/grant-directory-read",
        &grant_body("dr-p2", "peer-p2", &root),
    );
    base(
        "p2_dir_granted",
        &mother,
        &peers.p2,
        "peer-p2",
        dirt,
        "base-p2-dir-granted",
    );
    let revoked_grant = mother.govern(
        "watch_revoke",
        "/toys/revoke",
        &json!({"grant_id": "dr-p2"}),
    );
    base(
        "p2_dir_revoked_grant",
        &mother,
        &peers.p2,
        "peer-p2",
        dirt,
        "base-p2-dir-revoked-grant",
    );
    let e_expires = plus_secs(15);
    let e_add = mother.govern(
        "peer_add",
        "/peers/add",
        &peer_body(
            "peer-e",
            &PeerClient::endpoint_id_for(E_SECRET),
            &Value::Null,
            &e_expires,
        ),
    );
    base(
        "e_before_expiry",
        &mother,
        &peers.e,
        "peer-e",
        echo,
        "base-e-before-expiry",
    );
    baseline.insert("grant_p2_reply".into(), granted);
    baseline.insert("revoke_p2_grant_reply".into(), revoked_grant);
    baseline.insert("add_e_reply".into(), e_add);
    // peer-e's binding expires before the revoke, so its window probe tests expiry alone.
    let expiry = e_expires.parse::<jiff::Timestamp>().unwrap();
    while jiff::Timestamp::now()
        <= expiry
            .checked_add(jiff::SignedDuration::from_millis(500))
            .unwrap()
    {
        std::thread::sleep(Duration::from_millis(100));
    }

    // The last admitted hello before revoke supplies the old expectation.
    let ticket = mother.ticket();
    let presented_before = presented(&mother, "peer-p");
    let (pre_hello_fact, pre_hello) =
        peers
            .p
            .hello(&ticket, &presented_before, VISION, "pre-revoke-p-hello");
    let old_expectation = pre_hello
        .as_ref()
        .and_then(|response| response.receiving_grants_authority.clone());
    let receiver_before = mother.receiver_authority();

    // Revoke peer-p under the fault.
    let cfg_dir = mother.config.parent().unwrap().to_path_buf();
    let restore = Restore(cfg_dir.clone());
    let failures_before = mother
        .all_logs()
        .matches("governed config projection failed")
        .count();
    let prepared = mother.prepare_command(
        "peer_revoke",
        "/peers/revoke",
        &json!({"peer_node_id": "peer-p"}),
    );
    let issued_at = prepared["intent"]["issued_at"]
        .as_str()
        .unwrap_or("")
        .to_owned();
    let clock = mother.work.join("clock.txt");
    match fault {
        Fault::G1Projection => {
            std::fs::set_permissions(&cfg_dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        }
        Fault::G1HeldPublication => std::fs::write(gate.join("pause-next"), "").unwrap(),
        Fault::G1cSteadyClock => {
            std::fs::write(&clock, format!("{issued_at}\n{issued_at}\n")).unwrap();
        }
        Fault::G1cRollbackClock => {
            let earlier = issued_at
                .parse::<jiff::Timestamp>()
                .unwrap()
                .checked_sub(jiff::SignedDuration::from_secs(1))
                .unwrap();
            std::fs::write(&clock, format!("{issued_at}\n{earlier}\n")).unwrap();
        }
        Fault::G1Control => {}
    }
    let signed = mother.sign_day(&prepared);
    let sender = send_in_background(&mut mother, &signed);
    let (held, revoke, copied_during_window, window, window_summary) =
        if fault == Fault::G1HeldPublication {
            let held = wait_for_hold(&gate, Duration::from_secs(20));
            let copied = mother.peer_entry("peer-p");
            let (window, summary) = if held.is_some() {
                window_probes(
                    &mother,
                    &peers,
                    old_expectation.as_ref(),
                    &pre_hello,
                    &presented_before.binding_id,
                )
            } else {
                (
                    serde_json::Map::new(),
                    json!({"skipped": "publication was not held"}),
                )
            };
            std::fs::write(gate.join("release"), "").unwrap();
            let revoke = sender.join().unwrap();
            (held, revoke, copied, window, summary)
        } else {
            let revoke = sender.join().unwrap();
            let copied = mother.peer_entry("peer-p");
            let (window, summary) = window_probes(
                &mother,
                &peers,
                old_expectation.as_ref(),
                &pre_hello,
                &presented_before.binding_id,
            );
            (None, revoke, copied, window, summary)
        };
    let clock_left = std::fs::read_to_string(&clock).unwrap_or_default();
    let revoke_facts: Vec<Value> = mother
        .admin_facts()
        .into_iter()
        .filter(|fact| fact["route"] == "/peers/revoke")
        .collect();
    let failures_after = mother
        .all_logs()
        .matches("governed config projection failed")
        .count();
    let copied_after_reply = mother.peer_entry("peer-p");
    let log_tail: String = {
        let log = mother.log();
        let lines: Vec<&str> = log.lines().collect();
        lines[lines.len().saturating_sub(25)..].join("\n")
    };

    // Retry and recovery, with the fault removed first.
    drop(restore);
    std::fs::write(&clock, "").unwrap();
    let retry = mother.govern(
        "peer_revoke",
        "/peers/revoke",
        &json!({"peer_node_id": "peer-p"}),
    );
    let after_retry = mother.binding_state("peer-p");
    let later = mother.govern(
        "peer_add",
        "/peers/add",
        &peer_body("peer-heal", "endpoint-heal", &Value::Null, &plus_secs(3600)),
    );
    let after_later = mother.binding_state("peer-p");
    let (post_recovery, post_hello_admitted, _) = probe(
        &mother,
        &peers.p,
        &presented(&mother, "peer-p"),
        VISION,
        None,
        echo,
        "recovered-p-echo",
    );

    json!({
        "fault": fault.label(),
        "ready": true,
        "baseline": baseline,
        "pre_revoke_hello": pre_hello_fact,
        "receiver_authority_before_revoke": receiver_before,
        "revoke": {
            "issued_at": issued_at,
            "held_span": held,
            "reply_status": reply_status(&revoke),
            "exit": revoke.status.code(),
            "stderr": text(&revoke.stderr),
            "witness_clock_lines_left": clock_left.lines().filter(|line| !line.trim().is_empty()).count(),
            "ledger_revoke_facts": revoke_facts,
            "projection_failures_logged": failures_after - failures_before,
            "copied_config_entry_during_window": copied_during_window,
            "copied_config_entry": copied_after_reply,
        },
        "window": window,
        "window_summary": window_summary,
        "daemon_log_tail_after_window": log_tail,
        "gate_trace": read_trace(&gate),
        "recovery": {
            "retry_revoke": retry,
            "binding_after_retry": after_retry,
            "later_command": later,
            "binding_after_later_command": after_later,
            "post_recovery_probe": post_recovery,
            "post_recovery_hello_admitted": post_hello_admitted,
        },
        "admin_facts": mother.admin_facts(),
    })
}

fn hello_outcome(fact: &Value) -> &str {
    fact["hello"]["outcome"].as_str().unwrap_or("none")
}

/// Gate isolation. Baseline controls run before any fault. Window controls
/// count only where inbound reads still work, which `q_missing_binding`
/// reaching an evaluation shows.
fn gate_prerequisites(facts: &Value, label: &str) -> Vec<Value> {
    let base = &facts["baseline"];
    let window = &facts["window"];
    let mut items = vec![
        prerequisite(
            &format!("{label}: baseline peer-p echo succeeds (positive control)"),
            runs_completed(&base["p_echo"]) == 1,
            base["p_echo"]["call"].clone(),
        ),
        prerequisite(
            &format!("{label}: baseline peer-p directory Child runs under its Toy grant"),
            runs_completed(&base["p_dir"]) == 1,
            base["p_dir"]["call"].clone(),
        ),
        prerequisite(
            &format!(
                "{label}: narrow grant (outsider only) denies peer-p2's directory Child while its echo runs"
            ),
            hello_outcome(&base["p2_dir_only_outsider_grant"]) == "admitted"
                && runs_completed(&base["p2_dir_only_outsider_grant"]) == 0
                && runs_completed(&base["p2_echo"]) == 1,
            json!([
                base["p2_dir_only_outsider_grant"]["call"],
                base["p2_echo"]["call"]
            ]),
        ),
        prerequisite(
            &format!("{label}: granting peer-p2 lets the same directory call run"),
            runs_completed(&base["p2_dir_granted"]) == 1,
            base["p2_dir_granted"]["call"].clone(),
        ),
        prerequisite(
            &format!("{label}: revoking peer-p2's grant denies the same directory call"),
            hello_outcome(&base["p2_dir_revoked_grant"]) == "admitted"
                && runs_completed(&base["p2_dir_revoked_grant"]) == 0,
            base["p2_dir_revoked_grant"]["call"].clone(),
        ),
        prerequisite(
            &format!("{label}: peer-e is admitted before its binding expires"),
            hello_outcome(&base["e_before_expiry"]) == "admitted",
            base["e_before_expiry"]["hello"].clone(),
        ),
    ];
    if inbound_reads_work(facts) {
        items.extend([
            prerequisite(
                &format!("{label}: unknown peer-q is denied at hello"),
                hello_outcome(&window["q_missing_binding"]) == "denied",
                window["q_missing_binding"]["hello_ledger"].clone(),
            ),
            prerequisite(
                &format!("{label}: wrong Vision is denied at hello"),
                hello_outcome(&window["p_wrong_vision"]) == "denied",
                window["p_wrong_vision"]["hello_ledger"].clone(),
            ),
            prerequisite(
                &format!("{label}: expired peer-e is denied at hello"),
                hello_outcome(&window["e_expired_binding"]) == "denied",
                window["e_expired_binding"]["hello_ledger"].clone(),
            ),
            prerequisite(
                &format!("{label}: in the window peer-p2's echo runs and its revoked-grant directory call is denied"),
                runs_completed(&window["p2_echo"]) == 1
                    && hello_outcome(&window["p2_dir_revoked_grant"]) == "admitted"
                    && runs_completed(&window["p2_dir_revoked_grant"]) == 0,
                json!([window["p2_echo"]["call"], window["p2_dir_revoked_grant"]["call"]]),
            ),
        ]);
    }
    items
}

/// The unknown-peer hello reached an evaluation instead of a transport failure.
fn inbound_reads_work(facts: &Value) -> bool {
    facts["window"]["q_missing_binding"]["hello"]["outcome"].is_string()
}

fn ready_prerequisite(facts: &Value, label: &str) -> Value {
    prerequisite(
        &format!("{label}: governed Mother became ready"),
        facts["ready"] == true,
        facts.get("log_tail").cloned().unwrap_or(Value::Null),
    )
}

fn revoke_committed(facts: &Value) -> bool {
    facts["revoke"]["ledger_revoke_facts"]
        .as_array()
        .is_some_and(|facts| {
            facts.iter().any(|fact| {
                fact["mentions"]
                    .as_array()
                    .is_some_and(|m| m.contains(&json!("peer-p")))
            })
        })
}

fn inbound_report(candidate: &str, control: &Value, injected: &Value, injection: &str) -> Value {
    let mut prerequisites = vec![
        ready_prerequisite(control, "control"),
        ready_prerequisite(injected, "injected"),
    ];
    let both_ready = control["ready"] == true && injected["ready"] == true;
    if both_ready {
        prerequisites.extend(gate_prerequisites(control, "control"));
        prerequisites.extend(gate_prerequisites(injected, "injected"));
        prerequisites.push(prerequisite(
            "control: inbound reads work in the window",
            inbound_reads_work(control),
            control["window"]["q_missing_binding"]["hello"].clone(),
        ));
        prerequisites.push(prerequisite(
            "both runs committed the peer-p revoke to the ledger",
            revoke_committed(control) && revoke_committed(injected),
            json!([
                control["revoke"]["ledger_revoke_facts"],
                injected["revoke"]["ledger_revoke_facts"]
            ]),
        ));
        prerequisites.push(prerequisite(
            "fault-free control denies peer-p's fresh hello after revoke",
            control["window_summary"]["fresh_hello_admitted"] == false
                && hello_outcome(&control["window"]["p_fresh_hello_and_echo_current_expectation"])
                    == "denied",
            control["window"]["p_fresh_hello_and_echo_current_expectation"]["hello_ledger"].clone(),
        ));
        prerequisites.push(prerequisite(
            "injected: the copied binding said admitted while the window probes ran",
            injected["revoke"]["copied_config_entry_during_window"]["binding_state"] == "admitted",
            injected["revoke"]["copied_config_entry_during_window"]["binding_state"].clone(),
        ));
    }
    let w = &injected["window"];
    let current = &w["p_fresh_hello_and_echo_current_expectation"];
    let old = &w["p_fresh_hello_and_echo_old_expectation"];
    let current_generation = current["hello"]["receiving_grants_authority"]["generation"].as_u64();
    let uds_generation = w["receiver_authority_from_control_socket"]["generation"].as_u64();
    let old_generation =
        injected["pre_revoke_hello"]["receiving_grants_authority"]["generation"].as_u64();
    let actual = json!({
        "injected_inbound_reads_work": inbound_reads_work(injected),
        "injected_fresh_hello": current["hello"],
        "injected_hello_receiver_generation": current_generation,
        "receiver_generation_from_control_socket": uds_generation,
        "pre_revoke_generation": old_generation,
        "injected_current_expectation_echo_runs_completed": runs_completed(current),
        "injected_current_expectation_dir_runs_completed": runs_completed(&w["p_fresh_hello_and_dir_current_expectation"]),
        "injected_old_expectation_echo_runs_completed": runs_completed(old),
        "injected_old_expectation_call": old["call"],
        "injected_old_expectation_call_ledger": old["call_ledger"],
        "injected_call_on_pre_revoke_hello": w["p_call_on_pre_revoke_hello_current_expectation"],
        "control_fresh_hello": control["window"]["p_fresh_hello_and_echo_current_expectation"]["hello"],
        "control_call_on_pre_revoke_hello": control["window"]["p_call_on_pre_revoke_hello_current_expectation"],
        "injected_post_recovery_hello": injected["recovery"]["post_recovery_probe"]["hello"],
        "injected_daemon_log_tail_after_window": injected["daemon_log_tail_after_window"],
    });
    let expected = json!({
        "confirmed": "injected: fresh hello admitted on the stale copied binding; the call carrying the current receiver expectation (generation after revoke, equal to the control socket's) runs the Child; the same call with the pre-revoke expectation runs nothing; control: the same fresh hello is denied",
        "prevented": "injected: every inbound hello, including the unknown-peer control, fails before evaluation while the fault is present, and evaluation resumes after the fault is removed",
        "refuted": "injected: inbound reads work, the copy says admitted, and the fresh hello or the current-expectation call is still denied",
    });
    let reads = inbound_reads_work(injected);
    let (classification, reason) = if let Some(name) = unmet(&prerequisites) {
        ("inconclusive", format!("prerequisite not met: {name}"))
    } else if !reads {
        let blanket = [
            "p_fresh_hello_and_echo_current_expectation",
            "q_missing_binding",
            "p_wrong_vision",
            "e_expired_binding",
            "p2_echo",
        ]
        .iter()
        .all(|key| w[*key]["hello"]["transport_error"].is_string());
        let resumed = injected["recovery"]["post_recovery_probe"]["hello"]["outcome"].is_string();
        if blanket && resumed {
            (
                "prevented",
                format!(
                    "under {injection}, every inbound hello failed before evaluation, the unknown-peer control included, so the stale copy admitted nothing; after the fault was removed, hellos were evaluated again (peer-p: {})",
                    injected["recovery"]["post_recovery_probe"]["hello"]["outcome"]
                ),
            )
        } else {
            prerequisites.push(prerequisite(
                "observed schedule matches a classified outcome",
                false,
                actual.clone(),
            ));
            (
                "inconclusive",
                "inbound reads failed, but not uniformly or not recoverably".to_owned(),
            )
        }
    } else if hello_outcome(current) != "admitted" {
        (
            "refuted",
            "inbound reads worked and the copy said admitted, yet the fresh hello was denied"
                .to_owned(),
        )
    } else if runs_completed(current) == 1
        && current_generation.is_some()
        && current_generation == uds_generation
        && current_generation > old_generation
        && runs_completed(old) == 0
    {
        (
            "confirmed",
            format!(
                "after the canonical revoke ({injection}), a fresh supported hello from the revoked peer was admitted on the copied binding, received the current receiver expectation (generation {current_generation:?}, equal to the control socket's), and the call made with it ran the Child; the same call with the pre-revoke expectation (generation {old_generation:?}) ran nothing; the fault-free control denied the same fresh hello"
            ),
        )
    } else if runs_completed(current) == 0 {
        (
            "refuted",
            "the fresh hello was admitted, but the current-expectation call did not run the Child"
                .to_owned(),
        )
    } else {
        prerequisites.push(prerequisite(
            "observed schedule matches a classified outcome",
            false,
            actual.clone(),
        ));
        (
            "inconclusive",
            "observations matched no classified schedule".to_owned(),
        )
    };
    publish(Report {
        candidate,
        case: "fresh supported inbound hello and call after canonical revoke",
        prerequisites,
        schedule: json!({
            "injection": injection,
            "control": "same preconditions and steps with no fault",
            "steps": [
                "offline: bootstrap, stage and approve resident-echo and resident-dir, grant dr-p (peer-p) and dr-outsider, admit peer-p and peer-p2",
                "serve with two runtime workers; baseline probes; signed grant then revoke of dr-p2; signed admit of peer-e with a 15 s expiry; wait until it expires",
                "pre-revoke hello from peer-p records the old receiver expectation",
                "signed /peers/revoke peer-p under the fault",
                "window: fresh hello plus echo and directory calls with the current expectation; fresh hello plus echo with the old expectation; a call on the pre-revoke hello; unknown, wrong-Vision, expired and revoked-grant controls",
                "recovery: fault removed, retry the revoke, one unrelated signed command, one fresh peer-p hello",
            ],
        }),
        facts: json!({"control": control, "injected": injected}),
        expected,
        actual,
        classification,
        reason,
        limitations: vec![
            "Loopback Iroh with relays disabled; synthetic peers use the public mct-iroh client API, not a second Mother.",
            "Run evidence is the receiving Mother's state.sqlite run rows and ledger facts, read after each call.",
            "The directory Child imports wasi:filesystem but never opens the preopen; its completed run shows Toy admission, not a file read.",
            "Exposure is bounded by the copied binding expiry; how often publication fails, lags or is skipped is not measured.",
        ],
    })
}

fn publication_report(candidate: &str, control: &Value, injected: &Value) -> Value {
    let g1c = candidate.starts_with("G1c");
    let mut prerequisites = vec![
        ready_prerequisite(control, "control"),
        ready_prerequisite(injected, "injected"),
    ];
    if g1c && control["ready"] == true && injected["ready"] == true {
        prerequisites.push(prerequisite(
            "both runs consumed the witness clock (hook present in this binary)",
            control["revoke"]["witness_clock_lines_left"] == 0
                && injected["revoke"]["witness_clock_lines_left"] == 0,
            json!([
                control["revoke"]["witness_clock_lines_left"],
                injected["revoke"]["witness_clock_lines_left"]
            ]),
        ));
    }
    let row = |facts: &Value| {
        json!({
            "reply": facts["revoke"]["reply_status"],
            "ledger_revoke_committed": revoke_committed(facts),
            "copied_binding_after_revoke": facts["revoke"]["copied_config_entry"]["binding_state"],
            "projection_failures_logged": facts["revoke"]["projection_failures_logged"],
            "retry_reply": facts["recovery"]["retry_revoke"]["status"],
            "binding_after_retry": facts["recovery"]["binding_after_retry"],
            "later_command_reply": facts["recovery"]["later_command"]["status"],
            "binding_after_later_command": facts["recovery"]["binding_after_later_command"],
        })
    };
    let actual = json!({"control": row(control), "injected": row(injected)});
    let c = &actual["control"];
    let i = &actual["injected"];
    let control_ok = c["reply"] == "completed"
        && c["ledger_revoke_committed"] == true
        && c["copied_binding_after_revoke"] == "revoked";
    prerequisites.push(prerequisite(
        "fault-free control: revoke completed, committed and published revoked",
        control_ok,
        c.clone(),
    ));
    let injected_reply = if g1c { "indeterminate" } else { "completed" };
    let (classification, reason) = if let Some(name) = unmet(&prerequisites) {
        ("inconclusive", format!("prerequisite not met: {name}"))
    } else if i["ledger_revoke_committed"] == true
        && i["reply"] == injected_reply
        && i["copied_binding_after_revoke"] == "admitted"
        && (g1c || i["projection_failures_logged"].as_u64().unwrap_or(0) >= 1)
        && i["retry_reply"] == "rejected"
        && i["binding_after_retry"] == "admitted"
        && i["later_command_reply"] == "completed"
        && i["binding_after_later_command"] == "revoked"
    {
        (
            "confirmed",
            format!(
                "with identical preconditions, the control published revoked; the injected run committed the revoke, replied {injected_reply}, left the copy admitted, the retry was rejected without republishing, and only a later unrelated completed command republished revoked"
            ),
        )
    } else if i["copied_binding_after_revoke"] == "revoked" {
        (
            "refuted",
            "the injected run still published the revocation".to_owned(),
        )
    } else {
        prerequisites.push(prerequisite(
            "observed schedule matches a classified outcome",
            false,
            actual.clone(),
        ));
        (
            "inconclusive",
            "observations matched no classified schedule".to_owned(),
        )
    };
    publish(Report {
        candidate,
        case: if g1c {
            "post-commit clock failure beside steady-clock control"
        } else {
            "publication failure beside fault-free control"
        },
        prerequisites,
        schedule: json!({
            "injection": if g1c { "witness clock: second read one second before the commit time" } else { "config directory mode 0500 during the revoke; private storage refuses the publisher's first read" },
            "control": if g1c { "witness clock: both reads equal the commit time" } else { "no fault" },
            "recovery": "fault removed; retry the same revoke; then signed /peers/add peer-heal",
        }),
        facts: json!({"control_revoke": control["revoke"], "injected_revoke": injected["revoke"], "control_recovery": control["recovery"], "injected_recovery": injected["recovery"], "control_admin_facts": control["admin_facts"], "injected_admin_facts": injected["admin_facts"]}),
        expected: json!({"control": "completed, committed, copy revoked", "injected": format!("{injected_reply}, committed, copy admitted; retry rejected; later command republishes revoked")}),
        actual,
        classification,
        reason,
        limitations: vec![
            "The publication fault is config directory mode 0500. Private storage then refuses the publisher's read as well as its write, and the inbound binding read too.",
            "A write-only publication failure (ENOSPC, EIO) that leaves reads working was not injected: it needs a filesystem fault or privileges this harness does not use.",
            "The clock fault uses the nic131-witness clock file; production reads the system clock twice.",
        ],
    })
}

#[test]
fn nic131_ext_g1_publication_failure_and_held_window_vs_fault_free_control() {
    let control = revoke_scenario(Fault::G1Control);
    let failed = revoke_scenario(Fault::G1Projection);
    let held = revoke_scenario(Fault::G1HeldPublication);
    assert_reported(&publication_report("G1-control-pair", &control, &failed));
    assert_reported(&inbound_report(
        "G1-inbound-failed-publication",
        &control,
        &failed,
        "publication failure: config directory mode 0500 during the revoke, which private storage refuses for reads as well as writes",
    ));
    assert_reported(&inbound_report(
        "G1-inbound-held-publication",
        &control,
        &held,
        "publication held inside its existing synchronous span until the window probes finish",
    ));
}

#[test]
fn nic131_ext_g1c_postcommit_clock_vs_steady_clock_control() {
    let control = revoke_scenario(Fault::G1cSteadyClock);
    let injected = revoke_scenario(Fault::G1cRollbackClock);
    assert_reported(&publication_report("G1c-control-pair", &control, &injected));
    assert_reported(&inbound_report(
        "G1c-inbound",
        &control,
        &injected,
        "post-commit clock rollback: Indeterminate reply, publication skipped",
    ));
}

// ---------------------------------------------------------------- G13 resident API

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

fn uds_ok(mother: &Mother, path: &str, body: &Value) -> Value {
    let (status, reply) = uds(&mother.socket, "POST", path, Some(body));
    json!({"http": status, "body": reply})
}

#[test]
fn nic131_ext_g13_resident_api_governed_origin_vs_legacy_proof_control() {
    let dir = private_tempdir();
    let mut origin = Mother::new(dir.path(), "mother-a", [7; 32]);
    let mut receiver = Mother::new(dir.path(), "mother-b", [9; 32]);
    let mut legacy = Mother::new(dir.path(), "mother-l", [5; 32]);
    origin.bootstrap();
    receiver.bootstrap();
    stage_and_approve(&mut origin, "resident-echo", &echo_wasm(), ECHO_MANIFEST);
    stage_and_approve(
        &mut receiver,
        "resident-remote",
        &remote_wasm(),
        &remote_manifest(),
    );
    let ready = [origin.start(), receiver.start(), legacy.start()];
    let mut prerequisites: Vec<Value> = ready
        .iter()
        .zip([
            "mother-a governed",
            "mother-b governed",
            "mother-l legacy (virgin, Unix-socket administration)",
        ])
        .map(|(result, name)| {
            prerequisite(
                &format!("{name} became ready"),
                result.is_ok(),
                json!(result.as_ref().err()),
            )
        })
        .collect();
    let mut facts = serde_json::Map::new();
    if unmet(&prerequisites).is_none() {
        let a_ticket = origin.ticket_value();
        let b_ticket = receiver.ticket_value();
        let l_ticket = legacy.ticket_value();
        let endpoint = |ticket: &Value| ticket["endpoint_id"].as_str().unwrap_or("").to_owned();
        // Receiver admits both callers by signed command; the governed origin admits the receiver.
        facts.insert(
            "b_admits_a".into(),
            receiver.govern(
                "peer_add",
                "/peers/add",
                &peer_body(
                    "mother-a",
                    &endpoint(&a_ticket),
                    &a_ticket,
                    &plus_secs(3600),
                ),
            ),
        );
        facts.insert(
            "b_admits_l".into(),
            receiver.govern(
                "peer_add",
                "/peers/add",
                &peer_body(
                    "mother-l",
                    &endpoint(&l_ticket),
                    &l_ticket,
                    &plus_secs(3600),
                ),
            ),
        );
        facts.insert(
            "a_admits_b".into(),
            origin.govern(
                "peer_add",
                "/peers/add",
                &peer_body(
                    "mother-b",
                    &endpoint(&b_ticket),
                    &b_ticket,
                    &plus_secs(3600),
                ),
            ),
        );
        // The legacy caller records the receiver and the receiver-issued proof through its supported Unix-socket routes.
        let b_for_l = receiver.peer_entry("mother-l");
        facts.insert("l_adds_b".into(), uds_ok(&legacy, "/peers/add", &json!({
            "expected_config_path": legacy.config, "peer_node_id": "mother-b", "binding_id": "binding-l-admits-b",
            "endpoint_id": endpoint(&b_ticket), "vision_id": VISION, "ticket": b_ticket,
            "binding_signature_ref": null, "policy_revision": 1, "expires_at": plus_secs(3600)
        })));
        facts.insert("l_installs_b_issued_proof".into(), uds_ok(&legacy, "/peers/proof", &json!({
            "expected_config_path": legacy.config, "peer_node_id": "mother-b",
            "binding_id": b_for_l["binding_id"], "policy_revision": b_for_l["policy_revision"],
            "signature_ref": b_for_l["binding_signature_ref"], "expires_at": b_for_l["expires_at"]
        })));
        // Restart both callers: the refresh loop's first tick is immediate.
        let restarted = [origin.restart(), legacy.restart()];
        prerequisites.push(prerequisite(
            "callers restarted for an immediate peer refresh",
            restarted.iter().all(Result::is_ok),
            json!(
                restarted
                    .iter()
                    .map(|r| r.as_ref().err())
                    .collect::<Vec<_>>()
            ),
        ));
        let deadline = Instant::now() + Duration::from_secs(30);
        while surfaces(&legacy, "mother-b") == 0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(Duration::from_secs(2));
        let a_entry = origin.peer_entry("mother-b");
        let l_entry = legacy.peer_entry("mother-b");
        facts.insert(
            "a_config_entry_for_b".into(),
            json!({
                "binding_state": a_entry["binding_state"],
                "local_signature_present": a_entry["binding_signature_ref"].is_string(),
                "outbound_proof_present": !a_entry["outbound_binding"].is_null(),
                "ticket_present": !a_entry["ticket"].is_null(),
            }),
        );
        facts.insert(
            "l_config_entry_for_b".into(),
            json!({
                "binding_state": l_entry["binding_state"],
                "outbound_proof_present": !l_entry["outbound_binding"].is_null(),
            }),
        );
        let a_surfaces = surfaces(&origin, "mother-b");
        let l_surfaces = surfaces(&legacy, "mother-b");
        facts.insert(
            "remote_surfaces".into(),
            json!({"a_from_b": a_surfaces, "l_from_b": l_surfaces}),
        );
        let remote = (REMOTE_NAMESPACE, REMOTE_INTERFACE);
        let unknown = ("patina:nic131-none", "control@0.1.0");
        let b_runs_0 = receiver.completed_runs_of("resident-remote");
        let l_valid = resident_call(&legacy, remote, "remote-valid-proof");
        let b_runs_1 = receiver.completed_runs_of("resident-remote");
        let a_remote = resident_call(&origin, remote, "remote-governed");
        let b_runs_2 = receiver.completed_runs_of("resident-remote");
        let a_local = resident_call(&origin, (ECHO_NAMESPACE, ECHO_INTERFACE), "local-echo");
        let a_unknown = resident_call(&origin, unknown, "unknown-target");
        let l_unknown = resident_call(&legacy, unknown, "unknown-target");
        facts.insert("l_replaces_proof_with_wrong_signature".into(), uds_ok(&legacy, "/peers/proof", &json!({
            "expected_config_path": legacy.config, "peer_node_id": "mother-b",
            "binding_id": b_for_l["binding_id"], "policy_revision": b_for_l["policy_revision"],
            "signature_ref": "synthetic-wrong-signature", "expires_at": b_for_l["expires_at"]
        })));
        let l_wrong = resident_call(&legacy, remote, "remote-wrong-proof");
        let b_runs_3 = receiver.completed_runs_of("resident-remote");
        let a_log = origin.all_logs();
        facts.insert("calls".into(), json!({
            "l_remote_valid_proof": l_valid, "a_remote_governed": a_remote, "a_local_echo": a_local,
            "a_unknown_target": a_unknown, "l_unknown_target": l_unknown, "l_remote_wrong_proof": l_wrong,
        }));
        facts.insert("receiver_completed_remote_runs".into(), json!({
            "before": b_runs_0, "after_l_valid": b_runs_1, "after_a_governed": b_runs_2, "after_l_wrong_proof": b_runs_3,
        }));
        facts.insert(
            "a_log_mentions_refresh_to_b".into(),
            json!(a_log.contains("refresh hello to mother-b")),
        );
        for (name, key) in [
            ("b admitted mother-a", "b_admits_a"),
            ("b admitted mother-l", "b_admits_l"),
            ("a admitted mother-b", "a_admits_b"),
        ] {
            prerequisites.push(prerequisite(
                name,
                facts[key]["status"] == "completed",
                facts[key].clone(),
            ));
        }
        prerequisites.push(prerequisite(
            "legacy proof installed through /peers/proof",
            facts["l_installs_b_issued_proof"]["http"] == 200
                && l_entry["outbound_binding"].is_object(),
            facts["l_installs_b_issued_proof"].clone(),
        ));
        prerequisites.push(prerequisite(
            "legacy caller learned mother-b's callable surface",
            l_surfaces > 0,
            json!(l_surfaces),
        ));
        prerequisites.push(prerequisite(
            "present-proof control ran the remote Child on mother-b through the resident API",
            facts["calls"]["l_remote_valid_proof"]["outcome"] == "completed"
                && b_runs_1 == b_runs_0 + 1,
            facts["calls"]["l_remote_valid_proof"].clone(),
        ));
        prerequisites.push(prerequisite(
            "governed origin's local call runs",
            facts["calls"]["a_local_echo"]["outcome"] == "completed",
            facts["calls"]["a_local_echo"].clone(),
        ));
        prerequisites.push(prerequisite(
            "governed origin admitted mother-b in its copy",
            a_entry["binding_state"] == "admitted",
            a_entry["binding_state"].clone(),
        ));
    }
    let calls = facts.get("calls").cloned().unwrap_or(Value::Null);
    let runs = facts
        .get("receiver_completed_remote_runs")
        .cloned()
        .unwrap_or(Value::Null);
    let a_surfaces = facts
        .get("remote_surfaces")
        .map(|s| s["a_from_b"].clone())
        .unwrap_or(Value::Null);
    let a_proof = facts
        .get("a_config_entry_for_b")
        .map(|e| e["outbound_proof_present"].clone())
        .unwrap_or(Value::Null);
    let actual = json!({
        "a_outbound_proof_present": a_proof,
        "a_remote_surfaces_from_b": a_surfaces,
        "a_remote_call": calls["a_remote_governed"],
        "a_unknown_target_call": calls["a_unknown_target"],
        "l_wrong_proof_call": calls["l_remote_wrong_proof"],
        "receiver_completed_remote_runs": runs,
    });
    let (classification, reason) = if let Some(name) = unmet(&prerequisites) {
        ("inconclusive", format!("prerequisite not met: {name}"))
    } else if calls["a_remote_governed"]["outcome"] == "completed"
        || runs["after_a_governed"] != runs["after_l_valid"]
    {
        (
            "refuted",
            "the governed origin's resident call reached the remote Child".to_owned(),
        )
    } else if a_proof == false && a_surfaces == 0 {
        (
            "confirmed",
            format!(
                "with mutual signed admission, the governed origin holds no outbound proof for mother-b and learned no callable surface from it, so its resident /calls request was not routed (outcome {}, first barrier: no remote candidate); the same target through the same receiver ran under a legacy caller holding the receiver-issued proof; replacing that proof with a wrong signature stopped the remote run, and the unknown-target call returned {}",
                calls["a_remote_governed"]["outcome"], calls["a_unknown_target"]["outcome"]
            ),
        )
    } else {
        prerequisites.push(prerequisite(
            "observed schedule matches a classified outcome",
            false,
            actual.clone(),
        ));
        (
            "inconclusive",
            "observations matched no classified schedule".to_owned(),
        )
    };
    let report = publish(Report {
        candidate: "G13-resident",
        case: "governed origin resident /calls to a peer-only Child, beside a legacy caller holding a receiver-issued proof",
        prerequisites,
        schedule: json!({
            "injection": "none: the governed origin uses only signed commands and has no supported proof writer",
            "controls": ["legacy caller with receiver-issued proof (present proof)", "legacy caller with wrong signature (wrong proof)", "unknown target", "governed origin local Child"],
            "steps": [
                "offline: bootstrap mother-a and mother-b; stage and approve resident-echo on a and resident-remote on b",
                "serve a, b and a virgin legacy mother-l",
                "b admits a and l; a admits b; all by signed /peers/add over mct/admin/1",
                "l adds b and installs b's own binding signature for l through /peers/add and /peers/proof on its Unix socket",
                "restart a and l so the refresh loop's immediate first tick runs with the admitted peer",
                "resident /calls: l to b's Child; a to b's Child; a local; a and l unknown target; l with a wrong proof",
            ],
        }),
        facts: Value::Object(facts),
        expected: json!({"if_confirmed": "a's remote call does not run on b and b's completed run count does not change; l's identical call does", "if_refuted": "a's resident call reaches mother-b"}),
        actual,
        classification,
        reason,
        limitations: vec![
            "Three production daemon processes on loopback, relays disabled; no public in-process governed Mother constructor exists.",
            "The present-proof control uses the legacy profile's supported /peers/proof route; a governed Mother refuses it by design.",
            "A missing callable surface stops the governed call before candidate proof checks; the wrong-proof control exercises those checks with a surface present.",
        ],
    });
    assert_reported(&report);
}

// ---------------------------------------------------------------- G1b resident scheduler

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Order {
    /// Command A's publication is held after it captured snapshot R; B commits and publishes R+1; A is released.
    HeldA,
    /// No hold: A publishes, then B is prepared against the new generation.
    Reversed,
    /// B is prepared before A commits, so it carries A's predecessor generation.
    StaleGeneration,
}

fn read_trace(gate: &Path) -> Vec<Value> {
    std::fs::read_to_string(gate.join("trace.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn scheduler_run(order: Order, workers: u32) -> Value {
    let dir = private_tempdir();
    let gate = dir.path().join("gate");
    private_fs::ensure_directory(&gate).unwrap();
    let mut mother = Mother::new(dir.path(), "mother-a", [7; 32]);
    mother.env = vec![
        ("NIC131_GATE_DIR".into(), gate.to_str().unwrap().into()),
        ("NIC131_GATE_MAX_HOLD_MS".into(), "20000".into()),
        ("TOKIO_WORKER_THREADS".into(), workers.to_string()),
    ];
    mother.bootstrap();
    mother.command_offline(
        "peer_add",
        "/peers/add",
        &peer_body("peer-p", "endpoint-p", &Value::Null, &plus_secs(3600)),
    );
    std::fs::write(mother.work.join("clock.txt"), "").unwrap();
    if let Err(log) = mother.start() {
        return json!({"order": format!("{order:?}"), "workers": workers, "ready": false, "log_tail": log});
    }
    let boot_binding = mother.binding_state("peer-p");
    let a_signed = mother.signed_command(
        "peer_add",
        "/peers/add",
        &peer_body("peer-q", "endpoint-q", &Value::Null, &plus_secs(3600)),
    );
    let b_early = (order == Order::StaleGeneration).then(|| {
        mother.signed_command(
            "peer_revoke",
            "/peers/revoke",
            &json!({"peer_node_id": "peer-p"}),
        )
    });
    if order == Order::HeldA {
        std::fs::write(gate.join("pause-next"), "").unwrap();
    }
    let ticket = mother.work.join("resident-ticket.json");
    put(&ticket, &mother.ticket_value());
    let a_request = mother.file(&a_signed);
    let send = |request: String, ticket: PathBuf| {
        std::thread::spawn(move || {
            let started = now_text();
            let output = run(
                &[
                    "passkey-admin".into(),
                    "send-iroh".into(),
                    "--ticket".into(),
                    ticket.to_str().unwrap().into(),
                    "--request".into(),
                    request,
                ],
                45,
            );
            (started, now_text(), output)
        })
    };
    let a_thread = send(a_request, ticket.clone());
    let mut held = Value::Null;
    let mut binding_while_held = Value::Null;
    let mut b_before_release = Value::Null;
    let b_result;
    match order {
        Order::HeldA => {
            let deadline = Instant::now() + Duration::from_secs(20);
            let paused = loop {
                let found = std::fs::read_dir(&gate).ok().and_then(|entries| {
                    entries
                        .filter_map(Result::ok)
                        .find(|entry| entry.file_name().to_string_lossy().starts_with("paused-"))
                });
                if let Some(entry) = found {
                    break std::fs::read_to_string(entry.path()).ok();
                }
                if Instant::now() >= deadline {
                    break None;
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            held = paused
                .as_deref()
                .and_then(|text| serde_json::from_str(text).ok())
                .unwrap_or(Value::Null);
            // B is prepared now, against the authority A already committed.
            let b_signed = mother.signed_command(
                "peer_revoke",
                "/peers/revoke",
                &json!({"peer_node_id": "peer-p"}),
            );
            let b_thread = send(mother.file(&b_signed), ticket.clone());
            let wait = Instant::now() + Duration::from_secs(8);
            while !b_thread.is_finished() && Instant::now() < wait {
                std::thread::sleep(Duration::from_millis(20));
            }
            let finished_before_release = b_thread.is_finished();
            binding_while_held = mother.binding_state("peer-p");
            b_before_release = json!(finished_before_release);
            std::fs::write(gate.join("release"), "").unwrap();
            b_result = Some(b_thread.join().unwrap());
        }
        Order::Reversed | Order::StaleGeneration => {
            b_result = None;
        }
    }
    let a_result = a_thread.join().unwrap();
    let b_result = match (order, b_result) {
        (Order::HeldA, Some(result)) => result,
        (Order::Reversed, _) => {
            let b_signed = mother.signed_command(
                "peer_revoke",
                "/peers/revoke",
                &json!({"peer_node_id": "peer-p"}),
            );
            send(mother.file(&b_signed), ticket.clone()).join().unwrap()
        }
        (_, _) => send(mother.file(b_early.as_ref().unwrap()), ticket.clone())
            .join()
            .unwrap(),
    };
    let outcome = |result: &(String, String, Output)| {
        json!({
            "sent_at": result.0, "returned_at": result.1,
            "status": reply_status(&result.2), "stderr": text(&result.2.stderr),
        })
    };
    let available = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(0);
    json!({
        "order": format!("{order:?}"),
        "workers": workers,
        "host_available_parallelism": available,
        "ready": true,
        "binding_at_boot": boot_binding,
        "held_span": held,
        "b_reply_returned_before_release": b_before_release,
        "binding_while_a_held": binding_while_held,
        "a_reply": outcome(&a_result),
        "b_reply": outcome(&b_result),
        "final_binding_p": mother.binding_state("peer-p"),
        "final_binding_q": mother.binding_state("peer-q"),
        "admin_facts": mother.admin_facts(),
        "trace": read_trace(&gate),
    })
}

fn spans(run: &Value) -> Vec<Value> {
    run["trace"].as_array().cloned().unwrap_or_default()
}

/// Ledger fact for the peer-p revoke and the peer-q add, in commit order.
fn commit_order(run: &Value) -> Vec<String> {
    run["admin_facts"]
        .as_array()
        .map(|facts| {
            facts
                .iter()
                .filter_map(|fact| {
                    let mentions = fact["mentions"].as_array()?;
                    match fact["route"].as_str()? {
                        "/peers/add" if mentions.contains(&json!("peer-q")) => {
                            Some(format!("add-q@{}", fact["generation"]))
                        }
                        "/peers/revoke" if mentions.contains(&json!("peer-p")) => {
                            Some(format!("revoke-p@{}", fact["generation"]))
                        }
                        _ => None,
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn nic131_ext_g1b_resident_scheduler_held_publication() {
    let held_two = scheduler_run(Order::HeldA, 2);
    let held_one = scheduler_run(Order::HeldA, 1);
    let reverse = scheduler_run(Order::Reversed, 2);
    let stale = scheduler_run(Order::StaleGeneration, 2);
    let mut prerequisites: Vec<Value> = [
        (&held_two, "held, two workers"),
        (&held_one, "held, one worker"),
        (&reverse, "reverse order"),
        (&stale, "stale generation"),
    ]
    .iter()
    .map(|(run, label)| {
        prerequisite(
            &format!("{label}: governed Mother ready with peer-p admitted at boot"),
            run["ready"] == true && run["binding_at_boot"] == "admitted",
            run.get("log_tail")
                .cloned()
                .unwrap_or(run["binding_at_boot"].clone()),
        )
    })
    .collect();
    // Ordering obligations, derived from serving.rs and observation.rs at the pin:
    // O1 the writer actor commits A, then answers A's projected_authority, so A's snapshot is R (peer-p admitted);
    // O2 B can commit only if prepared against R's generation (generation is checked before commit);
    // O3 A's publication is one synchronous span after its await; it can finish after B's only if another worker runs B meanwhile.
    let held_a_span = spans(&held_two)
        .into_iter()
        .find(|event| event["event"] == "hold")
        .map(|event| event["seq"].clone())
        .unwrap_or(Value::Null);
    let enter = |run: &Value, seq: &Value| {
        spans(run)
            .into_iter()
            .find(|event| event["event"] == "enter" && &event["seq"] == seq)
            .unwrap_or(Value::Null)
    };
    let a_enter = enter(&held_two, &held_a_span);
    let b_enter = spans(&held_two)
        .into_iter()
        .find(|event| event["event"] == "enter" && event["snapshot_peers"]["peer-p"] == "revoked")
        .unwrap_or(Value::Null);
    let index_of =
        |run: &Value, predicate: &dyn Fn(&Value) -> bool| spans(run).iter().position(predicate);
    let b_exit_index = index_of(&held_two, &|event| {
        event["event"] == "exit" && event["seq"] == b_enter["seq"] && event["published"] == true
    });
    let a_release_index = index_of(&held_two, &|event| {
        event["event"] == "released" && event["seq"] == held_a_span
    });
    let a_exit_index = index_of(&held_two, &|event| {
        event["event"] == "exit" && event["seq"] == held_a_span && event["published"] == true
    });
    let actual = json!({
        "held_two_workers": {
            "commit_order": commit_order(&held_two),
            "a_snapshot": {"generation": a_enter["snapshot_generation"], "peer_p": a_enter["snapshot_peers"]["peer-p"], "thread": a_enter["thread"]},
            "b_snapshot": {"generation": b_enter["snapshot_generation"], "peer_p": b_enter["snapshot_peers"]["peer-p"], "thread": b_enter["thread"]},
            "b_published_before_a_released": matches!((b_exit_index, a_release_index), (Some(b), Some(a)) if b < a),
            "a_published_after_release": matches!((a_release_index, a_exit_index), (Some(r), Some(e)) if r < e),
            "binding_while_a_held": held_two["binding_while_a_held"],
            "a_reply": held_two["a_reply"]["status"], "b_reply": held_two["b_reply"]["status"],
            "final_binding_p": held_two["final_binding_p"],
        },
        "held_one_worker": {
            "commit_order": commit_order(&held_one),
            "b_reply_returned_before_release": held_one["b_reply_returned_before_release"],
            "a_reply": held_one["a_reply"]["status"], "b_reply": held_one["b_reply"]["status"],
            "final_binding_p": held_one["final_binding_p"],
        },
        "reverse_order": {"commit_order": commit_order(&reverse), "a_reply": reverse["a_reply"]["status"], "b_reply": reverse["b_reply"]["status"], "final_binding_p": reverse["final_binding_p"]},
        "stale_generation": {"commit_order": commit_order(&stale), "a_reply": stale["a_reply"]["status"], "b_reply": stale["b_reply"]["status"], "b_stderr": stale["b_reply"]["stderr"], "final_binding_p": stale["final_binding_p"]},
    });
    let h2 = &actual["held_two_workers"];
    prerequisites.push(prerequisite(
        "held run: the gate held command A's publication span",
        !held_a_span.is_null(),
        held_two["held_span"].clone(),
    ));
    prerequisites.push(prerequisite(
        "reverse-order control: both commands completed and the copy ends revoked",
        actual["reverse_order"]["a_reply"] == "completed"
            && actual["reverse_order"]["b_reply"] == "completed"
            && actual["reverse_order"]["final_binding_p"] == "revoked",
        actual["reverse_order"].clone(),
    ));
    prerequisites.push(prerequisite("stale-generation control: B prepared before A is rejected and the copy stays admitted with the ledger", actual["stale_generation"]["a_reply"] == "completed" && actual["stale_generation"]["b_reply"] == "rejected" && actual["stale_generation"]["final_binding_p"] == "admitted", actual["stale_generation"].clone()));
    let a_gen = a_enter["snapshot_generation"].as_u64();
    let b_gen = b_enter["snapshot_generation"].as_u64();
    let (classification, reason) = if let Some(name) = unmet(&prerequisites) {
        ("inconclusive", format!("prerequisite not met: {name}"))
    } else if h2["a_reply"] == "completed"
        && h2["b_reply"] == "completed"
        && h2["a_snapshot"]["peer_p"] == "approved"
        && h2["b_snapshot"]["peer_p"] == "revoked"
        && a_gen.is_some()
        && b_gen == a_gen.map(|g| g + 1)
        && h2["b_published_before_a_released"] == true
        && h2["a_published_after_release"] == true
        && h2["binding_while_a_held"] == "revoked"
        && h2["final_binding_p"] == "admitted"
        && a_enter["thread"]["id"] != b_enter["thread"]["id"]
    {
        let one = &actual["held_one_worker"];
        let one_note = if one["b_reply_returned_before_release"] == false
            && one["final_binding_p"] == "revoked"
        {
            "with one worker the same hold kept B from publishing until A finished, and the copy ended revoked"
        } else {
            "the one-worker run did not show that prevention; see facts"
        };
        (
            "confirmed",
            format!(
                "on the resident admin path with two workers, A committed and captured generation {a_gen:?} with peer-p admitted; while A's existing synchronous publication span was held on one thread, B was prepared against the now-current authority, committed generation {b_gen:?} and published revoked from another thread; A then published its older snapshot last, so both replies completed and the copy ended admitted while the ledger says revoked; {one_note}"
            ),
        )
    } else if h2["final_binding_p"] == "revoked" && h2["b_published_before_a_released"] == false {
        ("prevented", "with two workers the hold still kept B from publishing before A; the copy ended revoked".to_owned())
    } else {
        prerequisites.push(prerequisite(
            "observed schedule matches a classified outcome",
            false,
            actual.clone(),
        ));
        (
            "inconclusive",
            "observations matched no classified schedule".to_owned(),
        )
    };
    let report = publish(Report {
        candidate: "G1b-resident",
        case: "resident admin path: held publication of command A against B's newer publication",
        prerequisites,
        schedule: json!({
            "obligations": [
                "O1: the writer actor commits A, then serves A's projected_authority, so A's snapshot R names peer-p admitted",
                "O2: B commits only when prepared against the generation A produced; a B prepared earlier is rejected before commit",
                "O3: A's publication runs synchronously after its await; it can finish after B's only if another worker runs B meanwhile",
            ],
            "hook": "nic131-witness gate inside project_governed_config_from, after the snapshot is read and before config.json is written; it blocks the thread and adds no await",
            "variants": ["held A, TOKIO_WORKER_THREADS=2", "held A, TOKIO_WORKER_THREADS=1", "reverse order (no hold)", "stale generation (B prepared before A)"],
        }),
        facts: json!({"held_two_workers": held_two, "held_one_worker": held_one, "reverse_order": reverse, "stale_generation": stale}),
        expected: json!({"if_confirmed": "two workers: both replies completed, B published revoked first, A published admitted last; one worker: B waits for A", "if_prevented": "B cannot publish while A's span is held"}),
        actual,
        classification,
        reason,
        limitations: vec![
            "The hold stands in for a preempted worker thread or slow storage during A's existing synchronous span; how often that happens is not measured.",
            "TOKIO_WORKER_THREADS is set explicitly; the default is one worker per available CPU.",
            "Command A adds peer-q; any completed governed command publishes the same way.",
        ],
    });
    assert_reported(&report);
}
