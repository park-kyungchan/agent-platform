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
# cannot write its cwd, nor the scratch directory, the home or the account of another stage with
# `limits` running at the same time (directly or through that stage's /proc/<pid>/root), but writes
# its own scratch directory, which is gone after the return, and its home of the launch
# `<account>/homes/<session>`, the one thing it sees of its account: not the account's settings.json
# or skills (directly or through /proc/1/root), and a sign-in without its refreshToken and the
# account's CLAUDE.md, each read-only; the account's sign-in and settings are unchanged after it, no
# derived `*.sign-in` is left, and a resume whose home holds a link at `.credentials.json` launches
# nothing and creates nothing at the link's target; an implementer whose `limits` name a
# `write` directory, an `over` pair, a `read_only` file and `show` writes that directory and, through
# the target, the source, but neither its cwd nor the `read_only` file; a stage with `limits` sees
# only its view (specs/ap/run-limits.md R16): no key, login, store, socket of the host or other
# account, its uid map not the host's, no capability, no user namespace of its own, and its `ap check`
# reads its policy and asks the door; a reviewer stage with `limits` has `lo` alone in a network
# namespace of its own (specs/ap/run-limits.md R17 to R19): no address of the host, no name server,
# no abstract socket of the host, and its proxy refuses a name off its list and an address, each
# with its line in AP_EGRESS_LOG. Each case's request gives as `read_only` what its stage reads
# beyond that view (the fake's directory). Skipped, saying so, where `systemctl` is absent or this
# caller cannot start a transient unit in the system manager.
set -uo pipefail
ap=$(realpath -- "${1:-${CARGO_TARGET_DIR:-$(cd "$(dirname "$0")/.." && pwd -P)/target}/debug/ap}")
if ! command -v systemctl > /dev/null || [ ! -d /run/systemd/system ]; then
  echo "host-run-scope: skipped (no systemctl or systemd is not init)"
  [ -z "${CHECK_SKIPS:-}" ] || echo host-run-scope >> "$CHECK_SKIPS"; exit 0
fi
if ! systemd-run --scope --quiet --collect --unit="ap-run-probe-$$" --slice=agents.slice true 2> /dev/null; then
  echo "host-run-scope: skipped (this caller cannot start a transient unit in the system manager)"
  [ -z "${CHECK_SKIPS:-}" ] || echo host-run-scope >> "$CHECK_SKIPS"; exit 0
fi
t=$(mktemp -d "${TMPDIR:-/tmp}/ap-run-XXXXXX") || { echo "host-run-scope: cannot make a directory under ${TMPDIR:-/tmp}" >&2; exit 1; }
chmod 0755 "$t"
cleanup() { for p in "$t"/bin/*.pid; do [ -e "$p" ] && kill -KILL "$(cat "$p")" 2> /dev/null; done; rm -rf -- "$t"
  systemctl stop "ap-gate-bind-$$-stop.service" "ap-gate-bind-$$-kill.service" 2> /dev/null; }
trap cleanup EXIT
mkdir -p "$t/bin" "$t/acct" "$t/acctB" "$t/cwd"
cat > "$t/bin/claude" <<'FAKE'
#!/usr/bin/env bash
# `claude auth status`, which ap run calls outside the limits near the sign-in's end: nothing else.
[ "${1:-} ${2:-}" = "auth status" ] && exit 0
here=$(cd "$(dirname "$0")" && pwd)
env > "$here/env"; cat /proc/self/cgroup > "$here/cgroup"; echo "$PPID" > "$here/parent"; cat > /dev/null
sid=; prev=; for a in "$@"; do case "$prev" in --session-id|--resume) sid=$a ;; esac; prev=$a; done
printf '{"type":"system","subtype":"init","session_id":"%s","mcp_servers":[]}\n' "$sid"
# A shell forked into the background that outlives the answer, as a real session's can.
/bin/bash -c 'echo $$ > "'"$here"'/sleeper.pid"; sleep 600; :' &
# Under limits $here is read-only: the fake reports its writes in its answer. Under limits its
# CLAUDE_CONFIG_DIR is its home `<account>/homes/<session>`; on the account `acctB` it is the other
# stage: it says it is ready in its home and answers when told to.
acct=${CLAUDE_CONFIG_DIR%/homes/*}
if [ "$(cat "$here/mode")" = limits ] && [ "$(basename "$acct")" = acctB ]; then
  : > "$CLAUDE_CONFIG_DIR/ready"
  for _ in $(seq 300); do [ -e "$CLAUDE_CONFIG_DIR/go" ] && break; sleep 0.2; done
  printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"peer done","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$sid"
elif [ "$(cat "$here/mode")" = limits ]; then
  w() { if e=$( (: > "$1") 2>&1 ); then echo "$2 written"; else echo "$2 refused (${e##*: })"; fi; }
  c() { if "${@:2}" > /dev/null 2>&1; then echo "$1 done"; else echo "$1 refused"; fi; }
  # The other stage's files, directly and through the root of each of its processes (host pids).
  top=${here%/bin}; peer=; bhome=$(cat "$here/peer.home")
  for p in "$top/scratchB/from-a" "$top/acctB/from-a" "$bhome/from-a"; do peer="$peer $(w "$p" peer)"; done
  for pid in $(cat "$here/peer.pids"); do
    for p in "$top/scratchB/from-a" "$top/acctB/from-a" "$bhome/from-a"; do peer="$peer $(w "/proc/$pid/root$p" peer)"; done
  done
  case "$peer" in *written*) peer="peer written:$peer" ;; *) peer="peer refused" ;; esac
  # Its home, and what it sees of its account: the files a Lead's session loads, directly and
  # through /proc/1/root; the sign-in and CLAUDE.md the home holds. No byte of a token is said.
  case "$CLAUDE_CONFIG_DIR" in "$top/acct/homes/"?*) at=home ;; *) at=elsewhere ;; esac
  lead=
  for p in "$acct/settings.json" "$acct/skills/x" "/proc/1/root$acct/settings.json" "/proc/1/root$acct/skills/x"; do lead="$lead $(w "$p" lead)"; done
  case "$lead" in *written*) lead="lead written:$lead" ;; *) lead="lead refused" ;; esac
  si=$CLAUDE_CONFIG_DIR/.credentials.json
  if grep -q accessToken "$si" 2> /dev/null && ! grep -q refreshToken "$si"; then sign="sign-in read"; else sign="sign-in unfit"; fi
  sign="$sign; $(w "$si" sign-in); $(c sign-in-removal rm -f "$si"); $(c sign-in-rename mv "$si" "$CLAUDE_CONFIG_DIR/moved")"
  [ "$(cat "$CLAUDE_CONFIG_DIR/CLAUDE.md" 2> /dev/null)" = "invented instructions" ] && md="md read" || md="md unread"
  own="$(w "$CLAUDE_CONFIG_DIR/x" "$at"); account holds $(ls -A "$acct" | tr '\n' ' '); $lead; $sign; $md; $(w "$CLAUDE_CONFIG_DIR/CLAUDE.md" md)"
  said="$(w "$PWD/x" cwd); $(w "$TMPDIR/x" scratch); $(w "/proc/1/root$here/x" root); $peer; $own"
  printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"%s","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$said" "$sid"
elif [ "$(cat "$here/mode")" = named ]; then
  # An implementer with the paths its request names: it reports each write and exits.
  w() { if e=$( (: > "$1") 2>&1 ); then echo "$2 written"; else echo "$2 refused (${e##*: })"; fi; }
  c() { if e=$("$@" 2>&1); then echo "written"; else echo "refused (${e##*: })"; fi; }
  top=${here%/bin}
  said="$(w "$top/wdir/x" write); $(w "$top/dst/y" over); $(w "$top/wdir/keep" keep); keep removal $(c rm -f "$top/wdir/keep"); keep rename $(c mv "$top/wdir/keep" "$top/wdir/moved"); $(w "$PWD/n" cwd); $(w "/proc/1/root$PWD/m" root)"
  printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"%s","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$said" "$sid"
  exit 0
elif [ "$(cat "$here/mode")" = egress ]; then
  # The stage's network (specs/ap/run-limits.md R17, R18): what it reaches, printing no content:
  # the interfaces of /proc/net/dev, whether its ns/net is the script's, each TCP connect (3 s) that
  # did not fail, the abstract socket, a lookup, and the status lines of two requests to its proxy.
  ifs=$(awk -F: 'NR > 2 {gsub(/[ \t]/, "", $1); print $1}' /proc/net/dev | sort | paste -sd, -)
  [ "$(readlink /proc/self/ns/net)" != "$(cat "$here/netns")" ] && ns=other || ns=same
  tcp() { python3 -I -c 'import socket,sys; socket.create_connection((sys.argv[1], int(sys.argv[2])), timeout=3)' "$1" "$2" > /dev/null 2>&1; }
  reached=
  while read -r a; do tcp "$a" 22 && reached="$reached $a:22"; done < "$here/addrs"
  for a in 127.0.0.53 100.100.100.100; do tcp "$a" 53 && reached="$reached $a:53"; done
  python3 -I -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.settimeout(3); s.connect("\0"+sys.argv[1])' "$(cat "$here/abstract")" > /dev/null 2>&1 \
    && reached="$reached abstract"
  getent hosts github.com > /dev/null 2>&1 && reached="$reached getent"
  ask() { python3 -I -c 'import os,socket,sys
host, port = os.environ["HTTPS_PROXY"].split("//", 1)[1].rsplit(":", 1)
s = socket.create_connection((host, int(port)), timeout=10)
s.sendall(("CONNECT %s HTTP/1.1\r\nHost: %s\r\n\r\n" % (sys.argv[1], sys.argv[1])).encode())
print(s.makefile("rb").readline().decode().strip())' "$1" 2> /dev/null || echo "no answer"; }
  said="interfaces $ifs; namespace $ns; reached:$reached; example.com $(ask example.com:443); 127.0.0.1 $(ask 127.0.0.1:443)"
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
  CLAUDE_CODE_ENTRYPOINT=cli CLAUDE_CONFIG_DIR=/root/.claude AP_POLICY_FILE="$t/policy.json" \
  AP_EGRESS_LOG="$t/egress.jsonl" "$ap" run; }

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
#    acctB) waits while stage A tries to write B's scratch directory, home and account, directly and
#    through /proc/<pid>/root of every process of B's scope (each stage has its own pid namespace).
#    Each account holds what a Lead's session loads (settings.json, skills, CLAUDE.md) and an
#    invented sign-in with a refreshToken, whose end is a day ahead (so no `claude auth status`).
#    Under bwrap the CLI is a grandchild of ap run, so the check that it is a child of ap run is not
#    made here.
if ! /usr/bin/bwrap --ro-bind / / true 2> /dev/null; then
  echo "host-run-scope: limits skipped (/usr/bin/bwrap --ro-bind / / true fails here)"
  [ -z "${CHECK_SKIPS:-}" ] || echo host-run-scope-limits >> "$CHECK_SKIPS"
else
  ends=$(( ($(date +%s) + 86400) * 1000 ))
  for a in acct acctB; do
    mkdir -p "$t/$a/skills"; echo '{"env":{}}' > "$t/$a/settings.json"; echo skill > "$t/$a/skills/x"
    echo "invented instructions" > "$t/$a/CLAUDE.md"
    printf '{"claudeAiOauth":{"accessToken":"invented-access-%s","refreshToken":"invented-refresh-%s","expiresAt":%s,"refreshTokenExpiresAt":%s,"scopes":["user:inference"]}}' \
      "$a" "$a" "$ends" "$ends" > "$t/$a/.credentials.json"
  done
  digests() { sha256sum "$t/acct/.credentials.json" "$t/acct/settings.json" "$t/acctB/.credentials.json" "$t/acctB/settings.json"; }
  before=$(digests)
  echo limits > "$t/bin/mode"
  printf '{"role":"reviewer","account":"%s","model":"haiku","effort":"low","permission_mode":"auto","cwd":"%s","prompt":"%s","out":"%s","timeout_seconds":90,"limits":{"scratch":"%s","read_only":["%s"]}}' \
    "$t/acctB" "$t/cwd" "$t/prompt.md" "$t/lb.md" "$t/scratchB" "$t/bin" | apr > "$t/lb.out" 2> "$t/lb.err" & bpid=$!
  bhome=
  for _ in $(seq 100); do
    [ -s "$t/lb.md.run.json" ] && bhome=$(field "$t/lb.md.run.json" home)
    [ -n "$bhome" ] && [ "$bhome" != None ] && [ -e "$bhome/ready" ] && break; sleep 0.2
  done
  echo "$bhome" > "$t/bin/peer.home"
  bunit=$(field "$t/lb.md.run.json" unit)
  cat "/sys/fs/cgroup$(systemctl show -p ControlGroup --value "$bunit")/cgroup.procs" > "$t/bin/peer.pids" 2> /dev/null
  [ -e "$bhome/ready" ] && [ -s "$t/bin/peer.pids" ] && [ -d "$t/scratchB" ] && [[ "$bhome" == "$t/acctB/homes/"?* ]] \
    && ok "limits: stage B runs in $bunit with $(wc -l < "$t/bin/peer.pids") processes, in its home below acctB/homes" || bad "limits: stage B did not start: $(tail -c 400 "$t/lb.err")"
  req l.md ",\"limits\":{\"scratch\":\"$t/scratch\",\"read_only\":[\"$t/bin\"]}" | apr > "$t/l.out" 2> "$t/l.err"; rc=$?
  ans=$(cat "$t/l.md" 2> /dev/null); home=$(field "$t/l.md.run.json" home)
  { [ "$rc" -eq 0 ] && [ "$(field "$t/l.md.run.json" state)" = ok ] && [ ! -e "$t/cwd/x" ] && [ ! -e "$t/bin/x" ] \
    && [[ "$ans" == "cwd refused (Read-only file system); scratch written; root refused ("*"); peer refused; "* ]]; } \
    && ok "limits: the cwd, /proc/1/root and stage B's files refused, the scratch directory written: ${ans%%; home*}" \
    || bad "limits: rc=$rc $ans $(tail -c 400 "$t/l.err")"
  { [[ "$ans" == *"; home written; account holds homes ; lead refused; "* ]] && [ -e "$home/x" ] && [[ "$home" == "$t/acct/homes/"?* ]]; } \
    && ok "limits: the stage wrote its home $home, its account holds homes alone, settings.json and skills refused (also through /proc/1/root)" \
    || bad "limits: home and account: ${ans#*peer refused; }"
  { [[ "$ans" == *"; sign-in read; sign-in refused ("*"); sign-in-removal refused; sign-in-rename refused; md read; md refused ("*")" ]]; } \
    && ok "limits: the home's sign-in read without refreshToken and CLAUDE.md read, neither written, removed nor renamed" \
    || bad "limits: sign-in and CLAUDE.md: ${ans#*lead refused; }"
  { [ ! -e "$t/scratchB/from-a" ] && [ ! -e "$t/acctB/from-a" ] && [ ! -e "$bhome/from-a" ] && systemctl is-active --quiet "$bunit"; } \
    && ok "limits: nothing of stage A reached stage B's scratch directory, home or account while B ran" \
    || bad "limits: stage B's files: $(ls -A "$t/scratchB" "$t/acctB" "$bhome" 2> /dev/null | tr '\n' ' ')"
  python3 -I -c 'import json,sys; t=sys.argv[1]; r=json.load(open(t+"/l.md.run.json")); l=r["limits"]
hidden = l.pop("hidden", None); view = l.pop("view", None); l.pop("egress", None)
sys.exit(r.get("home") != t+"/acct/homes/"+r["session"]
         or l != {"scratch": t+"/scratch", "writable": [r["home"], t+"/scratch"], "bwrap": "/usr/bin/bwrap", "read_only": [t+"/bin"]}
         or not isinstance(hidden, list) or not isinstance(view, list) or "/" in view)' "$t" \
    && ok "limits: the record names its home, the scratch directory, the two writable paths, the hidden list and the view" || bad "limits member: $(field "$t/l.md.run.json" home) $(field "$t/l.md.run.json" limits)"
  [ ! -e "$t/scratch" ] && ok "limits: the scratch directory is gone after the return" || bad "limits: $t/scratch remains"
  : > "$bhome/go"; wait "$bpid"; rc=$?
  { [ "$rc" -eq 0 ] && [ "$(field "$t/lb.md.run.json" state)" = ok ] && [ ! -e "$t/scratchB" ] && ! systemctl is-active --quiet "$bunit"; } \
    && ok "limits: stage B answered, its scope ended and its scratch directory is gone" || bad "limits: stage B: rc=$rc $(tail -c 400 "$t/lb.err")"
  python3 -I -c 'import json,sys; t=sys.argv[1]; r=json.load(open(t+"/lb.md.run.json"))
sys.exit(r.get("home") != t+"/acctB/homes/"+r["session"] or r["limits"]["writable"][:2] != [r["home"], t+"/scratchB"])' "$t" \
    && ok "limits: stage B's record names its home, first of its writable paths" || bad "limits: stage B's record: $(field "$t/lb.md.run.json" home)"
  { [ "$(digests)" = "$before" ] && [ -z "$(find "$t/acct/homes" "$t/acctB/homes" -name '*.sign-in')" ]; } \
    && ok "limits: each account's sign-in and settings.json unchanged, no derived sign-in left" \
    || bad "limits: accounts after the stages: $(find "$t/acct/homes" "$t/acctB/homes" -name '*.sign-in' | tr '\n' ' ')"
  # A link left in a home: in place of the empty file bwrap left at `.credentials.json`, a link to a
  # path below /oldroot (bwrap's old root) of a file that does not exist; its resume launches nothing.
  echo answer > "$t/bin/mode"
  req k.md ",\"limits\":{\"scratch\":\"$t/scratchK\",\"read_only\":[\"$t/bin\"]}" | apr > "$t/k.out" 2> "$t/k.err"; rc=$?
  khome=$(field "$t/k.md.run.json" home)
  { [ "$rc" -eq 0 ] && [ -d "$khome" ] && [ -f "$khome/.credentials.json" ] && [ ! -s "$khome/.credentials.json" ]; } \
    && ok "limits: a first launch left an empty .credentials.json in its home" || bad "limits: first launch: rc=$rc $(tail -c 400 "$t/k.err")"
  rm -f "$khome/.credentials.json"; ln -s "/oldroot$t/no-such-file" "$khome/.credentials.json"
  printf '{"role":"reviewer","account":"%s","model":"haiku","effort":"low","permission_mode":"auto","cwd":"%s","prompt":"%s","out":"%s","resume":{"record":"%s"},"limits":{"scratch":"%s","read_only":["%s"]}}' \
    "$t/acct" "$t/cwd" "$t/prompt.md" "$t/k2.md" "$t/k.md.run.json" "$t/scratchK2" "$t/bin" | apr > "$t/k2.out" 2> "$t/k2.err"; rc=$?
  { [ "$rc" -eq 3 ] && [ "$(field "$t/k2.md.run.json" state)" = failed ] && [ "$(field "$t/k2.md.run.json" unmet)" = launch ] \
    && [ ! -e "$t/no-such-file" ] && [ -L "$khome/.credentials.json" ]; } \
    && ok "limits: a resume whose home holds a link at .credentials.json launched nothing, and nothing was made at its target" \
    || bad "limits: link in the home: rc=$rc $(field "$t/k2.md.run.json" state) $(field "$t/k2.md.run.json" unmet) $(ls -d "$t/no-such-file" 2> /dev/null)"
  # An implementer with the four members of limits (specs/ap/run-limits.md R1, R5, R7, R11).
  mkdir -p "$t/wdir" "$t/src" "$t/dst"; echo kept > "$t/wdir/keep"; echo named > "$t/bin/mode"
  req n.md ",\"limits\":{\"scratch\":\"$t/scratch2\",\"write\":[\"$t/wdir\"],\"over\":[[\"$t/src\",\"$t/dst\"]],\"read_only\":[\"$t/wdir/keep\",\"$t/bin\"],\"show\":[\"~/.config/gh\"]}" \
    | sed 's/"role":"reviewer"/"role":"implementer"/' | apr > "$t/n.out" 2> "$t/n.err"; rc=$?
  ans=$(cat "$t/n.md" 2> /dev/null)
  { [ "$rc" -eq 0 ] && [ "$(field "$t/n.md.run.json" state)" = ok ] && [ -e "$t/wdir/x" ] && [ -e "$t/src/y" ] && [ ! -e "$t/dst/y" ] \
    && [ "$(cat "$t/wdir/keep")" = kept ] && [ ! -e "$t/wdir/moved" ] && [ ! -e "$t/cwd/n" ] && [ ! -e "$t/cwd/m" ] \
    && [[ "$ans" == "write written; over written; keep refused ("*"); keep removal refused ("*"); keep rename refused ("*"); cwd refused (Read-only file system); root refused ("*")" ]]; } \
    && ok "limits: an implementer wrote its write directory and the over source, not the read_only file, its cwd or /proc/1/root: $ans" \
    || bad "limits: named paths: rc=$rc $ans $(tail -c 400 "$t/n.err")"
  python3 -I -c 'import json,sys; t=sys.argv[1]; l=json.load(open(t+"/n.md.run.json"))["limits"]
hidden = l.pop("hidden", None); l.pop("view", None); l.pop("egress", None)
r = json.load(open(t+"/n.md.run.json"))
sys.exit(l != {"scratch": t+"/scratch2", "writable": [r["home"], t+"/scratch2", t+"/wdir", t+"/dst"], "bwrap": "/usr/bin/bwrap",
               "over": [[t+"/src", t+"/dst"]], "read_only": [t+"/wdir/keep", t+"/bin"], "show": ["~/.config/gh"]}
         or not isinstance(hidden, list) or any(p.endswith("/.config/gh") for p in hidden))' "$t" \
    && ok "limits: the implementer's record names what it writes, the members as given, and ~/.config/gh shown" || bad "limits member of the implementer: $(field "$t/n.md.run.json" limits)"
  # spec: specs/ap/run-limits.md#R17
  # spec: specs/ap/run-limits.md#R18
  # spec: specs/ap/run-limits.md#R19
  # The stage's network: a reviewer stage with limits, given the host's addresses (every address of
  # every interface), the script's own ns/net and an abstract socket the script listens on. Its
  # interfaces are `lo` alone, its namespace is another, every connect and the lookup fail, and its
  # proxy refuses `CONNECT example.com:443` (not_listed) and `CONNECT 127.0.0.1:443` (literal),
  # each with its line in AP_EGRESS_LOG; no listed name is asked, so the case needs no network.
  echo egress > "$t/bin/mode"
  ip -o addr show | awk '{print $4}' | cut -d/ -f1 > "$t/bin/addrs"
  readlink /proc/self/ns/net > "$t/bin/netns"
  echo "ap-egress-$$" > "$t/bin/abstract"
  python3 -I -c 'import socket,sys,time; s=socket.socket(socket.AF_UNIX); s.bind("\0"+sys.argv[1]); s.listen(); time.sleep(300)' "ap-egress-$$" &
  echo $! > "$t/bin/abstract.pid"
  for _ in $(seq 50); do
    python3 -I -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.connect("\0"+sys.argv[1])' "ap-egress-$$" 2> /dev/null && break; sleep 0.1
  done
  req e.md ",\"limits\":{\"scratch\":\"$t/scratchE\",\"read_only\":[\"$t/bin\"]}" | apr > "$t/e.out" 2> "$t/e.err"; rc=$?
  kill "$(cat "$t/bin/abstract.pid")" 2> /dev/null
  ans=$(cat "$t/e.md" 2> /dev/null)
  echo "     egress: $ans"
  { [ "$rc" -eq 0 ] && [ "$(field "$t/e.md.run.json" state)" = ok ] && [ -s "$t/bin/addrs" ] \
    && [ "$ans" = "interfaces lo; namespace other; reached:; example.com HTTP/1.1 403 Forbidden; 127.0.0.1 HTTP/1.1 403 Forbidden" ]; } \
    && ok "egress: the stage has lo alone in another namespace; port 22 of each of the host's $(wc -l < "$t/bin/addrs") addresses, 127.0.0.53:53, 100.100.100.100:53, the abstract socket and getent fail; the proxy answered 403 twice" \
    || bad "egress: rc=$rc $ans $(tail -c 400 "$t/e.err")"
  python3 -I -c 'import json,os,sys; t=sys.argv[1]; f=t+"/egress.jsonl"
lines=[json.loads(l) for l in open(f)]
e=json.load(open(t+"/e.md.run.json"))["limits"]["egress"]
sys.exit(oct(os.stat(f).st_mode & 0o777) != "0o600" or len(lines) != 2
         or any(l["run"] != t+"/e.md.run.json" or l["role"] != "reviewer" or l["status"] != 403 for l in lines)
         or [(l["name"], l["port"], l["reason"]) for l in lines] != [("example.com", 443, "not_listed"), ("127.0.0.1", 443, "literal")]
         or e["names"] != ["api.anthropic.com"] or not e["proxy"].startswith("http://127.0.0.1:"))' "$t" \
    && ok "egress: AP_EGRESS_LOG is mode 0600 with two lines of this reviewer, not_listed and literal; the record names the proxy and api.anthropic.com" \
    || bad "egress: record $(ls -l "$t/egress.jsonl" 2> /dev/null) $(cat "$t/egress.jsonl" 2> /dev/null) $(field "$t/e.md.run.json" limits)"
  # spec: specs/ap/door.md#R1
  # The door (specs/ap/door.md): a verifier stage with limits asks for route and for another
  # function, and runs `ap check` for its worktree, whose PC (a host that does not exist) the ping
  # of a fake tailscale never reaches. The fakes ont and tailscale lie in bin, which the view cannot
  # write: their files show they ran outside it. The stage's ap check reads this second policy.
  printf '{"models":{"allowed":["haiku"],"never":[]},"accounts":{"claude":["%s","%s"]},"hosts":{"pc":{"ssh":"nobody@no-such-pc.invalid","work":"/nonexistent/ap-checks","busy_load":0.75}}}' \
    "$t/acct" "$t/acctB" > "$t/policy-door.json"
  mkdir -p "$t/dcwd"; printf '#!/bin/sh\necho checked\n' > "$t/dcwd/check"; chmod 0755 "$t/dcwd/check"
  git -C "$t/dcwd" init -q -b main && git -C "$t/dcwd" remote add origin https://github.com/example/door.git \
    && git -C "$t/dcwd" add -A && git -C "$t/dcwd" -c user.name=t -c user.email=t@e commit -q -m one
  ln -s "$ap" "$t/bin/ap"
  # The fake engine notes its arguments and role, answers `route`, and refuses any other function as the engine
  # refuses a role whose grant lacks it (the door has no list of its own: specs/ap/door.md R5).
  cat > "$t/bin/ont" <<'FAKEONT'
#!/bin/sh
printf '%s ONT_ROLE=%s\n' "$*" "$ONT_ROLE" >> "$(dirname "$0")/ont.calls"
cat > /dev/null
[ "$2" = route ] || { echo '{"status":"refused","code":"not_granted"}'; exit 2; }
echo route-answer
FAKEONT
  printf '#!/bin/sh\necho "$*" >> "$(dirname "$0")/tailscale.calls"\nexit 1\n' > "$t/bin/tailscale"
  cat > "$t/bin/claude" <<'DOOR'
#!/usr/bin/env bash
cat > /dev/null
sid=; prev=; for a in "$@"; do case "$prev" in --session-id|--resume) sid=$a ;; esac; prev=$a; done
printf '{"type":"system","subtype":"init","session_id":"%s","mcp_servers":[]}\n' "$sid"
r=$(ap fn route < /dev/null 2> /dev/null); rc1=$?
ap fn other < /dev/null > "$TMPDIR/other.out" 2> "$TMPDIR/other.err"; rc2=$?
grep -q 'not_granted' "$TMPDIR/other.err" "$TMPDIR/other.out" && code=not_granted || code=other
c=$(ap check "$PWD" 2>&1 | tr '\n' '|'); rc3=${PIPESTATUS[0]}
said="route $rc1 $r; other $rc2 $code; check $rc3 $c"
printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"%s","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$said" "$sid"
DOOR
  chmod 0755 "$t/bin/claude" "$t/bin/ont" "$t/bin/tailscale"
  printf '{"role":"reviewer","ont_role":"verifier","account":"%s","model":"haiku","effort":"low","permission_mode":"auto","cwd":"%s","prompt":"%s","out":"%s","timeout_seconds":90,"limits":{"scratch":"%s","read_only":["%s","%s","%s"]},"env":{"AP_POLICY_FILE":"%s"}}' \
    "$t/acct" "$t/dcwd" "$t/prompt.md" "$t/door.md" "$t/scratch3" "$t/bin" "$t/policy-door.json" "$ap" "$t/policy-door.json" \
    | env -i PATH="$t/bin:$PATH" HOME="${HOME:-/root}" LANG=C.UTF-8 AP_POLICY_FILE="$t/policy-door.json" \
      AP_DOOR_LOG="$t/door.jsonl" AP_CHECKS_LOG="$t/checks.jsonl" "$ap" run > "$t/door.out" 2> "$t/door.err"; rc=$?
  ans=$(cat "$t/door.md" 2> /dev/null)
  { [ "$rc" -eq 0 ] && [ "$ans" = "route 0 route-answer; other 2 not_granted; check 0 check: on this host (pc_unreachable)|checked|" ]; } \
    && ok "door: ap fn route answered, ap fn other was refused not_granted by the engine, ap check ran here after pc_unreachable: $ans" \
    || bad "door: rc=$rc $ans $(tail -c 400 "$t/door.err")"
  { [ "$(cat "$t/bin/ont.calls" 2> /dev/null)" = "$(printf 'fn route ONT_ROLE=verifier\nfn other ONT_ROLE=verifier')" ] && grep -q '^ping ' "$t/bin/tailscale.calls" 2> /dev/null; } \
    && ok "door: the engine was asked as the verifier for both names and the ping ran, both outside the view" \
    || bad "door: ont $(cat "$t/bin/ont.calls" 2> /dev/null) tailscale $(cat "$t/bin/tailscale.calls" 2> /dev/null)"
  python3 -I -c 'import json,os,sys; t=sys.argv[1]; f=t+"/door.jsonl"
lines=[json.loads(l) for l in open(f)]
sys.exit(oct(os.stat(f).st_mode & 0o777) != "0o600" or len(lines) != 3
         or any(l["run"] != t+"/door.md.run.json" or l["role"] != "verifier" for l in lines))' "$t" \
    && ok "door: its record is mode 0600 with three lines of this stage's run and role" || bad "door: record $(ls -l "$t/door.jsonl" 2> /dev/null) $(cat "$t/door.jsonl" 2> /dev/null)"
  { [ ! -e "$t/checks.jsonl" ] && [ ! -e "$t/scratch3.door" ] && [ ! -e "$t/scratch3" ]; } \
    && ok "door: no row of the checks log for a call placed here, and the door and the scratch directory are gone" \
    || bad "door: $(ls -d "$t/checks.jsonl" "$t/scratch3.door" "$t/scratch3" 2> /dev/null | tr '\n' ' ')"
  # spec: specs/ap/run-limits.md#R16
  # The view (specs/ap/run-limits.md R16): a verifier stage with limits, in a worktree of a clone
  # with a GitHub origin, its policy (whose PC no one answers) named in its request and in the
  # caller's environment; a bin directory of its own (its fake CLI, a tailscale that exits 1 and the
  # built ap; no fake ont; $t/bin neither on its PATH nor bound). Its fake CLI says, printing no
  # content, which of the security audit's must-not-see paths exist in its view (test -e and test -L,
  # never an open), its uid map, its capability sets, and whether a user namespace, the system bus,
  # `ont describe`, `ap unit run` of a unit and its door answer; and what its `ap check` printed.
  v=$t/view; mkdir -p "$v/bin"
  home=$(getent passwd "$(id -u)" | cut -d: -f6)
  git init -q -b main "$v/repo" && git -C "$v/repo" remote add origin https://github.com/example/view.git \
    && git -C "$v/repo" -c user.name=t -c user.email=t@e commit -q --allow-empty -m one \
    && git -C "$v/repo" worktree add -q -b view "$v/wt" 2> /dev/null
  printf '{"models":{"allowed":["haiku"],"never":[]},"accounts":{"claude":["%s"]},"hosts":{"pc":{"ssh":"nobody@no-such-pc.invalid","work":"/nonexistent/ap-checks","busy_load":0.75}}}' \
    "$t/acct" > "$v/policy.json"
  ln -s "$ap" "$v/bin/ap"
  printf '#!/bin/sh\necho "$*" >> "%s/tailscale.calls"\nexit 1\n' "$t" > "$v/bin/tailscale"
  for p in "$home/.ssh" "$home/.config/gh" /etc/shadow /etc/agent-platform/credentials /var/lib/ontology-platform \
    /var/lib/tailscale /run "$t/acctB" "$home/.claude" "$home/.config/rclone/rclone.conf" \
    "$home/.config/tailscale-oauth.env" "$home/.config/herdr/herdr.sock" "$home/.codex/auth.json" \
    "$home/.google_authenticator" /etc/ssh/ssh_host_ed25519_key /docker/hindsight/.env /var/backups \
    /etc/monarx-agent.conf; do echo "$p"; done > "$v/bin/paths"
  cat > "$v/bin/claude" <<'VIEW'
#!/usr/bin/env bash
cat > /dev/null
sid=; prev=; for a in "$@"; do case "$prev" in --session-id|--resume) sid=$a ;; esac; prev=$a; done
printf '{"type":"system","subtype":"init","session_id":"%s","mcp_servers":[]}\n' "$sid"
here=$(cd "$(dirname "$0")" && pwd)
seen=; while read -r p; do { test -e "$p" || test -L "$p"; } && seen="$seen $p"; done < "$here/paths"
uid=$(tr -s ' \t' ' ' < /proc/self/uid_map | sed 's/^ //')
eff=$(awk '/^CapEff/ {print $2}' /proc/self/status); bnd=$(awk '/^CapBnd/ {print $2}' /proc/self/status)
unshare --user true > /dev/null 2>&1 && ns=made || ns=failed
conn() { python3 -I -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.connect(sys.argv[1])' "$1" > /dev/null 2>&1; }
conn /run/dbus/system_bus_socket && bus=connected || bus=failed
conn "$AP_DOOR" && door=connected || door=failed
echo '{}' | ont describe > /dev/null 2>&1 && ont=0 || ont=nonzero
ap unit run view-probe-unit < /dev/null > /dev/null 2>&1 && run=0 || run=nonzero
ap check "$PWD" -- true > /dev/null 2> "$TMPDIR/check.err"; crc=$?
c=$(tr '\n' '|' < "$TMPDIR/check.err" | tr -d '"\\')
said="seen:$seen; uid_map $uid; CapEff $eff; CapBnd $bnd; userns $ns; system_bus_socket $bus; ont describe $ont; ap unit run $run; door $door; check $crc $c"
printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"%s","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$said" "$sid"
VIEW
  chmod 0755 "$v/bin/claude" "$v/bin/tailscale"
  printf '{"role":"reviewer","ont_role":"verifier","account":"%s","model":"haiku","effort":"low","permission_mode":"auto","cwd":"%s","prompt":"%s","out":"%s","timeout_seconds":90,"limits":{"scratch":"%s","read_only":["%s","%s"]},"env":{"AP_POLICY_FILE":"%s"}}' \
    "$t/acct" "$v/wt" "$t/prompt.md" "$t/view.md" "$t/scratchV" "$v/bin" "$ap" "$v/policy.json" \
    | env -i PATH="$v/bin:$PATH" HOME="${HOME:-/root}" LANG=C.UTF-8 AP_POLICY_FILE="$v/policy.json" \
      AP_DOOR_LOG="$t/door-view.jsonl" AP_CHECKS_LOG="$t/checks-view.jsonl" "$ap" run > "$t/view.out" 2> "$t/view.err"; rc=$?
  ans=$(cat "$t/view.md" 2> /dev/null)
  echo "     view: $ans"
  host_map=$(tr -s ' \t' ' ' < /proc/self/uid_map | sed 's/^ //')
  { [ "$rc" -eq 0 ] && [[ "$ans" == "seen:; "* ]]; } \
    && ok "view: none of the must-not-see paths exists in the stage's view (~/.ssh, ~/.config/gh, /etc/shadow, /etc/agent-platform/credentials, /var/lib/ontology-platform, /run, the other account, ...)" \
    || bad "view: paths seen: rc=$rc ${ans%%;*} $(tail -c 400 "$t/view.err")"
  { [[ "$ans" == *"; uid_map "* ]] && [[ "$ans" != *"; uid_map $host_map;"* ]] \
    && [[ "$ans" == *"; CapEff 0000000000000000; CapBnd 0000000000000000; userns failed;"* ]]; } \
    && ok "view: its uid_map is not the host's, CapEff and CapBnd are zero, and unshare --user fails" \
    || bad "view: namespace and capabilities: $ans"
  { [[ "$ans" == *"; system_bus_socket failed; ont describe nonzero; ap unit run nonzero; door connected;"* ]]; } \
    && ok "view: a connect to the system bus fails, ont describe and ap unit run exit non-zero, the door connects" \
    || bad "view: bus, store and door: $ans"
  { [[ "$ans" == *"; check 0 check: on this host (pc_unreachable)|"* ]] && [[ "$ans" != *no_pc_in_policy* ]] \
    && grep -q '^ping ' "$t/tailscale.calls" 2> /dev/null; } \
    && ok "view: ap check read its policy and asked the door, whose ping failed: check: on this host (pc_unreachable)" \
    || bad "view: ap check: $ans $(cat "$t/tailscale.calls" 2> /dev/null)"
fi

[ "$fails" -eq 0 ] && echo "host-run-scope: PASS" || { echo "host-run-scope: $fails FAILED"; exit 1; }
