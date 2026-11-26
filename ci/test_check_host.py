"""Tests of ci/check-host.py on scratch repositories: a clean host/ passes, and each finding names its code and path.

Run: python3 -I -B -m unittest discover -s ci -p test_check_host.py
"""
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SCRIPT = REPO / "ci" / "check-host.py"
GIT = ["git", "-c", "user.name=t", "-c", "user.email=t@e", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"]


class CheckHost(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="check-host-test."))
        self.addCleanup(shutil.rmtree, self.root, True)
        subprocess.run(GIT + ["init", "-q", str(self.root)], check=True)
        self.write("host/deployments/ssh/a.conf", "a\n")
        self.write("host/deployments/ssh/README.md", "readme\n")
        self.write("host/etc/agent-platform/policy.json", "{}\n")
        self.write("host/scripts/x.sh", "echo ok\n")
        self.write("host/scripts/plain", "#!/usr/bin/env bash\necho ok\n")
        self.inventory([("/etc/a.conf", "deployments/ssh/a.conf", "a\n")])

    def write(self, rel, text):
        p = self.root / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text)

    def inventory(self, rows):
        body = [{"source": s, "destination": d, "sha256": hashlib.sha256(t.encode()).hexdigest()} for s, d, t in rows]
        self.write("host/deployments/host-artifacts.json", json.dumps(body))

    def run_check(self):
        subprocess.run(GIT + ["-C", str(self.root), "add", "-A"], check=True)
        return subprocess.run([sys.executable, "-I", "-B", str(SCRIPT), str(self.root)], capture_output=True, text=True)

    def assert_finding(self, line):
        r = self.run_check()
        self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
        self.assertIn(f"check: {line}\n", r.stderr)

    def test_a_clean_tree_passes(self):
        r = self.run_check()
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(r.stdout, "check: host 2 json, 2 shell scripts, 1 inventory rows\n")

    def test_the_repository_s_own_host_passes(self):
        r = subprocess.run([sys.executable, "-I", "-B", str(SCRIPT), str(REPO)], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)

    def test_json_that_does_not_parse(self):
        self.write("host/etc/agent-platform/policy.json", "{")
        self.assert_finding("json_invalid@host/etc/agent-platform/policy.json")

    def test_shell_syntax_by_name_and_by_first_line(self):
        self.write("host/scripts/x.sh", "if then\n")
        self.write("host/scripts/plain", "#!/bin/sh\nif then\n")
        self.assert_finding("shell_syntax@host/scripts/x.sh")
        self.assert_finding("shell_syntax@host/scripts/plain")

    def test_a_copy_whose_digest_differs(self):
        self.write("host/deployments/ssh/a.conf", "changed\n")
        self.assert_finding("digest_mismatch@host/deployments/ssh/a.conf")

    def test_a_missing_copy_and_a_row_without_a_member(self):
        self.inventory([("/etc/a.conf", "deployments/ssh/a.conf", "a\n"), ("/etc/b", "deployments/b", "b\n")])
        self.assert_finding("destination_missing@host/deployments/b")
        self.write("host/deployments/host-artifacts.json", json.dumps([{"source": "/x", "destination": "d"}]))
        self.assert_finding("row_missing_sha256[0]@host/deployments/host-artifacts.json")

    def test_an_inventory_that_is_no_list(self):
        self.write("host/deployments/host-artifacts.json", "{}")
        self.assert_finding("inventory_not_list@host/deployments/host-artifacts.json")

    def test_a_file_under_deployments_without_a_row(self):
        self.write("host/deployments/extra.conf", "x\n")
        self.assert_finding("deployment_unlisted@host/deployments/extra.conf")


if __name__ == "__main__":
    unittest.main()
