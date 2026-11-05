# First real application on MCT

Goal: a Studio client submits useful work to its Mother, nicamaster's Mother
executes it over Iroh under existing authority, and the client keeps the real
result under a stable job ID. This plan starts from `0cef2dbc`. Implementation
and physical acceptance cover only Studio → nicamaster in slice 1.

## Gap list

| Path | Exists | Missing for the application |
| --- | --- | --- |
| Mother-to-Mother | LAN-only Iroh, persistent endpoint keys, receiver-issued signed peer bindings, hello admission and remote capability refresh. NIC-63 exercised these on Linux. | Prepare two persistent application roots and exchange their actual tickets/proofs. Exercise Darwin → Linux, not another Linux fixture. |
| Job dispatch | Owner-authenticated local `POST /calls`, WIT routing and remote forwarding. | A small application client with a stable request/job identity and useful input. No new scheduler is needed. |
| Wasm and Toys | WIT components, exact artifact approval/assignment, ToyGrant evaluation, WASI directory-read capability. | A repository-report Child and an ordinary-call read-only adapter, without fake Watch imports. Grant provisioning must name the remote caller. Verify generated WASI imports against the runtime's admitted versions before deployment. |
| Durable result | Before-effect canonical observations and recorded replies with inline bytes. Mother call dedupe has a 12-minute TTL. | A durable application result file and an immutable input digest. Do not claim the short-lived dedupe cache is a permanent job store. |
| Authorization | Separate Mother endpoint keys, scoped signed peer proofs, exact Child identity and scoped Toy grants. | Reuse these as fleet-key authorization. No shared fleet secret, new CA, passkey ceremony, or permission bypass. |

In MCT terminology the Wasm tool is a **Child**; its host capability is a **Toy**.
Mother places the authorized Child invocation, and the executing Child uses its
local Toy. A remote command run through SSH is not application acceptance.

## Chosen workload

A bot asks nicamaster for a dependency inventory of a real repository's Cargo.lock snapshot.
A Wasm Child reads the scoped snapshot through the existing directory-read Toy and returns its hash, package/source counts, duplicate versions, and selected dependency versions.
This helps release checks, uses a real host capability, and avoids inventing native shell execution or HTTP authority for the first application.

The input is content-addressed and bounded. The request includes the expected
snapshot hash; changed bytes cannot silently reuse an old job. The report is not
a vulnerability scan. Native repository builds and website fetching are later
workloads, not substitutes for this path.

## Fleet map

Proposed placement is one Mother per deployable node: Studio is the client ingress,
nicamaster is the first executor, the other six Pis are later Linux workers, and
each enrolled Lume VM is a separate Mac worker. Only nicamaster advertises the
report Child in slice 1. Mother still selects from authorized routes; the client
does not bypass placement by executing over SSH.

| Node | Platform and useful host role | Mother status / gap |
| --- | --- | --- |
| Studio | macOS arm64. Client ingress, Wasm applications, existing macOS-confined Git Toy and future explicitly admitted Mac tools. | Fresh process check found no active `mctMother`. A separate `patinaMother` is running and is out of scope. Slice 1 creates an MCT application root without replacing it. |
| nicamaster | Linux arm64. Initial remote Wasm worker and established ARM build machine. | NIC-53 admin smoke and NIC-63 resident federation passed historically. Fresh check found no active MCT daemon. Slice 1 starts the actual application Mother. |
| nicanode0 | Linux arm64. Bounded Wasm jobs; resource-constrained worker. | Historical NIC-53 acceptance and NIC-63 federation. Current service state is not established by those runs. Enrollment into this application is outside slice 1. |
| nicanode1 | Linux arm64. Bounded Wasm jobs; resource-constrained worker. | Historical NIC-53 acceptance and NIC-63 recovery/replacement. Current service state not probed in this slice. |
| nicanode2 | Linux arm64. Additional bounded worker. | Historical NIC-53 installation evidence; application enrollment and current service state unverified. |
| nicanode3 | Linux arm64. Additional bounded worker. | Historical NIC-53 installation evidence; application enrollment and current service state unverified. |
| nicanode4 | Linux arm64. Additional bounded worker. | NIC-53 acceptance was merged in #96; application enrollment and current service state unverified. |
| nicanode5 | Linux arm64. Additional bounded worker. | NIC-53 acceptance was merged in #98; application enrollment and current service state unverified. |
| Lume macOS VMs on Studio | Separate macOS nodes for isolated Mac-only capabilities and future macOS matrix jobs. | Planned, not verified as enrolled Mothers. `lume` was not found on PATH. NIC-73 remains an enrollment gate, not a completed deployment. |

Host capability availability is narrower than OS command availability:

| Host class | Implemented Toy adapters that could be admitted | Not yet a supported application capability |
| --- | --- | --- |
| Studio | Directory read, child keyvalue, logging and measurement through existing application adapters; confined Git operations on macOS. | Ordinary directory-read calls need the adapter above. Arbitrary shell, Xcode, HTTP fetch and a general build/test runner are not implied. |
| All seven Linux Pis | Directory read, child keyvalue, logging and measurement through existing application adapters. Each node needs its own grants and resources. | Native Git confinement fails closed on Linux. Wasm support does not authorize native compilation or shell commands. |
| Enrolled Lume macOS VM | Candidate for the same Mac adapters as Studio, with independent authority and local resources. | VM installation and capability acceptance are unverified. Native Mac test execution needs a separately defined build Toy. |

The supporting Toy contracts are in `crates/mct-daemon/src/daemon/watch.rs`;
selection and preopens are in
`crates/mct-daemon/src/daemon/resident/execution.rs`; Git confinement is in
`crates/mct-daemon/src/toy.rs`. These are source-supported possibilities, not
claims of active grants on every host.

Each VM needs its own compatible Mother binary, persistent private root and
endpoint key, reachable Iroh address, receiver-issued peer proofs, Child admission
and scoped Toy grants. Slice 3 must prove execution inside the VM, durable
results, restart, and revocation. A VM's NAT address or cloned key is not enrollment.
No Tailscale, DNS, Caddy, routing, VM creation, or fleet-wide rollout belongs to
slice 1. Admin-only NIC-53 acceptance is not resident Child/Toy enrollment:
`docs/passkey-admin.md` records that the runtime cutover is not built. Reuse the
existing resident authority path without pretending to migrate passkey ledgers.

## PiD's place

The local PiD checkout was inspected at `1e93cc9`, with pi-durable 1.3.0.
The distinct [PiD-next source](https://github.com/nicabarnimble/PiD-next/tree/a76fc1c41a4aa6106f467eec156c53385207eba1)
was then inspected at `a76fc1c`. Do not confuse those repositories or their deployment states.

PiD-next's [`src/server.ts`](https://github.com/nicabarnimble/PiD-next/blob/a76fc1c41a4aa6106f467eec156c53385207eba1/src/server.ts#L114-L127)
opens SQLite-backed `Harness`, installs configured extensions, and calls
`harness.resume()`. Its [`src/pid-service.ts`](https://github.com/nicabarnimble/PiD-next/blob/a76fc1c41a4aa6106f467eec156c53385207eba1/src/pid-service.ts#L177-L220)
submits agent input with a caller-supplied `requestId`, returns a submission ID,
and reacquires status by that ID. This confirms the durable-agent foundation,
not an existing MCT job client or tested PiD-to-fleet integration.
[`src/runner-tools.ts`](https://github.com/nicabarnimble/PiD-next/blob/a76fc1c41a4aa6106f467eec156c53385207eba1/src/runner-tools.ts#L51-L85)
demonstrates `defineTool`, `replay: "safe"`, and `api.memo` for identifiers that
must survive replay. Its `runner_exec` is an
unrestricted shell runner, not an MCT placement interface; the deployment's
integration document says runner execution is disabled. Do not enable it to fake
a Mother job.

The intended shape is correct: PiD owns the durable agent conversation and acts
as an MCT client. Mother owns admission, placement, Toy authority, and execution
facts. An agent's resumed conversation does not make an external effect exactly
once, and PiD must not hold the fleet's Mother private keys.

The smallest slice-2 integration is a PiD-next extension tool, following
`createRunnerExtension`'s registration pattern in `src/server.ts`, that invokes
the slice-1 client without a shell. It preserves the job ID through `api.memo`, submits/reacquires
that same job, and returns the stored outcome. Declare replay safe only after
interrupted-tool tests prove this contract. Its execution environment needs access
to an enrolled local Mother and that client's durable storage. PiD-next running
elsewhere is not assumed to have the Studio's Unix socket; that deployment binding
must be explicit. A new HTTP gateway, adoption of arbitrary terminal sessions,
and transcript-only result storage are not required by slice 1.

## Candidate applications, ranked by value

| Rank | Application | What it needs |
| --- | --- | --- |
| 1 | PiD fans a repository's test matrix out to Linux Pis and a macOS VM. | Slices 2/3 plus a separately admitted, confined build capability and artifact handling. Neither Linux shell execution nor Xcode execution is already a Toy. |
| 2 | Repository dependency/release inventory. **First slice.** | Real immutable input, directory-read Toy, bounded Wasm report, stable job/result storage. |
| 3 | Documentation index or link-policy check across repositories. | The same bounded read/compute path; actual network link checks additionally need HTTP authority. |
| 4 | Wiki retrieval and extraction for a durable research agent. | Bounded HTTP fetch Toy with destination/data policy, then Wasm extraction. The current Toy registry does not supply a general web client. |
| 5 | Fleet configuration validation before rollout. | Supplied configuration snapshots and pure policy computation first; live collection and mutation remain separately authorized effects. |

## Delivery order and acceptance

1. **Studio → nicamaster.** One `scripts/first-real-app-smoke.sh` invocation sends a
   real job through Studio Mother to nicamaster over Iroh using the existing
   signed peer bindings. Print the real report and durable job ID and exit 0.
   Repeat without creating another logical job. Verify remote execution, Toy
   admission, stored-result integrity, and client restart recovery. No arithmetic
   fixture, local fallback, or SSH workload execution can satisfy this check.
   Pass `cargo test`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo fmt --check`, and `bash scripts/ci-tier0.sh specs`. Open the account-owned
   PR using `gh`; do not merge it.
2. **PiD client.** Connect a durable PiD-next tool to that same job contract. Test
   interruption/resumption without changing job identity or confusing admission
   with completion. Do not widen the executor fleet in this slice.
3. **Lume VM Mother.** Pass NIC-73 enrollment on a macOS VM and run the same proven
   job contract with the VM as executor. Native Mac-only tests require their own
   build capability; they are not a prerequisite for enrollment. Qualify the
   remaining Pis and the native test-matrix capabilities separately.

Application state belongs in configured persistent private directories, not `/tmp`
receipts. Lost replies can be retried within Mother's dedupe window. After expiry,
recomputation is safe only because this selected workload reads an immutable input
and has no mutating effects. Interrupted or denied jobs must not become success.

This is a plan, not acceptance evidence. No physical application run, PiD tool
integration, or Lume enrollment is claimed here.
