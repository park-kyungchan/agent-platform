"""Tests of github/apply against a fake `gh` (fixtures/fake_gh.py) that answers from invented repositories.

No test reaches GitHub: every process a test starts (apply and the written undo.sh) runs with an environment
built from nothing but a PATH of the fake and the needed tools, an empty GH_CONFIG_DIR and no token, and
setUp fails before any run unless `gh` on that PATH is the fake and no other `gh` is on it.
Run: python3 -I -B -m unittest discover -s github -p 'test_*.py'
"""
import copy
import importlib.machinery
import importlib.util
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
APPLY = HERE / "apply"
FIXTURES = HERE / "fixtures"
FAKE = FIXTURES / "fake_gh.py"
BASH = shutil.which("bash")
TIMEOUT = shutil.which("timeout")
TOKENS = ("GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN", "GH_HOST")
ALPHA, BETA, GAMMA = "example-owner/alpha", "example-owner/beta", "example-owner/gamma"
REPOS = (ALPHA, BETA, GAMMA)
WANTED_SELECTED = {"github_owned_allowed": True, "verified_allowed": False, "patterns_allowed": ["example-org/cache-action@*"]}
MAIN = json.loads((FIXTURES / "ruleset-main.json").read_text())
TAGS = json.loads((FIXTURES / "ruleset-tags.json").read_text())

# Every endpoint point 3 names, with the methods allowed on it; a ruleset is deleted only when this run created it.
ALLOWED = [
    (r"repos/{r}", {"GET", "PATCH"}),
    (r"repos/{r}/actions/permissions", {"GET", "PUT"}),
    (r"repos/{r}/actions/permissions/selected-actions", {"GET", "PUT"}),
    (r"repos/{r}/vulnerability-alerts", {"GET", "PUT", "DELETE"}),
    (r"repos/{r}/automated-security-fixes", {"GET", "PUT", "DELETE"}),
    (r"repos/{r}/branches/main/protection", {"GET"}),
    (r"repos/{r}/rulesets\?per_page=100", {"GET"}),
    (r"repos/{r}/rulesets", {"POST"}),
    (r"repos/{r}/rulesets/\d+", {"GET", "PUT", "DELETE"}),
]


def load_module():
    loader = importlib.machinery.SourceFileLoader("github_apply", str(APPLY))
    spec = importlib.util.spec_from_loader("github_apply", loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def wanted_state(state):
    """The fake's state once every listed repository equals the fixture files."""
    state = copy.deepcopy(state)
    for name, repo in state["repositories"].items():
        repo["settings"].update(allow_squash_merge=False, allow_rebase_merge=False)
        repo["actions"]["sha_pinning_required"] = True
        if name != GAMMA:
            repo["actions"]["allowed_actions"] = "selected"
            repo["selected_actions"] = copy.deepcopy(WANTED_SELECTED)
            repo["vulnerability_alerts"] = repo["automated_security_fixes"] = True
    return state


def every_kind(state):
    """alpha differs in every kind of setting: four settings, actions, the features, the statuses, a ruleset
    that differs, one that is absent and a live one no file names."""
    state = copy.deepcopy(state)
    alpha = state["repositories"][ALPHA]
    alpha["settings"] = {"allow_merge_commit": False, "allow_squash_merge": True, "allow_rebase_merge": True, "delete_branch_on_merge": False}
    alpha["actions"] = {"enabled": False, "allowed_actions": "selected", "sha_pinning_required": False}
    alpha["selected_actions"] = {"github_owned_allowed": False, "verified_allowed": True, "patterns_allowed": []}
    alpha["security_and_analysis"] = {"secret_scanning": "disabled", "secret_scanning_push_protection": "disabled"}
    main = dict(MAIN, id=101, enforcement="evaluate")
    alpha["rulesets"] = [main, dict(TAGS, name="Old rule", id=103)]
    return state


def projection(name, repo):
    """What present.json must hold for one repository of the fake's state."""
    analysis = repo["security_and_analysis"] or {}
    allowed = repo["actions"]["allowed_actions"]
    return {
        "repository": name,
        "settings": repo["settings"],
        "actions": dict(repo["actions"], selected_actions=repo["selected_actions"] if allowed == "selected" else None),
        "vulnerability_alerts": repo["vulnerability_alerts"],
        "automated_security_fixes": repo["automated_security_fixes"],
        "secret_scanning": analysis.get("secret_scanning"),
        "secret_scanning_push_protection": analysis.get("secret_scanning_push_protection"),
        "classic_protection": repo["classic_protection"],
        "rulesets": repo["rulesets"],
    }


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp)
        self.bin, self.tools, self.config, self.files = (self.tmp / d for d in ("bin", "tools", "gh-config", "files"))
        for directory in (self.bin, self.tools, self.config, self.files, self.tmp / "home"):
            directory.mkdir()
        self.fake = self.bin / "gh"
        self.fake.write_text(f"#!/bin/sh\n# {load_fake_marker()}\nexec '{sys.executable}' -I -B '{FAKE}' \"$@\"\n")
        self.fake.chmod(0o755)
        (self.tools / "timeout").symlink_to(TIMEOUT)
        for name in ("repositories.json", "ruleset-main.json", "ruleset-tags.json"):
            shutil.copy(FIXTURES / name, self.files / name)
        self.settings = self.files / "repositories.json"
        self.state_path, self.log = self.tmp / "state.json", self.tmp / "calls.log"
        self.log.touch()
        self.initial = json.loads((FIXTURES / "state.json").read_text())
        self.set_state(self.initial)
        self.env = {"PATH": f"{self.bin}:{self.tools}", "GH_CONFIG_DIR": str(self.config), "HOME": str(self.tmp / "home"),
                    "LANG": "C.UTF-8", "FAKE_GH_STATE": str(self.state_path), "FAKE_GH_LOG": str(self.log)}
        # Invariant 6: fail before any run unless the only gh on the PATH is the fake, the config is empty and no token is set.
        found = shutil.which("gh", path=self.env["PATH"])
        if found != str(self.fake) or load_fake_marker() not in Path(found).read_text():
            self.fail(f"gh on the test PATH is not the fake: {found}")
        for directory in self.env["PATH"].split(":"):
            if directory != str(self.bin) and os.path.lexists(os.path.join(directory, "gh")):
                self.fail(f"another gh is on the test PATH: {directory}")
        if os.listdir(self.config) or any(t in self.env for t in TOKENS):
            self.fail("the test environment carries a GitHub configuration or token")

    # ------------------------------------------------------------ helpers

    def set_state(self, state):
        self.state_path.write_text(json.dumps(state, indent=1))

    def state(self):
        return json.loads(self.state_path.read_text())

    def repos_state(self):
        return self.state()["repositories"]

    def assert_reads_equal(self, repositories):
        """What the reads see (the projection) equals that of the given state; a member no read shows is not compared."""
        live = self.repos_state()
        self.assertEqual({n: projection(n, live[n]) for n in REPOS}, {n: projection(n, repositories[n]) for n in REPOS})

    def set_faults(self, **faults):
        state = self.state()
        state["faults"] = faults
        state["writes"] = 0
        self.set_state(state)

    def calls(self):
        return [json.loads(line) for line in self.log.read_text().splitlines()]

    def method(self, call):
        argv = call["argv"]
        return argv[argv.index("--method") + 1]

    def endpoint(self, call):
        argv = call["argv"]
        return argv[argv.index("--method") + 2]

    def writes(self, calls=None):
        return [c for c in (self.calls() if calls is None else calls) if self.method(c) != "GET"]

    def run_apply(self, *args, undo_dir=None):
        env = dict(self.env)
        if undo_dir is not None:
            env["FAKE_GH_UNDO_DIR"] = str(undo_dir)
            args = ("--undo-dir", str(undo_dir)) + args
        proc = subprocess.run([sys.executable, "-I", "-B", str(APPLY), "--file", str(self.settings), *args], env=env,
                              stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=600)
        output = json.loads(proc.stdout) if proc.returncode in (0, 1) else None
        self.assert_calls_allowed()
        return proc, output

    def check(self, *args):
        return self.run_apply("--check", *args)

    def apply(self, *args, name="undo"):
        directory = self.tmp / name
        proc, output = self.run_apply(*args, undo_dir=directory)
        return proc, output, directory

    def run_undo(self, script):
        return subprocess.run([BASH, str(script)], env=self.env, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=600)

    def assert_calls_allowed(self, repos=REPOS):
        """Invariant 3: only the listed repositories, only point 3's endpoints, a ruleset deleted only after its create."""
        created = set()
        for call in self.calls():
            argv = call["argv"]
            self.assertEqual(argv[:2], ["api", "--method"], argv)
            method, endpoint = self.method(call), self.endpoint(call)
            rest = argv[4:]
            self.assertTrue(rest == [] or (len(rest) == 2 and rest[0] == "--input"), argv)
            matched = [methods for pattern, methods in ALLOWED
                       if any(re.fullmatch(pattern.format(r=re.escape(r)), endpoint) for r in repos)]
            self.assertTrue(matched and method in matched[0], f"{method} {endpoint} is not an endpoint of point 3")
            if method == "POST":
                created.add(endpoint)
            if method == "DELETE" and "/rulesets/" in endpoint:
                self.assertIn(endpoint.rsplit("/", 1)[0], created, f"{endpoint} was not created by this run")

    def assert_only_gets(self, calls):
        for call in calls:
            argv = call["argv"]
            self.assertEqual(argv[:3], ["api", "--method", "GET"], argv)
            self.assertEqual(len(argv), 4, argv)
            self.assertIsNone(call["body"])
            for flag in ("-X", "-f", "-F", "--field", "--raw-field", "--input"):
                self.assertNotIn(flag, argv)

    def results(self, output):
        return {r["repository"]: r for r in output["repositories"]}


def load_fake_marker():
    found = re.search(r'^MARKER = "([^"]+)"$', FAKE.read_text(), re.M)
    return found.group(1)


class Check(Base):
    def test_baseline_names_the_table_differences_with_get_only(self):
        proc, output = self.check()
        self.assertEqual(proc.returncode, 1, proc.stderr)
        self.assertEqual(output["mode"], "check")
        results = self.results(output)
        common = [
            {"setting": "actions.allowed_actions", "present": "all", "wanted": "selected"},
            {"setting": "actions.sha_pinning_required", "present": False, "wanted": True},
            {"setting": "actions.selected_actions", "present": None, "wanted": WANTED_SELECTED},
            {"setting": "vulnerability_alerts", "present": False, "wanted": True},
            {"setting": "automated_security_fixes", "present": False, "wanted": True},
        ]
        self.assertEqual(results[ALPHA], {"repository": ALPHA, "result": "differs", "code": None, "differences": common, "undo": None})
        self.assertEqual(results[BETA]["differences"], [
            {"setting": "settings.allow_squash_merge", "present": True, "wanted": False},
            {"setting": "settings.allow_rebase_merge", "present": True, "wanted": False}] + common)
        self.assertEqual(results[GAMMA]["differences"], [{"setting": "actions.sha_pinning_required", "present": False, "wanted": True}])
        self.assertEqual([r["repository"] for r in output["repositories"]], list(REPOS))
        self.assert_only_gets(self.calls())
        self.assert_reads_equal(self.initial["repositories"])

    def test_every_kind_of_difference_and_classic_protection_with_get_only(self):
        state = every_kind(self.initial)
        state["repositories"][BETA]["classic_protection"] = True
        self.set_state(state)
        proc, output = self.check()
        self.assertEqual(proc.returncode, 1, proc.stderr)
        results = self.results(output)
        self.assertEqual(results[ALPHA]["result"], "differs")
        self.assertEqual(results[ALPHA]["differences"], [
            {"setting": "settings.allow_merge_commit", "present": False, "wanted": True},
            {"setting": "settings.allow_squash_merge", "present": True, "wanted": False},
            {"setting": "settings.allow_rebase_merge", "present": True, "wanted": False},
            {"setting": "settings.delete_branch_on_merge", "present": False, "wanted": True},
            {"setting": "actions.enabled", "present": False, "wanted": True},
            {"setting": "actions.sha_pinning_required", "present": False, "wanted": True},
            {"setting": "actions.selected_actions",
             "present": {"github_owned_allowed": False, "verified_allowed": True, "patterns_allowed": []}, "wanted": WANTED_SELECTED},
            {"setting": "vulnerability_alerts", "present": False, "wanted": True},
            {"setting": "automated_security_fixes", "present": False, "wanted": True},
            {"setting": "secret_scanning", "present": "disabled", "wanted": "enabled"},
            {"setting": "secret_scanning_push_protection", "present": "disabled", "wanted": "enabled"},
            {"setting": "ruleset:Fixture main", "present": dict(MAIN, enforcement="evaluate"), "wanted": MAIN},
            {"setting": "ruleset:Fixture tags", "present": None, "wanted": TAGS},
            {"setting": "ruleset:Old rule", "present": dict(TAGS, name="Old rule"), "wanted": None},
        ])
        self.assertEqual((results[BETA]["result"], results[BETA]["code"]), ("refused", "classic_protection_present"))
        self.assertEqual(len(results[BETA]["differences"]), 7)
        self.assert_only_gets(self.calls())
        self.assert_reads_equal(state["repositories"])

    def test_equal_state_is_exit_0_with_get_only(self):
        self.set_state(wanted_state(self.initial))
        proc, output = self.check()
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual([(r["result"], r["differences"]) for r in output["repositories"]], [("same", [])] * 3)
        self.assert_only_gets(self.calls())

    def test_repo_limits_the_run(self):
        proc, output = self.check("--repo", GAMMA)
        self.assertEqual(proc.returncode, 1)
        self.assertEqual([r["repository"] for r in output["repositories"]], [GAMMA])
        self.assertTrue(all(f"repos/{GAMMA}" in self.endpoint(c) for c in self.calls()))

    def test_vulnerability_alerts_404_is_off_and_another_failure_is_read_failed(self):
        state = wanted_state(self.initial)
        state["repositories"][ALPHA]["vulnerability_alerts"] = False
        state["faults"] = {"fail_get": {f"{BETA}/vulnerability-alerts": 403, f"{GAMMA}/automated-security-fixes": 404}}
        self.set_state(state)
        proc, output = self.check()
        self.assertEqual(proc.returncode, 1)
        results = self.results(output)
        self.assertEqual(results[ALPHA]["differences"], [{"setting": "vulnerability_alerts", "present": False, "wanted": True}])
        self.assertEqual((results[BETA]["result"], results[BETA]["code"], results[BETA]["differences"]), ("failed", "read_failed", []))
        self.assertIn("read_failed@github/apply: example-owner/beta", proc.stderr)
        self.assertEqual(results[GAMMA]["result"], "same")  # 404 of automated-security-fixes is off; gamma does not manage it
        self.assert_only_gets(self.calls())

    def test_automated_security_fixes_404_is_off(self):
        state = wanted_state(self.initial)
        state["faults"] = {"fail_get": {f"{ALPHA}/automated-security-fixes": 404}}
        self.set_state(state)
        proc, output = self.check("--repo", ALPHA)
        self.assertEqual(self.results(output)[ALPHA]["differences"],
                         [{"setting": "automated_security_fixes", "present": False, "wanted": True}])


class Apply(Base):
    def test_apply_writes_the_undo_first_then_a_second_apply_only_reads(self):
        proc, output, directory = self.apply()
        self.assertEqual(proc.returncode, 0, proc.stderr)
        results = self.results(output)
        self.assertEqual([r["result"] for r in output["repositories"]], ["applied"] * 3)
        writes = self.writes()
        self.assertEqual([(self.method(c), self.endpoint(c)) for c in writes], [
            ("PUT", f"repos/{ALPHA}/actions/permissions"),
            ("PUT", f"repos/{ALPHA}/actions/permissions/selected-actions"),
            ("PUT", f"repos/{ALPHA}/vulnerability-alerts"),
            ("PUT", f"repos/{ALPHA}/automated-security-fixes"),
            ("PATCH", f"repos/{BETA}"),
            ("PUT", f"repos/{BETA}/actions/permissions"),
            ("PUT", f"repos/{BETA}/actions/permissions/selected-actions"),
            ("PUT", f"repos/{BETA}/vulnerability-alerts"),
            ("PUT", f"repos/{BETA}/automated-security-fixes"),
            ("PUT", f"repos/{GAMMA}/actions/permissions"),
        ])
        self.assertEqual(writes[0]["body"], {"enabled": True, "allowed_actions": "selected", "sha_pinning_required": True})
        self.assertEqual(writes[4]["body"], {"allow_squash_merge": False, "allow_rebase_merge": False})
        self.assertEqual(writes[9]["body"], {"enabled": True, "allowed_actions": "all", "sha_pinning_required": True})
        for repo in REPOS:
            prefix = repo.replace("/", "__")
            first = next(c for c in writes if f"repos/{repo}" in self.endpoint(c))
            self.assertIn(f"{prefix}.undo.sh", first["undo_files"])
            self.assertIn(f"{prefix}.present.json", first["undo_files"])
            self.assertEqual(results[repo]["undo"], str(directory / f"{prefix}.undo.sh"))
            present = json.loads((directory / f"{prefix}.present.json").read_text())
            self.assertEqual(present, projection(repo, self.initial["repositories"][repo]))
        self.assertNotIn("temp_clone_token", (directory / "example-owner__gamma.present.json").read_text())
        self.assertEqual(stat.S_IMODE(directory.stat().st_mode), 0o700)
        self.assert_reads_equal(wanted_state(self.initial)["repositories"])
        self.log.write_text("")
        proc, output, second = self.apply(name="second")
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual([(r["result"], r["undo"]) for r in output["repositories"]], [("same", None)] * 3)
        self.assert_only_gets(self.calls())
        self.assertEqual(os.listdir(second), [])

    def test_undo_puts_the_state_back_exactly(self):
        proc, first, directory = self.apply()
        self.assertEqual(proc.returncode, 0, proc.stderr)
        for repo in reversed(REPOS):
            undo = self.run_undo(self.results(first)[repo]["undo"])
            self.assertEqual(undo.returncode, 0, undo.stdout)
            self.assertNotIn("failed:", undo.stdout)
        self.assert_reads_equal(self.initial["repositories"])
        for repo in REPOS:
            present = json.loads((directory / f"{repo.replace('/', '__')}.present.json").read_text())
            self.assertEqual(projection(repo, self.repos_state()[repo]), present)
        proc, again = self.check()
        self.assertEqual([r["differences"] for r in again["repositories"]], [r["differences"] for r in first["repositories"]])

    def test_every_kind_in_order_and_the_created_ruleset_is_deleted_by_its_new_id(self):
        state = every_kind(self.initial)
        self.set_state(state)
        proc, output, directory = self.apply("--repo", ALPHA)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        alpha = self.results(output)[ALPHA]
        self.assertEqual(alpha["result"], "applied")
        self.assertEqual(len(alpha["differences"]), 14)
        writes = self.writes()
        base = f"repos/{ALPHA}"
        self.assertEqual([(self.method(c), self.endpoint(c)) for c in writes], [
            ("PATCH", base), ("PUT", f"{base}/actions/permissions"), ("PUT", f"{base}/actions/permissions/selected-actions"),
            ("PUT", f"{base}/vulnerability-alerts"), ("PUT", f"{base}/automated-security-fixes"), ("PATCH", base),
            ("PUT", f"{base}/rulesets/101"), ("POST", f"{base}/rulesets")])
        self.assertEqual(writes[5]["body"], {"security_and_analysis": {"secret_scanning": {"status": "enabled"},
                                                                       "secret_scanning_push_protection": {"status": "enabled"}}})
        self.assertEqual(writes[7]["body"], TAGS)
        live = self.repos_state()[ALPHA]["rulesets"]
        self.assertIn(dict(TAGS, name="Old rule", id=103), live)  # the ruleset no file names is untouched
        self.assertIn(dict(TAGS, id=9001), live)
        script = Path(alpha["undo"]).read_text()
        calls = re.findall(r"^call (\S+) (\S+)", script, re.M)
        self.assertEqual(calls, [
            ("DELETE", f"{base}/rulesets/9001"), ("PUT", f"{base}/rulesets/101"), ("PATCH", base),
            ("DELETE", f"{base}/automated-security-fixes"), ("DELETE", f"{base}/vulnerability-alerts"),
            ("PUT", f"{base}/actions/permissions/selected-actions"), ("PUT", f"{base}/actions/permissions"), ("PATCH", base)])
        self.log.write_text("")
        undo = self.run_undo(alpha["undo"])
        self.assertEqual(undo.returncode, 0, undo.stdout)
        self.assertIn(("DELETE", f"{base}/rulesets/9001"), [(self.method(c), self.endpoint(c)) for c in self.calls()])
        self.assert_reads_equal(state["repositories"])

    def test_read_back_that_differs_stops_the_later_repositories(self):
        self.set_faults(ignore_write=[f"{ALPHA}/automated-security-fixes"])
        proc, output, directory = self.apply()
        self.assertEqual(proc.returncode, 1)
        results = self.results(output)
        self.assertEqual((results[ALPHA]["result"], results[ALPHA]["code"]), ("failed", "read_back_differs"))
        self.assertEqual(len(results[ALPHA]["differences"]), 5)
        self.assertIn("automated_security_fixes", proc.stderr)
        for repo in (BETA, GAMMA):
            self.assertEqual(results[repo], {"repository": repo, "result": "not_run", "code": None, "differences": [], "undo": None})
        self.assertFalse([c for c in self.calls() if ALPHA not in self.endpoint(c)])

    def test_classic_protection_is_refused_with_no_write(self):
        state = copy.deepcopy(self.initial)
        state["repositories"][ALPHA]["classic_protection"] = True
        self.set_state(state)
        proc, output, directory = self.apply()
        self.assertEqual(proc.returncode, 1)
        results = self.results(output)
        self.assertEqual((results[ALPHA]["result"], results[ALPHA]["code"], results[ALPHA]["undo"]),
                         ("refused", "classic_protection_present", None))
        self.assertEqual(len(results[ALPHA]["differences"]), 5)
        self.assertEqual([results[r]["result"] for r in (BETA, GAMMA)], ["applied", "applied"])
        self.assertFalse([c for c in self.writes() if ALPHA in self.endpoint(c)])
        self.assertFalse(list(directory.glob("example-owner__alpha.*")))
        self.assertEqual(self.repos_state()[ALPHA], state["repositories"][ALPHA])

    def test_an_entry_of_only_the_repository_gets_the_rulesets_and_nothing_else(self):
        settings = json.loads(self.settings.read_text())
        for entry in settings["repositories"]:
            if entry["repository"] == GAMMA:
                entry.clear()
                entry["repository"] = GAMMA
        self.settings.write_text(json.dumps(settings))
        base = f"repos/{GAMMA}"
        names = ["ruleset:Fixture main", "ruleset:Fixture tags"]
        # (a) gamma's live actions differ from the fixture's own entry, which this entry does not hold
        self.assertTrue(self.initial["repositories"][GAMMA]["actions"]["sha_pinning_required"] is False)
        proc, output = self.check("--repo", GAMMA)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        gamma = self.results(output)[GAMMA]
        self.assertEqual((gamma["result"], gamma["code"], gamma["differences"]), ("same", None, []))
        self.assert_only_gets(self.calls())
        # (b) live rulesets removed: the two rulesets are the only differences and the only writes
        bare = copy.deepcopy(self.initial)
        bare["repositories"][GAMMA]["rulesets"] = []
        self.set_state(bare)
        self.log.write_text("")
        proc, output = self.check("--repo", GAMMA)
        self.assertEqual(proc.returncode, 1)
        gamma = self.results(output)[GAMMA]
        self.assertEqual(gamma["result"], "differs")
        self.assertEqual([(d["setting"], d["present"]) for d in gamma["differences"]], [(n, None) for n in names])
        self.assert_only_gets(self.calls())
        self.log.write_text("")
        proc, output, directory = self.apply("--repo", GAMMA)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        applied = self.results(output)[GAMMA]
        self.assertEqual(applied["result"], "applied")
        self.assertEqual([(self.method(c), self.endpoint(c)) for c in self.writes()], [("POST", f"{base}/rulesets")] * 2)
        proc, output = self.check("--repo", GAMMA)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(self.results(output)[GAMMA]["result"], "same")
        script = Path(applied["undo"]).read_text()
        self.assertEqual(re.findall(r"^call (\S+) (\S+)", script, re.M), [
            ("DELETE", f"{base}/rulesets/9002"), ("DELETE", f"{base}/rulesets/9001")])
        undo = self.run_undo(applied["undo"])
        self.assertEqual(undo.returncode, 0, undo.stdout)
        self.assert_reads_equal(bare["repositories"])
        # (c) classic protection: refused with the same differences and no write
        classic = copy.deepcopy(bare)
        classic["repositories"][GAMMA]["classic_protection"] = True
        self.set_state(classic)
        self.log.write_text("")
        proc, output = self.check("--repo", GAMMA)
        self.assertEqual(proc.returncode, 1)
        refused_check = self.results(output)[GAMMA]
        self.assertEqual((refused_check["result"], refused_check["code"]), ("refused", "classic_protection_present"))
        self.assertEqual([(d["setting"], d["present"]) for d in refused_check["differences"]], [(n, None) for n in names])
        self.assert_only_gets(self.calls())
        self.log.write_text("")
        proc, output, refused_dir = self.apply("--repo", GAMMA, name="refused")
        self.assertEqual(proc.returncode, 1)
        refused = self.results(output)[GAMMA]
        self.assertEqual((refused["result"], refused["code"], refused["undo"]), ("refused", "classic_protection_present", None))
        self.assertEqual([(d["setting"], d["present"]) for d in refused["differences"]], [(n, None) for n in names])
        self.assert_only_gets(self.calls())
        self.assertEqual(os.listdir(refused_dir), [])
        self.assertEqual(self.repos_state()[GAMMA], classic["repositories"][GAMMA])

    def test_read_failed_before_a_write_stops_the_run(self):
        self.set_faults(fail_get={f"{ALPHA}/rulesets": 500})
        proc, output, directory = self.apply()
        self.assertEqual(proc.returncode, 1)
        self.assertEqual([(r["result"], r["code"]) for r in output["repositories"]],
                         [("failed", "read_failed"), ("not_run", None), ("not_run", None)])
        self.assertFalse(self.writes())
        self.assertEqual(os.listdir(directory), [])

    def test_failed_write_k_stops_later_writes_and_the_undo_puts_the_state_back(self):
        state = every_kind(self.initial)
        total = 8 + 5 + 1  # alpha's eight writes of every kind, beta's five, gamma's one
        order = [ALPHA] * 8 + [BETA] * 5 + [GAMMA]
        strict_failures = 0
        for strict in (False, True):
            for k in range(1, total + 1):
                with self.subTest(strict=strict, k=k):
                    self.log.write_text("")
                    self.set_state(dict(state, faults={"fail_write": k}, writes=0))
                    proc, output, directory = self.apply(name=f"undo-{strict}-{k}")
                    self.assertEqual(proc.returncode, 1)
                    failing = order[k - 1]
                    results = self.results(output)
                    self.assertEqual((results[failing]["result"], results[failing]["code"]), ("failed", "write_failed"))
                    position = REPOS.index(failing)
                    self.assertEqual([results[r]["result"] for r in REPOS[:position]], ["applied"] * position)
                    self.assertEqual([results[r]["result"] for r in REPOS[position + 1:]], ["not_run"] * (2 - position))
                    calls = self.calls()
                    self.assertEqual(len(self.writes(calls)), k)
                    self.assertNotEqual(self.method(calls[-1]), "GET", "nothing is called after the failed write")
                    self.set_faults(strict_delete=strict)
                    for repo in reversed(REPOS[:position + 1]):
                        script = Path(results[repo]["undo"])
                        undo = self.run_undo(script)
                        lines = len(re.findall(r"^call ", script.read_text(), re.M))
                        self.assertEqual(len(undo.stdout.splitlines()), lines, undo.stdout)
                        self.assertEqual(undo.returncode, 1 if "failed:" in undo.stdout else 0, undo.stdout)
                        if not strict:
                            self.assertEqual(undo.returncode, 0, undo.stdout)
                        elif undo.returncode:
                            strict_failures += 1
                            self.assertRegex(undo.stdout, r"failed: DELETE \S+/(vulnerability-alerts|automated-security-fixes)")
                        present = json.loads(script.with_name(script.name.replace(".undo.sh", ".present.json")).read_text())
                        self.assertEqual(projection(repo, self.repos_state()[repo]), present)
                    self.assert_reads_equal(state["repositories"])
        self.assertGreater(strict_failures, 0)

    def test_undo_with_a_422_to_a_delete_of_an_off_feature_runs_every_call(self):
        self.set_faults(fail_write=3)  # alpha's PUT vulnerability-alerts fails: both features are still off
        proc, output, directory = self.apply()
        alpha = self.results(output)[ALPHA]
        self.assertEqual(alpha["code"], "write_failed")
        self.set_faults(strict_delete=True)
        undo = self.run_undo(alpha["undo"])
        self.assertEqual(undo.returncode, 1)
        lines = undo.stdout.splitlines()
        self.assertEqual([line.split(":")[0] for line in lines], ["failed", "failed", "ok"], undo.stdout)
        self.assertIn("(HTTP 422)", undo.stdout)
        self.assert_reads_equal(self.initial["repositories"])


class Refusals(Base):
    def assert_refused(self, code, *args):
        proc = subprocess.run([sys.executable, "-I", "-B", str(APPLY), *args], env=self.env, stdin=subprocess.DEVNULL,
                              capture_output=True, text=True, timeout=60)
        self.assertEqual(proc.returncode, 2, proc.stderr)
        self.assertIn(f"{code}@github/apply", proc.stderr)
        self.assertEqual(proc.stdout, "")
        self.assertEqual(self.calls(), [])

    def test_invalid_usage(self):
        file = ("--file", str(self.settings))
        self.assert_refused("invalid_usage", *file)
        self.assert_refused("invalid_usage", "--check", "--undo-dir", str(self.tmp / "u"), *file)
        self.assert_refused("invalid_usage", "--check", "--check", *file)
        self.assert_refused("invalid_usage", "--check", "--repo", *file)
        self.assert_refused("invalid_usage", "--check", "--force", *file)
        self.assertFalse((self.tmp / "u").exists())

    def test_existing_undo_dir(self):
        existing = self.tmp / "existing"
        existing.mkdir()
        (existing / "kept").write_text("x")
        self.assert_refused("undo_dir_exists", "--undo-dir", str(existing), "--file", str(self.settings))
        self.assertEqual(os.listdir(existing), ["kept"])

    def test_unknown_repo(self):
        for mode in (["--check"], ["--undo-dir", str(self.tmp / "u")]):
            self.assert_refused("repository_not_listed", *mode, "--repo", "example-owner/delta", "--file", str(self.settings))
        self.assertFalse((self.tmp / "u").exists())

    def test_malformed_files(self):
        good = json.loads(self.settings.read_text())
        broken = []
        broken.append("{not json")
        top = copy.deepcopy(good); top["extra"] = 1; broken.append(top)
        entry = copy.deepcopy(good); entry["repositories"][0]["settings"]["allow_squash_merge"] = "no"; broken.append(entry)
        entry = copy.deepcopy(good); entry["repositories"][0]["unknown"] = True; broken.append(entry)
        entry = copy.deepcopy(good); entry["repositories"][2]["actions"]["selected_actions"] = WANTED_SELECTED; broken.append(entry)
        entry = copy.deepcopy(good); del entry["repositories"][0]["actions"]["selected_actions"]; broken.append(entry)
        entry = copy.deepcopy(good); entry["repositories"][0]["vulnerability_alerts"] = 1; broken.append(entry)
        entry = copy.deepcopy(good); entry["repositories"][0]["secret_scanning"] = "on"; broken.append(entry)
        entry = copy.deepcopy(good); entry["repositories"].append(copy.deepcopy(entry["repositories"][0])); broken.append(entry)
        entry = copy.deepcopy(good); entry["rulesets"].append("missing.json"); broken.append(entry)
        entry = copy.deepcopy(good); entry["rulesets"] = ["../ruleset-main.json"]; broken.append(entry)
        for body in broken:
            with self.subTest(body=body):
                self.settings.write_text(body if isinstance(body, str) else json.dumps(body))
                self.assert_refused("invalid_file", "--check", "--file", str(self.settings))
                self.assert_refused("invalid_file", "--undo-dir", str(self.tmp / "u"), "--file", str(self.settings))
                self.assertFalse((self.tmp / "u").exists())
        self.settings.write_text(json.dumps(good))
        for ruleset in (dict(MAIN, id=1), {k: v for k, v in MAIN.items() if k != "bypass_actors"}, dict(MAIN, rules={})):
            with self.subTest(ruleset=ruleset):
                (self.files / "ruleset-main.json").write_text(json.dumps(ruleset))
                self.assert_refused("invalid_file", "--check", "--file", str(self.settings))

    def test_a_fake_that_is_not_executable_makes_no_call(self):
        self.fake.chmod(0o644)
        proc, output = self.check()
        self.assertEqual(proc.returncode, 1)
        self.assertEqual([(r["result"], r["code"]) for r in output["repositories"]], [("failed", "read_failed")] * 3)
        proc, output, directory = self.apply()
        self.assertEqual(proc.returncode, 1)
        self.assertEqual([r["result"] for r in output["repositories"]], ["failed", "not_run", "not_run"])
        self.assertEqual(self.calls(), [])
        self.assert_reads_equal(self.initial["repositories"])


class Files(unittest.TestCase):
    """The files of the repository hold exactly point 2's values; they are read in this process, never run."""

    def test_repositories_json_holds_the_wanted_values(self):
        module = load_module()
        rulesets, entries = module.load(str(HERE / "repositories.json"))
        settings = {"allow_merge_commit": True, "allow_squash_merge": False, "allow_rebase_merge": False, "delete_branch_on_merge": True}
        public = {"settings": settings,
                  "actions": {"enabled": True, "allowed_actions": "selected", "sha_pinning_required": True,
                              "selected_actions": {"github_owned_allowed": True, "verified_allowed": False,
                                                   "patterns_allowed": ["Swatinem/rust-cache@*"]}},
                  "vulnerability_alerts": True, "automated_security_fixes": True}
        self.assertEqual(entries, [
            dict(public, repository="park-kyungchan/lifecycle-platform"),
            {"repository": "park-kyungchan/academy-platform", "settings": settings,
             "actions": {"enabled": True, "allowed_actions": "all", "sha_pinning_required": True}},
            {"repository": "park-kyungchan/lifecycle-corpus", "settings": settings,
             "actions": {"enabled": True, "allowed_actions": "selected", "sha_pinning_required": True,
                         "selected_actions": {"github_owned_allowed": True, "verified_allowed": False,
                                              "patterns_allowed": []}}},
        ])
        # The archived repositories have no entry, and the two regrouped ones are named by their new names only.
        text = (HERE / "repositories.json").read_text()
        for old in ("agent-platform", "education-platform", "ontology-platform", "agent-vps-operations",
                    "education-dc-gang-a", "education-problem-edit"):
            self.assertNotIn("park-kyungchan/" + old + '"', text)
        self.assertEqual(json.loads((HERE / "repositories.json").read_text())["rulesets"], ["ruleset-main.json", "ruleset-tags.json"])
        self.assertEqual([(r["name"], r["target"]) for r in rulesets],
                         [("Protected main with exact native review", "branch"), ("Preserve immutable tags", "tag")])

    def test_canon_tells_true_from_one(self):
        module = load_module()
        self.assertFalse(module.same({"a": True}, {"a": 1}))
        self.assertTrue(module.same({"a": 1, "b": [1, 2]}, {"b": [1, 2], "a": 1}))
        self.assertFalse(module.same([1, 2], [2, 1]))


if __name__ == "__main__":
    unittest.main()
