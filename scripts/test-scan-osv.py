#!/usr/bin/env python3
import copy
import datetime
import importlib.util
import json
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("scan_osv", Path(__file__).with_name("scan-osv.py"))
scan = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scan)
TODAY = datetime.date(2026, 10, 29)


def finding(path):
    return {"source": {"type": "lockfile", "path": path}, "packages": [{
        "package": {"name": "block-buffer", "version": "0.10.4", "ecosystem": "crates.io"},
        "vulnerabilities": [{"id": scan.ADVISORY}],
        "groups": [{"ids": [scan.ADVISORY], "aliases": [scan.ADVISORY]}],
    }]}


def findings():
    return {"results": [finding(path) for path in sorted(scan.LOCKS)]}


class ScanTests(unittest.TestCase):
    def test_exact_false_positive_only(self):
        report = findings()
        self.assertEqual(scan.scan_status(1, json.dumps(report), TODAY), 0)
        for field, value in (("version", "0.12.0"), ("version", "0.11.0"),
                             ("version", "0.10.3"), ("name", "other"),
                             ("ecosystem", "other")):
            changed = copy.deepcopy(report)
            changed["results"][0]["packages"][0]["package"][field] = value
            self.assertEqual(scan.scan_status(1, json.dumps(changed), TODAY), 1)

    def test_mixed_advisories_errors_expiry_and_unknown_schema_fail(self):
        report = findings()
        package = report["results"][0]["packages"][0]
        package["vulnerabilities"].append({"id": "NEW-ADVISORY"})
        self.assertEqual(scan.scan_status(1, json.dumps(report), TODAY), 1)
        report = findings()
        package = report["results"][0]["packages"][0]
        package["groups"].append({"ids": ["OTHER"], "aliases": ["OTHER"]})
        self.assertEqual(scan.scan_status(1, json.dumps(report), TODAY), 1)
        valid = json.dumps(findings())
        partial = findings()
        partial["results"].pop()
        self.assertEqual(scan.scan_status(1, json.dumps(partial), TODAY), 1)
        self.assertEqual(scan.scan_status(2, valid, TODAY), 1)
        self.assertEqual(scan.scan_status(-9, valid, TODAY), 1)
        self.assertEqual(scan.scan_status(1, valid, scan.REVIEW_BY), 1)
        for output in ("invalid json", "null", "[]", "{}", '{"results":[]}',
                       '{"results":[{"packages":[]}]}'):
            self.assertEqual(scan.scan_status(1, output, TODAY), 1)
        self.assertEqual(scan.scan_status(0, '{"results":[]}', TODAY), 0)


if __name__ == "__main__":
    unittest.main()
