#!/usr/bin/env python3
"""Local L17/L18 policy regressions; upstream provenance lives in the evidence."""
from pathlib import Path
import re
import tomllib
import unittest

ROOT = Path(__file__).resolve().parent.parent


class LowSecurityPolicyTests(unittest.TestCase):
    def test_l17_yank_checks_are_enabled_for_both_retained_graphs(self):
        policy = tomllib.loads((ROOT / ".cargo/audit.toml").read_text())
        self.assertTrue(policy["yanked"]["enabled"])
        self.assertIn("warnings", policy["output"]["deny"])
        bad = {("spin", "0.10.0"), ("chacha20", "0.10.0"),
               ("chacha20", "0.10.1"), ("der", "0.8.0")}
        for lock in ("Cargo.lock", "fuzz/Cargo.lock"):
            packages = tomllib.loads((ROOT / lock).read_text())["package"]
            identities = {(p["name"], p["version"]) for p in packages}
            self.assertFalse(identities & bad, lock)
            self.assertTrue({"spin", "chacha20", "der"} <= {p["name"] for p in packages})

    def test_l18_rust_setup_is_immutable_with_explicit_checked_compiler(self):
        action = (ROOT / ".github/actions/setup-rust/action.yml").read_text()
        refs = re.findall(r"uses: dtolnay/rust-toolchain@([^\s]+)", action)
        self.assertEqual(len(refs), 1)
        self.assertRegex(refs[0], r"^[0-9a-f]{40}$")
        compiler = tomllib.loads((ROOT / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
        self.assertEqual(compiler, "1.96.0")
        self.assertIn("toolchain: " + compiler, action)
        self.assertIn("components: rustfmt, clippy", action)
        self.assertIn('grep -q "^rustc ${pinned} "', action)


if __name__ == "__main__":
    unittest.main()
