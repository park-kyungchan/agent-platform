#!/usr/bin/env bash
# Tests of scripts/install-etc.sh. Everything runs in scratch directories from mktemp -d: the real git with a
# scratch bare repository as origin (file://), fakes of `ont`, `ap`, `cmp` and `date` first on PATH, and a
# scratch AP_ETC_ROOT. No network, no root, no host file.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
script=$repo_root/scripts/install-etc.sh

while IFS= read -r name; do
  unset "$name"
done < <(env | sed -n 's/^\(GIT_[A-Za-z0-9_]*\)=.*/\1/p')
unset AP_ETC_ROOT AP_ETC_CLONE AP_ETC_ORIGIN

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
export HOME=$scratch/home
mkdir -p "$HOME" "$scratch/bin"

root=$scratch/root
clone=$scratch/clone
origin=$scratch/origin.git
target=$root/etc/agent-platform/policy.json
state=$root/var/lib/agent-platform/etc
record=$state/installed
kept_dir=$root/var/backups/agent-platform-etc
export AP_ETC_ROOT=$root AP_ETC_CLONE=$clone AP_ETC_ORIGIN=file://$origin
export FAKE_ONT_DB=$scratch/reviews.jsonl
: >"$FAKE_ONT_DB"

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

if sys.argv[1:] != ["search"]:
    sys.exit(2)
if os.environ.get("FAKE_ONT_FAIL"):
    print("{}")
    sys.exit(1)
q = json.load(sys.stdin)
assert q["object_type"] == "review"
rows = [json.loads(line) for line in open(os.environ["FAKE_ONT_DB"]) if line.strip()]
conds = q["where"]["all"]
assert sorted(c["eq"][0] for c in conds) == ["head", "kind", "status"], conds
chosen = [r for r in rows if all(r.get(c["eq"][0]) == c["eq"][1] for c in conds)]
page = int(os.environ.get("FAKE_ONT_PAGE", "0")) or 1000
start = int(q.get("continuation") or 0)
chunk = chosen[start:start + page]
more = str(start + page) if start + page < len(chosen) else None
objects = []
for r in chunk:
    fields = {k: {"kind": "value", "value": v} for k, v in r.items() if k != "key"}
    ident = {"object_type": "review", "namespace": {"instance": "local", "account": "root"}, "native_key": r["key"]}
    objects.append({"id": ident, "head": {"fields": fields}})
print(json.dumps({"status": "ok", "result": {"basis": {"qualified": True}, "objects": objects, "continuation": more}}))
EOF
chmod 0755 "$scratch/bin/ap" "$scratch/bin/cmp" "$scratch/bin/date" "$scratch/bin/ont"
export PATH=$scratch/bin:$PATH

# ---- the history of the scratch origin ----------------------------------------------------------------------
policy_text() { printf 'policy version %s\n' "$1"; }
sha_of() { sha256sum | cut -d' ' -f1; }
declare -A M H # merge commit and head of merge n
work=$scratch/work
git init -q -b main "$work"
git init -q --bare -b main "$origin"
git -C "$work" remote add origin "file://$origin"
mkdir -p "$work/etc/agent-platform"
policy_text 0 >"$work/etc/agent-platform/policy.json"
git -C "$work" add -A
git -C "$work" commit -q -m "base"

add_review() { # <key> <kind> <head> <status> <verdict> <repository> <pull_request>
  printf '{"key":"%s","kind":"%s","head":"%s","status":"%s","verdict":"%s","repository":"%s","pull_request":%s}\n' \
    "$@" >>"$FAKE_ONT_DB"
}
repo=park-kyungchan/agent-vps-operations
merge_pr() { # <n> <extra file or ->: a feature branch with policy version n, merged without a fast-forward
  local n=$1 branch=feature-$1
  git -C "$work" checkout -q -b "$branch" main
  if [[ $n == 10 ]]; then
    printf 'INVALID\n' >"$work/etc/agent-platform/policy.json"
  else
    policy_text "$n" >"$work/etc/agent-platform/policy.json"
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
  if [[ $n == 2 ]]; then merge_pr 2 etc/ssh/x; else merge_pr "$n" -; fi
done
policy_text 13 >"$work/etc/agent-platform/policy.json"
git -C "$work" commit -q -a -m "a direct commit"
M[13]=$(git -C "$work" rev-parse HEAD)
git -C "$work" push -q origin main
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
inst() { run bash "$script" "$@"; }
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
ln -s /home/palantirkc/platforms "$scratch/pl"
for bad in /home/palantirkc/platforms/agent-vps-operations /home/palantirkc/platforms "$scratch/pl/agent-vps-operations"; do
  AP_ETC_CLONE=$bad inst check
  ok "AP_ETC_CLONE=$bad is refused" rc_is 2
  ok "AP_ETC_CLONE=$bad names clone_refused" contains "$ERR" clone_refused
done
ok "the script never runs systemctl" not grep -q 'systemctl [a-z]' "$script"

# ---- check ---------------------------------------------------------------------------------------------------------
set_state - 1
before=$(sig)
inst check "${M[1]}"
ok "check at the merge of the starting file prints same" equals "$OUT" same
ok "check same exits 0" rc_is 0
ok "the first check made the clone with mode 0700" equals "$(mode_of "$clone")" 700
ok "the lock directory has mode 0700" equals "$(mode_of "$state")" 700
inst check "${M[2]}"
ok "check against another commit prints differs <target>" equals "$OUT" "differs $target"
ok "check differs exits 1" rc_is 1
inst check
ok "check at origin/main (a later file) differs" equals "$OUT" "differs $target"
ok "check wrote nothing but the lock and the clone" equals "$(sig)" "$before"
inst check "${H[1]}"
ok "check refuses a commit off the first-parent history" rc_is 2
ok "check names not_on_origin_main" contains "$ERR" not_on_origin_main
inst check "$other_head"
ok "check refuses an unknown commit" contains "$ERR" not_on_origin_main
echo junk >"$clone/junk"
inst check
ok "a dirty clone is refused" rc_is 2
ok "a dirty clone names clone_dirty" contains "$ERR" clone_dirty
rm -f -- "$clone/junk"
AP_ETC_ORIGIN=file://$scratch/nowhere.git inst check
ok "a failed fetch is refused" rc_is 2
ok "a failed fetch names fetch_failed" contains "$ERR" fetch_failed
ok "no staging directory is left in the clone" not compgen -G "$clone/.git/etc-stage.*"
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

# ---- the starting point: no record, the target equals the source ----------------------------------------------------
FAKE_DATE=20261101T000001Z inst install "${M[1]}"
ok "the starting install succeeds" rc_is 0
d1=$(policy_text 1 | sha_of)
ok "the starting install prints one line with the digest and the kept copy" \
  equals "$OUT" "installed $target $d1 kept $kept_dir/policy.json.before-20261101T000001Z"
ok "the starting install changes no byte" target_is 1
ok "the record holds the merge and the digest" \
  equals "$(<"$record")" "$(printf 'commit %s\nsha256 policy.json %s' "${M[1]}" "$d1")"
ok "the record has mode 0600" equals "$(mode_of "$record")" 600
ok "the kept copy has mode 0600" equals "$(mode_of "$kept_dir/policy.json.before-20261101T000001Z")" 600
ok "the kept directory has mode 0700" equals "$(mode_of "$kept_dir")" 700
ok "the target kept its mode" equals "$(mode_of "$target")" 644
ok "install left no staging directory" not compgen -G "$clone/.git/etc-stage.*"

# ---- an install and its read-back; a commit planting etc/ssh/x --------------------------------------------------------
FAKE_DATE=20261101T000002Z inst install "${M[2]}"
ok "an install of the next accepted merge succeeds" rc_is 0
d2=$(policy_text 2 | sha_of)
ok "the install prints the digest and the kept copy" \
  equals "$OUT" "installed $target $d2 kept $kept_dir/policy.json.before-20261101T000002Z"
ok "the target holds the new bytes" target_is 2
ok "the target kept its mode" equals "$(mode_of "$target")" 644
ok "the kept copy holds the previous bytes" \
  equals "$(<"$kept_dir/policy.json.before-20261101T000002Z")" "$(policy_text 1)"
ok "a planted etc/ssh/x wrote nothing" not test -e "$root/etc/ssh"
ok "a planted etc/ssh/x wrote nothing elsewhere under the root" \
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
ok "a head off the first-parent history is refused" contains "$ERR" not_on_origin_main
inst install "$other_head"
ok "an unknown commit is refused" contains "$ERR" not_on_origin_main
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
ok "verify_failed left no staging directory" not compgen -G "$clone/.git/etc-stage.*"

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
# a legacy kept file in the target's directory; no record; no clone needed
rm -f -- "$record"
policy_text 7 >"$root/etc/agent-platform/policy.json.before-20261118T014522Z"
rm -rf -- "$clone"
AP_ETC_ORIGIN=file://$scratch/nowhere.git FAKE_DATE=20261101T000032Z inst rollback "$root/etc/agent-platform/policy.json.before-20261118T014522Z"
ok "rollback of a kept file in /etc/agent-platform needs no clone and no fetch" rc_is 0
ok "rollback made no clone" not test -e "$clone"
ok "rollback put the legacy bytes back" target_is 7
ok "rollback without a record writes commit none and the digest" \
  equals "$(<"$record")" "$(printf 'commit none\nsha256 policy.json %s' "$(policy_text 7 | sha_of)")"
FAKE_DATE=20261101T000033Z inst install "${M[12]}"
ok "install after a rollback with no commit recorded checks the commit alone" rc_is 0
ok "and checks the target against the recorded digest" target_is 12
ok "no staging directory is left after the cases" not compgen -G "$clone/.git/etc-stage.*"

if [[ $failures -ne 0 ]]; then
  printf 'test-install-etc: failed (%d of %d)\n' "$failures" "$checks" >&2
  exit 1
fi
printf 'test-install-etc: ok (%d checks)\n' "$checks"
