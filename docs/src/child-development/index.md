# Child development

A Child is identified by a WIT contract rather than its execution substrate.
A package-backed WIT Child contains:

```text
child.toml
child.toml.sha256
<package-relative-component>.wasm
<package-relative-component>.wasm.sha256
```

The manifest points to the package-relative component path. Flattening the
component into another directory breaks the identity that strict verification
checks.

## Authority boundary

A Child can declare what it needs, but declaration is not authority. Mother
must separately approve the exact artifact, assign it to a scope, and issue
any ToyGrants needed for MCT-mediated effects. Child code should handle typed
denial as a normal outcome.

The WASM/WIT runtime is the local Child confinement boundary. Handle-only
manifests and native/JVM artifacts are rejected for current local Child
loading. Integrate native or JVM software through authenticated ingress,
another Mother, or a separately reviewed trusted Mother adapter.

## Canonical application profiles

Start with one bounded application operation:

- **Pure compute:** a versioned domain interface with typed input/output and no
  Toy imports.
- **Read/compute:** the domain interface plus exact WASI filesystem types and
  preopens, backed by a scoped read-only `/project` grant.
- **Stateful/event:** opt in only when the application requires the specific
  bounded state or event capability. Do not begin with an all-Toys world or
  daemon-style lifecycle surface.

`mct-repo-report` is the reference read/compute Child. It reads one immutable,
content-addressed `Cargo.lock` snapshot and returns a bounded dependency
inventory. Its current string/JSON v1 contract remains until a separately
versioned typed WIT successor is designed.

## Test-only fixtures

The following components remain regression evidence, not application or
authoring templates:

- `h1-git-child` — adversarial Git confinement fixture;
- `slate-manager` — immutable compatibility fixture;
- `folder-watch-actor` and `watch-null-sink` — Watch compatibility and
  least-authority fixtures;
- `mct-test-echo` — installation/runtime smoke fixture; and
- inline WAT components — narrow ABI, routing, limit, and adapter fixtures.

Do not delete these fixtures. Do not copy their broad historical manifests,
legacy lifecycle hooks, malicious modes, null behavior, or synthetic arithmetic
into new Child guidance.

## API documentation

Rust crate and public-item documentation is built separately under `/api/`.
Product concepts and workflows stay in this book; function-level contracts
stay beside the Rust implementation.
