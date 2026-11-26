"""Tests of ci/engine-pin.sh, the choice of the engine of ontology/ENGINE-PIN (deploy/README.md, "The engine of a pin"),
and of its form --tree, the choice of the engine of a tree without the pin that holds engine/ (specs/ap/check.md R26),
run as a program with a fake `cargo` and fake engines in a temporary directory (no network, no real engine, no file of
/usr/local/bin but the absent engine of the pin of zeros)."""
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SCRIPT = REPO / "ci" / "engine-pin.sh"
PIN = "d35048a568230e4065910ddcea56efa8de1d1db4"
OTHER = "1111111111111111111111111111111111111111"
ZERO = "0" * 40
URL = "https://github.com/park-kyungchan/ontology-platform"


def engine_text(revision):
    return f'#!/bin/sh\nif [ "$1" = "--version" ]; then echo "ont 0.1.0 {revision}"; fi\n'


FAKE_CARGO = r"""#!/bin/sh
python3 -I -c 'import json, sys; open(sys.argv[1], "a").write(json.dumps(sys.argv[2:]) + "\n")' "$FAKE_CALLS" "$@"
if [ "$FAKE_MODE" = fail ]; then
  echo "FAKE-NETWORK-ERROR failed to authenticate"
  echo "FAKE-NETWORK-ERROR failed to authenticate" >&2
  exit 101
fi
root=
while [ "$#" -gt 0 ]; do
  if [ "$1" = --root ]; then root=$2; fi
  shift
done
mkdir -p "$root/bin"
printf '#!/bin/sh\nif [ "$1" = "--version" ]; then echo "ont 0.1.0 %s"; fi\n' "$FAKE_PIN" > "$root/bin/ont"
chmod 0755 "$root/bin/ont"
exit 0
"""


class EnginePin(unittest.TestCase):
    def setUp(self):
        for d in ("/usr/bin", "/bin"):
            self.assertFalse(os.path.exists(os.path.join(d, "ont")), d + "/ont would be found by the cases' PATH")
        self.tmp = Path(tempfile.mkdtemp(prefix="engine-pin-test."))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        (self.tmp / "bin").mkdir()
        (self.tmp / "engines").mkdir()
        self.root = self.tmp / "target" / "engine-pin"
        self.calls = self.tmp / "cargo.calls"

    def engine(self, path, revision, mode=0o755):
        path.write_text(engine_text(revision))
        path.chmod(mode)

    def cargo(self, mode):
        c = self.tmp / "bin" / "cargo"
        c.write_text(FAKE_CARGO)
        c.chmod(0o755)
        self.cargo_mode = mode

    def run_script(self, args=(PIN,), engine_dir="default", pin=PIN):
        env = {"PATH": f"{self.tmp}/bin:/usr/bin:/bin", "HOME": str(self.tmp), "CARGO_TARGET_DIR": str(self.tmp / "target"),
               "FAKE_CALLS": str(self.calls), "FAKE_MODE": getattr(self, "cargo_mode", "ok"), "FAKE_PIN": pin}
        if engine_dir == "default":
            env["CHECK_ENGINE_DIR"] = str(self.tmp / "engines")
        elif engine_dir is not None:
            env["CHECK_ENGINE_DIR"] = engine_dir
        return subprocess.run(["bash", str(SCRIPT), *args], env=env, cwd=self.tmp, capture_output=True, text=True)

    def cargo_calls(self):
        if not self.calls.exists():
            return []
        return [json.loads(line) for line in self.calls.read_text().splitlines()]

    def assert_path_chosen(self, r):
        self.assertEqual((r.returncode, r.stdout, r.stderr),
                         (0, f"{self.tmp}/bin/ont\n", f"check: engine {self.tmp}/bin/ont (path)\n"))
        self.assertEqual(self.cargo_calls(), [])
        self.assertFalse(self.root.exists())

    def test_pinned_engine_is_chosen_and_linked_as_ont(self):
        pinned = self.tmp / "engines" / f"ont-{PIN}"
        self.engine(pinned, PIN)
        self.engine(self.tmp / "bin" / "ont", PIN)
        (self.root / "pinned").mkdir(parents=True)
        other = self.tmp / "other"
        self.engine(other, OTHER)
        os.symlink(other, self.root / "pinned" / "ont")
        self.cargo("ok")
        r = self.run_script()
        out = f"{self.root}/pinned/ont"
        self.assertEqual((r.returncode, r.stdout, r.stderr), (0, out + "\n", f"check: engine {pinned} (pinned)\n"))
        self.assertEqual(os.readlink(out), str(pinned))
        v = subprocess.run([out, "--version"], capture_output=True, text=True)
        self.assertEqual(v.stdout.strip(), f"ont 0.1.0 {PIN}")
        self.assertEqual(os.listdir(os.path.dirname(out)), ["ont"])
        self.assertEqual(self.cargo_calls(), [])
        self.assertEqual([str(p.relative_to(self.root)) for p in sorted(self.root.rglob("*"))], ["pinned", "pinned/ont"])

    def test_pinned_file_not_executable_or_of_another_pin_is_passed_over(self):
        pinned = self.tmp / "engines" / f"ont-{PIN}"
        self.engine(self.tmp / "bin" / "ont", PIN)
        self.cargo("ok")
        self.engine(pinned, PIN, 0o644)
        self.assert_path_chosen(self.run_script())
        self.engine(pinned, OTHER)
        self.assert_path_chosen(self.run_script())

    def test_ont_on_path_of_the_pin_without_a_pinned_file(self):
        self.engine(self.tmp / "bin" / "ont", PIN)
        self.cargo("ok")
        self.assert_path_chosen(self.run_script())

    def test_the_base_build_when_neither_holds(self):
        self.engine(self.tmp / "bin" / "ont", OTHER)
        self.cargo("ok")
        r = self.run_script()
        root = str(self.root)
        self.assertEqual((r.returncode, r.stdout, r.stderr), (0, f"{root}/bin/ont\n", f"check: engine {root}/bin/ont (built)\n"))
        self.assertEqual(self.cargo_calls(), [["install", "--locked", "--git", URL, "--rev", PIN, "--root", root,
                                                "--target-dir", f"{root}/build", "ont"]])
        self.assertTrue((self.root / "install.log").exists())

    def test_a_failed_build_refuses_with_one_line_naming_the_file(self):
        self.engine(self.tmp / "bin" / "ont", OTHER)
        self.cargo("fail")
        r = self.run_script()
        line = (f"engine_missing@ci/engine-pin.sh: {self.tmp}/engines/ont-{PIN} is not the engine of the pin, nor is ont on PATH, "
                f"and the build of the pin failed ({self.root}/install.log); see deploy/README.md, \"The engine of a pin\"\n")
        self.assertEqual((r.returncode, r.stdout, r.stderr), (3, "", line))
        self.assertNotIn("FAKE-NETWORK-ERROR", r.stdout + r.stderr)
        self.assertIn("FAKE-NETWORK-ERROR", (self.root / "install.log").read_text())
        self.assertEqual(len(self.cargo_calls()), 1)

    def test_the_default_directory_is_usr_local_bin(self):
        self.assertFalse(os.path.exists(f"/usr/local/bin/ont-{ZERO}"))
        self.cargo("fail")
        line = (f"engine_missing@ci/engine-pin.sh: /usr/local/bin/ont-{ZERO} is not the engine of the pin, nor is ont on PATH, "
                f"and the build of the pin failed ({self.root}/install.log); see deploy/README.md, \"The engine of a pin\"\n")
        for engine_dir in (None, ""):
            with self.subTest(engine_dir=engine_dir):
                r = self.run_script((ZERO,), engine_dir=engine_dir, pin=ZERO)
                self.assertEqual((r.returncode, r.stdout, r.stderr), (3, "", line))

    def test_bad_arguments_refuse_before_anything(self):
        self.cargo("ok")
        cases = [((), "default", "usage"), ((PIN, PIN), "default", "usage"), (("abc",), "default", "engine_pin_invalid"),
                 ((PIN[:39],), "default", "engine_pin_invalid"), ((PIN.upper(),), "default", "engine_pin_invalid"),
                 ((PIN,), "relative/dir", "engine_dir_invalid")]
        for args, engine_dir, word in cases:
            with self.subTest(args=args, engine_dir=engine_dir):
                r = self.run_script(args, engine_dir=engine_dir)
                self.assertEqual(r.returncode, 2)
                self.assertEqual(r.stdout, "")
                self.assertTrue(r.stderr.startswith(word), r.stderr)
                self.assertEqual(self.cargo_calls(), [])
                self.assertFalse(self.root.exists())


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
    """The form --tree: the engine of the Git tree id of engine/ at HEAD, in the order of R26."""

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

    def run_tree(self, args=("--tree",), mode="ok", built_id=None, cwd=None):
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

    def test_engine_tree_a_failed_build_refuses_with_one_line(self):
        r = self.run_tree(mode="fail")
        line = (f"engine_missing@ci/engine-pin.sh: no engine names the tree {self.id} of engine/ and its build failed "
                f"({self.root}/build.log)\n")
        self.assertEqual((r.returncode, r.stdout, r.stderr), (3, "", line))
        self.assertIn("FAKE-BUILD-ERROR", (self.root / "build.log").read_text())

    def test_engine_tree_bad_calls_refuse_before_anything(self):
        for args, cwd, word in [(("--tree", "x"), None, "usage"), (("--tree",), self.repo / "engine", "engine_tree_unreadable")]:
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
