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


if __name__ == "__main__":
    unittest.main()
