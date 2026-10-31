---
type: feat
id: linux-admin-install
status: active
created: 2026-10-31
related:
  - docs/DECISIONS.md
  - docs/linux-admin-install.md
  - layer/core/safety-boundaries.md
exit_criteria:
  - id: exclusive-user-unit
    text: Linux user-unit installation refuses existing files, unsafe ancestry and directive injection without provisioning authority.
    checked: true
    verify: python3 scripts/test-mother-systemd.py
  - id: bounded-local-admin
    text: Explicit loopback binding preserves signed admission and default CLI compatibility.
    checked: true
    verify: cargo test -p mct-daemon --bin mct-daemon passkey_admin_optional_bind_is_loopback_only
  - id: native-hardware-gate
    text: The pinned Nix package builds and the isolated systemd gate passes on native nicamaster with resource and durable-write measurements.
    checked: false
    verify: nix run .#mother-install-smoke
---

# NIC-53: native Linux ARM64 admin Mother

Owner-authorized first installation slice under NIC-52's independent-Mother
architecture. Canonical state stays on each host's local filesystem. This is
not a replicated authority database, Pi integration, or task continuation.

## Contract

- Pin nixpkgs, Rust 1.96.0 and Cargo inputs. Build natively on ARM64 Linux.
- Package the existing bounded admin-only foreground server behind systemd's
  user manager. Do not route passkey authority through legacy launchd lifecycle,
  release ingestion or UID/offline administration.
- Nix store integrity/root ownership and systemd sandboxing are this delivery
  path's provenance and service controls. Darwin release/codesign checks remain
  intact for Darwin delivery; they are not deleted or bypassed.
- Installation consumes an already provisioned, independently trusted passkey
  genesis. It performs read-only validation, exclusively installs a new unit and
  retains the package with a Nix GC root. Starting, enabling and enabling linger
  are explicit operator actions. Never overwrite identity or bootstrap silently.
- Admin binds loopback in these units, with no new relay, firewall, CA, trust-root,
  Tailscale, or public-network setup. Transport possession remains insufficient.
- SIGINT is graceful stop; SIGKILL exercises automatic process recovery. Lock,
  hash-chain, snapshot, nonce, scope and revocation checks remain canonical.
- Linux Git Toy remains denied. No AOT deserialization, full Child cutover,
  migration of existing authorities, network-mounted state or Pi 3 rollout.

## Hardware gate

Each run creates new private local state, synthetic ES256 root and one-hour
status-only Ed25519 lease, then uses the packaged installer and actual user
service. It proves signed status, bad-signature denial, exclusive writer refusal,
root revocation, forced restart, unchanged endpoint identity and canonical prefix,
fresh authority epoch and a fresh revoked-key denial. It never retries an
indeterminate submission. Cleanup stops/removes only the unique verified unit
and its package root; private authority/evidence remains for inspection.

A separate short-lived probe compiles an embedded component from verified program
source, executes a real `wasi:random` host import and checks its exported result.
It is **not** an authorized Child routed through passkey runtime integration.
A separate local ledger measures forty acknowledged BeforeEffect appends and
verifies all forty after reopen. No benchmark changes production durability.

Report process-cold signed readiness (not cold disk), five idle RSS samples,
Mother high-water RSS across both tenures, isolated component compile time and
helper-process high-water RSS, and durable-write nearest-rank p50/p95/max with
raw samples. These are hardware observations, not power-loss guarantees or a
one-gigabyte fitness claim. Physical Pi 3 acceptance remains required separately.
