#!/usr/bin/env python3
"""ci-tier0.sh must not report success when a check it runs fails.

The test copies the real ci-tier0.sh into a temporary tree. Every script or
test file that ci-tier0.sh names becomes a stub that passes, except the one a
case makes fail. Then it runs the copied ci-tier0.sh and checks its exit status.
"""
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "scripts" / "ci-tier0.sh"
REFERENCED = re.compile(r"(?<![\w/.-])((?:scripts|docs)/[\w./-]+\.(?:sh|py|mjs))")


def stub(path: Path, fails: bool) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.suffix == ".sh":
        path.write_text(f"#!/usr/bin/env bash\nexit {1 if fails else 0}\n", encoding="utf-8")
    elif path.suffix == ".py":
        path.write_text(f"import sys\nsys.exit({1 if fails else 0})\n", encoding="utf-8")
    else:
        assertion = "assert.fail('stub fails')" if fails else ""
        path.write_text(
            "import test from 'node:test';\nimport assert from 'node:assert/strict';\n"
            f"test('stub', () => {{ {assertion} }});\n",
            encoding="utf-8",
        )


def run_specs(failing: str | None) -> subprocess.CompletedProcess:
    script = SOURCE.read_text(encoding="utf-8")
    referenced = sorted(set(REFERENCED.findall(script)) - {"scripts/ci-tier0.sh"})
    if failing is not None and failing not in referenced:
        raise AssertionError(f"ci-tier0.sh does not run {failing}")
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        (root / "scripts").mkdir()
        (root / "scripts" / "ci-tier0.sh").write_text(script, encoding="utf-8")
        for name in referenced:
            stub(root / name, fails=name == failing)
        return subprocess.run(
            ["bash", str(root / "scripts" / "ci-tier0.sh"), "specs"],
            cwd=root,
            env={**os.environ, "NODE_OPTIONS": ""},
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )


class SpecsExitTests(unittest.TestCase):
    def test_all_checks_pass(self):
        result = run_specs(None)
        self.assertEqual(result.returncode, 0, result.stdout)

    def test_release_version_failure_fails_specs(self):
        result = run_specs("scripts/check-release-version.sh")
        self.assertNotEqual(result.returncode, 0, result.stdout)

    def test_release_tooling_failure_fails_specs(self):
        result = run_specs("scripts/check-release-tooling.sh")
        self.assertNotEqual(result.returncode, 0, result.stdout)

    def test_vocabulary_failure_fails_specs(self):
        result = run_specs("scripts/check-comparative-vocabulary.sh")
        self.assertNotEqual(result.returncode, 0, result.stdout)


if __name__ == "__main__":
    unittest.main()
