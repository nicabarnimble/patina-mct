# Iroh Substrate Evidence for MCT

Date: 2026-05-31

## Purpose

This artifact gathers the Iroh/n0 evidence needed to reevaluate MCT's current design. It supports [[mct-builds-on-iroh-substrate]] and [[iroh-noq-evidence-before-rules]].

Question under evaluation:

> Does OSS Iroh include enough self-hostable DNS/discovery/relay/metrics pieces for MCT to assemble a federated edge without depending on `services.iroh.computer`, and should MCT be a WASM/WASI/WIT application system over Iroh?

## Repositories Added for Research

Added through `patina repo add --no-oxidize` to avoid long embedding builds:

- `n0-computer/noq`
- `n0-computer/iroh-address-lookups`
- `n0-computer/iroh-services`
- `n0-computer/iroh-metrics`
- `n0-computer/iroh-doctor`
- `n0-computer/iroh-tor-transport`
- `n0-computer/iroh-nym-transport`
- `n0-computer/iroh-proxy-utils`
- `n0-computer/iroh-gossip`
- `n0-computer/iroh-docs`
- `n0-computer/iroh-blobs`
- `n0-computer/irpc`
- `n0-computer/iroh-tickets`
- `n0-computer/rcan`
- `n0-computer/n0-qlog`
- `n0-computer/docs.iroh.computer`

Existing registered repository:

- `n0-computer/iroh`

## Evidence Summary

### Core Iroh stack

Evidence:

- `/Users/nicabar/.patina/cache/repos/n0-computer/docs.iroh.computer/what-is-iroh.mdx`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/TRANSPORTS.md`

Findings:

- Iroh is a modular networking stack, not a full application framework.
- Iroh's stack is explicitly layered as application protocols over Router/ALPN over Endpoint/discovery/NAT/relay over QUIC/TLS over transport.
- Iroh supports composable protocols such as blobs, docs, gossip, and user-defined protocols.
- Iroh endpoints use stable `EndpointId` public-key identity and discover address information through DNS/Pkarr/DHT/mDNS mechanisms.

MCT implication:

- MCT should define its own application semantics as Iroh protocols, not as a replacement transport.
- MCT's operation target can remain WIT-shaped while Iroh supplies peer connectivity.

### noq as QUIC substrate

Evidence:

- `/Users/nicabar/.patina/cache/repos/n0-computer/noq/README.md`
- Iroh blog: `https://www.iroh.computer/blog/noq-announcement`

Findings:

- noq is a pure Rust QUIC implementation forked from Quinn.
- Its explicit focus includes QUIC Multipath, QUIC Address Discovery, and QUIC NAT traversal.
- Iroh uses noq for first-class multipath/path-aware connection behavior.
- qlog support is extended for multipath and other Iroh-relevant QUIC extensions.

MCT implication:

- MCT should not implement path scheduling, NAT traversal, or relay/direct path mechanics.
- MCT should observe and govern allowed path classes, while Iroh/noq chooses and maintains actual network paths.

### Self-hostable relay

Evidence:

- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/iroh-relay/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/iroh-relay/src/server.rs`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/iroh-relay/src/main.rs`
- `/Users/nicabar/.patina/cache/repos/n0-computer/docs.iroh.computer/concepts/relays.mdx`
- `/Users/nicabar/.patina/cache/repos/n0-computer/docs.iroh.computer/add-a-relay.mdx`
- `/Users/nicabar/.patina/cache/repos/n0-computer/docs.iroh.computer/deployment/dedicated-infrastructure.mdx`

Findings:

- `iroh-relay` is a complete relay server implementation over HTTP/HTTPS.
- It serves `/relay`, `/ping`, and `/generate_204`.
- It can expose QUIC Address Discovery.
- It supports automatic TLS/ACME and manual TLS modes.
- It supports metrics.
- It supports access control: everyone, allowlist, denylist, and HTTP access checks.
- Relay clients can be configured through `RelayMap` / `RelayMode::Custom`.
- n0 docs explicitly support self-hosting relays and recommend dedicated relays for production.
- Relays are stateless connection facilitators; application state lives in clients.

MCT implication:

- MCT can operate its own relay fleet or federated relay set without depending on Iroh Services.
- MCT can attach peer admission to relay access checks and to endpoint hooks.
- Relay state should not become MCT application truth; MCT truth remains in local ledgers and app/fact replication.

### Self-hostable DNS/Pkarr discovery

Evidence:

- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/iroh-dns/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/iroh-dns-server/src/config.rs`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/iroh-dns-server/config.dev.toml`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/iroh-dns-server/config.prod.toml`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/iroh/src/address_lookup/pkarr.rs`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/iroh/src/address_lookup/dns.rs`
- `/Users/nicabar/.patina/cache/repos/n0-computer/docs.iroh.computer/concepts/discovery.mdx`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-address-lookups/README.md`

Findings:

- `iroh-dns-server` is a Pkarr relay and DNS server.
- It supports HTTP/HTTPS Pkarr publication and lookup at `/pkarr/{key}`.
- It supports DNS-over-HTTPS at `/dns-query`.
- It stores signed packets locally and can optionally use BitTorrent mainline DHT fallback.
- Iroh address lookup supports DNS discovery, Pkarr HTTP relay, Mainline DHT via `iroh-address-lookups`, and mDNS/local discovery.
- Pkarr records are signed by the endpoint key, so discovery data is authenticated at the endpoint level.
- Default publication filters avoid leaking IP addresses to public Pkarr by publishing relay addresses by default.

MCT implication:

- MCT can self-host discovery and can define a federation-specific DNS origin/Pkarr relay.
- MCT can map `MctNodeId`/Vision membership to Iroh `EndpointId` discovery records without trusting n0-hosted discovery.
- MCT should treat Iroh `EndpointId` as transport identity; MCT authority identity needs a separate binding and observation.

### Metrics, diagnostics, and qlog

Evidence:

- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-metrics/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/iroh/src/endpoint.rs`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/iroh-relay/src/server/metrics.rs`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/iroh-dns-server/src/metrics.rs`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-doctor/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/n0-qlog/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-services/src/net_diagnostics.rs`

Findings:

- `iroh-metrics` exposes counters and OpenMetrics/Prometheus-compatible data.
- Iroh endpoints expose metrics that can be gathered locally.
- Relay and DNS servers expose metrics.
- `iroh-doctor` can report network environment, accept/connect diagnostics, port mapping, relay latency, and metric plots.
- `n0-qlog` supports qlog data with Iroh/noq extensions for multipath, ACK frequency, and address discovery.
- Iroh Services diagnostics are built on top of local endpoint reports and capability grants, not magical SaaS-only state.

MCT implication:

- `MctObservation` should remain authoritative system truth.
- Iroh metrics, qlog, relay metrics, DNS metrics, and doctor reports are adapter evidence or projections to ingest/correlate.
- Direct data rate, path type, relay fallback, QAD, and NAT reports are good `NodeTelemetry` inputs but not authority inputs.

### Iroh Services vs OSS

Evidence:

- `/Users/nicabar/.patina/cache/repos/n0-computer/docs.iroh.computer/iroh-services/index.mdx`
- `/Users/nicabar/.patina/cache/repos/n0-computer/docs.iroh.computer/iroh-services/access.mdx`
- `/Users/nicabar/.patina/cache/repos/n0-computer/docs.iroh.computer/iroh-services/relays/public.mdx`
- `/Users/nicabar/.patina/cache/repos/n0-computer/docs.iroh.computer/iroh-services/relays/managed.mdx`
- `/Users/nicabar/.patina/cache/repos/n0-computer/docs.iroh.computer/iroh-services/metrics/index.mdx`
- `/Users/nicabar/.patina/cache/repos/n0-computer/docs.iroh.computer/iroh-services/net-diagnostics/usage.mdx`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-services/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-services/src/client.rs`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-services/src/caps.rs`

Findings:

- Iroh Services is a web platform for managing and monitoring Iroh infrastructure and networks.
- It offers managed relay hosting, dashboards, metrics, diagnostics, billing, support, and SLAs.
- API keys authorize pushing metrics and diagnostics to a project, not basic Iroh connectivity.
- The `iroh-services` client is itself an Iroh protocol.
- Iroh Services uses RCAN-style capability tokens for operations such as metrics, relay use, and diagnostics.
- n0 FAQ says revenue comes through Iroh Services while Iroh remains open source, including relay and DNS discovery server-side code.

MCT implication:

- MCT can remain independent of `services.iroh.computer`.
- MCT may optionally integrate with Iroh Services as an adapter/projection later, but must not require it for core federation.
- Iroh Services is evidence for useful product surfaces: direct data rate, relay metrics, net diagnostics, project-level control plane, and support workflows.

### Application protocols: gossip, docs, blobs, IRPC, tickets

Evidence:

- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-gossip/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-docs/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-blobs/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/irpc/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-tickets/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/docs.iroh.computer/concepts/protocols.mdx`

Findings:

- Iroh protocols are routed by ALPN through an endpoint `Router`.
- `iroh-gossip` implements epidemic broadcast trees over Iroh connections for topic-based message dissemination.
- `iroh-docs` implements signed, multi-author, namespace-scoped key-value replicas with range-based set reconciliation, and depends on blobs + gossip.
- `iroh-blobs` provides BLAKE3-addressed, verified blob transfer over QUIC streams.
- `irpc` provides streaming RPC over Iroh/noq and includes trace-context propagation.
- `iroh-tickets` serializes endpoint reachability information for bootstrap/signaling.

MCT implication:

- MCT should define its own protocol suite over Iroh ALPNs.
- MCT can reuse or adapt existing protocols: gossip for swarm notification, blobs for bulk immutable payloads, tickets for bootstrap, and IRPC for internal RPC patterns.
- `iroh-docs` is close to a replicated data substrate, but it has its own namespace/author/write-capability model. Treat it as evidence or optional adapter until MCT's own authority/ledger semantics are clear.
- MCT immutable thoughts and observations should not be silently delegated to `iroh-docs` without an explicit mapping from MCT Vision/Node/ToyGrant authority to docs namespace/author capabilities.

### Capability delegation and admission

Evidence:

- `/Users/nicabar/.patina/cache/repos/n0-computer/rcan/src/lib.rs`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-services/src/caps.rs`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-services/src/client_host.rs`
- `/Users/nicabar/.patina/cache/repos/n0-computer/docs.iroh.computer/connecting/endpoint-hooks.mdx`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/iroh-relay/src/main.rs`

Findings:

- RCAN defines attenuated capability delegation with a `Capability::permits` relation and signed delegation chains.
- Iroh Services uses capabilities for relay, metrics, and net diagnostics.
- Endpoint hooks can observe/reject before outgoing connections and after handshake before app data.
- Relay access control can admit/deny endpoint IDs and can delegate access checks to HTTP.

MCT implication:

- MCT's ToyGrant model is consistent with object-capability style attenuation.
- MCT should reuse the design pattern, but not assume Iroh Services caps are enough for MCT authority.
- Peer admission must happen at both endpoint protocol level and optionally relay access level.

### Privacy transports and privacy limits

Evidence:

- `/Users/nicabar/.patina/cache/repos/n0-computer/docs.iroh.computer/deployment/security-privacy.mdx`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-tor-transport/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh-nym-transport/README.md`
- `/Users/nicabar/.patina/cache/repos/n0-computer/iroh/deny.toml`

Findings:

- Iroh traffic is end-to-end encrypted, but relays can see metadata: IPs, connection times, and transfer sizes.
- Public relays are discouraged for sensitive/confidential production data.
- Dedicated infrastructure is the basic recommended protection.
- Tor and Nym transports exist but are experimental and require custom transport support.
- Nym provides stronger metadata privacy at high latency/low throughput.
- Iroh has upcoming relay-only mode but single-hop relay-only still requires trusting relay operators.
- Multi-hop relay routing is explored but not near-term and is not onion routing.
- The Iroh repo denies `openssl` and `native-tls`, so OpenSSL/BoringSSL ECH is unlikely to become a natural core dependency in Iroh as-is.

MCT implication:

- MCT privacy policy must model metadata exposure separately from payload encryption.
- MCT's privacy federation opportunity remains real, but v0 should start with self-hosted/dedicated relays, explicit privacy tiers, and observation.
- ECH/OHTTP should remain a researched privacy-edge lane, not a foundational assumption for the first MCT-over-Iroh protocol.

## External Direction Evidence: DevTools FM Iroh Interview

Source: user-provided transcript from a DevTools FM interview with Brendan O'Brien/B5 of n0 about Iroh. The source URL was not captured in this repository, but the transcript is preserved in [[session-20260529-070316-510393000]].

Relevant direction signals:

- Iroh's thesis is "ship the whole app": put client and server-shaped capabilities in the same app/device boundary instead of shipping only a frontend that depends on a hidden backend.
- Iroh intentionally avoids a "boil the ocean" stack. The core primitive is reliable, low-configuration, public-key-addressed connectivity: given a public key, dial the device.
- Iroh expects authorization, synchronization, app semantics, and higher-level protocol behaviour to live above the connection layer.
- Iroh's ALPN/protocol model is the intended composition seam. Apps create an endpoint, add protocols, and route protocol behaviour above QUIC.
- Iroh keys are described as primitives for building authorization schemes, not as the entire authorization scheme.
- Relays are pragmatic federation infrastructure: open-source, URL-addressed, self-hostable, and often run by production users after they graduate from public relays.
- The relay path is both a fast initial/reliable path and a fallback path; Iroh dynamically shifts between relay and direct paths as connectivity changes.
- Multipath is a strategic direction: relay, direct IPv4/IPv6, WebRTC, Wi-Fi-aware, or future paths can become paths under one logical connection.
- Iroh's business/services layer is framed around managed relays, metrics, diagnostics, deployment support, and protocol-developer tooling, not closed protocol authority.
- The intended ecosystem includes protocol/framework layers above Iroh so most application developers do not need to consume raw Rust networking primitives.

MCT implication:

- [[iroh-provides-connectivity-not-authority]]: MCT should occupy the protocol/runtime/authority layer above Iroh rather than embedding MCT authority into transport.
- The Mother/Child/Toy model fits the "ship the whole app" thesis: a Mother is a server-capable local authority/runtime, children are app components, toys are host powers, and Iroh connects whole-app nodes.
- MCT should define ALPN protocols such as `mct/hello/0`, `mct/call/0`, `mct/thought/0`, and `mct/observe/0` rather than treating Iroh as opaque HTTP replacement.
- The video reinforces [[iroh-endpointid-is-transport-identity]]: endpoint keys can start authentication/authorization design, but MCT still needs explicit peer bindings, Vision scope, grants, and observations.
- Content addressing via Iroh blobs is useful for immutable artifacts and bulk payloads, but live calls and streams remain distinct protocol flows.
- Iroh should own path continuity and multipath adaptation; MCT should own privacy tier policy, admission, and observation of path facts.

## Reevaluation of Current MCT Design

### Current design that holds

The current Allium direction is broadly validated:

- MCT remains a WASM/WASI/WIT-centered application/runtime system.
- Mother is the host/authority/control-plane boundary.
- Children are WASM/WIT components or WIT-shaped adapters.
- Toys are explicit host capabilities governed by ToyGrants.
- Iroh is a runtime/transport adapter, not the kernel authority API.
- MCT observations are authoritative; Iroh metrics/qlog are evidence/projections.
- Routing remains two-phase: authority filter before environment planner.

### Design that should become sharper

1. Mother owns Iroh endpoint lifecycle.
   - Children should not receive raw Iroh endpoints or socket-like power by default.
   - Children request mesh actions through WIT/Toy capabilities.

2. Iroh `EndpointId` is transport identity, not full MCT authority identity.
   - MCT needs explicit bindings from EndpointId to `MctNodeId`, Vision membership, peer admission, and policy revision.
   - These bindings must be observed.

3. MCT should define an ALPN protocol suite.
   Candidate early ALPNs:
   - `mct/hello/0` or `mct/federation/0` for peer admission, version negotiation, and capability view exchange.
   - `mct/call/0` for WIT-shaped `MctCall` request/reply.
   - `mct/observe/0` for selected observation replication.
   - `mct/thought/0` for immutable thought/fact exchange.
   - `mct/blob/0` may be unnecessary if `iroh-blobs` is used directly.

4. Thought mesh should be an MCT semantic layer over Iroh.
   - Iroh connects endpoints.
   - MCT connects signed, scoped, immutable thoughts/facts/observations.
   - Gossip can announce heads or topics, but the MCT ledger/fact model should define acceptance and authority.

5. Federation is not just relay configuration.
   - Federation membership must include operators, nodes, EndpointIds, relay URLs, DNS/Pkarr origins, privacy tier policy, admission rules, and observation obligations.
   - The federation manifest can be signed and replicated as MCT facts.

6. Existing Iroh protocols are adapters/options, not automatic truth.
   - `iroh-docs` may be useful for sync but has separate namespace/author capability semantics.
   - `iroh-gossip` may be useful for discovery/announcement but should not be the source of authority.
   - `iroh-blobs` is a strong candidate for large immutable payload transfer.
   - `irpc` may inform request/reply ergonomics but WIT remains the MCT operation identity.

### Design risk reduced

Before this pass, MCT risked reinventing transport, discovery, relay hosting, network diagnostics, and protocol routing.

After this pass, MCT can avoid that by depending on Iroh OSS for:

- Endpoint connectivity
- Direct/relay path management
- NAT traversal
- Relay and QAD
- DNS/Pkarr/DHT/mDNS discovery
- Metrics, doctor-style diagnostics, and qlog evidence
- Router/ALPN protocol dispatch
- Optional privacy transports

### Remaining open design questions

1. Should MCT implement its own thought replication protocol first, or prototype using `iroh-docs` as a temporary substrate?
2. Should MCT v0 require self-hosted `iroh-dns-server`, or allow tickets/manual peer config first and add DNS/Pkarr federation later?
3. Should relay admission be controlled by MCT HTTP access hooks from day one, or begin with endpoint hooks and add relay admission later?
4. Initial `MctPeerBinding` shape is now captured in `layer/allium/mct-product-map.allium`: EndpointId, MCT node, Vision scope, allowed ALPNs, issuer, policy revision, binding state, and time bounds. Remaining question: what exact signature/token envelope should carry it on the wire?
5. What privacy tiers are v0-safe: direct, dedicated relay, relay-only when available, Tor/Nym experimental, ECH-researched?
6. How should qlog and Iroh metrics be correlated into `MctObservation` without making them authoritative?

## Locked Follow-Up: EndpointId Is Transport Identity

After the evidence pass, MCT locked in a separate authority layer above Iroh transport identity:

- Iroh `EndpointId` proves endpoint-key possession for transport.
- Endpoint hooks and relay access control are useful early reject/observe gates.
- Discovery records, relay reachability, tickets, and successful handshakes are reachability facts, not MCT authority.
- MCT authority requires an explicit `MctPeerBinding` plus a `MctPeerAdmissionDecision` recorded as `MctObservation` facts.
- WASM/WASI/WIT children do not receive raw Iroh endpoints by default; Mother exposes scoped WIT/Toy capabilities.

Allium anchors: `MctPeerBinding`, `MctPeerAdmissionDecision`, `MctIrohPeerBindingAuthority`.

## Locked Follow-Up: `mct/hello/0` Admission Gate

MCT now defines `mct/hello/0` as the first MCT application protocol on a new Iroh peer path.

Purpose:

- consume authenticated Iroh connection facts without treating them as full authority;
- verify the presented EndpointId against the transport-authenticated remote;
- present and evaluate `MctPeerBinding`;
- select Vision scope;
- negotiate protocol version;
- admit a bounded ALPN set;
- return only safe denial/retry/version information to the peer;
- record hello receipt, protocol negotiation, admission/denial, and response observations before protected peer effects proceed.

Allium anchors: `MctHelloRequest`, `MctHelloAdmissionEvaluation`, `MctHelloResponse`, `MctHelloProtocol`.

Belief: [[mct-hello-precedes-protected-peer-effects]].

## Recommended Next MCT Shape

MCT should be stated as:

> A WASM/WASI/WIT application runtime and authority system that uses Iroh as its peer-to-peer substrate to connect Mothers and replicate authorized thoughts, calls, observations, and capabilities.

Layering:

```text
WASM/WASI/WIT Children and Toys
  MCT typed calls, thoughts, observations, ToyGrants
Mother authority kernel and local-first ledger
  MCT-over-Iroh ALPN protocols
Iroh Endpoint, Router, Discovery, Relay, Metrics
noq / QUIC / TLS / transports
```

Implementation recommendation:

1. Keep `MctObservation` ledger as source of truth.
2. Implement Mother-owned Iroh endpoint lifecycle as an adapter.
3. Add `mct/hello/0` for peer admission/version/capability exchange.
4. Add `mct/call/0` for WIT-shaped remote calls.
5. Add local integration tests with self-hosted relay and DNS/Pkarr only after manual/ticket bootstrap works.
6. Treat Iroh Services integration as optional telemetry/export adapter, not required infrastructure.
7. Defer ECH/OHTTP until the baseline self-hosted federated edge is operational and privacy tiers are explicit.
