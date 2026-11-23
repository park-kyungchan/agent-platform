"""What the three programs of the mining job share beyond lib: the window, the reads of the store (reviews, units,
intents, the log of stop_unit), the items, the report reader, the counting of result lines and the finding pass.

The programs load this file by its path and then set `L`, the lib they loaded (evals/batch/lib.py), so that both
use one `Refuse`. The standard library only; no socket, no model. Contract: evals/batch/mining/README.md.
"""
import argparse
import datetime
import json
import os
import re
import stat

L = None  # evals/batch/lib.py, set by the program that loads this file

HERE = os.path.dirname(os.path.abspath(__file__))
DETECTOR = "batch:mining"
AGENT = "batch-mining"
PREFIX = "bm-"
STATES = ("requested", "settled", "over_max")
KINDS = ("blocking", "repair", "stop")
ITEM_KEYS = {"item", "kind", "name", "review", "unit", "intent", "at", "text", "key", "state"}
REPORT_LIMIT = 1_048_576
BLOCKING_TEXT = 6000
REPAIR_TEXT = 12000
HEX64 = re.compile(r"^[0-9a-f]{64}$")
DATE = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$")
WORD = re.compile(r"^[a-z_]{1,32}$")

# The pattern and the lists of evals/readout.py (a test fails when they differ).
BLOCKING_KINDS = {
    "page": ("unbuildable", "contradiction", "check", "write_set", "unguarded_effect"),
    "plan": ("unreached_criterion", "dependency", "conflict", "unguarded_effect"),
}
BLOCKING_LINE = re.compile(r"(?:[ \-*]|\d+\.)*blocking(?:[ ]([a-z_]+):)?")
NOTE_LINE = re.compile(r"(?:[ \-*]|\d+\.)*note:")
SECTION_LINE = re.compile(r"[#* ]*(?:What I ran|What I could not check)")
UNIT_STATES = ["stopped", "accepted", "running", "ready", "draft", "merged", "done", "dropped"]
INTENT_STATES = ["draft", "assumed", "confirmed", "met", "retired"]


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise L.Refuse("usage")


def integer(text, low, high):
    if not isinstance(text, str) or not re.fullmatch(r"[0-9]+", text) or not low <= int(text) <= high:
        raise L.Refuse("usage")
    return int(text)


def window(since, until):
    """(since, until, the day after until), as YYYY-MM-DD; `usage` unless both are dates, since is not after until
    and they are at most 31 days apart."""
    try:
        if not DATE.match(since) or not DATE.match(until):
            raise ValueError
        a, b = datetime.date.fromisoformat(since), datetime.date.fromisoformat(until)
    except (ValueError, TypeError):
        raise L.Refuse("usage")
    if a > b or (b - a).days > 31:
        raise L.Refuse("usage")
    return since, until, (b + datetime.timedelta(days=1)).isoformat()


def in_window(at, since, until):
    """A time is in the window when its first ten characters lie within since..until, both included."""
    return isinstance(at, str) and since <= at[:10] <= until


def load_text(name):
    with open(os.path.join(HERE, name), encoding="utf-8") as f:
        return f.read()


def load_json(name):
    with open(os.path.join(HERE, name), encoding="utf-8") as f:
        return json.load(f)


def causes():
    return load_json("causes.json")


def system_of_classify():
    text = load_text("classify.md").rstrip("\n")
    lines = "\n".join(f"{c['cause']}: {c['means']}" for c in causes())
    return text + "\n\n" + lines + "\n"


def ref_key(value):
    """The native key of a reference value (an id), or None."""
    if isinstance(value, dict):
        value = value.get("native_key")
    return value if isinstance(value, str) and value else None


def word(value):
    return value if isinstance(value, str) and WORD.match(value) else "none"


def settled_keys(paths, counts):
    """The keys with a result line of type succeeded in any of the files; a line that is no result line is counted."""
    settled = set()
    for path in paths:
        for parsed in L.result_lines(path):
            if parsed is None:
                counts["result_lines_skipped"] += 1
            elif parsed[1]["type"] == "succeeded":
                settled.add(parsed[0])
    return settled


# The store.
def store_failed():
    return L.Refuse("store_failed", exit_code=3)


def search(engine, object_type, where):
    found = engine.search(object_type, where)
    if found is None:
        raise store_failed()
    return dict(found)


def search_closed_reviews(engine):
    return search(engine, "review", [{"eq": ["status", "closed"]}])


def search_units(engine):
    return search(engine, "unit", [{"in": ["state", UNIT_STATES]}])


def search_intents(engine):
    return search(engine, "intent", [{"in": ["status", INTENT_STATES]}])


def stop_rows(engine, since, after):
    """(rows, withheld) of `ont log` for stop_unit from `since` to the day `after`, every page; `store_failed` for an
    answer that is not ok, has no `rows` or `withheld`, or gives a continuation twice."""
    q = {"action_type": "stop_unit", "outcome": "applied", "since": since + "T00:00:00Z",
         "until": after + "T00:00:00Z", "parameters": True, "limit": 1000}
    rows, withheld, seen = [], 0, set()
    while True:
        v, ok = engine.call(["log"], q)
        r = v.get("result") if isinstance(v, dict) else None
        if not ok or v.get("status") != "ok" or not isinstance(r, dict):
            raise store_failed()
        if not isinstance(r.get("rows"), list) or not isinstance(r.get("withheld"), int) \
                or isinstance(r.get("withheld"), bool):
            raise store_failed()
        rows.extend(r["rows"])
        withheld += r["withheld"]
        c = r.get("continuation")
        if c is None:
            return rows, withheld
        token = json.dumps(c, sort_keys=True)
        if token in seen:
            raise store_failed()
        seen.add(token)
        q["continuation"] = c


# The reports.
def read_report(path):
    """The text of a report, or None: its path must be absolute, name a regular file reached without a link and
    hold at most 1,048,576 bytes; invalid bytes are replaced."""
    if not isinstance(path, str) or not path.startswith("/") or "\0" in path:
        return None
    parts = [p for p in path.split("/") if p]
    if not parts or any(p in (".", "..") for p in parts):
        return None
    current = "/"
    try:
        for i, part in enumerate(parts):
            current = os.path.join(current, part)
            st = os.lstat(current)
            if stat.S_ISLNK(st.st_mode):
                return None
            if i == len(parts) - 1 and (not stat.S_ISREG(st.st_mode) or st.st_size > REPORT_LIMIT):
                return None
        fd = os.open(current, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(fd, "rb") as f:
            if not stat.S_ISREG(os.fstat(f.fileno()).st_mode):
                return None
            data = f.read(REPORT_LIMIT + 1)
    except OSError:
        return None
    if len(data) > REPORT_LIMIT:
        return None
    return data.decode("utf-8", "replace")


def report_lines(text):
    lines = text.split("\n")
    if lines and lines[-1] == "":
        lines.pop()
    return lines


def boundary(line):
    return bool(BLOCKING_LINE.match(line) or NOTE_LINE.match(line) or SECTION_LINE.match(line))


def blocking_items(kind, lines):
    """[(n, name, text)] of the blocking lines of a page or plan report."""
    out, i, n = [], 0, 0
    while i < len(lines):
        m = BLOCKING_LINE.match(lines[i])
        if not m:
            i += 1
            continue
        j = i + 1
        while j < len(lines) and not boundary(lines[j]):
            j += 1
        n += 1
        name = m.group(1) if m.group(1) in BLOCKING_KINDS[kind] else None
        out.append((n, name, L.cut("\n".join(lines[i:j]), BLOCKING_TEXT)))
        i = j
    return out


# The items.
class Academy:
    """Whether an item belongs to the Academy, from the units and intents the store gave."""

    def __init__(self, units, intents):
        self.units, self.intents = units, intents
        repos = {}
        for f in units.values():
            ik, repo = ref_key(f.get("intent")), f.get("repository")
            if ik and isinstance(repo, str) and repo:
                repos.setdefault(ik, []).append(repo)
        self.repos = repos

    def intent(self, key):
        if key is None:
            return False
        directory = (self.intents.get(key) or {}).get("directory")
        if isinstance(directory, str) and L.is_academy(directory):
            return True
        repos = self.repos.get(key, [])
        return bool(repos) and all(L.is_academy(r) for r in repos)

    def holds(self, repository, unit, intents):
        if isinstance(repository, str) and L.is_academy(repository):
            return True
        repo = (self.units.get(unit) or {}).get("repository") if unit else None
        if isinstance(repo, str) and L.is_academy(repo):
            return True
        return any(self.intent(k) for k in intents)


def new_item(item_id, kind, name, review, unit, intent, at, text, context):
    return {"item": item_id, "kind": kind, "name": name, "review": review, "unit": unit, "intent": intent,
            "at": at, "text": text, "context": context}


def make_items(reviews, units, intents, rows, since, until, counts):
    """Every item of the window in the order of their time, then id. Each carries its `context`, the words the
    request names (review kind and verdict, unit class and lane), which items.json does not hold."""
    academy = Academy(units, intents)
    items = []
    for rkey, f in sorted(reviews.items()):
        kind, verdict = f.get("kind"), f.get("verdict")
        if kind not in ("page", "plan", "packet") or verdict == "abandoned" or f.get("carried_from") is not None:
            continue
        if f.get("status") not in (None, "closed") or not in_window(f.get("closed_at"), since, until):
            continue
        if kind == "packet" and verdict != "repair":
            continue
        unit = ref_key(f.get("unit"))
        own = ref_key(f.get("intent"))
        inherited = ref_key((units.get(unit) or {}).get("intent")) if unit else None
        intent = own if kind == "plan" else (inherited or own)
        report = f.get("report")
        if academy.holds(f.get("repository"), unit, [k for k in (own, inherited) if k]) \
                or (isinstance(report, str) and L.is_academy(report)):
            counts["academy_skipped"] += 1
            continue
        text = read_report(report)
        if text is None:
            counts["report_unreadable"] += 1
            continue
        u = units.get(unit) or {}
        context = {"review_kind": word(kind), "review_verdict": word(verdict), "unit_class": word(u.get("class")),
                   "unit_lane": word(u.get("lane"))}
        at = f["closed_at"]
        lines = report_lines(text)
        found = blocking_items(kind, lines) if kind in BLOCKING_KINDS else []
        for n, name, body in found:
            items.append(new_item(f"{rkey}#{n}", "blocking", name, rkey, unit, intent, at, body, context))
        if not found and verdict == "repair":
            body = L.cut("\n".join(lines[2:]), REPAIR_TEXT)
            items.append(new_item(rkey, "repair", None, rkey, unit, intent, at, body, context))
    for row in rows:
        params = row.get("parameters") if isinstance(row, dict) else None
        if not isinstance(params, dict) or not isinstance(row.get("action_id"), str) \
                or not isinstance(row.get("submitted_at"), str) or not isinstance(params.get("cause"), str):
            raise store_failed()
        at = row["submitted_at"]
        if not in_window(at, since, until):
            continue
        unit = ref_key(params.get("unit"))
        u = units.get(unit) or {}
        intent = ref_key(u.get("intent"))
        if academy.holds(None, unit, [intent] if intent else []):
            counts["academy_skipped"] += 1
            continue
        detail = params.get("detail")
        text = f"cause: {params['cause']}\ndetail: {detail if isinstance(detail, str) else ''}"
        context = {"review_kind": "none", "review_verdict": "none", "unit_class": word(u.get("class")),
                   "unit_lane": word(u.get("lane"))}
        items.append(new_item(row["action_id"], "stop", params["cause"], None, unit, intent, at, text, context))
    items.sort(key=lambda i: (i["at"], i["item"]))
    return items


def classify_text(item):
    c = item["context"]
    return (f"Item kind: {item['kind']}\nItem name: {item['name'] or 'none'}\nReview kind: {c['review_kind']}\n"
            f"Review verdict: {c['review_verdict']}\nUnit class: {c['unit_class']}\nUnit lane: {c['unit_lane']}\n\n"
            + item["text"])


def request_params(model, max_tokens, system, text, schema):
    return {"model": model, "max_tokens": max_tokens, "system": system,
            "messages": [{"role": "user", "content": text}],
            "output_config": {"format": {"type": "json_schema", "schema": schema}}}


# The result lines.
def count_lines(lines, keys, schema):
    """The lines of a result file as BD17 of evals/batch/README.md counts them, for the request keys `keys`:
    (counts, answers by key). Of the lines of one key, the first of type succeeded is counted, else the first."""
    counts = {k: 0 for k in ("read", "refused", "errored", "unreadable", "stale", "duplicate", "skipped")}
    chosen = {}
    for i, parsed in enumerate(lines):
        if parsed is None:
            counts["skipped"] += 1
        elif parsed[0] not in keys:
            counts["stale"] += 1
        elif parsed[0] not in chosen or (parsed[1]["type"] == "succeeded"
                                         and lines[chosen[parsed[0]]][1]["type"] != "succeeded"):
            chosen[parsed[0]] = i
    answers = {}
    for k, i in chosen.items():
        how, value = L.answer(lines[i][1], schema)
        counts[how] += 1
        if how == "read":
            answers[k] = value
    counted = len(lines) - counts["skipped"] - counts["stale"]
    counts["duplicate"] = counted - len(chosen)
    return counts, answers


def read_results(paths):
    lines = []
    for p in paths:
        lines.extend(L.result_lines(p))
    return lines


# The finding pass.
def finding_key(signature):
    return "batch-mining-" + L.sha256(signature)[:16]


def request_id(action, native_key, synthesis_key, parameters):
    return PREFIX + L.sha256(L.canon([action, native_key, synthesis_key, parameters]))[:40]


def apply(engine, action, native_key, synthesis_key, parameters):
    """("applied"|"already_applied", None) or ("failed", code)."""
    body = {"action_type": action, "parameters": parameters, "agent": AGENT,
            "request_id": request_id(action, native_key, synthesis_key, parameters)}
    return engine.apply(body)
