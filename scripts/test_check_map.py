import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("check-map.py")


def run(root, *args):
    return subprocess.run(
        [sys.executable, "-I", "-B", str(SCRIPT), *args],
        cwd=str(root),
        capture_output=True,
        text=True,
    )


class CheckMapTest(unittest.TestCase):
    def make(self, components=None, files=None):
        tmp = tempfile.TemporaryDirectory()
        root = Path(tmp.name)
        files = files if files is not None else {"a/x.txt": "one\ntwo\nthree\n", "a/y.txt": "hi\n"}
        for rel, content in files.items():
            p = root / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content)
        if components is None:
            components = [{
                "name": "alpha",
                "purpose": "Alpha component.",
                "paths": ["a/"],
                "entry": ["a/x.txt:1-2"],
                "spec": [],
                "tests": [],
            }]
        (root / "map.json").write_text(json.dumps({"components": components}))
        subprocess.run(["git", "init", "-q"], cwd=str(root), check=True)
        subprocess.run(["git", "add", "-A"], cwd=str(root), check=True)
        self.addCleanup(tmp.cleanup)
        return root

    def test_valid_passes(self):
        root = self.make()
        r = run(root, "a/")
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertEqual(r.stdout, "")

    def test_usage_no_root(self):
        root = self.make()
        r = run(root)
        self.assertEqual(r.returncode, 2)
        self.assertEqual(r.stdout.strip(), "usage: check-map.py <root>...")

    def test_map_unreadable(self):
        root = self.make()
        (root / "map.json").write_text("{not json")
        subprocess.run(["git", "add", "-A"], cwd=str(root), check=True)
        r = run(root, "a/")
        self.assertEqual(r.returncode, 1)
        self.assertTrue(r.stdout.startswith("map_unreadable"), r.stdout)

    def test_component_invalid_bad_name(self):
        root = self.make(components=[{
            "name": "Bad_Name", "purpose": "Alpha component.", "paths": ["a/"],
            "entry": [], "spec": [], "tests": [],
        }])
        r = run(root, "a/")
        self.assertEqual(r.returncode, 1)
        self.assertIn("component_invalid", r.stdout)

    def test_component_invalid_glob_path(self):
        root = self.make(components=[{
            "name": "alpha", "purpose": "Alpha component.", "paths": ["a/*"],
            "entry": [], "spec": [], "tests": [],
        }])
        r = run(root, "a/")
        self.assertEqual(r.returncode, 1)
        self.assertIn("component_invalid", r.stdout)

    def test_name_repeated(self):
        comp = {
            "name": "alpha", "purpose": "Alpha component.", "paths": ["a/"],
            "entry": [], "spec": [], "tests": [],
        }
        root = self.make(components=[comp, dict(comp)])
        r = run(root, "a/")
        self.assertEqual(r.returncode, 1)
        self.assertIn("name_repeated alpha", r.stdout)

    def test_path_missing(self):
        root = self.make(components=[{
            "name": "alpha", "purpose": "Alpha component.", "paths": ["a/", "missing.txt"],
            "entry": [], "spec": [], "tests": [],
        }])
        r = run(root, "a/")
        self.assertEqual(r.returncode, 1)
        self.assertIn("path_missing missing.txt", r.stdout)

    def test_path_missing_root(self):
        root = self.make()
        r = run(root, "nope/")
        self.assertEqual(r.returncode, 1)
        self.assertIn("path_missing nope/", r.stdout)

    def test_entry_out_of_range(self):
        root = self.make(components=[{
            "name": "alpha", "purpose": "Alpha component.", "paths": ["a/"],
            "entry": ["a/x.txt:1-99"], "spec": [], "tests": [],
        }])
        r = run(root, "a/")
        self.assertEqual(r.returncode, 1)
        self.assertIn("entry_out_of_range a/x.txt:1-99", r.stdout)

    def test_file_unmapped_non_ascii_name(self):
        root = self.make(
            components=[{
                "name": "alpha", "purpose": "Alpha component.", "paths": ["a/x.txt"],
                "entry": [], "spec": [], "tests": [],
            }],
            files={"a/x.txt": "one\n", "a/é.txt": "hi\n"},
        )
        r = run(root, "a/")
        self.assertEqual(r.returncode, 1)
        self.assertIn("file_unmapped a/é.txt", r.stdout)

    def test_file_unmapped(self):
        root = self.make(
            components=[{
                "name": "alpha", "purpose": "Alpha component.", "paths": ["a/x.txt"],
                "entry": [], "spec": [], "tests": [],
            }],
            files={"a/x.txt": "one\n", "a/y.txt": "hi\n", "b/z.txt": "zz\n"},
        )
        # b/ is a root; b/z.txt is tracked but unmapped. a/y.txt also unmapped under a/.
        r = run(root, "a/", "b/z.txt")
        self.assertEqual(r.returncode, 1)
        self.assertIn("file_unmapped", r.stdout)


if __name__ == "__main__":
    unittest.main()
