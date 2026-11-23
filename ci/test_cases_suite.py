"""Tests of evals/cases (`index`, `lost`, `select`, `reproduce`), run as programs against scratch Git repositories and a
fake `ont` first on PATH that records every call and answers from a state file (no model, no network, no engine, no
store of the host). The programs run from a scratch copy of the tree they need (`evals/cases`, `evals/batch/lib.py` and
the host digest lists), so that a scratch case file is admitted by the digest line a test writes. No test makes or
opens a path whose component begins `edu-` or `education-`: the Academy rule is tried on names that name nothing on disk
and on a tracked file under `evals/lesson/` of a scratch repository. Contract: evals/cases/README.md.
Run: python3 -I -B -m unittest discover -s ci -p test_cases_suite.py
"""
import fcntl
import hashlib
import importlib.machinery
import importlib.util
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

REPO = Path(__file__).resolve().parent.parent
PY = sys.executable
HOME = os.environ.get("HOME", "/")
HOST_DIRECTORY = "/var/lib/agent-platform/cases"

# The fake engine: `index --source cases` reads the record files of the directory in config.json; `search` answers
# case, kpi and task objects in pages of two; `action apply` makes and measures KPIs; every call is logged.
FAKE_ONT = r'''#!@PY@ -I
import glob, json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
STATE = os.path.join(HERE, "state.json")
CONFIG = json.load(open(os.path.join(HERE, "config.json")))
try:
    st = json.load(open(STATE))
except FileNotFoundError:
    st = {"cases": {}, "kpis": {}, "rids": {}, "n": 0}
args = sys.argv[1:]
raw = sys.stdin.read()
body = json.loads(raw) if raw.strip() else None
def save():
    json.dump(st, open(STATE, "w"))
def out(v, code=0):
    save()
    print(json.dumps(v))
    sys.exit(code)
def take(flag):
    if flag in args:
        i = args.index(flag)
        v = args[i + 1]
        del args[i:i + 2]
        return v
store, client = take("--store"), take("--client")
with open(os.path.join(HERE, "calls.jsonl"), "a") as f:
    f.write(json.dumps({"argv": args, "store": store, "client": client, "body": body}) + "\n")
NS = {"instance": "vps://test", "account": "root"}
def ident(kind, key): return {"object_type": kind, "namespace": NS, "native_key": key}
if args[:1] == ["index"]:
    one = {"source": "cases", "status": "ok", "appended": {"initial": 0, "refresh": 0, "retraction": 0, "update": 0}, "renewed": 0}
    if CONFIG.get("index_code"):
        one.update(status="source_failed", code=CONFIG["index_code"])
        out({"status": "source_failed", "result": {"sources": [one]}}, 2)
    seen = {}
    for p in sorted(glob.glob(os.path.join(CONFIG["cases_dir"], "*.cases.jsonl"))):
        for line in open(p):
            if line.strip():
                r = json.loads(line)
                seen[r["key"]] = {k: v for k, v in r.items() if k not in ("type", "key", "observed_at")}
    one["appended"]["initial"] = len(set(seen) - set(st["cases"]))
    one["appended"]["retraction"] = len(set(st["cases"]) - set(seen))
    st["cases"] = seen
    out({"status": "ok", "result": {"sources": [one]}})
if args[:1] == ["search"]:
    kind, where = body["object_type"], body.get("where")
    if kind == "case":
        rows = [(k, f) for k, f in sorted(st["cases"].items(), key=lambda x: x[1]["case_id"]) if f["suite"] == where["eq"][1]]
    elif kind == "kpi":
        rows = [(k, f) for k, f in sorted(st["kpis"].items()) if f["name"] == where["eq"][1]]
    elif kind == "task":
        rows = [(t, {"directory": "/fixture/" + t}) for t in CONFIG.get("tasks", [])]
    else:
        rows = []
    start = int(body.get("continuation") or 0)
    chunk = rows[start:start + 2]
    objs = [{"id": ident(kind, k), "head": {"fields": {p: {"kind": "value", "value": v} for p, v in f.items()}}} for k, f in chunk]
    nxt = str(start + 2) if start + 2 < len(rows) else None
    out({"status": "ok", "result": {"objects": objs, "basis": {"qualified": CONFIG.get("qualified", True)}, "continuation": nxt}})
if args[:2] == ["action", "apply"]:
    a, p, rid = body["action_type"], body["parameters"], body["request_id"]
    refuse = CONFIG.get("refuse", {}).get(a)
    if refuse:
        out({"status": "refused", "code": "submission_criteria", "failed": [refuse]}, 2)
    if rid in st["rids"]:
        out({"status": "ok", "result": {"outcome": "already_applied", "edits": []}})
    st["rids"][rid] = a
    if a == "define_kpi":
        st["n"] += 1
        key = "kpi-%d" % st["n"]
        st["kpis"][key] = {"task": p["task"], "name": p["name"], "definition": p["definition"]}
        out({"status": "ok", "result": {"outcome": "applied", "edits": [{"change": "initial", "id": ident("kpi", key)}]}})
    if a == "record_measurement":
        st["kpis"][p["kpi"]["native_key"]].update(value=p["value"], evidence=p["evidence"], observed_at=p["observed_at"])
        out({"status": "ok", "result": {"outcome": "applied", "edits": [{"change": "initial", "id": ident("measurement", rid)}]}})
out({"status": "refused", "code": "unknown_command"}, 2)
'''

GIT_ID = ["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "-c", "commit.gpgsign=false"]


def sha(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def git(d, *args):
    done = subprocess.run(["git", "-C", str(d), *GIT_ID, *args], capture_output=True, text=True)
    assert done.returncode == 0, (args, done.stderr)
    return done.stdout.strip()


def make_repo(root, name):
    d = Path(root) / name
    d.mkdir()
    git(d, "-c", "init.defaultBranch=main", "init", "-q")
    return d


def commit(d, files, message="c"):
    """Writes each path (None removes it), commits, and returns the commit; origin/main is moved to it."""
    for path, text in files.items():
        p = Path(d) / path
        if text is None:
            p.unlink()
            continue
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(text, encoding="utf-8")
    git(d, "add", "-A")
    git(d, "commit", "-q", "--allow-empty", "-m", message)
    rev = git(d, "rev-parse", "HEAD")
    git(d, "update-ref", "refs/remotes/origin/main", rev)
    return rev


def facts(*strings):
    return [{"fact": [s]} for s in strings]


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="cases-test-"))
        self.addCleanup(shutil.rmtree, self.tmp, ignore_errors=True)
        # The tree the programs run from, and the fake engine.
        self.tree = self.tmp / "tree"
        (self.tree / "evals" / "cases").mkdir(parents=True)
        (self.tree / "evals" / "batch").mkdir()
        for name in ("lib.py", "index", "lost", "select", "reproduce"):
            shutil.copy2(REPO / "evals" / "cases" / name, self.tree / "evals" / "cases" / name)
        shutil.copy2(REPO / "evals" / "batch" / "lib.py", self.tree / "evals" / "batch" / "lib.py")
        for suite in ("routing", "intake"):
            (self.tree / "evals" / suite).mkdir()
        self.fake = self.tmp / "fake"
        self.fake.mkdir()
        (self.fake / "ont").write_text(FAKE_ONT.replace("@PY@", PY))
        (self.fake / "ont").chmod(0o755)
        self.dir = self.tmp / "cases-dir"
        self.config = {"cases_dir": str(self.dir), "tasks": ["task-one"]}
        self.write_config()
        self.env = {"PATH": f"{self.fake}:{os.environ.get('PATH', '/usr/bin:/bin')}", "HOME": HOME, "LC_ALL": "C.UTF-8"}
        self.store = str(self.tmp / "store.sqlite")
        self.repo = make_repo(self.tmp, "repo")
        self.also = make_repo(self.tmp, "other")

    def write_config(self, **change):
        self.config.update(change)
        (self.fake / "config.json").write_text(json.dumps(self.config))

    def calls(self):
        p = self.fake / "calls.jsonl"
        return [json.loads(x) for x in p.read_text().splitlines()] if p.exists() else []

    def actions(self, name):
        return [c["body"] for c in self.calls() if c["argv"][:2] == ["action", "apply"] and c["body"]["action_type"] == name]

    def case_file(self, suite, cases, name):
        """Writes a case file outside the tree and admits its digest on the tree's list; returns its path."""
        path = self.tmp / f"{name}.json"
        data = json.dumps(cases)
        path.write_text(data)
        with open(self.tree / "evals" / suite / "host-cases.sha256", "a") as f:
            f.write(f"{sha(data)}  {name}\n")
        return path

    def program(self, name):
        return str(self.tree / "evals" / "cases" / name)

    def run_program(self, name, *args, env=None):
        done = subprocess.run([PY, "-I", "-B", self.program(name), *map(str, args)], capture_output=True, text=True,
                              env=env or self.env, cwd=str(self.tmp))
        self.assertEqual(done.stderr, "", (name, args, done.stderr))
        lines = [json.loads(x) for x in done.stdout.splitlines()]
        return done.returncode, lines

    def refusal(self, name, *args):
        code, lines = self.run_program(name, *args)
        self.assertEqual(code, 2, lines)
        self.assertEqual(len(lines), 1, lines)
        self.assertEqual(list(lines[0]), ["status"])
        return lines[0]["status"]

    # A repository with two homes: docs/a.md holds the fact of `ca`, docs/b.md the fact of `cb`.
    def standard(self):
        self.base = commit(self.repo, {"README.md": "# Readme\nStart here.\n", "docs/a.md": "# A\nThe backup pass keeps fourteen nightly copies.\n",
                                       "docs/b.md": "# B\nThe gate waits ten minutes for a merge.\n"}, "base")
        self.routing = self.case_file("routing", [
            {"id": "ca", "situation": "s", "must": facts("fourteen nightly copies"), "wrong": []},
            {"id": "cb", "situation": "s", "must": facts("ten minutes for a merge"), "wrong": []},
            {"id": "cn", "situation": "s", "must": facts("nothing in any file says this"), "wrong": []}], "host-routing-t")
        self.intake = self.case_file("intake", [
            {"id": "ia", "words": "w", "seed": "none", "expect": {"lane": "none", "out_of_order": None, "missing": []}}], "host-intake-t")
        self.cases = ["--cases", f"routing={self.routing}", "--cases", f"intake={self.intake}"]

    def add_line(self, text="A new sentence about nothing.\n"):
        """A change that only adds a line to docs/a.md; returns the new head."""
        return commit(self.repo, {"docs/a.md": "# A\nThe backup pass keeps fourteen nightly copies.\n" + text}, "head")

    def gate(self, readers, name="gate.json"):
        path = self.tmp / name
        path.write_text(json.dumps({"status": "fail", "readers": readers}))
        return path

    def select_args(self, base, head, *more):
        return [str(x) for x in ["--repository", f"o/r={self.repo}", "--base", base, "--head", head, *self.cases, "--directory",
                                 self.dir, "--store", self.store, *more]]

    def reproduce_args(self, base, head, gate, *more):
        return [str(x) for x in ["--repository", f"o/r={self.repo}", "--base", base, "--head", head, *self.cases, "--gate",
                                 gate, *more]]


class LostTests(Base):
    def lost(self, base, head, *more):
        return self.run_program("lost", "--repository", f"o/r={self.repo}", "--base", base, "--head", head, *more)

    def test_lost_line_without_term(self):
        a = commit(self.repo, {"c/A.md": "# A\nThe backup pass keeps fourteen nightly copies.\nThe gate waits ten minutes for a merge.\n"})
        b = commit(self.repo, {"c/A.md": "# A\n", "c/B.md": "# B\nThe gate waits ten minutes for a merge.\n"})
        code, lines = self.lost(a, b)
        self.assertEqual(code, 1)
        lost, totals = lines[:-1], lines[-1]
        self.assertEqual([(x["path"], x["line"], x["accepted"]) for x in lost], [("c/A.md", 2, False)])
        self.assertEqual(lost[0]["text"], "The backup pass keeps fourteen nightly copies.")
        self.assertEqual(lost[0]["digest"], sha(lost[0]["text"]))
        self.assertEqual(totals, {"files": 1, "read": 2, "kept": 1, "skipped": 0, "lost": 1, "accepted": 0})

    def test_moved_line_is_kept(self):
        a = commit(self.repo, {"c/A.md": "# A\n- `ap run` starts one implementer session in a worktree.\nThe gate waits ten minutes for a merge here.\n"})
        # The first line moves to another file under another list marker; the second is rewrapped inside its paragraph.
        b = commit(self.repo, {"c/A.md": "# A\nThe gate waits ten minutes\nfor a merge here.\n",
                               "c/B.md": "# B\n* `ap run` starts one implementer session in a worktree.\n"})
        code, lines = self.lost(a, b)
        self.assertEqual((code, lines[-1]["read"], lines[-1]["kept"], lines[-1]["lost"]), (0, 2, 2, 0), lines)
        # The same lines gone from every file are lost.
        c = commit(self.repo, {"c/A.md": "# A\nOnly this.\n", "c/B.md": None})
        code, lines = self.lost(a, c)
        self.assertEqual((code, lines[-1]["lost"]), (1, 2), lines)

    def test_section_keeps_reworded_line(self):
        a = commit(self.repo, {"c/A.md": "# Gate\nThe nightly gate runs `evals/cases/select` before the readers.\n# Other\nUnrelated words.\n"})
        # Reworded inside one section: every term and word is still in the section.
        b = commit(self.repo, {"c/A.md": "# Gate\nBefore the readers, `evals/cases/select` runs: this is the nightly gate.\n# Other\nUnrelated words.\n"})
        self.assertEqual(self.lost(a, b)[0], 0)
        # Reworded across two sections: lost.
        c = commit(self.repo, {"c/A.md": "# Gate\nThe nightly gate.\n# Other\nUnrelated words. Before the readers, `evals/cases/select` runs.\n"})
        code, lines = self.lost(a, c)
        self.assertEqual((code, lines[-1]["lost"]), (1, 1))

    def test_skipped_and_unread_lines(self):
        a = commit(self.repo, {"c/A.md": "# A\n\n```bash\n| --- | :-: |\n- 12\nok\n```\n"})
        b = commit(self.repo, {"c/A.md": "# A\n"})
        code, lines = self.lost(a, b)
        # blank, fence lines and the table rule are not read; `12` and `ok` have no word of four letters or a term in `ok`.
        self.assertEqual(lines[-1]["read"], 2, lines)
        self.assertEqual(lines[-1]["skipped"], 1, lines)
        self.assertEqual(lines[-1]["lost"], 1, lines)  # `12` holds a digit, it is a term, and no kept text holds it

    def test_accepted_digest(self):
        a = commit(self.repo, {"c/A.md": "# A\nThe backup pass keeps fourteen nightly copies.\n"})
        b = commit(self.repo, {"c/A.md": "# A\n"})
        code, lines = self.lost(a, b)
        self.assertEqual(code, 1)
        accepted = self.tmp / "accepted"
        accepted.write_text("\n" + lines[0]["digest"] + "\n")
        code, lines = self.lost(a, b, "--accepted", accepted)
        self.assertEqual(code, 0)
        self.assertEqual((lines[0]["accepted"], lines[-1]["lost"], lines[-1]["accepted"]), (True, 1, 1))
        accepted.write_text("not a digest\n")
        self.assertEqual(self.refusal("lost", "--repository", f"o/r={self.repo}", "--base", a, "--head", b, "--accepted", accepted),
                         "accepted_unreadable")
        self.assertEqual(self.refusal("lost", "--repository", f"o/r={self.repo}", "--base", a, "--head", b, "--accepted", self.tmp / "none"),
                         "accepted_unreadable")

    def test_also_clone_keeps_a_moved_line(self):
        a = commit(self.repo, {"c/A.md": "# A\nThe backup pass keeps fourteen nightly copies.\n"})
        b = commit(self.repo, {"c/A.md": "# A\n"})
        commit(self.also, {"docs/m.md": "# M\nThe backup pass keeps fourteen nightly copies.\n"})
        code, lines = self.lost(a, b, "--also", f"o/other={self.also}")
        self.assertEqual((code, lines[-1]["lost"]), (0, 0), lines)

    def test_deleted_file_and_kept_in_code(self):
        a = commit(self.repo, {"c/A.md": "# A\nThe nightly `max_widgets=7` bound is checked.\n", "src/x.py": "MAX = 'the nightly `max_widgets=7` bound is checked.'\n"})
        b = commit(self.repo, {"c/A.md": None})
        code, lines = self.lost(a, b)
        self.assertEqual((code, lines[-1]["lost"], lines[-1]["files"]), (0, 0, 1), lines)  # the line is kept in a Python file
        c = commit(self.repo, {"src/x.py": None})
        self.assertEqual(self.lost(b, c)[0], 0)

    def test_lost_line_text_is_cut(self):
        text = " ".join(["Gate"] * 80) + " lengthy"
        a = commit(self.repo, {"c/A.md": f"# A\n{text}\n"})
        b = commit(self.repo, {"c/A.md": "# A\n"})
        code, lines = self.lost(a, b)
        self.assertLessEqual(len(lines[0]["text"].encode()), 300)
        self.assertEqual(lines[0]["digest"], sha(" ".join(text.split())))

    def test_academy_lines_not_read(self):
        # A tracked file under evals/lesson/ is the Academy's: none of its lines is read or printed, by any program.
        a = commit(self.repo, {"evals/lesson/notes.md": "# Notes\nA secret sentence about a student schedule.\n", "docs/a.md": "# A\nThe backup pass keeps fourteen nightly copies.\n"})
        b = commit(self.repo, {"evals/lesson/notes.md": "# Notes\nA changed sentence about a student schedule.\n", "docs/a.md": "# A\nThe backup pass keeps fourteen nightly copies.\nNew.\n"})
        code, lines = self.lost(a, b)
        self.assertEqual((code, lines[-1]["files"], lines[-1]["read"], lines[-1]["lost"]), (0, 1, 0, 0), lines)  # only docs/a.md
        self.assertNotIn("student", json.dumps(lines))
        c = commit(self.repo, {"evals/lesson/notes.md": None})
        code, lines = self.lost(b, c)
        self.assertEqual((code, lines[-1]["files"]), (0, 0), lines)
        # select reads and prints none of it either, and no case is served by it.
        routing = self.case_file("routing", [{"id": "cs", "situation": "s", "must": facts("student schedule"), "wrong": []}], "host-routing-s")
        code, lines = self.run_program("select", "--repository", f"o/r={self.repo}", "--base", a, "--head", b, "--cases", f"routing={routing}",
                                       "--directory", self.dir, "--store", self.store)
        self.assertEqual(code, 0, lines)
        self.assertNotIn("student", json.dumps(lines))
        written = (self.dir / "routing.cases.jsonl").read_text()
        self.assertNotIn("lesson", written)
        record = json.loads(written)
        self.assertEqual((record["all"], record["unhomed"], record["serves"]), (True, 1, []))


class IndexTests(Base):
    def index(self, *more, also=False):
        repositories = ["--repository", f"o/r={self.repo}"] + (["--repository", f"o/other={self.also}"] if also else [])
        return self.run_program("index", *repositories, *self.cases, "--directory", self.dir, "--store", self.store, *more)

    def records(self, suite):
        return [json.loads(x) for x in (self.dir / f"{suite}.cases.jsonl").read_text().splitlines()]

    def test_serves_and_all(self):
        self.standard()
        other = commit(self.also, {"x/c.md": "# C\nAlso: the gate waits ten minutes for a merge.\n"})
        code, lines = self.index(also=True)
        self.assertEqual(code, 0, lines)
        self.assertEqual(lines[0]["status"], "ok")
        self.assertEqual(lines[0]["suites"], {"routing": {"cases": 3, "all": 1, "unhomed": 1}, "intake": {"cases": 1, "all": 1, "unhomed": 0}})
        self.assertEqual(lines[0]["index"]["status"], "ok")
        by = {r["case_id"]: r for r in self.records("routing")}
        self.assertEqual(by["ca"]["serves"], ["o/r:docs/a.md"])
        self.assertEqual(by["cb"]["serves"], ["o/other:x/c.md", "o/r:docs/b.md"])
        self.assertEqual((by["ca"]["all"], by["ca"]["unhomed"], by["ca"]["facts"]), (False, 0, 1))
        self.assertEqual((by["cn"]["all"], by["cn"]["unhomed"], by["cn"]["serves"]), (True, 1, []))
        self.assertEqual(by["ca"]["key"], sha("routing:ca"))
        self.assertEqual(by["ca"]["commits"], sorted([f"o/other@{other}", f"o/r@{self.base}"]))
        self.assertEqual(by["ca"]["file"], "host-routing-t")
        self.assertEqual(by["ca"]["file_digest"], hashlib.sha256(self.routing.read_bytes()).hexdigest())
        self.assertEqual(by["ca"]["map_digest"], hashlib.sha256((REPO / "evals" / "cases" / "lib.py").read_bytes()).hexdigest())
        (ia,) = self.records("intake")
        self.assertEqual((ia["all"], ia["facts"], ia["serves"], ia["type"]), (True, 0, [], "case"))
        # No text of a case in a record, and no host path.
        text = (self.dir / "routing.cases.jsonl").read_text() + (self.dir / "intake.cases.jsonl").read_text()
        self.assertNotIn("fourteen", text)
        self.assertNotIn(str(self.tmp), text)
        self.assertEqual(stat.S_IMODE((self.dir / "routing.cases.jsonl").stat().st_mode), 0o644)
        self.assertEqual(stat.S_IMODE(self.dir.stat().st_mode), 0o755)
        self.assertFalse([p.name for p in self.dir.iterdir() if p.name.startswith(".") and p.name != ".lock"])

    def test_at_revision_and_origin_main(self):
        self.standard()
        later = commit(self.repo, {"docs/a.md": "# A\nNothing is kept here now.\n"})
        code, lines = self.index("--at", f"o/r={self.base}")
        self.assertEqual(code, 0, lines)
        self.assertEqual({r["case_id"]: r["serves"] for r in self.records("routing")}["ca"], ["o/r:docs/a.md"])
        code, lines = self.index()  # origin/main is `later`: the fact has no home
        self.assertEqual(lines[0]["suites"]["routing"]["unhomed"], 2)
        self.assertIn(f"o/r@{later}", self.records("routing")[0]["commits"])

    def test_record_over_the_limit_serves_all(self):
        self.standard()
        many = {f"docs/f{i:04d}_{'x' * 90}.md": "# F\nThe gate waits ten minutes for a merge.\n" for i in range(700)}
        commit(self.repo, many)
        code, lines = self.index("--at", f"o/r={git(self.repo, 'rev-parse', 'HEAD')}")
        self.assertEqual(code, 0, lines)
        by = {r["case_id"]: r for r in self.records("routing")}
        self.assertEqual((by["cb"]["all"], by["cb"]["serves"]), (True, []))
        self.assertEqual(by["cb"]["unhomed"], 0)
        self.assertLess(len(json.dumps(by["cb"])), 60000)

    def test_a_suite_not_given_stays_and_a_removed_case_goes(self):
        self.standard()
        self.index()
        before = (self.dir / "intake.cases.jsonl").read_text()
        only = self.case_file("routing", [{"id": "ca", "situation": "s", "must": facts("fourteen nightly copies"), "wrong": []}], "host-routing-u")
        code, lines = self.run_program("index", "--repository", f"o/r={self.repo}", "--cases", f"routing={only}", "--directory", self.dir, "--store", self.store)
        self.assertEqual(code, 0, lines)
        self.assertEqual((self.dir / "intake.cases.jsonl").read_text(), before)
        self.assertEqual([r["case_id"] for r in self.records("routing")], ["ca"])
        self.assertEqual(lines[0]["index"]["retracted"], 2)

    def test_index_failed_exit_3(self):
        self.standard()
        self.write_config(index_code="glob_root_missing")
        code, lines = self.index()
        self.assertEqual(code, 3)
        self.assertEqual((lines[0]["status"], lines[0]["index"]["code"]), ("index_failed", "glob_root_missing"))
        self.assertEqual(len(lines), 1)

    def test_scratch_store_and_host_directory_are_kept_apart(self):
        self.standard()
        base = ["--repository", f"o/r={self.repo}", *self.cases]
        self.assertEqual(self.refusal("index", *base, "--store", self.store), "usage")
        self.assertEqual(self.refusal("index", *base, "--directory", self.dir), "usage")
        self.assertEqual(self.refusal("index", *base, "--directory", HOST_DIRECTORY, "--store", self.store), "usage")
        self.assertEqual(self.refusal("select", "--repository", f"o/r={self.repo}", "--base", self.base, "--head", self.base, *self.cases, "--store", self.store), "usage")
        self.assertEqual(self.refusal("select", "--repository", f"o/r={self.repo}", "--base", self.base, "--head", self.base, *self.cases, "--directory", self.dir), "usage")
        self.assertEqual(self.refusal("reproduce", "--repository", f"o/r={self.repo}", "--base", self.base, "--head", self.base, *self.cases, "--gate", self.gate({}), "--directory", self.dir), "usage")
        self.assertEqual(self.calls(), [])
        self.assertFalse(self.dir.exists())

    def test_refusals_before_any_effect(self):
        self.standard()
        r = f"o/r={self.repo}"
        self.assertEqual(self.refusal("index"), "usage")
        self.assertEqual(self.refusal("index", "--repository", r), "usage")
        self.assertEqual(self.refusal("index", "--repository", r, "--cases", f"other={self.routing}"), "usage")
        self.assertEqual(self.refusal("index", "--repository", "norepo=" + str(self.repo), "--cases", f"routing={self.routing}"), "usage")
        self.assertEqual(self.refusal("index", "--repository", r, "--at", f"o/zzz={self.base}", *self.cases), "usage")
        self.assertEqual(self.refusal("index", "--repository", r, "--bogus", "x", *self.cases), "usage")
        self.assertEqual(self.refusal("index", "--repository", f"o/r={self.tmp / 'nothing'}", *self.cases, "--directory", self.dir, "--store", self.store), "repo_unreadable")
        self.assertEqual(self.refusal("index", "--repository", r, "--at", f"o/r={'0' * 40}", *self.cases, "--directory", self.dir, "--store", self.store), "repo_unreadable")
        self.assertEqual(self.refusal("index", "--repository", f"o/other={self.also}", *self.cases, "--directory", self.dir, "--store", self.store), "repo_unreadable")  # no commit yet
        missing = ["--directory", self.dir, "--store", self.store]
        self.assertEqual(self.refusal("index", "--repository", r, "--cases", f"routing={self.tmp / 'none.json'}", *missing), "cases_unreadable")
        unknown = self.tmp / "unknown.json"
        unknown.write_text("[]")
        self.assertEqual(self.refusal("index", "--repository", r, "--cases", f"routing={unknown}", *missing), "cases_digest_unknown")
        bad = self.case_file("routing", [{"id": "x", "situation": "s", "must": []}], "bad")
        self.assertEqual(self.refusal("index", "--repository", r, "--cases", f"routing={bad}", *missing), "cases_invalid")
        notlist = self.case_file("routing", {"id": "x"}, "notlist")
        self.assertEqual(self.refusal("index", "--repository", r, "--cases", f"routing={notlist}", *missing), "cases_invalid")
        twice = self.case_file("routing", [{"id": "x", "situation": "s", "must": facts("a")}, {"id": "x", "situation": "s", "must": facts("b")}], "twice")
        self.assertEqual(self.refusal("index", "--repository", r, "--cases", f"routing={twice}", *missing), "case_id_twice")
        # A routing file does not pass as an intake file by its digest on the routing list.
        self.assertEqual(self.refusal("index", "--repository", r, "--cases", f"intake={self.routing}", *missing), "cases_digest_unknown")
        self.assertEqual(self.calls(), [])
        self.assertFalse(self.dir.exists())

    def test_case_files_of_the_repository_are_admitted_as_they_are(self):
        # The program of this repository reads the files of its own evals/ without a digest line.
        self.standard()
        self.write_config()
        env = dict(self.env)
        done = subprocess.run([PY, "-I", "-B", str(REPO / "evals" / "cases" / "index"), "--repository", f"o/r={self.repo}", "--cases",
                               f"routing={REPO / 'evals' / 'routing' / 'cases.json'}", "--cases", f"intake={REPO / 'evals' / 'intake' / 'cases.json'}",
                               "--directory", str(self.dir), "--store", self.store], capture_output=True, text=True, env=env)
        self.assertEqual(done.stderr, "")
        line = json.loads(done.stdout)
        self.assertEqual(line["status"], "ok", line)
        files = {r["file"] for s in ("routing", "intake") for r in self.records(s)}
        self.assertEqual(files, {"evals/routing/cases.json", "evals/intake/cases.json"})
        n = {s: len(json.loads((REPO / "evals" / s / "cases.json").read_text())) for s in ("routing", "intake")}
        self.assertEqual({s: line["suites"][s]["cases"] for s in n}, n)
        # A file outside the repository whose digest is on no list of it is not admitted.
        link = self.tmp / "outside.json"
        link.write_text((REPO / "evals" / "routing" / "cases.json").read_text() + " ")
        done = subprocess.run([PY, "-I", "-B", str(REPO / "evals" / "cases" / "index"), "--repository", f"o/r={self.repo}", "--cases", f"routing={link}",
                               "--directory", str(self.dir), "--store", self.store], capture_output=True, text=True, env=env)
        self.assertEqual(json.loads(done.stdout), {"status": "cases_digest_unknown"})


class SelectTests(Base):
    def select(self, base, head, *more):
        return self.run_program("select", *self.select_args(base, head, *more))

    def reproduce(self, base, head, readers, *more):
        gate = self.gate(readers)
        return self.run_program("reproduce", *self.reproduce_args(base, head, gate, *more))

    def reproduced(self, head):
        """The KPI of the standard change, made by `reproduce --record` into the fake store."""
        code, lines = self.reproduce(self.base, head, {"claude-sonnet": {"ok": False, "lost": ["ca"]}}, "--record", "task-one", "--store", self.store)
        self.assertEqual(code, 0, lines)
        return lines

    def test_full_without_reproduction(self):
        self.standard()
        head = self.add_line()
        code, lines = self.select(self.base, head)
        self.assertEqual(code, 0, lines)
        routing, intake, total = lines
        self.assertEqual((routing["suite"], routing["mode"], routing["cases"], routing["args"]), ("routing", "full", ["ca", "cb", "cn"], "--case ca --case cb --case cn"))
        self.assertEqual((intake["suite"], intake["mode"], intake["cases"], intake["args"]), ("intake", "full", ["ia"], "--case ia"))
        self.assertEqual((total["mode"], total["selected"], total["of"]), ("full", 4, 4))
        self.assertEqual(total["map_digest"], hashlib.sha256((self.tree / "evals/cases/lib.py").read_bytes()).hexdigest())
        # The index ran with the changed repository at the base.
        by = {json.loads(l)["case_id"]: json.loads(l) for l in (self.dir / "routing.cases.jsonl").read_text().splitlines()}
        self.assertIn(f"o/r@{self.base}", by["ca"]["commits"])

    def test_subset_after_reproduction(self):
        self.standard()
        head = self.add_line()
        lines = self.reproduced(head)
        value = lines[0]
        self.assertEqual((value["reproduced"], value["failures"], value["selected"], value["cases"], value["missed"], value["recorded"]), (True, 1, 3, 4, [], "applied"))
        code, lines = self.select(self.base, head)
        self.assertEqual(code, 0, lines)
        routing, intake, total = lines
        self.assertEqual((routing["mode"], routing["cases"], routing["args"]), ("subset", ["ca", "cn"], "--case ca --case cn"))  # cb is not reached; cn serves every path
        self.assertEqual((intake["mode"], intake["cases"]), ("subset", ["ia"]))
        self.assertEqual((total["mode"], total["selected"], total["of"]), ("subset", 3, 4))
        # Every call of the engine named the scratch store and the client `cases`.
        self.assertTrue(self.calls() and all(c["store"] == self.store and c["client"] in (None, "cases") for c in self.calls()))
        # A change that reaches no home selects only the cases that serve every path; an empty selection has no arguments.
        none_head = commit(self.repo, {"other/x.md": "# X\nUnrelated.\n"})
        code, lines = self.select(head, none_head)
        self.assertEqual([x["cases"] for x in lines[:2]], [["cn"], ["ia"]], lines)
        # A suite none of whose cases is reached prints an empty list and empty arguments: no reader starts.
        only = self.case_file("routing", [{"id": "ca", "situation": "s", "must": facts("fourteen nightly copies"), "wrong": []}], "host-routing-v")
        code, lines = self.run_program("select", "--repository", f"o/r={self.repo}", "--base", head, "--head", none_head, "--cases", f"routing={only}",
                                       "--directory", self.dir, "--store", self.store)
        self.assertEqual(lines[0], {"suite": "routing", "mode": "subset", "cases": [], "args": ""})
        self.assertEqual((lines[1]["selected"], lines[1]["of"]), (0, 1))

    def test_subset_names_the_route_to_a_home(self):
        self.standard()
        head = self.add_line()
        self.reproduced(head)
        # A change of a line that names the home of `cb` selects it, though no file of its home changed.
        routed = commit(self.repo, {"README.md": "# Readme\nStart here, then read b.md.\n"})
        code, lines = self.select(head, routed)
        self.assertEqual(lines[0]["cases"], ["cb", "cn"], lines)
        # A new home: the rename of docs/b.md changes a path the case serves.
        moved = commit(self.repo, {"docs/b.md": None, "docs/moved.md": "# B\nThe gate waits ten minutes for a merge.\n"})
        code, lines = self.select(routed, moved)
        self.assertEqual(lines[0]["cases"], ["cb", "cn"], lines)

    def test_full_after_rule_change(self):
        self.standard()
        head = self.add_line()
        self.reproduced(head)
        self.assertEqual(self.select(self.base, head)[1][-1]["mode"], "subset")
        # A change of a rule changes the digest of lib.py: the reproduction no longer holds.
        with open(self.tree / "evals" / "cases" / "lib.py", "a") as f:
            f.write("\n# a changed rule\n")
        code, lines = self.select(self.base, head)
        self.assertEqual(code, 0, lines)
        self.assertEqual((lines[-1]["mode"], lines[-1]["selected"], lines[-1]["of"]), ("full", 4, 4))
        self.assertEqual(lines[-1]["map_digest"], hashlib.sha256((self.tree / "evals/cases/lib.py").read_bytes()).hexdigest())

    def test_lost_stops_select(self):
        self.standard()
        head = commit(self.repo, {"docs/a.md": "# A\n"})
        code, lines = self.select(self.base, head)
        self.assertEqual(code, 1, lines)
        self.assertEqual(lines[-1], {"status": "lost_facts", "lost": 1})
        self.assertEqual([x["text"] for x in lines[:-2]], ["The backup pass keeps fourteen nightly copies."])
        self.assertEqual(lines[-2]["lost"], 1)
        # Nothing was indexed or selected: no call of the engine, no record file, no directory.
        self.assertEqual(self.calls(), [])
        self.assertFalse(self.dir.exists())
        # Accepted by digest, it goes on.
        accepted = self.tmp / "accepted"
        accepted.write_text(lines[0]["digest"] + "\n")
        code, lines = self.select(self.base, head, "--accepted", accepted)
        self.assertEqual(code, 0, lines)
        self.assertEqual(lines[-1]["mode"], "full")

    def test_select_busy(self):
        self.standard()
        head = self.add_line()
        self.dir.mkdir(mode=0o755)
        loader = importlib.machinery.SourceFileLoader("cases_select_busy", self.program("select"))
        module = importlib.util.module_from_spec(importlib.util.spec_from_loader("cases_select_busy", loader))
        loader.exec_module(module)
        module.L.LOCK_WAIT = 1
        fd = os.open(self.dir / ".lock", os.O_RDWR | os.O_CREAT, 0o644)
        fcntl.flock(fd, fcntl.LOCK_EX)
        try:
            with mock.patch.dict(os.environ, self.env, clear=True), self.assertRaises(module.L.Refuse) as caught:
                module.run(self.select_args(self.base, head))
        finally:
            os.close(fd)
        self.assertEqual((caught.exception.answer, caught.exception.exit_code), ({"status": "busy"}, 2))
        # It wrote nothing and asked the engine nothing.
        self.assertEqual(sorted(p.name for p in self.dir.iterdir()), [".lock"])
        self.assertEqual(self.calls(), [])

    def test_unreadable_store_objects_mean_full(self):
        self.standard()
        head = self.add_line()
        self.reproduced(head)
        self.write_config(qualified=False)
        code, lines = self.select(self.base, head)
        self.assertEqual((code, lines[-1]["mode"], lines[-1]["selected"]), (0, "full", 4))

    def test_academy_path_refused(self):
        self.standard()
        head = self.add_line()
        nothing = self.tmp / "no-such-place"
        for programs, bad in (
                (("index", "select", "reproduce", "lost"), ["--repository", f"o/education-platform={self.repo}"]),
                (("index", "select", "reproduce", "lost"), ["--repository", f"o/edu-x={self.repo}"]),
                (("index", "select", "reproduce", "lost"), ["--repository", f"o/r={nothing}/education-platform"]),
                (("index", "select", "reproduce", "lost"), ["--repository", f"o/r=/nowhere/edu-lesson/clone"]),
                (("index", "select", "reproduce", "lost"), ["--repository", f"o/r={self.repo}/../edu-x"]),
                (("select", "reproduce", "lost"), ["--repository", f"o/r={self.repo}", "--also", f"o/education-platform={self.also}"]),
                (("select", "reproduce", "lost"), ["--repository", f"o/r={self.repo}", "--also", f"o/other=/nowhere/education-platform/x"]),
                (("index", "select", "reproduce"), ["--repository", f"o/r={self.repo}", "--cases", "routing=/nowhere/edu-cases/cases.json"]),
                (("index", "select", "reproduce"), ["--repository", f"o/r={self.repo}", "--cases", "routing=/nowhere/evals/lesson/cases.json"]),
                (("index", "select"), ["--repository", f"o/r={self.repo}", "--directory", "/nowhere/edu-out", "--store", self.store]),
                (("reproduce",), ["--repository", f"o/r={self.repo}", "--gate", "/nowhere/education-platform/gate.json"]),
                (("lost", "select"), ["--repository", f"o/r={self.repo}", "--accepted", "/nowhere/edu-x/accepted"])):
            for program in programs:
                extra = []
                if program in ("lost", "select", "reproduce"):
                    extra += ["--base", self.base, "--head", head]
                if program != "lost" and "--cases" not in bad:
                    extra += self.cases
                if program == "reproduce" and "--gate" not in bad:
                    extra += ["--gate", self.gate({})]
                if program in ("index", "select") and "--directory" not in bad:
                    extra += ["--directory", self.dir, "--store", self.store]
                self.assertEqual(self.refusal(program, *bad, *extra), "academy_path", (program, bad))
        self.assertEqual(self.calls(), [])
        self.assertFalse(self.dir.exists())

    def test_reproduce_missed_case(self):
        self.standard()
        head = self.add_line()
        # `cb` failed but the change does not reach it: not reproduced, the case is named.
        code, lines = self.reproduce(self.base, head, {"claude-sonnet": {"ok": False, "lost": ["ca", "cb"]}})
        self.assertEqual(code, 1, lines)
        self.assertEqual(lines, [{"reproduced": False, "map_digest": lines[0]["map_digest"], "failures": 2, "selected": 3, "cases": 4, "missed": ["cb"]}])
        # A failing id that no case has is missed too.
        code, lines = self.reproduce(self.base, head, {"claude-sonnet": {"ok": False, "lost": ["no-such-case"]}})
        self.assertEqual((code, lines[0]["missed"]), (1, ["no-such-case"]))
        self.assertEqual(self.calls(), [])  # no store is read without --record

    def test_reproduce_all_selected_is_not_reproduced(self):
        self.standard()
        # Every case serves every path (no fact is homed): the rule names them all, which is no saving.
        every = self.case_file("routing", [{"id": "z1", "situation": "s", "must": facts("homeless one"), "wrong": []},
                                           {"id": "z2", "situation": "s", "must": facts("homeless two"), "wrong": []}], "host-routing-w")
        head = self.add_line()
        gate = self.gate({"claude-sonnet": {"ok": False, "lost": ["z1"]}})
        code, lines = self.run_program("reproduce", "--repository", f"o/r={self.repo}", "--base", self.base, "--head", head,
                                       "--cases", f"routing={every}", "--gate", gate)
        self.assertEqual(code, 1, lines)
        self.assertEqual((lines[0]["reproduced"], lines[0]["selected"], lines[0]["cases"], lines[0]["missed"]), (False, 2, 2, []))

    def test_reproduce_one_reader_loss_is_noise(self):
        self.standard()
        head = self.add_line()
        two = {"claude-sonnet": {"ok": False, "lost": ["ca", "cb"]}, "claude-haiku": {"ok": False, "lost": ["ca"]},
               "muse-ember": {"ok": True, "lost": ["cb"]}}
        code, lines = self.reproduce(self.base, head, two)
        # `cb` is lost by one reader whose ok is false, and by one whose ok is true: not a failure.
        self.assertEqual((code, lines[0]["reproduced"], lines[0]["failures"], lines[0]["missed"]), (0, True, 1, []), lines)
        # Only one reader failed: its `lost` is the failure.
        code, lines = self.reproduce(self.base, head, {"claude-sonnet": {"ok": False, "lost": ["ca", "cb"]}, "claude-haiku": {"ok": True, "lost": []}})
        self.assertEqual((code, lines[0]["failures"], lines[0]["missed"]), (1, 2, ["cb"]))
        # No reader failed: no failure, nothing reproduced.
        code, lines = self.reproduce(self.base, head, {"claude-sonnet": {"ok": True, "lost": []}})
        self.assertEqual((code, lines[0]["reproduced"], lines[0]["failures"]), (1, False, 0))

    def test_gate_invalid(self):
        self.standard()
        head = self.add_line()
        for text in ("not json", "[]", json.dumps({"readers": {}}), json.dumps({"readers": {"a": {"ok": "no", "lost": []}}}), json.dumps({"readers": {"a": {"ok": True}}})):
            gate = self.tmp / "g.json"
            gate.write_text(text)
            self.assertEqual(self.refusal("reproduce", *self.reproduce_args(self.base, head, gate)), "gate_invalid", text)
        self.assertEqual(self.refusal("reproduce", *self.reproduce_args(self.base, head, self.tmp / "absent.json")), "gate_invalid")

    def test_reproduce_record_already(self):
        self.standard()
        head = self.add_line()
        first = self.reproduced(head)[0]
        digest = first["map_digest"]
        (define,) = self.actions("define_kpi")
        self.assertEqual(define["request_id"], "cases-kpi-" + sha("task-one:case_map")[:32])
        self.assertEqual((define["parameters"]["name"], define["parameters"]["task"]["native_key"]), ("case_map", "task-one"))
        self.assertLessEqual(len(define["parameters"]["definition"].encode()), 600)
        (measure,) = self.actions("record_measurement")
        gate_digest = hashlib.sha256((self.tmp / "gate.json").read_bytes()).hexdigest()
        self.assertEqual(measure["parameters"]["value"], f"reproduced {digest} failures 1 selected 3/4")
        self.assertEqual(measure["parameters"]["evidence"], f"evals/cases/reproduce gate sha256:{gate_digest[:16]} {self.base[:12]}..{head[:12]}")
        self.assertNotIn(str(self.tmp), json.dumps(measure))
        self.assertRegex(measure["parameters"]["observed_at"], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")
        self.assertRegex(measure["request_id"], r"^cases-[0-9a-f]{32}$")
        # The same measurement again applies nothing.
        code, lines = self.reproduce(self.base, head, {"claude-sonnet": {"ok": False, "lost": ["ca"]}}, "--record", "task-one", "--store", self.store)
        self.assertEqual((code, lines[0]["recorded"]), (0, "already"), lines)
        self.assertEqual(len(self.actions("define_kpi")), 1)
        self.assertEqual(len(self.actions("record_measurement")), 1)
        # Another value measures again, on the KPI that exists.
        code, lines = self.reproduce(self.base, head, {"claude-sonnet": {"ok": False, "lost": ["cb"]}}, "--record", "task-one", "--store", self.store)
        self.assertEqual((code, lines[0]["recorded"], lines[0]["reproduced"]), (1, "applied", False), lines)
        self.assertEqual(len(self.actions("define_kpi")), 1)
        (_, second) = self.actions("record_measurement")
        self.assertTrue(second["parameters"]["value"].startswith("not_reproduced " + digest))

    def test_record_failed_exit_3(self):
        self.standard()
        head = self.add_line()
        self.write_config(refuse={"record_measurement": "older_measurement"})
        code, lines = self.reproduce(self.base, head, {"claude-sonnet": {"ok": False, "lost": ["ca"]}}, "--record", "task-one", "--store", self.store)
        self.assertEqual(code, 3, lines)
        self.assertEqual(len(lines), 2)
        self.assertEqual(lines[0]["reproduced"], True)
        self.assertNotIn("recorded", lines[0])
        self.assertEqual(lines[1], {"status": "record_failed", "code": "older_measurement"})
        # A task the store does not have: no code.
        code, lines = self.reproduce(self.base, head, {"claude-sonnet": {"ok": False, "lost": ["ca"]}}, "--record", "no-such-task", "--store", self.store)
        self.assertEqual((code, lines[-1]), (3, {"status": "record_failed", "code": None}))
        self.assertEqual(self.actions("define_kpi")[-1]["parameters"]["task"]["native_key"], "task-one")


if __name__ == "__main__":
    unittest.main()
