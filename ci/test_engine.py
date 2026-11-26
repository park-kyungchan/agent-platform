"""Tests of ci/engine.sh, the choice of the engine of the Git tree id of engine/ (specs/ap/check.md R26), run as a
program with a fake `cargo` and fake engines in a temporary directory (no network, no real engine)."""
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SCRIPT = REPO / "ci" / "engine.sh"
OTHER = "1111111111111111111111111111111111111111"
# The name of the file of the retired rule, spelled so that no check of the repository finds the word.
OLD_PIN = "ENGINE" + "-PIN"


FAKE_TREE_CARGO = r"""#!/bin/sh
python3 -I -c 'import json, os, sys; open(sys.argv[1], "a").write(json.dumps([os.getcwd(), os.environ.get("CARGO_TARGET_DIR", "")] + sys.argv[2:]) + "\n")' "$FAKE_CALLS" "$@"
if [ "$FAKE_MODE" = fail ]; then
  echo "FAKE-BUILD-ERROR" >&2
  exit 101
fi
mkdir -p "$CARGO_TARGET_DIR/debug"
printf '#!/bin/sh\nif [ "$1" = "--version" ]; then echo "ont 0.1.0 %s"; fi\n' "$FAKE_ID" > "$CARGO_TARGET_DIR/debug/ont"
chmod 0755 "$CARGO_TARGET_DIR/debug/ont"
exit 0
"""


def git(cwd, *args):
    env = {"PATH": "/usr/bin:/bin", "HOME": str(cwd), "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null"}
    return subprocess.run(["git", *args], cwd=cwd, env=env, check=True, capture_output=True, text=True).stdout.strip()


# spec: specs/ap/check.md#R26
class EngineTree(unittest.TestCase):
    """The engine of the Git tree id of engine/ at HEAD, in the order of R26."""

    def setUp(self):
        for d in ("/usr/bin", "/bin"):
            self.assertFalse(os.path.exists(os.path.join(d, "ont")), d + "/ont would be found by the cases' PATH")
        self.tmp = Path(tempfile.mkdtemp(prefix="engine-tree-test."))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        for d in ("bin", "engines", "repo/engine"):
            (self.tmp / d).mkdir(parents=True)
        self.repo = self.tmp / "repo"
        (self.repo / "engine" / "Cargo.toml").write_text("[workspace]\n")
        git(self.repo, "init", "-q", "-b", "main")
        git(self.repo, "add", "-A")
        git(self.repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-qm", "one")
        self.id = git(self.repo, "rev-parse", "HEAD:engine")
        self.root = self.repo / "target" / "engine-tree"
        self.calls = self.tmp / "cargo.calls"
        c = self.tmp / "bin" / "cargo"
        c.write_text(FAKE_TREE_CARGO)
        c.chmod(0o755)

    def engine(self, path, revision, version="0.1.0"):
        path.write_text(f'#!/bin/sh\nif [ "$1" = "--version" ]; then echo "ont {version} {revision}"; fi\n')
        path.chmod(0o755)

    def run_tree(self, args=(), mode="ok", built_id=None, cwd=None):
        env = {"PATH": f"{self.tmp}/bin:/usr/bin:/bin", "HOME": str(self.tmp), "CHECK_ENGINE_DIR": str(self.tmp / "engines"),
               "FAKE_CALLS": str(self.calls), "FAKE_MODE": mode, "FAKE_ID": built_id or self.id,
               "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null"}
        return subprocess.run(["bash", str(SCRIPT), *args], env=env, cwd=cwd or self.repo, capture_output=True, text=True)

    def cargo_calls(self):
        if not self.calls.exists():
            return []
        return [json.loads(line) for line in self.calls.read_text().splitlines()]

    def test_engine_tree_the_file_of_the_tree_id_is_taken_first_through_a_link(self):
        named = self.tmp / "engines" / f"ont-{self.id}"
        self.engine(named, self.id, "0.2.0")
        self.engine(self.tmp / "bin" / "ont", self.id)
        r = self.run_tree()
        out = f"{self.root}/tree/ont"
        self.assertEqual((r.returncode, r.stdout, r.stderr), (0, out + "\n", f"check: engine {named} (tree)\n"))
        self.assertEqual(os.readlink(out), str(named))
        self.assertEqual(self.cargo_calls(), [])

    def test_engine_tree_the_ont_on_path_then_the_installed_ont(self):
        self.engine(self.tmp / "engines" / f"ont-{self.id}", OTHER)
        self.engine(self.tmp / "bin" / "ont", self.id)
        r = self.run_tree()
        self.assertEqual((r.returncode, r.stdout, r.stderr),
                         (0, f"{self.tmp}/bin/ont\n", f"check: engine {self.tmp}/bin/ont (path)\n"))
        self.engine(self.tmp / "bin" / "ont", OTHER)
        installed = self.tmp / "engines" / "ont"
        self.engine(installed, self.id)
        r = self.run_tree()
        self.assertEqual((r.returncode, r.stdout, r.stderr),
                         (0, f"{self.root}/tree/ont\n", f"check: engine {installed} (installed)\n"))
        self.assertEqual(self.cargo_calls(), [])

    def test_engine_tree_a_pin_line_of_another_id_is_passed_over_and_engine_is_built_once(self):
        self.engine(self.tmp / "bin" / "ont", OTHER)
        r = self.run_tree()
        built = f"{self.root}/build/debug/ont"
        self.assertEqual((r.returncode, r.stdout, r.stderr), (0, built + "\n", f"check: engine {built} (built)\n"))
        self.assertEqual(self.cargo_calls(), [[str(self.repo / "engine"), f"{self.root}/build", "build", "--locked", "-p", "ont"]])

    def test_engine_tree_a_modified_engine_is_always_built(self):
        self.engine(self.tmp / "engines" / f"ont-{self.id}", self.id)
        self.engine(self.tmp / "bin" / "ont", self.id)
        (self.repo / "engine" / "Cargo.toml").write_text("[workspace]\n# edited\n")
        r = self.run_tree(built_id="unknown")
        built = f"{self.root}/build/debug/ont"
        self.assertEqual((r.returncode, r.stdout), (0, built + "\n"))
        self.assertEqual(len(self.cargo_calls()), 1)
        v = subprocess.run([built, "--version"], capture_output=True, text=True)
        self.assertEqual(v.stdout.strip(), "ont 0.1.0 unknown")

    def test_engine_tree_a_tree_that_still_holds_the_old_pin_file_is_read_by_the_tree_rule(self):
        (self.repo / "ontology").mkdir()
        (self.repo / "ontology" / OLD_PIN).write_text(OTHER + "\n")
        git(self.repo, "add", "-A")
        git(self.repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-qm", "pin")
        self.engine(self.tmp / "bin" / "ont", self.id)
        self.engine(self.tmp / "engines" / f"ont-{OTHER}", OTHER)
        r = self.run_tree()
        self.assertEqual((r.returncode, r.stdout, r.stderr),
                         (0, f"{self.tmp}/bin/ont\n", f"check: engine {self.tmp}/bin/ont (path)\n"))
        self.assertEqual(self.cargo_calls(), [])

    def test_engine_tree_a_failed_build_refuses_with_one_line(self):
        r = self.run_tree(mode="fail")
        line = (f"engine_missing@ci/engine.sh: no engine names the tree {self.id} of engine/ and its build failed "
                f"({self.root}/build.log)\n")
        self.assertEqual((r.returncode, r.stdout, r.stderr), (3, "", line))
        self.assertIn("FAKE-BUILD-ERROR", (self.root / "build.log").read_text())

    def test_engine_tree_bad_calls_refuse_before_anything(self):
        for args, cwd, word in [(("x",), None, "usage"), (("--tree",), None, "usage"),
                                ((), self.repo / "engine", "engine_tree_unreadable")]:
            with self.subTest(args=args, cwd=cwd):
                r = self.run_tree(args, cwd=cwd)
                self.assertEqual((r.returncode, r.stdout), (2, ""))
                self.assertTrue(r.stderr.startswith(word), r.stderr)
        git(self.repo, "rm", "-rq", "engine")
        git(self.repo, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-qm", "two")
        r = self.run_tree()
        self.assertEqual(r.returncode, 2)
        self.assertTrue(r.stderr.startswith("engine_tree_unreadable"), r.stderr)
        self.assertEqual(self.cargo_calls(), [])


if __name__ == "__main__":
    unittest.main()
