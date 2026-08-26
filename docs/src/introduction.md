# MCT

## Running code is not authority

MCT is an application runtime built around one rule: code should not gain
power merely because it is running.

A Mother runs application components called Children. When a Child needs to
affect the outside world, it uses a Toy. A ToyGrant states which Child may
perform which action, within which scope, and for how long. Mother evaluates
that authority before MCT performs the effect.

The authority and evidence stay with the node that enforces them. Mothers can
work together over [Iroh](https://iroh.computer) without depending on a central
cloud control plane.

**Mother decides. Children compute. Toys effect.**

## Choose a path

- **Run MCT:** begin with [Getting started](getting-started/index.md).
- **Understand the model:** read [Core concepts](concepts/index.md).
- **Build a Child:** use [Child development](child-development/index.md).
- **Operate a Mother:** use [Operations](operations/index.md).
- **Contribute:** read [Architecture](architecture/index.md) and
  [Contributing](contributing/index.md).
- **Audit how the product was built:** enter [Provenance](provenance/index.md).

## Status

MCT 0.2.0 is pre-GA and currently targets `aarch64-apple-darwin`. Its
WASM/WIT path enforces capability-mediated host access and records MCT
authority decisions and mediated effects in an append-only, hash-chained
observation ledger.

Process-backed Children are authorized before launch, but they are not
currently OS-sandboxed. They inherit ordinary host-process access and must not
be treated as zero-ambient-authority workloads.

CLI and API surfaces are still evolving. Documentation must distinguish proven
behavior from planned behavior; the project does not present roadmap intent as
shipped capability.

## Where authority lives

The published book explains the product. It does not replace the sources that
govern implementation:

1. `layer/allium/mct-product-map.allium` is semantic law.
2. Rust source implements that law and rustdoc describes public APIs.
3. Authored Markdown explains supported use and operation.
4. Generated references project implementation-owned surfaces.
5. Traces, sessions, and event logs are public evidence, not authority by
   themselves.
