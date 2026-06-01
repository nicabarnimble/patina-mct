---
type: belief
id: iroh-provides-connectivity-not-authority
persona: architect
facets: [mct, iroh, protocol-layer, authority, local-first, architecture]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-05-31
revised: 2026-05-31
---

# iroh-provides-connectivity-not-authority

Iroh intentionally focuses on reliable public-key-addressed connectivity and ALPN protocol composition, leaving authorization, sync, application semantics, and framework experience to layers above; MCT should occupy that protocol/runtime/authority layer rather than embedding MCT authority into transport.

## Statement

Iroh intentionally focuses on reliable public-key-addressed connectivity and ALPN protocol composition, leaving authorization, sync, application semantics, and framework experience to layers above; MCT should occupy that protocol/runtime/authority layer rather than embedding MCT authority into transport.

## Evidence

- [[session-20260529-070316-510393000]] includes a user-provided DevTools FM transcript with Brendan O'Brien/B5 describing Iroh as reliable public-key dialing, not a boil-the-ocean stack. The transcript says Iroh gives the primitive where a public key becomes dialable, while authorization, synchronization, and app semantics are layers above.
- `layer/surface/iroh-substrate-evidence.md` summarizes the transcript as external direction evidence and relates it to MCT's Mother/Child/Toy boundary.
- `layer/allium/mct-product-map.allium` applies this boundary by keeping Iroh implementation types outside the kernel authority API and defining `MctPeerBinding`, `MctPeerAdmissionDecision`, and `MctIrohPeerBindingAuthority` above transport identity.

## Supports

- [[mct-builds-on-iroh-substrate]] by clarifying that Iroh is the connectivity substrate while MCT owns runtime, authority, observations, and thought-mesh semantics.
- [[iroh-endpointid-is-transport-identity]] by explaining why Iroh endpoint keys are authorization primitives rather than full MCT authority.
- [[iroh-noq-evidence-before-rules]] by reinforcing that MCT rules should be placed above Iroh/noq path and connection mechanics.

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- [[session-20260529-070316-510393000]] applies this in the Iroh substrate reevaluation and EndpointId/MctPeerBinding Allium updates.
- `layer/slate/work/mct-iroh-substrate/work.toml` tracks MCT-over-Iroh ALPN work above Mother-owned Iroh endpoint lifecycle.
- `layer/slate/work/mct-node-identity-security/work.toml` tracks peer admission authority rather than delegating authority to transport identity.

## Revision Log

- 2026-05-31: Created from user-provided DevTools FM Iroh interview transcript; metrics computed by `patina scrape`
