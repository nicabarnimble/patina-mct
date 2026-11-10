#!/usr/bin/env python3
"""Regression tests for the committed pre-push gate."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "scripts" / "prepush.sh"
INSTALLER = ROOT / "scripts" / "install-git-hooks.sh"


class PrepushEnvironmentTests(unittest.TestCase):
    def test_hook_git_environment_cannot_capture_nested_test_commits(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / "scripts"
            scripts.mkdir()
            (scripts / "prepush.sh").write_text(
                SOURCE.read_text(encoding="utf-8"), encoding="utf-8"
            )
            (scripts / "ci-tier0.sh").write_text(
                "#!/usr/bin/env bash\n"
                "set -euo pipefail\n"
                "[[ -z ${GIT_DIR+x} ]]\n"
                "[[ -z ${GIT_WORK_TREE+x} ]]\n",
                encoding="utf-8",
            )
            (scripts / "check-deploy-together.sh").write_text(
                "#!/usr/bin/env bash\nexit 0\n", encoding="utf-8"
            )

            sentinel = root / "sentinel"
            sentinel.mkdir()
            subprocess.run(["git", "init", "-q"], cwd=sentinel, check=True)
            subprocess.run(
                ["git", "config", "user.name", "Prepush Test"],
                cwd=sentinel,
                check=True,
            )
            subprocess.run(
                ["git", "config", "user.email", "prepush@example.test"],
                cwd=sentinel,
                check=True,
            )
            (sentinel / "kept.txt").write_text("kept\n", encoding="utf-8")
            subprocess.run(["git", "add", "."], cwd=sentinel, check=True)
            subprocess.run(
                ["git", "commit", "-qm", "sentinel"], cwd=sentinel, check=True
            )
            before = subprocess.check_output(
                ["git", "rev-parse", "HEAD"], cwd=sentinel, text=True
            ).strip()

            result = subprocess.run(
                ["bash", str(scripts / "prepush.sh")],
                cwd=root,
                env={
                    **os.environ,
                    "GIT_DIR": str(sentinel / ".git"),
                    "GIT_WORK_TREE": str(sentinel),
                },
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
            )

            self.assertEqual(result.returncode, 0, result.stdout)
            after = subprocess.check_output(
                ["git", "rev-parse", "HEAD"], cwd=sentinel, text=True
            ).strip()
            self.assertEqual(after, before)
            self.assertEqual(
                subprocess.check_output(
                    ["git", "status", "--porcelain"], cwd=sentinel, text=True
                ),
                "",
            )


class HookInstallerTests(unittest.TestCase):
    def repository(self, root: Path) -> None:
        subprocess.run(["git", "init", "-q"], cwd=root, check=True)
        subprocess.run(
            ["git", "remote", "add", "origin", "git@example.test:repo.git"],
            cwd=root,
            check=True,
        )
        scripts = root / "scripts"
        scripts.mkdir()
        (scripts / "install-git-hooks.sh").write_text(
            INSTALLER.read_text(encoding="utf-8"), encoding="utf-8"
        )

    def test_installer_keeps_long_ssh_pushes_alive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.repository(root)
            subprocess.run(
                ["bash", "scripts/install-git-hooks.sh"], cwd=root, check=True
            )
            self.assertEqual(
                subprocess.check_output(
                    ["git", "config", "--local", "--get", "core.hooksPath"],
                    cwd=root,
                    text=True,
                ).strip(),
                ".githooks",
            )
            self.assertEqual(
                subprocess.check_output(
                    ["git", "config", "--local", "--get", "core.sshCommand"],
                    cwd=root,
                    text=True,
                ).strip(),
                "ssh -o ServerAliveInterval=30 -o ServerAliveCountMax=120",
            )

    def test_installer_preserves_an_existing_ssh_command(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.repository(root)
            subprocess.run(
                ["git", "config", "--local", "core.sshCommand", "custom-ssh"],
                cwd=root,
                check=True,
            )
            subprocess.run(
                ["bash", "scripts/install-git-hooks.sh"], cwd=root, check=True
            )
            self.assertEqual(
                subprocess.check_output(
                    ["git", "config", "--local", "--get", "core.sshCommand"],
                    cwd=root,
                    text=True,
                ).strip(),
                "custom-ssh",
            )


if __name__ == "__main__":
    unittest.main()
