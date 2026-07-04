# Resident Mother daemon

## Contract

`mct-daemon serve` becomes the one resident Mother process for a local node. It owns:

- the Mother Iroh endpoint (`mct/hello/0` + `mct/call/0`);
- the local control plane (HTTP or UDS, one transport per process);
- the observation ledger writer;
- the runtime SQLite state store access pattern;
- configured child loading and local call execution.

Incoming peer calls dispatch directly to the locally configured approved child, matching today's `iroh serve-process` semantics but resident and concurrent. This phase does not add routing, payload transfer, or binding-signature verification.

## Config surface

Inputs are explicit CLI/config facts, not ambient state:

- `--identity <path>` loads/creates the Mother Iroh key with the existing 0600 helper and must match the persisted local identity.
- Peer bindings come from the persisted config store's peer address book (`MctDaemonConfig.peers`), never positional CLI binding args.
- Bindings are refreshed per accepted connection by loading the config in a blocking task before kernel hello/call evaluation. This keeps authority current without adding a watcher.
- `--children-dir <path>` loads approved child packages for local execution.
- `--state <path>` selects the SQLite runtime store.
- `--ledger <path>` selects the JSONL observation ledger.
- `--http <addr>` or `--uds <path>` selects the control transport; specifying both is invalid.
- `--max-connections <n>` bounds concurrent peer connections; excess accepts fail closed.

## Concurrency model

- Iroh accepting and connection handling are separated. The accept loop owns admission capacity and spawns one task per accepted connection.
- Hello/call authority state is keyed by peer endpoint id. A peer's accepted hello evaluation cannot be evicted by another peer, and call evaluation still rechecks the presented endpoint and hello decision.
- Each connection task supplies its own adapter `now` to kernel evaluators.
- Observations are sent to a single ledger-writer task over an mpsc channel. Authority-critical observations await writer ack before the protected effect proceeds; fsync never blocks the async executor.
- SQLite access follows the existing `spawn_blocking` pattern. Control snapshots and per-call state mutations open or lock the store inside blocking work; no `rusqlite` handle is shared across async tasks.

## Shutdown

SIGINT/SIGTERM triggers graceful shutdown:

1. stop accepting new peer connections;
2. stop accepting new control-plane requests;
3. drain in-flight connection tasks up to a bounded deadline;
4. close the Iroh endpoint;
5. append a final daemon shutdown observation through the ledger writer;
6. flush/close the writer and remove the UDS socket file if one was bound.

If draining exceeds the deadline, remaining work is cancelled with caller-safe failure where possible and internal observations record the shutdown path.

## Non-goals

- No payload data plane or blob transfer.
- No route engine consumption of `AuthorizedRouteExecution`; local dispatch remains the single-candidate case.
- No cryptographic verification of `signature_ref`.
- No new toy categories, storage backends, relay fleet management, or UI/inspector surface.
