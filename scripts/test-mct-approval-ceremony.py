#!/usr/bin/env python3
import sys
import tempfile
import time
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from passkey_queue import Broker, grant_request_from_action


ORIGIN = "https://mct-approval.nicabar.org"


def grant_request():
    return {
        "kind": "grant",
        "asker": "Unraid Grok",
        "mother": "unraid-mother",
        "permissions": [
            {"name": "Revoke day key", "description": "End access for day"},
        ],
        "grant_ends_at": "On approval",
    }


def issue_request(scopes=("status",), grant_end="2026-11-09T11:00:00Z"):
    labels = {
        "status": {"name": "Read Mother status", "description": "See current governed runtime status"},
        "key_manage": {"name": "Manage day keys", "description": "Issue and revoke bounded day-to-day keys"},
    }
    return {
        "kind": "grant",
        "asker": "Unraid Grok",
        "mother": "unraid-mother",
        "permissions": [labels[scope] for scope in scopes],
        "grant_ends_at": grant_end,
    }


def bootstrap_request():
    return {
        "kind": "grant",
        "asker": "Unraid Grok",
        "mother": "unraid-mother",
        "permissions": [
            {"name": "Establish passkey administration", "description": "Pin this MCT passkey as the independent root"},
        ],
        "grant_ends_at": "On approval",
    }


def passkey_root(fill="11", credential_id="AQID"):
    return {
        "credential_id": credential_id,
        "public_key_sec1": "04" + fill * 64,
        "rp_id": "mct-approval.nicabar.org",
        "origin": ORIGIN,
    }


def proof_request():
    return {
        "kind": "proof",
        "asker": "Unraid Grok",
        "title": "The Example Handbook",
        "site": "wiki.nicabar.org",
        "path": "/library/example-handbook.pdf",
        "why": "Checking the library release works for you (NIC-132)",
        "expected_subject": "nicabar",
        "required_group": "wiki-admin",
    }


def prepared(action=None, mother="unraid-mother"):
    return {
        "root": passkey_root(),
        "challenge": "challenge",
        "intent": {
            "mother_node_id": mother,
            "ledger_id": "ledger",
            "epoch": "epoch",
            "generation": 1,
            "authority_observation_id": "authority",
            "nonce": "a" * 64,
            "issued_at": "2026-11-09T10:00:00Z",
            "expires_at": "2026-11-09T10:05:00Z",
            "action": action or {"action": "revoke_day_key", "grant_id": "day"},
        },
    }


def assertion(value=None):
    value = value or prepared()
    return {
        "intent": value["intent"],
        "proof": {
            "scheme": "webauthn-p256-day-key/v1",
            "credential_id": value["root"]["credential_id"],
            "authenticator_data": "auth",
            "client_data_json": "client",
            "signature": "signature",
        },
    }


class CeremonyAcceptanceTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix=".mct-approval-test-", dir=Path.cwd())
        self.directory = Path(self.temp.name)
        self.directory.chmod(0o700)
        self.broker = Broker(self.directory, ORIGIN)

    def tearDown(self):
        self.temp.cleanup()

    def enqueue(self, request):
        return self.broker.cli({"op": "enqueue", "request": request})

    def test_login_grants_nothing(self):
        job = self.enqueue(grant_request())
        session = self.broker.claim(job["token"], "nicabar")
        state = self.broker.page_state(session["cookie"], session["page_id"])
        self.assertEqual(state["phase"], "opened")
        self.assertFalse(state["consumed"])
        self.assertIsNone(state["prepared"])
        self.assertFalse((self.directory / (job["id"] + ".json")).exists())

    def test_queue_session_cannot_select_another_page(self):
        first = self.enqueue(proof_request())
        second = self.enqueue(proof_request())
        first_session = self.broker.claim(first["token"], "nicabar")
        second_session = self.broker.claim(second["token"], "nicabar")
        with self.assertRaises(ValueError):
            self.broker.page_state(first_session["cookie"], second_session["page_id"])

    def test_publish_rejects_all_wysiwys_mismatches(self):
        issue_24h = {"action": "issue_day_key", "grant_id": "day-new",
                     "public_half": {"scheme": "ed25519", "public_key": "22" * 32},
                     "scopes": ["status", "key_manage"],
                     "expires_at": "2026-11-10T10:00:00Z"}
        mismatches = [
            (issue_request(("status",), "2026-11-09T11:00:00Z"), prepared(issue_24h)),
            (grant_request(), prepared(issue_24h)),
            (bootstrap_request(), prepared({"action": "revoke_day_key", "grant_id": "day"})),
        ]
        for displayed, exact in mismatches:
            with self.subTest(displayed=displayed["permissions"][0]["name"]):
                job = self.enqueue(displayed)
                self.broker.claim(job["token"], "nicabar")
                with self.assertRaisesRegex(ValueError, "displayed request"):
                    self.broker.cli({"op": "publish", "id": job["id"], "prepared": exact})

    def test_matching_issue_receipt_is_derived_from_the_signed_action(self):
        action = {"action": "issue_day_key", "grant_id": "day-new",
                  "public_half": {"scheme": "ed25519", "public_key": "22" * 32},
                  "scopes": ["status", "key_manage"],
                  "expires_at": "2026-11-10T10:00:00Z"}
        displayed = grant_request_from_action("Unraid Grok", "unraid-mother", action)
        self.assertEqual(displayed["grant_ends_at"], action["expires_at"])
        self.assertIn("day-new", displayed["permissions"][0]["description"])
        self.assertRegex(displayed["permissions"][0]["description"], r"sha256:[0-9a-f]{64}$")
        job = self.enqueue(displayed)
        self.broker.claim(job["token"], "nicabar")
        result = self.broker.cli({"op": "publish", "id": job["id"], "prepared": prepared(action)})
        self.assertEqual(result["phase"], "prepared")

    def test_publish_rejects_mutations_of_a_real_derived_receipt(self):
        action = {"action": "issue_day_key", "grant_id": "day-new",
                  "public_half": {"scheme": "ed25519", "public_key": "22" * 32},
                  "scopes": ["status", "key_manage"],
                  "expires_at": "2026-11-10T10:00:00Z"}
        displayed = grant_request_from_action("Unraid Grok", "unraid-mother", action)
        mutations = [
            ("grant id", prepared(dict(action, grant_id="another-day"))),
            ("day key public half", prepared(dict(
                action, public_half={"scheme": "ed25519", "public_key": "33" * 32}))),
            ("scopes", prepared(dict(action, scopes=["status"]))),
            ("Mother", prepared(action, mother="another-mother")),
        ]
        for name, changed in mutations:
            with self.subTest(name=name):
                job = self.enqueue(displayed)
                self.broker.claim(job["token"], "nicabar")
                with self.assertRaisesRegex(ValueError, "displayed request"):
                    self.broker.cli({"op": "publish", "id": job["id"], "prepared": changed})

    def test_publish_rejects_bootstrap_root_swap(self):
        action = {"action": "bootstrap", "root": passkey_root()}
        displayed = grant_request_from_action("Unraid Grok", "unraid-mother", action)
        changed = {"action": "bootstrap", "root": passkey_root("22", "BAUG"),
                   "inventory_hash": "33" * 32}
        job = self.enqueue(displayed)
        self.broker.claim(job["token"], "nicabar")
        with self.assertRaisesRegex(ValueError, "displayed request"):
            self.broker.cli({"op": "publish", "id": job["id"], "prepared": prepared(changed)})

    def test_publish_rejects_unknown_signed_action_fields(self):
        action = {"action": "issue_day_key", "grant_id": "day-new",
                  "public_half": {"scheme": "ed25519", "public_key": "22" * 32},
                  "scopes": ["status"], "expires_at": "2026-11-10T10:00:00Z"}
        displayed = grant_request_from_action("Unraid Grok", "unraid-mother", action)
        job = self.enqueue(displayed)
        self.broker.claim(job["token"], "nicabar")
        with self.assertRaisesRegex(ValueError, "displayed request"):
            self.broker.cli({"op": "publish", "id": job["id"],
                             "prepared": prepared(dict(action, unexpected=True))})

    def test_publish_rejects_revoke_with_extra_field(self):
        action = {"action": "revoke_day_key", "grant_id": "day"}
        displayed = grant_request_from_action("Unraid Grok", "unraid-mother", action)
        job = self.enqueue(displayed)
        self.broker.claim(job["token"], "nicabar")
        with self.assertRaisesRegex(ValueError, "displayed request"):
            self.broker.cli({"op": "publish", "id": job["id"],
                             "prepared": prepared(dict(action, unexpected=True))})

    def test_duplicate_scopes_are_not_a_grant_action(self):
        action = {"action": "issue_day_key", "grant_id": "day-new",
                  "public_half": {"scheme": "ed25519", "public_key": "22" * 32},
                  "scopes": ["status", "status"], "expires_at": "2026-11-10T10:00:00Z"}
        with self.assertRaisesRegex(ValueError, "day-key scopes"):
            grant_request_from_action("Unraid Grok", "unraid-mother", action)

    def test_tampered_grant_is_refused_and_consumed(self):
        job = self.enqueue(grant_request())
        session = self.broker.claim(job["token"], "nicabar")
        exact = prepared()
        self.broker.cli({"op": "publish", "id": job["id"], "prepared": exact})
        changed = assertion(exact)
        changed["intent"] = dict(changed["intent"], generation=2)
        result = self.broker.complete(session["cookie"], session["page_id"], changed)
        self.assertEqual(result["phase"], "refused")
        self.assertTrue(result["consumed"])
        with self.assertRaises(ValueError):
            self.broker.complete(session["cookie"], session["page_id"], assertion(exact))

    def test_wrong_credential_is_refused_and_consumed(self):
        job = self.enqueue(grant_request())
        session = self.broker.claim(job["token"], "nicabar")
        exact = prepared()
        self.broker.cli({"op": "publish", "id": job["id"], "prepared": exact})
        changed = assertion(exact)
        changed["proof"]["credential_id"] = "wrong-credential"
        result = self.broker.complete(session["cookie"], session["page_id"], changed)
        self.assertEqual(result["phase"], "refused")
        self.assertTrue(result["consumed"])

    def test_valid_grant_is_accepted_exactly_once(self):
        job = self.enqueue(grant_request())
        session = self.broker.claim(job["token"], "nicabar")
        exact = prepared()
        self.broker.cli({"op": "publish", "id": job["id"], "prepared": exact})
        signed = self.broker.complete(session["cookie"], session["page_id"], assertion(exact))
        self.assertEqual(signed["phase"], "signed")
        self.assertTrue(signed["consumed"])
        path = self.broker.cli({"op": "take", "id": job["id"]})["path"]
        self.assertEqual(Path(path), self.directory / (job["id"] + ".json"))
        settled = self.broker.cli({"op": "settle", "id": job["id"], "outcome": "approved"})
        self.assertEqual(settled["phase"], "approved")
        with self.assertRaises(ValueError):
            self.broker.cli({"op": "take", "id": job["id"]})
        with self.assertRaises(ValueError):
            self.broker.complete(session["cookie"], session["page_id"], assertion(exact))

    def test_failed_proof_consumes_code(self):
        job = self.enqueue(proof_request())
        session = self.broker.claim(job["token"], "nicabar")
        failed = self.broker.proof_result(session["cookie"], session["page_id"], {
            "outcome": "fail", "reason": "denied", "subject": "nicabar",
            "path": proof_request()["path"], "nonce": job["nonce"],
        })
        self.assertEqual(failed["phase"], "failed")
        self.assertTrue(failed["consumed"])
        with self.assertRaises(ValueError):
            self.broker.proof_result(session["cookie"], session["page_id"], {
                "outcome": "pass", "reason": "ok", "subject": "nicabar",
                "path": proof_request()["path"], "nonce": job["nonce"],
            })

    def test_expired_code_cannot_be_claimed(self):
        job = self.enqueue(grant_request())
        self.broker.jobs[job["id"]].deadline = time.time() - 1
        status = self.broker.cli({"op": "status", "id": job["id"]})
        self.assertEqual(status["phase"], "expired")
        with self.assertRaises(ValueError):
            self.broker.claim(job["token"], "nicabar")


if __name__ == "__main__":
    unittest.main()
