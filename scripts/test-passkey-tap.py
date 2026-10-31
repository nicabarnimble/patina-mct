#!/usr/bin/env python3
import http.client
import importlib.util
from pathlib import Path
import ssl
import subprocess
import tempfile
import threading
import unittest

spec = importlib.util.spec_from_file_location(
    "tap", Path(__file__).with_name("serve-passkey-tap.py"))
tap = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tap)


class TapTests(unittest.TestCase):
    def test_origin_must_be_https_and_unambiguous(self):
        for bad in ("http://mct.localhost:8443", "https://user@host", "https://host/path",
                    "https://host?redirect=elsewhere", "https://host/#fragment"):
            with self.assertRaises(ValueError):
                tap.origin_parts(bad)
        self.assertEqual(tap.origin_parts("https://mct.localhost:8443"),
                         ("mct.localhost:8443", 8443))

    def test_https_serves_only_fixed_assets_with_host_and_csp_guards(self):
        with tempfile.TemporaryDirectory() as directory:
            cert, key = Path(directory) / "cert.pem", Path(directory) / "key.pem"
            subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes",
                            "-keyout", str(key), "-out", str(cert), "-days", "1",
                            "-subj", "/CN=mct.localhost"], check=True, capture_output=True)
            # Self-signed test certificate; real browsers require a trusted cert.
            with tap.make_server("https://mct.localhost:0", cert, key) as server:
                thread = threading.Thread(target=server.serve_forever, daemon=True)
                thread.start()
                try:
                    for path, host, status in [
                        ("/", "mct.localhost:0", 200),
                        ("/tap.mjs", "mct.localhost:0", 200),
                        ("/passkey-admin.mjs", "mct.localhost:0", 200),
                        ("/../cert.pem", "mct.localhost:0", 404),
                        ("/", "evil.test", 421),
                    ]:
                        conn = http.client.HTTPSConnection("127.0.0.1", server.server_port,
                                                         context=ssl._create_unverified_context())
                        conn.request("GET", path, headers={"Host": host})
                        response = conn.getresponse()
                        self.assertEqual(response.status, status)
                        if status == 200:
                            self.assertIn("frame-ancestors 'none'",
                                          response.getheader("Content-Security-Policy"))
                        response.read()
                        conn.close()
                finally:
                    server.shutdown()
                    thread.join()


if __name__ == "__main__":
    unittest.main()
