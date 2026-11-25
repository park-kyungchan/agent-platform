#!/usr/bin/env bash
# Test of scripts/backup-store.sh with a fake `ont`, a fake `rclone` and (but for the round trip) a fake `gpg` on PATH,
# every path under one temporary directory: each exit and each `code`, the name form, the encryption and the rule that
# a plain file never leaves, the pruning of a remote that holds foreign files and both name forms, an unchanged digest
# and key, a busy lock, the settings refusals, the working files, and last.json after a kill. The round trip makes its
# own key pair with the real gpg of the PATH the test started with (skipped, and printed, when there is none).
# Run by ./check.
set -uo pipefail
script=${1:-$(cd "$(dirname "$0")" && pwd -P)/backup-store.sh}
origpath=$PATH; realgpg=$(command -v gpg || true)
t=$(mktemp -d); gh=""
cleanup() {
  if [ -n "$gh" ]; then gpgconf --homedir "$gh" --kill all > /dev/null 2>&1; rm -rf -- "$gh"; fi
  rm -rf -- "$t"
}
trap cleanup EXIT
fake=$t/bin; fakegpg=$t/fakegpg; mkdir -p "$fake" "$fakegpg" "$t/remote/r" "$t/home"
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
  copyto) [ "$(head -c 15 "$2")" != "SQLite format 3" ] || echo PLAIN_COPIED >> "$FAKE_LOG"; cp "$2" "$(dir "${3%/*}")/${3##*/}" ;;
  lsf) [ -z "${KILL_PIDFILE:-}" ] || kill -9 "$(cat "$KILL_PIDFILE")"; ls -p "$(dir "$2")" | { [ -z "${LSF_HIDE:-}" ] && cat || grep -v "$LSF_HIDE"; }
       [ -z "${LSF_EXTRA:-}" ] || seq 1 "$LSF_EXTRA" | sed 's/^/zz-foreign-name-of-some-length-/' ;;
  deletefile) rm "$(dir "${2%/*}")/${2##*/}" ;;
  *) exit 9 ;;
esac
FAKE
# fake gpg: writes FAKEPGP, the digest of the recipient file and the input to --output; it prints a secret on both
# streams and needs its --homedir to exist. GPG_MODE: copy (the input as it is), empty (exit 0, no output), emptyfile
# (exit 0, an empty output), fail (exit 1).
cat > "$fakegpg/gpg" <<'FAKE'
#!/usr/bin/env bash
echo "gpg $*" >> "$FAKE_LOG"; echo SECRET_TOKEN; echo SECRET_TOKEN >&2
args=("$@"); out=""; rf=""; home=""
for (( i = 0; i < $#; i++ )); do
  case "${args[i]}" in --output) out=${args[i+1]} ;; --recipient-file) rf=${args[i+1]} ;; --homedir) home=${args[i+1]} ;; esac
done
in=${args[$#-1]}
[ -d "$home" ] && [ -f "$rf" ] && [ -f "$in" ] || { echo "fake gpg: unexpected: $*" >> "$FAKE_LOG"; exit 9; }
case "${GPG_MODE:-normal}" in
  copy) cp "$in" "$out" ;;
  empty) ;;
  emptyfile) : > "$out" ;;
  fail) exit 1 ;;
  *) { echo "FAKEPGP $(sha256sum < "$rf" | cut -d' ' -f1)"; cat "$in"; } > "$out" ;;
esac
FAKE
chmod +x "$fake/ont" "$fake/rclone" "$fakegpg/gpg"
# the home of the user running the script must stay untouched: HOME is an empty directory, GNUPGHOME names nothing.
export FAKE_DIR=$t FAKE_LOG=$t/calls.log PATH="$fake:$fakegpg:$PATH" HOME=$t/home GNUPGHOME=$t/home-gnupg-env
fails=0; ok() { echo "ok   $1"; }; bad() { echo "FAIL $1"; fails=$((fails+1)); }
state=$t/state\ dir; store=$t/my\ store.sqlite; echo one > "$store"
conf() { printf '{"store":"%s","state":"%s"%s}\n' "$store" "$state" "${1:-}" > "$t/c.json"; }
run() { AP_BACKUP_CONFIG=$t/c.json bash "$script" > "$t/out" 2> "$t/err"; rc=$?; }
rec() { python3 -I -c 'import json, sys; r = json.load(open(sys.argv[1])); v = eval(sys.argv[2], {"r": r}); print("null" if v is None else v)' "$state/last.json" "$1"; }
calls() { grep -c "^rclone $1" "$t/calls.log" || true; }
remote_files() { ls "$t/remote/r/dir" | tr '\n' ' '; }
rcp=$t/recipient.pub; echo PUBKEY-ONE > "$rcp"; rcp_sha=$(sha256sum < "$rcp" | cut -d' ' -f1)
R=',"remote":"r:dir","recipient":"'$rcp'"'
gcalls() { grep -c '^gpg ' "$t/calls.log" || true; }
nothing_left() { ! grep -q '^\(gpg\|rclone\) ' "$t/calls.log"; }
state_clean() { [ "$(ls -A "$state" | tr '\n' ' ')" = "last.json lock " ]; }
mkdir -p "$t/remote/r/dir"; conf "$R"

# a remote copy: name form, record, exit 0, only the three verbs, no secret anywhere.
run; name=$(rec 'r["remote_name"]')
# (changed from the base: the name ends .sqlite.gpg, and the remote file is the encryption of the store, not its bytes.)
sha=$(rec 'r["sha256"]'); path=$(rec 'r["backup"]')
{ [ "$rc" -eq 0 ] && [[ "$name" =~ ^[0-9]{8}T[0-9]{6}Z-[0-9a-f]{12}\.sqlite\.gpg$ ]] && [ "$(rec 'r["remote"]')" = copied ] && [ "$(rec 'r["code"]')" = null ] \
  && [ "$(remote_files)" = "$name " ] && ! cmp -s "$t/remote/r/dir/$name" "$store" && [ "$(head -n 1 "$t/remote/r/dir/$name")" = "FAKEPGP $rcp_sha" ] \
  && tail -n +2 "$t/remote/r/dir/$name" | cmp -s - "$store"; } && ok "copied: exit 0, name form <time>-<12 of digest>.sqlite.gpg, the remote holds the encryption of the bytes" || bad "copied rc=$rc $(cat "$t/err" "$t/out")"
{ [[ "$name" == *"-${sha:0:12}.sqlite.gpg" ]] && [ "$(sha256sum < "$path" | cut -d' ' -f1)" = "$sha" ] && [ "$(rec 'r["bytes"]')" = "$(stat -c %s "$path")" ] \
  && [ "$(cat "$t/out")" = "backup: ok $path" ] && [ "$(rec 'r["retention_failure"]')" = null ] && [ "$(rec 'r["deleted"]')" = '[]' ] && [ "$(rec 'r["recipient_sha256"]')" = "$rcp_sha" ] \
  && [ "$(rec 'list(r)')" = "['at', 'store', 'backup', 'sha256', 'bytes', 'retention_failure', 'remote', 'remote_name', 'recipient_sha256', 'deleted', 'code']" ]; } \
  && ok "record: exactly the members, digest, bytes, path, recipient digest, line 'backup: ok <path>'" || bad "record $(cat "$state/last.json")"
! grep -rq SECRET_TOKEN "$t/out" "$t/err" "$state" && ! grep -v '^rclone \(copyto\|lsf\|deletefile\) \|^ont \|^gpg ' "$t/calls.log" | grep -q . && ok "no credential in output or record; only copyto, lsf, deletefile, gpg and ont backup run" || bad "secret or verb"
# the gpg call: public-key encryption to the recipient file, a home of this run under state, no passphrase, no symmetric key, no import.
gl=$(grep '^gpg ' "$t/calls.log" | head -n 1)
{ [ "$(gcalls)" -eq 1 ] && [[ "$gl" == *" --batch "* ]] && [[ "$gl" == *" --encrypt "* ]] && [[ "$gl" == *" --recipient-file $rcp "* ]] && [[ "$gl" == *" --homedir $state/.enc."* ]] \
  && [[ "$gl" != *symmetric* ]] && [[ "$gl" != *passphrase* ]] && [[ "$gl" != *import* ]] && [[ "$gl" != *" -c "* ]]; } && ok "gpg: one call, --batch --encrypt --recipient-file <recipient>, a home under state, no passphrase" || bad "gpg call: $gl"
src=$(grep '^rclone copyto ' "$t/calls.log" | head -n 1 | cut -d' ' -f3-)
[[ "$src" == "$state/.enc."* ]] && ok "the source of the copy is the encrypted file of this run, under state" || bad "copy source: $src"
state_clean && ok "after the run state holds lock and last.json only" || bad "state: $(ls -A "$state")"

# an unchanged store: the digest equals the previous copied one: nothing copied or deleted.
: > "$t/calls.log"; run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = unchanged ] && [ "$(rec 'r["remote_name"]')" = null ] && [ "$(calls copyto)" -eq 0 ] && [ "$(calls lsf)" -eq 0 ] \
  && [ "$(calls deletefile)" -eq 0 ] && [ "$(remote_files)" = "$name " ] && [ "$(rec 'r["sha256"]')" = "$sha" ]; } && ok "unchanged digest: exit 0, remote unchanged, nothing copied or deleted" || bad "unchanged rc=$rc"
[ "$(gcalls)" -eq 0 ] && [ "$(rec 'r["recipient_sha256"]')" = "$rcp_sha" ] && ok "unchanged: no gpg call, the record keeps the recipient digest" || bad "unchanged gpg"
run; [ "$(rec 'r["remote"]')" = unchanged ] && ok "a second unchanged run stays unchanged" || bad "second unchanged"
echo two > "$store"; run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = copied ] && [ "$(rec 'r["sha256"]')" != "$sha" ]; } && ok "a changed store is copied again" || bad "changed rc=$rc"

# the rule of unchanged: the same digest AND the same key digest. A record of the base's script has no recipient_sha256; a changed key is a new key.
echo same > "$store"; run; [ "$(rec 'r["remote"]')" = copied ] || bad "3c setup"
python3 -I -c 'import json, sys; r = json.load(open(sys.argv[1])); del r["recipient_sha256"]; json.dump(r, open(sys.argv[1], "w"))' "$state/last.json"
: > "$t/calls.log"; run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = copied ] && [ "$(calls copyto)" -eq 1 ] && [ "$(gcalls)" -eq 1 ] && [ "$(rec 'r["recipient_sha256"]')" = "$rcp_sha" ]; } \
  && ok "a previous record without recipient_sha256 never counts: the unchanged store is copied" || bad "3c base record rc=$rc"
: > "$t/calls.log"; run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = unchanged ] && nothing_left; } && ok "same digest and same key: unchanged, no gpg and no rclone call" || bad "3c same rc=$rc"
echo PUBKEY-TWO > "$rcp"; : > "$t/calls.log"; run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = copied ] && [ "$(calls copyto)" -eq 1 ] && [ "$(rec 'r["recipient_sha256"]')" != "$rcp_sha" ] \
  && [ "$(rec 'r["recipient_sha256"]')" = "$(sha256sum < "$rcp" | cut -d' ' -f1)" ]; } && ok "another recipient file: the unchanged store is copied again" || bad "3c other key rc=$rc"
echo PUBKEY-ONE > "$rcp"

# pruning: sixteen files of the script's form and foreign names; the newest fourteen of the form stay, foreigners stay.
rm -rf "$t/remote/r/dir"; mkdir -p "$t/remote/r/dir/subdir"; cd "$t/remote/r/dir"
for i in $(seq 1 16); do : > "$(printf '2020%02d01T000000Z-0123456789ab.sqlite' "$i")"; done
: > 'notes.txt'; : > '20200101T000000Z-short.sqlite'; : > 'a b.sqlite'; : > '$(touch pwn).sqlite'; cd "$OLDPWD"
echo three > "$store"; run; name=$(rec 'r["remote_name"]')
form=$(ls "$t/remote/r/dir" | grep -cE '^[0-9]{8}T[0-9]{6}Z-[0-9a-f]{12}\.sqlite(\.gpg)?$')
{ [ "$rc" -eq 0 ] && [ "$form" -eq 14 ] && [ -e "$t/remote/r/dir/$name" ] && [ -e "$t/remote/r/dir/notes.txt" ] && [ -e "$t/remote/r/dir/a b.sqlite" ] \
  && [ -e "$t/remote/r/dir/\$(touch pwn).sqlite" ] && [ -e "$t/remote/r/dir/20200101T000000Z-short.sqlite" ] && [ -d "$t/remote/r/dir/subdir" ] \
  && [ ! -e "$t/remote/r/dir/20200101T000000Z-0123456789ab.sqlite" ] && [ -e "$t/remote/r/dir/20200401T000000Z-0123456789ab.sqlite" ] \
  && [ ! -e "$t/remote/r/dir/pwn" ] && [ ! -e "$PWD/pwn" ] \
  && [ "$(rec 'r["deleted"]')" = "['20200101T000000Z-0123456789ab.sqlite', '20200201T000000Z-0123456789ab.sqlite', '20200301T000000Z-0123456789ab.sqlite']" ]; } \
  && ok "pruning: 16 old of the form + 2 foreign (and odd) names: newest 14 stay, only the 3 oldest of the form deleted, foreign kept" || bad "prune rc=$rc form=$form $(cat "$state/last.json")"
# a keep of 1 keeps only the new file of the form.
sleep 1; conf "$R,\"keep\":1"; echo four > "$store"; run
{ [ "$rc" -eq 0 ] && [ "$(ls "$t/remote/r/dir" | grep -cE '^[0-9]{8}T[0-9]{6}Z-[0-9a-f]{12}\.sqlite(\.gpg)?$')" -eq 1 ] && [ -e "$t/remote/r/dir/notes.txt" ]; } && ok "keep 1 leaves the newest file of the form" || bad "keep 1 rc=$rc"

# a listing of more than 64 KiB after the new name: the name is still found (no SIGPIPE of the match), the run ends 0 and prunes.
conf "$R,\"keep\":2"; echo big > "$store"
LSF_EXTRA=4000 run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = copied ] && [ "$(rec 'r["code"]')" = null ] && [ "$(ls "$t/remote/r/dir" | grep -cE '^[0-9]{8}T[0-9]{6}Z-[0-9a-f]{12}\.sqlite(\.gpg)?$')" -le 2 ]; } \
  && ok "a remote listing over 64 KiB: the name is found, exit 0, pruned" || bad "big listing rc=$rc $(cat "$t/out")"

# pruning over both name forms: the plain copies of the base's script and the encrypted ones, newest keep=3 together.
rm -rf "$t/remote/r/dir"; mkdir -p "$t/remote/r/dir/subdir"; cd "$t/remote/r/dir"
: > 20200101T000000Z-aaaaaaaaaaaa.sqlite; : > 20200201T000000Z-bbbbbbbbbbbb.sqlite.gpg; : > 20200301T000000Z-cccccccccccc.sqlite
: > 20200401T000000Z-dddddddddddd.sqlite.gpg; : > 20200501T000000Z-eeeeeeeeeeee.sqlite
: > notes.txt; : > 20200101T000000Z-short.sqlite.gpg; : > x.sqlite.gpg; : > 20200101T000000Z-aaaaaaaaaaaa.sqlite.gpg.bak; cd "$OLDPWD"
conf "$R,\"keep\":3"; echo both > "$store"; : > "$t/calls.log"; run; name=$(rec 'r["remote_name"]')
order=$(grep '^rclone deletefile ' "$t/calls.log" | sed 's|.*/||' | tr '\n' ' ')
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = copied ] && [ -e "$t/remote/r/dir/$name" ] \
  && [ ! -e "$t/remote/r/dir/20200101T000000Z-aaaaaaaaaaaa.sqlite" ] && [ ! -e "$t/remote/r/dir/20200201T000000Z-bbbbbbbbbbbb.sqlite.gpg" ] && [ ! -e "$t/remote/r/dir/20200301T000000Z-cccccccccccc.sqlite" ] \
  && [ -e "$t/remote/r/dir/20200401T000000Z-dddddddddddd.sqlite.gpg" ] && [ -e "$t/remote/r/dir/20200501T000000Z-eeeeeeeeeeee.sqlite" ] \
  && [ -e "$t/remote/r/dir/notes.txt" ] && [ -e "$t/remote/r/dir/20200101T000000Z-short.sqlite.gpg" ] && [ -e "$t/remote/r/dir/x.sqlite.gpg" ] \
  && [ -e "$t/remote/r/dir/20200101T000000Z-aaaaaaaaaaaa.sqlite.gpg.bak" ] && [ -d "$t/remote/r/dir/subdir" ] \
  && [ "$order" = "20200101T000000Z-aaaaaaaaaaaa.sqlite 20200201T000000Z-bbbbbbbbbbbb.sqlite.gpg 20200301T000000Z-cccccccccccc.sqlite " ] \
  && [ "$(rec 'r["deleted"]')" = "['20200101T000000Z-aaaaaaaaaaaa.sqlite', '20200201T000000Z-bbbbbbbbbbbb.sqlite.gpg', '20200301T000000Z-cccccccccccc.sqlite']" ] \
  && ! grep -q '^rclone [^cld]' "$t/calls.log"; } \
  && ok "prune both forms: the newest 3 of .sqlite and .sqlite.gpg together stay, the older are deleted oldest first, the new file and foreign names stay, deleted lists exactly those" \
  || bad "prune both forms rc=$rc order=$order $(remote_files) $(cat "$state/last.json")"
conf "$R"

# a missing recipient: the remote step ends recipient_missing before any gpg or rclone call; the local backup stays.
conf ',"remote":"r:dir"'; echo norecipient > "$store"; : > "$t/calls.log"; run; p=$(rec 'r["backup"]')
{ [ "$rc" -eq 3 ] && [ "$(rec 'r["code"]')" = recipient_missing ] && [ "$(rec 'r["remote"]')" = failed ] && [ "$(rec 'r["recipient_sha256"]')" = null ] && nothing_left \
  && [ -f "$p" ] && [ "$(cat "$t/out")" = "backup: recipient_missing $p" ] && state_clean; } && ok "recipient_missing: exit 3, no gpg and no rclone call, the local backup stays" || bad "recipient_missing rc=$rc $(cat "$t/out")"
# a recipient file that is absent, empty or not a regular file: encrypt_failed, nothing run.
: > "$t/rcp-empty"; mkdir -p "$t/rcp-dir"
for rf in "$t/rcp-absent" "$t/rcp-empty" "$t/rcp-dir"; do
  conf ",\"remote\":\"r:dir\",\"recipient\":\"$rf\""; echo "rf-${rf##*/}" > "$store"; : > "$t/calls.log"; run; p=$(rec 'r["backup"]')
  { [ "$rc" -eq 3 ] && [ "$(rec 'r["code"]')" = encrypt_failed ] && [ "$(rec 'r["remote"]')" = failed ] && [ "$(rec 'r["recipient_sha256"]')" = null ] && nothing_left \
    && [ -f "$p" ] && [ "$(cat "$t/out")" = "backup: encrypt_failed $p" ] && state_clean; } && ok "recipient ${rf##*/}: encrypt_failed, exit 3, no gpg and no rclone call" || bad "recipient ${rf##*/} rc=$rc"
done
# gpg not on PATH: encrypt_failed, no rclone call.
mkdir -p "$t/nogpg"
for tool in bash env python3 mkdir flock rm date mktemp chmod mv sha256sum cut head od tr grep sort cp cat ls kill seq sed wc dirname tail; do
  ln -sf "$(PATH=$origpath command -v "$tool")" "$t/nogpg/$tool"
done
conf "$R"; echo nogpg > "$store"; : > "$t/calls.log"; PATH="$fake:$t/nogpg" run
{ [ "$rc" -eq 3 ] && [ "$(rec 'r["code"]')" = encrypt_failed ] && [ "$(rec 'r["remote"]')" = failed ] && nothing_left && state_clean; } && ok "gpg not on PATH: encrypt_failed, exit 3, no rclone call" || bad "no gpg rc=$rc $(cat "$t/out")"

# a plain file never leaves: gpg that copies its input, exits 0 with no output, with an empty output, or exits 1.
printf 'SQLite format 3\0%s' "plain-store-bytes" > "$store"; rm -rf "$t/remote/r/dir"; mkdir -p "$t/remote/r/dir"; conf "$R"
good=1
for mode in copy empty emptyfile fail; do
  : > "$t/calls.log"; GPG_MODE=$mode run; p=$(rec 'r["backup"]')
  { [ "$rc" -eq 3 ] && [ "$(rec 'r["code"]')" = encrypt_failed ] && [ "$(rec 'r["remote"]')" = failed ] && ! grep -q '^rclone ' "$t/calls.log" && ! grep -q PLAIN_COPIED "$t/calls.log" \
    && [ -z "$(remote_files)" ] && [ -f "$p" ] && cmp -s "$p" "$store" && [ "$(cat "$t/out")" = "backup: encrypt_failed $p" ] && state_clean && ! grep -rq SECRET_TOKEN "$t/out" "$t/err" "$state"; } || { good=0; bad "plain never leaves, gpg $mode: rc=$rc $(cat "$t/out")"; }
done
[ "$good" -eq 1 ] && ok "plain never leaves: a gpg that copies its input, writes nothing, writes an empty file or fails ends encrypt_failed, exit 3, rclone is never called, the local backup stays"

# no remote: exit 0, remote none, neither gpg nor rclone runs (a recipient or none).
conf ""; : > "$t/calls.log"; run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = none ] && [ "$(calls '')" -eq 0 ] && [ "$(gcalls)" -eq 0 ] && [ "$(rec 'r["sha256"]')" != null ]; } && ok "no remote: exit 0, remote none, rclone never runs" || bad "none rc=$rc"
conf ",\"recipient\":\"$rcp\""; : > "$t/calls.log"; run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = none ] && nothing_left && [ "$(rec 'r["recipient_sha256"]')" = null ] && state_clean; } && ok "no remote with a recipient: no gpg call, no rclone call, recipient_sha256 null" || bad "none+recipient rc=$rc"
conf ',"remote":""'; : > "$t/calls.log"; run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = none ] && [ "$(calls '')" -eq 0 ] && [ "$(gcalls)" -eq 0 ]; } && ok "an empty remote is no remote" || bad "empty remote rc=$rc"
ONT_RETENTION='"prune_failed"' run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["retention_failure"]')" = prune_failed ]; } && ok "retention_failure is recorded as the engine answered" || bad "retention"

# the four failures of the remote step and the failed backup.
conf "$R"; echo five > "$store"
for pair in "copyto remote_copy_failed" "lsf remote_list_failed" "deletefile remote_prune_failed"; do
  set -- $pair; echo "x$1" > "$store"; : > "$t/calls.log"
  [ "$1" = deletefile ] && for i in 1 2 3; do : > "$t/remote/r/dir/2020010${i}T000000Z-0123456789ab.sqlite"; done
  [ "$1" = deletefile ] && conf "$R,\"keep\":1"
  RCLONE_FAIL=$1 run; p=$(rec 'r["backup"]')
  { [ "$rc" -eq 3 ] && [ "$(rec 'r["code"]')" = "$2" ] && [ "$(rec 'r["remote"]')" = failed ] && [ -f "$p" ] && [ "$(cat "$t/out")" = "backup: $2 $p" ]; } \
    && ok "$2: exit 3, the local backup stays, line 'backup: $2 <path>'" || bad "$2 rc=$rc $(cat "$t/out" "$t/err")"
  state_clean || bad "$2: state holds $(ls -A "$state")"
  conf "$R"
done
echo hide > "$store"; LSF_HIDE='gpg$' run
{ [ "$rc" -eq 3 ] && [ "$(rec 'r["code"]')" = remote_list_failed ]; } && ok "remote_list_failed: the listing lacks the new name: exit 3" || bad "list hides rc=$rc"
# after a failed remote step the same digest is copied again (a failed record is not a previous copy).
: > "$t/calls.log"; run
{ [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = copied ] && [ "$(calls copyto)" -eq 1 ]; } && ok "a digest whose remote step failed is copied on the next run" || bad "retry rc=$rc"
: > "$t/calls.log"; ONT_FAIL=1 run
{ [ "$rc" -eq 4 ] && [ "$(rec 'r["code"]')" = backup_failed ] && [ "$(rec 'r["backup"]')" = null ] && [ "$(cat "$t/out")" = "backup: backup_failed -" ] && [ "$(calls '')" -eq 0 ] && [ "$(gcalls)" -eq 0 ] && state_clean; } \
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
  && [ -z "$(ls -A "$state" | grep '^\.last')" ] && ls -A "$state" | grep -q '^\.enc\.'; } && ok "kill between copy and listing: last.json is the previous record, whole (the working files of the killed run are left)" || bad "kill rc=$krc"
run; { [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = copied ] && state_clean; } && ok "the run after a kill recovers (the lock is free, the digest is copied, the left working files are removed)" || bad "after kill rc=$rc $(ls -A "$state")"
rm -rf "$state"; echo kill3 > "$store"
: $( (KILL_PIDFILE=$t/pid AP_BACKUP_CONFIG=$t/c.json bash -c 'echo $$ > "$1"; exec bash "$2"' x "$t/pid" "$script" > /dev/null 2>&1; echo $?) 2> /dev/null)
[ ! -e "$state/last.json" ] && [ -z "$(ls -A "$state" | grep '^\.last')" ] && ok "kill on the first run: no last.json and no partial file" || bad "kill first run"
run; { [ "$rc" -eq 0 ] && state_clean; } && ok "the first run after that kill leaves lock and last.json only" || bad "after first kill rc=$rc $(ls -A "$state")"

# settings that are refused: exit 2 before the lock; nothing runs, no record, nothing under a bad path.
rm -rf "$state"; : > "$t/calls.log"
for bad_conf in ',"remote":"r:../x"' ',"remote":"r:dir; touch pwn"' ',"remote":"-r:dir"' ',"remote":"r:dir x"' ',"remote":"dir"' ',"remote":"r:$(id)"' ',"remote":5' ',"keep":0' ',"keep":61' ',"keep":"5"' ',"keep":true' ',"keep":1.5'; do
  conf ",\"remote\":\"r:dir\"$bad_conf"; case "$bad_conf" in *remote*) conf "$bad_conf" ;; esac
  run; { [ "$rc" -eq 2 ] && [ ! -e "$state" ] && [ ! -s "$t/calls.log" ] && [ ! -e "$PWD/pwn" ] && [ -z "$(cat "$t/out")" ]; } || bad "refuse $bad_conf rc=$rc"
done; ok "refused settings (shell characters, spaces, a leading dash, no colon, wrong types, keep out of 1..60): exit 2, nothing run"
good=1
for bad_rcp in '"rel/path"' '5' '"/a\u0001b"' '"/a\nb"' '""' 'null' '["/x"]' 'true'; do
  conf ",\"remote\":\"r:dir\",\"recipient\":$bad_rcp"; : > "$t/calls.log"; run
  { [ "$rc" -eq 2 ] && [ ! -e "$state" ] && [ ! -s "$t/calls.log" ] && [ -z "$(cat "$t/out")" ]; } || { good=0; bad "refuse recipient $bad_rcp rc=$rc"; }
  conf ",\"recipient\":$bad_rcp"; run; { [ "$rc" -eq 2 ] && [ ! -e "$state" ] && [ ! -s "$t/calls.log" ]; } || { good=0; bad "refuse recipient (no remote) $bad_rcp rc=$rc"; }
done
[ "$good" -eq 1 ] && ok "refused recipient (relative, a number, a control character, empty, null, a list, a boolean): exit 2, nothing run, no state"
echo '{"store":"relative","state":"'"$state"'"}' > "$t/c.json"; run; [ "$rc" -eq 2 ] && ok "a relative path is refused" || bad "relative rc=$rc"
echo 'not json' > "$t/c.json"; run; [ "$rc" -eq 2 ] && ok "a settings file that is not JSON is refused" || bad "not json rc=$rc"
AP_BACKUP_CONFIG= bash "$script" > /dev/null 2>&1; [ "$?" -eq 2 ] && ok "an empty AP_BACKUP_CONFIG is refused" || bad "empty config var"
[ "$(ls "$t"/state* 2> /dev/null | wc -l)" -eq 0 ] || bad "a refusal made a state directory"

# the round trip with the real gpg of the PATH the test started with: its own key pair, in a short path (the agent's
# socket path is bounded), removed at exit with its agent.
if [ -z "$realgpg" ]; then
  echo "skip round trip: gpg is not on PATH"
  [ -z "${CHECK_SKIPS:-}" ] || echo backup-round-trip >> "$CHECK_SKIPS"
else
  # /tmp when it is writable; else the scratch TMPDIR (a stage's view), where the path may be too long for the socket.
  gh=$(mktemp -d /tmp/bkgpg.XXXXXX 2> /dev/null || mktemp -d "${TMPDIR:-/tmp}/g.XXXXXX"); chmod 700 "$gh"
  g() { "$realgpg" --homedir "$gh" --batch --pinentry-mode loopback --passphrase '' "$@"; }
  # the longest socket gpg-agent makes in its home is S.gpg-agent.browser; sun_path holds 108 bytes with the NUL, and gpg-agent refuses a path of 107.
  if [ "$((${#gh} + 1 + 19))" -gt 106 ]; then
    bad "round trip: the GnuPG home path is too long for gpg-agent's socket ($((${#gh} + 20)) > 106 bytes): $gh; set a shorter TMPDIR"
  elif g --quick-generate-key backup-test@example.invalid default default never > /dev/null 2>&1 \
      && g --export > "$t/rt.pub" 2> /dev/null && g --armor --export > "$t/rt.asc" 2> /dev/null && [ -s "$t/rt.pub" ] && [ -s "$t/rt.asc" ]; then
    rm -rf "$state" "$t/remote/r/dir"; mkdir -p "$t/remote/r/dir"; : > "$t/calls.log"; good=1
    for kind in pub asc; do
      { printf 'SQLite format 3\0'; head -c 20000 /dev/urandom; } > "$store"
      conf ",\"remote\":\"r:dir\",\"recipient\":\"$t/rt.$kind\""
      PATH="$fake:$origpath" run; name=$(rec 'r["remote_name"]'); path=$(rec 'r["backup"]'); f=$t/remote/r/dir/$name
      { [ "$rc" -eq 0 ] && [ "$(rec 'r["remote"]')" = copied ] && [[ "$name" =~ ^[0-9]{8}T[0-9]{6}Z-[0-9a-f]{12}\.sqlite\.gpg$ ]] && [ -s "$f" ] \
        && [ "$(head -c 16 "$f" | od -An -v -tx1 | tr -d ' \n')" != 53514c69746520666f726d6174203300 ] \
        && g --output "$t/rt.dec" --decrypt "$f" > /dev/null 2>&1 && cmp -s "$t/rt.dec" "$path" && cmp -s "$t/rt.dec" "$store" \
        && [ "$(sha256sum < "$t/rt.dec" | cut -c1-12)" = "$(sed -E 's/^[0-9]{8}T[0-9]{6}Z-([0-9a-f]{12})\.sqlite\.gpg$/\1/' <<< "$name")" ] \
        && state_clean && ! grep -q PLAIN_COPIED "$t/calls.log"; } || { good=0; bad "round trip ($kind) rc=$rc name=$name $(cat "$t/out" "$t/err")"; }
      rm -f "$t/rt.dec"
    done
    [ "$good" -eq 1 ] && ok "round trip: the remote file, decrypted with the test's private key, equals the backup byte for byte (binary and armored recipient), its digest begins with the 12 hex of the name, the name ends .sqlite.gpg, the file is not SQLite"
  else
    bad "round trip: the test could not make its key pair"
  fi
fi
# the home of the user was neither read nor written: HOME is still empty and GNUPGHOME was never made.
[ -z "$(ls -A "$HOME")" ] && [ ! -e "$GNUPGHOME" ] && ok "the GnuPG home of the user is untouched (HOME empty, GNUPGHOME not made)" || bad "HOME or GNUPGHOME touched: $(ls -A "$HOME")"
[ "$fails" -eq 0 ] && echo "test-backup-store: PASS" || { echo "test-backup-store: $fails FAILED"; exit 1; }
