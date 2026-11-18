"""Tests of ci/check-content.py: the verbs of `ap` and the sections of crates/ap/README.md that content/ names.

Each test copies what the check reads into a scratch directory, changes one file of content/ there
(and its line of SHA256SUMS, so that only the case under test fails) and runs the copied check.
Run: python3 -I -B -m unittest discover -s ci -p 'test_check_content.py'
"""
import hashlib
import os
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
    "ci/check-content.py",
)
TARGET = "runtime-references/NAMING.md"


def run_check(root, corpus=None):
    env = dict(os.environ, CONTENT_CHECK_CORPUS=str(corpus or (root / "no-corpus")))
    done = subprocess.run([sys.executable, "-I", "-B", str(root / "ci/check-content.py")], capture_output=True, text=True, timeout=120, env=env)
    return done.returncode, done.stdout


class Scratch:
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


class CheckContent(Scratch, unittest.TestCase):
    def test_the_repository_passes(self):
        code, out = run_check(REPO)
        self.assertEqual((code, out.splitlines()[-1].endswith(" 0 errors")), (0, True), out)

    def test_a_verb_the_command_has_passes(self):
        root = self.fixture()
        self.append(root, 'Run `ap unit < request.json`, `ap run`, `ap gate` and `ap deliver`; see `crates/ap/README.md` sections "One stage" and "The gate" and README "One unit".')
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
        line = self.append(root, '(README "One unit" and README "Gone")')
        code, out = run_check(root)
        self.assertEqual(code, 1, out)
        self.assertIn(f'unknown_readme_section@content/{TARGET}:{line}: "Gone"', out)


class ModelNamesAndLimits(Scratch, unittest.TestCase):
    """Only the Lead's table of the allowed models may name a model; the Lead's and report's files have byte limits."""

    MODEL_TABLE = "skills/lead/references/unit-page.md"

    def append_to(self, root, name, line):
        path = root / "content" / name
        text = path.read_text(encoding="utf-8")
        path.write_text(text + ("" if text.endswith("\n") else "\n") + line + "\n", encoding="utf-8")
        sums = root / "content/SHA256SUMS"
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.write_text("".join(f"{digest}  {name}\n" if l.endswith("  " + name) else l + "\n" for l in sums.read_text().splitlines()))

    def test_a_model_name_in_the_model_table_passes(self):
        root = self.fixture()
        self.append_to(root, self.MODEL_TABLE, "| Claude Code | Opus | another line of the table, with claude-sonnet-5-5 |")
        code, out = run_check(root)
        self.assertEqual(code, 0, out)

    def test_a_model_name_in_any_other_owned_file_fails(self):
        for name in ("skills/lead/SKILL.md", "skills/lead/references/judging.md", "skills/report/SKILL.md", TARGET, "AGENTS.md"):
            with self.subTest(name=name):
                root = self.fixture()
                self.append_to(root, name, "Run the verifier on Opus.")
                code, out = run_check(root)
                self.assertEqual(code, 1, out)
                self.assertIn(f"model_name_in_content@content/{name}: 'Opus'", out)
                self.assertEqual(out.count("model_name_in_content@"), 1, out)

    def test_the_lead_skill_and_its_references_have_byte_limits(self):
        for name, limit in (("skills/lead/SKILL.md", 6500), ("skills/report/SKILL.md", 6500), ("skills/lead/references/interview.md", 8000)):
            with self.subTest(name=name):
                root = self.fixture()
                size = len((root / "content" / name).read_bytes())
                self.append_to(root, name, "x" * (limit - size))
                code, out = run_check(root)
                self.assertEqual(code, 1, out)
                self.assertIn(f"file_over_limit@content/{name}: {limit + 1} bytes, limit {limit}", out)


class PathsAndDesignRoutes(Scratch, unittest.TestCase):
    """An elided path, a path into a main checkout's files, and the line ranges of OFFICIAL-DOCS.md section 4."""

    ROUTES = "runtime-references/OFFICIAL-DOCS.md"

    def add_route(self, root, row):
        path = root / "content" / self.ROUTES
        head = path.read_text(encoding="utf-8").split("\n## 4. ", 1)[0]
        path.write_text(head + "\n## 4. Local copies of the design sources\n\n| Question | Start | Also |\n| --- | --- | --- |\n" + row + "\n", encoding="utf-8")
        sums = root / "content/SHA256SUMS"
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.write_text("".join(f"{digest}  {self.ROUTES}\n" if l.endswith("  " + self.ROUTES) else l + "\n" for l in sums.read_text().splitlines()))

    def corpus(self, root):
        corpus = root / "corpus"
        (corpus / "docs/area").mkdir(parents=True)
        (corpus / "docs/area/one.md").write_text("".join(f"line {n}\n" for n in range(1, 10)), encoding="utf-8")
        return corpus

    def test_an_elided_path_fails(self):
        root = self.fixture()
        line = self.append(root, "Read `/home/palantirkc/work/ws-example/\u2026/REPORT.md` first.")
        code, out = run_check(root)
        self.assertEqual(code, 1, out)
        self.assertIn(f"elided_path@content/{TARGET}:{line}", out)

    def test_a_path_into_a_main_checkout_fails_and_a_pattern_passes(self):
        root = self.fixture()
        line = self.append(root, "Read `/home/palantirkc/platforms/agent-platform/crates/ap/src/run.rs`.")
        self.append(root, "Read `git -C /home/palantirkc/platforms/<repo> show origin/main:<path>`.")
        code, out = run_check(root)
        self.assertEqual(code, 1, out)
        self.assertIn(f"main_checkout_path@content/{TARGET}:{line}", out)
        self.assertEqual(out.count("main_checkout_path@"), 1, out)

    def test_a_corpus_file_name_is_no_model_name(self):
        root = self.fixture()
        self.append(root, "Cite `docs/area/10-anthropic-claude-fable-and-mythos-5-2.md:1-9`.")
        code, out = run_check(root)
        self.assertEqual(code, 0, out)

    def test_design_routes_inside_their_file_pass(self):
        root = self.fixture()
        self.add_route(root, "| A question | `docs/area/one.md:2-5` | `:7`; `:1-9` |")
        code, out = run_check(root, self.corpus(root))
        self.assertEqual(code, 0, out)
        self.assertIn("design routes:", out)

    def test_a_design_route_past_its_file_or_to_no_file_fails(self):
        root = self.fixture()
        self.add_route(root, "| A question | `docs/area/one.md:2-50` | `docs/area/none.md:1-2` |")
        code, out = run_check(root, self.corpus(root))
        self.assertEqual(code, 1, out)
        self.assertIn("design_route_out_of_range@content/runtime-references/OFFICIAL-DOCS.md: docs/area/one.md:2-50", out)
        self.assertIn("design_route_missing_file@content/runtime-references/OFFICIAL-DOCS.md: docs/area/none.md", out)

    def test_without_the_corpus_the_routes_are_not_read(self):
        root = self.fixture()
        self.add_route(root, "| A question | `docs/area/one.md:2-50` | — |")
        code, out = run_check(root)
        self.assertEqual(code, 0, out)
        self.assertNotIn("design routes:", out)

    def test_bare_ranges_and_several_ranges_in_a_cell_are_checked(self):
        root = self.fixture()
        self.add_route(root, "| A question | `docs/area/one.md:2-5, :7, :8-99` | `:12`; `:1-9` |")
        code, out = run_check(root, self.corpus(root))
        self.assertEqual(code, 1, out)
        self.assertIn("design_route_out_of_range@content/runtime-references/OFFICIAL-DOCS.md: docs/area/one.md:8-99", out)
        self.assertIn("design_route_out_of_range@content/runtime-references/OFFICIAL-DOCS.md: docs/area/one.md:12-12", out)
        self.assertEqual(out.count("design_route_out_of_range@"), 2, out)
        self.assertIn("design routes: 5 line ranges checked", out)

    def test_a_range_with_no_path_before_it_fails(self):
        root = self.fixture()
        self.add_route(root, "| A question | `:3-4` | `docs/area/one.md:1-2` |")
        code, out = run_check(root, self.corpus(root))
        self.assertEqual(code, 1, out)
        self.assertIn("design_route_missing_file@content/runtime-references/OFFICIAL-DOCS.md: :3-4 (a range with no path before it)", out)


    def test_a_missing_route_section_fails_where_the_corpus_is(self):
        root = self.fixture()
        path = root / "content" / self.ROUTES
        path.write_text(path.read_text(encoding="utf-8").replace("\n## 4. ", "\n## 9. "), encoding="utf-8")
        sums = root / "content/SHA256SUMS"
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.write_text("".join(f"{digest}  {self.ROUTES}\n" if l.endswith("  " + self.ROUTES) else l + "\n" for l in sums.read_text().splitlines()))
        code, out = run_check(root, self.corpus(root))
        self.assertEqual(code, 1, out)
        self.assertIn("design_routes_section_missing@content/runtime-references/OFFICIAL-DOCS.md", out)

if __name__ == "__main__":
    unittest.main()
