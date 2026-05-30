#!/usr/bin/env bash
set -euo pipefail

ALLIUM_VERSION="${ALLIUM_VERSION:-3.2.3}"
ALLIUM_SHA256_X86_64_LINUX="${ALLIUM_SHA256_X86_64_LINUX:-3e1afbed99b039a1fe659b1a3aaa69dbb274bfe18a0680571c9d0a4ab3ac205f}"

if command -v allium >/dev/null 2>&1; then
  allium --version
  exit 0
fi

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)
    asset="allium-x86_64-unknown-linux-gnu.tar.gz"
    expected_sha="${ALLIUM_SHA256_X86_64_LINUX}"
    ;;
  *)
    echo "Unsupported CI platform for pinned allium installer: $(uname -s)-$(uname -m)" >&2
    exit 1
    ;;
esac

url="https://github.com/juxt/allium-tools/releases/download/v${ALLIUM_VERSION}/${asset}"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

curl -fsSL "${url}" -o "${tmp_dir}/${asset}"
actual_sha="$(sha256sum "${tmp_dir}/${asset}" | awk '{print $1}')"
if [[ "${actual_sha}" != "${expected_sha}" ]]; then
  echo "allium archive checksum mismatch" >&2
  echo "expected: ${expected_sha}" >&2
  echo "actual:   ${actual_sha}" >&2
  exit 1
fi

tar -xzf "${tmp_dir}/${asset}" -C "${tmp_dir}"
install_dir="${HOME}/.local/bin"
mkdir -p "${install_dir}"
install -m 0755 "${tmp_dir}/allium" "${install_dir}/allium"
if [[ -n "${GITHUB_PATH:-}" ]]; then
  echo "${install_dir}" >> "${GITHUB_PATH}"
fi
"${install_dir}/allium" --version
