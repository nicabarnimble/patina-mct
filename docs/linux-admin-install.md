# Linux ARM64 admin Mother

The hardware gate passes on nicamaster with a system-level unit running as
`User=pi` and `Group=pi`. The system manager establishes filesystem confinement
without `PrivateUsers`; Mother's ownership check remains unchanged. No linger
is needed. See the [nicamaster evidence](linux-admin-install-evidence.md) and
[first Pi 3 acceptance](nicanode0-install-evidence.md). That run required the
privileged-installer bytecode fix, since merged in PR #92. The unchanged Rust
package came from PR #91. [Nicanode1 acceptance](nicanode1-install-evidence.md)
records the same sequence and passing gate on merged main.

This slice packages the **admin-only** Mother on Debian with Nix, not NixOS.
Each Mother keeps its own identity and canonical state on local storage. It does
not enable the full passkey Child runtime, Linux Git Toy, or task continuation.

## Build and hardware gate

On the ARM64 host, from the reviewed checkout:

```sh
. /nix/var/nix/profiles/default/etc/profile.d/nix-daemon.sh
nix build .#mother
nix run .#mother-install-smoke
```

Builds use Rust 1.96.0, the committed Cargo lock and pinned Nix inputs. The Cargo
Git dependency has an explicit fixed-output hash. Two Cargo build jobs bound
parallel compilation on the initial four-gigabyte builder; this is not a claim
that the build fits one gigabyte. Pi 3 nodes should consume a separately
configured trusted cache rather than compile locally.

Run the gate as `pi`, with noninteractive sudo authorized for system-unit
management, the host's `/usr/bin/systemctl`, and an ext4, XFS or Btrfs home
filesystem. It does not configure sudo policy. It creates a unique private directory under
that home, uses **software test credentials only**, installs a uniquely named
root-owned system unit through `mother-admin-service`, and tests signed administration over
loopback Iroh. It does not establish actual 1Password behavior or external
network reachability.

A successful run prints PASS for installation/start, signed status, second-writer
refusal, revocation, forced restart, fresh revoked-key denial and WASI execution.
It exits nonzero on a missing measurement, timeout, wrong result, transport
failure masquerading as rejection, or cleanup failure. An indeterminate signed
submission is not retried. Successful cleanup stops/removes only its own unit and
package retention link, then verifies that the packaged script inputs still
match their Nix hash. Privileged Python installation disables bytecode writes;
root must not add `__pycache__` to immutable package inputs. The printed private
evidence directory remains; it
contains test key material and must **not** be uploaded wholesale.

Measurements:

- **Cold readiness:** one process-cold start through the first authenticated
  status response. Includes CLI/signing overhead; the OS cache is not purged.
- **Idle RSS:** five process samples at 200 ms spacing; print min/max.
- **Mother peak RSS:** `/proc/PID/status` VmHWM over both tested tenures.
- **Compile peak:** whole fresh probe-process VmHWM immediately after component
  compilation, not Rust/Nix build peak or a heap delta. Compile time excludes
  WAT parsing, engine construction and execution. The component imports
  `wasi:random/random@0.2.3` and returns a checked result; no inherited filesystem,
  environment or stdio is provided. This is separate from the admin Mother, not
  a full Child-admission test or an AOT cache.
- **Durable writes:** forty single-frame canonical BeforeEffect append+sync_data
  calls, then validated reopen/count; nearest-rank p50/p95 and max. Raw samples
  are in the private `measurements.json`. Creation/reopen are outside the timer.

No result here establishes power-loss recovery, large-history startup, replication,
concurrent Child capacity or suitability for a one-gigabyte Pi 3. Run the same
hardware gate on a physical Pi 3 before any fleet rollout.

## Install a provisioned Mother

Installation and provisioning are separate. First use the explicit trusted-root
procedure in [passkey administration](passkey-admin.md) and
[the approval RUN](passkey-approval-RUN.md). The administrator's browser uses
**your default browser with 1Password**; the headless Mother does not need the
administrator's private day key. Root trust/CA installation is never implicit.

As `pi`, for an **already provisioned** private local root owned by `pi`:

```sh
nix profile add .#mother
nix run .#mother-admin-service -- \
  --root /absolute/private/mother-root \
  --mother YOUR-MOTHER-ID --ledger-id YOUR-LEDGER-ID \
  --unit-name mct-admin-YOUR-NAME.service
```

Use lowercase letters, numbers and hyphens in these identifiers. The installer
checks the canonical target and passkey provenance by read-only replay before
publishing a new unit. This replay runs as `pi`, not root. Sudo runs the immutable
installer only for privileged publication and removal. Units are root-owned 0644
files in `/etc/systemd/system/`; they contain paths and IDs, never signing keys.
Existing units/package roots are refused, not replaced, including transient or
vendor units already known to the system manager. The exact package is
retained through a root-owned indirect GC root under `/var/lib/mct/package-roots/`.
The installer does not start the service, generate keys, install a trust root,
change network settings or configure linger.

Then explicitly start/enable it:

```sh
sudo /usr/bin/systemctl enable --now mct-admin-YOUR-NAME.service
sudo /usr/bin/systemctl status mct-admin-YOUR-NAME.service
```

The unit belongs to `multi-user.target`, not a user manager. Enabling it provides
boot activation independently of login sessions; no linger is required. The gate
proves forced process restart, not a host reboot or power loss.
The canonical root and endpoint identity survive service restart; the authority
epoch advances. Public ticket output is `ROOT/admin-ticket.json`, mode 0600.
Intent preparation still needs local canonical access; remote preparation and
external exposure are not added here.

The installer prints the unit's `[device,inode,sha256]` identity. Preserve that
receipt. To remove the same unchanged unit:

```sh
nix run .#mother-admin-service -- --operation remove \
  --unit-name mct-admin-YOUR-NAME.service \
  --expected-identity '[DEVICE,INODE,"SHA256"]'
```

The helper verifies the root-owned unit and package-root target before it stops
and disables the service. It then removes only those two installer artifacts and
reloads the system manager. Changed units are preserved for inspection.
**Canonical state and identity are never removed.** No upgrade/replacement or
automatic recovery of partially installed artifacts is implemented. Inspect an
interrupted installation before manual action.

Lifecycle commands use the root-owned host `/usr/bin/systemctl`, not a newer Nix
client whose D-Bus methods may be incompatible with the installed manager.

## Security and resource envelope

Nix's pinned inputs/content hashes and the host's configured trusted binary-cache
keys establish package delivery provenance. The programs and their ancestry are
checked for ownership and writability. Before execution, `nix-store --verify-path`
checks the package content hash against Nix's registered hash, without repair.
The unit points at the exact immutable store
path, not PATH lookup or an arbitrary writable executable. This is not a new MCT
authority source or a claim of publisher signing equivalent to Apple codesign.
Existing Darwin release verification and launchd lifecycle are unchanged; this
path directly supervises the foreground admin-only server.

The unit uses a read-only system/home view with only its canonical root writable,
a private mount namespace and temporary storage, no-new-privileges, empty capability
sets, restricted address families and a 0077 service umask. It does not create a
user namespace, so filesystem ownership remains visible without translation. AF_NETLINK permits
Iroh's unprivileged Linux route monitor; it does not grant CAP_NET_ADMIN. The gate
checks live mount flags, UID/GID, root ownership, namespace identities, private
temporary storage, effective/ambient/bounding capabilities and the actual route-monitor socket. A 512 MiB service memory ceiling reserves half the smallest
planned one-gigabyte host for the OS/other work; it is a containment budget, **not
measured proof of fit**. TasksMax=64 is a conservative admin-only thread ceiling,
not a Child concurrency limit. Crashes restart after one second, bounded to three
starts per minute, so corrupt state cannot spin indefinitely. SIGINT gets up to
30 seconds for graceful shutdown before systemd's forced-stop policy applies.

The new explicit `serve-iroh --bind-address` accepts only loopback addresses.
Omitting it preserves the existing command's behavior. Units always specify
`127.0.0.1:0`. No firewall ports, public relay, Tailscale join or CA is installed.
If the required systemd sandbox facilities are unavailable, service startup
fails; do not remove hardening just to make the gate pass. Root/host compromise
and hostile same-UID processes remain outside the local isolation boundary.
