#!/usr/bin/env bash
set -euo pipefail

exec python3 - "$@" <<'PY'
import json
import os
import re
import shlex
import sqlite3
import stat
import subprocess
import sys


def fail(code, message):
    print(f"{code}@pane:1: {message}", file=sys.stderr)
    sys.exit(1)


args = sys.argv[1:]
if args in (["-h"], ["--help"]):
    print("Usage: pane [--exports | --focused | --resume-candidate]\n"
          "  default    Print the caller pane; warn for an unverified session match.\n"
          "  --exports  Print shell exports for the containing pane.\n"
          "  --focused  Print the UI-focused pane (not caller identity).\n"
          "  --resume-candidate  Print a unique Codex launch-time resume match (unverified current session).")
    sys.exit(0)
if args not in ([], ["--exports"], ["--focused"], ["--resume-candidate"]):
    fail("invalid_arguments", "use pane --help")
if os.environ.get("HERDR_ENV") != "1":
    fail("herdr_environment_missing", "run inside a Herdr-managed environment")


def query(*arguments):
    try:
        result = subprocess.run(["herdr", *arguments], capture_output=True,
                                text=True, timeout=5, check=False)
        if result.returncode:
            fail("herdr_query_failed", "could not query the current Herdr session")
        return json.loads(result.stdout)["result"]
    except (OSError, subprocess.TimeoutExpired, ValueError, KeyError):
        fail("herdr_query_failed", "could not parse the current Herdr session")


panes = query("pane", "list").get("panes", [])
if args == ["--focused"]:
    candidates = [p for p in panes if p.get("focused") is True]
else:
    # Only a process's own ancestry is evidence that it belongs to a pane.
    # The UI focus and inherited HERDR_* variables may refer to another pane.
    ancestors = set()
    pid = os.getpid()
    while pid > 1 and pid not in ancestors:
        ancestors.add(pid)
        try:
            with open(f"/proc/{pid}/stat", encoding="ascii") as stream:
                stat_line = stream.read()
            pid = int(stat_line[stat_line.rfind(")") + 2:].split()[1])
        except (OSError, ValueError, IndexError):
            break
    session_id = os.environ.get("CODEX_THREAD_ID", "")
    if not re.fullmatch(r"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}", session_id):
        session_id = ""

    def resumed_pids(info):
        matches = set()
        if not session_id:
            return matches
        for proc in info.get("foreground_processes", []):
            argv = proc.get("argv")
            pid = proc.get("pid")
            if (isinstance(argv, list) and len(argv) >= 3
                    and all(isinstance(value, str) for value in argv[:3])
                    and os.path.basename(argv[0]) == "codex"
                    and argv[1] == "resume" and argv[2] == session_id
                    and isinstance(pid, int)):
                matches.add(pid)
        return matches

    def reported_session(p):
        identity = p.get("agent_session")
        return (isinstance(identity, dict) and p.get("agent") == "codex"
                and identity.get("source") == "herdr:codex"
                and identity.get("kind") == "id"
                and identity.get("value") == session_id and bool(session_id))

    # New Codex TUI sessions do not have a resume ID in argv. If the local
    # metadata is available, correlate their creation time with a frontend's
    # process start and cwd; this is only a candidate, never proof of identity.
    session_origin = None
    if session_id:
        try:
            codex_home = os.environ.get("CODEX_HOME") or os.path.expanduser("~/.codex")
            db_path = os.path.join(codex_home, "state_5.sqlite")
            if os.path.isfile(db_path):
                with sqlite3.connect("file:" + db_path + "?mode=ro", uri=True, timeout=0.2) as db:
                    session_origin = db.execute("SELECT created_at_ms,cwd FROM threads WHERE id=?", (session_id,)).fetchone()
        except (OSError, sqlite3.Error):
            pass

    def new_frontend_pids(info):
        matches = set()
        if not session_origin or not isinstance(session_origin[0], int) or not isinstance(session_origin[1], str):
            return matches
        try:
            with open('/proc/stat', encoding='ascii') as stream:
                boot = next(int(line.split()[1]) for line in stream if line.startswith('btime '))
            ticks = os.sysconf('SC_CLK_TCK')
        except (OSError, ValueError, StopIteration):
            return matches
        for proc in info.get('foreground_processes', []):
            argv, pid, cwd = proc.get('argv'), proc.get('pid'), proc.get('cwd')
            if (not isinstance(argv, list) or len(argv) != 1 or not isinstance(argv[0], str)
                    or os.path.basename(argv[0]) != 'codex' or not isinstance(pid, int)
                    or not isinstance(cwd, str) or os.path.realpath(cwd) != os.path.realpath(session_origin[1])):
                continue
            try:
                with open(f'/proc/{pid}/stat', encoding='ascii') as stream:
                    raw = stream.read()
                started_ms = int((boot + int(raw[raw.rfind(')') + 2:].split()[19]) / ticks) * 1000)
            except (OSError, ValueError, IndexError):
                continue
            if 0 <= session_origin[0] - started_ms <= 15000:
                matches.add(pid)
        return matches

    # The caller's actual terminal is CLI-independent. Do not infer a terminal
    # from cwd, UI focus, inherited environment or another process's arguments.
    caller_ttys = set()
    for fd in (1, 2):
        try:
            current = os.stat(os.ttyname(fd))
            if stat.S_ISCHR(current.st_mode):
                caller_ttys.add(current.st_rdev)
        except OSError:
            pass

    def same_terminal(info):
        pid = info.get("shell_pid")
        if not caller_ttys or not isinstance(pid, int) or pid <= 1:
            return False
        try:
            device = os.stat(f"/proc/{pid}/fd/0")
            return stat.S_ISCHR(device.st_mode) and device.st_rdev in caller_ttys
        except OSError:
            return False

    ancestor_matches = []
    terminal_matches = []
    reported_matches = []
    session_matches = []
    new_matches = []
    for p in panes:
        info = query("pane", "process-info", "--pane", p["pane_id"]).get("process_info", {})
        roots = {info.get("shell_pid")}
        roots.update(proc.get("pid") for proc in info.get("foreground_processes", []))
        if ancestors.intersection(roots):
            ancestor_matches.append(p)
        if same_terminal(info):
            terminal_matches.append(p)
        if reported_session(p):
            reported_matches.append(p)
        pids = resumed_pids(info)
        if pids and not p.get("agent_session"):
            session_matches.append((p, pids))
        fresh_pids = new_frontend_pids(info)
        if fresh_pids and not p.get("agent_session"):
            new_matches.append((p, fresh_pids))
    if args == ["--resume-candidate"]:
        candidates = [p for p, _ in session_matches]
        match_mode = "codex_resume"
        initial_resume_pids = session_matches[0][1] if len(session_matches) == 1 else set()
    elif ancestor_matches:
        candidates = ancestor_matches
        match_mode = "ancestry"
    elif terminal_matches:
        candidates = terminal_matches
        match_mode = "terminal"
    elif reported_matches:
        candidates = reported_matches
        match_mode = "reported_session"
    elif args == [] and session_matches:
        # A resumed frontend supplies a useful best-effort ID, but its argv is
        # not proof of the conversation currently displayed by that frontend.
        candidates = [p for p, _ in session_matches]
        match_mode = "codex_resume"
        initial_resume_pids = session_matches[0][1] if len(session_matches) == 1 else set()
    elif args == [] and new_matches:
        candidates = [p for p, _ in new_matches]
        match_mode = "codex_new"
        initial_resume_pids = new_matches[0][1] if len(new_matches) == 1 else set()
    else:
        candidates = []
        match_mode = "unresolved"

if len(candidates) != 1:
    if args == ["--focused"]:
        fail("focused_pane_unresolved", f"matches={len(candidates)}")
    fail("caller_pane_unresolved", f"matches={len(candidates)}; no verified caller binding")

pane = candidates[0]
if args != ["--focused"]:
    # Reconcile against the live list after inspecting processes: a pane may
    # have moved or closed while the first snapshot was being traversed.
    live_panes = query("pane", "list").get("panes", [])
    live = [p for p in live_panes
            if p.get("pane_id") == pane.get("pane_id")]
    if len(live) != 1 or any(live[0].get(k) != pane.get(k)
                             for k in ("workspace_id", "tab_id")):
        fail("pane_changed_during_lookup", "pane moved or closed")
    info = query("pane", "process-info", "--pane", pane["pane_id"]).get("process_info", {})
    roots = {info.get("shell_pid")}
    roots.update(proc.get("pid") for proc in info.get("foreground_processes", []))
    if match_mode in ("codex_resume", "codex_new", "terminal"):
        current_matches = []
        for item in live_panes:
            current_info = info if item["pane_id"] == pane["pane_id"] else query(
                "pane", "process-info", "--pane", item["pane_id"]).get("process_info", {})
            if ((match_mode == "codex_resume" and resumed_pids(current_info) and not item.get("agent_session"))
                    or (match_mode == "codex_new" and new_frontend_pids(current_info) and not item.get("agent_session"))
                    or (match_mode == "terminal" and same_terminal(current_info))):
                current_matches.append(item["pane_id"])
    if (match_mode == "ancestry" and not ancestors.intersection(roots)) or (
            match_mode == "codex_resume" and (current_matches != [pane["pane_id"]]
                or not initial_resume_pids.intersection(resumed_pids(info)))) or (
            match_mode == "codex_new" and (current_matches != [pane["pane_id"]]
                or not initial_resume_pids.intersection(new_frontend_pids(info)))) or (
            match_mode == "reported_session" and [p["pane_id"] for p in live_panes
                if reported_session(p)] != [pane["pane_id"]]) or (
            match_mode == "terminal" and current_matches != [pane["pane_id"]]):
        fail("pane_changed_during_lookup", "process association changed")
if args == ["--exports"]:
    workspace = pane.get("workspace_id")
    pane_id = pane.get("pane_id")
    tab_id = pane.get("tab_id")
    if not isinstance(workspace, str) or not re.fullmatch(r"w[A-Za-z0-9]+", workspace):
        fail("pane_metadata_invalid", "workspace_id")
    if not isinstance(pane_id, str) or not re.fullmatch(re.escape(workspace) + r":p[A-Za-z0-9]+", pane_id):
        fail("pane_metadata_invalid", "pane_id")
    if not isinstance(tab_id, str) or not re.fullmatch(re.escape(workspace) + r":t[A-Za-z0-9]+", tab_id):
        fail("pane_metadata_invalid", "tab_id")
    exports = (("HERDR_PANE_ID", pane_id), ("HERDR_WORKSPACE_ID", workspace),
               ("HERDR_TAB_ID", tab_id))
    print("\n".join(f"export {name}={shlex.quote(value)}" for name, value in exports))
else:
    if args != ["--focused"] and match_mode in ("codex_resume", "codex_new"):
        print("unverified_session_candidate@pane:1: frontend may have changed sessions", file=sys.stderr)
    print(pane["pane_id"])
PY
