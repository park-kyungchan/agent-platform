"""Tests of evals/readout.py on invented records (ci/fixtures/readout/, synthetic values only).

Each invariant of the read-out has a test that fails on the wrong implementation:
I1 a message id is counted once, usage.iterations is never added; I2 a stage record's tokens are
never added to the session's; I3 every input line lands in one output line; I4 only the allowed
members are read and printed, nothing is written; I5 the same records give the same output.
Run: python3 -I -B -m unittest discover -s ci -p 'test_readout*.py'
"""
import contextlib
import datetime
import hashlib
import importlib.util
import io
import json
import os
import re
import shutil
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIXTURES = HERE / "fixtures" / "readout"
spec = importlib.util.spec_from_file_location("readout", HERE.parent / "evals" / "readout.py")
readout = importlib.util.module_from_spec(spec)
spec.loader.exec_module(readout)

PRS = {"park-kyungchan/agent-platform": [{"number": 7, "headRefName": "feat/a", "mergedAt": "2026-11-12T10:00:00Z", "additions": 10, "deletions": 5},
                                         {"number": 8, "headRefName": "feat/none", "mergedAt": "2026-11-13T10:00:00Z", "additions": 3, "deletions": 2},
                                         {"number": 10, "headRefName": "feat/det", "mergedAt": "2026-11-12T17:00:00Z", "additions": 20, "deletions": 0},
                                         {"number": 11, "headRefName": "feat/early", "mergedAt": "2026-11-12T18:00:00Z", "additions": 1, "deletions": 1},
                                         {"number": 9, "headRefName": "feat/old", "mergedAt": "2026-10-01T10:00:00Z"}],
       "park-kyungchan/ontology-platform": [{"number": 3, "headRefName": "feat/a", "mergedAt": "2026-11-12T19:00:00Z", "additions": 4, "deletions": 0}]}


def tree_digest(root):
    return sorted((str(p.relative_to(root)), hashlib.sha256(p.read_bytes()).hexdigest()) for p in Path(root).rglob("*") if p.is_file())


class Readout(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="readout-test-"))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        for src in FIXTURES.rglob("*"):
            if src.is_file():
                dst = self.tmp / src.relative_to(FIXTURES)
                dst.parent.mkdir(parents=True, exist_ok=True)
                dst.write_text(src.read_text().replace("@WORK@", str(self.tmp / "work")).replace("@PLATFORMS@", str(self.tmp / "platforms")))
        old = datetime.datetime(2026, 11, 10, 11, tzinfo=datetime.timezone.utc).timestamp()  # the day before the window
        os.utime(self.tmp / "acct1" / "projects" / "p9" / "s-early.jsonl", (old, old))
        readout.run_gh = lambda repo, since, until: PRS.get(repo, [])

    def run_readout(self, *extra):
        out = io.StringIO()
        args = ["--since", "2026-11-11", "--until", "2026-11-13", "--prices", str(self.tmp / "prices.tsv"),
                "--accounts", str(self.tmp / "acct1"), str(self.tmp / "acct2"), str(self.tmp / "acct1"),
                "--codex-dir", str(self.tmp / "codex"), "--work-root", str(self.tmp / "work"),
                "--platforms-root", str(self.tmp / "platforms"), "--lead-cwd", str(self.tmp / "work" / "lead"),
                "--repo", "park-kyungchan/agent-platform", "--repo", "park-kyungchan/ontology-platform", *extra]
        with contextlib.redirect_stdout(out):
            readout.main(args)
        return out.getvalue()

    def route(self, text, route):
        m = re.search(rf"^week \S+ route {route} TOTAL msgs=(\d+) input=([\d,]+) output=([\d,]+) cache_read=([\d,]+) "
                      rf"cache_write_5m=([\d,]+) cache_write_1h=([\d,]+) usd=([\d.]+)$", text, re.M)
        return [float(x.replace(",", "")) for x in m.groups()]

    def test_i1_message_counted_once_and_iterations_not_added(self):
        # m1 spans three lines, repeats in a forked session's file and carries usage.iterations
        # (input 1,000,000 output 500,000 cache read 2,000,000 write 100,000 + 200,000 = 4.1 USD at the fixture rates).
        text = self.run_readout()
        msgs, inp, out, cr, w5, w1, usd = self.route(text, "engineering")
        self.assertEqual((w5, w1), (100000, 200000))
        self.assertEqual(inp, 23400100)
        self.assertEqual(out, 2300000)  # m1 500,000 once; m5 1,000,000; m6 500,000; codex 300,000
        self.assertEqual(cr, 3000000)  # m1 2,000,000 once; codex 1,000,000
        self.assertIn("messages=16 ", text)
        self.assertIn("repeat=3 ", text)

    def test_i2_stage_tokens_are_not_added_and_roles_follow_the_stage_in_time(self):
        text = self.run_readout()
        self.assertEqual(self.route(text, "engineering")[6], 44.10)  # sessions only: every stage record says 7,000,000 of each kind
        self.assertRegex(text, r"role=implementer family=opus msgs=1 .* usd=4.10")
        self.assertRegex(text, r"role=fixer family=opus msgs=1 .* usd=1.00")  # a later record of the same session takes the later message
        self.assertRegex(text, r"role=reviewer family=opus msgs=1 .* usd=2.00")
        self.assertNotIn("7,000,000", text)

    def test_i2_calibration_skips_a_session_a_later_record_continues(self):
        text = self.run_readout("--calibrate")
        self.assertRegex(text, r"calibrate family=haiku stages=1 median\(readout/cli\)=1\.000")
        self.assertNotIn("calibrate family=opus", text)  # the implementer's session has two records and a cumulative figure

    def test_i3_every_line_lands_in_one_output_line(self):
        text = self.run_readout()
        self.assertRegex(text, r"input claude files=12 files_older_than_window=1 messages=16 other=4 read=24 repeat=3 unreadable=1 outside_window=1")
        n = {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", re.search(r"^input claude .*$", text, re.M).group(0))}
        self.assertEqual(n["read"], n["messages"] + n["other"] + n["repeat"] + n["unreadable"])
        self.assertRegex(text, r"role=\* family=unknown msgs=1 input=100 ")  # an unknown model keeps its tokens
        self.assertRegex(text, r"input stages records=10 unreadable=1 without_session_record=7")
        self.assertRegex(text, r"input codex events=2 files=1 other=2 .*read=5 repeat=1")
        for route in ("engineering", "lesson", "other"):
            block = re.search(rf"^week \S+ route {route} TOTAL.*?(?=^week|^route lesson|^K2|\Z)", text, re.M | re.S).group(0).splitlines()
            total, rows = block[0], block[1:]
            for kind in ("msgs", "input", "output", "cache_read", "cache_write_5m", "cache_write_1h", "usd"):
                pick = lambda s: float(re.search(rf"{kind}=([\d,.]+)", s).group(1).replace(",", ""))
                self.assertAlmostEqual(sum(pick(r) for r in rows), pick(total), places=2, msg=f"{route} {kind}")
        self.assertEqual([self.route(text, r)[0] for r in ("engineering", "lesson", "other")], [15, 1, 1])
        self.assertEqual([self.route(text, r)[6] for r in ("lesson", "other")], [3.0, 4.0])

    def test_i4_prints_no_text_no_tool_input_no_path_and_writes_nothing(self):
        before = tree_digest(self.tmp)
        text = self.run_readout("--calibrate", "--detail")
        self.assertEqual(before, tree_digest(self.tmp))
        for secret in ("SECRET", "learner-x", str(self.tmp), "edu-lessons", "total_cost_usd"):
            self.assertNotIn(secret, text)
        self.assertIn("lead calls first_tool=Read calls=1 usd=2.00", text)

    def test_i5_second_run_is_identical(self):
        self.assertEqual(self.run_readout("--calibrate", "--detail"), self.run_readout("--calibrate", "--detail"))

    def test_k1_k2_and_cut(self):
        text = self.run_readout("--detail")
        self.assertIn("merged pull requests 5 (agent-platform=4, ontology-platform=1) K1 engineering usd per merged pull request = 8.82; role lead usd=22.90 share of the route=51.9%", text)
        self.assertIn("K2 agent-platform#7 implementer=opus verifier=haiku first verdict=accept lines=15", text)
        self.assertIn("K2 agent-platform#8 implementer=unknown verifier=none first verdict=none lines=5", text)
        self.assertIn("K2 implementer=opus merged=1 lines=15: accept n=1 lines=15; verifier tiers: haiku=1", text)
        self.assertIn("K2 implementer=unknown merged=1 lines=5: none n=1 lines=5; verifier tiers: none", text)
        self.assertIn("K2 no reviewer stage: agent-platform=2, ontology-platform=1\n", text)
        self.assertNotIn("K2 no reviewer stage agent-platform:", text)  # the numbers only with the option
        self.assertIn("K2 no reviewer stage agent-platform: 8,11", self.run_readout("--list-no-reviewer"))
        self.assertIn("stages cut: implementer/sonnet=1; no date: integration/unknown=1\n", text)  # an older-form record is counted

    def test_k2_join_is_scoped_to_the_repository(self):
        # pull request 3 of another repository has the head branch of pull request 7: it takes its own stage, not the first one's
        text = self.run_readout("--detail")
        self.assertIn("K2 agent-platform#7 implementer=opus verifier=haiku", text)
        self.assertIn("K2 ontology-platform#3 implementer=fable verifier=none first verdict=none lines=4", text)

    def test_k2_reviewer_on_a_detached_head_joins_by_the_worktree(self):
        text = self.run_readout("--detail")
        self.assertIn("K2 agent-platform#10 implementer=sonnet verifier=opus first verdict=repair lines=20", text)

    def test_k2_detached_reviewer_after_the_merge_does_not_join_by_the_worktree(self):
        # a reused worktree: pull request 12 merged on the 12th, a detached reviewer of the same worktree started on the 13th
        wt = str(self.tmp / "work" / "t5" / "wt" / "agent-platform")
        stage = lambda role, model, session, start: json.dumps({"role": role, "model": model, "session": session, "cwd": wt, "start": start,
                                                                "state": "ok", "tokens": {}, "resumes": None, "answer": None}) + "\n"
        (self.tmp / "work" / "t5").mkdir()
        (self.tmp / "work" / "t5" / "i.md.run.json").write_text(stage("implementer", "sonnet", "s-late-impl", "2026-11-12T08:00:00Z"))
        (self.tmp / "work" / "t5" / "r.md.run.json").write_text(stage("reviewer", "opus", "s-late-rev", "2026-11-13T09:00:00Z"))
        user = lambda sid, branch: json.dumps({"type": "user", "sessionId": sid, "timestamp": "2026-11-12T08:00:00Z", "cwd": wt, "gitBranch": branch}) + "\n"
        (self.tmp / "acct1" / "projects" / "p8" / "s-late.jsonl").write_text(user("s-late-impl", "feat/late") + user("s-late-rev", "HEAD"))
        pr = {"number": 12, "headRefName": "feat/late", "mergedAt": "2026-11-12T12:00:00Z", "additions": 1, "deletions": 0}
        readout.run_gh = lambda repo, since, until: PRS.get(repo, []) + ([pr] if repo.endswith("/agent-platform") else [])
        self.assertIn("K2 agent-platform#12 implementer=sonnet verifier=none first verdict=none lines=1", self.run_readout("--detail"))

    def test_k2_join_uses_a_session_file_older_than_the_window(self):
        text = self.run_readout("--detail")
        self.assertIn("K2 agent-platform#11 implementer=haiku verifier=none first verdict=none lines=2", text)

    def test_a_session_has_one_cwd_for_role_and_route(self):
        # the session s-cd starts in the Lead's directory, then works in a worktree and in a directory of no route
        text = self.run_readout()
        eng = re.search(r"^week \S+ route engineering TOTAL.*?(?=^week)", text, re.M | re.S).group(0)
        self.assertRegex(eng, r"role=lead family=opus msgs=5 .* usd=7.00")
        self.assertRegex(eng, r"role=worker, no record family=opus msgs=2 .* usd=11.00")
        self.assertEqual(self.route(text, "other")[0], 1)

    def test_fork_file_order_does_not_change_the_output(self):
        before = self.run_readout("--calibrate", "--detail")
        (self.tmp / "acct1" / "projects" / "p0").mkdir()
        shutil.move(self.tmp / "acct2" / "projects" / "p1" / "s-fork.jsonl", self.tmp / "acct1" / "projects" / "p0" / "s-fork.jsonl")
        self.assertEqual(before, self.run_readout("--calibrate", "--detail"))

    def test_roles_of_sessions_without_a_stage_record(self):
        # The old rule filed every session without a record as `interactive`: the Lead's and two workers' sessions
        # (a worktree below the work root, a platform checkout) were in one line.
        text = self.run_readout()
        eng = re.search(r"^week \S+ route engineering TOTAL.*?(?=^week)", text, re.M | re.S).group(0)
        self.assertRegex(eng, r"role=lead family=opus msgs=5 .* usd=7.00")
        self.assertRegex(eng, r"role=lead family=haiku msgs=2 .* usd=15.90")
        self.assertRegex(eng, r"role=worker, no record family=opus msgs=2 .* usd=11.00")
        self.assertEqual(re.findall(r"role=interactive family=(\S+)", eng), ["gpt-9-x"])
        self.assertIn("role=interactive family=opus msgs=1 input=3,000,000", text)  # the lesson session stays interactive

    def test_empty_route_and_uncounted_cli_are_shown(self):
        text = self.run_readout("--since", "2026-11-16", "--until", "2026-11-16")
        for route in ("engineering", "lesson", "other"):
            self.assertIn(f"week 2026-11-16 route {route} msgs=0\n", text)
        self.assertRegex(text, r"(?m)^uncounted: agy \(no usage record")
        self.assertIn("route lesson session files in the window: claude=0 codex=0", text)
        self.assertIn("route lesson session files in the window: claude=1 codex=0", self.run_readout())

    def test_lead_calls_by_first_tool(self):
        text = self.run_readout()
        self.assertIn("lead calls first_tool=Bash calls=1 usd=15.00", text)
        self.assertIn("lead calls first_tool=none calls=5 usd=5.90", text)

    def test_haiku_long_prompt_rate_and_unknown_model_cost(self):
        rates = readout.load_prices(self.tmp / "prices.tsv")
        tok = lambda n: {"input": n, "output": 0, "cache_read": 0, "cache_write_5m": 0, "cache_write_1h": 0}
        self.assertAlmostEqual(readout.cost("claude-haiku-9-9", tok(900000), rates), 0.9)
        self.assertAlmostEqual(readout.cost("claude-haiku-9-9", tok(1500000), rates), 15.0)
        self.assertEqual(readout.cost("claude-nothing", tok(10 ** 6), rates), 0.0)

    def test_reviewer_answer_other_than_a_verdict_line_is_not_printed(self):
        self.assertEqual(readout.first_line(self.tmp / "work" / "t1" / "rev0.md"), "no-verdict-line")
        self.assertEqual(readout.first_line(self.tmp / "work" / "t1" / "missing.md"), "unknown")



NEW_LINES = ("lead session ", "lead state_bytes=", "stages runtime=", "uncounted outside stages", "units ")


class Rb1(unittest.TestCase):
    """The bookkeeping share of a Lead session, the stages of Muse Code and agy, the units by the Lead's model:
    every record is written by the test, with invented ids."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="readout-rb1-"))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.work, self.acct = self.tmp / "work", self.tmp / "acct" / "projects" / "p"
        (self.work / "lead").mkdir(parents=True)
        self.acct.mkdir(parents=True)
        (self.tmp / "prices.tsv").write_text((FIXTURES / "prices.tsv").read_text() + "muse-priced\tinput\t10\nmuse-priced\toutput\t20\nmuse-priced\tcache_read\t1\n"
                                             "muse-priced\tcache_write_5m\t0\nmuse-priced\tcache_write_1h\t0\n")
        readout.run_gh = lambda repo, since, until: []
        self.seq = 0

    def line(self, sid, ts, blocks, out=1000, cwd=None, mid=None, model="claude-opus-9-9"):
        self.seq += 1
        return json.dumps({"type": "assistant", "sessionId": sid, "timestamp": ts, "cwd": cwd or str(self.work / "lead"), "gitBranch": "main",
                           "message": {"id": mid or f"m{self.seq}", "model": model, "usage": {"input_tokens": 1, "output_tokens": out},
                                       "content": blocks}}) + "\n"

    def use(self, uid, name, **inp):
        return {"type": "tool_use", "id": uid, "name": name, "input": inp}

    def session(self, fname, *lines):
        with open(self.acct / fname, "a") as fh:
            fh.writelines(lines)

    def run_readout(self, *extra, lead=None):
        out = io.StringIO()
        args = ["--since", "2026-11-11", "--until", "2026-11-13", "--prices", str(self.tmp / "prices.tsv"), "--accounts", str(self.tmp / "acct"),
                "--codex-dir", str(self.tmp / "codex"), "--work-root", str(self.work), "--platforms-root", str(self.tmp / "platforms"),
                "--repo", "park-kyungchan/agent-platform", *(["--lead-cwd", str(lead or self.work / "lead")]), *extra]
        with contextlib.redirect_stdout(out):
            readout.main(args)
        return out.getvalue()

    def stage(self, name, **o):
        d = self.work / "t" / "runs"
        d.mkdir(parents=True, exist_ok=True)
        rec = {"role": "implementer", "runtime": "muse", "model": "muse-priced", "session": "s-" + name, "cwd": str(self.work / "t"),
               "start": "2026-11-12T08:00:00Z", "state": "ok", "tokens": {"input": 1, "output": 2, "cache_read": 3, "cache_creation": 4},
               "resumes": None, "answer": None, **o}
        (d / f"{name}.run.json").write_text(json.dumps(rec))
        return d / f"{name}.run.json"

    # point 1
    def test_each_class_and_the_first_class_wins(self):
        sid = "abcdefgh-1111"
        big = "x" * 100
        calls = [self.use("t1", "Write", file_path="/w/STATE.md", content=big),
                 self.use("t2", "Edit", file_path="/w/HANDOVER.md", old_string="a", new_string="b"),
                 self.use("t3", "Bash", command="cat /w/STATE.md"),  # a read: no class
                 self.use("t4", "Bash", command="echo hi >> /w/STATE.md"),
                 self.use("t5", "Bash", command="git commit -m 'msg'"),
                 self.use("t6", "Bash", command="ont describe x"),
                 self.use("t7", "Bash", command="ap row decide"),
                 self.use("t8", "Bash", command="python3 -c 'record_decision'"),
                 self.use("t9", "Bash", command="git commit -m x && echo y > /w/STATE.md"),  # state and commit: state
                 self.use("t10", "Bash", command="git commit -m x && ont apply"),  # commit and store: commit
                 self.use("t11", "Write", file_path="/w/other.md", content=big)]
        size = lambda b: len(json.dumps(b["input"], ensure_ascii=False))
        self.session("a.jsonl", *[self.line(sid, "2026-11-12T09:00:00Z", [c], out=0 if i else 4000, mid=f"x{i}") for i, c in enumerate(calls)])
        text = self.run_readout()
        cls = {"state": [calls[i] for i in (0, 1, 3, 8)], "commit": [calls[i] for i in (4, 9)], "store": [calls[i] for i in (5, 6, 7)]}
        pct = {k: f"{sum(size(b) for b in v) / 4 / 4000 * 100:.1f}" for k, v in cls.items()}
        total = f"{sum(size(b) for v in cls.values() for b in v) / 4 / 4000 * 100:.1f}"
        self.assertIn(f"lead session abcdefgh family=opus output=4,000 bookkeeping={total} state={pct['state']} commit={pct['commit']} store={pct['store']}\n", text)
        self.assertEqual(text.count("lead session "), 1)

    def test_a_block_is_counted_once_across_lines_and_fork_files(self):
        sid, call = "s-once-xyz", self.use("t1", "Write", file_path="/w/STATE.md", content="y" * 300)
        one = self.line(sid, "2026-11-12T09:00:00Z", [call], out=1000, mid="same")
        self.session("a.jsonl", one, one, self.line(sid, "2026-11-12T09:00:01Z", [call, self.use("t2", "Bash", command="ont x")], out=1000, mid="same"))
        (self.tmp / "acct" / "projects" / "q").mkdir()
        (self.tmp / "acct" / "projects" / "q" / "fork.jsonl").write_text(one)
        size = len(json.dumps(call["input"], ensure_ascii=False)) / 4 / 1000 * 100
        text = self.run_readout()
        self.assertRegex(text, rf"lead session s-once-x family=opus output=1,000 bookkeeping=\S+ state={size:.1f} commit=0.0 store=")

    def test_only_a_lead_session_with_a_message_in_the_window_has_a_line(self):
        call = self.use("t1", "Write", file_path="/w/STATE.md", content="z")
        self.session("a.jsonl", self.line("s-outside-1", "2026-11-12T09:00:00Z", [call], cwd=str(self.tmp / "elsewhere")),  # outside every --lead-cwd
                     self.line("s-staged-01", "2026-11-12T09:00:00Z", [call]),  # inside one, with a stage record
                     self.line("s-early-001", "2026-11-09T09:00:00Z", [call]))  # no message in the window
        self.stage("w", session="s-staged-01", runtime="claude")
        text = self.run_readout()
        self.assertNotIn("lead session ", text)
        self.assertIn("lead state_bytes=absent\n", text)

    def test_state_bytes_with_and_without_the_file(self):
        self.assertIn("lead state_bytes=absent\n", self.run_readout())
        (self.work / "lead" / "STATE.md").write_text("12345678")
        self.assertIn("lead state_bytes=8\n", self.run_readout())

    def test_family_is_the_one_of_most_output_and_the_input_is_never_printed(self):
        self.session("a.jsonl", self.line("s-fam-00001", "2026-11-12T09:00:00Z", [self.use("t", "Bash", command="echo SECRET-INPUT")], out=10),
                     self.line("s-fam-00001", "2026-11-12T09:01:00Z", [], out=90, model="claude-haiku-9-9"))
        text = self.run_readout()
        self.assertIn("lead session s-fam-00 family=haiku output=100 bookkeeping=0.0", text)
        self.assertNotIn("SECRET-INPUT", text)

    # point 2
    def test_stage_lines_per_runtime_and_model(self):
        self.stage("a1")
        self.stage("a2", tokens={"input": 10, "output": 20, "cache_read": 30, "cache_creation": 0})
        self.stage("b1", model="muse-nopriced", runtime="agy", tokens=None, state="running")
        self.stage("c1", runtime="claude")
        self.stage("d1", start="2026-11-14T00:00:00Z")
        text = self.run_readout()
        self.assertIn("stages runtime=muse model=muse-priced n=2 input=11 output=22 cache_read=33 usd=0.00\n", text)
        self.assertIn("stages runtime=agy model=muse-nopriced n=1 input=0 output=0 cache_read=0 usd=unknown\n", text)
        self.assertEqual(text.count("stages runtime="), 2)
        self.assertRegex(text, r"stages runtime=agy .*\nstages runtime=muse .*\nuncounted outside stages: agy muse\n")
        self.assertRegex(text, r"(?m)^uncounted: agy \(no usage record exists under its state directory\)$")
        self.stage("e1", tokens={"input": 1000000, "output": 1000000, "cache_read": 1000000, "cache_creation": 0})
        self.assertIn("n=3 input=1,000,011 output=1,000,022 cache_read=1,000,033 usd=31.00\n", self.run_readout())

    # point 3
    def unit(self, name, start, state, repo="park-kyungchan/agent-platform", pr=5, page=None, **o):
        d = self.work / "t" / "runs"
        d.mkdir(parents=True, exist_ok=True)
        rec = {"repository": repo, "pull_request": pr, "start": start, "state": state, "stopped_at": None, "page": {"path": page or "/pages/p.md"},
               "verdict_file": None, "stages": {"implementer": None, "verifier": None}, **o}
        (d / f"{name}.unit.json").write_text(json.dumps(rec))

    def answer(self, name, first):
        (self.work / "t" / "runs").mkdir(parents=True, exist_ok=True)
        p = self.work / "t" / "runs" / f"{name}.verifier.md"
        p.write_text(first + "\nrest\n")
        rec = self.stage(f"{name}.verifier", role="reviewer", runtime="claude", model="sonnet", answer={"path": str(p)})
        return str(rec)

    def lead_writes(self, sid, ts, page, model="claude-opus-9-9", fname="u.jsonl"):
        self.session(fname, self.line(sid, ts, [self.use(f"w{self.seq}", "Write", file_path=page, content="p")], model=model))

    def test_units_chains_and_first_verdicts(self):
        t = "2026-11-12T"
        page = lambda n: str(self.work / "t" / f"{n}.md")
        for n, sid, model in (("c1", "s-lead-opus", "claude-opus-9-9"), ("c2", "s-lead-opus", "claude-opus-9-9"), ("c3", "s-lead-haiku", "claude-haiku-9-9"),
                              ("c4", "s-lead-haiku", "claude-haiku-9-9"), ("c5", "s-lead-opus", "claude-opus-9-9")):
            self.lead_writes(sid, t + "07:00:00Z", page(n), model=model)
        impl = str(self.stage("c1.impl", runtime="claude", model="opus-impl"))
        # c1: three records, two verdict_repair then merged: repair_twice, first verdict repair
        self.unit("c1-a", t + "08:00:00Z", "stopped", pr=1, page=page("c1"), cause="verdict_repair", stages={"implementer": impl, "verifier": self.answer("c1-a", "Verdict: repair")})
        self.unit("c1-b", t + "09:00:00Z", "stopped", pr=1, page=page("c1"), cause="verdict_repair", verdict_file={"path": str(self.work / "t" / "runs" / "c1-a.verifier.md")})
        self.unit("c1-c", t + "10:00:00Z", "merged", pr=1, page=page("c1"))
        # c2: two records stopped at gate with other causes, then accepted: not repair_twice; first verdict accept
        self.unit("c2-a", t + "08:00:00Z", "stopped", pr=2, page=page("c2"), cause="gate_failed", stages={"implementer": None, "verifier": self.answer("c2-a", "Verdict: accept")})
        self.unit("c2-b", t + "09:00:00Z", "accepted", pr=2, page=page("c2"), cause="gate_failed")
        # c3: the verifier's answer is gone: unknown
        self.unit("c3-a", t + "08:00:00Z", "merged", pr=3, page=page("c3"), stages={"implementer": None, "verifier": str(self.work / "t" / "runs" / "gone.run.json")})
        # c4: still running: not counted; and a record without a pull request, merged: a chain of its own, no verifier: none
        self.unit("c4-a", t + "08:00:00Z", "running", pr=4, page=page("c4"))
        self.unit("c5-a", t + "08:30:00Z", "merged", pr=None, page=page("c5"))
        # a chain that began before the window
        self.unit("old-a", "2026-11-09T08:00:00Z", "merged", pr=9, page=page("c1"))
        text = self.run_readout("--detail")
        chains = [l for l in text.splitlines() if l.startswith("units chain ")]
        self.assertEqual(chains, [
            "units chain park-kyungchan/agent-platform#1 lead_family=opus implementer=opus-impl first_verdict=repair repair_twice=yes",
            "units chain park-kyungchan/agent-platform#2 lead_family=opus implementer=none first_verdict=accept repair_twice=no",
            "units chain park-kyungchan/agent-platform#3 lead_family=haiku implementer=none first_verdict=unknown repair_twice=no",
            "units chain park-kyungchan/agent-platform#none lead_family=opus implementer=none first_verdict=none repair_twice=no"])
        self.assertIn("units lead_family=haiku chains=1 first_verdict accept=0 repair=0 unknown=1 none=0 repair_twice=0\n"
                      "units lead_family=opus chains=3 first_verdict accept=1 repair=1 unknown=0 none=1 repair_twice=1\n", text)
        self.assertNotIn("units chain", self.run_readout())

    def test_the_writer_of_the_page_is_the_lead_and_without_one_the_latest_message_is(self):
        t, page = "2026-11-12T", str(self.work / "t" / "p.md")
        self.lead_writes("s-writer-opus", t + "06:00:00Z", page)
        self.session("u.jsonl", self.line("s-later-haiku", t + "07:00:00Z", [], model="claude-haiku-9-9"),
                     self.line("s-after-sonn", t + "09:30:00Z", [], model="claude-opus-9-9"))  # a message after the start counts for nobody
        self.unit("a", t + "08:00:00Z", "merged", pr=1, page=page)
        self.unit("b", t + "08:10:00Z", "merged", pr=2, page=str(self.work / "t" / "unwritten.md"))
        text = self.run_readout("--detail")
        self.assertIn("#1 lead_family=opus ", text)
        self.assertIn("#2 lead_family=haiku ", text)

    def test_a_chain_without_a_lead_session_before_it_is_unknown(self):
        self.unit("a", "2026-11-12T08:00:00Z", "merged", pr=1)
        self.session("u.jsonl", self.line("s-too-late-1", "2026-11-12T09:00:00Z", []))
        self.assertIn("units lead_family=unknown chains=1 first_verdict accept=0 repair=0 unknown=0 none=1 repair_twice=0\n", self.run_readout())

    # point 4
    def test_the_old_fixture_is_unchanged_except_the_new_lines(self):
        t = Readout("test_i5_second_run_is_identical")
        t.setUp()
        self.addCleanup(t.doCleanups)
        text = t.run_readout("--calibrate", "--detail")
        old = (FIXTURES / "expected-before.txt").read_text()
        self.assertEqual("".join(l for l in text.splitlines(True) if not l.startswith(NEW_LINES)), old)
        self.assertIn("uncounted outside stages: agy muse\n", text)
        order = [text.index(x) for x in ("stages cut:", "lead session ", "lead state_bytes=", "uncounted outside stages", "lead calls first_tool")]
        self.assertEqual(order, sorted(order))

    def test_the_new_lines_print_no_path_and_the_run_writes_nothing(self):
        self.lead_writes("s-lead-opus", "2026-11-12T07:00:00Z", str(self.work / "t" / "p.md"))
        self.unit("a", "2026-11-12T08:00:00Z", "merged", pr=1, page=str(self.work / "t" / "p.md"))
        self.stage("m1")
        before = tree_digest(self.tmp)
        text = self.run_readout("--detail")
        self.assertEqual(before, tree_digest(self.tmp))
        self.assertNotIn(str(self.tmp), text)


    # round 2: the state class names a write target; the rules of points 1, 3 and 4 each have a test that fails without them
    def test_a_bash_call_is_state_only_when_a_write_target_names_the_file(self):
        cls = lambda cmd: readout.call_of({"type": "tool_use", "id": "x", "name": "Bash", "input": {"command": cmd}})[0]
        py_reads = "python3 - <<'EOF'\ntext = open('/w/STATE.md').read()\nopen('/tmp/other.md', 'w').write(text)\nEOF"
        for cmd in ("cat /w/STATE.md", "cp STATE.md /tmp/bak", "mv /w/HANDOVER.md /tmp/h", "cp /w/STATE.md /tmp/b && echo done", py_reads,
                    "grep x /w/STATE.md > /tmp/out", "sed -n 1p /w/STATE.md", "cat /w/STATE.md | tee /tmp/copy", "sed -i s/a/b/ /tmp/other.md; cat /w/STATE.md"):
            self.assertIsNone(cls(cmd), cmd)
        py_writes = "python3 - <<'EOF'\nfrom pathlib import Path\nP = '/w/STATE.md'\nPath(P).write_text('x')\nEOF"
        py_open = "python3 - <<'EOF'\nSTATE = \"/w/STATE.md\"\nwith open(STATE, 'w') as f:\n    f.write('x')\nEOF"
        for cmd in ("echo x > /w/STATE.md", "cat >> /w/HANDOVER.md <<'EOF'\nx\nEOF", "echo x | tee -a /w/STATE.md", "sed -i s/a/b/ /w/STATE.md",
                    "cp /tmp/x /w/STATE.md", "mv /tmp/x /w/HANDOVER.md && git status", "python3 -c \"open('/w/STATE.md','w').write('x')\"",
                    "python3 -c \"from pathlib import Path; Path('/w/STATE.md').write_text('x')\"", py_writes, py_open):
            self.assertEqual(cls(cmd), "state", cmd)

    def test_every_block_of_a_message_counts_across_its_lines(self):
        sid = "s-multi-0001"
        store, commit = self.use("b1", "Bash", command="ont " + "a" * 400), self.use("b2", "Bash", command="git commit -m " + "b" * 400)
        state = self.use("b3", "Write", file_path="/w/STATE.md", content="c" * 400)
        first, second = self.line(sid, "2026-11-12T09:00:00Z", [store, commit], out=1000, mid="mm"), self.line(sid, "2026-11-12T09:00:01Z", [state], out=1000, mid="mm")
        self.session("a.jsonl", first, second)
        (self.tmp / "acct" / "projects" / "q").mkdir()
        (self.tmp / "acct" / "projects" / "q" / "fork.jsonl").write_text(first)
        pct = lambda b: f"{len(json.dumps(b['input'], ensure_ascii=False)) / 4 / 1000 * 100:.1f}"
        total = f"{sum(len(json.dumps(b['input'], ensure_ascii=False)) for b in (store, commit, state)) / 4 / 1000 * 100:.1f}"
        self.assertIn(f"output=1,000 bookkeeping={total} state={pct(state)} commit={pct(commit)} store={pct(store)}\n", self.run_readout())

    def test_only_the_window_counts_for_the_output_and_the_calls(self):
        inside = self.use("w1", "Bash", command="ont " + "a" * 400)
        outside = self.use("w2", "Bash", command="git commit -m " + "b" * 400)
        self.session("a.jsonl", self.line("s-win-00001", "2026-11-12T09:00:00Z", [inside], out=1000),
                     self.line("s-win-00001", "2026-11-15T09:00:00Z", [outside], out=3000))
        share = len(json.dumps(inside["input"], ensure_ascii=False)) / 4 / 1000 * 100
        self.assertIn(f"output=1,000 bookkeeping={share:.1f} state=0.0 commit=0.0 store={share:.1f}\n", self.run_readout())

    def test_apply_action_and_the_size_of_a_call_in_characters(self):
        call = self.use("u1", "Bash", command="apply_action " + "\u00e9" * 100)
        self.session("a.jsonl", self.line("s-uni-00001", "2026-11-12T09:00:00Z", [call], out=1000))
        share = len(json.dumps(call["input"], ensure_ascii=False)) / 4 / 1000 * 100  # 12.4: with ensure_ascii it would be 24.9
        self.assertIn(f"bookkeeping={share:.1f} state=0.0 commit=0.0 store={share:.1f}\n", self.run_readout())

    def test_chains_by_repository_and_pull_request_in_the_order_of_start(self):
        t, other = "2026-11-12T", "park-kyungchan/other-platform"
        pa, pb = str(self.work / "t" / "pa.md"), str(self.work / "t" / "pb.md")
        self.lead_writes("s-lead-haiku", t + "07:00:00Z", pa, model="claude-haiku-9-9")
        self.lead_writes("s-lead-opus", t + "07:00:00Z", pb)
        # `U-A-2.unit.json` sorts before `U-A.unit.json` by name, and starts after it
        self.unit("U-A", t + "08:00:00Z", "stopped", pr=7, page=pa, cause="verdict_repair", stages={"implementer": None, "verifier": self.answer("U-A", "Verdict: repair")})
        self.unit("U-A-2", t + "09:00:00Z", "merged", pr=7, page=pb, stages={"implementer": None, "verifier": self.answer("U-A-2", "Verdict: accept")})
        self.unit("U-B", t + "08:30:00Z", "merged", repo=other, pr=7, page=pb)  # the same number in another repository: a chain of its own
        text = self.run_readout("--detail")
        self.assertEqual([l for l in text.splitlines() if l.startswith("units chain ")], [
            "units chain park-kyungchan/agent-platform#7 lead_family=haiku implementer=none first_verdict=repair repair_twice=no",
            f"units chain {other}#7 lead_family=opus implementer=none first_verdict=none repair_twice=no"])

    def test_the_first_verdict_the_window_and_repair_twice_follow_the_records_of_the_chain(self):
        t, page = "2026-11-12T", str(self.work / "t" / "p.md")
        self.lead_writes("s-lead-opus", t + "07:00:00Z", page)
        # the earliest record names no verifier: the first verdict is the one of the first record that names one; two stops at the gate, another cause
        self.unit("F-a", t + "08:00:00Z", "stopped", pr=21, page=page, stopped_at="gate", cause="gate_red")
        self.unit("F-b", t + "09:00:00Z", "stopped", pr=21, page=page, stopped_at="gate", cause="gate_red")
        self.unit("F-c", t + "10:00:00Z", "merged", pr=21, page=page, stages={"implementer": None, "verifier": self.answer("F-c", "Verdict: accept")})
        # begun before the window, finished inside it: not counted
        self.unit("W-a", "2026-11-10T08:00:00Z", "stopped", pr=20, page=page)
        self.unit("W-b", t + "08:00:00Z", "merged", pr=20, page=page)
        text = self.run_readout("--detail")
        self.assertEqual([l for l in text.splitlines() if l.startswith("units ")], [
            "units chain park-kyungchan/agent-platform#21 lead_family=opus implementer=none first_verdict=accept repair_twice=no",
            "units lead_family=opus chains=1 first_verdict accept=1 repair=0 unknown=0 none=0 repair_twice=0"])

    def test_the_units_lines_follow_the_stages_lines(self):
        page = str(self.work / "t" / "p.md")
        self.lead_writes("s-lead-opus", "2026-11-12T07:00:00Z", page)
        self.unit("a", "2026-11-12T08:00:00Z", "merged", pr=1, page=page)
        self.stage("m1")
        text = self.run_readout()
        order = [text.index(x) for x in ("stages cut:", "lead state_bytes=", "stages runtime=", "uncounted outside stages", "units lead_family=")]
        self.assertEqual(order, sorted(order))


if __name__ == "__main__":
    unittest.main()
