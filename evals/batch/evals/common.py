"""What the three programs of the evals job share beyond lib: the paths, the instruction text read from Git's objects, the
Lead cases, the request lines, the reading and counting of result lines, the engine with the caller's environment, the
calls that record a score and the finding pass.

The programs load this file by its path and then set `L`, the lib they loaded (evals/batch/lib.py), so that all of them
use one `Refuse`. The standard library only; no socket, no model. Contract: evals/batch/evals/README.md.
"""
import argparse
import datetime
import hashlib
import importlib.machinery
import importlib.util
import json
import math
import os
import re
import stat
import subprocess

L = None  # evals/batch/lib.py, set by the program that loads this file

HERE = os.path.dirname(os.path.abspath(__file__))
DETECTOR = "batch:evals"
AGENT = "batch-evals"
PREFIX = "be-"
STATES = ("requested", "settled", "too_large")
SUITES = ("routing", "lead")
COUNT_WORDS = ("read", "refused", "errored", "unreadable", "stale", "duplicate", "skipped")
HEX64 = re.compile(r"^[0-9a-f]{64}$")
COMMIT = re.compile(r"^(?:[0-9a-f]{40}|[0-9a-f]{64})$")
TEXT_LIMIT = 300_000
GIT_ENV = {"GIT_OPTIONAL_LOCKS": "0", "GIT_TERMINAL_PROMPT": "0", "GIT_NO_LAZY_FETCH": "1", "GIT_CONFIG_NOSYSTEM": "1"}
GIT_TIMEOUT = 300
CACHE = {"type": "ephemeral", "ttl": "1h"}
SUMS_PATH = "content/SHA256SUMS"
SUMS_LINE = re.compile(r"^([0-9a-f]{64})  (.+)$")
KPI_DEFINITION = ("Score of one pass of a suite over the instruction text at one commit of content/, read from batch "
                  "results: mean of passed cases per sample, spread (twice the routing gate's noise of the pass against "
                  "an equal pass), samples per case, majority of cases, n, unanswered. Value: mean M spread S samples C1 "
                  "... Cn majority K/CASES n N unanswered U. Written by evals/batch/evals/score --record; no case text.")


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise L.Refuse("usage")


def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, path)
    mod = importlib.util.module_from_spec(importlib.util.spec_from_loader(name, loader))
    loader.exec_module(mod)
    return mod


def routing_module():
    return load("routing_run", os.path.join(HERE, "..", "..", "routing", "run"))


def lead_module():
    return load("lead_run", os.path.join(HERE, "..", "..", "lead", "run"))


def foreign(call, *args):
    """A call of the routing or the Lead module, whose refusal (its own `Refuse`, the code first) is ours."""
    try:
        return call(*args)
    except Exception as e:  # noqa: BLE001
        if type(e).__name__ == "Refuse" and e.args and isinstance(e.args[0], str):
            raise L.Refuse(e.args[0])
        raise


def integer(text, low, high):
    if not isinstance(text, str) or not re.fullmatch(r"[0-9]+", text) or not low <= int(text) <= high:
        raise L.Refuse("usage")
    return int(text)


def whole(v):
    return isinstance(v, int) and not isinstance(v, bool) and v >= 0


def text_file(name):
    with open(os.path.join(HERE, name), encoding="utf-8") as f:
        return f.read()


def json_file(name):
    with open(os.path.join(HERE, name), encoding="utf-8") as f:
        return json.load(f)


def sha_bytes(data):
    return hashlib.sha256(data).hexdigest()


# The paths: the Academy rule on every string as given, then each path resolved.
def paths(*given):
    """Each given path (None stays None) resolved. The rule is asked of every string first, so that a refused path
    refuses before any file system call on another."""
    for p in given:
        if p is not None:
            L.refuse_academy(p)
    return [L.resolve(p) if p is not None else None for p in given]


def read_file(path):
    try:
        with open(path, "rb") as f:
            return f.read()
    except OSError:
        raise L.Refuse("cases_unreadable")


def text_of(path):
    return read_file(path).decode("utf-8", "replace")


# The instruction text, from Git's objects.
class Clone:
    """The clone at one commit; nothing of its checkout is read."""

    def __init__(self, path, rev):
        self.path = path
        self.env = dict(os.environ, **GIT_ENV, GIT_CEILING_DIRECTORIES=os.path.dirname(path.rstrip("/")) or "/")
        out = self.git(["rev-parse", "--verify", "--quiet", rev + "^{commit}"])
        self.commit = out.decode("ascii", "replace").strip() if out is not None else ""
        if not COMMIT.match(self.commit):
            raise L.Refuse("repo_unreadable")

    def git(self, args, data=None):
        command = ["git", "-c", "core.quotePath=false", "-c", "protocol.allow=never", "-C", self.path, *args]
        try:
            done = subprocess.run(command, input=data, capture_output=True, timeout=GIT_TIMEOUT, env=self.env)
        except (OSError, subprocess.TimeoutExpired):
            return None
        return done.stdout if done.returncode == 0 else None

    def blobs(self, names):
        """The bytes of each path below the commit's root, in the order given."""
        wanted = "".join("%s:%s\n" % (self.commit, n) for n in names).encode("utf-8")
        out = self.git(["cat-file", "--batch"], wanted)
        if out is None:
            raise L.Refuse("repo_unreadable")
        got, i = [], 0
        for _ in names:
            nl = out.find(b"\n", i)
            header = out[i:nl].split(b" ") if nl >= 0 else []
            if len(header) != 3 or header[1] != b"blob" or not header[2].isdigit():
                raise L.Refuse("repo_unreadable")
            size = int(header[2])
            got.append(out[nl + 1:nl + 1 + size])
            i = nl + 1 + size + 1
        return got

    def instructions(self):
        """(the files the list names as [(path below content/, bytes)] with AGENTS.md first, the instructions
        {"commit", "sums"}). Refuses `repo_unreadable` and `content_sums_differ`."""
        sums = self.blobs([SUMS_PATH])[0]
        listed = []
        for line in sums.decode("utf-8", "replace").split("\n"):
            if not line:
                continue
            m = SUMS_LINE.match(line)
            if not m or m.group(2).startswith("/") or any(p in ("", ".", "..") for p in m.group(2).split("/")) \
                    or any(ord(c) < 32 for c in m.group(2)):
                raise L.Refuse("repo_unreadable")
            listed.append((m.group(2), m.group(1)))
        names = [p for p, _ in listed]
        if len(set(names)) != len(names) or "AGENTS.md" not in names:
            raise L.Refuse("repo_unreadable")
        for p in names:
            L.refuse_academy("content/" + p)
        data = self.blobs(["content/" + p for p in names])
        if any(sha_bytes(b) != d for b, (_, d) in zip(data, listed)):
            raise L.Refuse("content_sums_differ")
        files = sorted(zip(names, data), key=lambda f: (f[0] != "AGENTS.md", f[0].encode("utf-8")))
        return files, {"commit": self.commit, "sums": sha_bytes(sums)}


def instruction_text(files):
    """AGENTS.md first, then the other files by path, each under one line `=== <path> ===`."""
    parts = []
    for path, data in files:
        body = data.decode("utf-8", "replace")
        parts.append("=== %s ===\n%s%s" % (path, body, "" if body.endswith("\n") else "\n"))
    return "".join(parts)


# The cases.
def routing_cases(extra):
    """({id: case}, {name: digest}) of load_cases of the routing module, with that function's refusals."""
    return foreign(routing_module().load_cases, extra)


def lead_cases(cases_dir):
    """The ids of the Lead cases, checked as `check_cases` of the Lead module checks them without its account: the
    names below the directory are listed first (no file opened), each digest is that of tree_digest, no other
    directory. Refuses `academy_path`, `case_digest_differs` and `case_unknown`."""
    lead = lead_module()
    for dirpath, dirnames, filenames in os.walk(cases_dir):
        for name in dirnames + filenames:
            L.refuse_academy(os.path.relpath(os.path.join(dirpath, name), cases_dir))
    want = {}
    for line in read_file(os.path.join(HERE, "..", "..", "lead", "cases.sha256")).decode("utf-8").splitlines():
        digest, _, cid = line.partition("  ")
        want[cid] = digest
    bad = []
    for cid, digest in want.items():
        path = os.path.join(cases_dir, cid)
        try:
            same = "/" not in cid and not cid.startswith(".") and os.path.isdir(path) \
                and lead.tree_digest(path) == digest
        except Exception:  # noqa: BLE001 - a link or a device below the case is no unchanged case
            same = False
        if not same:
            bad.append(cid)
    if bad:
        raise L.Refuse("case_digest_differs")
    extra = [n for n in sorted(os.listdir(cases_dir)) if n not in want and os.path.isdir(os.path.join(cases_dir, n))]
    if extra:
        raise L.Refuse("case_unknown")
    return sorted(want)


def lead_user(cases_dir, cid):
    """The user text of a Lead case: lead.md, PROMPT.txt with @TASK@ made `task`, each regular file below TASK/."""
    base = os.path.join(cases_dir, cid)
    prompt = text_of(os.path.join(base, "PROMPT.txt")).replace("@TASK@", "task")
    task = os.path.join(base, "TASK")
    found = []
    for dirpath, _, filenames in os.walk(task):
        for name in filenames:
            p = os.path.join(dirpath, name)
            try:
                regular = stat.S_ISREG(os.lstat(p).st_mode)
            except OSError:
                regular = False
            if regular:
                found.append(os.path.relpath(p, task))
    text = text_file("lead.md").rstrip("\n") + "\n\n" + prompt.rstrip("\n") + "\n"
    for rel in sorted(found, key=lambda r: r.encode("utf-8", "surrogateescape")):
        body = text_of(os.path.join(task, rel))
        text += "\n=== task/%s ===\n%s%s" % (rel, body, "" if body.endswith("\n") else "\n")
    return text


def expected_text(cases_dir, cid):
    return text_of(os.path.join(cases_dir, cid, "EXPECTED.md"))


def lead_digest():
    return sha_bytes(read_file(os.path.join(HERE, "..", "..", "lead", "cases.sha256")))


# The requests.
def answer_params(model, max_tokens, system, user):
    return {"model": model, "max_tokens": max_tokens,
            "system": [{"type": "text", "text": system, "cache_control": dict(CACHE)}],
            "messages": [{"role": "user", "content": user}]}


def sample_key(params, sample):
    return L.sha256(L.canon([params, sample]))


def line_of(custom_id, params):
    return L.canon({"custom_id": custom_id, "params": params})


def text_bytes(params):
    """The bytes of the system and user text of a request."""
    system = params["system"]
    if isinstance(system, list):
        system = "".join(b["text"] for b in system)
    return len(system.encode("utf-8")) + len(params["messages"][0]["content"].encode("utf-8"))


def lines_file(lines):
    return "".join(x + "\n" for x in lines).encode("utf-8")


def json_text(doc):
    return (json.dumps(doc, indent=1, ensure_ascii=False) + "\n").encode("utf-8")


# The result lines.
def settled_keys(files, counts):
    """The keys with a result line of type succeeded in any of the files; a line that is no result line is counted."""
    settled = set()
    for path in files:
        for parsed in L.result_lines(path):
            if parsed is None:
                counts["result_lines_skipped"] += 1
            elif parsed[1]["type"] == "succeeded":
                settled.add(parsed[0])
    return settled


def read_results(files):
    lines = []
    for path in files:
        lines.extend(L.result_lines(path))
    return lines


def free_answer(result):
    """How a free answer is read: ("errored"|"refused"|"unreadable", None) or ("read", its text)."""
    if result["type"] in L.FAILED_TYPES:
        return "errored", None
    message = result["message"]
    stop = message.get("stop_reason")
    if stop == "refusal":
        return "refused", None
    content = message.get("content")
    if stop != "end_turn" or not isinstance(content, list):
        return "unreadable", None
    text = "".join(b["text"] for b in content
                   if isinstance(b, dict) and b.get("type") == "text" and isinstance(b.get("text"), str))
    if not text.strip():
        return "unreadable", None
    return "read", text


def schema_answer(schema):
    return lambda result: L.answer(result, schema)


def count_lines(lines, keys, reader):
    """The lines of a result file counted: (counts, answers by key). Of the lines of one key the first of type
    succeeded is the one counted, else the first; every other line of that key is a duplicate; a line that is no result
    line is skipped and one of a key outside `keys` is stale. `reader` reads a counted line."""
    counts = {w: 0 for w in COUNT_WORDS}
    chosen, extra = {}, 0
    for parsed in lines:
        if parsed is None:
            counts["skipped"] += 1
        elif parsed[0] not in keys:
            counts["stale"] += 1
        elif parsed[0] not in chosen:
            chosen[parsed[0]] = parsed
        else:
            extra += 1
            if parsed[1]["type"] == "succeeded" and chosen[parsed[0]][1]["type"] != "succeeded":
                chosen[parsed[0]] = parsed
    counts["duplicate"] = extra
    answers = {}
    for k, parsed in chosen.items():
        how, value = reader(parsed[1])
        counts[how] += 1
        if how == "read":
            answers[k] = value
    return counts, answers


# The documents.
def load_items(path, suite=None):
    """items.json in the form `build` writes: (the document, its SHA-256). Refuses `items_unreadable`."""
    def bad():
        raise L.Refuse("items_unreadable")

    try:
        with open(path, "rb") as f:
            data = f.read()
        doc = json.loads(data.decode("utf-8"))
    except (OSError, ValueError):
        bad()
    keys = {"job", "suite", "model", "instructions", "case_files", "samples", "items"}
    if not isinstance(doc, dict) or set(doc) != keys or doc["job"] != "evals" or doc["suite"] not in SUITES \
            or (suite is not None and doc["suite"] != suite):
        bad()
    ins = doc["instructions"]
    if not (isinstance(doc["model"], str) and doc["model"] and isinstance(ins, dict) and set(ins) == {"commit", "sums"}
            and isinstance(ins["commit"], str) and COMMIT.match(ins["commit"]) and isinstance(ins["sums"], str)
            and HEX64.match(ins["sums"])):
        bad()
    files = doc["case_files"]
    if not (isinstance(files, dict) and files and all(isinstance(k, str) and isinstance(v, str) and HEX64.match(v)
                                                      for k, v in files.items())):
        bad()
    n = doc["samples"]
    if not whole(n) or not 3 <= n <= 9 or not isinstance(doc["items"], list):
        bad()
    seen = set()
    for it in doc["items"]:
        if not isinstance(it, dict) or set(it) != {"case", "sample", "key", "state"} or not isinstance(it["case"], str) \
                or not it["case"] or not whole(it["sample"]) or not 1 <= it["sample"] <= n \
                or not isinstance(it["key"], str) or not HEX64.match(it["key"]) or it["state"] not in STATES \
                or (it["case"], it["sample"]) in seen:
            bad()
        seen.add((it["case"], it["sample"]))
    return doc, sha_bytes(data)


# The score.
def spread_of(passes, n):
    """Twice the routing gate's noise of the pass against an equal pass: 2 x sqrt(sum of p(1-p) x 2/n)."""
    total = 0.0
    for count in passes.values():
        p = count / n
        total += p * (1 - p) * 2 / n
    return round(2 * math.sqrt(total), 4)


def reader_score(items, passed, answered):
    """The score of one pass as score_reader of the routing module computes it: `passed` and `answered` are the sets of
    (case, sample) that passed and that were not left unanswered."""
    n = items["samples"]
    ids = sorted({i["case"] for i in items["items"]})
    passes = {c: sum((c, s) in passed for s in range(1, n + 1)) for c in ids}
    per_sample = [sum((c, s) in passed for c in ids) for s in range(1, n + 1)]
    unanswered = sum(1 for c in ids for s in range(1, n + 1) if (c, s) not in answered)
    return {"n": n, "passes": passes, "samples": per_sample, "mean": round(sum(per_sample) / n, 4),
            "majority": sorted(c for c in ids if passes[c] > n / 2), "unanswered": unanswered,
            "spread": spread_of(passes, n)}


# The store: the engine with the caller's environment.
def engine(store):
    """`ont` (ONT_BIN) with the program's environment unchanged, so it claims the role ONT_ROLE names; no member role
    is sent. `engine.last` is the last answer."""
    e = L.Engine(store, AGENT)
    e.env = dict(os.environ)
    inner = e.call

    def call(args, body):
        v, ok = inner(args, body)
        e.last = v
        return v, ok

    e.call = call
    e.last = None
    return e


def store_failed():
    return L.Refuse("store_failed", exit_code=3)


def search_full(e, object_type, where, qualified=True, limit=15):
    """[(id, fields)] of every page, each field its value or None; None when an answer is not `ok`, or (when
    `qualified`) a page is not qualified."""
    body = {"object_type": object_type, "limit": limit}
    if where is not None:
        body["where"] = {"all": where}
    found = []
    while True:
        v, ok = e.call(["search"], body)
        if not ok or not isinstance(v, dict) or v.get("status") != "ok" or not isinstance(v.get("result"), dict):
            return None
        r = v["result"]
        if not isinstance(r.get("basis"), dict) or (qualified and r["basis"].get("qualified") is not True):
            return None
        if not isinstance(r.get("objects"), list):
            return None
        for o in r["objects"]:
            ident = o.get("id") if isinstance(o, dict) else None
            fields = o.get("head", {}).get("fields") if isinstance(o, dict) and isinstance(o.get("head"), dict) else None
            if not isinstance(ident, dict) or not isinstance(ident.get("native_key"), str) or not isinstance(fields, dict):
                return None
            found.append((ident, {k: (f.get("value") if isinstance(f, dict) and f.get("kind") == "value" else None)
                                  for k, f in fields.items()}))
        c = r.get("continuation")
        if c is None:
            return found
        if c == body.get("continuation"):
            return None
        body["continuation"] = c


def is_ref(value, kind, key):
    return isinstance(value, dict) and value.get("native_key") == key and value.get("object_type", kind) == kind


def created(e, kind):
    """The id of the edit of the last apply answer that made an object of `kind`, or None."""
    result = e.last.get("result") if isinstance(e.last, dict) else None
    edits = result.get("edits") if isinstance(result, dict) else None
    for edit in edits if isinstance(edits, list) else []:
        if isinstance(edit, dict) and edit.get("change") == "initial" and isinstance(edit.get("id"), dict) \
                and edit["id"].get("object_type") == kind:
            return edit["id"]
    return None


def request_id(action, name, basis, parameters):
    return PREFIX + L.sha256(L.canon([action, name, basis, parameters]))[:40]


def apply_action(e, action, name, basis, parameters):
    """("applied"|"already_applied", None) or ("failed", code), the agent and the request id as the page fixes them."""
    body = {"action_type": action, "parameters": parameters, "agent": AGENT,
            "request_id": request_id(action, name, basis, parameters)}
    return e.apply(body)


def utc_now():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def read_findings(e, detector):
    """The findings of the detector by native key; `store_failed` when the search is not answered."""
    found = search_full(e, "finding", [{"eq": ["detector", detector]}])
    if found is None:
        raise store_failed()
    return {i["native_key"]: f for i, f in found if f.get("detector") == detector}


def finding_pass(e, detector, agent, prefix, signals, stored):
    """One finding per signal. A signal is {"key" (the native key), "summary", "evidence", "basis" (what enters its
    request id with the action and the key: the SHA-256 of the score), and optionally "kind" (note) and
    "severity" (info)}. A finding that is absent is opened; one `open` or `planned` is seen again only when its stored
    evidence differs; one `closed` or `dismissed` is left alone and counted. At most ten opens are applied a call, the
    rest are counted over_cap. `stored` is read before any apply (read_findings)."""
    opened, seen, failed = [], [], []
    left = over = already = 0
    for s in signals:
        found = stored.get(s["key"])
        fid = L.finding_id(s["key"])
        if found is None:
            action = "open_finding"
            parameters = {"finding": fid, "kind": s.get("kind", "note"), "detector": detector, "summary": s["summary"],
                          "evidence": s["evidence"], "severity": s.get("severity", "info")}
            if len(opened) >= L.CAP:
                over += 1
                continue
        elif found.get("state") in ("open", "planned"):
            if found.get("evidence") == s["evidence"]:
                continue
            action = "see_finding_again"
            parameters = {"finding": fid, "evidence": s["evidence"]}
        else:
            left += 1
            continue
        body = {"action_type": action, "parameters": parameters, "agent": agent,
                "request_id": prefix + L.sha256(L.canon([action, s["key"], s["basis"], parameters]))[:40]}
        outcome, code = e.apply(body)
        if outcome == "applied":
            (opened if action == "open_finding" else seen).append(s["key"])
        elif outcome == "already_applied":
            already += 1
        else:
            failed.append({"key": s["key"], "code": code})
    return {"opened": opened, "seen": seen, "left": left, "over_cap": over, "already_applied": already,
            "failed": failed}
