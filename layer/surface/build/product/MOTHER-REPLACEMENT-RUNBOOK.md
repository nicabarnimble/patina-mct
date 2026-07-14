# MCT v0 `patinaMother` replacement runbook

Purpose: run MCT as the replacement-ready Mother/Child/Toy product without pulling belief/scry/assay/session semantics into `mctMother`.

## Boundary

This is the v0 runtime replacement boundary:

- MCT owns local authority, child approval/assignment, toy grants, resident serving, peer admission, routing, payload delivery, and observations.
- Belief/scry/assay/session semantics remain the responsibility of Patina, which may operate as an `mctChild` rather than another resident coordinator.
- Start/stop/install via a system supervisor is not in the MCT binary yet; run `mct-daemon serve` directly or wrap it with launchd/systemd until the production packaging follow-up lands.

## One-node local workflow

```bash
# 1. Create/load local identity and write .mct/config.json local identity facts.
cargo run -p mct-daemon -- iroh identity .mct/identity/iroh-secret.hex --config .mct/config.json

# 2. Load and approve verified child packages from the project-local child dir.
cargo run -p mct-daemon -- children load .mct/children --strict-integrity
cargo run -p mct-daemon -- children approve <child-name> .mct/children --strict-integrity --config .mct/config.json

# 3. Authorize concrete toy grants. Paths/name inputs select resources; they do not create authority by themselves.
cargo run -p mct-daemon -- toys authorize-slate <child-name> /path/to/project --children-dir .mct/children --config .mct/config.json --state .mct/state.sqlite
cargo run -p mct-daemon -- toys authorize-secret <child-name> <secret-name> --children-dir .mct/children --config .mct/config.json --state .mct/state.sqlite

# 4. Invoke a typed WIT child through the authority-first path.
cargo run -p mct-daemon -- wasm call-wit <child-name> patina:slate/control@0.1.0.list-work '[{"project":"/project","status":null,"kind":null}]' --project-root /path/to/project --children-dir .mct/children --config .mct/config.json --state .mct/state.sqlite --ledger .mct/observations.jsonl
```

## Resident `mctMother` workflow

```bash
cargo run -p mct-daemon -- serve \
  --identity .mct/identity/iroh-secret.hex \
  --config .mct/config.json \
  --children-dir .mct/children \
  --state .mct/state.sqlite \
  --ledger .mct/observations.jsonl \
  --uds .mct/control.sock
```

The resident process owns the Iroh endpoint, authenticated local application-call ingress, current child/config projections, routing/execution state, and the single observation writer. It loads peer bindings from `.mct/config.json`, requires signed binding presentations, executes approved local process/WIT children through routing/revalidation, and removes the UDS socket on clean shutdown.

Verify the running resident through its UDS-backed projection:

```bash
cargo run -p mct-daemon -- status --uds .mct/control.sock --json
```

Submit production local application calls through owner-authenticated UDS `POST /calls`. The body is the `MctResidentCallSubmission` contract from `layer/surface/build/feat/resident-call-ingress/SPEC.md`; caller, origin, and peer authority are derived by Mother and are not accepted from JSON:

```bash
curl --unix-socket .mct/control.sock \
  -H 'content-type: application/json' \
  --data-binary @call.json \
  http://localhost/calls
```

The response is synchronous and typed. Retry a lost response with the same fingerprint and idempotency key; matching completed retries replay the durable result without another child effect.

Stop v0 with SIGINT/SIGTERM. Supervisor install/start/stop wrappers are deferred production packaging follow-up.

## Peer setup

On the receiving `mctMother`, add an admitted peer after local identity exists:

```bash
cargo run -p mct-daemon -- peers add <peer-node-id> <binding-id> <peer-endpoint-id> <vision-id> [peer-ticket.json] --config .mct/config.json
```

If local identity exists, this issues and stores a `binding_signature_ref`. Export the peer entry with:

```bash
cargo run -p mct-daemon -- peers list --json --config .mct/config.json
```

The calling `mctMother` must present that receiver-issued `binding_signature_ref` in `mct/hello/0`. Store the receiver-issued proof on the caller with `peers add ... --signature-ref <proof>`; then `iroh call-peer` sends it automatically. Raw `iroh call` accepts `--signature-ref <proof>`.

Unsigned, malformed, or tampered signatures fail closed before hello admission and receive only `not authorized`.

## Compatibility JVM bridge evidence

The one-shot stdio-friendly bridge remains compatibility and development evidence:

```bash
cargo run -p mct-daemon -- jvm call-json patina:slate/control@0.1.0.list-work '[{"project":"/project","status":null,"kind":null}]' \
  --children-dir .mct/children \
  --config .mct/config.json \
  --state .mct/state.sqlite \
  --ledger .mct/observations.jsonl
```

The adapter constructs one `MctCall` with `origin = jvm_adapter`, sends JSON bytes through the same payload-integrity and routing semantics, and returns caller-safe JSON with result/ref/route fields. It is not the normal resident boundary and must not be presented as independently owning the resident's state or ledger. The future JVM SDK targets UDS `POST /calls`.

## Inspection

Use resident projections during normal operation:

```bash
cargo run -p mct-daemon -- status --uds .mct/control.sock --json
curl --unix-socket .mct/control.sock http://localhost/runs
curl --unix-socket .mct/control.sock http://localhost/snapshot
```

Direct state commands remain useful for offline inspection and compatibility diagnostics:

```bash
cargo run -p mct-daemon -- state summary --state .mct/state.sqlite --json
cargo run -p mct-daemon -- runs list --state .mct/state.sqlite --json
cargo run -p mct-daemon -- metrics snapshot --state .mct/state.sqlite --json
```

The observation ledger is `.mct/observations.jsonl`; payload bytes and secret values must not be written there.

## Not in v0 replacement boundary

- Belief/scry/assay/session runtime internals.
- System supervisor install/uninstall in the `mct-daemon` binary.
- Multi-Vision publication and transitive/brokered routing. Bilaterally authorized single-hop cross-Mother forwarding is implemented in v0.
- Full JVM SDK/client library packaging beyond UDS `POST /calls`; `jvm call-json` remains compatibility/development evidence.
