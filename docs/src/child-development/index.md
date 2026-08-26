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

The WASM/WIT runtime is the confinement boundary. A process-backed Child is
authorized before launch but currently runs as an ordinary host process. Do
not use the process harness for untrusted code unless an external OS sandbox
provides the missing filesystem, network, environment, and process isolation.

## API documentation

Rust crate and public-item documentation is built separately under `/api/`.
Product concepts and workflows stay in this book; function-level contracts
stay beside the Rust implementation.
