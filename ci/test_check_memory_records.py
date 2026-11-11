"""Offline contract tests; fixtures contain invented evidence only."""
import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("memory_check", HERE / "check-memory-records.py")
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)
FIXTURES = HERE / "fixtures/memory-records"


class MemoryRecords(unittest.TestCase):
    def test_valid_kind_fixtures(self):
        count, errors = CHECK.check(FIXTURES / "valid")
        self.assertEqual((count, errors), (7, []))
        kinds = {json.loads(p.read_text().split("---")[1])["kind"]
                 for p in (FIXTURES / "valid").glob("*.md")}
        self.assertEqual(kinds, set(CHECK.SECTIONS))

    def test_error_code_fixtures(self):
        for directory in (FIXTURES / "invalid").iterdir():
            with self.subTest(code=directory.name):
                _, errors = CHECK.check(directory)
                self.assertEqual({e.split("@")[0] for e in errors}, {directory.name})

    def test_readme_section_table_matches_checker(self):
        rows = {}
        for line in (HERE.parent / "memory/README.md").read_text().split("\n"):
            cells = [cell.strip() for cell in line.split("|")]
            if len(cells) == 5 and cells[1] in CHECK.SECTIONS:
                rows[cells[1]] = tuple(name.strip() for name in cells[3].split(";"))
        self.assertEqual(rows, CHECK.SECTIONS)

    def test_self_predecessor_diagnostic_preserves_closed_header(self):
        self.assertEqual({e.split("@")[0] for e in self.check_text(
            self.mutate(lambda h: h.update(supersedes=h["record_id"])))}, {"memory_successor_is_self"})
        self.assertEqual({e.split("@")[0] for e in self.check_text(
            self.mutate(lambda h: h.update(supersedes="another-record")))}, {"record_fields"})

    def test_line_feed_only(self):
        original = (FIXTURES / "valid/example-incident.md").read_text()
        for text in [original.replace("\n", "\r\n"), original + "\r"]:
            self.assertEqual({e.split("@")[0] for e in self.check_text(text)}, {"record_carriage_return"})
        text = original.replace("\n## Cause\n", "\u2028## Cause\u2028")
        self.assertEqual({e.split("@")[0] for e in self.check_text(text)}, {"record_sections"})

    def check_text(self, text):
        with tempfile.TemporaryDirectory() as scratch:
            path = Path(scratch, "example-incident.md")
            path.write_bytes(text if isinstance(text, bytes) else text.encode())
            return CHECK.check(Path(scratch))[1]

    def mutate(self, change):
        original = (FIXTURES / "valid/example-incident.md").read_text()
        _, raw, body = original.split("---", 2)
        header = json.loads(raw)
        change(header)
        return "---\n" + json.dumps(header) + "\n---" + body

    def test_nested_shapes_and_vocabularies(self):
        changes = [
            lambda h: h.update(kind=[]),
            lambda h: h.update(kind="decision"),
            lambda h: h.update(subject=dict(type="student", id="example")),
            lambda h: h.update(subject=dict(type=[], id="example")),
            lambda h: h.update(subject=dict(type="platform", id="")),
            lambda h: h.update(subject=dict(type="platform", id="x", extra=True)),
            lambda h: h.update(schema_version=True),
            lambda h: h.update(sources=[]),
            lambda h: h.update(sources=[dict(date="2026-02-30", locator="fixture://x")]),
            lambda h: h.update(sources=[dict(date="2026-01-02", locator="", extra=True)]),
            lambda h: h.update(scope=" "),
            lambda h: h.update(owner_ref_reason=""),
            lambda h: h.update(owner_ref=dict(repository="repo", path="")),
            lambda h: h.update(owner_ref=dict(object_type="task", native_key="x", namespace={})),
            lambda h: h.update(record_id={}),
        ]
        for change in changes:
            with self.subTest(change=change):
                self.assertTrue(self.check_text(self.mutate(change)))

    def test_all_owner_forms(self):
        for owner in [dict(repository="example", path="docs/example.md"),
                      dict(object_type="decision", namespace=dict(instance="test", account="root"), native_key="example")]:
            def change(header):
                header["owner_ref"] = owner
                del header["owner_ref_reason"]
            self.assertEqual(self.check_text(self.mutate(change)), [])

    def test_delimiters_json_and_encoding(self):
        for text in [b"\xff", "no header", "---\n{}", "---\n[]\n---\n",
                     '---\n{"kind":"incident","kind":"lesson"}\n---\n',
                     '---\n{"schema_version":NaN}\n---\n']:
            with self.subTest(text=text):
                self.assertTrue(self.check_text(text))

    def test_sections_missing_empty_duplicate_or_inside_fence(self):
        original = (FIXTURES / "valid/example-incident.md").read_text()
        for text in [original.replace("## Cause", "### Cause"),
                     original.replace("## Cause", "## Verification"),
                     original.replace("## Cause", "```\n## Cause") + "\n```\n"]:
            with self.subTest(text=text):
                self.assertTrue(self.check_text(text))

    def test_path_and_cli_empty_and_failure(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            for directory in [root, root / "absent"]:
                result = subprocess.run([sys.executable, "-I", "-B", str(HERE / "check-memory-records.py"), str(directory)],
                                        capture_output=True, text=True, timeout=10)
                self.assertEqual((result.returncode, result.stdout), (0, "memory records: 0 files, 0 errors\n"))
            (root / "unexpected.txt").write_text("invented")
            result = subprocess.run([sys.executable, "-I", "-B", str(HERE / "check-memory-records.py"), str(root)],
                                    capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 1)
            self.assertIn("record_path@", result.stdout)
            (root / "unexpected.txt").unlink()
            (root / "symlink.md").symlink_to(FIXTURES / "valid/example-incident.md")
            self.assertIn("record_path@", CHECK.check(root)[1][0])


if __name__ == "__main__":
    unittest.main()
