#!/usr/bin/env python3
"""Regression tests for the completion gate itself (no Cargo or live services)."""
import contextlib
import copy
import importlib.util
import io
import json
from pathlib import Path
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("check_finish", Path(__file__).with_name("check-finish.py"))
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class CompletionCheckTests(unittest.TestCase):
    def setUp(self):
        self.data = json.loads(CHECK.MANIFEST.read_text())

    def test_current_manifest_is_valid(self):
        CHECK.validate(self.data)

    def test_missing_requirement_cannot_make_gate_green(self):
        self.data["requirements"].pop()
        with self.assertRaisesRegex(ValueError, "missing completion"):
            CHECK.validate(self.data)

    def test_duplicate_requirement_is_rejected(self):
        self.data["requirements"].append(copy.deepcopy(self.data["requirements"][0]))
        with self.assertRaisesRegex(ValueError, "duplicate requirement"):
            CHECK.validate(self.data)

    def test_claiming_implementation_requires_proofs_and_evidence(self):
        row = next(row for row in self.data["requirements"] if row["status"] == "open")
        row["status"] = "implemented"
        with self.assertRaisesRegex(ValueError, "lacks proof/evidence"):
            CHECK.validate(self.data)

    def test_unknown_status_is_not_completion(self):
        self.data["requirements"][0]["status"] = "done"
        with self.assertRaisesRegex(ValueError, "invalid status"):
            CHECK.validate(self.data)

    def test_unknown_proof_is_rejected(self):
        self.data["requirements"][0]["proofs"].append("missing")
        with self.assertRaisesRegex(ValueError, "unknown proof"):
            CHECK.validate(self.data)

    def test_missing_or_external_evidence_is_rejected(self):
        for path in ["missing-evidence.md", "../outside.md"]:
            self.data["requirements"][0]["evidence"] = [path]
            with self.assertRaisesRegex(ValueError, "missing or external"):
                CHECK.validate(self.data)

    def test_zero_test_filter_is_not_a_pass(self):
        with self.assertRaisesRegex(RuntimeError, "selected 0"):
            CHECK.registered_count("0 tests, 0 benchmarks\n", 1)

    def test_test_count_regression_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "selected 1"):
            CHECK.registered_count("one: test\n", 2)

    def test_selected_tests_must_all_pass_without_ignores(self):
        CHECK.verify_result("test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured;", 2)
        for passed, ignored in [(0, 0), (1, 0), (1, 1)]:
            with self.assertRaisesRegex(RuntimeError, "did not all run"):
                CHECK.verify_result(f"test result: ok. {passed} passed; 0 failed; {ignored} ignored; 0 measured;", 2)

    def test_missing_success_summary_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "exactly one"):
            CHECK.verify_result("test result: FAILED. 0 passed; 1 failed;", 1)

    def test_passing_proofs_cannot_hide_open_release_requirements(self):
        with patch.object(CHECK.sys, "argv", ["check-finish.py"]), \
             patch.object(CHECK, "run_proofs", return_value=True), \
             contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(CHECK.main(), 1)

    def test_timeout_or_interrupt_kills_only_owned_process_group(self):
        for error in [CHECK.subprocess.TimeoutExpired("cargo", 1), KeyboardInterrupt()]:
            with patch.object(CHECK.subprocess, "Popen") as spawn, \
                 patch.object(CHECK.os, "killpg") as kill, \
                 contextlib.redirect_stdout(io.StringIO()):
                process = spawn.return_value.__enter__.return_value
                process.pid = 12345
                process.communicate.side_effect = [error, ("partial output", None)]
                with self.assertRaises(type(error)):
                    CHECK.command(["cargo", "test"], timeout=1)
                self.assertTrue(spawn.call_args.kwargs["start_new_session"])
                kill.assert_called_once_with(12345, CHECK.signal.SIGKILL)

    def test_proof_package_is_allowlisted_and_defaults_to_daemon(self):
        proof = next(iter(self.data["proofs"].values()))
        for package in CHECK.PROOF_PACKAGES:
            proof["package"] = package
            CHECK.validate(self.data)
        for bad in ["--help", "other-package", None, [], "mct-kernel;exit 0"]:
            proof["package"] = bad
            with self.assertRaisesRegex(ValueError, "invalid cargo proof package"):
                CHECK.validate(self.data)
        proof.pop("package")
        CHECK.validate(self.data)

    def test_proofs_execute_the_declared_package_and_default_package(self):
        for package in [None, "mct-kernel", "mct-observation"]:
            proof = {"target": ["--lib"], "filter": "recovery::", "minimum_tests": 1}
            if package is not None:
                proof["package"] = package
            with patch.object(CHECK, "command", side_effect=[
                "recovery::one: test\n",
                "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;",
            ]) as command, contextlib.redirect_stdout(io.StringIO()):
                self.assertTrue(CHECK.run_proofs({"proofs": {"kernel": proof}}))
                expected = ["cargo", "test", "--locked", "-p", package or "mct-daemon",
                            "--lib", "recovery::"]
                self.assertEqual(command.call_args_list[0].args[0], expected + ["--", "--list"])
                self.assertEqual(command.call_args_list[1].args[0],
                                 expected + ["--", "--nocapture", "--test-threads=1"])

    def test_partial_recovery_evidence_does_not_complete_release(self):
        recovery = next(row for row in self.data["requirements"] if row["id"] == "key-recovery")
        recovery["status"] = "open"
        self.assertTrue(recovery["proofs"])
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertFalse(CHECK.inventory(self.data))

    def test_proof_cannot_disable_test_count_or_change_command(self):
        proof = next(iter(self.data["proofs"].values()))
        proof["minimum_tests"] = 0
        with self.assertRaisesRegex(ValueError, "positive minimum"):
            CHECK.validate(self.data)
        proof["minimum_tests"] = 1
        proof["target"] = ["--help"]
        with self.assertRaisesRegex(ValueError, "invalid cargo"):
            CHECK.validate(self.data)


if __name__ == "__main__":
    unittest.main()
