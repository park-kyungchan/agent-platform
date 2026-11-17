"""Tests of evals/routing/run, the routing suite, run as a program against a fake `systemd-run`, a fake `systemctl`
and fake CLIs (no model, no network, no real unit: PATH holds the fakes' directory and nothing else)."""
import hashlib
import importlib.machinery
import importlib.util
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

RUN = Path(__file__).resolve().parent.parent / "evals" / "routing" / "run"
PY = sys.executable

# The fake records its arguments and runs the command after `--` with only the environment its --setenv gives (as a
# unit gets), in its --working-directory. It refuses an option or a property it does not know, as the real one does.
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
PROPS = ("ProtectSystem=", "ProtectHome=", "RuntimeMaxSec=", "ReadWritePaths=", "InaccessiblePaths=")
env, cwd, k, desc = {"FAKE_IN_UNIT": "1"}, None, 0, ""
while k < len(opts):
    o = opts[k]
    if o == "-p":
        if not opts[k + 1].startswith(PROPS):
            sys.exit(1)
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
    print("Failed to start transient service unit", file=sys.stderr)
    sys.exit(1)
p = subprocess.Popen(cmd, cwd=cwd, env=env)
open(os.path.join(state, "pids", str(p.pid)), "w").close()
sys.exit(p.wait())
'''

FAKE_SYSTEMCTL = r'''#!@PY@
import os, sys
state = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
argv = [a for a in sys.argv[1:] if a != "--"]
with open(os.path.join(state, "systemctl.log"), "a") as f:
    f.write(" ".join(argv) + "\n")
if argv[0] == "is-active":
    path = os.path.join(state, "active.txt")
    active = set(open(path).read().split()) if os.path.exists(path) else set()
    for u in argv[1:]:
        print("active" if u in active else "inactive")
    sys.exit(0 if all(u in active for u in argv[1:]) else 3)
if argv[0] == "show":
    print("")
    sys.exit(0)
sys.exit(1)
'''

FAKE_CLI = r'''#!@PY@
import json, os, re, subprocess, sys, time
state = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
plan = json.load(open(os.path.join(state, "plan.json")))
name = os.path.basename(sys.argv[0])
args = sys.argv[1:]
prompt = args[args.index("--print") + 1] if name == "agy" else args[-1]
cid = re.search(r"CASEID (\S+)", prompt).group(1)
cnt = os.path.join(state, "count", name + "-" + cid)
n = int(open(cnt).read()) if os.path.exists(cnt) else 0
open(cnt, "w").write(str(n + 1))
with open(os.path.join(state, "cli.jsonl"), "a") as f:
    f.write(json.dumps({"cli": name, "case": cid, "try": n, "argv": args, "cwd": os.getcwd(),
                        "env": dict(os.environ)}) + "\n")
beh = plan.get("cli", {}).get(cid, [{}])
beh = beh[min(n, len(beh) - 1)]
mode = beh.get("mode", "ok")
answer = beh.get("answer", "")
if mode == "exit":
    sys.exit(1)
if mode == "sleep":
    time.sleep(beh["seconds"])
if mode == "mcp":
    subprocess.run([sys.executable, "-c", "import time; time.sleep(0.8)", "mcp-server.js"])
if name == "claude":
    sid = args[args.index("--session-id") + 1]
    events = [{"type": "system", "subtype": "init", "session_id": sid},
              {"type": "result", "result": answer, "num_turns": 2,
               "usage": {"input_tokens": 10, "cache_read_input_tokens": 100, "cache_creation_input_tokens": 5,
                         "output_tokens": 7}}]
elif name == "muse":
    events = [{"payload": {"kind": "run_usage", "models": [{"input_tokens": 40, "output_tokens": 4},
                                                           {"input_tokens": 2, "output_tokens": 1}]}},
              {"payload": {"kind": "run_terminal", "text": answer}}]
else:
    events = [{"event": "init", "session_id": "agy-session-1"},
              {"event": "result", "result": {"response": answer,
                                             "usage": {"input_tokens": 30, "cache_read_tokens": 3, "output_tokens": 9}}}]
for e in events:
    print(json.dumps(e), flush=True)
'''

GOOD = "Load `FACT-ALPHA   one` and FACT-GAMMA with FACT-DELTA."
CASES = [
    {"id": "c1", "situation": "CASEID c1 SITUATION-SECRET-ONE",
     "must": [{"fact": ["FACT-ALPHA one"]}, {"fact": ["FACT-GAMMA", "FACT-DELTA"]}],
     "wrong": [{"fact": ["FACT-STALE"]}]},
    {"id": "c2", "situation": "CASEID c2 SITUATION-SECRET-TWO", "must": [{"fact": ["FACT-ALPHA one"]}], "wrong": []},
]
HOST = [{"id": "h1", "situation": "CASEID h1 SITUATION-SECRET-HOST", "must": [{"fact": ["FACT-HOST"]}],
         "wrong": [], "kind": 3, "source": "not read"}]


def load_module():
    loader = importlib.machinery.SourceFileLoader("routing_run", str(RUN))
    spec = importlib.util.spec_from_loader("routing_run", loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


class World:
    def __init__(self, test):
        self.tmp = tempfile.TemporaryDirectory()
        test.addCleanup(self.close)
        t = self.t = Path(self.tmp.name)
        site = t / "site" / "evals" / "routing"
        site.mkdir(parents=True)
        shutil.copy2(RUN, site / "run")
        self.run_path = site / "run"
        (site / "cases.json").write_text(json.dumps(CASES))
        self.host = t / "host-cases.json"
        self.host.write_text(json.dumps(HOST))
        self.other_host = t / "other-host.json"
        self.other_host.write_text(json.dumps([dict(HOST[0], id="h2", situation="CASEID h2 x")]))
        self.unlisted = t / "unlisted.json"
        self.unlisted.write_text(json.dumps([dict(HOST[0], id="u1")]))
        digest = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
        (site / "host-cases.sha256").write_text("%s  fixture-host\n%s  fixture-other\n" % (
            digest(self.host), digest(self.other_host)))
        self.bin = t / "bin"
        self.bin.mkdir()
        for name, text in (("systemd-run", FAKE_SYSTEMD_RUN), ("systemctl", FAKE_SYSTEMCTL), ("claude", FAKE_CLI),
                           ("muse", FAKE_CLI), ("agy", FAKE_CLI)):
            (self.bin / name).write_text(text.replace("@PY@", PY))
            (self.bin / name).chmod(0o755)
        (t / "count").mkdir()
        (t / "pids").mkdir()
        self.root = t / "root"
        self.root.mkdir()
        (self.root / "AGENTS.md").write_text("agents\n")
        self.home = t / "home"
        self.acct = t / "acct"
        self.acct.mkdir()
        (self.home / ".claude-x" / "projects" / "-w-edu-lessons").mkdir(parents=True)
        (self.home / ".claude-x" / "projects" / "-w-other").mkdir(parents=True)
        self.secret = t / "secret-dir"
        self.secret.mkdir()
        self.out = t / "out"
        self.plan({})

    def close(self):
        for f in (self.t / "pids").iterdir() if (self.t / "pids").exists() else []:
            try:
                os.kill(int(f.name), signal.SIGKILL)
            except (ProcessLookupError, ValueError):
                pass
        self.tmp.cleanup()

    def plan(self, plan):
        (self.t / "plan.json").write_text(json.dumps(plan))

    def env(self, **extra):
        env = {"PATH": str(self.bin), "HOME": str(self.home), "CLAUDE_CONFIG_DIR": str(self.acct)}
        env.update(extra)
        return {k: v for k, v in env.items() if v is not None}

    def call(self, verb, *args, env=None):
        r = subprocess.run([PY, "-I", "-B", str(self.run_path), verb, *map(str, args)], env=env or self.env(),
                           capture_output=True, text=True, timeout=120)
        out = json.loads(r.stdout) if r.stdout.strip() else None
        return r.returncode, out, r

    def answers(self, *args, reader="claude-haiku", out=None, env=None):
        return self.call("answers", "--root", self.root, "--reader", reader, "--out", out or self.out, *args, env=env)

    def lines(self, name):
        p = self.t / name
        return [json.loads(x) for x in p.read_text().splitlines()] if p.exists() else []

    def cli_runs(self):
        return self.lines("cli.jsonl")

    def unit_calls(self, sessions_only=True):
        calls = self.lines("systemd-run.jsonl")
        if sessions_only:
            calls = [c for c in calls if not any(o.startswith("--description=") and o.endswith("-probe") for o in c)]
        return calls

    def sample(self, cid, n, reader="claude-haiku", out=None):
        return Path(out or self.out) / reader / ("%s__s%d" % (cid, n))


def props(call, name):
    opts = call[:call.index("--")]
    return [opts[i + 1].split("=", 1)[1] for i, o in enumerate(opts) if o == "-p" and opts[i + 1].startswith(name + "=")]


def option(call, name):
    opts = call[:call.index("--")]
    return [o.split("=", 1)[1] for o in opts if o.startswith(name + "=")]


class FactRule(unittest.TestCase):
    def test_every_string_after_normalisation_all_must_no_wrong(self):
        m = load_module()
        case = CASES[0]
        self.assertTrue(m.passes(case, GOOD))
        self.assertTrue(m.passes(case, "FACT-ALPHA\n\t one; FACT-DELTA then `FACT-GAMMA`"))
        self.assertFalse(m.passes(case, "FACT-ALPHAone FACT-GAMMA FACT-DELTA"), "whitespace is kept as one space")
        self.assertFalse(m.passes(case, "FACT-ALPHA one FACT-GAMMA"), "every string of a fact")
        self.assertFalse(m.passes(case, "FACT-GAMMA FACT-DELTA"), "every must fact")
        self.assertFalse(m.passes(case, GOOD + " FACT-STALE"), "no wrong fact")
        self.assertTrue(m.passes(case, GOOD + " FACT-STAL E"), "a wrong fact holds only when found")
        self.assertTrue(m.passes({"must": [{"fact": ["a `b`  c"]}], "wrong": []}, "x a b c y"), "the fact is normalised too")

    def test_repository_cases_are_the_instructions_cases(self):
        root = RUN.parent.parent
        old = json.loads((root / "instructions" / "cases.json").read_text())
        new = json.loads((root / "routing" / "cases.json").read_text())
        self.assertEqual([c["id"] for c in new], [c["id"] for c in old])
        for o, c in zip(old, new):
            self.assertEqual(c["situation"], o["situation"])
            self.assertEqual(c["must"], [{"fact": [t]} for t in o["tokens"]])
            self.assertEqual(c["wrong"], [])
        first = (root / "routing" / "host-cases.sha256").read_text().splitlines()[0]
        self.assertEqual(first, "ce1ad2830739cb760f4f7346bcfe66f061a97314dc32b7878e828666cbe5701f  host-routing-24")


class Unit(unittest.TestCase):
    def setUp(self):
        self.w = World(self)
        self.w.plan({"cli": {"c1": [{"answer": GOOD}], "c2": [{"answer": GOOD}]}})

    def test_claude_unit_properties(self):
        w = self.w
        code, out, r = w.answers("--case", "c1", "--samples", "1", "--deny", str(w.t / "secret-*"))
        self.assertEqual(code, 0, r.stdout + r.stderr)
        self.assertEqual(out["samples"], {"c1__s1": "ok"})
        [call] = w.unit_calls()
        self.assertIn("--wait", call)
        self.assertIn("--collect", call)
        self.assertEqual(option(call, "--slice"), ["agents.slice"])
        self.assertEqual(option(call, "--unit"), ["routing-" + hashlib.sha256(
            str(w.sample("c1", 1)).encode()).hexdigest()[:16]])
        self.assertEqual(option(call, "--description"), ["routing-claude-haiku-c1-s1"])
        self.assertEqual(option(call, "--expand-environment"), ["no"])
        self.assertEqual(props(call, "ProtectSystem"), ["strict"])
        self.assertEqual(props(call, "ProtectHome"), ["read-only"])
        self.assertEqual(props(call, "RuntimeMaxSec"), ["600"])
        self.assertEqual(sorted(props(call, "ReadWritePaths")), sorted(["-/tmp", "-" + str(w.acct)]))
        deny = props(call, "InaccessiblePaths")
        self.assertIn("-" + str(w.secret), deny)
        self.assertIn("-" + str(w.home / ".claude-x" / "projects" / "-w-edu-lessons"), deny)
        self.assertNotIn("-" + str(w.home / ".claude-x" / "projects" / "-w-other"), deny)
        self.assertTrue(all(d.startswith("-/") and " " not in d for d in deny), "one path per assignment")
        opts = call[:call.index("--")]
        self.assertFalse([o for o in opts if "SITUATION-SECRET" in o], "the prompt is no option of the unit")
        cmd = call[call.index("--") + 1:]
        self.assertEqual(cmd[0], "/bin/sh")
        cli = cmd[cmd.index(str(w.bin / "claude")):]
        self.assertIn("--strict-mcp-config", cli)
        self.assertNotIn("--tools", cli)
        self.assertEqual(cli[cli.index("--permission-mode") + 1], "bypassPermissions")
        self.assertEqual(cli[cli.index("--model") + 1], "haiku")
        self.assertEqual(cli[cli.index("--add-dir") + 1], str(w.root))
        self.assertEqual(cli.count("--add-dir"), 1)
        prompt = cli[-1]
        self.assertIn("SITUATION-SECRET-ONE", prompt)
        self.assertIn("start at %s/AGENTS.md" % w.root, prompt)
        self.assertIn("Never open anything under /home/palantirkc/work/edu-*", prompt)
        self.assertIn("Do not do the assignment.", prompt)
        self.assertIn("Read, search and run any reading command you need", prompt)
        [run] = w.cli_runs()
        self.assertEqual(run["env"].get("FAKE_IN_UNIT"), "1", "the CLI ran inside the unit")
        self.assertEqual(run["env"]["CLAUDE_CONFIG_DIR"], str(w.acct))
        usage = json.loads(w.sample("c1", 1).with_suffix(".json").read_text())
        self.assertEqual((usage["input_tokens"], usage["output_tokens"], usage["answered"]), (115, 7, True))
        self.assertEqual(usage["session"], cli[cli.index("--session-id") + 1])
        self.assertIsInstance(usage["seconds"], float)
        self.assertTrue(w.sample("c1", 1).with_suffix(".jsonl").read_text())
        self.assertEqual(w.sample("c1", 1).with_suffix(".md").read_text(), GOOD)

    def test_sonnet_is_the_haiku_row_with_sonnet(self):
        w = self.w
        code, out, _ = w.answers("--case", "c1", "--samples", "1", reader="claude-sonnet")
        self.assertEqual(code, 0)
        cli = w.cli_runs()[0]["argv"]
        self.assertEqual(cli[cli.index("--model") + 1], "sonnet")
        self.assertEqual(out["samples"], {"c1__s1": "ok"})

    def test_claude_without_config_dir(self):
        w = self.w
        code, _, _ = w.answers("--case", "c1", "--samples", "1", env=w.env(CLAUDE_CONFIG_DIR=None))
        self.assertEqual(code, 0)
        [call] = w.unit_calls()
        self.assertEqual(sorted(props(call, "ReadWritePaths")),
                         sorted(["-/tmp", "-%s/.claude" % w.home, "-%s/.claude.json" % w.home]))

    def test_muse_and_agy_write_only_their_own_state(self):
        w = self.w
        self.assertEqual(w.answers("--case", "c1", "--samples", "1", reader="muse-ember")[0], 0)
        self.assertEqual(w.answers("--case", "c1", "--samples", "1", reader="agy-hydrogen")[0], 0)
        muse, agy = w.unit_calls()
        h = str(w.home)
        self.assertEqual(sorted(props(muse, "ReadWritePaths")), sorted(
            ["-/tmp", "-%s/.config/muse" % h, "-%s/.local/share/muse" % h, "-%s/.local/state/muse" % h, "-%s/.cache" % h]))
        self.assertEqual(props(agy, "ReadWritePaths"), ["-/tmp", "-%s/.gemini/antigravity-cli" % h])
        mcli, acli = [r["argv"] for r in w.cli_runs()]
        self.assertNotIn("--disable-shell", mcli)
        self.assertNotIn("--yolo", mcli)
        self.assertNotIn("--trust-workspace", mcli)
        self.assertIn("--disable-approval", mcli)
        self.assertIn("--dangerously-skip-permissions", acli)
        self.assertNotEqual(w.cli_runs()[1]["env"]["HOME"], h, "agy runs with a new HOME")
        mu = json.loads(w.sample("c1", 1, "muse-ember").with_suffix(".json").read_text())
        ag = json.loads(w.sample("c1", 1, "agy-hydrogen").with_suffix(".json").read_text())
        self.assertEqual((mu["input_tokens"], mu["output_tokens"]), (42, 5))
        self.assertEqual((ag["input_tokens"], ag["output_tokens"], ag["session"]), (33, 9, "agy-session-1"))

    def test_no_systemd_run_no_session(self):
        w = self.w
        (w.bin / "systemd-run").unlink()
        code, out, _ = w.answers("--case", "c1")
        self.assertEqual((code, out), (2, {"status": "sandbox_unavailable"}))
        self.assertEqual(w.cli_runs(), [])
        self.assertFalse((w.out / "claude-haiku" / "run.json").exists())

    def test_unit_that_does_not_start_no_session(self):
        w = self.w
        w.plan({"systemd_run": "fail"})
        code, out, _ = w.answers("--case", "c1")
        self.assertEqual((code, out), (2, {"status": "sandbox_unavailable"}))
        self.assertEqual(w.cli_runs(), [])
        self.assertEqual(w.unit_calls(), [])

    def test_session_unit_that_does_not_start_stops_the_call(self):
        w = self.w
        w.plan({"systemd_run": "fail_sessions"})
        code, out, r = w.answers("--jobs", "1")
        self.assertEqual((code, out), (2, {"status": "sandbox_unavailable"}), r.stderr)
        self.assertEqual(w.cli_runs(), [])
        self.assertEqual(len(w.unit_calls()), 1, "no further session after the first unit did not start")

    def test_mcp_rule_stops_further_sessions(self):
        w = self.w
        w.plan({"cli": {"c1": [{"mode": "mcp", "answer": GOOD}], "c2": [{"answer": GOOD}]}})
        code, out, r = w.answers("--jobs", "1", "--samples", "1")
        self.assertEqual(code, 2, r.stdout)
        self.assertEqual(out["mcp_rule_broken"], ["c1__s1"])
        self.assertEqual(out["samples"]["c2__s1"], "not_run")
        self.assertEqual([x["case"] for x in w.cli_runs()], ["c1"])


class Samples(unittest.TestCase):
    def setUp(self):
        self.w = World(self)
        self.w.plan({"cli": {"c1": [{"answer": GOOD}], "c2": [{"mode": "exit"}, {"answer": GOOD}]}})

    def test_unanswered_sample_runs_again_and_nothing_else(self):
        w = self.w
        code, out, _ = w.answers("--samples", "1")
        self.assertEqual(code, 0)
        self.assertEqual(out["samples"], {"c1__s1": "ok", "c2__s1": "failed"})
        self.assertFalse(json.loads(w.sample("c2", 1).with_suffix(".json").read_text())["answered"])
        _, score, _ = w.call("score", "--out", w.out)
        self.assertEqual(score["readers"]["claude-haiku"]["unanswered"], 1)
        code, out, _ = w.answers("--samples", "1")
        self.assertEqual(out["samples"], {"c1__s1": "done", "c2__s1": "ok"})
        self.assertEqual(sorted((x["case"], x["try"]) for x in w.cli_runs()), [("c1", 0), ("c2", 0), ("c2", 1)])
        self.assertEqual(w.cli_runs()[-1]["case"], "c2")
        _, score, _ = w.call("score", "--out", w.out)
        self.assertEqual(score["readers"]["claude-haiku"]["unanswered"], 0)

    def test_raising_samples_runs_only_the_new_ones(self):
        w = self.w
        w.plan({"cli": {"c1": [{"answer": GOOD}], "c2": [{"answer": GOOD}]}})
        w.answers("--samples", "1")
        code, out, _ = w.answers("--samples", "2")
        self.assertEqual(out["samples"], {"c1__s1": "done", "c1__s2": "ok", "c2__s1": "done", "c2__s2": "ok"})
        self.assertEqual(json.loads((w.out / "claude-haiku" / "run.json").read_text())["samples"], 2)

    def test_half_written_answer_is_no_answer(self):
        w = self.w
        w.plan({"cli": {"c1": [{"answer": GOOD}], "c2": [{"answer": GOOD}]}})
        w.answers("--samples", "1")
        s1, s2 = w.sample("c1", 1), w.sample("c2", 1)
        s1.with_suffix(".json").unlink()  # the answer renamed, the usage not yet written
        s2.with_suffix(".md").write_text("")  # an empty answer
        (s1.parent / ".part-x").write_text(GOOD[:5])  # a half-written file under another name
        _, score, _ = w.call("score", "--out", w.out)
        x = score["readers"]["claude-haiku"]
        self.assertEqual((x["unanswered"], x["passes"]), (2, {"c1": 0, "c2": 0}))
        _, out, _ = w.answers("--samples", "1")
        self.assertEqual(out["samples"], {"c1__s1": "ok", "c2__s1": "ok"})

    def test_stopped_call_leaves_no_answer_and_continues(self):
        w = self.w
        w.plan({"cli": {"c1": [{"answer": GOOD}], "c2": [{"mode": "sleep", "seconds": 60, "answer": GOOD}, {"answer": GOOD}]}})
        p = subprocess.Popen([PY, "-I", "-B", str(w.run_path), "answers", "--root", str(w.root), "--reader",
                              "claude-haiku", "--out", str(w.out), "--samples", "1", "--jobs", "1"],
                             env=w.env(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.time() + 60
        while len(w.cli_runs()) < 2 and time.time() < deadline:
            time.sleep(0.1)
        p.kill()
        p.wait()
        for f in (w.t / "pids").iterdir():
            try:
                os.kill(int(f.name), signal.SIGKILL)
            except ProcessLookupError:
                pass
        self.assertFalse(w.sample("c2", 1).with_suffix(".md").exists())
        self.assertFalse(w.sample("c2", 1).with_suffix(".json").exists())
        _, out, _ = w.answers("--samples", "1")
        self.assertEqual(out["samples"], {"c1__s1": "done", "c2__s1": "ok"})

    def test_run_still_active_only_for_a_sample_it_would_start(self):
        w = self.w
        w.plan({"cli": {"c1": [{"answer": GOOD}], "c2": [{"answer": GOOD}]}})
        unit = lambda cid, n, out=None: "routing-%s.service" % hashlib.sha256(
            str(w.sample(cid, n, out=out)).encode()).hexdigest()[:16]
        (w.t / "active.txt").write_text(unit("c1", 1) + "\n")
        code, out, _ = w.answers("--samples", "1")
        self.assertEqual((code, out), (2, {"status": "run_still_active"}))
        self.assertEqual(w.cli_runs(), [])
        other = w.t / "out-other"
        code, out, _ = w.answers("--samples", "1", out=other)
        self.assertEqual(code, 0, "another --out runs beside it")
        (w.t / "active.txt").write_text(unit("c1", 1, other) + "\n")
        code, out, _ = w.answers("--samples", "2", out=other)
        self.assertEqual(code, 0, "the active unit is of a done sample")
        self.assertEqual(out["samples"]["c1__s1"], "done")


class Refusals(unittest.TestCase):
    def setUp(self):
        self.w = World(self)
        self.w.plan({"cli": {c: [{"answer": GOOD}] for c in ("c1", "c2", "h1", "h2")}})

    def refused(self, code_out, status):
        code, out, r = code_out
        self.assertEqual((code, out), (2, {"status": status}), r.stdout + r.stderr)
        self.assertEqual(r.stdout.strip(), '{"status":"%s"}' % status)

    def test_answers_refusals(self):
        w = self.w
        self.refused(w.answers("--cases", w.unlisted), "cases_digest_unknown")
        self.refused(w.answers("--cases", "host-cases.json"), "cases_unreadable")
        self.refused(w.answers("--cases", w.host, "--cases", w.host), "case_id_twice")
        self.refused(w.answers("--case", "zz"), "case_unknown")
        self.refused(w.answers("--samples", "10"), "usage")
        self.refused(w.answers("--jobs", "5"), "usage")
        self.refused(w.answers(reader="claude-opus"), "usage")
        (w.root / "AGENTS.md").unlink()
        self.refused(w.answers(), "root_invalid")
        self.assertEqual(w.cli_runs(), [])

    def test_run_differs(self):
        w = self.w
        self.assertEqual(w.answers("--cases", w.host, "--samples", "3", "--case", "c1", "--case", "h1")[0], 0)
        rec = json.loads((w.out / "claude-haiku" / "run.json").read_text())
        self.assertEqual(rec["cases"], ["c1", "h1"])
        self.assertEqual(set(rec["case_files"]), {"cases.json", "fixture-host"})
        self.assertEqual(rec["suite"], hashlib.sha256(w.run_path.read_bytes()).hexdigest())
        self.refused(w.answers("--cases", w.host, "--samples", "2", "--case", "c1", "--case", "h1"), "run_differs")
        self.refused(w.answers("--cases", w.host, "--case", "c1"), "run_differs")
        self.refused(w.answers("--case", "c1"), "run_differs")
        self.refused(w.answers("--cases", w.host, "--cases", w.other_host, "--case", "c1", "--case", "h1"), "run_differs")
        with open(w.run_path, "a") as f:
            f.write("# changed\n")
        self.refused(w.answers("--cases", w.host, "--case", "c1", "--case", "h1"), "run_differs")

    def test_score_refusals(self):
        w = self.w
        w.out.mkdir()
        self.refused(w.call("score", "--out", w.out), "no_reader")
        (w.out / "muse-ember").mkdir()
        self.refused(w.call("score", "--out", w.out), "run_record_missing")
        (w.out / "muse-ember").rmdir()
        w.answers("--cases", w.host, "--samples", "1")
        self.refused(w.call("score", "--out", w.out), "case_sets_differ")
        self.refused(w.call("score", "--out", w.out, "--cases", w.host, "--cases", w.other_host), "case_sets_differ")
        self.assertEqual(w.call("score", "--out", w.out, "--cases", w.host)[0], 0)
        w.answers("--cases", w.host, "--samples", "1", reader="muse-ember")
        rec = w.out / "muse-ember" / "run.json"
        rec.write_text(json.dumps(dict(json.loads(rec.read_text()), suite="0" * 64)))
        self.refused(w.call("score", "--out", w.out, "--cases", w.host), "run_differs")


def fake_reader(d, n, table, tokens=None):
    """A reader directory by hand: table is case -> list of n answers (None: unanswered)."""
    d.mkdir(parents=True)
    (d / "run.json").write_text(json.dumps({"samples": n, "cases": sorted(table), "case_files": "@FILES@", "suite": "s" * 64}))
    for cid, answers in table.items():
        for i, ans in enumerate(answers, 1):
            if ans is None:
                continue
            (d / ("%s__s%d.md" % (cid, i))).write_text(ans)
            (d / ("%s__s%d.json" % (cid, i))).write_text(json.dumps({"answered": True, "input_tokens": (tokens or {}).get((cid, i), 100)}))


class Score(unittest.TestCase):
    def setUp(self):
        self.w = World(self)

    def files(self, d):
        m = load_module()
        m.REPO_CASES = str(self.w.run_path.parent / "cases.json")
        m.HOST_DIGESTS = str(self.w.run_path.parent / "host-cases.sha256")
        _, digests = m.load_cases([str(self.w.host)])
        for r in d.iterdir():
            if not r.is_dir():
                continue
            rec = r / "run.json"
            rec.write_text(rec.read_text().replace('"@FILES@"', json.dumps(digests)))

    def test_majority_parity_and_sheet(self):
        w = self.w
        bad = "FACT-ALPHA one only"
        stale = GOOD + " FACT-STALE"
        fake_reader(w.out / "claude-sonnet", 3, {"c1": [GOOD, GOOD, bad], "c2": [GOOD, GOOD, GOOD],
                                                  "h1": ["FACT-HOST", "x", "x"]})
        fake_reader(w.out / "claude-haiku", 3, {"c1": [GOOD, stale, None], "c2": [GOOD, GOOD, "x"],
                                                 "h1": ["FACT-HOST", "FACT-HOST", "x"]},
                    tokens={("c1", 1): 10, ("c1", 2): 30, ("c2", 1): 50, ("c2", 2): 70, ("c2", 3): 90,
                            ("h1", 1): 20, ("h1", 2): 40, ("h1", 3): 60})
        (w.out / "claude-haiku" / "c1__s3.json").write_text(json.dumps({"answered": False}))
        self.files(w.out)
        code, s, r = w.call("score", "--out", w.out, "--cases", w.host)
        self.assertEqual(code, 0, r.stdout)
        son, hai = s["readers"]["claude-sonnet"], s["readers"]["claude-haiku"]
        self.assertEqual(son["passes"], {"c1": 2, "c2": 3, "h1": 1})
        self.assertEqual(son["samples"], [3, 2, 1])
        self.assertEqual(son["mean"], 2.0)
        self.assertEqual(son["majority"], ["c1", "c2"])
        self.assertEqual(hai["passes"], {"c1": 1, "c2": 2, "h1": 2})
        self.assertEqual(hai["samples"], [3, 2, 0])
        self.assertEqual(hai["majority"], ["c2", "h1"])
        self.assertEqual(hai["unanswered"], 1)
        self.assertEqual(hai["tokens_median"], 45)
        self.assertEqual(s["parity"], {"claude-haiku": 0.5})
        self.assertEqual(json.loads((w.out / "score.json").read_text()), s)
        sheet = (w.out / "sheet.md").read_text()
        for secret in ["SITUATION-SECRET", "FACT-", "CASEID", str(w.t), "fixture-host.json"]:
            self.assertNotIn(secret, sheet)
        for public in ["c1", "h1", "fixture-host", s["suite"], s["case_files"]["fixture-host"], "0.5", "45"]:
            self.assertIn(public, sheet)

    def test_parity_null_without_sonnet_or_its_majority(self):
        w = self.w
        fake_reader(w.out / "claude-haiku", 3, {"c1": [GOOD] * 3, "c2": [GOOD] * 3, "h1": ["x"] * 3})
        self.files(w.out)
        _, s, _ = w.call("score", "--out", w.out, "--cases", w.host)
        self.assertEqual(s["parity"], {"claude-haiku": None})
        fake_reader(w.out / "claude-sonnet", 3, {"c1": ["x"] * 3, "c2": ["x"] * 3, "h1": ["x"] * 3})
        self.files(w.out)
        _, s, _ = w.call("score", "--out", w.out, "--cases", w.host)
        self.assertEqual(s["parity"], {"claude-haiku": None})


def table(passes, n=3, unanswered=0):
    majority = sorted(c for c, v in passes.items() if v > n / 2)
    return {"n": n, "passes": passes, "samples": [], "mean": sum(passes.values()) / n, "majority": majority,
            "unanswered": unanswered, "tokens_median": None}


class Gate(unittest.TestCase):
    def setUp(self):
        self.w = World(self)
        self.rest = {"k%02d" % i: (3 if i % 2 else 0) for i in range(2, 21)}

    def gate(self, base, cand, files=("a", "a")):
        b, c = self.w.t / "b.json", self.w.t / "c.json"
        b.write_text(json.dumps({"suite": "x", "case_files": {"cases.json": files[0]}, "readers": base}))
        c.write_text(json.dumps({"suite": "y", "case_files": {"cases.json": files[1]}, "readers": cand}))
        return self.w.call("gate", "--baseline", b, "--candidate", c)

    def test_one_case_lost_entirely_fails(self):
        code, out, _ = self.gate({"muse-ember": table(dict(self.rest, k01=3))}, {"muse-ember": table(dict(self.rest, k01=0))})
        x = out["readers"]["muse-ember"]
        self.assertEqual((code, out["status"]), (1, "fail"))
        self.assertEqual((x["drop"], x["noise"], x["ok"], x["lost"]), (1.0, 0.408, False, ["k01"]))

    def test_one_case_from_two_to_one_is_noise(self):
        code, out, _ = self.gate({"muse-ember": table(dict(self.rest, k01=2))}, {"muse-ember": table(dict(self.rest, k01=1))})
        x = out["readers"]["muse-ember"]
        self.assertEqual((code, out["status"]), (0, "pass"))
        self.assertEqual((x["drop"], x["noise"], x["ok"], x["lost"]), (0.333, 0.408, True, ["k01"]))

    def test_unequal_samples_and_a_gain(self):
        code, out, _ = self.gate({"agy-hydrogen": table(dict(self.rest, k01=0))},
                                 {"agy-hydrogen": table(dict({k: v * 5 // 3 for k, v in self.rest.items()}, k01=5), n=5)})
        self.assertEqual((code, out["readers"]["agy-hydrogen"]["ok"]), (0, True))

    def test_reader_missing_fails_reader_only_in_candidate_not_judged(self):
        t = table(dict(self.rest, k01=3))
        code, out, _ = self.gate({"muse-ember": t, "claude-haiku": t}, {"claude-haiku": t, "agy-hydrogen": table(dict(self.rest, k01=0))})
        self.assertEqual((code, out["status"]), (1, "fail"))
        self.assertEqual(set(out["readers"]), {"muse-ember", "claude-haiku"})
        self.assertFalse(out["readers"]["muse-ember"]["ok"])
        self.assertTrue(out["readers"]["claude-haiku"]["ok"])

    def test_refusals(self):
        t = table(dict(self.rest, k01=3))
        for base, cand, files, status in [
            ({"m": t}, {"m": table(dict(self.rest, k01=1), n=2)}, ("a", "a"), "samples_too_few"),
            ({"m": table(dict(self.rest, k01=1), n=2)}, {"m": t}, ("a", "a"), "samples_too_few"),
            ({"m": t}, {"m": table(dict(self.rest, k01=3), unanswered=4)}, ("a", "a"), "run_incomplete"),
            ({"m": t}, {"m": table(dict(self.rest, k21=3))}, ("a", "a"), "case_sets_differ"),
            ({"m": t}, {"m": t}, ("a", "b"), "case_sets_differ"),
        ]:
            code, out, _ = self.gate(base, cand, files)
            self.assertEqual((code, out), (2, {"status": status}))
        code, _, _ = self.gate({"m": t}, {"m": table(dict(self.rest, k01=3), unanswered=3)})
        self.assertEqual(code, 0, "a twentieth of 20 cases x 3 samples is allowed")


if __name__ == "__main__":
    unittest.main()
