"""Tests of evals/batch/queue, the queue of the API credit, and of the unit files that run it, against a delivery
clone made by `git init` whose fake builder and reader are committed at K, and a fake `ap` that answers `--version`
and records `api batch` (no engine, no network, no cargo, no store or policy of the host). Contract:
evals/batch/README.md, section "The queue"; deploy/README.md, section "The API queue"."""
import json
import os
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
QUEUE = REPO / "evals" / "batch" / "queue"
DEPLOY = REPO / "deploy"
PY = sys.executable
HOME = os.environ.get("HOME", "/")
GIT_ID = ["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "-c", "commit.gpgsign=false",
          "-c", "init.defaultBranch=main"]
AP_REPO = "park-kyungchan/lifecycle-platform"

# The fake programs of the clone. Each appends one line to FAKE_LOG: which program, its arguments, its own path and
# its marker, so that a test sees what ran and from which copy.
FAKE_BUILD = r'''#!@PY@ -I
import json, os, sys
MARKER = "committed"
args = sys.argv[1:]
with open(os.environ["FAKE_LOG"], "a") as f:
    f.write(json.dumps({"program": "build", "argv": args, "file": os.path.abspath(__file__), "marker": MARKER}) + "\n")
out = args[args.index("--out") + 1]
os.makedirs(out, mode=0o700)
n = int(os.environ.get("FAKE_REQUESTED", "2"))
with open(os.path.join(out, "requests.jsonl"), "w") as f:
    for i in range(n):
        f.write(json.dumps({"custom_id": "g%d" % i, "params": {}}) + "\n")
with open(os.path.join(out, "groups.json"), "w") as f:
    json.dump({"job": "drift", "groups": []}, f)
print(json.dumps({"status": "ok", "out": out, "groups": n, "requested": n, "settled": 0}))
'''
FAKE_READ = r'''#!@PY@ -I
import json, os, sys
with open(os.environ["FAKE_LOG"], "a") as f:
    f.write(json.dumps({"program": "read", "argv": sys.argv[1:], "file": os.path.abspath(__file__),
                        "ont": os.environ.get("ONT_BIN")}) + "\n")
print(json.dumps({"status": "ok", "lines": 2, "read": 2, "opened": [], "over_cap": 0}))
'''
FAKE_AP = r'''#!@PY@ -I
import json, os, sys
args = sys.argv[1:]
if args == ["--version"]:
    print(os.environ["FAKE_VERSION"])
    sys.exit(0)
with open(os.environ["FAKE_LOG"], "a") as f:
    f.write(json.dumps({"program": "ap", "argv": args}) + "\n")
if os.environ.get("FAKE_BATCH") == "budget":
    sys.stderr.write(json.dumps({"error": {"code": "budget_exceeded", "field": "month",
        "detail": {"cap_usd_micros": 1, "counted_usd_micros": 1, "worst_usd_micros": 1}}}) + "\n")
    sys.exit(2)
out = args[args.index("--out") + 1]
os.makedirs(out, mode=0o700)
with open(os.path.join(out, "results.jsonl"), "w") as f:
    f.write("")
print(json.dumps({"job": "drift", "run": "run-1", "batch": "msgbatch_1", "requests": 2, "out": out}))
'''


def git(cwd, *words):
    return subprocess.run(["git", *GIT_ID, "-C", str(cwd), *words], check=True, capture_output=True,
                          text=True).stdout.strip()


def write(path, text, mode=0o644):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text.replace("@PY@", PY))
    path.chmod(mode)


class Queue(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="ap-queue-test-"))
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.clone = self.tmp / "deliver" / "lifecycle-platform"
        self.clone.mkdir(parents=True)
        git(self.clone, "init", "-q")
        write(self.clone / "evals" / "batch" / "lib.py", "# lib\n")
        write(self.clone / "evals" / "batch" / "drift" / "build", FAKE_BUILD, 0o755)
        write(self.clone / "evals" / "batch" / "drift" / "read", FAKE_READ, 0o755)
        write(self.clone / "README.md", "outside evals/batch\n")
        git(self.clone, "add", "-A")
        git(self.clone, "commit", "-q", "-m", "K")
        self.k = git(self.clone, "rev-parse", "HEAD")
        self.ap = self.tmp / "bin" / "ap"
        write(self.ap, FAKE_AP, 0o755)
        self.state = self.tmp / "state"
        self.log = self.tmp / "log.jsonl"
        self.policy = self.tmp / "policy.json"
        self.clones = {AP_REPO: "/invented/platforms/lifecycle-platform"}
        self.set_policy({"repositories": {name: {"clone": c} for name, c in self.clones.items()}})
        self.version = f"ap 0.1.0 {self.k}"

    def set_policy(self, value):
        self.policy.write_text(value if isinstance(value, str) else json.dumps(value))

    def run_queue(self, *argv, now="2026-11-23T18:00:00Z", **extra):
        env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "HOME": HOME, "AP_BIN": str(self.ap),
               "AP_DELIVER_CLONE": str(self.clone), "AP_API_STATE": str(self.state),
               "AP_POLICY_FILE": str(self.policy), "AP_QUEUE_NOW": now, "FAKE_LOG": str(self.log),
               "FAKE_VERSION": self.version, "ONT_BIN": "/invented/ont"}
        env.update(extra)
        done = subprocess.run([PY, "-I", "-B", str(QUEUE), *argv], env=env, capture_output=True, text=True,
                              timeout=120)
        self.assertEqual(done.stderr, "", "standard error stays empty")
        lines = done.stdout.splitlines()
        self.assertEqual(len(lines), 1, done.stdout)
        return json.loads(lines[0]), done.returncode

    def calls(self):
        if not self.log.exists():
            return []
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def of(self, program):
        return [c for c in self.calls() if c["program"] == program]

    def earlier(self, name):
        results = self.state / "drift" / name / "batch" / "results.jsonl"
        results.parent.mkdir(parents=True)
        results.write_text("")
        return str(results)

    def test_nightly_gives_earlier_results(self):
        first = self.earlier("20261120T180000Z-nightly")
        second = self.earlier("20261122T060000Z-weekly")
        (self.state / "drift" / "20261121T180000Z-nightly" / "build").mkdir(parents=True)
        v, code = self.run_queue("--pass", "nightly", "--model", "claude-sonnet-5-5")
        self.assertEqual(code, 0, v)
        pass_dir = str(self.state / "drift" / "20261123T180000Z-nightly")
        self.assertEqual(stat.S_IMODE(os.stat(pass_dir).st_mode), 0o700)
        self.assertEqual(set(v), {"status", "pass", "model", "queue", "commit", "dir", "build", "batch", "read"})
        self.assertEqual((v["status"], v["pass"], v["model"], v["queue"], v["commit"], v["dir"]),
                         ("ok", "nightly", "claude-sonnet-5-5", "standard", self.k, pass_dir))
        self.assertEqual(v["build"]["exit"], 0)
        self.assertEqual(v["build"]["answer"]["requested"], 2)
        self.assertEqual(v["batch"]["exit"], 0)
        self.assertEqual(v["batch"]["answer"]["run"], "run-1")
        self.assertEqual(v["read"], {"exit": 0, "answer": {"status": "ok", "lines": 2, "read": 2, "opened": [],
                                                          "over_cap": 0}})
        build, = self.of("build")
        self.assertEqual(build["argv"], [
            "--repo", f"{AP_REPO}={self.clones[AP_REPO]}",
            "--model", "claude-sonnet-5-5", "--out", pass_dir + "/build", "--results", first, "--results", second])
        ap, = self.of("ap")
        self.assertEqual(ap["argv"], ["api", "batch", "drift", "--requests", pass_dir + "/build/requests.jsonl",
                                      "--out", pass_dir + "/batch", "--queue", "standard"])
        read, = self.of("read")
        self.assertEqual(read["argv"], ["--groups", pass_dir + "/build/groups.json",
                                        "--results", pass_dir + "/batch/results.jsonl"])
        self.assertEqual(read["ont"], "/invented/ont", "ONT_BIN reaches the reader unchanged")
        self.assertEqual([c["program"] for c in self.calls()], ["build", "ap", "read"])
        # The next night gives this pass's results too.
        self.log.unlink()
        v, code = self.run_queue("--pass", "nightly", "--model", "claude-sonnet-5-5", now="2026-11-24T18:00:00Z")
        self.assertEqual(code, 0, v)
        results = [a for a in self.of("build")[0]["argv"] if a.endswith("results.jsonl")]
        self.assertEqual(results, [first, second, pass_dir + "/batch/results.jsonl"])

    def test_weekly_is_full(self):
        self.earlier("20261120T180000Z-nightly")
        v, code = self.run_queue("--model", "claude-opus-5-5", "--pass", "weekly", now="2026-11-22T06:00:00Z")
        self.assertEqual(code, 0, v)
        self.assertEqual((v["pass"], v["model"], v["status"]), ("weekly", "claude-opus-5-5", "ok"))
        pass_dir = str(self.state / "drift" / "20261122T060000Z-weekly")
        build, = self.of("build")
        self.assertNotIn("--results", build["argv"])
        self.assertEqual(build["argv"][-4:], ["--model", "claude-opus-5-5", "--out", pass_dir + "/build"])
        self.assertEqual(len(self.of("ap")), 1)

    def test_soak_in_the_last_six_days(self):
        cases = [("2026-11-24T18:00:00Z", "standard"), ("2026-11-25T18:00:00Z", "soak"),
                 ("2026-11-30T23:59:59Z", "soak"), ("2026-12-01T00:00:00Z", "standard"),
                 ("2026-12-26T18:00:00Z", "soak"), ("2028-02-23T18:00:00Z", "standard"),
                 ("2028-02-24T18:00:00Z", "soak"), ("2027-02-23T18:00:00Z", "soak")]
        for now, word in cases:
            with self.subTest(now=now):
                if self.log.exists():
                    self.log.unlink()
                v, code = self.run_queue("--pass", "nightly", "--model", "claude-sonnet-5-5", now=now)
                self.assertEqual((code, v["queue"]), (0, word), v)
                ap, = self.of("ap")
                self.assertEqual(ap["argv"][-2:], ["--queue", word])

    def test_no_request_no_batch(self):
        v, code = self.run_queue("--pass", "nightly", "--model", "claude-sonnet-5-5", FAKE_REQUESTED="0")
        self.assertEqual(code, 0, v)
        self.assertEqual((v["status"], v["batch"], v["read"]), ("ok", None, None))
        self.assertEqual(v["build"]["answer"]["requested"], 0)
        self.assertEqual([c["program"] for c in self.calls()], ["build"])

    def test_refused_batch_no_read(self):
        v, code = self.run_queue("--pass", "weekly", "--model", "claude-opus-5-5", FAKE_BATCH="budget")
        self.assertEqual(code, 1, v)
        self.assertEqual((v["status"], v["read"]), ("failed", None))
        self.assertEqual(v["batch"]["exit"], 2)
        self.assertEqual(v["batch"]["answer"]["error"]["code"], "budget_exceeded")
        self.assertEqual([c["program"] for c in self.calls()], ["build", "ap"])

    def test_not_installed_commit(self):
        other = "0" * 40
        for version in [f"ap 0.1.0 {other}", "ap 0.1.0", "not ap"]:
            with self.subTest(version=version):
                self.version = version
                v, code = self.run_queue("--pass", "nightly", "--model", "claude-sonnet-5-5")
                self.assertEqual((v, code), ({"status": "not_installed"}, 3))
                self.assertFalse(self.state.exists(), "nothing is written")
        # The clone's HEAD moved past the commit the installed ap names.
        self.version = f"ap 0.1.0 {self.k}"
        write(self.clone / "evals" / "batch" / "lib.py", "# lib, later\n")
        git(self.clone, "commit", "-q", "-am", "later")
        v, code = self.run_queue("--pass", "nightly", "--model", "claude-sonnet-5-5")
        self.assertEqual((v, code), ({"status": "not_installed"}, 3))
        self.assertFalse(self.state.exists())
        self.assertEqual(self.calls(), [])

    def test_programs_of_the_installed_commit(self):
        # A working tree changed after K is not run: the programs are those of K, copied into the pass directory.
        build = self.clone / "evals" / "batch" / "drift" / "build"
        build.write_text(build.read_text().replace('MARKER = "committed"', 'MARKER = "working tree"'))
        v, code = self.run_queue("--pass", "nightly", "--model", "claude-sonnet-5-5")
        self.assertEqual(code, 0, v)
        programs = Path(v["dir"]) / "programs"
        b, = self.of("build")
        r, = self.of("read")
        self.assertEqual(b["marker"], "committed")
        self.assertEqual(b["file"], str(programs / "evals" / "batch" / "drift" / "build"))
        self.assertEqual(r["file"], str(programs / "evals" / "batch" / "drift" / "read"))
        copied = sorted(str(p.relative_to(programs)) for p in programs.rglob("*") if p.is_file())
        self.assertEqual(copied, ["evals/batch/drift/build", "evals/batch/drift/read", "evals/batch/lib.py"])
        self.assertEqual(git(self.clone, "status", "--porcelain"), "M evals/batch/drift/build",
                         "the clone is not written")

    def test_usage_and_the_clones_of_the_policy(self):
        for argv in [[], ["--pass", "nightly"], ["--pass", "daily", "--model", "claude-sonnet-5-5"],
                     ["--pass", "nightly", "--pass", "weekly"], ["--pass", "nightly", "--model", "gpt"],
                     ["--pass", "nightly", "--model", "claude-sonnet-5-5", "--x"]]:
            with self.subTest(argv=argv):
                self.assertEqual(self.run_queue(*argv), ({"status": "usage"}, 2))
        for policy, code in [({}, "repository_not_mapped"), ({"repositories": {"invented/other": {"clone": "/x"}}},
                                                             "repository_not_mapped"),
                             ("{not json", "policy_invalid"), ({"repositories": []}, "policy_invalid"),
                             ({"repositories": {AP_REPO: {"clone": "x"}}},
                              "policy_invalid")]:
            with self.subTest(policy=policy):
                self.set_policy(policy)
                self.assertEqual(self.run_queue("--pass", "nightly", "--model", "claude-sonnet-5-5"),
                                 ({"status": code}, 3))
                self.assertFalse(self.state.exists())
        self.policy.unlink()
        self.assertEqual(self.run_queue("--pass", "nightly", "--model", "claude-sonnet-5-5"),
                         ({"status": "repository_not_mapped"}, 3))
        # A pass directory that exists.
        self.set_policy({"repositories": {name: {"clone": c} for name, c in self.clones.items()}})
        (self.state / "drift" / "20261123T180000Z-nightly").mkdir(parents=True)
        self.assertEqual(self.run_queue("--pass", "nightly", "--model", "claude-sonnet-5-5"),
                         ({"status": "pass_exists"}, 3))
        self.assertEqual(self.calls(), [])

    def test_unit_files(self):
        documentation = "Documentation=https://github.com/park-kyungchan/lifecycle-platform/blob/main/deploy/README.md"
        confinement = ["NoNewPrivileges=yes", "ProtectKernelModules=yes", "ProtectKernelLogs=yes",
                       "ProtectKernelTunables=yes", "ProtectControlGroups=yes", "ProtectHostname=yes",
                       "LockPersonality=yes", "RestrictRealtime=yes", "RestrictSUIDSGID=yes",
                       "SystemCallArchitectures=native", "ProtectSystem=full"]
        for name, model, calendar in [("nightly", "claude-sonnet-5-5", "*-*-* 18:00:00 UTC"),
                                      ("weekly", "claude-opus-5-5", "Sun *-*-* 06:00:00 UTC")]:
            with self.subTest(name=name):
                service = (DEPLOY / f"ap-api-{name}.service").read_text().splitlines()
                sections = {}
                section = None
                for line in service:
                    if line.startswith("["):
                        section = line
                    elif line and not line.startswith("#"):
                        sections.setdefault(section, []).append(line)
                unit = sections["[Unit]"]
                self.assertTrue(any(line.startswith("Description=") for line in unit))
                for line in [documentation, "Wants=network-online.target", "After=network-online.target"]:
                    self.assertEqual(unit.count(line), 1, line)
                lines = sections["[Service]"]
                for line in ["Type=oneshot",
                             "ExecStart=/usr/bin/python3 -I -B /var/lib/agent-platform/deliver/lifecycle-platform/"
                             f"evals/batch/queue --pass {name} --model {model}",
                             "TimeoutStartSec=26h", "StateDirectory=agent-platform/api", "StateDirectoryMode=0700",
                             "Slice=agents.slice", "Nice=10", "Environment=HOME=/root", "Environment=TZ=UTC",
                             "Environment=PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin", *confinement]:
                    self.assertEqual(lines.count(line), 1, line)
                keys = [line.split("=", 1)[0] for line in lines if not line.startswith("Environment=")]
                self.assertEqual(len(keys), len(set(keys)), "each line once")
                self.assertFalse(any(line.startswith("Restart") for line in service))
                timer = (DEPLOY / f"ap-api-{name}.timer").read_text().splitlines()
                self.assertTrue(any(line.startswith("Description=") for line in timer))
                for line in [documentation, "[Timer]", f"OnCalendar={calendar}", "Persistent=true", "[Install]",
                             "WantedBy=timers.target"]:
                    self.assertEqual(timer.count(line), 1, line)
        readme = (DEPLOY / "README.md").read_text()
        self.assertIn("\n## The API queue\n", readme)
        for name in ["ap-api-nightly.service", "ap-api-nightly.timer", "ap-api-weekly.service",
                     "ap-api-weekly.timer"]:
            self.assertIn(name, readme)
        self.assertIn("\n## The queue\n", (REPO / "evals" / "batch" / "README.md").read_text())
        self.assertEqual(QUEUE.read_text().splitlines()[0], "#!/usr/bin/env -S python3 -I -B")
        self.assertTrue(os.access(QUEUE, os.X_OK))


if __name__ == "__main__":
    unittest.main()
