#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_SCRIPT="$SCRIPT_DIR/install-ap.sh"
TMP_ROOT="$(mktemp -d)"
HOME_ROOT=""
trap 'rm -rf "$TMP_ROOT"; [[ -z "$HOME_ROOT" ]] || rm -rf "$HOME_ROOT"' EXIT
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

# The build base: every refusal names the deepest failing component; each case
# stops before any build (the expected commit 40 zeros is never origin/main).
ZEROS=0000000000000000000000000000000000000000

# Runs a refusing install with the build base $2; $3 is the expected code and
# $4 the component the refusal names.
expect_build_refusal() {
  local name="$1"
  local base="$2"
  local code="$3"
  local component="$4"
  expect_refusal "$name" "$code" \
    env AP_INSTALL_BUILD_DIR="$base" AP_INSTALL_DIR="$INSTALL_DIR1" AP_INSTALL_LOCK="$LOCK_FILE" CARGO_TARGET_DIR="$TMP_ROOT/target" bash "$INSTALL_SCRIPT" install "$ZEROS"
  local line=""
  line="$(grep -m 1 "^${code}@" "$TMP_ROOT/$name.err" || true)"
  if [[ "$line" =~ ^${code}@install-ap\.sh:[0-9]+:\ (.*)$ && "${BASH_REMATCH[1]}" == "$component" ]]; then
    report ok "${name}_component"
  else
    report FAIL "${name}_component"
  fi
}

mkdir -m 0777 "$TMP_ROOT/open"
expect_build_refusal build_dir_group_other_writable "$TMP_ROOT/open/build" build_dir_unsafe "$TMP_ROOT/open"
if [[ ! -e "$TMP_ROOT/open/build" ]]; then
  report ok build_dir_unsafe_creates_nothing
else
  report FAIL build_dir_unsafe_creates_nothing
fi

mkdir -m 1777 "$TMP_ROOT/sticky"
expect_build_refusal build_dir_sticky "$TMP_ROOT/sticky/build" build_dir_unsafe "$TMP_ROOT/sticky"

mkdir -m 0700 "$TMP_ROOT/real"
ln -s "$TMP_ROOT/real" "$TMP_ROOT/link"
expect_build_refusal build_dir_link "$TMP_ROOT/link/build" build_dir_unsafe "$TMP_ROOT/link"
expect_build_refusal build_dir_link_slash "$TMP_ROOT/link/" build_dir_unsafe "$TMP_ROOT/link"
if [[ ! -e "$TMP_ROOT/real/build" ]]; then
  report ok build_dir_link_creates_nothing
else
  report FAIL build_dir_link_creates_nothing
fi

expect_build_refusal build_dir_relative "relative/build" build_dir_unsafe "relative/build"

# True when every existing component of $1 passes the installer's check.
path_passes_check() {
  local path="$1"
  local current=""
  local comp=""
  local meta=""
  local owner=""
  local mode=""
  local kind=""
  local -a parts=()
  IFS=/ read -r -a parts <<<"$path"
  for comp in / "${parts[@]}"; do
    [[ -n "$comp" ]] || continue
    if [[ "$comp" == / ]]; then
      current=/
    else
      current="${current%/}/$comp"
    fi
    [[ ! -L "$current" ]] || return 1
    meta="$(stat -c '%u %a %F' -- "$current" 2>/dev/null)" || return 1
    read -r owner mode kind <<<"$meta"
    [[ "$kind" == directory ]] || return 1
    [[ "$owner" == 0 || "$owner" == "$EUID" ]] || return 1
    (( (8#$mode & 8#1022) == 0 )) || return 1
  done
}

# The cases that create and clear a base run a copy of the installer in a
# scratch repository, so its `git worktree prune` touches no real repository.
if [[ -z "${HOME:-}" ]]; then
  printf 'skip build_dir_created build_dir_leftover_removed: HOME is unset\n'
  [ -z "${CHECK_SKIPS:-}" ] || echo install-ap-build-dir >> "$CHECK_SKIPS"
elif ! HOME_ROOT="$(mktemp -d -p "$HOME" 2>/dev/null)"; then
  HOME_ROOT=""
  printf 'skip build_dir_created build_dir_leftover_removed: mktemp -d under HOME failed\n'
  [ -z "${CHECK_SKIPS:-}" ] || echo install-ap-build-dir >> "$CHECK_SKIPS"
elif ! path_passes_check "$HOME_ROOT"; then
  printf 'skip build_dir_created build_dir_leftover_removed: the path of a directory under HOME fails the build base check\n'
  [ -z "${CHECK_SKIPS:-}" ] || echo install-ap-build-dir >> "$CHECK_SKIPS"
else
  SCRATCH_REPO="$HOME_ROOT/repo"
  mkdir -p "$SCRATCH_REPO/scripts"
  cp "$INSTALL_SCRIPT" "$SCRATCH_REPO/scripts/install-ap.sh"
  git -C "$SCRATCH_REPO" init -q
  git -C "$SCRATCH_REPO" -c user.name=test -c user.email=test@example.invalid -c commit.gpgsign=false commit -q --allow-empty --no-verify -m scratch
  git -C "$SCRATCH_REPO" update-ref refs/remotes/origin/main HEAD
  SCRATCH_INSTALL="$SCRATCH_REPO/scripts/install-ap.sh"
  NEW_BASE="$HOME_ROOT/parent/build"

  expect_refusal build_dir_created expected_not_origin_main \
    env AP_INSTALL_BUILD_DIR="$NEW_BASE/" AP_INSTALL_DIR="$INSTALL_DIR1" AP_INSTALL_LOCK="$LOCK_FILE" CARGO_TARGET_DIR="$TMP_ROOT/target" bash "$SCRATCH_INSTALL" install "$ZEROS"
  if [[ "$(stat -c '%a' -- "$HOME_ROOT/parent" "$NEW_BASE" 2>/dev/null | tr '\n' ' ')" == "700 700 " ]]; then
    report ok build_dir_created_mode
  else
    report FAIL build_dir_created_mode
  fi

  mkdir -m 0700 "$HOME_ROOT/outside"
  printf 'kept\n' >"$HOME_ROOT/outside/file"
  mkdir -p "$NEW_BASE/ap.left/deep"
  printf 'left\n' >"$NEW_BASE/ap.left/deep/file"
  printf 'left\n' >"$NEW_BASE/.hidden"
  ln -s "$HOME_ROOT/outside" "$NEW_BASE/link"
  expect_refusal build_dir_leftover_removed expected_not_origin_main \
    env AP_INSTALL_BUILD_DIR="$NEW_BASE" AP_INSTALL_DIR="$INSTALL_DIR1" AP_INSTALL_LOCK="$LOCK_FILE" CARGO_TARGET_DIR="$TMP_ROOT/target" bash "$SCRATCH_INSTALL" install "$ZEROS"
  if [[ -d "$NEW_BASE" && -z "$(find "$NEW_BASE" -mindepth 1 -print -quit)" && -f "$HOME_ROOT/outside/file" ]]; then
    report ok build_dir_leftover_removed_contents
  else
    report FAIL build_dir_leftover_removed_contents
  fi
fi

if [[ "$FAILURES" -ne 0 ]]; then
  exit 1
fi
