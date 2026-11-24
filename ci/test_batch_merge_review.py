"""Tests of evals/batch/merge_review/build and evals/batch/merge_review/read, run as programs against scratch
repositories made by `git init`, a fake `ont` and a fake `gh` (no model, no network, no engine, no cargo, no store of
the host). Contract: evals/batch/merge_review/README.md."""
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
MR = BATCH / "merge_review"
BUILD = MR / "build"
READ = MR / "read"
PY = sys.executable
HOME = os.environ.get("HOME", "/")
OWNER = "o/n"
SINCE, UNTIL = "2026-11-16", "2026-11-22"
MODEL = "claude-opus-5-5"
STRONG = "claude-fable-5-2"
MARKER = "SECRET-MARKER-" + "9b4e2a"

GIT_ID = ["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "-c", "commit.gpgsign=false",
          "-c", "init.defaultBranch=main"]


def req(n, text):
    return "- R" + str(n) + ": " + text


def spec(*lines):
    return "# A spec\n\n## Requirements\n\n" + "".join(x + "\n" for x in lines)


def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    mod = importlib.util.module_from_spec(importlib.util.spec_from_loader(name, loader))
    loader.exec_module(mod)
    return mod


LIB = load("batch_lib", BATCH / "lib.py")


def sha(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def canon(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def oid(kind, key):
    return {"object_type": kind, "namespace": LIB.NAMESPACE, "native_key": key}


def sig(pr, repo=OWNER):
    return f"{repo}#{pr}"


def fkey(signature):
    return "batch-merge-review-" + sha(signature)[:16]


def defect(title="Wrong exit code.", where="prog.py:2", clause="specs/prog.md#R1",
           command="python3 prog.py --bad", why="It exits 0 instead of 2."):
    return {"title": title, "where": where, "clause": clause, "command": command, "why": why}


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


# The fake engine: objects by type in ont-state.json beside its directory; records argv, body and ONT_ROLE.
FAKE_ONT = r'''#!@PY@ -I
import json, os, sys
STATE = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "ont-state.json")
with open(STATE) as f:
    st = json.load(f)
args = sys.argv[1:]
raw = sys.stdin.read()
body = json.loads(raw) if raw else None
st["log"].append({"argv": list(args), "body": body, "ont_role": os.environ.get("ONT_ROLE")})
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
def fields_of(f):
    return {p: {"kind": "value", "value": v} for p, v in f.items()}
if args == ["search"]:
    kind = body["object_type"]
    if st.get("unqualified") in (True, kind):
        out({"status": "ok", "result": {"objects": [], "basis": {"qualified": False}, "continuation": None}})
    if st.get("search_fails") in (True, kind):
        out({"status": "refused", "code": "store_unavailable"}, 3)
    if st.get("repeat_cont") in (True, kind):
        out({"status": "ok", "result": {"objects": [], "basis": {"qualified": True}, "continuation": "loop"}})
    rows = sorted(st["objects"].get(kind, {}).items())
    for c in body["where"]["all"]:
        if "eq" in c:
            rows = [(k, f) for k, f in rows if f.get(c["eq"][0]) == c["eq"][1]]
        elif "in" in c:
            rows = [(k, f) for k, f in rows if f.get(c["in"][0]) in c["in"][1]]
        else:
            out({"status": "refused", "code": "bad_filter"}, 2)
    start = int(body.get("continuation") or 0)
    page = min(int(body.get("limit", 15)), st.get("page", 4))
    chunk = rows[start:start + page]
    nxt = str(start + page) if start + page < len(rows) else None
    objs = [{"id": {"object_type": kind, "native_key": k}, "head": {"fields": fields_of(f)}} for k, f in chunk]
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
    findings = st["objects"].setdefault("finding", {})
    f = findings.get(key)
    def crit(c):
        out({"status": "refused", "code": "submission_criteria", "failed": [c]}, 2)
    if st.get("refuse_open") and a == "open_finding" and key == st["refuse_open"]:
        crit("detector_unknown")
    if a == "open_finding":
        if f:
            out({"status": "refused", "code": "revision_conflict"}, 2)
        f = {k: p[k] for k in ("kind", "detector", "summary", "evidence", "severity") if k in p}
        f["state"] = "open"
    elif a == "see_finding_again":
        if not f or f["state"] not in ("open", "planned"):
            crit("finding_not_live")
        if "evidence" in p:
            f["evidence"] = p["evidence"]
        if "severity" in p:
            f["severity"] = p["severity"]
    else:
        out({"status": "refused", "code": "unknown_action"}, 2)
    findings[key] = f
    st["rids"][rid] = content
    out({"status": "ok", "result": {"outcome": "applied", "edits": [{"change": "modify", "id": p["finding"]}]}})
out({"status": "refused", "code": "usage"}, 2)
'''

# The fake gh: records every call and keeps comments per "<repo>#<pr>" in gh-state.json.
FAKE_GH = r'''#!@PY@ -I
import json, os, sys
STATE = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "gh-state.json")
with open(STATE) as f:
    st = json.load(f)
args = sys.argv[1:]
stdin = sys.stdin.read() if args[:2] == ["pr", "comment"] else ""
st["calls"].append({"argv": list(args), "stdin": stdin})
def save():
    with open(STATE + ".tmp", "w") as f:
        json.dump(st, f)
    os.replace(STATE + ".tmp", STATE)
if len(args) == 7 and args[:2] == ["pr", "view"] and args[3] == "--repo" and args[5:] == ["--json", "comments"]:
    pr_key = f"{args[4]}#{args[2]}"
    if pr_key in st.get("fail_view", []):
        save()
        sys.exit(1)
    if pr_key in st.get("bad_json", []):
        save()
        print("not-json")
        sys.exit(0)
    if pr_key in st.get("no_comments_key", []):
        save()
        print(json.dumps({"other": 1}))
        sys.exit(0)
    comments = st["comments"].get(pr_key, [])
    save()
    print(json.dumps({"comments": [{"body": b} for b in comments]}))
    sys.exit(0)
if len(args) == 7 and args[:2] == ["pr", "comment"] and args[3] == "--repo" and args[5:] == ["--body-file", "-"]:
    pr_key = f"{args[4]}#{args[2]}"
    if pr_key in st.get("fail_comment", []):
        save()
        sys.exit(1)
    st["comments"].setdefault(pr_key, []).append(stdin)
    save()
    print("https://example.invalid/comment/1")
    sys.exit(0)
save()
sys.exit(2)
'''

# Logging git wrapper: logs every git call and delegates to the real git binary.
GIT_WRAP = r'''#!@PY@ -I
import json, os, subprocess, sys
LOG = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "git-log.jsonl")
with open(LOG, "a") as f:
    f.write(json.dumps(sys.argv[1:]) + "\n")
os.execv("@GIT@", ["@GIT@"] + sys.argv[1:])
'''


class Repo:
    """A scratch repository with helper methods to commit and make real merge commits."""

    def __init__(self, root, files):
        self.root = root
        root.mkdir(parents=True)
        self.git("init", "-q")
        self.commit(files, "init")

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
            if isinstance(text, bytes):
                p.write_bytes(text)
            else:
                p.write_text(text)

    def commit(self, files, msg="c"):
        self.write(files)
        self.git("add", "-A")
        self.git("commit", "-q", "--allow-empty", "-m", msg)
        return self.head()

    def merge_branch(self, branch, files, msg="merge"):
        """Make a branch from HEAD, commit `files`, merge --no-ff back into main; return merge commit SHA."""
        self.git("checkout", "-q", "-b", branch)
        self.commit(files, f"branch {branch}")
        self.git("checkout", "-q", "main")
        self.git("merge", "-q", "--no-ff", branch, "-m", msg)
        return self.head()

    def head(self):
        return self.git("rev-parse", "HEAD").strip()


class Case(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.tmp = Path(os.path.realpath(tmp.name))
        self.bin = self.tmp / "bin"
        self.bin.mkdir()
        (self.bin / "ont").write_text(FAKE_ONT.replace("@PY@", PY))
        (self.bin / "ont").chmod(0o755)
        (self.bin / "gh").write_text(FAKE_GH.replace("@PY@", PY))
        (self.bin / "gh").chmod(0o755)
        real_git = shutil.which("git")
        self.assertIsNotNone(real_git)
        (self.bin / "git").write_text(GIT_WRAP.replace("@PY@", PY).replace("@GIT@", real_git))
        (self.bin / "git").chmod(0o755)
        self.put_ont({"objects": {}, "rids": {}, "log": []})
        self.put_gh({"comments": {}, "calls": []})
        self.n = 0

    def path(self, name):
        self.n += 1
        return self.tmp / f"{self.n}-{name}"

    def repo(self, files):
        return Repo(self.path("clone"), files)

    def ont_state(self):
        return json.loads((self.tmp / "ont-state.json").read_text())

    def put_ont(self, st):
        (self.tmp / "ont-state.json").write_text(json.dumps(st))

    def set_ont(self, **flags):
        st = self.ont_state()
        st.update(flags)
        self.put_ont(st)

    def gh_state(self):
        return json.loads((self.tmp / "gh-state.json").read_text())

    def put_gh(self, st):
        (self.tmp / "gh-state.json").write_text(json.dumps(st))

    def set_gh(self, **flags):
        st = self.gh_state()
        st.update(flags)
        self.put_gh(st)

    def git_log(self):
        p = self.tmp / "git-log.jsonl"
        return [json.loads(line) for line in p.read_text().splitlines()] if p.exists() else []

    def add(self, otype, key, **fields):
        st = self.ont_state()
        st["objects"].setdefault(otype, {})[key] = {k: v for k, v in fields.items() if v is not None}
        self.put_ont(st)

    def intent(self, key, directory="/w/i1", status="confirmed"):
        self.add("intent", key, directory=directory, status=status)

    def unit(self, key, commit, pr=1, repository=OWNER, intent="i1", cls="plain", lane="objective",
             state="merged", ended_at="2026-11-18T10:00:00Z", objective="Keep the contract.",
             verifier_model=None, **extra):
        fields = {"state": state, "repository": repository, "pull_request": pr, "merged_commit": commit,
                  "ended_at": ended_at, "class": cls, "lane": lane, "objective": objective,
                  "intent": oid("intent", intent) if intent else None}
        if verifier_model is not None:
            fields["verifier"] = {"runtime": "claude", "model": verifier_model, "effort": "high"}
        fields.update(extra)
        self.add("unit", key, **fields)

    def applies(self):
        return [e for e in self.ont_state()["log"] if "action" in e["argv"] and "apply" in e["argv"]]

    def env(self, **extra):
        e = {"PATH": str(self.bin), "HOME": HOME, "ONT_BIN": str(self.bin / "ont"), "GH_BIN": str(self.bin / "gh")}
        e.update(extra)
        return e

    def run_prog(self, prog, args, code=0, env=None):
        env = self.env() if env is None else env
        self.assertTrue(os.path.isabs(env["ONT_BIN"]) and os.path.isabs(env["GH_BIN"]))
        for d in env.get("PATH", "").split(os.pathsep):
            if not d:
                continue
            for name, var in (("ont", "ONT_BIN"), ("gh", "GH_BIN")):
                cand = os.path.join(d, name)
                if os.path.exists(cand):
                    self.assertTrue(os.path.samefile(cand, env[var]), (cand, env[var]))
        p = subprocess.run([PY, "-I", "-B", str(prog), *map(str, args)], capture_output=True, text=True, env=env)
        self.assertEqual(p.stderr, "", p.stderr)
        lines = p.stdout.splitlines()
        self.assertEqual(len(lines), 1, p.stdout)
        self.assertEqual(p.returncode, code, p.stdout)
        return json.loads(lines[0])

    def build(self, repos, out=None, since=SINCE, until=UNTIL, model=MODEL, strong=STRONG, extra=(), code=0, env=None):
        out = out or self.path("out")
        args = []
        for name, repo in repos.items():
            args += ["--repo", f"{name}={repo.root if isinstance(repo, Repo) else repo}"]
        args += ["--since", since, "--until", until, "--model", model, "--strong", strong, "--out", out, *extra]
        v = self.run_prog(BUILD, args, code=code, env=env)
        if code:
            return v
        self.assertEqual(v["status"], "ok")
        items = json.loads((Path(out) / "items.json").read_text())
        rm = [json.loads(x) for x in (Path(out) / "requests-model.jsonl").read_text().splitlines()]
        rs = [json.loads(x) for x in (Path(out) / "requests-strong.jsonl").read_text().splitlines()]
        return v, items, rm, rs, Path(out)

    def read(self, items, results, extra=(), code=0, env=None):
        res_args = []
        for r in (results if isinstance(results, (list, tuple)) else [results]):
            res_args += ["--results", r]
        return self.run_prog(READ, ["--items", items, *res_args, *extra], code=code, env=env)

    def results(self, *lines):
        p = self.path("results.jsonl")
        p.write_text("".join(x + "\n" for x in lines))
        return p


class BatchMergeReviewTest(Case):
    def test_planted_defect_gives_one_finding(self):
        r1 = req(1, "The program exits 2 when --bad is passed.")
        r2 = req(2, "The program prints ok when no flag is passed.")
        repo = self.repo({
            "specs/prog.md": spec(r1, r2),
            "prog.py": "import sys\nif '--bad' in sys.argv:\n    sys.exit(2)\nprint('ok')\n",
        })
        mc = repo.merge_branch("unit/u1", {
            "prog.py": "import sys\nif '--bad' in sys.argv:\n    sys.exit(0)\nprint('ok')\n",
        })
        self.intent("i1")
        self.unit("u1", mc, pr=42, objective="Keep prog.py exiting 2 on --bad.",
                  interface="CLI flag --bad exits 2.", invariants="No network.",
                  acceptance=[{"check": "python3 prog.py", "kind": "invariant"}],
                  not_built="No flags beyond --bad.", write_set=["prog.py"],
                  read_first=["prog.py"], specs=["specs/prog.md"], notes="Small fix.",
                  verifier_questions="Does --bad still exit 2?")

        v, items, rm, rs, out = self.build({OWNER: repo})
        self.assertEqual((v["items"], v["requested"], v["settled"], v["too_large"], v["over_max"],
                          v["protected"], v["academy_skipped"], v["academy_paths"], v["repo_not_given"],
                          v["commit_missing"], v["result_lines_skipped"]),
                         (1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0))
        self.assertEqual(len(rm), 1)
        self.assertEqual(rs, [])
        p = rm[0]["params"]
        self.assertEqual(set(p), {"model", "max_tokens", "system", "messages", "output_config"})
        self.assertEqual(p["model"], MODEL)
        self.assertEqual(p["max_tokens"], 8000)
        self.assertEqual(p["system"], (MR / "prompt.md").read_text())
        self.assertEqual(p["output_config"], {"format": {"type": "json_schema",
                                                         "schema": json.loads((MR / "verdict.schema.json").read_text())}})
        text = p["messages"][0]["content"]
        for needle in (OWNER, "42", "u1", mc, "Keep prog.py exiting 2 on --bad.", "CLI flag --bad exits 2.",
                       "No network.", "python3 prog.py", "No flags beyond --bad.", "Small fix.",
                       "Does --bad still exit 2?", "specs/prog.md", r1, r2, "-    sys.exit(2)", "+    sys.exit(0)"):
            self.assertIn(needle, text)
        # Order check: header -> page members -> spec requirements -> diff
        self.assertLess(text.index("objective"), text.index("specs/prog.md"))
        self.assertLess(text.index(r1), text.index("+    sys.exit(0)"))
        # No time or host path in params.
        self.assertNotIn("2026-11-18", canon(p))
        self.assertNotIn(str(self.tmp), canon(p))

        item = items["items"][0]
        self.assertEqual(item["key"], rm[0]["custom_id"])
        self.assertEqual(item["state"], "requested")
        self.assertEqual(item["specs"], ["specs/prog.md"])
        res = self.results(succeeded(item["key"], {"defects": [defect()]}))
        a = self.read(out / "items.json", res, env=self.env(ONT_ROLE="lead"))
        fk = fkey(sig(42))
        self.assertEqual((a["status"], a["lines"], a["read"], a["defects"], a["opened"], a["seen"],
                          a["left"], a["over_cap"], a["already_applied"], a["commented"],
                          a["comment_present"], a["failed"]),
                         ("ok", 1, 1, 1, [fk], [], 0, 0, 0, [item["key"]], 0, []))
        f = self.ont_state()["objects"]["finding"][fk]
        self.assertEqual((f["kind"], f["detector"], f["severity"], f["state"]),
                         ("drift", "batch:merge-review", "info", "open"))
        self.assertEqual(f["summary"],
                         f"{OWNER}#42 (unit u1, {mc[:12]}): 1 defects claimed against its page by {MODEL}, none reproduced")
        self.assertTrue(f["evidence"].startswith(f"key {item['key']}; 1. Wrong exit code."))
        comments = self.gh_state()["comments"][f"{OWNER}#42"]
        self.assertEqual(len(comments), 1)
        self.assertTrue(comments[0].startswith(f"<!-- batch:merge-review {OWNER}#42 -->\n"))
        self.assertIn("A claimed defect stays a finding of severity info until its command reproduces it at the merged commit.",
                      comments[0])

        # Second build given that result file requests nothing.
        v2, items2, rm2, rs2, out2 = self.build({OWNER: repo}, extra=["--results", res])
        self.assertEqual((v2["requested"], v2["settled"]), (0, 1))
        self.assertEqual((rm2, rs2), ([], []))
        self.assertEqual((out2 / "requests-model.jsonl").read_bytes(), b"")
        self.assertEqual((out2 / "requests-strong.jsonl").read_bytes(), b"")
        self.assertEqual(items2["items"][0]["state"], "settled")

        # Second read applies nothing and posts nothing.
        applies_before = len(self.applies())
        gh_comments_before = len(self.gh_state()["comments"][f"{OWNER}#42"])
        a2 = self.read(out / "items.json", res, env=self.env(ONT_ROLE="lead"))
        self.assertEqual((a2["opened"], a2["seen"], a2["commented"], a2["comment_present"], a2["failed"]),
                         ([], [], [], 1, []))
        self.assertEqual(len(self.applies()), applies_before)
        self.assertEqual(len(self.gh_state()["comments"][f"{OWNER}#42"]), gh_comments_before)

    def test_model_other_than_verifier(self):
        repo = self.repo({"src/a.py": "x = 1\n", "ci/check.py": "y = 1\n"})
        plain_c = repo.merge_branch("b-plain", {"src/a.py": "x = 2\n"})
        prot_c = repo.merge_branch("b-prot", {"ci/check.py": "y = 2\n"})
        self.intent("i1")
        cases = [
            ("u1", plain_c, 1, None, "model", False),
            ("u2", plain_c, 2, "sonnet", "model", False),
            ("u3", plain_c, 3, "opus", "strong", False),
            ("u4", plain_c, 4, "claude-opus-5-5", "strong", False),
            ("u5", plain_c, 5, "fable", "model", False),
            ("u6", prot_c, 6, None, "strong", True),
            ("u7", prot_c, 7, "sonnet", "strong", True),
            ("u8", prot_c, 8, "opus", "strong", True),
            ("u9", prot_c, 9, "fable", "model", True),
            ("u10", prot_c, 10, "claude-fable-5-2", "model", True),
        ]
        for idx, (uk, cm, pr, vm, _, _) in enumerate(cases, 1):
            self.unit(uk, cm, pr=pr, ended_at=f"2026-11-18T10:00:{idx:02d}Z", verifier_model=vm)

        v, items, rm, rs, out = self.build({OWNER: repo})
        self.assertEqual((v["items"], v["requested"], v["protected"]), (10, 10, 5))
        by_u = {it["unit"]: it for it in items["items"]}
        for uk, _, _, _, want_reader, want_prot in cases:
            self.assertEqual((by_u[uk]["reader"], by_u[uk]["protected"]), (want_reader, want_prot), uk)
        self.assertEqual(len(rm), 5)
        self.assertEqual(len(rs), 5)
        self.assertEqual({r["params"]["model"] for r in rm}, {MODEL})
        self.assertEqual({r["params"]["model"] for r in rs}, {STRONG})
        # Usage for two models of one family or invalid identifiers.
        for m, s in ((MODEL, "claude-opus-5-6"), ("gpt-5", STRONG), (MODEL, "claude-fable"),
                     ("claude-opus-", STRONG), ("", STRONG)):
            self.assertEqual(self.build({OWNER: repo}, model=m, strong=s, code=2), {"status": "usage"}, (m, s))

    def test_protected_paths_are_drive_rs(self):
        drive = (REPO / "crates" / "ap" / "src" / "unit" / "drive.rs").read_text()
        common = load("mr_common", MR / "common.py")
        for name in ("PROTECTED_EXACT", "PROTECTED_NAMES", "PROTECTED_CONFIGS", "PROTECTED_TREES"):
            m = re.search(rf"const\s+{name}\s*:\s*\[&str;\s*\d+\]\s*=\s*\[([^\]]+)\];", drive, re.S)
            self.assertIsNotNone(m, name)
            want = tuple(re.findall(r'"([^"]+)"', m.group(1)))
            self.assertEqual(tuple(getattr(common, name)), want, name)
        protected_examples = [
            "check", "REVIEW.md", "CI/x", ".GitHub/workflows/rust.yml", "rust-toolchain",
            "sub/rust-toolchain.toml", "clippy.toml", "a/.clippy.toml", "rustfmt.toml", "a/.rustfmt.toml",
            "crates/ap/build.rs", "Cargo.toml", "crates/ap/Cargo.toml", "Cargo.lock",
            ".cargo/config", "sub/.cargo/config.toml", ".cargo", "ci/test.py", ".github/x",
            "specs/ap/unit.md", "scripts/install-ap.sh", "deploy/x", "etc/x", "content/AGENTS.md",
            "github/apply", "ontology/rehearsal.sh", "ontology/ENGINE-PIN", "ontology/work.schema.json",
            "ontology/migrate",
        ]
        unprotected_examples = [
            "crates/ap/src/main.rs", "evals/batch/merge_review/build", "README.md", "map.json",
            "sub/check", "sub/REVIEW.md", "src/ci/x.rs", "src/specs/a.md",
        ]
        for p in protected_examples:
            self.assertTrue(common.protected(p), p)
        for p in unprotected_examples:
            self.assertFalse(common.protected(p), p)

    def test_academy_is_never_read(self):
        repo = self.repo({
            "src/ok.py": "a = 1\n",
            "evals/lesson/secret.sh": "echo 1\n",
            "specs/good.md": spec(req(1, "Good rule.")),
            "specs/edu-secret.md": spec(req(1, f"Hidden {MARKER}.")),
        })
        c_mixed = repo.merge_branch("b-mixed", {
            "src/ok.py": "a = 2\n",
            "evals/lesson/secret.sh": f"echo {MARKER}\n",
        })
        c_only_acad = repo.merge_branch("b-only-acad", {
            "evals/lesson/secret.sh": f"echo only-{MARKER}\n",
        })
        c_acad_unit = repo.merge_branch("b-acad-unit", {
            "src/ok.py": f"# {MARKER}\na = 3\n",
        })
        self.intent("i1", directory="/w/i1")
        self.intent("iDir", directory="/w/edu-math/task")
        self.intent("iAllAcad", directory="/w/plain")
        self.intent("iMixed", directory="/w/plain2")
        # Academy units: repo name, intent directory, intent whose units are all Academy.
        self.unit("uAcadRepo", c_acad_unit, pr=1, repository="o/education-platform", intent="i1",
                  objective=f"Repo {MARKER}")
        self.unit("uAcadDir", c_acad_unit, pr=2, repository=OWNER, intent="iDir",
                  objective=f"Dir {MARKER}")
        self.unit("uAcadIntentA", c_acad_unit, pr=3, repository="o/edu-app", intent="iAllAcad",
                  objective=f"AllA {MARKER}")
        self.unit("uAcadIntentB", c_acad_unit, pr=4, repository=None, intent="iAllAcad",
                  objective=f"AllB {MARKER}")
        # Mixed intent: one Academy unit, one normal unit -> normal unit IS included.
        self.unit("uMixAcad", c_acad_unit, pr=5, repository="o/edu-other", intent="iMixed",
                  objective=f"MixA {MARKER}")
        self.unit("uMixOk", c_mixed, pr=6, repository=OWNER, intent="iMixed",
                  specs=["specs/good.md", "specs/edu-secret.md"],
                  notes="Also see specs/edu-secret.md", objective="Normal mixed unit.")
        # Ordinary unit whose EVERY changed path is Academy -> empty diff, no content git diff call!
        self.unit("uOnlyAcadPath", c_only_acad, pr=7, repository=OWNER, intent="i1",
                  objective="Only academy changed path.")

        v, items, rm, rs, out = self.build({OWNER: repo})
        # uAcadRepo, uAcadDir, uAcadIntentA, uAcadIntentB, uMixAcad = 5 academy_skipped
        self.assertEqual(v["academy_skipped"], 5)
        # academy_paths: uMixOk (evals/lesson/secret.sh + specs/edu-secret.md = 2) + uOnlyAcadPath (evals/lesson/secret.sh = 1) = 3
        self.assertEqual(v["academy_paths"], 3)
        self.assertEqual([it["unit"] for it in items["items"]], ["uMixOk", "uOnlyAcadPath"])
        self.assertEqual(items["items"][0]["specs"], ["specs/good.md"])
        # Marker must appear in NO written file and NO request.
        for p in out.iterdir():
            self.assertNotIn(MARKER, p.read_text())
        self.assertNotIn(MARKER, canon(rm + rs))
        # Verify no content `git diff` (without --name-only) ran for c_only_acad!
        content_diffs_for_only = [
            argv for argv in self.git_log()
            if "diff" in argv and "--name-only" not in argv and c_only_acad in argv
        ]
        self.assertEqual(content_diffs_for_only, [], content_diffs_for_only)
        # Also verify uOnlyAcadPath diff is empty in its request.
        only_req = next(r for r in rm if r["custom_id"] == items["items"][1]["key"])
        self.assertNotIn("evals/lesson/secret.sh", only_req["params"]["messages"][0]["content"])

        # Path arguments refused academy_path before any read, write, ont or gh call (strings naming nothing + symlinks).
        os.symlink(self.tmp / "edu-gone", self.tmp / "link-edu")
        os.symlink("education-gone", self.tmp / "link-rel")
        refused = {"status": "academy_path"}
        ont_before = len(self.ont_state()["log"])
        gh_before = len(self.gh_state()["calls"])
        plain_out = self.path("never-made")
        build_cases = [
            ({"edu-o/n": repo}, plain_out, []),
            ({"o/education-n": repo}, plain_out, []),
            ({OWNER: self.tmp / "edu-clone"}, plain_out, []),
            ({OWNER: self.tmp / "link-edu"}, plain_out, []),
            ({OWNER: self.tmp / "link-rel"}, plain_out, []),
            ({OWNER: repo}, self.tmp / "education-out", []),
            ({OWNER: repo}, self.tmp / "link-edu" / "out", []),
            ({OWNER: repo}, plain_out, ["--results", self.tmp / "evals" / "lesson" / "r.jsonl"]),
            ({OWNER: repo}, plain_out, ["--store", self.tmp / "edu-store"]),
        ]
        for repos, o, extra in build_cases:
            self.assertEqual(self.build(repos, out=o, extra=extra, code=2), refused, (repos, o, extra))
        res_ok = self.results()
        for it_p, res_p, extra in (
            (self.tmp / "edu-items.json", res_ok, []),
            (out / "items.json", self.tmp / "education-r.jsonl", []),
            (self.tmp / "link-edu" / "items.json", res_ok, []),
            (out / "items.json", res_ok, ["--store", self.tmp / "edu-store"]),
            (self.tmp / "x" / "evals" / "lesson" / "items.json", res_ok, []),
        ):
            self.assertEqual(self.read(it_p, res_p, extra=extra, code=2), refused, (it_p, res_p, extra))
        self.assertFalse(plain_out.exists())
        self.assertEqual(len(self.ont_state()["log"]), ont_before)
        self.assertEqual(len(self.gh_state()["calls"]), gh_before)
        # No directory whose name begins with edu- or education- was made.
        self.assertFalse(any(p.is_dir() and p.name.startswith(("edu-", "education-")) for p in self.tmp.rglob("*")))

    def test_clauses_the_page_names(self):
        trace = load("check_spec_trace", REPO / "ci" / "check-spec-trace.py")
        common = load("mr_common", MR / "common.py")
        self.assertEqual(common.REQ_RE.pattern, trace.REQ_RE.pattern)

        ra1, ra2 = req(1, "Alpha one."), req(2, "Alpha two.")
        rb1 = req(1, "Beta one.")
        repo = self.repo({
            "specs/b.md": spec(rb1),
            "specs/a.md": spec(ra1, ra2),
            "src/x.py": "x = 1\n",
        })
        # Also commit a symlink under specs/link.md to verify non-regular file is treated as missing.
        os.symlink("a.md", repo.root / "specs" / "link.md")
        repo.git("add", "specs/link.md")
        repo.git("commit", "-q", "-m", "add symlink")
        mc = repo.merge_branch("b-clauses", {"src/x.py": "x = 2\n"})
        self.intent("i1")
        self.unit("u1", mc, pr=1,
                  specs=["specs/b.md#R1", "specs/missing.md"],
                  objective="See specs/a.md and specs/b.md.",
                  read_first=["specs/link.md"],
                  acceptance=[{"check": "test -f specs/a.md", "kind": "invariant"}])

        v, items, rm, rs, out = self.build({OWNER: repo})
        want_specs = ["specs/a.md", "specs/b.md", "specs/link.md", "specs/missing.md"]
        self.assertEqual(items["items"][0]["specs"], want_specs)
        text = rm[0]["params"]["messages"][0]["content"]
        self.assertIn(ra1, text)
        self.assertIn(ra2, text)
        self.assertIn(rb1, text)
        # Byte order: specs/a.md before specs/b.md before specs/link.md before specs/missing.md
        pos = [text.index(f"### {sp}") for sp in want_specs]
        self.assertEqual(pos, sorted(pos))
        self.assertIn("### specs/link.md\nmissing", text)
        self.assertIn("### specs/missing.md\nmissing", text)

    def test_results_that_are_not_read_make_no_finding(self):
        repo = self.repo({"a.py": "x = 1\n"})
        commits = [repo.merge_branch(f"b{i}", {"a.py": f"x = {i + 2}\n"}) for i in range(7)]
        self.intent("i1")
        for i, cm in enumerate(commits, 1):
            self.unit(f"u{i}", cm, pr=i, ended_at=f"2026-11-18T10:00:{i:02d}Z")
        v, items, rm, rs, out = self.build({OWNER: repo})
        k = {it["unit"]: it["key"] for it in items["items"]}

        # Every non-read BD17 word -> no action and no comment!
        res = self.results(
            "not json at all",                                             # skipped
            succeeded("0" * 64, {"defects": [defect()]}),                  # stale
            failed_line(k["u1"], "errored"),                               # errored
            failed_line(k["u2"], "canceled"),                              # errored
            failed_line(k["u3"], "expired"),                               # errored
            succeeded(k["u4"], {"defects": [defect()]}, stop="refusal"),   # refused
            succeeded(k["u5"], {"defects": [defect()]}, stop="max_tokens"),  # unreadable (cut)
            succeeded(k["u6"], text='{"defects": ['),                      # unreadable (bad JSON)
            succeeded(k["u7"], {"defects": [{"title": "only title"}]}),    # unreadable (schema)
            failed_line(k["u1"], "expired"),                               # duplicate
        )
        a = self.read(out / "items.json", res)
        want = {"lines": 10, "read": 0, "refused": 1, "errored": 3, "unreadable": 3, "stale": 1,
                "duplicate": 1, "skipped": 1, "defects": 0, "opened": [], "seen": [], "left": 0,
                "over_cap": 0, "already_applied": 0, "commented": [], "comment_present": 0, "failed": []}
        self.assertEqual({x: a[x] for x in want}, want)
        self.assertEqual(self.applies(), [])
        self.assertEqual(self.gh_state()["calls"], [])

        # Errored line then succeeded line with 0 defects: succeeded is read, 0 findings, 1 "No defect claimed." comment.
        res2 = self.results(failed_line(k["u1"]), succeeded(k["u1"], {"defects": []}))
        a2 = self.read(out / "items.json", res2)
        self.assertEqual((a2["read"], a2["duplicate"], a2["defects"], a2["opened"], a2["commented"]),
                         (1, 1, 0, [], [k["u1"]]))
        self.assertEqual(self.applies(), [])
        self.assertIn("No defect claimed.", self.gh_state()["comments"][f"{OWNER}#1"][0])

        # Bad items.json, missing results, unqualified store -> exact refusals.
        bad = self.path("bad.json")
        bad.write_text('{"job":"merge-review"}')
        self.assertEqual(self.read(bad, res, code=2), {"status": "items_unreadable"})
        self.assertEqual(self.read(out / "items.json", self.tmp / "missing.jsonl", code=2),
                         {"status": "results_unreadable"})
        self.set_ont(unqualified="finding")
        self.assertEqual(self.read(out / "items.json", res2, code=3), {"status": "store_failed"})

    def test_cap_of_ten(self):
        repo = self.repo({"a.py": "x = 0\n"})
        self.intent("i1")
        for i in range(1, 14):
            cm = repo.merge_branch(f"b{i}", {"a.py": f"x = {i}\n"})
            self.unit(f"u{i:02d}", cm, pr=i, ended_at=f"2026-11-18T10:00:{i:02d}Z")
        _, items, _, _, out = self.build({OWNER: repo})
        # Give item 1 six defects to also test first-five truncation in evidence & comment vs total 6 in summary.
        lines = []
        for idx, it in enumerate(items["items"], 1):
            ds = [defect(title=f"Bug {j} on pr {idx}.") for j in range(1, 7 if idx == 1 else 2)]
            lines.append(succeeded(it["key"], {"defects": ds}))
        res = self.results(*lines)

        a1 = self.read(out / "items.json", res)
        want_first_10 = [fkey(sig(i)) for i in range(1, 11)]
        self.assertEqual((a1["read"], a1["defects"], len(a1["opened"]), a1["over_cap"],
                          len(a1["commented"]), a1["comment_present"], a1["failed"]),
                         (13, 18, 10, 3, 13, 0, []))
        self.assertEqual(a1["opened"], want_first_10)
        f1 = self.ont_state()["objects"]["finding"][fkey(sig(1))]
        self.assertIn(": 6 defects claimed against its page by ", f1["summary"])
        self.assertIn("5. Bug 5 on pr 1.", f1["evidence"])
        self.assertNotIn("Bug 6 on pr 1.", f1["evidence"])
        c1 = self.gh_state()["comments"][f"{OWNER}#1"][0]
        self.assertIn("6 defects", c1)
        self.assertIn("Bug 5 on pr 1.", c1)
        self.assertNotIn("Bug 6 on pr 1.", c1)

        # Second call opens the remaining 3 and posts no comment.
        a2 = self.read(out / "items.json", res)
        self.assertEqual((a2["opened"], a2["over_cap"], a2["commented"], a2["comment_present"]),
                         ([fkey(sig(i)) for i in range(11, 14)], 0, [], 13))

        # Finding states: open+act, planned, closed, dismissed -> left; open+info changed evidence -> see_finding_again (no severity).
        st = self.ont_state()
        st["objects"]["finding"][fkey(sig(1))]["severity"] = "act"
        st["objects"]["finding"][fkey(sig(2))]["state"] = "planned"
        st["objects"]["finding"][fkey(sig(3))]["state"] = "closed"
        st["objects"]["finding"][fkey(sig(4))]["state"] = "dismissed"
        st["objects"]["finding"][fkey(sig(5))]["evidence"] = "old evidence"
        # Unrelated detector stays untouched.
        st["objects"]["finding"]["batch-drift-0123456789abcdef"] = {
            "kind": "drift", "detector": "batch:drift", "summary": "x", "evidence": "keep", "severity": "info", "state": "open"
        }
        self.put_ont(st)
        before_act = dict(self.ont_state()["objects"]["finding"][fkey(sig(1))])
        a3 = self.read(out / "items.json", res)
        self.assertEqual((a3["opened"], a3["seen"], a3["left"]), ([], [fkey(sig(5))], 4))
        self.assertEqual(self.ont_state()["objects"]["finding"][fkey(sig(1))], before_act)
        self.assertEqual(self.ont_state()["objects"]["finding"]["batch-drift-0123456789abcdef"]["evidence"], "keep")
        see_call = self.applies()[-1]["body"]
        self.assertEqual(see_call["action_type"], "see_finding_again")
        self.assertNotIn("severity", see_call["parameters"])

    def test_comment_is_inert(self):
        repo = self.repo({"a.py": "x = 1\n"})
        mc = repo.merge_branch("b1", {"a.py": "x = 2\n"})
        mc2 = repo.merge_branch("b2", {"a.py": "x = 3\n"})
        self.intent("i1")
        self.unit("u1", mc, pr=7, ended_at="2026-11-18T10:00:01Z")
        self.unit("u2", mc2, pr=8, ended_at="2026-11-18T10:00:02Z")
        # Seed a comment with ANOTHER pull request's marker on PR #7 -> must NOT stop posting!
        self.set_gh(comments={f"{OWNER}#7": [f"<!-- batch:merge-review {OWNER}#999 -->\nother pr"]})

        _, items, _, _, out = self.build({OWNER: repo})
        k1, k2 = items["items"][0]["key"], items["items"][1]["key"]
        tricky = defect(
            title="Has ``` and ```` and @alice\x07\x1b\r\tline1\nline2",
            where="a.py:1\x00",
            clause="objective",
            command="echo ```@bob```",
            why="Reason with ````` five backticks and @carol.",
        )
        huge = defect(title="H" * 20000, where="a.py:1", clause="objective", command="C" * 25000, why="W" * 25000)
        res = self.results(succeeded(k1, {"defects": [tricky]}), succeeded(k2, {"defects": [huge]}))
        a = self.read(out / "items.json", res)
        self.assertEqual((a["commented"], a["comment_present"], a["failed"]), ([k1, k2], 0, []))

        body = self.gh_state()["comments"][f"{OWNER}#7"][-1]
        self.assertTrue(body.startswith(f"<!-- batch:merge-review {OWNER}#7 -->\n"))
        # Control characters other than \n are replaced with spaces.
        for ctrl in ("\x07", "\x1b", "\r", "\t", "\x00"):
            self.assertNotIn(ctrl, body)
        self.assertIn("line1\nline2", body)
        # Verify every @mention is inside a fenced code block (walk fence state line by line).
        in_fence = None
        for line in body.splitlines():
            if in_fence is None:
                m = re.match(r"^(`{3,})$", line)
                if m:
                    in_fence = m.group(1)
                else:
                    self.assertNotIn("@", line, line)
            else:
                if line == in_fence:
                    in_fence = None
        self.assertIsNone(in_fence, "unclosed fence in comment body")
        # Verify evidence has NO control characters at all (including \n).
        ev = self.ont_state()["objects"]["finding"][fkey(sig(7))]["evidence"]
        self.assertNotIn("\n", ev)
        self.assertNotIn("\x07", ev)

        # Body over 60,000 bytes is cut on a character boundary and keeps its last line.
        body2 = self.gh_state()["comments"][f"{OWNER}#8"][-1]
        self.assertLessEqual(len(body2.encode("utf-8")), 60_000)
        self.assertEqual(
            body2.rstrip("\n").splitlines()[-1],
            "A claimed defect stays a finding of severity info until its command reproduces it at the merged commit.",
        )

        # A second item key of the same pull request (different max-tokens) posts nothing.
        _, items2, _, _, out2 = self.build({OWNER: repo}, extra=["--max-tokens", "2048"])
        k1_b = items2["items"][0]["key"]
        self.assertNotEqual(k1_b, k1)
        res2 = self.results(succeeded(k1_b, {"defects": [defect()]}))
        before_cnt = len(self.gh_state()["comments"][f"{OWNER}#7"])
        a2 = self.read(out2 / "items.json", res2)
        self.assertEqual((a2["commented"], a2["comment_present"]), ([], 1))
        self.assertEqual(len(self.gh_state()["comments"][f"{OWNER}#7"]), before_cnt)

        # GH failure lists gh_failed and continues to the next item.
        self.set_gh(fail_view=[f"{OWNER}#7"], fail_comment=[f"{OWNER}#8"], comments={})
        a3 = self.read(out / "items.json", res, code=1)
        self.assertEqual(a3["failed"], [{"key": k1, "code": "gh_failed"}, {"key": k2, "code": "gh_failed"}])
        self.set_gh(fail_view=[f"{OWNER}#7"], fail_comment=[], comments={})
        a4 = self.read(out / "items.json", res, code=1)
        self.assertEqual(a4["failed"], [{"key": k1, "code": "gh_failed"}])
        self.assertEqual(a4["commented"], [k2])

    def test_keys_and_settled_results(self):
        # Include an empty context line in the file so diff.suppressBlankEmpty is exercised.
        base_file = "line1\n\nline3\nline4\nline5\n"
        changed_file = "line1\n\nline3\nline4_changed\nline5\n"
        r1 = self.repo({"a.txt": base_file})
        mc = r1.merge_branch("b1", {"a.txt": changed_file})
        self.intent("i1")
        self.unit("u1", mc, pr=1, ended_at="2026-11-18T10:00:00Z")

        v1, items1, rm1, rs1, out1 = self.build({OWNER: r1})
        self.assertEqual(len(rm1), 1)
        self.assertEqual(rm1[0]["custom_id"], sha(canon(rm1[0]["params"])))
        self.assertEqual(items1["items"][0]["key"], rm1[0]["custom_id"])
        # Equal inputs -> byte-equal files and exact modes.
        _, _, _, _, out1b = self.build({OWNER: r1})
        for name in ("requests-model.jsonl", "requests-strong.jsonl", "items.json"):
            self.assertEqual((out1 / name).read_bytes(), (out1b / name).read_bytes())
            self.assertEqual((out1 / name).stat().st_mode & 0o777, 0o600)
        self.assertEqual(out1.stat().st_mode & 0o777, 0o700)

        # Clone 2: copy of r1 with hostile git config options.
        r2_path = self.path("clone-cfg")
        shutil.copytree(r1.root, r2_path)
        order_file = self.path("order.txt")
        order_file.write_text("z*\na*\n")
        attrs_file = self.path("attrs.txt")
        attrs_file.write_text("a.txt -diff\n")
        for key, val in (
            ("diff.noprefix", "true"),
            ("diff.mnemonicPrefix", "true"),
            ("diff.context", "10"),
            ("diff.algorithm", "patience"),
            ("diff.orderFile", str(order_file)),
            ("core.abbrev", "4"),
            ("diff.suppressBlankEmpty", "true"),
            ("core.attributesFile", str(attrs_file)),
        ):
            subprocess.run(["git", "-C", str(r2_path), "config", key, val], check=True)
        _, items2, rm2, rs2, out2 = self.build({OWNER: r2_path})
        self.assertEqual(items2["items"][0]["key"], items1["items"][0]["key"])
        self.assertEqual((out2 / "requests-model.jsonl").read_bytes(), (out1 / "requests-model.jsonl").read_bytes())

        # Clone 3: working tree holds .gitattributes marking a.txt -diff.
        r3_path = self.path("clone-wt-attr")
        shutil.copytree(r1.root, r3_path)
        (r3_path / ".gitattributes").write_text("a.txt -diff\n")
        _, items3, rm3, rs3, out3 = self.build({OWNER: r3_path})
        self.assertEqual(items3["items"][0]["key"], items1["items"][0]["key"])
        self.assertEqual((out3 / "requests-model.jsonl").read_bytes(), (out1 / "requests-model.jsonl").read_bytes())

        # Clone 4: info/attributes exists -> refused repo_unreadable before any write.
        r4_path = self.path("clone-info-attr")
        shutil.copytree(r1.root, r4_path)
        (r4_path / ".git" / "info").mkdir(parents=True, exist_ok=True)
        (r4_path / ".git" / "info" / "attributes").write_text("a.txt -diff\n")
        bad_out = self.path("no-out")
        self.assertEqual(self.build({OWNER: r4_path}, out=bad_out, code=2),
                         {"status": "repo_unreadable", "repository": OWNER})
        self.assertFalse(bad_out.exists())

        # Settled results: build given first build's succeeded result file requests nothing.
        res = self.results(succeeded(items1["items"][0]["key"], {"defects": []}), "not json")
        v_s, items_s, rm_s, rs_s, _ = self.build({OWNER: r1}, extra=["--results", res])
        self.assertEqual((v_s["requested"], v_s["settled"], v_s["result_lines_skipped"]), (0, 1, 1))
        self.assertEqual((rm_s, rs_s), ([], []))
        self.assertEqual(items_s["items"][0]["state"], "settled")

        # Errored result is NOT settled -> asked again.
        res_err = self.results(failed_line(items1["items"][0]["key"]))
        v_e, _, rm_e, _, _ = self.build({OWNER: r1}, extra=["--results", res_err])
        self.assertEqual((v_e["requested"], v_e["settled"], len(rm_e)), (1, 0, 1))

        # repo_not_given, commit_missing, too_large, and --max-requests.
        r_big = self.repo({"big.txt": "a\n", "small.txt": "s\n"})
        mc_big = r_big.merge_branch("b-big", {"big.txt": "x" * 305_000 + "\n"})
        mc_ok = r_big.merge_branch("b-ok", {"small.txt": "y\n"})
        self.unit("u-missing-repo", mc, pr=2, repository="other/repo", ended_at="2026-11-18T10:00:01Z")
        self.unit("u-missing-commit", "0" * 40, pr=3, repository=OWNER, ended_at="2026-11-18T10:00:02Z")
        self.unit("u-big", mc_big, pr=4, repository="o/big", ended_at="2026-11-18T10:00:03Z")
        self.unit("u-ok2", mc_ok, pr=5, repository="o/big", ended_at="2026-11-18T10:00:04Z")
        vm, im, _, _, _ = self.build({OWNER: r1, "o/big": r_big}, extra=["--max-requests", "1"])
        self.assertEqual((vm["repo_not_given"], vm["commit_missing"], vm["too_large"],
                          vm["requested"], vm["over_max"]),
                         (1, 1, 1, 1, 1))
        states = {it["unit"]: it["state"] for it in im["items"]}
        self.assertEqual(states, {"u1": "requested", "u-big": "too_large", "u-ok2": "over_max"})

        # Store search failures in build exit 3 and write nothing.
        for flag, val in (("unqualified", "unit"), ("search_fails", "intent"), ("repeat_cont", "unit")):
            self.set_ont(**{flag: val})
            no_out = self.path("store-fail-out")
            self.assertEqual(self.build({OWNER: r1}, out=no_out, code=3), {"status": "store_failed"}, (flag, val))
            self.assertFalse(no_out.exists())
            self.set_ont(**{flag: None})

    def test_role_is_the_callers(self):
        repo = self.repo({"a.py": "x = 1\n"})
        mc = repo.merge_branch("b1", {"a.py": "x = 2\n"})
        self.intent("i1")
        self.unit("u1", mc, pr=1)
        _, items, _, _, out = self.build({OWNER: repo})
        k = items["items"][0]["key"]
        res = self.results(succeeded(k, {"defects": [defect()]}))

        # Called with ONT_ROLE=lead -> fake ont records ONT_ROLE == "lead", and no body holds "role".
        self.read(out / "items.json", res, env=self.env(ONT_ROLE="lead"))
        applies_lead = self.applies()
        self.assertEqual(len(applies_lead), 1)
        self.assertEqual(applies_lead[0]["ont_role"], "lead")
        self.assertNotIn("role", applies_lead[0]["body"])

        # Called without ONT_ROLE -> fake ont records None, and no body holds "role".
        st = self.ont_state()
        st["objects"]["finding"][fkey(sig(1))]["evidence"] = "stale"
        self.put_ont(st)
        env_no_role = self.env()
        env_no_role.pop("ONT_ROLE", None)
        self.read(out / "items.json", res, env=env_no_role)
        applies_all = self.applies()
        self.assertEqual(len(applies_all), 2)
        self.assertIsNone(applies_all[1]["ont_role"])
        self.assertNotIn("role", applies_all[1]["body"])

    def test_actions_fit_the_schema(self):
        repo = self.repo({"a.py": "x = 1\n"})
        mc1 = repo.merge_branch("b1", {"a.py": "x = 2\n"})
        mc2 = repo.merge_branch("b2", {"a.py": "x = 3\n"})
        self.intent("i1")
        self.unit("u1", mc1, pr=10, ended_at="2026-11-18T10:00:01Z")
        self.unit("u2", mc2, pr=11, ended_at="2026-11-18T10:00:02Z")
        self.add("finding", fkey(sig(11)), kind="drift", detector="batch:merge-review",
                 summary="old", evidence="old", severity="info", state="open")
        _, items, _, _, out = self.build({OWNER: repo})
        res = self.results(*(succeeded(it["key"], {"defects": [defect()]}) for it in items["items"]))
        self.read(out / "items.json", res, env=self.env(ONT_ROLE="lead"))

        schema = json.loads((REPO / "ontology" / "work.schema.json").read_text())
        host = (REPO / "crates" / "ap" / "src" / "host.rs").read_text()
        m = re.search(r'fn id\(kind: &str, key: &str\) -> Value \{\s*json!\(\{"object_type":kind,"namespace":(\{[^}]*\})',
                      host)
        self.assertIsNotNone(m)
        namespace = json.loads(m.group(1))
        self.assertEqual(LIB.NAMESPACE, namespace)
        types = schema["value_types"]
        item_by_fk = {fkey(sig(it["pull_request"], it["repository"])): it["key"] for it in items["items"]}
        seen = set()
        applies = self.applies()
        self.assertTrue(applies)
        for e in applies:
            body = e["body"]
            self.assertEqual(set(body), {"action_type", "parameters", "agent", "request_id"})
            self.assertEqual(body["agent"], "batch-merge-review")
            fk = body["parameters"]["finding"]["native_key"]
            want_rid = "bmr-" + sha(canon([body["action_type"], fk, item_by_fk[fk], body["parameters"]]))[:40]
            self.assertEqual(body["request_id"], want_rid)
            self.assertRegex(body["request_id"], r"^bmr-[0-9a-f]{40}$")
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
                    self.assertRegex(value["native_key"], r"^batch-merge-review-[0-9a-f]{16}$")
                elif isinstance(t, dict) and "value_type" in t:
                    self.assertIn(value, types[t["value_type"]]["enum"])
                else:
                    self.assertEqual(t, "string")
                    self.assertTrue(value)
                    self.assertLessEqual(len(value.encode("utf-8")), declared[name]["max_bytes"])
            if body["action_type"] == "open_finding":
                self.assertEqual((p["kind"], p["detector"], p["severity"]), ("drift", "batch:merge-review", "info"))
            if body["action_type"] == "see_finding_again":
                self.assertEqual(set(p), {"finding", "evidence"})
        self.assertEqual(seen, {"open_finding", "see_finding_again"})


if __name__ == "__main__":
    unittest.main()
