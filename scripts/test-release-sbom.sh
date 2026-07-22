#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

temp=$(mktemp -d "${TMPDIR:-/tmp}/mct-release-sbom-test.XXXXXX")
trap 'rm -rf "$temp"' EXIT
scripts/generate-release-sbom.sh --target aarch64-apple-darwin --output "$temp/one"
scripts/generate-release-sbom.sh --target aarch64-apple-darwin --output "$temp/two"
cmp "$temp/one/SBOM.cdx.json" "$temp/two/SBOM.cdx.json"
cmp "$temp/one/FIXTURE-PROVENANCE.json" "$temp/two/FIXTURE-PROVENANCE.json"
python3 - "$temp/one/SBOM.cdx.json" "$temp/one/FIXTURE-PROVENANCE.json" <<'PY'
import json
import sys
from pathlib import Path

sbom = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
provenance = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
assert sbom["bomFormat"] == "CycloneDX"
assert sbom["specVersion"] == "1.6"
assert sbom["serialNumber"].startswith("urn:uuid:")
fixtures = {item["bom-ref"]: item for item in sbom["components"] if item["bom-ref"].startswith("mct-fixture:")}
assert set(fixtures) == {
    "mct-fixture:slate-manager@0.2.0",
    "mct-fixture:folder-watch-actor@0.1.0",
    "mct-fixture:watch-null-sink@0.1.0",
}
records = {item["name"]: item for item in provenance["fixtures"]}
for name, component in ((key.split(":", 1)[1].split("@", 1)[0], value) for key, value in fixtures.items()):
    assert component["scope"] == "excluded"
    primary = records[name]["files"][-1]
    assert component["hashes"] == [{"alg": "SHA-256", "content": primary["sha256"]}]
    properties = {item["name"]: item["value"] for item in component["properties"]}
    assert properties["mct:fixture:proof-only"] == "true"
    assert properties["mct:fixture:blake3"] == primary["blake3"]
print("release-sbom: deterministic CycloneDX and fixture provenance verified")
PY
