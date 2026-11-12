"""Offline contract tests; fixtures contain invented evidence only."""
import importlib.util
import json
import hashlib
import copy
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


class SchemaTwo(unittest.TestCase):
    def header(self, kind="incident", rid="invented-incident"):
        return dict(schema_version=2, record_id=rid, kind=kind, title="Invented condition",
                    subject=dict(type="platform", id="invented.platform"), scope="invented only",
                    owner_ref="none", owner_ref_reason="invented evidence",
                    sources=[dict(date="2026-01-02", locator="fixture://invented")],
                    abstract="Invented Identifier condition.", horizon="mid-term", review_on="2026-02-01",
                    recall_terms=["Identifier"], derived_from=[], codified_in=[])

    def subjects(self):
        root = dict(type="memory-map", id="engineering", parent=None, display_name="Engineering", aliases=[])
        child = dict(type="platform", id="invented.platform", parent=dict(type="memory-map", id="engineering"),
                     display_name="Invented", aliases=["InventedLiteral"])
        return dict(schema_version=1, nodes=[root, child])

    def raw(self, header, body=None):
        if body is None:
            body = "".join("\n## " + name + "\nInvented evidence.\n" for name in CHECK.SECTIONS[header["kind"]])
        return ("---\n" + json.dumps(header, ensure_ascii=False) + "\n---\n" + body).encode()

    def validate(self, headers, bodies=None):
        nodes = CHECK.vocabulary(json.dumps(self.subjects()).encode())
        return CHECK.validate([(Path(h["record_id"] + ".md"), self.raw(h, (bodies or {}).get(h["record_id"])))
                               for h in headers], nodes, publication=True)

    def codes(self, headers, bodies=None):
        return {e.split("@")[0] for e in self.validate(headers, bodies)[1]}

    def test_schema2_values_and_closed_shapes(self):
        self.assertEqual(self.codes([self.header()]), set())
        for kind, subject_type in [("user-profile", "user"), ("glossary", "term")]:
            d = self.subjects()
            d["nodes"].append(dict(type=subject_type, id="invented", parent=dict(type="memory-map", id="engineering"), display_name="Invented", aliases=[]))
            h = self.header(kind); h["subject"] = dict(type=subject_type, id="invented")
            nodes = CHECK.vocabulary(json.dumps(d).encode())
            self.assertFalse(CHECK.validate([(Path(h["record_id"] + ".md"), self.raw(h))], nodes)[1])
            h["subject"] = self.header()["subject"]
            self.assertIn("record_value", {e.split("@")[0] for e in CHECK.validate([(Path(h["record_id"] + ".md"), self.raw(h))], nodes)[1]})
        for field, values, code in [
            ("schema_version", [True, 0, 3, "2"], None), ("abstract", ["", [], None], "record_value"),
            ("horizon", ["short-term", [], None], "record_value"),
            ("review_on", ["2026-02-30", "2026-2-01", None], "record_value"),
            ("recall_terms", [[], ["x", "x"], [" "], [{}], list("123456789")], "recall_terms"),
            ("derived_from", [{}, [{}], [dict(record_id="missing", evidence_ref="")]], "derived_from"),
            ("codified_in", [{}, [{}], [dict(repository="x", path="x", commit="abc", anchor="x")]], "codified_in"),
            ("status", ["current"], "record_fields"), ("subject", [dict(type="platform", id="unknown")], "record_subject")]:
            for value in values:
                h = self.header(); h[field] = value
                with self.subTest(field=field, value=value):
                    errors = self.codes([h])
                    self.assertTrue(errors)
                    if code:
                        self.assertIn(code, errors)
        h = self.header(); h["codified_in"] = [dict(repository="invented", path="rule.md", commit="a" * 40, anchor="rule")]
        self.assertFalse(self.codes([h]))
        legacy = json.loads((FIXTURES / "valid/example-incident.md").read_text().split("---")[1])
        self.assertIn("publication_schema", self.codes([legacy]))
        with self.assertRaises(ValueError):
            CHECK.publication_bytes(dict(header=legacy, body=b""))

    def test_byte_limit_and_no_body_in_errors(self):
        h = self.header(); raw = self.raw(h)
        for length, valid in [(4096, True), (4097, False)]:
            data = raw + b"x" * (length - len(raw))
            _, errors = CHECK.validate([(Path(h["record_id"] + ".md"), data)],
                                       CHECK.vocabulary(json.dumps(self.subjects()).encode()))
            self.assertEqual(not errors, valid)
        data = raw + "é".encode() * 2048 + b"INVENTED_BODY_MARKER"
        _, errors = CHECK.validate([(Path(h["record_id"] + ".md"), data)], {})
        self.assertIn("record_size", {e.split("@")[0] for e in errors})
        self.assertNotIn("INVENTED_BODY_MARKER", "".join(errors))

    def test_derivation_graph_and_exception(self):
        a, b = self.header(rid="incident-a"), self.header(rid="incident-b")
        def linked(kind, rid, targets):
            h = self.header(kind, rid)
            h["derived_from"] = [dict(record_id=t, evidence_ref="fixture://invented") for t in targets]
            return h
        lesson = linked("lesson", "lesson-a", ["incident-a"])
        pattern = linked("pattern", "pattern-a", ["lesson-a", "incident-b"])
        proposal = linked("direction-proposal", "proposal-a", ["pattern-a"])
        self.assertFalse(self.codes([a, b, lesson, pattern, proposal]))
        for h, code in [(linked("incident", "bad", ["incident-a"]), "derived_level"),
                        (linked("lesson", "bad", []), "derived_level"),
                        (linked("lesson", "bad", ["pattern-a"]), "derived_level"),
                        (linked("lesson", "bad", ["absent"]), "derived_reference"),
                        (linked("pattern", "bad", ["incident-a", "lesson-a"]), "derived_chains")]:
            self.assertIn(code, self.codes([a, b, lesson, pattern, h]))
        x, y = linked("rationale", "x", ["y"]), linked("glossary", "y", ["x"])
        y["subject"] = dict(type="term", id="unknown")
        # Neutral kinds have no invented abstraction levels; cycles are still forbidden.
        y["kind"] = "rationale"
        y["subject"] = a["subject"]
        self.assertIn("derived_cycle", self.codes([x, y]))
        for targets in [[], ["incident-a"], ["lesson-a"]]:
            p = linked("direction-proposal", "proposal-a", targets)
            body = self.raw(p).decode().split("---\n", 2)[2]
            self.assertIn("proposal_without_pattern_or_exception", self.codes([a, lesson, p]))
            for line, valid in [("Critical exception: Invented contradiction.", True),
                                ("Critical exception: ", False), (" critical exception: x", False)]:
                text = body.replace("## Supporting records\n", "## Supporting records\n" + line + "\n")
                self.assertEqual(not self.codes([a, lesson, p], {p["record_id"]: text}), valid)

    def test_subject_shapes_parent_rules_and_graphs(self):
        original = self.subjects()
        CHECK.vocabulary(json.dumps(original).encode())
        mutations = [lambda d: d.update(schema_version=True), lambda d: d.update(extra=True),
                     lambda d: d["nodes"].append(copy.deepcopy(d["nodes"][1])),
                     lambda d: d["nodes"][1].update(aliases=["x", "x"]),
                     lambda d: d["nodes"][1].update(id="../x"),
                     lambda d: d["nodes"][1].update(parent=None),
                     lambda d: d["nodes"][1].update(parent=dict(type="platform", id="absent")),
                     lambda d: d["nodes"][1].update(parent=dict(type="platform", id="invented.platform")),
                     lambda d: d["nodes"][0].update(parent=dict(type="platform", id="invented.platform"))]
        for mutate in mutations:
            d = copy.deepcopy(original); mutate(d)
            with self.subTest(data=d), self.assertRaises(ValueError):
                CHECK.vocabulary(json.dumps(d).encode())
        for kind in CHECK.SUBJECTS:
            for parent_kind in CHECK.SUBJECTS:
                d = self.subjects()
                if parent_kind not in ("memory-map", "platform"):
                    parent = dict(type=parent_kind, id="parent", parent=dict(type="platform", id="invented.platform"), display_name="Parent", aliases=[])
                    if parent_kind in ("host", "user"):
                        parent["parent"] = dict(type="memory-map", id="engineering")
                    d["nodes"].append(parent)
                parent_id = {"memory-map": "engineering", "platform": "invented.platform"}.get(parent_kind, "parent")
                d["nodes"].append(dict(type=kind, id="child", parent=dict(type=parent_kind, id=parent_id), display_name="Child", aliases=[]))
                allowed = {"platform": {"memory-map"}, "host": {"memory-map"}, "user": {"memory-map"},
                           "module": {"platform"}, "procedure": {"platform", "module", "host"},
                           "task": {"memory-map", "platform", "module", "procedure", "host"},
                           "term": {"memory-map", "platform", "module", "procedure"}}
                valid = parent_kind in allowed.get(kind, set())
                try:
                    CHECK.vocabulary(json.dumps(d).encode())
                    self.assertTrue(valid, (kind, parent_kind))
                except ValueError:
                    self.assertFalse(valid, (kind, parent_kind))
        d = self.subjects()
        for kind, ident, parent_kind, parent_id in [("module", "m", "platform", "invented.platform"),
                ("procedure", "p", "module", "m"), ("procedure", "q", "procedure", "p"), ("task", "t", "procedure", "p")]:
            d["nodes"].append(dict(type=kind, id=ident, parent=dict(type=parent_kind, id=parent_id), display_name=ident, aliases=[]))
        d["nodes"] = [n for n in d["nodes"] if n["id"] != "q"]
        CHECK.vocabulary(json.dumps(d).encode())  # Four edges allowed.
        d["nodes"].append(dict(type="task", id="deep", parent=dict(type="task", id="t"), display_name="Deep", aliases=[]))
        with self.assertRaises(ValueError):
            CHECK.vocabulary(json.dumps(d).encode())

    def test_projection_tags_and_views(self):
        h = self.header(); h["abstract"] = "Invented café condition."
        records, errors = self.validate([h]); self.assertFalse(errors)
        r = records[h["record_id"]]; nodes = CHECK.vocabulary(json.dumps(self.subjects()).encode())
        projected = CHECK.publication_bytes(r)
        expected = {k: h[k] for k in ("title", "subject", "abstract")}
        expected.update((k, h[k]) for k in sorted(h) if k not in expected and k != "review_on")
        expected["subject"] = dict(id="invented.platform", type="platform")
        expected["sources"] = [dict(date="2026-01-02", locator="fixture://invented")]
        self.assertEqual(projected, ("---\n" + json.dumps(expected, ensure_ascii=False, separators=(",", ":")) + "\n---\n").encode() + r["body"])
        tags = ["subject:platform:invented.platform", "within:platform:invented.platform", "within:memory-map:engineering", "kind:incident", "horizon:mid-term"]
        self.assertEqual(CHECK.derived_tags(h, nodes), tags)
        self.assertEqual(CHECK.recall_tags(h["subject"]), dict(tags=[tags[0]], tags_match="all_strict"))
        self.assertEqual(CHECK.recall_tags(h["subject"], True, "long-term"), dict(tags=[tags[1], "horizon:long-term"], tags_match="all_strict"))
        h = r["header"]
        before = CHECK.publication_hash(r); h["review_on"] = "2026-12-12"
        self.assertEqual(before, CHECK.publication_hash(r)); self.assertEqual(tags, CHECK.derived_tags(h, nodes))
        h["abstract"] += " Changed"; self.assertNotEqual(before, CHECK.publication_hash(r))
        self.assertEqual(hashlib.sha256(projected).hexdigest(), before)
        self.assertEqual([n["id"] for n in CHECK.tree_view(records, nodes)], ["engineering", "invented.platform"])


    def test_projection_body_array_order_and_reparent(self):
        h = self.header(); h["recall_terms"] = ["Second", "First"]
        records, errors = self.validate([h]); self.assertFalse(errors)
        r = records[h["record_id"]]
        r["body"] += "\n\n```\n## Literal heading\n```\n終\n".encode()
        before = CHECK.publication_bytes(r)
        self.assertTrue(before.endswith(r["body"]))
        self.assertIn(b'"recall_terms":["Second","First"]', before)
        r["header"]["recall_terms"].reverse()
        self.assertNotEqual(before, CHECK.publication_bytes(r))
        d = self.subjects()
        d["nodes"].append(dict(type="module", id="module-1", parent=dict(type="platform", id="invented.platform"), display_name="Module", aliases=[]))
        d["nodes"].append(dict(type="platform", id="other", parent=dict(type="memory-map", id="engineering"), display_name="Other", aliases=[]))
        nodes = CHECK.vocabulary(json.dumps(d).encode())
        r["header"]["subject"] = dict(type="module", id="module-1")
        tags = CHECK.derived_tags(r["header"], nodes)
        d["nodes"][2]["parent"]["id"] = "other"
        after = CHECK.derived_tags(r["header"], CHECK.vocabulary(json.dumps(d).encode()))
        self.assertNotEqual(tags, after)
        self.assertEqual(after[:2], ["subject:module:module-1", "within:module:module-1"])
        self.assertEqual(CHECK.tree_view({}, nodes), [])
        # Literal metadata is not interpreted as a wildcard or hierarchy prefix.
        self.assertEqual(CHECK.recall_tags(dict(type="module", id="module-1"))["tags"], ["subject:module:module-1"])

    def test_commit_cli_pinned_blobs_and_path_refusals(self):
        with tempfile.TemporaryDirectory() as scratch:
            repo = Path(scratch); records = repo / "memory/records"; records.mkdir(parents=True)
            subjects = repo / "memory/subjects.json"; subjects.write_text(json.dumps(self.subjects()))
            original = records / "invented-incident.md"; original.write_bytes(self.raw(self.header()))
            def git(*args):
                return CHECK.git(repo, *args).decode().strip()
            git("init", "-q")
            def commit():
                git("add", ".")
                git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "Invented fixture")
                return git("rev-parse", "HEAD")
            def cli(*args):
                return subprocess.run([sys.executable, "-I", "-B", str(HERE / "check-memory-records.py"), *args],
                                      cwd=repo, capture_output=True, text=True, timeout=15)
            pinned = commit(); original.write_bytes(b"INVENTED_BODY_MARKER"); subjects.write_text("invalid worktree")
            result = cli("--commit", pinned)
            self.assertEqual((result.returncode, result.stdout), (0, "memory records: 1 files, 0 errors\n"))
            due = cli("--commit", pinned, "--due", "2026-02-01")
            self.assertEqual(json.loads(due.stdout), [dict(record_id="invented-incident", review_on="2026-02-01", horizon="mid-term")])
            self.assertEqual(json.loads(cli("--commit", pinned, "--due", "2026-01-31").stdout), [])
            self.assertEqual(len(json.loads(cli("--commit", pinned, "--tree").stdout)), 2)
            original.write_bytes(self.raw(self.header())); subjects.write_text(json.dumps(self.subjects()))
            for mode in ("symlink", "nested", "gitlink"):
                bad = records / ("bad.md" if mode != "nested" else "nested/bad.md")
                if mode == "nested":
                    bad.parent.mkdir(); bad.write_bytes(original.read_bytes())
                elif mode == "symlink":
                    bad.symlink_to(original.name)
                else:
                    git("update-index", "--add", "--cacheinfo", "160000," + pinned + ",memory/records/bad.md")
                if mode == "gitlink":
                    git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "commit", "-qm", "Invented gitlink")
                    bad_commit = git("rev-parse", "HEAD")
                    git("update-index", "--force-remove", "memory/records/bad.md")
                else:
                    bad_commit = commit(); bad.unlink()
                rejected = cli("--commit", bad_commit)
                self.assertEqual(rejected.returncode, 1)
                self.assertIn("record_path@", rejected.stdout)
                self.assertNotIn("INVENTED_BODY_MARKER", rejected.stdout)
            subjects.unlink(); subjects.symlink_to("records/invented-incident.md")
            self.assertIn("subjects@", cli("--commit", commit()).stdout)
            self.assertEqual(cli("--commit", "not-a-commit").returncode, 1)
            self.assertEqual(cli("--commit", pinned, "--due", "2026-02-30").returncode, 2)
            # Filesystem and Git modes both reject a symlinked vocabulary/root.
            self.assertIn("subjects", CHECK.load(records, subjects)[3][0])
            subjects.unlink(); subjects.write_text(json.dumps(self.subjects()))
            alias = repo / "alias"; alias.symlink_to("memory", target_is_directory=True)
            self.assertIn("record_path", CHECK.load(alias / "records", subjects)[3][0])
            git("add", "alias"); root_link_commit = commit()
            self.assertIn("record_path@", cli("alias/records", "--subjects", "memory/subjects.json", "--commit", root_link_commit).stdout)
            git("rm", "-q", "alias")
            self.assertIn("record_path", CHECK.load(records / "invented-incident.md" / "absent")[3][0])


if __name__ == "__main__":
    unittest.main()
