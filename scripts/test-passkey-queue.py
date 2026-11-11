#!/usr/bin/env python3
"""Queue and QR contract tests for the two authoritative request kinds."""
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

SCRIPTS = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))
from mct_approval_qr import code, terminal
from passkey_queue import Broker, normalize_request

spec = importlib.util.spec_from_file_location("ceremony_acceptance", SCRIPTS / "test-mct-approval-ceremony.py")
acceptance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(acceptance)
CeremonyAcceptanceTests = acceptance.CeremonyAcceptanceTests


class QRAndTypeContractTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix=".mct-queue-test-", dir=Path.cwd())
        self.directory = Path(self.temp.name)
        self.directory.chmod(0o700)
        self.broker = Broker(self.directory, "https://mct-approval.nicabar.org")

    def tearDown(self):
        self.temp.cleanup()

    def test_qr_is_version_4_m_with_packet_terminal_dimensions(self):
        job = self.broker.cli({"op": "enqueue", "request": acceptance.grant_request()})
        qr = code(job["url"])
        self.assertEqual(qr.version, 4)
        self.assertEqual(qr.error, "M")
        rows = terminal(job["url"], ansi=False).splitlines()
        self.assertEqual((len(rows), len(rows[0])), (21, 41))
        self.assertTrue(all(len(row) == 41 for row in rows))
        image = self.broker.display_qr(job["display_id"], "png")
        self.assertEqual(image[:8], b"\x89PNG\r\n\x1a\n")
        self.assertEqual((int.from_bytes(image[16:20], "big"), int.from_bytes(image[20:24], "big")),
                         (328, 328))

    def test_only_grant_and_proof_envelopes_are_accepted(self):
        self.assertEqual(normalize_request(acceptance.grant_request())["kind"], "grant")
        self.assertEqual(normalize_request(acceptance.proof_request())["kind"], "proof")
        with self.assertRaises(ValueError):
            normalize_request({"kind": "registration"})
        changed = acceptance.proof_request() | {"required_group": "other"}
        with self.assertRaises(ValueError):
            normalize_request(changed)

    def test_display_capability_is_read_only_and_has_no_session_secret(self):
        job = self.broker.cli({"op": "enqueue", "request": acceptance.proof_request()})
        before = self.broker.display(job["display_id"])
        after = self.broker.display(job["display_id"])
        self.assertEqual(before["phase"], "waiting")
        self.assertEqual(after["phase"], "waiting")
        self.assertEqual(after["url"], job["url"])
        self.assertNotIn("cookie", after)
        self.assertNotIn("page_id", after)

    def test_code_capability_is_128_bit_single_use(self):
        job = self.broker.cli({"op": "enqueue", "request": acceptance.grant_request()})
        self.assertGreaterEqual(len(job["token"]), 22)
        self.broker.claim(job["token"], "nicabar")
        with self.assertRaises(ValueError):
            self.broker.claim(job["token"], "nicabar")

    def test_wrong_rp_or_origin_cannot_be_published(self):
        job = self.broker.cli({"op": "enqueue", "request": acceptance.grant_request()})
        self.broker.claim(job["token"], "nicabar")
        wrong = acceptance.prepared()
        wrong["root"] = dict(wrong["root"], rp_id="evil.example.org")
        with self.assertRaises(ValueError):
            self.broker.cli({"op": "publish", "id": job["id"], "prepared": wrong})


if __name__ == "__main__":
    unittest.main()
