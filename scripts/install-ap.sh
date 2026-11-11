#!/usr/bin/env bash
set -euo pipefail

INSTALL_DIR="${AP_INSTALL_DIR:-/usr/local/bin}"
INSTALLED="$INSTALL_DIR/ap"
LOCK_FILE="/home/palantirkc/work/ws-replan/wt/.host-cargo.lock"
INSTALL_LOCK_FILE="/run/lock/ap-install.lock"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
WORKTREE_DIR=""

refuse() {
  local code="$1"
  local line="$2"
  printf '%s@install-ap.sh:%s\n' "$code" "$line" >&2
  exit 1
}

cleanup_worktree() {
  if [[ -n "$WORKTREE_DIR" && -d "$WORKTREE_DIR" ]]; then
    git -C "$REPO_ROOT" worktree remove --force "$WORKTREE_DIR" >/dev/null 2>&1 || rm -rf "$WORKTREE_DIR"
  fi
}

acquire_install_lock() {
  exec 9>"$INSTALL_LOCK_FILE"
  if ! flock -n 9; then
    refuse "install_busy" "$LINENO"
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

do_install() {
  acquire_install_lock
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
  if [[ -f "$INSTALLED" ]]; then
    had_previous=1
    local current_sha=""
    current_sha="$(sha256sum "$INSTALLED" | awk '{print $1}')"
    if [[ -z "$current_sha" ]]; then
      refuse "installed_sha_unavailable" "$LINENO"
    fi
    backup="$INSTALL_DIR/ap.before-${current_sha:0:7}"
    if ! cp "$INSTALLED" "$backup"; then
      refuse "backup_failed" "$LINENO"
    fi
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
    if [[ "$had_previous" -eq 1 && -n "$backup" && -f "$backup" ]]; then
      printf '%s\n' "$backup" >&2
      local restore_tmp=""
      restore_tmp="$(mktemp "$INSTALL_DIR/.ap.restore.XXXXXX" || true)"
      if [[ -n "$restore_tmp" ]] && install -m 0755 "$backup" "$restore_tmp"; then
        mv "$restore_tmp" "$INSTALLED" || rm -f "$restore_tmp"
      else
        [[ -n "$restore_tmp" ]] && rm -f "$restore_tmp"
      fi
    else
      rm -f "$INSTALLED"
    fi
    refuse "installed_version_mismatch_restored" "$LINENO"
  fi
  trap - EXIT
  cleanup_worktree
  WORKTREE_DIR=""
}

do_rollback() {
  acquire_install_lock
  if [[ $# -ne 1 ]]; then
    refuse "rollback_requires_before_file" "$LINENO"
  fi
  local before="$1"
  if [[ ! -f "$before" || -L "$before" ]]; then
    refuse "rollback_source_missing" "$LINENO"
  fi
  local base=""
  base="$(basename -- "$before")"
  if [[ ! "$base" =~ ^ap\.before-[0-9a-f]{7}$ ]]; then
    refuse "rollback_invalid_before_file" "$LINENO"
  fi
  local before_dir=""
  before_dir="$(cd -- "$(dirname -- "$before")" && pwd)"
  local install_dir_norm=""
  install_dir_norm="$(cd -- "$INSTALL_DIR" && pwd)"
  if [[ "$before_dir" != "$install_dir_norm" ]]; then
    refuse "rollback_invalid_before_file" "$LINENO"
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
    *)
      refuse "unknown_mode" "$LINENO"
      ;;
  esac
}

main "$@"
