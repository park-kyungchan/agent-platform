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
rejected() { printf '{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour"},"session_id":"%s"}\n' "$sid"; }
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
  *)       init; said "Verdict: accept"; result false 'Verdict: accept\nhead' 3 ;;
esac
