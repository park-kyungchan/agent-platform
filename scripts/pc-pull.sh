#!/usr/bin/env bash
# Pull-side installer for the user's own account on the PC (never pushed by VPS;
# specs/ap/deliver.md R64, security rows 28 and 35).
#
# Modes:
#   1. GitHub-anchored source build (default):
#      scripts/pc-pull.sh [--ap-url <url>] [--ont-url <url>] [--commit <40-hex>]
#                         [--bin-dir <dir>] [--state-dir <dir>]
#   2. Digest-verified pair install:
#      scripts/pc-pull.sh --from-dir <dir> --ap-sha256 <64-hex> --ont-sha256 <64-hex>
#                         [--bin-dir <dir>]
set -euo pipefail
umask 077

DEFAULT_AP_URL="https://github.com/park-kyungchan/agent-platform.git"
DEFAULT_ONT_URL="https://github.com/park-kyungchan/ontology-platform.git"

refuse() {
  local code="$1" line="$2" msg="${3:-}"
  printf '%s@pc-pull.sh:%s%s\n' "$code" "$line" "${msg:+: $msg}" >&2
  exit 2
}

fail() {
  local code="$1" line="$2" msg="${3:-}"
  printf '%s@pc-pull.sh:%s%s\n' "$code" "$line" "${msg:+: $msg}" >&2
  exit 1
}

is_hex40() { [[ "$1" =~ ^[0-9a-f]{40}$ ]]; }
is_hex64() { [[ "$1" =~ ^[0-9a-f]{64}$ ]]; }

json_escape() {
  local s="$1"
  s="${s//\\/\\\\}"
  s="${s//\"/\\\"}"
  s="${s//$'\n'/\\n}"
  s="${s//$'\r'/\\r}"
  s="${s//$'\t'/\\t}"
  printf '%s' "$s"
}

file_sha256() {
  sha256sum -- "$1" | awk '{print $1}'
}

check_version() {
  local bin="$1" prog="$2" commit="$3"
  local line=""
  line=$("$bin" --version </dev/null 2>/dev/null | head -n 1) || return 1
  local w1="" w2="" w3="" extra=""
  read -r w1 w2 w3 extra <<<"$line" || return 1
  [[ "$w1" == "$prog" && -n "$w2" && "$w3" == "$commit" && -z "$extra" ]]
}

# Parse arguments.
ap_url=""
ont_url=""
commit=""
bin_dir=""
state_dir=""
from_dir=""
want_ap_sha=""
want_ont_sha=""
seen_build_flag=0
seen_pair_flag=0

require_val() {
  local flag="$1" remaining="$2"
  if [[ "$remaining" -lt 2 ]]; then
    refuse "invalid_args" "$LINENO" "missing value for $flag"
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --ap-url)
      require_val "$1" "$#"
      ap_url="$2"; seen_build_flag=1; shift 2
      ;;
    --ont-url)
      require_val "$1" "$#"
      ont_url="$2"; seen_build_flag=1; shift 2
      ;;
    --commit)
      require_val "$1" "$#"
      commit="$2"; seen_build_flag=1; shift 2
      ;;
    --state-dir)
      require_val "$1" "$#"
      state_dir="$2"; seen_build_flag=1; shift 2
      ;;
    --from-dir)
      require_val "$1" "$#"
      from_dir="$2"; seen_pair_flag=1; shift 2
      ;;
    --ap-sha256)
      require_val "$1" "$#"
      want_ap_sha="$2"; seen_pair_flag=1; shift 2
      ;;
    --ont-sha256)
      require_val "$1" "$#"
      want_ont_sha="$2"; seen_pair_flag=1; shift 2
      ;;
    --bin-dir)
      require_val "$1" "$#"
      bin_dir="$2"; shift 2
      ;;
    *)
      refuse "invalid_args" "$LINENO" "unknown argument: $1"
      ;;
  esac
done

if [[ "$seen_build_flag" -eq 1 && "$seen_pair_flag" -eq 1 ]]; then
  refuse "invalid_args" "$LINENO" "cannot mix source-build and --from-dir flags"
fi

if [[ -z "$bin_dir" ]]; then
  if [[ -z "${HOME:-}" ]]; then
    refuse "invalid_args" "$LINENO" "HOME is unset and --bin-dir was not given"
  fi
  bin_dir="$HOME/.local/bin"
fi
while [[ "$bin_dir" != / && "$bin_dir" == */ ]]; do
  bin_dir="${bin_dir%/}"
done
if [[ -z "$bin_dir" || "$bin_dir" != /* ]]; then
  refuse "invalid_args" "$LINENO" "--bin-dir must be an absolute path"
fi

# Track temporary resources for cleanup.
BUILD_TMP=""
AP_CLONE=""
ONT_CLONE=""
AP_WT=""
ONT_WT=""
STAGE_AP=""
STAGE_ONT=""

cleanup() {
  if [[ -n "$STAGE_AP" ]]; then rm -f -- "$STAGE_AP" 2>/dev/null || true; fi
  if [[ -n "$STAGE_ONT" ]]; then rm -f -- "$STAGE_ONT" 2>/dev/null || true; fi
  if [[ -n "$AP_WT" && -n "$AP_CLONE" && -d "$AP_CLONE/.git" ]]; then
    git -c core.hooksPath=/dev/null -c gc.auto=0 -C "$AP_CLONE" worktree remove --force "$AP_WT" >/dev/null 2>&1 || true
  fi
  if [[ -n "$ONT_WT" && -n "$ONT_CLONE" && -d "$ONT_CLONE/.git" ]]; then
    git -c core.hooksPath=/dev/null -c gc.auto=0 -C "$ONT_CLONE" worktree remove --force "$ONT_WT" >/dev/null 2>&1 || true
  fi
  if [[ -n "$BUILD_TMP" && -d "$BUILD_TMP" ]]; then
    rm -rf -- "$BUILD_TMP" 2>/dev/null || true
  fi
}
trap cleanup EXIT

# Atomic pair installer with .before-<sha12> backup, readback, and rollback.
install_pair() {
  local src_ap="$1" src_ont="$2" expect_ap_sha="$3" expect_ont_sha="$4" check_commit="${5:-}" check_pin="${6:-}"

  if [[ -L "$bin_dir" || ( -e "$bin_dir" && ! -d "$bin_dir" ) ]]; then
    fail "bin_dir_unusable" "$LINENO" "$bin_dir"
  fi
  if [[ ! -d "$bin_dir" ]]; then
    mkdir -p -- "$bin_dir" || fail "bin_dir_unusable" "$LINENO" "$bin_dir"
  fi
  for name in ap ont; do
    local dst="$bin_dir/$name"
    if [[ -L "$dst" || ( -e "$dst" && ! -f "$dst" ) ]]; then
      fail "target_not_regular" "$LINENO" "$dst"
    fi
  done

  STAGE_AP="$bin_dir/.ap.new.$$"
  STAGE_ONT="$bin_dir/.ont.new.$$"
  cp -- "$src_ap" "$STAGE_AP" || fail "stage_failed" "$LINENO" "ap"
  chmod 0755 -- "$STAGE_AP" || fail "stage_failed" "$LINENO" "ap"
  cp -- "$src_ont" "$STAGE_ONT" || fail "stage_failed" "$LINENO" "ont"
  chmod 0755 -- "$STAGE_ONT" || fail "stage_failed" "$LINENO" "ont"

  if [[ "$(file_sha256 "$STAGE_AP")" != "$expect_ap_sha" ]]; then
    fail "stage_digest_mismatch" "$LINENO" "ap"
  fi
  if [[ "$(file_sha256 "$STAGE_ONT")" != "$expect_ont_sha" ]]; then
    fail "stage_digest_mismatch" "$LINENO" "ont"
  fi

  local had_ap=0 had_ont=0
  local bk_ap="" bk_ont=""
  local old_ap_sha="" old_ont_sha=""
  if [[ -f "$bin_dir/ap" && ! -L "$bin_dir/ap" ]]; then
    old_ap_sha=$(file_sha256 "$bin_dir/ap")
    bk_ap="$bin_dir/ap.before-${old_ap_sha:0:12}"
    cp -p -- "$bin_dir/ap" "${bk_ap}.tmp.$$" || fail "backup_failed" "$LINENO" "ap"
    chmod 0755 -- "${bk_ap}.tmp.$$" || { rm -f -- "${bk_ap}.tmp.$$"; fail "backup_failed" "$LINENO" "ap"; }
    mv -f -- "${bk_ap}.tmp.$$" "$bk_ap" || { rm -f -- "${bk_ap}.tmp.$$"; fail "backup_failed" "$LINENO" "ap"; }
    had_ap=1
  fi
  if [[ -f "$bin_dir/ont" && ! -L "$bin_dir/ont" ]]; then
    old_ont_sha=$(file_sha256 "$bin_dir/ont")
    bk_ont="$bin_dir/ont.before-${old_ont_sha:0:12}"
    cp -p -- "$bin_dir/ont" "${bk_ont}.tmp.$$" || fail "backup_failed" "$LINENO" "ont"
    chmod 0755 -- "${bk_ont}.tmp.$$" || { rm -f -- "${bk_ont}.tmp.$$"; fail "backup_failed" "$LINENO" "ont"; }
    mv -f -- "${bk_ont}.tmp.$$" "$bk_ont" || { rm -f -- "${bk_ont}.tmp.$$"; fail "backup_failed" "$LINENO" "ont"; }
    had_ont=1
  fi

  rollback_pair() {
    local rb_ok=1
    if [[ "$had_ap" -eq 1 ]]; then
      if cp -p -- "$bk_ap" "$bin_dir/.ap.rb.$$" && chmod 0755 -- "$bin_dir/.ap.rb.$$" && mv -f -- "$bin_dir/.ap.rb.$$" "$bin_dir/ap"; then
        if [[ "$(file_sha256 "$bin_dir/ap" 2>/dev/null || true)" != "$old_ap_sha" ]]; then rb_ok=0; fi
      else
        rm -f -- "$bin_dir/.ap.rb.$$" 2>/dev/null || true
        rb_ok=0
      fi
    else
      rm -f -- "$bin_dir/ap" 2>/dev/null || true
    fi
    if [[ "$had_ont" -eq 1 ]]; then
      if cp -p -- "$bk_ont" "$bin_dir/.ont.rb.$$" && chmod 0755 -- "$bin_dir/.ont.rb.$$" && mv -f -- "$bin_dir/.ont.rb.$$" "$bin_dir/ont"; then
        if [[ "$(file_sha256 "$bin_dir/ont" 2>/dev/null || true)" != "$old_ont_sha" ]]; then rb_ok=0; fi
      else
        rm -f -- "$bin_dir/.ont.rb.$$" 2>/dev/null || true
        rb_ok=0
      fi
    else
      rm -f -- "$bin_dir/ont" 2>/dev/null || true
    fi
    return $(( rb_ok == 1 ? 0 : 1 ))
  }

  local moved=1
  if mv -f -- "$STAGE_ONT" "$bin_dir/ont"; then
    STAGE_ONT=""
  else
    moved=0
  fi
  if [[ "$moved" -eq 1 ]]; then
    if mv -f -- "$STAGE_AP" "$bin_dir/ap"; then
      STAGE_AP=""
    else
      moved=0
    fi
  fi

  local verified=0
  if [[ "$moved" -eq 1 ]]; then
    local got_ont_sha="" got_ap_sha=""
    got_ont_sha=$(file_sha256 "$bin_dir/ont" 2>/dev/null || true)
    got_ap_sha=$(file_sha256 "$bin_dir/ap" 2>/dev/null || true)
    if [[ "$got_ont_sha" == "$expect_ont_sha" && "$got_ap_sha" == "$expect_ap_sha" ]]; then
      verified=1
      if [[ -n "$check_pin" ]] && ! check_version "$bin_dir/ont" "ont" "$check_pin"; then
        verified=0
      fi
      if [[ -n "$check_commit" ]] && ! check_version "$bin_dir/ap" "ap" "$check_commit"; then
        verified=0
      fi
    fi
  fi

  if [[ "$verified" -ne 1 ]]; then
    if rollback_pair; then
      fail "readback_failed_restored" "$LINENO" "restored previous binaries"
    else
      fail "readback_failed" "$LINENO" "rollback incomplete"
    fi
  fi
}

if [[ "$seen_pair_flag" -eq 1 ]]; then
  # Mode 2: Digest-verified pair mode.
  if [[ -z "$from_dir" || -z "$want_ap_sha" || -z "$want_ont_sha" ]]; then
    refuse "invalid_args" "$LINENO" "--from-dir, --ap-sha256, and --ont-sha256 are all required together"
  fi
  if ! is_hex64 "$want_ap_sha"; then
    refuse "invalid_digest" "$LINENO" "--ap-sha256 must be 64 lowercase hex characters"
  fi
  if ! is_hex64 "$want_ont_sha"; then
    refuse "invalid_digest" "$LINENO" "--ont-sha256 must be 64 lowercase hex characters"
  fi
  src_ap="$from_dir/ap"
  src_ont="$from_dir/ont"
  if [[ ! -f "$src_ap" || ! -r "$src_ap" || ! -f "$src_ont" || ! -r "$src_ont" ]]; then
    refuse "digest_mismatch" "$LINENO" "expected ap and ont in $from_dir"
  fi
  got_ap_sha=$(file_sha256 "$src_ap")
  got_ont_sha=$(file_sha256 "$src_ont")
  if [[ "$got_ap_sha" != "$want_ap_sha" || "$got_ont_sha" != "$want_ont_sha" ]]; then
    refuse "digest_mismatch" "$LINENO" "sha256 mismatch before write"
  fi
  install_pair "$src_ap" "$src_ont" "$want_ap_sha" "$want_ont_sha"
  printf '{"status":"installed","mode":"pair","bin_dir":"%s","ap_sha256":"%s","ont_sha256":"%s"}\n' \
    "$(json_escape "$bin_dir")" "$want_ap_sha" "$want_ont_sha"
  exit 0
fi

# Mode 1: GitHub-anchored source build mode.
ap_url="${ap_url:-$DEFAULT_AP_URL}"
ont_url="${ont_url:-$DEFAULT_ONT_URL}"
if [[ "$ap_url" == -* || "$ont_url" == -* ]]; then
  refuse "invalid_args" "$LINENO" "repository URL may not start with '-'"
fi
if [[ -n "$commit" ]] && ! is_hex40 "$commit"; then
  refuse "invalid_commit" "$LINENO" "--commit must be 40 lowercase hex characters"
fi
if [[ -z "$state_dir" ]]; then
  if [[ -n "${XDG_STATE_HOME:-}" ]]; then
    state_dir="$XDG_STATE_HOME/agent-platform/pc-pull"
  elif [[ -n "${HOME:-}" ]]; then
    state_dir="$HOME/.local/state/agent-platform/pc-pull"
  else
    refuse "invalid_args" "$LINENO" "HOME is unset and --state-dir was not given"
  fi
fi
while [[ "$state_dir" != / && "$state_dir" == */ ]]; do
  state_dir="${state_dir%/}"
done
if [[ -z "$state_dir" || "$state_dir" != /* ]]; then
  refuse "invalid_args" "$LINENO" "--state-dir must be an absolute path"
fi

mkdir -p -- "$state_dir" || fail "state_dir_unusable" "$LINENO" "$state_dir"
AP_CLONE="$state_dir/agent-platform"
ONT_CLONE="$state_dir/ontology-platform"

git_safe() {
  git -c core.hooksPath=/dev/null -c gc.auto=0 "$@"
}

if [[ ! -d "$AP_CLONE/.git" ]]; then
  git_safe init --quiet "$AP_CLONE" || fail "fetch_failed" "$LINENO" "agent-platform init"
fi
git_safe -C "$AP_CLONE" fetch --quiet --no-tags "$ap_url" '+refs/heads/main:refs/remotes/origin/main' \
  || fail "fetch_failed" "$LINENO" "agent-platform fetch"

ap_tip=$(git_safe -C "$AP_CLONE" rev-parse --verify 'refs/remotes/origin/main^{commit}' 2>/dev/null) \
  || fail "fetch_failed" "$LINENO" "agent-platform origin/main"
if [[ -z "$commit" ]]; then
  commit="$ap_tip"
fi
if ! git_safe -C "$AP_CLONE" cat-file -e "${commit}^{commit}" 2>/dev/null \
  || ! git_safe -C "$AP_CLONE" merge-base --is-ancestor "$commit" refs/remotes/origin/main 2>/dev/null; then
  refuse "not_on_main" "$LINENO" "$commit"
fi

pin=$(git_safe -C "$AP_CLONE" show "${commit}:ontology/ENGINE-PIN" 2>/dev/null | tr -d '[:space:]') || pin=""
if ! is_hex40 "$pin"; then
  refuse "engine_pin_invalid" "$LINENO" "$commit"
fi

if [[ ! -d "$ONT_CLONE/.git" ]]; then
  git_safe init --quiet "$ONT_CLONE" || fail "fetch_failed" "$LINENO" "ontology-platform init"
fi
git_safe -C "$ONT_CLONE" fetch --quiet --no-tags "$ont_url" '+refs/heads/main:refs/remotes/origin/main' \
  || fail "fetch_failed" "$LINENO" "ontology-platform fetch"

if ! git_safe -C "$ONT_CLONE" cat-file -e "${pin}^{commit}" 2>/dev/null \
  || ! git_safe -C "$ONT_CLONE" merge-base --is-ancestor "$pin" refs/remotes/origin/main 2>/dev/null; then
  refuse "pin_not_on_main" "$LINENO" "$pin"
fi

BUILD_TMP=$(mktemp -d "${TMPDIR:-/tmp}/ap-pc-pull.XXXXXXXX")
chmod 0700 -- "$BUILD_TMP"
ONT_WT="$BUILD_TMP/ontology-platform"
AP_WT="$BUILD_TMP/agent-platform"
ont_target="$state_dir/target-ont"
ap_target="$state_dir/target-ap"
mkdir -p -- "$ont_target" "$ap_target"

git_safe -C "$ONT_CLONE" worktree add --quiet --detach "$ONT_WT" "$pin" \
  || fail "worktree_failed" "$LINENO" "ontology-platform"
git_safe -C "$AP_CLONE" worktree add --quiet --detach "$AP_WT" "$commit" \
  || fail "worktree_failed" "$LINENO" "agent-platform"

built_ont="$ont_target/release/ont"
built_ap="$ap_target/release/ap"

if [[ -n "${AP_PULL_BUILD_CMD:-}" ]]; then
  (cd "$ONT_WT" && CARGO_TARGET_DIR="$ont_target" "$AP_PULL_BUILD_CMD" ont "$ONT_WT" "$built_ont" "$pin" >&2) \
    || fail "build_failed" "$LINENO" "ont"
else
  (cd "$ONT_WT" && CARGO_TARGET_DIR="$ont_target" cargo build --release --locked -p ont >&2) \
    || fail "build_failed" "$LINENO" "ont"
fi
if [[ ! -x "$built_ont" ]] || ! check_version "$built_ont" "ont" "$pin"; then
  fail "ont_version_mismatch" "$LINENO" "$pin"
fi

if [[ -n "${AP_PULL_BUILD_CMD:-}" ]]; then
  (cd "$AP_WT" && CARGO_TARGET_DIR="$ap_target" ONT_BIN="$built_ont" "$AP_PULL_BUILD_CMD" ap "$AP_WT" "$built_ap" "$commit" >&2) \
    || fail "build_failed" "$LINENO" "ap"
else
  (cd "$AP_WT" && CARGO_TARGET_DIR="$ap_target" ONT_BIN="$built_ont" cargo build --release --locked -p ap >&2) \
    || fail "build_failed" "$LINENO" "ap"
fi
if [[ ! -x "$built_ap" ]] || ! check_version "$built_ap" "ap" "$commit"; then
  fail "ap_version_mismatch" "$LINENO" "$commit"
fi

ont_sha=$(file_sha256 "$built_ont")
ap_sha=$(file_sha256 "$built_ap")
install_pair "$built_ap" "$built_ont" "$ap_sha" "$ont_sha" "$commit" "$pin"

printf '{"status":"installed","mode":"build","bin_dir":"%s","commit":"%s","pin":"%s","ap_sha256":"%s","ont_sha256":"%s"}\n' \
  "$(json_escape "$bin_dir")" "$commit" "$pin" "$ap_sha" "$ont_sha"
