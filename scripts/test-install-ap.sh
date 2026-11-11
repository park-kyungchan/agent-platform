#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_SCRIPT="$SCRIPT_DIR/install-ap.sh"
TMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMP_ROOT"' EXIT
FAILURES=0
LOCK_FILE="$TMP_ROOT/ap-install.lock"

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
cp "$TMP_ROOT/hash-source" "$INSTALL_DIR_HASH/ap.before-$WRONG_PREFIX"
expect_refusal rollback_backup_hash_mismatch rollback_backup_hash_mismatch \
  env AP_INSTALL_DIR="$INSTALL_DIR_HASH" AP_INSTALL_LOCK="$LOCK_FILE" bash "$INSTALL_SCRIPT" rollback "$INSTALL_DIR_HASH/ap.before-$WRONG_PREFIX"

# rollback round trip: backup name must be the file's own sha256 prefix
INSTALL_DIR3="$TMP_ROOT/install3"
mkdir -p "$INSTALL_DIR3"
make_fake_ap "$TMP_ROOT/roundtrip-source" aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
BEFORE_SHA="$(sha256sum "$TMP_ROOT/roundtrip-source" | awk '{print $1}')"
BACKUP_NAME="ap.before-${BEFORE_SHA:0:7}"
cp "$TMP_ROOT/roundtrip-source" "$INSTALL_DIR3/$BACKUP_NAME"
make_fake_ap "$INSTALL_DIR3/ap" bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
if env AP_INSTALL_DIR="$INSTALL_DIR3" AP_INSTALL_LOCK="$LOCK_FILE" bash "$INSTALL_SCRIPT" rollback "$INSTALL_DIR3/$BACKUP_NAME" >"$TMP_ROOT/rollback_round_trip.out" 2>"$TMP_ROOT/rollback_round_trip.err"; then
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
SNAP_BEFORE="$(find "$INSTALL_DIR4" -type f -exec sha256sum {} + | sort)"
if env AP_INSTALL_DIR="$INSTALL_DIR4" AP_INSTALL_LOCK="$LOCK_FILE" bash "$INSTALL_SCRIPT" check >"$TMP_ROOT/check_writes_nothing.out" 2>"$TMP_ROOT/check_writes_nothing.err"; then
  SNAP_AFTER="$(find "$INSTALL_DIR4" -type f -exec sha256sum {} + | sort)"
  if [[ "$SNAP_BEFORE" == "$SNAP_AFTER" ]]; then
    report ok check_writes_nothing
  else
    report FAIL check_writes_nothing
  fi
else
  report FAIL check_writes_nothing
fi

if [[ "$FAILURES" -ne 0 ]]; then
  exit 1
fi
