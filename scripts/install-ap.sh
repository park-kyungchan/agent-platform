#!/usr/bin/env bash
# Install `ap` on the host, and answer four verbs:
#   install <full commit>   build the commit of origin/main in a detached worktree and install it; records the
#                           commit, the sha256 of the binary, the time and the before-file in $BACKUP_DIR/installed.json
#   check                   compares the installed binary with origin/main (exit 0; writes nothing)
#   rollback                installs the before-file the record names (one level); it runs only when the installed
#                           binary is the one the record holds (sha256), else it refuses
#                           rollback_target_not_recorded_state: use `rollback <before-file>`; `rollback <before-file>` as before
#   status                  one JSON line: name, commit, installed_at, check (ok|drift|not_recorded), rollback
#                           (available|none); no write, no network, no Git call. `available` exactly when `rollback`
#                           without an argument would run: both ask rollback_plan.
# A rollback whose binary was put back but whose record could not be written exits 4 with
# `rollback_completed_record_write_failed`: the rollback itself completed (the installed binary is the before-file), the
# record still describes the binary that was rolled back, so `status` says drift and a rollback without an argument is
# refused until an install writes a record; the operator reads `status` and uses `rollback <before-file>` if needed.
set -euo pipefail

INSTALL_DIR="${AP_INSTALL_DIR:-/usr/local/bin}"
BACKUP_DIR="${AP_BACKUP_DIR:-/var/lib/agent-platform/install}"
# Strip trailing slashes so a symlink cannot hide behind a directory suffix.
while [[ "$BACKUP_DIR" != / && "$BACKUP_DIR" == */ ]]; do
  BACKUP_DIR="${BACKUP_DIR%/}"
done
INSTALLED="$INSTALL_DIR/ap"
RECORD="$BACKUP_DIR/installed.json"
LOCK_FILE="${AP_CARGO_LOCK:-/home/palantirkc/work/ws-replan/wt/.host-cargo.lock}"
INSTALL_LOCK_FILE="${AP_INSTALL_LOCK:-/run/lock/ap-install.lock}"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
WORKTREE_DIR=""
BACKUP_TMP=""

refuse() {
  local code="$1"
  local line="$2"
  printf '%s@install-ap.sh:%s%s\n' "$code" "$line" "${3:+: $3}" >&2
  exit 1
}

cleanup_worktree() {
  if [[ -n "$BACKUP_TMP" ]]; then
    rm -f -- "$BACKUP_TMP"
  fi
  if [[ -n "$WORKTREE_DIR" && -d "$WORKTREE_DIR" ]]; then
    git -C "$REPO_ROOT" worktree remove --force "$WORKTREE_DIR" >/dev/null 2>&1 || rm -rf "$WORKTREE_DIR"
  fi
}

acquire_install_lock() {
  if ! { exec 9>"$INSTALL_LOCK_FILE"; } 2>/dev/null; then
    refuse "install_lock_unavailable" "$LINENO"
  fi
  if ! flock -n 9; then
    refuse "install_busy" "$LINENO"
  fi
}

json_field() {
  # $1 = file, $2 = key; prints the string value or nothing (null and absent both print nothing).
  sed -n "s/.*\"$2\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" "$1" | head -n 1
}

# write_record <commit> <sha256 of the installed binary> <before-file or empty> <sha256 of the before-file or empty>
write_record() {
  local tmp="" kept=null kept_sha=null
  [[ -z "$3" ]] || kept="\"$3\""
  [[ -z "$4" ]] || kept_sha="\"$4\""
  tmp="$(mktemp "$BACKUP_DIR/.installed.json.XXXXXX")" || return 1
  if ! printf '{"commit":"%s","sha256":"%s","installed_at":"%s","before":%s,"before_sha256":%s}\n' \
    "$1" "$2" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$kept" "$kept_sha" >"$tmp" \
    || ! chmod 0644 "$tmp" || ! mv -T -- "$tmp" "$RECORD"; then
    rm -f -- "$tmp"
    return 1
  fi
}

check_install_dir_usable() {
  if [[ ! -d "$INSTALL_DIR" || ! -w "$INSTALL_DIR" ]]; then
    refuse "install_dir_unusable" "$LINENO"
  fi
}

check_backup_dir_usable() {
  local create="${1:-no}"
  if [[ -L "$BACKUP_DIR" || ( -e "$BACKUP_DIR" && ! -d "$BACKUP_DIR" ) ]]; then
    refuse "backup_dir_unusable" "$LINENO"
  fi
  if [[ ! -e "$BACKUP_DIR" && "$create" == yes ]]; then
    if ! mkdir -p -m 0755 -- "$BACKUP_DIR" || ! chmod 0755 -- "$BACKUP_DIR"; then
      refuse "backup_dir_unusable" "$LINENO"
    fi
  fi
  if [[ ! -d "$BACKUP_DIR" || -L "$BACKUP_DIR" || ! -w "$BACKUP_DIR" ]]; then
    refuse "backup_dir_unusable" "$LINENO"
  fi
}

version_commit() {
  local line="$1"
  printf '%s\n' "$line" | awk '{print $3}'
}

do_check() {
  local version_line="no_version_output"
  if [[ -x "$INSTALLED" ]]; then
    local out=""
    out="$("$INSTALLED" --version 2>/dev/null || true)"
    out="${out%%$'\n'*}"
    if [[ -n "$out" ]]; then
      version_line="$out"
    fi
  fi
  printf '%s\n' "$version_line"
  if [[ -f "$INSTALLED" ]]; then
    local sha=""
    sha="$(sha256sum "$INSTALLED" 2>/dev/null | awk '{print $1}' || true)"
    printf '%s\n' "$sha"
  else
    printf '%s\n' "missing"
  fi
  local origin_main=""
  origin_main="$(git -C "$REPO_ROOT" rev-parse origin/main 2>/dev/null || true)"
  if [[ -z "$origin_main" ]]; then
    origin_main="unknown"
  fi
  printf '%s\n' "$origin_main"
  local installed_commit=""
  if [[ "$version_line" != "no_version_output" ]]; then
    installed_commit="$(version_commit "$version_line")"
  fi
  if [[ -n "$installed_commit" && "$installed_commit" == "$origin_main" ]]; then
    printf '%s\n' "equal"
  else
    printf '%s\n' "differs"
  fi
  exit 0
}

# rollback_plan: the ONE decision of whether `rollback` without an argument runs; `status` asks the same function.
# Success sets RB_BEFORE and RB_SHA. Failure sets RB_CODE and RB_MSG. Writes nothing.
rollback_plan() {
  RB_CODE=rollback_not_available; RB_MSG=""; RB_BEFORE=""; RB_SHA=""
  if [[ ! -f "$RECORD" ]]; then RB_MSG="no install record"; return 1; fi
  local rsha="" before="" bsha="" actual="" base="" dir=""
  rsha="$(json_field "$RECORD" sha256)"
  before="$(json_field "$RECORD" before)"
  bsha="$(json_field "$RECORD" before_sha256)"
  if [[ -f "$INSTALLED" ]]; then actual="$(sha256sum "$INSTALLED" 2>/dev/null | awk '{print $1}' || true)"; fi
  if [[ -z "$rsha" || "$actual" != "$rsha" ]]; then
    RB_CODE=rollback_target_not_recorded_state
    RB_MSG="the installed binary is not the one the record holds (an interrupted or foreign change); read status, then use rollback <before-file> with the file you mean"
    return 1
  fi
  if [[ -z "$before" ]]; then RB_MSG="the record names no before-file (rolled back already, or the first install)"; return 1; fi
  if [[ -z "$bsha" ]]; then RB_MSG="the record holds no before_sha256"; return 1; fi
  base="$(basename -- "$before")"
  if [[ ! "$base" =~ ^ap\.before-[0-9a-f]{7}$ ]]; then RB_MSG="the record names a file that is not a before-file"; return 1; fi
  if [[ ! -f "$before" || -L "$before" ]]; then RB_MSG="the before-file is missing"; return 1; fi
  dir="$(cd -- "$(dirname -- "$before")" && pwd -P)"
  if [[ "$dir" != "$(realpath -m -- "$BACKUP_DIR")" ]]; then RB_MSG="the record names a before-file outside $(realpath -m -- "$BACKUP_DIR")"; return 1; fi
  local actual_before=""
  actual_before="$(sha256sum "$before" 2>/dev/null | awk '{print $1}' || true)"
  if [[ "$actual_before" != "$bsha" || "${base#ap.before-}" != "${actual_before:0:7}" ]]; then RB_MSG="the before-file does not match its recorded sha256"; return 1; fi
  RB_BEFORE="$before"; RB_SHA="$bsha"
}

do_status() {
  if [[ ! -d "$INSTALL_DIR" || ! -r "$INSTALL_DIR" ]]; then
    printf 'target_unreadable@install-ap.sh:%s\n' "$LINENO" >&2
    exit 1
  fi
  local commit=null at=null chk=not_recorded rb=none
  if [[ -f "$RECORD" ]]; then
    local rc="" rat="" rsha="" actual=""
    rc="$(json_field "$RECORD" commit)"
    rat="$(json_field "$RECORD" installed_at)"
    rsha="$(json_field "$RECORD" sha256)"
    chk=drift
    # Only values of the shape this script writes are printed, so the line is valid JSON whatever the record holds.
    if [[ "$rc" =~ ^([0-9a-f]{7,64}|unknown)$ && "$rat" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$ ]]; then
      commit="\"$rc\""
      at="\"$rat\""
      if [[ -f "$INSTALLED" && -n "$rsha" ]]; then
        actual="$(sha256sum "$INSTALLED" 2>/dev/null | awk '{print $1}' || true)"
        if [[ "$actual" == "$rsha" ]]; then
          chk=ok
        fi
      fi
    fi
    if rollback_plan; then
      rb=available
    fi
  fi
  printf '{"name":"ap","commit":%s,"installed_at":%s,"check":"%s","rollback":"%s"}\n' "$commit" "$at" "$chk" "$rb"
  exit 0
}

# restore_previous <had_previous> <before-file> <code>: put the before-file back over the installed binary (or remove
# a binary that had no predecessor) and refuse with <code>; restore_failed when the before-file cannot be put back.
restore_previous() {
  local had_previous="$1" backup="$2" code="$3"
  if [[ "$had_previous" -eq 1 && -n "$backup" && -f "$backup" ]]; then
    local restored=0
    local restore_tmp=""
    restore_tmp="$(mktemp "$INSTALL_DIR/.ap.restore.XXXXXX" || true)"
    if [[ -n "$restore_tmp" && -f "$restore_tmp" ]] && install -m 0755 "$backup" "$restore_tmp"; then
      if mv "$restore_tmp" "$INSTALLED"; then
        restored=1
      else
        rm -f "$restore_tmp"
      fi
    else
      [[ -n "$restore_tmp" ]] && rm -f "$restore_tmp"
    fi
    if [[ "$restored" -eq 1 ]]; then
      local restored_sha=""
      restored_sha="$(sha256sum "$INSTALLED" 2>/dev/null | awk '{print $1}' || true)"
      local backup_sha=""
      backup_sha="$(sha256sum "$backup" 2>/dev/null | awk '{print $1}' || true)"
      if [[ -n "$restored_sha" && "$restored_sha" == "$backup_sha" ]]; then
        printf '%s\n' "$backup" >&2
        refuse "$code" "$LINENO"
      fi
    fi
    printf '%s\n' "$backup" >&2
    printf '%s\n' "$INSTALLED" >&2
    refuse "restore_failed" "$LINENO"
  else
    rm -f "$INSTALLED"
  fi
  refuse "$code" "$LINENO"
}

do_install() {
  if [[ $# -ne 1 ]]; then
    refuse "install_requires_expected_commit" "$LINENO"
  fi
  local expected="$1"
  if [[ ! "$expected" =~ ^[0-9a-f]{40}$ ]]; then
    refuse "invalid_expected_commit" "$LINENO"
  fi
  if [[ -z "${CARGO_TARGET_DIR:-}" ]]; then
    refuse "missing_cargo_target_dir" "$LINENO"
  fi
  if [[ "$CARGO_TARGET_DIR" != /* ]]; then
    refuse "cargo_target_dir_not_absolute" "$LINENO"
  fi
  local target_norm=""
  target_norm="$(realpath -m "$CARGO_TARGET_DIR")"
  local repo_norm=""
  repo_norm="$(realpath -m "$REPO_ROOT")"
  if [[ "$target_norm" == "$repo_norm" || "$target_norm" == "$repo_norm"/* ]]; then
    refuse "cargo_target_dir_inside_repo" "$LINENO"
  fi
  check_install_dir_usable
  acquire_install_lock
  check_backup_dir_usable yes
  git -C "$REPO_ROOT" worktree prune
  local origin_main=""
  origin_main="$(git -C "$REPO_ROOT" rev-parse origin/main 2>/dev/null || true)"
  if [[ -z "$origin_main" ]]; then
    refuse "origin_main_unavailable" "$LINENO"
  fi
  if [[ "$expected" != "$origin_main" ]]; then
    refuse "expected_not_origin_main" "$LINENO"
  fi
  WORKTREE_DIR="$(mktemp -d)"
  trap cleanup_worktree EXIT
  if ! git -C "$REPO_ROOT" worktree add --detach "$WORKTREE_DIR" "$expected" >/dev/null; then
    refuse "worktree_add_failed" "$LINENO"
  fi
  local work_norm=""
  work_norm="$(realpath -m "$WORKTREE_DIR")"
  if [[ "$target_norm" == "$work_norm" || "$target_norm" == "$work_norm"/* ]]; then
    refuse "cargo_target_dir_inside_repo" "$LINENO"
  fi
  rm -f "$target_norm/release/ap"
  if ! (cd "$WORKTREE_DIR" && flock "$LOCK_FILE" env CARGO_TARGET_DIR="$target_norm" CARGO_BUILD_JOBS=2 cargo build --release -p ap --locked); then
    refuse "build_failed" "$LINENO"
  fi
  local built="$target_norm/release/ap"
  if [[ ! -x "$built" ]]; then
    refuse "built_binary_missing" "$LINENO"
  fi
  local backup=""
  local had_previous=0
  local current_sha=""
  if [[ -f "$INSTALLED" ]]; then
    had_previous=1
    current_sha="$(sha256sum "$INSTALLED" | awk '{print $1}')"
    if [[ -z "$current_sha" ]]; then
      refuse "installed_sha_unavailable" "$LINENO"
    fi
    backup="$BACKUP_DIR/ap.before-${current_sha:0:7}"
    if ! BACKUP_TMP="$(mktemp "$BACKUP_DIR/.ap.backup.XXXXXX")"; then
      refuse "backup_failed" "$LINENO"
    fi
    if ! cp "$INSTALLED" "$BACKUP_TMP"; then
      rm -f -- "$BACKUP_TMP"
      refuse "backup_failed" "$LINENO"
    fi
    local backup_sha=""
    backup_sha="$(sha256sum "$BACKUP_TMP" 2>/dev/null | awk '{print $1}' || true)"
    if [[ "$backup_sha" != "$current_sha" ]]; then
      rm -f -- "$BACKUP_TMP"
      refuse "backup_failed" "$LINENO"
    fi
    if ! mv -T -- "$BACKUP_TMP" "$backup"; then
      rm -f -- "$BACKUP_TMP"
      refuse "backup_failed" "$LINENO"
    fi
    BACKUP_TMP=""
  fi
  local tmp=""
  tmp="$(mktemp "$INSTALL_DIR/.ap.install.XXXXXX")"
  if ! install -m 0755 "$built" "$tmp"; then
    rm -f "$tmp"
    refuse "install_failed" "$LINENO"
  fi
  local staged_version=""
  staged_version="$("$tmp" --version 2>/dev/null || true)"
  staged_version="${staged_version%%$'\n'*}"
  local staged_commit=""
  staged_commit="$(version_commit "$staged_version")"
  if [[ "$staged_commit" != "$expected" ]]; then
    rm -f "$tmp"
    refuse "staged_version_mismatch" "$LINENO"
  fi
  if ! mv "$tmp" "$INSTALLED"; then
    rm -f "$tmp"
    refuse "install_move_failed" "$LINENO"
  fi
  local new_version=""
  new_version="$("$INSTALLED" --version 2>/dev/null || true)"
  new_version="${new_version%%$'\n'*}"
  local new_commit=""
  new_commit="$(version_commit "$new_version")"
  if [[ "$new_commit" != "$expected" ]]; then
    restore_previous "$had_previous" "$backup" "installed_version_mismatch_restored"
  fi
  # The record is written last, after the installed version was verified; a failed write puts the before-file back.
  local new_sha=""
  new_sha="$(sha256sum "$INSTALLED" | awk '{print $1}')"
  if ! write_record "$expected" "$new_sha" "$backup" "$current_sha"; then
    restore_previous "$had_previous" "$backup" "record_write_failed_restored"
  fi
  trap - EXIT
  cleanup_worktree
  WORKTREE_DIR=""
}

do_rollback() {
  check_install_dir_usable
  if [[ $# -gt 1 ]]; then
    refuse "rollback_takes_at_most_one_argument" "$LINENO"
  fi
  local before="" recorded_sha="" locked=0
  if [[ $# -eq 0 ]]; then
    # Without an argument: only when the installed binary is the one the record holds (rollback_plan), under the lock.
    check_backup_dir_usable
    acquire_install_lock
    locked=1
    if ! rollback_plan; then
      refuse "$RB_CODE" "$LINENO" "$RB_MSG"
    fi
    before="$RB_BEFORE"
    recorded_sha="$RB_SHA"
  else
    before="$1"
  fi
  if [[ ! -f "$before" || -L "$before" ]]; then
    refuse "rollback_source_missing" "$LINENO"
  fi
  local base=""
  base="$(basename -- "$before")"
  if [[ ! "$base" =~ ^ap\.before-[0-9a-f]{7}$ ]]; then
    refuse "rollback_invalid_before_file" "$LINENO" "accepted directory: $(realpath -m -- "$BACKUP_DIR")"
  fi
  local before_dir=""
  before_dir="$(cd -- "$(dirname -- "$before")" && pwd -P)"
  local backup_dir_norm=""
  backup_dir_norm="$(realpath -m -- "$BACKUP_DIR")"
  if [[ "$before_dir" != "$backup_dir_norm" ]]; then
    refuse "rollback_invalid_before_file" "$LINENO" "accepted directory: $backup_dir_norm"
  fi
  check_backup_dir_usable
  local before_sha=""
  before_sha="$(sha256sum "$before" | awk '{print $1}')"
  if [[ "${base#ap.before-}" != "${before_sha:0:7}" ]]; then
    refuse "rollback_backup_hash_mismatch" "$LINENO"
  fi
  if [[ -n "$recorded_sha" && "$before_sha" != "$recorded_sha" ]]; then
    refuse "rollback_backup_hash_mismatch" "$LINENO" "the before-file differs from the record"
  fi
  if [[ "$locked" -eq 0 ]]; then
    acquire_install_lock
  fi
  local tmp=""
  tmp="$(mktemp "$INSTALL_DIR/.ap.rollback.XXXXXX")"
  if ! install -m 0755 "$before" "$tmp"; then
    rm -f "$tmp"
    refuse "rollback_install_failed" "$LINENO"
  fi
  if ! mv "$tmp" "$INSTALLED"; then
    rm -f "$tmp"
    refuse "rollback_move_failed" "$LINENO"
  fi
  local out=""
  out="$("$INSTALLED" --version 2>/dev/null || true)"
  out="${out%%$'\n'*}"
  if [[ -n "$out" ]]; then
    printf '%s\n' "$out"
  else
    printf '%s\n' "no_version_output"
  fi
  # The record now describes the restored binary and names no before-file: one level of rollback.
  local restored_commit=""
  restored_commit="$(version_commit "$out")"
  if [[ ! "$restored_commit" =~ ^[0-9a-f]{7,64}$ ]]; then
    restored_commit=unknown
  fi
  if ! write_record "${restored_commit:-unknown}" "$before_sha" "" ""; then
    printf 'rollback_completed_record_write_failed@install-ap.sh:%s: the rollback itself completed (the installed binary is the before-file); only the record could not be written, so status says drift and a rollback without an argument is refused\n' "$LINENO" >&2
    exit 4
  fi
}

main() {
  if [[ $# -lt 1 ]]; then
    refuse "unknown_mode" "$LINENO"
  fi
  local mode="$1"
  case "$mode" in
    check)
      if [[ $# -ne 1 ]]; then
        refuse "check_takes_no_arguments" "$LINENO"
      fi
      do_check
      ;;
    install)
      shift
      do_install "$@"
      ;;
    rollback)
      shift
      do_rollback "$@"
      ;;
    status)
      if [[ $# -ne 1 ]]; then
        refuse "status_takes_no_arguments" "$LINENO"
      fi
      do_status
      ;;
    *)
      refuse "unknown_mode" "$LINENO"
      ;;
  esac
}

main "$@"
