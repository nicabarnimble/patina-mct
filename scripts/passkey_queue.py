"""Loopback-only approval queue. It stores no MCT root private key and invokes no subprocess."""
from dataclasses import dataclass, field
from enum import Enum
from http.cookies import SimpleCookie
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import hashlib
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
CODE_SECONDS = 10 * 60
ROOT = Path(__file__).resolve().parent
CODE_ALPHABET = "23456789ABCDEFGHJKLMNPQRSTUVWXYZ"


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


def text(value, name, maximum=256):
    if not isinstance(value, str) or not value or len(value) > maximum or any(ord(c) < 0x20 for c in value):
        raise ValueError("invalid " + name)
    return value


def exact(value, keys):
    return isinstance(value, dict) and set(value) == set(keys)


GRANT_SCOPE_PRESENTATIONS = {
    "status": {"name": "Read Mother status", "description": "See current governed runtime status"},
    "key_manage": {"name": "Manage day keys", "description": "Issue and revoke bounded day-to-day keys"},
}


def grant_request_from_action(asker, mother, action):
    if not isinstance(action, dict):
        raise ValueError("invalid grant action")
    kind = action.get("action")
    if kind == "bootstrap":
        root = action.get("root")
        if not isinstance(root, dict):
            raise ValueError("invalid passkey root")
        fingerprint = hashlib.sha256(encoded(root)).hexdigest()[:16]
        permissions = [{"name": "Establish passkey administration",
                        "description": "Trust passkey root sha256:" + fingerprint + " as the independent administrator"}]
        grant_end = "On approval"
    elif kind == "issue_day_key":
        grant_id = text(action.get("grant_id"), "grant id", 128)
        scopes = action.get("scopes")
        if (not isinstance(scopes, list) or not scopes
                or any(not isinstance(scope, str) for scope in scopes)
                or len(scopes) != len(set(scopes))
                or any(scope not in GRANT_SCOPE_PRESENTATIONS for scope in scopes)):
            raise ValueError("invalid day-key scopes")
        public_half = action.get("public_half")
        if not isinstance(public_half, dict):
            raise ValueError("invalid day-key public half")
        fingerprint = hashlib.sha256(encoded(public_half)).hexdigest()[:16]
        permissions = [{"name": "Issue day key",
                        "description": "Issue grant " + grant_id + " to key sha256:" + fingerprint}]
        permissions.extend(dict(GRANT_SCOPE_PRESENTATIONS[scope]) for scope in scopes)
        grant_end = text(action.get("expires_at"), "grant end", 64)
    elif kind == "revoke_day_key":
        grant_id = text(action.get("grant_id"), "grant id", 128)
        permissions = [{"name": "Revoke day key", "description": "End access for " + grant_id}]
        grant_end = "On approval"
    else:
        raise ValueError("unsupported grant action")
    return normalize_request({"kind": "grant", "asker": asker, "mother": mother,
                              "permissions": permissions, "grant_ends_at": grant_end})


def normalize_request(value):
    if not isinstance(value, dict) or value.get("kind") not in ("grant", "proof"):
        raise ValueError("invalid request kind")
    if value["kind"] == "grant":
        if not exact(value, ("kind", "asker", "mother", "permissions", "grant_ends_at")):
            raise ValueError("invalid grant request")
        permissions = value["permissions"]
        if not isinstance(permissions, list) or not 1 <= len(permissions) <= 12:
            raise ValueError("invalid permissions")
        normalized = []
        for item in permissions:
            if not exact(item, ("name", "description")):
                raise ValueError("invalid permission")
            normalized.append({"name": text(item["name"], "permission", 96),
                               "description": text(item["description"], "permission description", 256)})
        return {"kind": "grant", "asker": text(value["asker"], "asker", 64),
                "mother": text(value["mother"], "Mother", 128), "permissions": normalized,
                "grant_ends_at": text(value["grant_ends_at"], "grant end", 64)}
    if not exact(value, ("kind", "asker", "title", "site", "path", "why",
                         "expected_subject", "required_group")):
        raise ValueError("invalid proof request")
    path = text(value["path"], "proof path", 512)
    if (not path.startswith("/library/") or "?" in path or "#" in path or "\\" in path
            or "%" in path or "//" in path or ".." in path.split("/")):
        raise ValueError("invalid proof path")
    site = text(value["site"], "proof site", 253)
    if site != "wiki.nicabar.org":
        raise ValueError("proof site is not allowlisted")
    if value["expected_subject"] != "nicabar" or value["required_group"] != "wiki-admin":
        raise ValueError("proof identity contract changed")
    return {"kind": "proof", "asker": text(value["asker"], "asker", 64),
            "title": text(value["title"], "document title", 160), "site": site, "path": path,
            "why": text(value["why"], "proof reason", 256), "expected_subject": "nicabar",
            "required_group": "wiki-admin"}


class Phase(str, Enum):
    WAITING = "waiting"
    OPENED = "opened"
    PREPARED = "prepared"
    SIGNED = "signed"
    APPROVED = "approved"
    DECLINED = "declined"
    PASSED = "passed"
    FAILED = "failed"
    SKIPPED = "skipped"
    EXPIRED = "expired"
    REFUSED = "refused"


TERMINAL = {Phase.APPROVED, Phase.DECLINED, Phase.PASSED, Phase.FAILED,
            Phase.SKIPPED, Phase.EXPIRED, Phase.REFUSED}


@dataclass
class Job:
    id: str
    token: str
    display_id: str
    code: str
    request: dict
    created_at: float
    deadline: float
    nonce: str
    phase: Phase = Phase.WAITING
    prepared: dict | None = None
    consumed: bool = False
    cookie: str | None = None
    page_id: str | None = None
    subject: str | None = None
    outcome: str | None = None
    reason: str | None = None
    decided_at: float | None = None
    taken: bool = False
    revision: int = 0

    def public(self, details=True):
        value = {"id": self.id, "display_id": self.display_id, "code": self.code,
                 "kind": self.request["kind"], "phase": self.phase.value,
                 "created_at": self.created_at, "deadline": self.deadline,
                 "remaining_seconds": max(0, int(self.deadline - time.time())),
                 "consumed": self.consumed, "outcome": self.outcome, "reason": self.reason,
                 "decided_at": self.decided_at, "revision": self.revision}
        if details:
            value.update({"request": self.request, "prepared": self.prepared,
                          "subject": self.subject, "nonce": self.nonce})
        return value


class Broker:
    def __init__(self, directory, origin):
        self.directory = private_directory(directory)
        parts = urlsplit(origin)
        if (parts.scheme != "https" or not parts.hostname or parts.username or parts.password
                or parts.path or parts.query or parts.fragment):
            raise ValueError("expected an HTTPS origin without a path")
        self.origin = origin
        self.host = parts.netloc
        self.rp_id = parts.hostname
        self.jobs = {}
        self.lock = threading.RLock()
        self.changed = threading.Condition(self.lock)

    def touch(self, job):
        job.revision += 1
        self.changed.notify_all()

    def expire(self):
        now = time.time()
        for job in self.jobs.values():
            if now >= job.deadline and job.phase not in TERMINAL and job.phase != Phase.SIGNED:
                job.phase = Phase.EXPIRED
                job.decided_at = now
                self.touch(job)

    def new_code(self):
        active = {job.code for job in self.jobs.values() if job.phase not in TERMINAL}
        while True:
            raw = "".join(secrets.choice(CODE_ALPHABET) for _ in range(4))
            code = raw[:2] + "-" + raw[2:]
            if code not in active:
                return code

    def find_token(self, token):
        if not isinstance(token, str):
            raise ValueError("invalid code capability")
        found = None
        for job in self.jobs.values():
            if secrets.compare_digest(token, job.token):
                found = job
        if found is None:
            raise ValueError("unknown code capability")
        return found

    def session_job(self, cookie, page_id):
        if not isinstance(cookie, str) or not isinstance(page_id, str):
            raise ValueError("invalid page session")
        for job in self.jobs.values():
            if (job.cookie and job.page_id and secrets.compare_digest(cookie, job.cookie)
                    and secrets.compare_digest(page_id, job.page_id)):
                return job
        raise ValueError("unauthenticated page")

    def cli(self, request):
        with self.lock:
            self.expire()
            op = request["op"]
            if op == "enqueue":
                self.jobs = {key: job for key, job in self.jobs.items()
                             if job.phase not in TERMINAL or time.time() - (job.decided_at or 0) < 3600}
                if len(self.jobs) >= 32:
                    raise ValueError("approval queue full")
                body = normalize_request(request["request"])
                now = time.time()
                job = Job(secrets.token_hex(32), secrets.token_urlsafe(16), secrets.token_urlsafe(16),
                          self.new_code(), body, now, now + CODE_SECONDS, secrets.token_urlsafe(32))
                self.jobs[job.id] = job
                result = job.public()
                result.update({"token": job.token, "url": self.origin + "/a/" + job.token,
                               "display_url": self.origin + "/d/" + job.display_id})
                return result
            job = self.jobs[request["id"]]
            if op == "status":
                return job.public()
            if op == "publish":
                if job.request["kind"] != "grant" or job.phase != Phase.OPENED or job.consumed:
                    raise ValueError("grant is not ready for preparation")
                prepared = request["prepared"]
                encoded(prepared)
                try:
                    root = prepared["root"]
                    intent = prepared["intent"]
                    displayed = grant_request_from_action(job.request["asker"], intent["mother_node_id"],
                                                          intent["action"])
                    matches = (root["origin"] == self.origin and root["rp_id"] == self.rp_id
                               and encoded(displayed) == encoded(job.request))
                except (KeyError, TypeError, ValueError):
                    matches = False
                if not matches:
                    raise ValueError("prepared grant differs from the displayed request")
                job.prepared = prepared
                job.phase = Phase.PREPARED
                self.touch(job)
                return job.public()
            if op == "take":
                if job.phase != Phase.SIGNED or job.taken:
                    raise ValueError("signed grant is unavailable")
                job.taken = True
                self.touch(job)
                return {"path": str(self.directory / (job.id + ".json"))}
            if op == "settle":
                if job.phase != Phase.SIGNED or not job.taken or request["outcome"] not in ("approved", "refused"):
                    raise ValueError("grant cannot be settled")
                job.phase = Phase(request["outcome"])
                job.outcome = request["outcome"]
                job.reason = request.get("reason")
                job.decided_at = time.time()
                self.touch(job)
                return job.public()
            if op == "cancel" and job.phase not in TERMINAL and not job.consumed:
                job.phase = Phase.REFUSED
                job.consumed = True
                job.outcome = "refused"
                job.reason = "cancelled"
                job.decided_at = time.time()
                self.touch(job)
                return job.public()
            raise ValueError("invalid CLI transition")

    def inspect_token(self, token):
        with self.lock:
            self.expire()
            job = self.find_token(token)
            return {"code": job.code, "kind": job.request["kind"], "phase": job.phase.value,
                    "deadline": job.deadline, "consumed": job.consumed, "asker": job.request["asker"]}

    def display_job(self, display_id):
        for job in self.jobs.values():
            if secrets.compare_digest(display_id, job.display_id):
                return job
        raise ValueError("unknown display")

    def display(self, display_id):
        with self.lock:
            self.expire()
            job = self.display_job(display_id)
            value = job.public()
            value["url"] = self.origin + "/a/" + job.token
            return value

    def display_qr(self, display_id, kind="svg"):
        from mct_approval_qr import png as qr_png, svg as qr_svg
        with self.lock:
            self.expire()
            job = self.display_job(display_id)
            url = self.origin + "/a/" + job.token
            return qr_svg(url) if kind == "svg" else qr_png(url, scale=8)

    def claim(self, token, subject):
        with self.lock:
            self.expire()
            job = self.find_token(token)
            if job.phase == Phase.EXPIRED or job.consumed or job.phase != Phase.WAITING:
                raise ValueError("code is no longer available")
            subject = text(subject, "subject", 256)
            if subject != "nicabar":
                raise ValueError("wrong account")
            job.cookie = secrets.token_urlsafe(32)
            job.page_id = secrets.token_urlsafe(32)
            job.subject = subject
            job.phase = Phase.OPENED
            self.touch(job)
            return {"cookie": job.cookie, "page_id": job.page_id, "id": job.id}

    def page_state(self, cookie, page_id):
        with self.lock:
            self.expire()
            return self.session_job(cookie, page_id).public()

    def complete(self, cookie, page_id, result):
        with self.lock:
            self.expire()
            job = self.session_job(cookie, page_id)
            if job.request["kind"] != "grant" or job.phase != Phase.PREPARED or job.consumed:
                raise ValueError("grant is no longer signable")
            valid = (exact(result, ("intent", "proof"))
                     and encoded(result["intent"]) == encoded(job.prepared["intent"])
                     and isinstance(result["proof"], dict)
                     and result["proof"].get("scheme") == "webauthn-p256-day-key/v1"
                     and result["proof"].get("credential_id") == job.prepared["root"]["credential_id"])
            job.consumed = True
            if not valid:
                job.phase = Phase.REFUSED
                job.outcome = "refused"
                job.reason = "details_mismatch"
                job.decided_at = time.time()
                self.touch(job)
                return job.public()
            try:
                write_new(self.directory / (job.id + ".json"), result)
            except OSError:
                job.phase = Phase.REFUSED
                job.outcome = "refused"
                job.reason = "write_failed"
                job.decided_at = time.time()
                self.touch(job)
                raise
            job.phase = Phase.SIGNED
            self.touch(job)
            return job.public()

    def decline(self, cookie, page_id):
        with self.lock:
            self.expire()
            job = self.session_job(cookie, page_id)
            if job.request["kind"] != "grant" or job.phase != Phase.PREPARED or job.consumed:
                raise ValueError("grant cannot be declined")
            job.phase = Phase.DECLINED
            job.consumed = True
            job.outcome = "declined"
            job.decided_at = time.time()
            self.touch(job)
            return job.public()

    def proof_request(self, cookie, page_id):
        with self.lock:
            self.expire()
            job = self.session_job(cookie, page_id)
            if job.request["kind"] != "proof" or job.phase != Phase.OPENED or job.consumed:
                raise ValueError("proof is unavailable")
            value = dict(job.request)
            value.update({"id": job.id, "nonce": job.nonce})
            return value

    def proof_result(self, cookie, page_id, result):
        with self.lock:
            self.expire()
            job = self.session_job(cookie, page_id)
            if job.request["kind"] != "proof" or job.phase != Phase.OPENED or job.consumed:
                raise ValueError("proof is no longer available")
            job.consumed = True
            valid = (exact(result, ("outcome", "reason", "subject", "path", "nonce"))
                     and result["subject"] == job.subject == job.request["expected_subject"]
                     and result["path"] == job.request["path"] and result["nonce"] == job.nonce)
            if valid and result["outcome"] == "pass" and result["reason"] == "ok":
                job.phase, job.outcome, job.reason = Phase.PASSED, "pass", "ok"
            elif valid and result["outcome"] == "fail" and result["reason"] in ("denied", "not_found"):
                job.phase, job.outcome, job.reason = Phase.FAILED, "fail", result["reason"]
            else:
                job.phase, job.outcome, job.reason = Phase.REFUSED, "refused", "answer_mismatch"
            job.decided_at = time.time()
            self.touch(job)
            return job.public()

    def skip(self, cookie, page_id):
        with self.lock:
            self.expire()
            job = self.session_job(cookie, page_id)
            if job.request["kind"] != "proof" or job.phase != Phase.OPENED or job.consumed:
                raise ValueError("proof cannot be skipped")
            job.phase = Phase.SKIPPED
            job.consumed = True
            job.outcome = "skipped"
            job.reason = "skipped"
            job.decided_at = time.time()
            self.touch(job)
            return job.public()


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
    def __init__(self, directory, origin, cert, key, listen=None):
        self.broker = Broker(directory, origin)
        broker = self.broker

        class Handler(BaseHTTPRequestHandler):
            def setup(self):
                self.request.settimeout(15)
                super().setup()

            def reply(self, status, value, content_type="application/json", cookie=None):
                data = value if isinstance(value, bytes) else encoded(value)
                self.send_response(status)
                self.send_header("Content-Type", content_type + "; charset=utf-8")
                self.send_header("Content-Length", str(len(data)))
                self.send_header("Cache-Control", "no-store")
                self.send_header("Connection", "close")
                self.send_header("X-Content-Type-Options", "nosniff")
                self.send_header("Referrer-Policy", "no-referrer")
                self.send_header("Content-Security-Policy", "default-src 'none'; frame-ancestors 'none'; base-uri 'none'")
                if cookie:
                    self.send_header("Set-Cookie", "__Host-mct-queue=" + cookie
                                     + "; Path=/; Secure; HttpOnly; SameSite=Strict")
                self.end_headers()
                self.wfile.write(data)

            def host_ok(self):
                return self.headers.get_all("Host") == [broker.host]

            def auth(self):
                cookies = SimpleCookie(self.headers.get("Cookie", ""))
                cookie = cookies["__Host-mct-queue"].value if "__Host-mct-queue" in cookies else ""
                return cookie, self.headers.get("X-MCT-Page", "")

            def do_GET(self):
                try:
                    if not self.host_ok():
                        raise ValueError("wrong host")
                    if self.path.startswith("/d/") and self.path.endswith(".json"):
                        display_id = self.path[3:-5]
                        return self.reply(200, broker.display(display_id))
                    if self.path.startswith("/d/") and self.path.endswith("/qr.svg"):
                        display_id = self.path[3:-7]
                        return self.reply(200, broker.display_qr(display_id), "image/svg+xml")
                    if self.path.startswith("/d/") and self.path.endswith("/qr.png"):
                        display_id = self.path[3:-7]
                        return self.reply(200, broker.display_qr(display_id, "png"), "image/png")
                    if self.path == "/healthz":
                        return self.reply(200, {"ok": True})
                    return self.reply(404, {"error": "not found"})
                except (ValueError, KeyError, TypeError, RecursionError, OSError):
                    self.reply(400, {"error": "request refused"})

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
                    if self.path == "/api/inspect":
                        return self.reply(200, broker.inspect_token(request["token"]))
                    if self.path == "/api/session":
                        session = broker.claim(request["token"], request["subject"])
                        return self.reply(200, {"page_id": session["page_id"]}, cookie=session["cookie"])
                    cookie, page_id = self.auth()
                    if self.path == "/api/state":
                        result = broker.page_state(cookie, page_id)
                    elif self.path == "/api/complete":
                        result = broker.complete(cookie, page_id, request["result"])
                    elif self.path == "/api/decline":
                        result = broker.decline(cookie, page_id)
                    elif self.path == "/api/proof-request":
                        result = broker.proof_request(cookie, page_id)
                    elif self.path == "/api/proof-result":
                        result = broker.proof_result(cookie, page_id, request["result"])
                    elif self.path == "/api/skip":
                        result = broker.skip(cookie, page_id)
                    else:
                        raise ValueError("unknown route")
                    self.reply(200, result)
                except (ValueError, KeyError, TypeError, RecursionError, OSError):
                    self.reply(400, {"error": "request refused"})

            def log_message(self, *_):
                pass

        context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        context.minimum_version = ssl.TLSVersion.TLSv1_2
        context.load_cert_chain(cert, key)
        port = urlsplit(origin).port
        listen = listen or ("127.0.0.1", 443 if port is None else port)
        if (not isinstance(listen, tuple) or len(listen) != 2 or listen[0] != "127.0.0.1"
                or not isinstance(listen[1], int) or not 0 <= listen[1] <= 65535):
            raise ValueError("approval queue listen address must be 127.0.0.1:port")
        self.http = BoundedHTTPServer(listen, Handler)
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
