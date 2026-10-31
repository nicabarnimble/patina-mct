"""Render and exclusively publish a user unit; never repair an existing unit."""
import hashlib
import os
from pathlib import Path
import re
import stat


def quote(value, command=False):
    value = str(value)
    if any(ord(c) < 32 or ord(c) == 127 for c in value):
        raise ValueError("control character in systemd argument")
    value = value.replace("%", "%%").replace("\\", "\\\\").replace('"', '\\"')
    if command:
        value = value.replace("$", "$$")
    return '"' + value + '"'


def render_unit(template, binary, root, mother, ledger):
    for path in (binary, root):
        if not Path(path).is_absolute() or ".." in Path(path).parts:
            raise ValueError("unit paths must be absolute without parent traversal")
    for name in (mother, ledger):
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,100}", name):
            raise ValueError("invalid unit identity")
    argv = [binary, "passkey-admin", "serve-iroh", "--root", root,
            "--mother", mother, "--ledger-id", ledger,
            "--ticket-output", str(Path(root) / "admin-ticket.json"),
            "--bind-address", "127.0.0.1:0"]
    return template.replace("@EXEC@", " ".join(quote(arg, True) for arg in argv)).replace("@ROOT@", quote(root))


def safe_directory(path, create=False):
    path = Path(path)
    if not path.is_absolute() or ".." in path.parts:
        raise ValueError("unsafe directory path")
    for parent in reversed((path, *path.parents)):
        if create:
            try:
                parent.mkdir(mode=0o700)
            except FileExistsError:
                pass
        info = parent.lstat()
        sticky_root = info.st_uid == 0 and bool(info.st_mode & stat.S_ISVTX)
        if (not stat.S_ISDIR(info.st_mode) or info.st_uid not in (0, os.geteuid())
                or (info.st_mode & 0o022 and not sticky_root)):
            raise ValueError("unsafe directory ancestry")
    return path


def mount_options(mountinfo, path):
    """Select the effective (longest ancestor) mount, decoding procfs escapes."""
    candidates = []
    for line in mountinfo.splitlines():
        fields = line.split()
        mount = Path(re.sub(r"\\([0-7]{3})", lambda m: chr(int(m[1], 8)), fields[4]))
        if Path(path).is_relative_to(mount):
            candidates.append((len(mount.parts), set(fields[5].split(","))))
    if not candidates:
        raise ValueError("mount not found")
    return max(candidates, key=lambda entry: entry[0])[1]


def unit_identity(path):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, "rb") as stream:
        info = os.fstat(stream.fileno())
        if (not stat.S_ISREG(info.st_mode) or info.st_nlink != 1
                or info.st_uid != os.geteuid() or info.st_mode & 0o077):
            raise ValueError("unsafe unit file")
        data = stream.read(16385)
        if len(data) > 16384:
            raise ValueError("oversized unit file")
        return info.st_dev, info.st_ino, hashlib.sha256(data).hexdigest()


def install_unit(path, contents):
    safe_directory(path.parent, create=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(contents)
        stream.flush()
        os.fsync(stream.fileno())
    return unit_identity(path)


def remove_unit(path, expected):
    safe_directory(path.parent)
    if unit_identity(path) != expected:
        raise ValueError("unit changed; preserve it for inspection")
    path.unlink()
