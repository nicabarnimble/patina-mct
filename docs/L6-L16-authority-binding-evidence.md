# L6 / L16 — invocation data and Child manifest authority

## Status and checkpoint

Local implementation and deterministic regression evidence; **not independent audit closure**.
L6/L16 are checkpointed together on `experiment/git-toy-confinement`, directly atop:

```
0cc4544 fix(control): isolate listeners and bound local control transport
68280db fix(wasm): stage verified component bytes and bound invocation resources
039a277 fix(iroh): bound ingress work and connection occupancy
```

M5/L2/L3 is the single `0cc4544` checkpoint directly atop `68280db`. No push or
`main` change is part of this work. No pristine-baseline L6/L16 exploit run is
claimed. Earlier development runs exposed fixture assumptions and are not the
final acceptance receipts.

## L6: claims do not attest a narrower authority scope

### Audited sources

- `MctCall.caller.project_id` and payload classification/secret fields are
  serialized caller claims. An authenticated Iroh hop proves neither project
  membership nor the classification of payloads. Local application ingress
  derives the UID/node/Vision and sets project to `None`, but still receives
  payload labels from the application. A UID alone is not a data attestation.
- Child approval and assignment project scopes are Mother policy. Toy project
  and classification scopes are grant constraints. Previously these could be
  satisfied by comparing them directly with caller-supplied fields.
- There is **no independent production project/classification attestation
  resolver** in the inspected ingress. The fix does not invent one, infer facts
  from a matching grant, or promote manifest declarations into authority.

### Implemented boundary

`mct-kernel/src/invocation_data.rs` separates non-serialized
`LocalInvocationData` from wire metadata. Its constructor is an explicit
**trusted Rust adapter assertion**, not a signature verifier, ledger proof gate,
content classifier, or new public attestation protocol. Its observation ID is a
source reference, not independently validated evidence. A trusted adapter must
independently establish the facts for this invocation; copying a request into
this type is forbidden by its contract. Production ingress currently passes
`None`; only controlled tests construct these facts.

- Child project-scoped assignments/approvals and Toy project/classification-
  scoped grants cannot be satisfied without independent facts. An absent field,
  a matching forged label, and a wrong label do not substitute for proof.
- Supplied facts must match call ID, project, classification and secret flag.
  A caller cannot clear a known secret restriction or relabel a known context.
- Child and Toy capabilities freeze caller identity, origin and payload
  metadata. Reusing a call ID with changed project/classification/secret metadata
  fails both compatibility effect guards and snapshot-based effect admission.
- CLI and resident Toy construction inherit data facts from the admitted Child
  and reject changed invocation metadata before minting Toy authority.
- Child callout payloads are new, unclassified outputs: the Child may have read
  additional data through a Toy. The host conservatively marks them potentially
  secret, even when the parent claimed public/non-secret input. Existing
  placement policy consequently keeps them local. Parent labels are not an
  output declassification proof.
- Explicitly unscoped grants still work without data attestation. This does not
  certify payload contents or prove that an unscoped resource contains no secrets.

### Intentional compatibility restrictions

Existing classification-scoped slate grants remain narrow. An application
claiming `"project"` without independent facts now receives a denial, including
through the authenticated local bridge. The supervised regression checks that
this starts no runtime. It does **not** relax those grants to recover old
behavior. Project-scoped Child calls similarly deny without independent facts.

Child-generated remote forwarding is conservatively unavailable until a trusted
output-classification/declassification path exists. Local watch delivery and
ordinary explicitly unscoped calls remain covered by acceptance controls.
Implementing real attestation, resource classification and output declassification
requires separate policy/provenance design; this slice is not a universal DLP or
secret-flow proof. It does not claim secret containment against an already
explicitly authorized recipient.

## L16: the approval unit includes the manifest

The component artifact ID remains `sha256:<WASM bytes>` for compatibility. It is
**not** redefined as a bundle digest. Separate manifest pins bind authority:

1. Digest-pinned Child acquisition requires both existing `expected_digest`
   (tagged BLAKE3 of the component) and `expected_manifest_digest` (tagged SHA-256
   of the source manifest). CLI stage/acquire exposes
   `--expected-manifest-digest sha256:<hex>`. Missing paired pins and malformed
   manifest pins reject before source access. The kernel independently rejects
   an incomplete pair.
2. Operator-pointed acquisition decisions bind both expected values. SQLite
   retains the manifest pin with the decision; the additive nullable-column
   migration preserves old records. A different manifest pin cannot reuse that
   exact decision. Absent optional fields remain readable and are omitted from
   serialized historical shapes.
3. Source manifest bytes are bounded and checked before parsing/publication.
   A changed contract with unchanged WASM and recomputed source sidecars fails
   the expected manifest pin. Neither a sidecar nor an expected hash establishes
   publisher identity: existing source/effect authority is still required.
4. Package discovery uses **one bounded manifest snapshot** for component-path
   selection and authority metadata. It checks relative-path confinement and
   rejects invalid UTF-8 rather than interpreting replacement characters. It
   no longer invokes an independent, reopening SDK package loader.
5. Approval records pin the installed manifest SHA-256 alongside the existing
   component identity. Approval projection requires that pin to match the loaded
   manifest. Missing historical pins and changed manifests project as Candidate,
   not Approved; there is no silent grandfathering or auto-reapproval.
6. Child capabilities carry the admitted manifest hash. Fresh effect admission
   checks current artifact metadata against it. Typed WASM execution checks a
   bounded manifest snapshot against both the capability and loaded inventory,
   then compares the cached name/version/kind/role/ingress/operations/Toy needs/
   relationships with the parsed approved bytes. Changing cached authority
   fields while keeping the checksum string does not bypass this check.
7. The existing M4/L15 path still verifies and compiles the same immutable
   component bytes. Manifest changes after inventory/admission reject; restoring
   the exact approved bytes is a positive execution control.

Source and installed manifest hashes can differ: acquisition may canonically
insert an explicit component path into a flat source manifest. The expected pin
covers the **source** bytes before that transformation; artifact evidence and
approval cover the **emitted installed** bytes. Staging is never approval.
Unpinned acquisition (both pins absent) retains existing source-trust semantics;
it makes no content-pinning claim and does not automatically grant execution.

`release.rs` handles **daemon release archives**, not SDK Child packages. Their
existing exact layout excludes `child.toml`, and archive identity/checksums cover
the permitted package. The new regression injects a Child manifest into an
otherwise valid archive and verifies layout rejection. No new authority-bearing
Child member or release-format exception was introduced.

## Durability and administration

Required authority/acquisition observations still use existing awaited
`BeforeEffect` paths. A manifest mismatch after acquisition records rejection
through the existing required observer before any package publication. No
mandatory fact is sampled, made optional, or placed behind a transport timeout.
No new offline mutation fallback, publisher trust, admin privilege, or D-57
cutover exception is introduced. Historical configuration/ledger evidence is
not rewritten to fabricate missing manifest approval.

## Deterministic regressions and controls

- `l6_child_project_claims_need_independent_facts_and_tokens_freeze_metadata`:
  forged local/peer project claims deny; independent local facts admit; omitted
  or changed project/classification/secret values cannot reuse admission.
- `l6_toy_claims_cannot_satisfy_project_or_classification_grants`:
  matching local/peer claims alone deny each narrowed grant; explicitly unscoped
  grants remain a positive control.
- `l6_toy_local_facts_and_effect_tokens_reject_metadata_substitution`:
  valid snapshot-backed authority admits; metadata substitution denies at
  evaluation and effect time.
- `l6_child_callout_cannot_clear_parent_secret_classification`:
  both public-claiming and secret parents produce restricted, unclassified Child
  outputs; payload length and parent classification claim are retained.
- `l16_operator_source_authority_binds_both_digests`: complete pair admits;
  incomplete or changed pairs deny; old serialized decisions remain readable.
- `l16_component_only_and_malformed_manifest_pins_fail_before_source_access`.
- `l16_pinned_component_cannot_acquire_changed_manifest_even_with_fresh_sidecars`:
  unchanged WASM plus changed permissions rejects and publishes no artifact;
  explicit selection of the new pair stages successfully but grants no execution.
- `l16_manifest_mutations_with_recomputed_sidecars_cannot_reuse_approval`:
  changed operation, Toy need and role cannot reuse approval; missing historical
  pins are inert; exact original bytes retain approval.
- `l16_package_manifest_is_bounded_before_parsing_or_component_selection` and
  `l16_invalid_utf8_manifest_is_not_lossily_reinterpreted`.
- `l16_manifest_replacement_after_admission_is_rejected_before_execution`:
  original bytes execute (7 → 14); changed bytes, forged cached authority, or
  changed inventory digest reject; restored bytes execute.
- `l16_daemon_release_cannot_introduce_child_manifest_authority`.

Fixture updates pin actual manifest bytes, preserve grant constraints, and add
paired source pins where acquisition previously supplied only a component pin.
Synthetic fixture manifests are staged outside committed fixture directories.

## Validation and receipts

Final validation is retained under
`/tmp/mct-l6-l16-authority-binding-evidence/` (macOS/aarch64, Rust 1.96.0).

```sh
export CARGO_BUILD_JOBS=8 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_NET_OFFLINE=true
cargo test --workspace --locked --offline l6_ -- --test-threads=1
cargo test --workspace --locked --offline l16_ -- --test-threads=1
MCT_ORIGIN_KEEP=1 MCT_FLEET_KEEP=1 MCT_RESTART_KEEP=1 \
  cargo test --workspace --locked --offline --no-fail-fast -- --test-threads=1
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all --check
git diff --check
bash scripts/ci-tier0.sh specs
```

Final results: **822 workspace tests passed, 0 failed, 2 pre-existing ignored**.
All **4 L6** and **8 L16** focused regressions passed. Clippy (`-D warnings`),
formatting, diff checks and tier-0 specs passed; specs ran 17 structural/tooling
tests. All six `verified-*.exit` receipts are `0`. The index remained empty and
HEAD remained `0cc4544` after validation.

| Log | SHA-256 |
| --- | --- |
| `verified-l6-tests.log` | `88d158deb183527c480e5e0627026a8982713db1b2c7e4680a8b499aab96de5f` |
| `verified-l16-tests.log` | `e44cf2a8822b8342c84d6864f6bb76bd1612a0cb2e7b66cb50c93fa5a9a03588` |
| `verified-workspace-tests.log` | `a576f8ec28913d6ec61295ae8f1c6c1e133e581848d292c4376b9f21929d14c4` |
| `verified-clippy.log` | `20643e0d5a3fb24d77f004b4e1b7dda541aa9fc2d9639e1b8a594a0d418ee091` |
| `verified-fmt.log` | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `verified-specs.log` | `102b041cb2b05cc96c5361f6f60e13703b3b0c8f8d492cbf66334e6155e251c7` |

Independent review, severity reassessment, cross-platform assurance, production
attestation/declassification design and formal finding closure remain open.
