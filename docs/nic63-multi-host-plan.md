# NIC-63 multi-host acceptance plan

## Goal and order

Prove independent Mothers on physical LAN hosts before the OCIC Proxmox run.
Run slice 2 on nicamaster and nicanode0. After it passes, add nicanode1 for
slice 3, then test a rolling upgrade. A failed acceptance gate stops progression.
This document records the plan, not results. Refs NIC-63.

## Address and authority bootstrap today

`MotherIrohEndpointTicket` carries an endpoint public key, direct socket addresses,
and relay URLs. The local endpoint configuration disables relays by default.
The run pins each endpoint to its configured LAN address and UDP port and rejects
relay URLs. Host addresses, expected hostnames, SSH settings, ports, and run paths
belong in private configuration, not executable code.

Each Mother creates and retains its own endpoint secret. The controller reads
public tickets through each host's owner-only control socket. It installs scoped
peer entries through `/peers/add` and exchanges receiver-issued binding proofs
through `/peers/proof`. `MctDaemonConfigStore::upsert_peer` signs missing binding
proofs with the local endpoint key. Hello admission verifies the binding and
current policy. Iroh authenticates endpoint key possession. These are separate
checks, not a claim that a transport key alone grants authority.

Admitted hello responses publish callable WASM operations. No test inserts remote
catalog rows or execution results. Each host runs a distinct operation so a local
fallback cannot satisfy a remote-call assertion. The test uses isolated software
authority and the existing owner-only control interface, not a production passkey
ceremony or an installed production identity.

## Stable addressing gap for OCI-118

Persisted identity provides a stable endpoint key. Static tickets provide today's
reachability. Restart on the same configured LAN socket can retain both.
Inspection found that the current N0 preset retains public PKARR and DNS lookup
even when relays are disabled. Clear those services for local-only operation
before launching the fleet. Public discovery is not an acceptable LAN-only
fallback. There is no configured LAN-local key-only lookup service to repair an
obsolete ticket after an address changes. Capability refresh is not address
discovery. OCI-118 needs an explicit discovery or address-update mechanism,
authenticated freshness and expiry, key-rotation behavior, and changed-address
acceptance tests. This run does not claim a Tailscale replacement is complete.

## Implementation shape

Use a configuration-driven controller over strict-host-key SSH and a small
host-local fixture helper where the product CLI lacks fixture generation or
canonical ledger verification. Reuse the real daemon, control requests, signed
peer admission, runtime, and ledger verifier from the existing fleet proof.
Do not duplicate authority or ledger cryptography in the script.

The configuration describes a builder, a list of hosts, an immutable package,
a baseline package for upgrade, and run settings. Each host owns its process,
identity, state, and ledger. A new run gets a new private directory. Restarts reuse
that directory. The controller tracks only processes it started and retains
failure evidence. It refuses an existing unrelated Mother rather than killing it.

The alternative is a Rust-only distributed test executable. It would duplicate
SSH lifecycle and configuration plumbing in a compile-heavy test target. Prefer
the script plus a bounded native helper unless source inspection disproves that
choice. Node tests can validate controller behavior without compiling on this Mac.

## Build and delivery

Build the candidate once on nicamaster. Never compile packages or Rust on the
Pi 3s or the Mac. Reuse the existing Nix toolchain and trusted cache. Package the
daemon, fixture helper, and script dependencies in an immutable closure. Copy it
to target hosts in parallel and verify closure contents and executable digests.
Use an existing recorded older package for the rolling-upgrade baseline. A
same-binary restart does not count as an upgrade.

The critical path is the nicamaster build, closure delivery, then real remote
acceptance. In parallel, inspect transport guarantees and test the controller.
Do not run redundant per-node builds. No firmware changes or re-imaging are allowed.
Do not change Tailscale, routes, DNS, or persistent network policy.

## Acceptance gates

1. Slice 2 starts one Mother per host with distinct identities and independently
   owned state. Real hello admission yields remote capabilities. A call submitted
   to A executes the distinct WASM operation on B and returns the expected value.
   Both canonical ledgers verify. Execution facts belong only to the executing
   Mother. Forwarding observations on A remain distinct from B's execution facts.
   B revokes A while A retains its offer. A's next call is denied and B has no new
   child execution for that call.
2. Slice 3 repeats the gates for three hosts. Isolate each host in turn with a
   bounded, reversible test scoped to the test processes. Distinguish a paused
   process from a packet-loss partition in the evidence. Do not call one the
   other. Kill each Mother in turn. The two survivors must still serve each other.
   Restart the victim with the same key and ledger and prove remote service
   resumes without re-admission. Repeat ledger and revocation checks.
3. Rolling upgrade uses two distinct, recorded daemon artifacts. Replace one
   host's executable at a time without changing its identity, authority, or ledger.
   Check survivor calls during each transition, returning-host calls, canonical
   ledger continuity, and revocation denial. Record the actual old and new hashes.

Publish commands, package provenance, sanitized results, and the PR link after
each slice. Never publish secrets, full private roots, or pretend a skipped check
passed. Keep reusable host configuration separate so the same command can target
Proxmox VMs later.
