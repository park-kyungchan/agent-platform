#!/usr/bin/env bash
# Tests of host/scripts/install-etc.sh. Everything runs in scratch directories from mktemp -d: the real git with a
# scratch repository whose main the clone fetches as delivery's clone does (by URL, no remote), fakes of `ont`, `ap`,
# `cmp` and `date` first on PATH, and a scratch AP_ETC_ROOT. The script runs with the clone as its working
# directory. No network, no root, no host file.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
script=$repo_root/scripts/install-etc.sh

while IFS= read -r name; do
  unset "$name"
done < <(env | sed -n 's/^\(GIT_[A-Za-z0-9_]*\)=.*/\1/p')
unset AP_ETC_ROOT

scratch=$(mktemp -d)
scratch=$(cd -P "$scratch" && pwd)
trap 'rm -rf -- "$scratch"' EXIT

failures=0
checks=0
ok() { # <description> <command...>: the command must succeed
  local description=$1
  shift
  checks=$((checks + 1))
  if ! "$@"; then
    printf 'test-install-etc: FAIL %s\n' "$description" >&2
    failures=$((failures + 1))
  fi
}
not() { ! "$@"; }
equals() { [[ "$1" == "$2" ]]; }
contains() { [[ "$1" == *"$2"* ]]; }

# ---- the scratch world ---------------------------------------------------------------------------------
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.invalid
export GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.invalid
export HOME=$scratch/home TMPDIR=$scratch/tmp
mkdir -p "$HOME" "$scratch/bin" "$TMPDIR"

root=$scratch/root
clone=$scratch/clone
target=$root/etc/agent-platform/policy.json
state=$root/var/lib/agent-platform/etc
record=$state/installed
kept_dir=$root/var/backups/agent-platform-etc
export AP_ETC_ROOT=$root
export FAKE_ONT_DB=$scratch/reviews.jsonl
: >"$FAKE_ONT_DB"
export FAKE_ONT_GETS=$scratch/gets.log
: >"$FAKE_ONT_GETS"

cat >"$scratch/bin/ap" <<'EOF'
#!/usr/bin/env bash
[[ $# -eq 3 && $1 == policy && $2 == check ]] || exit 2
if grep -q INVALID -- "$3"; then
  echo '{"error":{"code":"policy_invalid"}}' >&2
  exit 2
fi
echo '{"status":"ok"}'
EOF
cat >"$scratch/bin/cmp" <<'EOF'
#!/usr/bin/env bash
if [[ -n ${FAKE_CMP_BAD:-} && ${!#} == "$FAKE_CMP_BAD" ]]; then exit 1; fi
exec /usr/bin/cmp "$@"
EOF
cat >"$scratch/bin/date" <<'EOF'
#!/usr/bin/env bash
if [[ -n ${FAKE_DATE:-} && $* == "-u +%Y%m%dT%H%M%SZ" ]]; then echo "$FAKE_DATE"; exit 0; fi
exec /usr/bin/date "$@"
EOF
cat >"$scratch/bin/ont" <<'EOF'
#!/usr/bin/env -S python3 -I
import json, os, sys

verb = sys.argv[1:]
if verb not in (["search"], ["get"]):
    sys.exit(2)
if os.environ.get("FAKE_ONT_FAIL") or (verb == ["get"] and os.environ.get("FAKE_ONT_GET_FAIL")):
    print("{}")
    sys.exit(1)
q = json.load(sys.stdin)
rows = [json.loads(line) for line in open(os.environ["FAKE_ONT_DB"]) if line.strip()]


def ident(key):
    return {"object_type": "review", "namespace": {"instance": "local", "account": "root"}, "native_key": key}


def fields_of(r):
    out = {}
    for k, v in r.items():
        if k != "key":
            out[k] = {"kind": "value", "value": ident(v) if k == "carried_from" else v}
    return out


if verb == ["get"]:
    with open(os.environ["FAKE_ONT_GETS"], "a") as log:
        log.write(q["id"]["native_key"] + "\n")
    assert sorted(q) == ["id"] and q["id"] == ident(q["id"]["native_key"]), q
    stale = {"fields": {"kind": {"kind": "value", "value": "stale"}}}
    objects = [
        {"id": q["id"], "observations": [stale, {"fields": fields_of(r)}]}
        for r in rows
        if r["key"] == q["id"]["native_key"]
    ]
    print(json.dumps({"status": "ok", "result": {"objects": objects}}))
    sys.exit(0)
assert q["object_type"] == "review"
conds = q["where"]["all"]
assert sorted(c["eq"][0] for c in conds) == ["head", "kind", "status"], conds
chosen = [r for r in rows if all(r.get(c["eq"][0]) == c["eq"][1] for c in conds)]
page = int(os.environ.get("FAKE_ONT_PAGE", "0")) or 1000
start = int(q.get("continuation") or 0)
chunk = chosen[start:start + page]
more = str(start + page) if start + page < len(chosen) else None
objects = [{"id": ident(r["key"]), "head": {"fields": fields_of(r)}} for r in chunk]
print(json.dumps({"status": "ok", "result": {"basis": {"qualified": True}, "objects": objects, "continuation": more}}))
EOF
chmod 0755 "$scratch/bin/ap" "$scratch/bin/cmp" "$scratch/bin/date" "$scratch/bin/ont"
export PATH=$scratch/bin:$PATH

# ---- the history of the scratch repository ------------------------------------------------------------------
policy_text() { printf 'policy version %s\n' "$1"; }
sha_of() { sha256sum | cut -d' ' -f1; }
declare -A M H # merge commit and head of merge n
work=$scratch/work
git init -q -b main "$work"
mkdir -p "$work/host/etc/agent-platform"
policy_text 0 >"$work/host/etc/agent-platform/policy.json"
git -C "$work" add -A
git -C "$work" commit -q -m "base"

add_review() { # <key> <kind> <head> <status> <verdict> <repository> <pull_request>
  printf '{"key":"%s","kind":"%s","head":"%s","status":"%s","verdict":"%s","repository":"%s","pull_request":%s}\n' \
    "$@" >>"$FAKE_ONT_DB"
}
repo=park-kyungchan/agent-platform
merge_pr() { # <n> <extra file or ->: a feature branch with policy version n, merged without a fast-forward
  local n=$1 branch=feature-$1
  git -C "$work" checkout -q -b "$branch" main
  if [[ $n == 10 ]]; then
    printf 'INVALID\n' >"$work/host/etc/agent-platform/policy.json"
  else
    policy_text "$n" >"$work/host/etc/agent-platform/policy.json"
  fi
  if [[ ${2:--} != - ]]; then
    mkdir -p "$(dirname "$work/$2")"
    echo planted >"$work/$2"
  fi
  git -C "$work" add -A
  git -C "$work" commit -q -m "change $n"
  H[$n]=$(git -C "$work" rev-parse HEAD)
  git -C "$work" checkout -q main
  if [[ $n == 9 ]]; then
    git -C "$work" merge -q --no-ff --no-commit "$branch" >/dev/null 2>&1
    echo extra >"$work/extra.txt"
    git -C "$work" add -A
    git -C "$work" commit -q -m "Merge pull request #$n from owner/$branch"
  else
    git -C "$work" merge -q --no-ff -m "Merge pull request #$n from owner/$branch" "$branch"
  fi
  M[$n]=$(git -C "$work" rev-parse HEAD)
}
for n in 1 2 3 4 5 6 7 8 9 10 11 12; do
  if [[ $n == 2 ]]; then merge_pr 2 host/etc/ssh/x; else merge_pr "$n" -; fi
done
policy_text 13 >"$work/host/etc/agent-platform/policy.json"
git -C "$work" commit -q -a -m "a direct commit"
M[13]=$(git -C "$work" rev-parse HEAD)
# The clone as delivery makes it: git init, a fetch of main by URL into refs/remotes/origin/main, no remote, and a
# checkout of the tip.
refresh() {
  git init -q "$clone"
  git -C "$clone" fetch -q --no-tags "file://$work" '+refs/heads/main:refs/remotes/origin/main'
  git -C "$clone" checkout -q --force --detach refs/remotes/origin/main
}
refresh
other_head=$(printf 'f%.0s' {1..40})
add_review r1 packet "${H[1]}" closed accept "$repo" 1
add_review r2 packet "${H[2]}" closed accept "$repo" 2
add_review r3 design "${H[3]}" closed accept "$repo" 3 # another kind
add_review r4 packet "$other_head" closed accept "$repo" 4 # another head
add_review r5 packet "${H[5]}" closed reject "$repo" 5
add_review r6 packet "${H[6]}" running accept "$repo" 6
add_review r7 packet "${H[7]}" closed accept other/repository 7
add_review r8 packet "${H[8]}" closed accept "$repo" 99 # another pull request
add_review r9 packet "${H[9]}" closed accept "$repo" 9
add_review r10 packet "${H[10]}" closed accept "$repo" 10
add_review r12a packet "${H[12]}" closed reject "$repo" 12
add_review r12b packet "${H[12]}" closed accept "$repo" 12

# ---- helpers of the cases -------------------------------------------------------------------------------------
OUT= ERR= RC=0
run() {
  RC=0
  OUT=$("$@" 2>"$scratch/err") || RC=$?
  ERR=$(<"$scratch/err")
}
inst() { run_in "$clone" bash "$script" "$@"; }
run_in() { # <directory> <command...>: run in that working directory
  local dir=$1
  shift
  RC=0
  OUT=$(cd "$dir" && "$@" 2>"$scratch/err") || RC=$?
  ERR=$(<"$scratch/err")
}
rc_is() { [[ $RC == "$1" ]]; }

set_state() { # <commit or -> <policy version>: the target and the record as an earlier install left them
  mkdir -p "$root/etc/agent-platform" "$state"
  chmod 0700 "$state"
  policy_text "$2" >"$target"
  chmod 0644 "$target"
  if [[ $1 == - ]]; then
    rm -f -- "$record"
  else
    printf 'commit %s\nsha256 policy.json %s\n' "$1" "$(policy_text "$2" | sha_of)" >"$record"
  fi
}
sig() { # the whole state a refused call must leave alone
  sha256sum <"$target"
  cat -- "$record" 2>/dev/null || echo no-record
  ls -1 "$kept_dir" 2>/dev/null || echo no-kept-dir
}
target_is() { equals "$(<"$target")" "$(policy_text "$1")"; }
mode_of() { stat -c %a -- "$1"; }

# ---- usage and the roots -----------------------------------------------------------------------------------------
inst
ok "no form is a usage refusal" rc_is 2
inst install abc123
ok "an abbreviated commit is a usage refusal" rc_is 2
inst install "${M[1]}" --past "${M[2]}"
ok "a past without a reason is a usage refusal" rc_is 2
inst rollback
ok "rollback without a file is a usage refusal" rc_is 2
inst check one two
ok "check with two arguments is a usage refusal" rc_is 2
ln -s /etc "$scratch/etclink"
for bad in "$root/" /etc /etc/x "$scratch/etclink" "$scratch/etclink/x" relative/root; do
  AP_ETC_ROOT=$bad inst check
  ok "AP_ETC_ROOT=$bad is refused" rc_is 2
  ok "AP_ETC_ROOT=$bad names etc_root_invalid" contains "$ERR" etc_root_invalid
done
ok "no root, no state: nothing was made" not test -e "$root"
mkdir -p "$scratch/plain"
run_in "$scratch/plain" bash "$script" check
ok "a working directory outside a Git repository is refused" rc_is 2
ok "it names repository_invalid" contains "$ERR" repository_invalid
ok "the clone has no remote" equals "$(git -C "$clone" remote | wc -l)" 0
ok "the script reads no remote and makes no clone" not grep -qE '^[^#]*(git clone|fetch |remote get-url)' "$script"
ok "the script never runs systemctl" not grep -q 'systemctl [a-z]' "$script"

# ---- check ---------------------------------------------------------------------------------------------------------
set_state - 1
before=$(sig)
inst check "${M[1]}"
ok "check at the merge of the starting file prints same" equals "$OUT" same
ok "check same exits 0" rc_is 0
ok "the lock directory has mode 0700" equals "$(mode_of "$state")" 700
inst check "${M[2]}"
ok "check against another commit prints differs <target>" equals "$OUT" "differs $target"
ok "check differs exits 1" rc_is 1
inst check
ok "check of the clone's HEAD (a later file) differs" equals "$OUT" "differs $target"
ok "check wrote nothing but the lock" equals "$(sig)" "$before"
inst check "$other_head"
ok "check refuses an unknown commit" rc_is 2
ok "check names commit_unknown" contains "$ERR" commit_unknown
ok "check wrote nothing into the clone" equals "$(git -C "$clone" status --porcelain --untracked-files=all)" ""
ok "no staging directory is left" not compgen -G "$TMPDIR/etc-stage.*"
rm -f -- "$target"
inst check "${M[1]}"
ok "check with an absent target differs" rc_is 1
set_state - 1

# ---- the lock --------------------------------------------------------------------------------------------------------
run flock -n "$state/lock" env AP_ETC_ROOT="$root" bash "$script" check
ok "check refuses busy while another holds the lock" rc_is 2
ok "check names busy" contains "$ERR" busy
run flock -n "$state/lock" bash "$script" install "${M[1]}"
ok "install refuses busy" contains "$ERR" busy
run flock -n "$state/lock" bash "$script" rollback "$kept_dir/policy.json.before-20261101T000000Z"
ok "rollback refuses busy" contains "$ERR" busy

# spec: specs/ap/deliver.md#R73
# ---- the starting point: no record, the target equals the source ----------------------------------------------------
before_inode=$(stat -c %i -- "$target")
FAKE_DATE=20261101T000001Z inst install "${M[1]}"
ok "the starting install succeeds" rc_is 0
d1=$(policy_text 1 | sha_of)
ok "the starting install of equal bytes prints one line with the digest and no kept copy" \
  equals "$OUT" "installed $target $d1 kept none"
ok "the starting install changes no byte" target_is 1
ok "equal bytes are not written: the target is the same file" equals "$(stat -c %i -- "$target")" "$before_inode"
ok "equal bytes keep no copy" not test -e "$kept_dir"
ok "the record holds the merge and the digest" \
  equals "$(<"$record")" "$(printf 'commit %s\nsha256 policy.json %s' "${M[1]}" "$d1")"
ok "the record has mode 0600" equals "$(mode_of "$record")" 600
ok "the target kept its mode" equals "$(mode_of "$target")" 644
ok "install left no staging directory" not compgen -G "$TMPDIR/etc-stage.*"

# ---- an install and its read-back; a commit planting etc/ssh/x --------------------------------------------------------
FAKE_DATE=20261101T000002Z inst install "${M[2]}"
ok "an install of the next accepted merge succeeds" rc_is 0
d2=$(policy_text 2 | sha_of)
ok "the install prints the digest and the kept copy" \
  equals "$OUT" "installed $target $d2 kept $kept_dir/policy.json.before-20261101T000002Z"
ok "the target holds the new bytes" target_is 2
ok "the target kept its mode" equals "$(mode_of "$target")" 644
ok "the kept copy has mode 0600" equals "$(mode_of "$kept_dir/policy.json.before-20261101T000002Z")" 600
ok "the kept directory has mode 0700" equals "$(mode_of "$kept_dir")" 700
ok "the kept copy holds the previous bytes" \
  equals "$(<"$kept_dir/policy.json.before-20261101T000002Z")" "$(policy_text 1)"
ok "a planted host/etc/ssh/x wrote nothing" not test -e "$root/etc/ssh"
ok "a planted host/etc/ssh/x wrote nothing elsewhere under the root" \
  equals "$(cd "$root" && find . -name x | wc -l)" 0
ok "the record names the new merge and digest" \
  equals "$(<"$record")" "$(printf 'commit %s\nsha256 policy.json %s' "${M[2]}" "$d2")"
inst check "${M[2]}"
ok "check is same after the install" equals "$OUT" same

# ---- not_forward ------------------------------------------------------------------------------------------------------
before=$(sig)
inst install "${M[2]}"
ok "a commit equal to the recorded one is refused" rc_is 2
ok "a commit equal to the recorded one names not_forward" contains "$ERR" not_forward
inst install "${M[1]}"
ok "a commit before the recorded one names not_forward" contains "$ERR" not_forward
ok "not_forward writes nothing" equals "$(sig)" "$before"

# spec: specs/ap/deliver.md#R74
# ---- the accepted check -------------------------------------------------------------------------------------------------
for n in 3 4 5 6 7 8; do
  set_state "${M[$((n - 1))]}" "$((n - 1))"
  before=$(sig)
  inst install "${M[$n]}"
  ok "merge $n has no closed accepting review: exit 2" rc_is 2
  ok "merge $n is not_accepted" equals "$ERR" "not_accepted ${M[$n]}"
  ok "merge $n writes nothing" equals "$(sig)" "$before"
done
set_state "${M[8]}" 8
before=$(sig)
inst install "${M[9]}"
ok "a merge whose tree differs from its head's is refused" equals "$ERR" "tree_differs ${M[9]}"
ok "tree_differs writes nothing" equals "$(sig)" "$before"
set_state "${M[2]}" 2
inst install "${M[4]}"
ok "the chain after the recorded commit is walked: merge 3 first" equals "$ERR" "not_accepted ${M[3]}"
set_state "${M[12]}" 12
inst install "${M[13]}"
ok "a commit that is not a merge is not_accepted" equals "$ERR" "not_accepted ${M[13]}"
set_state "${M[2]}" 2
inst install "${H[1]}"
ok "a head that does not hold the recorded commit is not_forward" contains "$ERR" not_forward
inst install "$other_head"
ok "an unknown commit is refused" contains "$ERR" commit_unknown
set_state - 5
before=$(sig)
inst install "${M[5]}"
ok "with no record the commit alone is checked, so merge 5 is not_accepted" equals "$ERR" "not_accepted ${M[5]}"
set_state - 12
printf 'commit %s\n' "${M[1]}" >"$record"
inst install "${M[12]}"
ok "a malformed record is refused" contains "$ERR" record_invalid
printf 'commit %s\nsha256 policy.json %s\n' "${M[12]}" "$(policy_text 12 | sha_of)" >"$record"
inst install "${M[12]}"
ok "a record naming the commit makes it not_forward" contains "$ERR" not_forward
printf 'commit %s\nsha256 policy.json %s\n' "$other_head" "$(policy_text 12 | sha_of)" >"$record"
inst install "${M[12]}"
ok "a record naming a commit the repository does not hold is record_invalid" contains "$ERR" record_invalid
set_state "${M[11]}" 11
FAKE_ONT_FAIL=1 inst install "${M[12]}"
ok "an unreadable store is refused as store_unreadable" contains "$ERR" store_unreadable
ok "an unreadable store is exit 2" rc_is 2
FAKE_ONT_PAGE=1 FAKE_DATE=20261101T000012Z inst install "${M[12]}"
ok "an accepting review found on the second page of the search is found" rc_is 0
ok "merge 12 is installed" target_is 12

# ---- past ---------------------------------------------------------------------------------------------------------------
set_state "${M[10]}" 10
before=$(sig)
inst install "${M[12]}"
ok "an unreviewed merge in the range stops the install" equals "$ERR" "not_accepted ${M[11]}"
inst install "${M[12]}" --past "${M[3]}:out of range"
ok "a named commit not in the range is past_not_on_chain" equals "$ERR" "past_not_on_chain ${M[3]}"
inst install "${M[12]}" --past "${M[12]}:the commit itself"
ok "past never steps over the commit itself: the range still stops at merge 11" \
  equals "$ERR" "not_accepted ${M[11]}"
ok "the refusals above wrote nothing" equals "$(sig)" "$before"
FAKE_DATE=20261101T000013Z inst install "${M[12]}" --past "${M[11]}:a hand merge"
ok "install with past succeeds" rc_is 0
ok "the output names the commit stepped over with its reason" \
  contains "$OUT" "stepped_over ${M[11]} a hand merge"
ok "the output ends with the installed line" contains "$OUT" "installed $target "
ok "merge 12 is installed after the past" target_is 12
set_state "${M[11]}" 11
FAKE_DATE=20261101T000014Z inst install "${M[12]}" --past "${M[12]}:ignored"
ok "naming the commit itself in past changes nothing when it is accepted" rc_is 0
ok "and nothing is stepped over" not contains "$OUT" stepped_over

# ---- the verify -----------------------------------------------------------------------------------------------------------
set_state "${M[9]}" 9
before=$(sig)
inst install "${M[10]}"
ok "a policy file ap refuses is verify_failed" equals "$ERR" "$(printf '{"error":{"code":"policy_invalid"}}\nverify_failed %s' "$target")"
ok "verify_failed is exit 2" rc_is 2
ok "verify_failed writes nothing: no kept copy, the old bytes, the old record" equals "$(sig)" "$before"
ok "verify_failed left no staging directory" not compgen -G "$TMPDIR/etc-stage.*"

# ---- the target ---------------------------------------------------------------------------------------------------------
set_state "${M[11]}" 11
echo "a hand edit" >"$target"
before=$(sig)
inst install "${M[12]}"
ok "a hand-edited target is target_changed" equals "$ERR" "target_changed $target"
ok "target_changed is exit 2" rc_is 2
ok "target_changed writes nothing" equals "$(sig)" "$before"
set_state - 5
inst install "${M[12]}"
ok "with no record a target that differs from the source is target_changed" equals "$ERR" "target_changed $target"
set_state - 12
FAKE_DATE=20261101T000015Z inst install "${M[12]}"
ok "with no record a target equal to the source is the starting point" rc_is 0
set_state "${M[11]}" 11
mv -- "$target" "$scratch/real-target"
ln -s "$scratch/real-target" "$target"
before=$(sig)
inst install "${M[12]}"
ok "a symbolic link target is target_not_regular" equals "$ERR" "target_not_regular $target"
ok "the symbolic link target left the link and its file alone" \
  equals "$(readlink "$target") $(<"$scratch/real-target")" "$scratch/real-target $(policy_text 11)"
rm -f -- "$target"
mv -- "$scratch/real-target" "$target"
rm -f -- "$target"
inst install "${M[12]}"
ok "an absent target is target_not_regular" contains "$ERR" target_not_regular
set_state "${M[11]}" 11
mv -- "$root/etc/agent-platform" "$root/etc/real-agent-platform"
ln -s real-agent-platform "$root/etc/agent-platform"
inst install "${M[12]}"
ok "a target directory behind a symbolic link is target_not_regular" contains "$ERR" target_not_regular
rm -f -- "$root/etc/agent-platform"
mv -- "$root/etc/real-agent-platform" "$root/etc/agent-platform"
mv -- "$kept_dir" "$scratch/real-kept"
ln -s "$scratch/real-kept" "$kept_dir"
before=$(sig)
inst install "${M[12]}"
ok "a kept directory behind a symbolic link is target_not_regular" contains "$ERR" target_not_regular
ok "and the target kept its bytes" target_is 11
rm -f -- "$kept_dir"
mv -- "$scratch/real-kept" "$kept_dir"
mv -- "$state" "$scratch/real-state"
ln -s "$scratch/real-state" "$state"
inst install "${M[12]}"
ok "a record directory behind a symbolic link is target_not_regular" contains "$ERR" target_not_regular
rm -f -- "$state"
mv -- "$scratch/real-state" "$state"
ok "the state was put back" equals "$(sig)" "$before"

# ---- the kept copy and the read-back ---------------------------------------------------------------------------------------
set_state "${M[11]}" 11
: >"$kept_dir/policy.json.before-20261101T000020Z"
before=$(sig)
FAKE_DATE=20261101T000020Z inst install "${M[12]}"
ok "an existing kept name is refused" contains "$ERR" kept_exists
ok "an existing kept name leaves the target and the record" equals "$(sig)" "$before"
rm -f -- "$kept_dir/policy.json.before-20261101T000020Z"
before=$(sig)
FAKE_CMP_BAD=$target FAKE_DATE=20261101T000021Z inst install "${M[12]}"
ok "a read-back mismatch exits 1" rc_is 1
ok "a read-back mismatch names readback_failed" equals "$ERR" "readback_failed $target"
ok "a read-back mismatch puts the kept bytes back" target_is 11
ok "a read-back mismatch keeps the target's mode" equals "$(mode_of "$target")" 644
ok "a read-back mismatch leaves the record" equals "$(<"$record")" "$(printf 'commit %s\nsha256 policy.json %s' "${M[11]}" "$(policy_text 11 | sha_of)")"
ok "a read-back mismatch leaves the kept copy" test -f "$kept_dir/policy.json.before-20261101T000021Z"
ok "a read-back mismatch leaves no temporary file" equals "$(find "$root/etc/agent-platform" -name '.policy.json.*' | wc -l)" 0

# ---- rollback ----------------------------------------------------------------------------------------------------------------
set_state "${M[11]}" 11
FAKE_DATE=20261101T000030Z inst install "${M[12]}"
kept=$kept_dir/policy.json.before-20261101T000030Z
ok "the install for the rollback case worked" target_is 12
before_commit=$(head -n 1 "$record")
FAKE_DATE=20261101T000031Z inst rollback "$kept"
ok "rollback succeeds" rc_is 0
ok "rollback prints one line" equals "$OUT" "installed $target $(policy_text 11 | sha_of) kept $kept_dir/policy.json.before-20261101T000031Z"
ok "rollback puts the kept bytes back" target_is 11
ok "rollback keeps the current bytes first" \
  equals "$(<"$kept_dir/policy.json.before-20261101T000031Z")" "$(policy_text 12)"
ok "rollback leaves the installed commit" equals "$(head -n 1 "$record")" "$before_commit"
ok "rollback updates the digest" contains "$(<"$record")" "$(policy_text 11 | sha_of)"
inst check "${M[12]}"
ok "check shows the difference after a rollback" rc_is 1
ok "the rolled-back file keeps its mode" equals "$(mode_of "$target")" 644
inst install "${M[12]}"
ok "install after a rollback is not_forward (the record names the commit)" contains "$ERR" not_forward
before=$(sig)
inst rollback "$scratch/policy.json.before-20261101T000030Z"
ok "rollback refuses a file outside the two directories" contains "$ERR" kept_invalid
cp -- "$kept" "$scratch/policy.json.before-20261101T000030Z"
inst rollback "$scratch/policy.json.before-20261101T000030Z"
ok "rollback refuses a file outside the two directories, even a copy" contains "$ERR" kept_invalid
inst rollback "$kept_dir/policy.json"
ok "rollback refuses a name that is not policy.json.before-<stamp>" contains "$ERR" kept_invalid
ln -s "$kept" "$kept_dir/policy.json.before-20261101T000099Z"
inst rollback "$kept_dir/policy.json.before-20261101T000099Z"
ok "rollback refuses a symbolic link" contains "$ERR" target_not_regular
rm -f -- "$kept_dir/policy.json.before-20261101T000099Z"
printf 'INVALID\n' >"$kept_dir/policy.json.before-20261101T000098Z"
inst rollback "$kept_dir/policy.json.before-20261101T000098Z"
ok "rollback runs the verify on the kept file" equals "$ERR" "$(printf '{"error":{"code":"policy_invalid"}}\nverify_failed %s' "$target")"
rm -f -- "$kept_dir/policy.json.before-20261101T000098Z"
ok "the refused rollbacks wrote nothing" equals "$(sig)" "$before"
# a legacy kept file in the target's directory; no record; no repository needed
rm -f -- "$record"
policy_text 7 >"$root/etc/agent-platform/policy.json.before-20261118T014522Z"
FAKE_DATE=20261101T000032Z run_in "$scratch/plain" bash "$script" rollback "$root/etc/agent-platform/policy.json.before-20261118T014522Z"
ok "rollback of a kept file in /etc/agent-platform needs no repository" rc_is 0
ok "rollback put the legacy bytes back" target_is 7
ok "rollback without a record writes commit none and the digest" \
  equals "$(<"$record")" "$(printf 'commit none\nsha256 policy.json %s' "$(policy_text 7 | sha_of)")"
FAKE_DATE=20261101T000033Z inst install "${M[12]}"
ok "install after a rollback with no commit recorded checks the commit alone" rc_is 0
ok "and checks the target against the recorded digest" target_is 12
ok "no staging directory is left after the cases" not compgen -G "$TMPDIR/etc-stage.*"

# ---- a review that ap unit run carried to an updated head ----------------------------------------------------------
# A carried review has carried_from and no pull_request: it counts by the number of the review it was carried from,
# read with ont get. The merges 14 to 26 are added to the repository after every case above.
add_row() { # <key> <kind> <head> <status> <verdict> <repository> <pull_request or -> <carried_from or ->
  python3 -I -c '
import json, sys
k, kind, head, status, verdict, repository, number, carried = sys.argv[1:]
r = {"key": k, "kind": kind, "head": head, "status": status, "verdict": verdict, "repository": repository}
if number != "-":
    r["pull_request"] = int(number)
if carried != "-":
    r["carried_from"] = carried
print(json.dumps(r))' "$@" >>"$FAKE_ONT_DB"
}
carried_chain() { # <n> <length>: reviews c<n>_1 .. c<n>_<length>, the first of the head of merge n, the last with number n
  local n=$1 len=$2 i h
  for ((i = 1; i <= len; i++)); do
    h=$other_head
    [[ $i != 1 ]] || h=${H[$n]}
    if ((i < len)); then
      add_row "c${n}_$i" packet "$h" closed accept "$repo" - "c${n}_$((i + 1))"
    else
      add_row "c${n}_$i" packet "$h" closed accept "$repo" "$n" -
    fi
  done
}
for n in 14 15 16 17 18 19 20 21 22 23 24 25 26; do merge_pr "$n" -; done
refresh
add_row c14 packet "${H[14]}" closed accept "$repo" - o14 # one step
add_row o14 packet "$other_head" closed accept "$repo" 14 -
add_row c15 packet "${H[15]}" closed accept "$repo" - m15 # two steps
add_row m15 packet "$other_head" closed accept "$repo" - o15
add_row o15 packet "$other_head" closed accept "$repo" 15 -
add_row c16 packet "${H[16]}" closed accept "$repo" - o16 # the origin has another number
add_row o16 packet "$other_head" closed accept "$repo" 99 -
add_row c17 packet "${H[17]}" closed accept "$repo" - o17 # the origin has no number and no carried_from
add_row o17 packet "$other_head" closed accept "$repo" - -
carried_chain 18 21 # a chain of 21 reviews
carried_chain 19 20 # a chain of 20 reviews
add_row c20 packet "${H[20]}" closed accept "$repo" - o20 # the origin is not closed
add_row o20 packet "$other_head" running accept "$repo" 20 -
add_row c21 packet "${H[21]}" closed accept "$repo" - o21 # the origin does not accept
add_row o21 packet "$other_head" closed reject "$repo" 21 -
add_row c22 packet "${H[22]}" closed accept "$repo" - o22 # the origin is of another repository
add_row o22 packet "$other_head" closed accept other/repository 22 -
add_row c23 packet "${H[23]}" closed accept "$repo" 99 o23 # a number that is present is not looked past
add_row o23 packet "$other_head" closed accept "$repo" 23 -
add_row c24 packet "${H[24]}" closed accept "$repo" - o24 # the origin is of another kind
add_row o24 design "$other_head" closed accept "$repo" 24 -
add_row c25 packet "${H[25]}" closed accept "$repo" - nowhere25 # the origin is not in the store
add_row c26 packet "${H[26]}" closed accept "$repo" - o26 # a get that fails
add_row o26 packet "$other_head" closed accept "$repo" 26 -

carried_state() { # <n>: the record at the merge before n
  set_state "${M[$(($1 - 1))]}" "$(($1 - 1))"
  : >"$FAKE_ONT_GETS"
}
carried_state 14
FAKE_DATE=20261103T000014Z inst install "${M[14]}"
ok "a carried review whose origin has the merge's number installs" rc_is 0
ok "and installs the merge's file" target_is 14
ok "the origin was read with one get" equals "$(<"$FAKE_ONT_GETS")" o14
carried_state 15
FAKE_DATE=20261103T000015Z inst install "${M[15]}"
ok "a carried chain of two installs" rc_is 0
ok "and installs the merge's file (chain of two)" target_is 15
ok "the chain of two was read with two gets" equals "$(<"$FAKE_ONT_GETS")" "$(printf 'm15\no15')"
carried_state 19
FAKE_DATE=20261103T000019Z inst install "${M[19]}"
ok "a carried chain of 20 reviews installs" rc_is 0
ok "and installs the merge's file (chain of 20)" target_is 19
for n in 16 17 18 20 21 22 23 24; do
  carried_state "$n"
  before=$(sig)
  inst install "${M[$n]}"
  ok "merge $n (carried, not accepted by the number): exit 2" rc_is 2
  ok "merge $n is not_accepted" equals "$ERR" "not_accepted ${M[$n]}"
  ok "merge $n wrote nothing" equals "$(sig)" "$before"
done
carried_state 25
before=$(sig)
inst install "${M[25]}"
ok "a carried_from that names no review in the store is store_unreadable" contains "$ERR" store_unreadable
ok "a carried_from that names no review exits 2" rc_is 2
ok "a carried_from that names no review wrote nothing" equals "$(sig)" "$before"
carried_state 26
before=$(sig)
FAKE_ONT_GET_FAIL=1 inst install "${M[26]}"
ok "a failing get is store_unreadable" equals "$ERR" "store_unreadable ${M[26]}"
ok "a failing get exits 2" rc_is 2
ok "a failing get wrote nothing" equals "$(sig)" "$before"
carried_state 26
FAKE_DATE=20261103T000026Z inst install "${M[26]}"
ok "the same merge installs when the get answers" rc_is 0

# ---- a recorded commit of the imported history ----------------------------------------------------------------------
# The policy came from another repository whose history a merge read into this one: that merge's second parent holds
# the commit the old installer recorded. Such a commit is forward; the chain starts at the first main merge whose
# first parent does not hold it, here the merge of the pull request that imported the history.
old=$scratch/old
git init -q -b main "$old"
mkdir -p "$old/etc/agent-platform"
policy_text 26 >"$old/etc/agent-platform/policy.json"
git -C "$old" add -A
git -C "$old" commit -q -m "old base"
policy_text 26 >"$old/etc/agent-platform/policy.json"
echo more >"$old/more.txt"
git -C "$old" add -A
git -C "$old" commit -q -m "old tip"
old_tip=$(git -C "$old" rev-parse HEAD)
old_base=$(git -C "$old" rev-parse HEAD~1)
git -C "$work" fetch -q "$old" main
git -C "$work" checkout -q -b feature-27 main
git -C "$work" merge -q -s ours --no-commit --allow-unrelated-histories FETCH_HEAD >/dev/null 2>&1
git -C "$work" read-tree --prefix=old/ -u FETCH_HEAD
git -C "$work" commit -q -m "import the old history"
H[27]=$(git -C "$work" rev-parse HEAD)
git -C "$work" checkout -q main
git -C "$work" merge -q --no-ff -m "Merge pull request #27 from owner/feature-27" feature-27
M[27]=$(git -C "$work" rev-parse HEAD)
merge_pr 28 -
refresh
add_review r27 packet "${H[27]}" closed accept "$repo" 27
add_review r28 packet "${H[28]}" closed accept "$repo" 28
ok "the old tip is no first-parent commit of main" \
  not grep -qx "$old_tip" <(git -C "$clone" rev-list --first-parent refs/remotes/origin/main)
set_state "$old_tip" 26
: >"$FAKE_ONT_GETS"
FAKE_DATE=20261104T000028Z inst install "${M[28]}"
ok "a recorded commit reached through a second parent is forward" rc_is 0
ok "the import's merge and the next one were judged and the file installed" target_is 28
ok "the record follows the merge" equals "$(head -n 1 "$record")" "commit ${M[28]}"
set_state "$old_base" 26
FAKE_DATE=20261104T000029Z inst install "${M[27]}"
ok "an older commit of the imported history is forward too, its chain the import's merge alone" rc_is 0
ok "the import's merge holds the file of merge 26" target_is 26
set_state "$old_tip" 26
sed -i '/"key":"r27"/d' "$FAKE_ONT_DB"
before=$(sig)
inst install "${M[28]}"
ok "the import's merge is judged: without its review the install is not_accepted" equals "$ERR" "not_accepted ${M[27]}"
ok "and writes nothing" equals "$(sig)" "$before"
add_review r27 packet "${H[27]}" closed accept "$repo" 27
set_state "${M[27]}" 27
mkdir -p "$scratch/unrelated"
git -C "$scratch/unrelated" init -q -b main
echo x >"$scratch/unrelated/x"
git -C "$scratch/unrelated" add -A
git -C "$scratch/unrelated" commit -q -m unrelated
git -C "$clone" fetch -q "$scratch/unrelated" main
unrelated=$(git -C "$clone" rev-parse FETCH_HEAD)
printf 'commit %s\nsha256 policy.json %s\n' "$unrelated" "$(policy_text 27 | sha_of)" >"$record"
before=$(sig)
inst install "${M[28]}"
ok "a recorded commit the repository holds but that is no ancestor is not_forward" contains "$ERR" not_forward
ok "not_forward of an unrelated record writes nothing" equals "$(sig)" "$before"

if [[ $failures -ne 0 ]]; then
  printf 'test-install-etc: failed (%d of %d)\n' "$failures" "$checks" >&2
  exit 1
fi
printf 'test-install-etc: ok (%d checks)\n' "$checks"
