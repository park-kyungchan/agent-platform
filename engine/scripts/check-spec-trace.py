#!/usr/bin/env python3
"""The trace between the requirements of specs/ont/ and the tests that cite them.

Usage: check-spec-trace.py [<repository root>]   (default: the current directory)

A requirement is a line of a tracked specs/ont/**/*.md file that begins with a dash, a space,
R and digits and a colon; its id is the file's path, '#' and R<digits>. A citation is the
word 'spec:', a space and such an id, in a tracked file that is not Markdown. The file
scripts/spec-trace-untested.txt lists, one id per line and sorted, the requirements no test
exercises; a missing or untracked file is an empty list. Faults, one line each, sorted:
requirement_untested, citation_missing, requirement_duplicate, untested_listed_cited,
untested_listed_unknown, untested_list_unsorted. The last line counts requirements,
citations, listed untested ids and errors; exit 1 on a fault, 0 without one, 2 when the
files cannot be listed or the argument is bad. Reads only the tracked files; writes nothing.
"""
import os
import re
import stat
import subprocess
import sys
from pathlib import Path

REQ_RE = re.compile(r"^- R([0-9]+):")
CITE_RE = re.compile(r"spec: (specs/[^\s#]+\.md)#R([0-9]+)")
UNTESTED_LIST = "scripts/spec-trace-untested.txt"
ROOT_PREFIX = "specs/ont/"


def unreadable(reason):
    print(f"spec_trace_unreadable {reason}")
    return 2


def tracked_files(root):
    try:
        out = subprocess.run(
            ["git", "-C", str(root), "-c", "core.quotePath=false", "ls-files", "-z"],
            capture_output=True,
            check=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    return sorted(p.decode("utf-8", "surrogateescape") for p in out.split(b"\0") if p)


def read_lines(root, rel):
    path = root / rel
    try:
        st = os.lstat(path)
    except OSError:
        return None
    if not stat.S_ISREG(st.st_mode):
        return None
    try:
        data = path.read_bytes()
    except OSError:
        return None
    return data.decode("utf-8", "replace").split("\n")


def is_spec(rel):
    return rel.startswith(ROOT_PREFIX) and rel.endswith(".md")


def main(argv):
    args = argv[1:]
    if len(args) > 1:
        return unreadable("usage: check-spec-trace.py [<repository root>]")
    root = Path(args[0]) if args else Path.cwd()
    if not root.is_dir():
        return unreadable(f"root is not a directory: {root}")
    tracked = tracked_files(root)
    if tracked is None:
        return unreadable("git ls-files failed")

    ids = set()
    traced = []
    faults = []
    for rel in tracked:
        if not is_spec(rel):
            continue
        lines = read_lines(root, rel)
        if lines is None:
            continue
        seen = set()
        for line in lines:
            m = REQ_RE.match(line)
            if not m:
                continue
            rid = f"{rel}#R{m.group(1)}"
            if rid in seen:
                faults.append(f"requirement_duplicate@{rid}")
                continue
            seen.add(rid)
            ids.add(rid)
            traced.append(rid)

    cited = set()
    citations = 0
    for rel in tracked:
        if rel.endswith(".md"):
            continue
        lines = read_lines(root, rel)
        if lines is None:
            continue
        for no, line in enumerate(lines, 1):
            for m in CITE_RE.finditer(line):
                citations += 1
                rid = f"{m.group(1)}#R{m.group(2)}"
                cited.add(rid)
                if rid not in ids:
                    faults.append(f"citation_missing@{rel}:{no} {rid}")

    listed = []
    if UNTESTED_LIST in set(tracked):
        lines = read_lines(root, UNTESTED_LIST)
        if lines is not None:
            if lines and lines[-1] == "":
                lines.pop()
            previous = None
            seen_lines = set()
            for no, line in enumerate(lines, 1):
                if (previous is not None and line.encode("utf-8", "surrogateescape") < previous) or line in seen_lines:
                    faults.append(f"untested_list_unsorted@{UNTESTED_LIST}:{no}")
                previous = line.encode("utf-8", "surrogateescape")
                seen_lines.add(line)
                listed.append(line)
    listed_set = set(listed)

    for rid in sorted(listed_set):
        if rid in cited:
            faults.append(f"untested_listed_cited@{rid}")
        if rid not in ids:
            faults.append(f"untested_listed_unknown@{rid}")

    for rid in traced:
        if rid not in cited and rid not in listed_set:
            faults.append(f"requirement_untested@{rid}")

    for f in sorted(set(faults)):
        print(f)
    errors = len(set(faults))
    print(
        f"spec-trace: {len(traced)} requirements, {citations} citations, "
        f"{len(listed_set)} listed untested, {errors} errors"
    )
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
