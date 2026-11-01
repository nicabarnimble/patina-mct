#!/usr/bin/env python3
import shlex
import importlib.util
import stat
from types import SimpleNamespace
from unittest.mock import Mock, patch
import mother_systemd
from pathlib import Path
import tempfile
import unittest
from mother_systemd import render_unit, install_unit, remove_unit, mount_options

TEMPLATE = Path(__file__).resolve().parent.parent / "nix/mother-admin.service.in"


INSTALLER_SPEC = importlib.util.spec_from_file_location("installer", Path(__file__).with_name("mother-admin-service.py"))
installer = importlib.util.module_from_spec(INSTALLER_SPEC)
INSTALLER_SPEC.loader.exec_module(installer)


class UnitTests(unittest.TestCase):
    def test_removal_uses_the_host_systemctl_not_a_newer_nix_client(self):
        with tempfile.TemporaryDirectory(dir=Path(tempfile.gettempdir()).resolve()) as tmp:
            package = Path('/nix/store/example')
            unit = SimpleNamespace(name='mct-admin-test.service', path=Path(tmp)/'unit', gcroot=Path(tmp)/'gcroot')
            unit.gcroot.symlink_to(package)
            calls = []
            def command(*args):
                calls.append(tuple(map(str, args)))
                if str(args[0]) != '/usr/bin/systemctl':
                    raise RuntimeError('client does not match the system manager')
                return SimpleNamespace(stdout='0\n')
            with patch.object(installer, 'unit_identity', return_value=(1, 2, 'hash')), patch.object(installer, 'run', side_effect=command), patch.object(installer, 'remove_unit') as remove, patch.object(installer, 'trusted_systemctl', return_value=Path('/usr/bin/systemctl'), create=True):
                installer.remove_system_unit(unit, (1, 2, 'hash'), package)
                remove.assert_called_once_with(unit.path, (1, 2, 'hash'), owner_uid=0)
            self.assertEqual([call[1] for call in calls], ['stop', 'show', 'disable', 'daemon-reload'])
            self.assertFalse(unit.gcroot.is_symlink())

    def test_transient_or_vendor_unit_cannot_be_shadowed_by_installation(self):
        unit = mother_systemd.SystemUnit('mct-admin-unpublished-test.service')
        with patch.object(installer.os.path, 'lexists', return_value=False), patch.object(installer, 'run') as command, patch.object(installer, 'trusted_systemctl', return_value=Path('/usr/bin/systemctl')):
            for state in ['loaded\n', 'masked\n', 'bad-setting\n', '']:
                command.return_value = SimpleNamespace(stdout=state)
                with self.assertRaises(FileExistsError):
                    installer.require_new_unit(unit)
            command.return_value = SimpleNamespace(stdout='not-found\n')
            installer.require_new_unit(unit)

    def test_changed_system_unit_is_not_stopped_or_removed(self):
        unit = mother_systemd.SystemUnit("mct-admin-test.service")
        with patch.object(installer, "unit_identity", return_value=(1, 2, "changed")), patch.object(installer, "run") as command:
            with self.assertRaisesRegex(ValueError, "unit changed"):
                installer.remove_system_unit(unit, (1, 2, "original"), Path("/nix/store/package"))
            command.assert_not_called()

    def test_system_publication_requires_root_before_creating_files(self):
        with tempfile.TemporaryDirectory(dir=Path(tempfile.gettempdir()).resolve()) as tmp:
            path = Path(tmp) / "unit.service"
            with patch.object(mother_systemd.os, "geteuid", return_value=1000):
                with self.assertRaises(PermissionError):
                    install_unit(path, b"unit", system=True)
            self.assertFalse(path.exists())

    def test_exec_arguments_cannot_inject_systemd_directives_or_expansions(self):
        unit = render_unit(TEMPLATE.read_text(), "/nix/store/example/bin/mct-daemon",
                           '/private/a "%x$HOME', "mother-test", "ledger-test")
        command = next(line.removeprefix('ExecStart=') for line in unit.splitlines() if line.startswith('ExecStart='))
        self.assertEqual(shlex.split(command)[-2:], ['--bind-address', '127.0.0.1:0'])
        self.assertIn('%%x$$HOME', unit)
        self.assertIn('WorkingDirectory=/\n', unit)
        self.assertIn('KillSignal=SIGINT', unit)
        self.assertIn('NoNewPrivileges=yes', unit)
        self.assertIn('User=pi\nGroup=pi\n', unit)
        self.assertNotIn('PrivateUsers=', unit)
        self.assertIn('ProtectSystem=strict', unit)
        self.assertIn('ReadWritePaths=', unit)
        self.assertIn('PrivateTmp=yes', unit)
        self.assertIn('WantedBy=multi-user.target', unit)
        self.assertIn('RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK', unit)
        self.assertIn('CapabilityBoundingSet=\n', unit)
        for root in ["relative", "/tmp/a\nExecStart=/bin/evil", "/tmp/a\x00"]:
            with self.assertRaises(ValueError):
                render_unit(TEMPLATE.read_text(), "/nix/store/example/bin/mct-daemon", root, "m", "l")

    def test_system_unit_paths_are_fixed_and_names_cannot_escape(self):
        unit = mother_systemd.SystemUnit('mct-admin-test.service')
        self.assertEqual(unit.path, Path('/etc/systemd/system/mct-admin-test.service'))
        self.assertEqual(unit.gcroot, Path('/var/lib/mct/package-roots/mct-admin-test.service'))
        for name in ['../other.service', 'other.service', 'mct-admin-x.service\nUser=root', 'mct-admin-x.target']:
            with self.assertRaises(ValueError):
                mother_systemd.SystemUnit(name)

    def test_root_owned_unit_identity_rejects_writable_or_wrong_owner(self):
        with tempfile.TemporaryDirectory(dir=Path(tempfile.gettempdir()).resolve()) as tmp:
            unit = Path(tmp) / 'unit.service'
            unit.write_bytes(b'unit')
            unit.chmod(0o644)
            with self.assertRaises(ValueError):
                mother_systemd.unit_identity(unit, owner_uid=0)
            metadata = SimpleNamespace(st_uid=0, st_mode=stat.S_IFREG | 0o644, st_nlink=1, st_dev=1, st_ino=2)
            with patch.object(mother_systemd.os, 'fstat', return_value=metadata):
                self.assertEqual(mother_systemd.unit_identity(unit, owner_uid=0)[:2], (1, 2))
                metadata.st_mode |= 0o020
                with self.assertRaises(ValueError):
                    mother_systemd.unit_identity(unit, owner_uid=0)

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
