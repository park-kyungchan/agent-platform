#!/usr/bin/env bash
# A fake `claude` for the tests of `ap run` (the form of ci/test-lead-review.sh's fake). It prints
# stream-json events in the shape a real `claude --print --output-format stream-json --verbose`
# printed on this host (Claude Code 2.1.325). Its directory holds its state: `mode-<account
# basename>` chooses the behaviour, `calls.log` gets one line per call, `env.<n>` the environment.
here=$(cd "$(dirname "$0")" && pwd)
n=$(( $(cat "$here/count" 2>/dev/null || echo 0) + 1 )); echo "$n" > "$here/count"
printf '%s|%s\n' "$CLAUDE_CONFIG_DIR" "$*" >> "$here/calls.log"
env > "$here/env.$n"
cat > "$here/prompt.$n"
sid=; prev=; for a in "$@"; do case "$prev" in --session-id|--resume) sid=$a ;; esac; prev=$a; done
mode=$(cat "$here/mode-$(basename "$CLAUDE_CONFIG_DIR")" 2>/dev/null)
init() { printf '{"type":"system","subtype":"init","session_id":"%s","mcp_servers":[]}\n' "$sid"; }
tool() { printf '{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"true"}}]},"session_id":"%s"}\n' "$sid"; }
said() { printf '{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"%s"}]},"session_id":"%s"}\n' "$1" "$sid"; }
rejected() { printf '{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour","resetsAt":1794648600},"session_id":"%s"}\n' "$sid"; }
# result <is_error> <text, JSON-escaped> <turns> [permission denials]
result() { printf '{"type":"result","subtype":"%s","is_error":%s,"num_turns":%s,"result":"%s","session_id":"%s","usage":{"input_tokens":2,"cache_creation_input_tokens":14047,"cache_read_input_tokens":9064,"output_tokens":4},"permission_denials":%s,"api_error_status":null}\n' \
  "$([ "$1" = true ] && echo error_during_execution || echo success)" "$1" "$3" "$2" "$sid" "${4:-[]}"; }
case "$mode" in
  noout)   exit 1 ;;
  hang)    # the answer arrives, then the session waits for a shell it started (as a real one does)
           init; said "Verdict: accept"; result false 'Verdict: accept\nhead' 3
           /bin/bash -c 'echo $$ > "'"$here"'/child.pid"; sleep 600; :' & wait $! ;;
  other)   # no answer ever: only a shell that never ends
           init; /bin/bash -c 'echo $$ > "'"$here"'/child.pid"; sleep 600; :' & wait $! ;;
  twice)   # an answer, then a shell that ends after 3 s, then a second turn's answer and the exit
           init; said "first"; result false 'Verdict: accept\nfirst' 2
           /bin/bash -c 'echo $$ > "'"$here"'/child.pid"; sleep 3; :' & wait $!
           said "second"; result false 'Verdict: accept\nsecond' 1 ;;
  limitok) # the usage-limit text in a result that does not say is_error, no rejected event
           init; result false 'You have hit your 5-hour limit · resets 6:30pm (UTC)' 1 ;;
  limitrej) # the same with a rejected rate-limit event
           init; rejected; result false 'You have hit your 5-hour limit · resets 6:30pm (UTC)' 1 ;;
  resets)  # a finished review that happens to say "resets"
           init; result false 'Verdict: accept\nThe guard resets the counter before each retry; no defect found.' 2 ;;
  swap)    # the stream's name is replaced by a file with another run's lines; this run answers on
           init; p=$(readlink "/proc/$$/fd/1"); rm -f "$p"
           { printf '%01000d\n' 0; printf '{"type":"result","is_error":false,"num_turns":1,"result":"ANSWER OF RUN A","session_id":"other"}\n'; } > "$p"
           sleep 0.5; result false 'Verdict: accept\nown' 1 ;;
  fifo)    # the stream's name becomes a named pipe that nobody writes
           init; p=$(readlink "/proc/$$/fd/1"); rm -f "$p"; mkfifo "$p"; sleep 0.5; result false 'Verdict: accept\nown' 1 ;;
  blockout) # a directory appears where the answer goes: the answer cannot be written
           init; p=$(readlink "/proc/$$/fd/1"); mkdir "${p%.stream.jsonl}"; result false 'Verdict: accept' 1 ;;
  late)    # no result before the bound; one is written while the stage is being stopped
           init; sleep 2.5; result false 'Verdict: accept\nlate' 1; sleep 600 ;;
  big)     # 40 MiB of other lines, then the result, then the exit
           init; python3 -I -c 'import sys; sys.stdout.write(("x" * 1023 + "\n") * 40960)'; result false 'Verdict: accept\nafter 40 MiB' 1 ;;
  nonl)    # the last result line has no newline
           init; result false 'Verdict: accept' 1 | tr -d '\n' ;;
  limit1)  init; rejected; result true 'You have hit your 5-hour limit · resets 6:30pm (UTC)' 1 ;;
  limit45) init; tool; rejected; result true 'You have hit your 5-hour limit · resets 6:30pm (UTC)' 45 ;;
  watch)   # the record as it is while the CLI runs
           init; cp "$(cat "$here/watch")" "$here/seen.json"; result false 'Verdict: accept' 2 ;;
  answer)  # the bytes of the file `answer`, JSON-encoded
           init; python3 -I -c 'import json,sys; print(json.dumps({"type":"result","subtype":"success","is_error":False,"num_turns":2,"result":open(sys.argv[1],encoding="utf-8",newline="").read(),"session_id":sys.argv[2],"usage":{"input_tokens":7,"output_tokens":9},"permission_denials":[]}))' "$here/answer" "$sid" ;;
  denied)  init; tool; result false 'done' 2 '[{"tool_name":"Bash","tool_use_id":"t1","tool_input":{"command":"git push"}}]' ;;
  file)    init; tool; echo ready > a.txt; result false 'done' 2 ;;
  skeleton) # replays the event skeleton in `skeleton`; the n-th result answers line n of `answers`
           python3 -I - "$here/skeleton" "$here/answers" "$sid" <<'PY'
import json, sys
answers, n = open(sys.argv[2], encoding="utf-8").read().split("\n"), 0
for line in open(sys.argv[1], encoding="utf-8"):
    if line.startswith("#") or not line.strip():
        continue
    kind, sub = line.split()
    e = {"type": kind, "session_id": sys.argv[3]}
    if kind == "result":
        e.update(subtype=sub, is_error=False, num_turns=1, result=answers[n].replace("\\n", "\n"),
                 usage={"input_tokens": 1, "output_tokens": 1}, permission_denials=[])
        n += 1
    elif kind in ("assistant", "user"):
        block = {"type": sub, "id": "t1", "name": "Bash", "input": {}} if sub == "tool_use" else {"type": sub}
        e["message"] = {"role": kind, "content": [block]}
    elif kind == "rate_limit_event":
        e["rate_limit_info"] = {"status": "allowed"}
    elif sub != "-":
        e["subtype"] = sub
    print(json.dumps(e), flush=True)
PY
           ;;
  object)  # a result whose structured output is the file `object` (none without it)
           init; python3 -I -c 'import json,os,sys; e={"type":"result","subtype":"success","is_error":False,"num_turns":1,"result":"Verdict: accept","session_id":sys.argv[2],"usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}
if os.path.exists(sys.argv[1]): e["structured_output"]=json.load(open(sys.argv[1]))
print(json.dumps(e))' "$here/object" "$sid" ;;
  script)  # `ap unit`'s stages: line n of `script` is the n-th call's word (see crates/ap/tests/unit.rs)
           python3 -I - "$here" "$n" "$sid" <<'PY'
import json, subprocess, sys, time
here, n, sid = sys.argv[1], int(sys.argv[2]), sys.argv[3]
word = open(here + "/script").read().split("\n")[n - 1].strip()
git = lambda *a: subprocess.run(["git", *a], capture_output=True, text=True, check=True).stdout.strip()
head = lambda: git("rev-parse", "HEAD")
emit = lambda e: print(json.dumps(e), flush=True)
def commit():
    open("work.txt", "a").write(f"{n}\n"); git("add", "-A"); git("commit", "-q", "-m", f"stage {n}")
def result(obj=None, text="done", error=False):
    e = {"type": "result", "subtype": "error_during_execution" if error else "success", "is_error": error,
         "num_turns": 2, "result": text if obj is None else json.dumps(obj), "session_id": sid,
         "usage": {"input_tokens": 10, "output_tokens": 5, "cache_read_input_tokens": 100, "cache_creation_input_tokens": 1},
         "permission_denials": []}
    if obj is not None:
        e["structured_output"] = obj
    emit(e)
emit({"type": "system", "subtype": "init", "session_id": sid})
if word.startswith("impl"):
    commit()
    if word == "impl-red":
        open(here + "/red", "w").write(head())
    result()
elif word.startswith("cut"):
    reset = int(time.time()) + (300 if word == "cut-in" else 30 * 86400)
    emit({"type": "rate_limit_event", "rate_limit_info": {"status": "rejected", "resetsAt": reset}, "session_id": sid})
    result(text="You have hit your 5-hour limit", error=True)
elif word == "fail":
    sys.exit(1)
elif word.startswith("fix"):
    items = json.loads([l for l in open(f"{here}/prompt.{n}") if l.startswith("Items: ")][-1][7:])
    if word == "fix-test":
        git("rm", "-q", "tests/x_test.rs")
    commit()
    open(here + "/answered", "w").write("1")
    d = [{"id": i["id"], "disposition": "taken", "reason": "fixed", "commit": head()} for i in items]
    if word == "fix-refuse":
        d[0].update(disposition="refused", commit=None, reason="not a defect")
    if word == "fix-red":
        open(here + "/red", "w").write(head())
    if word == "fix-short":
        d = d[1:]
    result({"head": head(), "dispositions": d})
else:
    v = {"verdict": "accept", "head": head(), "blocking": [], "notes": []}
    if word.startswith("repair"):
        v.update(verdict="repair", blocking=[{"id": "B1", "kind": 1, "path": "work.txt", "line": 1, "claim": "wrong", "shown_by": "cat work.txt"}])
    if word.endswith("notes"):
        v["notes"] = [{"id": "N1", "text": "a name"}]
    if word == "other-head":
        v["head"] = "0" * 40
    if word == "merge":
        open(here + "/merged", "w").write("1")
    if word == "moved":
        open(here + "/moved", "w").write("9" * 40)
    result(v)
PY
           ;;
  *)       init; said "Verdict: accept"; result false 'Verdict: accept\nhead' 3 ;;
esac
