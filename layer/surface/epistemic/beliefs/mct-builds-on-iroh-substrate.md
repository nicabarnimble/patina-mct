---
type: belief
id: mct-builds-on-iroh-substrate
persona: architect
facets: [mct, iroh, wasm, wasi, wit, p2p, architecture]
entrenchment: medium
status: active
endorsed: true
extracted: 2026-05-31
revised: 2026-05-31
---

# mct-builds-on-iroh-substrate

MCT should use Iroh as its peer-to-peer networking substrate while focusing MCT itself on the WASM/WASI/WIT application runtime, authority model, observation ledger, and thought-mesh semantics; Iroh is open-source, self-hostable, and forkable enough to preserve optionality if the substrate needs to diverge.

## Statement

MCT should use Iroh as its peer-to-peer networking substrate while focusing MCT itself on the WASM/WASI/WIT application runtime, authority model, observation ledger, and thought-mesh semantics; Iroh is open-source, self-hostable, and forkable enough to preserve optionality if the substrate needs to diverge.

## Evidence

- [[session-20260529-070316-510393000]] records the design reframing that MCT runs WASM/WASI/WIT applications over Iroh rather than reinventing networking. Iroh OSS evidence includes self-hostable relay, DNS/Pkarr discovery, custom relay maps, access controls, metrics, and diagnostics in the n0-computer repositories and docs.
- [[session-20260529-070316-510393000]] also includes a user-provided DevTools FM Iroh interview transcript that frames Iroh as reliable public-key dialing plus ALPN protocol composition, with authorization and synchronization intentionally left to higher layers.

## Supports

- [[iroh-noq-evidence-before-rules]] by turning the evidence pass into a bounded architectural dependency decision rather than new transport invention.
- [[iroh-provides-connectivity-not-authority]] by grounding the substrate/application boundary in both OSS evidence and n0's stated direction.

## Attacks

<!-- Add beliefs this defeats -->

## Attacked-By

<!-- Add beliefs that challenge this -->

## Applied-In

- [[session-20260529-070316-510393000]] applies this by framing Mother as the Iroh-owning host and WASM/WASI/WIT children as authority-scoped applications that use Iroh through WIT/Toy capabilities.

## Revision Log

- 2026-05-31: Created — metrics computed by `patina scrape`
