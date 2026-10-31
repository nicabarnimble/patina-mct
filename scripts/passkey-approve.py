#!/usr/bin/env python3
"""Queue a passkey ceremony, wait for its assertion, then invoke the existing MCT CLI."""
import argparse
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import subprocess
import sys
import time
import webbrowser

from passkey_queue import private_directory, read_private, rpc, write_new


def mct(binary, *args):
    result = subprocess.run([binary, "passkey-admin", *map(str, args)],
                            capture_output=True, timeout=60)
    if result.returncode:
        raise RuntimeError("MCT refused or did not complete; inspect canonical state before any retry")
    return json.loads(result.stdout)


def wait(session, job_id, phases):
    deadline = time.monotonic() + 3600
    while time.monotonic() < deadline:
        job = rpc(session, {"op": "status", "id": job_id})
        if job["phase"] in phases:
            return job
        if job["phase"] in ("failed", "taken"):
            raise RuntimeError("ceremony ended without a request; no action was submitted")
        time.sleep(0.1)
    raise TimeoutError("ceremony wait expired")


def execute(args):
    session = private_directory(args.session)
    if args.command == "open":
        url = rpc(session, {"op": "open"})["url"]
        if sys.platform == "darwin":
            opened = subprocess.run(["open", "-a", "Google Chrome", url], capture_output=True).returncode == 0
        else:
            opened = webbrowser.get("google-chrome").open(url)
        if not opened:
            raise RuntimeError("could not open Google Chrome")
        print("Opened authenticated approval page. No action was submitted.")
        return
    registration = args.command == "register"
    if registration:
        private_directory(Path(args.output).parent)
        if Path(args.output).exists():
            raise ValueError("refusing to replace a registration pin")
    job = rpc(session, {"op": "enqueue", "kind": "registration" if registration else "assertion",
                        "name": args.name if registration else "Doug"})
    job_id = job["id"]
    submitted = False
    try:
        if not registration:
            wait(session, job_id, ("ready",))
            common = ["--root", args.root, "--mother", args.mother, "--ledger-id", args.ledger_id]
            if args.command == "bootstrap":
                prepared = mct(args.binary, "prepare-genesis", *common, "--registered-root", args.registered_root)
            else:
                if args.command == "issue":
                    if not 0 < args.lease_seconds <= 86400:
                        raise ValueError("lease must be in (0, 86400] seconds")
                    action = {"action": "issue_day_key", "grant_id": args.grant,
                              "public_half": read_private(args.public_key), "scopes": args.scope,
                              "expires_at": (datetime.now(timezone.utc) + timedelta(seconds=args.lease_seconds)).isoformat()}
                else:
                    action = {"action": "revoke_day_key", "grant_id": args.grant}
                action_path = session / (job_id + ".action.json")
                write_new(action_path, action)
                prepared = mct(args.binary, "prepare", *common, "--action", action_path, "--signer", "root")
            rpc(session, {"op": "publish", "id": job_id, "prepared": prepared})
        wait(session, job_id, ("signed",))
        result_path = rpc(session, {"op": "take", "id": job_id})["path"]
        if Path(result_path) != session / (job_id + ".json"):
            raise ValueError("unexpected result path")
        if registration:
            write_new(args.output, read_private(result_path))
            print("PASS registration pin installed privately")
            return
        submitted = True
        if args.command == "bootstrap":
            reply = mct(args.binary, "bootstrap", *common, "--registered-root", args.registered_root,
                        "--request", result_path)
            if reply["observation"]["safe_message"] != "passkey administration committed":
                raise RuntimeError("genesis did not commit")
        else:
            reply = mct(args.binary, "send-iroh", "--ticket", args.ticket, "--request", result_path)
            if reply["status"] != "completed":
                raise RuntimeError("administration did not complete")
        write_new(session / (job_id + ".reply.json"), reply)
        print("PASS " + args.command + " committed")
    except BaseException:
        if not submitted:
            try:
                rpc(session, {"op": "cancel", "id": job_id})
            except (ValueError, OSError):
                pass
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", required=True)
    parser.add_argument("--binary", default="target/debug/mct-daemon")
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("open")
    register = commands.add_parser("register")
    register.add_argument("--output", required=True)
    register.add_argument("--name", default="Doug")
    for command in ("bootstrap", "issue", "revoke"):
        child = commands.add_parser(command)
        for flag in ("root", "mother", "ledger-id"):
            child.add_argument("--" + flag, required=True)
        if command == "bootstrap":
            child.add_argument("--registered-root", required=True)
        else:
            child.add_argument("--ticket", required=True)
            child.add_argument("--grant", required=True)
        if command == "issue":
            child.add_argument("--public-key", required=True)
            child.add_argument("--scope", action="append", choices=("status", "key_manage"), required=True)
            child.add_argument("--lease-seconds", type=int, default=86400)
    try:
        execute(parser.parse_args())
    except (ValueError, RuntimeError, OSError, TimeoutError, subprocess.TimeoutExpired, webbrowser.Error) as error:
        parser.exit(1, str(error) + "\n")


if __name__ == "__main__":
    main()
