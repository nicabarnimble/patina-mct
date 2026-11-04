# LAN federation acceptance

This Linux-only test harness runs real Mothers, signed Iroh hello admission,
and a small WASM component. It does not emulate execution or import another
Mother's observations into a local ledger.

## Run

Copy `config.example.json` to a private file outside the repository. Replace
all example values. Use actual RFC1918 addresses, matching SSH destinations,
unique expected hostnames, and an immutable package produced on the configured
builder. Keep the SSH identity and host inventory out of git.

Run the full consecutive sequence with one fail-fast command. Configure the
baseline package before starting; do not edit the inventory during the run.
The evidence parent must already exist, but the final directory must be new.

```sh
bash scripts/federation/suite.sh \
  /absolute/private/fleet.json /absolute/private/new-suite-evidence
```

The suite stops at the first failure and writes its aggregate `result.json` only
after all three slices pass. To run one slice independently:

```sh
node scripts/federation/run.mjs \
  --config /absolute/private/fleet.json \
  --slice two \
  --evidence /absolute/private/new-evidence-directory
```

Use `three` next, then `upgrade`. Each invocation requires a new evidence
directory. `two` selects the builder and first other host; the other scenarios
select the builder and first two other hosts. Change the inventory to reuse the
same controller with another Linux host set, including future Proxmox VMs.
Those VMs are not claimed tested by the Pi evidence.

Requirements:

- Node 22 or newer on the controller; strict SSH known-host entries.
- Linux, systemd, Nix, and noninteractive sudo on each test host.
- No existing `mct-daemon` on any selected host. A retained per-user host lock
  excludes concurrent fixture runs; observed boot identities must be distinct.
- A short, private `runRoot` beneath a trusted home directory. Do not use `/tmp`
  for staged executables: the existing executable trust policy rejects writable
  ancestors. Roots and fixtures are created exclusively, never reused.
- The builder already holds the complete immutable package. Targets import its
  signed closure over LAN SSH, with compilation and public substituters disabled.

`run.mjs` deploys the closure, verifies its signatures and content, then runs the
scenario. Transfers to the selected non-builder hosts are parallel. The lower
level `fleet.mjs` command has the same arguments but assumes deployment is done.

The helper requires an executable beneath trusted ancestors. Nix's sticky,
group-writable store does not satisfy that API. The controller therefore copies
each verified executable into a new mode-0700 run directory, checks its SHA256
against preflight, and uses the unchanged native trust check there. These copies
are removed only after the owned helper service is confirmed stopped.

## Build once on the builder

Use a dedicated source directory and the pinned, offline Cargo environment from
`docs/nic63-slice1-nicamaster.md`. Never run Cargo or Nix builds on target Pis.
Before transferring the committed source, create its compile-input manifest:

```sh
git ls-files -z Cargo.toml Cargo.lock crates flake.nix flake.lock |
  xargs -0 shasum -a 256 > source.sha256
```

On the configured builder, with the vendored Cargo source configuration already
prepared, run:

```sh
bash scripts/federation/build.sh EXPECTED_BUILDER_HOST FULL_SOURCE_COMMIT \
  /absolute/shared-target /absolute/new-artifacts-directory
```

The script checks hostname and source hashes, builds and tests the helper with
the daemon, then packages the already-built artifacts. `nix/federation-bundle.nix`
adds pinned `nft` and `bpftool` utilities. Set `package` to the resulting immutable
store path. The binaries are test-profile builds, not release/install claims.

## Gates

Every scenario requires local controls and remote WASM execution in every
selected direction, canonical verification of each local ledger, exact
caller/receiver ID correlation, and receiver-enforced revocation. A generic
failure or caller `no_route` does not count as revocation denial. Typed execution
facts under denied traces fail the run even if provenance markers are absent.

`three` additionally:

1. Takes a committed ledger checkpoint and deliberately SIGKILLs each Mother.
2. Calls both directions between the two survivors while the victim is stopped.
3. Restarts the victim through actual daemon recovery, without re-admission.
4. Requires its existing endpoint key and exact committed ledger prefix.
5. Temporarily drops only that Mother's test UDP traffic to/from the other two
   peers. Survivors must serve, the blocked probe must fail, and packet counters
   must prove real drops. The unique test table is deleted and pre-existing
   firewall policy is compared before/after. An independent 180-second rollback
   timer limits a lost-controller partition. SSH and Tailscale are not targeted.

A three-host run without verified packet partition evidence is **incomplete**,
not passed. A process pause is not a packet partition.

`upgrade` requires `baselinePackage`, distinct executable hashes, survivors
serving during each graceful replacement, mixed-version calls, preserved keys
and ledger prefixes, and the common verification/revocation gates. It is
harness-managed replacement, not the macOS-only product upgrade command.

The older package may retain public discovery and router mapping defaults.
Before any baseline Mother starts, its transient test service must have an
exact LAN-host IP allowlist and deny-all for other IPv4/IPv6 addresses. The
controller verifies both directly attached kernel filtering programs, not just
requested systemd properties. A configured peer must not be a default gateway,
otherwise the old router-mapping traffic could escape that IP fence. Missing
enforcement or an overlapping gateway blocks the upgrade. No
persistent network policy, Tailscale configuration, or service installation is
changed. Kernel BTF is not needed to inspect cgroup program attachment.

## Evidence and cleanup

`stages.jsonl` and `result.json` contain sanitized controller evidence. Canonical
ledgers, identities, and boot logs stay in private host run roots. Keep identities
private; publish only reviewed, sanitized evidence. The immutable packages remain
in the Nix store. Temporary transfer archives, caches, and signing secrets are
removed on success and ordinary failure paths.

Each helper runs in a unique transient systemd service with a bounded lifetime
and control-group cleanup. Normal stops use SIGTERM with bounded escalation;
only explicit crash scenarios request SIGKILL. Cleanup independently stops the
owned service and confirms it inactive/absent. Cleanup errors fail acceptance.
No PID-file-based broad kill, system re-image, or permanent service is used.

Static LAN tickets and persisted endpoint keys prove same-key restart at the
same configured address. Key-based reachability after an address change remains
a discovery/address-management gap for OCI-118; this harness does not solve it.
