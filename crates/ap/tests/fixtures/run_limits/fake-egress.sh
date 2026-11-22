#!/usr/bin/env bash
# A fake `claude` for the tests of the stage's proxy in tests/run.rs (specs/ap/run-limits.md R17
# to R20; the form of ../fake-claude.sh). Each call writes `env.<n>`, sends each line of the file
# `asks` to the proxy `$HTTPS_PROXY` names, each on a connection of its own, and answers with what
# it read back, one exchange each, separated by `;`, in the order the exchanges ended: the status
# line, and after a 200 ` | ` and the marker of the test's listener, and the echo of what it sent
# after the empty line. A line is a request line, sent with one header line and the empty line in
# one write, and after a tab the bytes sent behind them; `big` sends 8,193 bytes without an empty
# line; `silent` sends nothing; `bg <line>` runs its exchange in the background; `hold <line>`
# leaves its admitted connection to a reader in a session of its own, which writes `held` when it
# reads the end of the file. It is cut by a usage limit on an account whose `cut-<account
# basename>` exists, and answers on any other.
here=$(cd "$(dirname "$0")" && pwd)
n=$(( $(cat "$here/count" 2>/dev/null || echo 0) + 1 )); echo "$n" > "$here/count"
env > "$here/env.$n"
cat > /dev/null
acct=${CLAUDE_CONFIG_DIR%/homes/*}
hp=${HTTPS_PROXY#http://}; host=${hp%:*}; port=${hp##*:}
said=$here/said.$n; : > "$said"
exchange() {
  local line=$1 hold= req extra= status= blank m e out
  case $line in hold\ *) hold=1; line=${line#hold } ;; esac
  if ! exec 3<>"/dev/tcp/$host/$port"; then echo "no connection" >> "$said"; return; fi
  case $line in
    big) head -c 8193 /dev/zero | tr '\0' a >&3 ;;
    silent) ;;
    *) req=${line%%$'\t'*}; [[ $line == *$'\t'* ]] && extra="${line#*$'\t'}"$'\n'
       printf '%s\r\nHost: test\r\n\r\n%s' "$req" "$extra" >&3 ;;
  esac
  IFS= read -r -t 30 status <&3; status=${status%$'\r'}
  out=$status
  if [[ $status == *" 200 "* ]]; then
    IFS= read -r -t 5 blank <&3
    IFS= read -r -t 5 m <&3; out="$out | $m"
    if [ -n "$extra" ]; then IFS= read -r -t 5 e <&3; out="$out | $e"; fi
    if [ -n "$hold" ]; then
      setsid bash -c 'timeout 60 cat > /dev/null; echo eof > "$1"' _ "$here/held" <&3 > /dev/null 2>&1 &
    fi
  fi
  exec 3<&-
  echo "$out" >> "$said"
}
pids=
while IFS= read -r line; do
  case $line in
    bg\ *) exchange "${line#bg }" & pids="$pids $!" ;;
    *) exchange "$line" ;;
  esac
done < "$here/asks"
[ -n "$pids" ] && wait $pids
result=$(tr '\n' ';' < "$said")
sid=; prev=; for a in "$@"; do case "$prev" in --session-id|--resume) sid=$a ;; esac; prev=$a; done
printf '{"type":"system","subtype":"init","session_id":"%s","mcp_servers":[]}\n' "$sid"
if [ -e "$here/cut-$(basename "$acct")" ]; then
  printf '{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"true"}}]},"session_id":"%s"}\n' "$sid"
  printf '{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour"},"session_id":"%s"}\n' "$sid"
  printf '{"type":"result","subtype":"error_during_execution","is_error":true,"num_turns":1,"result":"You have hit your 5-hour limit","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$sid"
else
  printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"%s","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$result" "$sid"
fi
