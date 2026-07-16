//! Resident observation persistence and Iroh durability adaptation.

use super::*;

const RESIDENT_LEDGER_QUEUE_CAPACITY: usize = 256;

#[derive(Clone, Debug)]
pub(crate) struct ResidentLedgerWriter {
    sender: tokio::sync::mpsc::Sender<ResidentLedgerWrite>,
    fenced: Arc<std::sync::atomic::AtomicBool>,
    path: Option<Arc<PathBuf>>,
}

struct ResidentLedgerWrite {
    observations: Vec<MctObservation>,
    durability: DurabilityClass,
    ack: tokio::sync::oneshot::Sender<std::result::Result<(), String>>,
}

impl ResidentLedgerWriter {
    #[cfg(test)]
    pub(crate) fn fail_after_batches_for_test(path: PathBuf, allowed_batches: usize) -> Self {
        let mut ledger = JsonlObservationLedger::open(&path, "ledger-local", "local-mct").unwrap();
        let (sender, mut receiver) = tokio::sync::mpsc::channel::<ResidentLedgerWrite>(8);
        let fenced = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let task_fenced = Arc::clone(&fenced);
        tokio::task::spawn_blocking(move || {
            let mut completed = 0usize;
            while let Some(write) = receiver.blocking_recv() {
                if completed >= allowed_batches {
                    task_fenced.store(true, Ordering::SeqCst);
                    let _ = write.ack.send(Err("injected resident writer loss".into()));
                    continue;
                }
                let appended_at = mct_daemon::current_timestamp_string();
                let result = write
                    .observations
                    .into_iter()
                    .try_for_each(|observation| {
                        ledger
                            .append_before_effect(observation, appended_at.clone())
                            .map(|_| ())
                    })
                    .map_err(|error| error.to_string());
                completed += 1;
                let _ = write.ack.send(result);
            }
        });
        Self {
            sender,
            fenced,
            path: Some(Arc::new(path)),
        }
    }

    #[cfg(test)]
    pub(crate) fn failed_for_test() -> Self {
        let (sender, receiver) = tokio::sync::mpsc::channel(1);
        drop(receiver);
        Self {
            sender,
            fenced: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            path: None,
        }
    }

    pub(crate) fn spawn(path: PathBuf) -> Result<Self> {
        let mut ledger = JsonlObservationLedger::open(&path, "ledger-local", "local-mct")
            .with_context(|| format!("open observation ledger {}", path.display()))?;
        let (sender, mut receiver) =
            tokio::sync::mpsc::channel::<ResidentLedgerWrite>(RESIDENT_LEDGER_QUEUE_CAPACITY);
        tokio::task::spawn_blocking(move || {
            while let Some(write) = receiver.blocking_recv() {
                let appended_at = mct_daemon::current_timestamp_string();
                let result = write
                    .observations
                    .into_iter()
                    .try_for_each(|observation| match write.durability {
                        DurabilityClass::BeforeEffect => ledger
                            .append_before_effect(observation, appended_at.clone())
                            .map(|_| ()),
                        DurabilityClass::Buffered | DurabilityClass::ProjectionOnly => ledger
                            .append(
                                observation,
                                appended_at.clone(),
                                write.durability,
                                ExportStatus::NotRequired,
                            )
                            .map(|_| ()),
                    })
                    .map_err(|error| error.to_string());
                let _ = write.ack.send(result);
            }
        });
        Ok(Self {
            sender,
            fenced: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            path: Some(Arc::new(path)),
        })
    }

    pub(crate) fn is_fenced(&self) -> bool {
        self.fenced.load(Ordering::SeqCst)
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        self.path.as_deref().map(PathBuf::as_path)
    }

    pub(crate) async fn append(&self, observations: Vec<MctObservation>) -> Result<()> {
        self.append_with_durability(observations, DurabilityClass::BeforeEffect)
            .await
    }

    pub(crate) async fn append_with_durability(
        &self,
        observations: Vec<MctObservation>,
        durability: DurabilityClass,
    ) -> Result<()> {
        if observations.is_empty() {
            return Ok(());
        }
        if self.is_fenced() {
            bail!("resident observation writer is fenced");
        }
        let (ack, rx) = tokio::sync::oneshot::channel();
        if let Err(error) = self
            .sender
            .send(ResidentLedgerWrite {
                observations,
                durability,
                ack,
            })
            .await
        {
            self.fenced.store(true, Ordering::SeqCst);
            return Err(error).context("send observations to resident ledger writer");
        }
        let result = match rx.await {
            Ok(result) => result.map_err(anyhow::Error::msg),
            Err(error) => Err(error).context("receive resident ledger writer acknowledgement"),
        };
        if result.is_err() {
            self.fenced.store(true, Ordering::SeqCst);
        }
        result
    }

    pub(crate) async fn close(self) {
        drop(self.sender);
    }
}

pub(crate) fn resident_iroh_observation_sink(
    ledger: ResidentLedgerWriter,
) -> MctIrohObservationSink {
    MctIrohObservationSink::new(move |batch: MctIrohObservationBatch| {
        let ledger = ledger.clone();
        async move {
            let durability = match batch.durability {
                MctIrohObservationDurability::BeforeEffect => DurabilityClass::BeforeEffect,
                MctIrohObservationDurability::Buffered => DurabilityClass::Buffered,
            };
            let observed_at = current_timestamp();
            let observations = batch
                .facts
                .iter()
                .map(|fact| fact.to_observation(observed_at.clone()))
                .collect();
            ledger
                .append_with_durability(observations, durability)
                .await
                .map_err(|error| std::io::Error::other(error.to_string()))
        }
    })
}

pub(super) fn resident_endpoint_observation(
    observation_id: &'static str,
    endpoint_id: EndpointIdText,
    outcome: ObservationOutcome,
    safe_message: &'static str,
) -> MctObservation {
    MctObservation {
        observation_id: ObservationId::new(observation_id)
            .expect("string ID literal/generated value must be non-empty"),
        observed_at: current_timestamp(),
        kind: ObservationKind::AdapterEffectCompleted,
        source_plane: SourcePlane::Adapter,
        trace: ObservationTraceRef {
            trace_id: TraceId::new("trace-resident-mother")
                .expect("string ID literal/generated value must be non-empty"),
            span_id: None,
            parent_span_id: None,
            external_trace_id: None,
        },
        call_id: None,
        decision_id: None,
        subject_id: Some(endpoint_id.to_string()),
        resource_id: Some("mct-iroh-endpoint".into()),
        policy_revision: Some(1),
        grants_revision: Some(1),
        outcome,
        visibility: ObservationVisibility::InternalOnly,
        safe_message: safe_message.into(),
        detail_ref: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract_peer_expiry() -> Timestamp {
        Timestamp::new("2099-01-01T00:00:00Z").unwrap()
    }

    /// Covers `MctLocalFirstObservationLedger.BufferedEffectsAreBounded`.
    #[tokio::test]
    async fn resident_observation_queue_is_bounded_and_acknowledged() {
        let dir = tempfile::tempdir().unwrap();
        let ledger = ResidentLedgerWriter::spawn(dir.path().join("observations.jsonl")).unwrap();

        assert_eq!(ledger.sender.max_capacity(), RESIDENT_LEDGER_QUEUE_CAPACITY);
        assert_eq!(ledger.sender.capacity(), RESIDENT_LEDGER_QUEUE_CAPACITY);
        ledger
            .append_with_durability(
                vec![resident_endpoint_observation(
                    "obs-bounded-resident-writer",
                    EndpointIdText::new("endpoint-bounded-resident-writer").unwrap(),
                    ObservationOutcome::Completed,
                    "bounded resident writer",
                )],
                DurabilityClass::Buffered,
            )
            .await
            .unwrap();
        assert_eq!(ledger.sender.capacity(), RESIDENT_LEDGER_QUEUE_CAPACITY);
        ledger.close().await;

        let entries = JsonlObservationLedger::open_read_only(
            dir.path().join("observations.jsonl"),
            "ledger-local",
            "local-mct",
        )
        .unwrap()
        .entries()
        .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].durability_class, DurabilityClass::Buffered);
    }

    #[tokio::test]
    async fn resident_hello_observations_are_durable_before_responses() {
        let dir = tempfile::tempdir().unwrap();
        let ledger_path = dir.path().join("observations.jsonl");
        let ledger = ResidentLedgerWriter::spawn(ledger_path.clone()).unwrap();
        let server = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut admitted_client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let mut denied_client = MotherIrohEndpoint::bind_local_mct().await.unwrap();
        let ticket = server.ticket();
        let admitted_endpoint_id = admitted_client.snapshot().endpoint_id;
        let denied_endpoint_id = denied_client.snapshot().endpoint_id;
        let binding = MctPeerBinding {
            binding_id: PeerBindingId::new("binding-durable-hello")
                .expect("string ID literal/generated value must be non-empty"),
            iroh_endpoint_id: admitted_endpoint_id.clone(),
            scope: MctPeerBindingScope {
                mct_node_id: MctNodeId::new("mother-durable-client")
                    .expect("string ID literal/generated value must be non-empty"),
                vision_id: VisionId::new("vision-local")
                    .expect("string ID literal/generated value must be non-empty"),
                allowed_alpns: vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()],
                data_scope: None,
                observation_scope: None,
            },
            issuer_node_id: MctNodeId::new("local-mct")
                .expect("string ID literal/generated value must be non-empty"),
            policy_revision: 1,
            binding_state: BindingState::Admitted,
            issued_at: Timestamp::new("2026-07-09T00:00:00Z").unwrap(),
            expires_at: contract_peer_expiry(),
            created_by_observation_id: ObservationId::new("obs-binding-durable-hello")
                .expect("string ID literal/generated value must be non-empty"),
            superseded_by_observation_id: None,
        };
        let observation_sink = resident_iroh_observation_sink(ledger.clone());
        let serve_task = tokio::spawn(async move {
            server
                .serve_concurrent_with_call_handler(
                    MctIrohServeState::new(),
                    vec![binding],
                    MctIrohConcurrentServeConfig::new(observation_sink),
                    || Timestamp::new("2026-07-09T00:00:02Z").unwrap(),
                    |_, _, _| async { MctIrohCallHandlerResult::accepted_for_routing(None) },
                )
                .await
        });

        let admitted_trace = TraceId::new("trace-durable-admitted-hello")
            .expect("string ID literal/generated value must be non-empty");
        let signature_marker = "key-material-must-not-enter-hello-observation";
        let admitted_hello = cli_hello_request(
            &admitted_endpoint_id,
            &PeerBindingId::new("binding-durable-hello")
                .expect("string ID literal/generated value must be non-empty"),
            &MctNodeId::new("mother-durable-client")
                .expect("string ID literal/generated value must be non-empty"),
            &VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            &admitted_trace,
            Some(signature_marker.into()),
        );
        let admitted_response = admitted_client
            .send_hello(&ticket, &admitted_hello)
            .await
            .unwrap();
        assert_eq!(admitted_response.hello_outcome, HelloOutcome::Admitted);
        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        assert!(entries.iter().any(|entry| {
            entry.observation.trace.trace_id == admitted_trace
                && entry.observation.kind == ObservationKind::PeerAdmitted
                && entry.durability_class == mct_observation::DurabilityClass::BeforeEffect
        }));

        let denied_trace = TraceId::new("trace-durable-denied-hello")
            .expect("string ID literal/generated value must be non-empty");
        let denied_hello = cli_hello_request(
            &denied_endpoint_id,
            &PeerBindingId::new("binding-durable-hello")
                .expect("string ID literal/generated value must be non-empty"),
            &MctNodeId::new("mother-unknown-client")
                .expect("string ID literal/generated value must be non-empty"),
            &VisionId::new("vision-local")
                .expect("string ID literal/generated value must be non-empty"),
            &denied_trace,
            Some(signature_marker.into()),
        );
        let denied_response = denied_client
            .send_hello(&ticket, &denied_hello)
            .await
            .unwrap();
        assert_eq!(denied_response.hello_outcome, HelloOutcome::Denied);
        let entries =
            JsonlObservationLedger::open_read_only(&ledger_path, "ledger-local", "local-mct")
                .unwrap()
                .entries()
                .unwrap();
        assert!(entries.iter().any(|entry| {
            entry.observation.trace.trace_id == denied_trace
                && entry.observation.kind == ObservationKind::PeerRejected
                && entry.durability_class == mct_observation::DurabilityClass::BeforeEffect
        }));
        let ledger_text = std::fs::read_to_string(&ledger_path).unwrap();
        assert!(!ledger_text.contains(signature_marker));
        assert!(!ledger_text.contains("inline_payload_base64"));

        admitted_client.close().await;
        denied_client.close().await;
        serve_task.abort();
        ledger.close().await;
    }
}
