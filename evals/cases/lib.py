"""The rules of evals/cases: which Markdown files hold a case's facts, which lines a change removes without any file
keeping them, which cases a change reaches, and when that rule reproduced a gate. The programs `index`, `lost`, `select`
and `reproduce` load this file by its path; it loads evals/batch/lib.py by its path. The standard library only; no
model, no network; Git is only read. Contract: evals/cases/README.md.

The digest of this file is `map_digest` of every record, so a change of a rule here asks for a new reproduction.
"""
import fcntl
import hashlib
import importlib.machinery
import importlib.util
import json
import os
import re
import shlex
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))  # the repository this program lives in
SELF = os.path.join(HERE, "lib.py")


def _load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, path)
    mod = importlib.util.module_from_spec(importlib.util.spec_from_loader(name, loader))
    loader.exec_module(mod)
    return mod


B = _load("batch_lib", os.path.join(ROOT, "evals", "batch", "lib.py"))
Refuse = B.Refuse

SUITES = ("routing", "intake")
HOST_DIRECTORY = "/var/lib/agent-platform/cases"
LOCK_WAIT = 600  # seconds a call waits for the lock of the directory before it refuses `busy`
RECORD_LIMIT = 60000  # bytes of one record line
KPI_NAME = "case_map"
GIT_TIMEOUT = 300
MARKDOWN = ".md"
KEPT_ENDINGS = (".md", ".rs", ".py", ".sh", ".toml", ".json", ".yml", ".yaml", ".service", ".timer", ".txt")
LINE_TEXT = 300  # bytes of a lost line's text in the output
KPI_DEFINITION = ("Whether evals/cases/select names the cases a full gate failed on with fewer cases than the suites "
                  "hold. evals/cases/reproduce --record sets the value `reproduced <map_digest> failures <f> "
                  "selected <s>/<n>` or `not_reproduced ...`; map_digest is the SHA-256 of evals/cases/lib.py; the "
                  "evidence names the gate file's digest and the change.")
# The common English words a line's words are less. Words of four or more letters only: the rule never looks at others.
COMMON = frozenset("""
about above after again also always among another because been before being below between both cannot could does done
down during each either else enough even ever every from have having here into itself just like made make many more
most much must never next none once only other over same several should since some still such than that their them
then there these they this those through under until upon used uses using very were what when where whether which
while will with within without would your
""".split())


# ---------------------------------------------------------------- arguments and refusals
def parse(argv, spec):
    """`spec` maps each flag to whether it may repeat. Returns flag -> list of values; every other form is `usage`."""
    out = {}
    i = 0
    while i < len(argv):
        flag = argv[i]
        if flag not in spec or i + 1 >= len(argv):
            raise Refuse("usage")
        value = argv[i + 1]
        if value == "" or value.startswith("--") or (flag in out and not spec[flag]):
            raise Refuse("usage")
        out.setdefault(flag, []).append(value)
        i += 2
    return out


NAME = re.compile(r"^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$")


def pair(value):
    """`left=right`, split at the first `=`, both not empty."""
    left, sep, right = value.partition("=")
    if not sep or not left or not right:
        raise Refuse("usage")
    return left, right


def repositories(values):
    """[(owner/name, clone path)] in order; a name twice, or a name that is not owner/name, is `usage`."""
    out = []
    for v in values:
        name, clone = pair(v)
        if not NAME.match(name) or name in [n for n, _ in out]:
            raise Refuse("usage")
        out.append((name, clone))
    return out


def revisions(values, known):
    """{owner/name: rev} of `--at`; a name no `--repository` gave, or a rev that begins `-`, is `usage`."""
    out = {}
    for v in values:
        name, rev = pair(v)
        if name not in known or name in out or rev.startswith("-"):
            raise Refuse("usage")
        out[name] = rev
    return out


def case_args(values):
    """[(suite, file)] of `--cases`; a suite that is not `routing` or `intake` is `usage`."""
    out = []
    for v in values:
        suite, file = pair(v)
        if suite not in SUITES:
            raise Refuse("usage")
        out.append((suite, file))
    return out


def rev_value(value):
    if value.startswith("-"):
        raise Refuse("usage")
    return value


def academy(names=(), paths=()):
    """The Academy rule asked of every repository name and of every path, first as given, then resolved one
    component at a time. Refuses `academy_path`."""
    for n in names:
        B.refuse_academy(n)
    for p in paths:
        B.resolve(p)


def main(run):
    """Prints the lines `run(argv)` returns and exits with its code; a refusal is one line, exit 2."""
    try:
        lines, code = run(sys.argv[1:])
    except Refuse as r:
        lines, code = [r.answer], r.exit_code
    except Exception:  # noqa: BLE001 - every other failure is internal_error; standard error stays empty
        lines, code = [{"status": "internal_error"}], 2
    try:
        for line in lines:
            sys.stdout.write(json.dumps(line, ensure_ascii=False) + "\n")
        sys.stdout.flush()
    except (OSError, ValueError):
        return 4
    return code


# ---------------------------------------------------------------- Git, read only
GIT_DROP = ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_NAMESPACE", "GIT_COMMON_DIR", "GIT_PREFIX")


def git(clone, *args):
    """The standard output of `git -C clone <args>`; `repo_unreadable` when Git fails."""
    env = {k: v for k, v in os.environ.items() if k not in GIT_DROP}
    env.update(GIT_OPTIONAL_LOCKS="0", GIT_TERMINAL_PROMPT="0")
    try:
        done = subprocess.run(["git", "-C", clone, "-c", "core.quotePath=false", *args], capture_output=True,
                              timeout=GIT_TIMEOUT, env=env)
    except (OSError, subprocess.TimeoutExpired):
        raise Refuse("repo_unreadable")
    if done.returncode != 0:
        raise Refuse("repo_unreadable")
    return done.stdout


def commit_of(clone, rev):
    """The 40 hex digits of the commit `rev` names in the clone."""
    if rev.startswith("-"):
        raise Refuse("usage")
    out = git(clone, "rev-parse", "--verify", "--quiet", rev + "^{commit}").decode("ascii", "replace").strip()
    if not re.fullmatch(r"[0-9a-f]{40}", out):
        raise Refuse("repo_unreadable")
    return out


def tree(clone, commit):
    """[(path, blob id)] of every regular tracked file of the commit that is not the Academy's, by path."""
    out = git(clone, "ls-tree", "-r", "-z", "--full-tree", commit)
    files = []
    for entry in out.split(b"\0"):
        if not entry:
            continue
        meta, _, raw = entry.partition(b"\t")
        parts = meta.split(b" ")
        if len(parts) != 3 or parts[1] != b"blob" or parts[0] not in (b"100644", b"100755"):
            continue
        path = raw.decode("utf-8", "surrogateescape")
        if B.is_academy(path):
            continue
        files.append((path, parts[2].decode("ascii")))
    return sorted(files)


class Blobs:
    """The text of the blobs of one clone, read with `git show <blob id>`, each once."""

    def __init__(self, clone):
        self.clone, self.seen = clone, {}

    def text(self, blob):
        if blob not in self.seen:
            self.seen[blob] = git(self.clone, "show", blob).decode("utf-8", "replace")
        return self.seen[blob]


def changes(clone, base, head):
    """[(status letter, old path, new path)] of `git diff --name-status`."""
    out = git(clone, "diff", "--name-status", "-z", "--no-ext-diff", base, head).split(b"\0")
    found, i = [], 0
    while i < len(out) and out[i]:
        status = out[i].decode("ascii", "replace")
        n = 2 if status[:1] in ("R", "C") else 1
        paths = [p.decode("utf-8", "surrogateescape") for p in out[i + 1:i + 1 + n]]
        if len(paths) != n:
            raise Refuse("repo_unreadable")
        found.append((status[:1], paths[0], paths[-1]))
        i += 1 + n
    return found


def excludes(changed):
    """One `:(exclude)` pathspec for each changed path the Academy rule names."""
    named = sorted({p for _, old, new in changed for p in (old, new) if B.is_academy(p)})
    return [":(exclude)" + p for p in named]


def hunks(clone, base, head, paths, exclude):
    """[(line at base or None, sign, text)] of every removed ('-') and added ('+') line of the diff of the paths (the
    whole change when None), with the Academy's paths excluded; the number is the old file's, for a removed line."""
    spec = (list(paths) if paths is not None else ["."]) + exclude
    out = git(clone, "diff", "-U0", "--no-color", "--no-ext-diff", "--no-textconv", "--no-renames", base, head, "--",
              *spec)
    found, old, inside = [], 0, False
    for raw in out.split(b"\n"):
        line = raw.decode("utf-8", "replace")
        if line.startswith("diff --git "):
            inside = False
        elif line.startswith("@@"):
            m = re.match(r"@@ -(\d+)(?:,\d+)? ", line)
            old, inside = (int(m.group(1)) if m else 0), True
        elif inside and line[:1] == "-":
            found.append((old, "-", line[1:]))
            old += 1
        elif inside and line[:1] == "+":
            found.append((None, "+", line[1:]))
    return found


# ---------------------------------------------------------------- text
def norm(s):
    """Backticks removed, each run of white space made one space: the routing suite's comparison."""
    return " ".join(s.replace("`", "").split())


MARKER = re.compile(r"\s*(?:[-*+]\s+|\d+\.\s+|>\s*)")


def line_text(raw):
    """A line compared as the suites compare, without its list markers and table pipes."""
    s = raw
    while True:
        m = MARKER.match(s)
        if not m:
            break
        s = s[m.end():]
    return norm(s.replace("|", " "))


def flat_text(text):
    """The text of a file for a line to be found in: its lines as `line_text` makes them, joined by one space."""
    return norm(" ".join(line_text(line) for line in text.split("\n")))


def terms_of(raw, text):
    """The backticked spans of the line, and its words that hold a digit, `/`, `.`, `_`, `=` or `@` inside or begin
    with `--`."""
    found = []
    for span in re.findall(r"`([^`]*)`", raw):
        s = norm(span)
        if s and s not in found:
            found.append(s)
    for token in text.split(" "):
        t = token.lstrip("([{\"'").rstrip(".,;:!?)]}\"'")
        if t and (re.search(r"[0-9/._=@]", t) or t.startswith("--")) and t not in found:
            found.append(t)
    return found


def words_of(text):
    """The words of four or more letters, in lower case, less the common English words."""
    return sorted({w.lower() for w in re.findall(r"[A-Za-z]{4,}", text)} - COMMON)


FENCE = re.compile(r"^\s*(```|~~~)")
HEADING = re.compile(r"^ {0,3}#{1,6}(\s|$)")


def sections(path, text):
    """The sections of a kept file as normalised text: the lines from one ATX heading (outside a fence) to the next in a
    Markdown file, the whole file of another kind."""
    if not path.endswith(MARKDOWN):
        return [flat_text(text)]
    parts, current, fenced = [], [], False
    for line in text.split("\n"):
        if FENCE.match(line):
            fenced = not fenced
        elif not fenced and HEADING.match(line) and current:
            parts.append(current)
            current = []
        current.append(line)
    parts.append(current)
    return [flat_text("\n".join(p)) for p in parts]


# ---------------------------------------------------------------- case files
def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        h.update(f.read())
    return h.hexdigest()


def host_names(suite):
    """Digest to short name of `evals/<suite>/host-cases.sha256`."""
    names = {}
    try:
        with open(os.path.join(ROOT, "evals", suite, "host-cases.sha256"), encoding="utf-8") as f:
            for line in f:
                parts = line.split()
                if len(parts) == 2:
                    names[parts[0]] = parts[1]
    except OSError:
        pass
    return names


def fact_list(v):
    return isinstance(v, list) and all(
        isinstance(f, dict) and isinstance(f.get("fact"), list) and f["fact"]
        and all(isinstance(x, str) and norm(x) for x in f["fact"]) for f in v)


SEEDS = ("none", "drafted", "confirmed")
LANES = ("objective", "small", "none")
SKIPS = (None, "intent", "confirmation")


def _id_ok(c):
    return isinstance(c.get("id"), str) and c["id"] and "/" not in c["id"] and not c["id"].startswith(".")


def routing_facts(c):
    """The `must` facts of a routing case, or None when the case is invalid."""
    ok = (_id_ok(c) and isinstance(c.get("situation"), str) and fact_list(c.get("must")) and c["must"]
          and fact_list(c.get("wrong", [])))
    return c["must"] if ok else None


def intake_facts(c):
    """The `expect.missing` facts of an intake case, or None when the case is invalid."""
    ex = c.get("expect")
    ok = (_id_ok(c) and isinstance(c.get("words"), str) and c["words"] and c.get("seed") in SEEDS
          and isinstance(ex, dict) and ex.get("lane") in LANES and ex.get("out_of_order") in SKIPS
          and fact_list(ex.get("missing")) and bool(ex["missing"]) == (ex["out_of_order"] is not None))
    return ex["missing"] if ok else None


def load_cases(pairs):
    """{suite: [{"id", "file", "file_digest", "facts": [[strings]]}]} in file order. A file inside this repository's
    `evals/` is admitted as it is, any other by its SHA-256 in `evals/<suite>/host-cases.sha256`."""
    out = {}
    evals = os.path.realpath(os.path.join(ROOT, "evals"))
    for suite, path in pairs:
        real = os.path.realpath(path)
        if not os.path.isfile(real):
            raise Refuse("cases_unreadable")
        try:
            with open(real, "rb") as f:
                data = f.read()
        except OSError:
            raise Refuse("cases_unreadable")
        digest = hashlib.sha256(data).hexdigest()
        if real.startswith(evals + os.sep):
            name = os.path.relpath(real, os.path.realpath(ROOT)).replace(os.sep, "/")
        else:
            known = host_names(suite)
            if digest not in known:
                raise Refuse("cases_digest_unknown")
            name = known[digest]
        try:
            cases = json.loads(data.decode("utf-8"))
        except (UnicodeDecodeError, ValueError):
            raise Refuse("cases_invalid")
        if not isinstance(cases, list) or not all(isinstance(c, dict) for c in cases):
            raise Refuse("cases_invalid")
        got = out.setdefault(suite, [])
        for c in cases:
            facts = routing_facts(c) if suite == "routing" else intake_facts(c)
            if facts is None:
                raise Refuse("cases_invalid")
            if any(x["id"] == c["id"] for x in got):
                raise Refuse("case_id_twice")
            got.append({"id": c["id"], "file": name, "file_digest": digest,
                        "facts": [[norm(s) for s in f["fact"]] for f in facts]})
    return out


# ---------------------------------------------------------------- the mapping
def map_digest():
    with open(SELF, "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


class Source:
    """One repository at one revision: its name, clone, commit and tracked files."""

    def __init__(self, name, clone, rev):
        self.name, self.clone = name, clone
        self.commit = commit_of(clone, rev)
        self.files = tree(clone, self.commit)
        self.blobs = Blobs(clone)

    def markdown(self):
        """[(path, normalised text)] of the tracked `.md` files."""
        return [(p, norm(self.blobs.text(b))) for p, b in self.files if p.endswith(MARKDOWN)]


def key_of(suite, case_id):
    return B.sha256(suite + ":" + case_id)


def records(sources, cases, now):
    """One record per case of each suite, from the Markdown of `sources` (a list of Source)."""
    texts = [(s.name, p, t) for s in sources for p, t in s.markdown()]
    commits = sorted(f"{s.name}@{s.commit}" for s in sources)
    digest = map_digest()
    out = {}
    for suite, items in cases.items():
        rows = []
        for c in items:
            serves, unhomed = set(), 0
            for fact in c["facts"]:
                holders = {f"{n}:{p}" for n, p, t in texts if all(s in t for s in fact)}
                serves |= holders
                unhomed += 0 if holders else 1
            every = not c["facts"] or unhomed > 0
            r = {"type": "case", "key": key_of(suite, c["id"]), "observed_at": now, "suite": suite, "case_id": c["id"],
                 "file": c["file"], "file_digest": c["file_digest"], "facts": len(c["facts"]), "unhomed": unhomed,
                 "all": every, "serves": [] if every else sorted(serves), "commits": commits, "map_digest": digest}
            if len(B.canon(r).encode("utf-8")) + 1 > RECORD_LIMIT:
                r["all"], r["serves"] = True, []
            rows.append(r)
        out[suite] = rows
    return out


def record_line(r):
    return B.canon(r)


def summary(rows):
    return {"cases": len(rows), "all": sum(1 for r in rows if r["all"]), "unhomed": sum(r["unhomed"] for r in rows)}


# ---------------------------------------------------------------- the lock and the files
class Lock:
    """An exclusive `flock` on `<dir>/.lock`; the directory is made with mode 0755 when absent. Waits LOCK_WAIT
    seconds, then refuses `busy`."""

    def __init__(self, directory):
        self.directory, self.fd = directory, None

    def __enter__(self):
        try:
            if not os.path.isdir(self.directory):
                os.makedirs(self.directory, mode=0o755, exist_ok=True)
                os.chmod(self.directory, 0o755)
            fd = os.open(os.path.join(self.directory, ".lock"), os.O_RDWR | os.O_CREAT | os.O_CLOEXEC, 0o644)
        except OSError:
            raise Refuse("directory_unwritable")
        deadline = time.monotonic() + LOCK_WAIT
        while True:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except OSError:
                if time.monotonic() >= deadline:
                    os.close(fd)
                    raise Refuse("busy")
                time.sleep(0.05)
        self.fd = fd
        return self

    def __exit__(self, *exc):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None


def write_files(directory, by_suite):
    """`<dir>/<suite>.cases.jsonl` for each suite given, each under a temporary name and renamed, mode 0644."""
    temps = []
    try:
        for suite, rows in by_suite.items():
            tmp = os.path.join(directory, f".{suite}.cases.tmp-{os.getpid()}")
            fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC | os.O_NOFOLLOW, 0o644)
            temps.append((tmp, os.path.join(directory, f"{suite}.cases.jsonl")))
            with os.fdopen(fd, "wb") as f:
                f.write("".join(record_line(r) + "\n" for r in rows).encode("utf-8"))
                os.fchmod(f.fileno(), 0o644)
        for tmp, final in temps:
            os.replace(tmp, final)
    except OSError:
        for tmp, _ in temps:
            try:
                os.unlink(tmp)
            except OSError:
                pass
        raise Refuse("directory_unwritable")


# ---------------------------------------------------------------- the store
def engine(store):
    return B.Engine(store, "cases", "lead")


def index_source(eng):
    """(held, {"status","code","appended","renewed","retracted"}) of `ont index --source cases`."""
    v, _ = eng.call(["index", "--source", "cases"], None)
    one = None
    if isinstance(v, dict) and v.get("status") in ("ok", "source_failed") and isinstance(
            (v.get("result") or {}).get("sources"), list):
        one = next((s for s in v["result"]["sources"] if isinstance(s, dict) and s.get("source") == "cases"), None)
    if one is None:
        return False, {"status": None, "code": None, "appended": None, "renewed": None, "retracted": None}
    appended = one.get("appended") if isinstance(one.get("appended"), dict) else None
    shown = {"status": one.get("status"), "code": one.get("code"), "appended": appended, "renewed": one.get("renewed"),
             "retracted": appended.get("retraction") if appended else None}
    return v["status"] == "ok" and one.get("status") == "ok", shown


def search_all(eng, object_type, where=None):
    """([(id, fields)] of every page, whether every page was qualified), or None when an answer is not readable."""
    body = {"object_type": object_type, "limit": 200}
    if where is not None:
        body["where"] = where
    found, qualified = [], True
    while True:
        v, ok = eng.call(["search"], body)
        if not ok or not isinstance(v, dict) or v.get("status") != "ok" or not isinstance(v.get("result"), dict):
            return None
        r = v["result"]
        if not isinstance(r.get("objects"), list) or not isinstance(r.get("basis"), dict):
            return None
        qualified = qualified and r["basis"].get("qualified") is True
        for o in r["objects"]:
            ident = o.get("id") if isinstance(o, dict) else None
            fields = (o.get("head") or {}).get("fields") if isinstance(o, dict) else None
            if not isinstance(ident, dict) or not isinstance(fields, dict):
                return None
            found.append((ident, {k: (f.get("value") if isinstance(f, dict) and f.get("kind") == "value" else None)
                                  for k, f in fields.items()}))
        c = r.get("continuation")
        if c is None:
            return found, qualified
        if c == body.get("continuation"):
            return None
        body["continuation"] = c


def stored_cases(eng, suites):
    """{suite: [fields of each `case` object]}, or None when the store cannot be read in full."""
    out = {}
    for suite in suites:
        got = search_all(eng, "case", {"eq": ["suite", suite]})
        if got is None or not got[1]:
            return None
        out[suite] = [f for _, f in got[0]]
    return out


def case_map_value(eng):
    """The `value` of the KPI `case_map` with the latest `observed_at` (any task), or None."""
    got = search_all(eng, "kpi", {"eq": ["name", KPI_NAME]})
    if got is None or not got[1]:
        return None
    best = None
    for _, f in got[0]:
        if isinstance(f.get("value"), str) and isinstance(f.get("observed_at"), str):
            if best is None or f["observed_at"] > best["observed_at"]:
                best = f
    return best["value"] if best else None


# ---------------------------------------------------------------- the check of lost lines
class Kept:
    """The text a removed line may be kept in: the tracked files of the changed repository at its head and of the
    `--also` clones at origin/main, with the extensions of KEPT_ENDINGS."""

    def __init__(self, sources):
        flats, secs = [], []
        for s in sources:
            for path, blob in s.files:
                if not path.endswith(KEPT_ENDINGS):
                    continue
                text = s.blobs.text(blob)
                flats.append(flat_text(text))
                secs.extend(sections(path, text))
        self.big = "\0".join(flats)
        self.sections = secs
        self.index = {}
        for i, sec in enumerate(secs):
            for w in set(words_of(sec)):
                self.index.setdefault(w, set()).add(i)

    def in_file(self, text):
        return text in self.big

    def in_section(self, terms, words):
        if words:
            candidates = None
            for w in words:
                found = self.index.get(w)
                if not found:
                    return False
                candidates = set(found) if candidates is None else candidates & found
                if not candidates:
                    return False
        else:
            candidates = range(len(self.sections))
        return any(all(t in self.sections[i] for t in terms) for i in candidates)


def digests_of(path):
    """The set of digests of an `--accepted` file; `accepted_unreadable` when it cannot be read or a line is not one."""
    out = set()
    try:
        with open(path, encoding="utf-8") as f:
            for line in f:
                s = line.strip()
                if not s:
                    continue
                if not re.fullmatch(r"[0-9a-f]{64}", s):
                    raise Refuse("accepted_unreadable")
                out.add(s)
    except (OSError, UnicodeDecodeError):
        raise Refuse("accepted_unreadable")
    return out


def skippable(raw):
    """A line that is not read: blank, a fence line, or a table rule."""
    s = raw.strip()
    return not s or s.startswith("```") or s.startswith("~~~") or re.fullmatch(r"[|:\-\s]+", s) is not None


def lost_lines(changed, base, head, others, accepted):
    """([line answers], totals) of the removed lines of the changed Markdown files that no kept text keeps.
    `changed` is a Source at the head, `others` the Sources of the `--also` clones at origin/main."""
    diff = changes(changed.clone, base, head)
    skip = excludes(diff)
    read = []
    for status, old, _new in diff:
        if status in ("M", "D", "R", "T") and old.endswith(MARKDOWN) and not B.is_academy(old) and old not in read:
            read.append(old)
    kept_text = None
    lost, counts = [], {"files": len(read), "read": 0, "kept": 0, "skipped": 0}
    for path in read:
        for number, sign, raw in hunks(changed.clone, base, head, [path], skip):
            if sign != "-" or skippable(raw):
                continue
            counts["read"] += 1
            text = line_text(raw)
            if not text:
                counts["skipped"] += 1
                continue
            if kept_text is None:
                kept_text = Kept([changed] + others)
            terms, words = terms_of(raw, text), words_of(text)
            if kept_text.in_file(text):
                counts["kept"] += 1
            elif not terms and not words:
                counts["skipped"] += 1
            elif kept_text.in_section(terms, words):
                counts["kept"] += 1
            else:
                digest = B.sha256(text)
                lost.append({"path": path, "line": number, "text": B.cut(text, LINE_TEXT), "digest": digest,
                             "accepted": digest in accepted})
    totals = dict(counts, lost=len(lost), accepted=sum(1 for x in lost if x["accepted"]))
    return lost, totals


# ---------------------------------------------------------------- selection
def last_component(served):
    """The file name of an `<owner/name>:<path>`."""
    return served.partition(":")[2].rsplit("/", 1)[-1]


def changed_names(changed, base, head):
    """(the set of `<owner/name>:<path>` the change names, the text of every line it removes or adds)."""
    diff = changes(changed.clone, base, head)
    paths = {f"{changed.name}:{p}" for _, old, new in diff for p in (old, new) if not B.is_academy(p)}
    body = "\n".join(t for _n, _s, t in hunks(changed.clone, base, head, None, excludes(diff)))
    return paths, body


def select(rows, paths, body):
    """The `case_id`s of `rows` (dicts of a case's members) a change reaches: its `all` is true, a path of its `serves`
    is changed, or a removed or added line holds the last component of a path of its `serves`."""
    chosen = []
    for r in rows:
        serves = r.get("serves") or []
        if (r.get("all") is True or any(s in paths for s in serves)
                or any(last_component(s) and last_component(s) in body for s in serves)):
            chosen.append(r["case_id"])
    return chosen


def args_of(ids):
    return " ".join("--case " + shlex.quote(i) for i in ids)


# ---------------------------------------------------------------- failures of a gate
def failures(gate):
    """The ids of the cases a full gate failed on: those in the `lost` of at least two readers whose `ok` is false, or,
    when only one reader's `ok` is false, that reader's `lost`. None when no reader has an `ok` and a list `lost`."""
    readers = gate.get("readers") if isinstance(gate, dict) else None
    if not isinstance(readers, dict):
        return None
    judged = [r for r in readers.values()
              if isinstance(r, dict) and isinstance(r.get("ok"), bool) and isinstance(r.get("lost"), list)]
    if not judged:
        return None
    bad = [r for r in judged if r["ok"] is False]
    lost = [{x for x in r["lost"] if isinstance(x, str)} for r in bad]
    if len(bad) == 1:
        return sorted(lost[0])
    counts = {}
    for s in lost:
        for x in s:
            counts[x] = counts.get(x, 0) + 1
    return sorted(x for x, n in counts.items() if n >= 2)
