import assert from 'node:assert/strict';

function stagingRoot(root) {
  assert.match(root, /^\/[a-zA-Z0-9/._+-]+\/fleet-[a-f0-9-]{36}\/[0-9]+$/);
  assert.ok(!root.split('/').some(part => part === '.' || part === '..'));
  return `${root}-executables`;
}
export function stagedPath(root, kind) {
  assert.ok(['candidate', 'baseline'].includes(kind));
  return `${stagingRoot(root)}/${kind}/mct-daemon`;
}
export function stageCommand(source, root, kind, digest) {
  assert.match(source, /^\/nix\/store\/[a-z0-9]{32}-[a-zA-Z0-9._+-]+\/bin\/mct-daemon$/);
  assert.match(digest, /^[a-f0-9]{64}$/);
  stagedPath(root, kind);
  return ['bash', '-c', `set -euo pipefail
umask 077
source=$1
root=$2
kind=$3
expected=$4
[[ $(readlink -f "$source") == "$source" && -f $source && $(stat -c %u "$source") == 0 ]] || exit 72
mode=$(stat -c %a "$source")
(( (8#$mode & 022) == 0 )) || exit 72
parent=$(dirname "$root")
current=
IFS=/ read -r -a parts <<< "\${parent#/}"
for part in "\${parts[@]}"; do
  current="$current/$part"
  if [[ ! -e $current && ! -L $current ]]; then mkdir -m700 "$current"; fi
  [[ -d $current && ! -L $current ]] || exit 72
  owner=$(stat -c %u "$current")
  mode=$(stat -c %a "$current")
  [[ $owner == 0 || $owner == $(id -u) ]] || exit 72
  (( (8#$mode & 022) == 0 )) || exit 72
done
if [[ ! -e $root && ! -L $root ]]; then
  mkdir -m700 "$root"
  printf 'mct-federation-executables-v1\\n' > "$root/.owned"
fi
[[ -d $root && ! -L $root && $(stat -c %u "$root") == $(id -u) && $(stat -c %a "$root") == 700 ]] || exit 72
read -r marker < "$root/.owned"
[[ $marker == mct-federation-executables-v1 ]] || exit 72
mkdir -m700 "$root/$kind"
install -m700 "$source" "$root/$kind/mct-daemon"
actual=$(sha256sum "$root/$kind/mct-daemon")
[[ \${actual%% *} == "$expected" ]] || exit 72
printf '%s\\n' "$root/$kind/mct-daemon"
`, '--', source, stagingRoot(root), kind, digest];
}
export function cleanupStagedCommand(root) {
  return ['bash', '-c', `set -euo pipefail
root=$1
[[ ! -L $root ]] || exit 72
[[ -e $root ]] || exit 0
[[ -d $root && $(stat -c %u "$root") == $(id -u) && $(stat -c %a "$root") == 700 ]] || exit 72
read -r marker < "$root/.owned"
[[ $marker == mct-federation-executables-v1 ]] || exit 72
rm -rf -- "$root"
`, '--', stagingRoot(root)];
}
