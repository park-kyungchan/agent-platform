"""Tests of ci/skips.py: `declared` on the repository and on scratch copies, `owed` between commits of a scratch repository.

specs/ap/check.md R23 (the table and its sites) and R24 (the rows a head owes). Each test removes CHECK_SKIPS from the
environment it gives the script, so that no run of it reaches the file of a ./check around it.
Run: python3 -I -B -m unittest discover -s ci -p test_skips.py
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
SCRIPT = REPO / "ci" / "skips.py"
ENV = {k: v for k, v in os.environ.items() if k != "CHECK_SKIPS"}
GIT = ["git", "-c", "user.name=t", "-c", "user.email=t@e", "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"]


def row(i="one", file="scripts/one.sh", covers=("src/a.rs",), runs_on="vps", how="bash scripts/one.sh"):
    return {"id": i, "file": file, "covers": list(covers), "runs_on": runs_on, "how": how}


class Scratch(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="skips-test."))
        self.addCleanup(shutil.rmtree, self.root, True)
        (self.root / "ci").mkdir()
        (self.root / "scripts").mkdir()
        shutil.copy2(SCRIPT, self.root / "ci" / "skips.py")
        (self.root / "check").write_text("#!/bin/sh\nCHECK_SKIPS=$(mktemp)\nexport CHECK_SKIPS\n")

    def run_skips(self, *words):
        done = subprocess.run([sys.executable, "-I", "-B", str(self.root / "ci" / "skips.py"), *words], cwd=self.root,
                              capture_output=True, text=True, timeout=60, env=ENV)
        return done.returncode, done.stdout, done.stderr

    def table(self, *rows, raw=None):
        (self.root / "ci" / "skips.json").write_text(raw if raw is not None else json.dumps({"rows": list(rows)}))

    def write(self, rel, text):
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def git(self, *words):
        return subprocess.run([*GIT, "-C", str(self.root), *words], check=True, capture_output=True, text=True, env=ENV).stdout.strip()

    def commit(self):
        self.git("add", "-A")
        self.git("commit", "--quiet", "--allow-empty", "-m", "c")
        return self.git("rev-parse", "HEAD")


SHELL_SITE = '[ -z "${CHECK_SKIPS:-}" ] || echo one >> "$CHECK_SKIPS"\n'


class SkipsDeclared(Scratch):
    # spec: specs/ap/check.md#R23
    def test_skips_declared_the_repository(self):
        done = subprocess.run([sys.executable, "-I", "-B", str(SCRIPT), "declared"], cwd=REPO, capture_output=True, text=True,
                              timeout=60, env=ENV)
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertRegex(done.stdout, r"^skips: \d+ rows declared, \d+ sites\n$")

    def test_skips_declared_every_site_form(self):
        self.table(row(), row("two", "ci/two.py", runs_on="ci"), row("three", "crates/x/tests/three.rs", runs_on="vps-root"))
        self.write("scripts/one.sh", "echo skipped\n" + SHELL_SITE)
        self.write("ci/two.py", 'with open(os.environ.get("CHECK_SKIPS") or os.devnull, "a") as s: s.write(\'two\\n\')\n')
        self.write("crates/x/tests/three.rs", 'const CHECK_SKIPS: &str = "CHECK_SKIPS";\n    skipped(CHECK_SKIPS, "three");\n')
        code, out, err = self.run_skips("declared")
        self.assertEqual((code, out), (0, "skips: 3 rows declared, 3 sites\n"), err)

    def test_skips_declared_refuses_a_table_not_of_its_form(self):
        self.write("scripts/one.sh", SHELL_SITE)
        cases = {
            "not JSON": dict(raw="{"),
            "another member": dict(raw=json.dumps({"rows": [], "more": 1})),
            "rows not a list": dict(raw=json.dumps({"rows": {}})),
            "a member missing": dict(raw=json.dumps({"rows": [{k: v for k, v in row().items() if k != "how"}]})),
            "a member more": dict(raw=json.dumps({"rows": [dict(row(), extra=1)]})),
            "an id with a capital": (row("One"),),
            "an id with a space": (row("o ne"),),
            "an id twice": (row(), row()),
            "a file of a pattern": (row(file="scripts/**"),),
            "a file with ..": (row(file="scripts/../one.sh"),),
            "no covers": (row(covers=()),),
            "21 covers": (row(covers=[f"src/{n}" for n in range(21)]),),
            "a cover of no form": (row(covers=("src/*.rs",)),),
            "a cover with a leading /": (row(covers=("/src",)),),
            "runs_on of another host": (row(runs_on="pc"),),
            "how of two lines": (row(how="a\nb"),),
            "how blank": (row(how="  "),),
        }
        for name, case in cases.items():
            if isinstance(case, dict):
                self.table(**case)
            else:
                self.table(*case)
            code, _, err = self.run_skips("declared")
            self.assertEqual(code, 1, name)
            self.assertIn("skips_form@ci/skips.json", err, name)

    def test_skips_declared_refuses_a_site_or_a_row_without_its_other(self):
        self.table(row())
        # A row's file that does not exist.
        code, _, err = self.run_skips("declared")
        self.assertEqual(code, 1)
        self.assertIn("skips_file_missing@ci/skips.json: one", err)
        # A declared id that its file never appends.
        self.write("scripts/one.sh", "echo skipped\n")
        code, _, err = self.run_skips("declared")
        self.assertEqual(code, 1)
        self.assertIn("skips_no_site@scripts/one.sh: one", err)
        # A site of an id no row declares, in a script ./check reaches by the glob, not a row's file.
        self.write("scripts/one.sh", SHELL_SITE)
        self.write("ci/other.sh", '[ -z "${CHECK_SKIPS:-}" ] || echo undeclared >> "$CHECK_SKIPS"\n')
        code, _, err = self.run_skips("declared")
        self.assertEqual(code, 1)
        self.assertIn("skips_undeclared@ci/other.sh:1: undeclared", err)
        # A site in check itself.
        os.remove(self.root / "ci" / "other.sh")
        self.write("check", "#!/bin/sh\n" + SHELL_SITE)
        code, _, err = self.run_skips("declared")
        self.assertEqual(code, 1)
        self.assertIn("skips_other_file@check:2: one", err)
        # A line that appends to CHECK_SKIPS without a literal id.
        self.write("check", "#!/bin/sh\n")
        self.write("scripts/one.sh", SHELL_SITE + 'echo "$id" >> "$CHECK_SKIPS"\n')
        code, _, err = self.run_skips("declared")
        self.assertEqual(code, 1)
        self.assertIn("skips_site_form@scripts/one.sh:2", err)
        # Every other way to write to the file is refused: an unquoted or braced target, `>`, `>|`, tee, a second
        # write beside a site on its line, a Python open of it without its site's write, a Rust read of it.
        writes = {
            "scripts/one.sh": [
                "echo bogus-id >> $CHECK_SKIPS",
                "echo bogus-id >>${CHECK_SKIPS}",
                'echo bogus-id >> "${CHECK_SKIPS}"',
                'echo bogus-id > "$CHECK_SKIPS"',
                "echo bogus-id >| $CHECK_SKIPS",
                "echo bogus-id &>> $CHECK_SKIPS",
                'echo bogus-id | tee -a "$CHECK_SKIPS"',
                SHELL_SITE.rstrip("\n") + "; echo bogus-id >> $CHECK_SKIPS",
            ],
            "ci/two.py": [
                'skips = open(os.environ["CHECK_SKIPS"], "a")',
                'with open(os.environ.get("CHECK_SKIPS") or os.devnull, "a") as s:',
                'path = os.environ.get("CHECK_SKIPS")',
            ],
            "crates/x/tests/three.rs": [
                '    let path = std::env::var_os("CHECK_SKIPS");',
                '    skipped(CHECK_SKIPS, id);',
            ],
        }
        self.table(row(), row("two", "ci/two.py", runs_on="ci"), row("three", "crates/x/tests/three.rs", runs_on="ci"))
        good = {"scripts/one.sh": SHELL_SITE,
                "ci/two.py": 'with open(os.environ.get("CHECK_SKIPS") or os.devnull, "a") as s: s.write("two\\n")\n',
                "crates/x/tests/three.rs": '    skipped(CHECK_SKIPS, "three");\n'}
        for f, text in good.items():
            self.write(f, text)
        self.assertEqual(self.run_skips("declared")[0], 0)
        for f, lines in writes.items():
            for line in lines:
                self.write(f, good[f] + line + "\n")
                code, _, err = self.run_skips("declared")
                self.assertEqual(code, 1, line)
                self.assertIn(f"skips_site_form@{f}:2", err, line)
            self.write(f, good[f])
        # And the whole again.
        self.write("scripts/one.sh", SHELL_SITE)
        self.assertEqual(self.run_skips("declared")[0], 0)


# spec: specs/ap/check.md#R24
class SkipsOwed(Scratch):
    def setUp(self):
        super().setUp()
        self.git("init", "--quiet", "-b", "main")

    def test_skips_owed_the_rows_a_changed_path_meets(self):
        self.table(row(covers=("src/a.rs",)), row("dir", covers=("src/lib/**",), runs_on="vps-root", how="make dir"),
                   row("in-ci", covers=("src/**",), runs_on="ci"), row("other", covers=("docs/x",)))
        self.write("src/a.rs", "a\n")
        base = self.commit()
        self.write("src/a.rs", "b\n")
        self.write("src/lib/x.rs", "x\n")
        self.write("src/libx.rs", "x\n")
        head = self.commit()
        code, out, err = self.run_skips("owed", base, head)
        self.assertEqual((code, out), (0, "one\tvps\tbash scripts/one.sh\ndir\tvps-root\tmake dir\n"), err)
        # A head that changes no covered path owes nothing.
        self.write("docs/y", "y\n")
        later = self.commit()
        self.assertEqual(self.run_skips("owed", head, later)[:2], (0, ""))

    def test_skips_owed_a_head_cannot_free_itself(self):
        self.table(row(covers=("src/a.rs",)))
        base = self.commit()
        # The head removes the row, narrows its covers, or makes it `ci`: the base's row still counts.
        for head_rows in ([], [row(covers=("docs/x",))], [row(runs_on="ci")]):
            self.git("checkout", "--quiet", "--detach", base)
            self.table(*head_rows)
            self.write("src/a.rs", "changed\n")
            head = self.commit()
            code, out, err = self.run_skips("owed", base, head)
            self.assertEqual((code, out), (0, "one\tvps\tbash scripts/one.sh\n"), (head_rows, err))
        # A row the head adds or widens counts as well.
        self.git("checkout", "--quiet", "--detach", base)
        self.table(row(covers=("src/a.rs", "docs/**")), row("new", covers=("docs/x",), runs_on="vps-root", how="x"))
        self.write("docs/x", "x\n")
        head = self.commit()
        code, out, err = self.run_skips("owed", base, head)
        self.assertEqual((code, out), (0, "one\tvps\tbash scripts/one.sh\nnew\tvps-root\tx\n"), err)

    def test_skips_owed_without_the_table_and_with_one_not_of_its_form(self):
        self.write("src/a.rs", "a\n")
        base = self.commit()
        self.write("src/a.rs", "b\n")
        head = self.commit()
        self.assertEqual(self.run_skips("owed", base, head)[:2], (0, ""))
        self.table(raw='{"rows": [{"id": "x"}]}')
        bad = self.commit()
        code, out, err = self.run_skips("owed", base, bad)
        self.assertEqual((code, out), (1, ""))
        self.assertEqual(err.count("\n"), 1, err)
        self.assertIn("skips_form@", err)
        code, _, err = self.run_skips("owed", base, "no-such-commit")
        self.assertEqual(code, 1, err)


class CheckSkips(unittest.TestCase):
    """./check's side of the skips (specs/ap/check.md R22): its own file, the line before the last, the removal."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="check-skips-test."))
        self.addCleanup(shutil.rmtree, self.root, True)
        self.tmp = self.root / "tmp"
        self.tmp.mkdir()
        repo = self.repo = self.root / "repo"
        for d in ("ci", "scripts", "github"):
            (repo / d).mkdir(parents=True)
        shutil.copy2(REPO / "check", repo / "check")
        # Stand-ins of the content steps: the first appends two ids and one of them twice, as skip sites do.
        (repo / "ci" / "check-content.py").write_text(
            "import os\n"
            "for i in ('second', 'first', 'second'):\n"
            "    with open(os.environ.get('CHECK_SKIPS') or os.devnull, 'a') as f: f.write(i + '\\n')\n"
            "print('skipped here: ' + os.environ.get('CHECK_SKIPS', 'unset'))\n")
        for name in ("check-map.py", "check-spec-trace.py", "check-schema-use.py", "skips.py"):
            (repo / "ci" / name).write_text("")
        for name in ("test-install-content.sh", "test-pc-pull.sh", "test-backup-store.sh"):
            (repo / "scripts" / name).write_text("exit 0\n")
        for d in ("github", "ci"):
            (repo / d / "test_none.py").write_text("import unittest\nclass T(unittest.TestCase):\n    def test_x(self): pass\n")

    def run_check(self, extra=None, mode="content"):
        env = dict(ENV, TMPDIR=str(self.tmp), **(extra or {}))
        done = subprocess.run(["bash", str(self.repo / "check"), mode], cwd=self.repo, capture_output=True, text=True,
                              timeout=120, env=env)
        return done.returncode, done.stdout.splitlines()

    # spec: specs/ap/check.md#R22
    def test_check_skips_names_each_skip_once_before_its_last_line_and_removes_its_file(self):
        code, lines = self.run_check()
        self.assertEqual(code, 0, lines)
        self.assertEqual(lines[-2:], ["check: skipped second first", "check: ok (content)"])
        made = lines[0].removeprefix("skipped here: ")
        self.assertTrue(made.startswith(str(self.tmp) + "/check-skips."), made)
        self.assertEqual(list(self.tmp.iterdir()), [], "the file and its directory are removed at the exit")
        # No skip: no such line, and the last line as before.
        (self.repo / "ci" / "check-content.py").write_text("")
        code, lines = self.run_check()
        self.assertEqual((code, lines[-1]), (0, "check: ok (content)"))
        self.assertFalse(any(x.startswith("check: skipped") for x in lines), lines)

    # spec: specs/ap/check.md#R22
    def test_check_skips_makes_its_own_file_but_on_its_re_exec(self):
        inherited = self.root / "inherited"
        inherited.write_text("")
        # A ./check that another program started makes its own file, whatever it inherits.
        code, lines = self.run_check({"CHECK_SKIPS": str(inherited)})
        self.assertEqual(code, 0, lines)
        self.assertNotEqual(lines[0], f"skipped here: {inherited}")
        self.assertEqual(inherited.read_text(), "")
        # Its re-exec under the lock, known by CHECK_SKIPS_KEEP, keeps the file of the first process; the
        # marker leaves its environment before its first step, and the file is removed at its exit.
        other = self.root / "other"
        other.mkdir()
        (other / "ids").write_text("")
        code, lines = self.run_check({"CHECK_SKIPS": str(other / "ids"), "CHECK_SKIPS_KEEP": str(other / "ids")})
        self.assertEqual(code, 0, lines)
        self.assertTrue((other / "ids").exists(), "a file of another form is neither kept nor removed")
        kept = self.root / "check-skips.kept"
        kept.mkdir()
        (kept / "ids.kept").write_text("earlier\n")
        (self.repo / "ci" / "check-content.py").write_text(
            "import os\nprint('skipped here: ' + os.environ['CHECK_SKIPS'] + ' ' + os.environ.get('CHECK_SKIPS_KEEP', 'unset'))\n")
        code, lines = self.run_check({"CHECK_SKIPS": str(kept / "ids.kept"), "CHECK_SKIPS_KEEP": str(kept / "ids.kept")})
        self.assertEqual(code, 0, lines)
        self.assertEqual(lines[0], f"skipped here: {kept / 'ids.kept'} unset")
        self.assertEqual(lines[-2:], ["check: skipped earlier", "check: ok (content)"])
        self.assertFalse(kept.exists())


if __name__ == "__main__":
    unittest.main()
