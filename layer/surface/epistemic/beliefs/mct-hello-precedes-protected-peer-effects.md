---
type: belief
id: mct-hello-precedes-protected-peer-effects
persona: architect
facets: [mct, iroh, alpn, peer-admission, authority, observations]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-05-31
revised: 2026-05-31
---

# mct-hello-precedes-protected-peer-effects

`mct/hello/0` should be the first MCT application protocol on a new Iroh peer path; it must negotiate version, bind EndpointId to MCT authority, select Vision scope, admit ALPNs, and record observations before protected peer effects proceed.

## Statement

`mct/hello/0` should be the first MCT application protocol on a new Iroh peer path; it must negotiate version, bind EndpointId to MCT authority, select Vision scope, admit ALPNs, and record observations before protected peer effects proceed.

## Evidence

- [[session-20260529-070316-510393000]] records the design decision to define `mct/hello/0` before `mct/call/0`, `mct/thought/0`, and `mct/observe/0` so peer admission is safe and auditable.
- `layer/allium/mct-product-map.allium` defines `MctHelloRequest`, `MctHelloAdmissionEvaluation`, `MctHelloResponse`, and `MctHelloProtocol` to make hello the admission/version/capability gate.
- `layer/slate/work/mct-iroh-substrate/work.toml` now tracks completion of the initial `mct/hello/0` Allium definition and defers `mct/call/0` until after hello admission.

## Supports

- [[iroh-endpointid-is-transport-identity]] by operationalizing explicit binding/admission above Iroh EndpointId.
- [[iroh-provides-connectivity-not-authority]] by placing MCT authority in an ALPN protocol above Iroh connectivity.
- [[mct-builds-on-iroh-substrate]] by defining the first concrete MCT-over-Iroh protocol layer.

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- [[session-20260529-070316-510393000]] applies this in the `mct/hello/0` Allium and Slate updates.
- `layer/slate/work/mct-node-identity-security/work.toml` uses this to frame peer admission and local caller authentication.
- `layer/slate/work/mct-observation-log/work.toml` uses this to require peer hello receipt, protocol negotiation, and hello response observations.

## Revision Log

- 2026-05-31: Created from the `mct/hello/0` Allium pass; metrics computed by `patina scrape`
