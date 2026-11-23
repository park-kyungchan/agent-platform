"""Tests of evals/routing/run, the routing suite, run as a program against a fake `systemd-run`, a fake `systemctl`
and fake CLIs (no model, no network, no real unit: PATH holds the fakes' directory and nothing else)."""
import hashlib
import importlib.machinery
import importlib.util
import json
import os
import re
import shutil
import signal
import stat
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
    import json
    plan = json.load(open(os.path.join(state, "plan.json")))
    print(plan.get("control_group", ""))
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
def chain():
    """(pid, start time) of this process and of each process above it."""
    found, pid = [], os.getpid()
    while pid > 1:
        try:
            rest = open("/proc/%d/stat" % pid).read().rsplit(")", 1)[1].split()
        except OSError:
            break
        found.append([pid, int(rest[19])])
        pid = int(rest[1])
    return found
with open(os.path.join(state, "cli.jsonl"), "a") as f:
    f.write(json.dumps({"cli": name, "case": cid, "try": n, "argv": args, "cwd": os.getcwd(),
                        "env": dict(os.environ), "chain": chain()}) + "\n")
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
if mode == "mcp_orphan":
    # A server that left the process tree (its parent ended): only the unit's control group still holds it.
    subprocess.run(["/bin/sh", "-c", "(%s -c 'import time; time.sleep(4)' mcp-server.js &)" % sys.executable])
    time.sleep(1.5)
if name == "claude":
    sid = args[args.index("--session-id") + 1]
    events = [{"type": "system", "subtype": "init", "session_id": sid}]
    for blocks in beh.get("tool_use", []):
        events.append({"type": "assistant", "message": {"content": blocks}})
    result = {"type": "result", "result": answer, "num_turns": 2,
              "usage": {"input_tokens": 10, "cache_read_input_tokens": 100, "cache_creation_input_tokens": 5,
                        "output_tokens": 7}}
    if "usd" in beh:
        result["total_cost_usd"] = beh["usd"]
    events.append(result)
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

# A fake `ont` of the host's store: it logs every call and answers from the state file of the test (tasks, kpis,
# measurements, the request ids it has applied), as the engine does for the actions the suite applies.
FAKE_ONT = r'''#!@PY@
import json, os, sys
state = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
path = os.path.join(state, "ont-state.json")
st = json.load(open(path))
argv = sys.argv[1:]
body = json.loads(sys.stdin.read() or "null")
with open(os.path.join(state, "ont.jsonl"), "a") as f:
    f.write(json.dumps({"argv": argv, "body": body}) + "\n")
def out(v):
    json.dump(st, open(path, "w"))
    print(json.dumps(v))
    sys.exit(0)
def obj(kind, key, fields):
    return {"id": {"object_type": kind, "namespace": {"instance": "vps://x", "account": "root"}, "native_key": key},
            "head": {"fields": {k: {"kind": "value", "value": v} for k, v in fields.items()}}}
def kid(k):
    return {"object_type": "kpi", "namespace": {"instance": "vps://x", "account": "root"}, "native_key": k["key"]}
if "--store" in argv:
    sys.exit(97)
if argv == ["search"]:
    kind, where = body["object_type"], body.get("where")
    if st.get("search_fails"):
        out({"status": "failed", "code": "store_unreadable"})
    if kind == "task":
        objs = [obj("task", t, {"title": "x"}) for t in st["tasks"]]
    else:
        objs = [dict(obj("kpi", k["key"], {"name": k["name"], "task": k["task"], "value": k.get("value")}))
                for k in st["kpis"] if where is None or k["name"] == where["eq"][1]]
    out({"status": "ok", "result": {"objects": objs, "basis": {"qualified": st.get("qualified", True)}}})
if argv[:2] == ["action", "apply"] and "--client" in argv:
    kind, rid, par = body["action_type"], body["request_id"], body["parameters"]
    if rid in st["requests"]:
        out({"status": "ok", "result": {"outcome": "already_applied", "edits": st["requests"][rid]}})
    if st.get("refuse") == kind:
        out({"status": "refused", "code": "submission_criteria_failed", "failed": [st.get("refuse_code", "x")]})
    if kind == "define_kpi":
        k = {"key": "kpi-%d" % (len(st["kpis"]) + 1), "name": par["name"], "task": par["task"], "definition": par["definition"]}
        st["kpis"].append(k)
        edits = [{"id": kid(k), "change": "initial"}]
    else:
        k = next(x for x in st["kpis"] if x["key"] == par["kpi"]["native_key"])
        if any(m["kpi"] == k["key"] and m["observed_at"] > par["observed_at"] for m in st["measurements"]):
            out({"status": "refused", "code": "submission_criteria_failed", "failed": ["older_measurement"]})
        st["measurements"].append({"kpi": k["key"], "value": par["value"], "observed_at": par["observed_at"],
                                   "evidence": par["evidence"], "request_id": rid, "agent": body.get("agent")})
        k["value"] = par["value"]
        edits = [{"id": {"object_type": "measurement", "native_key": "m%d" % len(st["measurements"])}, "change": "initial"}]
    st["requests"][rid] = edits
    st.setdefault("applied", []).append({"kind": kind, "rid": rid, "parameters": par})
    out({"status": "ok", "result": {"outcome": "applied", "edits": edits}})
sys.exit(98)
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
                           ("muse", FAKE_CLI), ("agy", FAKE_CLI), ("ont", FAKE_ONT)):
            (self.bin / name).write_text(text.replace("@PY@", PY))
            (self.bin / name).chmod(0o755)
        (t / "count").mkdir()
        (t / "pids").mkdir()
        self.root = t / "root"
        self.root.mkdir()
        (self.root / "AGENTS.md").write_text("agents\n")
        self.tmpdir = t / "tmp"  # the runner's TMPDIR: its work directories are made, and swept, here only
        self.tmpdir.mkdir()
        self.home = t / "home"
        self.acct = t / "acct"
        self.acct.mkdir()
        (self.home / ".claude-x" / "projects" / "-w-edu-lessons").mkdir(parents=True)
        (self.home / ".claude-x" / "projects" / "-w-other").mkdir(parents=True)
        self.secret = t / "secret-dir"
        self.secret.mkdir()
        self.out = t / "out"
        self.plan({})
        self.ont_state({})

    def close(self):
        for f in (self.t / "pids").iterdir() if (self.t / "pids").exists() else []:
            try:
                os.kill(int(f.name), signal.SIGKILL)
            except (ProcessLookupError, ValueError):
                pass
        self.tmp.cleanup()

    def plan(self, plan):
        (self.t / "plan.json").write_text(json.dumps(plan))

    def ont_state(self, state):
        """The host store of the fake `ont`: the keys of its tasks, and what the test asks it to do."""
        full = {"tasks": [], "kpis": [], "measurements": [], "requests": {}}
        full.update(state)
        (self.t / "ont-state.json").write_text(json.dumps(full))

    def ont_now(self):
        return json.loads((self.t / "ont-state.json").read_text())

    def env(self, **extra):
        env = {"PATH": str(self.bin), "HOME": str(self.home), "CLAUDE_CONFIG_DIR": str(self.acct),
               "TMPDIR": str(self.tmpdir)}
        env.update(extra)
        return {k: v for k, v in env.items() if v is not None}

    def call(self, verb, *args, env=None, umask=-1):
        r = subprocess.run([PY, "-I", "-B", str(self.run_path), verb, *map(str, args)], env=env or self.env(),
                           capture_output=True, text=True, timeout=120, umask=umask)
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

    def test_repository_cases_hold_their_form(self):
        root = RUN.parent.parent
        new = json.loads((root / "routing" / "cases.json").read_text())
        ids = [c["id"] for c in new]
        self.assertEqual(len(ids), 16)
        self.assertEqual(len(set(ids)), 16)
        for c in new:
            self.assertTrue(c["must"], c["id"])
            for fact in c["must"]:
                self.assertEqual(len(fact["fact"]), 1, c["id"])
                self.assertIsInstance(fact["fact"][0], str)
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


class Watch(unittest.TestCase):
    """The MCP watch reads the process tree of the call, or the unit's control group once systemd names it."""

    def setUp(self):
        self.w = World(self)

    def own_group(self):
        """The control group this test runs in (the fake `systemctl show` gives it as the unit's)."""
        try:
            line = [x for x in Path("/proc/self/cgroup").read_text().splitlines() if x.startswith("0::")][0]
            cg = line[3:]
            Path("/sys/fs/cgroup" + cg, "cgroup.procs").read_text()
        except (OSError, IndexError):
            self.skipTest("no readable cgroup v2 of this process")
        return cg

    def test_control_group_branch_sees_a_server_the_process_tree_does_not(self):
        w = self.w
        cg = self.own_group()
        # The server's parent ended, so it is no descendant of the call: only the control group holds it.
        w.plan({"control_group": cg, "cli": {"c1": [{"mode": "mcp_orphan", "answer": GOOD}], "c2": [{"answer": GOOD}]}})
        code, out, r = w.answers("--jobs", "1", "--samples", "1")
        self.assertEqual(code, 2, r.stdout + r.stderr)
        self.assertEqual(out["mcp_rule_broken"], ["c1__s1"])
        self.assertEqual(out["samples"]["c2__s1"], "not_run")
        unit = "routing-" + hashlib.sha256(str(w.sample("c1", 1)).encode()).hexdigest()[:16] + ".service"
        self.assertIn("show -p ControlGroup --value %s" % unit, (w.t / "systemctl.log").read_text())
        usage = json.loads(w.sample("c1", 1).with_suffix(".json").read_text())
        self.assertTrue(usage["mcp_processes"])
        # Without a group the tree is read, and finds nothing of the orphan: the same plan, no break.
        w2 = World(self)
        w2.plan({"cli": {"c1": [{"mode": "mcp_orphan", "answer": GOOD}], "c2": [{"answer": GOOD}]}})
        code, out, r = w2.answers("--jobs", "1", "--samples", "1")
        self.assertEqual((code, out["mcp_rule_broken"]), (0, []), r.stdout + r.stderr)


class Deny(unittest.TestCase):
    def test_the_fixed_deny_patterns(self):
        edu = "/home/palantirkc/platforms/education-platform"
        wt = "/home/palantirkc/work/*/wt/education-platform*"
        want = (["/home/palantirkc/work/edu-*"]
                + ["%s/%s" % (base, d) for base in (edu, wt) for d in ("context", "apps", "modules", "migration")]
                + ["~/.claude*/projects/*edu-*", "~/.claude*/projects/*education-platform*"])
        self.assertEqual(want, load_module().DENY)
        self.assertEqual(len(want), 11)


class WorkDirs(unittest.TestCase):
    """A call's work directories are `routing-<pid>-<start time>-<random>` in the TMPDIR, swept only when dead."""

    FORM = re.compile(r"routing-([0-9]+)-([0-9]+)-[a-z0-9_]+")

    def setUp(self):
        self.w = World(self)
        self.w.plan({"cli": {"c1": [{"answer": GOOD}], "c2": [{"answer": GOOD}]}})
        self.procs = []
        self.addCleanup(self.stop)

    def stop(self):
        for p in self.procs:
            p.kill()
            p.wait()

    def process(self):
        p = subprocess.Popen(["sleep", "300"])
        self.procs.append(p)
        mod = load_module()
        started = mod.start_time(p.pid)
        self.assertIsNotNone(started)
        return p, started

    def dead(self):
        """The identity of a process that has ended."""
        p, started = self.process()
        p.kill()
        p.wait()
        self.assertIsNone(load_module().start_time(p.pid))
        return p.pid, started

    def left(self):
        return sorted(x.name for x in self.w.tmpdir.iterdir())

    def test_a_directory_is_named_by_its_owner_the_call(self):
        w = self.w
        code, _, r = w.answers("--case", "c1", "--samples", "1")
        self.assertEqual(code, 0, r.stderr)
        [run] = w.cli_runs()
        work = Path(run["cwd"])
        self.assertEqual(work.parent.parent, w.tmpdir)
        m = self.FORM.fullmatch(work.parent.name)
        self.assertTrue(m, work.parent.name)
        self.assertIn([int(m.group(1)), int(m.group(2))], run["chain"], "the call that made it owns it")
        self.assertEqual(self.left(), [], "removed when the call ends")

    def test_only_directories_of_dead_owners_are_removed(self):
        w = self.w
        alive, started = self.process()
        gone_pid, gone_start = self.dead()
        made = {
            "dead": "routing-%d-%d-abcd1234" % (gone_pid, gone_start),
            "reused": "routing-%d-%d-abcd1235" % (alive.pid, started + 1),  # the pid lives, another process owns it
            "live": "routing-%d-%d-abcd1236" % (alive.pid, started),
            "no_owner": "routing-abcd1237",
            "no_start": "routing-%d-abcd1238" % gone_pid,
            "letters": "routing-%d-%d-ABCD" % (gone_pid, gone_start),
            "other": "other-%d-%d-abcd1239" % (gone_pid, gone_start),
            "after": "routing-%d-%d-abcd1240-more" % (gone_pid, gone_start),
        }
        for name in made.values():
            d = w.tmpdir / name
            d.mkdir()
            (d / "home").mkdir()
            (d / "home" / "token").symlink_to(w.secret)
        file_like = w.tmpdir / ("routing-%d-%d-abcd1241" % (gone_pid, gone_start))
        file_like.write_text("a file is not a directory")
        link = w.tmpdir / ("routing-%d-%d-abcd1242" % (gone_pid, gone_start))
        link.symlink_to(w.secret)
        code, _, r = w.answers("--case", "c1", "--samples", "1")
        self.assertEqual(code, 0, r.stderr)
        self.assertEqual(
            self.left(),
            sorted([made["live"], made["no_owner"], made["no_start"], made["letters"], made["other"], made["after"],
                    file_like.name, link.name]))
        self.assertTrue(w.secret.is_dir(), "a link is removed, never followed")
        self.assertEqual(os.kill(alive.pid, 0), None, "a live process is not touched")

    def test_a_refused_call_removes_nothing(self):
        w = self.w
        gone_pid, gone_start = self.dead()
        d = w.tmpdir / ("routing-%d-%d-abcd1234" % (gone_pid, gone_start))
        d.mkdir()
        code, out, _ = w.call("answers", "--root", w.root, "--reader", "claude-haiku", "--out", w.out, "--case", "nope")
        self.assertEqual((code, out), (2, {"status": "case_unknown"}))
        self.assertTrue(d.is_dir())

    def test_a_killed_call_of_the_agy_reader_leaves_its_home_until_the_next_call_sweeps_it(self):
        w = self.w
        token = w.home / ".gemini" / "antigravity-cli"
        token.mkdir(parents=True)
        (token / "antigravity-oauth-token").write_text("not a token")
        w.plan({"cli": {"c1": [{"mode": "sleep", "seconds": 60, "answer": GOOD}], "c2": [{"answer": GOOD}]}})
        p = subprocess.Popen([PY, "-I", "-B", str(w.run_path), "answers", "--root", str(w.root), "--reader",
                              "agy-hydrogen", "--out", str(w.out), "--samples", "1", "--jobs", "1", "--case", "c1"],
                             env=w.env(), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        deadline = time.time() + 60
        while not w.cli_runs() and time.time() < deadline:
            time.sleep(0.1)
        self.assertTrue(w.cli_runs(), "the session started")
        p.kill()
        p.wait()
        for f in (w.t / "pids").iterdir():
            try:
                os.kill(int(f.name), signal.SIGKILL)
            except ProcessLookupError:
                pass
        [left] = self.left()
        self.assertTrue(self.FORM.fullmatch(left), left)
        self.assertTrue((w.tmpdir / left / "home" / ".gemini" / "antigravity-cli" / "antigravity-oauth-token").is_symlink())
        w.plan({"cli": {"c1": [{"answer": GOOD}]}})
        code, _, r = w.answers("--samples", "1", "--case", "c1", "--jobs", "1", reader="agy-hydrogen")
        self.assertEqual(code, 0, r.stdout + r.stderr)
        self.assertEqual(self.left(), [], "no routing directory, and no link to the token, is left")
        self.assertEqual((token / "antigravity-oauth-token").read_text(), "not a token")

    def test_the_event_stream_and_the_error_file_are_private(self):
        w = self.w
        code, _, r = w.call("answers", "--root", w.root, "--reader", "claude-haiku", "--out", w.out, "--samples", "1",
                            umask=0)
        self.assertEqual(code, 0, r.stderr)
        for cid in ("c1", "c2"):
            for suffix in (".jsonl", ".err", ".md", ".json"):
                mode = stat.S_IMODE(w.sample(cid, 1).with_suffix(suffix).stat().st_mode)
                self.assertEqual(mode, 0o600, "%s%s" % (cid, suffix))
        # A file of an earlier try, with a wider mode, is private again.
        w.sample("c2", 1).with_suffix(".md").unlink()
        w.sample("c2", 1).with_suffix(".json").unlink()
        w.sample("c2", 1).with_suffix(".jsonl").chmod(0o644)
        code, _, r = w.call("answers", "--root", w.root, "--reader", "claude-haiku", "--out", w.out, "--samples", "1",
                            umask=0)
        self.assertEqual(code, 0, r.stderr)
        self.assertEqual(stat.S_IMODE(w.sample("c2", 1).with_suffix(".jsonl").stat().st_mode), 0o600)


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


def fake_reader(d, n, table, tokens=None, extra=None):
    """A reader directory by hand: table is case -> list of n answers (None: unanswered); extra gives the members of
    a sample's usage record beyond the answered flag and the tokens, as (case, sample) -> dict."""
    d.mkdir(parents=True)
    (d / "run.json").write_text(json.dumps({"samples": n, "cases": sorted(table), "case_files": "@FILES@", "suite": "s" * 64}))
    for cid, answers in table.items():
        for i, ans in enumerate(answers, 1):
            if ans is None:
                continue
            (d / ("%s__s%d.md" % (cid, i))).write_text(ans)
            (d / ("%s__s%d.json" % (cid, i))).write_text(json.dumps(
                dict({"answered": True, "input_tokens": (tokens or {}).get((cid, i), 100)}, **(extra or {}).get((cid, i), {}))))


def case_files(w, d):
    """The digests of the case files put into each hand-made reader's run record."""
    m = load_module()
    m.REPO_CASES = str(w.run_path.parent / "cases.json")
    m.HOST_DIGESTS = str(w.run_path.parent / "host-cases.sha256")
    _, digests = m.load_cases([str(w.host)])
    for r in d.iterdir():
        if r.is_dir():
            rec = r / "run.json"
            rec.write_text(rec.read_text().replace('"@FILES@"', json.dumps(digests)))


class Score(unittest.TestCase):
    def setUp(self):
        self.w = World(self)

    def files(self, d):
        case_files(self.w, d)

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


def tool_use(tid, command):
    return {"type": "tool_use", "id": tid, "name": "Bash", "input": {"command": command, "description": "d"}}


ROUTE_CMD = """echo '{"parameters":{"words":"where"}}' | ont fn route"""


class SampleRecord(unittest.TestCase):
    """The members of a sample's usage record: its start and end, its calls of the functions, its cost."""
    def setUp(self):
        self.w = World(self)

    def usage(self, cid, n=1, reader="claude-haiku"):
        return json.loads(self.w.sample(cid, n, reader).with_suffix(".json").read_text())

    def test_fn_calls_count_each_tool_use_once_by_its_id_and_are_null_for_the_other_readers(self):
        w = self.w
        stream = [[tool_use("t1", ROUTE_CMD)],
                  [tool_use("t1", ROUTE_CMD), tool_use("t2", "ap status --brief nightly-backup")],  # t1 again
                  [tool_use("t3", "ls /tmp"), {"type": "text", "text": "ont fn route"}],
                  [tool_use("t4", "cd / && ap fn route < /dev/null"), tool_use("t5", "ap status --brief x | head")],
                  [{"type": "tool_use", "name": "Bash", "input": {"command": ROUTE_CMD}}]]  # no id: not counted
        w.plan({"cli": {"c1": [{"answer": GOOD, "tool_use": stream}], "c2": [{"answer": GOOD}]}})
        for reader in ("claude-haiku", "claude-sonnet", "muse-ember", "agy-hydrogen"):
            self.assertEqual(w.answers("--samples", "1", reader=reader)[0], 0, reader)
        self.assertEqual(self.usage("c1")["fn_calls"], {"route": 2, "brief": 2})
        self.assertEqual(self.usage("c1", reader="claude-sonnet")["fn_calls"], {"route": 2, "brief": 2})
        self.assertEqual(self.usage("c2")["fn_calls"], {"route": 0, "brief": 0}, "a Claude reader that called none")
        for reader in ("muse-ember", "agy-hydrogen"):
            for cid in ("c1", "c2"):
                self.assertIsNone(self.usage(cid, reader=reader)["fn_calls"], reader)

    def test_fn_calls_read_the_stream_by_the_function(self):
        m = load_module()
        stream = "\n".join(json.dumps(e) for e in [
            {"type": "assistant", "message": {"content": [tool_use("a", ROUTE_CMD)]}},
            {"type": "assistant", "message": {"content": [tool_use("a", ROUTE_CMD)]}},
            {"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "a", "content": "ont fn route"}]}},
            {"type": "assistant", "message": {"content": "ap status --brief"}}]) + "\nnot json\n"
        self.assertEqual(m.fn_calls_of("claude-haiku", stream), {"route": 1, "brief": 0})
        self.assertIsNone(m.fn_calls_of("muse-ember", stream))
        self.assertIsNone(m.fn_calls_of("agy-hydrogen", stream))

    def test_started_at_and_ended_at_are_utc_seconds_of_the_unit_that_started(self):
        w = self.w
        w.plan({"cli": {"c1": [{"answer": GOOD}], "c2": [{"answer": GOOD}]}})
        before = int(time.time())
        self.assertEqual(w.answers("--samples", "1", "--case", "c1")[0], 0)
        after = int(time.time()) + 1
        u = self.usage("c1")
        for k in ("started_at", "ended_at"):
            self.assertRegex(u[k], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")
        at = lambda t: int(time.mktime(time.strptime(t, "%Y-%m-%dT%H:%M:%SZ"))) - time.timezone
        self.assertTrue(before - 1 <= at(u["started_at"]) <= at(u["ended_at"]) <= after, u)

    def test_started_at_is_null_for_a_unit_that_did_not_start(self):
        w = self.w
        w.plan({"systemd_run": "fail_sessions"})
        code, out, _ = w.answers("--jobs", "1", "--case", "c1", "--samples", "1")
        self.assertEqual((code, out), (2, {"status": "sandbox_unavailable"}))
        u = self.usage("c1")
        self.assertEqual((u["state"], u["started_at"], u["ended_at"], u["fn_calls"], u["usd"]),
                         ("sandbox_unavailable", None, None, None, None))

    def test_usd_is_the_total_cost_of_the_last_result_of_a_claude_stream(self):
        w = self.w
        w.plan({"cli": {"c1": [{"answer": GOOD, "usd": 0.0123}], "c2": [{"answer": GOOD}]}})
        for reader in ("claude-haiku", "muse-ember", "agy-hydrogen"):
            self.assertEqual(w.answers("--samples", "1", reader=reader)[0], 0, reader)
        self.assertEqual(self.usage("c1")["usd"], 0.0123)
        self.assertIsNone(self.usage("c2")["usd"], "absent in the stream")
        for reader in ("muse-ember", "agy-hydrogen"):
            self.assertIsNone(self.usage("c1", reader=reader)["usd"], reader)

    def test_usd_of_takes_the_last_result_and_nothing_that_is_not_a_number(self):
        m = load_module()
        line = lambda **kw: json.dumps(dict({"type": "result", "result": "x"}, **kw))
        self.assertEqual(m.usd_of("claude-haiku", line(total_cost_usd=1) + "\n" + line(total_cost_usd=0.5)), 0.5)
        self.assertIsNone(m.usd_of("claude-haiku", line(total_cost_usd=1) + "\n" + line()))
        self.assertIsNone(m.usd_of("claude-haiku", line(total_cost_usd="1.0")))
        self.assertIsNone(m.usd_of("claude-haiku", line(total_cost_usd=True)))
        self.assertIsNone(m.usd_of("muse-ember", line(total_cost_usd=1)))

    def test_the_prompt_names_the_functions_in_a_sentence_after_its_first_paragraph(self):
        w = self.w
        w.plan({"cli": {"c1": [{"answer": GOOD}]}})
        self.assertEqual(w.answers("--samples", "1", "--case", "c1")[0], 0)
        prompt = w.cli_runs()[0]["argv"][-1]
        first, second, third = prompt.split("\n\n", 2)
        self.assertEqual(first, "You are an agent on this host. Your assignment: CASEID c1 SITUATION-SECRET-ONE")
        for word in ("ont fn route", '{"parameters":{"words":"<what you look for>"}}', "ap status --brief <intent>"):
            self.assertIn(word, second)
        self.assertTrue(third.startswith("Before you act, find the context"), third)
        self.assertEqual(second.count(". "), 0, "one sentence")


class ScoreMembers(unittest.TestCase):
    def setUp(self):
        self.w = World(self)

    def test_score_usd_and_fn_samples_per_reader(self):
        w = self.w
        both = {"input_tokens": 100}
        table = {"c1": [GOOD] * 3, "c2": [GOOD] * 3, "h1": ["FACT-HOST"] * 3}
        extra = {}
        for cid in table:
            for i in (1, 2, 3):
                extra[(cid, i)] = {"fn_calls": {"route": 1 if i == 1 else 0, "brief": 0}, "usd": 0.25}
        extra[("c1", 3)] = {"fn_calls": None, "usd": None}  # no count, no cost: not in either figure
        extra[("c2", 3)] = {"fn_calls": {"route": 0, "brief": 2}, "usd": 0.1}
        fake_reader(w.out / "claude-sonnet", 3, table, extra=extra)
        fake_reader(w.out / "muse-ember", 3, table)
        fake_reader(w.out / "claude-haiku", 3, {"c1": [GOOD, None, GOOD], "c2": [GOOD] * 3, "h1": [GOOD] * 3},
                    extra={("c1", 1): {"fn_calls": {"route": 0, "brief": 0}, "usd": 1.234}})
        # a sample that is not answered holds no figure of the score, whatever its record says
        (w.out / "claude-haiku" / "c1__s2.json").write_text(json.dumps(
            {"answered": False, "usd": 50.0, "fn_calls": {"route": 3, "brief": 3}}))
        case_files(w, w.out)
        code, s, r = w.call("score", "--out", w.out, "--cases", w.host)
        self.assertEqual(code, 0, r.stdout)
        son, mus, hai = (s["readers"][k] for k in ("claude-sonnet", "muse-ember", "claude-haiku"))
        self.assertEqual((son["fn_samples"], son["usd"]), (0.5, 1.85))  # 8 counts, 4 with a call; 7 x 0.25 and 0.1
        self.assertEqual((mus["fn_samples"], mus["usd"]), (None, None))
        self.assertEqual((hai["fn_samples"], hai["usd"]), (0.0, 1.23))
        self.assertEqual(json.loads((w.out / "score.json").read_text()), s)
        for k in ("n", "passes", "samples", "mean", "majority", "unanswered", "tokens_median"):
            self.assertIn(k, son)
        self.assertEqual(set(s), {"suite", "case_files", "readers", "parity"})
        self.assertEqual(set(son), {"n", "passes", "samples", "mean", "majority", "unanswered", "tokens_median",
                                    "fn_samples", "usd"})

    def test_equal_answers_give_an_equal_score_file(self):
        w = self.w
        fake_reader(w.out / "claude-haiku", 3, {"c1": [GOOD] * 3, "c2": [GOOD] * 3, "h1": ["x"] * 3},
                    extra={(c, 1): {"started_at": "2026-11-20T10:00:00Z", "ended_at": "2026-11-20T10:01:00Z",
                                    "usd": 0.5, "fn_calls": {"route": 1, "brief": 0}} for c in ("c1", "c2", "h1")})
        case_files(w, w.out)
        self.assertEqual(w.call("score", "--out", w.out, "--cases", w.host)[0], 0)
        first = (w.out / "score.json").read_bytes()
        self.assertEqual(w.call("score", "--out", w.out, "--cases", w.host)[0], 0)
        self.assertEqual((w.out / "score.json").read_bytes(), first)
        self.assertNotIn(b"2026-11-20", first)

    def test_latest_end_is_of_the_scored_samples_only(self):
        w = self.w
        fake_reader(w.out / "claude-haiku", 3, {"c1": [GOOD] * 3},
                    extra={("c1", 1): {"ended_at": "2026-11-20T10:00:00Z"}, ("c1", 2): {"ended_at": "2026-11-20T10:05:00Z"},
                           ("c1", 3): {"ended_at": None}})
        (w.out / "claude-haiku" / "c1__s3.json").write_text(json.dumps({"answered": False, "ended_at": "2026-12-01T00:00:00Z"}))
        m = load_module()
        rec = json.loads((w.out / "claude-haiku" / "run.json").read_text())
        self.assertEqual(m.latest_end(str(w.out / "claude-haiku"), rec), "2026-11-20T10:05:00Z")
        rec["cases"] = []
        self.assertIsNone(m.latest_end(str(w.out / "claude-haiku"), rec))


KEY = "task-suite"


class ScoreRecord(unittest.TestCase):
    """`score --record <task key>`: one measurement per reader of the KPI of that reader, through the `ont` of PATH."""
    def setUp(self):
        self.w = w = World(self)
        w.ont_state({"tasks": [KEY, "other-task"]})
        ends = {(c, i): "2026-11-20T10:00:0%dZ" % i for c in ("c1", "c2", "h1") for i in (1, 2, 3)}
        sonnet = {k: {"ended_at": e, "usd": 0.25, "fn_calls": {"route": 1, "brief": 0}} for k, e in ends.items()}
        fake_reader(w.out / "claude-sonnet", 3, {"c1": [GOOD, GOOD, "FACT-ALPHA one only"], "c2": [GOOD] * 3,
                                                  "h1": ["FACT-HOST", "x", "x"]}, extra=sonnet)
        fake_reader(w.out / "claude-haiku", 3, {"c1": [GOOD] * 3, "c2": [GOOD] * 3, "h1": ["x"] * 3},
                    extra={k: {"ended_at": "2026-11-20T11:00:00Z"} for k in ends})
        fake_reader(w.out / "muse-ember", 3, {"c1": [GOOD] * 3, "c2": [GOOD] * 3, "h1": ["x"] * 3})  # no ended_at
        case_files(w, w.out)

    def score(self, *extra):
        return self.w.call("score", "--out", self.w.out, "--cases", self.w.host, *extra)

    def calls(self):
        return self.w.lines("ont.jsonl")

    def test_score_record_defines_each_kpi_once_and_records_one_measurement_per_reader(self):
        w = self.w
        code, out, r = self.score("--record", KEY)
        self.assertEqual((code, r.stderr), (0, ""), r.stdout)
        self.assertEqual(out["recorded"], {"measurements": 2, "already": 0, "older": 0, "skipped": 1, "failed": 0})
        written = json.loads((w.out / "score.json").read_text())
        self.assertNotIn("recorded", written, "the counts are printed, not written")
        self.assertEqual({k: v for k, v in out.items() if k != "recorded"}, written)
        digest = hashlib.sha256((w.out / "score.json").read_bytes()).hexdigest()
        st = w.ont_now()
        self.assertEqual(sorted(k["name"] for k in st["kpis"]), ["suite routing claude-haiku", "suite routing claude-sonnet"])
        for k in st["kpis"]:
            reader = k["name"].split()[-1]
            self.assertEqual(k["task"]["native_key"], KEY)
            self.assertEqual(k["definition"], "Score of the routing suite for the reader %s in one scored run: mean share "
                             "of passed cases, majority over all cases, samples per case, unanswered samples, share of "
                             "answered samples that called a function, and the readers' own cost in USD; recorded by "
                             "evals/routing/run score --record. No case text." % reader)
        defines = [a for a in st["applied"] if a["kind"] == "define_kpi"]
        self.assertEqual([a["rid"] for a in defines], [
            "suite-kpi-" + hashlib.sha256(("%s:suite routing %s" % (KEY, k)).encode()).hexdigest()[:32]
            for k in ("claude-sonnet", "claude-haiku")])
        want = {"claude-sonnet": ("mean 2.0000 majority 2/3 n 3 unanswered 0 fn 1.0000 usd 2.25", "2026-11-20T10:00:03Z"),
                "claude-haiku": ("mean 2.0000 majority 2/3 n 3 unanswered 0 fn n/a usd n/a", "2026-11-20T11:00:00Z")}
        by_kpi = {k["key"]: k["name"].split()[-1] for k in st["kpis"]}
        self.assertEqual(len(st["measurements"]), 2)
        for m in st["measurements"]:
            reader = by_kpi[m["kpi"]]
            self.assertEqual((m["value"], m["observed_at"]), want[reader])
            self.assertEqual(m["evidence"], "evals/routing/run score sha256:" + digest)
            self.assertEqual(m["request_id"], "suite-" + hashlib.sha256(("%s:%s" % (reader, digest)).encode()).hexdigest()[:32])
            self.assertLessEqual(len(m["value"].encode()), 200)
        self.assertEqual(sorted(a["kind"] for a in st["applied"]), ["define_kpi"] * 2 + ["record_measurement"] * 2)
        everything = json.dumps(st["applied"])
        for private in ("SITUATION", "FACT-", "CASEID", str(w.t), "fixture-host", "host-cases"):
            self.assertNotIn(private, everything)
        for c in self.calls():
            self.assertNotIn("--store", c["argv"])
            if c["argv"][0] == "action":
                self.assertEqual(c["argv"], ["action", "apply", "--client", "suite"])

    def test_score_record_a_second_score_of_the_same_answers_applies_nothing_new(self):
        w = self.w
        self.assertEqual(self.score("--record", KEY)[0], 0)
        before = json.dumps(w.ont_now()["applied"])
        code, out, r = self.score("--record", KEY)
        self.assertEqual((code, r.stderr), (0, ""))
        self.assertEqual(out["recorded"], {"measurements": 0, "already": 2, "older": 0, "skipped": 1, "failed": 0})
        st = w.ont_now()
        self.assertEqual(json.dumps(st["applied"]), before)
        self.assertEqual((len(st["kpis"]), len(st["measurements"])), (2, 2))
        self.assertEqual([c["body"]["action_type"] for c in self.calls() if c["argv"][0] == "action"].count("define_kpi"), 2)

    def test_score_record_a_kpi_of_another_task_is_not_used(self):
        w = self.w
        other = {"object_type": "task", "native_key": "other-task"}
        w.ont_state({"tasks": [KEY, "other-task"], "kpis": [
            {"key": "kpi-x", "name": "suite routing claude-sonnet", "task": other}]})
        code, out, _ = self.score("--record", KEY)
        self.assertEqual((code, out["recorded"]["failed"]), (0, 0))
        st = w.ont_now()
        self.assertEqual(sorted((k["task"]["native_key"], k["name"]) for k in st["kpis"]), sorted([
            ("other-task", "suite routing claude-sonnet"), (KEY, "suite routing claude-sonnet"),
            (KEY, "suite routing claude-haiku")]))
        self.assertNotIn("kpi-x", [m["kpi"] for m in st["measurements"]])

    def test_score_record_an_older_measurement_is_counted_and_is_no_failure(self):
        w = self.w
        self.assertEqual(self.score("--record", KEY)[0], 0)
        for i in (1, 2, 3):  # other answers, so another score file; one reader's samples ended before its measurement
            (w.out / "claude-haiku" / ("c1__s%d.md" % i)).write_text("x")
            meta = json.loads((w.out / "claude-haiku" / ("c1__s%d.json" % i)).read_text())
            (w.out / "claude-haiku" / ("c1__s%d.json" % i)).write_text(json.dumps(dict(meta, ended_at="2026-11-19T00:00:00Z")))
        for c in ("c2", "h1"):
            for i in (1, 2, 3):
                p = w.out / "claude-haiku" / ("%s__s%d.json" % (c, i))
                p.write_text(json.dumps(dict(json.loads(p.read_text()), ended_at="2026-11-19T00:00:00Z")))
        code, out, r = self.score("--record", KEY)
        self.assertEqual((code, r.stderr), (0, ""), r.stdout)
        self.assertEqual(out["recorded"], {"measurements": 1, "already": 0, "older": 1, "skipped": 1, "failed": 0})

    def test_score_record_a_refusal_or_an_unknown_task_is_a_failure_with_exit_3(self):
        w = self.w
        code, out, _ = self.score("--record", "no-such-task")
        self.assertEqual((code, out["recorded"]["failed"], out["recorded"]["measurements"]), (3, 3, 0))
        self.assertEqual([c for c in self.calls() if c["argv"][0] == "action"], [], "no KPI for a task the store lacks")
        w.ont_state({"tasks": [KEY], "refuse": "record_measurement", "refuse_code": "bad"})
        code, out, _ = self.score("--record", KEY)
        self.assertEqual((code, out["recorded"]), (3, {"measurements": 0, "already": 0, "older": 0, "skipped": 1, "failed": 2}))
        w.ont_state({"tasks": [KEY], "search_fails": True})
        code, out, _ = self.score("--record", KEY)
        self.assertEqual((code, out["recorded"]["failed"]), (3, 3))
        self.assertTrue((w.out / "score.json").exists(), "the score is written whatever the record does")

    def test_score_record_uses_an_answer_that_is_not_qualified(self):
        w = self.w
        w.ont_state({"tasks": [KEY], "qualified": False})
        code, out, _ = self.score("--record", KEY)
        self.assertEqual((code, out["recorded"]["measurements"]), (0, 2))

    def test_score_record_without_the_option_starts_no_ont_and_an_empty_one_is_usage(self):
        w = self.w
        code, out, _ = self.score()
        self.assertEqual(code, 0)
        self.assertNotIn("recorded", out)
        self.assertEqual(w.call("gate", "--baseline", w.out / "score.json", "--candidate", w.out / "score.json")[0], 0)
        self.assertEqual(w.answers("--cases", w.host, "--samples", "3", "--case", "c1", "--case", "h1", reader="muse-ember")[0], 2)
        self.assertEqual(self.calls(), [], "no `ont` call without --record")
        code, out, _ = self.score("--record", "")
        self.assertEqual((code, out), (2, {"status": "usage"}))
        self.assertEqual(self.calls(), [])

    def test_score_record_never_reaches_the_engine_ont_bin_names(self):
        w = self.w
        trap = w.t / "engine"
        trap.write_text("#!/bin/sh\necho reached >> %s/engine.log\nexit 99\n" % w.t)
        trap.chmod(0o755)
        for extra in ((), ("--record", KEY)):
            self.assertEqual(w.call("score", "--out", w.out, "--cases", w.host, *extra, env=w.env(ONT_BIN=str(trap)))[0], 0)
        self.assertFalse((w.t / "engine.log").exists())
        self.assertTrue(self.calls())


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
