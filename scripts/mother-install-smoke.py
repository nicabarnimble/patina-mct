#!/usr/bin/env python3
"""Native hardware gate. Generates only isolated software test authority."""
import argparse
import base64
from datetime import datetime, timedelta, timezone
import hashlib
import ipaddress
import json
import os
from pathlib import Path
import platform
import secrets
import shutil
import subprocess
import tempfile
import time

from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from mother_systemd import SystemUnit, mount_options, render_unit, safe_directory, trusted_program, trusted_systemctl, unit_identity
from passkey_queue import read_private, write_new


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def execute(argv, success=True, timeout=60):
    result = subprocess.run([str(x) for x in argv], capture_output=True, text=True, timeout=timeout)
    if success and result.returncode:
        raise RuntimeError(f"command failed: {argv[0]} {argv[1]}: {result.stderr.strip()}")
    return result


def systemctl(*args, **kwargs):
    return execute(["sudo", "-n", trusted_systemctl(), *args], **kwargs)


def properties(unit):
    text = systemctl("show", unit, "-p", "MainPID", "-p", "ActiveState", "-p", "FragmentPath").stdout
    return dict(line.split("=", 1) for line in text.splitlines() if "=" in line)


def rss(pid):
    fields = {}
    for line in Path(f"/proc/{pid}/status").read_text().splitlines():
        if line.startswith(("VmRSS:", "VmHWM:")):
            name, value, unit = line.split()
            require(unit == "kB", "unexpected RSS units")
            fields[name.rstrip(":")] = int(value)
    require(len(fields) == 2, "RSS unavailable")
    return fields


def verify_sandbox(pid, root, outside):
    mountinfo = Path(f"/proc/{pid}/mountinfo").read_text()
    require("ro" in mount_options(mountinfo, "/"), "system root is not read-only")
    require("rw" in mount_options(mountinfo, root), "canonical root is not writable")
    require("ro" in mount_options(mountinfo, outside), "home sandbox is not read-only")
    require("ro" in mount_options(mountinfo, "/nix/store"), "package store is not read-only")
    status = dict(line.split(":", 1) for line in Path(f"/proc/{pid}/status").read_text().splitlines() if ":" in line)
    require(status["NoNewPrivs"].strip() == "1", "no-new-privileges not active")
    require(set(status["Uid"].split()) == {str(os.geteuid())}, "service UID mismatch")
    require(set(status["Gid"].split()) == {str(os.getegid())}, "service GID mismatch")
    require(os.readlink(f"/proc/{pid}/ns/user") == os.readlink("/proc/self/ns/user"), "unexpected user namespace")
    require(os.readlink(f"/proc/{pid}/ns/mnt") != os.readlink("/proc/self/ns/mnt"), "missing mount namespace")
    require(Path(f"/proc/{pid}/root").stat().st_uid == 0, "host-root ownership changed")
    require(Path(f"/proc/{pid}/root/tmp").stat().st_ino != Path("/tmp").stat().st_ino, "temporary directory is not private")
    require(all(int(status[field], 16) == 0 for field in ("CapEff", "CapAmb", "CapBnd")), "unexpected service capabilities")
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        sockets = set()
        for fd in Path(f"/proc/{pid}/fd").iterdir():
            try:
                target = os.readlink(fd)
            except FileNotFoundError:
                continue
            if target.startswith("socket:["):
                sockets.add(target[8:-1])
        routes = Path(f"/proc/{pid}/net/netlink").read_text().splitlines()[1:]
        if any(line.split()[1] == "0" and line.split()[-1] in sockets for line in routes):
            return
        time.sleep(0.1)
    raise RuntimeError("Iroh route monitor has no NETLINK_ROUTE socket")


class Ceremony:
    def __init__(self, binary, directory):
        self.binary = binary
        self.directory = directory
        self.root = directory / "mother"
        self.root.mkdir(mode=0o700)
        suffix = secrets.token_hex(8)
        self.mother, self.ledger = f"mother-smoke-{suffix}", f"ledger-smoke-{suffix}"
        self.common = ["--root", self.root, "--mother", self.mother, "--ledger-id", self.ledger]
        self.ticket = self.root / "admin-ticket.json"
        self.key = ec.generate_private_key(ec.SECP256R1())
        self.pin = self.put({
            "credential_id": self.b64(secrets.token_bytes(32)),
            "public_key_sec1": self.key.public_key().public_bytes(
                serialization.Encoding.X962, serialization.PublicFormat.UncompressedPoint).hex(),
            "rp_id": "mct-install.invalid", "origin": "https://mct-install.invalid"})
        self.secret = directory / "day-key.json"
        self.nonces = set()

    @staticmethod
    def b64(data):
        return base64.urlsafe_b64encode(data).rstrip(b"=").decode()

    def put(self, value):
        path = self.directory / (secrets.token_hex(16) + ".json")
        write_new(path, value)
        return path

    def cli(self, *args):
        return json.loads(execute([self.binary, "passkey-admin", *args]).stdout)

    def root_signature(self, prepared):
        auth = hashlib.sha256(b"mct-install.invalid").digest() + b"\x05" + bytes(4)
        client = json.dumps({"type": "webauthn.get", "origin": "https://mct-install.invalid",
                             "challenge": prepared["challenge"], "crossOrigin": False}).encode()
        signature = self.key.sign(auth + hashlib.sha256(client).digest(), ec.ECDSA(hashes.SHA256()))
        return {"intent": prepared["intent"], "proof": {
            "scheme": "webauthn-p256-day-key/v1", "credential_id": prepared["root"]["credential_id"],
            "authenticator_data": self.b64(auth), "client_data_json": self.b64(client),
            "signature": self.b64(signature)}}

    def bootstrap(self):
        prepared = self.cli("prepare-genesis", *self.common, "--registered-root", self.pin)
        request = self.put(self.root_signature(prepared))
        result = self.cli("bootstrap", *self.common, "--registered-root", self.pin, "--request", request)
        require(result["observation"]["safe_message"] == "passkey administration committed", "bootstrap failed")

    def prepare(self, action, signer):
        prepared = self.cli("prepare", *self.common, "--action", self.put(action), "--signer", signer)
        nonce = prepared["intent"]["nonce"]
        require(nonce not in self.nonces, "request nonce reused")
        self.nonces.add(nonce)
        return prepared

    def send(self, request, rejected=False):
        path = self.put(request)
        result = execute([self.binary, "passkey-admin", "send-iroh", "--ticket", self.ticket,
                          "--request", path], success=False)
        require(bool(result.stdout.strip()), "transport failure is not a cryptographic refusal")
        reply = json.loads(result.stdout)
        self.put(reply)
        expected = "rejected" if rejected else "completed"
        require(reply.get("status") == expected and (result.returncode != 0) == rejected,
                f"expected {expected}; inspect private evidence before retry")
        return reply.get("result")

    def action(self, action):
        return self.send(self.root_signature(self.prepare(action, "root")))

    def status(self, signer="root", rejected=False, corrupt=False):
        prepared = self.prepare({"action": "command", "scope": "status", "route": "/status", "body": ""}, signer)
        if signer == "root":
            signed = self.root_signature(prepared)
        else:
            signed = self.cli("sign-day", "--intent", self.put(prepared["intent"]),
                              "--grant", signer, "--secret", self.secret)
        if corrupt:
            signed["proof"]["signature"] = "00" * 64
        result = self.send(signed, rejected)
        if not rejected:
            require(result["mother_node_id"] == self.mother and result["ledger_id"] == self.ledger,
                    "status target mismatch")
        return prepared["intent"]["epoch"]


def wait_ready(unit, unit_path, binary, ticket, previous_pid=0, previous_mtime=0):
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
        state = properties(unit)
        require(state.get("FragmentPath") == str(unit_path), "systemd loaded a different unit")
        pid = int(state.get("MainPID", "0"))
        require(state.get("ActiveState") != "failed", "system service failed; inspect its journal")
        if pid and pid != previous_pid and ticket.exists() and ticket.stat().st_mtime_ns > previous_mtime:
            require(Path(f"/proc/{pid}/exe").resolve() == binary, "service executable mismatch")
            address = read_private(ticket)
            require(not address["relay_urls"] and bool(address["direct_addresses"]), "unexpected admin transport")
            require(all(ipaddress.ip_address(value.rsplit(":", 1)[0].strip("[]")).is_loopback
                        for value in address["direct_addresses"]), "admin ticket advertises non-loopback access")
            return pid
        time.sleep(0.1)
    raise RuntimeError("service readiness timeout")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--probe", required=True)
    parser.add_argument("--installer", required=True)
    parser.add_argument("--unit-template", required=True)
    args = parser.parse_args()
    require(platform.system() == "Linux" and platform.machine() == "aarch64", "native Linux aarch64 required")
    require(os.geteuid() != 0, "run as the unprivileged user, never root")
    def verify_contents(package):
        execute(["nix-store", "--verify-path", package])
    binary, probe = trusted_program(args.binary, verify_contents), trusted_program(args.probe, verify_contents)
    installer = trusted_program(args.installer, verify_contents)
    home = safe_directory(Path.home())
    filesystem = execute(["findmnt", "-n", "-o", "FSTYPE", "-T", home]).stdout.strip()
    require(filesystem in {"ext4", "xfs", "btrfs"}, "gate requires a local supported filesystem")
    systemctl("show-environment")
    directory = Path(tempfile.mkdtemp(prefix=".mct-install-smoke-", dir=home))
    print(f"Private test evidence: {directory}", flush=True)
    print("Software test authority only; not a 1Password ceremony or production identity.", flush=True)
    ceremony = Ceremony(binary, directory)
    ceremony.bootstrap()
    service = SystemUnit("mct-admin-smoke-" + secrets.token_hex(12) + ".service")
    unit, unit_path = service.name, service.path
    write_new(directory / "service.json", {"unit": unit, "path": str(unit_path)})
    text = render_unit(Path(args.unit_template).read_text(), str(binary), str(ceremony.root),
                       ceremony.mother, ceremony.ledger)
    execute([installer, "--root", ceremony.root, "--mother", ceremony.mother,
             "--ledger-id", ceremony.ledger, "--unit-name", unit])
    identity = unit_identity(unit_path, owner_uid=0)
    write_new(directory / "installed-unit.json", {"unit": unit, "identity": identity})
    require(identity[2] == hashlib.sha256(text.encode()).hexdigest(), "installer unit mismatch")
    retained = execute(["sudo", "-n", shutil.which("readlink"), service.gcroot]).stdout.strip()
    require(retained == str(binary.parent.parent), "package retention missing")
    metrics = {}
    try:
        systemctl("daemon-reload")
        started = time.monotonic()
        systemctl("start", unit)
        pid = wait_ready(unit, unit_path, binary, ceremony.ticket)
        first_epoch = ceremony.status()
        metrics["cold_readiness_ms"] = (time.monotonic() - started) * 1000
        verify_sandbox(pid, ceremony.root, directory)
        print("PASS install/start the system service as pi:pi", flush=True)
        idle = []
        for _ in range(5):
            idle.append(rss(pid)["VmRSS"])
            time.sleep(0.2)
        metrics["idle_rss_kib"] = idle
        public = ceremony.cli("day-keygen", "--secret-output", ceremony.secret)
        expiry = (datetime.now(timezone.utc) + timedelta(hours=1)).isoformat().replace("+00:00", "Z")
        ceremony.action({"action": "issue_day_key", "grant_id": "smoke-day", "public_half": public,
                         "scopes": ["status"], "expires_at": expiry})
        ceremony.status("smoke-day")
        ceremony.status("smoke-day", rejected=True, corrupt=True)
        print("PASS signed status", flush=True)
        before = (ceremony.root / "observations.jsonl").read_bytes()
        second = execute([binary, "passkey-admin", "serve-iroh", *ceremony.common,
                          "--ticket-output", ceremony.root / "second-ticket.json", "--bind-address", "127.0.0.1:0"],
                         success=False, timeout=10)
        require(second.returncode != 0 and "already locked by another writer" in second.stderr,
                "second writer not refused by the ledger lock")
        require((ceremony.root / "observations.jsonl").read_bytes() == before, "second writer changed the ledger")
        print("PASS second writer rejected", flush=True)
        ceremony.action({"action": "revoke_day_key", "grant_id": "smoke-day"})
        ceremony.status("smoke-day", rejected=True)
        print("PASS revoke", flush=True)
        before = (ceremony.root / "observations.jsonl").read_bytes()
        endpoint_identity = hashlib.sha256((ceremony.root / "identity/admin-iroh-secret.hex").read_bytes()).digest()
        old_mtime = ceremony.ticket.stat().st_mtime_ns
        peaks = [rss(pid)["VmHWM"]]
        systemctl("kill", "--signal=SIGKILL", "--kill-who=main", unit)
        pid = wait_ready(unit, unit_path, binary, ceremony.ticket, pid, old_mtime)
        require((ceremony.root / "observations.jsonl").read_bytes().startswith(before), "restart lost canonical history")
        require(hashlib.sha256((ceremony.root / "identity/admin-iroh-secret.hex").read_bytes()).digest() == endpoint_identity,
                "restart replaced endpoint identity")
        require(ceremony.status() != first_epoch, "restart did not establish a fresh authority epoch")
        verify_sandbox(pid, ceremony.root, directory)
        print("PASS forced restart", flush=True)
        ceremony.status("smoke-day", rejected=True)
        print("PASS revoked key rejected", flush=True)
        peaks.append(rss(pid)["VmHWM"])
        metrics["mother_peak_rss_kib"] = max(peaks)
        component = json.loads(execute([probe, "component"]).stdout)
        require(component["wasi_probe"] and component["compile_peak_kib"] > 0, "WASI measurements unavailable")
        metrics["component"] = component
        print("PASS WASI probe runs", flush=True)
        metrics["durable_write"] = json.loads(execute([probe, "ledger", directory / "durability"]).stdout)
        write_new(directory / "measurements.json", metrics)
        print(f"METRIC cold readiness {metrics['cold_readiness_ms']:.3f} ms (n=1, process-cold through signed status; OS cache not purged)")
        print(f"METRIC idle RSS {min(idle)}..{max(idle)} KiB (n=5); Mother peak RSS {max(peaks)} KiB")
        print(f"METRIC component compile {component['compile_ms']:.3f} ms; compile peak RSS {component['compile_peak_kib']} KiB (fresh probe process, n=1)")
        durable = metrics["durable_write"]
        print(f"METRIC durable-write p50={durable['p50_ms']:.3f} ms p95={durable['p95_ms']:.3f} ms max={durable['max_ms']:.3f} ms (n=40; append+sync_data, verified reopen)", flush=True)
    finally:
        require(unit_identity(unit_path, owner_uid=0) == identity, "unit changed; cleanup refused")
        execute([installer, "--operation", "remove", "--unit-name", unit,
                 "--expected-identity", json.dumps(identity)])
        require(properties(unit).get("MainPID") == "0", "service did not stop")
        require(not os.path.lexists(unit_path), "test unit still installed")
    print("PASS isolated service cleanup; private evidence preserved", flush=True)


if __name__ == "__main__":
    main()
