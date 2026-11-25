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
                dst.write_text(src.read_text().replace("@WORK@", str(self.tmp / "work")).replace("@PLATFORMS@", str(self.tmp / "platforms")).replace("@BLOCKING@", str(self.tmp / "blocking")))
        old = datetime.datetime(2026, 11, 10, 11, tzinfo=datetime.timezone.utc).timestamp()  # the day before the window
        os.utime(self.tmp / "acct1" / "projects" / "p9" / "s-early.jsonl", (old, old))
        readout.run_gh = lambda repo, since, until: PRS.get(repo, [])
        # The fixture's sessions under this test's temporary directory are temporary, as under /tmp, where the directory
        # lies outside a stage; a stage's TMPDIR, and so the directory, lies elsewhere.
        self.addCleanup(setattr, readout, "TEMP_ROOTS", readout.TEMP_ROOTS)
        readout.TEMP_ROOTS = (*readout.TEMP_ROOTS, tempfile.gettempdir())

    def run_readout(self, *extra, lead_flag=True, stage_dirs=None):
        out = io.StringIO()
        stage_dirs = [self.tmp / "stage-none"] if stage_dirs is None else stage_dirs  # a scratch directory: no test reads the host's records
        args = ["--since", "2026-11-11", "--until", "2026-11-13", "--prices", str(self.tmp / "prices.tsv"),
                "--accounts", str(self.tmp / "acct1"), str(self.tmp / "acct2"), str(self.tmp / "acct1"),
                "--codex-dir", str(self.tmp / "codex"), "--work-root", str(self.tmp / "work"),
                "--platforms-root", str(self.tmp / "platforms"), *(["--lead-cwd", str(self.tmp / "work" / "lead")] if lead_flag else []),
                *[x for d in stage_dirs for x in ("--stage-dir", str(d))],
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
        for route in ("engineering", "lesson", "temporary", "other"):  # temporary: msgs=0 here, so no block
            if f"route {route} msgs=0" in text:
                continue
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
        for route in ("engineering", "lesson", "temporary", "other"):
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

    # w0-cost: stage records of the store, the Lead without a flag, review kinds, the temporary route.
    def session(self, sid, cwd, mid, usd_input, ts="2026-11-12T20:00:00Z", model="claude-opus-9-9"):
        line = {"type": "assistant", "sessionId": sid, "timestamp": ts, "cwd": cwd, "gitBranch": "main",
                "message": {"id": mid, "model": model, "usage": {"input_tokens": usd_input, "output_tokens": 0, "cache_read_input_tokens": 0,
                                                                  "cache_creation_input_tokens": 0}}}
        (self.tmp / "acct1" / "projects" / "p8" / f"{sid}.jsonl").write_text(json.dumps(line) + "\n")

    def stage_record(self, path, role, sid, prompt, cwd, start="2026-11-12T19:00:00Z"):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps({"role": role, "model": "claude-opus-9-9", "session": sid, "cwd": cwd, "start": start, "state": "ok",
                                    "prompt": {"path": prompt}, "tokens": {}, "resumes": None, "answer": None}) + "\n")

    def store(self):
        """Stage records as the store lays them: reviews at the top level, a unit's records one level down, a copy deeper."""
        root, u = self.tmp / "store", "/var/lib/x/units/u1/1.verifier.prompt.md"
        elsewhere = str(self.tmp / "elsewhere")  # a cwd under no root: only the stage directory makes these engineering
        self.stage_record(root / "reviews" / "a.md.run.json", "reviewer", "s-pg", "/r/prompts/page-a-1.md", elsewhere)
        self.stage_record(root / "reviews" / "b.md.run.json", "reviewer", "s-pl", "/r/prompts/plan-b-1.md", elsewhere)
        self.stage_record(root / "units" / "u1" / "2.verifier.md.run.json", "reviewer", "s-pk", u, elsewhere)
        self.stage_record(root / "reviews" / "c.md.run.json", "reviewer", "s-ot", "/r/prompts/label-1.md", elsewhere)
        self.stage_record(root / "units" / "u1" / "1.implementer.md.run.json", "implementer", "s-im", "/x/1.implementer.prompt.md", elsewhere)
        self.stage_record(root / "units" / "u1" / "wt" / "deep" / "d.md.run.json", "reviewer", "s-deep", "/r/prompts/page-d-1.md", elsewhere)
        for sid, n in (("s-pg", 1), ("s-pl", 2), ("s-pk", 3), ("s-ot", 4), ("s-im", 5), ("s-deep", 6)):
            self.session(sid, elsewhere, "m-" + sid, n * 1000000)
        return [root / "reviews", root / "units"]

    def test_stage_record_outside_work_gives_role_route_and_review_kind(self):
        text = self.run_readout(stage_dirs=self.store())
        eng = re.search(r"^week 2026-11-09 route engineering TOTAL.*?(?=^week)", text, re.M | re.S).group(0)
        self.assertRegex(eng, r"role=implementer family=opus msgs=2 .* usd=9.10")  # 4.10 of the fixture and s-im 5.00
        self.assertRegex(eng, r"role=reviewer family=opus msgs=5 .* usd=12.00")  # the fixture's 2.00 and four store reviews 1+2+3+4
        self.assertEqual(self.route(text, "other")[0], 1)  # s-other stays; none of the store's
        self.assertEqual(self.route(text, "engineering")[0], 20)  # 15 + five store sessions; the one deeper than the depth is not read
        self.assertIn("stage_dir_records=5", text)
        for kind, usd in (("page", "1.00"), ("plan", "2.00"), ("packet", "3.00"), ("other", "6.00")):
            self.assertRegex(text, rf"(?m)^week 2026-11-09 reviewer kind {kind} msgs=\d+ .* usd={usd}$")
        self.assertNotIn("s-deep", text)

    def test_stage_dir_depth_is_fixed_and_the_default_is_replaced(self):
        # the copy below a unit's worktree is not read; --stage-dir replaces the defaults, it does not add to them
        text = self.run_readout(stage_dirs=self.store())
        self.assertNotRegex(text, r"reviewer kind page msgs=\d+ .* usd=7.00")  # 1.00 only: the deep copy (6.00) is not counted
        none = self.run_readout()
        self.assertNotIn("stage_dir_records", none)
        self.assertEqual(self.route(none, "engineering")[0], 15)

    def test_review_kinds_do_not_change_a_route_total(self):
        text = self.run_readout(stage_dirs=self.store())
        base = self.run_readout()
        self.assertEqual(self.route(text, "lesson"), self.route(base, "lesson"))
        rows = lambda t: sum(float(x) for x in re.findall(r"^  role=reviewer .* usd=([\d.]+)$", t, re.M))
        kinds = sum(float(x) for x in re.findall(r"^week \S+ reviewer kind \w+ msgs=\d+ .* usd=([\d.]+)$", text, re.M))
        self.assertAlmostEqual(kinds, rows(text))
        self.assertIn("week 2026-11-09 reviewer kind packet msgs=0", base)  # no store: the fixture's reviewer has no kind of the store

    def test_lead_without_flag(self):
        # no --lead-cwd: the sessions of a task directory below the work root with no record are the Lead's; a worktree,
        # the lesson directory, a platform checkout and a directory of no root are not
        text = self.run_readout(lead_flag=False)
        eng = re.search(r"^week \S+ route engineering TOTAL.*?(?=^week)", text, re.M | re.S).group(0)
        self.assertRegex(eng, r"role=lead family=opus msgs=5 .* usd=7.00")
        self.assertRegex(eng, r"role=lead family=haiku msgs=2 .* usd=15.90")
        self.assertRegex(eng, r"role=worker, no record family=opus msgs=2 .* usd=11.00")
        self.assertIn("role=interactive family=opus msgs=1 input=3,000,000", text)  # the lesson session
        self.assertIn("lead calls first_tool=Bash calls=1 usd=15.00", text)
        # a flag that names another directory keeps the old rule: only that directory is the Lead's
        other = self.run_readout("--lead-cwd", str(self.tmp / "work" / "nowhere"), lead_flag=False)
        self.assertNotIn("role=lead", other)

    def test_a_session_without_a_stage_record_under_a_temporary_directory_is_temporary(self):
        for n, d in enumerate(("/tmp/x", "/var/tmp/y/z", "/tmp")):
            self.session(f"s-t{n}", d, f"m-t{n}", 1000000)
        self.session("s-tl", "/tmp/l", "m-tl", 7000000)  # has a stage record of the work root: the rule of the temporary route does not apply
        self.stage_record(self.tmp / "work" / "t6" / "x.md.run.json", "worker", "s-tl", "/p/other-1.md", "/tmp/l")
        self.session("s-tmpx", "/tmpx/a", "m-tmpx", 1000000)  # not a temporary directory
        text = self.run_readout()
        self.assertEqual(self.route(text, "temporary")[:1] + self.route(text, "temporary")[6:], [3, 3.0])
        self.assertEqual(self.route(text, "other")[0], 3)  # s-other, s-tl, s-tmpx
        self.assertRegex(text, r"week 2026-11-09 route temporary TOTAL msgs=3 .*\n  role=interactive family=opus msgs=3 ")

    def test_work_root_and_platforms_root_come_before_the_temporary_route(self):
        route = lambda cwd, stage=None: readout.route_of(cwd, "/tmp/w", "/var/tmp/p", "/tmp/w/edu", stage)  # roots that lie under temporary directories
        self.assertEqual(route("/tmp/w/t"), "engineering")
        self.assertEqual(route("/var/tmp/p/repo"), "engineering")
        self.assertEqual(route("/tmp/w/edu/x"), "lesson")
        self.assertEqual(route("/tmp/other"), "temporary")
        self.assertEqual(route("/tmp/other", {"store": True}), "engineering")  # a record of a stage directory comes first
        self.assertEqual(route("/tmp/other", {"store": False}), "other")  # a record of the work root: not a session without one
        self.assertEqual(route("/elsewhere"), "other")

    def test_readout_with_stage_dirs_prints_no_path_and_is_repeatable(self):
        dirs = self.store()
        text = self.run_readout("--detail", stage_dirs=dirs)
        self.assertEqual(text, self.run_readout("--detail", stage_dirs=dirs))
        for secret in (str(self.tmp), "/var/lib/x", "prompts"):
            self.assertNotIn(secret, text)

    # rev-kinds: blocking findings of page and plan reviews, counted by kind (the subtree blocking/ is passed by this test alone).
    def test_blocking_by_kind(self):
        frames = HERE.parent / "crates" / "ap" / "src" / "unit" / "frames"
        for kind, frame in (("page", "reviewer.md"), ("plan", "plan-reviewer.md")):
            text = (frames / frame).read_text()
            for name in readout.BLOCKING_KINDS[kind]:
                self.assertIn(f"blocking {name}:", text, f"{frame} names {name}")
        self.assertEqual(readout.BLOCKING_KINDS["page"], ("unbuildable", "contradiction", "check", "write_set", "unguarded_effect"))
        self.assertEqual(readout.BLOCKING_KINDS["plan"], ("unreached_criterion", "dependency", "conflict", "unguarded_effect"))
        dirs = [self.tmp / "blocking" / "reviews"]
        text = self.run_readout(stage_dirs=dirs)
        blocking = [l for l in text.splitlines() if l.startswith("BLOCKING")]
        self.assertEqual(blocking, [
            # page1, page2 and page3 (its answer file is missing: a review, nothing else); page4 starts after the window; the packet is no page
            "BLOCKING page reviews=3 unbuildable=1 contradiction=1 check=1 write_set=2 unguarded_effect=1 unnamed=2",
            # plan1 and plan2 (no answer file); plan3 starts after the window
            "BLOCKING plan reviews=2 unreached_criterion=1 dependency=1 conflict=1 unguarded_effect=1 unnamed=1",
        ])
        later = self.run_readout("--since", "2026-11-14", "--until", "2026-11-20", stage_dirs=dirs)
        self.assertEqual([l for l in later.splitlines() if l.startswith("BLOCKING")], [
            "BLOCKING page reviews=1 unbuildable=0 contradiction=0 check=1 write_set=0 unguarded_effect=0 unnamed=0",
            "BLOCKING plan reviews=1 unreached_criterion=0 dependency=1 conflict=0 unguarded_effect=0 unnamed=0"])
        self.assertEqual(blocking, [l for l in self.run_readout(stage_dirs=dirs).splitlines() if l.startswith("BLOCKING")])
        for secret in ("SECRET", str(self.tmp), "mid-line"):  # counts only: no finding's text
            self.assertNotIn(secret, text + later)
        self.assertFalse([l for l in self.run_readout().splitlines() if l.startswith("BLOCKING")])  # no review in the window, no line



class WithoutAcademy(unittest.TestCase):
    """--without-academy (specs/ap/measure.md R3), on records of this test's own directory: no fixture changes, and no
    directory whose name begins with edu- is made (that rule is tested on the function that decides a name)."""

    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="readout-academy-test-"))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        shutil.copy(FIXTURES / "prices.tsv", self.tmp / "prices.tsv")
        for d in ("work/task", "platforms", "stages", "codex", "acct/projects", "acct/homes"):
            (self.tmp / d).mkdir(parents=True)
        readout.run_gh = lambda repo, since, until: []

    def session(self, rel, sid, n):
        path = self.tmp / "acct" / rel / f"{sid}.jsonl"
        path.parent.mkdir(parents=True, exist_ok=True)
        line = {"type": "assistant", "sessionId": sid, "timestamp": "2026-11-12T10:00:00Z", "cwd": str(self.tmp / "work" / "task"),
                "message": {"id": "m-" + sid, "model": "claude-opus-9-9", "usage": {"input_tokens": n * 1000000}}}
        path.write_text(json.dumps(line) + "\n")

    def run_readout(self, *extra):
        out = io.StringIO()
        args = ["--since", "2026-11-11", "--until", "2026-11-13", "--prices", str(self.tmp / "prices.tsv"),
                "--accounts", str(self.tmp / "acct"), "--codex-dir", str(self.tmp / "codex"), "--work-root", str(self.tmp / "work"),
                "--platforms-root", str(self.tmp / "platforms"), "--stage-dir", str(self.tmp / "stages"), *extra]
        with contextlib.redirect_stdout(out):
            readout.main(args)
        return out.getvalue()

    def engineering_msgs(self, text):
        return int(re.search(r"^week \S+ route engineering TOTAL msgs=(\d+) ", text, re.M).group(1))

    def test_without_academy_the_function_decides_a_name(self):
        self.assertTrue(readout.academy_name("edu-lessons"))
        self.assertTrue(readout.academy_name("edu-lessons", project=True))
        self.assertTrue(readout.academy_name("-home-palantirkc-work-edu-lessons", project=True))
        self.assertTrue(readout.academy_name("-home-palantirkc-platforms-education-platform", project=True))
        self.assertFalse(readout.academy_name("-home-palantirkc-work-edu-lessons"))  # the substring rule is a project directory's
        self.assertFalse(readout.academy_name("-home-palantirkc-work-task", project=True))
        self.assertFalse(readout.academy_name("reduce-x"))

    def test_without_academy_a_project_directory_is_not_read(self):
        self.session("projects/-scratch-work-task", "s-kept", 1)
        self.session("projects/-scratch-work-edu-x", "s-academy", 2)
        self.assertEqual(self.engineering_msgs(self.run_readout()), 2)
        text = self.run_readout("--without-academy")
        self.assertEqual(self.engineering_msgs(text), 1)
        self.assertIn("input claude files=1 ", text)

    def test_without_academy_a_homes_project_directory_is_not_read(self):
        self.session("homes/h1/projects/-scratch-work-task", "s-home", 1)
        self.session("homes/h2/projects/-scratch-work-edu-x", "s-home-academy", 2)
        self.session("homes/h2/projects/-scratch-work-edu-x/sub", "s-home-deeper", 3)
        self.assertEqual(self.engineering_msgs(self.run_readout()), 3)
        self.assertEqual(self.engineering_msgs(self.run_readout("--without-academy")), 1)

    def test_without_academy_the_output_is_unchanged_where_nothing_is_the_academys(self):
        self.session("projects/-scratch-work-task", "s-a", 1)
        self.session("homes/h1/projects/-scratch-work-task", "s-b", 2)
        stage = {"role": "implementer", "model": "claude-opus-9-9", "session": "s-b", "start": "2026-11-12T09:00:00Z", "state": "ok"}
        (self.tmp / "stages" / "u1").mkdir()
        (self.tmp / "stages" / "u1" / "1.implementer.md.run.json").write_text(json.dumps(stage))
        (self.tmp / "work" / "task" / "x.md.run.json").write_text(json.dumps(dict(stage, session="s-a", role="writer")))
        (self.tmp / "work" / "top.run.json").write_text(json.dumps(dict(stage, session="s-a", role="fixer")))  # directly in the work root: not read
        text = self.run_readout()
        self.assertEqual(text, self.run_readout("--without-academy"))
        self.assertIn("role=implementer family=opus msgs=1 ", text)
        self.assertIn("role=writer family=opus msgs=1 ", text)
        self.assertNotIn("role=fixer", text)


if __name__ == "__main__":
    unittest.main()
