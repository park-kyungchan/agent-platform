#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_SCRIPT="$SCRIPT_DIR/install-ap.sh"
TMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMP_ROOT"' EXIT
FAILURES=0
LOCK_FILE="$TMP_ROOT/ap-install.lock"
BACKUP_DIR="$TMP_ROOT/backups"
export AP_BACKUP_DIR="$BACKUP_DIR"
mkdir -m 0755 "$BACKUP_DIR"

report() {
  local status="$1"
  local name="$2"
  printf '%s %s\n' "$status" "$name"
  if [[ "$status" == "FAIL" ]]; then
    FAILURES=$((FAILURES + 1))
  fi
}

make_fake_ap() {
  local path="$1"
  local rev="$2"
  cat >"$path" <<EOF
#!/usr/bin/env bash
printf 'ap 0.1.0 %s\n' '$rev'
EOF
  chmod +x "$path"
}

# Runs a command expected to refuse; $1=case $2=expected code, rest=command.
expect_refusal() {
  local name="$1"
  local expected="$2"
  shift 2
  local out="$TMP_ROOT/$name.out"
  local err="$TMP_ROOT/$name.err"
  if "$@" >"$out" 2>"$err"; then
    report FAIL "$name"
  elif grep -q "^${expected}@" "$err"; then
    report ok "$name"
  else
    report FAIL "$name"
  fi
}

INSTALL_DIR1="$TMP_ROOT/install1"
mkdir -p "$INSTALL_DIR1"

expect_refusal malformed_commit invalid_expected_commit \
  env AP_INSTALL_DIR="$INSTALL_DIR1" AP_INSTALL_LOCK="$LOCK_FILE" CARGO_TARGET_DIR="$TMP_ROOT/target" bash "$INSTALL_SCRIPT" install not-a-commit

expect_refusal relative_cargo_target_dir cargo_target_dir_not_absolute \
  env AP_INSTALL_DIR="$INSTALL_DIR1" AP_INSTALL_LOCK="$LOCK_FILE" CARGO_TARGET_DIR="relative/target" bash "$INSTALL_SCRIPT" install 0000000000000000000000000000000000000000

INSTALL_DIR2="$TMP_ROOT/install2"
mkdir -p "$INSTALL_DIR2"
make_fake_ap "$INSTALL_DIR2/ap" 1111111111111111111111111111111111111111
printf 'wrong\n' >"$INSTALL_DIR2/not-a-backup"
expect_refusal rollback_wrong_name rollback_invalid_before_file \
  env AP_INSTALL_DIR="$INSTALL_DIR2" AP_INSTALL_LOCK="$LOCK_FILE" bash "$INSTALL_SCRIPT" rollback "$INSTALL_DIR2/not-a-backup"

# A staged backup is never an accepted rollback source.
printf 'staged backup\n' >"$BACKUP_DIR/.ap.backup.abcdef"
expect_refusal rollback_temporary_backup rollback_invalid_before_file \
  env AP_INSTALL_DIR="$INSTALL_DIR2" AP_INSTALL_LOCK="$LOCK_FILE" bash "$INSTALL_SCRIPT" rollback "$BACKUP_DIR/.ap.backup.abcdef"

# install_busy: another process holds the lock
HOLDER_READY="$TMP_ROOT/holder.ready"
( exec 8>"$LOCK_FILE"; flock 8; printf 'ready\n' >"$HOLDER_READY"; sleep 5 ) &
HOLDER_PID=$!
for _ in 1 2 3 4 5 6 7 8 9 10; do
  [[ -f "$HOLDER_READY" ]] && break
  sleep 0.1
done
expect_refusal install_busy install_busy \
  env AP_INSTALL_DIR="$INSTALL_DIR1" AP_INSTALL_LOCK="$LOCK_FILE" CARGO_TARGET_DIR="$TMP_ROOT/target" bash "$INSTALL_SCRIPT" install 0000000000000000000000000000000000000000
kill "$HOLDER_PID" 2>/dev/null || true
wait "$HOLDER_PID" 2>/dev/null || true

# install_dir_unusable: AP_INSTALL_DIR does not exist
expect_refusal install_dir_unusable install_dir_unusable \
  env AP_INSTALL_DIR="$TMP_ROOT/no-such-dir" AP_INSTALL_LOCK="$LOCK_FILE" CARGO_TARGET_DIR="$TMP_ROOT/target" bash "$INSTALL_SCRIPT" install 0000000000000000000000000000000000000000

# rollback_backup_hash_mismatch: name prefix differs from file sha256
INSTALL_DIR_HASH="$TMP_ROOT/install-hash"
mkdir -p "$INSTALL_DIR_HASH"
make_fake_ap "$INSTALL_DIR_HASH/ap" 1111111111111111111111111111111111111111
make_fake_ap "$TMP_ROOT/hash-source" aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
HASH_SHA="$(sha256sum "$TMP_ROOT/hash-source" | awk '{print $1}')"
WRONG_PREFIX="0000000"
if [[ "${HASH_SHA:0:7}" == "$WRONG_PREFIX" ]]; then
  WRONG_PREFIX="1111111"
fi
cp "$TMP_ROOT/hash-source" "$BACKUP_DIR/ap.before-$WRONG_PREFIX"
expect_refusal rollback_backup_hash_mismatch rollback_backup_hash_mismatch \
  env AP_INSTALL_DIR="$INSTALL_DIR_HASH" AP_INSTALL_LOCK="$LOCK_FILE" bash "$INSTALL_SCRIPT" rollback "$BACKUP_DIR/ap.before-$WRONG_PREFIX"

# rollback round trip: backup name must be the file's own sha256 prefix
INSTALL_DIR3="$TMP_ROOT/install3"
mkdir -p "$INSTALL_DIR3"
make_fake_ap "$TMP_ROOT/roundtrip-source" aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
BEFORE_SHA="$(sha256sum "$TMP_ROOT/roundtrip-source" | awk '{print $1}')"
BACKUP_NAME="ap.before-${BEFORE_SHA:0:7}"
cp "$TMP_ROOT/roundtrip-source" "$BACKUP_DIR/$BACKUP_NAME"
cp "$TMP_ROOT/roundtrip-source" "$INSTALL_DIR3/$BACKUP_NAME"
expect_refusal rollback_install_dir rollback_invalid_before_file \
  env AP_INSTALL_DIR="$INSTALL_DIR3" AP_INSTALL_LOCK="$LOCK_FILE" bash "$INSTALL_SCRIPT" rollback "$INSTALL_DIR3/$BACKUP_NAME"
if grep -Fq "accepted directory: $BACKUP_DIR" "$TMP_ROOT/rollback_install_dir.err"; then
  report ok rollback_accepted_directory_message
else
  report FAIL rollback_accepted_directory_message
fi

make_fake_ap "$INSTALL_DIR3/ap" bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
if env AP_INSTALL_DIR="$INSTALL_DIR3" AP_INSTALL_LOCK="$LOCK_FILE" bash "$INSTALL_SCRIPT" rollback "$BACKUP_DIR/$BACKUP_NAME" >"$TMP_ROOT/rollback_round_trip.out" 2>"$TMP_ROOT/rollback_round_trip.err"; then
  AFTER_SHA="$(sha256sum "$INSTALL_DIR3/ap" | awk '{print $1}')"
  if [[ "$BEFORE_SHA" == "$AFTER_SHA" ]]; then
    report ok rollback_round_trip
  else
    report FAIL rollback_round_trip
  fi
else
  report FAIL rollback_round_trip
fi

# check exits 0 and writes nothing
INSTALL_DIR4="$TMP_ROOT/install4"
mkdir -p "$INSTALL_DIR4"
make_fake_ap "$INSTALL_DIR4/ap" cccccccccccccccccccccccccccccccccccccccc
SNAP_BEFORE="$(find "$INSTALL_DIR4" "$BACKUP_DIR" -type f -exec sha256sum {} + | sort)"
if env AP_INSTALL_DIR="$INSTALL_DIR4" AP_INSTALL_LOCK="$LOCK_FILE" bash "$INSTALL_SCRIPT" check >"$TMP_ROOT/check_writes_nothing.out" 2>"$TMP_ROOT/check_writes_nothing.err"; then
  SNAP_AFTER="$(find "$INSTALL_DIR4" "$BACKUP_DIR" -type f -exec sha256sum {} + | sort)"
  if [[ "$SNAP_BEFORE" == "$SNAP_AFTER" ]]; then
    report ok check_writes_nothing
  else
    report FAIL check_writes_nothing
  fi
else
  report FAIL check_writes_nothing
fi

# check must not create an absent backup directory.
MISSING_BACKUP="$TMP_ROOT/missing-backups"
if env AP_BACKUP_DIR="$MISSING_BACKUP" AP_INSTALL_DIR="$INSTALL_DIR4" AP_INSTALL_LOCK="$LOCK_FILE" bash "$INSTALL_SCRIPT" check >"$TMP_ROOT/check_missing.out" 2>"$TMP_ROOT/check_missing.err" && [[ ! -e "$MISSING_BACKUP" ]]; then
  report ok check_missing_backup_directory
else
  report FAIL check_missing_backup_directory
fi

# Invalid backup directories refuse before any build or replacement.
EXPECTED_SHA="$(sha256sum "$INSTALL_DIR4/ap" | awk '{print $1}')"
ln -s "$BACKUP_DIR" "$TMP_ROOT/backup-link"
printf 'not a directory\n' >"$TMP_ROOT/backup-file"
for bad_backup in "$TMP_ROOT/backup-link" "$TMP_ROOT/backup-link/" "$TMP_ROOT/backup-file"; do
  expect_refusal "backup_dir_unusable_$(basename "$bad_backup")" backup_dir_unusable \
    env AP_BACKUP_DIR="$bad_backup" AP_INSTALL_DIR="$INSTALL_DIR4" AP_INSTALL_LOCK="$LOCK_FILE" CARGO_TARGET_DIR="$TMP_ROOT/target" bash "$INSTALL_SCRIPT" install 0000000000000000000000000000000000000000
done
AFTER_SHA="$(sha256sum "$INSTALL_DIR4/ap" | awk '{print $1}')"
if [[ "$AFTER_SHA" == "$EXPECTED_SHA" ]]; then
  report ok backup_refusal_preserves_installed
else
  report FAIL backup_refusal_preserves_installed
fi

# --- the record, status and rollback without an argument, in a temporary repository with a fake cargo.
# The copy of the script lives in its own repository so that origin/main, the worktree and the build are the test's.
RREPO="$TMP_ROOT/rrepo"; mkdir -p "$RREPO/scripts" "$TMP_ROOT/fakebin"
cp "$INSTALL_SCRIPT" "$RREPO/scripts/install-ap.sh"
git -C "$RREPO" init -q .
rev_commit() {
  printf '%s\n' "$1" >"$RREPO/rev"
  git -C "$RREPO" add -A
  git -C "$RREPO" -c user.email=t@example.invalid -c user.name=t commit -q -m "$1"
  git -C "$RREPO" update-ref refs/remotes/origin/main HEAD
  git -C "$RREPO" rev-parse HEAD
}
cat >"$TMP_ROOT/fakebin/cargo" <<'CARGO'
#!/usr/bin/env bash
# The build of the test: a binary that prints the commit of the worktree it is built in.
rev="$(git rev-parse HEAD)"
mkdir -p "$CARGO_TARGET_DIR/release"
printf '#!/usr/bin/env bash\nprintf "ap 0.1.0 %%s\\n" "%s"\n' "$rev" >"$CARGO_TARGET_DIR/release/ap"
chmod +x "$CARGO_TARGET_DIR/release/ap"
CARGO
chmod +x "$TMP_ROOT/fakebin/cargo"
RI="$TMP_ROOT/rinst"; RB="$TMP_ROOT/rbk"; mkdir -p "$RI"; mkdir -m 0755 "$RB"
apx() {
  env AP_INSTALL_DIR="$RI" AP_BACKUP_DIR="$RB" AP_INSTALL_LOCK="$TMP_ROOT/r-install.lock" AP_CARGO_LOCK="$TMP_ROOT/r-cargo.lock" \
    CARGO_TARGET_DIR="$TMP_ROOT/rtarget" PATH="$TMP_ROOT/fakebin:$PATH" bash "$RREPO/scripts/install-ap.sh" "$@"
}
status_is() { # <commit> <check> <rollback>
  local s=""
  s="$(apx status 2>/dev/null)" || return 1
  [[ "$(printf '%s\n' "$s" | wc -l)" -eq 1 ]] \
    && printf '%s\n' "$s" | grep -Eq "^\{\"name\":\"ap\",\"commit\":\"$1\",\"installed_at\":\"[0-9T:Z-]+\",\"check\":\"$2\",\"rollback\":\"$3\"\}$"
}
REH_OK=1
reh_fail() { REH_OK=0; printf 'rehearsal: %s\n' "$1"; }
C1="$(rev_commit one)"
apx install "$C1" >"$TMP_ROOT/reh.out" 2>"$TMP_ROOT/reh.err" || reh_fail "install 1 refused: $(cat "$TMP_ROOT/reh.err")"
SHA1="$(sha256sum "$RI/ap" | awk '{print $1}')"
status_is "$C1" ok none || reh_fail "status after install 1"
C2="$(rev_commit two)"
apx install "$C2" >"$TMP_ROOT/reh.out" 2>"$TMP_ROOT/reh.err" || reh_fail "install 2 refused: $(cat "$TMP_ROOT/reh.err")"
status_is "$C2" ok available || reh_fail "status after install 2"
CHECK_OUT="$(apx check 2>&1 || true)"
[[ "$(printf '%s\n' "$CHECK_OUT" | sed -n 1p)" == "ap 0.1.0 $C2" && "$(printf '%s\n' "$CHECK_OUT" | sed -n 4p)" == equal ]] || reh_fail "check after install 2: $CHECK_OUT"
apx rollback >"$TMP_ROOT/reh.out" 2>"$TMP_ROOT/reh.err" || reh_fail "rollback refused: $(cat "$TMP_ROOT/reh.err")"
[[ "$(cat "$TMP_ROOT/reh.out")" == "ap 0.1.0 $C1" ]] || reh_fail "rollback printed $(cat "$TMP_ROOT/reh.out")"
[[ "$(sha256sum "$RI/ap" | awk '{print $1}')" == "$SHA1" ]] || reh_fail "rollback did not restore the bytes of version 1"
status_is "$C1" ok none || reh_fail "status after rollback"
APX_SNAP="$(find "$RI" "$RB" -type f -exec sha256sum {} + | sort)"
if apx rollback >"$TMP_ROOT/reh.out" 2>"$TMP_ROOT/reh.err"; then reh_fail "second rollback was admitted"; fi
grep -q '^rollback_not_available@' "$TMP_ROOT/reh.err" || reh_fail "second rollback: $(cat "$TMP_ROOT/reh.err")"
[[ "$APX_SNAP" == "$(find "$RI" "$RB" -type f -exec sha256sum {} + | sort)" ]] || reh_fail "second rollback changed a file"
apx install "$C2" >"$TMP_ROOT/reh.out" 2>"$TMP_ROOT/reh.err" || reh_fail "install 2 again refused: $(cat "$TMP_ROOT/reh.err")"
status_is "$C2" ok available || reh_fail "status after install 2 again"
CHECK_OUT="$(apx check 2>&1 || true)"
[[ "$(printf '%s\n' "$CHECK_OUT" | sed -n 1p)" == "ap 0.1.0 $C2" ]] || reh_fail "check after install 2 again: $CHECK_OUT"
if [[ "$REH_OK" -eq 1 ]]; then report ok rehearsal_install_install_check_status_rollback_refuse_install; else report FAIL rehearsal_install_install_check_status_rollback_refuse_install; fi

# status says drift when the installed binary is not the recorded one, and writes nothing.
printf '# changed\n' >>"$RI/ap"
SNAP="$(find "$RI" "$RB" -type f -exec sha256sum {} + | sort)"
if status_is "$C2" drift none && [[ "$SNAP" == "$(find "$RI" "$RB" -type f -exec sha256sum {} + | sort)" ]]; then report ok status_drift_writes_nothing; else report FAIL status_drift_writes_nothing; fi

# status without a record is not_recorded; an unreadable target is a refusal.
RI2="$TMP_ROOT/rinst2"; mkdir -p "$RI2"; make_fake_ap "$RI2/ap" dddddddddddddddddddddddddddddddddddddddd
S2="$(env AP_INSTALL_DIR="$RI2" AP_BACKUP_DIR="$TMP_ROOT/no-record-dir" bash "$INSTALL_SCRIPT" status 2>/dev/null || true)"
if [[ "$S2" == '{"name":"ap","commit":null,"installed_at":null,"check":"not_recorded","rollback":"none"}' && ! -e "$TMP_ROOT/no-record-dir" ]]; then report ok status_not_recorded; else report FAIL status_not_recorded; fi
expect_refusal status_unreadable_target target_unreadable \
  env AP_INSTALL_DIR="$TMP_ROOT/no-such-dir" AP_BACKUP_DIR="$BACKUP_DIR" bash "$INSTALL_SCRIPT" status
expect_refusal status_takes_no_argument status_takes_no_arguments \
  env AP_INSTALL_DIR="$RI2" AP_BACKUP_DIR="$BACKUP_DIR" bash "$INSTALL_SCRIPT" status extra
mkdir -m 0755 "$TMP_ROOT/empty-bk"
expect_refusal rollback_without_record rollback_not_available \
  env AP_INSTALL_DIR="$RI2" AP_BACKUP_DIR="$TMP_ROOT/empty-bk" AP_INSTALL_LOCK="$LOCK_FILE" bash "$INSTALL_SCRIPT" rollback

# a failed write of the record puts the before-file back: the installed binary is the previous one and no record is claimed.
RI="$TMP_ROOT/rinst3"; RB="$TMP_ROOT/rbk3"; mkdir -p "$RI" "$RB/installed.json"; chmod 0755 "$RB"
make_fake_ap "$RI/ap" ffffffffffffffffffffffffffffffffffffffff
PREV_SHA="$(sha256sum "$RI/ap" | awk '{print $1}')"
expect_refusal install_record_write_failed record_write_failed_restored apx install "$C2"
if [[ "$(sha256sum "$RI/ap" | awk '{print $1}')" == "$PREV_SHA" ]]; then report ok install_record_write_failed_keeps_previous_binary; else report FAIL install_record_write_failed_keeps_previous_binary; fi


# --- round 2: rollback without an argument runs only when the installed binary is the one the record holds.
mkdir -p "$TMP_ROOT/killbin" "$TMP_ROOT/tmpd"
printf '#!/bin/sh\n/bin/mv "$@" || exit $?\ncase "$*" in *" $KILL_TARGET") kill -9 "$PPID" ;; esac\nexit 0\n' >"$TMP_ROOT/killbin/mv"
chmod +x "$TMP_ROOT/killbin/mv"
json_ok() { printf '%s' "$1" | python3 -I -c 'import json,sys; d=json.loads(sys.stdin.read()); assert d["name"]=="ap"' 2>/dev/null; }
RI="$TMP_ROOT/rinst4"; RB="$TMP_ROOT/rbk4"; mkdir -p "$RI"; mkdir -m 0755 "$RB"
make_fake_ap "$RI/ap" eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
C3="$(rev_commit three)"
apx install "$C3" >/dev/null 2>&1 || report FAIL r2_setup_install_3
V3_SHA="$(sha256sum "$RI/ap" | awk '{print $1}')"
C4="$(rev_commit four)"
# a kill right after the staged binary replaced ap: the binary is version 4, the record still says version 3.
KRC=0
TMPDIR="$TMP_ROOT/tmpd" KILL_TARGET="$RI/ap" PATH="$TMP_ROOT/killbin:$PATH" apx install "$C4" >/dev/null 2>&1 || KRC=$?
KSNAP="$(find "$RI" "$RB" -type f -exec sha256sum {} + | sort)"
KST="$(apx status 2>/dev/null || true)"
KRB_RC=0; apx rollback >"$TMP_ROOT/kill.out" 2>"$TMP_ROOT/kill.err" || KRB_RC=$?
if [[ "$KRC" -eq 137 && "$(sha256sum "$RI/ap" | awk '{print $1}')" != "$V3_SHA" ]] \
  && printf '%s' "$KST" | grep -q '"check":"drift","rollback":"none"' \
  && [[ "$KRB_RC" -eq 1 ]] && grep -q '^rollback_target_not_recorded_state@' "$TMP_ROOT/kill.err" && grep -q 'rollback <before-file>' "$TMP_ROOT/kill.err" \
  && [[ "$KSNAP" == "$(find "$RI" "$RB" -type f -exec sha256sum {} + | sort)" ]]; then
  report ok rollback_after_kill_refuses_target_not_recorded_state
else
  report FAIL rollback_after_kill_refuses_target_not_recorded_state
fi
# the explicit before-file still works from that state, and the record then describes the restored binary.
if apx rollback "$RB/ap.before-${V3_SHA:0:7}" >/dev/null 2>&1 && [[ "$(sha256sum "$RI/ap" | awk '{print $1}')" == "$V3_SHA" ]] && status_is "$C3" ok none; then
  report ok rollback_by_file_after_kill
else
  report FAIL rollback_by_file_after_kill
fi

# status and rollback decide alike: a before-file outside the backup directory, a record without before_sha256.
RI="$TMP_ROOT/rinst5"; RB="$TMP_ROOT/rbk5"; mkdir -p "$RI"; mkdir -m 0755 "$RB"
make_fake_ap "$RI/ap" 9999999999999999999999999999999999999999
apx install "$C4" >/dev/null 2>&1 || report FAIL r2_setup_install_4
cp -p "$RB/installed.json" "$TMP_ROOT/rec5.good"
status_is "$C4" ok available || report FAIL r2_status_available
decide_ok=1
BF="$(sed -n 's/.*"before":"\([^"]*\)".*/\1/p' "$RB/installed.json")"
mkdir -p "$TMP_ROOT/elsewhere"; cp -p "$BF" "$TMP_ROOT/elsewhere/"
sed -i "s|\"before\":\"[^\"]*\"|\"before\":\"$TMP_ROOT/elsewhere/$(basename "$BF")\"|" "$RB/installed.json"
BIN_SHA="$(sha256sum "$RI/ap" | awk '{print $1}')"
status_is "$C4" ok none || { decide_ok=0; echo "outside: status"; }
if apx rollback >"$TMP_ROOT/d.out" 2>"$TMP_ROOT/d.err"; then decide_ok=0; echo "outside: rollback ran"; fi
grep -q '^rollback_not_available@' "$TMP_ROOT/d.err" || { decide_ok=0; echo "outside: $(cat "$TMP_ROOT/d.err")"; }
cp -p "$TMP_ROOT/rec5.good" "$RB/installed.json"
sed -i 's|"before_sha256":"[^"]*"|"before_sha256":null|' "$RB/installed.json"
status_is "$C4" ok none || { decide_ok=0; echo "null sha: status"; }
if apx rollback >"$TMP_ROOT/d.out" 2>"$TMP_ROOT/d.err"; then decide_ok=0; echo "null sha: rollback ran"; fi
grep -q '^rollback_not_available@' "$TMP_ROOT/d.err" || { decide_ok=0; echo "null sha: $(cat "$TMP_ROOT/d.err")"; }
[[ "$(sha256sum "$RI/ap" | awk '{print $1}')" == "$BIN_SHA" ]] || { decide_ok=0; echo "binary changed"; }
if [[ "$decide_ok" -eq 1 ]]; then report ok status_and_rollback_decide_alike; else report FAIL status_and_rollback_decide_alike; fi
# a record value with a backslash or a quote gives drift and still valid JSON.
cp -p "$TMP_ROOT/rec5.good" "$RB/installed.json"
sed -i 's|"commit":"[^"]*"|"commit":"a\\\\b\\"c"|' "$RB/installed.json"
BAD_ST="$(apx status 2>/dev/null || true)"
if json_ok "$BAD_ST" && printf '%s' "$BAD_ST" | grep -q '"commit":null' && printf '%s' "$BAD_ST" | grep -q '"check":"drift"'; then report ok status_valid_json_for_damaged_record; else report FAIL status_valid_json_for_damaged_record; fi

# a rollback that completed but could not write its record exits 4 and says the rollback itself completed.
RI="$TMP_ROOT/rinst6"; RB="$TMP_ROOT/rbk6"; mkdir -p "$RI" "$RB/installed.json"; chmod 0755 "$RB"
make_fake_ap "$TMP_ROOT/old-ap" 1212121212121212121212121212121212121212
OLD_SHA="$(sha256sum "$TMP_ROOT/old-ap" | awk '{print $1}')"
cp -p "$TMP_ROOT/old-ap" "$RB/ap.before-${OLD_SHA:0:7}"
make_fake_ap "$RI/ap" 3434343434343434343434343434343434343434
RC4=0; apx rollback "$RB/ap.before-${OLD_SHA:0:7}" >"$TMP_ROOT/rc4.out" 2>"$TMP_ROOT/rc4.err" || RC4=$?
if [[ "$RC4" -eq 4 && "$(sha256sum "$RI/ap" | awk '{print $1}')" == "$OLD_SHA" ]] \
  && grep -q '^rollback_completed_record_write_failed@' "$TMP_ROOT/rc4.err" && grep -q 'rollback itself completed' "$TMP_ROOT/rc4.err"; then
  report ok rollback_record_write_failure_says_completed_exit_4
else
  report FAIL rollback_record_write_failure_says_completed_exit_4
fi

if [[ "$FAILURES" -ne 0 ]]; then
  exit 1
fi
