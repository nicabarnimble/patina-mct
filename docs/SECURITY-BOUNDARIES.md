# Current security boundaries

This page distinguishes guarantees from their limits. Baseline: `4330876` plus
the ledger-first-call-recovery slice on this branch. It clarifies existing
contracts; proposals below do not supersede decisions.

## What is protected, and when

| Boundary | Current guarantee | Limit |
| --- | --- | --- |
| Local execution | WASM components; explicit permitted host imports | Mother, trusted adapters, Wasmtime, OS, and storage remain trusted |
| Authority | Kernel evaluation before mediated admission | An allow is not evidence that an effect completed |
| Revocation | New mediated admissions check current authority | Already-admitted WASI preopens survive until bounded expiry (D-G7); external effects are not undone |
| Credentials | No ambient guest credentials; secret observations omit values | The secrets backend returns explicitly requested values; universal opaque-use-only access is not built |
| Administration | Owner-authenticated UDS mutations; admin-key facts and route evaluator | Iroh admin transport, signed issuance authorization, and hard cutover remain incomplete |
| Admin scope | Command/route-class scopes and day-to-day expiry | Project/resource scope and issuance ceilings are not specified by the current grant type |
| Hash chain | Detects invalid framing, sequence, identity, and hash linkage | No claim of detecting coherent rollback of all local evidence without an independent trusted anchor |
| Hybrid signatures | Verifies both halves when a scheme-v1 signature is present | Unsigned admin facts still replay; self-signing proves possession, not authorization; iroh transport is classical |
| Keyed retry | Durable dispatch fence survives lost SQLite call rows within the existing TTL | Lost results become unknown/interrupted, not reconstructed successes; old calls without facts and unkeyed calls do not gain this protection |
| Replay | Authority reconstruction and interrupted-call settlement | Durable task continuation and effect-result replay are not implemented |

## Trust model

MCT confines untrusted local components and evaluates requests against local
authority. It does not claim protection from a compromised kernel, replaced
Mother binary, arbitrary mutation of all trusted host state, or rollback of
every independent copy of the evidence. Eliminating an offline CLI path is
not a substitute for defining and protecting those trust roots.

A verified hash chain is meaningful relative to the identity and history the
verifier trusts. A signature proves possession of a key over specific bytes.
Permission requires an authorized issuer; freshness requires current history.
A digest of a signature cannot verify it without the original signature bytes.

## Recovery-key consequence

D-16 intentionally provides no in-epoch revocation or replacement of the
recovery grant. Lost recovery custody does not revoke still-valid day-to-day
keys, but removes the recovery path. Compromise is different: there is no
implemented way to retire the stolen recovery credential within that history.
D-54 now approves a bounded Voice-bound guardian replacement path, but it is
not implemented. Until its canonical enrollment/recovery and operational proofs
exist, the runtime still has this limitation. D-54 does not recover compromised
hosts or lost canonical history.

`key-manage` is root-like when it can issue broad grants: denying a direct
route does not eliminate the indirect authority to issue a key for that route.
This is a design consequence, not a newly implemented privilege restriction.

## Safe tools are not safe information flow

An authorized private-file read combined with an authorized model/network send
can disclose private data. Separate handles, opaque provider credentials, and
redacted observations do not prevent that composition. No general
information-flow or prompt-injection-resistance claim is made. Grant design
must consider combinations, destinations, and data classes, not just each Toy
in isolation.

## Decisions still required before admin cutover

1. D-54 settles recovery policy; implementation and custody/backup drills remain
   required evidence, not additional permission inferred from tests.
2. Issuance authority: node-wide versus resource-scoped administration, maximum
   issuable rights/expiry, and the bootstrap exception.
3. Portable Child handle: bearer versus recipient-bound proof, issuer/epoch,
   operation/resource, expiry, and cascading delegation checks.
4. Trusted supervisor lifecycle: install/start when Mother is stopped, endpoint
   failure, quarantine diagnostics, and recovery without a second grantor.
5. Signature enforcement and legacy-history policy, without mistaking valid
   signatures for authorized issuance or anti-rollback proof.

Do not turn these questions into permissive defaults while wiring transport.
