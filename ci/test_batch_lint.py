"""Tests of evals/batch/lint/build and read, the spec-lint job of the batch evaluations, run as programs against scratch
repositories made by `git init` and the fake `ont` of ci/test_batch_evals.py (no model, no network, no engine, no store of
the host, no cargo). Contract: evals/batch/lint/README.md."""
import hashlib
import importlib.machinery
import importlib.util
import json
import os
import re
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
LINT = REPO / "evals" / "batch" / "lint"
PY = sys.executable
HOME = os.environ.get("HOME", "/")
PATH = os.environ["PATH"]
COUNT_WORDS = ("read", "refused", "errored", "unreadable", "stale", "duplicate", "skipped")
KINDS = ["ambiguity", "contradiction", "gap"]


def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    mod = importlib.util.module_from_spec(importlib.util.spec_from_loader(name, loader))
    loader.exec_module(mod)
    return mod


# The fake engine and the result lines of the evals job's tests.
EV = load("evals_tests_fake", REPO / "ci" / "test_batch_evals.py")
succeeded, failed_line, GIT_ID = EV.succeeded, EV.failed_line, EV.GIT_ID


def sha(text):
    return hashlib.sha256(text if isinstance(text, bytes) else text.encode("utf-8")).hexdigest()


def canon(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def req(n, text="a sentence"):
    """A requirement line, built from parts so that this file holds none."""
    return "- " + "R" + str(n) + ": " + text + "\n"


def spec(*rules, head="# Spec\n"):
    return head + "\n" + "".join(req(n, "Rule %d holds." % n) for n in rules)


def item(kind, rules, quote="q", why="w"):
    return {"kind": kind, "rules": list(rules), "quote": quote, "why": why}


def answer(cid, *items):
    return succeeded(cid, json.dumps({"items": list(items)}))


class Repo:
    """A scratch repository of files at one commit, with origin/main at it."""

    def __init__(self, root, files):
        self.root = root
        root.mkdir(parents=True)
        self.git("init", "-q")
        self.commit(files)

    def git(self, *args):
        return subprocess.run(["git", *GIT_ID, "-C", str(self.root), *args], check=True, capture_output=True,
                              text=True, env={"PATH": PATH, "HOME": HOME}).stdout

    def commit(self, files):
        for rel, text in files.items():
            p = self.root / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            if text is None:
                p.unlink()
            else:
                p.write_text(text)
        self.git("add", "-A")
        self.git("commit", "-q", "--allow-empty", "-m", "c")
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")
        return self.head()

    def head(self):
        return self.git("rev-parse", "HEAD").strip()


class Case(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.tmp = Path(tmp.name)
        self.n = 0
        self.bin = self.tmp / "bin"
        self.bin.mkdir()
        (self.bin / "ont").write_text(EV.FAKE_ONT.replace("@PY@", PY))
        (self.bin / "ont").chmod(0o755)
        self.ont = str(self.bin / "ont")

    def path(self, name):
        self.n += 1
        return self.tmp / ("%d-%s" % (self.n, name))

    def repo(self, files, name="clone"):
        return Repo(self.path(name), files)

    def run_prog(self, name, args, env=None, code=0):
        env = env if env is not None else {"PATH": PATH, "HOME": HOME}
        p = subprocess.run([PY, "-I", "-B", str(LINT / name), *map(str, args)], capture_output=True, text=True,
                           env=env, cwd=str(self.tmp))
        self.assertEqual(p.stderr, "")
        lines = p.stdout.splitlines()
        self.assertEqual(len(lines), 1, p.stdout)
        self.assertEqual(p.returncode, code, lines[0])
        return json.loads(lines[0])

    def build(self, repos, model="m1", extra=(), out=None, rev="HEAD", code=0):
        """repos: [(name, Repo)]. Returns (answer, files.json, request lines, out)."""
        out = out or self.path("build")
        args = []
        for name, r in repos:
            args += ["--repo", "%s=%s" % (name, r.root)]
        args += ["--model", model, "--out", out, *extra]
        if rev is not None:
            args += ["--rev", rev]
        v = self.run_prog("build", args, code=code)
        if code:
            return v
        return v, self.files(out), self.requests(out), Path(out)

    @staticmethod
    def files(out):
        return json.loads((Path(out) / "files.json").read_text())

    @staticmethod
    def requests(out):
        return [json.loads(x) for x in (Path(out) / "requests.jsonl").read_text().splitlines()]

    def results(self, *lines):
        p = self.path("results.jsonl")
        p.write_text("".join(x + "\n" for x in lines))
        return p

    def env(self, **more):
        env = {"PATH": str(self.bin), "HOME": HOME, "ONT_BIN": self.ont}
        env.update(more)
        return env

    def read(self, out, results, extra=(), env=None, code=0):
        return self.run_prog("read", ["--files", Path(out) / "files.json", "--results", results, *extra],
                             env=env or self.env(), code=code)

    def state(self):
        p = self.tmp / "ont-state.json"
        return json.loads(p.read_text()) if p.exists() else {"findings": {}, "rids": {}, "log": []}

    def put(self, **members):
        st = self.state()
        st.update(members)
        (self.tmp / "ont-state.json").write_text(json.dumps(st))

    def applies(self):
        return [e for e in self.state()["log"] if e["argv"][-4:-1] == ["action", "apply", "--client"]]

    @staticmethod
    def fkey(repository, path, anchor, kind):
        return "batch-spec-lint-" + sha("%s:%s#%s:%s" % (repository, path, anchor, kind))[:16]


AGENT = "park-kyungchan/lifecycle-platform"
ONTOLOGY = "zinvented/second-platform"
TWO = {"specs/a.md": spec(1, 2), "specs/ap/b.md": spec(7, head="# B\n") + req(7, "again"),
       "docs/readme.md": "# Docs\n" + req(1), "specs/notes.txt": "not markdown\n"}


class Builds(Case):
    def test_one_request_per_spec_file(self):
        files = dict(TWO)
        files["specs/evals/" + "lesson/x.md"] = spec(1)
        r = self.repo(files)
        link = r.root / "specs" / "link.md"
        os.symlink("a.md", link)
        r.commit({})
        v, doc, reqs, out = self.build([(AGENT, r)])
        self.assertEqual(v, {"status": "ok", "out": str(out), "files": 2, "requested": 2, "settled": 0, "too_large": 0,
                             "academy_skipped": 1, "result_lines_skipped": 0})
        self.assertEqual(doc["job"], "spec-lint")
        self.assertEqual(doc["model"], "m1")
        self.assertEqual([(f["repository"], f["commit"], f["path"], f["rules"], f["state"]) for f in doc["files"]],
                         [(AGENT, r.head(), "specs/a.md", ["R1", "R2"], "requested"),
                          (AGENT, r.head(), "specs/ap/b.md", ["R7"], "requested")])
        self.assertEqual([x["custom_id"] for x in reqs], [f["key"] for f in doc["files"]])
        system = (LINT / "prompt.md").read_text()
        schema = json.loads((LINT / "verdict.schema.json").read_text())
        for x, f in zip(reqs, doc["files"]):
            p = x["params"]
            self.assertEqual(set(x), {"custom_id", "params"})
            self.assertEqual(set(p), {"model", "max_tokens", "system", "messages", "output_config"})
            self.assertEqual((p["model"], p["max_tokens"], p["system"]), ("m1", 4000, system))
            self.assertEqual(p["output_config"], {"format": {"type": "json_schema", "schema": schema}})
            self.assertEqual(len(p["messages"]), 1)
            self.assertEqual(p["messages"][0]["role"], "user")
            user = p["messages"][0]["content"]
            self.assertIsInstance(user, str)
            self.assertIn(AGENT, user)
            self.assertIn(f["path"], user)
            self.assertIn(TWO[f["path"]], user)
            self.assertNotIn("lesson", user)
            self.assertEqual(x["custom_id"], sha(canon(p)))
        # Nothing of the Academy and nothing that is no spec file is in a request; no checkout file is read.
        text = (out / "requests.jsonl").read_text()
        self.assertNotIn("not markdown", text)
        self.assertNotIn("Docs", text)
        self.assertEqual(stat.S_IMODE(out.stat().st_mode), 0o700)
        for name in ("requests.jsonl", "files.json"):
            self.assertEqual(stat.S_IMODE((out / name).stat().st_mode), 0o600)
        # A change of a checkout file that is not committed changes nothing: the files come from Git's objects.
        (r.root / "specs" / "a.md").write_text("changed in the checkout\n")
        v2, doc2, reqs2, out2 = self.build([(AGENT, r)])
        self.assertEqual((out2 / "requests.jsonl").read_bytes(), (out / "requests.jsonl").read_bytes())
        self.assertEqual((out2 / "files.json").read_bytes(), (out / "files.json").read_bytes())
        r.git("checkout", "--", "specs/a.md")
        # A second build given the result file requests nothing; the key depends on params alone.
        res = self.results(*[answer(f["key"]) for f in doc["files"]])
        v3, doc3, reqs3, out3 = self.build([(AGENT, r)], extra=["--results", res])
        self.assertEqual((v3["requested"], v3["settled"], v3["files"]), (0, 2, 2))
        self.assertEqual((out3 / "requests.jsonl").read_bytes(), b"")
        self.assertEqual([f["state"] for f in doc3["files"]], ["settled", "settled"])
        self.assertEqual([f["key"] for f in doc3["files"]], [f["key"] for f in doc["files"]])
        r.commit({"docs/readme.md": "# Docs\nchanged\n"})
        v4, doc4, _, _ = self.build([(AGENT, r)], extra=["--results", res])
        self.assertEqual((v4["requested"], v4["settled"]), (0, 2))
        self.assertEqual({f["commit"] for f in doc4["files"]}, {r.head()})

    def test_settling_options_and_two_repositories(self):
        a = self.repo(TWO, "agent")
        o = self.repo({"specs/z.md": spec(1), "specs/b.md": spec(2)}, "onto")
        v, doc, reqs, out = self.build([(ONTOLOGY, o), (AGENT, a)])
        self.assertEqual([(f["repository"], f["path"]) for f in doc["files"]],
                         [(AGENT, "specs/a.md"), (AGENT, "specs/ap/b.md"), (ONTOLOGY, "specs/b.md"),
                          (ONTOLOGY, "specs/z.md")])
        keys = [f["key"] for f in doc["files"]]
        # A refusal is settled, an error and a line that is no result line are not; a skipped line is counted.
        refusal = succeeded(keys[0], "x", "refusal")
        res = self.results(refusal, failed_line(keys[1]), failed_line(keys[2], "expired"), "not json", answer(keys[3]))
        v2, doc2, reqs2, out2 = self.build([(AGENT, a), (ONTOLOGY, o)], extra=["--results", res])
        self.assertEqual((v2["requested"], v2["settled"], v2["result_lines_skipped"]), (2, 2, 1))
        self.assertEqual([f["state"] for f in doc2["files"]], ["settled", "requested", "requested", "settled"])
        self.assertEqual([x["custom_id"] for x in reqs2], keys[1:3])
        # A changed model or max_tokens changes every key; the same options give byte-equal files.
        _, doc3, _, out3 = self.build([(AGENT, a), (ONTOLOGY, o)], model="m2")
        self.assertFalse(set(keys) & {f["key"] for f in doc3["files"]})
        _, doc4, _, out4 = self.build([(AGENT, a), (ONTOLOGY, o)], extra=["--max-tokens", "2048"])
        self.assertFalse(set(keys) & {f["key"] for f in doc4["files"]})
        self.assertEqual(self.requests(out4)[0]["params"]["max_tokens"], 2048)
        # The default revision is origin/main.
        _, _, _, out5 = self.build([(AGENT, a), (ONTOLOGY, o)], rev=None)
        self.assertEqual((out5 / "files.json").read_bytes(), (out / "files.json").read_bytes())
        # The form: usage, results_unreadable, repo_unreadable.
        for extra in (["--max-tokens", "1023"], ["--max-tokens", "8001"], ["--max-tokens", "x"], ["--results", ""]):
            self.assertEqual(self.build([(AGENT, a)], extra=extra, code=2), {"status": "usage"}, extra)
        self.assertEqual(self.build([(AGENT, a)], rev="-x", code=2), {"status": "usage"})
        self.assertEqual(self.build([(AGENT, a)], extra=["--results", self.tmp / "missing"], code=2),
                         {"status": "results_unreadable"})
        self.assertEqual(self.build([(AGENT, a)], rev="nobranch", code=2),
                         {"status": "repo_unreadable", "repository": AGENT})
        plain = self.path("plain")
        plain.mkdir()
        v = self.run_prog("build", ["--repo", "o/n=%s" % plain, "--model", "m", "--out", self.path("o")], code=2)
        self.assertEqual(v, {"status": "repo_unreadable", "repository": "o/n"})
        for bad in ("noslash=x", "o/n", "o/n="):
            self.assertEqual(self.run_prog("build", ["--repo", bad, "--model", "m", "--out", self.path("o")], code=2),
                             {"status": "usage"})
        dup = ["--repo", "o/n=%s" % a.root, "--repo", "o/n=%s" % a.root, "--model", "m", "--out", self.path("o")]
        self.assertEqual(self.run_prog("build", dup, code=2), {"status": "usage"})

    def test_too_large_file_gets_no_request(self):
        big = spec(1) + "x" * 300_001 + "\n"
        r = self.repo({"specs/big.md": big, "specs/small.md": spec(1)})
        v, doc, reqs, out = self.build([(AGENT, r)])
        self.assertEqual((v["requested"], v["too_large"]), (1, 1))
        self.assertEqual([(f["path"], f["state"]) for f in doc["files"]],
                         [("specs/big.md", "too_large"), ("specs/small.md", "requested")])
        self.assertEqual(len(reqs), 1)
        self.assertNotIn("xxxx", (out / "requests.jsonl").read_text())
        empty = self.repo({"README.md": "x\n"}, "empty")
        v, doc, reqs, out = self.build([(AGENT, empty)])
        self.assertEqual((v["files"], doc["files"], reqs), (0, [], []))

    def test_rules_are_those_of_the_spec_trace(self):
        trace = load("check_spec_trace", REPO / "ci" / "check-spec-trace.py")
        common = load("lint_common_probe", LINT / "common.py")
        self.assertEqual(common.REQ_RE.pattern, trace.REQ_RE.pattern)
        text = "# T\n" + req(10) + "- R" + "2 not a rule\n" + req(3) + req(10, "later") + " " + req(4) + req(2)
        self.assertEqual(common.rules_of(text), ["R10", "R3", "R2"])
        r = self.repo({"specs/t.md": text})
        v, doc, reqs, out = self.build([(AGENT, r)])
        self.assertEqual(doc["files"][0]["rules"], ["R10", "R3", "R2"])


class Reads(Case):
    def scenario(self, files=None, model="m1"):
        r = self.repo(files or {"specs/a.md": spec(1, 2), "specs/b.md": spec(7)})
        v, doc, reqs, out = self.build([(AGENT, r)], model=model)
        return r, doc, out

    def test_findings_and_anchors(self):
        r, doc, out = self.scenario()
        ka, kb = (f["key"] for f in doc["files"])
        long_why = "w" * 1995 + "ééé"
        res = self.results(
            answer(ka,
                   item("ambiguity", ["R2", "R1"], "quote one"),
                   item("contradiction", ["R9", "R1"], "tab\there\nline\x00end", long_why),
                   item("gap", ["R9"], "no such rule"),
                   item("gap", [], "again", "same signature"),
                   item("ambiguity", ["R2"], "later item of the signature")),
            answer(kb))
        v = self.read(out, res)
        sigs = [(AGENT, "specs/a.md", "R2", "ambiguity"), (AGENT, "specs/a.md", "R1", "contradiction"),
                (AGENT, "specs/a.md", "file", "gap")]
        keys = [self.fkey(*s) for s in sigs]
        self.assertEqual(v, {"status": "ok", "lines": 2, "read": 2, "refused": 0, "errored": 0, "unreadable": 0,
                             "stale": 0, "duplicate": 0, "skipped": 0, "items": 3, "opened": keys, "seen": [],
                             "left": 0, "over_cap": 0, "already_applied": 0, "failed": []})
        found = self.state()["findings"]
        self.assertEqual(sorted(found), sorted(keys))
        for k, (repository, path, anchor, kind) in zip(keys, sigs):
            f = found[k]
            signature = "%s:%s#%s:%s" % (repository, path, anchor, kind)
            self.assertEqual((f["kind"], f["detector"], f["severity"], f["state"]), ("note", "batch:spec-lint", "info",
                                                                                        "open"))
            self.assertEqual(f["summary"], "%s: the weekly reading of the specifications found this %s"
                             % (signature, kind))
            self.assertTrue(f["evidence"].startswith("key %s commit %s; rules " % (ka, r.head())))
        self.assertEqual(found[keys[0]]["evidence"],
                         "key %s commit %s; rules R2,R1; quote: quote one; why: w" % (ka, r.head()))
        ev = found[keys[1]]["evidence"]
        self.assertTrue(ev.startswith("key %s commit %s; rules R9,R1; quote: tab here line end; why: www" % (ka, r.head())))
        self.assertFalse(any(ord(c) < 32 for c in ev))
        self.assertLessEqual(len(ev.encode("utf-8")), 2000)
        self.assertGreater(len(ev.encode("utf-8")), 1990)
        ev.encode("utf-8").decode("utf-8")
        self.assertEqual(found[keys[2]]["evidence"], "key %s commit %s; rules R9; quote: no such rule; why: w"
                         % (ka, r.head()))
        # The request id enters the file's key; a second read applies nothing.
        before = len(self.applies())
        self.assertEqual(before, 3)
        v2 = self.read(out, res)
        self.assertEqual((v2["opened"], v2["seen"], v2["left"], v2["over_cap"], v2["already_applied"], v2["items"]),
                         ([], [], 0, 0, 0, 3))
        self.assertEqual(len(self.applies()), before)
        # An answer of another evidence sees an open finding again, once; the key stays.
        res2 = self.results(answer(ka, item("ambiguity", ["R2"], "a changed quote")))
        v3 = self.read(out, res2)
        self.assertEqual((v3["opened"], v3["seen"], v3["items"]), ([], [keys[0]], 1))
        self.assertIn("a changed quote", self.state()["findings"][keys[0]]["evidence"])
        v4 = self.read(out, res2)
        self.assertEqual((v4["seen"], v4["already_applied"]), ([], 0))
        # Two files of one repository may carry the same rule and kind: signatures differ by path.
        res3 = self.results(answer(ka, item("gap", ["R1"])), answer(kb, item("gap", ["R7", "R1"])))
        v5 = self.read(out, res3)
        self.assertEqual(sorted(v5["opened"]), sorted([self.fkey(AGENT, "specs/a.md", "R1", "gap"),
                                                      self.fkey(AGENT, "specs/b.md", "R7", "gap")]))

    def test_cap_and_states(self):
        r, doc, out = self.scenario({"specs/a.md": spec(1, 2, 3, 4, 5)})
        ka = doc["files"][0]["key"]
        items = [item(kind, [rule]) for kind, rules in (("ambiguity", range(1, 6)), ("contradiction", range(1, 6)),
                                                          ("gap", range(1, 4))) for rule in ["R%d" % n for n in rules]]
        self.assertEqual(len(items), 13)
        res = self.results(answer(ka, *items))
        keys = [self.fkey(AGENT, "specs/a.md", i["rules"][0], i["kind"]) for i in items]
        # A finding of another detector is not read for closing or changed.
        other = {"kind": "drift", "detector": "batch:drift", "summary": "s", "evidence": "e", "severity": "info",
                 "state": "open"}
        self.put(findings={"batch-drift-0000000000000000": dict(other)})
        v = self.read(out, res, extra=["--store", self.tmp / "store"])
        self.assertEqual((v["items"], v["opened"], v["over_cap"]), (13, keys[:10], 3))
        self.assertEqual(len([e for e in self.applies() if e["body"]["action_type"] == "open_finding"]), 10)
        self.assertTrue(all(e["argv"][:2] == ["--store", str(self.tmp / "store")] for e in self.state()["log"]))
        # The next call opens the next three.
        v = self.read(out, res)
        self.assertEqual((v["opened"], v["over_cap"], v["seen"]), (keys[10:], 0, []))
        # A closed or dismissed finding is left, whatever the evidence; an open one with other evidence is seen again.
        st = self.state()
        st["findings"][keys[0]]["state"] = "closed"
        st["findings"][keys[1]]["state"] = "dismissed"
        st["findings"][keys[2]]["state"] = "planned"
        st["findings"][keys[2]]["evidence"] = "old"
        st["findings"][keys[3]]["evidence"] = "old"
        (self.tmp / "ont-state.json").write_text(json.dumps(st))
        before = len(self.applies())
        v = self.read(out, res)
        self.assertEqual((v["left"], v["opened"], v["over_cap"], v["seen"]), (2, [], 0, [keys[2], keys[3]]))
        self.assertEqual([e["body"]["action_type"] for e in self.applies()[before:]], ["see_finding_again"] * 2)
        after = self.state()["findings"]
        self.assertEqual(after[keys[0]]["state"], "closed")
        self.assertEqual(after[keys[1]]["state"], "dismissed")
        self.assertEqual(after["batch-drift-0000000000000000"], other)
        self.assertTrue(all(k.startswith("batch-spec-lint-") for k in after if k != "batch-drift-0000000000000000"))

    def test_refusal_and_error_open_nothing(self):
        r, doc, out = self.scenario({"specs/f%d.md" % i: spec(1) for i in range(9)})
        k = [f["key"] for f in doc["files"]]
        good = json.dumps({"items": [item("gap", ["R1"])]})
        wrong = json.dumps({"items": [dict(item("gap", ["R1"]), extra=1)]})
        wrong_kind = json.dumps({"items": [item("wording", ["R1"])]})
        lines = [succeeded(k[0], good, "refusal"),                  # refused
                 succeeded(k[1], good, "max_tokens"),               # cut
                 failed_line(k[2]),                                 # errored
                 failed_line(k[3], "expired"),                      # errored
                 failed_line(k[4], "canceled"),                     # errored
                 succeeded(k[5], "{not json"),                      # unparsable
                 succeeded(k[6], wrong),                            # not of the schema
                 succeeded(k[7], wrong_kind),                       # not of the schema
                 succeeded("f" * 64, good),                         # stale
                 "garbage",                                         # skipped
                 json.dumps({"custom_id": k[8], "result": {"type": "succeeded"}}),  # skipped: no message
                 succeeded(k[0], good)]                             # duplicate: the refusal is the line counted
        v = self.read(out, self.results(*lines))
        self.assertEqual({w: v[w] for w in COUNT_WORDS}, {"read": 0, "refused": 1, "errored": 3, "unreadable": 4,
                                                          "stale": 1, "duplicate": 1, "skipped": 2})
        self.assertEqual(v["lines"], 12)
        self.assertEqual((v["items"], v["opened"], v["seen"], v["left"], v["failed"]), (0, [], [], 0, []))
        # Nothing was opened and the engine was not called at all.
        self.assertEqual(self.state()["log"], [])
        v = self.run_prog("read", ["--files", Path(out) / "files.json", "--results", self.results(*lines)],
                          env=self.env(ONT_BIN="/nonexistent"))
        self.assertEqual((v["items"], v["opened"]), (0, []))
        # Of the lines of one key the first that is succeeded is the one counted: an error before it is a duplicate.
        v = self.read(out, self.results(*lines[:11], failed_line(k[8]), succeeded(k[8], good)))
        self.assertEqual({w: v[w] for w in COUNT_WORDS}, {"read": 1, "refused": 1, "errored": 3, "unreadable": 4,
                                                          "stale": 1, "duplicate": 1, "skipped": 2})
        self.assertEqual(v["opened"], [self.fkey(AGENT, "specs/f8.md", "R1", "gap")])
        self.assertEqual(len(self.applies()), 1)

    def test_unreadable_inputs_and_failures(self):
        r, doc, out = self.scenario()
        ka = doc["files"][0]["key"]
        res = self.results(answer(ka, item("gap", ["R1"])))
        bad = self.path("bad.json")
        sample = json.loads((out / "files.json").read_text())
        mutations = [lambda d: d.update(job="drift"), lambda d: d.pop("model"), lambda d: d.update(extra=1),
                     lambda d: d["files"][0].update(state="done"), lambda d: d["files"][0].update(rules=["x"]),
                     lambda d: d["files"][0].update(path="docs/a.md"), lambda d: d["files"][0].pop("commit"),
                     lambda d: d["files"][1].update(key=d["files"][0]["key"]),
                     lambda d: d["files"][1].update(commit="a" * 40),
                     lambda d: d["files"][0].update(rules=["R1", "R1"])]
        for mutate in mutations:
            doc2 = json.loads(json.dumps(sample))
            mutate(doc2)
            bad.write_text(json.dumps(doc2))
            self.assertEqual(self.run_prog("read", ["--files", bad, "--results", res], env=self.env(), code=2),
                             {"status": "files_unreadable"})
        bad.write_text("not json")
        self.assertEqual(self.run_prog("read", ["--files", bad, "--results", res], env=self.env(), code=2),
                         {"status": "files_unreadable"})
        self.assertEqual(self.run_prog("read", ["--files", self.tmp / "missing", "--results", res], env=self.env(),
                                       code=2), {"status": "files_unreadable"})
        self.assertEqual(self.run_prog("read", ["--files", out / "files.json", "--results", self.tmp / "missing"],
                                       env=self.env(), code=2), {"status": "results_unreadable"})
        for args in ([], ["--files", "x"], ["--files", "x", "--results", "y", "--store", ""], ["--bogus"]):
            self.assertEqual(self.run_prog("read", args, env=self.env(), code=2), {"status": "usage"})
        # A files.json that names a path of the Academy is refused before any action.
        academy = json.loads(json.dumps(sample))
        academy["files"][0]["path"] = "specs/evals/" + "lesson/a.md"
        bad.write_text(json.dumps(academy))
        self.assertEqual(self.run_prog("read", ["--files", bad, "--results", res], env=self.env(), code=2),
                         {"status": "academy_path"})
        self.assertEqual(self.applies(), [])
        # A store that cannot be searched is store_failed, exit 3, before any action.
        v = self.run_prog("read", ["--files", out / "files.json", "--results", res], env=self.env(ONT_BIN="/nonexistent"),
                          code=3)
        self.assertEqual(v, {"status": "store_failed"})
        self.put(finding_unqualified=True)
        self.assertEqual(self.read(out, res, code=3), {"status": "store_failed"})
        self.assertEqual(self.applies(), [])
        # A refused apply is listed in failed, the call exits 1 and the pass goes on.
        fake = (self.bin / "ont").read_text()
        marker = '    rid, a, p = body["request_id"]'
        self.assertEqual(fake.count(marker), 1)
        (self.bin / "ont").write_text(fake.replace(marker, '    if st.get("refuse_apply"):\n        crit("refused_here")\n' + marker))
        self.put(finding_unqualified=False, refuse_apply=True)
        v = self.read(out, res, code=1)
        self.assertEqual((v["opened"], v["failed"]), ([], [{"key": self.fkey(AGENT, "specs/a.md", "R1", "gap"),
                                                           "code": "refused_here"}]))

    def test_academy_path_refused(self):
        edu = str(self.tmp / ("ed" + "u-x") / "f")
        lesson = str(self.tmp / "evals" / "lesson" / "f")
        education = str(self.tmp / ("education-" + "platform") / "x")
        before = sorted(p.name for p in self.tmp.iterdir())
        host = str(self.tmp / "h")
        base = {"build": {"--repo": "o/n=" + host, "--model": "m", "--out": host + "o"},
                "read": {"--files": host, "--results": host}}
        options = {"build": ["--repo", "--out", "--results"], "read": ["--files", "--results", "--store"]}
        env = {"PATH": PATH, "HOME": HOME, "ONT_BIN": "/nonexistent"}
        for prog, opts in options.items():
            for opt in opts:
                for bad in (edu, lesson, education):
                    args = [x for k, v in base[prog].items() for x in (k, v)]
                    value = "o/n=" + bad if opt == "--repo" else bad
                    if opt in base[prog]:
                        args[args.index(opt) + 1] = value
                    else:
                        args += [opt, value]
                    v = self.run_prog(prog, args, env=env, code=2)
                    self.assertEqual(v, {"status": "academy_path"}, (prog, opt, bad))
        # The name of a repository is asked too, and a path below a link to the Academy is not followed.
        v = self.run_prog("build", ["--repo", ("ed" + "u-x") + "/n=" + host, "--model", "m", "--out", host + "o"],
                          env=env, code=2)
        self.assertEqual(v, {"status": "academy_path"})
        self.assertEqual(sorted(p.name for p in self.tmp.iterdir()), before)

    def test_actions_fit_the_schema(self):
        r, doc, out = self.scenario({"specs/a.md": spec(1, 2)})
        ka = doc["files"][0]["key"]
        res = self.results(answer(ka, item("ambiguity", ["R1"], "q"), item("gap", ["R2"], "q")))
        self.read(out, res)
        res2 = self.results(answer(ka, item("ambiguity", ["R1"], "changed"), item("gap", ["R2"], "q")))
        self.read(out, res2)
        schema = json.loads((REPO / "ontology" / "work.schema.json").read_text())
        host = (REPO / "crates" / "ap" / "src" / "host.rs").read_text()
        m = re.search(r'fn id\(kind: &str, key: &str\) -> Value \{\s*json!\(\{"object_type":kind,"namespace":(\{[^}]*\})',
                      host)
        self.assertIsNotNone(m)
        namespace = json.loads(m.group(1))
        types = schema["value_types"]
        seen = set()
        applies = self.applies()
        self.assertEqual(len(applies), 3)
        for e in applies:
            body = e["body"]
            self.assertEqual(set(body), {"action_type", "parameters", "agent", "request_id", "role"})
            self.assertEqual((body["role"], body["agent"]), ("api", "batch-spec-lint"))
            self.assertEqual(e["argv"][-4:], ["action", "apply", "--client", "batch-spec-lint"])
            self.assertIsNone(e["role"])
            self.assertRegex(body["request_id"], r"^bl-[0-9a-f]{40}$")
            a = body["action_type"]
            seen.add(a)
            declared = schema["action_types"][a]["parameters"]
            p = body["parameters"]
            self.assertLessEqual(set(p), set(declared))
            self.assertLessEqual({k for k, d in declared.items() if d["required"]}, set(p))
            for name, value in p.items():
                t = declared[name]["type"]
                if isinstance(t, dict) and "ref" in t:
                    self.assertEqual(value, {"object_type": t["ref"], "namespace": namespace,
                                             "native_key": value["native_key"]})
                    self.assertRegex(value["native_key"], r"^batch-spec-lint-[0-9a-f]{16}$")
                elif isinstance(t, dict) and "value_type" in t:
                    self.assertIn(value, types[t["value_type"]]["enum"])
                else:
                    self.assertEqual(t, "string")
                    self.assertTrue(value)
                    self.assertLessEqual(len(value.encode("utf-8")), declared[name]["max_bytes"])
            self.assertEqual(body["request_id"], "bl-" + sha(canon([a, p["finding"]["native_key"], ka, p]))[:40])
            if a == "open_finding":
                self.assertEqual((p["kind"], p["detector"], p["severity"]), ("note", "batch:spec-lint", "info"))
        self.assertEqual(seen, {"open_finding", "see_finding_again"})
        self.assertEqual(self.read(out, res2)["opened"], [])
        self.assertEqual(len(self.applies()), 3)


class Texts(Case):
    def test_schema_and_prompt(self):
        schema = json.loads((LINT / "verdict.schema.json").read_text())
        self.assertEqual(schema["type"], "object")
        self.assertEqual(set(schema["properties"]), {"items"})
        self.assertEqual(schema["required"], ["items"])
        self.assertIs(schema["additionalProperties"], False)
        items = schema["properties"]["items"]
        self.assertEqual(items["type"], "array")
        one = items["items"]
        self.assertEqual(one["type"], "object")
        self.assertEqual(set(one["properties"]), {"kind", "rules", "quote", "why"})
        self.assertEqual(set(one["required"]), set(one["properties"]))
        self.assertIs(one["additionalProperties"], False)
        self.assertEqual(one["properties"]["kind"], {"type": "string", "enum": KINDS})
        self.assertEqual(one["properties"]["rules"], {"type": "array", "items": {"type": "string"}})
        self.assertEqual(one["properties"]["quote"], {"type": "string"})
        self.assertEqual(one["properties"]["why"], {"type": "string"})
        prompt = (LINT / "prompt.md").read_text()
        for word in KINDS + ["verbatim", "one sentence", "rules", "quote", "why", "wording", "two readings",
                             "cannot both hold", "without an answer"]:
            self.assertIn(word, prompt)
        for word in ("/home/", "/var/", "edu-", "education", "lesson"):
            self.assertNotIn(word, prompt)
        for name in ("build", "read"):
            p = LINT / name
            self.assertEqual(p.read_text().splitlines()[0], "#!/usr/bin/env -S python3 -I -B")
            self.assertEqual(stat.S_IMODE(p.stat().st_mode), 0o755)
        self.assertTrue((LINT / "README.md").is_file())
        m = json.loads((REPO / "map.json").read_text())
        component = next(c for c in m["components"] if c["name"] == "evals")
        self.assertIn("ci/test_batch_lint.py", component["tests"])

    def test_the_schema_reading_of_lib_accepts_and_refuses_items(self):
        lib = load("batch_lib_probe", REPO / "evals" / "batch" / "lib.py")
        schema = json.loads((LINT / "verdict.schema.json").read_text())
        self.assertTrue(lib.valid({"items": []}, schema))
        self.assertTrue(lib.valid({"items": [item("gap", [])]}, schema))
        for bad in ({}, {"items": [item("wording", [])]}, {"items": [dict(item("gap", []), more=1)]},
                    {"items": [{"kind": "gap", "rules": [], "quote": "q"}]}, {"items": [item("gap", [1])]},
                    {"items": [], "more": 1}, {"items": {}}):
            self.assertFalse(lib.valid(bad, schema), bad)

    def test_sources_hold_no_network_and_no_host_path(self):
        net = "|".join(["url" + "lib", "ht" + "tp", "sock" + "et", "s" + "sl", "async" + "io", "requ" + "ests", "ht" + "tpx",
                        "anthro" + "pic"])
        bad = [r"^\s*(import|from)\s+(%s)" % net, "api" + r"\." + "anthro" + "pic" + r"\.com", "/home/" + "palantirkc",
               "/var/" + "lib", "cu" + "rl", r"\bap\b +(api|run|unit)"]
        for f in LINT.iterdir():
            if f.is_dir() or f.suffix == ".md":
                continue
            text = f.read_text()
            for pat in bad:
                self.assertIsNone(re.search(pat, text, re.M), (f.name, pat))
        for name in ("build", "read", "common.py"):
            text = (LINT / name).read_text()
            for m in re.finditer(r"^\s*(?:import|from)\s+([A-Za-z_][A-Za-z0-9_.]*)", text, re.M):
                self.assertIn(m.group(1).split(".")[0], sys.stdlib_module_names, (name, m.group(1)))
        # No program starts anything but git (build) and the engine (read), and neither copies a function of lib.
        self.assertNotIn("subprocess", (LINT / "read").read_text())
        self.assertNotIn("subprocess", (LINT / "build").read_text())
        lib = (REPO / "evals" / "batch" / "lib.py").read_text()
        for name in re.findall(r"^def ([a-z_]+)\(", lib, re.M):
            for f in LINT.iterdir():
                if f.is_file() and f.suffix != ".md":
                    self.assertIsNone(re.search(r"^\s*def %s\(" % name, f.read_text(), re.M), (f.name, name))
        # This test makes no directory of the Academy and holds no literal requirement line or citation.
        mine = Path(__file__).read_text()
        self.assertIsNone(re.search(r"mkdir\([^)]*(edu-|education-)", mine))
        self.assertIsNone(re.search(r"^\s*[\"']?- R[0-9]+:", mine, re.M))
        self.assertIsNone(re.search(r"spec: specs/[^\s#]+\.md#R[0-9]+", mine))

    def test_the_real_repository_gives_one_request_per_spec_file(self):
        # The page's second acceptance check, on a clone made here: every tracked specs/**/*.md of the repository.
        r = self.repo({"specs/x/one.md": spec(1), "specs/two.md": spec(1, 2, 3)})
        v, doc, reqs, out = self.build([(AGENT, r)])
        listed = [p for p in r.git("ls-files", "--", "specs/*.md").split("\n") if p]
        self.assertEqual(len(doc["files"]), len(reqs))
        self.assertEqual(len(doc["files"]), len(listed))
        self.assertEqual([x["custom_id"] for x in reqs], [f["key"] for f in doc["files"]])


if __name__ == "__main__":
    unittest.main()
