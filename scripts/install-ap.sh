#!/usr/bin/env bash
set -euo pipefail

INSTALLED="/usr/local/bin/ap"
LOCK_FILE="/home/palantirkc/work/ws-replan/wt/.host-cargo.lock"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"

refuse() {
  local code="$1"
  local line="$2"
  printf '%s@install-ap.sh:%s\n' "$code" "$line" >&2
  exit 1
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
    local third=""
    third="$(printf '%s\n' "$version_line" | awk '{print $3}')"
    installed_commit="$third"
  fi
  if [[ -n "$installed_commit" && "$installed_commit" == "$origin_main" ]]; then
    printf '%s\n' "equal"
  else
    printf '%s\n' "differs"
  fi
  exit 0
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
  local head=""
  head="$(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || true)"
  if [[ -z "$head" ]]; then
    refuse "head_unavailable" "$LINENO"
  fi
  if [[ "$head" != "$expected" ]]; then
    refuse "head_mismatch" "$LINENO"
  fi
  local origin_main=""
  origin_main="$(git -C "$REPO_ROOT" rev-parse origin/main 2>/dev/null || true)"
  if [[ -z "$origin_main" ]]; then
    refuse "origin_main_unavailable" "$LINENO"
  fi
  if [[ "$head" != "$origin_main" ]]; then
    refuse "head_not_origin_main" "$LINENO"
  fi
  local status_output=""
  status_output="$(git -C "$REPO_ROOT" status --porcelain 2>/dev/null || true)"
  local -a allowed=()
  if [[ -n "${AP_INSTALL_ALLOW_DIRTY:-}" ]]; then
    IFS=':' read -r -a allowed <<< "$AP_INSTALL_ALLOW_DIRTY"
  fi
  if [[ -n "$status_output" ]]; then
    local line=""
    while IFS= read -r line; do
      if [[ -z "$line" ]]; then
        continue
      fi
      local path="${line:3}"
      if [[ "$path" == *" -> "* ]]; then
        path="${path##* -> }"
      fi
      path="${path%\"}"
      path="${path#\"}"
      local is_allowed=0
      local entry=""
      for entry in "${allowed[@]:-}"; do
        if [[ -n "$entry" && "$entry" == "$path" ]]; then
          is_allowed=1
          break
        fi
      done
      if [[ "$is_allowed" -ne 1 ]]; then
        refuse "dirty_tree" "$LINENO"
      fi
    done <<< "$status_output"
  fi
  if ! (cd "$REPO_ROOT" && flock "$LOCK_FILE" env CARGO_TARGET_DIR="$CARGO_TARGET_DIR" CARGO_BUILD_JOBS=2 cargo build --release -p ap --locked); then
    refuse "build_failed" "$LINENO"
  fi
  local built="$CARGO_TARGET_DIR/release/ap"
  if [[ ! -x "$built" ]]; then
    refuse "built_binary_missing" "$LINENO"
  fi
  if [[ -f "$INSTALLED" ]]; then
    local current_sha=""
    current_sha="$(sha256sum "$INSTALLED" | awk '{print $1}')"
    if [[ -z "$current_sha" ]]; then
      refuse "installed_sha_unavailable" "$LINENO"
    fi
    local backup="/usr/local/bin/ap.before-${current_sha:0:7}"
    if ! cp "$INSTALLED" "$backup"; then
      refuse "backup_failed" "$LINENO"
    fi
  fi
  local tmp=""
  tmp="$(mktemp /usr/local/bin/.ap.install.XXXXXX)"
  if ! install -m 0755 "$built" "$tmp"; then
    rm -f "$tmp"
    refuse "install_failed" "$LINENO"
  fi
  if ! mv "$tmp" "$INSTALLED"; then
    rm -f "$tmp"
    refuse "install_move_failed" "$LINENO"
  fi
  local new_version=""
  new_version="$("$INSTALLED" --version 2>/dev/null || true)"
  new_version="${new_version%%$'\n'*}"
  local new_commit=""
  new_commit="$(printf '%s\n' "$new_version" | awk '{print $3}')"
  local new_base="${new_commit%-dirty}"
  if [[ "$new_base" != "$expected" ]]; then
    refuse "installed_version_mismatch" "$LINENO"
  fi
}

do_rollback() {
  if [[ $# -ne 1 ]]; then
    refuse "rollback_requires_before_file" "$LINENO"
  fi
  local before="$1"
  if [[ ! -f "$before" ]]; then
    refuse "rollback_source_missing" "$LINENO"
  fi
  local tmp=""
  tmp="$(mktemp /usr/local/bin/.ap.rollback.XXXXXX)"
  if ! install -m 0755 "$before" "$tmp"; then
    rm -f "$tmp"
    refuse "rollback_install_failed" "$LINENO"
  fi
  if ! mv "$tmp" "$INSTALLED"; then
    rm -f "$tmp"
    refuse "rollback_move_failed" "$LINENO"
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
