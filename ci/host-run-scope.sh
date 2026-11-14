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
  systemctl stop "ap-gate-bind-$$-stop.service" "ap-gate-bind-$$-kill.service" ${uunit:+"$uunit"} 2> /dev/null; }
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

# 5. ap unit: a real chain unit under agents.slice whose stages are a fake `claude` and whose GitHub
#    is a fake `gh`. Stopped while its implementer runs: nothing of the chain or of its stage remains
#    and the record says `stopped`. A killed caller (`ap unit wait`) leaves a chain that reaches its end.
mkdir -p "$t/repo" "$t/uout" "$t/uout2" "$t/acct2"
git -C "$t/repo" init -q -b ap/probe && git -C "$t/repo" -c user.email=t@example.invalid -c user.name=t commit -q --allow-empty -m base
echo "$t/repo" > "$t/bin/repo"
cat > "$t/bin/claude" <<'FAKE'
#!/usr/bin/env bash
here=$(cd "$(dirname "$0")" && pwd); cat > /dev/null
sid=; prev=; schema=; for a in "$@"; do case "$prev" in --session-id|--resume) sid=$a ;; --json-schema) schema=$a ;; esac; prev=$a; done
printf '{"type":"system","subtype":"init","session_id":"%s"}\n' "$sid"
res() { printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"done","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]%s}\n' "$sid" "$1"; }
case "$schema" in
  *verdict*) res ",\"structured_output\":{\"verdict\":\"accept\",\"head\":\"$(git rev-parse HEAD)\",\"blocking\":[],\"notes\":[]}" ;;
  *) /bin/bash -c 'echo $$ > "'"$here"'/stage.pid"; sleep "$(cat "'"$here"'/umode")"; :' & wait $!; res "" ;;
esac
FAKE
cat > "$t/bin/gh" <<'FAKE'
#!/usr/bin/python3 -I
import json, subprocess, sys, os
a = sys.argv[1:]
if a[:1] in (["--version"], ["pr"]):
    sys.exit(0)
repo = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "repo")).read().strip()
head = subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
q = [x for x in a if x.startswith("query=")][-1]
pr = {"number": 7, "state": "OPEN", "headRefOid": head, "baseRefName": "main", "headRepository": {"nameWithOwner": "o/r"}, "isDraft": False}
if "reviewThreads" in q:
    pr = {"reviewThreads": {"pageInfo": {"hasNextPage": False}, "nodes": []}}
elif "statusCheckRollup" in q:
    marker = "codex-review-state:v1 " + json.dumps({"code_review": {"commit_sha": head, "status": "completed"}})
    pr["commits"] = {"nodes": [{"commit": {"oid": head, "statusCheckRollup": {"contexts": {"pageInfo": {"hasNextPage": False},
        "nodes": [{"__typename": "CheckRun", "name": "rust", "status": "COMPLETED", "conclusion": "SUCCESS", "isRequired": True}]}}}}]}
    pr["comments"] = {"nodes": [{"author": {"__typename": "Bot", "login": "chatgpt-codex-connector"}, "body": marker}]}
elif "pullRequests(" in q:
    print(json.dumps({"data": {"repository": {"pullRequests": {"nodes": [pr]}}}})); sys.exit(0)
print(json.dumps({"data": {"repository": {"pullRequest": pr}}}))
FAKE
chmod 0755 "$t/bin/claude" "$t/bin/gh"
ureq() { printf '{"repository":"o/r","cwd":"%s","out":"%s","implementer":{"account":"%s","model":"haiku","effort":"low","prompt":"%s"},"reviewer":{"account":"%s","model":"haiku","effort":"low","prompt":"%s"},"bound_minutes":10}' \
  "$t/repo" "$1" "$t/acct" "$t/prompt.md" "$t/acct2" "$t/prompt.md"; }
apu() { env -i PATH="$t/bin:$PATH" HOME="${HOME:-/root}" LANG=C.UTF-8 "$ap" unit "$@"; }
echo 600 > "$t/bin/umode"; rm -f "$t/bin/stage.pid"
ureq "$t/uout" | apu > "$t/u.out" 2> "$t/u.err"; rc=$?
uunit=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["unit"])' "$t/u.out" 2> /dev/null)
{ [ "$rc" -eq 0 ] && [ "$(systemctl show -p Slice --value "$uunit")" = agents.slice ]; } \
  && ok "ap unit started $uunit under agents.slice and returned" || bad "ap unit: rc=$rc $(cat "$t/u.err")"
for _ in $(seq 100); do [ -s "$t/bin/stage.pid" ] && break; sleep 0.2; done
sunit=$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["unit"])' "$t/uout/1-implementer.md.run.json" 2> /dev/null)
[ "$(systemctl show -p BindsTo --value "$sunit")" = "$uunit" ] && ok "its implementer $sunit is bound to the chain's unit" || bad "BindsTo of $sunit"
ureq "$t/uout2" | apu > /dev/null 2> "$t/u2.err"; rc=$?
{ [ "$rc" -eq 2 ] && grep -q chain_active "$t/u2.err"; } && ok "a second chain on the worktree is refused chain_active" || bad "second chain: rc=$rc $(cat "$t/u2.err")"
systemctl stop "$uunit"
{ ! systemctl is-active --quiet "$uunit" && ! systemctl is-active --quiet "$sunit" && ! alive "$(cat "$t/bin/stage.pid")" \
  && [ -z "$(systemctl list-units --all --no-legend "$uunit" "$sunit")" ]; } \
  && ok "the stopped chain: no unit and no process of it or of its stage remains" || bad "after stop: $(systemctl list-units --all --no-legend "$uunit" "$sunit")"
[ "$(field "$t/uout/unit.json" state)" = stopped ] && ok "its record says stopped" || bad "record: $(field "$t/uout/unit.json" state)"
echo 3 > "$t/bin/umode"; rm -f "$t/bin/stage.pid"
ureq "$t/uout2" | apu > "$t/u.out" 2> "$t/u.err"
printf '{"record":"%s","timeout_seconds":120}' "$t/uout2/unit.json" | apu wait > /dev/null & wpid=$!
for _ in $(seq 100); do [ -s "$t/bin/stage.pid" ] && break; sleep 0.2; done
kill -KILL "$wpid"; wait "$wpid" 2> /dev/null
printf '{"record":"%s","timeout_seconds":120}' "$t/uout2/unit.json" | apu wait > "$t/w.out"; rc=$?
{ [ "$rc" -eq 0 ] && [ "$(field "$t/uout2/unit.json" state)" = accepted ] && ! systemctl is-active --quiet "$uunit"; } \
  && ok "after the waiter was killed the chain reached its end: accepted" || bad "killed caller: rc=$rc $(cut -c1-300 "$t/w.out")"

[ "$fails" -eq 0 ] && echo "host-run-scope: PASS" || { echo "host-run-scope: $fails FAILED"; exit 1; }
