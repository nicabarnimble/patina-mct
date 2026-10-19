//! Six real resident processes, one Vision, independent policy counters.
//! Loopback Iroh is real; this is not WAN/partition or admin-cutover proof.
//! Setup uses today's owner-only UDS and an offline config change for policy
//! revisions. No application result, capability advertisement, or ledger fact
//! is injected. MCT_FLEET_KEEP preserves isolated roots/logs on failure.
#![cfg(unix)]

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use mct_daemon::{MctDaemonConfigStore, MctRuntimeStateStore};
use mct_iroh::{MotherIrohEndpointSnapshot, MotherIrohEndpointTicket};
use mct_kernel::{BindingState, MctNodeId, VisionId};
use mct_observation::verify_ledger_streaming;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    net::UdpSocket,
    os::unix::net::UnixStream,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const VISION: &str = "vision-fleet";
const COUNT: usize = 6;
const READY_TIMEOUT: Duration = Duration::from_secs(60);

struct Mother {
    _work: Option<tempfile::TempDir>,
    work: PathBuf,
    root: PathBuf,
    node: String,
    revision: u64,
    bind: String,
    process: Option<Child>,
    boot: usize,
}

impl Mother {
    fn new(index: usize) -> Self {
        // Short paths also fit macOS's sockaddr_un limit.
        let work = tempfile::Builder::new()
            .prefix("mct-fleet-")
            .tempdir_in("/tmp")
            .unwrap();
        let work_path = work.path().to_path_buf();
        let root = work_path.join("root");
        std::fs::create_dir(&root).unwrap();
        let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
        let bind = socket.local_addr().unwrap().to_string();
        drop(socket);
        let work = if std::env::var_os("MCT_FLEET_KEEP").is_some() {
            eprintln!("fleet mother-{index}: keeping {}", work_path.display());
            let _ = work.keep();
            None
        } else {
            Some(work)
        };
        Self {
            _work: work,
            work: work_path,
            root,
            node: format!("mother-{index}"),
            revision: [1, 7, 2, 11, 3, 5][index],
            bind,
            process: None,
            boot: 0,
        }
    }

    fn config(&self) -> MctDaemonConfigStore {
        MctDaemonConfigStore::new(self.root.join("config.json"))
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&Value>,
    ) -> std::io::Result<(u16, Value)> {
        let body = body.map_or_else(Vec::new, |v| serde_json::to_vec(v).unwrap());
        let mut stream = UnixStream::connect(self.root.join("control.sock"))?;
        stream.set_read_timeout(Some(Duration::from_secs(15)))?;
        stream.set_write_timeout(Some(Duration::from_secs(15)))?;
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: local\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )?;
        stream.write_all(&body)?;
        let mut response = String::new();
        stream.read_to_string(&mut response)?;
        let invalid = || std::io::Error::other(format!("invalid response: {response}"));
        let status = response
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or_else(invalid)?;
        let (_, body) = response.split_once("\r\n\r\n").ok_or_else(invalid)?;
        let body = serde_json::from_str(body).map_err(|_| invalid())?;
        Ok((status, body))
    }

    fn ok(&self, method: &str, path: &str, body: Option<&Value>) -> Value {
        let (status, reply) = self
            .request(method, path, body)
            .unwrap_or_else(|e| panic!("{} {method} {path}: {e}\n{}", self.node, self.log()));
        assert_eq!(
            status,
            200,
            "{} {method} {path}: {reply}\n{}",
            self.node,
            self.log()
        );
        reply
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.work.join(format!("boot-{}.log", self.boot)))
            .unwrap_or_default()
    }

    fn start(&mut self) {
        assert!(self.process.is_none());
        self.boot += 1;
        let log = std::fs::File::create(self.work.join(format!("boot-{}.log", self.boot))).unwrap();
        self.process = Some(
            Command::new(env!("CARGO_BIN_EXE_mct-daemon"))
                .args([
                    "serve",
                    "--node-id",
                    &self.node,
                    "--vision",
                    VISION,
                    "--bind",
                    &self.bind,
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
                .current_dir(&self.work)
                .stdin(Stdio::null())
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + READY_TIMEOUT;
        loop {
            let exit = self.process.as_mut().unwrap().try_wait().unwrap();
            assert!(
                exit.is_none(),
                "{} exited {exit:?}: {}",
                self.node,
                self.log()
            );
            if let Ok((200, status)) = self.request("GET", "/status", None)
                && status["readiness"] == "ready"
            {
                assert_eq!(status["resident"]["node_id"], self.node);
                assert_eq!(status["resident"]["vision_id"], VISION);
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{} startup timed out: {}",
                self.node,
                self.log()
            );
            thread::sleep(Duration::from_millis(25));
        }
    }

    fn stop(&mut self) {
        if let Some(mut process) = self.process.take() {
            let _ = process.kill();
            process.wait().unwrap();
        }
    }

    fn ticket(&self) -> MotherIrohEndpointTicket {
        let status = self.ok("GET", "/status", None);
        let snapshot: MotherIrohEndpointSnapshot =
            serde_json::from_value(status["iroh_endpoint"].clone()).unwrap();
        MotherIrohEndpointTicket::from_snapshot(&snapshot)
    }

    fn provision(&self, index: usize) {
        let source = self.work.join("source");
        std::fs::create_dir(&source).unwrap();
        let namespace = format!("patina:fleet-n{index}");
        let component = wat::parse_str(format!(
            r#"(component
          (core module $m (func (export "step") (param i32) (result i32)
            (i32.add (local.get 0) (i32.const {index}))))
          (core instance $i (instantiate $m))
          (func $step (param "value" s32) (result s32) (canon lift (core func $i "step")))
          (instance $work (export "step" (func $step)))
          (export "{namespace}/work@0.1.0" (instance $work)))"#
        ))
        .unwrap();
        std::fs::write(source.join("worker.wasm"), &component).unwrap();
        std::fs::write(
            source.join("child.toml"),
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
            ),
        )
        .unwrap();
        let stage = self.ok("POST", "/artifacts/stage", Some(&json!({
            "source_root": source, "manifest_path": "child.toml", "component_path": "worker.wasm",
            "claimed_child_name": "worker", "claimed_artifact_version": "0.1.0",
            "expected_digest": format!("blake3:{}", blake3::hash(&component).to_hex()),
            "standing_source_authority_id": null, "claimed_publisher": null,
            "require_source_sidecars": false, "children_dir": self.root.join("children"),
            "state_path": self.root.join("state.sqlite")
        })));
        assert_eq!(stage["verification_outcome"], "verified");
        let approved = self.ok("POST", "/children/approve", Some(&json!({
            "expected_config_path": self.root.join("config.json"),
            "expected_children_dir": self.root.join("children"),
            "expected_state_path": self.root.join("state.sqlite"),
            "expected_artifact_id": stage["artifact_id"], "child_name": "worker", "strict_integrity": true
        })));
        assert_eq!(approved["approval_state"], "approved");
    }

    fn call(&self, destination: usize, label: &str) -> Value {
        let authority = self.ok("GET", "/receiver-authority", None);
        let payload = b"[41]";
        let id = format!("{}-to-{destination}-{label}", self.node);
        self.ok("POST", "/calls", Some(&json!({
            "protocol_request_id": format!("proto-{id}"), "call_id": id,
            "target": {"namespace": format!("patina:fleet-n{destination}"), "interface_name": "work@0.1.0", "function_name": "step"},
            "payload_metadata": {"data_classification": "public", "size_bytes": payload.len(), "contains_secret_scoped_material": false},
            "authority_context": {"policy_revision": self.revision, "expected_receiver_grants_authority": authority, "vision_policy_revision": 1},
            "deadline": "2099-01-01T00:00:00Z",
            "trace_context": {"trace_id": format!("trace-{id}"), "span_id": format!("span-{id}")},
            "payload": {"payload_kind": "inline_payload", "inline_payload_ref": format!("payload-{id}"), "content_type": "application/json", "size_bytes": payload.len(), "blake3_digest_hex": blake3::hash(payload).to_hex().to_string()},
            "inline_payload_base64": BASE64.encode(payload), "idempotency_key": format!("key-{id}")
        })))
    }

    fn assert_call(&self, destination: usize, label: &str) {
        let reply = self.call(destination, label);
        assert_eq!(
            reply["outcome"],
            "completed",
            "{} -> {destination}: {reply}\n{}",
            self.node,
            self.log()
        );
        let bytes = BASE64
            .decode(reply["inline_result_payload_base64"].as_str().unwrap())
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            json!({"results": [41 + destination]})
        );
        assert_eq!(
            reply["route_taken"]["node_id"],
            format!("mother-{destination}")
        );
    }
}

impl Drop for Mother {
    fn drop(&mut self) {
        self.stop();
    }
}

fn admit(receiver: &Mother, caller: &Mother) {
    let caller_ticket = caller.ticket();
    receiver.ok("POST", "/peers/add", Some(&json!({
        "expected_config_path": receiver.root.join("config.json"), "peer_node_id": caller.node,
        "binding_id": format!("{}-admits-{}", receiver.node, caller.node),
        "endpoint_id": caller_ticket.endpoint_id, "vision_id": VISION, "ticket": caller_ticket,
        "binding_signature_ref": null, "policy_revision": receiver.revision, "expires_at": "2099-01-01T00:00:00Z"
    })));
}

fn install_reverse_proof(caller: &Mother, receiver: &Mother) {
    let config = receiver.config().load().unwrap();
    let binding = &config.peers[&caller.node];
    caller.ok(
        "POST",
        "/peers/proof",
        Some(&json!({
            "expected_config_path": caller.root.join("config.json"), "peer_node_id": receiver.node,
            "binding_id": binding.binding_id, "policy_revision": binding.policy_revision,
            "signature_ref": binding.binding_signature_ref, "expires_at": binding.expires_at
        })),
    );
}

fn wait_for_catalog(mothers: &[Mother]) {
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        let mut missing = Vec::new();
        for mother in mothers {
            let state = MctRuntimeStateStore::open(mother.root.join("state.sqlite")).unwrap();
            for peer in mothers.iter().filter(|peer| peer.node != mother.node) {
                let surfaces = state
                    .remote_callable_surfaces(
                        &MctNodeId::new(&peer.node).unwrap(),
                        &VisionId::new(VISION).unwrap(),
                    )
                    .unwrap();
                if surfaces.is_empty() {
                    missing.push(format!("{} <- {}", mother.node, peer.node));
                } else {
                    assert!(
                        surfaces
                            .iter()
                            .all(|surface| surface.publisher_policy_revision == peer.revision)
                    );
                }
            }
        }
        if missing.is_empty() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "missing admitted hello publications: {missing:?}"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn six_mothers_independent_revisions_survive_peer_loss_and_restart() {
    let mut mothers: Vec<_> = (0..COUNT)
        .map(|index| {
            let mut mother = Mother::new(index);
            mother.start(); // Virgin bootstrap, no pre-seeded authority or ledger.
            mother.stop();
            let store = mother.config();
            let mut config = store.load().unwrap();
            config.local_identity.as_mut().unwrap().policy_revision = mother.revision;
            store.save(&config).unwrap(); // Explicit offline fixture setup, not an admin guarantee.
            mother.start();
            mother.provision(index);
            mother
        })
        .collect();
    let identities: Vec<_> = mothers
        .iter()
        .map(|m| m.config().load().unwrap().local_identity.unwrap())
        .collect();
    assert_eq!(
        identities
            .iter()
            .map(|id| &id.endpoint_id)
            .collect::<BTreeSet<_>>()
            .len(),
        COUNT
    );
    assert_eq!(
        identities
            .iter()
            .map(|id| &id.node_id)
            .collect::<BTreeSet<_>>()
            .len(),
        COUNT
    );
    assert_eq!(
        mothers
            .iter()
            .map(|m| m.process.as_ref().unwrap().id())
            .collect::<BTreeSet<_>>()
            .len(),
        COUNT
    );

    for receiver in &mothers {
        for caller in mothers.iter().filter(|caller| caller.node != receiver.node) {
            admit(receiver, caller);
        }
    }
    for caller in &mothers {
        for receiver in mothers
            .iter()
            .filter(|receiver| receiver.node != caller.node)
        {
            install_reverse_proof(caller, receiver);
        }
    }
    // Startup's real admitted-peer refresh exchanges catalogs over Iroh. No
    // synthetic publication rows and no 120-second timer wait in the fixture.
    for mother in &mut mothers {
        mother.stop();
        mother.start();
    }
    wait_for_catalog(&mothers);
    for mother in &mothers {
        for destination in 0..COUNT {
            mother.assert_call(destination, "connected");
        }
    }

    // Receiver-side revocation must beat the caller's still-fresh cached offer.
    mothers[5].ok("POST", "/peers/revoke", Some(&json!({
        "expected_config_path": mothers[5].root.join("config.json"), "peer_node_id": mothers[0].node
    })));
    assert_eq!(mothers[0].call(5, "revoked")["outcome"], "denied");

    let mut prefixes = Vec::new();
    for victim in [5, 4] {
        mothers[victim].stop(); // SIGKILL: unrelated Mothers stay alive.
        if victim == 5 {
            // Boot refresh now encounters an unreachable admitted peer. Its
            // pending hello must not prevent inbound work on this survivor.
            mothers[0].stop();
            mothers[0].start();
        }
        prefixes.push((
            victim,
            std::fs::read(mothers[victim].root.join("observations.jsonl")).unwrap(),
        ));
        for (index, mother) in mothers[..victim].iter().enumerate() {
            mother.assert_call(index, &format!("local-with-{victim}-survivors"));
            mother.assert_call(
                (index + 1) % victim,
                &format!("remote-with-{victim}-survivors"),
            );
        }
    }
    for (index, prefix) in prefixes {
        mothers[index].start();
        let identity = mothers[index]
            .config()
            .load()
            .unwrap()
            .local_identity
            .unwrap();
        assert_eq!(identity.node_id, identities[index].node_id);
        assert_eq!(identity.endpoint_id, identities[index].endpoint_id);
        assert!(
            std::fs::read(mothers[index].root.join("observations.jsonl"))
                .unwrap()
                .starts_with(&prefix)
        );
        mothers[index].assert_call(index, "restarted-local");
    }
    assert_eq!(
        mothers[5].config().load().unwrap().peers[&mothers[0].node].binding_state,
        BindingState::Revoked
    );
    assert_eq!(
        mothers[0].call(5, "still-revoked-after-restart")["outcome"],
        "denied"
    );
    mothers[1].assert_call(5, "restart-allowed-peer");

    for (index, mother) in mothers.iter_mut().enumerate() {
        mother.stop();
        let identity = mother.config().load().unwrap().local_identity.unwrap();
        assert_eq!(identity.node_id, identities[index].node_id);
        assert_eq!(identity.endpoint_id, identities[index].endpoint_id);
        verify_ledger_streaming(
            mother.root.join("observations.jsonl"),
            "ledger-local",
            &mother.node,
        )
        .unwrap();
        let ledger = std::fs::read_to_string(mother.root.join("observations.jsonl")).unwrap();
        assert!(ledger.contains(&format!("executed_on:{};forwarded_from:", mother.node)));
        assert!(ledger.contains(&format!("forwarded_from:{};forwarded_to:", mother.node)));
    }
}
