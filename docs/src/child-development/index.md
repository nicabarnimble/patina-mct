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

## API documentation

Rust crate and public-item documentation is built separately under `/api/`.
Product concepts and workflows stay in this book; function-level contracts
stay beside the Rust implementation.
