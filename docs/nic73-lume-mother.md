# NIC-73 Lume Mother acceptance

NIC-73 uses exactly one disposable Lume VM in **NAT** network mode. Lume and SSH are administration only: the dependency report enters through the Studio Mother and executes over Iroh in the VM Mother as the approved `mct-repo-report` Child using its scoped directory-read Toy.

Config version 2 binds the VM name, exact NAT IP, SHA-256 of the Lume machine identifier, SHA-256 of the guest `IOPlatformUUID`, and independent SHA-256 pins for the remote daemon and helper. These attestation fields do not change the logical job ID, which remains the version-1 workload/input/component/target/worker identity.

The commands below use environment variables rather than machine-specific values. Run them from the repository root and set every required variable explicitly.

## 1. Disable Lume telemetry and create or start the one NAT VM

```sh
export REPO_ROOT="$PWD"
export LUME_VM='EDIT_ME'
export LUME_NAT_IP='EDIT_ME'
export LUME_IPSW='/absolute/path/to/a/compatible-macos-restore.ipsw'
export LUME_UNATTENDED_PRESET='tahoe'
export REMOTE_USER='EDIT_ME'
export VISION_ID='EDIT_ME'
export STUDIO_NODE='EDIT_ME'
export WORKER_NODE='EDIT_ME'
export STUDIO_BIND_IP='EDIT_ME_PRIVATE_IPV4'
export STUDIO_BIND_PORT='EDIT_ME'
export WORKER_BIND_PORT='EDIT_ME'

lume config telemetry disable
lume config telemetry status

# Create exactly one VM when it does not already exist. Do not clone another VM.
lume create "$LUME_VM" \
  --ipsw "$LUME_IPSW" \
  --unattended "$LUME_UNATTENDED_PRESET" \
  --cpu 4 \
  --memory 8GB \
  --disk-size 60GB \
  --network nat

# Start that same VM headlessly. For an already-running VM, omit this command.
lume run "$LUME_VM" \
  --detach \
  --display none \
  --vnc disabled \
  --network nat

lume get "$LUME_VM" --format json
```

These are operator commands, not steps performed by the acceptance wrapper. Use an IPSW whose guest macOS version is no newer than the Studio host and whose release matches the unattended preset; do not use `--ipsw latest` without checking that compatibility. Confirm that the selected record reports network mode `nat` and the exact address in `LUME_NAT_IP`. The smoke wrapper repeats and enforces both checks.

## 2. Create fresh private local roots and key

```sh
export NIC73_ROOT="${HOME}/.local/state/mct-nic73"
export STUDIO_ROOT="${NIC73_ROOT}/studio-root"
export STUDIO_DATA_ROOT="${NIC73_ROOT}/studio-data"
export JOBS_ROOT="${NIC73_ROOT}/jobs"
export KEY_PATH="${NIC73_ROOT}/keys/worker_ed25519"
export KNOWN_HOSTS_PATH="${NIC73_ROOT}/keys/worker_known_hosts"
export CONFIG_PATH="${NIC73_ROOT}/config.json"

install -d -m 700 \
  "$NIC73_ROOT" \
  "$STUDIO_ROOT" \
  "$STUDIO_DATA_ROOT" \
  "$JOBS_ROOT" \
  "$(dirname "$KEY_PATH")"
ssh-keygen -q -t ed25519 -N '' -f "$KEY_PATH"
chmod 600 "$KEY_PATH"

host_keys_new="${KNOWN_HOSTS_PATH}.new"
rm -f "$host_keys_new"
ssh-keyscan -T 10 -p 22 -t ed25519 "$LUME_NAT_IP" >"$host_keys_new"
node --input-type=module - "$host_keys_new" "$LUME_NAT_IP" <<'NODE'
import {readFileSync} from 'node:fs';
import {validateKnownHosts} from './scripts/first-real-app/client.mjs';
validateKnownHosts(readFileSync(process.argv[2]),process.argv[3]);
NODE
ssh-keygen -lf "$host_keys_new"
# Stop here until this one fingerprint is verified through an independent channel.
install -m 600 "$host_keys_new" "$KNOWN_HOSTS_PATH"
rm -f "$host_keys_new"
```

Authorize `"${KEY_PATH}.pub"` for `REMOTE_USER` using the VM's existing administrative path. The dedicated `KNOWN_HOSTS_PATH` must contain exactly one unmarked `ssh-ed25519` entry: the plain numeric `LUME_NAT_IP` host token for fixed port 22 and one canonical base64 key, with no wildcard, hash, alternate host, certificate-authority marker, trailing field, or additional entry. Do not rely on a personal or global `known_hosts` file.

After key authentication succeeds, remove the unattended/default password or disable password authentication. For example, run the following inside the guest, then verify a new key-only connection before closing the administrative session:

```sh
sudo install -d -m 755 /etc/ssh/sshd_config.d
printf '%s\n' \
  'PasswordAuthentication no' \
  'KbdInteractiveAuthentication no' |
  sudo tee /etc/ssh/sshd_config.d/100-nic73-key-only.conf >/dev/null
sudo chown root:wheel /etc/ssh/sshd_config.d/100-nic73-key-only.conf
sudo chmod 600 /etc/ssh/sshd_config.d/100-nic73-key-only.conf
sudo /usr/sbin/sshd -t
sudo /usr/sbin/sshd -T | grep -E '^(passwordauthentication|kbdinteractiveauthentication) no$'
```

Do not leave the unattended default password usable.

## 3. Build and stage the pinned daemon and helper

```sh
scripts/first-real-app/build-child.sh
CARGO_TARGET_DIR="$REPO_ROOT/target" \
  cargo build --locked --offline -p mct-daemon \
  --bin mct-daemon --example first_real_app_host

export REMOTE_BASE="/Users/${REMOTE_USER}/.local/state/mct-nic73"
export WORKER_ROOT="${REMOTE_BASE}/worker-root"
export WORKER_DATA_ROOT="${REMOTE_BASE}/worker-data"
export REMOTE_BIN_DIR="/Users/${REMOTE_USER}/.local/libexec/mct-nic73"
export REMOTE_DAEMON="${REMOTE_BIN_DIR}/mct-daemon"
export REMOTE_HELPER="${REMOTE_BIN_DIR}/first_real_app_host"

SSH_OPTS=(
  -F /dev/null
  -p 22
  -o BatchMode=yes
  -o IdentitiesOnly=yes
  -o StrictHostKeyChecking=yes
  -o "UserKnownHostsFile=$KNOWN_HOSTS_PATH"
  -o GlobalKnownHostsFile=/dev/null
  -o ControlMaster=no
  -o ClearAllForwardings=yes
  -i "$KEY_PATH"
)
SCP_OPTS=(
  -F /dev/null
  -P 22
  -o BatchMode=yes
  -o IdentitiesOnly=yes
  -o StrictHostKeyChecking=yes
  -o "UserKnownHostsFile=$KNOWN_HOSTS_PATH"
  -o GlobalKnownHostsFile=/dev/null
  -o ControlMaster=no
  -o ClearAllForwardings=yes
  -i "$KEY_PATH"
)

/usr/bin/ssh "${SSH_OPTS[@]}" "$REMOTE_USER@$LUME_NAT_IP" \
  "umask 077; install -d -m 700 '$WORKER_ROOT' '$WORKER_DATA_ROOT' '$REMOTE_BIN_DIR'"
/usr/bin/scp "${SCP_OPTS[@]}" \
  "$REPO_ROOT/target/debug/mct-daemon" \
  "$REMOTE_USER@$LUME_NAT_IP:${REMOTE_DAEMON}.new"
/usr/bin/scp "${SCP_OPTS[@]}" \
  "$REPO_ROOT/target/debug/examples/first_real_app_host" \
  "$REMOTE_USER@$LUME_NAT_IP:${REMOTE_HELPER}.new"
/usr/bin/ssh "${SSH_OPTS[@]}" "$REMOTE_USER@$LUME_NAT_IP" \
  "chmod 700 '${REMOTE_DAEMON}.new' '${REMOTE_HELPER}.new' && mv '${REMOTE_DAEMON}.new' '$REMOTE_DAEMON' && mv '${REMOTE_HELPER}.new' '$REMOTE_HELPER'"

# Mandatory before the VM Mother is started for the first time.
/usr/bin/ssh "${SSH_OPTS[@]}" "$REMOTE_USER@$LUME_NAT_IP" \
  "test ! -e '$WORKER_ROOT/identity/iroh-secret.hex'"

/usr/bin/ssh "${SSH_OPTS[@]}" "$REMOTE_USER@$LUME_NAT_IP" \
  "uid=\$(/usr/bin/id -u); for path in '$REMOTE_DAEMON' '$REMOTE_HELPER'; do test ! -L \"\$path\" && test -f \"\$path\" && test -x \"\$path\"; test \"\$(/usr/bin/stat -f '%u' \"\$path\")\" = \"\$uid\"; test \"\$(/usr/bin/stat -f '%l' \"\$path\")\" = 1; done"

export WORKER_BINARY_SHA256=$(/usr/bin/ssh "${SSH_OPTS[@]}" "$REMOTE_USER@$LUME_NAT_IP" \
  "/usr/bin/shasum -a 256 '$REMOTE_DAEMON'" | awk '{print $1}')
export WORKER_HELPER_SHA256=$(/usr/bin/ssh "${SSH_OPTS[@]}" "$REMOTE_USER@$LUME_NAT_IP" \
  "/usr/bin/shasum -a 256 '$REMOTE_HELPER'" | awk '{print $1}')
```

`run.mjs` independently checks each configured remote path's no-symlink file type, SSH-user ownership, single hard link, and trusted executable mode before running a separate strict-SSH hash operation over both paths and before starting the helper. Every config-v2 SSH path uses `-F /dev/null -p 22`; the staging SCP commands use `-F /dev/null -P 22`, so hostile `Host *` defaults cannot redirect the endpoint, port, proxy, or local command.

## 4. Capture Lume and guest identity digests

```sh
export LUME_JSON=$(lume get "$LUME_VM" --format json)
export MACHINE_IDENTIFIER=$(node -e '
const raw=JSON.parse(process.env.LUME_JSON);
const rows=Array.isArray(raw)?raw:Array.isArray(raw.vms)?raw.vms:[raw];
const vm=rows.find(v=>(v.name??v.vmName??v.vm_name)===process.env.LUME_VM);
if(!vm) throw Error("named VM not found");
const value=vm.machineIdentifier??vm.machine_identifier??vm.machineId??vm.machine_id;
if(typeof value!=="string") throw Error("machine identifier missing");
process.stdout.write(value.trim());
')
export MACHINE_IDENTIFIER_SHA256=$(printf '%s' "$MACHINE_IDENTIFIER" | shasum -a 256 | awk '{print $1}')
export GUEST_HARDWARE_UUID=$(
  /usr/bin/ssh "${SSH_OPTS[@]}" "$REMOTE_USER@$LUME_NAT_IP" \
    /usr/sbin/ioreg -rd1 -c IOPlatformExpertDevice |
  /usr/bin/awk -F' = ' '/IOPlatformUUID/{gsub(/"/,"",$2); print $2}'
)
export GUEST_HARDWARE_SHA256=$(printf '%s' "$GUEST_HARDWARE_UUID" | shasum -a 256 | awk '{print $1}')
```

## 5. Write the private version-2 config

```sh
export STUDIO_DAEMON="$REPO_ROOT/target/debug/mct-daemon"
export STUDIO_HELPER="$REPO_ROOT/target/debug/examples/first_real_app_host"
export COMPONENT="$REPO_ROOT/target/first-real-app/mct-repo-report.wasm"
export MANIFEST="$REPO_ROOT/target/first-real-app/child.toml"
export INPUT="$REPO_ROOT/Cargo.lock"

node --input-type=module <<'NODE'
import {writeFileSync} from 'node:fs';
const e=process.env;
const required=[
  'CONFIG_PATH','VISION_ID','INPUT','COMPONENT','MANIFEST','JOBS_ROOT',
  'STUDIO_NODE','STUDIO_ROOT','STUDIO_DATA_ROOT','STUDIO_DAEMON','STUDIO_HELPER',
  'STUDIO_BIND_IP','STUDIO_BIND_PORT','WORKER_NODE','WORKER_ROOT','WORKER_DATA_ROOT',
  'REMOTE_DAEMON','REMOTE_HELPER','LUME_NAT_IP','WORKER_BIND_PORT','WORKER_BINARY_SHA256',
  'WORKER_HELPER_SHA256','REMOTE_USER','KEY_PATH','KNOWN_HOSTS_PATH','LUME_VM','MACHINE_IDENTIFIER_SHA256',
  'GUEST_HARDWARE_SHA256'
];
for(const name of required) if(!e[name]) throw Error(`missing ${name}`);
const config={
  version:2,
  vision:e.VISION_ID,
  input:e.INPUT,
  component:e.COMPONENT,
  manifest:e.MANIFEST,
  jobs_root:e.JOBS_ROOT,
  studio:{
    node:e.STUDIO_NODE,
    root:e.STUDIO_ROOT,
    data_root:e.STUDIO_DATA_ROOT,
    binary:e.STUDIO_DAEMON,
    helper:e.STUDIO_HELPER,
    bind:`${e.STUDIO_BIND_IP}:${e.STUDIO_BIND_PORT}`
  },
  worker:{
    node:e.WORKER_NODE,
    root:e.WORKER_ROOT,
    data_root:e.WORKER_DATA_ROOT,
    binary:e.REMOTE_DAEMON,
    helper:e.REMOTE_HELPER,
    bind:`${e.LUME_NAT_IP}:${e.WORKER_BIND_PORT}`,
    binary_sha256:e.WORKER_BINARY_SHA256,
    helper_sha256:e.WORKER_HELPER_SHA256,
    ssh:{host:e.LUME_NAT_IP,user:e.REMOTE_USER,identity:e.KEY_PATH,known_hosts:e.KNOWN_HOSTS_PATH},
    lume:{
      vm_name:e.LUME_VM,
      machine_identifier_sha256:e.MACHINE_IDENTIFIER_SHA256,
      hardware_uuid_sha256:e.GUEST_HARDWARE_SHA256,
      nat_ip:e.LUME_NAT_IP
    }
  }
};
writeFileSync(e.CONFIG_PATH,JSON.stringify(config,null,2)+'\n',{mode:0o600,flag:'wx'});
NODE
chmod 600 "$CONFIG_PATH"
```

## 6. Run acceptance

```sh
scripts/first-real-app-lume-smoke.sh --config "$CONFIG_PATH"
```

The wrapper only inspects Lume metadata and guest identity, builds local artifacts, and invokes `run.mjs` twice. It does not create, clone, start, stop, or configure a VM. The first invocation may execute the job/revocation or may already be fully durable cache. The second process must load both records from durable cache without starting a helper or SSH transport.

The JSON receipt carries the exact `executor_attestation` inspected by the wrapper: VM name, machine-identifier digest, guest hardware-UUID digest, and NAT IP. The same object is bound into the durable result, revocation intent, post-endpoint ledger baseline, and revocation receipt. The smoke validator always requires `worker_execution_count === 1` for both processes and one receiver-binding revocation for a fresh physical claim. `run.mjs` still supports `recovered_recomputed` when an interrupted run outlives the idempotency TTL, but that mode necessarily records an additional runtime start and cannot satisfy this clean physical acceptance. Interrupted revocation recovery may contain more than one before-effect revoke observation, but selects the last matching revoke before the first `BindingRevoked` rejection after the durable baseline.
