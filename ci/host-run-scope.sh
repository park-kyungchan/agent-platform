#!/usr/bin/env bash
# `ap run` against the real systemd of this host, with a fake `claude` on PATH (no model call).
#   ci/host-run-scope.sh [path of ap]      (default: target/debug/ap)
# Shows: the CLI runs in a scope under agents.slice; after the return no unit and no forked
# sleeper remain; `timeout_seconds: 5` gives state `timeout`; after `kill -9` of `ap run` the unit
# still ends at its own bound and the record on disk says `running` with the session id; no
# HERDR_* or inherited CLAUDE_* variable reaches the CLI; a fresh request while the unit of a killed
# runner lives is refused (by existence; its resume by the live unit); a stage bound to a unit stops when
# that unit is stopped or its main process is killed; the policy file of the host is replaced through
# AP_POLICY_FILE by one of this script's directory that allows its model and its account, and a
# stage on another model is refused; ONT_ROLE and ONT_AGENT (and with git_author the two author
# variables) are in the stage's environment; a reviewer stage with `limits` under the real `bwrap`
# cannot write its cwd, nor the scratch directory or the account of another stage with `limits`
# running at the same time (directly or through that stage's /proc/<pid>/root), but writes its
# own scratch directory, which is gone after the return; an implementer whose `limits` name a
# `write` directory, an `over` pair, a `read_only` file and `show` writes that directory and, through
# the target, the source, but neither its cwd nor the `read_only` file. Skipped, saying so, where `systemctl` is absent or this caller cannot start a
# transient unit in the system manager.
set -uo pipefail
ap=$(realpath -- "${1:-${CARGO_TARGET_DIR:-$(cd "$(dirname "$0")/.." && pwd -P)/target}/debug/ap}")
if ! command -v systemctl > /dev/null || [ ! -d /run/systemd/system ]; then
  echo "host-run-scope: skipped (no systemctl or systemd is not init)"; exit 0
fi
if ! systemd-run --scope --quiet --collect --unit="ap-run-probe-$$" --slice=agents.slice true 2> /dev/null; then
  echo "host-run-scope: skipped (this caller cannot start a transient unit in the system manager)"; exit 0
fi
t=$(mktemp -d "${TMPDIR:-/tmp}/ap-run-XXXXXX") || { echo "host-run-scope: cannot make a directory under ${TMPDIR:-/tmp}" >&2; exit 1; }
chmod 0755 "$t"
cleanup() { for p in "$t"/bin/*.pid; do [ -e "$p" ] && kill -KILL "$(cat "$p")" 2> /dev/null; done; rm -rf -- "$t"
  systemctl stop "ap-gate-bind-$$-stop.service" "ap-gate-bind-$$-kill.service" 2> /dev/null; }
trap cleanup EXIT
mkdir -p "$t/bin" "$t/acct" "$t/acctB" "$t/cwd"
cat > "$t/bin/claude" <<'FAKE'
#!/usr/bin/env bash
here=$(cd "$(dirname "$0")" && pwd)
env > "$here/env"; cat /proc/self/cgroup > "$here/cgroup"; echo "$PPID" > "$here/parent"; cat > /dev/null
sid=; prev=; for a in "$@"; do case "$prev" in --session-id|--resume) sid=$a ;; esac; prev=$a; done
printf '{"type":"system","subtype":"init","session_id":"%s","mcp_servers":[]}\n' "$sid"
# A shell forked into the background that outlives the answer, as a real session's can.
/bin/bash -c 'echo $$ > "'"$here"'/sleeper.pid"; sleep 600; :' &
# Under limits $here is read-only: the fake reports its writes in its answer. On the account
# `acctB` it is the other stage: it says it is ready in its account and answers when told to.
if [ "$(cat "$here/mode")" = limits ] && [ "$(basename "$CLAUDE_CONFIG_DIR")" = acctB ]; then
  : > "$CLAUDE_CONFIG_DIR/ready"
  for _ in $(seq 300); do [ -e "$CLAUDE_CONFIG_DIR/go" ] && break; sleep 0.2; done
  printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"peer done","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$sid"
elif [ "$(cat "$here/mode")" = limits ]; then
  w() { if e=$( (: > "$1") 2>&1 ); then echo "$2 written"; else echo "$2 refused (${e##*: })"; fi; }
  # The other stage's files, directly and through the root of each of its processes (host pids).
  top=${here%/bin}; peer=
  for p in "$top/scratchB/from-a" "$top/acctB/from-a"; do peer="$peer $(w "$p" peer)"; done
  for pid in $(cat "$here/peer.pids"); do
    for p in "$top/scratchB/from-a" "$top/acctB/from-a"; do peer="$peer $(w "/proc/$pid/root$p" peer)"; done
  done
  case "$peer" in *written*) peer="peer written:$peer" ;; *) peer="peer refused" ;; esac
  said="$(w "$PWD/x" cwd); $(w "$TMPDIR/x" scratch); $(w "/proc/1/root$here/x" root); $peer"
  printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"%s","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$said" "$sid"
elif [ "$(cat "$here/mode")" = named ]; then
  # An implementer with the paths its request names: it reports each write and exits.
  w() { if e=$( (: > "$1") 2>&1 ); then echo "$2 written"; else echo "$2 refused (${e##*: })"; fi; }
  c() { if e=$("$@" 2>&1); then echo "written"; else echo "refused (${e##*: })"; fi; }
  top=${here%/bin}
  said="$(w "$top/wdir/x" write); $(w "$top/dst/y" over); $(w "$top/wdir/keep" keep); keep removal $(c rm -f "$top/wdir/keep"); keep rename $(c mv "$top/wdir/keep" "$top/wdir/moved"); $(w "$PWD/n" cwd); $(w "/proc/1/root$PWD/m" root)"
  printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"%s","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$said" "$sid"
  exit 0
fi
if [ "$(cat "$here/mode")" = answer ]; then
  printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"ready","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$sid"
fi
wait
FAKE
chmod 0755 "$t/bin/claude"
printf 'Print the word ready and stop.\n' > "$t/prompt.md"
# The host's policy (specs/ap/policy.md) is the file of this directory, never /etc/agent-platform/policy.json.
printf '{"models":{"allowed":["haiku"],"never":[]},"accounts":{"claude":["%s","%s"]}}' "$t/acct" "$t/acctB" > "$t/policy.json"
req() { printf '{"role":"reviewer","account":"%s","model":"haiku","effort":"low","permission_mode":"auto","cwd":"%s","prompt":"%s","out":"%s"%s}' \
  "$t/acct" "$t/cwd" "$t/prompt.md" "$t/$1" "${2:-}"; }
field() { python3 -I -c 'import json,sys; v=json.load(open(sys.argv[1])); print(v.get(sys.argv[2]))' "$1" "$2"; }
alive() { [ -e "/proc/$1" ] && [ "$(awk '{print $3}' "/proc/$1/stat" 2> /dev/null)" != Z ]; }
fails=0; ok() { echo "ok   $1"; }; bad() { echo "FAIL $1"; fails=$((fails+1)); }
apr() { env -i PATH="$t/bin:$PATH" HOME="${HOME:-/root}" LANG=C.UTF-8 HERDR_ENV=1 HERDR_PANE_ID=w0:p0 \
  CLAUDE_CODE_ENTRYPOINT=cli CLAUDE_CONFIG_DIR=/root/.claude AP_POLICY_FILE="$t/policy.json" "$ap" run; }

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

# The record names the policy file this run was admitted under; a model the file does not allow is
# refused before any effect.
[ "$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["policy"]["path"])' "$t/a.md.run.json")" = "$t/policy.json" ] \
  && ok "the record names the policy file of AP_POLICY_FILE" || bad "policy member: $(field "$t/a.md.run.json" policy)"
req p.md | sed 's/"haiku"/"opus"/' | apr > "$t/p.out" 2> "$t/p.err"; rc=$?
{ [ "$rc" -eq 2 ] && grep -q '"code":"model_not_allowed"' "$t/p.err" && [ ! -e "$t/p.md.run.json" ]; } \
  && ok "a model the policy does not allow is refused model_not_allowed, no file written" || bad "policy refusal: rc=$rc $(cat "$t/p.err")"

# The role reaches the engine as the actor's claim: ONT_ROLE and ONT_AGENT are in every stage's
# environment (never the caller's), and only with git_author do the two author variables follow.
# No unit or intent is given here: this script runs on the host and touches no store.
echo answer > "$t/bin/mode"
req f.md ',"ont_role":"verifier","git_author":true' | ONT_ROLE=lead ONT_AGENT=caller GIT_AUTHOR_NAME=caller apr > "$t/f.out" 2> "$t/f.err"; rc=$?
{ [ "$rc" -eq 0 ] && [ "$(grep -c '^ONT_' "$t/bin/env")" -eq 2 ] && grep -qx 'ONT_ROLE=verifier' "$t/bin/env" \
  && grep -qx 'ONT_AGENT=claude:haiku' "$t/bin/env" && grep -qx 'GIT_AUTHOR_NAME=verifier haiku' "$t/bin/env" \
  && grep -qx 'GIT_AUTHOR_EMAIL=verifier.haiku@agents.invalid' "$t/bin/env"; } \
  && ok "ONT_ROLE=verifier, ONT_AGENT=claude:haiku and, with git_author, the two author variables reached the CLI" \
  || bad "actor and author: rc=$rc $(grep -E '^(ONT|GIT)_' "$t/bin/env") $(cat "$t/f.err")"
req g.md | apr > "$t/g.out" 2> "$t/g.err"; rc=$?
{ [ "$rc" -eq 0 ] && grep -qx 'ONT_ROLE=reviewer' "$t/bin/env" && ! grep -q '^GIT_' "$t/bin/env"; } \
  && ok "without git_author no variable of Git's reached the CLI; ONT_ROLE=reviewer" || bad "no author: rc=$rc $(grep -E '^(ONT|GIT)_' "$t/bin/env")"

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

# 5. limits (specs/ap/run-limits.md): the real bwrap between the scope and the CLI. Stage B (account
#    acctB) waits while stage A tries to write B's scratch directory and account, directly and
#    through /proc/<pid>/root of every process of B's scope (each stage has its own pid namespace).
#    Under bwrap the CLI is a grandchild of ap run, so the check that it is a child of ap run is not
#    made here.
if ! /usr/bin/bwrap --ro-bind / / true 2> /dev/null; then
  echo "host-run-scope: limits skipped (/usr/bin/bwrap --ro-bind / / true fails here)"
else
  echo limits > "$t/bin/mode"
  printf '{"role":"reviewer","account":"%s","model":"haiku","effort":"low","permission_mode":"auto","cwd":"%s","prompt":"%s","out":"%s","timeout_seconds":90,"limits":{"scratch":"%s"}}' \
    "$t/acctB" "$t/cwd" "$t/prompt.md" "$t/lb.md" "$t/scratchB" | apr > "$t/lb.out" 2> "$t/lb.err" & bpid=$!
  for _ in $(seq 100); do [ -e "$t/acctB/ready" ] && break; sleep 0.2; done
  bunit=$(field "$t/lb.md.run.json" unit)
  cat "/sys/fs/cgroup$(systemctl show -p ControlGroup --value "$bunit")/cgroup.procs" > "$t/bin/peer.pids" 2> /dev/null
  [ -e "$t/acctB/ready" ] && [ -s "$t/bin/peer.pids" ] && [ -d "$t/scratchB" ] \
    && ok "limits: stage B runs in $bunit with $(wc -l < "$t/bin/peer.pids") processes" || bad "limits: stage B did not start: $(tail -c 400 "$t/lb.err")"
  req l.md ",\"limits\":{\"scratch\":\"$t/scratch\"}" | apr > "$t/l.out" 2> "$t/l.err"; rc=$?
  ans=$(cat "$t/l.md" 2> /dev/null)
  { [ "$rc" -eq 0 ] && [ "$(field "$t/l.md.run.json" state)" = ok ] && [ ! -e "$t/cwd/x" ] && [ ! -e "$t/bin/x" ] \
    && [[ "$ans" == "cwd refused (Read-only file system); scratch written; root refused ("*"); peer refused" ]]; } \
    && ok "limits: the cwd, /proc/1/root and stage B's files refused, the scratch directory written: $ans" \
    || bad "limits: rc=$rc $ans $(tail -c 400 "$t/l.err")"
  { [ ! -e "$t/scratchB/from-a" ] && [ ! -e "$t/acctB/from-a" ] && systemctl is-active --quiet "$bunit"; } \
    && ok "limits: nothing of stage A reached stage B's scratch directory or account while B ran" \
    || bad "limits: stage B's files: $(ls -A "$t/scratchB" "$t/acctB" 2> /dev/null | tr '\n' ' ')"
  python3 -I -c 'import json,sys; t=sys.argv[1]; l=json.load(open(t+"/l.md.run.json"))["limits"]
hidden = l.pop("hidden", None)
sys.exit(l != {"scratch": t+"/scratch", "writable": [t+"/acct", t+"/scratch"], "bwrap": "/usr/bin/bwrap"}
         or not isinstance(hidden, list))' "$t" \
    && ok "limits: the record names the scratch directory, the two writable paths and the hidden list" || bad "limits member: $(field "$t/l.md.run.json" limits)"
  [ ! -e "$t/scratch" ] && ok "limits: the scratch directory is gone after the return" || bad "limits: $t/scratch remains"
  : > "$t/acctB/go"; wait "$bpid"; rc=$?
  { [ "$rc" -eq 0 ] && [ "$(field "$t/lb.md.run.json" state)" = ok ] && [ ! -e "$t/scratchB" ] && ! systemctl is-active --quiet "$bunit"; } \
    && ok "limits: stage B answered, its scope ended and its scratch directory is gone" || bad "limits: stage B: rc=$rc $(tail -c 400 "$t/lb.err")"
  # An implementer with the four members of limits (specs/ap/run-limits.md R1, R5, R7, R11).
  mkdir -p "$t/wdir" "$t/src" "$t/dst"; echo kept > "$t/wdir/keep"; echo named > "$t/bin/mode"
  req n.md ",\"limits\":{\"scratch\":\"$t/scratch2\",\"write\":[\"$t/wdir\"],\"over\":[[\"$t/src\",\"$t/dst\"]],\"read_only\":[\"$t/wdir/keep\"],\"show\":[\"~/.config/gh\"]}" \
    | sed 's/"role":"reviewer"/"role":"implementer"/' | apr > "$t/n.out" 2> "$t/n.err"; rc=$?
  ans=$(cat "$t/n.md" 2> /dev/null)
  { [ "$rc" -eq 0 ] && [ "$(field "$t/n.md.run.json" state)" = ok ] && [ -e "$t/wdir/x" ] && [ -e "$t/src/y" ] && [ ! -e "$t/dst/y" ] \
    && [ "$(cat "$t/wdir/keep")" = kept ] && [ ! -e "$t/wdir/moved" ] && [ ! -e "$t/cwd/n" ] && [ ! -e "$t/cwd/m" ] \
    && [[ "$ans" == "write written; over written; keep refused ("*"); keep removal refused ("*"); keep rename refused ("*"); cwd refused (Read-only file system); root refused ("*")" ]]; } \
    && ok "limits: an implementer wrote its write directory and the over source, not the read_only file, its cwd or /proc/1/root: $ans" \
    || bad "limits: named paths: rc=$rc $ans $(tail -c 400 "$t/n.err")"
  python3 -I -c 'import json,sys; t=sys.argv[1]; l=json.load(open(t+"/n.md.run.json"))["limits"]
hidden = l.pop("hidden", None)
sys.exit(l != {"scratch": t+"/scratch2", "writable": [t+"/acct", t+"/scratch2", t+"/wdir", t+"/dst"], "bwrap": "/usr/bin/bwrap",
               "over": [[t+"/src", t+"/dst"]], "read_only": [t+"/wdir/keep"], "show": ["~/.config/gh"]}
         or not isinstance(hidden, list) or any(p.endswith("/.config/gh") for p in hidden))' "$t" \
    && ok "limits: the implementer's record names what it writes, the members as given, and ~/.config/gh shown" || bad "limits member of the implementer: $(field "$t/n.md.run.json" limits)"
fi

[ "$fails" -eq 0 ] && echo "host-run-scope: PASS" || { echo "host-run-scope: $fails FAILED"; exit 1; }
