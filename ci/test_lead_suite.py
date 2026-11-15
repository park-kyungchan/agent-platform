"""Tests of evals/lead/run, the Lead suite, run as a program against a fake of `ap` and invented cases."""
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

RUN = Path(__file__).resolve().parent.parent / "evals" / "lead" / "run"

FAKE_AP = r'''#!/usr/bin/env python3
import json, os, re, sys
r = json.load(sys.stdin)
plan = json.load(open(os.environ["FAKE_PLAN"]))
prompt = open(r["prompt"], "rb").read().decode()
root = r["cwd"] if "SCORER-PROMPT" in prompt else os.path.dirname(r["cwd"].rstrip("/"))
below = {}
for d, _, fs in os.walk(root):
    for f in fs:
        p = os.path.join(d, f)
        below[os.path.relpath(p, root)] = open(p, "rb").read().decode("utf-8", "replace")[:400]
row = {"request": r, "prompt_text": prompt, "root": root, "below": below, "argv": sys.argv[1:]}
if "SCORER-PROMPT" in prompt:
    row["kind"] = "score"
    sc = plan.get("score", {})
    state, mode = sc.get("state", "ok"), sc.get("mode", "normal")
    lines = []
    cdir = os.path.join(r["cwd"], "cases")
    for cid in sorted(os.listdir(cdir)):
        for letter in "pq":
            f = os.path.join(cdir, cid, letter + ".md")
            if os.path.exists(f):
                tag = re.search(r"SET=(\S+)", open(f).read()).group(1)
                res = sc["results"][tag + ":" + cid]
                lines.append('%s | %s | %s | "quote of %s %s" | clause of %s' % (cid, letter, res, tag, cid, cid))
    if mode == "drop": lines = lines[:-1]
    if mode == "double": lines.append(lines[0])
    if mode == "unknown": lines[0] = lines[0].replace(lines[0].split(" ")[0], "zz-9", 1)
    if mode == "badword": lines[0] = lines[0].replace("caught", "probably").replace("partly", "probably").replace("missed", "probably")
    if mode == "prose": lines.append("Some closing remark.")
    text = ("scored" if mode == "noverdict" else "Verdict: scored") + "\n" + "\n".join(lines) + "\n"
    stream, answer, beh, acct, sess = [], text, {}, r["account"], "sess-score"
else:
    row["kind"] = "answer"
    cid = re.search(r"CASEID (\S+)", prompt).group(1)
    cnt = os.path.join(os.environ["FAKE_COUNT"], cid)
    n = int(open(cnt).read()) if os.path.exists(cnt) else 0
    open(cnt, "w").write(str(n + 1))
    beh = plan.get("answers", {}).get(cid, [{}])
    beh = beh[min(n, len(beh) - 1)]
    state, mode = beh.get("state", "ok"), beh.get("mode", "")
    acct, sess = beh.get("account", r["account"]), beh.get("session", "sess-default")
    stream = [s.replace("@ROOT@", root).replace("@ACCOUNT@", acct).replace("@SESSION@", sess) for s in beh.get("stream", [])]
    answer = "" if beh.get("empty") else "SET=%s ID=%s\nAn answer.\n" % (os.environ.get("FAKE_SET", "X"), cid)
with open(os.environ["FAKE_LOG"], "a") as f:
    f.write(json.dumps(row) + "\n")
if mode == "crash":
    sys.exit(1)
if mode == "refuse":
    print(json.dumps({"error": {"code": "invalid_input"}}), file=sys.stderr)
    sys.exit(2)
if state == "ok" or state == "no_result":
    open(r["out"], "w").write(answer)
with open(r["out"] + ".stream.jsonl", "w") as f:
    f.write(json.dumps({"type": "system", "subtype": "init", "cwd": "/somewhere/else"}) + "\n")
    f.write(json.dumps({"type": "assistant", "message": {"content": [{"type": "text", "text": "I read /etc/hosts"}]}}) + "\n")
    for s in stream:
        blocks = [{"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": s}}] if not s.startswith("@") else \
            [{"type": "tool_use", "id": "t1", "name": "Read", "input": json.loads(s[1:])}]
        f.write(json.dumps({"type": "assistant", "message": {"content": blocks}}) + "\n")
        f.write(json.dumps({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "content": "/etc/passwd"}]}}) + "\n")
rec = {"account": acct, "session": sess} if row["kind"] == "answer" else {"account": r["account"], "session": "sess-score"}
if beh.get("no_account") and row["kind"] == "answer": del rec["account"]
json.dump({"state": state, **rec, "role": r["role"], "model": r.get("model"), "effort": r.get("effort"), "cwd": r["cwd"],
           "turns": 3, "tokens": {"input": 10, "output": 20, "cache_read": 0, "cache_creation": 0}}, open(r["out"] + ".run.json", "w"))
sys.exit(0 if state == "ok" else 3)
'''


def tree_digest(root):
    rows = []
    for d, _, fs in os.walk(root):
        for f in fs:
            p = os.path.join(d, f)
            rows.append((os.path.relpath(p, root).encode(), hashlib.sha256(Path(p).read_bytes()).hexdigest()))
    return hashlib.sha256(b"".join(h.encode() + b"  " + r + b"\n" for r, h in sorted(rows))).hexdigest()


class World:
    def __init__(self, ids=("c-one", "c-two")):
        self.tmp = tempfile.TemporaryDirectory()
        t = self.t = Path(self.tmp.name)
        site = t / "site" / "evals" / "lead"
        site.mkdir(parents=True)
        shutil.copy2(RUN, site / "run")
        self.run_path = site / "run"
        (site / "prompt.md").write_text("INSTR @INSTRUCTIONS@\nLeft: @CASE@\n")
        (site / "scoring.md").write_text("scoring words\n")
        (site / "score-prompt.md").write_text("SCORER-PROMPT read scoring.md\n")
        self.cases = t / "cases"
        lines = []
        for cid in ids:
            c = self.cases / cid
            (c / "TASK" / "sub").mkdir(parents=True)
            (c / "PROMPT.txt").write_text("CASEID %s look at @TASK@/STATE.md\n" % cid)
            (c / "EXPECTED.md").write_text("RUBRIC-SECRET-%s\n" % cid)
            (c / "TASK" / "STATE.md").write_text("state of %s\n" % cid)
            (c / "TASK" / "sub" / "n.md").write_text("note\n")
            lines.append("%s  %s\n" % (tree_digest(c), cid))
        (site / "cases.sha256").write_text("".join(lines))
        self.instr = t / "instr"
        (self.instr / "skills" / "lead").mkdir(parents=True)
        (self.instr / "AGENTS.md").write_text("agents\n")
        (self.instr / "skills" / "lead" / "SKILL.md").write_text("lead\n")
        self.acct, self.next = t / "acct", t / "acct-next"
        self.acct.mkdir()
        self.next.mkdir()
        self.fake = t / "fake-ap"
        self.fake.write_text(FAKE_AP)
        self.fake.chmod(0o755)
        self.log = t / "log.jsonl"
        (t / "count").mkdir()
        self.n = 0

    def rows(self, kind=None):
        if not self.log.exists():
            return []
        rows = [json.loads(line) for line in self.log.read_text().splitlines()]
        return [r for r in rows if kind in (None, r["kind"])]

    def call(self, verb, *args, plan=None, tag="X", ap=True):
        self.n += 1
        pf = self.t / ("plan%d.json" % self.n)
        pf.write_text(json.dumps(plan or {}))
        env = {"PATH": os.environ["PATH"], "FAKE_PLAN": str(pf), "FAKE_LOG": str(self.log), "FAKE_COUNT": str(self.t / "count"),
               "FAKE_SET": tag}
        argv = [str(self.run_path), verb, "--cases", str(self.cases), "--account", str(self.acct)]
        if ap:
            argv += ["--ap", str(self.fake)]
        return subprocess.run(argv + [str(a) for a in args], env=env, stdin=subprocess.DEVNULL, capture_output=True, text=True)

    def answers(self, out, *args, plan=None, tag="X", model="m1", instr=None):
        return self.call("answers", "--instructions", str(instr or self.instr), "--out", str(out), "--model", model,
                         "--effort", "high", *args, plan=plan, tag=tag)

    def score(self, out, a, *args, b=None, plan=None, model="sm", seed=1):
        more = ["--b", str(b)] if b else []
        return self.call("score", "--a", str(a), *more, "--out", str(out), "--model", model, "--effort", "high",
                         "--seed", str(seed), *args, plan=plan)


def err_code(done):
    return json.loads(done.stderr.strip().splitlines()[-1])["code"]


class LeadSuite(unittest.TestCase):
    def setUp(self):
        self.w = World()

    def tearDown(self):
        self.w.tmp.cleanup()

    def out(self, name="set"):
        return self.w.t / name

    def test_l1_case_digest_is_checked_before_any_effect(self):
        w = self.w
        with open(w.cases / "c-two" / "TASK" / "STATE.md", "a") as f:
            f.write("x")
        done = w.answers(self.out())
        self.assertEqual(done.returncode, 2, done.stderr)
        self.assertEqual(err_code(done), "case_digest_differs")
        self.assertIn("c-two", done.stderr)
        self.assertFalse(self.out().exists())
        self.assertEqual(w.rows(), [])
        (w.cases / "c-two" / "TASK" / "STATE.md").write_text("state of c-two\n")
        (w.cases / "c-three").mkdir()
        done = w.answers(self.out())
        self.assertEqual((done.returncode, err_code(done)), (2, "case_unknown"))
        self.assertIn("c-three", done.stderr)
        shutil.rmtree(w.cases / "c-three")
        shutil.rmtree(w.cases / "c-one")
        done = w.answers(self.out())
        self.assertEqual((done.returncode, err_code(done)), (2, "case_digest_differs"))
        self.assertIn("c-one", done.stderr)
        self.assertFalse(self.out().exists())
        self.assertEqual(w.rows(), [])

    def test_l1_stage_request_and_prompt_are_the_fixed_ones(self):
        w = self.w
        done = w.answers(self.out(), "--case", "c-one", "--next-account", w.next)
        self.assertEqual(done.returncode, 0, done.stderr)
        (row,) = w.rows("answer")
        r = row["request"]
        self.assertEqual(row["argv"], ["run"])
        self.assertEqual(sorted(r), sorted(["role", "account", "next_accounts", "model", "effort", "permission_mode", "cwd",
                                            "prompt", "out", "timeout_seconds"]))
        self.assertEqual((r["role"], r["permission_mode"], r["timeout_seconds"]), ("reviewer", "auto", 900))
        self.assertEqual((r["account"], r["next_accounts"], r["model"], r["effort"]), (str(w.acct), [str(w.next)], "m1", "high"))
        self.assertEqual(r["out"], str(self.out() / "c-one.1.md"))
        self.assertEqual(r["prompt"], str(self.out() / "c-one.1.prompt.md"))
        self.assertEqual(r["cwd"], row["root"] + "/task")
        self.assertEqual(row["below"]["task/STATE.md"], "state of c-one\n")
        self.assertEqual(row["below"]["instructions/skills/lead/SKILL.md"], "lead\n")
        want = "INSTR %s/instructions\nLeft: CASEID c-one look at %s/task/STATE.md\n\n" % (row["root"], row["root"])
        self.assertEqual(row["prompt_text"], want)
        self.assertEqual((self.out() / "c-one.1.prompt.md").read_text(), row["prompt_text"])
        self.assertNotIn("@", row["prompt_text"])
        self.assertNotIn("c-one", row["root"])
        done = w.answers(self.out("two"), "--case", "c-one")
        self.assertNotIn("next_accounts", w.rows("answer")[-1]["request"])

    def test_l1_no_rubric_is_below_the_scratch_root(self):
        w = self.w
        self.assertEqual(w.answers(self.out()).returncode, 0)
        rows = w.rows("answer")
        self.assertEqual(len(rows), 2)
        for row in rows:
            self.assertEqual(sorted({p.split("/")[0] for p in row["below"]}), ["instructions", "task"])
            for name, text in row["below"].items():
                self.assertNotIn("EXPECTED", name)
                self.assertNotIn("RUBRIC-SECRET", text)
            self.assertNotIn("RUBRIC-SECRET", row["prompt_text"])
        for p in self.out().iterdir():
            self.assertNotIn("RUBRIC-SECRET", p.read_text(errors="replace"), p.name)

    def test_l1_outside_path_makes_the_answer_invalid(self):
        w = self.w
        good = ["cat @ROOT@/task/STATE.md", "ls @ROOT@/instructions/skills", "cat x 2>/dev/null", "cat ./sub/n.md",
                "@" + json.dumps({"file_path": "@ROOT@/task/STATE.md"}), "grep -r 'a/b' ."]
        bad = {"c-one": ["cat /etc/passwd"], "c-two": ["cat @ROOT@/task/../../x/y"]}
        plan = {"answers": {"c-one": [{"stream": good + bad["c-one"]}], "c-two": [{"stream": good + bad["c-two"]}]}}
        done = w.answers(self.out(), plan=plan)
        self.assertEqual(done.returncode, 1, done.stderr)
        a = json.loads((self.out() / "answers.json").read_text())
        for cid in ("c-one", "c-two"):
            self.assertIsNone(a[cid]["valid_try"])
            self.assertEqual([(t["valid"], t["reason"]) for t in a[cid]["tries"]], [(False, "outside_path")] * 2)
        # a path in a tool input of another form, and a tool path outside, are found; text and results are not read
        plan = {"answers": {"c-one": [{"stream": ["@" + json.dumps({"file_path": "/opt/x/file.md"})]}, {}]}}
        w2 = World(("c-one",))
        done = w2.answers(w2.t / "s", plan=plan)
        self.assertEqual(done.returncode, 0, done.stderr)
        a = json.loads((w2.t / "s" / "answers.json").read_text())
        self.assertEqual([t["reason"] for t in a["c-one"]["tries"]], ["outside_path", None])
        self.assertEqual(a["c-one"]["valid_try"], 2)
        w2.tmp.cleanup()
        # the clean runs of the list `good` alone are valid
        w3 = World(("c-one",))
        done = w3.answers(w3.t / "s", plan={"answers": {"c-one": [{"stream": good}]}})
        self.assertEqual(done.returncode, 0, done.stderr)
        w3.tmp.cleanup()

    def test_l1_installed_reads_are_counted(self):
        w = self.w
        stream = ["cat /root/.agents/AGENTS.md", "ls /root/.agents/skills /dev/null", "cat @ROOT@/instructions/AGENTS.md"]
        done = w.answers(self.out(), "--case", "c-one", plan={"answers": {"c-one": [{"stream": stream}]}})
        self.assertEqual(done.returncode, 0, done.stderr)
        t = json.loads((self.out() / "answers.json").read_text())["c-one"]["tries"][0]
        self.assertEqual((t["valid"], t["installed_reads"], t["state"], t["turns"]), (True, 2, "ok", 3))
        self.assertEqual(t["tokens"]["output"], 20)

    def test_l1_second_call_tries_only_what_has_no_valid_answer(self):
        w = self.w
        out = self.out()
        self.assertEqual(w.answers(out, "--case", "c-one").returncode, 0)
        before = {p.name: p.read_bytes() for p in out.iterdir() if p.name != "answers.json"}
        # an interrupted first try of c-two: its files stay as they are
        (out / "c-two.1.prompt.md").write_text("old prompt\n")
        (out / "c-two.1.md.run.json").write_text(json.dumps({"state": "failed", "cwd": "/gone/task", "turns": 1}))
        done = w.answers(out)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(len(w.rows("answer")), 2)
        self.assertEqual([r["request"]["out"] for r in w.rows("answer")], [str(out / "c-one.1.md"), str(out / "c-two.2.md")])
        for name, data in before.items():
            self.assertEqual((out / name).read_bytes(), data)
        self.assertEqual((out / "c-two.1.prompt.md").read_text(), "old prompt\n")
        a = json.loads((out / "answers.json").read_text())
        self.assertEqual(a["c-two"]["valid_try"], 2)
        self.assertEqual([(t["try"], t["valid"], t["reason"]) for t in a["c-two"]["tries"]], [(1, False, "stage_failed"), (2, True, None)])
        self.assertEqual(w.answers(out).returncode, 0)
        self.assertEqual(len(w.rows("answer")), 2)

    def test_l1_third_try_is_never_made(self):
        w = self.w
        out = self.out()
        plan = {"answers": {"c-two": [{"state": "failed"}]}}
        done = w.answers(out, plan=plan)
        self.assertEqual(done.returncode, 1, done.stderr)
        self.assertEqual(len(w.rows("answer")), 3)
        done = w.answers(out, plan=plan)
        self.assertEqual(done.returncode, 1)
        self.assertEqual(len(w.rows("answer")), 3)
        self.assertFalse(any(p.name.startswith("c-two.3") for p in out.iterdir()))
        a = json.loads((out / "answers.json").read_text())
        self.assertEqual([t["reason"] for t in a["c-two"]["tries"]], ["stage_failed", "stage_failed"])
        self.assertIsNone(a["c-two"]["valid_try"])
        # an empty answer and a stage without a record are invalid with their own reasons
        w2 = World(("c-one",))
        w2.answers(w2.t / "s", plan={"answers": {"c-one": [{"empty": True}, {"mode": "refuse"}]}})
        a = json.loads((w2.t / "s" / "answers.json").read_text())
        self.assertEqual([t["reason"] for t in a["c-one"]["tries"]], ["empty", "stage_refused"])
        w2.tmp.cleanup()

    def test_l1_meta_differs_is_refused(self):
        w = self.w
        out = self.out()
        self.assertEqual(w.answers(out, "--case", "c-one").returncode, 0)
        meta = json.loads((out / "META.json").read_text())
        self.assertIn("m1", json.dumps(meta))
        files = sorted(p.name for p in out.iterdir())
        n = len(w.rows())
        done = w.answers(out, model="m2")
        self.assertEqual((done.returncode, err_code(done)), (2, "meta_differs"))
        (w.instr / "AGENTS.md").write_text("agents changed\n")
        done = w.answers(out)
        self.assertEqual((done.returncode, err_code(done)), (2, "meta_differs"))
        done = w.call("answers", "--instructions", str(w.instr), "--out", str(out), "--model", "m1", "--effort", "low")
        self.assertEqual((done.returncode, err_code(done)), (2, "meta_differs"))
        self.assertEqual(sorted(p.name for p in out.iterdir()), files)
        self.assertEqual(len(w.rows()), n)
        self.assertEqual(json.loads((out / "META.json").read_text()), meta)

    def test_l1_scratch_is_removed_in_every_outcome(self):
        w = self.w
        for i, beh in enumerate([{}, {"state": "failed"}, {"mode": "refuse"}, {"mode": "crash"}, {"state": "cut"}, {"empty": True}]):
            w.answers(w.t / ("s%d" % i), "--case", "c-one", plan={"answers": {"c-one": [beh]}})
        rows = w.rows("answer")
        self.assertEqual(len(rows), 6 + 5)  # every outcome but the first is invalid and tried twice
        for row in rows:
            self.assertFalse(os.path.exists(row["root"]), row["root"])
        self.assertEqual(len({row["root"] for row in rows}), len(rows))
        self.assertEqual(w.score(w.t / "sc", w.t / "s0", "--case", "c-one", plan={"score": {"results": {"X:c-one": "caught"}}}).returncode, 0)
        self.assertFalse(os.path.exists(w.rows("score")[-1]["root"]))
        w.score(w.t / "sc2", w.t / "s0", "--case", "c-one", plan={"score": {"mode": "crash", "results": {"X:c-one": "caught"}}})
        self.assertFalse(os.path.exists(w.rows("score")[-1]["root"]))

    def make_sets(self, w, ids):
        plan_a = {"answers": {}}
        self.assertEqual(w.answers(w.t / "A", plan=plan_a, tag="A").returncode, 0)
        self.assertEqual(w.answers(w.t / "B", plan=plan_a, tag="B").returncode, 0)

    def test_l1_score_draws_per_case_and_unblinds(self):
        ids = ["c-%d" % i for i in range(8)]
        self.w.tmp.cleanup()
        w = self.w = World(ids)
        self.make_sets(w, ids)
        results = {}
        for i, cid in enumerate(ids):
            results["A:" + cid] = ["caught", "partly", "missed"][i % 3]
            results["B:" + cid] = ["caught", "missed", "partly", "caught"][i % 4]
        plan = {"score": {"results": results}}
        done = w.score(w.t / "sc", w.t / "A", b=w.t / "B", plan=plan, seed=7)
        self.assertEqual(done.returncode, 0, done.stderr)
        (row,) = w.rows("score")
        mapping = json.loads((w.t / "sc" / "mapping.json").read_text())
        self.assertEqual(sorted(mapping), ids)
        self.assertGreater(len({m["p"] for m in mapping.values()}), 1)
        for cid in ids:
            self.assertEqual(sorted(mapping[cid].values()), ["a", "b"])
            self.assertEqual(sorted(mapping[cid]), ["p", "q"])
        # what the scorer saw below its root: scoring.md, rubric, p, q; no mapping, no set name
        self.assertEqual(sorted(p for p in row["below"] if p.count("/") == 0), ["scoring.md"])
        self.assertEqual(sorted(p for p in row["below"] if "c-3/" in p), ["cases/c-3/p.md", "cases/c-3/q.md", "cases/c-3/rubric.md"])
        self.assertEqual(row["below"]["cases/c-3/rubric.md"], "RUBRIC-SECRET-c-3\n")
        self.assertNotIn("mapping", json.dumps(row["below"]))
        self.assertEqual(row["request"]["cwd"], row["root"])
        r = row["request"]
        self.assertEqual((r["role"], r["permission_mode"], r["timeout_seconds"], r["out"]), ("reviewer", "auto", 1800, str(w.t / "sc" / "scorer.md")))
        self.assertEqual(r["expect"], {"first_line": "Verdict: scored"})
        self.assertEqual(row["prompt_text"], "SCORER-PROMPT read scoring.md\n")
        for set_name in ("p.md", "q.md"):
            for cid in ids:
                tag = row["below"]["cases/%s/%s" % (cid, set_name)].split()[0]
                want = "A" if mapping[cid][set_name[0]] == "a" else "B"
                self.assertEqual(tag, "SET=" + want)
        sheet = json.loads((w.t / "sc" / "sheet.json").read_text())
        self.assertIs(sheet["complete"], True)
        for cid in ids:
            self.assertEqual(sheet["cases"][cid]["a"]["result"], results["A:" + cid])
            self.assertEqual(sheet["cases"][cid]["b"]["result"], results["B:" + cid])
            self.assertEqual(sheet["cases"][cid]["a"]["quote"], "quote of A " + cid)
        lost = [c for c in ids if results["A:" + c] == "caught" and results["B:" + c] != "caught"]
        gained = [c for c in ids if results["B:" + c] == "caught" and results["A:" + c] != "caught"]
        self.assertEqual((sheet["lost"], sheet["gained"]), (lost, gained))
        self.assertTrue(lost and gained)
        for s in ("a", "b"):
            want = {k: sum(results[s.upper() + ":" + c] == k for c in ids) for k in ("caught", "partly", "missed")}
            self.assertEqual(sheet["sets"][s]["counts"], want)
            self.assertEqual(sheet["sets"][s]["installed_reads"], 0)
            self.assertEqual(sheet["sets"][s]["meta"]["model"], "m1")
        self.assertEqual((sheet["scorer"]["model"], sheet["scorer"]["effort"]), ("sm", "high"))
        # the draw is a function of the seed; another seed draws another mapping
        done = w.score(w.t / "sc2", w.t / "A", b=w.t / "B", plan=plan, seed=7)
        self.assertEqual(json.loads((w.t / "sc2" / "mapping.json").read_text()), mapping)
        w.score(w.t / "sc3", w.t / "A", b=w.t / "B", plan=plan, seed=8)
        self.assertNotEqual(json.loads((w.t / "sc3" / "mapping.json").read_text()), mapping)
        # one set only: every answer is p
        done = w.score(w.t / "sc4", w.t / "A", "--case", "c-1", plan=plan)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(sorted(w.rows("score")[-1]["below"]), ["cases/c-1/p.md", "cases/c-1/rubric.md", "scoring.md"])
        self.assertEqual(json.loads((w.t / "sc4" / "sheet.json").read_text())["cases"].keys(), {"c-1"})

    def test_l1_score_refuses_incomplete_or_incomparable_sets(self):
        w = self.w
        self.make_sets(w, ["c-one", "c-two"])
        plan = {"score": {"results": {"A:c-one": "caught", "A:c-two": "caught", "B:c-one": "caught", "B:c-two": "caught"}}}
        # a set that lacks a valid answer
        a = json.loads((w.t / "B" / "answers.json").read_text())
        a["c-two"]["valid_try"] = None
        (w.t / "B" / "answers.json").write_text(json.dumps(a))
        done = w.score(w.t / "sc", w.t / "A", b=w.t / "B", plan=plan)
        self.assertEqual((done.returncode, err_code(done)), (2, "set_incomplete"))
        self.assertIn("c-two", done.stderr)
        self.assertFalse((w.t / "sc").exists())
        self.assertEqual(w.score(w.t / "sc", w.t / "A", b=w.t / "B", plan=plan, *["--case", "c-one"]).returncode, 0)
        shutil.rmtree(w.t / "sc")
        a["c-two"]["valid_try"] = 1
        (w.t / "B" / "answers.json").write_text(json.dumps(a))
        self.assertEqual(len(w.rows("score")), 1)
        # sets that cannot be compared
        for key, value in (("model", "other"), ("effort", "low"), ("prompt_sha256", "0" * 64), ("cases_sha256", "0" * 64)):
            meta_file = w.t / "B" / "META.json"
            saved = meta_file.read_text()
            meta = json.loads(saved)
            self.assertIn(key, meta)
            meta[key] = value
            meta_file.write_text(json.dumps(meta))
            done = w.score(w.t / "sc", w.t / "A", b=w.t / "B", plan=plan)
            self.assertEqual((done.returncode, err_code(done)), (2, "sets_not_comparable"), key)
            self.assertFalse((w.t / "sc").exists())
            meta_file.write_text(saved)
        # an output that exists
        (w.t / "sc").mkdir()
        done = w.score(w.t / "sc", w.t / "A", b=w.t / "B", plan=plan)
        self.assertEqual((done.returncode, err_code(done)), (2, "out_exists"))
        self.assertEqual(list((w.t / "sc").iterdir()), [])
        # a changed case is refused before the sets are read
        with open(w.cases / "c-one" / "EXPECTED.md", "a") as f:
            f.write("x")
        done = w.score(w.t / "sc9", w.t / "A", b=w.t / "B", plan=plan)
        self.assertEqual((done.returncode, err_code(done)), (2, "case_digest_differs"))
        self.assertEqual(len(w.rows("score")), 1)

    def test_l1_scorer_lines_are_read_strictly(self):
        w = self.w
        self.make_sets(w, ["c-one", "c-two"])
        res = {"A:c-one": "caught", "A:c-two": "partly", "B:c-one": "missed", "B:c-two": "caught"}
        for i, mode in enumerate(["drop", "double", "unknown", "badword", "prose", "noverdict"]):
            out = w.t / ("bad%d" % i)
            done = w.score(out, w.t / "A", b=w.t / "B", plan={"score": {"mode": mode, "results": res}})
            self.assertEqual(done.returncode, 1, (mode, done.stderr))
            sheet = json.loads((out / "sheet.json").read_text())
            self.assertIs(sheet["complete"], False, mode)
            self.assertTrue(sheet["reason"], mode)
            self.assertFalse((out / "sheet.md").exists(), mode)
        done = w.score(w.t / "bad-state", w.t / "A", b=w.t / "B", plan={"score": {"state": "failed", "results": res}})
        self.assertEqual(done.returncode, 1)
        self.assertIs(json.loads((w.t / "bad-state" / "sheet.json").read_text())["complete"], False)
        self.assertFalse((w.t / "bad-state" / "sheet.md").exists())
        done = w.score(w.t / "good", w.t / "A", b=w.t / "B", plan={"score": {"results": res}})
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertTrue((w.t / "good" / "sheet.md").exists())

    def test_l1_public_sheet_holds_no_quote_and_no_path(self):
        w = self.w
        self.make_sets(w, ["c-one", "c-two"])
        res = {"A:c-one": "caught", "A:c-two": "partly", "B:c-one": "missed", "B:c-two": "caught"}
        done = w.score(w.t / "sc", w.t / "A", b=w.t / "B", plan={"score": {"results": res}})
        self.assertEqual(done.returncode, 0, done.stderr)
        text = (w.t / "sc" / "sheet.md").read_text()
        for word in ("c-one", "c-two", "caught", "partly", "missed", "lost", "gained", "m1", "sm", "high"):
            self.assertIn(word, text)
        meta = json.loads((w.t / "A" / "META.json").read_text())
        for value in meta.values():
            self.assertIn(value, text)
        for secret in ("quote of", "clause of", "RUBRIC", "SET=", str(w.t), '"'):
            self.assertNotIn(secret, text)
        self.assertNotIn("/", text)
        self.assertIn("quote of", (w.t / "sc" / "sheet.json").read_text())
        # one set: one column, no lost and no gained claim
        done = w.score(w.t / "sc1", w.t / "A", "--case", "c-one", plan={"score": {"results": res}})
        self.assertEqual(done.returncode, 0, done.stderr)
        one = (w.t / "sc1" / "sheet.md").read_text()
        self.assertIn("c-one", one)
        self.assertNotIn("c-two", one)
        self.assertNotIn("quote of", one)

    def test_l1_a_link_below_a_case_or_the_instructions_is_refused(self):
        w = self.w
        task = w.cases / "c-one" / "TASK"
        for make in (lambda p: os.symlink("../EXPECTED.md", p), lambda p: os.symlink("sub", p), lambda p: os.mkfifo(p)):
            link = task / "sub" / "l"
            make(link)
            for verb in ("answers", "score"):
                out = self.out(verb)
                if verb == "answers":
                    done = w.answers(out)
                else:
                    done = w.score(out, w.t / "nowhere")
                self.assertEqual((done.returncode, err_code(done)), (2, "not_regular"), (verb, done.stderr))
                self.assertIn("TASK/sub/l", done.stderr)
                self.assertNotIn(str(w.cases), done.stderr)
                self.assertFalse(out.exists())
            os.unlink(link)
        # a link at the top of a case, and a link to a directory, are refused as well
        os.symlink("TASK", w.cases / "c-two" / "alias")
        done = w.answers(self.out())
        self.assertEqual((done.returncode, err_code(done)), (2, "not_regular"))
        self.assertIn("alias", done.stderr)
        os.unlink(w.cases / "c-two" / "alias")
        self.assertEqual(w.answers(self.out("ok"), "--case", "c-one").returncode, 0)
        # a link below the instructions, whatever it points to
        for target in ("AGENTS.md", "/nonexistent"):
            os.symlink(target, w.instr / "skills" / "lead" / "extra.md")
            done = w.answers(self.out("ok"))
            self.assertEqual((done.returncode, err_code(done)), (2, "not_regular"))
            self.assertIn("skills/lead/extra.md", done.stderr)
            done = w.answers(self.out("fresh"))
            self.assertEqual((done.returncode, err_code(done)), (2, "not_regular"))
            self.assertFalse(self.out("fresh").exists())
            os.unlink(w.instr / "skills" / "lead" / "extra.md")
        self.assertEqual(len(w.rows()), 1)


    def test_l1b_own_saved_tool_result_is_no_outside_path(self):
        w = World(("c-one",))
        own = "@ACCOUNT@/projects/-scratch-task/@SESSION@/tool-results/"
        cases = {
            "own": ([own + "abc.txt", "cat " + own + "sub/dir/abc.json", "@" + json.dumps({"file_path": own + "x.txt"})], None, True),
            "own_unnormal": (["cat " + own + "../../../../etc/hosts"], None, False),
            "other_session": (["cat @ACCOUNT@/projects/-scratch-task/other-session/tool-results/abc.txt"], None, False),
            "other_account": (["cat /elsewhere/acct/projects/-scratch-task/@SESSION@/tool-results/abc.txt"], None, False),
            "projects_dir": (["ls @ACCOUNT@/projects"], None, False),
            "other_subdir": (["cat @ACCOUNT@/projects/-scratch-task/@SESSION@/memory/abc.txt"], None, False),
            "session_dir": (["ls @ACCOUNT@/projects/-scratch-task/@SESSION@/tool-results"], None, False),
            "two_directories": (["cat @ACCOUNT@/projects/a/b/@SESSION@/tool-results/abc.txt"], None, False),
            "no_session_named": ([own + "abc.txt"], {"session": ""}, False),
        }
        for name, (stream, extra, valid) in cases.items():
            beh = {"stream": stream + ["cat @ROOT@/task/STATE.md"], "account": str(w.acct), "session": "s-own-1", **(extra or {})}
            out = w.t / name
            w.answers(out, "--case", "c-one", plan={"answers": {"c-one": [beh, beh]}})
            t = json.loads((out / "answers.json").read_text())["c-one"]["tries"][0]
            self.assertEqual((t["valid"], t["reason"]), (True, None) if valid else (False, "outside_path"), name)
            self.assertEqual(t["own_results"] > 0, valid, name)
        # the record names the account that answered: an entry of next_accounts
        out = w.t / "next"
        beh = {"stream": [own + "abc.txt"], "account": str(w.next), "session": "s-own-2"}
        w.answers(out, "--case", "c-one", "--next-account", w.next, plan={"answers": {"c-one": [beh]}})
        t = json.loads((out / "answers.json").read_text())["c-one"]["tries"][0]
        self.assertEqual((t["valid"], t["own_results"]), (True, 1))
        # a record without an account: the same path is outside
        out = w.t / "noacct"
        beh = {"stream": [own + "abc.txt"], "session": "s-own-3", "no_account": True}
        w.answers(out, "--case", "c-one", plan={"answers": {"c-one": [beh, beh]}})
        t = json.loads((out / "answers.json").read_text())["c-one"]["tries"][0]
        self.assertEqual((t["valid"], t["reason"], t["own_results"]), (False, "outside_path", 0))
        w.tmp.cleanup()

    def test_l1b_a_set_judged_before_is_judged_again_without_a_stage(self):
        w = World(("c-one",))
        own = "@ACCOUNT@/projects/-scratch-task/@SESSION@/tool-results/abc.txt"
        beh = {"stream": ["cat " + own], "account": str(w.acct), "session": "s-own-1"}
        out = w.t / "set"
        self.assertEqual(w.answers(out, plan={"answers": {"c-one": [beh]}}).returncode, 0)
        before = len(w.rows())
        # the file as an older judgment left it: both tries invalid, no valid try
        old = {"c-one": {"tries": [{"try": n, "state": "ok", "valid": False, "reason": "outside_path", "turns": 3, "tokens": None,
                                    "installed_reads": 0} for n in (1, 2)], "valid_try": None}}
        (out / "c-one.2.prompt.md").write_text("old\n")
        (out / "c-one.2.md.run.json").write_text((out / "c-one.1.md.run.json").read_text())
        shutil.copy(out / "c-one.1.md.stream.jsonl", out / "c-one.2.md.stream.jsonl")
        shutil.copy(out / "c-one.1.md", out / "c-one.2.md")
        (out / "answers.json").write_text(json.dumps(old))
        done = w.answers(out, "--case", "c-one")
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertEqual(len(w.rows()), before)
        c = json.loads((out / "answers.json").read_text())["c-one"]
        self.assertEqual(c["valid_try"], 1)
        self.assertEqual([t["own_results"] for t in c["tries"]], [1, 1])
        w.tmp.cleanup()

    def test_l1b_score_refuses_a_stale_set(self):
        w = self.w
        self.make_sets(w, ["c-one", "c-two"])
        plan = {"score": {"results": {"A:c-one": "caught", "A:c-two": "caught", "B:c-one": "caught", "B:c-two": "caught"}}}
        rows = len(w.rows())

        def refused(first, second, want):
            done = w.score(w.t / "sc", w.t / first, b=(w.t / second) if second else None, plan=plan)
            self.assertEqual((done.returncode, err_code(done)), (2, "set_stale"), (first, second))
            self.assertEqual(json.loads(done.stderr.strip().splitlines()[-1])["detail"], want, (first, second))
            self.assertFalse((w.t / "sc").exists())

        for s, other in (("A", "B"), ("B", "A")):
            f = w.t / s / "META.json"
            saved = f.read_text()
            for what in ("absent", "unreadable", "prompt_sha256", "cases_sha256"):
                if what == "absent":
                    f.unlink()
                elif what == "unreadable":
                    f.write_text("{not json")
                else:
                    f.write_text(json.dumps({**json.loads(saved), what: "0" * 64}))
                refused(s, None, "a")  # the set given as --a, alone
                if what in ("absent", "unreadable"):
                    refused(s, other, "a")  # beside a good one, as a
                    refused(other, s, "b")  # and as b
                f.write_text(saved)
        # both stale in the same way: both named
        saved = {s: (w.t / s / "META.json").read_text() for s in "AB"}
        for s in "AB":
            (w.t / s / "META.json").write_text(json.dumps({**json.loads(saved[s]), "prompt_sha256": "0" * 64}))
        refused("A", "B", "a b")
        for s in "AB":
            (w.t / s / "META.json").write_text(saved[s])
        self.assertEqual(len(w.rows()), rows)
        self.assertEqual(w.score(w.t / "sc", w.t / "A", b=w.t / "B", plan=plan).returncode, 0)

    def test_l1b_a_prefix_of_an_allowed_root_is_outside(self):
        w = World(("c-one",))
        stream = ["cat @ROOT@/task/STATE.md", "cat /dev/null"]
        for bad in ("cat @ROOT@-sibling/task/STATE.md", "cat @ROOT@x", "cat /devx/null",
                    "cat /root/.agents-x/AGENTS.md", "cat /root/.agentsx"):
            out = w.t / ("s%d" % w.n)
            w.answers(out, "--case", "c-one", plan={"answers": {"c-one": [{"stream": stream + [bad]}]}})
            t = json.loads((out / "answers.json").read_text())["c-one"]["tries"][0]
            self.assertEqual((t["valid"], t["reason"], t["installed_reads"]), (False, "outside_path", 0), bad)
        w.tmp.cleanup()


if __name__ == "__main__":
    unittest.main()
