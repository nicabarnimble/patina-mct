#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
command_name=${1:-}
[[ -n $command_name ]] && shift || true

usage() {
  cat >&2 <<EOF
usage:
  $0 build [--target aarch64-apple-darwin] [--output directory]
  $0 smoke --artifact archive [--nocapture]

The real launchd smoke uses the fixed io.patina.mct.mother label. If that
label is loaded, smoke refuses without stopping it. Stop a production resident
explicitly before smoke and restart it explicitly afterward. Production
supervisor files are snapshotted and must remain byte-for-byte unchanged.
EOF
}

absolute_path() {
  case $1 in
    /*) printf '%s\n' "$1" ;;
    *) printf '%s\n' "$repo_root/$1" ;;
  esac
}

build_release() {
  local target=aarch64-apple-darwin
  local output="$repo_root/dist"
  while (($#)); do
    case $1 in
      --target) target=${2:?missing --target value}; shift 2 ;;
      --output) output=$(absolute_path "${2:?missing --output value}"); shift 2 ;;
      *) printf 'unknown build argument: %s\n' "$1" >&2; exit 2 ;;
    esac
  done
  mkdir -p "$output"
  output=$(cd "$output" && pwd -P)

  local work source
  work=$(mktemp -d "${TMPDIR:-/tmp}/mct-release-local.XXXXXX")
  source="$work/source"
  cleanup_build() {
    git -C "$repo_root" worktree remove --force "$source" >/dev/null 2>&1 || true
    rm -rf "$work"
  }
  trap cleanup_build RETURN
  git -C "$repo_root" worktree add --detach "$source" HEAD >/dev/null
  cd "$source"
  [[ -z $(git status --porcelain --untracked-files=all) ]] || {
    printf 'detached release worktree is not clean\n' >&2
    exit 1
  }
  scripts/check-release-version.sh
  local version source_commit source_epoch rust_version cargo_version
  version=$(python3 - <<'PY'
import tomllib
with open('Cargo.toml', 'rb') as handle:
    print(tomllib.load(handle)['workspace']['package']['version'])
PY
)
  source_commit=$(git rev-parse HEAD)
  source_epoch=$(git show -s --format=%ct HEAD)
  rust_version=$(rustc -Vv)
  cargo_version=$(cargo -V)
  export CARGO_TARGET_DIR="$work/target"

  cargo build --release --locked --target "$target" -p mct-daemon \
    --bin mct-daemon --example release-digests
  local binary digest_helper metadata payload archive
  binary="$CARGO_TARGET_DIR/$target/release/mct-daemon"
  digest_helper="$CARGO_TARGET_DIR/$target/release/examples/release-digests"
  [[ $($binary version) == "mct-daemon $version" ]] || {
    printf 'built daemon version does not match workspace version\n' >&2
    exit 1
  }

  metadata="$work/metadata"
  scripts/generate-release-sbom.sh \
    --target "$target" --output "$metadata" \
    --source-commit "$source_commit" --source-epoch "$source_epoch"
  scripts/check-release-notes.sh "$version"
  python3 scripts/extract-release-notes.py "$version" "$metadata/RELEASE-NOTES.md"

  payload="$work/payload"
  mkdir -p "$payload"
  scripts/release-target.sh "$target" assemble "$binary" "$payload" "$version"
  python3 scripts/package-release.py \
    --source "$source" \
    --payload "$payload" \
    --notes "$metadata/RELEASE-NOTES.md" \
    --sbom "$metadata/SBOM.cdx.json" \
    --provenance "$metadata/FIXTURE-PROVENANCE.json" \
    --output "$output" \
    --target "$target" \
    --version "$version" \
    --source-commit "$source_commit" \
    --source-epoch "$source_epoch" \
    --rust-version "$rust_version" \
    --cargo-version "$cargo_version" \
    --release-mode smoke \
    --signing-mode adhoc \
    --executable-relative-path payload/mct-daemon.app/Contents/MacOS/mct-daemon \
    --digest-helper "$digest_helper"
  archive="$output/mct-daemon-v$version-$target.tar.gz"
  scripts/verify-release-artifact.sh "$archive"
  printf 'release-local: verified %s from detached clean %s\n' "$archive" "$source_commit"
  cleanup_build
  trap - RETURN
}

smoke_release() {
  local artifact= nocapture=false
  while (($#)); do
    case $1 in
      --artifact) artifact=$(absolute_path "${2:?missing --artifact value}"); shift 2 ;;
      --nocapture) nocapture=true; shift ;;
      *) printf 'unknown smoke argument: %s\n' "$1" >&2; exit 2 ;;
    esac
  done
  [[ -n $artifact ]] || { usage; exit 2; }
  [[ -f $artifact && -f $artifact.sha256 && -f $artifact.blake3 ]] || {
    printf 'smoke requires archive and both sidecars: %s\n' "$artifact" >&2
    exit 1
  }
  [[ $(uname -s) == Darwin && $(uname -m) == arm64 ]] || {
    printf 'release smoke requires aarch64-apple-darwin; refusing rather than skipping\n' >&2
    exit 1
  }
  for tool in /bin/launchctl /usr/bin/codesign cargo git jq python3; do
    command -v "$tool" >/dev/null || { printf 'release smoke missing tool: %s\n' "$tool" >&2; exit 1; }
  done

  local uid lock work source smoke_root extract_dir snapshot_dir transcript success=false
  uid=$(/usr/bin/id -u)
  lock="${TMPDIR:-/tmp}/mct-release-smoke-$uid.lock"
  if ! mkdir "$lock" 2>/dev/null; then
    printf 'another per-UID release smoke holds %s\n' "$lock" >&2
    exit 1
  fi
  work=$(mktemp -d "${TMPDIR:-/tmp}/mct-release-smoke.XXXXXX")
  chmod 700 "$work"
  source="$work/source"
  smoke_root="$work/service-root"
  extract_dir="$work/extracted"
  snapshot_dir="$work/production-snapshot"
  transcript="$work/release-smoke.transcript"
  mkdir -m 700 "$smoke_root" "$snapshot_dir"

  local production_record production_plist harness installed=false
  production_record="$HOME/.mct/supervisor.json"
  production_plist="$HOME/Library/LaunchAgents/io.patina.mct.mother.plist"
  harness=

  snapshot_one() {
    local path=$1 name=$2
    if [[ -e $path ]]; then
      [[ -f $path ]] || { printf 'production snapshot path is not a regular file: %s\n' "$path" >&2; return 1; }
      printf 'present\n' >"$snapshot_dir/$name.state"
      cp -p "$path" "$snapshot_dir/$name.bytes"
    else
      printf 'absent\n' >"$snapshot_dir/$name.state"
    fi
  }
  compare_one() {
    local path=$1 name=$2 state
    state=$(<"$snapshot_dir/$name.state")
    if [[ $state == present ]]; then
      [[ -f $path ]] && cmp -s "$snapshot_dir/$name.bytes" "$path" || {
        printf 'production file changed during release smoke: %s\n' "$path" >&2
        return 1
      }
    else
      [[ ! -e $path ]] || {
        printf 'production file appeared during release smoke: %s\n' "$path" >&2
        return 1
      }
    fi
  }
  compare_production() {
    compare_one "$production_record" record
    compare_one "$production_plist" plist
  }
  cleanup_smoke() {
    local status=$?
    set +e
    if [[ -n $harness && -x $harness && -e $smoke_root/supervisor.json ]]; then
      "$harness" release-smoke-internal stop --root "$smoke_root" >>"$transcript" 2>&1
      "$harness" release-smoke-internal uninstall --root "$smoke_root" >>"$transcript" 2>&1
    fi
    compare_production >>"$transcript" 2>&1
    local files_safe=$?
    git -C "$repo_root" worktree remove --force "$source" >/dev/null 2>&1
    rmdir "$lock" >/dev/null 2>&1
    if [[ $status -eq 0 && $files_safe -eq 0 && $success == true ]]; then
      rm -rf "$work"
    else
      printf 'release smoke failed; preserved root and transcript at %s\n' "$work" >&2
      status=1
    fi
    trap - EXIT INT TERM
    exit "$status"
  }
  trap cleanup_smoke EXIT INT TERM

  exec > >(tee "$transcript") 2>&1
  printf '=== release smoke: detached harness build ===\n'
  git -C "$repo_root" worktree add --detach "$source" HEAD >/dev/null
  [[ -z $(git -C "$source" status --porcelain --untracked-files=all) ]] || {
    printf 'detached smoke worktree is not clean\n' >&2
    exit 1
  }
  export CARGO_TARGET_DIR="$work/target"
  cargo build --manifest-path "$source/Cargo.toml" --release --locked \
    -p mct-daemon --bin mct-daemon --example release-digests --example verify-release \
    --features release-smoke-internal
  harness="$CARGO_TARGET_DIR/release/mct-daemon"
  local digest_helper verifier
  digest_helper="$CARGO_TARGET_DIR/release/examples/release-digests"
  verifier="$CARGO_TARGET_DIR/release/examples/verify-release"

  printf '=== release smoke: archive and signature verification ===\n'
  "$source/scripts/verify-release-artifact.sh" "$artifact"
  "$verifier" "$artifact" "$extract_dir" aarch64-apple-darwin
  local release_root binary manifest_commit source_commit
  release_root=$(find "$extract_dir" -mindepth 1 -maxdepth 1 -type d -print)
  [[ -n $release_root && $release_root != *$'\n'* ]] || { printf 'smoke extraction root mismatch\n' >&2; exit 1; }
  binary="$release_root/payload/mct-daemon.app/Contents/MacOS/mct-daemon"
  "$source/scripts/release-target.sh" aarch64-apple-darwin verify "$release_root/payload"
  manifest_commit=$(jq -er '.source_commit' "$release_root/RELEASE-MANIFEST.json")
  source_commit=$(git -C "$source" rev-parse HEAD)
  [[ $manifest_commit == "$source_commit" ]] || {
    printf 'smoke artifact source commit %s does not equal current detached HEAD %s\n' "$manifest_commit" "$source_commit" >&2
    exit 1
  }
  [[ $($binary version) == "mct-daemon 0.2.0" ]] || { printf 'packaged product version mismatch\n' >&2; exit 1; }
  if "$binary" release-smoke-internal preflight --root "$smoke_root" >"$work/production-cli-probe" 2>&1; then
    printf 'distributed CLI unexpectedly exposes release-smoke-internal\n' >&2
    exit 1
  fi
  grep -q "unknown command 'release-smoke-internal'" "$work/production-cli-probe" || {
    printf 'distributed CLI internal-seam refusal was not explicit\n' >&2
    exit 1
  }

  printf '=== release smoke: D1.18 production-file snapshot and label preflight ===\n'
  snapshot_one "$production_record" record
  snapshot_one "$production_plist" plist
  "$harness" release-smoke-internal preflight --root "$smoke_root"

  printf '=== release smoke: real install, start, and owner-authenticated readiness ===\n'
  "$harness" release-smoke-internal install --root "$smoke_root" --executable "$binary"
  installed=true
  "$harness" release-smoke-internal start --root "$smoke_root"
  "$binary" status --uds "$smoke_root/control.sock" --json | tee "$work/initial-status.json"
  jq -e '.running == true and .health == "healthy" and .readiness == "ready" and .version == "0.2.0"' \
    "$work/initial-status.json" >/dev/null

  printf '=== release smoke: three-fixture authority and restart proof ===\n'
  python3 "$source/scripts/release-smoke-proof.py" \
    --binary "$binary" --harness "$harness" --digest-helper "$digest_helper" \
    --root "$smoke_root" --fixtures "$source/crates/mct-daemon/tests/fixtures" \
    --primary-archive "$artifact" | tee "$work/fixture-proof.json"

  printf '=== release smoke: same-version different-digest candidate ===\n'
  local alternate_meta alternate_output alternate_notes version target source_epoch rust_version cargo_version alternate
  alternate_meta="$work/alternate-meta"
  alternate_output="$work/alternate-output"
  mkdir "$alternate_meta" "$alternate_output"
  cp "$release_root/RELEASE-NOTES.md" "$alternate_meta/RELEASE-NOTES.md"
  printf '\nSmoke candidate marker: same-version distinct archive identity.\n' >>"$alternate_meta/RELEASE-NOTES.md"
  cp "$release_root/SBOM.cdx.json" "$alternate_meta/SBOM.cdx.json"
  cp "$release_root/FIXTURE-PROVENANCE.json" "$alternate_meta/FIXTURE-PROVENANCE.json"
  version=$(jq -er '.product_version' "$release_root/RELEASE-MANIFEST.json")
  target=$(jq -er '.target_triple' "$release_root/RELEASE-MANIFEST.json")
  source_epoch=$(git -C "$source" show -s --format=%ct HEAD)
  rust_version=$(rustc -Vv)
  cargo_version=$(cargo -V)
  python3 "$source/scripts/package-release.py" \
    --source "$source" --payload "$release_root/payload" \
    --notes "$alternate_meta/RELEASE-NOTES.md" \
    --sbom "$alternate_meta/SBOM.cdx.json" \
    --provenance "$alternate_meta/FIXTURE-PROVENANCE.json" \
    --output "$alternate_output" --target "$target" --version "$version" \
    --source-commit "$source_commit" --source-epoch "$source_epoch" \
    --rust-version "$rust_version" --cargo-version "$cargo_version" \
    --release-mode smoke --signing-mode adhoc \
    --executable-relative-path payload/mct-daemon.app/Contents/MacOS/mct-daemon \
    --digest-helper "$digest_helper"
  alternate="$alternate_output/mct-daemon-v$version-$target.tar.gz"
  "$source/scripts/verify-release-artifact.sh" "$alternate"
  cmp -s "$artifact" "$alternate" && { printf 'alternate smoke archive did not change identity\n' >&2; exit 1; }
  local alternate_id wrong_id
  alternate_id="sha256:$(cut -d' ' -f1 "$alternate.sha256")"
  wrong_id="sha256:$(printf '0%.0s' {1..64})"

  printf '=== release smoke: missing and wrong exact approval refuse before lifecycle ===\n'
  cp "$smoke_root/supervisor.json" "$work/record-before-refusals.json"
  if "$harness" release-smoke-internal upgrade --root "$smoke_root" "$alternate" --json; then
    printf 'upgrade without exact approval unexpectedly succeeded\n' >&2
    exit 1
  fi
  cmp -s "$work/record-before-refusals.json" "$smoke_root/supervisor.json" || {
    printf 'missing approval changed supervisor record\n' >&2; exit 1;
  }
  if "$harness" release-smoke-internal upgrade --root "$smoke_root" "$alternate" \
      --approve-artifact "$wrong_id" --json; then
    printf 'upgrade with wrong exact approval unexpectedly succeeded\n' >&2
    exit 1
  fi
  cmp -s "$work/record-before-refusals.json" "$smoke_root/supervisor.json" || {
    printf 'wrong approval changed supervisor record\n' >&2; exit 1;
  }

  printf '=== release smoke: exact-approved shared replacement and post-verification ===\n'
  "$harness" release-smoke-internal upgrade --root "$smoke_root" "$alternate" \
    --approve-artifact "$alternate_id" --json
  "$binary" status --uds "$smoke_root/control.sock" --json | tee "$work/upgraded-status.json"
  jq -e '.running == true and .health == "healthy" and .readiness == "ready" and .version == "0.2.0" and .supervisor_revision == 2' \
    "$work/upgraded-status.json" >/dev/null
  [[ $(find "$smoke_root/releases/sha256" -mindepth 1 -maxdepth 1 -type d | wc -l | tr -d ' ') == 2 ]] || {
    printf 'smoke did not preserve both immutable daemon releases\n' >&2
    exit 1
  }
  jq -e '.daemon_release_artifacts == 2 and .artifacts == 3' \
    <("$binary" state summary --state "$smoke_root/state.sqlite" --json) >/dev/null

  printf '=== release smoke: clean stop, uninstall, preservation, and D1.18 postflight ===\n'
  "$harness" release-smoke-internal stop --root "$smoke_root"
  "$harness" release-smoke-internal uninstall --root "$smoke_root"
  installed=false
  "$harness" release-smoke-internal postflight --root "$smoke_root"
  [[ -f $smoke_root/observations.jsonl && -f $smoke_root/state.sqlite && -d $smoke_root/releases ]] || {
    printf 'uninstall did not preserve release evidence\n' >&2
    exit 1
  }
  [[ ! -e $smoke_root/supervisor.json && ! -e $smoke_root/launchd/io.patina.mct.mother.plist ]] || {
    printf 'uninstall left current smoke supervision policy\n' >&2
    exit 1
  }
  compare_production
  success=true
  printf 'release-smoke: PASS archive=%s alternate=%s transcript=%s nocapture=%s\n' \
    "$artifact" "$alternate_id" "$transcript" "$nocapture"
}

case $command_name in
  build) build_release "$@" ;;
  smoke) smoke_release "$@" ;;
  *) usage; exit 2 ;;
esac
