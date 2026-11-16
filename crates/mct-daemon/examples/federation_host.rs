#![cfg(unix)]
#![forbid(unsafe_code)]

use anyhow::{Context, Result, bail, ensure};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use mct_daemon::{
    MctDaemonConfigStore, MctLocalNodeIdentity, control_transport::control_socket_request,
};
use mct_iroh::{
    MotherIrohEndpointLifecycle, MotherIrohEndpointSnapshot, MotherIrohEndpointTicket,
    MotherIrohRelayMode,
};
use mct_kernel::{EndpointIdText, MctNodeId, ObservationKind, VisionId};
use mct_observation::{MctObservationLedgerEntry, private_fs, verify_ledger_streaming};
use rustix::process::{Pid, Signal, kill_process};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    net::SocketAddr,
    os::unix::{fs::DirBuilderExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const MAX_LINE: usize = 1024 * 1024;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);
const CALL_EXCHANGE_TIMEOUT: Duration = Duration::from_secs(90);
const READY_TIMEOUT: Duration = Duration::from_secs(120);
const STOP_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Fixture {
        index: u32,
    },
    Init {
        root: PathBuf,
        node: String,
        vision: String,
        bind: SocketAddr,
        binary: PathBuf,
        fixture: Fixture,
    },
    Start {
        binary: Option<PathBuf>,
    },
    Stop {
        #[serde(default)]
        crash: bool,
    },
    Control {
        method: String,
        path: String,
        body: Option<Value>,
    },
    Provision {},
    Peers {},
    Call {
        destination: u32,
        label: String,
    },
    ProvisionRepoReport {
        package: PathBuf,
        snapshot: RepoSnapshot,
    },
    GrantRepoReport {
        caller_node: Option<String>,
        mutation_id: String,
        expires_at: String,
    },
    MeshJob {
        job_id: String,
        pieces: Vec<MeshJobPiece>,
    },
    CallTarget {
        call_id: String,
        payload_json: String,
        target: Target,
    },
    Verify {},
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Target {
    namespace: String,
    interface_name: String,
    function_name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MeshJobPiece {
    piece_id: String,
    required_node_id: Option<String>,
    payload_json: String,
    target: Target,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum RepoSnapshot {
    A,
    B,
}

impl RepoSnapshot {
    fn as_str(self) -> &'static str {
        match self {
            Self::A => "a",
            Self::B => "b",
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Fixture {
    component_base64: String,
    manifest: String,
    component_blake3: String,
    manifest_sha256: String,
    index: u32,
}

struct Checkpoint {
    length: u64,
    sha256: String,
}

impl Checkpoint {
    fn check_prefix(&self, reader: impl Read) -> Result<()> {
        let (length, digest) = hash_reader(reader.take(self.length))?;
        ensure!(
            length == self.length && digest == self.sha256,
            "previous ledger byte prefix was not preserved"
        );
        Ok(())
    }
}

struct Host {
    root: PathBuf,
    node: MctNodeId,
    vision: VisionId,
    bind: SocketAddr,
    binary: PathBuf,
    fixture: Fixture,
    process: Option<Child>,
    boot: u64,
    endpoint_id: Option<String>,
    checkpoint: Option<Checkpoint>,
    recovery_required: bool,
    repo_report: Option<RepoReportFixture>,
    _ownership_lock: fs::File,
}

struct RepoReportFixture {
    data_root: PathBuf,
    input_sha256: String,
    artifact_id: String,
}

fn validate_name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 64,
        "name must contain 1 to 64 ASCII characters"
    );
    ensure!(
        value.as_bytes()[0].is_ascii_alphanumeric()
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "name must start with an alphanumeric and contain only alphanumerics, dash or underscore"
    );
    Ok(())
}

fn validate_absolute(path: &Path) -> Result<()> {
    let text = path.to_str().context("path must be UTF-8")?;
    ensure!(
        path.is_absolute() && text != "/",
        "path must be absolute and not filesystem root"
    );
    ensure!(
        text.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-+".contains(&b)),
        "unsafe path characters"
    );
    ensure!(
        text[1..]
            .split('/')
            .all(|part| !matches!(part, "" | "." | "..")),
        "unsafe path components"
    );
    Ok(())
}

fn validate_bind(bind: SocketAddr) -> Result<()> {
    ensure!(
        bind.port() != 0 && matches!(bind.ip(), std::net::IpAddr::V4(ip) if ip.is_private()),
        "bind must be an RFC1918 IPv4 socket with a nonzero port"
    );
    Ok(())
}

fn lan_ticket(
    snapshot: &MotherIrohEndpointSnapshot,
    endpoint: &EndpointIdText,
    bind: SocketAddr,
) -> Result<MotherIrohEndpointTicket> {
    ensure!(
        snapshot.lifecycle == MotherIrohEndpointLifecycle::Bound,
        "endpoint is not bound"
    );
    ensure!(
        snapshot.relay_mode == MotherIrohRelayMode::Disabled && snapshot.relay_urls.is_empty(),
        "relay-enabled daemon refused"
    );
    ensure!(
        snapshot
            .direct_addresses
            .iter()
            .any(|s| s.parse::<SocketAddr>().ok() == Some(bind)),
        "endpoint does not advertise the exact configured LAN socket"
    );
    ensure!(
        &snapshot.endpoint_id == endpoint,
        "status endpoint differs from persisted identity"
    );
    Ok(MotherIrohEndpointTicket {
        endpoint_id: snapshot.endpoint_id.clone(),
        direct_addresses: vec![bind.to_string()],
        relay_urls: Vec::new(),
    })
}

fn validate_index(index: u32) -> Result<()> {
    ensure!(
        index <= (i32::MAX - 41) as u32,
        "fixture index would overflow the signed arithmetic result"
    );
    Ok(())
}

fn validate_target(target: &Target) -> Result<()> {
    for field in [
        &target.namespace,
        &target.interface_name,
        &target.function_name,
    ] {
        ensure!(
            !field.trim().is_empty() && field.len() <= 256,
            "invalid target field"
        );
    }
    Ok(())
}

fn validated_payload(payload_json: &str) -> Result<&[u8]> {
    ensure!(payload_json.len() <= 32 * 1024, "payload too large");
    ensure!(
        serde_json::from_str::<Value>(payload_json)?.is_array(),
        "payload must be a JSON array"
    );
    Ok(payload_json.as_bytes())
}

fn read_bounded_regular(path: &Path, max: usize) -> Result<Vec<u8>> {
    validate_absolute(path)?;
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file(),
        "fixture path is not a regular file"
    );
    ensure!(
        metadata.len() <= max as u64,
        "fixture file exceeds size bound"
    );
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    fs::File::open(path)?
        .take((max + 1) as u64)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= max, "fixture file exceeds size bound");
    Ok(bytes)
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn file_sha256(file: fs::File) -> Result<String> {
    let mut command = if cfg!(target_os = "linux") {
        Command::new("sha256sum")
    } else {
        let mut command = Command::new("shasum");
        command.args(["-a", "256"]);
        command
    };
    let output = command.stdin(Stdio::from(file)).output()?;
    ensure!(output.status.success(), "executable SHA256 command failed");
    let text = std::str::from_utf8(&output.stdout)?;
    let mut fields = text.split_whitespace();
    let digest = fields.next().context("missing executable SHA256")?;
    ensure!(
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid executable SHA256"
    );
    ensure!(
        fields.next() == Some("-") && fields.next().is_none(),
        "unexpected SHA256 output"
    );
    Ok(digest.to_owned())
}

fn decode_result_payload(reply: &mut Value) -> Result<()> {
    let decoded = match reply.get("inline_result_payload_base64") {
        Some(Value::String(encoded)) => serde_json::from_slice::<Value>(&BASE64.decode(encoded)?)?,
        Some(Value::Null) => Value::Null,
        _ => bail!("call response has malformed or missing result payload"),
    };
    ensure!(
        reply.get("outcome").is_some_and(Value::is_string),
        "call response has no outcome"
    );
    reply
        .as_object_mut()
        .context("call response must be an object")?
        .insert("decoded_result".into(), decoded);
    Ok(())
}

fn hash_reader(mut reader: impl Read) -> Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut length = 0;
    let mut buffer = [0; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        length += count as u64;
        hasher.update(&buffer[..count]);
    }
    Ok((length, format!("{:x}", hasher.finalize())))
}

fn manifest(index: u32) -> String {
    let namespace = format!("patina:fleet-n{index}");
    format!(
        r#"[child]
name = "worker"
version = "0.1.0"
kind = "child"
[child.ingress]
mode = "wit-only"
[child.artifact]
wasm = "worker.wasm"
[child.contract]
default = "{namespace}/work@0.1.0.step"
allow = ["{namespace}/work@0.1.0.step"]
[needs]
toys = []
"#
    )
}

fn generate_fixture(index: u32) -> Result<Fixture> {
    validate_index(index)?;
    let namespace = format!("patina:fleet-n{index}");
    let component = wat::parse_str(format!(
        r#"(component
          (core module $m (func (export "step") (param i32) (result i32)
            (i32.add (local.get 0) (i32.const {index}))))
          (core instance $i (instantiate $m))
          (func $step (param "value" s32) (result s32) (canon lift (core func $i "step")))
          (instance $work (export "step" (func $step)))
          (export "{namespace}/work@0.1.0" (instance $work)))"#
    ))?;
    let manifest = manifest(index);
    Ok(Fixture {
        component_base64: BASE64.encode(&component),
        component_blake3: blake3::hash(&component).to_hex().to_string(),
        manifest_sha256: sha256(manifest.as_bytes()),
        manifest,
        index,
    })
}

impl Fixture {
    fn validate(&self) -> Result<Vec<u8>> {
        validate_index(self.index)?;
        ensure!(
            self.manifest == manifest(self.index),
            "fixture manifest differs from the fleet arithmetic contract"
        );
        ensure!(
            self.component_base64.len() <= MAX_LINE / 2,
            "fixture component exceeds size limit"
        );
        let component = BASE64
            .decode(&self.component_base64)
            .context("invalid fixture base64")?;
        ensure!(
            component.starts_with(b"\0asm\x0d\0\x01\0"),
            "fixture must be a WebAssembly component"
        );
        ensure!(
            self.component_blake3 == blake3::hash(&component).to_hex().as_str(),
            "fixture component digest mismatch"
        );
        ensure!(
            self.manifest_sha256 == sha256(self.manifest.as_bytes()),
            "fixture manifest digest mismatch"
        );
        Ok(component)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PeerCorrelation {
    caller_call_id: String,
    peer_binding_id: String,
    caller_node_id: String,
    caller_vision_id: String,
}

fn peer_correlation(kind: ObservationKind, detail: &str) -> Result<Option<PeerCorrelation>> {
    if kind != ObservationKind::CallConstructed {
        return Ok(None);
    }
    let Some(payload) = detail.strip_prefix("mct-peer-call-correlation-v1:") else {
        return Ok(None);
    };
    let correlation: PeerCorrelation =
        serde_json::from_str(payload).context("invalid peer call correlation")?;
    for value in [
        &correlation.caller_call_id,
        &correlation.peer_binding_id,
        &correlation.caller_node_id,
        &correlation.caller_vision_id,
    ] {
        ensure!(
            !value.trim().is_empty(),
            "empty peer call correlation field"
        );
    }
    Ok(Some(correlation))
}

fn provenance(detail: &str, node: &str) -> Result<BTreeMap<String, String>> {
    let mut markers = BTreeMap::new();
    for part in detail.split(';') {
        for key in ["executed_on", "forwarded_from", "forwarded_to", "operation"] {
            let prefix = format!("{key}:");
            if let Some(value) = part.strip_prefix(&prefix) {
                ensure!(!value.is_empty(), "empty provenance marker");
                ensure!(
                    markers.insert(key.to_owned(), value.to_owned()).is_none(),
                    "duplicate provenance marker"
                );
                if key == "executed_on" {
                    ensure!(value == node, "ledger claims execution on another Mother");
                }
            } else if key == "executed_on" && part.contains(&prefix) {
                bail!("malformed executor provenance marker");
            }
        }
    }
    Ok(markers)
}

fn retain_observation(
    kind: ObservationKind,
    has_markers: bool,
    has_correlation: bool,
    has_mesh_job_detail: bool,
) -> bool {
    has_markers
        || has_correlation
        || has_mesh_job_detail
        || matches!(
            kind,
            ObservationKind::RuntimeExecutionStarted
                | ObservationKind::RuntimeExecutionCompleted
                | ObservationKind::RuntimeExecutionFailed
                | ObservationKind::RuntimeExecutionTimedOut
                | ObservationKind::ToyGrantAllowed
                | ObservationKind::ToyGrantDenied
                | ObservationKind::ToyGrantExpired
                | ObservationKind::ToyGrantRevoked
                | ObservationKind::PeerAdmitted
                | ObservationKind::PeerRejected
                | ObservationKind::PeerHelloReceived
                | ObservationKind::PeerHelloResponded
                | ObservationKind::PeerBindingRecorded
                | ObservationKind::PeerBindingRevoked
                | ObservationKind::PeerCallSent
                | ObservationKind::PeerCallReplied
        )
}

fn reduced_detail(
    kind: ObservationKind,
    raw_detail: &str,
    markers: &BTreeMap<String, String>,
) -> String {
    if raw_detail.starts_with("local_mesh_job_") {
        return raw_detail.to_owned();
    }
    raw_detail
        .split(';')
        .filter(|part| {
            part.split_once(':').is_some_and(|(key, _)| {
                markers.contains_key(key)
                    || (key == "hello_reason"
                        && matches!(
                            kind,
                            ObservationKind::PeerAdmitted | ObservationKind::PeerRejected
                        ))
                    || (matches!(key, "remote_reply" | "remote_reason")
                        && kind == ObservationKind::PeerCallReplied)
                    || key == "toy_grant_reason"
            })
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn acquire_host_lock(home: &Path) -> Result<fs::File> {
    ensure!(home.is_absolute(), "HOME must be absolute");
    private_fs::trusted_directory(home)?;
    let directory = home.join(".mct-federation");
    private_fs::ensure_directory(&directory)?;
    let (file, _) = private_fs::open_or_create(&directory.join("mother.lock"))?;
    file.try_lock()
        .context("another federation helper owns this host")?;
    Ok(file)
}

struct ChildStop {
    escalated: bool,
    recovery_required: bool,
    exit_signal: Option<i32>,
}

fn stop_owned_child(child: &mut Child, crash: bool, timeout: Duration) -> Result<ChildStop> {
    let mut escalated = false;
    let running = child.try_wait()?.is_none();
    ensure!(!crash || running, "crash requires a live owned child");
    if running {
        if crash {
            child.kill()?;
        } else {
            // The unreaped Child retains PID ownership until wait observes exit.
            if let Err(error) = kill_process(Pid::from_child(child), Signal::TERM)
                && child.try_wait()?.is_none()
            {
                return Err(error.into());
            }
            let deadline = Instant::now() + timeout;
            while child.try_wait()?.is_none() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(50));
            }
            if child.try_wait()?.is_none() {
                escalated = true;
                child.kill()?;
            }
        }
    }
    let status = child.wait()?;
    let exit_signal = status.signal();
    ensure!(
        !crash || exit_signal == Some(9),
        "crash did not terminate with SIGKILL"
    );
    Ok(ChildStop {
        escalated,
        recovery_required: crash || escalated || !status.success(),
        exit_signal,
    })
}

fn mother_pids() -> Result<Vec<u32>> {
    let output = Command::new("pgrep").args(["-x", "mct-daemon"]).output()?;
    match output.status.code() {
        Some(1) => Ok(Vec::new()),
        Some(0) => String::from_utf8(output.stdout)?
            .split_whitespace()
            .map(|pid| pid.parse::<u32>().context("invalid pgrep PID"))
            .collect(),
        _ => bail!("failed to check existing Mothers"),
    }
}

fn auxiliary_path(root: &Path, name: &str) -> PathBuf {
    let mut path = root.as_os_str().to_os_string();
    path.push(".");
    path.push(name);
    PathBuf::from(path)
}

impl Host {
    fn init(
        root: PathBuf,
        node: String,
        vision: String,
        bind: SocketAddr,
        binary: PathBuf,
        fixture: Fixture,
    ) -> Result<Self> {
        validate_absolute(&root)?;
        validate_absolute(&binary)?;
        validate_name(&node)?;
        validate_name(&vision)?;
        validate_bind(bind)?;
        ensure!(
            root.join("control.sock").as_os_str().len() < 104,
            "root is too long for a Unix socket"
        );
        let node = MctNodeId::new(node)?;
        let vision = VisionId::new(vision)?;
        private_fs::executable(&binary).context("validate daemon executable")?;
        let component = fixture.validate()?;
        let home =
            PathBuf::from(std::env::var_os("HOME").context("HOME is required for host ownership")?);
        let ownership_lock = acquire_host_lock(&home)?;
        ensure!(
            mother_pids()?.is_empty(),
            "existing mct-daemon while holding host lock"
        );
        match fs::symlink_metadata(&root) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
            Ok(_) => bail!("init requires a nonexistent root, including no symlink"),
        }
        let parent = root.parent().context("root parent missing")?;
        match private_fs::trusted_directory(parent) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => private_fs::ensure_directory(parent)?,
            result => result?,
        }
        fs::DirBuilder::new().mode(0o700).create(&root)?;
        private_fs::ensure_directory(&root)?;
        let source = auxiliary_path(&root, "source");
        private_fs::ensure_directory(&source)?;
        for (name, bytes) in [
            ("worker.wasm", component.as_slice()),
            ("child.toml", fixture.manifest.as_bytes()),
        ] {
            let mut file = private_fs::create_new(&source.join(name))?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        Ok(Self {
            root,
            node,
            vision,
            bind,
            binary,
            fixture,
            process: None,
            boot: 0,
            endpoint_id: None,
            checkpoint: None,
            recovery_required: false,
            repo_report: None,
            _ownership_lock: ownership_lock,
        })
    }

    fn identity(&self) -> Result<MctLocalNodeIdentity> {
        let identity = MctDaemonConfigStore::new(self.root.join("config.json"))
            .load()?
            .local_identity
            .context("daemon has not persisted a local identity")?;
        ensure!(
            identity.node_id == self.node && identity.vision_id == self.vision,
            "persisted node or vision changed"
        );
        ensure!(
            identity.identity_path == self.root.join("identity/iroh-secret.hex"),
            "persisted identity path escaped the private root"
        );
        if let Some(expected) = &self.endpoint_id {
            ensure!(
                identity.endpoint_id.as_str() == expected.as_str(),
                "persisted endpoint identity changed across boots"
            );
        }
        Ok(identity)
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
        timeout: Duration,
    ) -> Result<Value> {
        ensure!(
            matches!(method, "GET" | "POST"),
            "control method must be GET or POST"
        );
        ensure!(
            path.starts_with('/')
                && path.len() <= 256
                && path
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"/-_".contains(&b)),
            "unsafe control path"
        );
        let body = body
            .map(serde_json::to_vec)
            .transpose()?
            .unwrap_or_default();
        let (status, bytes) = control_socket_request(
            &self.root.join("control.sock"),
            method,
            path,
            &body,
            timeout,
        )?
        .context("owned daemon control socket is unavailable")?;
        ensure!(
            status == 200,
            "control {method} {path} returned HTTP {status}"
        );
        serde_json::from_slice(&bytes).context("control response is not JSON")
    }

    fn require_running(&mut self) -> Result<()> {
        let child = self
            .process
            .as_mut()
            .context("owned daemon is not running")?;
        ensure!(
            child.try_wait()?.is_none(),
            "owned daemon has exited; stop it before restarting"
        );
        Ok(())
    }

    fn start(&mut self, binary: Option<PathBuf>) -> Result<Value> {
        ensure!(
            self.process.is_none(),
            "stop the owned child before starting again"
        );
        ensure!(
            mother_pids()?.is_empty(),
            "existing mct-daemon while holding host lock"
        );
        let binary = binary.unwrap_or_else(|| self.binary.clone());
        validate_absolute(&binary)?;
        let binary_sha256 = file_sha256(private_fs::executable(&binary)?)?;
        if self.endpoint_id.is_some() {
            self.identity()?;
        }
        self.boot += 1;
        let log_path = auxiliary_path(&self.root, &format!("boot-{}.log", self.boot));
        let log = private_fs::create_new(&log_path)?;
        let child = Command::new(&binary)
            .args([
                "serve",
                "--node-id",
                self.node.as_str(),
                "--vision",
                self.vision.as_str(),
                "--bind",
                &self.bind.to_string(),
            ])
            .arg("--identity")
            .arg(self.root.join("identity/iroh-secret.hex"))
            .arg("--config")
            .arg(self.root.join("config.json"))
            .arg("--children-dir")
            .arg(self.root.join("children"))
            .arg("--state")
            .arg(self.root.join("state.sqlite"))
            .arg("--ledger")
            .arg(self.root.join("observations.jsonl"))
            .arg("--uds")
            .arg(self.root.join("control.sock"))
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()?;
        let pid = child.id();
        self.process = Some(child);
        self.binary = binary;
        let result = self.wait_ready(pid, &binary_sha256);
        if result.is_err() {
            match self.stop(false) {
                Ok(stopped) => eprintln!("startup cleanup: {stopped}"),
                Err(error) => eprintln!("startup cleanup failed: {error:#}"),
            }
        }
        result.with_context(|| {
            format!(
                "boot {} failed; retained log {}",
                self.boot,
                log_path.display()
            )
        })
    }

    fn wait_ready(&mut self, pid: u32, binary_sha256: &str) -> Result<Value> {
        let deadline = Instant::now() + READY_TIMEOUT;
        let mut last = String::from("no ready status");
        while Instant::now() < deadline {
            self.require_running()?;
            let timeout = CONTROL_TIMEOUT.min(deadline.saturating_duration_since(Instant::now()));
            match self.request("GET", "/status", None, timeout) {
                Ok(status) if status["readiness"] == "ready" => {
                    ensure!(
                        status["resident"]["node_id"] == self.node.as_str()
                            && status["resident"]["vision_id"] == self.vision.as_str(),
                        "ready status names another node or vision"
                    );
                    let snapshot: MotherIrohEndpointSnapshot =
                        serde_json::from_value(status["iroh_endpoint"].clone())?;
                    let identity = self.identity()?;
                    let ticket = lan_ticket(&snapshot, &identity.endpoint_id, self.bind)?;
                    self.endpoint_id = Some(identity.endpoint_id.to_string());
                    let after = file_sha256(private_fs::executable(&self.binary)?)?;
                    ensure!(
                        after == binary_sha256,
                        "daemon executable changed during startup"
                    );
                    self.require_running()?;
                    ensure!(
                        mother_pids()? == vec![pid],
                        "host does not have exactly the owned Mother"
                    );
                    self.recovery_required = false;
                    return Ok(
                        json!({"pid": pid, "status": status, "ticket": ticket, "owned_mother_count": 1,
                        "binary": self.binary, "binary_sha256": binary_sha256, "boot": self.boot}),
                    );
                }
                Ok(status) => last = format!("daemon readiness: {}", status["readiness"]),
                Err(error) => last = format!("{error:#}"),
            }
            thread::sleep(Duration::from_millis(100));
        }
        bail!("ready deadline elapsed: {last}")
    }

    fn stop(&mut self, crash: bool) -> Result<Value> {
        ensure!(
            !crash || self.process.is_some(),
            "crash requires an owned child"
        );
        let mut escalated = false;
        let mut exit_signal = None;
        if let Some(child) = self.process.as_mut() {
            let stopped = stop_owned_child(child, crash, STOP_TIMEOUT)?;
            escalated = stopped.escalated;
            exit_signal = stopped.exit_signal;
            self.recovery_required |= stopped.recovery_required;
            self.process = None;
        }
        Ok(
            json!({"stopped": true, "mode": if crash { "crash" } else { "graceful" },
            "escalated": escalated, "exit_signal": exit_signal, "recovery_required": self.recovery_required}),
        )
    }

    fn provision(&mut self) -> Result<Value> {
        self.require_running()?;
        let stage = self.request("POST", "/artifacts/stage", Some(&json!({
            "source_root": auxiliary_path(&self.root, "source"), "manifest_path": "child.toml", "component_path": "worker.wasm",
            "claimed_child_name": "worker", "claimed_artifact_version": "0.1.0",
            "expected_digest": format!("blake3:{}", self.fixture.component_blake3),
            "expected_manifest_digest": format!("sha256:{}", self.fixture.manifest_sha256),
            "standing_source_authority_id": null, "claimed_publisher": null, "require_source_sidecars": false,
            "children_dir": self.root.join("children"), "state_path": self.root.join("state.sqlite")
        })), CONTROL_TIMEOUT)?;
        ensure!(
            stage["verification_outcome"] == "verified",
            "artifact staging was not verified"
        );
        ensure!(
            stage["artifact_id"].is_string(),
            "staging response has no artifact ID"
        );
        let approval = self.request("POST", "/children/approve", Some(&json!({
            "expected_config_path": self.root.join("config.json"), "expected_children_dir": self.root.join("children"),
            "expected_state_path": self.root.join("state.sqlite"), "expected_artifact_id": stage["artifact_id"],
            "child_name": "worker", "strict_integrity": true
        })), CONTROL_TIMEOUT)?;
        ensure!(
            approval["approval_state"] == "approved",
            "child approval was not approved"
        );
        Ok(
            json!({"index": self.fixture.index, "artifact_id": stage["artifact_id"],
            "verification_outcome": stage["verification_outcome"], "approval_state": approval["approval_state"],
            "stage": stage, "approval": approval}),
        )
    }

    fn provision_repo_report(&mut self, package: &Path, snapshot: RepoSnapshot) -> Result<Value> {
        self.require_running()?;
        ensure!(
            self.repo_report.is_none(),
            "repo-report fixture already provisioned"
        );
        validate_absolute(package)?;
        let component =
            read_bounded_regular(&package.join("share/mct-repo-report.wasm"), 2 * 1024 * 1024)?;
        let manifest = read_bounded_regular(&package.join("share/child.toml"), 256 * 1024)?;
        let input = read_bounded_regular(
            &package
                .join("share/snapshots")
                .join(snapshot.as_str())
                .join("Cargo.lock"),
            2 * 1024 * 1024,
        )?;
        let source = auxiliary_path(&self.root, "repo-source");
        let data_root = auxiliary_path(&self.root, "repo-data");
        private_fs::ensure_directory(&source)?;
        private_fs::ensure_directory(&data_root)?;
        for (path, bytes) in [
            (source.join("mct-repo-report.wasm"), component.as_slice()),
            (source.join("child.toml"), manifest.as_slice()),
            (data_root.join("Cargo.lock"), input.as_slice()),
        ] {
            let mut file = private_fs::create_new(&path)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        let stage = self.request("POST", "/artifacts/stage", Some(&json!({
            "source_root": source, "manifest_path": "child.toml", "component_path": "mct-repo-report.wasm",
            "claimed_child_name": "mct-repo-report", "claimed_artifact_version": "0.2.0",
            "expected_digest": format!("blake3:{}", blake3::hash(&component).to_hex()),
            "expected_manifest_digest": format!("sha256:{}", sha256(&manifest)),
            "standing_source_authority_id": null, "claimed_publisher": null, "require_source_sidecars": false,
            "children_dir": self.root.join("children"), "state_path": self.root.join("state.sqlite")
        })), CONTROL_TIMEOUT)?;
        ensure!(
            stage["verification_outcome"] == "verified",
            "repo-report artifact staging was not verified"
        );
        let artifact_id = stage["artifact_id"]
            .as_str()
            .context("repo-report staging response has no artifact ID")?
            .to_owned();
        let approval = self.request("POST", "/children/approve", Some(&json!({
            "expected_config_path": self.root.join("config.json"), "expected_children_dir": self.root.join("children"),
            "expected_state_path": self.root.join("state.sqlite"), "expected_artifact_id": artifact_id,
            "child_name": "mct-repo-report", "strict_integrity": true
        })), CONTROL_TIMEOUT)?;
        ensure!(
            approval["approval_state"] == "approved",
            "repo-report Child approval was not approved"
        );
        let input_sha256 = sha256(&input);
        self.repo_report = Some(RepoReportFixture {
            data_root: data_root.clone(),
            input_sha256: input_sha256.clone(),
            artifact_id: artifact_id.clone(),
        });
        Ok(json!({
            "artifact_id": artifact_id,
            "snapshot": snapshot,
            "input_sha256": input_sha256,
            "data_root": data_root,
            "verification_outcome": stage["verification_outcome"],
            "approval_state": approval["approval_state"]
        }))
    }

    fn grant_repo_report(
        &mut self,
        caller_node: Option<&str>,
        mutation_id: &str,
        expires_at: &str,
    ) -> Result<Value> {
        self.require_running()?;
        if let Some(caller_node) = caller_node {
            validate_name(caller_node)?;
        }
        validate_name(mutation_id)?;
        let fixture = self
            .repo_report
            .as_ref()
            .context("repo-report fixture is not provisioned")?;
        let expires = expires_at.parse::<jiff::Timestamp>()?;
        ensure!(
            expires > jiff::Timestamp::now(),
            "repo-report grant expiry is not in the future"
        );
        let response = self.request(
            "POST",
            "/watch/supporting-grant",
            Some(&json!({
                "caller_node_id": caller_node,
                "mutation_id": mutation_id,
                "expected_config_path": self.root.join("config.json"),
                "expected_children_dir": self.root.join("children"),
                "expected_state_path": self.root.join("state.sqlite"),
                "child_name": "mct-repo-report",
                "expires_at": expires_at,
                "grant": {"kind": "directory_read", "canonical_root": fixture.data_root}
            })),
            CONTROL_TIMEOUT,
        )?;
        let grants = response["grants"]
            .as_array()
            .context("repo-report grant response has no grants")?;
        ensure!(
            grants.len() == 1 && grants[0]["grant_state"] == "active",
            "repo-report grant was not active"
        );
        Ok(json!({
            "expires_at": expires_at,
            "input_sha256": fixture.input_sha256,
            "artifact_id": fixture.artifact_id,
            "grant": grants[0],
            "mutation_result": response["mutation_result"]
        }))
    }

    fn submit_call(&mut self, id: &str, payload_json: &str, target: &Target) -> Result<Value> {
        self.require_running()?;
        validate_name(id)?;
        validate_target(target)?;
        let payload = validated_payload(payload_json)?;
        let identity = self.identity()?;
        let authority = self.request("GET", "/receiver-authority", None, CONTROL_TIMEOUT)?;
        let trace_id = format!("trace-{id}");
        let deadline = jiff::Timestamp::now()
            .checked_add(jiff::SignedDuration::from_secs(90))?
            .to_string();
        let mut reply = self.request("POST", "/calls", Some(&json!({
            "protocol_request_id": format!("proto-{id}"), "call_id": id,
            "target": target,
            "payload_metadata": {"data_classification": "public", "size_bytes": payload.len(), "contains_secret_scoped_material": false},
            "authority_context": {"policy_revision": identity.policy_revision, "expected_receiver_grants_authority": authority, "vision_policy_revision": 1},
            "deadline": deadline,
            "trace_context": {"trace_id": trace_id, "span_id": format!("span-{id}")},
            "payload": {"payload_kind": "inline_payload", "inline_payload_ref": format!("payload-{id}"), "content_type": "application/json", "size_bytes": payload.len(), "blake3_digest_hex": blake3::hash(payload).to_hex().to_string()},
            "inline_payload_base64": BASE64.encode(payload), "idempotency_key": format!("key-{id}")
        })), CALL_EXCHANGE_TIMEOUT)?;
        decode_result_payload(&mut reply)?;
        let object = reply
            .as_object_mut()
            .context("call response must be an object")?;
        object.insert("submitted_call_id".into(), json!(id));
        object.insert("submitted_trace_id".into(), json!(trace_id));
        Ok(reply)
    }

    fn call(&mut self, destination: u32, label: &str) -> Result<Value> {
        validate_index(destination)?;
        validate_name(label)?;
        let mut random = [0u8; 16];
        getrandom::fill(&mut random)
            .map_err(|e| anyhow::anyhow!("call ID randomness unavailable: {e}"))?;
        let nonce: String = random.iter().map(|b| format!("{b:02x}")).collect();
        let id = format!("{}-to-{destination}-{label}-{nonce}", self.node);
        self.submit_call(
            &id,
            "[41]",
            &Target {
                namespace: format!("patina:fleet-n{destination}"),
                interface_name: "work@0.1.0".into(),
                function_name: "step".into(),
            },
        )
    }

    fn submit_mesh_job(&mut self, job_id: &str, pieces: &[MeshJobPiece]) -> Result<Value> {
        self.require_running()?;
        validate_name(job_id)?;
        ensure!(
            (2..=16).contains(&pieces.len()),
            "mesh job piece count is outside the supported bound"
        );
        let identity = self.identity()?;
        let authority = self.request("GET", "/receiver-authority", None, CONTROL_TIMEOUT)?;
        let deadline = jiff::Timestamp::now()
            .checked_add(jiff::SignedDuration::from_secs(90))?
            .to_string();
        let mut seen = std::collections::BTreeSet::new();
        let piece_values = pieces
            .iter()
            .map(|piece| {
                validate_name(&piece.piece_id)?;
                ensure!(seen.insert(piece.piece_id.clone()), "duplicate mesh job piece id");
                if let Some(required_node_id) = &piece.required_node_id {
                    validate_name(required_node_id)?;
                }
                validate_target(&piece.target)?;
                let payload = validated_payload(&piece.payload_json)?;
                Ok(json!({
                    "piece_id": piece.piece_id,
                    "required_node_id": piece.required_node_id,
                    "target": piece.target,
                    "payload_metadata": {"data_classification": "public", "size_bytes": payload.len(), "contains_secret_scoped_material": false},
                    "payload": {"payload_kind": "inline_payload", "inline_payload_ref": format!("payload-{job_id}-{}", piece.piece_id), "content_type": "application/json", "size_bytes": payload.len(), "blake3_digest_hex": blake3::hash(payload).to_hex().to_string()},
                    "inline_payload_base64": BASE64.encode(payload)
                }))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut reply = self.request("POST", "/calls", Some(&json!({
            "job_id": job_id,
            "authority_context": {"policy_revision": identity.policy_revision, "expected_receiver_grants_authority": authority, "vision_policy_revision": 1},
            "deadline": deadline,
            "trace_context": {"trace_id": format!("trace-{job_id}"), "span_id": format!("span-{job_id}")},
            "pieces": piece_values
        })), CALL_EXCHANGE_TIMEOUT)?;
        ensure!(reply["job_id"] == job_id, "mesh job response ID mismatch");
        let returned = reply["pieces"]
            .as_array_mut()
            .context("mesh job response has no pieces")?;
        ensure!(
            returned.len() == pieces.len(),
            "mesh job response piece count mismatch"
        );
        for piece in returned {
            decode_result_payload(piece)?;
        }
        Ok(reply)
    }

    fn verify(&mut self) -> Result<Value> {
        ensure!(
            !self.recovery_required,
            "restart for daemon recovery before verifying a crash-stopped ledger"
        );
        ensure!(
            self.process.is_none(),
            "stop the daemon before verifying its ledger"
        );
        let identity = self.identity()?;
        let path = self.root.join("observations.jsonl");
        if let Some(checkpoint) = &self.checkpoint {
            checkpoint.check_prefix(private_fs::read_file(&path)?)?;
        }
        let verified = verify_ledger_streaming(&path, "ledger-local", self.node.as_str())?;
        let head = verified.head.as_ref().map(|head| json!({
            "local_sequence": head.local_sequence, "entry_hash": head.entry_hash,
            "observation_id": head.observation_id, "ledger_id": head.ledger_id, "mother_node_id": head.mother_node_id
        }));
        let mut reader = BufReader::new(private_fs::read_file(&path)?);
        let mut observations = Vec::new();
        let mut sha = Sha256::new();
        let mut blake = blake3::Hasher::new();
        let mut length = 0u64;
        let mut count = 0u64;
        while let Some(line) = read_line_bounded(&mut reader)? {
            sha.update(&line);
            blake.update(&line);
            length += line.len() as u64;
            let entry: MctObservationLedgerEntry =
                serde_json::from_slice(&line).context("parse verified ledger observation")?;
            count += 1;
            let observation = &entry.observation;
            let markers = provenance(
                observation.detail_ref.as_deref().unwrap_or(""),
                self.node.as_str(),
            )?;
            let correlation = peer_correlation(
                observation.kind,
                observation.detail_ref.as_deref().unwrap_or(""),
            )?;
            if let Some(correlation) = &correlation {
                let execution_id = observation
                    .call_id
                    .as_ref()
                    .context("peer correlation missing receiver call ID")?;
                ensure!(
                    execution_id.as_str() != correlation.caller_call_id,
                    "peer correlation reuses caller call ID"
                );
            }
            let raw_detail = observation.detail_ref.as_deref().unwrap_or("");
            let relevant = retain_observation(
                observation.kind,
                !markers.is_empty(),
                correlation.is_some(),
                raw_detail.contains("local_mesh_job_"),
            );
            if relevant {
                let detail = reduced_detail(observation.kind, raw_detail, &markers);
                let mut reduced = json!({
                    "local_sequence": entry.local_sequence, "observation_id": observation.observation_id,
                    "kind": observation.kind, "source_plane": observation.source_plane, "outcome": observation.outcome,
                    "observed_at": observation.observed_at, "trace_id": observation.trace.trace_id,
                    "call_id": observation.call_id, "decision_id": observation.decision_id,
                    "subject_id": observation.subject_id, "resource_id": observation.resource_id,
                    "policy_revision": observation.policy_revision, "grants_revision": observation.grants_revision,
                    "provenance": markers, "detail_ref": if detail.is_empty() { None } else { Some(detail) },
                });
                if let Some(correlation) = correlation {
                    reduced["peer_correlation"] = serde_json::to_value(correlation)?;
                }
                observations.push(reduced);
            }
        }
        ensure!(
            length == verified.prefix_len
                && blake.finalize().to_hex().as_str() == verified.prefix_blake3,
            "ledger changed while reducing verified observations"
        );
        let digest = format!("{:x}", sha.finalize());
        let result = json!({
            "verified": true,
            "verification": {"verified": true, "head": head, "entry_count": count,
                "canonical_fact_count": verified.replay.canonical_fact_count, "prefix_blake3": verified.prefix_blake3},
            "endpoint_id": identity.endpoint_id, "node_id": self.node,
            "ledger_bytes": length, "ledger_sha256": digest,
            "prefix_preserved": true, "previous_checkpoint_bytes": self.checkpoint.as_ref().map(|c| c.length),
            "observations": observations
        });
        self.checkpoint = Some(Checkpoint {
            length,
            sha256: digest,
        });
        Ok(result)
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if self.process.is_some() {
            match self.stop(false) {
                Ok(stopped) => eprintln!("owned child cleanup: {stopped}"),
                Err(error) => eprintln!("owned child cleanup failed: {error:#}"),
            }
        }
    }
}

fn dispatch(host: &mut Option<Host>, request: Request) -> Result<Value> {
    match request {
        Request::Fixture { index } => {
            serde_json::to_value(generate_fixture(index)?).map_err(Into::into)
        }
        Request::Init {
            root,
            node,
            vision,
            bind,
            binary,
            fixture,
        } => {
            ensure!(host.is_none(), "helper already initialized");
            let initialized = Host::init(root, node, vision, bind, binary, fixture)?;
            let value = json!({"initialized": true, "root": initialized.root, "node": initialized.node,
                "vision": initialized.vision, "index": initialized.fixture.index});
            *host = Some(initialized);
            Ok(value)
        }
        Request::Stop { crash } => match host {
            Some(host) => host.stop(crash),
            None => Ok(
                json!({"stopped": true, "mode": if crash { "crash" } else { "graceful" },
                "escalated": false, "recovery_required": false}),
            ),
        },
        request => {
            let host = host.as_mut().context("init is required first")?;
            match request {
                Request::Start { binary } => host.start(binary),
                Request::Control { method, path, body } => {
                    host.require_running()?;
                    host.request(&method, &path, body.as_ref(), CONTROL_TIMEOUT)
                }
                Request::Provision {} => host.provision(),
                Request::Peers {} => {
                    let config = MctDaemonConfigStore::new(host.root.join("config.json")).load()?;
                    serde_json::to_value(config.peers).map_err(Into::into)
                }
                Request::Call { destination, label } => host.call(destination, &label),
                Request::ProvisionRepoReport { package, snapshot } => {
                    host.provision_repo_report(&package, snapshot)
                }
                Request::GrantRepoReport {
                    caller_node,
                    mutation_id,
                    expires_at,
                } => host.grant_repo_report(caller_node.as_deref(), &mutation_id, &expires_at),
                Request::MeshJob { job_id, pieces } => host.submit_mesh_job(&job_id, &pieces),
                Request::CallTarget {
                    call_id,
                    payload_json,
                    target,
                } => host.submit_call(&call_id, &payload_json, &target),
                Request::Verify {} => host.verify(),
                _ => unreachable!("handled before requiring initialization"),
            }
        }
    }
}

fn read_line_bounded(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    reader
        .take((MAX_LINE + 1) as u64)
        .read_until(b'\n', &mut line)?;
    ensure!(line.len() <= MAX_LINE, "JSON line exceeds 1 MiB limit");
    if line.is_empty() {
        return Ok(None);
    }
    ensure!(line.ends_with(b"\n"), "unterminated JSON line");
    Ok(Some(line))
}

fn main() -> Result<()> {
    let mut host = None;
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = stdout.lock();
    loop {
        let (result, terminal) = match read_line_bounded(&mut input) {
            Ok(None) => break,
            Ok(Some(line)) => (
                serde_json::from_slice(&line)
                    .context("invalid helper request")
                    .and_then(|request| dispatch(&mut host, request)),
                false,
            ),
            Err(error) => (Err(error), true),
        };
        let reply = match result {
            Ok(value) => json!({"ok": true, "value": value}),
            Err(error) => json!({"ok": false, "error": format!("{error:#}")}),
        };
        serde_json::to_writer(&mut output, &reply)?;
        output.write_all(b"\n")?;
        output.flush()?;
        if terminal {
            break;
        }
    }
    if let Some(host) = host.as_mut() {
        let stopped = host.stop(false)?;
        eprintln!("EOF cleanup: {stopped}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_paths_reject_injection_and_traversal() {
        for name in ["", "../mother", "mother;id", "mother\nother", "-option"] {
            assert!(validate_name(name).is_err(), "{name:?}");
        }
        assert!(validate_name("mother-a_2").is_ok());
        for path in [
            "relative",
            "/",
            "/tmp/../root",
            "/tmp/./root",
            "/tmp//root",
            "/tmp/root\n",
            "/tmp/root/",
        ] {
            assert!(validate_absolute(Path::new(path)).is_err(), "{path:?}");
        }
        assert!(validate_absolute(Path::new("/tmp/federation-root")).is_ok());
    }

    #[test]
    fn call_exchange_outlives_hello_and_call_transport_bounds() {
        let outbound_hello_bound = Duration::from_secs(30);
        let outbound_call_bound = Duration::from_secs(30);
        assert!(CALL_EXCHANGE_TIMEOUT > outbound_hello_bound + outbound_call_bound);
        assert!(CALL_EXCHANGE_TIMEOUT < Duration::from_secs(150));
    }

    #[test]
    fn optimized_file_digest_matches_canonical_sha256() {
        use std::io::Seek as _;
        let bytes = b"fixture executable bytes";
        let mut file = tempfile::tempfile().unwrap();
        file.write_all(bytes).unwrap();
        file.rewind().unwrap();
        assert_eq!(
            file_sha256(file).unwrap(),
            format!("{:x}", Sha256::digest(bytes))
        );
    }

    #[test]
    fn auxiliary_files_leave_the_authority_root_virgin() {
        let root = Path::new("/home/operator/run/0");
        for name in ["source", "boot-1.log"] {
            let auxiliary = auxiliary_path(root, name);
            assert_eq!(auxiliary.parent(), root.parent());
            assert!(!auxiliary.starts_with(root));
            assert_eq!(
                auxiliary,
                PathBuf::from(format!("{}.{}", root.display(), name))
            );
        }
    }

    #[test]
    fn bind_requires_private_ipv4_and_nonzero_port() {
        use std::net::{Ipv4Addr, Ipv6Addr};
        for ip in [
            Ipv4Addr::LOCALHOST,
            Ipv4Addr::UNSPECIFIED,
            Ipv4Addr::BROADCAST,
            Ipv4Addr::new(169, 254, 1, 1),
            Ipv4Addr::new(203, 0, 113, 1),
        ] {
            assert!(validate_bind(SocketAddr::from((ip, 1234))).is_err());
        }
        let lan = Ipv4Addr::new(10, 0, 0, 1);
        assert!(validate_bind(SocketAddr::from((lan, 0))).is_err());
        assert!(validate_bind(SocketAddr::from((Ipv6Addr::LOCALHOST, 1234))).is_err());
        assert!(validate_bind(SocketAddr::from((lan, 1234))).is_ok());
    }

    #[test]
    fn ticket_preserves_identity_but_only_exposes_the_pinned_lan_socket() {
        use mct_kernel::EndpointIdText;
        use std::net::{Ipv4Addr, Ipv6Addr};
        let bind = SocketAddr::from((Ipv4Addr::new(10, 0, 0, 1), 1234));
        let endpoint = EndpointIdText::new("endpoint-a").unwrap();
        let mut snapshot = MotherIrohEndpointSnapshot {
            endpoint_id: endpoint.clone(),
            lifecycle: MotherIrohEndpointLifecycle::Bound,
            accepted_alpns: Vec::new(),
            direct_addresses: vec![
                SocketAddr::from((Ipv4Addr::LOCALHOST, 1234)).to_string(),
                SocketAddr::from((Ipv6Addr::LOCALHOST, 1234)).to_string(),
                bind.to_string(),
            ],
            relay_urls: Vec::new(),
            relay_mode: MotherIrohRelayMode::Disabled,
        };
        let original = snapshot.clone();
        let ticket = lan_ticket(&snapshot, &endpoint, bind).unwrap();
        assert_eq!(ticket.direct_addresses, vec![bind.to_string()]);
        assert_eq!(ticket.endpoint_id, endpoint);
        assert_eq!(snapshot, original);
        assert!(lan_ticket(&snapshot, &EndpointIdText::new("other").unwrap(), bind).is_err());
        snapshot.relay_mode = MotherIrohRelayMode::Default;
        assert!(lan_ticket(&snapshot, &endpoint, bind).is_err());
        snapshot = original.clone();
        snapshot.relay_urls.push("https://relay.invalid".into());
        assert!(lan_ticket(&snapshot, &endpoint, bind).is_err());
        snapshot = original;
        snapshot.direct_addresses.pop();
        assert!(lan_ticket(&snapshot, &endpoint, bind).is_err());
    }

    #[test]
    fn bounded_lines_reject_truncation_and_oversized_input() {
        let mut input = io::Cursor::new(b"{\"op\":\"stop\"}\n");
        assert!(read_line_bounded(&mut input).unwrap().is_some());
        assert!(read_line_bounded(&mut input).unwrap().is_none());
        assert!(read_line_bounded(&mut io::Cursor::new(b"{}")).is_err());
        assert!(read_line_bounded(&mut io::Cursor::new(vec![b'x'; MAX_LINE + 1])).is_err());
    }

    #[test]
    fn provenance_rejects_every_foreign_executor_marker() {
        let detail = "executed_on:mother-a;forwarded_from:mother-b;operation:patina:fleet-n0/work@0.1.0.step";
        let markers = provenance(detail, "mother-a").unwrap();
        assert_eq!(markers["executed_on"], "mother-a");
        assert_eq!(markers["forwarded_from"], "mother-b");
        for detail in [
            "executed_on:mother-b;forwarded_from:mother-a",
            "executed_on:mother-a;executed_on:mother-b",
            "executed_on:mother-a;executed_on:mother-a",
            "executed_on:",
            "prefix executed_on:mother-b",
        ] {
            assert!(provenance(detail, "mother-a").is_err(), "{detail}");
        }
        assert!(
            provenance("hello_reason:Admitted", "mother-a")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn peer_correlation_requires_a_real_constructed_record_and_typed_fields() {
        let fields = json!({
            "caller_call_id": "caller-call",
            "peer_binding_id": "receiver-admits-caller",
            "caller_node_id": "caller",
            "caller_vision_id": "vision"
        });
        let detail = format!("mct-peer-call-correlation-v1:{fields}");
        let correlation = peer_correlation(ObservationKind::CallConstructed, &detail)
            .unwrap()
            .unwrap();
        assert_eq!(serde_json::to_value(correlation).unwrap(), fields);
        assert!(
            peer_correlation(ObservationKind::PeerCallReceived, &detail)
                .unwrap()
                .is_none()
        );
        assert!(
            peer_correlation(ObservationKind::CallConstructed, "other")
                .unwrap()
                .is_none()
        );
        assert!(
            peer_correlation(
                ObservationKind::CallConstructed,
                "mct-peer-call-correlation-v1:{"
            )
            .is_err()
        );
        for key in [
            "caller_call_id",
            "peer_binding_id",
            "caller_node_id",
            "caller_vision_id",
        ] {
            for invalid in [Value::Null, json!(1), json!(""), json!(" ")] {
                let mut changed = fields.clone();
                changed[key] = invalid;
                assert!(
                    peer_correlation(
                        ObservationKind::CallConstructed,
                        &format!("mct-peer-call-correlation-v1:{changed}")
                    )
                    .is_err()
                );
            }
            let mut changed = fields.clone();
            changed.as_object_mut().unwrap().remove(key);
            assert!(
                peer_correlation(
                    ObservationKind::CallConstructed,
                    &format!("mct-peer-call-correlation-v1:{changed}")
                )
                .is_err()
            );
        }
        let duplicate = detail.replacen("{", "{\"caller_call_id\":\"duplicate\",", 1);
        assert!(peer_correlation(ObservationKind::CallConstructed, &duplicate).is_err());
    }

    #[test]
    fn checkpoint_requires_the_exact_previous_bytes() {
        let checkpoint = Checkpoint {
            length: 4,
            sha256: sha256(b"old\n"),
        };
        assert!(checkpoint.check_prefix(b"old\nnew\n".as_slice()).is_ok());
        assert!(checkpoint.check_prefix(b"new\nnew\n".as_slice()).is_err());
        assert!(checkpoint.check_prefix(b"old".as_slice()).is_err());
    }

    #[test]
    fn fixture_is_pinned_without_compiling_during_validation() {
        let fixture = generate_fixture(2).unwrap();
        assert!(fixture.validate().is_ok());
        let mut changed = fixture.clone();
        changed.manifest.push('\n');
        assert!(changed.validate().is_err());
        let mut changed = fixture.clone();
        changed.component_blake3 = "blake3:wrong".into();
        assert!(changed.validate().is_err());
        let mut changed = fixture;
        changed.index = 3;
        assert!(changed.validate().is_err());
        assert!(validate_index(u32::MAX).is_err());
    }

    #[test]
    fn crash_rejects_an_already_exited_child() {
        let mut child = Command::new("sh").args(["-c", "exit 0"]).spawn().unwrap();
        assert!(child.wait().unwrap().success());
        assert!(stop_owned_child(&mut child, true, Duration::from_secs(1)).is_err());
        assert!(stop_owned_child(&mut child, false, Duration::from_secs(1)).is_ok());
    }

    #[test]
    fn owned_child_stop_distinguishes_drain_crash_and_escalation() {
        for (crash, ignore_term, timeout, expected_escalation) in [
            (false, false, Duration::from_secs(2), false),
            (true, false, Duration::from_secs(2), false),
            (false, true, Duration::ZERO, true),
        ] {
            let trap = if ignore_term { "" } else { "exit 0" };
            let mut child = Command::new("sh")
                .args([
                    "-c",
                    &format!("trap '{trap}' TERM; echo ready; while :; do sleep 0.05; done"),
                ])
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            let mut ready = String::new();
            BufReader::new(child.stdout.take().unwrap())
                .read_line(&mut ready)
                .unwrap();
            assert_eq!(ready, "ready\n");
            let result = stop_owned_child(&mut child, crash, timeout).unwrap();
            assert_eq!(result.escalated, expected_escalation);
            assert_eq!(result.recovery_required, crash || expected_escalation);
            assert_eq!(
                result.exit_signal,
                if crash || expected_escalation {
                    Some(9)
                } else {
                    None
                }
            );
            assert!(child.try_wait().unwrap().is_some());
        }
    }

    #[test]
    fn reduction_retains_typed_execution_facts_without_provenance() {
        for kind in [
            ObservationKind::RuntimeExecutionStarted,
            ObservationKind::RuntimeExecutionCompleted,
            ObservationKind::RuntimeExecutionFailed,
            ObservationKind::RuntimeExecutionTimedOut,
        ] {
            assert!(retain_observation(kind, false, false, false));
        }
        assert!(!retain_observation(
            ObservationKind::CallConstructed,
            false,
            false,
            false
        ));
        assert!(retain_observation(
            ObservationKind::CallConstructed,
            false,
            true,
            false
        ));
        assert!(retain_observation(
            ObservationKind::CallConstructed,
            false,
            false,
            true
        ));
        assert!(retain_observation(
            ObservationKind::ToyGrantExpired,
            false,
            false,
            false
        ));
        assert_eq!(
            reduced_detail(
                ObservationKind::ResultRecorded,
                "local_mesh_job_result:completed=2;denied=0;failed=0",
                &BTreeMap::new(),
            ),
            "local_mesh_job_result:completed=2;denied=0;failed=0"
        );
    }

    #[test]
    fn ownership_lock_excludes_other_roots_and_rejects_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let private_tempdir = || {
            let dir = tempfile::Builder::new()
                .prefix("mct-federation-lock-")
                .tempdir_in(std::env::var_os("HOME").unwrap())
                .unwrap();
            std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
            dir
        };
        let home = private_tempdir();
        let lock = acquire_host_lock(home.path()).unwrap();
        assert!(acquire_host_lock(home.path()).is_err());
        drop(lock);
        assert!(acquire_host_lock(home.path()).is_ok());
        let other = private_tempdir();
        symlink(
            home.path().join(".mct-federation"),
            other.path().join(".mct-federation"),
        )
        .unwrap();
        assert!(acquire_host_lock(other.path()).is_err());
    }

    #[test]
    fn protocol_fails_closed_on_unknown_or_malformed_requests() {
        for request in [
            r#"{"op":"unknown"}"#,
            r#"{"op":"stop","pid":7}"#,
            r#"{"op":"stop","crash":"yes"}"#,
            r#"{"op":"stop","crash":null}"#,
            r#"{"op":"provision","pid":7}"#,
            r#"{"op":"peers","pid":7}"#,
            r#"{"op":"verify","pid":7}"#,
            r#"{"op":"fixture","index":-1}"#,
            r#"{"op":"call","destination":0}"#,
        ] {
            assert!(
                serde_json::from_str::<Request>(request).is_err(),
                "{request}"
            );
        }
        for request in [
            r#"{"op":"stop"}"#,
            r#"{"op":"stop","crash":true}"#,
            r#"{"op":"verify"}"#,
            r#"{"op":"peers"}"#,
            r#"{"op":"provision"}"#,
        ] {
            assert!(
                serde_json::from_str::<Request>(request).is_ok(),
                "{request}"
            );
        }
    }
}
