# MCT Call Envelope Adapter Mapping

Status: v0 mapping guidance for the `mct-call-envelope` epic.

## Purpose

MCT has one semantic call shape: `MctCall`. Adapters may receive very different edge inputs, but after validation they construct the same immutable domain record and send it through the same authority, routing, execution, result, and observation pipeline.

```text
edge request
  -> adapter validation / native malformed errors
  -> MctCall construction
  -> kernel authority + routing
  -> adapter execution effect
  -> MctResult + MctObservation trail
```

## Boundary Rules

1. `MctCall` is not a wire format.
2. `MctCall.origin` is audit/telemetry only; it does not grant authority.
3. Adapter-specific transport facts stay outside `MctCall` unless the field is part of the semantic call.
4. Malformed adapter input that cannot become a valid `MctCall` returns the adapter's native validation error, not a fabricated `MctResult`.
5. Payload bytes may travel inline, by content address, or by external reference; authority consumes `PayloadMetadata` and validates it against the edge payload handle before execution.
6. `MctResult` is the terminal caller-safe outcome for a valid `MctCall`; full route reasoning and policy internals stay in observations/operator records.

## Common Target Mapping

All adapters map operation identity into the same WIT-shaped target:

| `OperationTarget` field | Meaning |
| --- | --- |
| `namespace` | WIT package/domain namespace, for example `patina` |
| `interface_name` | WIT interface or equivalent adapter-normalized interface |
| `function_name` | function/operation inside that interface |

Adapters that receive a single legacy string such as `patina/watch.control` must split it into interface/function only after validation. New adapters should prefer explicit namespace/interface/function fields.

## Adapter Mapping Matrix

| Adapter edge | External input | MCT construction | Authority notes | Result path |
| --- | --- | --- | --- | --- |
| Iroh `mct/call/0` | `MctCallProtocolRequest` over ALPN after `mct/hello/0` | Uses embedded `MctCall`; validates `MctCallProtocolAuthority`, endpoint, ALPN, policy revisions, and payload handle | Prior hello narrows peer/protocol scope but does not authorize the call; normal kernel authority still runs | `MctCallProtocolReply` carries safe outcome/result ref; observations carry full trace |
| Daemon local control API | local JSON/UDS/HTTP command request | Adapter validates local actor identity, target, payload metadata, deadline, idempotency, trace, then constructs `MctCall` with `origin = cli` or future local-control origin | Local control identity is not implicit admin authority; it must map to caller identity and policy snapshot | Caller receives `MctResult` or native malformed/control error |
| CLI | command args + config + stdin/file/blob references | CLI adapter delegates to daemon/control edge; does not bypass daemon authority | CLI convenience must not create authority not present in policy/grants | CLI renders safe `MctResult` fields and opaque audit refs |
| Process harness | child/process invocation request from an already authorized route | Harness consumes selected route + child assignment, not arbitrary caller authority; call target remains WIT-shaped | Process details are execution substrate facts, not operation identity | Harness reports execution outcome into `MctResult` and runtime observations |
| JVM bridge | JVM method/event request | Bridge maps JVM-facing operation into namespace/interface/function and payload metadata | JVM substrate does not create a separate authority model; Vision/node/child/toy policy still applies | Bridge returns safe result/error and records runtime adapter observations |
| WASM/WIT host | WIT function invocation | WIT package/interface/function maps directly to `OperationTarget`; payload metadata comes from canonical host boundary | WASM child receives only authorized host capabilities/ToyGrants; raw substrate handles are not implied | Host returns WIT-compatible safe result while MCT records `MctResult`/observations |

## `patinaMother` comparison

`patinaMother` prior art has:

```rust
pub struct ChildCallRequest {
    pub operation_id: String,
    pub args: serde_json::Value,
    pub correlation: Option<CallCorrelation>,
}
```

MCT keeps the useful part and narrows the rest:

| `patinaMother` field | MCT replacement | Change |
| --- | --- | --- |
| `operation_id: String` | `OperationTarget { namespace, interface_name, function_name }` | Avoids one string carrying all operation identity; aligns with WIT-shaped calls |
| `args: serde_json::Value` | `PayloadMetadata` + `MctCallPayloadHandle`/adapter payload transport | Avoids forcing JSON as kernel truth; payload bytes can be inline/blob/external |
| `correlation` | `TraceContext` plus opaque audit/observation refs | Makes tracing first-class and authority-neutral |
| implicit caller/child context | `CallerIdentity`, `AuthorityContextSnapshot`, later route/child decisions | Makes policy revisions and caller identity explicit |

The `patinaChild` `ChildCallRequest` is evidence that a typed call seam is useful; it does not define the `mctChild` envelope.

## Minimum Valid Adapter Translation

An adapter may construct a valid `MctCall` only when it can supply:

- stable `call_id`;
- `CallerIdentity` scoped to node/Vision/project/user when known;
- WIT-shaped `OperationTarget`;
- `PayloadMetadata` that matches the payload handle;
- `AuthorityContextSnapshot` revisions used for the decision;
- deadline;
- trace/span context;
- origin for audit only.

If any required semantic field is missing or malformed, the adapter rejects before `MctCall` construction and records/returns a safe native adapter error path.
