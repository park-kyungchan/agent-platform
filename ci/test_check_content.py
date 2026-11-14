"""Tests of ci/check-content.py: the verbs of `ap` and the sections of crates/ap/README.md that content/ names.

Each test copies what the check reads into a scratch directory, changes one file of content/ there
(and its line of SHA256SUMS, so that only the case under test fails) and runs the copied check.
Run: python3 -I -B -m unittest discover -s ci -p 'test_check_content.py'
"""
import hashlib
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
READ_BY_THE_CHECK = (
    "content",
    "crates/ap/README.md",
    "crates/ap/src/main.rs",
    "crates/ap/src/native/cli.rs",
    "crates/ap/src/native/content_inventory.rs",
    "ci/check-content.py",
)
TARGET = "runtime-references/NAMING.md"


def run_check(root):
    done = subprocess.run([sys.executable, "-I", "-B", str(root / "ci/check-content.py")], capture_output=True, text=True, timeout=120)
    return done.returncode, done.stdout


class CheckContent(unittest.TestCase):
    def fixture(self):
        scratch = Path(tempfile.mkdtemp(prefix="check-content-test."))
        self.addCleanup(shutil.rmtree, scratch, True)
        for name in READ_BY_THE_CHECK:
            source, target = REPO / name, scratch / name
            target.parent.mkdir(parents=True, exist_ok=True)
            if source.is_dir():
                shutil.copytree(source, target)
            else:
                shutil.copy2(source, target)
        return scratch

    def append(self, root, line):
        """Append one line to a file of content/ and return its line number there."""
        path = root / "content" / TARGET
        text = path.read_text(encoding="utf-8")
        if not text.endswith("\n"):
            text += "\n"
        path.write_text(text + line + "\n", encoding="utf-8")
        sums = root / "content/SHA256SUMS"
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.write_text("".join(f"{digest}  {TARGET}\n" if l.endswith("  " + TARGET) else l + "\n" for l in sums.read_text().splitlines()))
        return text.count("\n") + 1

    def test_the_repository_passes(self):
        code, out = run_check(REPO)
        self.assertEqual((code, out.splitlines()[-1].endswith(" 0 errors")), (0, True), out)

    def test_a_verb_the_command_has_passes(self):
        root = self.fixture()
        self.append(root, 'Run `ap unit < request.json`, `ap run`, `ap gate` and `ap work start`; see `crates/ap/README.md` sections "One stage" and "The gate" and README "One unit".')
        code, out = run_check(root)
        self.assertEqual(code, 0, out)

    def test_a_verb_the_command_does_not_have_fails_with_file_and_line(self):
        root = self.fixture()
        line = self.append(root, "Start it with `ap frobnicate < request.json`.")
        code, out = run_check(root)
        self.assertEqual(code, 1, out)
        self.assertIn(f"unknown_ap_verb@content/{TARGET}:{line}: `ap frobnicate`", out)

    def test_a_readme_section_that_does_not_exist_fails_with_file_and_line(self):
        root = self.fixture()
        line = self.append(root, 'The contract is `crates/ap/README.md`, section "One stage" and "No such part".')
        code, out = run_check(root)
        self.assertEqual(code, 1, out)
        self.assertIn(f'unknown_readme_section@content/{TARGET}:{line}: "No such part"', out)
        self.assertNotIn('"One stage"', out)

    def test_a_section_named_after_the_word_readme_is_read_too(self):
        root = self.fixture()
        line = self.append(root, '(README "One worker run" and README "Gone")')
        code, out = run_check(root)
        self.assertEqual(code, 1, out)
        self.assertIn(f'unknown_readme_section@content/{TARGET}:{line}: "Gone"', out)


if __name__ == "__main__":
    unittest.main()
