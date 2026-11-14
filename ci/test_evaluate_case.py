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

    def evaluate(self, tests, commands=(("./tests/t.sh",),)):
        self.case.write_text(json.dumps({"head": self.head, "tests": tests, "commands": [list(c) for c in commands]}))
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

    def test_c6_a_parent_the_candidate_made_a_file_is_replaced_and_the_case_is_judged(self):
        (self.copy / "tests").write_text("not a directory")
        done = self.evaluate(["tests/t.sh"])
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertTrue(os.access(self.copy / "tests" / "t.sh", os.X_OK))

    def test_c6_nothing_is_written_through_a_parent_the_candidate_made_a_link(self):
        outside = Path(self.tmp.name) / "outside"
        outside.mkdir()
        os.symlink(outside, self.copy / "tests")
        done = self.evaluate(["tests/t.sh"])
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(os.listdir(outside), [])
        self.assertFalse(os.path.islink(self.copy / "tests"))
        self.assertTrue(os.access(self.copy / "tests" / "t.sh", os.X_OK))

    def test_c6_an_error_of_the_evaluator_itself_exits_2_not_1(self):
        # A path no program can be started with (a NUL byte) raises inside the evaluator: that is
        # no statement about the candidate.
        done = self.evaluate(["tests/t\0.sh"])
        self.assertEqual(done.returncode, 2, done.stderr)
        self.assertIn("evaluate: cannot judge", done.stderr)
        self.assertNotIn("evaluate: command=", done.stdout)

    def test_c10_a_replace_ref_in_the_clone_changes_nothing_the_evaluator_restores(self):
        # A replace ref makes Git read another commit, so another tree and another blob, for `head`.
        git(self.clone, "checkout", "-q", "--detach", self.head)
        (self.clone / "tests" / "t.sh").write_text("#!/bin/sh\nexit 1\n")
        git(self.clone, "commit", "-qam", "fake")
        git(self.clone, "replace", self.head, git(self.clone, "rev-parse", "HEAD"))
        self.assertIn("exit 1", git(self.clone, "show", self.head + ":tests/t.sh"))
        done = self.evaluate(["tests/t.sh"])
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual((self.copy / "tests" / "t.sh").read_text(), "#!/bin/sh\nexit 0\n")

    def test_c12_a_search_path_program_that_cannot_start_is_exit_2(self):
        # The host has no such program: no statement about the candidate, so not exit 1.
        done = self.evaluate(["tests/t.sh"], [["./tests/t.sh"], ["no-such-program-of-the-host"]])
        self.assertEqual(done.returncode, 2, done.stderr)
        self.assertIn("evaluate: cannot judge", done.stderr)
        self.assertRegex(done.stdout, r"(?m)^evaluate: command=1 exit=0 ")
        self.assertNotIn("command=2", done.stdout)

    def test_c12_a_program_of_the_copy_that_cannot_start_is_a_fail(self):
        # A program with a `/` is the candidate's tree: a missing one is a failed command.
        done = self.evaluate(["tests/t.sh"], [["./tests/not-there.sh"]])
        self.assertEqual(done.returncode, 1, done.stderr)
        self.assertRegex(done.stdout, r"(?m)^evaluate: command=1 exit=127 ")


if __name__ == "__main__":
    unittest.main()
