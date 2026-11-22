"""Tests of ci/engine-pin.sh, the choice of the engine of ontology/ENGINE-PIN (deploy/README.md, "The engine of a pin"),
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


if __name__ == "__main__":
    unittest.main()
