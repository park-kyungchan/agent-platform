import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("check-spec-trace.py")
REPO = Path(__file__).resolve().parent.parent
LIST = "scripts/spec-trace-untested.txt"

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
            "specs/ont/a.md": "# A\n\n## Requirements\n\n- R1: one.\n- R2: two.\n",
            "tests/a.rs": f"// {cite('specs/ont/a.md', 1)}\n// {cite('specs/ont/a.md', 2)}\nfn t() {{}}\n",
        }

    def test_the_repository_passes(self):
        r = run(str(REPO))
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        last = r.stdout.splitlines()[-1]
        self.assertRegex(
            last,
            r"^spec-trace: [0-9]+ requirements, [0-9]+ citations, [0-9]+ listed untested, 0 errors$",
        )

    def test_a_traced_scratch_repository_passes_and_counts(self):
        root = self.make(self.base())
        r = run(str(root))
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertEqual(r.stdout, "spec-trace: 2 requirements, 2 citations, 0 listed untested, 0 errors\n")
        # The default root is the current directory.
        r = run(cwd=root)
        self.assertEqual(r.returncode, 0, r.stdout)

    def test_a_requirement_no_file_cites_is_untested(self):
        files = self.base()
        files["tests/a.rs"] = f"// {cite('specs/ont/a.md', 1)}\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertEqual(
            r.stdout,
            "requirement_untested@specs/ont/a.md#R2\n"
            "spec-trace: 2 requirements, 1 citations, 0 listed untested, 1 errors\n",
        )

    def test_a_listed_requirement_is_not_untested_and_is_counted(self):
        files = self.base()
        files["tests/a.rs"] = f"// {cite('specs/ont/a.md', 1)}\n"
        files[LIST] = "specs/ont/a.md#R2\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertEqual(r.stdout, "spec-trace: 2 requirements, 1 citations, 1 listed untested, 0 errors\n")

    def test_a_citation_of_a_missing_id_or_a_missing_file_is_missing_with_its_line(self):
        files = self.base()
        files["tests/b.sh"] = (
            "#!/bin/sh\n"
            f"# {cite('specs/ont/a.md', 9)}\n"
            f"# {cite('specs/ont/none.md', 1)} and {cite('specs/ont/a.md', 1)}\n"
        )
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertEqual(
            r.stdout.splitlines(),
            [
                "citation_missing@tests/b.sh:2 specs/ont/a.md#R9",
                "citation_missing@tests/b.sh:3 specs/ont/none.md#R1",
                "spec-trace: 2 requirements, 5 citations, 0 listed untested, 2 errors",
            ],
        )

    def test_an_id_twice_in_one_file_is_a_duplicate(self):
        files = self.base()
        files["specs/ont/a.md"] += "- R2: two again.\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertIn("requirement_duplicate@specs/ont/a.md#R2\n", r.stdout)
        self.assertTrue(r.stdout.endswith(" errors\n"))
        self.assertNotIn(" 0 errors", r.stdout)

    def test_a_listed_id_that_a_citation_names_is_a_fault(self):
        files = self.base()
        files[LIST] = "specs/ont/a.md#R1\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertEqual(
            r.stdout,
            "untested_listed_cited@specs/ont/a.md#R1\n"
            "spec-trace: 2 requirements, 2 citations, 1 listed untested, 1 errors\n",
        )

    def test_a_listed_id_that_is_no_requirement_is_a_fault(self):
        files = self.base()
        files[LIST] = "specs/ont/a.md#R7\nspecs/ont/none.md#R1\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertEqual(
            r.stdout,
            "untested_listed_unknown@specs/ont/a.md#R7\n"
            "untested_listed_unknown@specs/ont/none.md#R1\n"
            "spec-trace: 2 requirements, 2 citations, 2 listed untested, 2 errors\n",
        )

    def test_an_unsorted_or_repeating_list_is_a_fault_with_its_line(self):
        files = self.base()
        files["specs/ont/a.md"] += "- R3: three.\n- R4: four.\n"
        files[LIST] = "specs/ont/a.md#R4\nspecs/ont/a.md#R3\nspecs/ont/a.md#R3\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertEqual(
            r.stdout,
            f"untested_list_unsorted@{LIST}:2\n"
            f"untested_list_unsorted@{LIST}:3\n"
            "spec-trace: 4 requirements, 2 citations, 2 listed untested, 2 errors\n",
        )

    def test_a_missing_or_untracked_list_is_an_empty_list(self):
        files = self.base()
        files["tests/a.rs"] = f"// {cite('specs/ont/a.md', 1)}\n"
        root = self.make(files, untracked={LIST: "specs/ont/a.md#R2\n"})
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertEqual(
            r.stdout,
            "requirement_untested@specs/ont/a.md#R2\n"
            "spec-trace: 2 requirements, 1 citations, 0 listed untested, 1 errors\n",
        )

    def test_a_citation_inside_markdown_counts_for_nothing(self):
        files = self.base()
        files["tests/a.rs"] = f"// {cite('specs/ont/a.md', 1)}\n"
        files["notes.md"] = f"{cite('specs/ont/a.md', 2)}\n{cite('specs/ont/none.md', 3)}\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 1)
        self.assertEqual(
            r.stdout,
            "requirement_untested@specs/ont/a.md#R2\n"
            "spec-trace: 2 requirements, 1 citations, 0 listed untested, 1 errors\n",
        )

    def test_a_requirement_line_outside_specs_ont_is_not_a_requirement(self):
        files = self.base()
        files["specs/other/c.md"] = "- R1: elsewhere.\n"
        root = self.make(files)
        r = run(str(root))
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertEqual(r.stdout, "spec-trace: 2 requirements, 2 citations, 0 listed untested, 0 errors\n")

    def test_only_tracked_files_are_read(self):
        files = self.base()
        root = self.make(
            files,
            untracked={
                "specs/ont/new.md": "- R1: untracked.\n",
                "tests/new.rs": f"// {cite('specs/ont/missing.md', 1)}\n",
            },
        )
        r = run(str(root))
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertEqual(r.stdout, "spec-trace: 2 requirements, 2 citations, 0 listed untested, 0 errors\n")

    def test_faults_are_sorted(self):
        files = self.base()
        files["specs/ont/b.md"] = "- R1: b.\n"
        files["tests/a.rs"] = f"// {cite('specs/ont/z.md', 1)}\n"
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
