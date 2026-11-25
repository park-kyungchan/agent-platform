import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("check-spec-trace.py")
REPO = Path(__file__).resolve().parent.parent

# A planted citation is built from parts here, so that this file holds none in the literal form.
WORD = "spec" + ":"


def cite(spec_path, n):
    return f"{WORD} {spec_path}#R{n}"


def run(*args, cwd=None):
    return subprocess.run(
        [sys.executable, "-I", "-B", str(SCRIPT), *args],
        cwd=str(cwd) if cwd else None,
        capture_output=True,
        text=True,
    )


class CheckSpecTraceTest(unittest.TestCase):
    def make(self, files, untracked=None):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        for rel, content in files.items():
            p = root / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content)
        subprocess.run(["git", "init", "-q"], cwd=str(root), check=True)
        subprocess.run(["git", "add", "-A"], cwd=str(root), check=True)
        for rel, content in (untracked or {}).items():
            p = root / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content)
        return root

    def base(self):
        return {
            "specs/ap/a.md": "# A\n\n## Requirements\n\n- R1: one.\n- R2: two.\n",
            "specs/content/c.md": "# C\n\n- R1: content.\n",
            "tests/a.rs": f"// {cite('specs/ap/a.md', 1)}\n// {cite('specs/ap/a.md', 2)}\nfn t() {{}}\n",
        }

    def test_the_repository_passes(self):
        r = run(str(REPO))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        last = r.stdout.splitlines()[-1]
        self.assertRegex(last, r"^spec-trace: [0-9]+ requirements, [0-9]+ citations, 0 errors$")

    def test_a_traced_scratch_repository_passes_and_counts(self):
        root = self.make(self.base())
        r = run(str(root))
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertEqual(r.stdout, "spec-trace: 2 requirements, 2 citations, 0 errors\n")
        # The default root is the current directory.
        r = run(cwd=root)
        self.assertEqual(r.returncode, 0, r.stdout)

    def test_a_requirement_no_file_cites_is_untested(self):
        files = self.base()
        files["tests/a.rs"] = f"// {cite('specs/ap/a.md', 1)}\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertEqual(
            r.stdout,
            "requirement_untested@specs/ap/a.md#R2\n"
            "spec-trace: 2 requirements, 1 citations, 1 errors\n",
        )

    def test_a_citation_of_a_missing_id_or_a_missing_file_is_missing_with_its_line(self):
        files = self.base()
        files["tests/b.sh"] = (
            "#!/bin/sh\n"
            f"# {cite('specs/ap/a.md', 9)}\n"
            f"# {cite('specs/ap/none.md', 1)} and {cite('specs/ap/a.md', 1)}\n"
        )
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertEqual(
            r.stdout.splitlines(),
            [
                "citation_missing@tests/b.sh:2 specs/ap/a.md#R9",
                "citation_missing@tests/b.sh:3 specs/ap/none.md#R1",
                "spec-trace: 2 requirements, 5 citations, 2 errors",
            ],
        )

    def test_a_retired_requirement_is_never_untested_and_never_cited(self):
        files = self.base()
        files["specs/ap/a.md"] += "- R3: Retired with the runtimes codex and muse.\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertEqual(r.stdout, "spec-trace: 3 requirements, 2 citations, 0 errors\n")
        # A citation of a retired requirement is a fault with its line.
        files["tests/b.rs"] = f"// x\n// {cite('specs/ap/a.md', 3)}\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertEqual(
            r.stdout,
            "citation_retired@tests/b.rs:2 specs/ap/a.md#R3\n"
            "spec-trace: 3 requirements, 3 citations, 1 errors\n",
        )
        # Only the one sentence retires: a longer text, or another sentence, is a requirement.
        for text in [
            "Retired with the runtimes codex and muse. Its tests left.",
            "Retired.",
            "retired with the runtimes codex and muse.",
            "The runtime codex. Retired with it.",
        ]:
            files = self.base()
            files["specs/ap/a.md"] += f"- R3: {text}\n"
            root = self.make(files)
            r = run(str(root))
            self.assertEqual(r.returncode, 1, text)
            self.assertEqual(
                r.stdout,
                "requirement_untested@specs/ap/a.md#R3\n"
                "spec-trace: 3 requirements, 2 citations, 1 errors\n",
                text,
            )

    def test_an_id_twice_in_one_file_is_a_duplicate(self):
        files = self.base()
        files["specs/ap/a.md"] += "- R2: two again.\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertIn("requirement_duplicate@specs/ap/a.md#R2\n", r.stdout)
        self.assertTrue(r.stdout.endswith(" errors\n"))
        self.assertNotIn(" 0 errors", r.stdout)

    def test_a_citation_inside_markdown_counts_for_nothing(self):
        files = self.base()
        files["tests/a.rs"] = f"// {cite('specs/ap/a.md', 1)}\n"
        files["notes.md"] = f"{cite('specs/ap/a.md', 2)}\n{cite('specs/ap/none.md', 3)}\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertEqual(
            r.stdout,
            "requirement_untested@specs/ap/a.md#R2\n"
            "spec-trace: 2 requirements, 1 citations, 1 errors\n",
        )

    def test_a_citation_of_an_id_outside_specs_ap_passes_and_is_never_untested(self):
        files = self.base()
        files["tests/c.py"] = f"# {cite('specs/content/c.md', 1)}\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertEqual(r.stdout, "spec-trace: 2 requirements, 3 citations, 0 errors\n")
        # specs/content/ ids are checked for existence but are never untested.
        files["tests/c.py"] = f"# {cite('specs/content/c.md', 2)}\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertIn("citation_missing@tests/c.py:1 specs/content/c.md#R2\n", r.stdout)

    def test_only_tracked_files_are_read(self):
        files = self.base()
        root = self.make(
            files,
            untracked={
                "specs/ap/new.md": "- R1: untracked.\n",
                "tests/new.rs": f"// {cite('specs/ap/missing.md', 1)}\n",
            },
        )
        r = run(str(root))
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertEqual(r.stdout, "spec-trace: 2 requirements, 2 citations, 0 errors\n")

    def test_every_path_under_engine_is_skipped(self):
        files = self.base()
        files["engine/specs/ap/e.md"] = "- R1: the engine's own.\n"
        files["engine/specs/ont/cli.md"] = "- R1: engine.\n"
        files["engine/tests/e.rs"] = f"// {cite('specs/ont/cli.md', 1)}\n// {cite('specs/ap/a.md', 9)}\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertEqual(r.stdout, "spec-trace: 2 requirements, 2 citations, 0 errors\n")
        # Outside engine/ the same citation is still missing.
        files["tests/e.rs"] = f"// {cite('specs/ont/cli.md', 1)}\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertIn("citation_missing@tests/e.rs:1 specs/ont/cli.md#R1\n", r.stdout)

    def test_faults_are_sorted(self):
        files = self.base()
        files["specs/ap/b.md"] = "- R1: b.\n"
        files["tests/a.rs"] = f"// {cite('specs/ap/z.md', 1)}\n"
        root = self.make(files)
        r = run(str(root))
        faults = r.stdout.splitlines()[:-1]
        self.assertEqual(faults, sorted(faults))
        self.assertEqual(len(faults), 4, r.stdout)

    def test_a_bad_argument_or_no_repository_is_unreadable(self):
        r = run("a", "b")
        self.assertEqual(r.returncode, 2)
        self.assertTrue(r.stdout.startswith("spec_trace_unreadable "), r.stdout)
        with tempfile.TemporaryDirectory() as d:
            r = run(str(Path(d) / "absent"))
            self.assertEqual(r.returncode, 2)
            self.assertTrue(r.stdout.startswith("spec_trace_unreadable "), r.stdout)
            r = run(d)
            self.assertEqual(r.returncode, 2)
            self.assertEqual(r.stdout, "spec_trace_unreadable git ls-files failed\n")


if __name__ == "__main__":
    unittest.main()
