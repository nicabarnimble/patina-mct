#!/usr/bin/env python3
"""Install a new unit for an already provisioned admin Mother. Never bootstrap."""
import argparse
import os
from pathlib import Path
import re
import subprocess
import tempfile

from mother_systemd import install_unit, render_unit, safe_directory
from passkey_queue import write_new


def run(*args):
    return subprocess.run([str(x) for x in args], check=True, capture_output=True, text=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--unit-template", required=True)
    parser.add_argument("--root", required=True)
    parser.add_argument("--mother", required=True)
    parser.add_argument("--ledger-id", required=True)
    parser.add_argument("--unit-name", required=True)
    args = parser.parse_args()
    if os.geteuid() == 0 or not re.fullmatch(r"mct-admin-[a-z0-9-]{1,64}\.service", args.unit_name):
        raise ValueError("run unprivileged; use a new mct-admin-NAME.service unit")
    root = safe_directory(Path(args.root))
    filesystem = run("findmnt", "-n", "-o", "FSTYPE", "-T", root).stdout.strip()
    if filesystem not in {"ext4", "xfs", "btrfs"}:
        raise ValueError("Mother state must be on a supported local filesystem")
    binary = Path(args.binary).resolve(strict=True)
    safe_directory(binary.parent)
    info = binary.stat()
    if not str(binary).startswith("/nix/store/") or info.st_uid != 0 or info.st_mode & 0o022:
        raise ValueError("expected an immutable Nix-installed executable")
    home = safe_directory(Path.home())
    with tempfile.TemporaryDirectory(prefix=".mct-install-check-", dir=home) as temporary:
        action = Path(temporary) / "action.json"
        write_new(action, {"action": "command", "scope": "status", "route": "/status", "body": ""})
        # Read-only replay checks the canonical target and passkey provenance.
        run(binary, "passkey-admin", "prepare", "--root", root, "--mother", args.mother,
            "--ledger-id", args.ledger_id, "--action", action, "--signer", "root")
    config = Path(os.environ.get("XDG_CONFIG_HOME", str(home / ".config")))
    unit_path = config / "systemd/user" / args.unit_name
    contents = render_unit(Path(args.unit_template).read_text(), str(binary), str(root), args.mother, args.ledger_id)
    # Keep this package alive even after a subsequent profile upgrade or GC.
    roots = safe_directory(config / "mct/package-roots", create=True)
    gcroot = roots / args.unit_name
    if os.path.lexists(gcroot) or os.path.lexists(unit_path):
        raise FileExistsError("unit/package root exists; inspect it rather than replacing it")
    run("nix-store", "--add-root", gcroot, "--indirect", "--realise", binary.parent.parent)
    install_unit(unit_path, contents.encode())
    run("systemctl", "--user", "daemon-reload")
    print(f"Installed {unit_path}; retained package root {gcroot}")
    print(f"Start explicitly: systemctl --user enable --now {args.unit_name}")
    print("Unattended boot also requires explicit loginctl enable-linger for this user.")


if __name__ == "__main__":
    main()
