"""Local assertion transport. This module never invokes Mother or a subprocess."""
from dataclasses import dataclass
from enum import Enum
from http.cookies import SimpleCookie
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import json
import os
import secrets
import socket
import ssl
import stat
import threading
import time
from urllib.parse import urlsplit

LIMIT = 96 * 1024
ROOT = Path(__file__).resolve().parent


def loads(data):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError("duplicate field")
            result[key] = value
        return result
    def nonfinite(_):
        raise ValueError("nonfinite number")
    return json.loads(data, object_pairs_hook=pairs, parse_constant=nonfinite)


def encoded(value):
    data = json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()
    if len(data) > LIMIT:
        raise ValueError("message exceeds 96 KiB")
    return data


def private_directory(path):
    path = Path(os.path.abspath(path))
    for parent in [*reversed(path.parents), path]:
        info = parent.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid not in (0, os.getuid()):
            raise ValueError("untrusted directory")
        if info.st_mode & 0o022 and not (info.st_uid == 0 and info.st_mode & stat.S_ISVTX):
            raise ValueError("writable ancestry")
    info = path.stat()
    if info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError("session must be owner-private")
    return path


def write_new(path, value):
    private_directory(Path(path).parent)
    data = encoded(value)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(data)
        stream.flush()
        os.fsync(stream.fileno())


def read_private(path):
    private_directory(Path(path).parent)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, "rb") as stream:
        info = os.fstat(stream.fileno())
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid()
                or info.st_nlink != 1 or info.st_mode & 0o077 or info.st_size > LIMIT):
            raise ValueError("unsafe private file")
        data = stream.read(LIMIT + 1)
        if len(data) > LIMIT:
            raise ValueError("private file exceeds 96 KiB")
        return loads(data)


class Phase(str, Enum):
    QUEUED = "queued"
    READY = "ready"
    PREPARED = "prepared"
    OFFERED = "offered"
    SIGNED = "signed"
    TAKEN = "taken"
    FAILED = "failed"


@dataclass
class Job:
    id: str
    kind: str
    name: str
    deadline: float
    phase: Phase = Phase.QUEUED
    prepared: dict | None = None

    def view(self):
        return {"id": self.id, "kind": self.kind, "name": self.name,
                "phase": self.phase, "prepared": self.prepared}


class Broker:
    def __init__(self, directory, origin):
        self.directory = private_directory(directory)
        parts = urlsplit(origin)
        if (parts.scheme != "https" or parts.hostname != "mct.localhost"
                or parts.username or parts.password or parts.path or parts.query or parts.fragment):
            raise ValueError("expected https://mct.localhost:PORT")
        self.origin = origin
        self.host = parts.netloc
        self.jobs = {}
        self.lock = threading.Lock()
        self.page_token = None
        self.token_deadline = 0
        self.cookie = None

    def expire(self):
        for job in self.jobs.values():
            if time.monotonic() >= job.deadline and job.phase not in (Phase.TAKEN, Phase.FAILED):
                job.phase = Phase.FAILED

    def cli(self, request):
        with self.lock:
            self.expire()
            op = request["op"]
            if op == "open":
                self.page_token = secrets.token_urlsafe(32)
                self.token_deadline = time.monotonic() + 120
                return {"url": self.origin + "/#" + self.page_token}
            if op == "enqueue":
                self.jobs = {k: j for k, j in self.jobs.items()
                             if j.phase not in (Phase.TAKEN, Phase.FAILED)}
                if len(self.jobs) >= 8 or request["kind"] not in ("registration", "assertion"):
                    raise ValueError("queue full or invalid kind")
                name = request.get("name", "Doug")
                if not isinstance(name, str) or not 1 <= len(name) <= 64:
                    raise ValueError("invalid name")
                job = Job(secrets.token_hex(32), request["kind"], name, time.monotonic() + 3600)
                self.jobs[job.id] = job
                return job.view()
            job = self.jobs[request["id"]]
            if op == "status":
                return job.view()
            if op == "cancel" and job.phase not in (Phase.TAKEN, Phase.FAILED):
                job.phase = Phase.FAILED
                return job.view()
            if op == "publish" and job.kind == "assertion" and job.phase == Phase.READY:
                prepared = request["prepared"]
                encoded(prepared)
                root = prepared["root"]
                if (root["origin"] != self.origin or root["rp_id"] != "mct.localhost"
                        or prepared["intent"]["action"]["action"] not in
                        ("bootstrap", "issue_day_key", "revoke_day_key")):
                    raise ValueError("wrong root or action")
                encoded({"result": dict(job.view(), prepared=prepared, phase=Phase.PREPARED)})
                job.prepared = prepared
                job.phase = Phase.PREPARED
                return job.view()
            if op == "take" and job.phase == Phase.SIGNED:
                job.phase = Phase.TAKEN
                return {"path": str(self.directory / (job.id + ".json"))}
            raise ValueError("invalid CLI transition")

    def authenticate(self, token):
        with self.lock:
            if (not isinstance(token, str) or self.page_token is None
                    or time.monotonic() >= self.token_deadline
                    or not secrets.compare_digest(token, self.page_token)):
                raise ValueError("invalid page capability")
            self.page_token = None
            self.cookie = secrets.token_urlsafe(32)
            return self.cookie

    def page(self, op, request, cookie):
        with self.lock:
            if not self.cookie or not secrets.compare_digest(cookie, self.cookie):
                raise ValueError("unauthenticated page")
            self.expire()
            active = next((j for j in self.jobs.values()
                           if j.phase in (Phase.QUEUED, Phase.READY, Phase.PREPARED, Phase.OFFERED)), None)
            if op == "next":
                if active is None or active.phase == Phase.OFFERED:
                    return None
                if active.phase == Phase.QUEUED:
                    active.phase = Phase.READY
                if active.kind == "registration" or active.phase == Phase.PREPARED:
                    active.phase = Phase.OFFERED
                return active.view()
            job = self.jobs[request["id"]]
            if job is not active:
                raise ValueError("not the active operation")
            if op == "fail":
                job.phase = Phase.FAILED
                return job.view()
            if op != "complete":
                raise ValueError("unknown page operation")
            result = request["result"]
            encoded(result)
            if job.kind == "registration" and job.phase == Phase.OFFERED:
                if (set(result) != {"rp_id", "origin", "credential_id", "public_key_sec1"}
                        or result["origin"] != self.origin or result["rp_id"] != "mct.localhost"):
                    raise ValueError("wrong registration origin")
            elif job.kind == "assertion" and job.phase == Phase.OFFERED:
                if (set(result) != {"intent", "proof"}
                        or encoded(result["intent"]) != encoded(job.prepared["intent"])
                        or result["proof"]["scheme"] != "webauthn-p256-day-key/v1"
                        or result["proof"]["credential_id"] != job.prepared["root"]["credential_id"]):
                    raise ValueError("assertion substitution")
            else:
                raise ValueError("invalid page transition")
            write_new(self.directory / (job.id + ".json"), result)
            job.phase = Phase.SIGNED
            return job.view()


def receive(stream):
    line = stream.readline(LIMIT + 2)
    if not line.endswith(b"\n") or len(line) > LIMIT + 1:
        raise ValueError("invalid frame")
    return loads(line)


def rpc(directory, request):
    directory = private_directory(directory)
    path = directory / "control.sock"
    info = path.lstat()
    if not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError("unsafe control socket")
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(5)
        client.connect(str(path))
        client.sendall(encoded(request) + b"\n")
        with client.makefile("rb") as stream:
            response = receive(stream)
    if "error" in response:
        raise ValueError("broker refused operation")
    return response["result"]


class BoundedHTTPServer(ThreadingHTTPServer):
    daemon_threads = True
    request_queue_size = 16

    def __init__(self, *args):
        self.slots = threading.BoundedSemaphore(16)
        super().__init__(*args)

    def process_request(self, request, address):
        if not self.slots.acquire(blocking=False):
            self.shutdown_request(request)
            return
        try:
            super().process_request(request, address)
        except BaseException:
            self.slots.release()
            raise

    def process_request_thread(self, request, address):
        try:
            super().process_request_thread(request, address)
        finally:
            self.slots.release()

    def handle_error(self, *_):
        pass


class Service:
    def __init__(self, directory, origin, cert, key):
        self.broker = Broker(directory, origin)
        broker = self.broker
        assets = {"/": ("text/html", "passkey-approval.html"),
                  "/approval.mjs": ("text/javascript", "passkey-approval.mjs"),
                  "/passkey-admin.mjs": ("text/javascript", "passkey-admin.mjs")}

        class Handler(BaseHTTPRequestHandler):
            def setup(self):
                self.request.settimeout(5)
                self.timer = threading.Timer(5, self.abort)
                self.timer.daemon = True
                self.timer.start()
                super().setup()

            def abort(self):
                try:
                    self.request.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass

            def finish(self):
                try:
                    super().finish()
                finally:
                    self.timer.cancel()

            def reply(self, status, value, content_type="application/json", cookie=None):
                data = value if isinstance(value, bytes) else encoded(value)
                self.send_response(status)
                self.send_header("Content-Type", content_type + "; charset=utf-8")
                self.send_header("Content-Length", str(len(data)))
                self.send_header("Cache-Control", "no-store")
                self.send_header("Connection", "close")
                self.send_header("X-Content-Type-Options", "nosniff")
                self.send_header("Referrer-Policy", "no-referrer")
                self.send_header("Cross-Origin-Opener-Policy", "same-origin")
                self.send_header("Content-Security-Policy", "default-src 'none'; script-src 'self'; "
                                 "connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'")
                if cookie:
                    self.send_header("Set-Cookie", "__Host-mct=" + cookie + "; Path=/; Secure; HttpOnly; SameSite=Strict")
                self.end_headers()
                self.wfile.write(data)

            def host_ok(self):
                return self.headers.get_all("Host") == [broker.host]

            def do_GET(self):
                if not self.host_ok():
                    return self.reply(421, {"error": "wrong host"})
                if self.path not in assets:
                    return self.reply(404, {"error": "not found"})
                mime, name = assets[self.path]
                self.reply(200, (ROOT / name).read_bytes(), mime)

            def do_POST(self):
                try:
                    if (not self.host_ok() or self.headers.get_all("Origin") != [broker.origin]
                            or self.headers.get("Content-Type") != "application/json"
                            or self.headers.get("Transfer-Encoding") is not None
                            or len(self.headers.get_all("Content-Length", [])) != 1):
                        raise ValueError("invalid request headers")
                    size = int(self.headers["Content-Length"])
                    if not 0 < size <= LIMIT:
                        raise ValueError("invalid body size")
                    body = self.rfile.read(size)
                    if len(body) != size:
                        raise ValueError("incomplete body")
                    request = loads(body)
                    if self.path == "/api/session":
                        cookie = broker.authenticate(request["token"])
                        return self.reply(200, {}, cookie=cookie)
                    cookies = SimpleCookie(self.headers.get("Cookie", ""))
                    cookie = cookies["__Host-mct"].value if "__Host-mct" in cookies else ""
                    if self.path not in ("/api/next", "/api/complete", "/api/fail"):
                        raise ValueError("unknown route")
                    result = broker.page(self.path.removeprefix("/api/"), request, cookie)
                    self.reply(200, result)
                except (ValueError, KeyError, TypeError, RecursionError, OSError):
                    self.reply(400, {"error": "request refused"})

            def log_message(self, *_):
                pass

        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.minimum_version = ssl.TLSVersion.TLSv1_2
        context.load_cert_chain(cert, key)
        port = urlsplit(origin).port
        self.http = BoundedHTTPServer(("127.0.0.1", 443 if port is None else port), Handler)
        self.http.socket = context.wrap_socket(self.http.socket, server_side=True, do_handshake_on_connect=False)
        self.control = socket.socket(socket.AF_UNIX)
        self.socket_path = broker.directory / "control.sock"
        try:
            self.control.bind(str(self.socket_path))
            os.chmod(self.socket_path, 0o600)
            self.control.listen(8)
            self.control.settimeout(0.2)
        except BaseException:
            self.control.close()
            self.http.server_close()
            raise
        self.stopped = threading.Event()
        self.thread = threading.Thread(target=self.control_loop, daemon=True)

    def control_loop(self):
        while not self.stopped.is_set():
            try:
                client, _ = self.control.accept()
            except TimeoutError:
                continue
            except OSError:
                return
            with client:
                client.settimeout(5)
                try:
                    with client.makefile("rb") as stream:
                        request = receive(stream)
                    result = {"result": self.broker.cli(request)}
                except (ValueError, KeyError, TypeError, RecursionError, OSError):
                    result = {"error": "request refused"}
                try:
                    client.sendall(encoded(result) + b"\n")
                except OSError:
                    pass

    def __enter__(self):
        self.thread.start()
        return self.http

    def __exit__(self, *_):
        self.stopped.set()
        self.control.close()
        self.thread.join(6)
        self.http.server_close()
        # Keep the socket and signed artifacts. A fresh session is required after
        # shutdown so an interrupted caller cannot attach to a different broker.
