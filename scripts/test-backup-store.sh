#!/usr/bin/env bash
# Test of scripts/backup-store.sh with a fake `ont` and a fake `rclone` on PATH, every path under one temporary
# directory: each exit and each `code`, the name form, the pruning of a remote that holds foreign files, an
# unchanged digest, a busy lock, the settings refusals, and last.json after a kill. Run by ./check.
set -uo pipefail
script=${1:-$(cd "$(dirname "$0")" && pwd -P)/backup-store.sh}
t=$(mktemp -d); trap 'rm -rf -- "$t"' EXIT
fake=$t/bin; mkdir -p "$fake" "$t/remote/r"
# fake ont: `--store S backup` copies S into $t/backups/ and answers like the engine; ONT_FAIL=1 refuses.
cat > "$fake/ont" <<'FAKE'
#!/usr/bin/env bash
[ "$1" = --store ] && [ "$3" = backup ] && [ "$#" -eq 3 ] || { echo "fake ont: unexpected: $*" >> "$FAKE_LOG"; exit 9; }
echo "ont $*" >> "$FAKE_LOG"
[ -z "${ONT_FAIL:-}" ] || { echo '{"code":"store_not_found","status":"refused"}'; exit 2; }
mkdir -p "$FAKE_DIR/backups"; n=$(ls "$FAKE_DIR/backups" | wc -l); f=$FAKE_DIR/backups/ontology-$n.sqlite
cp "$2" "$f"; s=$(sha256sum "$f" | cut -d' ' -f1)
printf '{"result":{"edits_log":"written","path":"%s","retention_failure":%s,"sha256":"%s"},"status":"ok"}\n' "$f" "${ONT_RETENTION:-null}" "$s"
FAKE
# fake rclone: the remote `r:dir` is the directory $t/remote/r/dir; it prints a secret on both streams.
cat > "$fake/rclone" <<'FAKE'
#!/usr/bin/env bash
verb=$1; echo "rclone $*" >> "$FAKE_LOG"; echo SECRET_TOKEN; echo SECRET_TOKEN >&2
dir() { printf '%s/remote/%s' "$FAKE_DIR" "${1/:/\/}"; }
[ "${RCLONE_FAIL:-}" != "$verb" ] || exit 1
case "$verb" in
  copyto) cp "$2" "$(dir "${3%/*}")/${3##*/}" ;;
  lsf) [ -z "${KILL_PIDFILE:-}" ] || kill -9 "$(cat "$KILL_PIDFILE")"; ls -p "$(dir "$2")" | { [ -z "${LSF_HIDE:-}" ] && cat || grep -v "$LSF_HIDE"; }
       [ -z "${LSF_EXTRA:-}" ] || seq 1 "$LSF_EXTRA" | sed 's/^/zz-foreign-name-of-some-length-/' ;;
  deletefile) rm "$(dir "${2%/*}")/${2##*/}" ;;
  *) exit 9 ;;
esac
FAKE
chmod +x "$fake/ont" "$fake/rclone"
export FAKE_DIR=$t FAKE_LOG=$t/calls.log PATH="$fake:$PATH"
fails=0; ok() { echo "ok   $1"; }; bad() { echo "FAIL $1"; fails=$((fails+1)); }
state=$t/state\ dir; store=$t/my\ store.sqlite; echo one > "$store"
conf() { printf '{"store":"%s","state":"%s"%s}\n' "$store" "$state" "${1:-}" > "$t/c.json"; }
run() { AP_BACKUP_CONFIG=$t/c.json bash "$script" > "$t/out" 2> "$t/err"; rc=$?; }
rec() { python3 -I -c 'import json, sys; r = json.load(open(sys.argv[1])); v = eval(sys.argv[2], {"r": r}); print("null" if v is None else v)' "$state/last.json" "$1"; }
calls() { grep -c "^rclone $1" "$t/calls.log" || true; }
remote_files() { ls "$t/remote/r/dir" | tr '\n' ' '; }
mkdir -p "$t/remote/r/dir"; conf ',"remote":"r:dir"'

# a remote copy: name form, record, exit 0, only the three verbs, no secret anywhere.
run; name=$(rec 'r["remote_name"]')
{ [ "$rc" -eq 0 ] && [[ "$name" =~ ^[0-9]{8}T[0-9]{6}Z-[0-9a-f]{12}\.sqlite$ ]] && [ "$(rec 'r["remote"]')" = copied ] && [ "$(rec 'r["code"]')" = null ] \
  && [ "$(remote_files)" = "$name " ] && cmp -s "$t/remote/r/dir/$name" "$store"; } && ok "copied: exit 0, name form <time>-<12 of digest>.sqlite, the remote holds the bytes" || bad "copied rc=$rc $(cat "$t/err" "$t/out")"
sha=$(rec 'r["sha256"]'); path=$(rec 'r["backup"]')
{ [[ "$name" == *"-${sha:0:12}.sqlite" ]] && [ "$(sha256sum < "$path" | cut -d' ' -f1)" = "$sha" ] && [ "$(rec 'r["bytes"]')" = "$(stat -c %s "$path")" ] \
  && [ "$(cat "$t/out")" = "backup: ok $path" ] && [ "$(rec 'r["retention_failure"]')" = null ] && [ "$(rec 'r["deleted"]')" = '[]' ]; } && ok "record: digest, bytes, path, line 'backup: ok <path>'" || bad "record $(cat "$state/last.json")"
! grep -rq SECRET_TOKEN "$t/out" "$t/err" "$state" && ! grep -v '^rclone \(copyto\|lsf\|deletefile\) \|^ont ' "$t/calls.log" | grep -q . && ok "no credential in output or record; only copyto, lsf, deletefile and ont backup run" || bad "secret or verb"

# an unchanged store: the digest equals the previous copied one: nothing copied or deleted.
: > "$t/calls.log"; run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = unchanged ] && [ "$(rec 'r["remote_name"]')" = null ] && [ "$(calls copyto)" -eq 0 ] && [ "$(calls lsf)" -eq 0 ] \
  && [ "$(calls deletefile)" -eq 0 ] && [ "$(remote_files)" = "$name " ] && [ "$(rec 'r["sha256"]')" = "$sha" ]; } && ok "unchanged digest: exit 0, remote unchanged, nothing copied or deleted" || bad "unchanged rc=$rc"
run; [ "$(rec 'r["remote"]')" = unchanged ] && ok "a second unchanged run stays unchanged" || bad "second unchanged"
echo two > "$store"; run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = copied ] && [ "$(rec 'r["sha256"]')" != "$sha" ]; } && ok "a changed store is copied again" || bad "changed rc=$rc"

# pruning: sixteen files of the script's form and foreign names; the newest fourteen of the form stay, foreigners stay.
rm -rf "$t/remote/r/dir"; mkdir -p "$t/remote/r/dir/subdir"; cd "$t/remote/r/dir"
for i in $(seq 1 16); do : > "$(printf '2020%02d01T000000Z-0123456789ab.sqlite' "$i")"; done
: > 'notes.txt'; : > '20200101T000000Z-short.sqlite'; : > 'a b.sqlite'; : > '$(touch pwn).sqlite'; cd "$OLDPWD"
echo three > "$store"; run; name=$(rec 'r["remote_name"]')
form=$(ls "$t/remote/r/dir" | grep -cE '^[0-9]{8}T[0-9]{6}Z-[0-9a-f]{12}\.sqlite$')
{ [ "$rc" -eq 0 ] && [ "$form" -eq 14 ] && [ -e "$t/remote/r/dir/$name" ] && [ -e "$t/remote/r/dir/notes.txt" ] && [ -e "$t/remote/r/dir/a b.sqlite" ] \
  && [ -e "$t/remote/r/dir/\$(touch pwn).sqlite" ] && [ -e "$t/remote/r/dir/20200101T000000Z-short.sqlite" ] && [ -d "$t/remote/r/dir/subdir" ] \
  && [ ! -e "$t/remote/r/dir/20200101T000000Z-0123456789ab.sqlite" ] && [ -e "$t/remote/r/dir/20200401T000000Z-0123456789ab.sqlite" ] \
  && [ ! -e "$t/remote/r/dir/pwn" ] && [ ! -e "$PWD/pwn" ] \
  && [ "$(rec 'r["deleted"]')" = "['20200101T000000Z-0123456789ab.sqlite', '20200201T000000Z-0123456789ab.sqlite', '20200301T000000Z-0123456789ab.sqlite']" ]; } \
  && ok "pruning: 16 old of the form + 2 foreign (and odd) names: newest 14 stay, only the 3 oldest of the form deleted, foreign kept" || bad "prune rc=$rc form=$form $(cat "$state/last.json")"
# a keep of 1 keeps only the new file of the form.
sleep 1; conf ',"remote":"r:dir","keep":1'; echo four > "$store"; run
{ [ "$rc" -eq 0 ] && [ "$(ls "$t/remote/r/dir" | grep -cE '^[0-9]{8}T[0-9]{6}Z-[0-9a-f]{12}\.sqlite$')" -eq 1 ] && [ -e "$t/remote/r/dir/notes.txt" ]; } && ok "keep 1 leaves the newest file of the form" || bad "keep 1 rc=$rc"

# a listing of more than 64 KiB after the new name: the name is still found (no SIGPIPE of the match), the run ends 0 and prunes.
conf ',"remote":"r:dir","keep":2'; echo big > "$store"
LSF_EXTRA=4000 run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = copied ] && [ "$(rec 'r["code"]')" = null ] && [ "$(ls "$t/remote/r/dir" | grep -cE '^[0-9]{8}T[0-9]{6}Z-[0-9a-f]{12}\.sqlite$')" -le 2 ]; } \
  && ok "a remote listing over 64 KiB: the name is found, exit 0, pruned" || bad "big listing rc=$rc $(cat "$t/out")"

# no remote: exit 0, remote none, no rclone call.
conf ""; : > "$t/calls.log"; run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = none ] && [ "$(calls '')" -eq 0 ] && [ "$(rec 'r["sha256"]')" != null ]; } && ok "no remote: exit 0, remote none, rclone never runs" || bad "none rc=$rc"
conf ',"remote":""'; : > "$t/calls.log"; run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = none ] && [ "$(calls '')" -eq 0 ]; } && ok "an empty remote is no remote" || bad "empty remote rc=$rc"
ONT_RETENTION='"prune_failed"' run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["retention_failure"]')" = prune_failed ]; } && ok "retention_failure is recorded as the engine answered" || bad "retention"

# the four failures of the remote step and the failed backup.
conf ',"remote":"r:dir"'; echo five > "$store"
for pair in "copyto remote_copy_failed" "lsf remote_list_failed" "deletefile remote_prune_failed"; do
  set -- $pair; echo "x$1" > "$store"; : > "$t/calls.log"
  [ "$1" = deletefile ] && for i in 1 2 3; do : > "$t/remote/r/dir/2020010${i}T000000Z-0123456789ab.sqlite"; done
  [ "$1" = deletefile ] && conf ',"remote":"r:dir","keep":1'
  RCLONE_FAIL=$1 run; p=$(rec 'r["backup"]')
  { [ "$rc" -eq 3 ] && [ "$(rec 'r["code"]')" = "$2" ] && [ "$(rec 'r["remote"]')" = failed ] && [ -f "$p" ] && [ "$(cat "$t/out")" = "backup: $2 $p" ]; } \
    && ok "$2: exit 3, the local backup stays, line 'backup: $2 <path>'" || bad "$2 rc=$rc $(cat "$t/out" "$t/err")"
  conf ',"remote":"r:dir"'
done
echo hide > "$store"; LSF_HIDE='sqlite$' run
{ [ "$rc" -eq 3 ] && [ "$(rec 'r["code"]')" = remote_list_failed ]; } && ok "remote_list_failed: the listing lacks the new name: exit 3" || bad "list hides rc=$rc"
# after a failed remote step the same digest is copied again (a failed record is not a previous copy).
: > "$t/calls.log"; run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = copied ] && [ "$(calls copyto)" -eq 1 ]; } && ok "a digest whose remote step failed is copied on the next run" || bad "retry rc=$rc"
: > "$t/calls.log"; ONT_FAIL=1 run
{ [ "$rc" -eq 4 ] && [ "$(rec 'r["code"]')" = backup_failed ] && [ "$(rec 'r["backup"]')" = null ] && [ "$(cat "$t/out")" = "backup: backup_failed -" ] && [ "$(calls '')" -eq 0 ]; } \
  && ok "backup_failed: exit 4, line 'backup: backup_failed -', nothing copied or deleted" || bad "backup_failed rc=$rc"
! grep -q '^ont .*\(compact\|schema\|action\)' "$t/calls.log" && ok "the script runs only 'ont --store S backup'" || bad "other ont verb"

# a busy lock: exit 0, `backup: busy`, the record untouched.
cp "$state/last.json" "$t/before.json"; : > "$t/calls.log"
flock "$state/lock" sleep 20 & holder=$!; sleep 0.5; run; pkill -P "$holder"; kill "$holder" 2> /dev/null; wait "$holder" 2> /dev/null
{ [ "$rc" -eq 0 ] && [ "$(cat "$t/out")" = "backup: busy" ] && cmp -s "$state/last.json" "$t/before.json" && [ ! -s "$t/calls.log" ]; } && ok "busy lock: exit 0, 'backup: busy', nothing run, record untouched" || bad "busy rc=$rc $(cat "$t/out")"

# a kill between the copy and the listing: last.json is the previous record, whole; with no previous one, absent (never partial).
echo kill1 > "$store"; run; cp "$state/last.json" "$t/before.json"; echo kill2 > "$store"
krc=$( (KILL_PIDFILE=$t/pid AP_BACKUP_CONFIG=$t/c.json bash -c 'echo $$ > "$1"; exec bash "$2"' x "$t/pid" "$script" > /dev/null 2>&1; echo $?) 2> /dev/null)
{ [ "$krc" -eq 137 ] && cmp -s "$state/last.json" "$t/before.json" && python3 -I -c 'import json, sys; json.load(open(sys.argv[1]))' "$state/last.json" \
  && [ -z "$(ls -A "$state" | grep '^\.last')" ]; } && ok "kill between copy and listing: last.json is the previous record, whole" || bad "kill rc=$krc"
run; { [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = copied ]; } && ok "the run after a kill recovers (the lock is free, the digest is copied)" || bad "after kill rc=$rc"
rm -rf "$state"; echo kill3 > "$store"
: $( (KILL_PIDFILE=$t/pid AP_BACKUP_CONFIG=$t/c.json bash -c 'echo $$ > "$1"; exec bash "$2"' x "$t/pid" "$script" > /dev/null 2>&1; echo $?) 2> /dev/null)
[ ! -e "$state/last.json" ] && [ -z "$(ls -A "$state" | grep '^\.last')" ] && ok "kill on the first run: no last.json and no partial file" || bad "kill first run"

# settings that are refused: exit 2 before the lock; nothing runs, no record, nothing under a bad path.
rm -rf "$state"; : > "$t/calls.log"
for bad_conf in ',"remote":"r:../x"' ',"remote":"r:dir; touch pwn"' ',"remote":"-r:dir"' ',"remote":"r:dir x"' ',"remote":"dir"' ',"remote":"r:$(id)"' ',"remote":5' ',"keep":0' ',"keep":61' ',"keep":"5"' ',"keep":true' ',"keep":1.5'; do
  conf ",\"remote\":\"r:dir\"$bad_conf"; case "$bad_conf" in *remote*) conf "$bad_conf" ;; esac
  run; { [ "$rc" -eq 2 ] && [ ! -e "$state" ] && [ ! -s "$t/calls.log" ] && [ ! -e "$PWD/pwn" ] && [ -z "$(cat "$t/out")" ]; } || bad "refuse $bad_conf rc=$rc"
done; ok "refused settings (shell characters, spaces, a leading dash, no colon, wrong types, keep out of 1..60): exit 2, nothing run"
echo '{"store":"relative","state":"'"$state"'"}' > "$t/c.json"; run; [ "$rc" -eq 2 ] && ok "a relative path is refused" || bad "relative rc=$rc"
echo 'not json' > "$t/c.json"; run; [ "$rc" -eq 2 ] && ok "a settings file that is not JSON is refused" || bad "not json rc=$rc"
AP_BACKUP_CONFIG= bash "$script" > /dev/null 2>&1; [ "$?" -eq 2 ] && ok "an empty AP_BACKUP_CONFIG is refused" || bad "empty config var"
[ "$(ls "$t"/state* 2> /dev/null | wc -l)" -eq 0 ] || bad "a refusal made a state directory"
[ "$fails" -eq 0 ] && echo "test-backup-store: PASS" || { echo "test-backup-store: $fails FAILED"; exit 1; }
