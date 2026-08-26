# Integrations

MCT-managed integrations enter through explicit interfaces rather than ambient
access.

- Application components operate as Children.
- WASM/WIT host effects pass through Toys.
- Peer nodes authenticate and negotiate through Mother-owned protocols.
- Iroh supplies public-key connectivity; MCT supplies application authority.
- Logs, metrics, and audit views consume observation projections rather than
  becoming independent truth stores.

An integration guide must identify its WIT contract, authority requirements,
data scope, denial behavior, and observations before describing transport or
framework convenience. Process-backed integrations must also state which
external OS confinement boundary, if any, controls their ambient host access.
