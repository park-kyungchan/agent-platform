#!/usr/bin/env bash
# `ap run` against the real systemd of this host, with a fake `claude` on PATH (no model call).
#   ci/host-run-scope.sh [path of ap]      (default: target/debug/ap)
# Shows: the CLI runs in a scope under agents.slice; after the return no unit and no forked
# sleeper remain; `timeout_seconds: 5` gives state `timeout`; after `kill -9` of `ap run` the unit
# still ends at its own bound and the record on disk says `running` with the session id; no
# HERDR_* or inherited CLAUDE_* variable reaches the CLI; a fresh request while the unit of a killed
# runner lives is refused (by existence; its resume by the live unit); a stage bound to a unit stops when
# that unit is stopped or its main process is killed. Skipped, saying so, where `systemctl` is
# absent or this caller cannot start a transient unit in the system manager.
set -uo pipefail
ap=$(realpath -- "${1:-${CARGO_TARGET_DIR:-$(cd "$(dirname "$0")/.." && pwd -P)/target}/debug/ap}")
if ! command -v systemctl > /dev/null || [ ! -d /run/systemd/system ]; then
  echo "host-run-scope: skipped (no systemctl or systemd is not init)"; exit 0
fi
if ! systemd-run --scope --quiet --collect --unit="ap-run-probe-$$" --slice=agents.slice true 2> /dev/null; then
  echo "host-run-scope: skipped (this caller cannot start a transient unit in the system manager)"; exit 0
fi
t=$(mktemp -d /tmp/ap-run-XXXXXX); chmod 0755 "$t"
cleanup() { for p in "$t"/bin/*.pid; do [ -e "$p" ] && kill -KILL "$(cat "$p")" 2> /dev/null; done; rm -rf -- "$t"
  systemctl stop "ap-gate-bind-$$-stop.service" "ap-gate-bind-$$-kill.service" 2> /dev/null; }
trap cleanup EXIT
mkdir -p "$t/bin" "$t/acct" "$t/cwd"
cat > "$t/bin/claude" <<'FAKE'
#!/usr/bin/env bash
here=$(cd "$(dirname "$0")" && pwd)
env > "$here/env"; cat /proc/self/cgroup > "$here/cgroup"; echo "$PPID" > "$here/parent"; cat > /dev/null
sid=; prev=; for a in "$@"; do case "$prev" in --session-id|--resume) sid=$a ;; esac; prev=$a; done
printf '{"type":"system","subtype":"init","session_id":"%s","mcp_servers":[]}\n' "$sid"
# A shell forked into the background that outlives the answer, as a real session's can.
/bin/bash -c 'echo $$ > "'"$here"'/sleeper.pid"; sleep 600; :' &
if [ "$(cat "$here/mode")" = answer ]; then
  printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"ready","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$sid"
fi
wait
FAKE
chmod 0755 "$t/bin/claude"
printf 'Print the word ready and stop.\n' > "$t/prompt.md"
req() { printf '{"role":"reviewer","account":"%s","model":"haiku","effort":"low","permission_mode":"auto","cwd":"%s","prompt":"%s","out":"%s"%s}' \
  "$t/acct" "$t/cwd" "$t/prompt.md" "$t/$1" "${2:-}"; }
field() { python3 -I -c 'import json,sys; v=json.load(open(sys.argv[1])); print(v.get(sys.argv[2]))' "$1" "$2"; }
alive() { [ -e "/proc/$1" ] && [ "$(awk '{print $3}' "/proc/$1/stat" 2> /dev/null)" != Z ]; }
fails=0; ok() { echo "ok   $1"; }; bad() { echo "FAIL $1"; fails=$((fails+1)); }
apr() { env -i PATH="$t/bin:$PATH" HOME="${HOME:-/root}" LANG=C.UTF-8 HERDR_ENV=1 HERDR_PANE_ID=w0:p0 \
  CLAUDE_CODE_ENTRYPOINT=cli CLAUDE_CONFIG_DIR=/root/.claude "$ap" run; }

# 1. An answer with a forked sleeper left behind: the run ends at the answer, in agents.slice, and
#    nothing of it remains.
echo answer > "$t/bin/mode"
req a.md | apr > "$t/a.out" 2> "$t/a.err"; rc=$?
unit=$(field "$t/a.md.run.json" unit)
echo "     record: $(tr -d '\n ' < "$t/a.md.run.json" | cut -c1-400)"
{ [ "$rc" -eq 0 ] && [ "$(field "$t/a.md.run.json" state)" = ok ] && [ "$(cat "$t/a.md")" = ready ]; } \
  && ok "answered run: state ok, answer saved" || bad "answered run: rc=$rc $(cat "$t/a.err")"
grep -q "^0::/agents.slice/$unit\$" "$t/bin/cgroup" && ok "the CLI ran in $unit under agents.slice" \
  || bad "cgroup: $(cat "$t/bin/cgroup")"
{ ! systemctl is-active --quiet "$unit" && [ -z "$(systemctl list-units --all --no-legend "$unit")" ]; } \
  && ok "no unit remains after the return" || bad "unit remains: $(systemctl list-units --all --no-legend "$unit")"
sleeper=$(cat "$t/bin/sleeper.pid"); ! alive "$sleeper" && ok "the forked sleeper $sleeper is gone" || bad "sleeper $sleeper alive"
{ ! grep -q '^HERDR_' "$t/bin/env" && [ "$(grep '^CLAUDE_' "$t/bin/env")" = "CLAUDE_CONFIG_DIR=$t/acct" ]; } \
  && ok "no HERDR_* and no inherited CLAUDE_* reached the CLI" || bad "environment: $(grep -E '^(HERDR|CLAUDE)_' "$t/bin/env")"

# 2. No answer: timeout_seconds 5 gives state timeout, and nothing remains.
echo silent > "$t/bin/mode"; rm -f "$t/bin/sleeper.pid"; s0=$(date +%s)
req b.md ',"timeout_seconds":5' | apr > "$t/b.out" 2> "$t/b.err"; rc=$?; took=$(( $(date +%s) - s0 ))
unit=$(field "$t/b.md.run.json" unit)
{ [ "$rc" -eq 3 ] && [ "$(field "$t/b.md.run.json" state)" = timeout ] && [ "$took" -ge 5 ] && [ "$took" -lt 15 ] && [ ! -e "$t/b.md" ]; } \
  && ok "timeout_seconds 5: state timeout after ${took}s" || bad "timeout: rc=$rc took=${took}s state=$(field "$t/b.md.run.json" state)"
{ ! systemctl is-active --quiet "$unit" && ! alive "$(cat "$t/bin/sleeper.pid")"; } && ok "nothing of the timed-out run remains" || bad "timed-out run remains"

# 3. kill -9 of ap run: the unit still ends at its own bound (RuntimeMaxSec = 5 + 10 s); the record
#    on disk says running with the session id.
rm -f "$t/bin/sleeper.pid" "$t/c.md.run.json"; s0=$(date +%s)
req c.md ',"timeout_seconds":5' | apr > "$t/c.out" 2> "$t/c.err" & pid=$!
rm -f "$t/bin/parent"
for _ in $(seq 50); do [ -s "$t/bin/sleeper.pid" ] && [ -s "$t/bin/parent" ] && break; sleep 0.2; done
unit=$(field "$t/c.md.run.json" unit); apid=$(cat "$t/bin/parent")
# systemd-run --scope execs the CLI: the CLI's parent is ap run itself.
[ "$(tr '\0' ' ' < "/proc/$apid/cmdline")" = "$ap run " ] && ok "the CLI is a child of ap run ($apid)" || bad "parent $apid: $(tr '\0' ' ' < "/proc/$apid/cmdline")"
[ "$(systemctl show -p Slice --value "$unit")" = agents.slice ] && ok "while it runs, $unit is in agents.slice" || bad "slice of $unit"
kill -KILL "$apid"; wait "$pid" 2> /dev/null
{ [ "$(field "$t/c.md.run.json" state)" = running ] && [ "$(field "$t/c.md.run.json" session | wc -c)" -eq 37 ]; } \
  && ok "after kill -9 the record says running with session $(field "$t/c.md.run.json" session)" || bad "record after kill: $(cat "$t/c.md.run.json")"
# A second fresh request on the same out while that unit lives is refused; the record is unchanged.
before=$(sha256sum < "$t/c.md.run.json")
req c.md ',"timeout_seconds":5' | apr > "$t/c2.out" 2> "$t/c2.err"; rc=$?
{ [ "$rc" -eq 2 ] && grep -q '"code":"out_exists"' "$t/c2.err" && [ "$(sha256sum < "$t/c.md.run.json")" = "$before" ] \
  && systemctl is-active --quiet "$unit"; } \
  && ok "a second request on the same out while $unit lives is refused out_exists, record unchanged" || bad "second request: rc=$rc $(cat "$t/c2.err")"
# A resume that names that record is refused while its unit lives.
printf '{"role":"reviewer","account":"%s","model":"haiku","effort":"low","permission_mode":"auto","cwd":"%s","prompt":"%s","out":"%s","resume":{"record":"%s"}}' \
  "$t/acct" "$t/cwd" "$t/prompt.md" "$t/c3.md" "$t/c.md.run.json" | apr > "$t/c3.out" 2> "$t/c3.err"; rc=$?
{ [ "$rc" -eq 2 ] && grep -q '"code":"run_still_active"' "$t/c3.err" && [ ! -e "$t/c3.md.run.json" ]; } \
  && ok "a resume of it while $unit lives is refused run_still_active, no file written" || bad "resume while alive: rc=$rc $(cat "$t/c3.err")"
for _ in $(seq 60); do systemctl is-active --quiet "$unit" || break; sleep 0.5; done; took=$(( $(date +%s) - s0 ))
{ ! systemctl is-active --quiet "$unit" && [ "$took" -ge 14 ] && [ "$took" -le 25 ] && ! alive "$(cat "$t/bin/sleeper.pid")"; } \
  && ok "the unit ended at its own bound after ${took}s, sleeper gone" || bad "bound: active=$(systemctl is-active "$unit") took=${took}s"

# 4. bind: a stage bound to a transient unit stops when that unit is stopped, and when that unit's
#    main process is killed with SIGKILL; the stage's record says failed (no result) at once.
for how in stop kill; do
  b="ap-gate-bind-$$-$how.service"; rm -f "$t/bin/sleeper.pid"
  systemd-run --quiet --collect --unit="$b" --slice=agents.slice sleep 600
  req "d$how.md" ",\"timeout_seconds\":120,\"bind\":\"$b\"" | apr > "$t/d.out" 2> "$t/d.err" & pid=$!
  for _ in $(seq 50); do [ -s "$t/bin/sleeper.pid" ] && break; sleep 0.2; done
  unit=$(field "$t/d$how.md.run.json" unit); s0=$(date +%s)
  [ "$(systemctl show -p BindsTo --value "$unit")" = "$b" ] && ok "$unit is bound to $b" || bad "BindsTo of $unit"
  if [ "$how" = stop ]; then systemctl stop "$b"; else kill -KILL "$(systemctl show -p MainPID --value "$b")"; fi
  wait "$pid"; rc=$?; took=$(( $(date +%s) - s0 ))
  { [ "$rc" -eq 3 ] && [ "$(field "$t/d$how.md.run.json" state)" = failed ] && [ "$took" -lt 15 ] \
    && ! systemctl is-active --quiet "$unit" && ! alive "$(cat "$t/bin/sleeper.pid")"; } \
    && ok "bound unit ended by $how: the stage stopped after ${took}s, nothing remains" \
    || bad "bind $how: rc=$rc took=${took}s $(cat "$t/d.err") $(field "$t/d$how.md.run.json" state)"
  systemctl stop "$b" 2> /dev/null
done

[ "$fails" -eq 0 ] && echo "host-run-scope: PASS" || { echo "host-run-scope: $fails FAILED"; exit 1; }
