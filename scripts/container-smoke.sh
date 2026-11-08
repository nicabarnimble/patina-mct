#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

command -v docker >/dev/null || { printf 'container-smoke: docker is required\n' >&2; exit 1; }
[[ $(uname -m) == x86_64 ]] || { printf 'container-smoke: BOX must be Linux x86_64\n' >&2; exit 1; }
[[ $(uname -s) == Linux ]] || { printf 'container-smoke: BOX must be Linux x86_64\n' >&2; exit 1; }
[[ -z $(git status --porcelain --untracked-files=no) ]] || { printf 'container-smoke: tracked checkout is dirty\n' >&2; exit 1; }

source_commit=$(git rev-parse HEAD)
source_tree=$(git rev-parse 'HEAD^{tree}')
lock_sha256=$(sha256sum Cargo.lock | cut -d' ' -f1)
short=${source_commit:0:12}
image_ref=${MCT_IMAGE_REF:-patina-mct-mother:$short}
smoke_ref=${MCT_SMOKE_IMAGE_REF:-patina-mct-mother-smoke:$short}
output=${MCT_IMAGE_OUTPUT:-$repo_root/dist/container}
mkdir -p "$output"

build_args=(
  --platform linux/amd64
  --build-arg "SOURCE_COMMIT=$source_commit"
  --build-arg "SOURCE_TREE=$source_tree"
  --build-arg "LOCK_SHA256=$lock_sha256"
)

# This is the independent clean compilation. Network is available to the
# pinned builder only for `cargo fetch`; compilation itself is --offline.
docker build --pull --no-cache "${build_args[@]}" --target mother -t "$image_ref" .
# Reuse only the layers produced by the clean build above to compile the
# disposable driver. The daemon under test is copied from the mother stage.
docker build "${build_args[@]}" --target smoke -t "$smoke_ref" .

[[ $(docker image inspect "$image_ref" --format '{{.Architecture}}/{{.Os}}') == amd64/linux ]]
[[ $(docker image inspect "$image_ref" --format '{{.Config.User}}') == 10001:10001 ]]
[[ $(docker image inspect "$image_ref" --format '{{index .Config.Labels "org.opencontainers.image.revision"}}') == "$source_commit" ]]
[[ $(docker image inspect "$image_ref" --format '{{index .Config.Labels "io.patina.mct.source-tree"}}') == "$source_tree" ]]
[[ $(docker image inspect "$image_ref" --format '{{index .Config.Labels "io.patina.mct.cargo-lock-sha256"}}') == "$lock_sha256" ]]

volume="mct-smoke-${short}-$$"
cleanup() { docker volume rm -f "$volume" >/dev/null 2>&1 || true; }
trap cleanup EXIT
docker volume create "$volume" >/dev/null

timeout 300 docker run --rm \
  --network none \
  --read-only \
  --volume "$volume:/var/lib/mct-smoke" \
  --cap-drop ALL \
  --security-opt no-new-privileges=true \
  --pids-limit 256 \
  --memory 6g \
  --cpus 4 \
  "$smoke_ref"

archive="$output/patina-mct-mother-${short}-linux-amd64.tar.gz"
docker save "$image_ref" | gzip -n > "$archive"
sha256sum "$archive" > "$archive.sha256"
docker image inspect "$image_ref" > "$output/patina-mct-mother-${short}-image-inspect.json"
printf 'source_commit=%s\nsource_tree=%s\nlock_sha256=%s\nimage_ref=%s\nimage_id=%s\narchive=%s\narchive_sha256=%s\n' \
  "$source_commit" "$source_tree" "$lock_sha256" "$image_ref" \
  "$(docker image inspect "$image_ref" --format '{{.Id}}')" "$archive" \
  "$(cut -d' ' -f1 "$archive.sha256")"
