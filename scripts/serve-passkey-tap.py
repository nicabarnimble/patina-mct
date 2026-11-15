#!/usr/bin/env python3
"""Loopback HTTPS passkey harness. Optional private assertion queue; never applies actions."""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import ssl
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parent
ASSETS = {
    "/": ("text/html; charset=utf-8", ROOT / "passkey-tap.html"),
    "/tap.mjs": ("text/javascript; charset=utf-8", ROOT / "passkey-tap.mjs"),
    "/passkey-admin.mjs": ("text/javascript; charset=utf-8", ROOT / "passkey-admin.mjs"),
}


def origin_parts(origin):
    parts = urlsplit(origin)
    if (parts.scheme != "https" or not parts.hostname or parts.username
            or parts.password or parts.path or parts.query or parts.fragment):
        raise ValueError("an HTTPS origin without path, credentials or query is required")
    return parts.netloc, parts.port if parts.port is not None else 443


def listen_parts(value):
    parts = urlsplit("//" + value)
    if (parts.hostname != "127.0.0.1" or parts.port is None
            or parts.username or parts.password or parts.path or parts.query or parts.fragment):
        raise ValueError("--listen must be 127.0.0.1:port")
    return parts.hostname, parts.port


def make_server(origin, cert, key):
    host, port = origin_parts(origin)

    class Handler(BaseHTTPRequestHandler):
        def setup(self):
            self.request.settimeout(3)
            super().setup()

        def do_GET(self):
            if self.headers.get("Host") != host:
                self.send_error(421)
                return
            asset = ASSETS.get(self.path)
            if asset is None:
                self.send_error(404)
                return
            content_type, path = asset
            content = path.read_bytes()
            self.send_response(200)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(content)))
            self.send_header("Cache-Control", "no-store")
            self.send_header("X-Content-Type-Options", "nosniff")
            self.send_header("Content-Security-Policy",
                             "default-src 'none'; script-src 'self'; "
                             "base-uri 'none'; form-action 'none'; frame-ancestors 'none'")
            self.end_headers()
            self.wfile.write(content)

        def log_message(self, *_):
            pass

    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.minimum_version = ssl.TLSVersion.TLSv1_2
    context.load_cert_chain(cert, key)
    server = ThreadingHTTPServer(("127.0.0.1", port), Handler)
    server.socket = context.wrap_socket(server.socket, server_side=True,
                                        do_handshake_on_connect=False)
    return server


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--origin", default="https://mct.localhost:8443")
    parser.add_argument("--cert", required=True)
    parser.add_argument("--key", required=True)
    parser.add_argument("--session", help="owner-private directory for the approvals-only queue")
    parser.add_argument("--listen", help="private queue listener, 127.0.0.1:port; session mode only")
    args = parser.parse_args()
    if args.session:
        from passkey_queue import Service
        service = Service(args.session, args.origin, args.cert, args.key,
                          listen=listen_parts(args.listen) if args.listen else None)
    else:
        if args.listen:
            parser.error("--listen requires --session")
        service = make_server(args.origin, args.cert, args.key)
    with service as server:
        if args.listen:
            host, port = server.server_address
            print(f"Private approval queue listening at {host}:{port} for gate origin {args.origin}. Ctrl-C to stop.",
                  flush=True)
        else:
            print(f"Open {args.origin} in a browser with 1Password enabled. Ctrl-C to stop.",
                  flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass


if __name__ == "__main__":
    main()
