# Current security boundaries

This page distinguishes guarantees from their limits. Baseline: `4330876` plus
this branch's ledger-first, canonical guardian recovery and bounded issuance
slices. Ratified decisions govern; unfinished runtime boundaries remain explicit.

## What is protected, and when

| Boundary | Current guarantee | Limit |
| --- | --- | --- |
| Local execution | WASM components; explicit permitted host imports | Mother, trusted adapters, Wasmtime, OS, and storage remain trusted |
| Authority | Kernel evaluation before mediated admission | An allow is not evidence that an effect completed |
| Revocation | New mediated admissions check current authority | Already-admitted WASI preopens survive until bounded expiry (D-G7); external effects are not undone |
| Credentials | No ambient guest credentials; secret observations omit values | The secrets backend returns explicitly requested values; universal opaque-use-only access is not built |
| Administration | Existing owner-authenticated UDS runtime; new canonical signed issuance path has bounded-policy tests | New issuance ledgers refuse old runtime; Iroh ingress and the signature/legacy cutover remain incomplete |
| Admin scope | D-56 explicit issuance ceilings, separate from execution scopes, with bounded recipient expiry | Node-wide scopes only; no project/resource isolation or live cutover claim |
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
live authenticated service yet for retiring the stolen credential in that history.
D-54/D-55 approve a bounded Voice-bound guardian replacement path. Canonical
recovery and protected roster transitions now have executable evidence, but the
live authenticated runtime and operational proofs remain incomplete. D-54 does not recover compromised
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
2. D-56 settles explicit issuance ceilings and node-wide scope. Global signature
   cutover and trusted virgin bootstrap are now ratified by D-57; new issuance
   policy tests do not authorize promoting legacy authority.
3. Portable Child handle: bearer versus recipient-bound proof, issuer/epoch,
   operation/resource, expiry, and cascading delegation checks.
4. Trusted supervisor lifecycle: install/start when Mother is stopped, endpoint
   failure, quarantine diagnostics, and recovery without a second grantor.
5. D-57 settles strict hybrid signatures and fresh cutover. Canonical genesis
   provenance and denial tests do not complete the trusted setup adapter or live
   cutover, and are not independent anti-rollback proof.

Do not turn these questions into permissive defaults while wiring transport.
