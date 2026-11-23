"""Tests of evals/batch/drift/build and evals/batch/drift/read, the drift job of the batch evaluations, run as programs
against scratch repositories made by `git init` and a fake `ont` that keeps its findings in a file (no model, no
network, no engine, no store of the host). Contract: evals/batch/README.md."""
import hashlib
import importlib.machinery
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
BATCH = REPO / "evals" / "batch"
BUILD = BATCH / "drift" / "build"
READ = BATCH / "drift" / "read"
PY = sys.executable
HOME = os.environ.get("HOME", "/")
OWNER = "o/n"

# A planted citation or requirement line is built from parts here, so that this file holds none in the literal form.
WORD = "spec" + ":"


def cite(path, n):
    return f"{WORD} {path}#R{n}"


def req(n, text):
    return "- R" + str(n) + ": " + text


def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    mod = importlib.util.module_from_spec(importlib.util.spec_from_loader(name, loader))
    loader.exec_module(mod)
    return mod


LIB = load("batch_lib", BATCH / "lib.py")

GIT_ID = ["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "-c", "commit.gpgsign=false",
          "-c", "init.defaultBranch=main"]

# The fake engine: findings in ont-state.json beside its directory; every request recorded; request ids kept as the
# engine keeps them; the submission criteria of the four finding actions.
FAKE_ONT = r'''#!@PY@ -I
import json, os, sys
STATE = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "ont-state.json")
try:
    with open(STATE) as f:
        st = json.load(f)
except FileNotFoundError:
    st = {"findings": {}, "rids": {}, "log": []}
args = sys.argv[1:]
raw = sys.stdin.read()
body = json.loads(raw) if raw else None
st["log"].append({"argv": args, "body": body})
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
if args == ["search"]:
    if st.get("unqualified"):
        out({"status": "ok", "result": {"objects": [], "basis": {"qualified": False}, "continuation": None}})
    eqs = [c["eq"] for c in body["where"]["all"]]
    rows = sorted(k for k, f in st["findings"].items() if all(f.get(p) == v for p, v in eqs))
    start = int(body.get("continuation") or 0)
    page = min(int(body.get("limit", 15)), 4)
    chunk = rows[start:start + page]
    nxt = str(start + page) if start + page < len(rows) else None
    objs = []
    for k in chunk:
        shown = st.get("shown", {}).get(k, st["findings"][k])
        fields = {p: {"kind": "value", "value": v} for p, v in shown.items()}
        fields["unit"] = {"kind": "absent"}
        objs.append({"id": {"object_type": "finding", "native_key": k}, "head": {"fields": fields}})
    out({"status": "ok", "result": {"objects": objs, "basis": {"qualified": True}, "continuation": nxt}})
if args == ["action", "apply"]:
    rid = body["request_id"]
    content = json.dumps([body["action_type"], body["parameters"], body.get("agent"), client], sort_keys=True)
    if rid in st["rids"]:
        if st["rids"][rid] == content:
            out({"status": "ok", "result": {"outcome": "already_applied", "edits": []}})
        out({"status": "refused", "code": "request_id_reused"}, 2)
    a, p = body["action_type"], body["parameters"]
    key = p["finding"]["native_key"]
    f = st["findings"].get(key)
    state = f["state"] if f else None
    def crit(c):
        out({"status": "refused", "code": "submission_criteria", "failed": [c]}, 2)
    if a == "open_finding":
        if f:
            out({"status": "refused", "code": "revision_conflict"}, 2)
        f = {k: p[k] for k in ("kind", "detector", "summary", "evidence", "severity") if k in p}
        f["state"] = "open"
    elif a == "see_finding_again":
        if state not in ("open", "planned"):
            crit("finding_not_live")
        f["evidence"] = p.get("evidence")
    elif a == "reopen_finding":
        if state not in ("dismissed", "closed"):
            crit("finding_live")
        f["evidence"], f["state"] = p["evidence"], "open"
    elif a == "close_finding":
        if state not in ("open", "planned"):
            crit("finding_not_live")
        if not p.get("reason"):
            crit("reason_empty")
        f["state"], f["reason"] = "closed", p["reason"]
    else:
        out({"status": "refused", "code": "unknown_action"}, 2)
    st["findings"][key] = f
    st["rids"][rid] = content
    out({"status": "ok", "result": {"outcome": "applied", "edits": [{"change": "modify", "id": p["finding"]}]}})
out({"status": "refused", "code": "usage"}, 2)
'''


def sha(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def canon(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def fkey(signature):
    return "batch-drift-" + sha(signature)[:16]


def sig(path, n, repo=OWNER):
    return f"{repo}:{path}#R{n}"


def verdict(*rules):
    return {"rules": [{"rule": r, "verdict": v, "tests": list(t), "why": w} for r, v, t, w in rules]}


def succeeded(key, body=None, stop="end_turn", text=None):
    text = json.dumps(body) if text is None else text
    message = {"id": "msg_1", "type": "message", "role": "assistant", "model": "m", "stop_reason": stop,
               "content": [{"type": "text", "text": text}], "usage": {"input_tokens": 1, "output_tokens": 1}}
    return json.dumps({"custom_id": key, "result": {"type": "succeeded", "message": message}})


def failed_line(key, kind="errored"):
    result = {"type": kind}
    if kind == "errored":
        result["error"] = {"type": "error", "error": {"type": "api_error", "message": "x"}}
    return json.dumps({"custom_id": key, "result": result})


class Repo:
    """A scratch repository; origin/main follows each commit unless told otherwise."""

    def __init__(self, root, files):
        self.root = root
        root.mkdir(parents=True)
        self.git("init", "-q")
        self.commit(files)

    def git(self, *args):
        return subprocess.run(["git", *GIT_ID, "-C", str(self.root), *args], check=True, capture_output=True,
                              text=True, env={"PATH": os.environ["PATH"], "HOME": HOME}).stdout

    def write(self, files):
        for rel, text in files.items():
            p = self.root / rel
            if text is None:
                p.unlink()
                continue
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(text)

    def commit(self, files, origin=True):
        self.write(files)
        self.git("add", "-A")
        self.git("commit", "-q", "--allow-empty", "-m", "c")
        if origin:
            self.git("update-ref", "refs/remotes/origin/main", "HEAD")
        return self.head()

    def head(self):
        return self.git("rev-parse", "HEAD").strip()


class Case(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.tmp = Path(tmp.name)
        self.bin = self.tmp / "bin"
        self.bin.mkdir()
        (self.bin / "ont").write_text(FAKE_ONT.replace("@PY@", PY))
        (self.bin / "ont").chmod(0o755)
        self.n = 0

    def path(self, name):
        self.n += 1
        return self.tmp / f"{self.n}-{name}"

    def repo(self, files):
        return Repo(self.path("clone"), files)

    def run_prog(self, prog, args, env):
        if prog == READ:
            # Every reader call names the fake engine by ONT_BIN, with a PATH that holds no other ont.
            self.assertTrue(os.path.isabs(env.get("ONT_BIN", "")) and os.path.basename(env["ONT_BIN"]) == "ont", env)
            self.assertTrue(all(not os.path.exists(os.path.join(d, "ont")) or os.path.samefile(os.path.join(d, "ont"), env["ONT_BIN"])
                                for d in env.get("PATH", "").split(os.pathsep) if d), env)
        p = subprocess.run([PY, "-I", "-B", str(prog), *map(str, args)], capture_output=True, text=True, env=env)
        self.assertEqual(p.stderr, "")
        lines = p.stdout.splitlines()
        self.assertEqual(len(lines), 1, p.stdout)
        return p.returncode, json.loads(lines[0])

    def build(self, repos, out=None, model="m", extra=(), code=0, env=None):
        out = out or self.path("out")
        args = []
        for name, repo in repos.items():
            args += ["--repo", f"{name}={repo.root if isinstance(repo, Repo) else repo}"]
        args += ["--model", model, "--out", out, *extra]
        rc, v = self.run_prog(BUILD, args, env or {"PATH": os.environ["PATH"], "HOME": HOME})
        self.assertEqual(rc, code, v)
        if code == 0:
            self.assertEqual(v["status"], "ok")
            g = json.loads((Path(out) / "groups.json").read_text())
            r = [json.loads(x) for x in (Path(out) / "requests.jsonl").read_text().splitlines()]
            return v, g, r, Path(out)
        return v

    def read(self, groups, results, extra=(), code=0, env=None):
        env = env or {"PATH": str(self.bin), "HOME": HOME, "ONT_BIN": str(self.bin / "ont")}
        rc, v = self.run_prog(READ, ["--groups", groups, "--results", results, *extra], env)
        self.assertEqual(rc, code, v)
        return v

    def results(self, *lines):
        p = self.path("results.jsonl")
        p.write_text("".join(x + "\n" for x in lines))
        return p

    # The fake's state.
    def state(self):
        p = self.tmp / "ont-state.json"
        return json.loads(p.read_text()) if p.exists() else {"findings": {}, "rids": {}, "log": []}

    def put(self, st):
        (self.tmp / "ont-state.json").write_text(json.dumps(st))

    def seed(self, signature, state, evidence="old", detector="batch:drift", key=None, rest="untested: x"):
        st = self.state()
        st["findings"][key or fkey(signature)] = {
            "kind": "drift", "detector": detector, "summary": f"{signature} {rest}", "evidence": evidence,
            "severity": "info", "state": state}
        self.put(st)

    def set_state(self, key, state):
        st = self.state()
        st["findings"][key]["state"] = state
        self.put(st)

    def applies(self):
        return [e for e in self.state()["log"] if e["argv"][-2:] == ["action", "apply"] or "apply" in e["argv"]]

    def group(self, g, path, rule="R1"):
        return next(x for x in g["groups"] if x["path"] == path and any(r["rule"] == rule for r in x["rules"]))


def spec(*lines):
    return "# A spec\n\n## Requirements\n\n" + "".join(x + "\n" for x in lines)


def excerpts(text):
    return dict(re.findall(r'<excerpt location="([^"]+)">\n(.*?)\n</excerpt>', text, re.S))


def user_text(request):
    return request["params"]["messages"][0]["content"]


class BatchDriftTest(Case):
    def test_fixture_one_request_one_finding_then_none(self):
        repo = self.repo({"specs/a.md": spec(req(1, "the thing holds.")),
                          "tests/a.sh": f"# {cite('specs/a.md', 1)}\ntest -n x\n"})
        repo.commit({"tests/a.sh": None})  # the test is removed
        only = self.path("only-bin")
        only.mkdir()
        os.symlink(shutil.which("git"), only / "git")
        os.symlink(self.bin / "ont", only / "ont")
        env = {"PATH": str(only), "HOME": HOME}
        ont_env = dict(env, ONT_BIN=str(only / "ont"))
        v, g, r, out = self.build({OWNER: repo}, env=env)
        self.assertEqual((v["groups"], v["requested"], v["settled"], v["requirements"]), (1, 1, 0, 1))
        self.assertEqual(len(r), 1)
        key = g["groups"][0]["key"]
        self.assertEqual(g["groups"][0]["rules"], [{"rule": "R1", "line": 5, "tests": []}])
        res = self.results(succeeded(key, verdict(("R1", "untested", [], "No test cites it."))))
        a = self.read(out / "groups.json", res, env=ont_env)
        signature = sig("specs/a.md", 1)
        self.assertEqual(a["opened"], [fkey(signature)])
        self.assertEqual((a["read"], a["failed"]), (1, []))
        f = self.state()["findings"][fkey(signature)]
        self.assertEqual((f["kind"], f["detector"], f["severity"], f["state"]), ("drift", "batch:drift", "info", "open"))
        self.assertEqual(f["summary"], f"{signature} untested: no citing test tests this requirement")
        self.assertTrue(f["evidence"].startswith(f"key {key} commit {repo.head()}"))
        before = len(self.applies())
        v2, g2, r2, out2 = self.build({OWNER: repo}, extra=["--results", res], env=env)
        self.assertEqual((v2["requested"], v2["settled"]), (0, 1))
        self.assertEqual((out2 / "requests.jsonl").read_bytes(), b"")
        self.assertEqual(g2["groups"][0]["state"], "settled")
        a2 = self.read(out2 / "groups.json", res, env=ont_env)
        self.assertEqual((a2["opened"], a2["seen"], a2["reopened"], a2["closed"]), ([], [], [], []))
        self.assertEqual(len(self.applies()), before)

    def test_patterns_are_those_of_check_spec_trace(self):
        trace = load("check_spec_trace", REPO / "ci" / "check-spec-trace.py")
        build = load("batch_drift_build", BUILD)
        self.assertEqual(build.REQ_RE.pattern, trace.REQ_RE.pattern)
        self.assertEqual(build.CITE_RE.pattern, trace.CITE_RE.pattern)

    def test_groups_of_twelve_and_keys(self):
        rs = "".join(f"// {cite('specs/ap/b.md', n)}\n#[test]\nfn t{n}() {{\n    assert_eq!({n}, {n});\n}}\n\n"
                     for n in range(1, 26))
        repo = self.repo({"specs/ap/b.md": spec(*(req(n, f"rule {n}.") for n in range(1, 26))), "tests/b.rs": rs})
        v, g, r, out = self.build({OWNER: repo})
        self.assertEqual([len(x["rules"]) for x in g["groups"]], [12, 12, 1])
        self.assertEqual((v["groups"], v["requested"], v["requirements"], v["citations"]), (3, 3, 25, 25))
        self.assertEqual(g["job"], "drift")
        self.assertEqual(g["model"], "m")
        schema = json.loads((BATCH / "drift" / "verdict.schema.json").read_text())
        prompt = (BATCH / "drift" / "prompt.md").read_text()
        for x, req_line in zip(g["groups"], r):
            self.assertEqual(req_line["custom_id"], x["key"])
            self.assertEqual(sha(canon(req_line["params"])), x["key"])
            p = req_line["params"]
            self.assertEqual(set(p), {"model", "max_tokens", "system", "messages", "output_config"})
            self.assertEqual((p["model"], p["max_tokens"], p["system"]), ("m", 8000, prompt))
            self.assertEqual(p["output_config"], {"format": {"type": "json_schema", "schema": schema}})
            self.assertEqual([m["role"] for m in p["messages"]], ["user"])
            self.assertIsInstance(p["messages"][0]["content"], str)
            self.assertEqual(x["state"], "requested")
            self.assertEqual(x["commit"], repo.head())
            self.assertNotIn(repo.head(), canon(p))
            self.assertNotIn(str(self.tmp), canon(p))
        self.assertEqual(g["groups"][0]["rules"][0], {"rule": "R1", "line": 5, "tests": ["tests/b.rs:1"]})
        keys = [x["key"] for x in g["groups"]]
        # A second run writes byte-equal files.
        _, _, _, out2 = self.build({OWNER: repo})
        for name in ("groups.json", "requests.jsonl"):
            self.assertEqual((out / name).read_bytes(), (out2 / name).read_bytes())
            self.assertEqual((out / name).stat().st_mode & 0o777, 0o600)
        self.assertEqual(out.stat().st_mode & 0o777, 0o700)
        # Another max_tokens or model changes every key; one out of range is refused.
        _, g3, _, _ = self.build({OWNER: repo}, extra=["--max-tokens", "1024"])
        self.assertTrue(all(a != b["key"] for a, b in zip(keys, g3["groups"])))
        _, g3, _, _ = self.build({OWNER: repo}, model="m2")
        self.assertTrue(all(a != b["key"] for a, b in zip(keys, g3["groups"])))
        for bad in ("1023", "32001", "x"):
            self.assertEqual(self.build({OWNER: repo}, extra=["--max-tokens", bad], code=2), {"status": "usage"})
        # A changed excerpt changes only its group's key (R13 is the second group's first rule).
        repo.commit({"tests/b.rs": rs.replace("assert_eq!(13, 13)", "assert_eq!(13, 14)")})
        _, g4, _, _ = self.build({OWNER: repo})
        new = [x["key"] for x in g4["groups"]]
        self.assertEqual((new[0] == keys[0], new[1] == keys[1], new[2] == keys[2]), (True, False, True))
        # The default --rev reads refs/remotes/origin/main; a commit after it and the working tree are not read.
        origin = repo.git("rev-parse", "refs/remotes/origin/main").strip()
        repo.commit({"tests/b.rs": rs.replace("assert_eq!(1, 1)", "assert_eq!(1, 2)")}, origin=False)
        repo.write({"tests/b.rs": rs.replace("assert_eq!(25, 25)", "assert_eq!(25, 2)")})
        _, g5, _, _ = self.build({OWNER: repo})
        self.assertEqual([x["key"] for x in g5["groups"]], new)
        self.assertEqual({x["commit"] for x in g5["groups"]}, {origin})
        self.assertNotEqual(origin, repo.head())
        # --rev names another commit; one that cannot be read is repo_unreadable.
        _, g6, _, _ = self.build({OWNER: repo}, extra=["--rev", "HEAD"])
        self.assertNotEqual(g6["groups"][0]["key"], new[0])
        self.assertEqual(self.build({OWNER: repo}, extra=["--rev", "nothing"], code=2),
                         {"status": "repo_unreadable", "repository": OWNER})
        self.assertEqual(self.build({OWNER: self.tmp / "missing"}, code=2),
                         {"status": "repo_unreadable", "repository": OWNER})
        self.assertEqual(self.build({OWNER: repo}, extra=["--results", self.tmp / "missing.jsonl"], code=2),
                         {"status": "results_unreadable"})

    def test_excerpts(self):
        s = "specs/x.md"
        long_body = "".join(f"    let v{i} = {i};\n" for i in range(398))
        repo = self.repo({
            s: spec(*(req(n, f"rule {n}.") for n in range(1, 9))),
            # agent-platform's form: the citation above the item.
            "tests/above.rs": f"use x;\n\n// {cite(s, 1)}\n#[test]\nfn above() {{\n    let a = 1;\n}}\n\nfn other() {{\n}}\n",
            # ontology-platform's form: the citation inside the body.
            "tests/inside.rs": ("#[cfg(test)]\nmod tests {\n    /// Doc.\n    #[test]\n    fn inside() {\n"
                                f"        // {cite(s, 2)}\n        assert!(true);\n    }}\n}}\n"),
            "tests/after.rs": f"// {cite(s, 3)}\n\nfn later() {{\n}}\n",
            "tests/none.rs": f"// {cite(s, 4)}\nconst X: u8 = 1;\n",
            "tests/t.sh": f"#!/bin/sh\n# {cite(s, 5)}\n# {cite(s, 6)}\necho one\necho two\n# {cite(s, 7)}\necho three\n",
            "tests/long.rs": f"// {cite(s, 8)}\nfn long() {{\n{long_body}}}\n",
        })
        _, g, r, _ = self.build({OWNER: repo})
        text = user_text(r[0])
        ex = excerpts(text)
        self.assertEqual(set(ex), {"tests/above.rs:3-7", "tests/inside.rs:3-8", "tests/after.rs:3-4",
                                   "tests/none.rs:1-1", "tests/t.sh:2-5", "tests/t.sh:6-7", "tests/long.rs:1-400"})
        self.assertEqual(ex["tests/above.rs:3-7"].split("\n")[-1], "}")
        self.assertEqual(ex["tests/inside.rs:3-8"].split("\n")[0], "    /// Doc.")
        self.assertEqual(ex["tests/t.sh:6-7"].split("\n")[-1], "echo three")
        long = ex["tests/long.rs:1-400"].split("\n")
        self.assertEqual(len(long), 401)
        self.assertIn("cut", long[-1])
        self.assertEqual(long[-2], "    let v397 = 397;")
        self.assertEqual(text.count('<excerpt location="tests/t.sh:2-5">'), 1)
        rules = {x["rule"]: x["tests"] for x in g["groups"][0]["rules"]}
        self.assertEqual(rules["R5"], ["tests/t.sh:2"])
        self.assertEqual(rules["R6"], ["tests/t.sh:3"])
        self.assertEqual(rules["R2"], ["tests/inside.rs:6"])
        # The spec file is in the text whole.
        self.assertIn(spec(*(req(n, f"rule {n}.") for n in range(1, 9))), text)

    def test_academy_path_is_refused(self):
        table = {"edu-x": True, "a/edu-x/b": True, "/x/education-platform": True, "o/education-platform": True,
                 "a/evals/lesson": True, "evals/lesson/x.sh": True, "/evals/lesson/": True,
                 "evals/lessons": False, "lesson/evals": False, "a/xedu-b": False, "educational/x": False,
                 "evals/x/lesson": False, "medu-x/y": False, "edu": False, "education": False, "": False}
        for path, want in table.items():
            self.assertEqual(LIB.is_academy(path), want, path)
        repo = self.repo({"specs/a.md": spec(req(1, "one."), req(2, "two.")),
                          "evals/lesson/t.sh": f"# {cite('specs/a.md', 1)}\necho lesson\n",
                          "tests/t.sh": f"# {cite('specs/a.md', 2)}\necho test\n"})
        v, g, r, out = self.build({OWNER: repo})
        self.assertEqual((v["academy_skipped"], v["citations"]), (1, 1))
        self.assertNotIn("evals/lesson", canon(r))
        self.assertEqual([x["tests"] for x in g["groups"][0]["rules"]], [[], ["tests/t.sh:1"]])
        groups = out / "groups.json"
        res = self.results()
        refused = {"status": "academy_path"}
        # A link whose target is an Academy path that does not exist.
        os.symlink(self.tmp / "edu-gone", self.tmp / "link")
        os.symlink("education-gone", self.tmp / "rel")
        cases = [
            ({"edu-o/n": repo}, []),
            ({"o/education-n": repo}, []),
            ({OWNER: self.tmp / "edu-clone"}, []),
            ({OWNER: repo}, ["--results", self.tmp / "evals" / "lesson" / "r.jsonl"]),
            ({OWNER: repo}, ["--results", self.tmp / "link" / "r.jsonl"]),
            ({OWNER: self.tmp / "rel"}, []),
        ]
        for repos, extra in cases:
            out = self.path("out")
            self.assertEqual(self.build(repos, out=out, extra=extra, code=2), refused, (repos, extra))
            self.assertFalse(out.exists())
        for bad in (self.tmp / "education-out", self.tmp / "link" / "out", self.tmp / "rel"):
            self.assertEqual(self.build({OWNER: repo}, out=bad, code=2), refused)
        self.assertFalse((self.tmp / "link").resolve().exists())
        for args in ([self.tmp / "edu-g.json", res], [groups, self.tmp / "edu-r.jsonl"],
                     [self.tmp / "link" / "g.json", res], [groups, res, "--store", self.tmp / "edu-s"],
                     [self.tmp / "x" / "evals" / "lesson" / "g.json", res]):
            self.assertEqual(self.read(args[0], args[1], extra=args[2:], code=2), refused)
        self.assertEqual(self.state()["log"], [])
        self.assertFalse(any(p.name.startswith(("edu-", "education-")) for p in self.tmp.iterdir()))

    def test_refusal_and_error_are_counted_not_findings(self):
        files = {f"specs/{x}.md": spec(req(1, f"rule {x}.")) for x in "abcde"}
        files["tests/d.sh"] = f"# {cite('specs/d.md', 1)}\necho d\n"
        repo = self.repo(files)
        _, g, _, out = self.build({OWNER: repo})
        k = {x["path"][6]: x["key"] for x in g["groups"]}
        untested = verdict(("R1", "untested", [], "none."))
        tested = verdict(("R1", "tested", ["tests/d.sh:1"], "it fails."))
        res = self.results(
            "not a result line",
            succeeded("0" * 64, untested),               # stale
            failed_line(k["a"]),                         # errored
            succeeded(k["b"], untested, stop="refusal"),  # refused
            succeeded(k["c"], untested, stop="max_tokens"),  # unreadable: cut
            succeeded(k["e"], text='{"rules": ['),       # unreadable: no JSON
            succeeded(k["d"], tested),                   # read
            failed_line(k["d"], "expired"),              # duplicate
        )
        a = self.read(out / "groups.json", res)
        want = {"lines": 8, "skipped": 1, "stale": 1, "errored": 1, "refused": 1, "unreadable": 2, "read": 1,
                "duplicate": 1, "opened": [], "seen": [], "reopened": [], "closed": [], "failed": [], "over_cap": 0}
        self.assertEqual({x: a[x] for x in want}, want)
        self.assertEqual(self.applies(), [])
        for kind in ("canceled", "expired"):
            a = self.read(out / "groups.json", self.results(failed_line(k["a"], kind)))
            self.assertEqual((a["errored"], a["opened"]), (1, []))
        # A verdict for no rule of the group, or of another shape, is unreadable.
        a = self.read(out / "groups.json", self.results(succeeded(k["a"], {"rules": [{"rule": "R1"}]})))
        self.assertEqual((a["unreadable"], a["opened"]), (1, []))
        # An errored line then a succeeded line of one key: the succeeded one is read.
        res2 = self.results(failed_line(k["a"]), succeeded(k["a"], untested))
        a = self.read(out / "groups.json", res2)
        self.assertEqual((a["read"], a["duplicate"], a["errored"]), (1, 1, 0))
        self.assertEqual(a["opened"], [fkey(sig("specs/a.md", 1))])
        # The builder given the first file settles only the succeeded keys.
        v, g2, r2, _ = self.build({OWNER: repo}, extra=["--results", res])
        self.assertEqual((v["settled"], v["requested"], v["result_lines_skipped"]), (4, 1, 1))
        self.assertEqual([x["custom_id"] for x in r2], [k["a"]])
        self.assertEqual({x["path"][6]: x["state"] for x in g2["groups"]},
                         {"a": "requested", "b": "settled", "c": "settled", "d": "settled", "e": "settled"})
        v, _, r3, _ = self.build({OWNER: repo}, extra=["--results", res, "--results", res2])
        self.assertEqual((v["settled"], r3), (5, []))

    def test_cap_of_ten(self):
        repo = self.repo({"specs/a.md": spec(req(1, "alone.")),
                          "specs/b.md": spec(*(req(n, f"rule {n}.") for n in range(1, 14)))})
        _, g, _, out = self.build({OWNER: repo})
        groups = out / "groups.json"
        lines = {x["key"]: succeeded(x["key"], {"rules": []}) for x in g["groups"]}
        ka = self.group(g, "specs/a.md")["key"]
        only_a = self.results(lines[ka])
        every = self.results(*lines.values())
        a_key = fkey(sig("specs/a.md", 1))
        self.assertEqual(self.read(groups, only_a)["opened"], [a_key])
        self.set_state(a_key, "closed")
        self.assertEqual(self.read(groups, only_a)["reopened"], [a_key])
        # The same reopen again answers already_applied: counted so, in no list, no place under the cap.
        self.set_state(a_key, "closed")
        a = self.read(groups, every)
        self.assertEqual((a["already_applied"], a["reopened"], len(a["opened"]), a["over_cap"]), (1, [], 10, 3))
        self.assertEqual(a["opened"], [fkey(sig("specs/b.md", n)) for n in range(1, 11)])
        self.assertEqual(a["unjudged"], 0)
        a = self.read(groups, every)
        self.assertEqual((a["opened"], a["over_cap"], a["already_applied"]),
                         ([fkey(sig("specs/b.md", n)) for n in range(11, 14)], 0, 1))
        a = self.read(groups, every)
        self.assertEqual((a["opened"], a["over_cap"]), ([], 0))

    def test_one_result_file_under_three_commits(self):
        repo = self.repo({"specs/a.md": spec(req(1, "one."), req(2, "two.")),
                          "tests/t.sh": f"# {cite('specs/a.md', 2)}\necho two\n", "other.txt": "1\n"})
        k1, k2 = fkey(sig("specs/a.md", 1)), fkey(sig("specs/a.md", 2))
        self.seed(sig("specs/a.md", 2), "open")
        _, g1, _, out1 = self.build({OWNER: repo})
        key = g1["groups"][0]["key"]
        res = self.results(succeeded(key, verdict(("R1", "untested", [], "no test."),
                                                  ("R2", "tested", ["tests/t.sh:1"], "it fails."))))
        a = self.read(out1 / "groups.json", res)
        self.assertEqual((a["opened"], a["closed"], a["failed"]), ([k1], [k2], []))
        self.set_state(k2, "open")  # reopened by the Lead
        commits = []
        for n in (2, 3):
            commits.append(repo.commit({"other.txt": f"{n}\n"}))
            _, g, _, out = self.build({OWNER: repo})
            self.assertEqual(g["groups"][0]["key"], key)
            a = self.read(out / "groups.json", res)
            self.assertEqual(a["failed"], [])
            self.assertEqual(a["seen"], [k1])
            self.assertEqual(a["closed"], [k2] if n == 2 else [])
        f = self.state()["findings"]
        self.assertTrue(f[k1]["evidence"].startswith(f"key {key} commit {commits[-1]}"))
        self.assertEqual(f[k2]["state"], "closed")
        self.assertEqual(f[k2]["reason"], f"not shown by batch drift, key {key}, commit {commits[0]}")

    def scenario(self):
        """Every branch of the pass; returns the keys by name and the two answers."""
        s = "specs/a.md"
        tests = "".join(f"# {cite(s, n)}\necho {n}\n" for n in (2, 3, 4, 7, 8, 9, 10) for _ in (0, 1))
        tests += f"# {cite(s, 11)}\necho a\n# {cite(s, 11)}\necho b\n"
        repo = self.repo({s: spec(*(req(n, f"rule {n}.") for n in range(1, 12))), "tests/t.sh": tests})
        t = lambda n: [x for x in self.rules[f"R{n}"]]  # noqa: E731
        _, g, _, out = self.build({OWNER: repo})
        self.rules = {x["rule"]: x["tests"] for x in g["groups"][0]["rules"]}
        key = g["groups"][0]["key"]
        K = {n: fkey(sig(s, n)) for n in range(1, 12)}
        for n, state in ((3, "open"), (4, "planned"), (5, "closed"), (6, "dismissed"), (7, "open"), (8, "open"),
                         (9, "planned"), (10, "open")):
            self.seed(sig(s, n), state)
        K["old1"], K["old2"], K["a99"] = fkey(sig("specs/old.md", 1)), fkey(sig("specs/old.md", 2)), fkey(sig(s, 99))
        K["other_repo"], K["other_detector"] = fkey(sig(s, 1, "x/y")), "watch-drift-0123456789abcdef"
        self.seed(sig("specs/old.md", 1), "open")
        self.seed(sig("specs/old.md", 2), "closed")
        self.seed(sig(s, 99), "planned", rest="miscited: 1 of 2 citing tests do not test this requirement")
        self.seed(sig(s, 1, "x/y"), "open")
        self.seed(sig("specs/old.md", 3), "open", detector="watch:drift", key=K["other_detector"])
        v = verdict(("R1", "tested", [], "says tested, but no test cites it."),
                    ("R2", "untested", [], "the tests check another thing."),
                    ("R3", "untested", [], "no."),
                    ("R4", "miscited", [t(4)[0]], "one does not.\nreally\x07."),
                    ("R7", "tested", t(7), "yes."),
                    ("R8", "miscited", ["tests/zzz.sh:1"], "a test of no rule."),
                    ("R10", "tested", [], "twice."), ("R10", "untested", [], "twice."),
                    ("R11", "miscited", [t(11)[0], "tests/zzz.sh:9"], "one of two."),
                    ("R40", "untested", [], "a rule outside the group."))
        res = self.results(succeeded(key, v))
        before = self.state()["findings"]
        first = self.read(out / "groups.json", res)
        second = self.read(out / "groups.json", res)
        return K, before, first, second, out, repo, key

    def test_closes_when_no_longer_shown(self):
        K, before, a, b, out, repo, key = self.scenario()
        self.assertEqual(a["opened"], [K[1], K[2], K[11]])
        self.assertEqual(a["seen"], [K[3], K[4]])
        self.assertEqual(a["reopened"], [K[5]])
        self.assertEqual(a["closed"], [K[7], K["a99"], K["old1"]])
        self.assertEqual((a["dismissed"], a["unjudged"], a["failed"], a["read"]), (1, 3, [], 1))
        f = self.state()["findings"]
        self.assertEqual(f[K[11]]["summary"],
                         f"{sig('specs/a.md', 11)} miscited: 1 of 2 citing tests do not test this requirement")
        self.assertEqual(f[K[2]]["summary"], f"{sig('specs/a.md', 2)} untested: no citing test tests this requirement")
        self.assertIn("tests/t.sh:", f[K[4]]["evidence"])
        self.assertIn("one does not. really .", f[K[4]]["evidence"])
        self.assertTrue(f[K[1]]["evidence"].startswith(f"key {key} commit {repo.head()}"))
        self.assertEqual(f[K[7]]["reason"], f"not shown by batch drift, key {key}, commit {repo.head()}")
        self.assertEqual(f[K["old1"]]["reason"], f"the requirement is not in the specs at {repo.head()}")
        for name in (6, 8, 9, 10, "old2", "other_repo", "other_detector"):
            self.assertEqual(f[K[name]], before[K[name]], name)
        # The same file again: nothing changes.
        self.assertEqual((b["opened"], b["seen"], b["reopened"], b["closed"], b["dismissed"]), ([], [], [], [], 1))
        n = len(self.applies())
        # A requirement that left the specs is closed also by a result file without a read line.
        self.seed(sig("specs/old.md", 4), "open")
        c = self.read(out / "groups.json", self.results(failed_line(key)))
        self.assertEqual((c["read"], c["errored"], c["closed"]), (0, 1, [fkey(sig("specs/old.md", 4))]))
        self.assertEqual(len(self.applies()), n + 1)
        # A store that answers unqualified is store_failed before any action.
        st = self.state()
        st["unqualified"] = True
        self.put(st)
        self.assertEqual(self.read(out / "groups.json", self.results(), code=3), {"status": "store_failed"})
        self.assertEqual(len(self.applies()), n + 1)

    def test_refused_apply_is_failed_and_the_pass_goes_on(self):
        repo = self.repo({"specs/a.md": spec(req(1, "one."), req(2, "two."))})
        _, g, _, out = self.build({OWNER: repo})
        res = self.results(succeeded(g["groups"][0]["key"], {"rules": []}))
        k1, k2 = fkey(sig("specs/a.md", 1)), fkey(sig("specs/a.md", 2))
        a = self.read(out / "groups.json", res)
        self.assertEqual(a["opened"], [k1, k2])
        # The request id of k1's open is now kept with other content: refused request_id_reused.
        st = self.state()
        del st["findings"][k1]
        rid = next(e["body"]["request_id"] for e in st["log"] if e["body"] and e["body"].get("parameters", {})
                   .get("finding", {}).get("native_key") == k1)
        st["rids"][rid] = "other"
        st["findings"][k2]["state"] = "closed"
        self.put(st)
        a = self.read(out / "groups.json", res, code=1)
        self.assertEqual((a["failed"], a["reopened"]), ([{"key": k1, "code": "request_id_reused"}], [k2]))
        # A refusal by a criterion is failed with the criterion's code (the search showed a state the apply no
        # longer finds).
        st = self.state()
        st["findings"][k1] = dict(st["findings"][k2], state="closed")
        st["shown"] = {k1: dict(st["findings"][k2], state="open", evidence="x")}
        self.put(st)
        a = self.read(out / "groups.json", res, code=1)
        self.assertEqual(a["failed"], [{"key": k1, "code": "finding_not_live"}])
        # Bad groups and usage.
        bad = self.path("g.json")
        bad.write_text('{"job":"drift"}')
        self.assertEqual(self.read(bad, res, code=2), {"status": "groups_unreadable"})
        self.assertEqual(self.read(out / "groups.json", self.tmp / "none.jsonl", code=2),
                         {"status": "results_unreadable"})
        rc, v = self.run_prog(READ, ["--groups"], {"PATH": str(self.bin), "HOME": HOME, "ONT_BIN": str(self.bin / "ont")})
        self.assertEqual((rc, v), (2, {"status": "usage"}))

    def test_actions_fit_the_schema(self):
        self.scenario()
        schema = json.loads((REPO / "ontology" / "work.schema.json").read_text())
        host = (REPO / "crates" / "ap" / "src" / "host.rs").read_text()
        m = re.search(r'fn id\(kind: &str, key: &str\) -> Value \{\s*json!\(\{"object_type":kind,"namespace":(\{[^}]*\})',
                      host)
        self.assertIsNotNone(m)
        namespace = json.loads(m.group(1))
        self.assertEqual(LIB.NAMESPACE, namespace)
        types = schema["value_types"]
        seen = set()
        applies = self.applies()
        self.assertTrue(applies)
        for e in applies:
            body = e["body"]
            self.assertEqual(set(body), {"action_type", "parameters", "agent", "request_id", "role"})
            self.assertEqual(body["role"], "api")
            self.assertEqual(body["agent"], "batch-drift")
            self.assertRegex(body["request_id"], r"^bd-[0-9a-f]{40}$")
            action = schema["action_types"][body["action_type"]]
            seen.add(body["action_type"])
            declared = action["parameters"]
            p = body["parameters"]
            self.assertLessEqual(set(p), set(declared))
            self.assertLessEqual({k for k, d in declared.items() if d["required"]}, set(p))
            for name, value in p.items():
                t = declared[name]["type"]
                if isinstance(t, dict) and "ref" in t:
                    self.assertEqual(value, {"object_type": t["ref"], "namespace": namespace,
                                             "native_key": value["native_key"]})
                    self.assertRegex(value["native_key"], r"^batch-drift-[0-9a-f]{16}$")
                elif isinstance(t, dict) and "value_type" in t:
                    self.assertIn(value, types[t["value_type"]]["enum"])
                else:
                    self.assertEqual(t, "string")
                    self.assertTrue(value)
                    self.assertLessEqual(len(value.encode("utf-8")), declared[name]["max_bytes"])
            if body["action_type"] == "open_finding":
                self.assertEqual((p["kind"], p["detector"], p["severity"]), ("drift", "batch:drift", "info"))
        self.assertEqual(seen, {"open_finding", "see_finding_again", "reopen_finding", "close_finding"})


if __name__ == "__main__":
    unittest.main()
