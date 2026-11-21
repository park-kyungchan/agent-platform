#!/usr/bin/env python3
"""Read-out of K1 (cost of the same work) and the field reading of K2 (implementer tier).

Reads records that already exist and nothing else; stores nothing, writes no file, calls no store (with --detail the
one read of the store is `ap status <key>` per merged pull request of a head branch `unit/<key>`, and it only reads).
  Claude session records   <account>/projects/**/*.jsonl   keys: type, sessionId, cwd, gitBranch, timestamp,
                           message.id, message.model, message.usage (top-level members and cache_creation),
                           the name of a tool_use block
  Stage records            <work>/*/**/*.run.json and, per --stage-dir, <dir>/*.run.json and <dir>/*/*.run.json
                           (the store's unit and review directories by default; nothing deeper is read)
                           role, model, session, state, start, answer.path, prompt.path, resumes, cut
  Codex session records    <dir>/**/*.jsonl                session_meta.id and cwd, turn_context.model, token_count totals
  Merged pull requests     gh pr list --state merged       number, headRefName, mergedAt
  Reviewer answers         the first line of the answer file of a reviewer stage; of a page or plan review of the
                           window, the lines that begin `blocking` (after leading spaces, `-`, `*`, `<digits>.`),
                           counted by the kind they name and never printed
  --detail also reads      the stage streams <units>/<key>/*.stream.jsonl (and their run records' `model`) and, for the review
                           keys `ap status <key>` names, <reviews>/<key>.md.run.json (`cwd`) and .md.stream.jsonl:
                           type, subtype, model of `init`, total_cost_usd, num_turns, message.usage (counts), compact_boundary
  --calibrate also reads   total_cost_usd of the `result` lines of the stream of first-run stages
It prints no message text, no tool input and no path, and writes nothing. Prices: evals/prices.tsv.
Run: python3 -I -B evals/readout.py --since 2026-11-11 --until 2026-11-14
"""
import argparse
import bisect
import collections
import datetime
import json
import re
import statistics
import subprocess
import sys
from pathlib import Path

KINDS = ("input", "output", "cache_read", "cache_write_5m", "cache_write_1h")
ROUTES = ("engineering", "lesson", "temporary", "other")
REVIEW_KINDS = ("page", "plan", "packet", "other")
STAGE_DIRS = ("/var/lib/agent-platform/units", "/var/lib/agent-platform/reviews")  # the store's stage records
TEMP_ROOTS = ("/tmp", "/var/tmp")
UNCOUNTED = (("agy", "no usage record exists under its state directory"),)  # CLIs whose sessions this command cannot count
BLOCKING_KINDS = {  # the closed lists of the frames reviewer.md and plan-reviewer.md, in the order of the frames
    "page": ("unbuildable", "contradiction", "check", "write_set", "unguarded_effect"),
    "plan": ("unreached_criterion", "dependency", "conflict", "unguarded_effect"),
}
BLOCKING_LINE = re.compile(r"(?:[ \-*]|\d+\.)*blocking(?:[ ]([a-z_]+):)?")
UNIT_KEY = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*")
REVIEW_KEY = re.compile(r"act-rq-[0-9a-f]{32}")
MODEL_NAME = re.compile(r"[A-Za-z0-9._:/+-]{1,80}")
REPOS = ("park-kyungchan/agent-platform", "park-kyungchan/ontology-platform", "park-kyungchan/education-platform")


def load_prices(path):
    rates = collections.defaultdict(dict)
    for line in Path(path).read_text().splitlines():
        if line.strip() and not line.startswith("#"):
            model, kind, value = line.split("\t")
            rates[model][kind] = float(value)
    return rates


def family(model, rates):
    if model not in rates:
        return "unknown"
    parts = model.split("-")
    return parts[1] if parts[0] == "claude" else model


def cost(model, tok, rates):
    """USD at list price; 0 for a model without a price row (it is shown under `unknown`)."""
    r = rates.get(model)
    if r is None:
        return 0.0
    prompt = tok["input"] + tok["cache_read"] + tok["cache_write_5m"] + tok["cache_write_1h"]
    if prompt > r.get("long_over_tokens", float("inf")):
        r = rates[model + "+long"]
    return sum(tok[k] * r[k] for k in KINDS) / 1e6


def num(x):
    return x if isinstance(x, int) and not isinstance(x, bool) and x > 0 else 0


def jsonl(path, lines):
    with open(path, encoding="utf-8", errors="replace") as fh:
        for raw in fh:
            lines["read"] += 1
            try:
                o = json.loads(raw)
            except ValueError:
                o = None
            if isinstance(o, dict):
                yield o
            else:
                lines["unreadable"] += 1


def read_claude(projects_dirs, since_epoch):
    """One message per message id (a message spans several lines, and a forked session's file repeats earlier ones).
    A message belongs to the earliest-started session that holds it, and a session has one cwd, its earliest line's:
    neither depends on the order of the files. Files older than the window give only that and the branches of
    their sessions (the K2 join needs them), no message."""
    msgs, branches, lines = {}, collections.defaultdict(set), collections.Counter()
    first_cwd, start = {}, {}
    files = sorted({f.resolve() for d in projects_dirs for f in Path(d).resolve().rglob("*.jsonl")})
    for f in files:
        old = f.stat().st_mtime < since_epoch
        lines["files_older_than_window" if old else "files"] += 1
        for o in jsonl(f, collections.Counter() if old else lines):
            sid, ts = o.get("sessionId"), str(o.get("timestamp") or "")
            if sid and isinstance(o.get("gitBranch"), str):
                branches[sid].add(o["gitBranch"])
            if sid and ts:
                start[sid] = min(start.get(sid, ts), ts)
                if isinstance(o.get("cwd"), str) and o["cwd"] and (ts, o["cwd"]) < first_cwd.get(sid, ("\uffff", "")):
                    first_cwd[sid] = (ts, o["cwd"])
            m = o.get("message")
            if old:
                continue
            if o.get("type") != "assistant" or not isinstance(m, dict) or not isinstance(m.get("usage"), dict):
                lines["other"] += 1
                continue
            if not isinstance(m.get("id"), str):
                lines["unreadable"] += 1
                continue
            u, cc = m["usage"], m["usage"].get("cache_creation")
            w5, w1 = ((num(cc.get("ephemeral_5m_input_tokens")), num(cc.get("ephemeral_1h_input_tokens")))
                      if isinstance(cc, dict) else (num(u.get("cache_creation_input_tokens")), 0))
            tok = dict(zip(KINDS, (num(u.get("input_tokens")), num(u.get("output_tokens")),
                                   num(u.get("cache_read_input_tokens")), w5, w1)))
            tool = next((b["name"] for b in m["content"] if isinstance(b, dict) and b.get("type") == "tool_use"
                         and isinstance(b.get("name"), str)), None) if isinstance(m.get("content"), list) else None
            rec = msgs.get(m["id"])
            if rec is None:
                msgs[m["id"]] = {"ts": ts, "model": m.get("model"), "sessions": {sid}, "cwd": str(o.get("cwd") or ""),
                                 "tool": tool or "none", "tok": tok, "src": "claude", "file": str(f)}
            else:
                lines["repeat"] += 1
                rec["tok"] = {k: max(rec["tok"][k], tok[k]) for k in KINDS}
                rec["sessions"].add(sid)
                rec["file"], rec["ts"] = min(rec["file"], str(f)), min(rec["ts"], ts) if ts else rec["ts"]
                if rec["tool"] == "none" and tool:
                    rec["tool"] = tool
    for rec in msgs.values():
        rec["session"] = min(rec.pop("sessions"), key=lambda s: (start.get(s, "~"), str(s)))
        rec["cwd"] = first_cwd.get(rec["session"], ("", rec["cwd"]))[1]
    lines["messages"] = len(msgs)
    return list(msgs.values()), branches, {s: c for s, (_, c) in first_cwd.items()}, lines


def read_codex(codex_dir, since_epoch):
    """Token events carry cumulative totals: each event contributes its increase over the one before."""
    out, lines = [], collections.Counter()
    for f in sorted(Path(codex_dir).rglob("*.jsonl")) if Path(codex_dir).is_dir() else []:
        if f.stat().st_mtime < since_epoch:
            lines["files_older_than_window"] += 1
            continue
        lines["files"] += 1
        model, cwd, sid, prev = None, "", f.stem, {}
        for o in jsonl(f, lines):
            p = o.get("payload") if isinstance(o.get("payload"), dict) else {}
            if o.get("type") == "session_meta":
                cwd, sid = str(p.get("cwd") or cwd), str(p.get("id") or sid)  # the session's cwd: the first one seen
            elif o.get("type") == "turn_context":
                model, cwd = p.get("model") or model, cwd or str(p.get("cwd") or "")
            elif o.get("type") == "event_msg" and p.get("type") == "token_count" and isinstance(p.get("info"), dict) \
                    and isinstance(p["info"].get("total_token_usage"), dict):
                u = {k: num(v) for k, v in p["info"]["total_token_usage"].items()}
                d = {k: max(0, u[k] - prev.get(k, 0)) for k in u}
                prev = {k: max(u[k], prev.get(k, 0)) for k in u}
                if any(d.values()):
                    cw, cr = d.get("cache_write_input_tokens", 0), d.get("cached_input_tokens", 0)
                    tok = dict(zip(KINDS, (max(0, d.get("input_tokens", 0) - cw - cr), d.get("output_tokens", 0), cr, cw, 0)))
                    out.append({"ts": str(o.get("timestamp") or ""), "model": model, "session": sid, "cwd": cwd,
                                "tool": "none", "tok": tok, "src": "codex", "file": str(f)})
                    lines["events"] += 1
                    continue
                lines["repeat"] += 1
                continue
            lines["other"] += 1
    return out, lines


def review_kind(path):
    """page, plan or packet from the prompt file of a stage record; anything else is `other`."""
    if not isinstance(path, str):
        return "other"
    p = path.split("/")
    if p[-1].startswith("page-"):
        return "page"
    if p[-1].startswith("plan-"):
        return "plan"
    return "packet" if len(p) >= 3 and p[-3] == "units" and re.fullmatch(r"\d+\.verifier\.prompt\.md", p[-1]) else "other"


def read_stages(work_root, stage_dirs=()):
    """A record read from a stage directory is marked `store`; a file reached by both ways is read once."""
    stages, lines, found = [], collections.Counter(), {}
    for f in Path(work_root).glob("*/**/*.run.json"):
        found[f.resolve()] = (f, False)
    for d in stage_dirs:
        d = Path(d)
        for f in (*d.glob("*.run.json"), *d.glob("*/*.run.json")) if d.is_dir() else ():
            found[f.resolve()] = (f, True)
    for f, store in sorted(found.values(), key=lambda v: str(v[0])):
        try:
            o = json.loads(f.read_text(encoding="utf-8", errors="replace"))
        except (ValueError, OSError):
            o = None
        if not isinstance(o, dict) or not isinstance(o.get("session"), str):
            lines["unreadable"] += 1
            continue
        lines["records"] += 1
        if store:
            lines["stage_dir_records"] += 1
        a, pr = o.get("answer"), o.get("prompt")
        stages.append({"role": str(o.get("role") or "unknown"), "model": str(o.get("model") or "unknown"), "session": o["session"], "cwd": str(o.get("cwd") or ""),
                       "state": "cut" if o.get("state") == "cut" or o.get("cut") is True else str(o.get("state")),
                       "start": str(o.get("start") or ""), "answer": a.get("path") if isinstance(a, dict) else None,
                       "kind": review_kind(pr.get("path") if isinstance(pr, dict) else None), "store": store,
                       "ap": "tokens" in o and "resumes" in o, "first_run": o.get("resumes", "x") is None, "file": str(f)})
    return stages, lines


def route_of(cwd, work, platforms, lesson, stage=None):
    """The rules in order: lesson; a stage record read from a stage directory; the work and platforms roots;
    a session with no stage record under a temporary directory; else `other`."""
    def under(root):
        return cwd == root or cwd.startswith(root.rstrip("/") + "/")
    if under(lesson):
        return "lesson"
    if (stage or {}).get("store") or under(work) or under(platforms):
        return "engineering"
    return "temporary" if stage is None and any(under(t) for t in TEMP_ROOTS) else "other"


def role_without_stage(cwd, a):
    """lead: the Lead's own directory, or (no --lead-cwd given) a task directory under the work root outside the lesson
    directory; worker, no record: a task worktree or a platform checkout; else interactive."""
    work, platforms, here = a.work_root.rstrip("/") + "/", a.platforms_root.rstrip("/") + "/", cwd + "/"
    if cwd in a.lead_cwd:
        return "lead"
    if (here.startswith(work) and "/wt/" in here[len(work) - 1:]) or here.startswith(platforms):
        return "worker, no record"
    task = here.startswith(work) and here != work and not here.startswith(a.lesson_dir.rstrip("/") + "/")
    return "lead" if task and not a.lead_cwd else "interactive"


def week_of(ts, since, until):
    try:
        d = datetime.date.fromisoformat(ts[:10])
    except ValueError:
        return None
    return (d - datetime.timedelta(days=d.weekday())).isoformat() if since <= ts[:10] <= until else None


def add(row, tok, usd):
    for k in KINDS:
        row[k] += tok[k]
    row["msgs"] += 1
    row["usd"] += usd


def aggregate(msgs, a):
    by_session = collections.defaultdict(list)
    for s in sorted(a.stages, key=lambda s: s["start"]):
        by_session[s["session"]].append(s)
    totals, rows = collections.defaultdict(collections.Counter), collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
    leads, outside, session_usd = collections.defaultdict(collections.Counter), 0, collections.defaultdict(float)
    kinds = collections.defaultdict(collections.Counter)
    lesson_files = collections.defaultdict(set)
    for m in msgs:
        usd = cost(m["model"], m["tok"], a.rates)
        session_usd[m["session"]] += usd
        week = week_of(m["ts"], a.since, a.until)
        if week is None:
            outside += 1
            continue
        ss = by_session.get(m["session"])
        st = ss[max(bisect.bisect_right([s["start"] for s in ss], m["ts"]) - 1, 0)] if ss else None
        role = st["role"] if st else role_without_stage(m["cwd"], a)
        route = route_of(m["cwd"], a.work_root, a.platforms_root, a.lesson_dir, st)
        fam = family(m["model"], a.rates)
        add(totals[(week, route)], m["tok"], usd)
        add(rows[(week, route)][("*" if fam == "unknown" else role, fam)], m["tok"], usd)  # one line for every role of an unknown model
        if role == "reviewer":
            add(kinds[(week, st["kind"])], m["tok"], usd)  # outside the route blocks: no route total changes
        if route == "lesson":
            lesson_files[m["src"]].add(m["file"])
        if role == "lead" and m["src"] == "claude":
            name = "__".join(m["tool"].split("__")[:2])  # mcp__server__tool -> mcp__server
            leads[name]["calls"] += 1
            leads[name]["usd"] += usd
    return totals, rows, leads, outside, session_usd, lesson_files, kinds


def first_line(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            line = fh.readline().strip()
    except (OSError, TypeError):
        return "unknown"
    return line[:40] if line.startswith("Verdict:") else "no-verdict-line"


def blocking_counts(stages, since, until):
    """{kind: (reviews, Counter of names and `unnamed`)} for the page and plan reviews that started in the window.

    A line counts when it begins `blocking` (case-sensitive) after the leading marks; it is of a kind's name when
    `blocking <name>:` names one of that kind's list, else `unnamed`. An answer that cannot be read is a review and adds nothing."""
    out = {}
    for s in stages:
        if s["kind"] not in BLOCKING_KINDS or not since <= s["start"][:10] <= until:
            continue
        row = out.setdefault(s["kind"], [0, collections.Counter()])
        row[0] += 1
        counts = row[1]
        try:
            with open(s["answer"], encoding="utf-8", errors="replace") as fh:
                text = fh.read()
        except (OSError, TypeError, ValueError):
            continue
        for line in text.split("\n"):
            m = BLOCKING_LINE.match(line)
            if m:
                counts[m.group(1) if m.group(1) in BLOCKING_KINDS[s["kind"]] else "unnamed"] += 1
    return out


DETACHED = {"HEAD", "main", "master"}  # a reviewer's branch when it ran on a detached head or on the base


def repo_of(cwd, names, platforms):
    """The repository of a stage's worktree: `<work>/<task>/wt/<repo>[-suffix]` or `<platforms>/<repo>`."""
    p, root = cwd.split("/"), platforms.rstrip("/") + "/"
    name = p[p.index("wt", 1) + 1] if "wt" in p[1:-1] else cwd[len(root):].split("/")[0] if (cwd + "/").startswith(root) else ""
    return next((n for n in sorted(names, key=len, reverse=True) if name == n or name.startswith(n + "-")), None)


def k2(prs, stages, branches, session_cwd, platforms):
    """Per merged pull request: the implementer's tier, the first reviewer's tier and verdict line, changed lines.
    A stage joins by repository (its worktree) and head branch; a reviewer on a detached head or on the base joins
    by the implementer's worktree when it started after the implementer and not after the merge."""
    names = {r.split("/")[1] for r, _ in prs}
    where = {id(s): (s["cwd"] or session_cwd.get(s["session"], "")) for s in stages}
    out = []
    for repo, pr in prs:
        mine = [s for s in stages if repo_of(where[id(s)], names, platforms) == repo.split("/")[1]]
        def pick(role, ok):
            hit = [s for s in mine if s["role"] == role and ok(s)]
            return min(hit, key=lambda s: s["start"]) if hit else None
        on_branch = lambda s: pr["headRefName"] in branches.get(s["session"], ())
        impl = pick("implementer", on_branch)
        rev = pick("reviewer", lambda s: on_branch(s) or bool(impl and where[id(s)] == where[id(impl)] and impl["start"] <= s["start"] <= str(pr["mergedAt"])
                                                              and branches.get(s["session"]) and branches[s["session"]] <= DETACHED))
        out.append({"repo": repo.split("/")[1], "number": pr["number"], "impl": impl["model"] if impl else "unknown",
                    "verdict": first_line(rev["answer"]) if rev else None, "verifier": rev["model"] if rev else None,
                    "lines": num(pr.get("additions")) + num(pr.get("deletions"))})
    return out


def bucket(verdict):
    return "none" if verdict is None else verdict[len("Verdict: "):] if verdict in ("Verdict: accept", "Verdict: repair") else "other"


def run_gh(repo, since, until):
    r = subprocess.run(["gh", "pr", "list", "--repo", repo, "--state", "merged", "--limit", "1000", "--search",
                        f"merged:{since}..{until}", "--json", "number,headRefName,mergedAt,additions,deletions"],
                       capture_output=True, text=True, check=False)
    if r.returncode:
        sys.exit(f"gh_failed@evals/readout.py: gh pr list exit {r.returncode} for {repo}")
    return json.loads(r.stdout)


def last_cost(stream):
    value = None
    try:
        with open(stream, encoding="utf-8", errors="replace") as fh:
            for raw in fh:
                if '"total_cost_usd"' in raw:
                    try:
                        o = json.loads(raw)
                    except ValueError:
                        continue
                    if isinstance(o, dict) and o.get("type") == "result" and isinstance(o.get("total_cost_usd"), (int, float)):
                        value = o["total_cost_usd"]
    except OSError:
        pass
    return value


def fmt(c):
    return " ".join(f"{k}={c[k]:,}" for k in KINDS) + f" usd={c['usd']:.2f}"


def unit_key(branch):
    """The unit key of a head branch `unit/<key>`, or None (also for a key that is no plain file name)."""
    if not isinstance(branch, str) or not branch.startswith("unit/"):
        return None
    key = branch[len("unit/"):]
    return key if UNIT_KEY.fullmatch(key) and ".." not in key else None


def stage_of(path):
    """One stage stream: cost and turns from its `result` lines (the CLI's own figures), the model of its `system` `init`
    line, peak context (largest input + cache read + cache creation of one message), compactions. None when unreadable."""
    out = {"usd": 0.0, "turns": 0, "peak_ctx": 0, "compactions": 0, "model": "unknown"}
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            for raw in fh:
                try:
                    o = json.loads(raw)
                except ValueError:
                    continue
                if not isinstance(o, dict):
                    continue
                if o.get("type") == "result":
                    c = o.get("total_cost_usd")
                    out["usd"] += c if isinstance(c, (int, float)) and not isinstance(c, bool) else 0
                    out["turns"] += num(o.get("num_turns"))
                elif o.get("type") == "system" and o.get("subtype") == "compact_boundary":
                    out["compactions"] += 1
                elif o.get("type") == "system" and o.get("subtype") == "init":
                    m = o.get("model")
                    out["model"] = m if isinstance(m, str) and MODEL_NAME.fullmatch(m) else "unknown"
                elif o.get("type") == "assistant" and isinstance(o.get("message"), dict) and isinstance(o["message"].get("usage"), dict):
                    u = o["message"]["usage"]
                    cc = u.get("cache_creation")
                    write = num(cc.get("ephemeral_5m_input_tokens")) + num(cc.get("ephemeral_1h_input_tokens")) if isinstance(cc, dict) \
                        else num(u.get("cache_creation_input_tokens"))
                    out["peak_ctx"] = max(out["peak_ctx"], num(u.get("input_tokens")) + num(u.get("cache_read_input_tokens")) + write)
    except OSError:
        return None
    return out


def run_ap(key):
    """`ap status <key>` answered as an object, or None when it fails, times out or answers no JSON object."""
    try:
        r = subprocess.run(["ap", "status", key], capture_output=True, text=True, check=False, timeout=60)
        o = json.loads(r.stdout) if r.returncode == 0 else None
    except (OSError, ValueError, subprocess.SubprocessError):
        return None
    return o if isinstance(o, dict) else None


def natural(name):
    head = name.split(".")[0]
    return (int(head) if head.isdigit() else float("inf"), name)


def record_of(path, suffix):
    try:
        o = json.loads(Path(str(path)[:-len(".stream.jsonl")] + suffix).read_text(encoding="utf-8", errors="replace"))
    except (OSError, ValueError):
        return None
    return o if isinstance(o, dict) else None


def unit_report(key, stage_dirs):
    """The stages of one unit and the model of its first implementer's run record: {"stages", "impl", "ap_failed"}.
    Streams: <units>/<key>/*.stream.jsonl (name without the stream suffix and a trailing `.md`; the role is the second
    dotted part, `implementer` or `fixer`). Reviews: the `reviews[].review` keys of `ap status <key>` that have a run
    record and a stream under <reviews>; `verifier` when the run record's cwd is under /units/<key>/, else `page review`."""
    by_base = {}
    for d in stage_dirs:
        by_base.setdefault(Path(d).name, Path(d))
    stages, impl = [], None
    units = by_base.get("units")
    names = sorted(((p.name[:-len(".stream.jsonl")], p) for p in (units / key).glob("*.stream.jsonl")), key=lambda v: natural(v[0])) if units else []
    for raw, p in names:
        name = raw[:-len(".md")] if raw.endswith(".md") else raw
        role = name.split(".")[1] if name.count(".") >= 1 and name.split(".")[1] else "unknown"
        s = stage_of(p)
        if s is None:
            continue
        stages.append({"role": role, "name": name, **s})
        if impl is None and role == "implementer":
            rec = record_of(p, ".run.json")
            impl = rec["model"] if rec and isinstance(rec.get("model"), str) and MODEL_NAME.fullmatch(rec["model"]) else None
    answer = run_ap(key)
    reviews = by_base.get("reviews")
    if answer is not None and reviews:
        found = sorted({r["review"] for r in answer.get("reviews") or [] if isinstance(r, dict) and isinstance(r.get("review"), str)
                        and REVIEW_KEY.fullmatch(r["review"])})
        for k in found:
            stream = reviews / f"{k}.md.stream.jsonl"
            rec = record_of(stream, ".run.json")
            s = stage_of(stream) if rec is not None else None
            if s is not None:
                stages.append({"role": "verifier" if f"/units/{key}/" in str(rec.get("cwd") or "") + "/" else "page review", "name": k, **s})
    return {"stages": stages, "impl": impl, "ap_failed": answer is None}


def stage_lines(key, pr_label, rep):
    out = [f"stage {key} {s['role']} {s['name']} model={s['model']} usd={s['usd']:.2f} turns={s['turns']} peak_ctx={s['peak_ctx']} "
           f"compactions={s['compactions']}" for s in rep["stages"]]
    out.append(f"unit {key} {pr_label} stages={len(rep['stages'])} usd={sum(s['usd'] for s in rep['stages']):.2f}"
               + (" ap_status_failed=1" if rep["ap_failed"] else ""))
    return out


def main(argv=None):
    here = Path(__file__).resolve().parent
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--since", required=True)
    ap.add_argument("--until", required=True)
    ap.add_argument("--accounts", nargs="*", help="Claude configuration directories (default: ~/.claude*)")
    ap.add_argument("--codex-dir", default=str(Path.home() / ".codex" / "sessions"))
    ap.add_argument("--work-root", default="/home/palantirkc/work")
    ap.add_argument("--platforms-root", default="/home/palantirkc/platforms")
    ap.add_argument("--stage-dir", action="append", help="a directory of stage records, read at depth <dir>/*.run.json and <dir>/*/*.run.json, "
                    "beside the work root; repeatable (default: the store's unit and review directories)")
    ap.add_argument("--lesson-dir", help="default: <work-root>/edu-lessons")
    ap.add_argument("--lead-cwd", action="append", default=[], help="a cwd whose interactive calls are listed by first tool")
    ap.add_argument("--repo", action="append", help="owner/name (default: the three platform repositories)")
    ap.add_argument("--prices", default=str(here / "prices.tsv"))
    ap.add_argument("--list-no-reviewer", action="store_true", help="list the numbers of merged pull requests without a reviewer stage")
    ap.add_argument("--detail", action="store_true", help="one K2 line per merged pull request; after it, the stages of a unit/<key> pull request, and a closing `lead share=` line")
    ap.add_argument("--calibrate", action="store_true", help="also compare the cost of first-run stages with the CLI's own figure")
    a = ap.parse_args(argv)
    for d in (a.since, a.until):
        datetime.date.fromisoformat(d)
    a.lesson_dir = a.lesson_dir or a.work_root.rstrip("/") + "/edu-lessons"
    a.stage_dir = a.stage_dir or list(STAGE_DIRS)
    a.rates = load_prices(a.prices)
    a.stages, st_lines = read_stages(a.work_root, a.stage_dir)
    accounts = a.accounts if a.accounts is not None else [str(p.parent) for p in sorted(Path.home().glob(".claude*/projects"))]
    since_epoch = datetime.datetime.fromisoformat(a.since).replace(tzinfo=datetime.timezone.utc).timestamp()
    c_msgs, branches, session_cwd, c_lines = read_claude([Path(x) / "projects" for x in accounts if (Path(x) / "projects").is_dir()], since_epoch)
    x_msgs, x_lines = read_codex(a.codex_dir, since_epoch)
    totals, rows, leads, outside, session_usd, lesson_files, kinds = aggregate(c_msgs + x_msgs, a)
    prs = [(r, p) for r in (a.repo or REPOS) for p in run_gh(r, a.since, a.until) if a.since <= str(p.get("mergedAt"))[:10] <= a.until]

    print(f"readout window {a.since}..{a.until} (UTC, inclusive); weights evals/prices.tsv; weeks start on Monday")
    print("input claude " + " ".join(f"{k}={v}" for k, v in sorted(c_lines.items())) + f" outside_window={sum(1 for m in c_msgs if week_of(m['ts'], a.since, a.until) is None)}")
    print("input codex " + " ".join(f"{k}={v}" for k, v in sorted(x_lines.items())) + f" outside_window={sum(1 for m in x_msgs if week_of(m['ts'], a.since, a.until) is None)}")
    seen = {m["session"] for m in c_msgs + x_msgs}
    print(f"input stages " + " ".join(f"{k}={v}" for k, v in sorted(st_lines.items())) + f" without_session_record={sum(1 for s in a.stages if s['session'] not in seen)}")
    days = (datetime.date.fromisoformat(a.until) - datetime.date.fromisoformat(a.since)).days + 1
    weeks = sorted({week_of((datetime.date.fromisoformat(a.since) + datetime.timedelta(n)).isoformat(), a.since, a.until) for n in range(days)})
    for w in weeks:
        for route in ROUTES:
            key = (w, route)
            if key not in totals:
                print(f"week {w} route {route} msgs=0")
                continue
            print(f"week {w} route {route} TOTAL msgs={totals[key]['msgs']} {fmt(totals[key])}")
            for (role, fam), c in sorted(rows[key].items()):
                print(f"  role={role} family={fam} msgs={c['msgs']} {fmt(c)}")
    print("route lesson session files in the window: " + " ".join(f"{s}={len(lesson_files[s])}" for s in ("claude", "codex")))
    for w in weeks:
        for kind in REVIEW_KINDS:
            c = kinds.get((w, kind))
            print(f"week {w} reviewer kind {kind} " + (f"msgs={c['msgs']} {fmt(c)}" if c else "msgs=0"))
    print("uncounted: " + "; ".join(f"{cli} ({why})" for cli, why in UNCOUNTED))
    for w in weeks:
        mine = [(r, p) for r, p in prs if week_of(str(p["mergedAt"]), a.since, a.until) == w]
        eng = totals.get((w, "engineering"), collections.Counter())["usd"]
        lead = sum(c["usd"] for (role, _), c in rows.get((w, "engineering"), {}).items() if role == "lead")
        per_repo = collections.Counter(r.split("/")[1] for r, _ in mine)
        print(f"week {w} merged pull requests {len(mine)} (" + ", ".join(f"{r}={n}" for r, n in sorted(per_repo.items())) + ")"
              + f" K1 engineering usd per merged pull request = " + (f"{eng / len(mine):.2f}" if mine else "n/a")
              + f"; role lead usd={lead:.2f} share of the route={100 * lead / eng if eng else 0:.1f}%")
    reading = k2(prs, a.stages, branches, session_cwd, a.platforms_root)
    reports, stages_usd = {}, 0.0  # with --detail only: the stages of each merged pull request of a head branch unit/<key>
    if a.detail:
        heads = {(r.split("/")[1], p["number"]): (r, unit_key(p.get("headRefName"))) for r, p in prs}
        by_key = {}
        for r in reading:
            full, key = heads[(r["repo"], r["number"])]
            if key is None:
                continue
            if key not in by_key:  # one `ap status` read per unit
                by_key[key] = unit_report(key, a.stage_dir)
                stages_usd += sum(s["usd"] for s in by_key[key]["stages"])
            reports[(r["repo"], r["number"])] = (stage_lines(key, f"{full}#{r['number']}", by_key[key]))
            r["impl"] = by_key[key]["impl"] or r["impl"]
        for r in sorted(reading, key=lambda r: (r["repo"], r["number"])):
            print(f"K2 {r['repo']}#{r['number']} implementer={r['impl']} verifier={r['verifier'] or 'none'} first verdict={bucket(r['verdict'])} lines={r['lines']}")
            for line in reports.get((r["repo"], r["number"]), ()):
                print(line)
    by_impl = collections.defaultdict(lambda: collections.defaultdict(list))
    for r in reading:
        by_impl[r["impl"]][bucket(r["verdict"])].append(r)
    for impl, buckets in sorted(by_impl.items()):
        every = [r for rs in buckets.values() for r in rs]
        print(f"K2 implementer={impl} merged={len(every)} lines={sum(r['lines'] for r in every)}: " + ", ".join(
            f"{b} n={len(rs)} lines={sum(r['lines'] for r in rs)}" for b, rs in sorted(buckets.items()))
            + "; verifier tiers: " + (", ".join(f"{v}={n}" for v, n in sorted(collections.Counter(r["verifier"] for r in every if r["verifier"]).items())) or "none"))
    missing = collections.defaultdict(list)
    for r in sorted(reading, key=lambda r: (r["repo"], r["number"])):
        if r["verdict"] is None:
            missing[r["repo"]].append(r["number"])
    print("K2 no reviewer stage: " + (", ".join(f"{k}={len(v)}" for k, v in sorted(missing.items())) or "none"))
    if a.list_no_reviewer:
        for k, v in sorted(missing.items()):
            print(f"K2 no reviewer stage {k}: " + ",".join(map(str, v)))
    cut, undated = collections.Counter(), collections.Counter()
    for s in a.stages:  # an older-form record has no start: it is counted under `no date`
        if s["state"] == "cut" and not s["start"]:
            undated[(s["role"], s["model"])] += 1
        elif s["state"] == "cut" and a.since <= s["start"][:10] <= a.until:
            cut[(s["role"], s["model"])] += 1
    show = lambda c: ", ".join(f"{r}/{m}={n}" for (r, m), n in sorted(c.items()))
    print("stages cut: " + (show(cut) or "none") + (f"; no date: {show(undated)}" if undated else ""))
    found = blocking_counts(a.stages, a.since, a.until)
    for kind in BLOCKING_KINDS:
        if kind in found:
            n, c = found[kind]
            print(f"BLOCKING {kind} reviews={n} " + " ".join(f"{k}={c[k]}" for k in (*BLOCKING_KINDS[kind], "unnamed")))
    ranked = sorted(leads.items(), key=lambda kv: (-kv[1]["usd"], kv[0]))
    for tool, c in ranked[:6]:
        print(f"lead calls first_tool={tool} calls={c['calls']} usd={c['usd']:.2f}")
    if ranked[6:]:
        print(f"lead calls other {len(ranked) - 6} tool names calls={sum(c['calls'] for _, c in ranked[6:])} usd={sum(c['usd'] for _, c in ranked[6:]):.2f}")
    if a.calibrate:
        ratios, shared = collections.defaultdict(list), collections.Counter(s["session"] for s in a.stages)
        for s in a.stages:  # a session that a later record continues holds more than this run's figure
            ok = s["ap"] and s["first_run"] and shared[s["session"]] == 1 and a.since <= s["start"][:10] <= a.until
            figure = last_cost(s["file"][:-len(".run.json")] + ".stream.jsonl") if ok else None
            if figure and s["session"] in session_usd:
                ratios[s["model"]].append(session_usd[s["session"]] / figure)
        for fam, r in sorted(ratios.items()):
            print(f"calibrate family={fam} stages={len(r)} median(readout/cli)={statistics.median(r):.3f} min={min(r):.3f} max={max(r):.3f}")
    if a.detail:  # two bases: lead_usd is priced from token counts as K1 prices them, stages_usd is the CLI's own total_cost_usd
        lead = sum(c["usd"] for by_role in rows.values() for (role, _), c in by_role.items() if role == "lead")
        print(f"lead share=" + (f"{100 * lead / (lead + stages_usd):.1f}%" if lead + stages_usd else "n/a") + f" lead_usd={lead:.2f} stages_usd={stages_usd:.2f}")


if __name__ == "__main__":
    main()
