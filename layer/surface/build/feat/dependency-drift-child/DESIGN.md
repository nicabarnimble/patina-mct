# Design: canonical read/compute Child and dependency drift

## Architecture checkpoint

NIC-105 keeps the existing authority and execution model. The smallest change
has three data boundaries and one local orchestration constraint:

```text
Cargo.lock snapshot
  -> mct-repo-report on the snapshot's Mother
  -> mct-repo-report/v1 inventory
  -> pure client comparison
  -> mct-dependency-drift/v1
```

## Child world

Do not add a second world or a comparison export. Keep the deployed operation:

```text
patina:repo-report/report@0.1.0.inspect(string) -> result<string, string>
```

The component continues to import only WASI filesystem types and preopens. Its
manifest requests only filesystem capability at `/project`; that request is not
a grant. A separately versioned typed WIT successor remains future work.

## Inventory shape

Keep `mct-repo-report/v1` and add one backward-compatible field:

```json
{
  "schema": "mct-repo-report/v1",
  "sha256": "...",
  "package_count": 2,
  "registry_count": 2,
  "git_count": 0,
  "local_count": 0,
  "packages": {
    "example": ["1.0.0", "2.0.0"]
  },
  "duplicates": {
    "example": ["1.0.0", "2.0.0"]
  },
  "selected": {}
}
```

`packages` is a sorted map from package name to the sorted distinct versions in
the lockfile. It is sufficient for package/version drift, remains compact under
the existing bounded output envelope for the target repositories, and does not
pretend to be an advisory or full dependency graph.

## Drift shape

Comparison is a pure client function. It validates two v1 inventories and
returns:

```json
{
  "schema": "mct-dependency-drift/v1",
  "left_mother": { "node_id": "...", "sha256": "...", "package_count": 1 },
  "right_mother": { "node_id": "...", "sha256": "...", "package_count": 1 },
  "difference_count": 1,
  "differences": [
    {
      "package": "example",
      "kind": "version_mismatch",
      "left_versions": ["1.0.0"],
      "right_versions": ["2.0.0"]
    }
  ]
}
```

The other closed kinds are `only_left` and `only_right`. Results are sorted by
package name and contain no host addresses or filesystem paths.

## Exact Mother placement

The same operation is present on both Mothers, while the existing route ranker
prefers local execution. A mesh piece therefore accepts an optional
`required_node_id` local-orchestration constraint. The constraint is not added
to `MctCall`, is not forwarded as authority, and cannot mint permission.

At Mother A it filters already-evaluated route candidates:

- matching candidates retain their ordinary authority result;
- nonmatching candidates are eliminated as `RouteMismatch` and observed; and
- no matching admissible candidate yields the ordinary caller-safe no-route
  denial.

The constraint participates in A's idempotency fingerprint so retrying the same
piece with a different required Mother is rejected as key reuse mismatch rather
than replaying or duplicating work. After A selects B, B receives the ordinary
single-hop call and independently evaluates local Child and Toy authority.

## Physical package

The builder closure contains one component and manifest plus two immutable
snapshot files selected through private builder inputs. The acceptance helper
stages the same component on both Mothers and preopens only the snapshot chosen
for that Mother. Host addresses remain in the private federation config.

The daemon, helper and component are built on nicamaster. Nicanode0 imports the
signed closure with compilation and public substituters disabled.
