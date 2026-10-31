//! Wire-only resource policy. Historical/local kernel IDs are not rewritten.
use super::*;

pub(super) const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);
pub(super) const FRAME_TIMEOUT: Duration = Duration::from_secs(3);
pub(super) const RESPONSE_TIMEOUT: Duration = Duration::from_secs(3);
pub(super) const CLOSE_TIMEOUT: Duration = Duration::from_secs(1);
pub(super) const IDENTIFIER_MAX_BYTES: usize = 256;
const IDENTIFIER_TOTAL_MAX_BYTES: usize = 8 * 1024;

fn identifier_field(field: &str) -> bool {
    field.ends_with("_id")
        || matches!(
            field,
            "authority_epoch"
                | "idempotency_key"
                | "namespace"
                | "interface_name"
                | "function_name"
                | "data_classification"
                | "child_name"
                | "protocol_name"
        )
}

/// Check the decoded *typed* wire shape, including nested capability/authority
/// identities. Unknown JSON fields never become facts. Payload bytes, signature
/// references and URLs are not IDs; they retain the outer 96 KiB frame bound.
/// No attacker text is included in the error or a rejected request's facts.
pub(super) fn validate_identifiers(request: &impl Serialize) -> MotherIrohEndpointResult<()> {
    fn walk(value: &serde_json::Value, field: &str, remaining: &mut usize) -> bool {
        match value {
            serde_json::Value::String(value) if identifier_field(field) => {
                if value.is_empty()
                    || value.len() > IDENTIFIER_MAX_BYTES
                    || value.len() > *remaining
                    || !value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.:/@".contains(&b))
                {
                    return false;
                }
                *remaining -= value.len();
                true
            }
            serde_json::Value::Object(fields) => fields
                .iter()
                .all(|(key, value)| walk(value, key, remaining)),
            serde_json::Value::Array(values) => {
                values.iter().all(|value| walk(value, field, remaining))
            }
            _ => true,
        }
    }
    let value =
        serde_json::to_value(request).map_err(|source| MotherIrohEndpointError::ProtocolJson {
            action: "inspect wire identifiers",
            source,
        })?;
    let mut remaining = IDENTIFIER_TOTAL_MAX_BYTES;
    if walk(&value, "", &mut remaining) {
        Ok(())
    } else {
        Err(MotherIrohEndpointError::ProtocolIo {
            action: "validate wire identifiers",
            source: boxed_source(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid wire identifier",
            )),
        })
    }
}

pub(super) fn hello_request(
    bytes: &[u8],
    remote: &EndpointIdText,
    state: &mut MctIrohServeState,
) -> (MctHelloRequest, bool) {
    if let Ok(request) = serde_json::from_slice::<MctHelloRequest>(bytes)
        && validate_identifiers(&request).is_ok()
    {
        return (request, true);
    }
    // Malformed hello still receives a durable rejection. Use fresh local
    // correlation, never truncate/alias a hostile ID or retain its capability.
    let suffix = state.next_suffix();
    (
        MctHelloRequest {
            hello_id: format!("malformed-hello-{suffix}"),
            received_over: IrohConnectionPresentation {
                endpoint_id: remote.clone(),
                alpn: MCT_HELLO_ALPN.into(),
                connection_side: ConnectionSide::Incoming,
                path_class: PathClass::Unknown,
                relay_url: None,
                presented_capability_ref: None,
            },
            requested_protocol: HelloPolicy::default().protocol,
            requested_vision_id: None,
            requested_alpns: Vec::new(),
            presented_binding: MctPeerBindingPresentation {
                binding_id: None,
                endpoint_id: remote.clone(),
                mct_node_id: None,
                vision_id: None,
                policy_revision: None,
                allowed_alpns_claim: Vec::new(),
                signature_ref: None,
                expires_at: None,
            },
            capability_view: None,
            local_policy_revision_seen: None,
            trace_id: TraceId::new(format!("trace-malformed-hello-{suffix}")).unwrap(),
            received_observation_id: state.next_observation_id("hello-malformed"),
        },
        false,
    )
}

pub(super) fn no_peer_authority() -> MctPeerAuthoritySnapshot {
    MctPeerAuthoritySnapshot {
        bindings: Vec::new(),
        policy_revision: 0,
    }
}

pub(super) async fn deadline<T>(
    duration: Duration,
    action: &'static str,
    future: impl Future<Output = MotherIrohEndpointResult<T>>,
) -> MotherIrohEndpointResult<T> {
    tokio::time::timeout(duration, future)
        .await
        .map_err(|_| MotherIrohEndpointError::ProtocolTimeout { action })?
}

pub(super) async fn accept_connection(
    incoming: iroh::endpoint::Incoming,
    limit: Duration,
) -> MotherIrohEndpointResult<(iroh::endpoint::Connection, Vec<u8>)> {
    deadline(limit, "incoming handshake/ALPN", async {
        let mut accepting =
            incoming
                .accept()
                .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                    action: "accept incoming connection",
                    source: boxed_source(source),
                })?;
        let alpn =
            accepting
                .alpn()
                .await
                .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                    action: "read incoming ALPN",
                    source: boxed_source(source),
                })?;
        let connection = accepting
            .await
            .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                action: "finish incoming connection",
                source: boxed_source(source),
            })?;
        Ok((connection, alpn))
    })
    .await
}

pub(super) async fn read_frame(
    connection: &iroh::endpoint::Connection,
    limit: Duration,
) -> MotherIrohEndpointResult<(
    iroh::endpoint::SendStream,
    MotherIrohEndpointResult<Vec<u8>>,
)> {
    let deadline = tokio::time::Instant::now() + limit;
    let (send, mut recv) = tokio::time::timeout_at(deadline, connection.accept_bi())
        .await
        .map_err(|_| MotherIrohEndpointError::ProtocolTimeout {
            action: "incoming first stream",
        })?
        .map_err(|source| MotherIrohEndpointError::ProtocolIo {
            action: "accept bidirectional stream",
            source: boxed_source(source),
        })?;
    let frame =
        tokio::time::timeout_at(deadline, recv.read_to_end(MCT_CALL_FRAME_READ_BUDGET_BYTES))
            .await
            .map_err(|_| MotherIrohEndpointError::ProtocolTimeout {
                action: "incoming request frame",
            })
            .and_then(|result| {
                result.map_err(|source| MotherIrohEndpointError::ProtocolIo {
                    action: "read request stream",
                    source: boxed_source(source),
                })
            });
    Ok((send, frame))
}

pub(super) async fn write_response(
    send: &mut iroh::endpoint::SendStream,
    bytes: &[u8],
    limit: Duration,
) -> MotherIrohEndpointResult<()> {
    deadline(limit, "write response stream", async {
        send.write_all(bytes)
            .await
            .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                action: "write response stream",
                source: boxed_source(source),
            })?;
        send.finish()
            .map_err(|source| MotherIrohEndpointError::ProtocolIo {
                action: "finish response stream",
                source: boxed_source(source),
            })
    })
    .await
}

pub(super) async fn close_after_response(connection: &iroh::endpoint::Connection, limit: Duration) {
    // FIN queues delivery; do not immediately close and discard the response.
    // But a peer that reads it without closing cannot retain an admission slot.
    if tokio::time::timeout(limit, connection.closed())
        .await
        .is_err()
    {
        connection.close(0u32.into(), b"MCT response complete");
    }
}
