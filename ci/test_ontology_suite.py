"""Tests of evals/ontology/run, the Ontology Engineering suite, run as a program against a fake `systemd-run`, a fake
`systemctl`, a fake `claude` and the invented fixture corpus evals/ontology/fixtures/ (no model, no network, no real
unit, no text of the corpus). Each test cites the requirement of specs/evals/ontology.md it holds.
Run: python3 -I -B -m unittest discover -s ci -p 'test_ontology_suite.py'
"""
import hashlib
import importlib.machinery
import importlib.util
import inspect
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
RUN = REPO / "evals" / "ontology" / "run"
ROUTING = REPO / "evals" / "routing" / "run"
CASES = REPO / "evals" / "ontology" / "cases.json"
FIXTURES = REPO / "evals" / "ontology" / "fixtures"
INVENTORY = FIXTURES / "inventory.json"
SPEC = REPO / "specs" / "evals" / "ontology.md"
PY = sys.executable
ASK = ("Cite the passage of the corpus that decides this as path:a-b, the file's path and the first and last line "
       "of the passage.")
URL = "https://docs.example.invalid/foundry/object-types/"
OBJ = "palantir/docs/foundry/object-types.md"
ACT = "palantir/docs/foundry/action-types.md"

FAKE_SYSTEMD_RUN = r'''#!@PY@
import json, os, subprocess, sys
state = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
argv = sys.argv[1:]
with open(os.path.join(state, "systemd-run.jsonl"), "a") as f:
    f.write(json.dumps(argv) + "\n")
i = argv.index("--")
opts, cmd = argv[:i], argv[i + 1:]
env, cwd, k = {}, None, 0
while k < len(opts):
    o = opts[k]
    if o == "-p":
        k += 2
        continue
    if o.startswith("--setenv="):
        key, _, val = o[len("--setenv="):].partition("=")
        env[key] = val
    if o.startswith("--working-directory="):
        cwd = o.split("=", 1)[1]
    k += 1
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

# A session: found by its prompt (the last argument) in plan.json; reads the files of its plan from the corpus (one
# tool_use and its tool_result each), runs one command outside the corpus, and answers.
FAKE_CLAUDE = r'''#!@PY@
import json, os, sys
state = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
plan = json.load(open(os.path.join(state, "plan.json")))
args = sys.argv[1:]
with open(os.path.join(state, "cli.jsonl"), "a") as f:
    f.write(json.dumps({"argv": args}) + "\n")
beh = plan["cli"].get(args[-1], {})
if beh.get("exit"):
    sys.exit(1)
events = []
for i, (path, body) in enumerate(beh.get("reads", [])):
    tid = "read%d" % i
    events.append({"type": "assistant", "message": {"content": [
        {"type": "tool_use", "id": tid, "name": "Bash", "input": {"command": "cat %s/%s" % (plan["corpus"], path)}}]}})
    content = body if i % 2 == 0 else [{"type": "text", "text": body}]
    events.append({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": tid, "content": content}]}})
    events.append({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": tid, "content": content}]}})
events.append({"type": "assistant", "message": {"content": [
    {"type": "tool_use", "id": "other", "name": "Bash", "input": {"command": "ls /tmp"}}]}})
events.append({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "other", "content": "x" * 999}]}})
events.append({"type": "result", "result": beh.get("answer", ""), "total_cost_usd": 0.5,
               "usage": {"input_tokens": 10, "output_tokens": 7}})
for e in events:
    print(json.dumps(e), flush=True)
'''


def load(path, name):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    mod = importlib.util.module_from_spec(importlib.util.spec_from_loader(name, loader))
    loader.exec_module(mod)
    return mod


def sha(text):
    return hashlib.sha256(text.encode()).hexdigest()


def case(cid, kind="route", deciding=None, wrong=None, situation="An invented question?"):
    return {"id": cid, "kind": kind, "cell": "language.object-type", "situation": situation + "\n\n" + ASK,
            "deciding": deciding or [{"source": URL, "path": OBJ, "lines": [7, 9]}], "wrong": wrong or []}


class World:
    """A copy of the suite's tree in a scratch directory, with fakes on PATH and the fixture corpus."""

    def __init__(self, test, cases=None):
        self.tmp = tempfile.TemporaryDirectory()
        test.addCleanup(self.tmp.cleanup)
        t = self.t = Path(self.tmp.name)
        site = t / "site"
        (site / "evals" / "ontology").mkdir(parents=True)
        (site / "evals" / "routing").mkdir()
        shutil.copy2(RUN, site / "evals" / "ontology" / "run")
        shutil.copy2(ROUTING, site / "evals" / "routing" / "run")
        self.run_path = site / "evals" / "ontology" / "run"
        self.routing_path = site / "evals" / "routing" / "run"
        self.cases_path = site / "evals" / "ontology" / "cases.json"
        self.cases_path.write_text(json.dumps(cases if cases is not None else json.loads(CASES.read_text())))
        self.digests = site / "evals" / "ontology" / "host-cases.sha256"
        self.digests.write_text("")
        self.corpus = t / "corpus"
        shutil.copytree(FIXTURES, self.corpus)
        self.bin = t / "bin"
        self.bin.mkdir()
        for name, text in (("systemd-run", FAKE_SYSTEMD_RUN), ("systemctl", FAKE_SYSTEMCTL), ("claude", FAKE_CLAUDE)):
            (self.bin / name).write_text(text.replace("@PY@", PY))
            (self.bin / name).chmod(0o755)
        self.home = t / "home"
        self.home.mkdir()
        self.acct = t / "acct"
        self.acct.mkdir()
        self.tmpdir = t / "tmp"
        self.tmpdir.mkdir()
        self.out = t / "out"
        self.plan({})

    def plan(self, cli):
        (self.t / "plan.json").write_text(json.dumps({"corpus": str(self.corpus), "cli": cli}))

    def env(self):
        return {"PATH": str(self.bin), "HOME": str(self.home), "CLAUDE_CONFIG_DIR": str(self.acct),
                "TMPDIR": str(self.tmpdir)}

    def call(self, verb, *args):
        r = subprocess.run([PY, "-I", "-B", str(self.run_path), verb, *map(str, args)], env=self.env(),
                           capture_output=True, text=True, timeout=300)
        return r.returncode, r.stdout, r

    def json_call(self, verb, *args):
        code, out, r = self.call(verb, *args)
        return code, (json.loads(out) if out.strip() else None), r

    def answers(self, *args, reader="claude-opus", cases=None):
        return self.json_call("answers", "--reader", reader, "--out", self.out, "--cases", cases or self.cases_path,
                              "--corpus", self.corpus, *args)

    def score(self, *args, cases=None, corpus=None):
        return self.json_call("score", "--out", self.out, "--cases", cases or self.cases_path,
                              "--corpus", corpus or self.corpus, *args)

    def cli_calls(self):
        p = self.t / "cli.jsonl"
        return [json.loads(x) for x in p.read_text().splitlines()] if p.exists() else []

    def sample(self, cid, n=1, answer="", stream="", samples=1, cases=None, corpus=None, reader="claude-opus"):
        """A sample written by hand, as `answers` leaves it, and a run.json that holds it."""
        d = self.out / reader
        d.mkdir(parents=True, exist_ok=True)
        run = d / "run.json"
        rec = json.loads(run.read_text()) if run.exists() else {
            "samples": samples, "cases": [], "suite": "s", "corpus": str(corpus or self.corpus),
            "case_files": {"cases.json": hashlib.sha256((cases or self.cases_path).read_bytes()).hexdigest()}}
        rec["cases"] = sorted(set(rec["cases"]) | {cid})
        run.write_text(json.dumps(rec))
        base = d / ("%s__s%d" % (cid, n))
        if answer:
            base.with_suffix(".md").write_text(answer)
            (d / ("%s__s%d.jsonl" % (cid, n))).write_text(stream)
            (d / ("%s__s%d.json" % (cid, n))).write_text(json.dumps(
                {"answered": True, "input_tokens": 100, "fn_calls": {"route": 0, "brief": 0}, "usd": 0.25}))


class Answers(unittest.TestCase):
    """One run of the synthetic cases through the fakes, then its score."""

    def setUp(self):
        self.w = World(self)
        self.cases = json.loads(CASES.read_text())
        self.by_id = {c["id"]: c for c in self.cases}

    def test_answers_starts_claude_opus_with_the_situation_alone(self):
        # spec: specs/evals/ontology.md#R1
        code, out, r = self.w.answers("--samples", "1", "--case", "route-3d1b053aeadc")
        self.assertEqual((code, r.stderr), (0, ""), r.stdout)
        self.assertEqual(out["samples"], {"route-3d1b053aeadc__s1": "failed"})  # the fake answered nothing
        (call,) = self.w.cli_calls()
        argv = call["argv"]
        self.assertEqual(argv[-1], self.by_id["route-3d1b053aeadc"]["situation"])
        self.assertEqual(argv[argv.index("--model") + 1], "opus")
        self.assertEqual(argv[argv.index("--add-dir") + 1], "/root/.agents")
        self.assertNotIn(str(self.w.corpus), " ".join(argv))  # the reader finds the corpus through the instructions

    def test_answers_refuses_every_other_reader(self):
        # spec: specs/evals/ontology.md#R1
        for reader in ("claude-sonnet", "claude-haiku", "agy-hydrogen", "fable"):
            code, out, _ = self.w.answers(reader=reader)
            self.assertEqual((code, out), (2, {"status": "usage"}), reader)
        self.assertEqual(self.w.answers("--samples", "0")[:2], (2, {"status": "usage"}))
        self.assertFalse(self.w.cli_calls())

    def test_run_json_names_the_suite_by_both_files_and_the_corpus(self):
        # spec: specs/evals/ontology.md#R1
        self.assertEqual(self.w.answers("--samples", "1", "--case", "route-3d1b053aeadc")[0], 0)
        rec = json.loads((self.w.out / "claude-opus" / "run.json").read_text())
        digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
        self.assertEqual(rec["suite"], sha(digest(self.w.run_path) + digest(self.w.routing_path)))
        self.assertEqual(rec["corpus"], str(self.w.corpus))
        self.assertEqual(rec["case_files"], {"cases.json": digest(self.w.cases_path)})
        self.assertEqual(rec["cases"], ["route-3d1b053aeadc"])

    def test_a_changed_routing_file_or_corpus_refuses_run_differs(self):
        # spec: specs/evals/ontology.md#R1
        self.assertEqual(self.w.answers("--samples", "1", "--case", "route-3d1b053aeadc")[0], 0)
        other = self.w.t / "other-corpus"
        other.mkdir()
        code, out, _ = self.w.json_call("answers", "--reader", "claude-opus", "--out", self.w.out, "--cases",
                                        self.w.cases_path, "--corpus", other, "--samples", "1", "--case",
                                        "route-3d1b053aeadc")
        self.assertEqual((code, out), (2, {"status": "run_differs"}))
        with open(self.w.routing_path, "a") as f:
            f.write("\n# changed\n")
        code, out, _ = self.w.answers("--samples", "1", "--case", "route-3d1b053aeadc")
        self.assertEqual((code, out), (2, {"status": "run_differs"}))

    def test_a_run_answers_and_scores_with_the_corpus_bytes(self):
        # spec: specs/evals/ontology.md#R1
        # spec: specs/evals/ontology.md#R4
        good = {c["id"]: "%s:%d-%d" % (c["deciding"][0]["path"], *c["deciding"][0]["lines"]) for c in self.cases}
        cli = {c["situation"]: {"answer": "The passage: " + good[c["id"]], "reads": [[OBJ, "abc"], [ACT, "deé"]]}
               for c in self.cases}
        cli[self.by_id["trend-6e758ec68046"]["situation"]] = {"answer": "No citation."}
        self.w.plan(cli)
        code, out, r = self.w.answers("--samples", "2")
        self.assertEqual((code, r.stderr), (0, ""), r.stdout)
        self.assertEqual(set(out["samples"].values()), {"ok"})
        code, score, r = self.w.score()
        self.assertEqual(code, 0, r.stdout)
        x = score["readers"]["claude-opus"]
        self.assertEqual(x["n"], 2)
        self.assertEqual(x["passes"]["trend-6e758ec68046"], 0)
        self.assertEqual(sum(x["passes"].values()), 2 * (len(self.cases) - 1))
        self.assertEqual(x["by_kind"]["trend"], {"passes": 2, "samples": 4})
        self.assertEqual(x["by_kind"]["route"], {"passes": 10, "samples": 10})
        # 3 bytes, and 4 for "de" and a two-byte letter: each result once; the command outside the corpus not.
        self.assertEqual(x["corpus_bytes_median"], 7)  # 14 samples read 7 bytes, the trend case's two nothing
        self.assertEqual(x["usd"], round(0.5 * 2 * len(self.cases), 2))
        self.assertEqual(x["fn_samples"], 0.0)


class Readers(unittest.TestCase):
    def test_the_reader_row_is_the_routing_builders_and_the_routing_file_has_none(self):
        # spec: specs/evals/ontology.md#R2
        routing = load(ROUTING, "routing_fresh")
        self.assertNotIn("claude-opus", routing.READERS)
        mod = load(RUN, "ontology_run")
        row = mod.R.READERS["claude-opus"]
        self.assertEqual(row["argv"]("P", "/root/.agents", "w", "sid"), routing.claude_argv("opus")("P", "/root/.agents", "w", "sid"))
        self.assertEqual((row["cli"], row["env"]), ("claude", ("CLAUDE_CONFIG_DIR",)))
        self.assertEqual(mod.R.PROMPT, "{situation}")
        self.assertEqual(mod.READER_NAMES, ("claude-opus",))
        self.assertNotIn("def run_sample", RUN.read_text())  # loaded, not copied
        self.assertNotIn("def cmd_gate", RUN.read_text())


class Cases(unittest.TestCase):
    def setUp(self):
        self.w = World(self)

    def test_a_case_file_is_validated(self):
        # spec: specs/evals/ontology.md#R3
        good = case("route-aaaaaaaaaaaa")
        bad = [dict(good, kind="other"), dict(good, situation="no fixed sentence"), dict(good, deciding=[]),
               dict(good, deciding=[{"source": URL, "path": OBJ, "lines": [9, 7]}]), dict(good, wrong=[{"fact": []}]),
               dict(good, id="a/b"), {k: v for k, v in good.items() if k != "cell"}]
        for c in bad:
            self.w.cases_path.write_text(json.dumps([c]))
            self.assertEqual(self.w.json_call("reach", "--cases", self.w.cases_path, "--tables", INVENTORY,
                                              "--instructions", self.w.t)[1], {"status": "cases_invalid"}, c)
        self.w.cases_path.write_text(json.dumps([good, good]))
        self.assertEqual(self.w.score()[1], {"status": "no_reader"})
        self.w.sample(good["id"])
        self.assertEqual(self.w.score()[1], {"status": "case_id_twice"})

    def test_a_host_file_is_admitted_by_its_digest_only(self):
        # spec: specs/evals/ontology.md#R3
        host = self.w.t / "host.json"
        host.write_text(json.dumps([case("route-bbbbbbbbbbbb")]))
        args = lambda f: ("reach", "--cases", f, "--tables", self.w.corpus / "ROUTES.md", "--instructions", self.w.t)
        (self.w.t / "AGENTS.md").write_text("x")
        (self.w.t / "runtime-references").mkdir()
        (self.w.t / "runtime-references" / "OFFICIAL-DOCS.md").write_text("ROUTES.md")
        self.assertEqual(self.w.json_call(*args(host))[1], {"status": "cases_digest_unknown"})
        copy = self.w.t / "copy.json"
        shutil.copy2(self.w.cases_path, copy)  # the repository's file is admitted at its own path only
        self.assertEqual(self.w.json_call(*args(copy))[1], {"status": "cases_digest_unknown"})
        self.assertEqual(self.w.json_call(*args(self.w.t / "none.json"))[1], {"status": "cases_unreadable"})
        self.w.digests.write_text("%s  host-ontology-1\n" % hashlib.sha256(host.read_bytes()).hexdigest())
        code, out, _ = self.w.call(*args(host))
        self.assertEqual(code, 0)
        self.assertEqual(json.loads(out.splitlines()[0])["case"], "route-bbbbbbbbbbbb")
        # a run names its one file: a score with the other file refuses
        self.w.sample("route-bbbbbbbbbbbb", answer="x", cases=host)
        rec = json.loads((self.w.out / "claude-opus" / "run.json").read_text())
        self.assertEqual(self.w.score()[1], {"status": "case_sets_differ"})
        rec["case_files"] = {"host-ontology-1": rec["case_files"]["cases.json"]}
        (self.w.out / "claude-opus" / "run.json").write_text(json.dumps(rec))
        self.assertEqual(self.w.score(cases=host)[0], 0)

    def test_cases_json_holds_the_cut_of_the_fixture_inventory(self):
        # spec: specs/evals/ontology.md#R3
        # spec: specs/evals/ontology.md#R6
        code, out, r = self.w.call("cases", "--inventory", INVENTORY, "--out", "/dev/stdout")
        self.assertEqual((code, r.stderr), (0, ""))
        cut = json.loads(out)
        repo = json.loads(CASES.read_text())
        self.assertEqual([c for c in repo if c["kind"] != "design"], cut)
        self.assertTrue(any(c["kind"] == "design" for c in repo))
        for c in cut:
            self.assertEqual(set(c), {"id", "kind", "cell", "situation", "deciding", "wrong"})
            self.assertTrue(c["situation"].endswith("\n\n" + ASK))
            self.assertEqual(c["wrong"], [])


class Score(unittest.TestCase):
    def setUp(self):
        self.w = World(self, [case("route-aaaaaaaaaaaa"),
                              case("design-bbbbbbbbbbbb", kind="design", deciding=[
                                  {"source": ACT, "path": ACT, "lines": [3, 6]}, {"source": URL, "path": OBJ, "lines": [19, 21]}],
                                  wrong=[{"fact": ["the key", "changes"]}])])

    def passes(self, cid, answer, *args):
        shutil.rmtree(self.w.out, ignore_errors=True)
        self.w.sample(cid, answer=answer)
        code, score, r = self.w.score(*args)
        self.assertEqual(code, 0, r.stdout)
        return score["readers"]["claude-opus"]["passes"][cid] == 1

    def test_a_citation_passes_when_its_lines_overlap_the_deciding_lines(self):
        # spec: specs/evals/ontology.md#R4
        a = "route-aaaaaaaaaaaa"
        self.assertTrue(self.passes(a, "See `palantir/docs/foundry/object-types.md:8-12`."))
        self.assertTrue(self.passes(a, "See palantir/docs/foundry/object-types.md:9"))
        self.assertTrue(self.passes(a, "(/srv/corpus/palantir/docs/foundry/object-types.md:1-7)"))
        self.assertFalse(self.passes(a, "| docs/foundry/object-types.md:9-9 |"))  # shorter than the path
        self.assertFalse(self.passes(a, "palantir/docs/foundry/object-types.md:10-12"))
        self.assertFalse(self.passes(a, "palantir/docs/foundry/xobject-types.md:7-9"))
        self.assertFalse(self.passes(a, "palantir/docs/foundry/object-types.md lines 7-9"))
        self.assertTrue(self.passes(a, "%s:7-9" % URL))  # the source itself

    def test_a_wrong_fact_fails_a_sample_and_any_deciding_entry_passes(self):
        # spec: specs/evals/ontology.md#R4
        d = "design-bbbbbbbbbbbb"
        self.assertTrue(self.passes(d, "%s:20" % OBJ))
        self.assertTrue(self.passes(d, "%s:4-5" % ACT))
        self.assertFalse(self.passes(d, "%s:4-5, as the key never changes" % ACT))
        self.assertTrue(self.passes(d, "%s:4-5, as the key stays" % ACT))

    def test_a_url_source_passes_without_paths(self):
        # spec: specs/evals/ontology.md#R4
        self.assertTrue(self.passes("route-aaaaaaaaaaaa", "%s:7-9" % OBJ))

    def test_with_paths_a_cited_path_resolves_only_through_the_map(self):
        # spec: specs/evals/ontology.md#R4
        a = "route-aaaaaaaaaaaa"
        paths = self.w.t / "paths.json"
        paths.write_text(json.dumps({URL: [OBJ, "palantir/docs/language/object-types.md"]}))
        self.assertTrue(self.passes(a, "palantir/docs/language/object-types.md:7-9", "--paths", paths))
        self.assertTrue(self.passes(a, "%s:7-9" % OBJ, "--paths", paths))
        self.assertFalse(self.passes(a, "palantir/docs/language/object-types.md:7-9"))
        self.assertFalse(self.passes(a, "%s:7-9" % URL, "--paths", paths))  # not a path the file had
        paths.write_text(json.dumps({"other": [OBJ]}))
        self.assertFalse(self.passes(a, "%s:7-9" % OBJ, "--paths", paths))
        paths.write_text("[]")
        self.w.sample(a, answer="x")
        self.assertEqual(self.w.score("--paths", paths)[1], {"status": "paths_invalid"})

    def test_score_json_has_the_routing_members_by_kind_by_cell_and_corpus_bytes(self):
        # spec: specs/evals/ontology.md#R4
        corpus = str(self.w.corpus)
        use = lambda i, cmd: {"type": "assistant", "message": {"content": [{"type": "tool_use", "id": i, "input": {"command": cmd}}]}}
        res = lambda i, c: {"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": i, "content": c}]}}
        stream = "\n".join(json.dumps(e) for e in [
            use("a", "cat %s/%s" % (corpus, OBJ)), res("a", "12345"), res("a", "12345"),
            use("b", "grep -r x %s/palantir" % corpus), res("b", [{"type": "text", "text": "éé"}, {"type": "image"}]),
            use("c", "cat %s" % corpus), res("c", "not under the corpus"), res("z", "no use")])
        self.w.sample("route-aaaaaaaaaaaa", 1, answer="%s:7" % OBJ, stream=stream, samples=2)
        self.w.sample("route-aaaaaaaaaaaa", 2, answer="nothing", stream="")
        self.w.sample("design-bbbbbbbbbbbb", 1, answer="%s:3" % ACT, stream=stream)
        code, score, r = self.w.score()
        self.assertEqual(code, 0, r.stdout)
        x = score["readers"]["claude-opus"]
        for k in ("n", "passes", "samples", "mean", "majority", "unanswered", "tokens_median", "fn_samples", "usd"):
            self.assertIn(k, x)
        self.assertEqual(x["unanswered"], 1)
        self.assertEqual(x["by_kind"], {"design": {"passes": 1, "samples": 2}, "route": {"passes": 1, "samples": 2}})
        self.assertEqual(x["by_cell"], {"language.object-type": {"passes": 2, "samples": 4}})
        self.assertEqual(x["corpus_bytes_median"], 9)  # 5 + 4 in each of two samples, 0 in the third
        self.assertEqual(set(score), {"suite", "case_files", "readers", "parity"})

    def test_score_writes_only_score_and_sheet_and_refuses_another_corpus(self):
        # spec: specs/evals/ontology.md#R4
        self.w.sample("route-aaaaaaaaaaaa", answer="x")
        before = sorted(p.name for p in self.w.out.rglob("*"))
        self.assertEqual(self.w.score()[0], 0)
        after = sorted(p.name for p in self.w.out.rglob("*"))
        self.assertEqual(sorted(set(after) - set(before)), ["score.json", "sheet.md"])
        sheet = (self.w.out / "sheet.md").read_text()
        self.assertIn("# Ontology Engineering suite: score", sheet)
        self.assertNotIn(OBJ, sheet)
        self.assertEqual(self.w.score(corpus=self.w.t)[1], {"status": "run_differs"})

    def test_gate_is_the_routing_suites(self):
        # spec: specs/evals/ontology.md#R1
        for n in (1, 2, 3):
            self.w.sample("route-aaaaaaaaaaaa", n, answer="%s:7" % OBJ, samples=3)
            self.w.sample("design-bbbbbbbbbbbb", n, answer="%s:3" % ACT)
        self.assertEqual(self.w.score()[0], 0)
        s = self.w.out / "score.json"
        code, out, _ = self.w.json_call("gate", "--baseline", s, "--candidate", s)
        self.assertEqual((code, out["status"]), (0, "pass"))
        code, out, _ = self.w.json_call("gate", "--baseline", s)
        self.assertEqual((code, out), (2, {"status": "usage"}))


class Reach(unittest.TestCase):
    def setUp(self):
        self.w = World(self)
        self.ins = self.w.t / "instructions"
        (self.ins / "runtime-references").mkdir(parents=True)
        (self.ins / "AGENTS.md").write_text("Read `/root/.agents/runtime-references/OFFICIAL-DOCS.md` section 4.\n")
        self.entry = self.ins / "runtime-references" / "OFFICIAL-DOCS.md"
        self.entry.write_text("## 4\nThe route table is `reference-docs/ROUTES.md`.\n")
        self.table = self.w.corpus / "ROUTES.md"

    def reach(self, *args):
        code, out, r = self.w.call("reach", "--cases", self.w.cases_path, "--instructions", self.ins, *map(str, args))
        self.assertEqual((code, r.stderr), (0, ""), out)
        return [json.loads(x) for x in out.splitlines()]

    def test_reach_reports_the_entry_the_table_and_the_row_of_each_case(self):
        # spec: specs/evals/ontology.md#R5
        lines = self.reach("--tables", self.table)
        cases = json.loads(CASES.read_text())
        self.assertEqual([x["case"] for x in lines[:-1]], sorted(c["id"] for c in cases))
        by = {x["case"]: x for x in lines[:-1]}
        self.assertEqual(by["route-3d1b053aeadc"], {"case": "route-3d1b053aeadc", "entry_named": True,
                                                    "table_named": True, "row": "%s:6" % self.table})
        self.assertIsNone(by["route-a246f5d568b3"]["row"])  # the row of lines 3-5 does not overlap 7-9
        self.assertEqual(lines[-1], {"cases": len(cases), "entry_named": len(cases), "table_named": len(cases),
                                     "row": 1, "reached": 1})
        self.assertFalse(list(self.w.t.glob("out*")))

    def test_reach_names_and_paths(self):
        # spec: specs/evals/ontology.md#R5
        unnamed = self.w.t / "OTHER.md"
        unnamed.write_text("| x | palantir/docs/foundry/object-types.md:8 |\n")
        lines = self.reach("--tables", self.table, "--tables", unnamed)
        by = {x["case"]: x for x in lines[:-1]}
        self.assertFalse(by["route-a246f5d568b3"]["table_named"])
        self.assertEqual(by["route-a246f5d568b3"]["row"], "%s:1" % unnamed)
        self.assertEqual(lines[-1]["reached"], 0)
        # the entry as its own table; an entry AGENTS.md does not name
        self.entry.write_text("| x | palantir/docs/foundry/action-types.md:5 |\n")
        (self.ins / "AGENTS.md").write_text("nothing\n")
        by = {x["case"]: x for x in self.reach("--tables", self.entry)[:-1]}
        self.assertEqual(by["route-3d1b053aeadc"], {"case": "route-3d1b053aeadc", "entry_named": False,
                                                    "table_named": True, "row": "%s:1" % self.entry})
        # with --paths a row resolves only through the map
        paths = self.w.t / "paths.json"
        paths.write_text(json.dumps({ACT: ["palantir/docs/logic/action-types.md"]}))
        self.entry.write_text("| x | palantir/docs/logic/action-types.md:5 |\n")
        by = {x["case"]: x for x in self.reach("--tables", self.entry, "--paths", paths)[:-1]}
        self.assertEqual(by["route-3d1b053aeadc"]["row"], "%s:1" % self.entry)
        by = {x["case"]: x for x in self.reach("--tables", self.entry)[:-1]}
        self.assertIsNone(by["route-3d1b053aeadc"]["row"])
        other = self.w.t / "elsewhere.md"
        other.write_text("x\n")
        by = {x["case"]: x for x in self.reach("--tables", self.table, "--entry", other)[:-1]}
        self.assertEqual((by["route-3d1b053aeadc"]["entry_named"], by["route-3d1b053aeadc"]["table_named"]), (False, False))


class Cut(unittest.TestCase):
    def setUp(self):
        self.w = World(self)

    def cut(self, *args, inventory=INVENTORY):
        code, out, r = self.w.call("cases", "--inventory", inventory, "--out", "/dev/stdout", *args)
        self.assertEqual((code, r.stderr), (0, ""), out)
        return json.loads(out)

    def test_cases_draws_route_and_trend_cases_per_cell(self):
        # spec: specs/evals/ontology.md#R6
        inv = json.loads(INVENTORY.read_text())
        cut = self.cut()
        self.assertEqual(cut, self.cut())
        drawn = lambda f, p: sha("%s:%d-%d" % (f["path"], *p["lines"]))
        for c in cut:
            (d,) = c["deciding"]
            f = next(f for f in inv if f["path"] == d["path"])
            p = next(p for p in f["passages"] if p["lines"] == d["lines"])
            self.assertEqual(c["id"], "%s-%s" % (c["kind"], drawn(f, p)[:12]))
            self.assertEqual(d["source"], f["publisher_url"] or f["path"])
            self.assertEqual(c["cell"], p["concept"])
            self.assertEqual(c["situation"], p["question"] + "\n\n" + ASK)
            self.assertFalse(c["cell"].startswith(("out.", "other:")))
            if c["kind"] == "route":
                self.assertEqual((f["source_kind"], p["strength"]), ("official-doc", "normative"))
            else:
                self.assertEqual(p["kind"], "announcement")
        kinds = sorted((c["kind"], c["cell"]) for c in cut)
        self.assertEqual(kinds.count(("route", "language.object-type")), 3)  # four drawn, three kept
        self.assertIn("route-ac0367aaf56a", [c["id"] for c in cut])  # one passage as both kinds: two ids
        self.assertIn("trend-ac0367aaf56a", [c["id"] for c in cut])
        one = self.cut("--per-cell", "1")
        self.assertEqual(len(one), len({(c["kind"], c["cell"]) for c in cut}))
        self.assertEqual(self.w.json_call("cases", "--inventory", INVENTORY, "--out", "/dev/stdout", "--per-cell", "0")[1],
                         {"status": "usage"})

    def test_cases_refuses_an_inventory_of_another_form(self):
        # spec: specs/evals/ontology.md#R6
        inv = json.loads(INVENTORY.read_text())
        bad = self.w.t / "bad.json"
        for change in (lambda v: v[0].update(source_kind="blog"), lambda v: v[0]["passages"][0].update(strength="x"),
                       lambda v: v[0]["passages"][0].update(lines=[3]), lambda v: v[0].pop("passages"), lambda v: v.clear() or v.append(1)):
            v = json.loads(json.dumps(inv))
            change(v)
            bad.write_text(json.dumps(v))
            out = self.w.t / "cut.json"
            self.assertEqual(self.w.json_call("cases", "--inventory", bad, "--out", out)[1], {"status": "inventory_invalid"})
            self.assertFalse(out.exists())

    def test_the_fixture_inventory_has_the_form_of_the_inventory(self):
        # spec: specs/evals/ontology.md#R6
        inv = json.loads(INVENTORY.read_text())
        for f in inv:
            self.assertEqual(set(f), {"path", "source_kind", "publisher_url", "passages"})
            self.assertTrue((FIXTURES / f["path"]).is_file())
            for p in f["passages"]:
                self.assertEqual(set(p), {"lines", "concept", "kind", "strength", "question", "quote"})
        self.assertTrue(any(f["publisher_url"] for f in inv))
        self.assertTrue(any(f["publisher_url"] is None for f in inv))

    def test_cases_writes_its_out_in_place(self):
        # spec: specs/evals/ontology.md#R6
        out = self.w.t / "cut.json"
        code, stdout, _ = self.w.call("cases", "--inventory", INVENTORY, "--out", out)
        self.assertEqual((code, stdout), (0, ""))
        self.assertEqual(json.loads(out.read_text()), self.cut())
        self.assertEqual(sorted(p.name for p in self.w.t.iterdir() if p.name.startswith(".part")), [])

    def test_a_cut_case_of_a_url_source_scores_by_its_path(self):
        # spec: specs/evals/ontology.md#R6
        # spec: specs/evals/ontology.md#R4
        cases = self.w.t / "cut.json"
        self.assertEqual(self.w.call("cases", "--inventory", INVENTORY, "--out", cases)[0], 0)
        shutil.copy2(cases, self.w.cases_path)
        c = next(c for c in json.loads(cases.read_text()) if c["deciding"][0]["source"].startswith("https://"))
        d = c["deciding"][0]
        self.w.sample(c["id"], answer="It is %s:%d-%d." % (d["path"], *d["lines"]))
        code, score, r = self.w.score()
        self.assertEqual(code, 0, r.stdout)
        self.assertEqual(score["readers"]["claude-opus"]["passes"][c["id"]], 1)


class Spec(unittest.TestCase):
    def test_every_requirement_names_a_test_of_this_file_and_every_test_cites_one(self):
        # spec: specs/evals/ontology.md#R7
        source = Path(__file__).read_text()
        tests = set(re.findall(r"^    def (test_\w+)\(", source, re.M))
        reqs = re.findall(r"^- (R[0-9]+): (.*)$", SPEC.read_text(), re.M)
        self.assertEqual([r for r, _ in reqs], ["R%d" % i for i in range(1, len(reqs) + 1)])
        self.assertGreaterEqual(len(reqs), 7)
        named_by = lambda body: set(re.findall(r"`(test_\w+)`", body.rpartition(" Tests: ")[2]))
        for rid, body in reqs:
            named = named_by(body)
            self.assertTrue(named, rid)
            self.assertLessEqual(named, tests, rid)
        mentioned = {t for _, body in reqs for t in named_by(body)}
        self.assertEqual(mentioned, tests)
        for cls in (Answers, Readers, Cases, Score, Reach, Cut, Spec):
            for name, fn in inspect.getmembers(cls, inspect.isfunction):
                if name.startswith("test_"):
                    cited = re.findall(r"spec: specs/evals/ontology\.md#(R[0-9]+)", inspect.getsource(fn))
                    self.assertTrue(cited, name)
                    for rid in cited:
                        self.assertIn(name, named_by(dict(reqs)[rid]), (name, rid))


if __name__ == "__main__":
    unittest.main()
