"""Tests of the stage lines of `evals/readout.py --detail` on invented stage streams and a fake `ap`.

Held: the stage lines and the unit line of a pull request of a head branch `unit/<key>`; the implementer
named by the run record of the unit's first implementer stage; a unit whose `ap status` fails or answers no JSON
prints its stream stages only with `ap_status_failed=1`; one `ap status` read per unit and none without `--detail`;
the closing `lead share=` line; nothing is written and no message text, tool input or path is printed.
Run: python3 -I -B -m unittest discover -s ci -p 'test_readout_stages.py'
"""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
import re
import shutil
import stat
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("readout", HERE.parent / "evals" / "readout.py")
readout = importlib.util.module_from_spec(spec)
spec.loader.exec_module(readout)

REPO_A, REPO_O = "park-kyungchan/agent-platform", "park-kyungchan/ontology-platform"
PRS = {REPO_A: [{"number": 21, "headRefName": "unit/alpha", "mergedAt": "2026-11-12T10:00:00Z", "additions": 5, "deletions": 1},
                {"number": 22, "headRefName": "feat/plain", "mergedAt": "2026-11-12T11:00:00Z", "additions": 2, "deletions": 0},
                {"number": 23, "headRefName": "unit/beta", "mergedAt": "2026-11-12T12:00:00Z", "additions": 3, "deletions": 0},
                {"number": 24, "headRefName": "unit/delta", "mergedAt": "2026-11-12T13:00:00Z", "additions": 1, "deletions": 0},
                {"number": 25, "headRefName": "unit/../escape", "mergedAt": "2026-11-12T14:00:00Z", "additions": 1, "deletions": 0}],
       REPO_O: [{"number": 5, "headRefName": "unit/gamma", "mergedAt": "2026-11-12T15:00:00Z", "additions": 4, "deletions": 4}]}
KEY = {c: f"act-rq-{c * 32}" for c in "1234"}  # 1 page review, 2 verifier, 3 named without a run record, 4 not named
SECRET = "SECRET-MESSAGE-TEXT"


def line(**o):
    return json.dumps(o) + "\n"


def usage(**u):
    return {"id": "m" + str(len(u)) + str(sorted(u.items())), "model": "claude-sonnet-9", "role": "assistant", "usage": u,
            "content": [{"type": "text", "text": SECRET}, {"type": "tool_use", "name": "Bash", "input": {"command": SECRET}}]}


def tree_digest(root):
    return sorted((str(p.relative_to(root)), hashlib.sha256(p.read_bytes()).hexdigest()) for p in Path(root).rglob("*") if p.is_file())


class Stages(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="readout-stages-test-"))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        t = self.tmp
        for d in ("work/lead", "platforms", "acct/projects/p", "units/alpha", "units/beta", "units/delta", "reviews", "bin", "apdir"):
            (t / d).mkdir(parents=True)
        shutil.copy(HERE / "fixtures" / "readout" / "prices.tsv", t / "prices.tsv")
        # the Lead's session: 2,000,000 input tokens at 1 USD per million = 2.00 USD
        (t / "acct/projects/p/lead.jsonl").write_text(line(
            type="assistant", sessionId="s-lead", timestamp="2026-11-12T09:00:00Z", cwd=str(t / "work" / "lead"),
            message={"id": "lead-1", "model": "claude-opus-9-9", "usage": {"input_tokens": 2000000}, "content": []}))
        u = t / "units"
        (u / "alpha/1.implementer.md.stream.jsonl").write_text(
            line(type="system", subtype="init", model="claude-sonnet-9", session_id="x", cwd="/nowhere/" + SECRET)
            + line(type="assistant", message=usage(input_tokens=100, cache_read_input_tokens=1000,
                                                    cache_creation={"ephemeral_5m_input_tokens": 200, "ephemeral_1h_input_tokens": 50}))
            + "not json\n" + line(type="result", total_cost_usd=1.25, num_turns=10)
            + line(type="system", subtype="compact_boundary")
            + line(type="assistant", message=usage(input_tokens=10, cache_read_input_tokens=5000, cache_creation_input_tokens=40, output_tokens=9))
            + line(type="result", total_cost_usd=0.5, num_turns=3))
        (u / "alpha/1.implementer.md.run.json").write_text(line(session="sess-impl", role="implementer", model="sonnet", cwd=str(t / "x")))
        (u / "alpha/3.fixer.md.stream.jsonl").write_text(
            line(type="system", subtype="init", model="claude-opus-9-9")
            + line(type="assistant", message=usage(input_tokens=300)) + line(type="result", total_cost_usd=0.75, num_turns=4))
        (u / "alpha/3.fixer.md.run.json").write_text(line(session="sess-fix", role="fixer", model="opus"))
        (u / "alpha/2.verifier.prompt.md").write_text(SECRET)
        (u / "beta/1.implementer.md.stream.jsonl").write_text(
            line(type="system", subtype="init", model="claude-haiku-9-9") + line(type="result", total_cost_usd=0.1, num_turns=2))
        (u / "delta/10.implementer.md.stream.jsonl").write_text(line(type="system", subtype="init", model="bad model\nname") + line(type="result", total_cost_usd=0.2, num_turns=1))
        (u / "delta/2.implementer.md.stream.jsonl").write_text(line(type="result", total_cost_usd=0.3, num_turns=1))
        r = t / "reviews"
        for c, cwd, cost, turns in (("1", "/var/lib/x/reviews/work/rev", 1.0, 20), ("2", "/var/lib/x/units/alpha/wt", 0.5, 13), ("4", "/var/lib/x/reviews/work/z", 9.0, 9)):
            (r / f"{KEY[c]}.md.stream.jsonl").write_text(line(type="system", subtype="init", model="claude-opus-9-9")
                                                          + line(type="result", total_cost_usd=cost, num_turns=turns))
            (r / f"{KEY[c]}.md.run.json").write_text(line(session="sess-" + c, role="reviewer", model="opus", cwd=cwd))
        (r / f"{KEY['3']}.md.stream.jsonl").write_text(line(type="result", total_cost_usd=7.0, num_turns=7))  # no run record
        ap = t / "bin" / "ap"
        ap.write_text(f"#!{sys.executable}\nimport os, sys\nd = {str(t / 'apdir')!r}\n"
                      "key = sys.argv[2] if sys.argv[1:2] == ['status'] and len(sys.argv) == 3 else sys.exit(3)\n"
                      "open(os.path.join(d, 'calls.log'), 'a').write(key + '\\n')\n"
                      "p = os.path.join(d, key + '.json')\n"
                      "if not os.path.exists(p):\n    sys.stderr.write('unknown_key')\n    sys.exit(1)\n"
                      "sys.stdout.write(open(p).read())\n")
        ap.chmod(ap.stat().st_mode | stat.S_IXUSR)
        reviews = [{"review": KEY["1"], "kind": "page"}, {"review": KEY["2"], "kind": "packet"}, {"review": KEY["3"], "kind": "page"},
                   {"review": "act-rq-short", "kind": "page"}, {"kind": "page"}]
        (t / "apdir/alpha.json").write_text(json.dumps({"unit": "alpha", "reviews": reviews}))
        (t / "apdir/gamma.json").write_text(json.dumps({"unit": "gamma", "reviews": []}))
        (t / "apdir/delta.json").write_text("this is not json")  # beta has no answer: exit 1
        old = readout.run_gh
        readout.run_gh = lambda repo, since, until: PRS.get(repo, [])
        self.addCleanup(setattr, readout, "run_gh", old)
        self.addCleanup(os.environ.__setitem__, "PATH", os.environ["PATH"])
        os.environ["PATH"] = f"{t / 'bin'}{os.pathsep}{os.environ['PATH']}"

    def run_readout(self, *extra, stage_dirs=None):
        t = self.tmp
        dirs = stage_dirs if stage_dirs is not None else [t / "units", t / "reviews"]
        args = ["--since", "2026-11-11", "--until", "2026-11-13", "--prices", str(t / "prices.tsv"), "--accounts", str(t / "acct"),
                "--codex-dir", str(t / "codex"), "--work-root", str(t / "work"), "--platforms-root", str(t / "platforms"),
                "--lead-cwd", str(t / "work" / "lead"), *[x for d in dirs for x in ("--stage-dir", str(d))],
                "--repo", REPO_A, "--repo", REPO_O, *extra]
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            readout.main(args)
        return out.getvalue()

    def calls(self):
        p = self.tmp / "apdir" / "calls.log"
        return sorted(p.read_text().split()) if p.exists() else []

    def test_stage_lines_and_unit_line_of_a_unit_branch(self):
        text = self.run_readout("--detail")
        k2 = "K2 agent-platform#21 implementer=sonnet verifier=none first verdict=none lines=6"
        want = [k2,
                "stage alpha implementer 1.implementer model=claude-sonnet-9 usd=1.75 turns=13 peak_ctx=5050 compactions=1",
                "stage alpha fixer 3.fixer model=claude-opus-9-9 usd=0.75 turns=4 peak_ctx=300 compactions=0",
                f"stage alpha page review {KEY['1']} model=claude-opus-9-9 usd=1.00 turns=20 peak_ctx=0 compactions=0",
                f"stage alpha verifier {KEY['2']} model=claude-opus-9-9 usd=0.50 turns=13 peak_ctx=0 compactions=0",
                f"unit alpha {REPO_A}#21 stages=4 usd=4.00"]
        lines = text.split("\n")
        i = lines.index(k2)
        self.assertEqual(lines[i:i + len(want)], want)
        self.assertNotIn(KEY["3"], text)  # named by ap, but no run record
        self.assertNotIn(KEY["4"], text)  # a run record, but ap does not name it for the unit

    def test_peak_context_counts_both_forms_of_cache_creation(self):
        s = readout.stage_of(self.tmp / "units/alpha/1.implementer.md.stream.jsonl")
        self.assertEqual((s["peak_ctx"], s["turns"], s["compactions"], s["model"]), (5050, 13, 1, "claude-sonnet-9"))
        self.assertAlmostEqual(s["usd"], 1.75)

    def test_implementer_is_the_model_of_the_first_implementer_run_record(self):
        text = self.run_readout("--detail")
        self.assertIn("K2 implementer=sonnet merged=1 lines=6: none n=1 lines=6; verifier tiers: none", text)
        self.assertIn("K2 agent-platform#22 implementer=unknown verifier=none first verdict=none lines=2", text)  # not a unit branch
        self.assertIn("K2 agent-platform#23 implementer=unknown ", text)  # no run record: as before
        self.assertIn("K2 implementer=unknown merged=5 lines=15: none n=5 lines=15; verifier tiers: none", text)

    def test_a_unit_whose_ap_status_fails_or_is_no_json_prints_its_streams_only(self):
        text = self.run_readout("--detail")
        self.assertIn("stage beta implementer 1.implementer model=claude-haiku-9-9 usd=0.10 turns=2 peak_ctx=0 compactions=0\n"
                      f"unit beta {REPO_A}#23 stages=1 usd=0.10 ap_status_failed=1\n", text)
        # natural order of stage names (2 before 10); a model name that is no plain token is `unknown`
        self.assertIn("stage delta implementer 2.implementer model=unknown usd=0.30 turns=1 peak_ctx=0 compactions=0\n"
                      "stage delta implementer 10.implementer model=unknown usd=0.20 turns=1 peak_ctx=0 compactions=0\n"
                      f"unit delta {REPO_A}#24 stages=2 usd=0.50 ap_status_failed=1\n", text)
        self.assertIn(f"unit gamma {REPO_O}#5 stages=0 usd=0.00\n", text)  # ap answered, nothing to list
        self.assertNotIn("escape", text)
        self.assertNotIn("stage plain", text)

    def test_one_ap_read_per_unit_and_none_without_detail(self):
        self.run_readout("--detail")
        self.assertEqual(self.calls(), ["alpha", "beta", "delta", "gamma"])
        (self.tmp / "apdir" / "calls.log").unlink()
        text = self.run_readout()
        self.assertEqual(self.calls(), [])
        for word in ("stage ", "unit ", "lead share="):
            self.assertFalse(any(l.startswith(word) for l in text.split("\n")), word)
        self.assertIn("K2 implementer=unknown merged=6", text)

    def test_lead_share_closes_the_detail_output(self):
        text = self.run_readout("--detail")
        last = text.rstrip("\n").split("\n")[-1]
        m = re.fullmatch(r"lead share=([\d.]+)% lead_usd=([\d.]+) stages_usd=([\d.]+)", last)
        self.assertIsNotNone(m, last)
        # stages: alpha 4.00 + beta 0.10 + delta 0.50 + gamma 0.00
        self.assertEqual((m.group(2), m.group(3)), ("2.00", "4.60"))
        self.assertEqual(m.group(1), f"{100 * 2.0 / 6.6:.1f}")

    def test_lead_share_without_cost_is_not_a_division(self):
        for d in ("units", "reviews"):
            shutil.rmtree(self.tmp / d)
            (self.tmp / d).mkdir()
        shutil.rmtree(self.tmp / "acct")
        (self.tmp / "acct" / "projects").mkdir(parents=True)
        last = self.run_readout("--detail").rstrip("\n").split("\n")[-1]
        self.assertEqual(last, "lead share=n/a lead_usd=0.00 stages_usd=0.00")

    def test_writes_nothing_and_prints_no_text_or_path(self):
        before = tree_digest(self.tmp)
        text = self.run_readout("--detail")
        after = [x for x in tree_digest(self.tmp) if not x[0].startswith("apdir/")]
        self.assertEqual(after, [x for x in before if not x[0].startswith("apdir/")])
        self.assertNotIn(SECRET, text)
        self.assertNotIn(str(self.tmp), text)
        self.assertNotIn("/var/lib/x", text)

    def test_unit_key_of_a_head_branch(self):
        for branch in ("unit/a", "unit/a-b_c.1"):
            self.assertEqual(readout.unit_key(branch), branch[5:])
        for branch in ("unit/", "unit/../x", "unit/a/b", "unit/-x", "unit/a b", "feat/a", "xunit/a", None, 7):
            self.assertIsNone(readout.unit_key(branch), branch)


if __name__ == "__main__":
    unittest.main()
