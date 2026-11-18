#!/usr/bin/env bash
# A fake `claude` for the tests of `limits` in tests/run.rs (the form of ../fake-claude.sh). Each
# call writes `env.<n>` and what it finds of its scratch directory (`$TMPDIR` without `/tmp`) to
# `scratch.<n>` (its mode, then its entries), leaves `mark.<n>` in it and runs the file `then`
# when there is one. It is cut by a usage limit on an account whose `cut-<account basename>`
# exists, and answers on any other.
here=$(cd "$(dirname "$0")" && pwd)
n=$(( $(cat "$here/count" 2>/dev/null || echo 0) + 1 )); echo "$n" > "$here/count"
printf '%s|%s\n' "$CLAUDE_CONFIG_DIR" "$*" >> "$here/calls.log"
env > "$here/env.$n"
cat > /dev/null
s=${TMPDIR%/tmp}
{ stat -c '%a' "$s"; ls -A "$s"; } > "$here/scratch.$n"
: > "$s/mark.$n"
[ -f "$here/then" ] && S=$s bash "$here/then"
sid=; prev=; for a in "$@"; do case "$prev" in --session-id|--resume) sid=$a ;; esac; prev=$a; done
printf '{"type":"system","subtype":"init","session_id":"%s","mcp_servers":[]}\n' "$sid"
if [ -e "$here/cut-$(basename "$CLAUDE_CONFIG_DIR")" ]; then
  printf '{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"true"}}]},"session_id":"%s"}\n' "$sid"
  printf '{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour"},"session_id":"%s"}\n' "$sid"
  printf '{"type":"result","subtype":"error_during_execution","is_error":true,"num_turns":1,"result":"You have hit your 5-hour limit","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$sid"
else
  printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"Verdict: accept","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$sid"
fi
