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
use mct_kernel::{CallId, MctNodeId, MctObservation, ObservationKind, OperationTarget, VisionId};
use mct_observation::{MctObservationLedgerEntry, private_fs, verify_ledger_streaming};
use rustix::process::{Pid, Signal, kill_process};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, BufRead, BufReader, Read, Write},
    net::{IpAddr, SocketAddr},
    os::unix::{
        fs::{FileTypeExt, MetadataExt},
        net::UnixStream,
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const MAX_LINE: usize = 4 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Open {
        root: String,
        data_root: String,
        binary: String,
        node: String,
        vision: String,
        bind: SocketAddr,
    },
    Start {},
    Control {
        method: String,
        path: String,
        body: Option<Value>,
    },
    Peers {},
    Put {
        relative_path: String,
        sha256: String,
        content_base64: String,
    },
    Call {
        job_id: String,
        payload_json: String,
        target: Target,
    },
    Stop {},
    LedgerCallSummary {
        call_id: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    namespace: String,
    interface_name: String,
    function_name: String,
}

fn validate_name(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 64,
        "invalid portable ID length"
    );
    ensure!(
        value.as_bytes()[0].is_ascii_alphanumeric()
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "invalid portable ID"
    );
    Ok(())
}

fn validate_absolute(value: &str) -> Result<()> {
    ensure!(
        value.starts_with('/') && value != "/" && value != "/tmp",
        "invalid absolute path"
    );
    ensure!(
        !Path::new(value).starts_with("/tmp") && !Path::new(value).starts_with("/private/tmp"),
        "temporary paths are not permitted"
    );
    ensure!(
        value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-+".contains(&b)),
        "unsafe path characters"
    );
    ensure!(
        value[1..]
            .split('/')
            .all(|part| !matches!(part, "" | "." | "..")),
        "unsafe path components"
    );
    Ok(())
}

fn validate_bind(bind: SocketAddr) -> Result<()> {
    ensure!(
        bind.port() != 0 && matches!(bind.ip(), IpAddr::V4(ip) if ip.is_private()),
        "bind must be RFC1918 IPv4 with nonzero port"
    );
    Ok(())
}

fn allowed_control(method: &str, path: &str) -> bool {
    matches!(
        (method, path),
        ("GET", "/status" | "/snapshot" | "/receiver-authority")
            | (
                "POST",
                "/peers/add"
                    | "/peers/proof"
                    | "/artifacts/stage"
                    | "/children/approve"
                    | "/watch/supporting-grant"
                    | "/calls"
            )
    )
}

#[derive(Debug)]
struct ChildStop {
    escalated: bool,
    exit_signal: Option<i32>,
    recovery_required: bool,
}

fn stop_owned_child(child: &mut Child, timeout: Duration) -> Result<ChildStop> {
    let mut escalated = false;
    if child.try_wait()?.is_none() {
        // An unreaped Child retains ownership of its PID until wait observes exit.
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
    let status = child.wait()?;
    Ok(ChildStop {
        escalated,
        exit_signal: status.signal(),
        recovery_required: escalated || !status.success(),
    })
}

struct Host {
    root: PathBuf,
    data_root: PathBuf,
    binary: PathBuf,
    node: MctNodeId,
    vision: VisionId,
    bind: SocketAddr,
    process: Option<Child>,
    boot: u64,
    endpoint_id: Option<String>,
    recovery_required: bool,
    ownership_lock: fs::File,
}

fn auxiliary_path(root: &Path, suffix: &str) -> PathBuf {
    let mut path = root.as_os_str().to_os_string();
    path.push(format!(".{suffix}"));
    path.into()
}

fn acquire_lock(root: &Path) -> Result<fs::File> {
    let (file, _) = private_fs::open_or_create(&auxiliary_path(root, "host.lock"))?;
    file.try_lock().context("another helper owns this root")?;
    Ok(file)
}

fn binary_hash(path: &Path) -> Result<String> {
    let mut file = private_fs::executable(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

impl Host {
    fn open(
        root: PathBuf,
        data_root: PathBuf,
        binary: PathBuf,
        node: String,
        vision: String,
        bind: SocketAddr,
    ) -> Result<Self> {
        let node = MctNodeId::new(node)?;
        let vision = VisionId::new(vision)?;
        private_fs::executable(&binary).context("validate daemon executable")?;
        ensure!(
            !root.starts_with(&data_root) && !data_root.starts_with(&root),
            "roots must be distinct and non-overlapping"
        );
        let parent = root.parent().context("root has no parent")?;
        private_fs::ensure_directory(parent).context("private root parent")?;
        let ownership_lock = acquire_lock(&root)?;
        private_fs::ensure_directory(&data_root).context("private data root")?;
        private_fs::ensure_directory(&root).context("private daemon root")?;
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            let name = entry.file_name();
            ensure!(
                [
                    "identity",
                    "config.json",
                    "children",
                    "state.sqlite",
                    "state.sqlite-wal",
                    "state.sqlite-shm",
                    "observations.jsonl",
                    "control.sock"
                ]
                .iter()
                .any(|allowed| name == *allowed),
                "unexpected daemon root entry: {:?}",
                name
            );
        }
        let host = Self {
            root,
            data_root,
            binary,
            node,
            vision,
            bind,
            process: None,
            boot: 0,
            endpoint_id: None,
            recovery_required: false,
            ownership_lock,
        };
        host.ensure_no_resident()?;
        Ok(host)
    }

    fn ensure_no_resident(&self) -> Result<()> {
        let path = self.root.join("control.sock");
        private_fs::ensure_directory(&self.root)?;
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        ensure!(
            meta.file_type().is_socket() && meta.uid() == private_fs::current_uid(),
            "control path is not an owned socket"
        );
        match UnixStream::connect(&path) {
            Ok(_) => bail!("live resident on root control socket"),
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                private_fs::ensure_directory(&self.root)?;
                let current = fs::symlink_metadata(&path)?;
                ensure!(
                    current.file_type().is_socket()
                        && current.uid() == meta.uid()
                        && current.dev() == meta.dev()
                        && current.ino() == meta.ino(),
                    "control socket changed during probe"
                );
                fs::remove_file(path)?;
                fs::File::open(&self.root)?.sync_all()?;
                Ok(())
            }
            Err(e) => Err(e).context("cannot safely probe root control socket"),
        }
    }

    fn identity(&self) -> Result<MctLocalNodeIdentity> {
        let identity = MctDaemonConfigStore::new(self.root.join("config.json"))
            .load()?
            .local_identity
            .context("daemon has not persisted local identity")?;
        ensure!(
            identity.node_id == self.node && identity.vision_id == self.vision,
            "persisted node or vision changed"
        );
        ensure!(
            identity.identity_path == self.root.join("identity/iroh-secret.hex"),
            "persisted identity path differs from root"
        );
        if let Some(expected) = &self.endpoint_id {
            ensure!(
                identity.endpoint_id.as_str() == expected,
                "persisted endpoint changed"
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
        .context("control socket unavailable")?;
        ensure!(
            (200..300).contains(&status),
            "control {method} {path} returned HTTP {status}: {}",
            String::from_utf8_lossy(&bytes)
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
            "owned daemon exited; stop before restarting"
        );
        Ok(())
    }

    fn start(&mut self) -> Result<Value> {
        ensure!(self.process.is_none(), "stop owned child before starting");
        self.ensure_no_resident()?;
        let digest = binary_hash(&self.binary).context("hash executable before startup")?;
        let (log_path, log) = loop {
            self.boot += 1;
            let path = auxiliary_path(&self.root, &format!("boot-{}.log", self.boot));
            match private_fs::create_new(&path) {
                Ok(file) => break (path, file),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error).context("create boot log"),
            }
        };
        let child = Command::new(&self.binary)
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
            .spawn()
            .context("spawn daemon")?;
        let pid = child.id();
        self.process = Some(child);
        let result = self.wait_ready(pid, &digest);
        if result.is_err() {
            let _ = self.stop();
        }
        result.with_context(|| format!("boot {} failed; log {}", self.boot, log_path.display()))
    }

    fn wait_ready(&mut self, pid: u32, digest: &str) -> Result<Value> {
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut last = String::from("no status");
        while Instant::now() < deadline {
            self.require_running()?;
            let timeout =
                Duration::from_secs(30).min(deadline.saturating_duration_since(Instant::now()));
            match self.request("GET", "/status", None, timeout) {
                Ok(status) if status["readiness"] == "ready" => {
                    ensure!(
                        status["resident"]["node_id"] == self.node.as_str()
                            && status["resident"]["vision_id"] == self.vision.as_str(),
                        "ready resident node or vision mismatch"
                    );
                    let snapshot: MotherIrohEndpointSnapshot =
                        serde_json::from_value(status["iroh_endpoint"].clone())
                            .context("invalid endpoint snapshot")?;
                    let identity = self.identity()?;
                    ensure!(
                        snapshot.lifecycle == MotherIrohEndpointLifecycle::Bound,
                        "endpoint not bound"
                    );
                    ensure!(
                        snapshot.relay_mode == MotherIrohRelayMode::Disabled
                            && snapshot.relay_urls.is_empty(),
                        "relays must be disabled"
                    );
                    ensure!(
                        snapshot
                            .direct_addresses
                            .iter()
                            .any(|s| s.parse::<SocketAddr>().ok() == Some(self.bind)),
                        "configured direct bind not advertised"
                    );
                    ensure!(
                        snapshot.endpoint_id == identity.endpoint_id,
                        "endpoint differs from persisted identity"
                    );
                    let ticket = MotherIrohEndpointTicket {
                        endpoint_id: snapshot.endpoint_id.clone(),
                        direct_addresses: vec![self.bind.to_string()],
                        relay_urls: vec![],
                    };
                    ensure!(
                        binary_hash(&self.binary)? == digest,
                        "daemon executable changed during startup"
                    );
                    self.require_running()?;
                    self.endpoint_id = Some(identity.endpoint_id.to_string());
                    self.recovery_required = false;
                    return Ok(
                        json!({"pid": pid, "status": status, "endpoint_id": self.endpoint_id, "ticket": ticket, "binary_sha256": digest, "boot": self.boot}),
                    );
                }
                Ok(status) => last = format!("readiness: {}", status["readiness"]),
                Err(error) => last = format!("{error:#}"),
            }
            thread::sleep(Duration::from_millis(100));
        }
        bail!("ready deadline elapsed: {last}")
    }

    fn stop(&mut self) -> Result<Value> {
        let mut escalated = false;
        let mut exit_signal = None;
        if let Some(child) = self.process.as_mut() {
            let stopped = stop_owned_child(child, Duration::from_secs(30))?;
            escalated = stopped.escalated;
            exit_signal = stopped.exit_signal;
            self.recovery_required |= stopped.recovery_required;
            self.process = None;
        }
        Ok(
            json!({"stopped": true, "escalated": escalated, "exit_signal": exit_signal, "recovery_required": self.recovery_required}),
        )
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.stop();
        let _ = &self.ownership_lock;
    }
}

const MAX_PUT: usize = 2 * 1024 * 1024;

fn validate_job(id: &str) -> Result<()> {
    ensure!(
        !id.is_empty()
            && id.len() <= 128
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b)),
        "invalid call ID"
    );
    CallId::new(id)?;
    Ok(())
}

fn put(root: &Path, relative: &str, digest: &str, encoded: &str) -> Result<Value> {
    ensure!(
        digest.len() == 64
            && digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid sha256"
    );
    ensure!(
        !relative.is_empty()
            && relative.split('/').all(|p| !p.is_empty()
                && p != "."
                && p != ".."
                && p.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))),
        "invalid relative path"
    );
    ensure!(
        encoded.len() <= MAX_PUT.div_ceil(3) * 4,
        "content too large"
    );
    let bytes = BASE64.decode(encoded).context("invalid base64")?;
    ensure!(bytes.len() <= MAX_PUT, "content too large");
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == digest,
        "sha256 mismatch"
    );
    private_fs::ensure_directory(root)?;
    let mut parent = root.to_path_buf();
    let mut parts = relative.split('/').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            break;
        }
        parent.push(part);
        private_fs::ensure_directory(&parent)?;
    }
    let path = root.join(relative);
    let existing = || -> Result<()> {
        let mut old = Vec::new();
        private_fs::read_file(&path)?
            .take((MAX_PUT + 1) as u64)
            .read_to_end(&mut old)?;
        ensure!(old == bytes, "existing file conflicts");
        Ok(())
    };
    match fs::symlink_metadata(&path) {
        Ok(_) => {
            existing()?;
            return Ok(
                json!({"path": relative, "sha256": digest, "size_bytes": bytes.len(), "reused": true}),
            );
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let temp = parent.join(private_fs::nonce_name("put")?);
    let mut file = private_fs::create_new(&temp)?;
    let result = (|| -> Result<bool> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        private_fs::ensure_directory(root)?;
        let mut checked = root.to_path_buf();
        for part in parent.strip_prefix(root)?.components() {
            checked.push(part.as_os_str());
            private_fs::ensure_directory(&checked)?;
        }
        match fs::hard_link(&temp, &path) {
            Ok(()) => {
                fs::File::open(&parent)?.sync_all()?;
                Ok(false)
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                existing()?;
                Ok(true)
            }
            Err(e) => Err(e.into()),
        }
    })();
    let cleanup = fs::remove_file(&temp);
    let reused = result?;
    cleanup?;
    fs::File::open(&parent)?.sync_all()?;
    existing()?;
    Ok(json!({"path": relative, "sha256": digest, "size_bytes": bytes.len(), "reused": reused}))
}

fn call_envelope(
    job: &str,
    payload: &str,
    target: &Target,
    revision: u64,
    authority: Value,
    deadline: &str,
) -> Result<Value> {
    validate_job(job)?;
    ensure!(payload.len() <= 32 * 1024, "payload too large");
    ensure!(
        serde_json::from_str::<Value>(payload)?.is_array(),
        "payload must be a JSON array"
    );
    for field in [
        &target.namespace,
        &target.interface_name,
        &target.function_name,
    ] {
        ensure!(
            field.len() <= 256 && !field.trim().is_empty(),
            "invalid target field"
        );
    }
    let target = OperationTarget::new(
        &target.namespace,
        &target.interface_name,
        &target.function_name,
    )?;
    let bytes = payload.as_bytes();
    Ok(json!({
        "protocol_request_id": format!("proto-{job}"), "call_id": job, "target": target,
        "payload_metadata": {"data_classification": "public", "size_bytes": bytes.len(), "contains_secret_scoped_material": false},
        "authority_context": {"policy_revision": revision, "expected_receiver_grants_authority": authority, "vision_policy_revision": 1},
        "deadline": deadline, "trace_context": {"trace_id": format!("trace-{job}"), "span_id": format!("span-{job}")},
        "payload": {"payload_kind": "inline_payload", "inline_payload_ref": format!("payload-{job}"), "content_type": "application/json", "size_bytes": bytes.len(), "blake3_digest_hex": blake3::hash(bytes).to_hex().to_string()},
        "inline_payload_base64": BASE64.encode(bytes), "idempotency_key": format!("first-real-app:{job}")
    }))
}

#[derive(Default)]
struct Counts {
    started: u64,
    completed: u64,
    failed: u64,
    allowed: u64,
    denied: u64,
}
impl Counts {
    fn add(&mut self, observation: &MctObservation, id: &str) {
        if observation.call_id.as_ref().map(|v| v.as_str()) != Some(id) {
            return;
        }
        match observation.kind {
            ObservationKind::RuntimeExecutionStarted => self.started += 1,
            ObservationKind::RuntimeExecutionCompleted => self.completed += 1,
            ObservationKind::RuntimeExecutionFailed | ObservationKind::RuntimeExecutionTimedOut => {
                self.failed += 1
            }
            ObservationKind::ToyGrantAllowed => self.allowed += 1,
            ObservationKind::ToyGrantDenied => self.denied += 1,
            _ => {}
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PeerCorrelation {
    caller_call_id: String,
    caller_node_id: String,
    caller_vision_id: String,
    peer_binding_id: String,
}

#[derive(Default)]
struct CorrelatedCounts {
    counts: Counts,
    correlated_id: Option<String>,
    direct_runtime: bool,
}

impl CorrelatedCounts {
    fn correlate(&mut self, observation: &MctObservation, id: &str) -> Result<()> {
        if observation.kind != ObservationKind::CallConstructed {
            return Ok(());
        }
        let Some(payload) = observation
            .detail_ref
            .as_deref()
            .and_then(|detail| detail.strip_prefix("mct-peer-call-correlation-v1:"))
        else {
            return Ok(());
        };
        // An invalid payload cannot safely be attributed to another caller.
        let correlation: PeerCorrelation =
            serde_json::from_str(payload).context("invalid peer call correlation")?;
        for field in [
            &correlation.caller_call_id,
            &correlation.caller_node_id,
            &correlation.caller_vision_id,
            &correlation.peer_binding_id,
        ] {
            ensure!(
                !field.trim().is_empty() && field.len() <= 1024,
                "invalid peer call correlation field"
            );
        }
        if correlation.caller_call_id != id {
            return Ok(());
        }
        let execution = observation
            .call_id
            .as_ref()
            .map(|value| value.as_str())
            .filter(|value| !value.trim().is_empty() && value.len() <= 1024)
            .context("missing or invalid correlation execution ID")?;
        if let Some(previous) = &self.correlated_id {
            ensure!(previous == execution, "conflicting peer call correlation");
        } else {
            self.correlated_id = Some(execution.to_owned());
        }
        Ok(())
    }

    fn count(&mut self, observation: &MctObservation, id: &str) {
        let call = observation.call_id.as_ref().map(|value| value.as_str());
        if call == Some(id) {
            if matches!(
                observation.kind,
                ObservationKind::RuntimeExecutionStarted
                    | ObservationKind::RuntimeExecutionCompleted
                    | ObservationKind::RuntimeExecutionFailed
                    | ObservationKind::RuntimeExecutionTimedOut
            ) {
                self.direct_runtime = true;
            }
            self.counts.add(observation, id);
        } else if let Some(execution) = &self.correlated_id {
            self.counts.add(observation, execution);
        }
    }

    fn execution_call_id<'a>(&'a self, id: &'a str) -> Option<&'a str> {
        self.correlated_id
            .as_deref()
            .or(if self.direct_runtime { Some(id) } else { None })
    }
}

impl Host {
    fn call(&mut self, job: &str, payload: &str, target: &Target) -> Result<Value> {
        self.require_running()?;
        let identity = self.identity()?;
        let authority =
            self.request("GET", "/receiver-authority", None, Duration::from_secs(30))?;
        let deadline = jiff::Timestamp::now()
            .checked_add(jiff::SignedDuration::from_secs(90))?
            .to_string();
        let envelope = call_envelope(
            job,
            payload,
            target,
            identity.policy_revision,
            authority,
            &deadline,
        )?;
        let mut reply = self.request("POST", "/calls", Some(&envelope), Duration::from_secs(90))?;
        let decoded = match reply.get("inline_result_payload_base64") {
            Some(Value::String(encoded)) => {
                serde_json::from_slice::<Value>(&BASE64.decode(encoded)?)?
            }
            Some(Value::Null) => Value::Null,
            _ => bail!("malformed or missing result payload"),
        };
        let object = reply
            .as_object_mut()
            .context("call response must be an object")?;
        object.insert("decoded_result".into(), decoded);
        object.insert("submitted_call_id".into(), json!(job));
        object.insert("submitted_trace_id".into(), json!(format!("trace-{job}")));
        Ok(reply)
    }

    fn ledger_summary(&self, id: &str) -> Result<Value> {
        validate_job(id)?;
        ensure!(self.process.is_none(), "stop daemon before reading ledger");
        let path = self.root.join("observations.jsonl");
        let verified = verify_ledger_streaming(&path, "ledger-local", self.node.as_str())?;
        let mut counts = CorrelatedCounts::default();
        // Each pass is checked against the verified ledger, including the counting pass.
        for correlation_pass in [true, false] {
            let mut reader = BufReader::new(private_fs::read_file(&path)?);
            let mut hasher = blake3::Hasher::new();
            let mut length = 0u64;
            loop {
                let mut line = Vec::new();
                let n = (&mut reader)
                    .take(1024 * 1024 + 1)
                    .read_until(b'\n', &mut line)?;
                if n == 0 {
                    break;
                }
                ensure!(
                    n <= 1024 * 1024 && line.last() == Some(&b'\n'),
                    "ledger line too long or unterminated"
                );
                hasher.update(&line);
                length += n as u64;
                let entry: MctObservationLedgerEntry = serde_json::from_slice(&line)?;
                if correlation_pass {
                    counts.correlate(&entry.observation, id)?;
                } else {
                    counts.count(&entry.observation, id);
                }
            }
            ensure!(
                length == verified.prefix_len
                    && hasher.finalize().to_hex().as_str() == verified.prefix_blake3,
                "ledger changed after verification"
            );
        }
        let mut result = json!({"call_id": id, "ledger_length": verified.head.as_ref().map_or(0, |h| h.local_sequence), "ledger_head_hash": verified.head.as_ref().map(|h| &h.entry_hash), "runtime_started": counts.counts.started, "runtime_completed": counts.counts.completed, "runtime_failed": counts.counts.failed, "toy_allowed": counts.counts.allowed, "toy_denied": counts.counts.denied});
        if let Some(execution) = counts.execution_call_id(id) {
            result["execution_call_id"] = json!(execution);
        }
        Ok(result)
    }
}

fn dispatch(host: &mut Option<Host>, request: Request) -> Result<Value> {
    match request {
        Request::Open {
            root,
            data_root,
            binary,
            node,
            vision,
            bind,
        } => {
            validate_absolute(&root)?;
            validate_absolute(&data_root)?;
            validate_absolute(&binary)?;
            validate_name(&node)?;
            validate_name(&vision)?;
            validate_bind(bind)?;
            ensure!(root != data_root, "root and data root must differ");
            ensure!(
                Path::new(&root).join("control.sock").as_os_str().len() < 104,
                "socket path too long"
            );
            ensure!(host.is_none(), "host already open");
            let opened = Host::open(
                root.into(),
                data_root.into(),
                binary.into(),
                node,
                vision,
                bind,
            )?;
            let metadata = json!({"root": opened.root, "data_root": opened.data_root, "binary": opened.binary, "node": opened.node.as_str(), "vision": opened.vision.as_str(), "bind": opened.bind});
            *host = Some(opened);
            Ok(metadata)
        }
        other => {
            let host = host.as_mut().context("open a host first")?;
            match other {
                Request::Start {} => host.start(),
                Request::Stop {} => host.stop(),
                Request::Peers {} => Ok(serde_json::to_value(
                    MctDaemonConfigStore::new(host.root.join("config.json"))
                        .load()?
                        .peers,
                )?),
                Request::Control { method, path, body } => {
                    ensure!(
                        allowed_control(&method, &path),
                        "control operation not permitted"
                    );
                    host.require_running()?;
                    host.request(
                        &method,
                        &path,
                        body.as_ref(),
                        Duration::from_secs(if path == "/calls" { 90 } else { 30 }),
                    )
                }
                Request::Put {
                    relative_path,
                    sha256,
                    content_base64,
                } => put(&host.data_root, &relative_path, &sha256, &content_base64),
                Request::Call {
                    job_id,
                    payload_json,
                    target,
                } => host.call(&job_id, &payload_json, &target),
                Request::LedgerCallSummary { call_id } => host.ledger_summary(&call_id),
                Request::Open { .. } => unreachable!(),
            }
        }
    }
}

fn main() -> Result<()> {
    let mut host = None;
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut stdout = io::stdout().lock();
    loop {
        let mut line = Vec::new();
        let mut oversized = false;
        loop {
            let available = reader.fill_buf()?;
            if available.is_empty() {
                break;
            }
            let n = available
                .iter()
                .position(|&b| b == b'\n')
                .map_or(available.len(), |i| i + 1);
            if line.len() + n > MAX_LINE {
                oversized = true;
            }
            if !oversized {
                line.extend_from_slice(&available[..n]);
            }
            let ended = available[n - 1] == b'\n';
            reader.consume(n);
            if ended {
                break;
            }
        }
        if line.is_empty() && !oversized {
            break;
        }
        let result = if oversized {
            Err(anyhow::anyhow!("request exceeds 4 MiB"))
        } else {
            serde_json::from_slice::<Request>(&line)
                .map_err(anyhow::Error::from)
                .and_then(|request| dispatch(&mut host, request))
        };
        let response = match result {
            Ok(value) => json!({"ok": true, "value": value}),
            Err(error) => {
                let text = format!("{error:#}");
                let bounded: String = text.chars().take(2048).collect();
                json!({"ok": false, "error": bounded})
            }
        };
        serde_json::to_writer(&mut stdout, &response)?;
        writeln!(stdout)?;
        stdout.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validation() {
        assert!(validate_name("node-1").is_ok());
        assert!(validate_name("../node").is_err());
        assert!(validate_absolute("/private/var/host").is_ok());
        assert!(validate_absolute("/private/tmp/host").is_err());
        assert!(validate_absolute("/tmp/host").is_err());
        for path in ["/", "/tmp", "/a/../b", "/a//b", "relative"] {
            assert!(validate_absolute(path).is_err());
        }
        assert!(validate_bind("192.168.1.2:1000".parse().unwrap()).is_ok());
        assert!(validate_bind("127.0.0.1:1000".parse().unwrap()).is_err());
    }
    #[test]
    fn allowlist() {
        assert!(allowed_control("GET", "/status"));
        assert!(allowed_control("POST", "/calls"));
        assert!(!allowed_control("POST", "/status"));
        assert!(!allowed_control("GET", "/calls"));
    }
    #[test]
    fn rejects_unknown_fields() {
        assert!(serde_json::from_str::<Request>(r#"{"op":"start","unexpected":1}"#).is_err());
    }
    fn private_tempdir(prefix: &str) -> tempfile::TempDir {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::Builder::new()
            .prefix(prefix)
            .tempdir_in(std::env::var_os("HOME").unwrap())
            .unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        dir
    }

    #[test]
    fn open_reuse_and_lock() {
        let dir = private_tempdir("mct-test-");
        let root = dir.path().join("root");
        let data = dir.path().join("data");
        let make_request = || Request::Open {
            root: root.to_str().unwrap().into(),
            data_root: data.to_str().unwrap().into(),
            binary: "/bin/sleep".into(),
            node: "node".into(),
            vision: "vision".into(),
            bind: "192.168.1.2:1234".parse().unwrap(),
        };
        let mut first = None;
        dispatch(&mut first, make_request()).unwrap();
        assert!(data.is_dir());
        let mut second = None;
        assert!(dispatch(&mut second, make_request()).is_err());
        assert!(
            dispatch(
                &mut first,
                Request::Control {
                    method: "GET".into(),
                    path: "/status".into(),
                    body: None
                }
            )
            .is_err()
        );
        drop(first);
        dispatch(&mut second, make_request()).unwrap();
        assert!(dispatch(&mut second, Request::Stop {}).is_ok());
        assert!(dispatch(&mut second, Request::Stop {}).is_ok());
    }

    #[test]
    fn put_validation_and_reuse() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let dir = private_tempdir("mct-put-");
        let root = dir.path().join("data");
        private_fs::ensure_directory(&root).unwrap();
        let digest = format!("{:x}", Sha256::digest(b"hello"));
        let encoded = BASE64.encode(b"hello");
        assert_eq!(
            put(&root, "a/file", &digest, &encoded).unwrap()["reused"],
            false
        );
        assert_eq!(
            put(&root, "a/file", &digest, &encoded).unwrap()["reused"],
            true
        );
        let other = format!("{:x}", Sha256::digest(b"other"));
        assert!(put(&root, "a/file", &other, &BASE64.encode(b"other")).is_err());
        for path in ["../file", "/file", "a/./file", "a//file", "a\\file"] {
            assert!(put(&root, path, &digest, &encoded).is_err());
        }
        assert!(put(&root, "x", &digest.to_uppercase(), &encoded).is_err());
        assert!(put(&root, "x", &digest, "!!").is_err());
        assert!(put(&root, "x", &other, &encoded).is_err());
        fs::hard_link(root.join("a/file"), root.join("hardlink")).unwrap();
        assert!(put(&root, "a/file", &digest, &encoded).is_err());
        fs::remove_file(root.join("hardlink")).unwrap();
        symlink(root.join("a/file"), root.join("link")).unwrap();
        assert!(put(&root, "link", &digest, &encoded).is_err());
        symlink(root.join("a"), root.join("dirlink")).unwrap();
        assert!(put(&root, "dirlink/new", &digest, &encoded).is_err());
        fs::set_permissions(root.join("a/file"), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(put(&root, "a/file", &digest, &encoded).is_err());
    }

    #[test]
    fn envelope_is_deterministic_and_array_only() {
        let target = Target {
            namespace: "patina:demo".into(),
            interface_name: "work@0.1.0".into(),
            function_name: "step".into(),
        };
        let a = call_envelope("job-1", "[41]", &target, 1, json!({}), "later").unwrap();
        let b = call_envelope(
            "job-1",
            "[41]",
            &target,
            2,
            json!({"changed": true}),
            "later-still",
        )
        .unwrap();
        for key in [
            "call_id",
            "target",
            "payload",
            "protocol_request_id",
            "trace_context",
            "idempotency_key",
        ] {
            assert_eq!(a[key], b[key]);
        }
        assert!(call_envelope("job", "{}", &target, 1, json!({}), "later").is_err());
        assert!(call_envelope("bad/job", "[]", &target, 1, json!({}), "later").is_err());
    }

    #[test]
    fn reduction_counts_only_matching_calls() {
        use mct_kernel::{ObservationId, Timestamp, TraceId};
        let mut o = MctObservation::informational(
            ObservationId::new("obs").unwrap(),
            Timestamp::new("2025-01-01T00:00:00Z").unwrap(),
            ObservationKind::RuntimeExecutionStarted,
            TraceId::new("trace").unwrap(),
            "test",
        );
        o.call_id = Some(CallId::new("job").unwrap());
        let mut counts = Counts::default();
        counts.add(&o, "other");
        counts.add(&o, "job");
        o.kind = ObservationKind::RuntimeExecutionTimedOut;
        counts.add(&o, "job");
        o.kind = ObservationKind::ToyGrantAllowed;
        counts.add(&o, "job");
        assert_eq!((counts.started, counts.failed, counts.allowed), (1, 1, 1));
    }

    fn observation(kind: ObservationKind, call: Option<&str>, detail: &str) -> MctObservation {
        use mct_kernel::{ObservationId, Timestamp, TraceId};
        let mut observation = MctObservation::informational(
            ObservationId::new("obs").unwrap(),
            Timestamp::new("2025-01-01T00:00:00Z").unwrap(),
            kind,
            TraceId::new("trace").unwrap(),
            detail,
        );
        observation.call_id = call.map(|id| CallId::new(id).unwrap());
        observation.detail_ref = Some(detail.to_owned());
        observation
    }

    fn correlation(caller: &str, execution: Option<&str>) -> MctObservation {
        observation(
            ObservationKind::CallConstructed,
            execution,
            &format!(
                "mct-peer-call-correlation-v1:{{\"caller_call_id\":\"{caller}\",\"caller_node_id\":\"node\",\"caller_vision_id\":\"vision\",\"peer_binding_id\":\"binding\"}}"
            ),
        )
    }

    #[test]
    fn correlated_counts_direct() {
        let mut counts = CorrelatedCounts::default();
        let fact = observation(
            ObservationKind::RuntimeExecutionStarted,
            Some("job"),
            "test",
        );
        counts.count(&fact, "job");
        assert_eq!(counts.counts.started, 1);
        assert_eq!(counts.execution_call_id("job"), Some("job"));
    }

    #[test]
    fn correlated_counts_forwarded_even_when_facts_precede_mapping() {
        let facts = [
            observation(
                ObservationKind::RuntimeExecutionStarted,
                Some("internal"),
                "test",
            ),
            observation(
                ObservationKind::RuntimeExecutionCompleted,
                Some("internal"),
                "test",
            ),
            observation(ObservationKind::ToyGrantAllowed, Some("internal"), "test"),
            correlation("job", Some("internal")),
        ];
        let mut counts = CorrelatedCounts::default();
        for fact in &facts {
            counts.correlate(fact, "job").unwrap();
        }
        for fact in &facts {
            counts.count(fact, "job");
        }
        assert_eq!(
            (
                counts.counts.started,
                counts.counts.completed,
                counts.counts.allowed
            ),
            (1, 1, 1)
        );
        assert_eq!(counts.execution_call_id("job"), Some("internal"));
    }

    #[test]
    fn correlated_counts_reject_malformed_matching_details() {
        for detail in [
            "mct-peer-call-correlation-v1:{\"caller_call_id\":\"job\"}",
            "mct-peer-call-correlation-v1:{\"caller_call_id\":\"job\",\"caller_node_id\":\"node\",\"caller_vision_id\":\"vision\",\"peer_binding_id\":\"binding\",\"extra\":1}",
            "mct-peer-call-correlation-v1:{\"caller_call_id\":\"job\"",
            "mct-peer-call-correlation-v1:{\"caller_call_id\":\"job\",\"caller_node_id\":\"\",\"caller_vision_id\":\"vision\",\"peer_binding_id\":\"binding\"}",
        ] {
            assert!(
                CorrelatedCounts::default()
                    .correlate(
                        &observation(ObservationKind::CallConstructed, Some("internal"), detail),
                        "job"
                    )
                    .is_err()
            );
        }
        assert!(
            CorrelatedCounts::default()
                .correlate(&correlation("job", None), "job")
                .is_err()
        );
    }

    #[test]
    fn correlated_counts_reject_conflicting_mapping() {
        let mut counts = CorrelatedCounts::default();
        counts
            .correlate(&correlation("job", Some("one")), "job")
            .unwrap();
        counts
            .correlate(&correlation("job", Some("one")), "job")
            .unwrap();
        assert!(
            counts
                .correlate(&correlation("job", Some("two")), "job")
                .is_err()
        );
    }

    #[test]
    fn correlated_counts_ignore_unrelated_mapping() {
        let mut counts = CorrelatedCounts::default();
        counts
            .correlate(&correlation("other", None), "job")
            .unwrap();
        counts
            .correlate(&correlation("other", Some("internal")), "job")
            .unwrap();
        counts.count(
            &observation(
                ObservationKind::RuntimeExecutionStarted,
                Some("internal"),
                "test",
            ),
            "job",
        );
        counts
            .correlate(
                &observation(
                    ObservationKind::CallConstructed,
                    Some("internal"),
                    "other mct-peer-call-correlation-v1:invalid",
                ),
                "job",
            )
            .unwrap();
        assert_eq!(counts.counts.started, 0);
        assert_eq!(counts.execution_call_id("job"), None);
    }

    #[test]
    fn stale_socket_is_removed_but_live_and_regular_paths_are_not() {
        use std::os::unix::net::UnixListener;
        let dir = private_tempdir("mct-socket-");
        let root = dir.path().join("root");
        let data = dir.path().join("data");
        let host = Host::open(
            root.clone(),
            data,
            "/bin/sleep".into(),
            "node".into(),
            "vision".into(),
            "192.168.1.2:1234".parse().unwrap(),
        )
        .unwrap();
        let path = root.join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        assert!(host.ensure_no_resident().is_err());
        assert!(path.exists());
        drop(listener);
        host.ensure_no_resident().unwrap();
        assert!(!path.exists());
        private_fs::create_new(&path).unwrap();
        assert!(host.ensure_no_resident().is_err());
        assert!(path.exists());
    }

    #[test]
    fn stops_owned_process() {
        let mut child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let result = stop_owned_child(&mut child, Duration::from_secs(2)).unwrap();
        assert!(!result.escalated);
        assert_eq!(result.exit_signal, Some(15));
    }
}
