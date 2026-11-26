#!/usr/bin/env bash
# Install the host's policy file from reviewed source, or put a kept copy back.
#   scripts/install-etc.sh install <commit> [--past <commit>:<reason>]...
#   scripts/install-etc.sh check [<commit>]
#   scripts/install-etc.sh rollback <kept file>
# The one target is etc/agent-platform/policy.json -> /etc/agent-platform/policy.json. No other path
# of the host is ever written. AP_ETC_ROOT (default empty) prefixes every host path, for the tests.
# The script works in its own clone (AP_ETC_CLONE, default /var/lib/agent-platform/clones/agent-vps-operations),
# never in a main checkout of this repository, and reads the files with `git show`.
# Exit codes: 0 done (check: same), 1 check differs / a failure after a write began, 2 refused before any write.
# Order of install: lock, clone clean and fetched, <commit> on origin/main's first-parent history, the accepted
# check of every merge after the recorded commit, stage the file, `ap policy check` on the staged bytes, target
# checks, the kept copy (read back), the write by rename (read back), the record.
set -euo pipefail
umask 077
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR GIT_OBJECT_DIRECTORY \
  GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_NAMESPACE GIT_CEILING_DIRECTORIES

REPOSITORY=park-kyungchan/agent-vps-operations
SOURCE_REL=etc/agent-platform/policy.json
NAME=policy.json
PLATFORMS=/home/palantirkc/platforms

refuse() { printf '%s\n' "$*" >&2; exit 2; }
fail() { printf '%s\n' "$*" >&2; exit 1; }

# ---- the roots ---------------------------------------------------------------------------------
ROOT=${AP_ETC_ROOT:-}
if [[ -n $ROOT ]]; then
  [[ $ROOT == /* ]] || refuse "etc_root_invalid not_absolute"
  [[ $ROOT != */ ]] || refuse "etc_root_invalid trailing_slash"
  resolved=$(realpath -m -- "$ROOT") || refuse "etc_root_invalid unresolvable"
  case $resolved in
    /etc | /etc/*) refuse "etc_root_invalid under_etc" ;;
  esac
fi
TARGET_DIR=$ROOT/etc/agent-platform
TARGET=$TARGET_DIR/$NAME
STATE_DIR=$ROOT/var/lib/agent-platform/etc
LOCK=$STATE_DIR/lock
RECORD=$STATE_DIR/installed
BACKUP_DIR=$ROOT/var/backups/agent-platform-etc
CLONE=${AP_ETC_CLONE:-/var/lib/agent-platform/clones/agent-vps-operations}
SCRIPT_REPO=$(cd -- "$(dirname -- "$(realpath -- "${BASH_SOURCE[0]}")")/.." && pwd -P)

STAGE=
TMP_TARGET=
TMP_RECORD=
cleanup() {
  [[ -z $STAGE ]] || rm -rf -- "$STAGE"
  [[ -z $TMP_TARGET ]] || rm -f -- "$TMP_TARGET"
  [[ -z $TMP_RECORD ]] || rm -f -- "$TMP_RECORD"
}
trap cleanup EXIT

usage() {
  refuse "usage: install-etc.sh install <commit> [--past <commit>:<reason>]... | check [<commit>] | rollback <kept file>"
}

digest() { sha256sum -- "$1" | cut -d' ' -f1; }
is_commit() { [[ ${1:-} =~ ^[0-9a-f]{40}$ ]]; }
# A directory whose literal path is its real path: no symbolic link in any component.
plain_dir() { [[ -d $1 && ! -L $1 && "$(realpath -e -- "$1" 2>/dev/null)" == "$1" ]]; }

# ---- the lock and the record ---------------------------------------------------------------------
take_lock() {
  if [[ ! -e $STATE_DIR ]]; then
    mkdir -p -- "$(dirname -- "$STATE_DIR")"
    mkdir -m 0700 -- "$STATE_DIR"
  fi
  plain_dir "$STATE_DIR" || refuse "target_not_regular $STATE_DIR"
  [[ ! -L $LOCK ]] || refuse "target_not_regular $LOCK"
  exec 9>>"$LOCK"
  flock -n 9 || refuse "busy $LOCK"
}

REC_COMMIT=     # the installed commit, "none" when only a rollback wrote the record, empty without a record
REC_SHA=        # the digest of the policy file this script last wrote
read_record() {
  REC_COMMIT=
  REC_SHA=
  [[ -e $RECORD || -L $RECORD ]] || return 0
  [[ -f $RECORD && ! -L $RECORD ]] || refuse "target_not_regular $RECORD"
  local lines
  mapfile -t lines <"$RECORD"
  if [[ ${#lines[@]} -ne 2 ]] || ! [[ ${lines[0]} =~ ^commit\ ([0-9a-f]{40}|none)$ ]]; then
    refuse "record_invalid $RECORD"
  fi
  REC_COMMIT=${BASH_REMATCH[1]}
  if ! [[ ${lines[1]} =~ ^sha256\ $NAME\ ([0-9a-f]{64})$ ]]; then
    refuse "record_invalid $RECORD"
  fi
  REC_SHA=${BASH_REMATCH[1]}
}
write_record() { # <commit|none> <sha256>
  TMP_RECORD=$(mktemp "$STATE_DIR/.installed.XXXXXX") || return 1
  printf 'commit %s\nsha256 %s %s\n' "$1" "$NAME" "$2" >"$TMP_RECORD" || return 1
  mv -T -- "$TMP_RECORD" "$RECORD" || return 1
  TMP_RECORD=
}

# ---- the clone --------------------------------------------------------------------------------------
g() { git -C "$CLONE" --no-optional-locks -c core.hooksPath=/dev/null -c gc.auto=0 -c maintenance.auto=false "$@"; }

prepare_clone() {
  [[ $CLONE == /* ]] || refuse "clone_invalid not_absolute"
  local resolved
  resolved=$(realpath -m -- "$CLONE") || refuse "clone_invalid unresolvable"
  case $resolved/ in
    "$PLATFORMS"/*) refuse "clone_refused $CLONE" ;;
  esac
  case $CLONE/ in
    "$PLATFORMS"/*) refuse "clone_refused $CLONE" ;;
  esac
  if [[ ! -e $CLONE ]]; then
    local url=${AP_ETC_ORIGIN:-}
    if [[ -z $url ]]; then
      url=$(git -C "$SCRIPT_REPO" remote get-url origin 2>/dev/null) || refuse "origin_unknown"
    fi
    mkdir -p -- "$(dirname -- "$CLONE")"
    timeout 300 git clone --quiet -- "$url" "$CLONE" || refuse "clone_failed"
    chmod 0700 -- "$CLONE"
  fi
  [[ -d $CLONE/.git && ! -L $CLONE/.git ]] || refuse "clone_invalid not_a_clone $CLONE"
  [[ "$(g rev-parse --show-toplevel 2>/dev/null)" == "$(realpath -e -- "$CLONE")" ]] ||
    refuse "clone_invalid not_a_clone $CLONE"
  local dirty
  dirty=$(g status --porcelain --untracked-files=all) || refuse "clone_invalid status_failed"
  [[ -z $dirty ]] || refuse "clone_dirty $CLONE"
  timeout 300 git -C "$CLONE" -c core.hooksPath=/dev/null -c gc.auto=0 -c maintenance.auto=false \
    fetch --quiet --no-tags "${AP_ETC_ORIGIN:-origin}" '+refs/heads/main:refs/remotes/origin/main' ||
    refuse "fetch_failed"
}

FIRST_PARENT=() # first-parent commits of origin/main, newest first
TIP=
load_history() {
  local out
  TIP=$(g rev-parse --verify --quiet 'refs/remotes/origin/main^{commit}') || refuse "fetch_failed no_origin_main"
  out=$(g rev-list --first-parent "$TIP") || refuse "history_unreadable"
  mapfile -t FIRST_PARENT <<<"$out"
}
index_of() { # <commit>: its index in FIRST_PARENT, empty when absent
  local i
  for i in "${!FIRST_PARENT[@]}"; do
    if [[ ${FIRST_PARENT[$i]} == "$1" ]]; then
      printf '%s' "$i"
      return 0
    fi
  done
  return 0
}

source_ok() { # <commit>: the policy file is a regular blob there
  local line mode
  line=$(g ls-tree "$1" -- "$SOURCE_REL") || return 1
  mode=${line%% *}
  [[ $mode == 100644 || $mode == 100755 ]] && [[ $line == *" blob "* ]]
}

# ---- the accepted check --------------------------------------------------------------------------
# ap deliver's step 6 for this repository: a merge whose second parent has the merge's tree and for which
# the store holds a closed accepting review of kind packet for that head, this repository and the number of
# the merge's subject. A review that `ap unit run` carried to an updated head (`carried_from`, no
# `pull_request`) counts by the number of the review it was carried from: that review is read with `ont get`,
# must be of kind packet, closed, accept and of this repository, and holds the number or is itself carried,
# followed the same way, at most 20 reviews in all. A `pull_request` that is present is never looked past.
# The store is read through `ont search` and `ont get`; no GitHub read.
REVIEW_PY=$(
  cat <<'PY'
import json, subprocess, sys

repo, head, pr = sys.argv[1], sys.argv[2], int(sys.argv[3])


def field(o, k):
    return (((o.get("head") or {}).get("fields") or {}).get(k) or {}).get("value")


def complete(i):
    if not isinstance(i, dict) or not isinstance(i.get("namespace"), dict):
        return False
    names = [i.get("object_type"), i.get("native_key")]
    names += [i["namespace"].get("instance"), i["namespace"].get("account")]
    return all(isinstance(n, str) and n for n in names)


MAX_CHAIN = 20


def gfields(o):  # the fields of the newest observation of an `ont get` object
    obs = o.get("observations")
    if not isinstance(obs, list) or not obs or not isinstance(obs[-1], dict):
        raise ValueError("no observation")
    f = obs[-1].get("fields")
    if not isinstance(f, dict):
        raise ValueError("no fields")
    return f


def gvalue(f, k):
    return (f.get(k) or {}).get("value")


def is_number(n):
    return isinstance(n, int) and not isinstance(n, bool)


def followed(ref):
    """True when the chain of carried reviews that starts at ref ends at pull request pr."""
    seen = 1  # the carried review found by the search is the first of the at most MAX_CHAIN
    while True:
        if seen >= MAX_CHAIN or not isinstance(ref, dict) or ref.get("object_type") != "review":
            return False
        seen += 1
        try:
            p = subprocess.run(["ont", "get"], input=json.dumps({"id": ref}).encode(), capture_output=True, timeout=60)
            v = json.loads(p.stdout)
            ok = p.returncode == 0 and v["status"] == "ok"
            objects = v["result"]["objects"]
            f = gfields(objects[0]) if ok else None
        except Exception:
            sys.exit(2)
        if not ok:
            sys.exit(2)
        if not (
            gvalue(f, "kind") == "packet"
            and gvalue(f, "status") == "closed"
            and gvalue(f, "verdict") == "accept"
            and gvalue(f, "repository") == repo
        ):
            return False
        number = gvalue(f, "pull_request")
        if number is not None:
            return is_number(number) and number == pr
        ref = gvalue(f, "carried_from")


query = {
    "object_type": "review",
    "where": {"all": [{"eq": ["kind", "packet"]}, {"eq": ["head", head]}, {"eq": ["status", "closed"]}]},
    "limit": 200,
}
for _ in range(20):
    try:
        p = subprocess.run(["ont", "search"], input=json.dumps(query).encode(), capture_output=True, timeout=60)
        v = json.loads(p.stdout)
        result = v["result"]
        objects = result["objects"]
        ok = p.returncode == 0 and v["status"] == "ok" and result["basis"]["qualified"] is True
        ok = ok and isinstance(objects, list)
    except Exception:
        sys.exit(2)
    if not ok:
        sys.exit(2)
    for o in objects:
        number = field(o, "pull_request")
        if (
            field(o, "kind") == "packet"
            and field(o, "head") == head
            and field(o, "status") == "closed"
            and field(o, "verdict") == "accept"
            and field(o, "repository") == repo
            and complete(o.get("id"))
        ):
            if is_number(number) and number == pr:
                sys.exit(0)
            if number is None and field(o, "carried_from") is not None and followed(field(o, "carried_from")):
                sys.exit(0)
    cont = result.get("continuation")
    if cont is None:
        sys.exit(1)
    query["continuation"] = cont
sys.exit(2)
PY
)

SUBJECT_RE='^Merge pull request #([1-9][0-9]*) from '
accepted_one() { # <commit>: a merge, same tree as its second parent, a closed accepting review
  local commit=$1 parents trees subject pr rc=0
  parents=$(g rev-list --parents -n 1 "$commit") || refuse "not_accepted $commit"
  # shellcheck disable=SC2206
  parents=($parents)
  [[ ${#parents[@]} -eq 3 ]] || refuse "not_accepted $commit"
  trees=$(g rev-parse "$commit^{tree}" "$commit^2^{tree}") || refuse "not_accepted $commit"
  # shellcheck disable=SC2206
  trees=($trees)
  [[ ${#trees[@]} -eq 2 && ${trees[0]} == "${trees[1]}" ]] || refuse "tree_differs $commit"
  subject=$(g log -n 1 --format=%s "$commit") || refuse "not_accepted $commit"
  [[ $subject =~ $SUBJECT_RE ]] || refuse "not_accepted $commit"
  pr=${BASH_REMATCH[1]}
  python3 -I -c "$REVIEW_PY" "$REPOSITORY" "${parents[2]}" "$pr" || rc=$?
  case $rc in
    0) ;;
    1) refuse "not_accepted $commit" ;;
    *) refuse "store_unreadable $commit" ;;
  esac
}

PAST_COMMITS=()
PAST_REASONS=()
accepted_chain() { # <commit>: the commits after the recorded one up to <commit>
  local commit=$1 at rec_at i p found
  local chain=()
  at=$(index_of "$commit")
  [[ -n $at ]] || refuse "not_on_origin_main $commit"
  if [[ -n $REC_COMMIT && $REC_COMMIT != none ]]; then
    rec_at=$(index_of "$REC_COMMIT")
    [[ -n $rec_at ]] || refuse "record_invalid $REC_COMMIT"
    ((at < rec_at)) || refuse "not_forward $commit"
    for ((i = rec_at - 1; i >= at; i--)); do chain+=("${FIRST_PARENT[$i]}"); done
  else
    chain=("$commit")
  fi
  for p in "${PAST_COMMITS[@]}"; do
    found=false
    for i in "${chain[@]}"; do [[ $i == "$p" ]] && found=true; done
    [[ $found == true ]] || refuse "past_not_on_chain $p"
  done
  for i in "${chain[@]}"; do
    found=
    for p in "${!PAST_COMMITS[@]}"; do
      if [[ ${PAST_COMMITS[$p]} == "$i" && $i != "$commit" ]]; then found=$p; fi
    done
    if [[ -n $found ]]; then
      printf 'stepped_over %s %s\n' "$i" "${PAST_REASONS[$found]}"
      continue
    fi
    accepted_one "$i"
  done
}

# ---- target checks, the kept copy and the write ---------------------------------------------------------
target_checks() {
  plain_dir "$TARGET_DIR" || refuse "target_not_regular $TARGET"
  [[ -f $TARGET && ! -L $TARGET ]] || refuse "target_not_regular $TARGET"
}

# put_bytes <source file> <sha256 of it>: keep the current bytes, write the source over the target by
# rename, read it back. Sets KEPT. The target keeps its old bytes on every failure but a read-back
# mismatch, where the kept bytes go back by the same rename.
KEPT=
put_bytes() {
  local src=$1 sum=$2 stamp mode owner restore
  target_checks
  if [[ ! -e $BACKUP_DIR && ! -L $BACKUP_DIR ]]; then
    mkdir -p -- "$(dirname -- "$BACKUP_DIR")"
    mkdir -m 0700 -- "$BACKUP_DIR"
  fi
  plain_dir "$BACKUP_DIR" || refuse "target_not_regular $BACKUP_DIR"
  stamp=$(date -u +%Y%m%dT%H%M%SZ)
  KEPT=$BACKUP_DIR/$NAME.before-$stamp
  [[ ! -e $KEPT && ! -L $KEPT ]] || refuse "kept_exists $KEPT"
  (
    set -C
    cat -- "$TARGET" >"$KEPT"
  ) 2>/dev/null || refuse "kept_failed $KEPT"
  chmod 0600 -- "$KEPT"
  if ! cmp -s -- "$TARGET" "$KEPT"; then
    rm -f -- "$KEPT"
    refuse "kept_unreadable $KEPT"
  fi
  mode=$(stat -c %a -- "$TARGET")
  owner=$(stat -c %u:%g -- "$TARGET")
  TMP_TARGET=$(mktemp "$TARGET_DIR/.$NAME.XXXXXX")
  cat -- "$src" >"$TMP_TARGET"
  [[ "$(digest "$TMP_TARGET")" == "$sum" ]] || refuse "staged_changed $TARGET"
  chmod "$mode" -- "$TMP_TARGET"
  chown "$owner" -- "$TMP_TARGET"
  [[ -f $TARGET && ! -L $TARGET ]] || refuse "target_not_regular $TARGET"
  mv -T -- "$TMP_TARGET" "$TARGET"
  TMP_TARGET=
  if ! cmp -s -- "$src" "$TARGET"; then
    restore=$(mktemp "$TARGET_DIR/.$NAME.XXXXXX")
    TMP_TARGET=$restore
    cat -- "$KEPT" >"$restore"
    chmod "$mode" -- "$restore"
    chown "$owner" -- "$restore"
    mv -T -- "$restore" "$TARGET"
    TMP_TARGET=
    fail "readback_failed $TARGET"
  fi
}

verify() { # <file>: ap policy check, exit 0
  timeout 60 ap policy check "$1" >/dev/null || refuse "verify_failed $TARGET"
}

# ---- the forms -------------------------------------------------------------------------------------------
do_install() {
  local commit=$1 src sum cur
  take_lock
  read_record
  prepare_clone
  load_history
  accepted_chain "$commit"
  source_ok "$commit" || refuse "source_missing $commit:$SOURCE_REL"
  STAGE=$(mktemp -d "$CLONE/.git/etc-stage.XXXXXX")
  src=$STAGE/$NAME
  g cat-file blob "$commit:$SOURCE_REL" >"$src" || refuse "source_missing $commit:$SOURCE_REL"
  verify "$src"
  sum=$(digest "$src")
  target_checks
  cur=$(digest "$TARGET")
  if [[ $cur != "$sum" && ( -z $REC_SHA || $cur != "$REC_SHA" ) ]]; then
    refuse "target_changed $TARGET"
  fi
  put_bytes "$src" "$sum"
  write_record "$commit" "$sum" || fail "record_failed $RECORD: the target holds the new bytes; the kept copy is $KEPT"
  printf 'installed %s %s kept %s\n' "$TARGET" "$sum" "$KEPT"
}

do_check() {
  local commit=${1:-} src
  take_lock
  prepare_clone
  load_history
  if [[ -z $commit ]]; then
    commit=$TIP
  else
    [[ -n "$(index_of "$commit")" ]] || refuse "not_on_origin_main $commit"
  fi
  source_ok "$commit" || refuse "source_missing $commit:$SOURCE_REL"
  STAGE=$(mktemp -d "$CLONE/.git/etc-stage.XXXXXX")
  src=$STAGE/$NAME
  g cat-file blob "$commit:$SOURCE_REL" >"$src" || refuse "source_missing $commit:$SOURCE_REL"
  if [[ -f $TARGET && ! -L $TARGET ]] && cmp -s -- "$src" "$TARGET"; then
    echo same
    return 0
  fi
  echo "differs $TARGET"
  return 1
}

do_rollback() {
  local kept=$1 dir base sum sum_after
  take_lock
  read_record
  dir=$(dirname -- "$kept")
  base=$(basename -- "$kept")
  [[ $base =~ ^policy\.json\.before-[0-9]{8}T[0-9]{6}Z$ ]] || refuse "kept_invalid $kept"
  [[ $dir == "$TARGET_DIR" || $dir == "$BACKUP_DIR" ]] || refuse "kept_invalid $kept"
  plain_dir "$dir" || refuse "target_not_regular $kept"
  [[ -f $kept && ! -L $kept ]] || refuse "target_not_regular $kept"
  target_checks
  sum=$(digest "$kept")
  verify "$kept"
  sum_after=$(digest "$kept")
  [[ $sum == "$sum_after" ]] || refuse "kept_changed $kept"
  put_bytes "$kept" "$sum"
  write_record "${REC_COMMIT:-none}" "$sum" || fail "record_failed $RECORD: the target holds the kept bytes; the kept copy is $KEPT"
  printf 'installed %s %s kept %s\n' "$TARGET" "$sum" "$KEPT"
}

case ${1:-} in
  install)
    shift
    [[ $# -ge 1 ]] || usage
    commit=$1
    shift
    is_commit "$commit" || usage
    while [[ $# -gt 0 ]]; do
      [[ $1 == --past && $# -ge 2 ]] || usage
      pair=$2
      shift 2
      pc=${pair%%:*}
      reason=${pair#*:}
      is_commit "$pc" && [[ $pair == *:* && -n $reason && ${#reason} -le 200 ]] || usage
      [[ $reason != *[[:cntrl:]]* ]] || usage
      PAST_COMMITS+=("$pc")
      PAST_REASONS+=("$reason")
    done
    do_install "$commit"
    ;;
  check)
    shift
    [[ $# -le 1 ]] || usage
    [[ $# -eq 0 ]] || is_commit "$1" || usage
    do_check "${1:-}"
    ;;
  rollback)
    shift
    [[ $# -eq 1 ]] || usage
    do_rollback "$1"
    ;;
  *) usage ;;
esac
