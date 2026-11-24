"""Tests of evals/intake/run, the intake suite, run as a program against a fake `systemd-run`, a fake `systemctl`,
fake CLIs and a scratch store of the pinned engine (no model, no network, no real unit). PATH holds the fakes'
directory and nothing else; its `ont` is a shim that refuses a call without `--store`, logs every call, and runs the
engine ONT_BIN names. The fake `systemd-run` reads the `BindPaths` property it is given and hands the source
directory to the fake CLI as FAKE_STORE: so every `ont` call of a fake session names the scratch store its unit
binds, and a test asserts that for every call, and that none names the host's store.
Run (by `check`, with the engine it found or built): ONT_BIN=<ont> python3 -I -B -m unittest discover -s ci -p 'test_intake_suite.py'
"""
import glob
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
from unittest import mock
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
RUN = REPO / "evals" / "intake" / "run"
ROUTING = REPO / "evals" / "routing" / "run"
SCHEMA = REPO / "ontology" / "work.schema.json"
ONT = os.environ.get("ONT_BIN", "")
PY = sys.executable
HOST_STORE = "/var/lib/ontology-platform/ont"

FAKE_SYSTEMD_RUN = r'''#!@PY@
import json, os, subprocess, sys
state = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
plan = json.load(open(os.path.join(state, "plan.json")))
argv = sys.argv[1:]
with open(os.path.join(state, "systemd-run.jsonl"), "a") as f:
    f.write(json.dumps(argv) + "\n")
i = argv.index("--")
opts, cmd = argv[:i], argv[i + 1:]
FLAGS = {"--wait", "--pipe", "--quiet", "--collect"}
VALUED = ("--unit=", "--slice=", "--description=", "--expand-environment=", "--working-directory=", "--setenv=")
PROPS = ("ProtectSystem=", "ProtectHome=", "RuntimeMaxSec=", "ReadWritePaths=", "InaccessiblePaths=", "BindPaths=",
         "CapabilityBoundingSet=", "NoNewPrivileges=", "PrivateDevices=", "ReadOnlyPaths=")
env, cwd, k, desc = {"FAKE_IN_UNIT": "1"}, None, 0, ""
while k < len(opts):
    o = opts[k]
    if o == "-p":
        if not opts[k + 1].startswith(PROPS):
            sys.exit(1)
        if opts[k + 1].startswith("BindPaths="):
            bind = opts[k + 1].split("=", 1)[1].strip('"')
            env["FAKE_STORE"] = bind.rsplit(":", 1)[0]
        k += 2
        continue
    if o not in FLAGS and not o.startswith(VALUED):
        sys.exit(1)
    if o.startswith("--setenv="):
        key, _, val = o[len("--setenv="):].partition("=")
        env[key] = val
    if o.startswith("--working-directory="):
        cwd = o.split("=", 1)[1]
    if o.startswith("--description="):
        desc = o.split("=", 1)[1]
    k += 1
mode = plan.get("systemd_run", "ok")
if mode == "fail" or (mode == "fail_sessions" and not desc.endswith("-probe")):
    sys.exit(1)
sys.exit(subprocess.Popen(cmd, cwd=cwd, env=env).wait())
'''

FAKE_SYSTEMCTL = r'''#!@PY@
import sys
args = [a for a in sys.argv[1:] if a != "--"]
if args[0] == "is-active":
    for u in args[1:]:
        print("inactive")
    sys.exit(3)
if args[0] == "show":
    print("")
    sys.exit(0)
sys.exit(1)
'''

# The engine behind a shim. A call with --store is logged (ont.jsonl) and runs the engine. A call without it would be
# the host's store (the engine's default): it is logged apart (host-ont.jsonl), answered from the file of the test
# (host-ont.json: the keys of its tasks) and never reaches the engine.
ONT_SHIM = r'''#!@PY@
import json, os, sys
state = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
a = sys.argv[1:]
if "--store" in a:
    with open(os.path.join(state, "ont.jsonl"), "a") as f:
        f.write(json.dumps({"argv": a, "fake_store": os.environ.get("FAKE_STORE")}) + "\n")
    os.execv("@ONT@", ["@ONT@"] + a)
body = json.loads(sys.stdin.read() or "null")
with open(os.path.join(state, "host-ont.jsonl"), "a") as f:
    f.write(json.dumps({"argv": a, "body": body, "engine": False}) + "\n")
tasks = json.load(open(os.path.join(state, "host-ont.json")))["tasks"]
ns = {"instance": "vps://x", "account": "root"}
if a == ["search"]:
    objs = [{"id": {"object_type": "task", "namespace": ns, "native_key": t}, "head": {"fields": {}}} for t in tasks] \
        if body["object_type"] == "task" else []
    print(json.dumps({"status": "ok", "result": {"objects": objs, "basis": {"qualified": True}}}))
elif a[:2] == ["action", "apply"]:
    edits = [{"id": {"object_type": "kpi", "namespace": ns, "native_key": "kpi-" + body["request_id"][-6:]}, "change": "initial"}]
    print(json.dumps({"status": "ok", "result": {"outcome": "applied", "edits": edits}}))
else:
    sys.exit(97)
'''

# A session: applies the actions of its plan through `ont` on FAKE_STORE, then answers. A plan is found by the words.
FAKE_CLI = r'''#!@PY@
import json, os, subprocess, sys
state = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
plan = json.load(open(os.path.join(state, "plan.json")))
name, args = os.path.basename(sys.argv[0]), sys.argv[1:]
words = args[args.index("--print") + 1] if name == "agy" else args[-1]
beh = plan["cli"].get(name + "|" + words) or plan["cli"].get(words) or {}
store, made = os.environ["FAKE_STORE"], {}
with open(os.path.join(state, "cli.jsonl"), "a") as f:
    f.write(json.dumps({"cli": name, "words": words, "argv": args, "cwd": os.getcwd(), "env": dict(os.environ)}) + "\n")
def fill(v):
    if isinstance(v, dict) and "@new" in v:
        ns = made["request"]["namespace"] if "request" in made else {"instance": "vps://palantirkc", "account": "root"}
        return {"object_type": v["@new"], "namespace": ns, "native_key": v["key"]}
    if isinstance(v, str) and v.startswith("@"):
        return made[v[1:]]
    return {k: fill(x) for k, x in v.items()} if isinstance(v, dict) else v
for act in beh.get("actions", []):
    body = {"action_type": act["type"], "request_id": act["id"], "agent": "t", "session": "t", "role": "lead", "parameters": fill(act["params"])}
    if act.get("role"):
        body["role"] = act["role"]
    p = subprocess.run(["ont", "--store", store + "/ontology.sqlite", "action", "apply"], input=json.dumps(body),
                       capture_output=True, text=True)
    for e in (json.loads(p.stdout).get("result") or {}).get("edits", []):
        if e["change"] == "initial":
            made[e["id"]["object_type"]] = e["id"]
if beh.get("exit"):
    sys.exit(1)
answer = beh.get("answer", "")
if name == "claude":
    events = [{"type": "assistant", "message": {"content": [{"type": "tool_use", "id": "t1", "name": "Bash",
                                                              "input": {"command": "ont fn route < /dev/null"}}]}},
              {"type": "result", "result": answer, "total_cost_usd": 0.0123, "usage": {"input_tokens": 10, "output_tokens": 7}}]
elif name == "muse":
    events = [{"payload": {"kind": "run_terminal", "text": answer}}]
else:
    events = [{"event": "result", "result": {"response": answer, "usage": {"input_tokens": 3, "output_tokens": 9}}}]
for e in events:
    print(json.dumps(e), flush=True)
'''

NS = "@new"
SEED_ID = {NS: "intent", "key": "nightly-backup"}
KEEP = [{"fact": ["nightly-backup"]}, {"fact": ["confirm"]}]
OK_ANSWER = "The nightly-backup intent is a draft: please confirm it before units are added."


def case(cid, lane, seed="none", ooo=None, missing=None):
    return {"id": cid, "words": "T-%s words" % cid, "seed": seed,
            "expect": {"lane": lane, "out_of_order": ooo, "missing": missing or []}}


def rec(cid, verified=False, words=None):
    return {"type": "record_request", "id": "r-" + cid, "params": {
        "words": words or "T-%s words" % cid, "words_verified": verified, "reading": "x", "kind": "order", "topic": "t",
        "said_at": "2026-11-01T00:00:00Z"}}


def draft(cid):
    return {"type": "draft_intent", "id": "d-" + cid, "params": {
        "intent": {NS: "intent", "key": "k-" + cid}, "origin": "@request", "outcome": "o", "first_user": "f"}}


def route(cid):
    return {"type": "route_request", "id": "ro-" + cid, "params": {"request": "@request", "subject": SEED_ID}}


def unit(cid, **kw):
    params = {"unit": {NS: "unit", "key": "u-" + cid}, "lane": "objective", "class": "plain", "objective": "o"}
    params.update(kw)
    return {"type": "add_unit", "id": "u-" + cid, "params": params}


def small(cid):
    return unit(cid, lane="small", origin="@request", repository="o/r", write_set=["x"])


# Every case: the sample built to hold all five facts, or to fail one. (id, case, plan)
SCENES = [
    ("objective", case("objective", "objective"), {"actions": [rec("objective"), draft("objective")], "answer": "ok"}),
    ("a-fails", case("a-fails", "objective"),
     {"actions": [rec("a-fails", words="T-a-fails words!"), draft("a-fails")], "answer": "ok"}),
    ("b-fails", case("b-fails", "objective"), {"actions": [rec("b-fails")], "answer": "ok"}),
    ("c-unit-refused", case("c-unit-refused", "objective"),
     {"actions": [rec("c-unit-refused"), draft("c-unit-refused"),
                  unit("c-unit-refused", intent={NS: "intent", "key": "k-c-unit-refused"})], "answer": "ok"}),
    ("c-assumed", case("c-assumed", "objective"),
     {"actions": [rec("c-assumed"), draft("c-assumed"),
                  {"type": "assume_intent", "id": "as-c", "role": "lead",
                   "params": {"intent": {NS: "intent", "key": "k-c-assumed"}, "reason": "r"}},
                  unit("c-assumed", intent={NS: "intent", "key": "k-c-assumed"})], "answer": "ok"}),
    ("d-assume", case("d-assume", "objective"),
     {"actions": [rec("d-assume"), draft("d-assume"),
                  {"type": "assume_intent", "id": "as-d", "role": "lead",
                   "params": {"intent": {NS: "intent", "key": "k-d-assume"}, "reason": "r"}}], "answer": "ok"}),
    ("skip-ok", case("skip-ok", "objective", "drafted", "confirmation", KEEP),
     {"actions": [rec("skip-ok"), route("skip-ok")], "answer": OK_ANSWER}),
    ("skip-confirmed", case("skip-confirmed", "objective", "drafted", "confirmation", KEEP),
     {"actions": [rec("skip-confirmed"), route("skip-confirmed"),
                  {"type": "confirm_intent", "id": "cf", "params": {"intent": SEED_ID, "by": "@request"}}],
      "answer": OK_ANSWER}),
    ("e-missing", case("e-missing", "objective", "drafted", "confirmation", KEEP),
     {"actions": [rec("e-missing"), route("e-missing")], "answer": "The nightly-backup intent is a draft."}),
    ("e-six", case("e-six", "objective", "drafted", "confirmation", KEEP),
     {"actions": [rec("e-six"), route("e-six")], "answer": OK_ANSWER + " a? b? c? d? e? f?"}),
    ("e-five", case("e-five", "objective", "drafted", "confirmation", KEEP),
     {"actions": [rec("e-five"), route("e-five")],
      "answer": OK_ANSWER + " a? b? c? d? ```\nx? y? z?\n``` and `w?` e？"}),
    ("small", case("small", "small"), {"actions": [rec("small"), small("small")], "answer": "ok"}),
    ("small-no-unit", case("small-no-unit", "small"), {"actions": [rec("small-no-unit")], "answer": "ok"}),
    ("none", case("none", "none", "drafted"), {"actions": [], "answer": "The draft is nightly-backup."}),
    ("none-note", case("none-note", "none", "drafted"),
     {"actions": [{"type": "note_intent", "id": "n", "params": {"intent": SEED_ID, "notes": "n"}}], "answer": "ok"}),
    ("none-recorded", case("none-recorded", "none"), {"actions": [rec("none-recorded")], "answer": "ok"}),
    ("verified", case("verified", "objective"),
     {"actions": [rec("verified", verified=True), draft("verified")], "answer": "ok"}),
    ("unanswered", case("unanswered", "none"), {"exit": True}),
]
EXPECTED = {  # the facts a sample of each scene holds
    "objective": "abcde", "a-fails": "cde", "b-fails": "acde", "c-unit-refused": "abde", "c-assumed": "abe",
    "d-assume": "abce", "skip-ok": "abcde", "skip-confirmed": "abde", "e-missing": "abcd", "e-six": "abcd",
    "e-five": "abcde", "small": "abcde", "small-no-unit": "acde", "none": "abcde", "none-note": "acde",
    "none-recorded": "cde", "verified": "abcde"}


def load(path, name):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    mod = importlib.util.module_from_spec(importlib.util.spec_from_loader(name, loader))
    loader.exec_module(mod)
    return mod


class World:
    def __init__(self, test, cases):
        self.tmp = tempfile.TemporaryDirectory()
        test.addCleanup(self.tmp.cleanup)
        t = self.t = Path(self.tmp.name)
        site = t / "site"
        (site / "evals" / "intake").mkdir(parents=True)
        (site / "evals" / "routing").mkdir()
        (site / "ontology").mkdir()
        shutil.copy2(RUN, site / "evals" / "intake" / "run")
        shutil.copy2(ROUTING, site / "evals" / "routing" / "run")
        shutil.copy2(SCHEMA, site / "ontology" / "work.schema.json")
        self.run_path = site / "evals" / "intake" / "run"
        self.cases_path = site / "evals" / "intake" / "cases.json"
        self.cases_path.write_text(json.dumps(cases))
        (site / "evals" / "intake" / "host-cases.sha256").write_text("")
        self.bin = t / "bin"
        self.bin.mkdir()
        for name, text in (("systemd-run", FAKE_SYSTEMD_RUN), ("systemctl", FAKE_SYSTEMCTL), ("claude", FAKE_CLI),
                           ("muse", FAKE_CLI), ("agy", FAKE_CLI), ("ont", ONT_SHIM)):
            (self.bin / name).write_text(text.replace("@PY@", PY).replace("@ONT@", ONT))
            (self.bin / name).chmod(0o755)
        self.root = t / "root"
        self.root.mkdir()
        (self.root / "AGENTS.md").write_text("agents\n")
        self.home = t / "home"
        for d in (".config/gh", ".ssh", ".claude-x/projects/-w-edu-lessons"):
            (self.home / d).mkdir(parents=True)
        (self.home / ".netrc").write_text("")
        self.acct = t / "acct"
        self.acct.mkdir()
        self.tmpdir = t / "tmp"  # the runner's TMPDIR: its work directories are made, and swept, here only
        self.tmpdir.mkdir()
        self.out = t / "out"
        self.plan({})

    def plan(self, plan):
        (self.t / "plan.json").write_text(json.dumps(plan))

    def env(self, **extra):
        env = {"PATH": str(self.bin), "HOME": str(self.home), "CLAUDE_CONFIG_DIR": str(self.acct),
               "TMPDIR": str(self.tmpdir)}
        env.update(extra)
        return {k: v for k, v in env.items() if v is not None}

    def call(self, verb, *args, env=None):
        r = subprocess.run([PY, "-I", "-B", str(self.run_path), verb, *map(str, args)], env=env or self.env(),
                           capture_output=True, text=True, timeout=300)
        return r.returncode, (json.loads(r.stdout) if r.stdout.strip() else None), r

    def answers(self, *args, reader="claude-haiku", env=None):
        return self.call("answers", "--root", self.root, "--reader", reader, "--out", self.out, *args, env=env)

    def lines(self, name):
        p = self.t / name
        return [json.loads(x) for x in p.read_text().splitlines()] if p.exists() else []

    def units(self):
        return [c for c in self.lines("systemd-run.jsonl")
                if not any(o.startswith("--description=") and o.endswith("-probe") for o in c)]

    def sample(self, cid, n=1, reader="claude-haiku"):
        return self.out / reader / ("%s__s%d" % (cid, n))


def props(call, name):
    opts = call[:call.index("--")]
    return [opts[i + 1].split("=", 1)[1] for i, o in enumerate(opts) if o == "-p" and opts[i + 1].startswith(name + "=")]


@unittest.skipUnless(ONT, "ONT_BIN names the engine `check` found or built")
class Scored(unittest.TestCase):
    """One run of every scene by claude-haiku, scored."""
    @classmethod
    def setUpClass(cls):
        cls.holder = unittest.TestCase()
        cls.w = World(cls.holder, [c for _, c, _ in SCENES])
        cls.w.plan({"cli": {c["words"]: p for _, c, p in SCENES}})
        cls.code, cls.out, cls.proc = cls.w.answers("--samples", "1")
        cls.score = cls.w.call("score", "--out", cls.w.out)

    @classmethod
    def tearDownClass(cls):
        cls.holder.doCleanups()

    def test_the_run_ended_ok_and_the_score_has_the_routing_members(self):
        self.assertEqual((self.code, self.proc.stderr), (0, ""), self.out)
        code, score, _ = self.score
        self.assertEqual(code, 0)
        x = score["readers"]["claude-haiku"]
        for k in ("n", "passes", "samples", "mean", "majority", "unanswered", "tokens_median", "facts"):
            self.assertIn(k, x)
        self.assertEqual((x["n"], x["unanswered"]), (1, 1))
        self.assertEqual(x["passes"]["unanswered"], 0)
        self.assertEqual(score["parity"], {"claude-haiku": None})
        self.assertTrue((self.w.out / "score.json").exists() and (self.w.out / "sheet.md").exists())

    def test_each_fact_holds_or_fails_by_the_sample_built_for_it(self):
        facts = self.score[1]["readers"]["claude-haiku"]["facts"]
        for cid, holds in EXPECTED.items():
            self.assertEqual("".join(k for k in "abcde" if facts[cid][k]), holds, cid)

    def test_a_sample_passes_when_all_five_hold(self):
        x = self.score[1]["readers"]["claude-haiku"]
        self.assertEqual(sorted(k for k, v in x["passes"].items() if v), sorted(k for k, v in EXPECTED.items() if v == "abcde"))
        self.assertEqual(x["majority"], sorted(k for k, v in EXPECTED.items() if v == "abcde"))

    def test_a_request_with_equal_words_passes_a_without_a_record_to_verify(self):
        x = self.score[1]["readers"]["claude-haiku"]
        self.assertEqual(x["facts"]["objective"]["a"], 1)  # words_verified false
        self.assertEqual(x["requests_verified"], 1)  # only the sample whose request was recorded verified
        self.assertGreater(x["requests_recorded"], x["requests_verified"])

    def test_every_engine_call_names_a_scratch_store_and_a_sessions_calls_name_its_own(self):
        calls = self.w.lines("ont.jsonl")
        self.assertGreater(len(calls), 100)
        stores = set()
        for c in calls:
            a = c["argv"]
            self.assertIn("--store", a)
            path = a[a.index("--store") + 1]
            self.assertTrue(path.startswith(str(self.w.out)), path)
            self.assertFalse(path.startswith(HOST_STORE))
            stores.add(os.path.dirname(path))
            if c["fake_store"]:
                self.assertEqual(os.path.dirname(path), c["fake_store"])
        sessions = {c["fake_store"] for c in calls if c["fake_store"]}
        self.assertEqual(sessions, {str(self.w.sample(cid)) + ".store" for cid, _, p in SCENES if p.get("actions")})
        for c in self.w.lines("cli.jsonl"):
            self.assertTrue(c["env"]["FAKE_STORE"].startswith(str(self.w.out)))

    def test_every_store_is_its_own_and_built_from_the_schema_of_the_tree(self):
        for cid, c, _ in SCENES:
            store = str(self.w.sample(cid)) + ".store"
            self.assertTrue(os.path.isfile(store + "/ontology.sqlite"), cid)
        out = subprocess.run([ONT, "--store", str(self.w.sample("none")) + ".store/ontology.sqlite", "schema", "check", str(SCHEMA)],
                             capture_output=True, text=True)
        self.assertEqual(out.returncode, 0)
        host = [json.loads(x) for x in Path(self.w.sample("none").with_suffix(".jsonl")).read_text().splitlines()]
        self.assertEqual(host[-1]["result"], "The draft is nightly-backup.")  # after the stream's tool_use line


@unittest.skipUnless(ONT, "ONT_BIN names the engine `check` found or built")
class ScoreRecord(unittest.TestCase):
    """`score --record <task key>` of the intake suite: the scratch stores are read with `--store`, the record goes to
    the `ont` of PATH without it, and that call is answered by the shim from a file, never by the engine."""
    def setUp(self):
        self.w = World(self, [case("one", "none")])
        self.words = self.w.cases_path and json.loads(self.w.cases_path.read_text())[0]["words"]
        self.w.plan({"cli": {self.words: {"answer": "fine"}}})
        (self.w.t / "host-ont.json").write_text(json.dumps({"tasks": ["task-suite"]}))
        for reader in ("claude-haiku", "muse-ember"):
            self.assertEqual(self.w.answers("--samples", "1", reader=reader)[0], 0)
        trap = self.w.t / "engine-trap"  # what ONT_BIN names for the score call: it must never run
        trap.write_text("#!/bin/sh\necho reached >> %s/trap.log\nexit 99\n" % self.w.t)
        trap.chmod(0o755)
        self.trap = trap

    def score(self, *extra):
        return self.w.call("score", "--out", self.w.out, *extra, env=self.w.env(ONT_BIN=str(self.trap)))

    def test_score_record_without_the_option_makes_no_call_without_a_store(self):
        code, out, r = self.score()
        self.assertEqual((code, r.stderr), (0, ""))
        self.assertNotIn("recorded", out)
        self.assertEqual(self.w.lines("host-ont.jsonl"), [])
        self.assertFalse((self.w.t / "trap.log").exists())

    def test_score_record_one_measurement_per_reader_and_no_call_reached_the_engine(self):
        w = self.w
        code, out, r = self.score("--record", "task-suite")
        self.assertEqual((code, r.stderr), (0, ""), out)
        self.assertEqual(out["recorded"], {"measurements": 2, "already": 0, "older": 0, "skipped": 0, "failed": 0})
        x = out["readers"]["claude-haiku"]
        self.assertEqual((x["fn_samples"], x["usd"]), (1.0, 0.01))
        self.assertEqual((out["readers"]["muse-ember"]["fn_samples"], out["readers"]["muse-ember"]["usd"]), (None, None))
        written = json.loads((w.out / "score.json").read_text())
        self.assertNotIn("recorded", written)
        digest = hashlib.sha256((w.out / "score.json").read_bytes()).hexdigest()
        calls = w.lines("host-ont.jsonl")
        self.assertTrue(calls and all("--store" not in c["argv"] and c["engine"] is False for c in calls))
        self.assertFalse((w.t / "trap.log").exists(), "no call reached the engine ONT_BIN names")
        actions = [c["body"] for c in calls if c["argv"][0] == "action"]
        self.assertEqual(sorted(a["action_type"] for a in actions), ["define_kpi"] * 2 + ["record_measurement"] * 2)
        self.assertEqual({a["role"] for a in actions}, {"lead"})  # the score's record is the Lead's (roles.md R3)
        self.assertEqual({a["parameters"]["name"] for a in actions if a["action_type"] == "define_kpi"},
                         {"suite intake claude-haiku", "suite intake muse-ember"})
        for a in actions:
            if a["action_type"] == "define_kpi":
                self.assertIn("recorded by evals/intake/run score --record. No case text.", a["parameters"]["definition"])
        measured = {a["request_id"]: a["parameters"] for a in actions if a["action_type"] == "record_measurement"}
        for reader, value in (("claude-haiku", "mean 1.0000 majority 1/1 n 1 unanswered 0 fn 1.0000 usd 0.01"),
                              ("muse-ember", "mean 1.0000 majority 1/1 n 1 unanswered 0 fn n/a usd n/a")):
            rid = "suite-" + hashlib.sha256(("%s:%s" % (reader, digest)).encode()).hexdigest()[:32]
            p = measured[rid]
            self.assertEqual((p["value"], p["evidence"]), (value, "evals/intake/run score sha256:" + digest))
            self.assertRegex(p["observed_at"], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")
        self.assertNotIn(self.words, json.dumps(actions))
        self.assertNotIn(str(w.t), json.dumps(actions))
        # every call with a store is the score of the scratch stores, and none names the host's
        for c in w.lines("ont.jsonl"):
            self.assertTrue(c["argv"][c["argv"].index("--store") + 1].startswith(str(w.out)))

    def test_score_record_the_members_of_the_score_are_the_intake_suites_and_the_new_two(self):
        code, out, _ = self.score()
        x = out["readers"]["claude-haiku"]
        self.assertEqual(set(x), {"n", "passes", "samples", "mean", "majority", "unanswered", "tokens_median", "facts",
                                  "requests_recorded", "requests_verified", "fn_samples", "usd"})
        self.assertEqual(set(out), {"suite", "case_files", "readers", "parity"})
        first = (self.w.out / "score.json").read_bytes()
        self.score()
        self.assertEqual((self.w.out / "score.json").read_bytes(), first)


@unittest.skipUnless(ONT, "ONT_BIN names the engine `check` found or built")
class Session(unittest.TestCase):
    def setUp(self):
        self.w = World(self, [case("one", "none"), case("two", "objective", "confirmed")])
        self.words = {c["id"]: c["words"] for c in json.loads(self.w.cases_path.read_text())}
        self.w.plan({"cli": {self.words["one"]: {"answer": "fine"}, self.words["two"]: {"answer": "fine"}}})
        self.token_env = {"GH_TOKEN": "x", "ANTHROPIC_API_KEY": "x", "CLAUDE_CODE_OAUTH_TOKEN": "x", "MY_SECRET": "x"}

    def run_one(self, **kw):
        code, out, r = self.w.answers("--samples", "1", "--case", "one", **kw)
        self.assertEqual((code, r.stderr), (0, ""), out)
        return self.w.units()[0]

    def test_a_session_has_the_routing_suites_work_directory_and_private_files(self):
        import stat
        self.run_one(env=self.w.env(**self.token_env))
        [run] = self.w.lines("cli.jsonl")
        work = Path(run["cwd"])
        self.assertEqual(work.parent.parent, self.w.tmpdir)
        self.assertRegex(work.parent.name, r"routing-[0-9]+-[0-9]+-[a-z0-9_]+")
        self.assertEqual(sorted(x.name for x in self.w.tmpdir.iterdir()), [], "removed when the call ends")
        for suffix in (".jsonl", ".err"):
            mode = stat.S_IMODE(self.w.sample("one").with_suffix(suffix).stat().st_mode)
            self.assertEqual(mode, 0o600, suffix)

    def test_the_unit_holds_every_property_of_the_isolation(self):
        call = self.run_one(env=self.w.env(**self.token_env))
        opts = call[:call.index("--")]
        self.assertEqual(props(call, "ProtectSystem"), ["strict"])
        self.assertEqual(props(call, "ProtectHome"), ["read-only"])
        self.assertEqual(props(call, "RuntimeMaxSec"), ["600"])
        self.assertEqual(props(call, "BindPaths"), ["%s:%s" % (str(self.w.sample("one")) + ".store", HOST_STORE)])
        self.assertEqual(props(call, "CapabilityBoundingSet"),
                         ["CAP_CHOWN CAP_DAC_OVERRIDE CAP_FOWNER CAP_FSETID CAP_SETGID CAP_SETUID"])
        self.assertEqual((props(call, "NoNewPrivileges"), props(call, "PrivateDevices")), (["yes"], ["yes"]))
        self.assertEqual(props(call, "ReadOnlyPaths"), ["/sys", "/proc/sys", "/proc/sysrq-trigger"])
        self.assertFalse([o for o in opts if "ProtectKernelTunables" in o])
        self.assertTrue(all(os.path.basename(o.split("=", 1)[1]).startswith("intake-") for o in opts if o.startswith("--unit=")))
        self.assertFalse([o for o in opts if self.words["one"] in o], "no words in a unit's description or property")

    def test_what_is_hidden_is_inaccessible_each_in_its_own_assignment(self):
        call = self.run_one()
        hidden = {p[1:] for p in props(call, "InaccessiblePaths")}
        home = str(self.w.home)
        for path in (home + "/.config/gh", home + "/.ssh", home + "/.netrc", home + "/.claude-x/projects/-w-edu-lessons"):
            self.assertIn(path, hidden)
        self.assertNotIn(home + "/.git-credentials", hidden, "a path that does not exist is not listed")
        for path in ("/etc/agent-platform/credentials", "/etc/agent-platform/ci", "/run/user", "/run/docker.sock",
                     "/run/containerd/containerd.sock", "/run/tailscale", "/run/systemd/private",
                     "/run/dbus/system_bus_socket", "/home/palantirkc/work/edu-*"):
            self.assertLessEqual(set(glob.glob(path)), hidden, path)
            self.assertEqual(os.path.exists(path), path in hidden) if "*" not in path else None

    def test_no_variable_that_holds_a_token_reaches_the_session(self):
        call = self.run_one(env=self.w.env(**self.token_env))
        names = {o[len("--setenv="):].split("=")[0] for o in call if o.startswith("--setenv=")}
        self.assertEqual(names & set(self.token_env), set())
        self.assertIn("CLAUDE_CONFIG_DIR", names)
        self.assertEqual(set(self.w.lines("cli.jsonl")[0]["env"]) & set(self.token_env), set())

    def test_the_session_gets_the_words_as_its_only_message_and_the_readers_of_the_routing_suite(self):
        routing = load(ROUTING, "routing_for_test")
        for reader, cli in (("claude-haiku", "claude"), ("claude-sonnet", "claude"), ("muse-ember", "muse"), ("agy-hydrogen", "agy")):
            code, out, r = self.w.answers("--samples", "1", "--case", "one", reader=reader)
            self.assertEqual((code, r.stderr), (0, ""), (reader, out))
            run = [c for c in self.w.lines("cli.jsonl") if c["cli"] == cli][-1]
            self.assertEqual(run["words"], self.words["one"])
            self.assertEqual((self.w.sample("one", reader=reader).with_suffix(".md")).read_text(), "fine")
            sid = run["argv"][run["argv"].index("--session-id") + 1] if "--session-id" in run["argv"] else None
            want = routing.READERS[reader]["argv"]("PROMPT", str(self.w.root), run["cwd"], sid)[1:]
            got = [("PROMPT" if x == run["words"] else x) for x in run["argv"]]
            if cli == "claude":
                self.assertNotIn("--disable-slash-commands", got)
                want = [x for x in want if x != "--disable-slash-commands"]
                self.assertIn("--strict-mcp-config", got)
            self.assertEqual(got, want, reader)

    def test_a_session_ends_when_the_unit_cannot_start_and_the_probe_runs_the_same_properties(self):
        self.w.plan({"systemd_run": "fail"})
        code, out, _ = self.w.answers("--samples", "1")
        self.assertEqual((code, out), (2, {"status": "sandbox_unavailable"}))
        self.w.plan({"systemd_run": "fail_sessions"})
        code, out, _ = self.w.answers("--samples", "1", "--case", "one")
        self.assertEqual((code, out), (2, {"status": "sandbox_unavailable"}))
        probe = [c for c in self.w.lines("systemd-run.jsonl") if any(o.endswith("-probe") for o in c)][0]
        self.assertTrue(props(probe, "BindPaths") and props(probe, "CapabilityBoundingSet"))
        self.assertEqual((props(probe, "PrivateDevices"), props(probe, "NoNewPrivileges")), (["yes"], ["yes"]))
        code, out, _ = self.w.answers("--samples", "1", env=self.w.env(PATH=str(self.w.root)))
        self.assertEqual((code, out), (2, {"status": "sandbox_unavailable"}))

    def test_each_sample_is_a_fresh_store_with_its_seed(self):
        code, out, r = self.w.answers("--samples", "2", "--case", "two")
        self.assertEqual((code, r.stderr), (0, ""), out)
        stores = [str(self.w.sample("two", n)) + ".store" for n in (1, 2)]
        self.assertEqual(len({str(c["env"]["FAKE_STORE"]) for c in self.w.lines("cli.jsonl")}), 2)
        for s in stores:
            r = subprocess.run([ONT, "--store", s + "/ontology.sqlite", "search"], input='{"object_type":"intent"}',
                               capture_output=True, text=True)
            fields = json.loads(r.stdout)["result"]["objects"][0]["head"]["fields"]
            self.assertEqual(fields["status"]["value"], "confirmed")
        # a sample run again begins with an empty store: nothing the first session left remains
        marker = Path(stores[0]) / "left-by-a-session"
        marker.write_text("x")
        (self.w.sample("two", 1).with_suffix(".json")).unlink()
        self.assertEqual(self.w.answers("--samples", "2", "--case", "two")[0], 0)
        self.assertFalse(marker.exists())

    def test_the_callers_identity_reaches_neither_the_seed_nor_a_session(self):
        who = {"ONT_ROLE": "lead", "ONT_AGENT": "someone-else"}
        code, out, r = self.w.answers("--samples", "1", "--case", "two", env=self.w.env(**who))
        self.assertEqual((code, r.stderr), (0, ""), out)
        self.assertEqual([c for c in self.w.lines("cli.jsonl") if any(k in c["env"] for k in who)], [])
        store = str(self.w.sample("two")) + ".store/ontology.sqlite"
        rows = json.loads(subprocess.run([ONT, "--store", store, "log"], input="{}", capture_output=True, text=True).stdout)["result"]["rows"]
        self.assertEqual(len(rows), 6)
        self.assertEqual({(x["actor"]["agent"], x["actor"].get("role")) for x in rows}, {("seed", "lead")})
        self.assertEqual(self.w.answers("--samples", "1", "--case", "two", env=self.w.env())[0], 0)

    def test_the_seeds_are_none_drafted_and_confirmed(self):
        mod = load(self.w.run_path, "intake_for_test")
        # The engine is found on PATH, as `run` finds it; a caller whose PATH lacks it (CI under sudo) puts it there.
        env = {"PATH": str(self.w.bin) + os.pathsep + os.environ.get("PATH", "")}
        for seed, status, requests in (("none", None, 0), ("drafted", "draft", 1), ("confirmed", "confirmed", 2)):
            d = tempfile.mkdtemp(dir=self.w.t)
            with mock.patch.dict(os.environ, env):
                self.assertTrue(mod.build_store(d, seed))
                intents, reqs, rows = mod.objects(d, "intent"), mod.objects(d, "request"), mod.log_rows(d)
            self.assertEqual(len(reqs), requests)
            self.assertEqual([f["status"] for _, f in intents], [status] if status else [])
            self.assertTrue(all(r["submitted_at"] <= mod.SEED_END for r in rows))


    def test_parity_is_the_share_of_sonnets_majority_the_reader_also_holds_and_the_gate_judges_each_reader(self):
        for reader in ("claude-sonnet", "muse-ember"):
            self.assertEqual(self.w.answers("--samples", "3", "--case", "one", reader=reader)[0], 0)
        code, score, _ = self.w.call("score", "--out", self.w.out)
        self.assertEqual((code, score["parity"]), (0, {"muse-ember": 1.0}))
        self.assertEqual(score["readers"]["muse-ember"]["majority"], ["one"])
        path = self.w.out / "score.json"
        code, out, _ = self.w.call("gate", "--baseline", path, "--candidate", path)
        self.assertEqual((code, out["status"]), (0, "pass"))
        worse = json.loads(path.read_text())
        for r in worse["readers"].values():
            r["passes"] = {k: 0 for k in r["passes"]}
        (self.w.t / "worse.json").write_text(json.dumps(worse))
        code, out, _ = self.w.call("gate", "--baseline", path, "--candidate", self.w.t / "worse.json")
        self.assertEqual((code, out["status"]), (1, "fail"))


class Refusals(unittest.TestCase):
    def setUp(self):
        self.w = World(self, [case("one", "none")])

    def status(self, *args, **kw):
        code, out, _ = self.w.call(*args, **kw)
        self.assertEqual(code, 2, out)
        return out

    def test_usage_and_roots_and_cases(self):
        self.assertEqual(self.status("answers", "--reader", "nobody", "--root", self.w.root, "--out", self.w.out), {"status": "usage"})
        self.assertEqual(self.status("answers", "--reader", "claude-haiku", "--out", self.w.out, "--samples", "0"), {"status": "usage"})
        self.assertEqual(self.status("bogus"), {"status": "usage"})
        self.assertEqual(self.status("answers", "--reader", "claude-haiku", "--root", self.w.t, "--out", self.w.out),
                         {"status": "root_invalid"})
        self.assertEqual(self.status("answers", "--reader", "claude-haiku", "--root", self.w.root, "--out", self.w.out,
                                     "--case", "nope")["status"], "case_unknown")

    @unittest.skipUnless(ONT, "ONT_BIN")
    def test_a_missing_cli_or_engine(self):
        (self.w.bin / "claude").unlink()
        self.assertEqual(self.status("answers", "--reader", "claude-haiku", "--root", self.w.root, "--out", self.w.out),
                         {"status": "cli_missing"})
        (self.w.bin / "ont").unlink()
        self.assertEqual(self.status("answers", "--reader", "muse-ember", "--root", self.w.root, "--out", self.w.out),
                         {"status": "ont_not_found"})

    def test_a_case_file_is_checked_before_anything_runs(self):
        good = case("one", "none")
        bad = [dict(good, seed="halfway"), dict(good, words=""), dict(good, expect=dict(good["expect"], lane="big")),
               dict(good, expect=dict(good["expect"], out_of_order="signing")),
               dict(good, expect=dict(good["expect"], missing=[{"fact": ["x"]}])),
               dict(good, expect=dict(good["expect"], out_of_order="intent", missing=[])),
               dict(good, expect=dict(good["expect"], out_of_order="intent", missing=["x"])), dict(good, id="a/b")]
        for b in bad:
            self.w.cases_path.write_text(json.dumps([b]))
            self.assertEqual(self.status("answers", "--reader", "claude-haiku", "--root", self.w.root, "--out", self.w.out),
                             {"status": "cases_invalid"}, b)
        self.w.cases_path.write_text(json.dumps([good, good]))
        self.assertEqual(self.status("answers", "--reader", "claude-haiku", "--root", self.w.root, "--out", self.w.out),
                         {"status": "case_id_twice"})
        self.assertFalse(self.w.out.exists())

    def test_host_cases_are_admitted_by_their_digest_only(self):
        host = self.w.t / "host.json"
        host.write_text(json.dumps([case("h1", "none")]))
        args = ("answers", "--reader", "claude-haiku", "--root", self.w.root, "--out", self.w.out, "--cases", host)
        self.assertEqual(self.status(*args), {"status": "cases_digest_unknown"})
        self.assertEqual(self.status("answers", "--reader", "claude-haiku", "--root", self.w.root, "--out", self.w.out,
                                     "--cases", "relative.json"), {"status": "cases_unreadable"})
        digest = hashlib.sha256(host.read_bytes()).hexdigest()
        (self.w.run_path.parent / "host-cases.sha256").write_text("%s  fixture-host\n" % digest)
        self.w.plan({"systemd_run": "fail"})
        self.assertEqual(self.status(*args), {"status": "sandbox_unavailable"})  # past the case check
        host.write_text(json.dumps([case("one", "none")]))
        (self.w.run_path.parent / "host-cases.sha256").write_text("%s  fixture-host\n" % hashlib.sha256(host.read_bytes()).hexdigest())
        self.assertEqual(self.status(*args), {"status": "case_id_twice"})

    @unittest.skipUnless(ONT, "ONT_BIN")
    def test_score_and_gate_refusals(self):
        self.assertEqual(self.status("score", "--out", self.w.out), {"status": "no_reader"})
        (self.w.out / "claude-haiku").mkdir(parents=True)
        self.assertEqual(self.status("score", "--out", self.w.out), {"status": "run_record_missing"})
        self.w.plan({"cli": {}})
        shutil.rmtree(self.w.out)
        self.assertEqual(self.w.answers("--samples", "1")[0], 0)
        self.w.cases_path.write_text(json.dumps([case("two", "none")]))
        self.assertEqual(self.status("score", "--out", self.w.out), {"status": "case_sets_differ"})
        self.w.cases_path.write_text(json.dumps([case("one", "none")]))
        self.w.plan({"cli": {}, "other": 1})
        self.w.cases_path.write_text(json.dumps([case("one", "none"), case("two", "none")]))
        self.assertEqual(self.status("answers", "--reader", "claude-haiku", "--root", self.w.root, "--out", self.w.out,
                                     "--samples", "1"), {"status": "run_differs"})
        self.w.cases_path.write_text(json.dumps([case("one", "none")]))
        one = {"readers": {"r": {"n": 1, "passes": {"c": 1}, "unanswered": 0, "majority": ["c"]}}, "case_files": {}}
        (self.w.t / "s.json").write_text(json.dumps(one))
        self.assertEqual(self.status("gate", "--baseline", self.w.t / "s.json", "--candidate", self.w.t / "s.json"),
                         {"status": "samples_too_few"})


class Repository(unittest.TestCase):
    def setUp(self):
        self.cases = json.loads((REPO / "evals" / "intake" / "cases.json").read_text())

    def test_the_cases_are_valid_and_cover_what_the_suite_asks(self):
        mod = load(RUN, "intake_cases")
        self.assertGreaterEqual(len(self.cases), 12)
        loaded, digests = mod.load_cases([])
        self.assertEqual(len(loaded), len(self.cases))
        self.assertEqual(set(digests), {"cases.json"})
        lanes = {c["expect"]["lane"] for c in self.cases}
        self.assertEqual(lanes, {"objective", "small", "none"})
        self.assertEqual({c["seed"] for c in self.cases}, {"none", "drafted", "confirmed"})
        self.assertTrue(any(c["expect"]["out_of_order"] == "confirmation" for c in self.cases))
        self.assertTrue(any(c["expect"]["out_of_order"] == "intent" for c in self.cases))
        self.assertTrue(any(re.search("[가-힣]", c["words"]) for c in self.cases))
        by_id = {c["id"]: c for c in self.cases}
        self.assertTrue(by_id["new-objective-ko"]["seed"] == "none" and by_id["skip-confirmation-start"]["seed"] == "drafted")
        for needed in ("small-fix-word", "correction-drafted", "question-command", "wrong-word-approve", "wrong-word-ticket"):
            self.assertIn(needed, by_id)

    def test_no_case_orders_a_signal_a_restart_a_setting_or_a_message_and_none_names_a_host_path(self):
        for c in self.cases:
            self.assertIsNone(re.search(r"kill|signal|restart|reboot|systemctl|settings|send |mail|slack|push|/home/|/root/|/var/",
                                        c["words"], re.I), c["id"])

    def test_the_digest_file_holds_only_digest_lines(self):
        for line in (REPO / "evals" / "intake" / "host-cases.sha256").read_text().splitlines():
            self.assertRegex(line, r"^[0-9a-f]{64}  [a-z0-9-]+$")

    def test_the_run_names_the_isolation_and_the_routing_suite_is_loaded_not_copied(self):
        text = RUN.read_text()
        for word in ("ProtectSystem=strict", "InaccessiblePaths", "BindPaths", "CapabilityBoundingSet", "PrivateDevices",
                     "assume_intent", "NoNewPrivileges", "ReadOnlyPaths"):
            self.assertIn(word, text)
        self.assertNotIn("ProtectKernelTunables=yes", text.replace("not ProtectKernelTunables=yes", ""))
        self.assertIn("load_routing", text)
        self.assertNotIn("def run_sample", text)


if __name__ == "__main__":
    unittest.main()
