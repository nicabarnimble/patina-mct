---
type: belief
id: mct-call-protocol-wraps-semantic-call
persona: architect
facets: [mct, iroh, alpn, calls, wit, authority, observations]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-05-31
revised: 2026-05-31
---

# mct-call-protocol-wraps-semantic-call

`mct/call/0` should be a peer transport protocol around the semantic `MctCall`, not a competing call model; it requires prior `mct/hello/0` admission, constructs one immutable WIT-shaped call, runs normal MCT authority, and returns only caller-safe result information.

## Statement

`mct/call/0` should be a peer transport protocol around the semantic `MctCall`, not a competing call model; it requires prior `mct/hello/0` admission, constructs one immutable WIT-shaped call, runs normal MCT authority, and returns only caller-safe result information.

## Evidence

- [[session-20260529-070316-510393000]] records the decision sequence: first lock `mct/hello/0`, then define `mct/call/0` for WIT-shaped remote calls after hello admission.
- `layer/allium/mct-product-map.allium` defines `MctCallProtocolAuthority`, `MctCallPayloadHandle`, `MctCallProtocolRequest`, `MctCallProtocolEvaluation`, `MctCallProtocolReply`, and `MctCallProtocol`.
- `layer/slate/work/mct-call-envelope/work.toml` tracks `mct/call/0` as the remote peer mapping for the stable semantic call envelope.

## Supports

- [[mct-hello-precedes-protected-peer-effects]] by making prior hello admission mandatory for remote calls.
- [[iroh-provides-connectivity-not-authority]] by keeping Iroh stream details outside the operation identity and MCT authority model.
- [[mct-builds-on-iroh-substrate]] by defining the first useful admitted Mother-to-Mother behavior over Iroh.

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- [[session-20260529-070316-510393000]] applies this in the `mct/call/0` Allium and Slate updates.
- `layer/slate/work/mct-iroh-substrate/work.toml` uses this to mark the `mct/call/0` specification item complete.
- `layer/slate/work/mct-observation-log/work.toml` uses this to require peer call receipt, malformed peer call, and peer call reply observations.

## Revision Log

- 2026-05-31: Created from the `mct/call/0` Allium pass; metrics computed by `patina scrape`
