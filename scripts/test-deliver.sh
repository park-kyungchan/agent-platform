#!/usr/bin/env bash
# The walk of `ap deliver` on the real installer scripts (crates/ap/README.md, "Delivery").
#   scripts/test-deliver.sh <ont binary> <ap binary>
# It runs the built `ap deliver`, kills the process group of `ap` itself at each rename of the content
# installer, and checks that the next call leaves the pair at one commit (`sha256sum -c`). It fabricates
# no attempt. Every path it touches lies under one directory made with `mktemp -d` and removed at the end:
# a scratch origin repository, a scratch store reached through a wrapper named `ont` first on PATH, every
# installer variable and AP_DELIVER_LOCK. It never calls systemctl. The origin's install-content.sh is
# this repository's file unchanged; its install-ap.sh differs only in the build lock's path; a `cargo`
# shim writes a file that answers --version with the commit it is built from. Passes i and j give the
# request the member `engine`: a scratch engine repository whose `scripts/install-ont.sh` is a fake (the
# real script's own `git fetch origin` and ancestor check, then a file that answers --version with the
# pin), and a merged higher version of the work schema with an `ontology/migrate` that records its call,
# installed on the scratch store. The engine file and the store lie under ONT_INSTALL_BIN_DIR and
# ONT_INSTALL_STATE_DIR of that directory.
# One line `walk pass=<a..j> k=<k> pair=<commit or neither> outcome=<outcome>` per case, then
# `deliver-walk: ok`. Under CI=true it is skipped: the engine there has no object for the target's key.
set -euo pipefail
[ "$#" -eq 2 ] || { echo "usage: scripts/test-deliver.sh <ont binary> <ap binary>" >&2; exit 2; }
if [ "${CI:-}" = true ]; then echo "deliver-walk: skipped (CI)"; exit 0; fi
ONT=$(realpath -- "$1"); AP=$(realpath -- "$2")
REPO=$(cd -- "$(dirname -- "$(realpath "$0")")/.." && pwd -P)
MV=$(command -v mv)
W=$(mktemp -d)
trap '[ -z "${waiter:-}" ] || kill -KILL -- "-$waiter" 2>/dev/null || true; [ -z "${holder:-}" ] || kill -KILL -- "-$holder" 2>/dev/null || true; rm -rf -- "$W"' EXIT
fail() { echo "deliver-walk: FAILED: $*" >&2; [ -f "$W/out.json" ] && cat "$W/out.json" >&2; exit 1; }

# The host store's install objects, read only, to show that the walk wrote none of them.
host_count() {
  printf '{"object_type":"install"}\n' | "$ONT" search 2>/dev/null |
    python3 -I -c 'import json, sys; d = json.load(sys.stdin); print(len(d["result"]["objects"]) if d.get("status") == "ok" else "none")' 2>/dev/null || echo none
}
host_before=$(host_count)

mkdir -p "$W"/{bin,tmp,origin,store,work/walk,install,backup,agents,content-backup,state,reports}
export TMPDIR=$W/tmp GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=walk GIT_AUTHOR_EMAIL=walk@example.invalid GIT_COMMITTER_NAME=walk GIT_COMMITTER_EMAIL=walk@example.invalid
export AP_INSTALL_DIR=$W/install AP_BACKUP_DIR=$W/backup AP_INSTALL_LOCK=$W/install.lock
export AP_CONTENT_DST=$W/agents AP_CONTENT_BACKUP_DIR=$W/content-backup AP_DELIVER_LOCK=$W/deliver.lock
export ONT_INSTALL_BIN_DIR=$W/ont-bin ONT_INSTALL_STATE_DIR=$W/ont-state
STORE=$ONT_INSTALL_STATE_DIR/ont/ontology.sqlite
mkdir -p "$ONT_INSTALL_BIN_DIR" "$ONT_INSTALL_STATE_DIR/ont"
unset INVOCATION_ID CARGO_TARGET_DIR
export WALK=$W WALK_MV=$MV

# The wrapper `ont`: the built ap has no other way to a scratch store.
printf '#!/bin/sh\nexec %q --store %q "$@"\n' "$ONT" "$STORE" > "$W/bin/ont"
# The cargo shim: the build of install-ap.sh, one second at least so that the content installer's
# tarball is stamped after the attempt's start; with $WALK/slow it lasts until $WALK/release exists.
cat > "$W/bin/cargo" <<'EOF'
#!/bin/sh
[ -e "$WALK/slow" ] && { : > "$WALK/building"; while [ ! -e "$WALK/release" ]; do sleep 0.2; done; }
sleep 1
c=$(git rev-parse HEAD) || exit 1
mkdir -p "$CARGO_TARGET_DIR/release"
printf '#!/bin/sh\necho "ap 0.1.0 %s"\n' "$c" > "$CARGO_TARGET_DIR/release/ap"
chmod 0755 "$CARGO_TARGET_DIR/release/ap"
EOF
# The mv shim: counts the renames under the content tree; after the one $WALK/kill_at names, it kills
# its process group, which is the group of ap (started with setsid) and of every installer of the call.
cat > "$W/bin/mv" <<'EOF'
#!/bin/bash
"$WALK_MV" "$@"; rc=$?
case " $* " in
  *" $AP_CONTENT_DST/"*)
    if [ -e "$WALK/invalidate" ]; then rm -f -- "$AP_CONTENT_BACKUP_DIR"/*.sha256; fi
    if [ -e "$WALK/count" ]; then
      n=$(( $(cat "$WALK/count") + 1 )); echo "$n" > "$WALK/count"
      if [ -e "$WALK/kill_at" ] && [ "$n" -eq "$(cat "$WALK/kill_at")" ]; then kill -KILL 0; sleep 60; fi
    fi ;;
esac
exit "$rc"
EOF
chmod 0755 "$W/bin/ont" "$W/bin/cargo" "$W/bin/mv"
export PATH=$W/bin:$PATH

# The scratch store: the work schema with the task source reading this directory only (its version is
# unchanged, and the origin carries the same file), the installed files, one task.
O=$W/origin
mkdir -p "$O/scripts" "$O/ontology"
printf '# walk\n\nOwner: walk\n' > "$W/work/walk/STATE.md"
python3 -I - "$REPO/ontology/work.schema.json" "$O/ontology/work.schema.json" "$W/work" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
d['sources']['task_states']['glob'] = sys.argv[3] + '/*/STATE.md'
d['sources']['task_states']['exclude'] = []
open(sys.argv[2], 'w').write(json.dumps(d, indent=1) + '\n')
PY
ont schema install "$O/ontology/work.schema.json" > /dev/null
ont index --source task_states --source installed_binaries > /dev/null
"$ONT" --version | awk '{print $3}' > "$O/ontology/ENGINE-PIN"
cp -p "$REPO/scripts/install-content.sh" "$O/scripts/install-content.sh"
sed "s|^LOCK_FILE=\".*\"$|LOCK_FILE=\"$W/build.lock\"|" "$REPO/scripts/install-ap.sh" > "$O/scripts/install-ap.sh"
[ "$(diff "$REPO/scripts/install-ap.sh" "$O/scripts/install-ap.sh" | grep -c '^[<>]')" = 2 ] || fail "the build lock line of install-ap.sh"

# The origin: content of four top paths, so that one content install renames eight times.
FILES="AGENTS.md skills/a/SKILL.md skills/b/SKILL.md skills/c/SKILL.md"
content() {
  local f
  for f in $FILES; do mkdir -p "$O/content/$(dirname "$f")"; printf 'walk %s revision %s\n' "$f" "$1" > "$O/content/$f"; done
  # shellcheck disable=SC2086
  (cd "$O/content" && sha256sum $FILES > SHA256SUMS)
}
git -C "$O" init -q -b main
content 0
git -C "$O" add -A && git -C "$O" commit -q -m "walk 0"
C0=$(git -C "$O" rev-parse HEAD)
# A merge of a branch that changes every content file, and an accepting review of its head; it prints
# the merge (it runs in a command substitution, so its number comes from the origin).
merge() {
  local n; n=$(git -C "$O" rev-list --first-parent --count main)
  git -C "$O" checkout -q -b "b$n" main
  content "$n"
  # A move of the pin, a schema, a migration: the files $W/next-* name, when they exist.
  [ ! -e "$W/next-pin" ] || cp "$W/next-pin" "$O/ontology/ENGINE-PIN"
  [ ! -e "$W/next-schema" ] || cp "$W/next-schema" "$O/ontology/work.schema.json"
  [ ! -e "$W/next-migrate" ] || install -m 0755 "$W/next-migrate" "$O/ontology/migrate"
  git -C "$O" add -A && git -C "$O" commit -q -m "change $n"
  local head; head=$(git -C "$O" rev-parse HEAD)
  git -C "$O" checkout -q main
  git -C "$O" merge -q --no-ff -m "merge $n" "b$n"
  accept "$head" walk/origin
  git -C "$O" rev-parse HEAD
}
# An accepting review of the head $1 in the repository $2.
accept() {
  printf 'Verdict: accept\n%s\n' "$1" > "$W/reports/$1.md"
  python3 -I - "$1" "$W" "$2" <<'PY'
import hashlib, json, subprocess, sys
head, w, repository = sys.argv[1:]
def apply(action, rid, p):
    body = dict(action_type=action, request_id=rid, agent='walk', parameters=p)
    out = subprocess.run(['ont', 'action', 'apply', '--client', 'walk'], input=json.dumps(body), capture_output=True, text=True, check=True).stdout
    v = json.loads(out); assert v['result']['outcome'] == 'applied', v
    return v['result']['edits'][0]['id']
task = dict(object_type='task', namespace=dict(instance='vps://palantirkc', account='root'), native_key='walk')
review = apply('start_review', 'rv-' + head, dict(task=task, kind='packet', repository=repository, head=head, model='walk', effort='walk', prompt=w + '/reports/prompt.md', prompt_sha256='b' * 64))
report = w + '/reports/' + head + '.md'
apply('close_review', 'rv-' + head + '-close', dict(review=review, verdict='accept', report=report, report_sha256=hashlib.sha256(open(report, 'rb').read()).hexdigest()))
PY
}

# The requests: a Lead's install, a Lead's rollback, a Lead's install with `engine`.
request() {
  python3 -I -c 'import json, sys; w = sys.argv[1]; r = dict(repository="walk/origin", url=w + "/origin", state=w + "/state", build_lock=w + "/build.lock", by="walk", reason="the walk")
if sys.argv[2] == "rollback": r["rollback"] = True
if sys.argv[2] == "engine": r["engine"] = dict(repository="walk/engine", url=w + "/engine-origin")
print(json.dumps(r))' "$W" "$1" > "$W/$1.json"
}
request lead; request rollback; request engine
CLONE=$W/state/origin

# One call of the built ap in a process group of its own; RC its exit, $W/out.json its record.
# The subshell reports a killed call on its own stderr, which is discarded.
call() { RC=$( (setsid -w "$AP" deliver < "$W/$1.json" > "$W/out.json" 2> "$W/err"; echo $?) 2> /dev/null); }
member() { python3 -I -c 'import json, sys; r = json.load(open(sys.argv[1])); v = eval(sys.argv[2], {"r": r}); print("null" if v is None else v)' "$W/out.json" "$1"; }
# The commit the pair is at: the one `ap --version` names, when the tree passes its manifest.
pair() {
  local c; c=$("$AP_INSTALL_DIR/ap" --version 2>/dev/null | awk '{print $3}') || true
  if [ -n "$c" ] && git -C "$O" show "$c:content/SHA256SUMS" > "$W/sums" 2>/dev/null &&
     (cd "$AP_CONTENT_DST" && sha256sum -c --quiet "$W/sums" > /dev/null 2>&1); then echo "$c"; else echo neither; fi
}
line() { echo "walk pass=$1 k=$2 pair=$3 outcome=$4"; }
# A call killed after the k-th rename of the content tree.
killed() { echo 0 > "$W/count"; echo "$2" > "$W/kill_at"; call "$1"; rm -f "$W/count" "$W/kill_at"; [ "$RC" = 137 ] || fail "the call was not killed at rename $2 (exit $RC)"; }

# The pair at C0, installed by the scripts themselves from a clone of the origin.
git clone -q "$O" "$W/boot"
CARGO_TARGET_DIR=$W/boot-target bash "$W/boot/scripts/install-ap.sh" install "$C0" > /dev/null 2> "$W/err" ||
  fail "the first install of ap: $(tail -n 3 "$W/err")"
bash "$W/boot/scripts/install-content.sh" install "$C0" > /dev/null
[ "$(pair)" = "$C0" ] || fail "the pair is not at the first commit"

# (a) An install; it counts the renames of one content install.
M=$(merge)
echo 0 > "$W/count"; call lead; N=$(cat "$W/count"); rm -f "$W/count"
[ "$RC" = 0 ] && [ "$(member 'r["end"]')" = installed ] && [ "$(pair)" = "$M" ] || fail "pass a (exit $RC)"
[ "$N" -ge 2 ] || fail "pass a counted $N renames"
line a 0 "$(pair)" "$(member 'r["outcome"]')"

# spec: specs/ap/deliver.md#R21
# (b) For every k, highest first: an install killed after the k-th rename, then one call, which repairs
# the dead attempt or closes it by the read; the last one (k=1) is repaired and rolled back in (e).
for k in $(seq "$N" -1 1); do
  M=$(merge); killed lead "$k"
  call lead; p=$(pair)
  [ "$p" = "$M" ] && [ "$RC" = 0 ] || fail "pass b k=$k: pair $p, exit $RC"
  line b "$k" "$p" "$(member 'r["closed"]["outcome"]')"
done

# spec: specs/ap/deliver.md#R18
# spec: specs/ap/deliver.md#R33
# (e) A rollback of the attempt pass b repaired: the oldest tarball brings the tree to its from.
FROM=$(git -C "$O" rev-parse "$M^1")
echo 0 > "$W/count"; call rollback; NRB=$(cat "$W/count"); rm -f "$W/count"
p=$(pair)
[ "$RC" = 0 ] && [ "$(member 'r["end"]')" = installed ] && [ "$p" = "$FROM" ] || fail "pass e: pair $p, exit $RC"
line e 1 "$p" "$(member 'r["outcome"]')"

# spec: specs/ap/deliver.md#R21
# spec: specs/ap/deliver.md#R29
# (c) For every k: an install killed after the k-th rename, the clone's origin/main moved off the
# attempt's commit so that the repair cannot reach it, then one call: the command's rollback on the real
# kept copy and tarball, and the call goes on to install the tip.
for k in $(seq 1 "$N"); do
  M=$(merge); killed lead "$k"
  git -C "$CLONE" update-ref refs/remotes/origin/main "$(git -C "$O" rev-parse "$M^1")"
  call lead; p=$(pair)
  [ "$p" = "$M" ] && [ "$RC" = 0 ] || fail "pass c k=$k: pair $p, exit $RC"
  line c "$k" "$p" "$(member 'r["closed"]["outcome"]')"
done

# spec: specs/ap/deliver.md#R21
# spec: specs/ap/deliver.md#R33
# (d) For every k: a rollback killed after the k-th rename of its content rollback; one call refuses
# attempt_running; the two commands of the README with the handles of the attempt it rolls back; the
# close by hand; then a Lead's install brings the pair to the tip again.
M=$(merge); call lead
[ "$RC" = 0 ] && [ "$(member 'r["end"]')" = installed ] || fail "pass d: the install before it (exit $RC)"
for k in $(seq 1 "$NRB"); do
  BIN=$(member 'r["rollback_binary"]'); TAR=$(member 'r["rollback_content"]')
  BACK=$(member 'r["from"]'); HERE=$(member 'r["to"]')
  [ "$(pair)" = "$HERE" ] || fail "pass d k=$k: the pair is not at the installed commit"
  killed rollback "$k"
  call lead
  [ "$RC" = 2 ] && [ "$(member 'r["code"]')" = attempt_running ] || fail "pass d k=$k: exit $RC"
  ID=$(python3 -I -c 'import json, sys; print(json.dumps(json.load(open(sys.argv[1]))["detail"]["install"]))' "$W/out.json")
  bash "$CLONE/scripts/install-content.sh" rollback "$TAR" > /dev/null
  bash "$CLONE/scripts/install-ap.sh" rollback "$BIN" > /dev/null
  p=$(pair); [ "$p" = "$BACK" ] || fail "pass d k=$k: the README's commands left the pair at $p"
  python3 -I - "$ID" "$p" <<'PY'
import json, subprocess, sys
install, to = json.loads(sys.argv[1]), sys.argv[2]
def ont(args, body):
    return json.loads(subprocess.run(['ont', *args], input=json.dumps(body), capture_output=True, text=True).stdout)
f = {k: v['value'] for k, v in ont(['get'], dict(id=install))['result']['objects'][0]['observations'][-1]['fields'].items()}
assert f['kind'] == 'rollback' and f['status'] == 'running' and f['commit'] == to, f
body = dict(action_type='close_install', request_id='ap.%s.rb%s.close' % (f['commit'], f['attempt']), agent='ap-deliver',
            parameters=dict(install=install, outcome='installed', to=to))
v = ont(['action', 'apply', '--client', 'ap-deliver'], body)
assert v['status'] == 'ok' and v['result']['outcome'] == 'applied', v
PY
  line d "$k" "$p" attempt_running
  call lead
  [ "$RC" = 0 ] && [ "$(pair)" = "$HERE" ] || fail "pass d k=$k: the install after the close by hand (exit $RC)"
done

# spec: specs/ap/deliver.md#R16
# (f) ap killed alone while an installer lives under a slow shim: the installer holds the inherited
# lock, and the next call ends busy. Then the installer finishes and one call repairs the attempt.
M=$(merge)
: > "$W/slow"
setsid "$AP" deliver < "$W/lead.json" > /dev/null 2>&1 &
pid=$!
for _ in $(seq 1 300); do [ -e "$W/building" ] && break; sleep 0.2; done
[ -e "$W/building" ] || fail "pass f: the installer did not start"
[ "$(readlink "/proc/$pid/exe")" = "$AP" ] || fail "pass f: $pid is not ap"
kill -KILL "$pid"; wait "$pid" 2> /dev/null || true
call lead
[ "$RC" = 2 ] && [ "$(member 'r["code"]')" = busy ] || fail "pass f: exit $RC"
line f 0 "$(pair)" busy
rm -f "$W/slow"; : > "$W/release"
flock -w 120 "$AP_DELIVER_LOCK" true || fail "pass f: the installer did not end"
call lead; p=$(pair)
[ "$RC" = 0 ] && [ "$p" = "$M" ] || fail "pass f: the call after the installer ended left the pair at $p (exit $RC)"


# spec: specs/ap/deliver.md#R18
# (g) A close records no refused tarball, and a rollback rechecks a recorded one.
M=$(merge); : > "$W/invalidate"; call lead; rm "$W/invalidate"
[ "$RC" = 0 ] && [ "$(pair)" = "$M" ] && [ "$(member 'r["rollback_content"]')" = null ] || fail "pass g: a close recorded a refused tarball"
M=$(merge); call lead
[ "$RC" = 0 ] && [ "$(pair)" = "$M" ] || fail "pass g: install before rollback"
TAR=$(member 'r["rollback_content"]'); [ "$TAR" != null ] || fail "pass g: no recorded tarball"
rm "$TAR.sha256"
call rollback
[ "$RC" = 2 ] && [ "$(member 'r["code"]')" = nothing_to_roll_back ] && [ "$(pair)" = "$M" ] || fail "pass g: rollback used a refused tarball"
line g 0 "$(pair)" nothing_to_roll_back

# spec: specs/ap/deliver.md#R26
# (h) Real build-lock contention: release, timeout, and a killed waiter.
python3 -I - "$W/lead.json" "$W/wait.json" <<'PYWAIT'
import json, sys
r = json.load(open(sys.argv[1])); r['build_wait_seconds'] = 15
json.dump(r, open(sys.argv[2], 'w'))
PYWAIT
hold_build() {
  rm -f "$W/holding" "$W/unlock"
  setsid flock -x "$W/build.lock" sh -c ': > "$WALK/holding"; while [ ! -e "$WALK/unlock" ]; do sleep 0.05; done' &
  holder=$!
  for _ in $(seq 1 100); do [ -e "$W/holding" ] && return; sleep 0.05; done
  fail "pass h: holder did not take the lock"
}
waiting() {
  python3 -I - "$1" "$2" <<'PYWAIT'
import datetime, json, sys
lines = open(sys.argv[1]).read().splitlines()
assert len(lines) == 1, lines
r = json.loads(lines[0]); assert set(r) == {'at', 'waiting', 'seconds'}, r
assert r['waiting'] == 'build_lock' and r['seconds'] == int(sys.argv[2]), r
assert len(r['at']) == 20 and r['at'].endswith('Z'), r
datetime.datetime.strptime(r['at'], '%Y-%m-%dT%H:%M:%SZ')
PYWAIT
}
attempts() {
  printf '{"object_type":"install","where":{"all":[{"eq":["commit","%s"]}]},"limit":200}\n' "$M" | ont search |
    python3 -I -c 'import json, sys; r=json.load(sys.stdin); assert r["status"] == "ok", r; print(len(r["result"]["objects"]))'
}
start_waiter() {
  setsid "$AP" deliver < "$W/wait.json" > "$W/wait.out" 2> "$W/wait.err" &
  waiter=$!
  for _ in $(seq 1 200); do [ -s "$W/wait.err" ] && return; sleep 0.05; done
  fail "pass h: call did not wait"
}
release_build() { : > "$W/unlock"; wait "$holder"; holder=; }
M=$(merge); hold_build; start_waiter
sleep 2; release_build
RC=0; wait "$waiter" || RC=$?; waiter=
cp "$W/wait.out" "$W/out.json"; cp "$W/wait.err" "$W/err"
[ "$RC" = 0 ] && [ "$(member 'r["end"]')" = installed ] && [ "$(pair)" = "$M" ] || fail "pass h k=1: exit $RC"
waiting "$W/err" 15 || fail "pass h k=1: waiting line"
[ "$(wc -l < "$W/out.json")" = 1 ] || fail "pass h k=1: stdout is not one line"
line h 1 "$(pair)" installed

M=$(merge); BEFORE=$(pair)
python3 -I - "$W/wait.json" "$W/bound.json" <<'PYWAIT'
import json, sys
r=json.load(open(sys.argv[1])); r['build_wait_seconds']=1; json.dump(r,open(sys.argv[2],'w'))
PYWAIT
hold_build; call bound; release_build
[ "$RC" = 2 ] && [ "$(member 'r["code"]')" = build_busy ] && [ "$(pair)" = "$BEFORE" ] && [ "$(attempts)" = 0 ] || fail "pass h k=2: exit $RC"
waiting "$W/err" 1 || fail "pass h k=2: waiting line"
line h 2 "$(pair)" build_busy

hold_build; start_waiter
waiting "$W/wait.err" 15 || fail "pass h k=3: did not wait"
[ ! -s "$W/wait.out" ] && [ "$(pair)" = "$BEFORE" ] && [ "$(attempts)" = 0 ] || fail "pass h k=3: effect while waiting"
call lead
[ "$RC" = 2 ] && [ "$(member 'r["code"]')" = busy ] || fail "pass h k=3: second call is not busy"
kill -KILL -- "-$waiter"; wait "$waiter" 2>/dev/null || true; waiter=
release_build
call lead
[ "$RC" = 0 ] && [ "$(member 'r["end"]')" = installed ] && [ "$(pair)" = "$M" ] && [ "$(attempts)" = 1 ] || fail "pass h k=3: call after killed waiter"
[ ! -s "$W/err" ] || fail "pass h k=3: free lock printed a waiting line"
line h 3 "$(pair)" installed

# spec: specs/ap/deliver.md#R43
# (i) The engine: a scratch engine repository at E0 (its fake installer) and one accepted merge E1; the
# engine file answers E0. A merge moves ontology/ENGINE-PIN to E1; one call installs the engine and ends,
# the next installs the pair.
E=$W/engine-origin
mkdir -p "$E/scripts"
cat > "$E/scripts/install-ont.sh" <<'EOF'
#!/bin/bash
# The walk's engine installer: the real script's fetch and ancestor check, then the file answers the pin.
set -euo pipefail
REPO_ROOT=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
BIN=$ONT_INSTALL_BIN_DIR/ont; KEEP=$ONT_INSTALL_STATE_DIR/install
case ${1:-} in
  install)
    git -C "$REPO_ROOT" fetch origin +refs/heads/main:refs/remotes/origin/main --refmap= || { echo "refused: git_unavailable: fetch origin failed" >&2; exit 2; }
    git -C "$REPO_ROOT" merge-base --is-ancestor "$2" refs/remotes/origin/main || { echo "refused: commit_not_merged" >&2; exit 2; }
    mkdir -p "$KEEP"; sha=$(sha256sum "$BIN" | awk '{print $1}')
    cp "$BIN" "$KEEP/ont.before-${sha:0:12}"; echo "$sha" > "$KEEP/previous"
    sed "s/^REV=.*/REV=$2/" "$BIN" > "$BIN.new"; chmod 0755 "$BIN.new"; mv "$BIN.new" "$BIN" ;;
  rollback)
    sha=$(cat "$KEEP/previous"); install -m 0755 "$KEEP/ont.before-${sha:0:12}" "$BIN.new"; mv "$BIN.new" "$BIN" ;;
  *) exit 2 ;;
esac
EOF
git -C "$E" init -q -b main
git -C "$E" add -A && git -C "$E" commit -q -m "engine 0"
E0=$(git -C "$E" rev-parse HEAD)
git -C "$E" checkout -q -b e1 && echo 1 > "$E/VERSION" && git -C "$E" add -A && git -C "$E" commit -q -m "engine change"
EH=$(git -C "$E" rev-parse HEAD)
git -C "$E" checkout -q main && git -C "$E" merge -q --no-ff -m "engine merge" e1
E1=$(git -C "$E" rev-parse HEAD)
accept "$EH" walk/engine
printf '#!/bin/sh\nREV=%s\nif [ "$1" = --version ]; then echo "ont 0.1.0 $REV"; exit 0; fi\nexec %q --store %q "$@"\n' "$E0" "$ONT" "$STORE" > "$ONT_INSTALL_BIN_DIR/ont"
chmod 0755 "$ONT_INSTALL_BIN_DIR/ont"
echo "$E1" > "$W/next-pin"; M=$(merge); rm "$W/next-pin"
BEFORE=$(pair)
call engine
[ "$RC" = 0 ] && [ "$(member 'r["target"]')" = /usr/local/bin/ont ] && [ "$(member 'r["end"]')" = installed ] &&
  [ "$(member 'r["to"]')" = "$E1" ] && [ "$("$ONT_INSTALL_BIN_DIR/ont" --version)" = "ont 0.1.0 $E1" ] &&
  [ "$(pair)" = "$BEFORE" ] || fail "pass i: the engine (exit $RC)"
printf '{"object_type":"install","where":{"all":[{"eq":["commit","%s"]}]},"limit":10}\n' "$E1" | ont search |
  python3 -I -c 'import json, sys; o = json.load(sys.stdin)["result"]["objects"]; f = {k: v.get("value") for k, v in o[0]["head"]["fields"].items()}
assert len(o) == 1 and f["target"]["native_key"] == "/usr/local/bin/ont" and f["outcome"] == "installed" and f["repository"] == "walk/engine", f' ||
  fail "pass i: the engine's attempt"
# The engine's clone has the remote origin: the installer's own fetch, run again here, answers.
CE=$W/state/engine
[ "$(git -C "$CE" remote get-url origin)" = "$E" ] || fail "pass i: the clone's remote"
git -C "$CE" fetch origin +refs/heads/main:refs/remotes/origin/main --refmap= || fail "pass i: the installer's fetch"
line i 0 "$(pair)" "$(member 'r["outcome"]')"
call engine
[ "$RC" = 0 ] && [ "$(member 'r["target"]')" = /usr/local/bin/ap ] && [ "$(pair)" = "$M" ] || fail "pass i: the pair after the engine (exit $RC)"
line i 1 "$(pair)" "$(member 'r["outcome"]')"

# spec: specs/ap/deliver.md#R44
# spec: specs/ap/deliver.md#R45
# (j) A merged higher version of the work schema with an `ontology/migrate`: one call backs the store up,
# installs the schema, runs the migration with the store's path and the two versions, installs the pair.
python3 -I -c 'import json, sys; d = json.load(open(sys.argv[1])); d["version"] += 1; open(sys.argv[2], "w").write(json.dumps(d, indent=1) + "\n")' \
  "$O/ontology/work.schema.json" "$W/next-schema"
printf '#!/bin/sh\n[ "$#" = 3 ] || exit 2\n[ -e "$WALK/state/migrate.pending" ] || exit 3\nprintf "%%s %%s %%s\\n" "$1" "$2" "$3" >> "$WALK/migrated"\n' > "$W/next-migrate"
version() { printf '{"object_type":"install","limit":1}\n' | ont search | python3 -I -c 'import json, sys; print(json.load(sys.stdin)["schema"]["work"]["version"])'; }
V0=$(version)
M=$(merge); rm "$W/next-schema" "$W/next-migrate"
call engine
[ "$RC" = 0 ] && [ "$(member 'r["end"]')" = installed ] && [ "$(pair)" = "$M" ] || fail "pass j: exit $RC"
[ "$(version)" = $((V0 + 1)) ] || fail "pass j: the store's version"
[ "$(cat "$W/migrated")" = "$STORE $V0 $((V0 + 1))" ] || fail "pass j: the migration's arguments"
[ ! -e "$W/state/migrate.pending" ] || fail "pass j: the marker stayed"
python3 -I -c 'import json, os, sys; s = json.load(open(sys.argv[1]))["schema"]; v = int(sys.argv[2])
assert s["from"] == v and s["to"] == v + 1 and s["migrate"] == "ok" and os.path.isfile(s["backup"]), s' "$W/out.json" "$V0" ||
  fail "pass j: the record's schema"
line j 0 "$(pair)" "$(member 'r["schema"]["migrate"]')"

[ "$(host_count)" = "$host_before" ] || fail "the host store's install objects changed"
echo "deliver-walk: ok"
