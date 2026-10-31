#!/usr/bin/env python3
"""Fresh, isolated approvals-only registration/genesis/day-key/revocation run."""
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

from passkey_queue import private_directory, read_private, write_new


def run(args):
    directory = Path(args.directory).absolute()
    private_directory(directory.parent)
    directory.mkdir(mode=0o700)
    mother = directory / "mother"
    mother.mkdir(mode=0o700)
    binary = str(Path(args.binary).absolute())
    common = ["--root", str(mother), "--mother", "mother-approval", "--ledger-id", "ledger-approval"]
    ticket = directory / "ticket.json"
    pin = directory / "registered-root.json"
    secret = directory / "day-secret.json"
    public = directory / "day-public.json"
    approve = [sys.executable, str(Path(__file__).with_name("passkey-approve.py")),
               "--session", args.session, "--binary", binary]

    def cli(*command, expected=0):
        result = subprocess.run([binary, "passkey-admin", *map(str, command)], capture_output=True, timeout=60)
        if (result.returncode == 0) != (expected == 0):
            raise RuntimeError("unexpected MCT exit; inspect the private run directory")
        return json.loads(result.stdout)

    def root_action(command, *options):
        print("WAIT " + command + " passkey approval", flush=True)
        subprocess.run([*approve, command, *map(str, options)], check=True, timeout=3700)

    def status(name, expected):
        action = directory / (name + ".action.json")
        write_new(action, {"action": "command", "scope": "status", "route": "/status", "body": ""})
        prepared = cli("prepare", *common, "--action", action, "--signer", "doug-day")
        intent = directory / (name + ".intent.json")
        write_new(intent, prepared["intent"])
        request = directory / (name + ".request.json")
        write_new(request, cli("sign-day", "--intent", intent, "--grant", "doug-day", "--secret", secret))
        reply = cli("send-iroh", "--ticket", ticket, "--request", request,
                    expected=0 if expected == "completed" else 1)
        write_new(directory / (name + ".reply.json"), reply)
        if reply.get("status") != expected:
            raise RuntimeError("unexpected administration reply")
        if expected == "completed" and (reply["result"]["mother_node_id"] != "mother-approval"
                                       or reply["result"]["ledger_id"] != "ledger-approval"
                                       or reply["result"]["day_key_count"] != 1):
            raise RuntimeError("wrong status target")
        print("PASS " + name + " " + expected, flush=True)

    resident = None
    log = None
    def start():
        nonlocal resident, log
        previous = ticket.stat().st_mtime_ns if ticket.exists() else None
        log = os.fdopen(os.open(directory / "resident.log",
                                os.O_WRONLY | os.O_CREAT | os.O_APPEND | os.O_NOFOLLOW, 0o600), "ab")
        resident = subprocess.Popen([binary, "passkey-admin", "serve-iroh", *common,
                                     "--ticket-output", str(ticket)], stdout=log, stderr=log)
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if resident.poll() is not None:
                raise RuntimeError("resident exited; inspect resident.log")
            if ticket.exists() and ticket.stat().st_mtime_ns != previous:
                read_private(ticket)
                return
            time.sleep(0.05)
        raise TimeoutError("resident did not become ready")

    def stop():
        nonlocal resident, log
        if resident is not None:
            if resident.poll() is None:
                resident.send_signal(signal.SIGINT)
            try:
                code = resident.wait(timeout=15)
            except subprocess.TimeoutExpired:
                resident.kill()
                resident.wait()
                raise RuntimeError("resident did not drain; inspect canonical state")
            finally:
                log.close()
                resident = None
            if code != 0:
                raise RuntimeError("resident did not shut down cleanly")

    try:
        write_new(public, cli("day-keygen", "--secret-output", secret))
        root_action("register", "--output", pin, "--name", "MCT approvals " + directory.parent.name[-32:])
        root_action("bootstrap", *common, "--registered-root", pin)
        start()
        root_action("issue", *common, "--ticket", ticket, "--grant", "doug-day",
                    "--public-key", public, "--scope", "status")
        status("day-status", "completed")
        root_action("revoke", *common, "--ticket", ticket, "--grant", "doug-day")
        status("revoked-status", "rejected")
        stop()
        start()
        status("restarted-status", "rejected")
        stop()
        print("PASS full ceremony; private artifacts retained", flush=True)
    finally:
        stop()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", required=True)
    parser.add_argument("--directory", required=True, help="new private run directory; must not exist")
    parser.add_argument("--binary", default="target/debug/mct-daemon")
    run(parser.parse_args())
