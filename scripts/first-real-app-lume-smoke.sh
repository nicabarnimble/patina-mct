#!/usr/bin/env bash
set -euo pipefail
umask 077

if [[ $# -eq 0 ]]; then
  config=${MCT_FIRST_REAL_APP_CONFIG:-}
elif [[ $# -eq 2 && $1 == --config && -n $2 ]]; then
  config=$2
else
  echo 'usage: first-real-app-lume-smoke.sh [--config /absolute/private/config.json]' >&2
  exit 2
fi
[[ $config == /* ]] || { echo 'an absolute config path is required' >&2; exit 2; }

cd "$(dirname "$0")/.."
root=$PWD
lume=${LUME_BIN:-lume}
command -v "$lume" >/dev/null || { echo 'lume CLI not found' >&2; exit 1; }
node --input-type=module -e "import {loadConfig} from './scripts/first-real-app/client.mjs'; const c=loadConfig(process.argv[1]); if(c.version!==2) throw Error('NIC-73 requires config version 2');" "$config"
scripts/first-real-app/build-child.sh >&2
CARGO_TARGET_DIR="$root/target" cargo build --locked --offline -p mct-daemon --bin mct-daemon --example first_real_app_host >&2
field() { node -e 'const c=JSON.parse(require("fs").readFileSync(process.argv[1])); let v=c; for(const k of process.argv[2].split(".")) v=v[k]; process.stdout.write(String(v));' "$config" "$1"; }
vm_name=$(field worker.lume.vm_name)
nat_ip=$(field worker.lume.nat_ip)
machine_sha=$(field worker.lume.machine_identifier_sha256)
hardware_sha=$(field worker.lume.hardware_uuid_sha256)
ssh_host=$(field worker.ssh.host)
ssh_user=$(field worker.ssh.user)
ssh_key=$(field worker.ssh.identity)
known_hosts=$(field worker.ssh.known_hosts)
executor_attestation=$(node -e 'const c=JSON.parse(require("fs").readFileSync(process.argv[1]));process.stdout.write(JSON.stringify(c.worker.lume));' "$config")
[[ $ssh_host == "$nat_ip" ]] || { echo 'SSH host must equal the exact Lume NAT IP' >&2; exit 1; }

# Inspection only: this wrapper never creates, clones, starts, stops, or mutates a VM.
lume_json=$("$lume" get "$vm_name" --format json)
node --input-type=module -e '
import {validateLumeInspection} from "./scripts/first-real-app/client.mjs";
validateLumeInspection(JSON.parse(process.argv[1]),{vm_name:process.argv[2],nat_ip:process.argv[3],machine_identifier_sha256:process.argv[4]});
' "$lume_json" "$vm_name" "$nat_ip" "$machine_sha"

key_mode=$(stat -f '%Lp' "$ssh_key")
key_uid=$(stat -f '%u' "$ssh_key")
key_links=$(stat -f '%l' "$ssh_key")
[[ ! -L $ssh_key && -f $ssh_key && $key_mode == 600 && $key_uid == "$(id -u)" && $key_links == 1 ]] || { echo 'SSH identity must be an owned, nonsymlinked, single-link 0600 file' >&2; exit 1; }
known_mode=$(stat -f '%Lp' "$known_hosts")
known_uid=$(stat -f '%u' "$known_hosts")
known_links=$(stat -f '%l' "$known_hosts")
[[ ! -L $known_hosts && -f $known_hosts && $known_mode == 600 && $known_uid == "$(id -u)" && $known_links == 1 && $known_hosts != "$ssh_key" ]] || { echo 'known_hosts must be a dedicated owned, nonsymlinked, single-link 0600 file' >&2; exit 1; }
# loadConfig already proved this is one exact unmarked ssh-ed25519 pin for the NAT IP.
ssh_args=(-F /dev/null -p 22 -o BatchMode=yes -o IdentitiesOnly=yes -o StrictHostKeyChecking=yes -o "UserKnownHostsFile=$known_hosts" -o GlobalKnownHostsFile=/dev/null -o ControlMaster=no -o ClearAllForwardings=yes -i "$ssh_key" "$ssh_user@$ssh_host")
guest=$(/usr/bin/ssh "${ssh_args[@]}" 'set -eu; uname -s; uname -m; /usr/sbin/ioreg -rd1 -c IOPlatformExpertDevice | /usr/bin/awk -F" = " '\''/IOPlatformUUID/{gsub(/"/,"",$2); print $2}'\''; /sbin/ifconfig -a | /usr/bin/awk '\''$1=="inet"{print $2}'\''')
guest_os=$(printf '%s\n' "$guest" | awk 'NR==1')
guest_arch=$(printf '%s\n' "$guest" | awk 'NR==2')
guest_uuid=$(printf '%s\n' "$guest" | awk 'NR==3')
[[ $guest_os == Darwin && $guest_arch == arm64 && -n $guest_uuid ]] || { echo 'guest must be Darwin arm64 with a hardware UUID' >&2; exit 1; }
printf '%s\n' "$guest" | tail -n +4 | grep -Fx "$nat_ip" >/dev/null || { echo 'exact NAT IP is not present inside the guest' >&2; exit 1; }
actual_hardware_sha=$(printf '%s' "$guest_uuid" | shasum -a 256 | awk '{print $1}')
[[ $actual_hardware_sha == "$hardware_sha" ]] || { echo 'guest hardware UUID mismatch' >&2; exit 1; }
host_uuid=$(/usr/sbin/ioreg -rd1 -c IOPlatformExpertDevice | awk -F' = ' '/IOPlatformUUID/{gsub(/"/,"",$2); print $2}')
[[ -n $host_uuid && $guest_uuid != "$host_uuid" ]] || { echo 'guest hardware UUID must differ from Studio' >&2; exit 1; }

first=$(node scripts/first-real-app/run.mjs --config "$config")
second=$(node scripts/first-real-app/run.mjs --config "$config")
node --input-type=module -e '
import {validateSmokeRuns} from "./scripts/first-real-app/client.mjs";
process.stdout.write(JSON.stringify(validateSmokeRuns(JSON.parse(process.argv[1]),JSON.parse(process.argv[2]),JSON.parse(process.argv[3])))+"\n");
' "$first" "$second" "$executor_attestation"
