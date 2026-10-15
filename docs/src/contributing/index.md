# Contributing

Before changing MCT, read the relevant material under `layer/`. Product
behavior is scoped by Allium and active build specifications; code alone does
not redefine the product.

Run the standard checks before landing a change:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
./scripts/ci-tier0.sh
```

Documentation changes also run:

```bash
./scripts/build-docs.sh
```

A behavior change requires a documentation impact assessment. Update authored
explanations, generated-reference inputs, rustdoc, or explicitly record why no
documentation surface changed.

See [Documentation system](documentation-system.md) for authority and build
boundaries. The public activity record is governed by the
[Open trace contract](../provenance/open-trace-contract.md).
