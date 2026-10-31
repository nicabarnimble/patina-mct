use super::{ingress::*, *};

#[test]
fn wire_identifier_limits_cover_exact_boundary_characters_nested_and_aggregate_cases() {
    for field in [
        "protocol_request_id",
        "call_id",
        "trace_id",
        "span_id",
        "decision_id",
        "node_id",
        "vision_id",
        "user_id",
        "project_id",
        "binding_id",
        "operation_id",
        "received_observation_id",
        "source_authority_observation_id",
        "authority_epoch",
        "idempotency_key",
        "namespace",
        "interface_name",
        "function_name",
        "data_classification",
        "child_name",
        "protocol_name",
    ] {
        for good in ["a".repeat(IDENTIFIER_MAX_BYTES), "Az09-_.:/@".into()] {
            validate_identifiers(&serde_json::json!({ "nested": [{(field): good}] })).unwrap();
        }
        for bad in [
            "a".repeat(IDENTIFIER_MAX_BYTES + 1),
            "".into(),
            "a b".into(),
            "a\n".into(),
            "a\0".into(),
            "é".into(),
            "a\\b".into(),
        ] {
            assert!(
                validate_identifiers(&serde_json::json!({ "nested": [{(field): bad}] })).is_err(),
                "{field}"
            );
        }
    }
    let ids: Vec<_> = (0..32)
        .map(|_| serde_json::json!({"node_id": "x".repeat(256)}))
        .collect();
    validate_identifiers(&ids).unwrap();
    let mut over = ids;
    over.push(serde_json::json!({"node_id": "x"}));
    assert!(validate_identifiers(&over).is_err());
    // Wire-only validation must not truncate historical/local kernel IDs.
    assert!(TraceId::new("x".repeat(257)).is_ok());
}

#[tokio::test(start_paused = true)]
async fn phase_deadlines_use_their_own_clocks_and_cancel_pending_work() {
    for (limit, action) in [
        (HANDSHAKE_TIMEOUT, "incoming handshake/ALPN"),
        (RESPONSE_TIMEOUT, "write response stream"),
        (Duration::from_secs(60), "serve incoming MCT connection"),
    ] {
        let start = tokio::time::Instant::now();
        let result: MotherIrohEndpointResult<()> =
            deadline(limit, action, std::future::pending()).await;
        assert!(
            matches!(result, Err(MotherIrohEndpointError::ProtocolTimeout { action: observed }) if observed == action)
        );
        assert_eq!(start.elapsed(), limit);
    }
    let start = tokio::time::Instant::now();
    assert_eq!(
        deadline(HANDSHAKE_TIMEOUT, "ready control", async { Ok(42) })
            .await
            .unwrap(),
        42
    );
    assert_eq!(start.elapsed(), Duration::ZERO);
}
