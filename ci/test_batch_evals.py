"""Tests of evals/batch/evals/build, blind and score, the evals job of the batch evaluations, run as programs against a
temporary copy of evals/batch, evals/routing and evals/lead, scratch repositories made by `git init` and a fake `ont`
that keeps its objects in a file (no model, no network, no engine, no store of the host, no cargo). Contract:
evals/batch/evals/README.md."""
import hashlib
import importlib.machinery
import importlib.util
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PY = sys.executable
HOME = os.environ.get("HOME", "/")
PATH = os.environ["PATH"]
COUNT_WORDS = ("read", "refused", "errored", "unreadable", "stale", "duplicate", "skipped")
GIT_ID = ["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "-c", "commit.gpgsign=false",
          "-c", "init.defaultBranch=main"]
CACHE = {"type": "ephemeral", "ttl": "1h"}


def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    mod = importlib.util.module_from_spec(importlib.util.spec_from_loader(name, loader))
    loader.exec_module(mod)
    return mod


def sha(text):
    return hashlib.sha256(text if isinstance(text, bytes) else text.encode("utf-8")).hexdigest()


def canon(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def sample_key(params, s):
    return sha(canon([params, s]))


def succeeded(cid, text, stop="end_turn"):
    message = {"id": "msg_1", "type": "message", "role": "assistant", "model": "m", "stop_reason": stop,
               "content": [{"type": "text", "text": text}], "usage": {"input_tokens": 1, "output_tokens": 1}}
    return json.dumps({"custom_id": cid, "result": {"type": "succeeded", "message": message}})


def scored(cid, result, quote="q", clause="c"):
    return succeeded(cid, json.dumps({"result": result, "quote": quote, "clause": clause}))


def failed_line(cid, kind="errored"):
    result = {"type": kind}
    if kind == "errored":
        result["error"] = {"type": "error", "error": {"type": "api_error", "message": "x"}}
    return json.dumps({"custom_id": cid, "result": result})


# The fake engine: tasks, kpis and findings in ont-state.json beside its directory; every call recorded with the role
# of its environment; request ids kept as the engine keeps them; the submission criteria of the five actions.
FAKE_ONT = r'''#!@PY@ -I
import json, os, sys
STATE = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "ont-state.json")
NS = {"instance": "vps://palantirkc", "account": "root"}
try:
    with open(STATE) as f:
        st = json.load(f)
except FileNotFoundError:
    st = {}
for k, v in (("tasks", {}), ("kpis", {}), ("findings", {}), ("rids", {}), ("log", []), ("n", 0)):
    st.setdefault(k, v)
args = sys.argv[1:]
raw = sys.stdin.read()
body = json.loads(raw) if raw else None
st["log"].append({"argv": list(args), "body": body, "role": os.environ.get("ONT_ROLE"), "agent": os.environ.get("ONT_AGENT")})
def out(v, code=0):
    with open(STATE + ".tmp", "w") as f:
        json.dump(st, f)
    os.replace(STATE + ".tmp", STATE)
    print(json.dumps(v))
    sys.exit(code)
def take(flag):
    if flag in args:
        i = args.index(flag)
        v = args[i + 1]
        del args[i:i + 2]
        return v
store, client = take("--store"), take("--client")
def ident(kind, key):
    return {"object_type": kind, "namespace": NS, "native_key": key}
def fields_of(d):
    return {k: ({"kind": "absent"} if v is None else {"kind": "value", "value": v}) for k, v in d.items()}
def page(rows, qualified):
    start = int(body.get("continuation") or 0)
    size = min(int(body.get("limit", 15)), 4)
    chunk = rows[start:start + size]
    nxt = str(start + size) if start + size < len(rows) else None
    objs = [{"id": i, "head": {"fields": fields_of(f)}} for i, f in chunk]
    out({"status": "ok", "result": {"objects": objs, "basis": {"qualified": qualified}, "continuation": nxt}})
def crit(c):
    out({"status": "refused", "code": "submission_criteria", "failed": [c]}, 2)
if args == ["search"]:
    t = body["object_type"]
    eqs = [c["eq"] for c in body.get("where", {}).get("all", [])]
    if t == "task":
        page([(ident("task", k), v) for k, v in sorted(st["tasks"].items())], not st.get("task_unqualified"))
    if t == "kpi":
        rows = []
        for k, v in sorted(st["kpis"].items()):
            if all(v.get(p) == x for p, x in eqs):
                rows.append((ident("kpi", k), dict(v, task=ident("task", v["task"]))))
        page(rows, not st.get("kpi_unqualified"))
    if t == "finding":
        rows = [(ident("finding", k), f) for k, f in sorted(st["findings"].items()) if all(f.get(p) == x for p, x in eqs)]
        page(rows, not st.get("finding_unqualified"))
    out({"status": "refused", "code": "usage"}, 2)
if args == ["action", "apply"]:
    if st.get("require_role") and not os.environ.get("ONT_ROLE"):
        out({"status": "refused", "code": "not_granted"}, 2)
    rid, a, p = body["request_id"], body["action_type"], body["parameters"]
    content = json.dumps([a, p, body.get("agent"), client], sort_keys=True)
    if rid in st["rids"]:
        if st["rids"][rid] == content:
            out({"status": "ok", "result": {"outcome": "already_applied", "edits": []}})
        out({"status": "refused", "code": "request_id_reused"}, 2)
    edits = []
    if a == "define_kpi":
        tk = p["task"].get("native_key")
        if p["task"] != ident("task", tk) or tk not in st["tasks"]:
            crit("unknown_task")
        if any(v["name"] == p["name"] and v["task"] == tk for v in st["kpis"].values()):
            crit("duplicate_kpi")
        st["n"] += 1
        nk = "kpi-%d" % st["n"]
        st["kpis"][nk] = {"name": p["name"], "task": tk, "definition": p["definition"], "value": None,
                          "evidence": None, "observed_at": None}
        edits = [{"change": "initial", "id": ident("kpi", nk)}]
    elif a == "record_measurement":
        nk = p["kpi"].get("native_key")
        if p["kpi"] != ident("kpi", nk) or nk not in st["kpis"]:
            crit("unknown_kpi")
        if st.get("floor") and p["observed_at"] < st["floor"]:
            crit("older_measurement")
        st["kpis"][nk].update(value=p["value"], evidence=p["evidence"], observed_at=p["observed_at"])
        edits = [{"change": "initial", "id": ident("measurement", "m-" + rid)}, {"change": "modify", "id": p["kpi"]}]
    else:
        key = p["finding"]["native_key"]
        f = st["findings"].get(key)
        state = f["state"] if f else None
        if a == "open_finding":
            if f:
                out({"status": "refused", "code": "revision_conflict"}, 2)
            f = {k: p[k] for k in ("kind", "detector", "summary", "evidence", "severity") if k in p}
            f["state"] = "open"
        elif a == "see_finding_again":
            if state not in ("open", "planned"):
                crit("finding_not_live")
            f["evidence"] = p.get("evidence")
        else:
            out({"status": "refused", "code": "unknown_action"}, 2)
        st["findings"][key] = f
        edits = [{"change": "initial" if a == "open_finding" else "modify", "id": p["finding"]}]
    st["rids"][rid] = content
    out({"status": "ok", "result": {"outcome": "applied", "edits": edits}})
out({"status": "refused", "code": "usage"}, 2)
'''


class Repo:
    """A scratch repository of content/ files and the SHA256SUMS that lists them."""

    def __init__(self, root, files):
        self.root = root
        root.mkdir(parents=True)
        self.git("init", "-q")
        self.commit(files)

    def git(self, *args):
        return subprocess.run(["git", *GIT_ID, "-C", str(self.root), *args], check=True, capture_output=True,
                              text=True, env={"PATH": PATH, "HOME": HOME}).stdout

    def commit(self, files, sums=True):
        for rel, text in files.items():
            p = self.root / "content" / rel
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text)
        if sums:
            rows = sorted(str(p.relative_to(self.root / "content")) for p in (self.root / "content").rglob("*")
                          if p.is_file() and p.name != "SHA256SUMS")
            listed = "".join("%s  %s\n" % (sha((self.root / "content" / r).read_bytes()), r) for r in rows)
            (self.root / "content" / "SHA256SUMS").write_text(listed)
        self.git("add", "-A")
        self.git("commit", "-q", "--allow-empty", "-m", "c")
        self.git("update-ref", "refs/remotes/origin/main", "HEAD")
        return self.head()

    def head(self):
        return self.git("rev-parse", "HEAD").strip()

    def sums(self):
        return sha((self.root / "content" / "SHA256SUMS").read_bytes())


CONTENT = {"AGENTS.md": "# Shared\nStart here.\n", "skills/lead/SKILL.md": "Lead skill\nline two\n",
           "runtime-references/GLOSSARY.md": "Glossary \u00e9 text\n"}


def expected_system(files):
    """The instruction text the page defines: AGENTS.md first, then the others by path."""
    rest = sorted(p for p in files if p != "AGENTS.md")
    return "".join("=== %s ===\n%s" % (p, files[p]) for p in ["AGENTS.md"] + rest)


def routing_cases(n):
    return [{"id": "r%02d" % i, "situation": "Situation %d about topic-%d." % (i, i),
             "must": [{"fact": ["fact-%d" % i]}], "wrong": []} for i in range(1, n + 1)]


class Case(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.tmp = Path(tmp.name)
        self.n = 0
        self.bin = self.tmp / "bin"
        self.bin.mkdir()
        (self.bin / "ont").write_text(FAKE_ONT.replace("@PY@", PY))
        (self.bin / "ont").chmod(0o755)
        # The site: the three directories the programs reach, with cases of this test's own.
        self.site = self.tmp / "site"
        for d in ("batch", "routing", "lead"):
            shutil.copytree(REPO / "evals" / d, self.site / "evals" / d, ignore=shutil.ignore_patterns("__pycache__"))
        self.set_routing(routing_cases(8))
        self.lead = load("lead_run", self.site / "evals" / "lead" / "run")
        self.routing = load("routing_run", self.site / "evals" / "routing" / "run")
        self.clone = Repo(self.path("clone"), CONTENT)

    def path(self, name):
        self.n += 1
        return self.tmp / ("%d-%s" % (self.n, name))

    def prog(self, name):
        return self.site / "evals" / "batch" / "evals" / name

    def set_routing(self, cases):
        (self.site / "evals" / "routing" / "cases.json").write_text(json.dumps(cases))
        self.cases = cases

    def make_lead(self, cases):
        """cases: id -> {"prompt", "expected", "task": {relative path: bytes}}; the cases directory is returned and the
        site's cases.sha256 made for it."""
        root = self.path("leadcases")
        root.mkdir()
        rows = []
        for cid, c in cases.items():
            d = root / cid
            d.mkdir()
            (d / "PROMPT.txt").write_text(c["prompt"])
            (d / "EXPECTED.md").write_text(c["expected"])
            for rel, data in c.get("task", {}).items():
                p = d / "TASK" / rel
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_bytes(data)
            rows.append("%s  %s\n" % (self.lead.tree_digest(str(d)), cid))
        (self.site / "evals" / "lead" / "cases.sha256").write_text("".join(rows))
        return root

    def run_prog(self, name, args, env=None, code=0, stderr_empty=True):
        env = env if env is not None else {"PATH": PATH, "HOME": HOME}
        p = subprocess.run([PY, "-I", "-B", str(self.prog(name)), *map(str, args)], capture_output=True, text=True,
                           env=env, cwd=str(self.tmp))
        if stderr_empty:
            self.assertEqual(p.stderr, "")
        lines = p.stdout.splitlines()
        self.assertEqual(len(lines), 1, p.stdout)
        self.assertEqual(p.returncode, code, lines[0])
        return json.loads(lines[0])

    def build(self, suite="routing", model="m1", extra=(), out=None, clone=None, code=0, rev="HEAD"):
        out = out or self.path("build")
        args = ["--suite", suite, "--clone", (clone or self.clone).root, "--model", model, "--out", out, *extra]
        if rev is not None:
            args += ["--rev", rev]
        v = self.run_prog("build", args, code=code)
        if code:
            return v
        return v, self.items(out), self.requests(out), Path(out)

    @staticmethod
    def items(out):
        return json.loads((Path(out) / "items.json").read_text())

    @staticmethod
    def requests(out):
        return [json.loads(x) for x in (Path(out) / "requests.jsonl").read_text().splitlines()]

    def results(self, *lines):
        p = self.path("results.jsonl")
        p.write_text("".join(x + "\n" for x in lines))
        return p

    def pass_lines(self, items, counts, texts=("fact-%d", "nothing")):
        """Result lines for a routing pass: case i passes in its first counts[i-1] samples."""
        lines = []
        for it in items["items"]:
            i = int(it["case"][1:])
            text = texts[0] % i if it["sample"] <= counts[i - 1] else texts[1]
            lines.append(succeeded(it["key"], text))
        return lines

    def routing_pass(self, counts, model="m1", clone=None, extra=()):
        v, items, reqs, out = self.build("routing", model, clone=clone, extra=extra)
        res = self.results(*self.pass_lines(items, counts))
        return items, out, res

    def score(self, items_dir, results, out=None, extra=(), env=None, code=0, cases=()):
        out = out or self.path("score")
        args = ["--items", Path(items_dir) / "items.json", "--results", results, "--out", out, *extra]
        for c in cases:
            args += ["--cases", c]
        v = self.run_prog("score", args, env=env, code=code)
        return v, Path(out)

    def gate(self, base, cand):
        p = subprocess.run([PY, "-I", "-B", str(self.site / "evals" / "routing" / "run"), "gate", "--baseline", str(base),
                            "--candidate", str(cand)], capture_output=True, text=True,
                           env={"PATH": PATH, "HOME": HOME})
        return p.returncode, json.loads(p.stdout)

    # The fake's state.
    def state(self):
        p = self.tmp / "ont-state.json"
        return json.loads(p.read_text()) if p.exists() else {"tasks": {}, "kpis": {}, "findings": {}, "rids": {}, "log": []}

    def put(self, **members):
        st = self.state()
        for k, v in members.items():
            if isinstance(v, dict) and isinstance(st.get(k), dict):
                st[k].update(v)
            else:
                st[k] = v
        (self.tmp / "ont-state.json").write_text(json.dumps(st))

    def applies(self):
        return [e for e in self.state()["log"] if e["argv"][-4:-1] == ["action", "apply", "--client"]]

    def env(self, role="lead", **more):
        env = {"PATH": str(self.bin), "HOME": HOME, "ONT_BIN": str(self.bin / "ont")}
        if role:
            env["ONT_ROLE"] = role
        env.update(more)
        return env

    def fkey(self, suite, reader, case, clone=None):
        sig = "evals:%s:%s:%s:%s" % (suite, reader, case, (clone or self.clone).sums()[:16])
        return "batch-evals-" + sha(sig)[:16]


class Builds(Case):
    def test_samples_are_three_requests(self):
        head = self.clone.head()
        v, items, reqs, out = self.build()
        text = expected_system(CONTENT)
        self.assertEqual(len(reqs), 24)
        self.assertEqual((v["status"], v["cases"], v["requested"], v["settled"], v["too_large"]), ("ok", 8, 24, 0, 0))
        self.assertEqual(v["system_bytes"], len(text.encode("utf-8")))
        self.assertEqual(v["result_lines_skipped"], 0)
        by_case = {}
        for r in reqs:
            by_case.setdefault(r["params"]["messages"][0]["content"], []).append(r)
        self.assertEqual(len(by_case), 8)
        for rows in by_case.values():
            self.assertEqual(len(rows), 3)
            self.assertEqual(len({canon(r["params"]) for r in rows}), 1)
            p = rows[0]["params"]
            self.assertEqual(set(p), {"model", "max_tokens", "system", "messages"})
            self.assertEqual((p["model"], p["max_tokens"]), ("m1", 4000))
            self.assertEqual(p["system"], [{"type": "text", "text": text, "cache_control": CACHE}])
            self.assertEqual([m["role"] for m in p["messages"]], ["user"])
            self.assertEqual({r["custom_id"] for r in rows}, {sample_key(p, s) for s in (1, 2, 3)})
            self.assertEqual(set(rows[0]), {"custom_id", "params"})
        # The items: by case, then sample; the key is the custom_id of the sample.
        self.assertEqual(items["job"], "evals")
        self.assertEqual((items["suite"], items["model"], items["samples"]), ("routing", "m1", 3))
        self.assertEqual(items["instructions"], {"commit": head, "sums": self.clone.sums()})
        self.assertEqual(items["case_files"], {"cases.json": sha((self.site / "evals/routing/cases.json").read_bytes())})
        self.assertEqual([(i["case"], i["sample"]) for i in items["items"]],
                         [("r%02d" % c, s) for c in range(1, 9) for s in (1, 2, 3)])
        self.assertEqual([i["key"] for i in items["items"]], [r["custom_id"] for r in reqs])
        self.assertTrue(all(i["state"] == "requested" for i in items["items"]))
        # Byte-equal files from the same inputs; mode 0600; the directory 0700.
        again = self.path("again")
        self.build(out=again)
        for name in ("requests.jsonl", "items.json"):
            self.assertEqual((out / name).read_bytes(), (again / name).read_bytes())
            self.assertEqual(stat.S_IMODE((out / name).stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(out.stat().st_mode), 0o700)

    def test_prompt_and_options(self):
        v, items, reqs, out = self.build(extra=["--samples", "4", "--max-tokens", "300"])
        self.assertEqual(len(reqs), 32)
        self.assertEqual({r["params"]["max_tokens"] for r in reqs}, {300})
        user = reqs[0]["params"]["messages"][0]["content"]
        self.assertIn("Situation 1 about topic-1.", user)
        self.assertNotIn("{situation}", user)
        for bad in (["--samples", "2"], ["--samples", "10"], ["--samples", "x"], ["--max-tokens", "255"],
                    ["--max-tokens", "8001"], ["--suite", "x"]):
            args = ["--suite", "routing", "--clone", self.clone.root, "--model", "m", "--out", self.path("o")]
            if bad[0] == "--suite":
                args[1] = "x"
            else:
                args += bad
            self.assertEqual(self.run_prog("build", args, code=2), {"status": "usage"})
        # The model is part of every key; so is max_tokens.
        other = self.build(model="m2")[2]
        self.assertTrue(set(r["custom_id"] for r in reqs).isdisjoint(r["custom_id"] for r in other))

    def test_instructions_come_from_git_objects(self):
        a = self.build()[3]
        (self.clone.root / "content" / "AGENTS.md").write_text("dirty checkout\n")
        (self.clone.root / "content" / "extra.md").write_text("untracked\n")
        b = self.build()[3]
        self.assertEqual(self.requests(a), self.requests(b))
        # origin/main is the default revision; a later commit changes the text and the instructions.
        old = self.clone.head()
        self.clone.git("checkout", "-q", "--", ".")
        (self.clone.root / "content" / "extra.md").unlink()
        new = self.clone.commit({"AGENTS.md": "# Shared\nChanged.\n"})
        self.assertNotEqual(old, new)
        v, items, reqs, out = self.build(rev=None)
        self.assertEqual(items["instructions"]["commit"], new)
        self.assertTrue(reqs[0]["params"]["system"][0]["text"].startswith("=== AGENTS.md ===\n# Shared\nChanged.\n"))
        self.assertEqual(self.build(rev=old)[1]["instructions"]["commit"], old)
        self.assertEqual(self.build(clone=self.clone, rev="no-such-rev", code=2), {"status": "repo_unreadable"})
        self.assertEqual(self.run_prog("build", ["--suite", "routing", "--clone", self.tmp / "none", "--model", "m",
                                                 "--out", self.path("o")], code=2), {"status": "repo_unreadable"})

    def test_sums_that_differ_are_refused(self):
        bad = Repo(self.path("bad"), CONTENT)
        (bad.root / "content" / "skills" / "lead" / "SKILL.md").write_text("changed after the list\n")
        bad.git("add", "-A")
        bad.git("commit", "-q", "-m", "x")
        out = self.path("o")
        v = self.build(clone=bad, out=out, code=2)
        self.assertEqual(v, {"status": "content_sums_differ"})
        self.assertFalse(out.exists())
        # A file the list does not name is not in the text; one it names that is absent is no repository.
        extra = Repo(self.path("extra"), CONTENT)
        (extra.root / "content" / "unlisted.md").write_text("not listed\n")
        extra.git("add", "-A")
        extra.git("commit", "-q", "-m", "x")
        text = self.build(clone=extra)[2][0]["params"]["system"][0]["text"]
        self.assertNotIn("unlisted", text)
        gone = Repo(self.path("gone"), CONTENT)
        (gone.root / "content" / "AGENTS.md").unlink()
        gone.git("add", "-A")
        gone.git("commit", "-q", "-m", "x")
        self.assertEqual(self.build(clone=gone, code=2), {"status": "repo_unreadable"})

    def test_settled_results_are_not_requested_again(self):
        v, items, reqs, out = self.build()
        keys = [i["key"] for i in items["items"]]
        lines = [succeeded(k, "x") for k in keys[:20]] + [failed_line(keys[20]), succeeded(keys[21], "x", "refusal"),
                                                          "not json", ""]
        res = self.results(*lines)
        v2, items2, reqs2, out2 = self.build(extra=["--results", res])
        self.assertEqual((v2["requested"], v2["settled"], v2["result_lines_skipped"]), (3, 20 + 1, 1))
        self.assertEqual([r["custom_id"] for r in reqs2], keys[20:21] + keys[22:])
        self.assertEqual([i["state"] for i in items2["items"]], ["settled"] * 20 + ["requested", "settled", "requested",
                                                                                      "requested"])
        everything = self.results(*[succeeded(k, "x") for k in keys])
        v3, _, reqs3, out3 = self.build(extra=["--results", everything])
        self.assertEqual((v3["requested"], v3["settled"]), (0, 24))
        self.assertEqual((out3 / "requests.jsonl").read_bytes(), b"")
        self.assertEqual(self.run_prog("build", ["--suite", "routing", "--clone", self.clone.root, "--model", "m1",
                                                 "--out", self.path("o"), "--results", self.tmp / "missing"], code=2),
                         {"status": "results_unreadable"})

    def test_too_large_request_is_not_written(self):
        cases = routing_cases(3)
        cases[1]["situation"] = "x" * 300_001
        self.set_routing(cases)
        v, items, reqs, out = self.build()
        self.assertEqual((v["requested"], v["too_large"]), (6, 3))
        self.assertEqual({i["state"] for i in items["items"] if i["case"] == "r02"}, {"too_large"})
        self.assertEqual(len(reqs), 6)
        self.assertTrue(all("xxxx" not in r["params"]["messages"][0]["content"] for r in reqs))

    def test_engine_store_is_passed_on(self):
        items, out, res = self.routing_pass([3] * 8)
        self.put(tasks={"T": {}})
        self.score(out, res, extra=["--record", "T", "--store", self.tmp / "store"], env=self.env())
        calls = self.state()["log"]
        self.assertTrue(calls and all(c["argv"][:2] == ["--store", str(self.tmp / "store")] for c in calls))


class Passes(Case):
    A = [3, 3, 2, 2, 1, 1, 0, 0]
    B = [3, 3, 2, 2, 1, 0, 0, 0]

    def test_two_passes_inside_spread(self):
        items, out, res_a = self.routing_pass(self.A)
        v, sa = self.score(out, res_a)
        items_b, out_b, res_b = self.routing_pass(self.B)
        w, sb = self.score(out_b, res_b)
        a = json.loads((sa / "score.json").read_text())
        b = json.loads((sb / "score.json").read_text())
        ra, rb = a["readers"]["m1"], b["readers"]["m1"]
        self.assertEqual(a["job"], "evals")
        self.assertEqual(set(a), {"job", "suite", "instructions", "case_files", "items", "scorer", "readers"})
        self.assertEqual((a["suite"], a["scorer"], a["instructions"]), ("routing", None, items["instructions"]))
        self.assertEqual(a["items"], sha((out / "items.json").read_bytes()))
        self.assertEqual(set(ra), {"n", "passes", "samples", "mean", "majority", "unanswered", "spread"})
        self.assertEqual(ra["n"], 3)
        self.assertEqual(ra["passes"], {"r%02d" % (i + 1): c for i, c in enumerate(self.A)})
        self.assertEqual(ra["samples"], [6, 4, 2])
        self.assertEqual((ra["mean"], rb["mean"]), (4.0, 3.6667))
        self.assertEqual(ra["majority"], ["r01", "r02", "r03", "r04"])
        self.assertEqual((ra["unanswered"], ra["spread"]), (0, 1.5396))
        # The objective: the means differ by less than the recorded spread, and the routing gate passes.
        self.assertLessEqual(abs(ra["mean"] - rb["mean"]), ra["spread"])
        code, g = self.gate(sa / "score.json", sb / "score.json")
        self.assertEqual((code, g["status"]), (0, "pass"))
        code, g = self.gate(sb / "score.json", sa / "score.json")
        self.assertEqual((code, g["status"]), (0, "pass"))
        # The stdout of score and byte-equal files from equal inputs.
        self.assertEqual((v["status"], v["suite"], v["reader"], v["cases"], v["n"], v["mean"], v["spread"],
                          v["unanswered"], v["lost"]), ("ok", "routing", "m1", 8, 3, 4.0, 1.5396, 0, []))
        self.assertEqual(v["counts"], {"read": 24, "refused": 0, "errored": 0, "unreadable": 0, "stale": 0,
                                       "duplicate": 0, "skipped": 0})
        self.assertNotIn("recorded", v)
        _, again = self.score(out, res_a)
        self.assertEqual((again / "score.json").read_bytes(), (sa / "score.json").read_bytes())
        self.assertEqual(stat.S_IMODE((sa / "score.json").stat().st_mode), 0o600)
        self.assertEqual(sorted(p.name for p in sa.iterdir()), ["score.json"])
        # A real drop is judged by the same gate.
        _, out_c, res_c = self.routing_pass([0] * 8)
        _, sc = self.score(out_c, res_c)
        code, g = self.gate(sa / "score.json", sc / "score.json")
        self.assertEqual((code, g["status"]), (1, "fail"))

    def test_case_files_and_items_must_agree(self):
        items, out, res = self.routing_pass(self.A)
        self.set_routing(routing_cases(7))
        self.assertEqual(self.score(out, res, code=2)[0], {"status": "case_sets_differ"})
        self.set_routing(routing_cases(8))
        bad = self.path("badcases")
        bad.write_text("[]")
        self.assertEqual(self.score(out, res, cases=[bad], code=2)[0], {"status": "cases_digest_unknown"})
        broken = self.path("items")
        broken.mkdir()
        (broken / "items.json").write_text(json.dumps({"job": "evals"}))
        self.assertEqual(self.score(broken, res, code=2)[0], {"status": "items_unreadable"})
        (broken / "items.json").write_text("not json")
        self.assertEqual(self.score(broken, res, code=2)[0], {"status": "items_unreadable"})
        self.assertEqual(self.score(out, self.tmp / "missing", code=2)[0], {"status": "results_unreadable"})
        self.assertEqual(self.run_prog("score", ["--items", out / "items.json", "--results", res], code=2),
                         {"status": "usage"})

    def test_results_not_read_make_no_pass(self):
        self.set_routing(routing_cases(2))
        v, items, reqs, out = self.build()
        k = {(i["case"], i["sample"]): i["key"] for i in items["items"]}
        lines = [succeeded(k["r01", 1], "fact-1"),
                 succeeded(k["r01", 2], "fact-1", "refusal"),
                 failed_line(k["r01", 3]),
                 succeeded(k["r02", 1], "fact-2", "max_tokens"),
                 succeeded(k["r02", 2], "  "),
                 succeeded("0" * 64, "fact-1"),
                 succeeded(k["r01", 1], "fact-1"),
                 "junk"]
        v, score = self.score(out, self.results(*lines))
        self.assertEqual(v["counts"], {"read": 1, "refused": 1, "errored": 1, "unreadable": 2, "stale": 1,
                                       "duplicate": 1, "skipped": 1})
        r = json.loads((score / "score.json").read_text())["readers"]["m1"]
        self.assertEqual(r["passes"], {"r01": 1, "r02": 0})
        self.assertEqual((r["unanswered"], r["samples"], r["majority"]), (5, [1, 0, 0], []))
        # With the line of a key counted once: an error before a success is replaced by the success.
        v, _ = self.score(out, self.results(failed_line(k["r01", 1]), succeeded(k["r01", 1], "fact-1")))
        self.assertEqual((v["counts"]["read"], v["counts"]["errored"], v["counts"]["duplicate"]), (1, 0, 1))
        # No line gives an action: record nothing was asked and no engine was started.
        self.assertFalse((self.tmp / "ont-state.json").exists())

    def test_academy_path_refused(self):
        edu = str(self.tmp / ("ed" + "u-x") / "f")
        lesson = str(self.tmp / "evals" / "lesson" / "f")
        before = sorted(p.name for p in self.tmp.iterdir())
        host = str(self.tmp / "h")
        base = {
            "build": {"--suite": "routing", "--clone": host, "--model": "m", "--out": host + "o"},
            "blind": {"--items": host, "--results": host, "--cases": host, "--model": "m", "--out": host + "o"},
            "score": {"--items": host, "--results": host, "--out": host + "o"},
        }
        multi = {"build": ["--cases", "--results"], "score": ["--cases", "--scores"], "blind": []}
        options = {"build": ["--clone", "--out", "--cases", "--results"],
                   "blind": ["--items", "--results", "--cases", "--out"],
                   "score": ["--items", "--results", "--out", "--cases", "--blind", "--scores", "--baseline", "--store"]}
        for prog, opts in options.items():
            for opt in opts:
                for bad in (edu, lesson, str(self.tmp / ("education-" + "platform") / "x")):
                    args = [x for k, v in base[prog].items() for x in (k, v)]
                    if opt in base[prog]:
                        args[args.index(opt) + 1] = bad
                    else:
                        args += [opt, bad]
                    v = self.run_prog(prog, args, env={"PATH": PATH, "HOME": HOME, "ONT_BIN": "/nonexistent"}, code=2)
                    self.assertEqual(v, {"status": "academy_path"}, (prog, opt, bad))
        self.assertEqual(sorted(p.name for p in self.tmp.iterdir()), before)
        # A Lead case that holds a path of the Academy is refused before any file of the case is opened.
        cases = self.make_lead({"gate-a": {"prompt": "p @TASK@", "expected": "rubric", "task": {"evals/lesson/x.md": b"x"}}})
        (self.site / "evals" / "lead" / "cases.sha256").write_text("0" * 64 + "  gate-a\n")
        out = self.path("o")
        v = self.run_prog("build", ["--suite", "lead", "--clone", self.clone.root, "--model", "m", "--out", out,
                                    "--cases", cases], code=2)
        self.assertEqual(v, {"status": "academy_path"})
        self.assertFalse(out.exists())
        items = self.path("items")
        items.mkdir()
        (items / "items.json").write_text("{}")
        v = self.run_prog("blind", ["--items", items / "items.json", "--results", host, "--cases", cases, "--model", "m",
                                    "--out", out], code=2)
        self.assertEqual(v, {"status": "academy_path"})


class Lead(Case):
    def lead_cases(self):
        return self.make_lead({
            "gate-alpha": {"prompt": "CASEID alpha. Look in @TASK@ first.\n", "expected": "RUBRIC-SECRET alpha\n",
                           "task": {"notes.md": b"notes of alpha\n", "sub/b.txt": b"bytes \xff of b\n"}},
            "gate-beta": {"prompt": "CASEID beta.\n", "expected": "RUBRIC-SECRET beta\n", "task": {}},
            "gate-gamma": {"prompt": "CASEID gamma @TASK@\n", "expected": "RUBRIC-SECRET gamma\n",
                           "task": {"z.txt": b"z\n", "a.txt": b"a\n"}}})

    def lead_build(self, model="answerer-model", extra=()):
        cases = self.lead_cases()
        v, items, reqs, out = self.build("lead", model, extra=["--cases", cases, *extra])
        return cases, v, items, reqs, out

    def answer_lines(self, items):
        """alpha: three equal answers; beta: three different ones; gamma: one read, one error, one cut."""
        lines = []
        for it in items["items"]:
            c, s = it["case"], it["sample"]
            if c == "gate-alpha":
                lines.append(succeeded(it["key"], "same answer of alpha"))
            elif c == "gate-beta":
                lines.append(succeeded(it["key"], "beta answer %d" % s))
            elif s == 1:
                lines.append(succeeded(it["key"], "gamma answer"))
            elif s == 2:
                lines.append(failed_line(it["key"]))
            else:
                lines.append(succeeded(it["key"], "cut answer", "max_tokens"))
        return lines

    def test_lead_scored_blind(self):
        cases, v, items, reqs, out = self.lead_build()
        self.assertEqual((v["cases"], v["requested"]), (3, 9))
        self.assertEqual(items["case_files"], {"cases.sha256": sha((self.site / "evals/lead/cases.sha256").read_bytes())})
        raw = (out / "requests.jsonl").read_text() + (out / "items.json").read_text()
        self.assertNotIn("RUBRIC-SECRET", raw)
        self.assertNotIn("EXPECTED", raw)
        alpha = next(r for r in reqs if "CASEID alpha" in r["params"]["messages"][0]["content"])
        user = alpha["params"]["messages"][0]["content"]
        lead_md = (self.site / "evals/batch/evals/lead.md").read_text().rstrip("\n")
        self.assertTrue(user.startswith(lead_md + "\n\n"))
        self.assertIn("CASEID alpha. Look in task first.", user)
        self.assertNotIn("@TASK@", user)
        self.assertLess(user.index("=== task/notes.md ==="), user.index("=== task/sub/b.txt ==="))
        self.assertIn("=== task/notes.md ===\nnotes of alpha\n", user)
        self.assertIn("bytes \ufffd of b\n", user)
        gamma = next(r for r in reqs if "CASEID gamma" in r["params"]["messages"][0]["content"])
        g = gamma["params"]["messages"][0]["content"]
        self.assertLess(g.index("=== task/a.txt ==="), g.index("=== task/z.txt ==="))
        self.assertEqual(alpha["params"]["system"][0]["text"], expected_system(CONTENT))
        # Exactly one --cases for the Lead suite.
        args = ["--suite", "lead", "--clone", self.clone.root, "--model", "m", "--out", self.path("o")]
        self.assertEqual(self.run_prog("build", args, code=2), {"status": "usage"})
        self.assertEqual(self.run_prog("build", args + ["--cases", cases, "--cases", cases], code=2), {"status": "usage"})
        # A changed case file is refused; so is a directory the list does not name.
        (cases / "gate-beta" / "EXPECTED.md").write_text("changed\n")
        self.assertEqual(self.run_prog("build", args + ["--cases", cases], code=2), {"status": "case_digest_differs"})
        (cases / "gate-beta" / "EXPECTED.md").write_text("RUBRIC-SECRET beta\n")
        (cases / "other").mkdir()
        self.assertEqual(self.run_prog("build", args + ["--cases", cases], code=2), {"status": "case_unknown"})
        (cases / "other").rmdir()

        # The blind requests.
        res = self.results(*self.answer_lines(items))
        bout = self.path("blind")
        b = self.run_prog("blind", ["--items", out / "items.json", "--results", res, "--cases", cases,
                                    "--model", "scorer-model", "--out", bout])
        self.assertEqual((b["status"], b["answers"], b["requested"], b["unanswered"]), ("ok", 9, 5, 2))
        self.assertEqual({k: b[k] for k in COUNT_WORDS},
                         {"read": 7, "refused": 0, "errored": 1, "unreadable": 1, "stale": 0, "duplicate": 0, "skipped": 0})
        breqs = self.requests(bout)
        self.assertEqual(len(breqs), 5)
        schema = json.loads((self.site / "evals/batch/evals/blind.schema.json").read_text())
        scoring = (self.site / "evals/lead/scoring.md").read_text()
        blind_md = (self.site / "evals/batch/evals/blind.md").read_text()
        banned = ["gate-alpha", "gate-beta", "gate-gamma", "answerer-model", self.clone.head(), self.clone.sums(),
                  "CASEID", "instructions"]
        banned += [i["key"] for i in items["items"]]
        for r in breqs:
            p = r["params"]
            self.assertEqual(r["custom_id"], sha(canon(p)))
            self.assertEqual(set(p), {"model", "max_tokens", "system", "messages", "output_config"})
            self.assertEqual((p["model"], p["max_tokens"]), ("scorer-model", 2000))
            self.assertEqual(p["output_config"], {"format": {"type": "json_schema", "schema": schema}})
            self.assertTrue(p["system"].startswith(scoring.rstrip("\n")))
            self.assertIn(blind_md.rstrip("\n"), p["system"])
            u = p["messages"][0]["content"]
            self.assertRegex(u, r"^Rubric:\nRUBRIC-SECRET (alpha|beta|gamma)\n\nAnswer:\n")
            for word in banned[:-1]:
                if word != "instructions":
                    self.assertNotIn(word, json.dumps(p), word)
        users = sorted(r["params"]["messages"][0]["content"] for r in breqs)
        self.assertEqual(sum("same answer of alpha" in u for u in users), 1)
        self.assertEqual(sum("RUBRIC-SECRET beta" in u for u in users), 3)
        bj = json.loads((bout / "blind.json").read_text())
        self.assertEqual(set(bj), {"job", "suite", "model", "items", "answers"})
        self.assertEqual((bj["job"], bj["suite"], bj["model"]), ("evals", "lead", "scorer-model"))
        self.assertEqual(bj["items"], sha((out / "items.json").read_bytes()))
        self.assertEqual([(a["case"], a["sample"], a["key"]) for a in bj["answers"]],
                         [(i["case"], i["sample"], i["key"]) for i in items["items"]])
        keys = {(a["case"], a["sample"]): a["score_key"] for a in bj["answers"]}
        self.assertEqual(len({keys["gate-alpha", s] for s in (1, 2, 3)}), 1)
        self.assertIsNone(keys["gate-gamma", 2])
        self.assertIsNone(keys["gate-gamma", 3])
        self.assertEqual({r["custom_id"] for r in breqs}, {v for v in keys.values() if v})

        # The score: caught counts, partly does not, a line not read does not.
        sk = keys
        scores = self.results(scored(sk["gate-alpha", 1], "caught"),
                              scored(sk["gate-beta", 1], "caught"), scored(sk["gate-beta", 2], "partly"),
                              scored(sk["gate-beta", 3], "missed"), scored(sk["gate-gamma", 1], "caught"))
        s, sdir = self.score(out, res, extra=["--blind", bout / "blind.json", "--scores", scores])
        card = json.loads((sdir / "score.json").read_text())
        r = card["readers"]["answerer-model"]
        self.assertEqual((card["suite"], card["scorer"]), ("lead", "scorer-model"))
        self.assertEqual(card["case_files"], items["case_files"])
        self.assertEqual(r["passes"], {"gate-alpha": 3, "gate-beta": 1, "gate-gamma": 1})
        self.assertEqual((r["n"], r["samples"], r["mean"], r["majority"], r["unanswered"]),
                         (3, [3, 1, 1], 1.6667, ["gate-alpha"], 2))
        self.assertEqual((s["suite"], s["reader"], s["cases"], s["lost"]), ("lead", "answerer-model", 3, []))
        self.assertEqual(s["counts"], b["counts"] if "counts" in b else s["counts"])
        # A score line that is not read is no caught; a missing --blind or --scores is usage.
        bad = self.results(failed_line(sk["gate-alpha", 1]), succeeded(sk["gate-beta", 1], "caught"),
                           succeeded(sk["gate-gamma", 1], json.dumps({"result": "other", "quote": "q", "clause": "c"})),
                           scored(sk["gate-beta", 2], "caught", "q") .replace('"stop_reason": "end_turn"',
                                                                               '"stop_reason": "max_tokens"'))
        _, sdir2 = self.score(out, res, extra=["--blind", bout / "blind.json", "--scores", bad])
        r2 = json.loads((sdir2 / "score.json").read_text())["readers"]["answerer-model"]
        self.assertEqual(r2["passes"], {"gate-alpha": 0, "gate-beta": 0, "gate-gamma": 0})
        self.assertEqual(self.run_prog("score", ["--items", out / "items.json", "--results", res, "--out", self.path("o")],
                                       code=2), {"status": "usage"})
        self.assertEqual(self.run_prog("score", ["--items", out / "items.json", "--results", res, "--out", self.path("o"),
                                                 "--blind", bout / "blind.json"], code=2), {"status": "usage"})
        # A blind.json of other items is refused.
        other = self.path("other")
        _, _, items2, _, out2 = self.lead_build(model="another-model")
        v = self.run_prog("score", ["--items", out2 / "items.json", "--results", res, "--out", self.path("o"),
                                    "--blind", bout / "blind.json", "--scores", scores], code=2)
        self.assertEqual(v, {"status": "blind_unreadable"})
        (bout / "blind.json").write_text(json.dumps({"job": "evals"}))
        v = self.run_prog("score", ["--items", out / "items.json", "--results", res, "--out", self.path("o"),
                                    "--blind", bout / "blind.json", "--scores", scores], code=2)
        self.assertEqual(v, {"status": "blind_unreadable"})
        self.assertEqual(self.run_prog("blind", ["--items", out / "items.json", "--results", self.tmp / "missing",
                                                 "--cases", cases, "--model", "m", "--out", self.path("o")], code=2),
                         {"status": "results_unreadable"})

    def test_blind_needs_items_of_the_lead_suite(self):
        items, out, res = self.routing_pass([3] * 8)
        cases = self.lead_cases()
        v = self.run_prog("blind", ["--items", out / "items.json", "--results", res, "--cases", cases, "--model", "m",
                                    "--out", self.path("o")], code=2)
        self.assertEqual(v, {"status": "items_unreadable"})


class Recording(Case):
    A = [3, 3, 2, 2, 1, 1, 0, 0]
    B = [3, 3, 0, 0, 1, 1, 0, 0]

    def two(self, a=None, b=None):
        a = a or self.A
        b = b or self.B
        _, out_a, res_a = self.routing_pass(a)
        _, sa = self.score(out_a, res_a)
        _, out_b, res_b = self.routing_pass(b)
        return sa / "score.json", out_b, res_b

    def record(self, out, res, baseline=None, key="T-1", env=None, code=0, extra=()):
        args = ["--record", key, *extra]
        if baseline:
            args += ["--baseline", baseline]
        return self.score(out, res, extra=args, env=env or self.env(), code=code)

    def test_lost_case_finding_and_measurement(self):
        self.put(tasks={"T-1": {"directory": "d"}, "T-2": {}}, task_unqualified=True)
        base, out, res = self.two()
        v, sdir = self.record(out, res, base)
        card = (sdir / "score.json").read_bytes()
        self.assertEqual(v["lost"], ["r03", "r04"])
        rec = v["recorded"]
        self.assertEqual(rec, {"kpi": "defined", "measurement": "applied", "opened": [self.fkey("routing", "m1", "r03"),
                                                                                       self.fkey("routing", "m1", "r04")],
                               "seen": [], "left": 0, "over_cap": 0, "already_applied": 0, "failed": []})
        log = self.state()["log"]
        # The role of the caller's environment, no member role, and every read before the first apply.
        self.assertTrue(all(e["role"] == "lead" for e in log))
        self.assertTrue(all("role" not in (e["body"] or {}) for e in log))
        kinds = ["apply" if e["argv"][-4:-1] == ["action", "apply", "--client"] else "read" for e in log]
        self.assertEqual(kinds.index("apply"), kinds.count("read"))
        self.assertEqual([e["body"]["object_type"] for e in log if kinds[log.index(e)] == "read"][:2], ["task", "kpi"])
        types = [e["body"]["action_type"] for e in self.applies()]
        self.assertEqual(types, ["define_kpi", "record_measurement", "open_finding", "open_finding"])
        st = self.state()
        kpi = next(iter(st["kpis"].values()))
        self.assertEqual(kpi["name"], "batch evals routing m1")
        self.assertLessEqual(len(kpi["definition"].encode()), 600)
        self.assertIn("evals/batch/evals/score", kpi["definition"])
        r = json.loads(card)["readers"]["m1"]
        self.assertEqual(kpi["value"], "mean %.4f spread %.4f samples %s majority %d/8 n 3 unanswered 0" % (
            r["mean"], r["spread"], " ".join(map(str, r["samples"])), len(r["majority"])))
        self.assertRegex(kpi["observed_at"], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")
        self.assertEqual(kpi["evidence"], "evals/batch/evals/score sha256:%s content sha256:%s" % (
            sha(card)[:16], self.clone.sums()[:16]))
        # The findings: signature, summary, evidence; nothing of a case's text.
        f = st["findings"][self.fkey("routing", "m1", "r03")]
        self.assertEqual((f["kind"], f["detector"], f["severity"], f["state"]), ("note", "batch:evals", "info", "open"))
        self.assertEqual(f["summary"], "routing case r03 lost for m1: passed in 2 of 3 samples of the baseline and 0 of 3 "
                                       "of this pass")
        self.assertEqual(f["evidence"], "score sha256:%s baseline sha256:%s content sha256:%s" % (
            sha(card)[:16], sha(base.read_bytes())[:16], self.clone.sums()[:16]))
        blob = json.dumps([e["body"] for e in self.applies()])
        for c in self.cases:
            self.assertNotIn(c["situation"], blob)
            self.assertNotIn("fact-", blob)
        # The same call again: the measurement is already there and no apply is sent.
        before = len(self.applies())
        v2, _ = self.record(out, res, base)
        self.assertEqual(v2["recorded"], {"kpi": "found", "measurement": "already", "opened": [], "seen": [], "left": 0,
                                          "over_cap": 0, "already_applied": 0, "failed": []})
        self.assertEqual(len(self.applies()), before)
        # Evidence that changed is seen again, not opened again.
        self.put(findings={self.fkey("routing", "m1", "r03"): dict(f, evidence="old")})
        v3, _ = self.record(out, res, base)
        self.assertEqual((v3["recorded"]["opened"], v3["recorded"]["seen"]), ([], [self.fkey("routing", "m1", "r03")]))

    def test_findings_by_state_and_cap(self):
        self.set_routing(routing_cases(12))
        self.put(tasks={"T-1": {}})
        base, out, res = self.two([3] * 12, [0] * 12)
        v, _ = self.record(out, res, base)
        self.assertEqual((len(v["recorded"]["opened"]), v["recorded"]["over_cap"]), (10, 2))
        self.assertEqual(len([e for e in self.applies() if e["body"]["action_type"] == "open_finding"]), 10)
        v, _ = self.record(out, res, base)
        self.assertEqual((len(v["recorded"]["opened"]), v["recorded"]["over_cap"], v["recorded"]["measurement"]),
                         (2, 0, "already"))
        # A closed or dismissed finding is left, whatever the evidence; one of another detector is not read.
        base, out, res = self.two([3] * 12, [3] * 4 + [0] * 8)
        st = self.state()
        for i, state in ((5, "closed"), (6, "dismissed")):
            k = self.fkey("routing", "m1", "r%02d" % i)
            st["findings"][k]["state"] = state
            st["findings"][k]["evidence"] = "other"
        (self.tmp / "ont-state.json").write_text(json.dumps(st))
        before = len(self.applies())
        v, _ = self.record(out, res, base)
        rec = v["recorded"]
        # The open ones carry the new pass's evidence and are seen again; the closed and the dismissed are left.
        self.assertEqual((rec["left"], rec["opened"], len(rec["seen"])), (2, [], 6))
        self.assertNotIn(self.fkey("routing", "m1", "r05"), rec["seen"])
        self.assertNotIn(self.fkey("routing", "m1", "r06"), rec["seen"])
        self.assertEqual([e["body"]["action_type"] for e in self.applies()[before:]], ["record_measurement"] +
                         ["see_finding_again"] * 6)

    def test_role_and_store_failures(self):
        self.put(tasks={"T-1": {}}, require_role=True)
        base, out, res = self.two()
        v, sdir = self.record(out, res, base, env=self.env(role=None), code=1)
        self.assertEqual(v["recorded"]["measurement"], "failed")
        self.assertEqual({f["code"] for f in v["recorded"]["failed"]}, {"not_granted"})
        self.assertTrue(len(v["recorded"]["failed"]) >= 3)
        self.assertTrue((sdir / "score.json").exists())
        self.assertTrue(all(e["role"] is None for e in self.state()["log"]))
        # The repeat with the role applies what failed (a request id of the same content is safe).
        v, _ = self.record(out, res, base)
        self.assertEqual((v["recorded"]["kpi"], v["recorded"]["measurement"], len(v["recorded"]["opened"])),
                         ("defined", "applied", 2))
        # The environment passes unchanged: an agent of the caller's is the engine's to see.
        v, _ = self.record(out, res, base, env=self.env(ONT_AGENT="someone"))
        self.assertEqual(self.state()["log"][-1]["agent"], "someone")

    def test_unknown_task_and_unqualified_pages(self):
        base, out, res = self.two()
        v, sdir = self.record(out, res, base, code=3)
        self.assertEqual(v, {"status": "unknown_task"})
        self.assertEqual(self.applies(), [])
        self.assertFalse((sdir / "score.json").exists())
        self.put(tasks={"T-1": {}}, kpi_unqualified=True)
        v, _ = self.record(out, res, base, code=3)
        self.assertEqual(v, {"status": "store_failed"})
        self.assertEqual(self.applies(), [])
        self.put(kpi_unqualified=False, finding_unqualified=True)
        v, _ = self.record(out, res, base, code=3)
        self.assertEqual(v, {"status": "store_failed"})
        self.assertEqual(self.applies(), [])
        self.assertEqual(self.record(out, res, key="", code=2)[0], {"status": "usage"})
        # No engine is started without --record, and a missing one is a store failure.
        self.put(finding_unqualified=False)
        env = self.env(ONT_BIN=str(self.tmp / "none"))
        v, _ = self.record(out, res, base, env=env, code=3)
        self.assertEqual(v, {"status": "store_failed"})

    def test_older_measurement_is_counted(self):
        self.put(tasks={"T-1": {}}, floor="2999-01-01T00:00:00Z")
        base, out, res = self.two()
        v, _ = self.record(out, res, base)
        self.assertEqual(v["recorded"]["measurement"], "older")
        self.assertEqual(v["recorded"]["failed"], [])

    def test_baseline_must_be_of_the_pass(self):
        base, out, res = self.two()
        bad = self.path("bad.json")
        bad.write_text("{}")
        self.assertEqual(self.score(out, res, extra=["--baseline", bad], code=2)[0], {"status": "score_unreadable"})
        card = json.loads(base.read_text())
        other = dict(card, readers={"zzz": card["readers"]["m1"]})
        p = self.path("other.json")
        p.write_text(json.dumps(other))
        self.assertEqual(self.score(out, res, extra=["--baseline", p], code=2)[0], {"status": "case_sets_differ"})
        other = dict(card, case_files={"cases.json": "0" * 64})
        p.write_text(json.dumps(other))
        self.assertEqual(self.score(out, res, extra=["--baseline", p], code=2)[0], {"status": "case_sets_differ"})
        other = dict(card, suite="lead")
        p.write_text(json.dumps(other))
        self.assertEqual(self.score(out, res, extra=["--baseline", p], code=2)[0], {"status": "case_sets_differ"})
        v, _ = self.score(out, res, extra=["--baseline", base])
        self.assertEqual(v["lost"], ["r03", "r04"])

    def test_actions_fit_the_schema(self):
        self.put(tasks={"T-1": {}})
        base, out, res = self.two()
        self.put(findings={self.fkey("routing", "m1", "r04"): {"kind": "note", "detector": "batch:evals", "summary": "s",
                                                               "evidence": "old", "severity": "info", "state": "open"}})
        v, sdir = self.record(out, res, base)
        card_sha = sha((sdir / "score.json").read_bytes())
        schema = json.loads((REPO / "ontology" / "work.schema.json").read_text())
        host = (REPO / "crates" / "ap" / "src" / "host.rs").read_text()
        m = re.search(r'fn id\(kind: &str, key: &str\) -> Value \{\s*json!\(\{"object_type":kind,"namespace":(\{[^}]*\})', host)
        self.assertIsNotNone(m)
        namespace = json.loads(m.group(1))
        types = schema["value_types"]
        seen = set()
        for e in self.applies():
            body = e["body"]
            self.assertEqual(set(body), {"action_type", "parameters", "agent", "request_id"})
            self.assertEqual(body["agent"], "batch-evals")
            self.assertRegex(body["request_id"], r"^be-[0-9a-f]{40}$")
            self.assertEqual(e["argv"][-4:], ["action", "apply", "--client", "batch-evals"])
            a = body["action_type"]
            seen.add(a)
            declared = schema["action_types"][a]["parameters"]
            p = body["parameters"]
            self.assertLessEqual(set(p), set(declared))
            self.assertLessEqual({k for k, d in declared.items() if d["required"]}, set(p))
            for name, value in p.items():
                t = declared[name]["type"]
                if isinstance(t, dict) and "ref" in t:
                    self.assertEqual(value["object_type"], t["ref"])
                    self.assertEqual(value["namespace"], namespace)
                    self.assertEqual(set(value), {"object_type", "namespace", "native_key"})
                elif isinstance(t, dict) and "value_type" in t:
                    vt = types[t["value_type"]]
                    if "enum" in vt:
                        self.assertIn(value, vt["enum"])
                    else:
                        self.assertRegex(value, vt["pattern"])
                else:
                    self.assertEqual(t, "string")
                    self.assertTrue(value)
                    self.assertLessEqual(len(value.encode("utf-8")), declared[name]["max_bytes"])
            name = p["name"] if a == "define_kpi" else (
                "batch evals routing m1" if a == "record_measurement" else p["finding"]["native_key"])
            self.assertEqual(body["request_id"], "be-" + sha(canon([a, name, card_sha, p]))[:40])
            if a in ("open_finding", "see_finding_again"):
                self.assertRegex(p["finding"]["native_key"], r"^batch-evals-[0-9a-f]{16}$")
            if a == "open_finding":
                self.assertEqual((p["kind"], p["detector"], p["severity"]), ("note", "batch:evals", "info"))
        self.assertEqual(seen, {"define_kpi", "record_measurement", "open_finding", "see_finding_again"})


class Texts(Case):
    def test_schemas_and_prompts(self):
        d = self.site / "evals" / "batch" / "evals"
        schema = json.loads((d / "blind.schema.json").read_text())
        self.assertEqual(schema["type"], "object")
        self.assertEqual(set(schema["properties"]), {"result", "quote", "clause"})
        self.assertEqual(set(schema["required"]), set(schema["properties"]))
        self.assertIs(schema["additionalProperties"], False)
        self.assertEqual(schema["properties"]["result"]["type"], "string")
        self.assertEqual(schema["properties"]["result"]["enum"], list(self.lead.RESULTS))
        self.assertEqual(schema["properties"]["quote"]["type"], "string")
        self.assertEqual(schema["properties"]["clause"]["type"], "string")
        prompt = self.routing.PROMPT
        answer = (d / "answer.md").read_text()
        last = prompt.rsplit("\n\n", 1)[1]
        self.assertTrue(last.startswith("Answer with the context"))
        self.assertTrue(answer.rstrip("\n").endswith(last))
        first = "You are an agent on this host. Your assignment: {situation}"
        before = "Before you act, find the context you must load for this assignment."
        self.assertIn(first, prompt)
        self.assertIn(before, prompt)
        self.assertEqual(answer.count("{situation}"), 1)
        order = [answer.index(x) for x in (first, before, "/root/.agents", "Do not do the assignment.", last)]
        self.assertEqual(order, sorted(order))
        for word in ("ont fn route", "ap status", "Ask the functions", "/home/", "/var/lib", "edu-", "education", "store",
                     "read-only", "Read, search"):
            self.assertNotIn(word, answer)
        lead = (d / "lead.md").read_text()
        for word in ("/root/.agents", "no tool", "system"):
            self.assertIn(word, lead.lower())
        blind = (d / "blind.md").read_text()
        for word in ("result", "quote", "clause", "caught", "partly", "missed"):
            self.assertIn(word, blind)
        for word in ("model", "sample", "instructions", "commit"):
            self.assertNotIn(word, blind.lower())
        for name in ("build", "blind", "score"):
            p = REPO / "evals" / "batch" / "evals" / name
            self.assertEqual(p.read_text().splitlines()[0], "#!/usr/bin/env -S python3 -I -B")
            self.assertEqual(stat.S_IMODE(p.stat().st_mode), 0o755)
        # The map names this test for the component that holds the programs.
        m = json.loads((REPO / "map.json").read_text())
        evals = next(c for c in m["components"] if c["name"] == "evals")
        self.assertIn("ci/test_batch_evals.py", evals["tests"])

    def test_the_last_paragraph_of_the_real_prompt(self):
        routing = load("routing_real", REPO / "evals" / "routing" / "run")
        answer = (REPO / "evals" / "batch" / "evals" / "answer.md").read_text()
        self.assertTrue(answer.rstrip("\n").endswith(routing.PROMPT.rsplit("\n\n", 1)[1]))

    def test_sources_hold_no_network_and_no_host_path(self):
        net = "|".join(["url" + "lib", "ht" + "tp", "sock" + "et", "s" + "sl", "async" + "io", "requ" + "ests", "ht" + "tpx",
                        "anthro" + "pic"])
        bad = [r"^\s*(import|from)\s+(%s)" % net, "api" + r"\." + "anthro" + "pic" + r"\.com", "/home/" + "palantirkc",
               "/var/" + "lib", "cu" + "rl"]
        for f in (REPO / "evals" / "batch" / "evals").iterdir():
            if f.is_dir() or f.suffix == ".md":
                continue
            text = f.read_text()
            for pat in bad:
                self.assertIsNone(re.search(pat, text, re.M), (f.name, pat))
        for name in ("build", "blind", "score", "common.py"):
            text = (REPO / "evals" / "batch" / "evals" / name).read_text()
            for m in re.finditer(r"^\s*(?:import|from)\s+([A-Za-z_][A-Za-z0-9_.]*)", text, re.M):
                self.assertIn(m.group(1).split(".")[0], sys.stdlib_module_names, (name, m.group(1)))
        # The test itself makes no directory of the Academy.
        mine = Path(__file__).read_text()
        self.assertIsNone(re.search(r"mkdir\([^)]*(edu-|education-)", mine))


if __name__ == "__main__":
    unittest.main()
