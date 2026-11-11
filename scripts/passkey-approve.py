#!/usr/bin/env python3
"""Show a one-use QR, wait for phone review, then submit only Mother-verified approval."""
import argparse
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import subprocess
import sys
import time

from mct_approval_qr import terminal as terminal_qr
from passkey_queue import grant_request_from_action, private_directory, read_private, rpc, write_new


TERMINAL = {"approved", "declined", "passed", "failed", "skipped", "expired", "refused"}


def mct(binary, *args):
    result = subprocess.run([binary, "passkey-admin", *map(str, args)], capture_output=True, timeout=60)
    if result.returncode:
        raise RuntimeError("MCT refused or did not complete; inspect canonical state before any retry")
    return json.loads(result.stdout)


def wait(session, job_id, phases):
    deadline = time.monotonic() + 11 * 60
    while time.monotonic() < deadline:
        job = rpc(session, {"op": "status", "id": job_id})
        if job["phase"] in phases or job["phase"] in TERMINAL:
            return job
        time.sleep(0.1)
    raise TimeoutError("approval wait expired")


def instant(value):
    return value.astimezone().strftime("%-I:%M %p %Z")


def show(job):
    print(terminal_qr(job["url"], ansi=sys.stdout.isatty()), end="")
    request = job["request"]
    if request["kind"] == "grant":
        print(f"{request['asker']} wants to use {request['mother']}")
        for item in request["permissions"]:
            print("  " + item["name"] + " — " + item["description"])
        print("Until " + request["grant_ends_at"])
    else:
        print(f"For NIC-132, {request['asker']} needs proof you can open {request['title']} on {request['site']}.")
        print("  " + request["path"])
        print("  " + request["why"])
        print("Nothing is granted. The bot receives only pass or fail.")
    print("Code " + job["code"] + " · expires " + instant(datetime.fromtimestamp(job["deadline"], timezone.utc)))
    print(job["url"])
    print("Desktop panel " + job["display_url"])
    print("⠼ Waiting for you on your phone…", flush=True)


def terminal(job):
    decided = datetime.fromtimestamp(job["decided_at"] or time.time(), timezone.utc)
    at = instant(decided)
    request = job["request"]
    phase = job["phase"]
    if request["kind"] == "grant":
        if phase == "approved":
            return 0, f"✓ Approved {at} · {request['mother']} until {request['grant_ends_at']}"
        if phase == "declined":
            return 10, f"– Declined {at} · nothing changed"
        if phase == "expired":
            return 11, f"! Code expired {at} · run the command again for a new code"
        return 12, "× Refused · the approval didn't match this request · nothing changed"
    if phase == "passed":
        return 0, f"✓ Passed {at} · nicabar can open it"
    if phase == "failed" and job["reason"] == "denied":
        return 20, f"× Failed {at} · not allowed for nicabar · new code to retry"
    if phase == "failed" and job["reason"] == "not_found":
        return 21, f"× Failed {at} · file not found on wiki.nicabar.org"
    if phase == "skipped":
        return 10, f"– Skipped {at} · nothing was checked"
    if phase == "expired":
        return 11, f"! Code expired {at} · run the command again for a new code"
    return 12, "× Refused · the answer didn't match this check"


def grant_action(args, grant_end):
    if args.command == "bootstrap":
        return {"action": "bootstrap", "root": read_private(args.registered_root)}
    if args.command == "issue":
        return {"action": "issue_day_key", "grant_id": args.grant,
                "public_half": read_private(args.public_key), "scopes": args.scope,
                "expires_at": grant_end.isoformat()}
    return {"action": "revoke_day_key", "grant_id": args.grant}


def prepare_grant(args, session, job_id, action):
    common = ["--root", args.root, "--mother", args.mother, "--ledger-id", args.ledger_id]
    if args.command == "bootstrap":
        return common, mct(args.binary, "prepare-genesis", *common, "--registered-root", args.registered_root)
    action_path = session / (job_id + ".action.json")
    write_new(action_path, action)
    return common, mct(args.binary, "prepare", *common, "--action", action_path, "--signer", "root")


def execute_grant(args, session):
    if args.command == "issue" and not 0 < args.lease_seconds <= 86400:
        raise ValueError("lease must be in (0, 86400] seconds")
    now = datetime.now(timezone.utc)
    grant_end = now + timedelta(seconds=args.lease_seconds) if args.command == "issue" else None
    action = grant_action(args, grant_end)
    request = grant_request_from_action(args.asker, args.mother, action)
    job = rpc(session, {"op": "enqueue", "request": request})
    show(job)
    submitted = False
    try:
        opened = wait(session, job["id"], {"opened"})
        if opened["phase"] != "opened":
            code, line = terminal(opened)
            print(line)
            return code
        common, prepared = prepare_grant(args, session, job["id"], action)
        rpc(session, {"op": "publish", "id": job["id"], "prepared": prepared})
        signed = wait(session, job["id"], {"signed"})
        if signed["phase"] != "signed":
            code, line = terminal(signed)
            print(line)
            return code
        result_path = rpc(session, {"op": "take", "id": job["id"]})["path"]
        if Path(result_path) != session / (job["id"] + ".json"):
            raise ValueError("unexpected signed result path")
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
        write_new(session / (job["id"] + ".reply.json"), reply)
        final = rpc(session, {"op": "settle", "id": job["id"], "outcome": "approved"})
        code, line = terminal(final)
        print(line)
        return code
    except BaseException:
        if submitted:
            try:
                final = rpc(session, {"op": "settle", "id": job["id"], "outcome": "refused",
                                      "reason": "mother_refused"})
                print(terminal(final)[1])
            except (ValueError, OSError):
                pass
        else:
            try:
                rpc(session, {"op": "cancel", "id": job["id"]})
            except (ValueError, OSError):
                pass
        raise


def execute_proof(args, session):
    request = {"kind": "proof", "asker": args.asker, "title": args.title,
               "site": "wiki.nicabar.org", "path": args.path, "why": args.why,
               "expected_subject": "nicabar", "required_group": "wiki-admin"}
    job = rpc(session, {"op": "enqueue", "request": request})
    show(job)
    final = wait(session, job["id"], TERMINAL)
    code, line = terminal(final)
    print(line)
    return code


def execute(args):
    session = private_directory(args.session)
    if args.command == "proof":
        return execute_proof(args, session)
    return execute_grant(args, session)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--session", required=True)
    parser.add_argument("--binary", default="target/debug/mct-daemon")
    parser.add_argument("--asker", default="Unraid Grok")
    commands = parser.add_subparsers(dest="command", required=True)
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
    proof = commands.add_parser("proof")
    proof.add_argument("--title", required=True)
    proof.add_argument("--path", required=True)
    proof.add_argument("--why", default="Checking the library release works for you (NIC-132)")
    try:
        code = execute(parser.parse_args())
    except (ValueError, RuntimeError, OSError, TimeoutError, subprocess.TimeoutExpired) as error:
        parser.exit(1, str(error) + "\n")
    parser.exit(code)


if __name__ == "__main__":
    main()
