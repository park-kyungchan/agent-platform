#!/usr/bin/env bash
# The walk of `ap deliver` on the real installer scripts (crates/ap/README.md, "Delivery").
#   scripts/test-deliver.sh <ont binary> <ap binary>
# It runs the built `ap deliver`, kills the process group of `ap` itself at each rename of the content
# installer, and checks that the next call leaves the pair at one commit (`sha256sum -c`). It fabricates
# no attempt. Every path it touches lies under one directory made with `mktemp -d` and removed at the end:
# a scratch origin repository, a scratch store reached through a wrapper named `ont` first on PATH, every
# installer variable and AP_DELIVER_LOCK. It never calls systemctl. The origin's install-content.sh is
# this repository's file unchanged; its install-ap.sh differs only in the build lock's path; a `cargo`
# shim writes a file that answers --version with the commit it is built from.
# One line `walk pass=<a..f> k=<k> pair=<commit or neither> outcome=<outcome>` per case, then
# `deliver-walk: ok`. Under CI=true it is skipped: the engine there has no object for the target's key.
set -euo pipefail
[ "$#" -eq 2 ] || { echo "usage: scripts/test-deliver.sh <ont binary> <ap binary>" >&2; exit 2; }
if [ "${CI:-}" = true ]; then echo "deliver-walk: skipped (CI)"; exit 0; fi
ONT=$(realpath -- "$1"); AP=$(realpath -- "$2")
REPO=$(cd -- "$(dirname -- "$(realpath "$0")")/.." && pwd -P)
MV=$(command -v mv)
W=$(mktemp -d)
trap 'rm -rf -- "$W"' EXIT
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
unset INVOCATION_ID CARGO_TARGET_DIR
export WALK=$W WALK_MV=$MV

# The wrapper `ont`: the built ap has no other way to a scratch store.
printf '#!/bin/sh\nexec %q --store %q "$@"\n' "$ONT" "$W/store/store.sqlite" > "$W/bin/ont"
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
FILES="AGENTS.md runtime-references/walk.md skills/a/SKILL.md skills/b/SKILL.md"
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
  git -C "$O" commit -q -am "change $n"
  local head; head=$(git -C "$O" rev-parse HEAD)
  git -C "$O" checkout -q main
  git -C "$O" merge -q --no-ff -m "merge $n" "b$n"
  printf 'Verdict: accept\n%s\n' "$head" > "$W/reports/$head.md"
  python3 -I - "$head" "$W" <<'PY'
import hashlib, json, subprocess, sys
head, w = sys.argv[1:]
def apply(action, rid, p):
    body = dict(action_type=action, request_id=rid, agent='walk', parameters=p)
    out = subprocess.run(['ont', 'action', 'apply', '--client', 'walk'], input=json.dumps(body), capture_output=True, text=True, check=True).stdout
    v = json.loads(out); assert v['result']['outcome'] == 'applied', v
    return v['result']['edits'][0]['id']
task = dict(object_type='task', namespace=dict(instance='vps://palantirkc', account='root'), native_key='walk')
review = apply('start_review', 'rv-' + head, dict(task=task, kind='packet', repository='walk/origin', head=head, model='walk', effort='walk', prompt=w + '/reports/prompt.md', prompt_sha256='b' * 64))
report = w + '/reports/' + head + '.md'
apply('close_review', 'rv-' + head + '-close', dict(review=review, verdict='accept', report=report, report_sha256=hashlib.sha256(open(report, 'rb').read()).hexdigest()))
PY
  git -C "$O" rev-parse HEAD
}

# The requests: a Lead's install, a Lead's rollback.
request() {
  python3 -I -c 'import json, sys; w = sys.argv[1]; r = dict(repository="walk/origin", url=w + "/origin", state=w + "/state", build_lock=w + "/build.lock", by="walk", reason="the walk")
if sys.argv[2] == "rollback": r["rollback"] = True
print(json.dumps(r))' "$W" "$1" > "$W/$1.json"
}
request lead; request rollback
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

# (b) For every k, highest first: an install killed after the k-th rename, then one call, which repairs
# the dead attempt or closes it by the read; the last one (k=1) is repaired and rolled back in (e).
for k in $(seq "$N" -1 1); do
  M=$(merge); killed lead "$k"
  call lead; p=$(pair)
  [ "$p" = "$M" ] && [ "$RC" = 0 ] || fail "pass b k=$k: pair $p, exit $RC"
  line b "$k" "$p" "$(member 'r["closed"]["outcome"]')"
done

# (e) A rollback of the attempt pass b repaired: the oldest tarball brings the tree to its from.
FROM=$(git -C "$O" rev-parse "$M^1")
echo 0 > "$W/count"; call rollback; NRB=$(cat "$W/count"); rm -f "$W/count"
p=$(pair)
[ "$RC" = 0 ] && [ "$(member 'r["end"]')" = installed ] && [ "$p" = "$FROM" ] || fail "pass e: pair $p, exit $RC"
line e 1 "$p" "$(member 'r["outcome"]')"

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

[ "$(host_count)" = "$host_before" ] || fail "the host store's install objects changed"
echo "deliver-walk: ok"
