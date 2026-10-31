#!/usr/bin/env python3
import http.client
import json
from pathlib import Path
import ssl
import subprocess
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

from passkey_queue import Broker, LIMIT, Service, loads, read_private, rpc

ORIGIN = "https://mct.localhost:18443"


def prepared():
    return {"root": {"origin": ORIGIN, "rp_id": "mct.localhost", "credential_id": "fixture"},
            "challenge": "fixture", "intent": {"nonce": "fixture", "action": {"action": "bootstrap"}}}


def signed(value):
    return {"intent": value["intent"], "proof": {"scheme": "webauthn-p256-day-key/v1",
            "credential_id": "fixture", "signature": "not-cryptographically-verified-by-broker"}}


class QueueTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.broker = Broker(self.directory.name, ORIGIN)
        token = self.broker.cli({"op": "open"})["url"].split("#")[1]
        self.cookie = self.broker.authenticate(token)

    def queue(self):
        return self.broker.cli({"op": "enqueue", "kind": "assertion"})["id"]

    def page(self, op, **args):
        return self.broker.page(op, args, self.cookie)

    def test_queue_requires_foreground_claim_before_prepare_and_single_take(self):
        job_id = self.queue()
        self.assertEqual(self.broker.cli({"op": "status", "id": job_id})["phase"], "queued")
        with self.assertRaises(ValueError):
            self.broker.cli({"op": "publish", "id": job_id, "prepared": prepared()})
        self.assertEqual(self.page("next")["phase"], "ready")
        self.broker.cli({"op": "publish", "id": job_id, "prepared": prepared()})
        with self.assertRaises(ValueError):
            self.page("complete", id=job_id, result=signed(prepared()))
        self.assertEqual(self.page("next")["phase"], "offered")
        self.assertIsNone(self.page("next"), "a second tab must not receive the same ceremony")
        self.page("complete", id=job_id, result=signed(prepared()))
        with self.assertRaises(ValueError):
            self.page("complete", id=job_id, result=signed(prepared()))
        path = Path(self.broker.cli({"op": "take", "id": job_id})["path"])
        self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        self.assertEqual(read_private(path), signed(prepared()))
        with self.assertRaises(ValueError):
            self.broker.cli({"op": "take", "id": job_id})

    def test_page_cannot_enqueue_publish_substitute_or_complete_unclaimed_work(self):
        job_id = self.queue()
        for op in ("enqueue", "publish", "complete"):
            with self.assertRaises(ValueError):
                self.page(op, id=job_id, result=signed(prepared()))
        self.page("next")
        self.broker.cli({"op": "publish", "id": job_id, "prepared": prepared()})
        self.page("next")
        wrong = signed(prepared())
        wrong["intent"]["nonce"] = "substitution"
        with self.assertRaises(ValueError):
            self.page("complete", id=job_id, result=wrong)
        wrong = signed(prepared())
        wrong["proof"]["credential_id"] = "other-root"
        with self.assertRaises(ValueError):
            self.page("complete", id=job_id, result=wrong)
        self.assertFalse(list(Path(self.directory.name).glob("*.json")))

    def test_one_use_page_token_expiry_cookie_rotation_and_capacity(self):
        token = self.broker.cli({"op": "open"})["url"].split("#")[1]
        self.broker.authenticate(token)
        with self.assertRaises(ValueError):
            self.broker.authenticate(token)
        with self.assertRaises(ValueError):
            self.page("next")
        token = self.broker.cli({"op": "open"})["url"].split("#")[1]
        with patch("passkey_queue.time.monotonic", return_value=time.monotonic() + 121):
            with self.assertRaises(ValueError):
                self.broker.authenticate(token)
        for _ in range(8):
            self.queue()
        with self.assertRaises(ValueError):
            self.queue()
        with patch("passkey_queue.time.monotonic", return_value=time.monotonic() + 3601):
            self.queue()
            self.assertEqual(len(self.broker.jobs), 1)

    def test_duplicate_json_and_unsafe_sessions_are_refused(self):
        with self.assertRaises(ValueError):
            loads('{"op":"next","op":"enqueue"}')
        with self.assertRaises(ValueError):
            loads('{"value":NaN}')
        link = Path(self.directory.name) / "alias"
        link.symlink_to(self.directory.name)
        with self.assertRaises(ValueError):
            Broker(link, ORIGIN)

    def test_cancelled_or_expired_offer_cannot_be_completed_or_taken(self):
        for expire in (False, True):
            job_id = self.queue()
            self.page("next")
            self.broker.cli({"op": "publish", "id": job_id, "prepared": prepared()})
            self.page("next")
            if expire:
                with patch("passkey_queue.time.monotonic", return_value=time.monotonic() + 3601):
                    self.assertEqual(self.broker.cli({"op": "status", "id": job_id})["phase"], "failed")
            else:
                self.broker.cli({"op": "cancel", "id": job_id})
            with self.assertRaises(ValueError):
                self.page("complete", id=job_id, result=signed(prepared()))
            with self.assertRaises(ValueError):
                self.broker.cli({"op": "take", "id": job_id})
        self.assertFalse(list(Path(self.directory.name).glob("*.json")))

    def test_existing_artifact_is_never_overwritten(self):
        job_id = self.queue()
        self.page("next")
        self.broker.cli({"op": "publish", "id": job_id, "prepared": prepared()})
        self.page("next")
        path = Path(self.directory.name) / (job_id + ".json")
        path.write_text("existing evidence")
        with self.assertRaises(FileExistsError):
            self.page("complete", id=job_id, result=signed(prepared()))
        self.assertEqual(path.read_text(), "existing evidence")


class HTTPTests(unittest.TestCase):
    def test_https_auth_origin_host_route_and_body_guards(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            cert, key = directory / "cert.pem", directory / "key.pem"
            subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
                            "-keyout", str(key), "-out", str(cert), "-days", "1",
                            "-subj", "/CN=mct.localhost"], check=True, capture_output=True)
            service = Service(directory, "https://mct.localhost:0", cert, key)
            with service as server:
                thread = threading.Thread(target=server.serve_forever, daemon=True)
                thread.start()
                origin = "https://mct.localhost:0"
                def post(route, data, **headers):
                    connection = http.client.HTTPSConnection("127.0.0.1", server.server_port,
                                                            context=ssl._create_unverified_context())
                    merged = {"Host": "mct.localhost:0", "Origin": origin, "Content-Type": "application/json"}
                    merged.update(headers)
                    connection.request("POST", route, json.dumps(data), merged)
                    response = connection.getresponse()
                    result = response.status, response.getheaders(), response.read()
                    connection.close()
                    return result
                try:
                    self.assertEqual(server.server_address[0], "127.0.0.1")
                    self.assertEqual(post("/api/next", {})[0], 400)
                    url = rpc(directory, {"op": "open"})["url"]
                    status, headers, _ = post("/api/session", {"token": url.split("#")[1]})
                    self.assertEqual(status, 200)
                    cookie = dict(headers)["Set-Cookie"]
                    self.assertIn("Secure; HttpOnly; SameSite=Strict", cookie)
                    self.assertEqual(post("/api/next", {}, Cookie=cookie)[0], 200)
                    for route, data, headers in [
                        ("/api/next", {}, {"Host": "evil.test"}),
                        ("/api/next", {}, {"Origin": "https://evil.test"}),
                        ("/api/enqueue", {"kind": "assertion"}, {}),
                        ("/api/next", {}, {"Content-Length": str(LIMIT + 1)}),
                        ("/api/next", {}, {"Content-Type": "text/plain"}),
                    ]:
                        self.assertEqual(post(route, data, Cookie=cookie, **headers)[0], 400)
                    self.assertFalse(service.broker.jobs)
                finally:
                    server.shutdown()
                    thread.join()


if __name__ == "__main__":
    unittest.main()
