"""Render and exclusively publish units; never repair an existing unit."""
from dataclasses import dataclass
import hashlib
import os
from pathlib import Path
import re
import stat


@dataclass(frozen=True)
class SystemUnit:
    name: str

    def __post_init__(self):
        if not re.fullmatch(r"mct-admin-[a-z0-9-]{1,64}\.service", self.name):
            raise ValueError("use a new mct-admin-NAME.service unit")

    @property
    def path(self):
        return Path("/etc/systemd/system") / self.name

    @property
    def gcroot(self):
        return Path("/var/lib/mct/package-roots") / self.name


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


def safe_directory(path, create=False, owner_uid=None):
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
        if (not stat.S_ISDIR(info.st_mode) or info.st_uid not in (0, os.geteuid() if owner_uid is None else owner_uid)
                or (info.st_mode & 0o022 and not sticky_root)):
            raise ValueError("unsafe directory ancestry")
    return path


def trusted_program(path, verify_contents):
    path = Path(path).resolve(strict=True)
    if not str(path).startswith("/nix/store/"):
        raise ValueError("expected a Nix-installed executable")
    safe_directory(path.parent)
    info = path.stat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
        raise ValueError("program must be immutable and root-owned")
    verify_contents(path.parent.parent)
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


def unit_identity(path, owner_uid=None):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, "rb") as stream:
        info = os.fstat(stream.fileno())
        if (not stat.S_ISREG(info.st_mode) or info.st_nlink != 1
                or info.st_uid != (os.geteuid() if owner_uid is None else owner_uid)
                or info.st_mode & (0o022 if owner_uid == 0 else 0o077)):
            raise ValueError("unsafe unit file")
        data = stream.read(16385)
        if len(data) > 16384:
            raise ValueError("oversized unit file")
        return info.st_dev, info.st_ino, hashlib.sha256(data).hexdigest()


def install_unit(path, contents, system=False):
    if system and os.geteuid() != 0:
        raise PermissionError("system unit publication requires root")
    safe_directory(path.parent, create=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o644 if system else 0o600)
    with os.fdopen(fd, "wb") as stream:
        if system:
            os.fchmod(stream.fileno(), 0o644)
        stream.write(contents)
        stream.flush()
        os.fsync(stream.fileno())
    return unit_identity(path, owner_uid=0 if system else None)


def remove_unit(path, expected, owner_uid=None):
    safe_directory(path.parent)
    if unit_identity(path, owner_uid) != expected:
        raise ValueError("unit changed; preserve it for inspection")
    path.unlink()
