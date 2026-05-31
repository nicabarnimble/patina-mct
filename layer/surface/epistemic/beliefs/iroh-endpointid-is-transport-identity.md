---
type: belief
id: iroh-endpointid-is-transport-identity
persona: architect
facets: [mct, iroh, identity, authority, peer-admission, observations]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-05-31
revised: 2026-05-31
---

# iroh-endpointid-is-transport-identity

Iroh EndpointId is transport identity only; MCT authority requires an explicit observed binding from EndpointId to MCT Node, Vision scope, peer admission, allowed protocols, policy revision, and time bounds.

## Statement

Iroh EndpointId is transport identity only; MCT authority requires an explicit observed binding from EndpointId to MCT Node, Vision scope, peer admission, allowed protocols, policy revision, and time bounds.

## Evidence

- [[session-20260529-070316-510393000]] records the discussion that Iroh can prove a peer controls an endpoint key, but cannot decide whether that key is an authorized MCT Mother, Vision member, child caller, toy user, or observation reader.
- `layer/surface/iroh-substrate-evidence.md` documents Iroh endpoint hooks, relay access control, RCAN-style capability patterns, and the boundary between Iroh transport identity and MCT application authority.
- `layer/allium/mct-product-map.allium` now defines `MctPeerBinding`, `MctPeerAdmissionDecision`, and `MctIrohPeerBindingAuthority` as the MCT authority layer above Iroh EndpointId.

## Supports

- [[mct-builds-on-iroh-substrate]] by clarifying how MCT uses Iroh without delegating MCT authority to Iroh transport identity.
- [[iroh-noq-evidence-before-rules]] by recording the specific rule boundary learned from the Iroh/noq evidence pass.

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- [[session-20260529-070316-510393000]] applies this in the Allium and Slate updates for peer binding and admission.
- `layer/slate/work/mct-node-identity-security/work.toml` tracks implementation of explicit peer binding and admission policy.
- `layer/slate/work/mct-iroh-substrate/work.toml` tracks implementation of Mother-owned Iroh endpoint lifecycle and MCT-over-Iroh ALPN admission.

## Revision Log

- 2026-05-31: Created after user agreement; metrics computed by `patina scrape`
