"""Tests of evals/readout.py on invented records (ci/fixtures/readout/, synthetic values only).

Each invariant of the read-out has a test that fails on the wrong implementation:
I1 a message id is counted once, usage.iterations is never added; I2 a stage record's tokens are
never added to the session's; I3 every input line lands in one output line; I4 only the allowed
members are read and printed, nothing is written; I5 the same records give the same output.
Run: python3 -I -B -m unittest discover -s ci -p 'test_readout*.py'
"""
import contextlib
import hashlib
import importlib.util
import io
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

PRS = {"park-kyungchan/agent-platform": [{"number": 7, "headRefName": "feat/a", "mergedAt": "2026-11-12T10:00:00Z"},
                                         {"number": 8, "headRefName": "feat/none", "mergedAt": "2026-11-13T10:00:00Z"},
                                         {"number": 9, "headRefName": "feat/old", "mergedAt": "2026-10-01T10:00:00Z"}]}


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
                dst.write_text(src.read_text().replace("@WORK@", str(self.tmp / "work")))
        readout.run_gh = lambda repo, since, until: PRS.get(repo, [])

    def run_readout(self, *extra):
        out = io.StringIO()
        args = ["--since", "2026-11-11", "--until", "2026-11-13", "--prices", str(self.tmp / "prices.tsv"),
                "--accounts", str(self.tmp / "acct1"), str(self.tmp / "acct2"), str(self.tmp / "acct1"),
                "--codex-dir", str(self.tmp / "codex"), "--work-root", str(self.tmp / "work"),
                "--platforms-root", str(self.tmp / "platforms"), "--lead-cwd", str(self.tmp / "work" / "lead"),
                "--repo", "park-kyungchan/agent-platform", *extra]
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
        self.assertEqual(out, 2300000)  # m1 500,000 once; m5 1,000,000; m6 500,000; codex 300,000
        self.assertEqual(cr, 3000000)  # m1 2,000,000 once; codex 1,000,000
        self.assertIn("messages=11 ", text)
        self.assertIn("repeat=3 ", text)

    def test_i2_stage_tokens_are_not_added_and_roles_follow_the_stage_in_time(self):
        text = self.run_readout()
        self.assertEqual(self.route(text, "engineering")[6], 29.10)  # sessions only: every stage record says 7,000,000 of each kind
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
        self.assertRegex(text, r"input claude files=6 messages=11 other=1 read=16 repeat=3 unreadable=1 outside_window=1")
        n = {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", re.search(r"^input claude .*$", text, re.M).group(0))}
        self.assertEqual(n["read"], n["messages"] + n["other"] + n["repeat"] + n["unreadable"])
        self.assertRegex(text, r"role=implementer family=unknown msgs=1 input=100 ")  # an unknown model keeps its tokens
        self.assertRegex(text, r"input stages records=6 unreadable=1 without_session_record=3")
        self.assertRegex(text, r"input codex events=2 files=1 other=2 .*read=5 repeat=1")
        for route in ("engineering", "lesson", "other"):
            block = re.search(rf"^week \S+ route {route} TOTAL.*?(?=^week|^K2|\Z)", text, re.M | re.S).group(0).splitlines()
            total, rows = block[0], block[1:]
            for kind in ("msgs", "input", "output", "cache_read", "cache_write_5m", "cache_write_1h", "usd"):
                pick = lambda s: float(re.search(rf"{kind}=([\d,.]+)", s).group(1).replace(",", ""))
                self.assertAlmostEqual(sum(pick(r) for r in rows), pick(total), places=2, msg=f"{route} {kind}")
        self.assertEqual([self.route(text, r)[0] for r in ("engineering", "lesson", "other")], [10, 1, 1])
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
        self.assertIn("merged pull requests 2 (agent-platform=2) K1 engineering usd per merged pull request = 14.55", text)
        self.assertIn("K2 agent-platform#7 implementer=opus reviewer=Verdict: accept", text)
        self.assertIn("K2 agent-platform#8 implementer=unknown reviewer=unknown", text)
        self.assertIn("K2 no reviewer stage agent-platform: 8", text)
        self.assertIn("stages cut: implementer/sonnet=1", text)

    def test_lead_calls_by_first_tool(self):
        text = self.run_readout()
        self.assertIn("lead calls first_tool=Bash calls=1 usd=15.00", text)
        self.assertIn("lead calls first_tool=none calls=2 usd=1.90", text)

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
