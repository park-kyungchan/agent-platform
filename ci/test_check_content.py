"""Tests of ci/check-content.py: the verbs of `ap` and the sections of crates/ap/README.md that content/ names,
the model names and byte limits, the words of the Lead skill's execution mode, the paths and design routes, and the form
of AGENTS.md.

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


def run_check(root, corpus=None, host=None, skips=None):
    """The check on a scratch copy; it skips on purpose, so CHECK_SKIPS of a ./check around it is removed, and `skips`
    names the file it appends to instead."""
    env = dict(os.environ, CONTENT_CHECK_CORPUS=str(corpus or (root / "no-corpus")), CONTENT_CHECK_HOST=str(host or (root / "no-host")))
    env.pop("CHECK_SKIPS", None)
    if skips is not None:
        env["CHECK_SKIPS"] = str(skips)
    done = subprocess.run([sys.executable, "-I", "-B", str(root / "ci/check-content.py")], capture_output=True, text=True, timeout=120, env=env)
    return done.returncode, done.stdout


class Scratch:
    def fixture(self, neutral=False):
        """A copy of what the check reads. neutral: a test that writes the scratch path into content/ puts it under
        /tmp, whose path trips no rule of the check; the default temporary directory may name a version or a retired word."""
        parent = "/tmp" if neutral and os.access("/tmp", os.W_OK) else None
        scratch = Path(tempfile.mkdtemp(prefix="check-content-test.", dir=parent))
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


class RetiredNames(Scratch, unittest.TestCase):
    """A file of content/ that names a removed skill or path fails, with its file and line."""

    def test_a_retired_skill_or_path_fails(self):
        for name in ("lessons-loop", "hindsight-control", "memory/README.md", "publish-memory", "evaluate-memory", "explain-precisely", "herdr-hermes", "evals/instructions"):
            with self.subTest(name=name):
                root = self.fixture()
                line = self.append(root, f"See {name} for the rest.")
                code, out = run_check(root)
                self.assertEqual(code, 1, out)
                self.assertIn(f"retired_name@content/{TARGET}:{line}: {name}", out)

    def test_a_retired_name_in_the_readme_fails(self):
        root = self.fixture()
        readme = root / "content/README.md"
        text = readme.read_text(encoding="utf-8")
        readme.write_text(text + "The skill lessons-loop is gone.\n", encoding="utf-8")
        code, out = run_check(root)
        self.assertEqual(code, 1, out)
        self.assertIn(f"retired_name@content/README.md:{text.count(chr(10)) + 1}: lessons-loop", out)

    def test_the_legacy_memory_service_named_in_any_case_fails(self):
        for word in ("Hindsight", "hindsight", "HINDSIGHT"):
            with self.subTest(word=word):
                root = self.fixture()
                line = self.append(root, f"Long-term memory lives in {word}.")
                code, out = run_check(root)
                self.assertEqual(code, 1, out)
                self.assertIn(f"retired_memory_named@content/{TARGET}:{line}: '{word}'", out)

    def test_no_retired_name_passes(self):
        root = self.fixture()
        self.append(root, "See the lead skill for the closure of a worker.")
        code, out = run_check(root)
        self.assertEqual(code, 0, out)
        self.assertNotIn("retired_name@", out)
        self.assertNotIn("retired_memory_named@", out)


class ModelNamesAndLimits(Scratch, unittest.TestCase):
    """Only the Lead's page form may name a model; the Lead's and report's files have byte limits."""

    MODEL_FILE = "skills/lead/references/unit-page.md"

    def append_to(self, root, name, line):
        path = root / "content" / name
        text = path.read_text(encoding="utf-8")
        path.write_text(text + ("" if text.endswith("\n") else "\n") + line + "\n", encoding="utf-8")
        sums = root / "content/SHA256SUMS"
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.write_text("".join(f"{digest}  {name}\n" if l.endswith("  " + name) else l + "\n" for l in sums.read_text().splitlines()))

    def test_a_model_name_in_the_page_form_passes(self):
        root = self.fixture()
        self.append_to(root, self.MODEL_FILE, "| Claude Code | Opus | another line of the table, with claude-sonnet-5-5 |")
        code, out = run_check(root)
        self.assertEqual(code, 0, out)

    def test_the_page_form_still_has_its_routes_and_links_checked(self):
        root = self.fixture()
        self.append_to(root, self.MODEL_FILE, "See [x](gone.md) and `/root/.agents/skills/lead/references/gone.md`.")
        code, out = run_check(root)
        self.assertEqual(code, 1, out)
        self.assertIn(f"route_to_missing_file@content/{self.MODEL_FILE}", out)
        self.assertIn(f"broken_relative_link@content/{self.MODEL_FILE}", out)

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
        for name, limit in (("skills/lead/SKILL.md", 6500), ("skills/report/SKILL.md", 6500), ("skills/lead/references/interview.md", 8200),
                            ("skills/lead/references/unit-page.md", 8000), ("skills/lead/references/judging.md", 8100)):
            with self.subTest(name=name):
                root = self.fixture()
                size = len((root / "content" / name).read_bytes())
                self.append_to(root, name, "x" * (limit - size))
                code, out = run_check(root)
                self.assertEqual(code, 1, out)
                self.assertIn(f"file_over_limit@content/{name}: {limit + 1} bytes, limit {limit}", out)


class LeadExecutionMode(Scratch, unittest.TestCase):
    """specs/content/lead-skill.md R4: the execution mode of the Lead skill holds the fixed words of its five rules."""

    SKILL = "skills/lead/SKILL.md"
    WORDS = ("session of its own", "ap status --wait", "never by a repeated", "subagent", "only the member", "ont render")

    def rewrite(self, root, change):
        """Apply change(execution mode text) -> new text to the section, and refresh its checksum."""
        path = root / "content" / self.SKILL
        text = path.read_text(encoding="utf-8")
        start = text.index("### Execution mode\n")
        end = text.index("\n## 3. ", start)
        path.write_text(text[:start] + change(text[start:end]) + text[end:], encoding="utf-8")
        sums = root / "content/SHA256SUMS"
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.write_text("".join(f"{digest}  {self.SKILL}\n" if l.endswith("  " + self.SKILL) else l + "\n" for l in sums.read_text().splitlines()))

    def test_the_skill_holds_every_word(self):
        section = (REPO / "content" / self.SKILL).read_text(encoding="utf-8").split("### Execution mode\n", 1)[1].split("\n## ", 1)[0]
        for word in self.WORDS:
            with self.subTest(word=word):
                self.assertIn(word, section.lower())

    def test_a_missing_word_fails_with_the_word(self):
        for word in self.WORDS:
            with self.subTest(word=word):
                root = self.fixture()
                self.rewrite(root, lambda section: section.replace(word, "x").replace(word.capitalize(), "x"))
                code, out = run_check(root)
                self.assertEqual(code, 1, out)
                self.assertIn(f"lead_execution_rule_missing@content/{self.SKILL}: `### Execution mode` lacks '{word}'", out)
                self.assertEqual(out.count("lead_execution_rule_missing@"), 1, out)

    def test_a_word_outside_the_execution_mode_does_not_count(self):
        root = self.fixture()
        self.rewrite(root, lambda section: section.replace("ont render", "a view"))
        path = root / "content" / self.SKILL
        text = path.read_text(encoding="utf-8").replace("### Design mode\n", "### Design mode\n\nRead the store by `ont render`.\n", 1)
        path.write_text(text, encoding="utf-8")
        sums = root / "content/SHA256SUMS"
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.write_text("".join(f"{digest}  {self.SKILL}\n" if l.endswith("  " + self.SKILL) else l + "\n" for l in sums.read_text().splitlines()))
        code, out = run_check(root)
        self.assertEqual(code, 1, out)
        self.assertIn("lacks 'ont render'", out)

    def test_a_skill_without_the_execution_mode_fails_with_every_word(self):
        root = self.fixture()
        self.rewrite(root, lambda section: section.replace("### Execution mode", "### Running"))
        code, out = run_check(root)
        self.assertEqual(code, 1, out)
        self.assertIn("lacks " + ", ".join(repr(w) for w in self.WORDS), out)


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
        line = self.append(root, "Read `/home/palantirkc/platforms/lifecycle-platform/crates/ap/src/run.rs`.")
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
        skips = root / "skips"
        skips.write_text("")
        code, out = run_check(root, skips=skips)
        self.assertEqual(code, 0, out)
        self.assertNotIn("design routes:", out)
        # The skip says so, and appends the ids of its rows of ci/skips.json (specs/ap/check.md R22).
        self.assertIn(f"corpus routes: skipped, {root / 'no-corpus'} is absent", out)
        self.assertEqual(skips.read_text(), "content-agents-paths\ncontent-corpus-routes\n")
        # With the corpus, no such line and no id of it.
        skips.write_text("")
        code, out = run_check(root, self.corpus(root), skips=skips)
        self.assertNotIn("corpus routes: skipped", out)
        self.assertEqual(skips.read_text(), "content-agents-paths\n")

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

class AgentsFile(Scratch, unittest.TestCase):
    """The always-loaded AGENTS.md (specs/content/agents-file.md): its size, its eight sections, prose, existing paths."""

    def write(self, root, text):
        path = root / "content/AGENTS.md"
        path.write_text(text, encoding="utf-8")
        sums = root / "content/SHA256SUMS"
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        sums.write_text("".join(f"{digest}  AGENTS.md\n" if l.endswith("  AGENTS.md") else l + "\n" for l in sums.read_text().splitlines()))

    def append_lines(self, root, *lines):
        """Append lines to AGENTS.md and return the line number of each."""
        text = (root / "content/AGENTS.md").read_text(encoding="utf-8")
        first = text.count("\n") + 1
        self.write(root, text + "".join(line + "\n" for line in lines))
        return [first + n for n in range(len(lines))]

    def test_over_the_byte_limit_fails(self):
        root = self.fixture()
        size = len((root / "content/AGENTS.md").read_bytes())
        self.append_lines(root, *(["y" * 99] * ((6500 - size) // 100)), "z" * ((6500 - size) % 100))
        code, out = run_check(root)
        self.assertEqual(code, 1, out)
        self.assertIn("file_over_limit@content/AGENTS.md: 6501 bytes, limit 6500", out)

    def test_a_missing_section_fails(self):
        root = self.fixture()
        text = (root / "content/AGENTS.md").read_text(encoding="utf-8")
        self.write(root, text.replace("## 4. Evidence\n", "## 4. Proof\n"))
        code, out = run_check(root)
        self.assertEqual(code, 1, out)
        self.assertIn("agents_section_missing@content/AGENTS.md: '## 4. Evidence'", out)
        self.assertIn("agents_section_unexpected@content/AGENTS.md: '## 4. Proof'", out)

    def test_sections_out_of_order_fail(self):
        root = self.fixture()
        text = (root / "content/AGENTS.md").read_text(encoding="utf-8")
        head, rest = text.split("## 4. Evidence\n", 1)
        evidence, tail = rest.split("## 5. Effects\n", 1)
        self.write(root, head + "## 5. Effects\n" + tail + "\n## 4. Evidence\n" + evidence)
        code, out = run_check(root)
        self.assertEqual(code, 1, out)
        self.assertIn("agents_sections_out_of_order@content/AGENTS.md", out)
        self.assertNotIn("agents_section_missing@", out)

    def test_a_long_line_and_a_table_row_fail_with_their_line(self):
        root = self.fixture()
        long_line, row, fine = self.append_lines(root, "w " * 201, "| Situation | Read |", "w " * 199)
        code, out = run_check(root)
        self.assertEqual(code, 1, out)
        self.assertIn(f"agents_line_too_long@content/AGENTS.md:{long_line}: 402 characters, limit 400", out)
        self.assertIn(f"agents_table@content/AGENTS.md:{row}", out)
        self.assertNotIn(f"AGENTS.md:{fine}", out)
        self.assertEqual(out.count("agents_line_too_long@"), 1, out)

    def test_a_missing_absolute_path_fails_on_the_host(self):
        root = self.fixture(neutral=True)
        host = root / "host-agents.md"
        host.write_text("installed\n")
        missing, present, pattern = self.append_lines(
            root,
            f"- Read `{root}/no-such-dir/file.md`.",
            f"- Read `{root}/content/` and {host}.",
            "- Read `/no-such-root/<repo>/` and `/no-such-root/*.md`.",
        )
        code, out = run_check(root, host=host)
        self.assertEqual(code, 1, out)
        self.assertIn(f"agents_path_missing@content/AGENTS.md:{missing}: {root}/no-such-dir/file.md", out)
        self.assertNotIn(f"AGENTS.md:{present}:", out)
        self.assertNotIn(f"AGENTS.md:{pattern}:", out)
        self.assertNotIn("agents paths: skipped", out)

    def test_without_the_host_the_paths_are_skipped_with_one_line(self):
        root = self.fixture(neutral=True)
        added = f"- Read `{root}/no-such-dir/file.md`."
        # Cut the copy's last lines (section 8's body) so the added line fits the byte limit
        # whatever headroom the real file leaves.
        lines = (root / "content/AGENTS.md").read_text(encoding="utf-8").splitlines(keepends=True)
        while len("".join(lines).encode()) + len(added.encode()) + 1 > 6500:
            self.assertFalse(lines[-1].startswith("## "), "no body line of section 8 is left to cut")
            lines.pop()
        self.write(root, "".join(lines))
        self.append_lines(root, added)
        code, out = run_check(root)
        self.assertEqual(code, 0, out)
        self.assertNotIn("agents_path_missing@", out)
        self.assertEqual(out.count("agents paths: skipped"), 1, out)


if __name__ == "__main__":
    unittest.main()
