#!/usr/bin/env python3
"""Install a root-owned system unit for a provisioned Mother running as pi:pi."""
import argparse
import json
import os
from pathlib import Path
import pwd
import shutil
import subprocess
import tempfile

from mother_systemd import SystemUnit, install_unit, remove_unit, render_unit, safe_directory, trusted_program, trusted_systemctl, unit_identity
from passkey_queue import write_new


def run(*args):
    result = subprocess.run([str(x) for x in args], capture_output=True, text=True, timeout=60)
    if result.returncode:
        raise RuntimeError(f"{args[0]} failed: {result.stderr.strip()}")
    return result


def root_command(entrypoint, *args):
    sudo = shutil.which("sudo")
    if sudo is None:
        raise RuntimeError("sudo is required for system-unit management")
    return run(sudo, "-n", entrypoint, *args)


def require_new_unit(unit):
    if os.path.lexists(unit.gcroot) or os.path.lexists(unit.path):
        raise FileExistsError("unit/package root exists; inspect it rather than replacing it")
    state = run(trusted_systemctl(), "show", unit.name, "-p", "LoadState", "--value").stdout.strip()
    if state != "not-found":
        raise FileExistsError("unit name is already known to the system manager")


def remove_system_unit(unit, expected, package):
    if unit_identity(unit.path, owner_uid=0) != expected:
        raise ValueError("unit changed; preserve it for inspection")
    safe_directory(unit.gcroot.parent)
    if not unit.gcroot.is_symlink() or unit.gcroot.resolve() != package:
        raise ValueError("package root changed; cleanup refused")
    run(trusted_systemctl(), "stop", unit.name)
    if run(trusted_systemctl(), "show", unit.name, "-p", "MainPID", "--value").stdout.strip() != "0":
        raise RuntimeError("service did not stop")
    run(trusted_systemctl(), "disable", unit.name)
    remove_unit(unit.path, expected, owner_uid=0)
    unit.gcroot.unlink()
    run(trusted_systemctl(), "daemon-reload")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--unit-template", required=True)
    parser.add_argument("--entrypoint", required=True)
    parser.add_argument("--operation", choices=("install", "publish", "remove"), default="install")
    parser.add_argument("--root")
    parser.add_argument("--mother")
    parser.add_argument("--ledger-id")
    parser.add_argument("--unit-name", required=True)
    parser.add_argument("--expected-identity")
    args = parser.parse_args()
    unit = SystemUnit(args.unit_name)
    verify = lambda package: run("nix-store", "--verify-path", package)
    binary = trusted_program(args.binary, verify)
    entrypoint = trusted_program(args.entrypoint, verify)
    if args.operation == "remove":
        if args.expected_identity is None:
            parser.error("removal requires the original unit identity")
        if os.geteuid() != 0:
            print(root_command(entrypoint, "--operation", "remove", "--unit-name", unit.name,
                               "--expected-identity", args.expected_identity).stdout, end="")
            return
        remove_system_unit(unit, tuple(json.loads(args.expected_identity)), binary.parent.parent)
        print(f"Removed {unit.name}; canonical state unchanged")
        return
    if any(value is None for value in (args.root, args.mother, args.ledger_id)):
        parser.error("installation requires --root, --mother and --ledger-id")
    pi_uid = pwd.getpwnam("pi").pw_uid
    root = safe_directory(Path(args.root), owner_uid=pi_uid)
    info = root.stat()
    if info.st_uid != pi_uid or info.st_mode & 0o077:
        raise ValueError("Mother root must be a private directory owned by pi")
    filesystem = run("findmnt", "-n", "-o", "FSTYPE", "-T", root).stdout.strip()
    if filesystem not in {"ext4", "xfs", "btrfs"}:
        raise ValueError("Mother state must be on a supported local filesystem")
    if args.operation == "install":
        if os.geteuid() != pi_uid:
            raise ValueError("run the installer as pi; only publication uses sudo")
        home = safe_directory(Path.home())
        with tempfile.TemporaryDirectory(prefix=".mct-install-check-", dir=home) as temporary:
            action = Path(temporary) / "action.json"
            write_new(action, {"action": "command", "scope": "status", "route": "/status", "body": ""})
            # Replay as pi, not root: canonical ownership checks are unchanged.
            run(binary, "passkey-admin", "prepare", "--root", root, "--mother", args.mother,
                "--ledger-id", args.ledger_id, "--action", action, "--signer", "root")
        print(root_command(entrypoint, "--operation", "publish", "--root", root, "--mother", args.mother,
                           "--ledger-id", args.ledger_id, "--unit-name", unit.name).stdout, end="")
        return
    if os.geteuid() != 0:
        raise PermissionError("system-unit publication requires root")
    verify(Path(args.unit_template))
    contents = render_unit(Path(args.unit_template).read_text(), str(binary), str(root), args.mother, args.ledger_id)
    safe_directory(unit.gcroot.parent, create=True)
    safe_directory(unit.path.parent)
    require_new_unit(unit)
    run("nix-store", "--add-root", unit.gcroot, "--indirect", "--realise", binary.parent.parent)
    identity = install_unit(unit.path, contents.encode(), system=True)
    run(trusted_systemctl(), "daemon-reload")
    print(json.dumps({"unit": unit.name, "identity": identity, "package_root": str(unit.gcroot)}))
    print(f"Start explicitly: sudo systemctl enable --now {unit.name}")


if __name__ == "__main__":
    main()
