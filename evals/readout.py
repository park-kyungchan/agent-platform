#!/usr/bin/env python3
"""Read-out of K1 (cost of the same work) and the field reading of K2 (implementer tier).

Reads records that already exist and nothing else; stores nothing, writes no file, calls no store.
  Claude session records   <account>/projects/**/*.jsonl   keys: type, sessionId, cwd, gitBranch, timestamp,
                           message.id, message.model, message.usage (top-level members and cache_creation),
                           the name of a tool_use block
  Stage records            <work>/*/**/*.run.json          role, model, session, state, start, answer.path, resumes, cut
  Codex session records    <dir>/**/*.jsonl                session_meta.id and cwd, turn_context.model, token_count totals
  Merged pull requests     gh pr list --state merged       number, headRefName, mergedAt
  Reviewer answers         the first line of the answer file of a reviewer stage
  --calibrate also reads   total_cost_usd of the `result` lines of the stream of first-run stages
It prints no message text, no tool input and no path. Prices: evals/prices.tsv.
Run: python3 -I -B evals/readout.py --since 2026-11-11 --until 2026-11-14
"""
import argparse
import bisect
import collections
import datetime
import json
import statistics
import subprocess
import sys
from pathlib import Path

KINDS = ("input", "output", "cache_read", "cache_write_5m", "cache_write_1h")
ROUTES = ("engineering", "lesson", "other")
UNCOUNTED = (("agy", "no usage record exists under its state directory"),)  # CLIs whose sessions this command cannot count
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


def read_stages(work_root):
    stages, lines = [], collections.Counter()
    for f in sorted(Path(work_root).glob("*/**/*.run.json")):
        try:
            o = json.loads(f.read_text(encoding="utf-8", errors="replace"))
        except (ValueError, OSError):
            o = None
        if not isinstance(o, dict) or not isinstance(o.get("session"), str):
            lines["unreadable"] += 1
            continue
        lines["records"] += 1
        a = o.get("answer")
        stages.append({"role": str(o.get("role") or "unknown"), "model": str(o.get("model") or "unknown"), "session": o["session"], "cwd": str(o.get("cwd") or ""),
                       "state": "cut" if o.get("state") == "cut" or o.get("cut") is True else str(o.get("state")),
                       "start": str(o.get("start") or ""), "answer": a.get("path") if isinstance(a, dict) else None,
                       "ap": "tokens" in o and "resumes" in o, "first_run": o.get("resumes", "x") is None, "file": str(f)})
    return stages, lines


def route_of(cwd, work, platforms, lesson):
    def under(root):
        return cwd == root or cwd.startswith(root.rstrip("/") + "/")
    return "lesson" if under(lesson) else "engineering" if under(work) or under(platforms) else "other"


def role_without_stage(cwd, a):
    """lead: the Lead's own directory; worker, no record: a task worktree or a platform checkout; else interactive."""
    work, platforms, here = a.work_root.rstrip("/") + "/", a.platforms_root.rstrip("/") + "/", cwd + "/"
    if cwd in a.lead_cwd:
        return "lead"
    return "worker, no record" if (here.startswith(work) and "/wt/" in here[len(work) - 1:]) or here.startswith(platforms) else "interactive"


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
    lesson_files = collections.defaultdict(set)
    for m in msgs:
        usd = cost(m["model"], m["tok"], a.rates)
        session_usd[m["session"]] += usd
        week = week_of(m["ts"], a.since, a.until)
        if week is None:
            outside += 1
            continue
        ss = by_session.get(m["session"])
        role = ss[max(bisect.bisect_right([s["start"] for s in ss], m["ts"]) - 1, 0)]["role"] if ss else role_without_stage(m["cwd"], a)
        route = route_of(m["cwd"], a.work_root, a.platforms_root, a.lesson_dir)
        fam = family(m["model"], a.rates)
        add(totals[(week, route)], m["tok"], usd)
        add(rows[(week, route)][("*" if fam == "unknown" else role, fam)], m["tok"], usd)  # one line for every role of an unknown model
        if route == "lesson":
            lesson_files[m["src"]].add(m["file"])
        if role == "lead" and m["src"] == "claude":
            name = "__".join(m["tool"].split("__")[:2])  # mcp__server__tool -> mcp__server
            leads[name]["calls"] += 1
            leads[name]["usd"] += usd
    return totals, rows, leads, outside, session_usd, lesson_files


def first_line(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            line = fh.readline().strip()
    except (OSError, TypeError):
        return "unknown"
    return line[:40] if line.startswith("Verdict:") else "no-verdict-line"


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


def main(argv=None):
    here = Path(__file__).resolve().parent
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--since", required=True)
    ap.add_argument("--until", required=True)
    ap.add_argument("--accounts", nargs="*", help="Claude configuration directories (default: ~/.claude*)")
    ap.add_argument("--codex-dir", default=str(Path.home() / ".codex" / "sessions"))
    ap.add_argument("--work-root", default="/home/palantirkc/work")
    ap.add_argument("--platforms-root", default="/home/palantirkc/platforms")
    ap.add_argument("--lesson-dir", help="default: <work-root>/edu-lessons")
    ap.add_argument("--lead-cwd", action="append", default=[], help="a cwd whose interactive calls are listed by first tool")
    ap.add_argument("--repo", action="append", help="owner/name (default: the three platform repositories)")
    ap.add_argument("--prices", default=str(here / "prices.tsv"))
    ap.add_argument("--list-no-reviewer", action="store_true", help="list the numbers of merged pull requests without a reviewer stage")
    ap.add_argument("--detail", action="store_true", help="one K2 line per merged pull request")
    ap.add_argument("--calibrate", action="store_true", help="also compare the cost of first-run stages with the CLI's own figure")
    a = ap.parse_args(argv)
    for d in (a.since, a.until):
        datetime.date.fromisoformat(d)
    a.lesson_dir = a.lesson_dir or a.work_root.rstrip("/") + "/edu-lessons"
    a.rates = load_prices(a.prices)
    a.stages, st_lines = read_stages(a.work_root)
    accounts = a.accounts if a.accounts is not None else [str(p.parent) for p in sorted(Path.home().glob(".claude*/projects"))]
    since_epoch = datetime.datetime.fromisoformat(a.since).replace(tzinfo=datetime.timezone.utc).timestamp()
    c_msgs, branches, session_cwd, c_lines = read_claude([Path(x) / "projects" for x in accounts if (Path(x) / "projects").is_dir()], since_epoch)
    x_msgs, x_lines = read_codex(a.codex_dir, since_epoch)
    totals, rows, leads, outside, session_usd, lesson_files = aggregate(c_msgs + x_msgs, a)
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
    if a.detail:
        for r in sorted(reading, key=lambda r: (r["repo"], r["number"])):
            print(f"K2 {r['repo']}#{r['number']} implementer={r['impl']} verifier={r['verifier'] or 'none'} first verdict={bucket(r['verdict'])} lines={r['lines']}")
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


if __name__ == "__main__":
    main()
