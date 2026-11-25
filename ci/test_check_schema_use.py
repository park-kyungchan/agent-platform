import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("check-schema-use.py")
REPO = Path(__file__).resolve().parent.parent


def run(root):
    return subprocess.run(
        [sys.executable, "-I", "-B", str(SCRIPT), str(root)],
        capture_output=True,
        text=True,
    )


class CheckSchemaUseTest(unittest.TestCase):
    def make(self, actions, views, files):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        schema = {"version": 1, "action_types": {a: {} for a in actions}, "views": {v: {} for v in views}}
        (root / "ontology").mkdir()
        (root / "ontology/work.schema.json").write_text(json.dumps(schema))
        for rel, content in files.items():
            p = root / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content)
        return root

    def test_every_name_found_passes(self):
        root = self.make(
            ["record_head", "open_finding", "define_kpi", "lift_hold", "set_hold"],
            ["units_live", "findings_open"],
            {
                "crates/ap/src/unit.rs": 'store::apply(s, "record_head", p);\n',
                "content/USE.md": "| `define_kpi` | by hand |\n",
                "deploy/units.sh": "ont render units_live\n",
                "scripts/x.sh": "echo findings_open lift_hold\n",
                "ontology/migrate": "action_type='set_hold'\n",
                "crates/ap/src/gen.rs": "work::actions::OpenFinding { finding }\n",
            },
        )
        done = run(root)
        self.assertEqual((done.returncode, done.stdout, done.stderr), (0, "", ""))

    def test_unused_names_are_printed_in_schema_order_and_fail(self):
        root = self.make(
            ["close_unit", "record_head", "set_order"],
            ["kpi_board", "units_live"],
            {"crates/ap/src/unit.rs": "record_head units_live\n"},
        )
        done = run(root)
        self.assertEqual(done.returncode, 1)
        self.assertEqual(done.stdout, "unused action close_unit\nunused action set_order\nunused view kpi_board\n")

    def test_a_name_inside_a_longer_word_is_not_a_use(self):
        root = self.make(
            ["close_unit", "record_use"],
            ["lead_state"],
            {"crates/ap/src/unit.rs": "close_unit_review record_used CloseUnitReview lead_state_old\n"},
        )
        done = run(root)
        self.assertEqual(done.returncode, 1)
        self.assertEqual(done.stdout, "unused action close_unit\nunused action record_use\nunused view lead_state\n")

    def test_a_view_is_not_found_by_a_type_name(self):
        root = self.make([], ["kpi_board"], {"crates/ap/src/x.rs": "KpiBoard\n"})
        self.assertEqual(run(root).stdout, "unused view kpi_board\n")

    def test_places_outside_the_searched_ones_do_not_count(self):
        root = self.make(
            ["close_unit"],
            [],
            {
                "crates/ap/tests/unit.rs": "close_unit\n",
                "ontology/rehearsal.sh": "close_unit\n",
                "ontology/README.md": "close_unit\n",
                "evals/run": "close_unit\n",
                "specs/ap/unit.md": "close_unit\n",
            },
        )
        done = run(root)
        self.assertEqual((done.returncode, done.stdout), (1, "unused action close_unit\n"))

    def test_the_repository_passes(self):
        done = run(REPO)
        self.assertEqual((done.returncode, done.stdout), (0, ""))


if __name__ == "__main__":
    unittest.main()
