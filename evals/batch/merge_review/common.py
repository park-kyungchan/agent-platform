"""What evals/batch/merge_review/build and read share beyond evals/batch/lib.py and evals/batch/mining/common.py:
the Git reads, protected paths, model selection, items.json validation, finding pass, and GH_BIN comment posting.

Both programs load this file by its path and bind `L` (evals/batch/lib.py) and `M` (evals/batch/mining/common.py).
The standard library only; no socket, no model. Contract: evals/batch/merge_review/README.md.
"""
import json
import os
import re
import subprocess
import unicodedata

L = None  # evals/batch/lib.py, set by caller
M = None  # evals/batch/mining/common.py, set by caller

HERE = os.path.dirname(os.path.abspath(__file__))
JOB = "merge-review"
DETECTOR = "batch:merge-review"
AGENT = "batch-merge-review"
PREFIX = "bmr-"
KEY_PREFIX = "batch-merge-review-"
STATES = ("requested", "settled", "too_large", "over_max")
READERS = ("model", "strong")
COUNT_WORDS = ("read", "refused", "errored", "unreadable", "stale", "duplicate", "skipped")
TEXT_LIMIT = 300_000
SUMMARY_LIMIT = 600
EVIDENCE_LIMIT = 2000
COMMENT_LIMIT = 60_000
MAX_DEFECTS_SHOWN = 5
GIT_TIMEOUT = 300
GH_TIMEOUT = 60
LAST_LINE = "A claimed defect stays a finding of severity info until its command reproduces it at the merged commit."

# Pattern from ci/check-spec-trace.py (a test fails when it differs).
REQ_RE = re.compile(r"^- R([0-9]+):")
SPEC_RE = re.compile(r"specs/[A-Za-z0-9._/-]+\.md")
REPO_NAME = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")
COMMIT = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")
MODEL_RE = re.compile(r"^claude-([a-z][a-z0-9]*)-([a-z0-9.-]+)$")

# Protected path lists of crates/ap/src/unit/drive.rs (a test reads that file and fails when these differ).
PROTECTED_EXACT = ("check", "review.md")
PROTECTED_NAMES = (
    "clippy.toml",
    ".clippy.toml",
    "rustfmt.toml",
    ".rustfmt.toml",
    "rust-toolchain",
    "rust-toolchain.toml",
    "build.rs",
    "cargo.toml",
    "cargo.lock",
)
PROTECTED_CONFIGS = (".cargo/config.toml", ".cargo/config")
PROTECTED_TREES = (
    "ci",
    ".github",
    "specs",
    "scripts",
    ".cargo",
    "deploy",
    "etc",
    "content",
    "github",
    "ontology",
)

PAGE_TEXT_MEMBERS = ("objective", "interface", "invariants", "not_built", "notes", "verifier_questions")
PAGE_MEMBERS = (
    "objective",
    "interface",
    "invariants",
    "acceptance",
    "not_built",
    "write_set",
    "read_first",
    "specs",
    "notes",
    "verifier_questions",
)

GIT_ENV = {
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_CONFIG_GLOBAL": "/dev/null",
    "GIT_ATTR_NOSYSTEM": "1",
    "GIT_OPTIONAL_LOCKS": "0",
    "GIT_TERMINAL_PROMPT": "0",
    "GIT_NO_LAZY_FETCH": "1",
}
GIT_OPTS = (
    "-c", "core.quotePath=true",
    "-c", "diff.suppressBlankEmpty=false",
    "-c", "core.attributesFile=/dev/null",
    "-c", "protocol.allow=never",
    "-c", "core.hooksPath=/dev/null",
)
DIFF_OPTS = (
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--no-renames",
    "--full-index",
    "--diff-algorithm=myers",
    "--unified=3",
    "--inter-hunk-context=0",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--no-relative",
    "-O/dev/null",
    "--submodule=short",
    "--indent-heuristic",
)


def bind(lib_mod, mining_mod):
    global L, M
    L = lib_mod
    M = mining_mod
    M.L = lib_mod


def load_text(name):
    with open(os.path.join(HERE, name), encoding="utf-8") as f:
        return f.read()


def load_json(name):
    with open(os.path.join(HERE, name), encoding="utf-8") as f:
        return json.load(f)


def protected(path):
    """Whether `path` is protected by protected() of crates/ap/src/unit/drive.rs (case-insensitive)."""
    p = path.lower()
    first = p.split("/", 1)[0]
    last = p.rsplit("/", 1)[-1]
    config = any(p == c or p.endswith("/" + c) for c in PROTECTED_CONFIGS)
    return p in PROTECTED_EXACT or last in PROTECTED_NAMES or first in PROTECTED_TREES or config


def model_family(model_id):
    """The family of a Claude API identifier `claude-<family>-<rest>`, or None."""
    if not isinstance(model_id, str):
        return None
    m = MODEL_RE.match(model_id)
    return m.group(1) if m else None


def verifier_names_family(verifier_model, family):
    """Whether verifier.model equals `family` or begins with `claude-<family>-`."""
    if not isinstance(verifier_model, str) or not verifier_model:
        return False
    return verifier_model == family or verifier_model.startswith(f"claude-{family}-")


def choose_reader(is_protected, verifier_model, model_fam, strong_fam):
    """Returns 'model' or 'strong' per point 8."""
    reader = "strong" if is_protected else "model"
    fam = strong_fam if reader == "strong" else model_fam
    if verifier_names_family(verifier_model, fam):
        reader = "model" if reader == "strong" else "strong"
    return reader


def spec_paths_of(unit):
    """Unique spec file paths named by the unit page, in byte order."""
    found = set()
    specs = unit.get("specs")
    if isinstance(specs, list):
        for entry in specs:
            if isinstance(entry, str):
                p = entry.split("#", 1)[0].strip()
                if p:
                    found.add(p)
    for member in PAGE_TEXT_MEMBERS:
        val = unit.get(member)
        if isinstance(val, str):
            found.update(SPEC_RE.findall(val))
    rf = unit.get("read_first")
    if isinstance(rf, list):
        for entry in rf:
            if isinstance(entry, str):
                found.update(SPEC_RE.findall(entry))
    acc = unit.get("acceptance")
    if isinstance(acc, list):
        for entry in acc:
            if isinstance(entry, dict) and isinstance(entry.get("check"), str):
                found.update(SPEC_RE.findall(entry["check"]))
    return sorted(found, key=lambda p: p.encode("utf-8"))


class Clone:
    """One repository read from Git's objects only; never reads working tree files or mutates the clone."""

    def __init__(self, name, path):
        self.name = name
        self.path = path
        self.env = dict(os.environ, **GIT_ENV, GIT_CEILING_DIRECTORIES=os.path.dirname(path.rstrip("/")) or "/")
        out = self.git(["rev-parse", "--git-path", "info/attributes"])
        if out is None:
            raise L.Refuse("repo_unreadable", repository=name)
        rel = out.decode("utf-8", "surrogateescape").rstrip("\r\n")
        if not rel or os.path.lexists(os.path.join(self.path, rel)):
            raise L.Refuse("repo_unreadable", repository=name)

    def git(self, args, data=None):
        command = ["git", *GIT_OPTS, "-C", self.path, *args]
        try:
            done = subprocess.run(command, input=data, capture_output=True, timeout=GIT_TIMEOUT, env=self.env)
        except (OSError, subprocess.TimeoutExpired):
            return None
        return done.stdout if done.returncode == 0 else None

    def resolve_commit(self, commit):
        """Returns (first_parent, commit) when `commit` is a commit with a first parent in this clone, else None."""
        if not isinstance(commit, str) or not COMMIT.match(commit):
            return None
        c = self.git(["rev-parse", "--verify", "--quiet", commit + "^{commit}"])
        p = self.git(["rev-parse", "--verify", "--quiet", commit + "^1^{commit}"])
        if c is None or p is None:
            return None
        c_sha = c.decode("ascii", "replace").strip()
        p_sha = p.decode("ascii", "replace").strip()
        if c_sha != commit:
            return None
        return p_sha, c_sha

    def changed_paths(self, parent, commit):
        out = self.git([f"--attr-source={commit}", "diff", *DIFF_OPTS, "--name-only", "-z", parent, commit])
        if out is None:
            raise L.Refuse("repo_unreadable", repository=self.name)
        return [p.decode("utf-8", "surrogateescape") for p in out.split(b"\0") if p]

    def diff(self, parent, commit, paths):
        if not paths:
            return ""
        out = self.git(["--literal-pathspecs", f"--attr-source={commit}", "diff", *DIFF_OPTS,
                        parent, commit, "--", *paths])
        if out is None:
            raise L.Refuse("repo_unreadable", repository=self.name)
        return out.decode("utf-8", "replace")

    def spec_requirements(self, commit, path):
        """Lines matching REQ_RE in file order when `path` is a regular file at `commit`, else None."""
        if not path or path.startswith("/") or "\0" in path or any(p in ("", ".", "..") for p in path.split("/")):
            return None
        out = self.git(["--literal-pathspecs", "ls-tree", "-z", commit, "--", path])
        if out is None:
            return None
        obj = None
        for rec in out.split(b"\0"):
            if not rec:
                continue
            meta, _, found_path = rec.partition(b"\t")
            parts = meta.split(b" ")
            if len(parts) == 3 and found_path.decode("utf-8", "surrogateescape") == path:
                mode, kind, sha = parts[0].decode(), parts[1].decode(), parts[2].decode()
                if kind == "blob" and mode in ("100644", "100755"):
                    obj = sha
                break
        if obj is None:
            return None
        raw = self.git(["cat-file", "blob", obj])
        if raw is None:
            return None
        return [line for line in raw.decode("utf-8", "replace").split("\n") if REQ_RE.match(line)]


# The store: engine with caller's environment (ONT_ROLE unchanged, no member role).
def engine(store):
    e = L.Engine(store, AGENT)
    e.env = dict(os.environ)
    return e


def read_findings(eng):
    found = M.search(eng, "finding", [{"eq": ["detector", DETECTOR]}])
    return {k: f for k, f in found.items() if f.get("detector") == DETECTOR}


def signature(repo, pr):
    return f"{repo}#{pr}"


def finding_key(sig):
    return KEY_PREFIX + L.sha256(sig)[:16]


def load_items(path):
    """Validates items.json of build's form; refuses items_unreadable or academy_path."""
    def bad():
        raise L.Refuse("items_unreadable")

    try:
        with open(path, encoding="utf-8") as f:
            doc = json.load(f)
    except (OSError, ValueError):
        bad()
    top = {"job", "since", "until", "model", "strong", "items"}
    if not isinstance(doc, dict) or set(doc) != top or doc["job"] != JOB:
        bad()
    try:
        M.window(doc["since"], doc["until"])
    except L.Refuse:
        bad()
    mf, sf = model_family(doc["model"]), model_family(doc["strong"])
    if not mf or not sf or mf == sf:
        bad()
    items = doc["items"]
    if not isinstance(items, list):
        bad()
    item_keys = {"unit", "repository", "pull_request", "commit", "verifier", "protected", "reader", "specs", "key", "state"}
    seen_keys = set()
    for it in items:
        if not isinstance(it, dict) or set(it) != item_keys:
            bad()
        if not (isinstance(it["unit"], str) and it["unit"]
                and isinstance(it["repository"], str) and REPO_NAME.match(it["repository"])
                and isinstance(it["pull_request"], int) and not isinstance(it["pull_request"], bool)
                and it["pull_request"] > 0
                and isinstance(it["commit"], str) and COMMIT.match(it["commit"])
                and (it["verifier"] is None or isinstance(it["verifier"], str))
                and isinstance(it["protected"], bool)
                and it["reader"] in READERS
                and isinstance(it["specs"], list)
                and all(isinstance(s, str) and s for s in it["specs"])
                and isinstance(it["key"], str) and M.HEX64.match(it["key"])
                and it["state"] in STATES):
            bad()
        if it["key"] in seen_keys:
            bad()
        seen_keys.add(it["key"])
        L.refuse_academy(it["repository"])
        for sp in it["specs"]:
            L.refuse_academy(sp)
    return doc


def reader_model(doc, item):
    return doc["strong"] if item["reader"] == "strong" else doc["model"]


def finding_fields(doc, item, defects):
    sig = signature(item["repository"], item["pull_request"])
    nk = finding_key(sig)
    model_id = reader_model(doc, item)
    summary = (f"{sig} (unit {item['unit']}, {item['commit'][:12]}): {len(defects)} defects claimed "
               f"against its page by {model_id}, none reproduced")
    parts = [f"key {item['key']}"]
    for i, d in enumerate(defects[:MAX_DEFECTS_SHOWN], 1):
        parts.append(f"{i}. {d['title']} at {d['where']}, clause {d['clause']}, "
                     f"reproduce: {d['command']}, why: {d['why']}")
    evidence = "; ".join(parts)
    return nk, L.cut(summary, SUMMARY_LIMIT), L.cut(L.plain(evidence), EVIDENCE_LIMIT)


def finding_pass(eng, doc, read_items, verdicts, stored):
    """Finding pass per point 15: absent -> open_finding (cap 10 applied); open+info with changed evidence ->
    see_finding_again (no severity); any other state or severity -> left."""
    opened, seen, failed = [], [], []
    left = over_cap = already_applied = applied_opens = 0
    for item in read_items:
        defects = verdicts[item["key"]]["defects"]
        if not defects:
            continue
        nk, summary, evidence = finding_fields(doc, item, defects)
        fid = L.finding_id(nk)
        f = stored.get(nk)
        if f is None:
            if applied_opens >= L.CAP:
                over_cap += 1
                continue
            action = "open_finding"
            params = {"finding": fid, "kind": "drift", "detector": DETECTOR, "summary": summary,
                      "evidence": evidence, "severity": "info"}
        elif f.get("state") == "open" and f.get("severity") == "info":
            if f.get("evidence") == evidence:
                continue
            action = "see_finding_again"
            params = {"finding": fid, "evidence": evidence}
        else:
            left += 1
            continue
        rid = PREFIX + L.sha256(L.canon([action, nk, item["key"], params]))[:40]
        body = {"action_type": action, "parameters": params, "agent": AGENT, "request_id": rid}
        outcome, code = eng.apply(body)
        if outcome == "applied":
            if action == "open_finding":
                opened.append(nk)
                applied_opens += 1
                stored[nk] = {"state": "open", "severity": "info", "evidence": evidence, "detector": DETECTOR}
            else:
                seen.append(nk)
                stored[nk] = {**f, "evidence": evidence}
        elif outcome == "already_applied":
            already_applied += 1
        else:
            failed.append({"key": nk, "code": code})
    return {"opened": opened, "seen": seen, "left": left, "over_cap": over_cap,
            "already_applied": already_applied, "failed": failed}


def comment_clean(text):
    """Each control character except line feed replaced by a space."""
    return "".join(" " if (unicodedata.category(c) == "Cc" and c != "\n") else c for c in text)


def fenced_block(text):
    cleaned = comment_clean(text)
    runs = re.findall(r"`+", cleaned)
    n = max(max((len(r) for r in runs), default=2) + 1, 3)
    fence = "`" * n
    if cleaned and not cleaned.endswith("\n"):
        cleaned += "\n"
    return f"{fence}\n{cleaned}{fence}"


def open_fence_of(text):
    """Returns the currently open backtick fence string at the end of `text`, or None."""
    open_f = None
    for line in text.split("\n"):
        if open_f is None:
            if re.fullmatch(r"`{3,}", line):
                open_f = line
        else:
            if line == open_f:
                open_f = None
    return open_f


def comment_body(doc, item, defects):
    sig = signature(item["repository"], item["pull_request"])
    marker = f"<!-- batch:merge-review {sig} -->"
    model_id = reader_model(doc, item)
    header = (f"Unit {item['unit']}, commit {item['commit']}, key {item['key']}, "
              f"model {model_id}: {len(defects)} defects claimed.")
    if not defects:
        return f"{marker}\n{header}\n\nNo defect claimed.\n"
    sections = [marker, header]
    for i, d in enumerate(defects[:MAX_DEFECTS_SHOWN], 1):
        block = [
            f"### Defect {i}",
            "title:\n" + fenced_block(d["title"]),
            "where:\n" + fenced_block(d["where"]),
            "clause:\n" + fenced_block(d["clause"]),
            "command:\n" + fenced_block(d["command"]),
            "why:\n" + fenced_block(d["why"]),
        ]
        sections.append("\n\n".join(block))
    prefix = "\n\n".join(sections)
    tail = "\n\n" + LAST_LINE + "\n"
    full = prefix + tail
    if len(full.encode("utf-8")) <= COMMENT_LIMIT:
        return full
    # Cut prefix on a UTF-8 character boundary while closing any open fence and keeping LAST_LINE.
    budget = COMMENT_LIMIT - len(tail.encode("utf-8"))
    cut_prefix = L.cut(prefix, budget)
    fence = open_fence_of(cut_prefix)
    if fence is not None:
        close = "\n" + fence
        cut_prefix = L.cut(prefix, budget - len(close.encode("utf-8")))
        while True:
            fence2 = open_fence_of(cut_prefix)
            if fence2 is None:
                break
            close = "\n" + fence2
            if len((cut_prefix + close + tail).encode("utf-8")) <= COMMENT_LIMIT:
                cut_prefix = cut_prefix + close
                break
            cut_prefix = L.cut(cut_prefix, len(cut_prefix.encode("utf-8")) - len(close.encode("utf-8")))
    return cut_prefix + tail


def gh_call(gh_bin, args, stdin=None):
    try:
        done = subprocess.run([gh_bin, *args], input=stdin, capture_output=True, text=True, timeout=GH_TIMEOUT)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return done.stdout if done.returncode == 0 else None


def post_comments(doc, read_items, verdicts):
    """Posts at most one comment per pull request via GH_BIN per point 16."""
    gh_bin = os.environ.get("GH_BIN", "gh")
    commented, failed = [], []
    comment_present = 0
    for item in read_items:
        repo = item["repository"]
        pr = str(item["pull_request"])
        sig = signature(repo, item["pull_request"])
        marker = f"<!-- batch:merge-review {sig} -->"
        out = gh_call(gh_bin, ["pr", "view", pr, "--repo", repo, "--json", "comments"])
        if out is None:
            failed.append({"key": item["key"], "code": "gh_failed"})
            continue
        try:
            parsed = json.loads(out)
        except ValueError:
            parsed = None
        if not isinstance(parsed, dict) or not isinstance(parsed.get("comments"), list):
            failed.append({"key": item["key"], "code": "gh_failed"})
            continue
        present = False
        for c in parsed["comments"]:
            if isinstance(c, dict) and isinstance(c.get("body"), str):
                first = c["body"].split("\n", 1)[0].rstrip("\r")
                if first == marker:
                    present = True
                    break
        if present:
            comment_present += 1
            continue
        body = comment_body(doc, item, verdicts[item["key"]]["defects"])
        posted = gh_call(gh_bin, ["pr", "comment", pr, "--repo", repo, "--body-file", "-"], stdin=body)
        if posted is None:
            failed.append({"key": item["key"], "code": "gh_failed"})
            continue
        commented.append(item["key"])
    return commented, comment_present, failed
