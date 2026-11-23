"""Tests of evals/cases (`index`, `select`, `reproduce`) against a scratch store of the pinned engine: the repository's
schema installed with the glob of the source `cases` rewritten to a scratch directory, as the rehearsal rewrites it
(ontology/rehearsal.sh), and only scratch Git repositories that the test makes, read through revisions and never the
checkout this file runs in. The programs run from a scratch copy of the tree they need, so that a scratch case file is
admitted by a digest line the test writes. No test makes or opens a path whose component begins `edu-` or `education-`.
Run (by `check`, with the engine it found or built): ONT_BIN=<ont> python3 -I -B -m unittest discover -s ci -p test_cases_engine.py
"""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SCHEMA = REPO / "ontology" / "work.schema.json"
ONT = os.environ.get("ONT_BIN") or shutil.which("ont") or ""
PY = sys.executable
HOST_GLOB = "/var/lib/agent-platform/cases/*.cases.jsonl"
GIT_ID = ["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "-c", "commit.gpgsign=false"]


def sha(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def git(d, *args):
    done = subprocess.run(["git", "-C", str(d), *GIT_ID, *args], capture_output=True, text=True)
    assert done.returncode == 0, (args, done.stderr)
    return done.stdout.strip()


def commit(d, files, message="c"):
    for path, text in files.items():
        p = Path(d) / path
        if text is None:
            p.unlink()
            continue
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8")
    git(d, "add", "-A")
    git(d, "commit", "-q", "--allow-empty", "-m", message)
    return git(d, "rev-parse", "HEAD")


def facts(*strings):
    return [{"fact": [s]} for s in strings]


@unittest.skipUnless(ONT, "no engine: set ONT_BIN")
class EngineTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(os.path.realpath(tempfile.mkdtemp(prefix="cases-engine-")))
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        self.tree = self.tmp / "tree"
        (self.tree / "evals" / "cases").mkdir(parents=True)
        (self.tree / "evals" / "batch").mkdir()
        for name in ("lib.py", "index", "lost", "select", "reproduce"):
            shutil.copy2(REPO / "evals" / "cases" / name, self.tree / "evals" / "cases" / name)
        shutil.copy2(REPO / "evals" / "batch" / "lib.py", self.tree / "evals" / "batch" / "lib.py")
        for suite in ("routing", "intake"):
            (self.tree / "evals" / suite).mkdir()
        # The schema of this repository, its source `cases` and the source of the tasks reading scratch directories.
        self.dir = self.tmp / "cases"
        work = self.tmp / "work"
        (work / "task-one").mkdir(parents=True)
        (work / "task-one" / "STATE.md").write_text("# A scratch task\nOwner: nobody\n")
        schema = json.loads(SCHEMA.read_text())
        self.assertEqual(schema["sources"]["cases"]["glob"], HOST_GLOB)
        schema["sources"]["cases"]["glob"] = f"{self.dir}/*.cases.jsonl"
        schema["sources"]["task_states"]["glob"] = f"{work}/*/STATE.md"
        schema["sources"]["task_states"]["exclude"] = []
        (self.tmp / "schema.json").write_text(json.dumps(schema))
        self.store = str(self.tmp / "store.sqlite")
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("ONT_")}
        self.env["ONT_BIN"] = ONT
        self.ont(["schema", "install", str(self.tmp / "schema.json")])
        # The roles file beside the store: this uid may act as every role of the grants (engine action.md R113).
        roles = sorted(g["role"] for g in schema["grants"] if "role" in g)
        (self.tmp / "roles.json").write_text(json.dumps({"principals": {f"uid:{os.getuid()}": roles}}) + "\n")
        (self.tmp / "roles.json").chmod(0o644)
        self.ont(["index", "--source", "task_states"])
        self.repo = self.tmp / "repo"
        self.repo.mkdir()
        git(self.repo, "-c", "init.defaultBranch=main", "init", "-q")
        self.base = commit(self.repo, {"README.md": "# Readme\nStart here.\n", "docs/a.md": "# A\nThe backup pass keeps fourteen nightly copies.\n",
                                       "docs/b.md": "# B\nThe gate waits ten minutes for a merge.\n"}, "base")
        self.head = commit(self.repo, {"docs/a.md": "# A\nThe backup pass keeps fourteen nightly copies.\nA new sentence.\n"}, "head")
        self.routing_cases = [{"id": "ca", "situation": "s", "must": facts("fourteen nightly copies"), "wrong": []},
                              {"id": "cb", "situation": "s", "must": facts("ten minutes for a merge"), "wrong": []},
                              {"id": "cn", "situation": "s", "must": facts("nothing in any file says this"), "wrong": []}]
        self.intake_cases = [{"id": "ia", "words": "w", "seed": "none", "expect": {"lane": "none", "out_of_order": None, "missing": []}}]
        self.routing = self.case_file("routing", self.routing_cases, "host-routing-e")
        self.intake = self.case_file("intake", self.intake_cases, "host-intake-e")

    def ont(self, args, body=None):
        done = subprocess.run([ONT, "--store", self.store, *args], input=None if body is None else json.dumps(body), capture_output=True,
                              text=True, env=self.env, timeout=120)
        self.assertEqual(done.returncode, 0, (args, done.stdout, done.stderr))
        return json.loads(done.stdout)["result"]

    def objects(self, kind, where=None):
        found, body = {}, {"object_type": kind, "limit": 1000}
        if where:
            body["where"] = where
        for o in self.ont(["search"], body)["objects"]:
            found[o["id"]["native_key"]] = {k: v.get("value") for k, v in o["head"]["fields"].items()}
        return found

    def case_file(self, suite, cases, name):
        path = self.tmp / f"{name}.json"
        data = json.dumps(cases)
        path.write_text(data)
        with open(self.tree / "evals" / suite / "host-cases.sha256", "a") as f:
            f.write(f"{sha(data)}  {name}\n")
        return path

    def program(self, name, *args):
        done = subprocess.run([PY, "-I", "-B", str(self.tree / "evals" / "cases" / name), *map(str, args)], capture_output=True, text=True,
                              env=self.env, cwd=str(self.tmp), timeout=300)
        self.assertEqual(done.stderr, "", (name, done.stderr))
        return done.returncode, [json.loads(x) for x in done.stdout.splitlines()]

    def index(self, *cases, at=None):
        args = ["--repository", f"o/r={self.repo}", "--at", f"o/r={at or self.base}", "--directory", self.dir, "--store", self.store]
        for c in cases:
            args += ["--cases", c]
        return self.program("index", *args)

    def select(self):
        return self.program("select", "--repository", f"o/r={self.repo}", "--base", self.base, "--head", self.head, "--cases", f"routing={self.routing}",
                            "--cases", f"intake={self.intake}", "--directory", self.dir, "--store", self.store)

    def test_index_reads_back_and_retracts(self):
        code, lines = self.index(f"routing={self.routing}", f"intake={self.intake}")
        self.assertEqual(code, 0, lines)
        (line,) = lines
        self.assertEqual((line["status"], line["index"]["status"], line["index"]["appended"]["initial"]), ("ok", "ok", 4), line)
        self.assertEqual(line["suites"], {"routing": {"cases": 3, "all": 1, "unhomed": 1}, "intake": {"cases": 1, "all": 1, "unhomed": 0}})
        found = self.objects("case")
        self.assertEqual(len(found), 4)
        digest = hashlib.sha256((REPO / "evals" / "cases" / "lib.py").read_bytes()).hexdigest()
        ca = found[sha("routing:ca")]
        self.assertEqual(ca, {"suite": "routing", "case_id": "ca", "file": "host-routing-e", "file_digest": hashlib.sha256(self.routing.read_bytes()).hexdigest(),
                              "facts": 1, "unhomed": 0, "all": False, "serves": ["o/r:docs/a.md"], "commits": [f"o/r@{self.base}"], "map_digest": digest})
        cn = found[sha("routing:cn")]
        self.assertEqual((cn["all"], cn["unhomed"], cn["serves"]), (True, 1, []))
        self.assertEqual(self.objects("case", {"eq": ["suite", "intake"]}).keys(), {sha("intake:ia")})
        # A case removed from its file is retracted by the next index.
        smaller = self.case_file("routing", self.routing_cases[:1] + self.routing_cases[2:], "host-routing-f")
        code, lines = self.index(f"routing={smaller}")
        self.assertEqual(code, 0, lines)
        self.assertEqual(lines[0]["index"]["retracted"], 1, lines)
        self.assertEqual(set(self.objects("case")), {sha("routing:ca"), sha("routing:cn"), sha("intake:ia")})
        # Indexed again unchanged, it appends nothing.
        code, lines = self.index(f"routing={smaller}")
        self.assertEqual(sum(lines[0]["index"]["appended"].values()), 0, lines)

    def test_select_full_then_subset_after_reproduce_record(self):
        code, lines = self.select()
        self.assertEqual(code, 0, lines)
        self.assertEqual([(x["suite"], x["mode"], x["cases"]) for x in lines[:2]], [("routing", "full", ["ca", "cb", "cn"]), ("intake", "full", ["ia"])])
        self.assertEqual((lines[2]["mode"], lines[2]["selected"], lines[2]["of"]), ("full", 4, 4))
        gate = self.tmp / "gate.json"
        gate.write_text(json.dumps({"status": "fail", "readers": {"claude-sonnet": {"ok": False, "lost": ["ca"]}, "claude-haiku": {"ok": True, "lost": []}}}))
        record = ["--repository", f"o/r={self.repo}", "--base", self.base, "--head", self.head, "--cases", f"routing={self.routing}",
                  "--cases", f"intake={self.intake}", "--gate", gate, "--record", "task-one", "--store", self.store]
        code, lines = self.program("reproduce", *record)
        self.assertEqual(code, 0, lines)
        (result,) = lines
        self.assertEqual((result["reproduced"], result["failures"], result["selected"], result["cases"], result["missed"], result["recorded"]), (True, 1, 3, 4, [], "applied"))
        (kpi,) = self.objects("kpi", {"eq": ["name", "case_map"]}).values()
        self.assertEqual(kpi["value"], f"reproduced {result['map_digest']} failures 1 selected 3/4")
        self.assertEqual(kpi["task"]["native_key"], "task-one")
        self.assertTrue(kpi["evidence"].startswith("evals/cases/reproduce gate sha256:"), kpi)
        self.assertIn(f"{self.base[:12]}..{self.head[:12]}", kpi["evidence"])
        self.assertEqual(len(self.objects("measurement")), 1)
        # The same measurement again applies nothing.
        code, lines = self.program("reproduce", *record)
        self.assertEqual((code, lines[0]["recorded"]), (0, "already"), lines)
        self.assertEqual(len(self.objects("measurement")), 1)
        # Into the same store, select now names the subset: fewer cases than the suites hold.
        code, lines = self.select()
        self.assertEqual(code, 0, lines)
        routing, intake, total = lines
        self.assertEqual((routing["mode"], routing["cases"], routing["args"]), ("subset", ["ca", "cn"], "--case ca --case cn"))
        self.assertEqual((intake["mode"], intake["cases"]), ("subset", ["ia"]))
        self.assertEqual((total["mode"], total["selected"], total["of"]), ("subset", 3, 4))
        # A change of the rule asks for a new reproduction.
        with open(self.tree / "evals" / "cases" / "lib.py", "a") as f:
            f.write("\n# a changed rule\n")
        code, lines = self.select()
        self.assertEqual((code, lines[-1]["mode"], lines[-1]["selected"]), (0, "full", 4), lines)

    def test_academy_refusal_on_names_and_a_tracked_lesson_file(self):
        # Names that name nothing on disk are refused before any effect.
        code, lines = self.program("index", "--repository", "o/education-platform=/nowhere/x", "--cases", f"routing={self.routing}",
                                   "--directory", self.dir, "--store", self.store)
        self.assertEqual((code, lines), (2, [{"status": "academy_path"}]))
        self.assertFalse(self.dir.exists())
        # A tracked file under evals/lesson/ of a scratch repository is never read: its text is in no record.
        lesson = commit(self.repo, {"evals/lesson/notes.md": "# Notes\nA fact about the student schedule.\n"}, "lesson")
        routing = self.case_file("routing", [{"id": "cs", "situation": "s", "must": facts("student schedule"), "wrong": []}], "host-routing-g")
        code, lines = self.index(f"routing={routing}", at=lesson)
        self.assertEqual(code, 0, lines)
        (found,) = self.objects("case", {"eq": ["suite", "routing"]}).values()
        self.assertEqual((found["all"], found["unhomed"], found["serves"]), (True, 1, []))
        self.assertNotIn("lesson", json.dumps(found))


if __name__ == "__main__":
    unittest.main()
