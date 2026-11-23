"""Tests of evals/batch/mining/build, synth and read, the mining job of the batch evaluations, run as programs against
a fake `ont` that serves invented reviews, units, intents and log rows and keeps findings in a file (no model, no
network, no engine, no store of the host). Reports are files of a temporary directory. Contract:
evals/batch/mining/README.md."""
import hashlib
import importlib.machinery
import importlib.util
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
MINING = REPO / "evals" / "batch" / "mining"
PY = sys.executable
HOME = os.environ.get("HOME", "/")
SINCE, UNTIL = "2026-11-16", "2026-11-22"
MARKER = "MARKER-" + "7f3a9c"  # a text that must reach no file


def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    mod = importlib.util.module_from_spec(importlib.util.spec_from_loader(name, loader))
    loader.exec_module(mod)
    return mod


LIB = load("batch_lib", REPO / "evals" / "batch" / "lib.py")
READOUT = load("readout", REPO / "evals" / "readout.py")
CAUSES = [c["cause"] for c in json.loads((MINING / "causes.json").read_text())]

# The fake engine: objects by type in ont-state.json beside its directory; log rows; every call recorded; request ids
# kept as the engine keeps them.
FAKE_ONT = r'''#!@PY@ -I
import json, os, sys
STATE = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "ont-state.json")
with open(STATE) as f:
    st = json.load(f)
args = sys.argv[1:]
raw = sys.stdin.read()
body = json.loads(raw) if raw else None
st["log"].append({"argv": list(args), "body": body})
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
    if st.get("unqualified") == kind:
        out({"status": "ok", "result": {"objects": [], "basis": {"qualified": False}, "continuation": None}})
    if st.get("search_fails") == kind:
        out({"status": "refused", "code": "store_unavailable"}, 3)
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
if args == ["log"]:
    if st.get("log_fails"):
        out({"status": "refused", "code": "store_unavailable"}, 3)
    rows = st.get("rows", [])
    start = int(body.get("continuation") or 0)
    page = min(int(body.get("limit", 100)), st.get("log_page", 2))
    chunk = rows[start:start + page]
    nxt = str(start + page) if start + page < len(rows) else None
    if st.get("log_repeats"):
        nxt = "same"
    result = {"rows": chunk, "withheld": st.get("withheld", 0) if start == 0 else 0, "continuation": nxt}
    if st.get("log_no_withheld"):
        del result["withheld"]
    out({"status": "ok", "result": result})
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
    if st.get("refuse_open") and a == "open_finding" and key.endswith(st["refuse_open"]):
        crit("detector_unknown")
    if a == "open_finding":
        if f:
            out({"status": "refused", "code": "revision_conflict"}, 2)
        f = {k: p[k] for k in ("kind", "detector", "summary", "evidence", "severity") if k in p}
        f["state"] = "open"
    elif a == "see_finding_again":
        if not f or f["state"] not in ("open", "planned"):
            crit("finding_not_live")
        f["evidence"] = p.get("evidence")
    else:
        out({"status": "refused", "code": "unknown_action"}, 2)
    findings[key] = f
    st["rids"][rid] = content
    out({"status": "ok", "result": {"outcome": "applied", "edits": [{"change": "modify", "id": p["finding"]}]}})
out({"status": "refused", "code": "usage"}, 2)
'''


def sha(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def canon(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def oid(kind, key):
    return {"object_type": kind, "namespace": LIB.NAMESPACE, "native_key": key}


def fkey(signature):
    return "batch-mining-" + sha(signature)[:16]


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


def verdict(cause, machine=False, check="", why="because"):
    return {"cause": cause, "machine": machine, "check": check, "why": why}


def report(*lines):
    return "".join(x + "\n" for x in lines)


# The reports of the invented week.
REPORT_A = report("Verdict: repair", "0123abcd", "blocking check: the acceptance check passes on the base.",
                  "  evidence line", "- blocking write_set: needs ci/x.py.", "note: only a nit", "What I ran",
                  "  ran a thing")
REPORT_B = report("Verdict: repair", "0123abcd", "1. blocking dependency: named for a plan, not a page", "detail line")
REPORT_C = report("Verdict: repair", "0123abcd", "blocking dependency: unit u3 is missing from the plan.", "  Add it.",
                  "", "## What I ran", "ran")
REPORT_D = report("Verdict: repair", "0123abcd", "note: only a note", "What I ran", "ran")
REPORT_E = report("Verdict: repair", "deadbeef", "blocking wrong behaviour: x", "more")


class Case(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.tmp = Path(os.path.realpath(tmp.name))
        self.bin = self.tmp / "bin"
        self.bin.mkdir()
        (self.bin / "ont").write_text(FAKE_ONT.replace("@PY@", PY))
        (self.bin / "ont").chmod(0o755)
        self.reports = self.tmp / "reports"
        self.reports.mkdir()
        self.put({"objects": {}, "rows": [], "rids": {}, "log": []})
        self.n = 0

    def path(self, name):
        self.n += 1
        return self.tmp / f"{self.n}-{name}"

    # The fake's state.
    def state(self):
        return json.loads((self.tmp / "ont-state.json").read_text())

    def put(self, st):
        (self.tmp / "ont-state.json").write_text(json.dumps(st))

    def set(self, **flags):
        st = self.state()
        st.update(flags)
        self.put(st)

    def add(self, otype, key, **fields):
        st = self.state()
        st["objects"].setdefault(otype, {})[key] = {k: v for k, v in fields.items() if v is not None}
        self.put(st)

    def stop(self, action_id, at, unit, cause="limit", detail="hit the limit"):
        st = self.state()
        st["rows"].append({"action_id": action_id, "action_type": "stop_unit", "submitted_at": at,
                           "parameters": {"unit": oid("unit", unit), "cause": cause, "detail": detail}})
        self.put(st)

    def write_report(self, name, text):
        p = self.reports / name
        p.write_text(text)
        return str(p)

    def review(self, key, kind, verdict_, closed_at, text=None, unit=None, intent=None, repository=None, report_path=None,
               **extra):
        if text is not None:
            report_path = self.write_report(key + ".md", text)
        self.add("review", key, kind=kind, status="closed", verdict=verdict_, closed_at=closed_at,
                 report=report_path, unit=oid("unit", unit) if unit else None,
                 intent=oid("intent", intent) if intent else None, repository=repository, **extra)

    def unit(self, key, repository="o/n", intent="i1", cls="plain", lane="objective"):
        self.add("unit", key, repository=repository, intent=oid("intent", intent) if intent else None,
                 **{"class": cls}, lane=lane, state="merged")

    def intent(self, key, directory="/w/i1"):
        self.add("intent", key, directory=directory, status="confirmed")

    def applies(self):
        return [e for e in self.state()["log"] if e["argv"][-4:-2] == ["action", "apply"]]

    def calls(self):
        return self.state()["log"]

    # The programs.
    def env(self):
        return {"PATH": os.environ["PATH"], "HOME": HOME, "ONT_BIN": str(self.bin / "ont")}

    def prog(self, name, args, code=0):
        env = self.env()
        self.assertTrue(os.path.isabs(env["ONT_BIN"]))
        p = subprocess.run([PY, "-I", "-B", str(MINING / name), *map(str, args)], capture_output=True, text=True,
                           env=env)
        self.assertEqual(p.stderr, "")
        lines = p.stdout.splitlines()
        self.assertEqual(len(lines), 1, p.stdout)
        self.assertEqual(p.returncode, code, p.stdout)
        return json.loads(lines[0])

    def build(self, extra=(), out=None, code=0, since=SINCE, until=UNTIL):
        out = out or self.path("out")
        v = self.prog("build", ["--since", since, "--until", until, "--model", "m", "--out", out, *extra], code)
        if code:
            return v
        self.assertEqual(v["status"], "ok")
        items = json.loads((Path(out) / "items.json").read_text())
        reqs = [json.loads(x) for x in (Path(out) / "requests.jsonl").read_text().splitlines()]
        return v, items, reqs, Path(out)

    def synth(self, items, results, extra=(), out=None, code=0):
        out = out or self.path("out")
        v = self.prog("synth", ["--items", items, *sum((["--results", r] for r in results), []), "--model", "m2",
                                "--out", out, *extra], code)
        if code:
            return v
        synthesis = json.loads((Path(out) / "synthesis.json").read_text())
        reqs = [json.loads(x) for x in (Path(out) / "requests.jsonl").read_text().splitlines()]
        return v, synthesis, reqs, Path(out)

    def read(self, synthesis, results, extra=(), code=0):
        return self.prog("read", ["--synthesis", synthesis, "--results", results, *extra], code)

    def results(self, *lines):
        p = self.path("results.jsonl")
        p.write_text("".join(x + "\n" for x in lines))
        return p

    def week(self):
        """The invented week: eight items in the window, and what must not be one."""
        self.intent("i1")
        self.unit("u1")
        self.unit("u2", cls="contract")
        self.review("rq-a", "page", "repair", "2026-11-17T10:00:00Z", REPORT_A, unit="u1")
        self.review("rq-b", "page", "repair", "2026-11-18T09:00:00Z", REPORT_B, unit="u2")
        self.review("rq-c", "plan", "repair", "2026-11-19T10:00:00Z", REPORT_C, intent="i1")
        self.review("rq-d", "page", "repair", "2026-11-20T10:00:00Z", REPORT_D, unit="u1")
        self.review("rq-e", "packet", "repair", "2026-11-20T12:00:00Z", REPORT_E, unit="u2")
        # No item, and no report is read: the paths name nothing.
        gone = str(self.reports / "gone.md")
        self.review("rq-f", "page", "abandoned", "2026-11-18T10:00:00Z", unit="u1", report_path=gone)
        self.review("rq-g", "page", "accept", "2026-11-18T11:00:00Z", unit="u1", report_path=gone,
                    carried_from=oid("review", "rq-a"))
        self.review("rq-h", "page", "repair", "2026-11-30T11:00:00Z", REPORT_A, unit="u1")
        self.review("rq-i", "design", "repair", "2026-11-18T12:00:00Z", REPORT_A, unit="u1")
        self.review("rq-j", "packet", "accept", "2026-11-18T13:00:00Z", REPORT_E, unit="u1")
        self.stop("stop-1", "2026-11-18T08:00:00Z", "u1")
        self.stop("stop-2", "2026-11-21T23:59:59Z", "u2", "failed_check", "line one\nline two")
        self.stop("stop-3", "2026-11-23T00:00:01Z", "u1", "late", "outside the window")
        self.set(withheld=2)

    ITEM_IDS = ["rq-a#1", "rq-a#2", "stop-1", "rq-b#1", "rq-c#1", "rq-d", "rq-e", "stop-2"]

    def classes(self):
        return {"rq-a#1": verdict("base_claim", True, "Refuse a path the base lacks."),
                "rq-a#2": verdict("weak_check"), "stop-1": verdict("environment"),
                "rq-b#1": verdict("base_claim", True, "Refuse a name the base lacks."), "rq-c#1": verdict("other"),
                "rq-d": verdict("base_claim"), "rq-e": verdict("weak_check"), "stop-2": verdict("environment")}

    def classified(self, items, classes=None):
        classes = classes or self.classes()
        return self.results(*(succeeded(i["key"], classes[i["item"]]) for i in items["items"] if i["item"] in classes))

    SYNTHESIS = {"causes": [
        {"cause": "base_claim", "change": "Refuse a page that names a path the base lacks, in `ap unit ready`.",
         "why": "Three items name facts the base did not hold."},
        {"cause": "weak_check", "change": "Add this\tsentence.\nSecond line.", "why": "Two checks passed on the base."},
        {"cause": "environment", "change": "Name the limit in the page.", "why": "Both stops were the host's."}]}

    def to_synthesis(self):
        self.week()
        v, items, reqs, out = self.build()
        sv, synthesis, sreqs, sout = self.synth(out / "items.json", [self.classified(items)])
        return items, synthesis, sreqs, sout


class BatchMiningTest(Case):
    def test_invented_week_gives_expected_causes(self):
        self.week()
        v, items, reqs, out = self.build()
        self.assertEqual([i["item"] for i in items["items"]], self.ITEM_IDS)
        self.assertEqual((v["items"], v["requested"], v["settled"], v["over_max"]), (8, 8, 0, 0))
        self.assertEqual((v["blocking"], v["repair"], v["stop"]), (4, 2, 2))
        self.assertEqual((v["academy_skipped"], v["report_unreadable"], v["stops_withheld"],
                          v["result_lines_skipped"]), (0, 0, 2, 0))
        self.assertEqual((items["job"], items["since"], items["until"], items["model"]), ("mining", SINCE, UNTIL, "m"))
        by = {i["item"]: i for i in items["items"]}
        want = {
            "rq-a#1": ("blocking", "check", "rq-a", "u1", "i1", "2026-11-17T10:00:00Z",
                       "blocking check: the acceptance check passes on the base.\n  evidence line"),
            "rq-a#2": ("blocking", "write_set", "rq-a", "u1", "i1", "2026-11-17T10:00:00Z",
                       "- blocking write_set: needs ci/x.py."),
            "stop-1": ("stop", "limit", None, "u1", "i1", "2026-11-18T08:00:00Z", "cause: limit\ndetail: hit the limit"),
            "rq-b#1": ("blocking", None, "rq-b", "u2", "i1", "2026-11-18T09:00:00Z",
                       "1. blocking dependency: named for a plan, not a page\ndetail line"),
            "rq-c#1": ("blocking", "dependency", "rq-c", None, "i1", "2026-11-19T10:00:00Z",
                       "blocking dependency: unit u3 is missing from the plan.\n  Add it.\n"),
            "rq-d": ("repair", None, "rq-d", "u1", "i1", "2026-11-20T10:00:00Z", "note: only a note\nWhat I ran\nran"),
            "rq-e": ("repair", None, "rq-e", "u2", "i1", "2026-11-20T12:00:00Z", "blocking wrong behaviour: x\nmore"),
            "stop-2": ("stop", "failed_check", None, "u2", "i1", "2026-11-21T23:59:59Z",
                       "cause: failed_check\ndetail: line one\nline two"),
        }
        for item_id, (kind, name, review, unit, intent, at, text) in want.items():
            i = by[item_id]
            self.assertEqual((i["kind"], i["name"], i["review"], i["unit"], i["intent"], i["at"], i["text"], i["state"]),
                             (kind, name, review, unit, intent, at, text, "requested"), item_id)
        self.assertNotIn("What I ran", by["rq-c#1"]["text"])
        self.assertNotIn("What I ran", by["rq-a#1"]["text"])
        # The queries: every page of three searches, and the log with the day after the window's end.
        searches = [c["body"] for c in self.calls() if c["argv"][-1:] == ["search"]]
        self.assertEqual({s["object_type"] for s in searches}, {"review", "unit", "intent"})
        reviews = [s for s in searches if s["object_type"] == "review"]
        self.assertEqual(reviews[0]["where"], {"all": [{"eq": ["status", "closed"]}]})
        self.assertGreater(len(reviews), 1)
        units = next(s for s in searches if s["object_type"] == "unit")
        self.assertEqual(units["where"], {"all": [{"in": ["state", ["stopped", "accepted", "running", "ready", "draft",
                                                                      "merged", "done", "dropped"]]}]})
        intents = next(s for s in searches if s["object_type"] == "intent")
        self.assertEqual(intents["where"], {"all": [{"in": ["status", ["draft", "assumed", "confirmed", "met",
                                                                        "retired"]]}]})
        logs = [c["body"] for c in self.calls() if c["argv"][-1:] == ["log"]]
        self.assertEqual(len(logs), 2)
        self.assertEqual({k: v for k, v in logs[0].items()},
                         {"action_type": "stop_unit", "outcome": "applied", "since": "2026-11-16T00:00:00Z",
                          "until": "2026-11-23T00:00:00Z", "parameters": True, "limit": 1000})
        self.assertEqual(logs[1]["continuation"], "2")
        # The requests.
        schema = json.loads((MINING / "classify.schema.json").read_text())
        system = (MINING / "classify.md").read_text().rstrip("\n") + "\n\n" + "\n".join(
            f"{c['cause']}: {c['means']}" for c in json.loads((MINING / "causes.json").read_text())) + "\n"
        self.assertEqual(len(reqs), 8)
        for item, req in zip(items["items"], reqs):
            p = req["params"]
            self.assertEqual(set(p), {"model", "max_tokens", "system", "messages", "output_config"})
            self.assertEqual((p["model"], p["max_tokens"], p["system"]), ("m", 1024, system))
            self.assertEqual(p["output_config"], {"format": {"type": "json_schema", "schema": schema}})
            self.assertEqual((req["custom_id"], item["key"]), (sha(canon(p)), sha(canon(p))))
            self.assertEqual([m["role"] for m in p["messages"]], ["user"])
            self.assertTrue(p["messages"][0]["content"].endswith(item["text"]))
        first = reqs[0]["params"]["messages"][0]["content"]
        self.assertEqual(first.split("\n\n")[0], "Item kind: blocking\nItem name: check\nReview kind: page\n"
                         "Review verdict: repair\nUnit class: plain\nUnit lane: objective")
        stop = reqs[2]["params"]["messages"][0]["content"]
        self.assertIn("Review kind: none\nReview verdict: none\nUnit class: plain", stop)
        # What the program adds holds no key, time or path.
        for item, req in zip(items["items"], reqs):
            added = req["params"]["messages"][0]["content"][:-len(item["text"])]
            self.assertNotRegex(added, r"rq-|stop-|\bu[12]\b|\bi1\b|20[0-9]{2}-|/")
        # Classification, synthesis, reading.
        results = self.classified(items)
        sv, synthesis, sreqs, sout = self.synth(out / "items.json", [results])
        self.assertEqual((sv["causes"], sv["classified"], sv["unclassified"], sv["requested"], sv["read"]), (3, 8, 0, 1, 8))
        self.assertEqual(synthesis["causes"], [
            {"cause": "base_claim", "count": 3, "machine": 2, "remedy": "check", "items": ["rq-a#1", "rq-b#1", "rq-d"]},
            {"cause": "weak_check", "count": 2, "machine": 0, "remedy": "sentence", "items": ["rq-a#2", "rq-e"]},
            {"cause": "environment", "count": 2, "machine": 0, "remedy": "sentence", "items": ["stop-1", "stop-2"]}])
        self.assertEqual(synthesis["counts"], {"read": 8, "refused": 0, "errored": 0, "unreadable": 0, "stale": 0,
                                               "duplicate": 0, "skipped": 0})
        self.assertEqual((synthesis["job"], synthesis["since"], synthesis["until"], synthesis["model"]),
                         ("mining", SINCE, UNTIL, "m2"))
        self.assertEqual(len(sreqs), 1)
        sp = sreqs[0]["params"]
        self.assertEqual(synthesis["key"], sreqs[0]["custom_id"])
        self.assertEqual(sreqs[0]["custom_id"], sha(canon(sp)))
        self.assertEqual((sp["model"], sp["max_tokens"], sp["system"]), ("m2", 4000, (MINING / "synthesize.md").read_text()))
        self.assertEqual(sp["output_config"]["format"]["schema"],
                         json.loads((MINING / "synthesis.schema.json").read_text()))
        text = sp["messages"][0]["content"]
        self.assertIn(f"{SINCE}..{UNTIL}", text)
        self.assertIn("Cause: base_claim", text)
        self.assertIn("Refuse a path the base lacks.", text)
        self.assertIn("- id: rq-a#1", text)
        sresult = self.results(succeeded(synthesis["key"], self.SYNTHESIS))
        a = self.read(sout / "synthesis.json", sresult)
        sigs = [f"mining:{SINCE}..{UNTIL}:{c}" for c in ("base_claim", "weak_check", "environment")]
        self.assertEqual(a["opened"], [fkey(s) for s in sigs])
        self.assertEqual((a["status"], a["lines"], a["read"], a["seen"], a["left"], a["already_applied"], a["failed"]),
                         ("ok", 1, 1, [], 0, 0, []))
        found = self.state()["objects"]["finding"]
        f = found[fkey(sigs[0])]
        self.assertEqual((f["kind"], f["detector"], f["severity"], f["state"]), ("lesson", "batch:mining", "info", "open"))
        self.assertEqual(f["summary"], f"{SINCE}..{UNTIL} base_claim: 3 of 8 classified items, 2 of them a machine "
                                       "could have refused (check)")
        self.assertEqual(f["evidence"], "items rq-a#1,rq-b#1,rq-d; change: Refuse a page that names a path the base "
                                        "lacks, in `ap unit ready`.; why: Three items name facts the base did not hold.")
        g = found[fkey(sigs[1])]
        self.assertEqual(g["summary"], f"{SINCE}..{UNTIL} weak_check: 2 of 8 classified items, 0 of them a machine "
                                       "could have refused (sentence)")
        self.assertEqual(g["evidence"], "items rq-a#2,rq-e; change: Add this sentence. Second line.; why: Two checks "
                                        "passed on the base.")
        before = len(self.applies())
        again = self.read(sout / "synthesis.json", sresult)
        self.assertEqual((again["opened"], again["seen"], again["already_applied"], again["failed"]), ([], [], 0, []))
        self.assertEqual(len(self.applies()), before)

    def test_patterns_are_those_of_readout(self):
        common = load("mining_common", MINING / "common.py")
        self.assertEqual(common.BLOCKING_LINE.pattern, READOUT.BLOCKING_LINE.pattern)
        self.assertEqual(common.BLOCKING_LINE.flags, READOUT.BLOCKING_LINE.flags)
        self.assertEqual(common.BLOCKING_KINDS, READOUT.BLOCKING_KINDS)
        self.assertEqual(common.PREFIX, "bm-")

    def test_block_boundaries_and_cuts(self):
        self.intent("i1")
        self.unit("u1")
        long_line = "x" * 7000
        text = report("Verdict: repair", "d", "* blocking check: a", "  *  What I could not check", "blocking write_set: b",
                      "  1. note: n", "blocking c", "# What I ran", "blocking after the sections", "ü" * 3100)
        self.review("rq-a", "page", "repair", "2026-11-17T10:00:00Z", text, unit="u1")
        self.review("rq-b", "page", "accept", "2026-11-17T11:00:00Z", report("Verdict: accept", "d", "blocking check: " + long_line),
                    unit="u1")
        self.review("rq-c", "packet", "repair", "2026-11-17T12:00:00Z", report("Verdict: repair", "d") + "ü" * 7000,
                    unit="u1")
        v, items, reqs, out = self.build()
        got = [(i["item"], i["name"], i["text"]) for i in items["items"]]
        self.assertEqual(got[0], ("rq-a#1", "check", "* blocking check: a"))
        self.assertEqual(got[1], ("rq-a#2", "write_set", "blocking write_set: b"))
        self.assertEqual(got[2], ("rq-a#3", None, "blocking c"))
        self.assertEqual(got[3][0], "rq-a#4")
        self.assertLessEqual(len(got[3][2].encode()), 6000)
        self.assertEqual(got[3][2], ("blocking after the sections\n" + "ü" * 3100).encode()[:6000].decode("utf-8", "ignore"))
        b = next(i for i in items["items"] if i["item"] == "rq-b#1")
        self.assertEqual(len(b["text"].encode()), 6000)
        c = next(i for i in items["items"] if i["item"] == "rq-c")
        self.assertEqual(len(c["text"].encode()), 12000)
        self.assertEqual(c["kind"], "repair")

    def test_report_that_cannot_be_read(self):
        self.intent("i1")
        self.unit("u1")
        real = self.write_report("real.md", REPORT_A)
        os.symlink(real, self.reports / "link.md")
        os.symlink(self.reports, self.tmp / "dirlink")
        (self.reports / "big.md").write_bytes(b"x" * 1_048_577)
        (self.reports / "edge.md").write_bytes(b"Verdict: repair\nd\n" + b"blocking \xff\xfe check\n" + b"y" * (1_048_576 - 40))
        self.reports.joinpath("dir.md").mkdir()
        paths = {"rq-1": "relative/report.md", "rq-2": str(self.reports / "link.md"), "rq-3": str(self.tmp / "dirlink" / "real.md"),
                 "rq-4": str(self.reports / "big.md"), "rq-5": str(self.reports / "dir.md"),
                 "rq-6": str(self.reports / "gone.md"), "rq-7": None, "rq-8": str(self.reports / ".." / "reports" / "real.md")}
        for k, p in paths.items():
            self.review(k, "page", "repair", "2026-11-17T10:00:00Z", unit="u1", report_path=p)
        self.review("rq-9", "page", "repair", "2026-11-17T10:00:00Z", unit="u1", report_path=str(self.reports / "edge.md"))
        v, items, reqs, out = self.build()
        self.assertEqual(v["report_unreadable"], 8)
        self.assertEqual([i["item"] for i in items["items"]], ["rq-9#1"])
        self.assertIn("�", items["items"][0]["text"])

    def test_academy_reports_are_never_opened(self):
        self.intent("i1")
        self.unit("u1")
        self.intent("iA", directory="/w/edu-math/task")
        self.intent("iB")
        self.intent("iC")
        self.unit("uR", repository="o/education-platform", intent="i1")
        self.unit("uD", intent="iA")
        self.unit("uS", intent="iB", repository=None)  # no repository itself; its intent's other units are the Academy's
        self.unit("uS1", intent="iB", repository="o/education-p")
        self.unit("uS2", intent="iB", repository="o/edu-q")
        self.unit("uM1", intent="iC", repository="o/education-p")
        self.unit("uM2", intent="iC", repository="o/n")
        self.unit("uOk", intent="iC", repository=None)
        real = self.write_report("marker.md", report("Verdict: repair", "d", "blocking check: " + MARKER))
        gone = str(self.reports / "nothing-here.md")
        cases = [
            ("rq-1", dict(kind="page", unit="u1", repository="o/education-platform")),
            ("rq-2", dict(kind="page", unit="uR")),
            ("rq-3", dict(kind="page", unit="uD")),
            ("rq-4", dict(kind="plan", intent="iA")),
            ("rq-5", dict(kind="plan", intent="iB")),
            ("rq-6", dict(kind="page", unit="uS")),
            ("rq-7", dict(kind="packet", unit="uR")),
            ("rq-8", dict(kind="page", unit="u1", repository="edu-x/y")),
        ]
        for n, (k, c) in enumerate(cases):
            path = real if n % 2 == 0 else gone
            self.review(k, c.pop("kind"), "repair", "2026-11-17T10:00:00Z", report_path=path, **c)
        # A report that is itself in an Academy path is not opened either.
        self.review("rq-9", "page", "repair", "2026-11-17T10:00:00Z", unit="u1",
                    report_path=str(self.reports / "edu-sub" / "r.md"))
        # Stops: the unit's repository, its intent's directory, its intent's units.
        self.stop("s-1", "2026-11-17T10:00:00Z", "uR", detail=MARKER)
        self.stop("s-2", "2026-11-17T10:00:00Z", "uD", detail=MARKER)
        self.stop("s-3", "2026-11-17T10:00:00Z", "uS", detail=MARKER)
        # Not the Academy's: a mixed intent, a unit of an intent with a mixed set, a stop of a plain unit.
        self.review("rq-ok", "page", "repair", "2026-11-17T10:00:00Z", report("Verdict: repair", "d", "blocking check: fine"),
                    unit="uOk")
        self.stop("s-ok", "2026-11-17T10:00:00Z", "uM2", detail="fine")
        v, items, reqs, out = self.build()
        self.assertEqual((v["academy_skipped"], v["report_unreadable"]), (12, 0))
        self.assertEqual([i["item"] for i in items["items"]], ["rq-ok#1", "s-ok"])
        for p in out.iterdir():
            self.assertNotIn(MARKER, p.read_text())
        self.assertNotIn(MARKER, canon(reqs))
        # Every path argument of the three programs, as a string that names nothing.
        refused = {"status": "academy_path"}
        before = self.calls()
        edu, education = self.tmp / "edu-x", self.tmp / "education-y" / "f"
        plain = self.tmp / "plain-file"
        b = ["--since", SINCE, "--until", UNTIL, "--model", "m"]
        for extra in (["--out", edu], ["--out", plain, "--results", education], ["--out", plain, "--store", edu],
                      ["--out", plain, "--results", self.tmp / "x" / "evals" / "lesson" / "r"]):
            self.assertEqual(self.prog("build", b + extra, 2), refused, extra)
        for extra in (["--items", education, "--results", plain, "--out", plain],
                      ["--items", plain, "--results", edu, "--out", plain],
                      ["--items", plain, "--results", plain, "--out", self.tmp / "evals" / "lesson" / "o"]):
            self.assertEqual(self.prog("synth", ["--model", "m", *extra], 2), refused, extra)
        for extra in (["--synthesis", edu, "--results", plain], ["--synthesis", plain, "--results", education],
                      ["--synthesis", plain, "--results", plain, "--store", education]):
            self.assertEqual(self.prog("read", extra, 2), refused, extra)
        self.assertEqual(self.calls(), before)
        self.assertFalse(plain.exists())
        self.assertFalse(any(p.name.startswith(("edu-", "education-")) for p in self.tmp.rglob("*")))

    def test_keys_and_settled_results(self):
        self.week()
        self.stop("stop-4", "2026-11-18T08:00:00Z", "u1")  # equal params with stop-1: one request
        v, items, reqs, out = self.build()
        self.assertEqual((v["items"], v["requested"], v["settled"]), (9, 8, 0))
        self.assertEqual(len(reqs), 8)
        keys = [i["key"] for i in items["items"]]
        self.assertEqual(len(set(keys)), 8)
        a = {i["item"]: i["key"] for i in items["items"]}
        self.assertEqual(a["stop-1"], a["stop-4"])
        self.assertEqual([r["custom_id"] for r in reqs], list(dict.fromkeys(keys)))
        for r in reqs:
            self.assertEqual(r["custom_id"], sha(canon(r["params"])))
        # Byte-equal files for equal inputs.
        v2, items2, reqs2, out2 = self.build()
        for name in ("requests.jsonl", "items.json"):
            self.assertEqual((out / name).read_bytes(), (out2 / name).read_bytes())
        self.assertEqual((out / "requests.jsonl").stat().st_mode & 0o777, 0o600)
        self.assertEqual(out.stat().st_mode & 0o777, 0o700)
        # A changed model or max_tokens changes every key.
        v3, items3, reqs3, out3 = self.build(extra=["--max-tokens", "2000"])
        self.assertTrue(set(i["key"] for i in items3["items"]).isdisjoint(keys))
        # A second build given the first's result file requests nothing; an error is asked again.
        errored = items["items"][0]["key"]
        refused = items["items"][1]["key"]
        res = self.results(*(succeeded(k, verdict("other")) for k in dict.fromkeys(keys) if k not in (errored, refused)),
                           failed_line(errored), succeeded(refused, None, stop="refusal", text=""), "not json", "")
        v4, items4, reqs4, out4 = self.build(extra=["--results", res])
        self.assertEqual((v4["requested"], v4["settled"], v4["result_lines_skipped"]), (1, 7, 1))
        self.assertEqual([r["custom_id"] for r in reqs4], [errored])
        self.assertEqual({i["state"] for i in items4["items"] if i["key"] != errored}, {"settled"})
        res2 = self.results(*(succeeded(k, verdict("other")) for k in dict.fromkeys(keys)))
        v5, items5, reqs5, out5 = self.build(extra=["--results", res, "--results", res2])
        self.assertEqual((v5["requested"], v5["settled"]), (0, 8))
        self.assertEqual((out5 / "requests.jsonl").read_bytes(), b"")
        self.build(extra=["--results", self.tmp / "missing.jsonl"], code=2)
        self.assertEqual(self.build(extra=["--results", self.tmp / "missing.jsonl"], code=2), {"status": "results_unreadable"})

    def test_max_requests(self):
        self.week()
        v, items, reqs, out = self.build(extra=["--max-requests", "3"])
        self.assertEqual((v["requested"], v["over_max"], v["settled"]), (3, 5, 0))
        self.assertEqual([i["state"] for i in items["items"]], ["requested"] * 3 + ["over_max"] * 5)
        self.assertEqual([r["custom_id"] for r in reqs], [i["key"] for i in items["items"][:3]])
        res = self.results(succeeded(items["items"][1]["key"], verdict("other")))
        v2, items2, reqs2, out2 = self.build(extra=["--max-requests", "3", "--results", res])
        self.assertEqual([i["state"] for i in items2["items"]],
                         ["requested", "settled", "requested", "requested"] + ["over_max"] * 4)
        for bad in (["--max-requests", "0"], ["--max-requests", "100001"], ["--max-requests", "x"],
                    ["--max-tokens", "255"], ["--max-tokens", "8001"], ["--max-tokens", "1e3"], ["--bogus", "1"]):
            self.assertEqual(self.build(extra=bad, code=2), {"status": "usage"}, bad)
        for since, until in (("2026-11-23", "2026-11-22"), ("2026-10-01", "2026-11-02"), ("2026-02-30", "2026-03-01"),
                             ("2026-11-1", "2026-11-22"), ("", "2026-11-22")):
            self.assertEqual(self.build(since=since, until=until, code=2), {"status": "usage"}, (since, until))
        v3, items3, reqs3, out3 = self.build(since="2026-10-01", until="2026-11-01")  # 31 days apart
        self.assertEqual(v3["items"], 0)
        self.assertEqual((out3 / "requests.jsonl").read_bytes(), b"")
        v4, items4, reqs4, out4 = self.build(since="2026-11-17", until="2026-11-17")
        self.assertEqual([i["item"] for i in items4["items"]], ["rq-a#1", "rq-a#2"])

    def test_store_failures_write_nothing(self):
        self.week()
        for flag, value in (("unqualified", "review"), ("unqualified", "unit"), ("search_fails", "intent"),
                            ("log_fails", True), ("log_repeats", True), ("log_no_withheld", True)):
            self.set(**{flag: value})
            out = self.path("out")
            self.assertEqual(self.build(out=out, code=3), {"status": "store_failed"}, (flag, value))
            self.assertFalse(out.exists())
            self.set(**{flag: None})
        st = self.state()
        st["rows"].append({"action_id": "x", "submitted_at": "2026-11-18T00:00:00Z", "parameters": "no"})
        self.put(st)
        self.assertEqual(self.build(code=3), {"status": "store_failed"})

    def test_results_that_are_not_read_make_no_cause(self):
        self.week()
        v, items, reqs, out = self.build()
        k = {i["item"]: i["key"] for i in items["items"]}
        lines = [
            succeeded(k["rq-a#1"], verdict("base_claim", True, "c")),
            succeeded(k["rq-a#2"], None, stop="refusal", text=""),
            failed_line(k["rq-b#1"]),
            succeeded(k["rq-c#1"], None, stop="max_tokens", text='{"cause": "oth'),
            succeeded(k["rq-d"], verdict("not_a_cause")),
            succeeded(k["rq-e"], {"cause": "other", "machine": "yes", "check": "", "why": ""}),
            succeeded(k["stop-1"], verdict("environment")),
            succeeded(k["stop-1"], verdict("other")),  # a second line of a key
            succeeded("0" * 64, verdict("other")),  # no item
            "garbage",
        ]
        sv, synthesis, sreqs, sout = self.synth(out / "items.json", [self.results(*lines)])
        self.assertEqual(synthesis["counts"], {"read": 2, "refused": 1, "errored": 1, "unreadable": 3, "stale": 1,
                                               "duplicate": 1, "skipped": 1})
        self.assertEqual((synthesis["classified"], synthesis["unclassified"]), (2, 6))
        self.assertEqual([(c["cause"], c["count"]) for c in synthesis["causes"]], [("base_claim", 1), ("environment", 1)])
        self.assertEqual(sv["read"], 2)
        # The first succeeded line of a key is the one counted, whatever the order.
        sv2, synthesis2, _, _ = self.synth(out / "items.json", [self.results(failed_line(k["stop-1"]),
                                                                              succeeded(k["stop-1"], verdict("other")))])
        self.assertEqual((synthesis2["counts"]["read"], synthesis2["counts"]["duplicate"]), (1, 1))
        # Two files are one list of lines.
        sv3, synthesis3, _, _ = self.synth(out / "items.json", [self.results(lines[0]), self.results(lines[6])])
        self.assertEqual(synthesis3["classified"], 2)
        # No cause: an empty file and no key.
        sv4, synthesis4, sreqs4, sout4 = self.synth(out / "items.json", [self.results(lines[1], lines[2])])
        self.assertEqual((synthesis4["key"], synthesis4["causes"], sreqs4, sv4["requested"], sv4["causes"]), (None, [], [], 0, 0))
        self.assertEqual((sout4 / "requests.jsonl").read_bytes(), b"")
        # Unreadable inputs.
        bad = self.path("bad.json")
        bad.write_text("{}")
        self.assertEqual(self.synth(bad, [self.results()], code=2), {"status": "items_unreadable"})
        self.assertEqual(self.synth(out / "items.json", [self.tmp / "none"], code=2), {"status": "results_unreadable"})
        # The reader: a synthesis line that is refused, errored, cut or of another key causes no action.
        key = synthesis["key"]
        for line in (succeeded(key, None, stop="refusal", text=""), failed_line(key), failed_line(key, "expired"),
                     succeeded(key, None, stop="max_tokens", text='{"causes": ['),
                     succeeded(key, {"causes": [{"cause": "other"}]}), succeeded(k["rq-d"], self.SYNTHESIS),
                     "garbage"):
            a = self.read(sout / "synthesis.json", self.results(line))
            self.assertEqual((a["opened"], a["seen"], a["failed"], a["read"]), ([], [], [], 0), line)
        a = self.read(sout / "synthesis.json", self.results(failed_line(key), succeeded(key, self.SYNTHESIS)))
        self.assertEqual((a["read"], a["duplicate"], len(a["opened"])), (1, 1, 2))
        self.assertEqual(len(self.applies()), 2)
        self.assertEqual(self.read(sout / "synthesis.json", self.results(), code=0)["lines"], 0)
        self.assertEqual(self.read(sout / "synthesis.json", self.tmp / "none", code=2), {"status": "results_unreadable"})
        self.assertEqual(self.read(bad, self.results(), code=2), {"status": "synthesis_unreadable"})
        # No cause and a read line of the empty key causes none.
        a = self.read(sout4 / "synthesis.json", self.results(succeeded("0" * 64, self.SYNTHESIS)))
        self.assertEqual((a["read"], a["stale"], a["opened"]), (0, 1, []))

    def test_findings_by_state(self):
        items, synthesis, sreqs, sout = self.to_synthesis()
        key = synthesis["key"]
        sig = {c: f"mining:{SINCE}..{UNTIL}:{c}" for c in ("base_claim", "weak_check", "environment")}
        keys = {c: fkey(s) for c, s in sig.items()}
        self.add("finding", keys["base_claim"], kind="lesson", detector="batch:mining", summary="s", evidence="old",
                 severity="info", state="dismissed")
        self.add("finding", keys["weak_check"], kind="lesson", detector="batch:mining", summary="s", evidence="old",
                 severity="info", state="open")
        self.add("finding", keys["environment"], kind="lesson", detector="batch:mining", summary="s", evidence="old",
                 severity="info", state="closed")
        other = fkey("other-detector")
        self.add("finding", other, kind="drift", detector="batch:drift", summary="s", evidence="old", severity="info",
                 state="open")
        res = self.results(succeeded(key, self.SYNTHESIS))
        a = self.read(sout / "synthesis.json", res)
        self.assertEqual((a["opened"], a["seen"], a["left"], a["already_applied"]), ([], [keys["weak_check"]], 2, 0))
        st = self.state()["objects"]["finding"]
        self.assertEqual((st[keys["base_claim"]]["state"], st[keys["environment"]]["state"], st[keys["environment"]]["evidence"]),
                         ("dismissed", "closed", "old"))
        self.assertEqual(st[other]["evidence"], "old")
        self.assertEqual(len(self.applies()), 1)
        e = self.applies()[0]["body"]
        self.assertEqual(e["action_type"], "see_finding_again")
        self.assertEqual(set(e["parameters"]), {"finding", "evidence"})
        # The same evidence again: nothing. A planted planned finding with the same evidence: nothing.
        a = self.read(sout / "synthesis.json", res)
        self.assertEqual((a["seen"], a["left"], len(self.applies())), ([], 2, 1))
        # An evidence changed from A to B and back to A answers already_applied and leaves B stored.
        def with_change(text):
            return self.results(succeeded(key, {"causes": [{"cause": "weak_check", "change": text, "why": "w"}]}))
        for text in ("A", "B"):
            a = self.read(sout / "synthesis.json", with_change(text))
            self.assertEqual(a["seen"], [keys["weak_check"]], text)
        self.assertEqual(self.state()["objects"]["finding"][keys["weak_check"]]["evidence"], "items rq-a#2,rq-e; change: B; why: w")
        a = self.read(sout / "synthesis.json", with_change("A"))
        self.assertEqual((a["seen"], a["already_applied"], a["failed"]), ([], 1, []))
        self.assertEqual(self.state()["objects"]["finding"][keys["weak_check"]]["evidence"], "items rq-a#2,rq-e; change: B; why: w")
        # A verdict without an entry for the cause: change none, why none.
        a = self.read(sout / "synthesis.json", self.results(succeeded(key, {"causes": []})))
        self.assertEqual(a["seen"], [keys["weak_check"]])
        self.assertTrue(self.state()["objects"]["finding"][keys["weak_check"]]["evidence"].endswith("change: none; why: none"))

    def test_read_failures(self):
        items, synthesis, sreqs, sout = self.to_synthesis()
        res = self.results(succeeded(synthesis["key"], self.SYNTHESIS))
        self.set(unqualified="finding")
        before = len(self.calls())
        self.assertEqual(self.read(sout / "synthesis.json", res, code=3), {"status": "store_failed"})
        self.assertEqual(len(self.applies()), 0)
        self.set(unqualified=None, refuse_open=fkey(f"mining:{SINCE}..{UNTIL}:weak_check")[-16:])
        a = self.read(sout / "synthesis.json", res, code=1)
        self.assertEqual(a["failed"], [{"key": fkey(f"mining:{SINCE}..{UNTIL}:weak_check"), "code": "detector_unknown"}])
        self.assertEqual(len(a["opened"]), 2)
        self.assertGreater(len(self.calls()), before)

    def test_closed_list(self):
        causes = json.loads((MINING / "causes.json").read_text())
        names = [c["cause"] for c in causes]
        self.assertEqual(names, ["base_claim", "outside_write_set", "class_or_lane", "contradiction", "weak_check",
                                 "unguarded_effect", "plan_order", "build_defect", "environment", "other"])
        self.assertTrue(all(set(c) == {"cause", "means"} and c["means"] for c in causes))
        schema = json.loads((MINING / "classify.schema.json").read_text())
        self.assertEqual(schema["properties"]["cause"]["enum"], names)
        self.assertEqual(schema["properties"]["machine"]["type"], "boolean")
        self.assertEqual(set(schema["required"]), set(schema["properties"]))
        self.assertEqual(set(schema["properties"]), {"cause", "machine", "check", "why"})
        self.assertIs(schema["additionalProperties"], False)
        readme = (MINING / "README.md").read_text()
        section = readme.split("## The closed list")[1].split("\n## ")[0]
        self.assertEqual(re.findall(r"^- `([a-z_]+)`:", section, re.M), names)
        synth = json.loads((MINING / "synthesis.schema.json").read_text())
        item = synth["properties"]["causes"]["items"]
        self.assertEqual((set(synth["required"]), synth["additionalProperties"]), ({"causes"}, False))
        self.assertEqual((set(item["required"]), set(item["properties"]), item["additionalProperties"]),
                         ({"cause", "change", "why"},) * 2 + (False,))
        for name in ("build", "synth", "read"):
            p = MINING / name
            self.assertEqual(p.stat().st_mode & 0o777, 0o755, name)
            self.assertEqual(p.read_text().splitlines()[0], "#!/usr/bin/env -S python3 -I -B")

    def test_sources_hold_no_network_and_no_host_path(self):
        files = [*MINING.iterdir(), REPO / "ci" / "test_batch_mining.py"]
        banned_imports = re.compile(r"^\s*(import|from)\s+(urllib|http|socket|ssl|asyncio|requests|httpx|anthropic)\b", re.M)
        host = "/home/" + "palantirkc"
        for p in files:
            if p.suffix in (".py", "") and p.name != "README.md":
                text = p.read_text()
                self.assertIsNone(banned_imports.search(text), p.name)
                self.assertNotIn("api.anthropic.com", text.replace("api.anthropic" + ".com", ""), p.name)
            text = p.read_text()
            self.assertNotIn(host, text, p.name)
            self.assertNotIn("/var/lib/" + "agent-platform/store", text, p.name)
        # No requirement line and no citation of the specs in a file of this unit.
        req, cite = re.compile(r"^- R[0-9]+:", re.M), re.compile("spec" + r": specs/")
        for p in files:
            self.assertIsNone(req.search(p.read_text()), p.name)
            self.assertIsNone(cite.search(p.read_text()), p.name)

    def test_actions_fit_the_schema(self):
        items, synthesis, sreqs, sout = self.to_synthesis()
        key = synthesis["key"]
        self.add("finding", fkey(f"mining:{SINCE}..{UNTIL}:weak_check"), kind="lesson", detector="batch:mining",
                 summary="s", evidence="old", severity="info", state="open")
        self.read(sout / "synthesis.json", self.results(succeeded(key, self.SYNTHESIS)))
        schema = json.loads((REPO / "ontology" / "work.schema.json").read_text())
        host = (REPO / "crates" / "ap" / "src" / "host.rs").read_text()
        m = re.search(r'fn id\(kind: &str, key: &str\) -> Value \{\s*json!\(\{"object_type":kind,"namespace":(\{[^}]*\})', host)
        self.assertIsNotNone(m)
        namespace = json.loads(m.group(1))
        self.assertEqual(LIB.NAMESPACE, namespace)
        types = schema["value_types"]
        seen = set()
        applies = self.applies()
        self.assertEqual(len(applies), 3)
        for e in applies:
            body = e["body"]
            self.assertEqual(set(body), {"action_type", "parameters", "agent", "request_id"})
            self.assertEqual(body["agent"], "batch-mining")
            self.assertRegex(body["request_id"], r"^bm-[0-9a-f]{40}$")
            self.assertEqual(e["argv"][-4:], ["action", "apply", "--client", "batch-mining"])
            action = schema["action_types"][body["action_type"]]
            seen.add(body["action_type"])
            declared = action["parameters"]
            p = body["parameters"]
            self.assertLessEqual(set(p), set(declared))
            self.assertLessEqual({k for k, d in declared.items() if d["required"]}, set(p))
            for name, value in p.items():
                t = declared[name]["type"]
                if isinstance(t, dict) and "ref" in t:
                    self.assertEqual(value, {"object_type": t["ref"], "namespace": namespace, "native_key": value["native_key"]})
                    self.assertRegex(value["native_key"], r"^batch-mining-[0-9a-f]{16}$")
                elif isinstance(t, dict) and "value_type" in t:
                    self.assertIn(value, types[t["value_type"]]["enum"])
                else:
                    self.assertEqual(t, "string")
                    self.assertTrue(value)
                    self.assertLessEqual(len(value.encode("utf-8")), declared[name]["max_bytes"])
            if body["action_type"] == "open_finding":
                self.assertEqual((p["kind"], p["detector"], p["severity"]), ("lesson", "batch:mining", "info"))
            self.assertEqual(body["request_id"], "bm-" + sha(canon([body["action_type"], p["finding"]["native_key"], key, p]))[:40])
        self.assertEqual(seen, {"open_finding", "see_finding_again"})

    def test_engine_store_is_passed_on(self):
        self.week()
        store = self.tmp / "store"
        v, items, reqs, out = self.build(extra=["--store", store])
        calls = self.calls()
        self.assertTrue(calls and all(c["argv"][:2] == ["--store", str(store)] for c in calls))


if __name__ == "__main__":
    unittest.main()
