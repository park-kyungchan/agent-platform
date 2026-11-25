#!/usr/bin/env python3
"""The declared skips of ./check: ci/skips.json and the lines that append to CHECK_SKIPS (specs/ap/check.md R22 to R24).

  python3 -I -B ci/skips.py declared             exits 0 only when ci/skips.json has its form, every row's file exists,
                                                 every skip site names a declared id of its own file and every
                                                 declared id has a site in its file
  python3 -I -B ci/skips.py owed <base> <head>   one line "<id>\\t<runs_on>\\t<how>" per row of ci/skips.json, at the base
                                                 or at the head, whose covers meet a path changed between the two and
                                                 whose runs_on is not "ci" at either commit

A row is exactly {id, file, covers, runs_on, how}. A skip site is one line that holds the name CHECK_SKIPS and the id as a
literal, in one of three forms: a shell line `echo <id> >> "$CHECK_SKIPS"`, a Python line that holds CHECK_SKIPS and
`.write("<id>\\n"` (single quotes alike), a Rust test line `skipped(CHECK_SKIPS, "<id>")`. A line that holds CHECK_SKIPS and
appends (`>> "$CHECK_SKIPS"`, `.write(`, `skipped(CHECK_SKIPS`) in none of these forms is refused. The script reads Git and
files only: it starts no check.
"""
import json
import re
import subprocess
import sys
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TABLE = "ci/skips.json"
KEYS = {"id", "file", "covers", "runs_on", "how"}
RUNS_ON = ("vps-root", "vps", "ci")
ID = re.compile(r"[a-z0-9-]+")
SITES = (
    re.compile(r'\becho ([a-z0-9-]+) >> "\$CHECK_SKIPS"'),
    re.compile(r"CHECK_SKIPS.*\.write\(([\"'])([a-z0-9-]+)\\n\1"),
    re.compile(r'\bskipped\(CHECK_SKIPS, "([a-z0-9-]+)"\)'),
)
APPENDS = re.compile(r'>> ?"\$\{?CHECK_SKIPS|\.write\(|\bskipped\(CHECK_SKIPS\b')


class Refused(Exception):
    pass


def control(text):
    """Whether the text holds a control character (Unicode category Cc, as Rust's char::is_control)."""
    return any(unicodedata.category(ch) == "Cc" for ch in text)


def exact_path(text):
    """A path of the write-set grammar's exact form (specs/ap/unit.md R11)."""
    return bool(text) and all(c not in ("", ".", "..") and not any(x in c for x in "*?[") for c in text.split("/"))


def pattern_ok(text):
    """One pattern of the write-set grammar (specs/ap/unit.md R11): an exact path, or `<dir>/**`."""
    path = text[:-3] if text.endswith("/**") else text
    return 1 <= len(text.encode()) <= 200 and not control(text) and exact_path(path)


def meets(covers, path):
    """Whether a pattern of `covers` covers `path`, as the matcher of the write set reads it (specs/ap/unit-run.md R16)."""
    for c in covers:
        if c.endswith("/**"):
            if path.startswith(c[:-3] + "/") and len(path) > len(c) - 2:
                return True
        elif path == c:
            return True
    return False


def rows_of(text, where):
    """The rows of a text of ci/skips.json, or Refused with the first rule it breaks."""
    try:
        table = json.loads(text)
    except ValueError:
        raise Refused(f"skips_form@{where}: not JSON")
    if not isinstance(table, dict) or set(table) != {"rows"} or not isinstance(table["rows"], list):
        raise Refused(f'skips_form@{where}: not exactly {{"rows": [...]}}')
    seen = set()
    for i, row in enumerate(table["rows"]):
        at = f"skips_form@{where}: rows[{i}]"
        if not isinstance(row, dict) or set(row) != KEYS:
            raise Refused(f"{at} is not exactly {{id, file, covers, runs_on, how}}")
        if not isinstance(row["id"], str) or not ID.fullmatch(row["id"]):
            raise Refused(f"{at}.id is not lower-case letters, digits and hyphens")
        if row["id"] in seen:
            raise Refused(f"{at}.id {row['id']} is not unique")
        seen.add(row["id"])
        if not isinstance(row["file"], str) or not pattern_ok(row["file"]) or not exact_path(row["file"]):
            raise Refused(f"{at}.file is not a path of the repository")
        covers = row["covers"]
        if not isinstance(covers, list) or not 1 <= len(covers) <= 20 or not all(isinstance(c, str) and pattern_ok(c) for c in covers):
            raise Refused(f"{at}.covers is not 1 to 20 patterns of the write-set grammar")
        if row["runs_on"] not in RUNS_ON:
            raise Refused(f"{at}.runs_on is not one of {', '.join(RUNS_ON)}")
        how = row["how"]
        if not isinstance(how, str) or not how.strip() or control(how):
            raise Refused(f"{at}.how is not one command line")
    return table["rows"]


def sites(text):
    """(line number, id) of every skip site of a file's text; Refused for an appending line of no site form."""
    found = []
    for number, line in enumerate(text.splitlines(), 1):
        if "CHECK_SKIPS" not in line:
            continue
        ids = [m.group(m.lastindex) for form in SITES for m in form.finditer(line)]
        if not ids and APPENDS.search(line):
            raise Refused(f"skips_site_form@{{}}:{number}: a line that appends to CHECK_SKIPS without a literal id of a site form")
        found.extend((number, i) for i in ids)
    return found


def declared():
    rows = rows_of((ROOT / TABLE).read_text(encoding="utf-8"), TABLE)
    by_id = {r["id"]: r for r in rows}
    files = ["check"] + sorted(str(p.relative_to(ROOT)) for d in ("ci", "scripts") for p in (ROOT / d).glob("*.sh"))
    for r in rows:
        if not (ROOT / r["file"]).is_file():
            raise Refused(f"skips_file_missing@{TABLE}: {r['id']}: {r['file']} is not a file")
        if r["file"] not in files:
            files.append(r["file"])
    appended, count = set(), 0
    for f in files:
        path = ROOT / f
        if not path.is_file():
            continue
        try:
            found = sites(path.read_text(encoding="utf-8", errors="replace"))
        except Refused as e:
            raise Refused(str(e).replace("{}", f))
        for number, i in found:
            if i not in by_id:
                raise Refused(f"skips_undeclared@{f}:{number}: {i} is no row of {TABLE}")
            if by_id[i]["file"] != f:
                raise Refused(f"skips_other_file@{f}:{number}: {i} is a row of {by_id[i]['file']}")
            appended.add(i)
            count += 1
    for r in rows:
        if r["id"] not in appended:
            raise Refused(f"skips_no_site@{r['file']}: {r['id']} is declared and never appended")
    print(f"skips: {len(rows)} rows declared, {count} sites")


def git(*words):
    return subprocess.run(["git", "-C", str(ROOT), *words], capture_output=True, timeout=60)


def rows_at(commit):
    """The rows of ci/skips.json at a commit: none when the file is absent there."""
    kind = git("cat-file", "-t", f"{commit}:{TABLE}")
    if kind.returncode != 0:
        return []
    if kind.stdout.strip() != b"blob":
        raise Refused(f"skips_form@{commit}:{TABLE}: not a file")
    shown = git("cat-file", "blob", f"{commit}:{TABLE}")
    if shown.returncode != 0:
        raise Refused(f"skips_git@{commit}:{TABLE}: git cat-file failed")
    return rows_of(shown.stdout.decode("utf-8", errors="replace"), f"{commit}:{TABLE}")


def owed(base, head):
    for c in (base, head):
        if git("rev-parse", "--verify", "--quiet", f"{c}^{{commit}}").returncode != 0:
            raise Refused(f"skips_commit@{c}: not a commit")
    diff = git("diff", "--name-only", "--no-renames", "-z", base, head)
    if diff.returncode != 0:
        raise Refused(f"skips_git@{base}..{head}: git diff failed")
    paths = [p for p in diff.stdout.decode("utf-8", errors="surrogateescape").split("\0") if p]
    at_base, at_head = rows_at(base), rows_at(head)
    order = [r["id"] for r in at_head] + [r["id"] for r in at_base if r["id"] not in {h["id"] for h in at_head}]
    both = {}
    for r in at_base + at_head:
        both.setdefault(r["id"], []).append(r)
    for i in order:
        rows = both[i]
        if not any(meets(r["covers"], p) for r in rows for p in paths):
            continue
        host = [r for r in reversed(rows) if r["runs_on"] != "ci"]
        if host:
            print(f"{i}\t{host[0]['runs_on']}\t{host[0]['how']}")


def main(argv):
    try:
        if argv == ["declared"]:
            declared()
        elif len(argv) == 3 and argv[0] == "owed":
            owed(argv[1], argv[2])
        else:
            print("usage: ci/skips.py declared | ci/skips.py owed <base> <head>", file=sys.stderr)
            return 2
    except Refused as e:
        print(e, file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
