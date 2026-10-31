#!/usr/bin/env python3
import shlex
import stat
from types import SimpleNamespace
from unittest.mock import Mock, patch
import mother_systemd
from pathlib import Path
import tempfile
import unittest
from mother_systemd import render_unit, install_unit, remove_unit, mount_options

TEMPLATE = Path(__file__).resolve().parent.parent / "nix/mother-admin.service.in"


class UnitTests(unittest.TestCase):
    def test_exec_arguments_cannot_inject_systemd_directives_or_expansions(self):
        unit = render_unit(TEMPLATE.read_text(), "/nix/store/example/bin/mct-daemon",
                           '/private/a "%x$HOME', "mother-test", "ledger-test")
        command = next(line.removeprefix('ExecStart=') for line in unit.splitlines() if line.startswith('ExecStart='))
        self.assertEqual(shlex.split(command)[-2:], ['--bind-address', '127.0.0.1:0'])
        self.assertIn('%%x$$HOME', unit)
        self.assertIn('WorkingDirectory=/\n', unit)
        self.assertIn('KillSignal=SIGINT', unit)
        self.assertIn('NoNewPrivileges=yes', unit)
        self.assertIn('PrivateUsers=yes', unit)
        self.assertIn('RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK', unit)
        self.assertIn('CapabilityBoundingSet=\n', unit)
        for root in ["relative", "/tmp/a\nExecStart=/bin/evil", "/tmp/a\x00"]:
            with self.assertRaises(ValueError):
                render_unit(TEMPLATE.read_text(), "/nix/store/example/bin/mct-daemon", root, "m", "l")

    def test_program_content_verification_must_succeed_before_use(self):
        binary = Path('/nix/store/example/bin/mct-daemon')
        metadata = SimpleNamespace(st_uid=0, st_mode=stat.S_IFREG | 0o555)
        verify = Mock(side_effect=RuntimeError('package content hash mismatch'))
        with patch.object(Path, 'resolve', return_value=binary), patch.object(Path, 'stat', return_value=metadata), patch.object(mother_systemd, 'safe_directory'):
            with self.assertRaisesRegex(RuntimeError, 'content hash mismatch'):
                mother_systemd.trusted_program(binary, verify)
            verify.assert_called_once_with(binary.parent.parent)
            verify.reset_mock(side_effect=True)
            self.assertEqual(mother_systemd.trusted_program(binary, verify), binary)
            verify.assert_called_once_with(binary.parent.parent)

    def test_mount_check_uses_the_effective_ancestor_and_decodes_spaces(self):
        mounts = "1 0 0:1 / / ro - ext4 /dev/root ro\n2 1 0:1 / /home/pi/mother\\040root rw - ext4 /dev/root rw\n"
        self.assertIn("rw", mount_options(mounts, "/home/pi/mother root/ledger"))
        self.assertIn("ro", mount_options(mounts, "/home/pi/mother root-other"))
        self.assertIn("ro", mount_options(mounts, "/home/pi/outside"))

    def test_install_is_exclusive_and_cleanup_refuses_changed_unit(self):
        with tempfile.TemporaryDirectory(dir=Path(tempfile.gettempdir()).resolve()) as tmp:
            root = Path(tmp)
            unit = root / "mct-smoke-test.service"
            original = install_unit(unit, b"test unit\n")
            self.assertEqual(unit.stat().st_mode & 0o777, 0o600)
            with self.assertRaises(FileExistsError):
                install_unit(unit, b"replacement")
            unit.write_bytes(b"changed")
            with self.assertRaises(ValueError):
                remove_unit(unit, original)
            self.assertEqual(unit.read_bytes(), b"changed")

    def test_install_refuses_symlink_and_writable_ancestry(self):
        with tempfile.TemporaryDirectory(dir=Path(tempfile.gettempdir()).resolve()) as tmp:
            root = Path(tmp)
            target = root / "target"
            target.write_bytes(b"preserve")
            link = root / "mct-smoke-link.service"
            link.symlink_to(target)
            with self.assertRaises(FileExistsError):
                install_unit(link, b"replacement")
            self.assertEqual(target.read_bytes(), b"preserve")
            unsafe = root / "unsafe"
            unsafe.mkdir(mode=0o777)
            unsafe.chmod(0o777)
            with self.assertRaises(ValueError):
                install_unit(unsafe / "mct-smoke-test.service", b"bad")


if __name__ == "__main__":
    unittest.main()
