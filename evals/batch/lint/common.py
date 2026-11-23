"""What the two programs of the spec-lint job share beyond lib and the evals job's common module: the names of the job,
the form of a spec file's request, the reading of a repository's spec files from Git's objects and the form of
files.json.

The programs load this file by its path and then call `bind` with the lib they loaded (evals/batch/lib.py), so that all of
them use one `Refuse`. The standard library only; no socket, no model. Contract: evals/batch/lint/README.md.
"""
import importlib.machinery
import importlib.util
import json
import os
import re

L = None  # evals/batch/lib.py, set by `bind`
C = None  # evals/batch/evals/common.py, loaded by `bind`

HERE = os.path.dirname(os.path.abspath(__file__))
JOB = "spec-lint"
DETECTOR = "batch:spec-lint"
AGENT = "batch-spec-lint"
PREFIX = "bl-"
KEY_PREFIX = "batch-spec-lint-"
KINDS = ("ambiguity", "contradiction", "gap")
STATES = ("requested", "settled", "too_large")
TEXT_LIMIT = 300_000
EVIDENCE = 2000
SUMMARY = 600
# The pattern of a requirement line of ci/check-spec-trace.py; a test fails when it differs.
REQ_RE = re.compile(r"^- R([0-9]+):")
REPO_NAME = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
RULE = re.compile(r"^R[0-9]+$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
COMMIT = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")


def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, path)
    mod = importlib.util.module_from_spec(importlib.util.spec_from_loader(name, loader))
    loader.exec_module(mod)
    return mod


def bind(lib):
    """Sets the lib of this module and of the evals job's common module to the one the calling program loaded, and
    returns that common module."""
    global L, C
    L = lib
    C = load("evals_common", os.path.join(HERE, "..", "evals", "common.py"))
    C.L = lib
    return C


def text_file(name):
    with open(os.path.join(HERE, name), encoding="utf-8") as f:
        return f.read()


def json_file(name):
    with open(os.path.join(HERE, name), encoding="utf-8") as f:
        return json.load(f)


def rules_of(text):
    """The rules of a spec file in the order of their first line: `R<digits>` of each line that REQ_RE matches, a later
    line of the same rule none."""
    found = []
    for line in text.split("\n"):
        m = REQ_RE.match(line)
        if m and "R" + m.group(1) not in found:
            found.append("R" + m.group(1))
    return found


def user_text(repository, path, text):
    """The user message of a file: the repository, the path and the file whole."""
    return "Repository: %s\nSpec: %s\n\n<spec path=\"%s\">\n%s%s</spec>\n" % (
        repository, path, path, text, "" if text.endswith("\n") else "\n")


def tree_of(clone):
    """Every tracked entry of the clone's commit as (mode, object, path), by path."""
    out = clone.git(["ls-tree", "-r", "-z", "--full-tree", clone.commit])
    if out is None:
        raise L.Refuse("repo_unreadable", repository=clone.name)
    entries = []
    for rec in out.split(b"\0"):
        if not rec:
            continue
        meta, _, path = rec.partition(b"\t")
        parts = meta.split(b" ")
        if len(parts) != 3:
            raise L.Refuse("repo_unreadable", repository=clone.name)
        entries.append((parts[0].decode(), parts[2].decode(), path.decode("utf-8", "surrogateescape")))
    return sorted(entries, key=lambda e: e[2])


def objects_of(clone, ids):
    """The bytes of each blob, by object id."""
    wanted = sorted(set(ids))
    if not wanted:
        return {}
    out = clone.git(["cat-file", "--batch"], ("\n".join(wanted) + "\n").encode())
    if out is None:
        raise L.Refuse("repo_unreadable", repository=clone.name)
    got, i = {}, 0
    for obj in wanted:
        nl = out.find(b"\n", i)
        header = out[i:nl].split(b" ") if nl >= 0 else []
        if len(header) != 3 or header[0].decode("ascii", "replace") != obj or header[1] != b"blob" \
                or not header[2].isdigit():
            raise L.Refuse("repo_unreadable", repository=clone.name)
        size = int(header[2])
        got[obj] = out[nl + 1:nl + 1 + size]
        i = nl + 1 + size + 1
    return got


def clone_at(name, path, rev):
    """The clone at one commit (the evals job's Clone); `repo_unreadable` names the repository."""
    try:
        clone = C.Clone(path, rev)
    except L.Refuse:
        raise L.Refuse("repo_unreadable", repository=name)
    clone.name = name
    return clone


def spec_files(clone, counts):
    """The regular files below `specs/` ending `.md` at the clone's commit, as (path, text, rules) by path. A tracked
    path of the Academy is not opened and is counted `academy_skipped`."""
    wanted = []
    for mode, obj, path in tree_of(clone):
        if L.is_academy(path):
            counts["academy_skipped"] += 1
        elif mode in ("100644", "100755") and path.startswith("specs/") and path.endswith(".md"):
            wanted.append((path, obj))
    data = objects_of(clone, [obj for _, obj in wanted])
    found = []
    for path, obj in wanted:
        text = data[obj].decode("utf-8", "replace")
        found.append((path, text, rules_of(text)))
    return found


def load_files(path):
    """files.json in the form `build` writes: the list of its files. Refuses `files_unreadable`, and `academy_path` for
    a repository or a path of the Academy that it names."""
    def bad():
        raise L.Refuse("files_unreadable")

    try:
        with open(path, encoding="utf-8") as f:
            doc = json.load(f)
    except (OSError, ValueError):
        bad()
    if not isinstance(doc, dict) or set(doc) != {"job", "model", "files"} or doc["job"] != JOB \
            or not isinstance(doc["model"], str) or not isinstance(doc["files"], list):
        bad()
    keys, commits, places = set(), {}, set()
    for f in doc["files"]:
        if not isinstance(f, dict) or set(f) != {"key", "repository", "commit", "path", "rules", "state"}:
            bad()
        if not (isinstance(f["key"], str) and HEX64.match(f["key"]) and isinstance(f["repository"], str)
                and REPO_NAME.match(f["repository"]) and isinstance(f["commit"], str) and COMMIT.match(f["commit"])
                and isinstance(f["path"], str) and f["path"].startswith("specs/") and f["path"].endswith(".md")
                and f["state"] in STATES and isinstance(f["rules"], list)
                and all(isinstance(r, str) and RULE.match(r) for r in f["rules"])
                and len(set(f["rules"])) == len(f["rules"])):
            bad()
        place = (f["repository"], f["path"])
        if f["key"] in keys or place in places or commits.setdefault(f["repository"], f["commit"]) != f["commit"]:
            bad()
        keys.add(f["key"])
        places.add(place)
    for f in doc["files"]:
        L.refuse_academy(f["repository"])
        L.refuse_academy(f["path"])
    return doc["files"]
