---
name: verify-governed-peer-admission
description: "Model-check governed peer admission on a passkey Mother: config publication order, and the separate binding, receiver-identity and dispatch reads of inbound hello and call, against a revoke. Use for /verify-governed-peer-admission, or after changing serve.rs hello or call evaluation, the resident inbound binding provider, governed publication, or the pipeline's dispatch check."
disable-model-invocation: true
---

# Verify governed peer admission

Two TLA+ models describe how a governed Mother admits a peer while a revoke commits. Each has the repaired configuration and broken counterparts. A broken counterpart must break, which shows that the check can still fail.

## Checks

| Model | Property | Meaning |
|---|---|---|
| `models/PeerSplitReads.tla` | `HelloNamesAdmittedIdentity` | An admitted hello names a receiver identity under which the peer was still admitted. |
| `models/PeerSplitReads.tla` | `RevokedPeerNeverDispatched` | A peer call passes its dispatch check only while the ledger admits the peer. |
| `models/PeerPublication.tla` | `RevokedPeerNeverAdmitted` | Admission never follows a canonical revoke (atomic admission; source of the binding only). |
| `models/PeerPublication.tla` | `CopyNeverGoesBack` | `config.json` is never rewritten from an older generation. |

### Linearization contract

A peer call linearizes at the resident pipeline's dispatch check (`execute_resident_call`: `fresh_resident_receiver_identity`, expected identity equal to current).

- **Revoke before that check:** every governed commit makes a new receiver identity, so the call is refused.
- **Revoke after that check:** the revoke applies to the next call.

An admitted hello reads the bindings again after its receiver-identity read, so the identity it returns is one under which the binding admitted.

### Covered code

Last checked at `c9f1060`; the repair commit is `762badb`.

- `crates/mct-iroh/src/serve.rs`:
  - the `mct/hello/0` arm: binding read, receiver read, recheck;
  - the `mct/call/0` arm: binding read, receiver read, call evaluation.
- `crates/mct-kernel/src/call/internal.rs`: `evaluate_call_protocol`, the expected receiver identity check.
- `crates/mct-daemon/src/daemon/resident/pipeline.rs`: `execute_resident_call`, the dispatch check.
- `crates/mct-daemon/src/daemon/resident/serving.rs`:
  - `read_peer_bindings_for_iroh`;
  - `project_governed_config_from` and `GOVERNED_PUBLICATION`;
  - the admin handler's publication trigger.
- `crates/mct-daemon/src/daemon/resident/observation.rs`: `governed_peers` (fence check).
- `crates/mct-observation/src/passkey_admin.rs`: `PasskeyAdminFactV1::identity`.

### Bounds and assumptions

- **Bounds:**
  - `PeerSplitReads`: 2 commits, 2 hellos and 2 calls, one of each in flight.
  - `PeerPublication`: 3 commands and 2 concurrent publications.
- **Identity:** a caller presents only identities an admitted hello returned. The identity includes the committing fact's hash, which a caller cannot predict.
- **Not modeled:**
  - observation-sink writes (a dead writer also fails the hello's record);
  - expiry, wrong Vision and signatures, which have their own regressions;
  - execution after dispatch, where Toy effects recheck the receiver generation at effect admission.

### Expected outcomes

| Config | Expected |
|---|---|
| `PeerSplitReads-repaired` | all hold |
| `PeerSplitReads-no-kernel-check` | all hold: the dispatch check subsumes the call-evaluation check |
| `PeerSplitReads-no-hello-recheck` | `HelloNamesAdmittedIdentity` breaks (the code before `762badb`) |
| `PeerSplitReads-no-pipeline-check` | `RevokedPeerNeverDispatched` breaks: revoke between evaluation and dispatch |
| `PeerSplitReads-no-generation-checks` | `RevokedPeerNeverDispatched` breaks |
| `PeerSplitReads-config-fallback` | `HelloNamesAdmittedIdentity` breaks: binding read falls back to a stale copy when the writer fails |
| `PeerPublication-repaired-TRUE` | all hold |
| `PeerPublication-repaired-FALSE` | `RevokedPeerNeverAdmitted` breaks (the code at `503d0e9`) |

## Run

You need Java and TLC 1.7.x. Point `TLA2TOOLS_JAR` at `tla2tools.jar`.

```sh
TLA2TOOLS_JAR=/path/to/tla2tools.jar \
  .cursor/skills/verify-governed-peer-admission/scripts/check.sh /tmp/peer-admission-$RUN_ID
```

The script runs each config with one worker and prints `ok` or `FAIL` per row. It exits nonzero on any mismatch.

To draw a chart, use the `statechart` skill: `scripts/statechart models/PeerSplitReads.tla --config models/PeerSplitReads-no-hello-recheck.cfg --out <new dir>`.

## Runtime counterparts

Each broken counterpart has a regression in `crates/mct-daemon/tests/nic131_repair.rs` (feature `nic131-witness`) that holds the real boundary and fails when the guard is removed:

| Regression | Counterpart |
|---|---|
| `nic131_repair_revoke_between_hello_binding_and_receiver_reads_refuses_admission` | `no-hello-recheck` |
| `nic131_repair_revoke_between_call_evaluation_and_dispatch_refuses_execution` | `no-pipeline-check` |
| `nic131_repair_revoke_between_call_binding_and_receiver_reads_refuses_dispatch` | `no-generation-checks`, when both guards are removed |
| `nic131_repair_fenced_writer_refuses_fresh_hello_and_call` and `nic131_repair_stopped_writer_refuses_fresh_hello_and_call` | `config-fallback`, by asserting the failed governed binding load |
| `nic131_repair_stale_publication_cannot_finish_after_a_newer_one` | `PeerPublication-repaired-FALSE` (`CopyNeverGoesBack`) |

## Maintenance

When covered code changes, follow `maintain-formal-verification-skill`:

1. Update the source map and the model from the code.
2. Rerun.
3. Break the change and confirm a counterpart fails.
4. Record the new "last checked" commit here.
