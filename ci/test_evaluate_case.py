"""Tests of evals/engineering/evaluate, the evaluator of an engineering case, run as a program."""
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

EVALUATE = Path(__file__).resolve().parent.parent / "evals" / "engineering" / "evaluate"


def git(repo, *words):
    out = subprocess.run(["git", "-C", str(repo), "-c", "user.name=t", "-c", "user.email=t@e", *words],
                         capture_output=True, text=True, check=True)
    return out.stdout.strip()


class EvaluateCase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        t = Path(self.tmp.name)
        self.clone, self.copy = t / "clone", t / "copy"
        self.copy.mkdir()
        git(t, "init", "-q", str(self.clone))
        (self.clone / "tests").mkdir()
        (self.clone / "tests" / "t.sh").write_text("#!/bin/sh\nexit 0\n")
        (self.clone / "tests" / "t.sh").chmod(0o755)
        os.symlink("t.sh", self.clone / "tests" / "link.sh")
        git(self.clone, "add", "-A")
        git(self.clone, "commit", "-qm", "head")
        self.head = git(self.clone, "rev-parse", "HEAD")
        self.case = t / "tc-1.json"

    def tearDown(self):
        self.tmp.cleanup()

    def evaluate(self, tests):
        self.case.write_text(json.dumps({"head": self.head, "tests": tests, "commands": [["./tests/t.sh"]]}))
        env = {k: v for k, v in os.environ.items() if k not in ("HOST_CARGO_LOCK", "CHECK_LOCKED")}
        return subprocess.run([str(EVALUATE), str(self.case), str(self.clone)], cwd=self.copy, env=env,
                              stdin=subprocess.DEVNULL, capture_output=True, text=True)

    def test_c9_a_test_path_that_is_a_link_at_head_exits_2_and_writes_nothing(self):
        done = self.evaluate(["tests/t.sh", "tests/link.sh"])
        self.assertEqual(done.returncode, 2, done.stderr)
        self.assertEqual(os.listdir(self.copy), [])
        self.assertNotIn("evaluate: command=", done.stdout)

    def test_c6_a_regular_test_file_is_restored_with_its_mode_and_judged(self):
        done = self.evaluate(["tests/t.sh"])
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertTrue(os.access(self.copy / "tests" / "t.sh", os.X_OK))
        self.assertRegex(done.stdout, r"(?m)^evaluate: command=1 exit=0 seconds=\d+$")


if __name__ == "__main__":
    unittest.main()
