#!/usr/bin/env bash
# Delivers ONE pull request head: required check, automatic reviewer, ONE verifier, and the merge of
# exactly that head. It replaces the Lead's hand steps; every decision it takes is a rule written here.
#   deliver.sh <owner/repo> <pr> <head sha> <role> <cwd> <verifier prompt> <verifier out> [options]
# Options:
#   --merge               the Lead read the diff: merge when the verifier accepts
#   --merge-if-no-effect  merge on accept only when every changed path is a test or documentation
#                         (decision lead-diff-reading); otherwise stop with accepted_effect_needs_lead
#   --resume              continue a cut verifier (review.sh --resume) instead of a fresh one
#   --check NAME          the required check (default rust)
#   --no-autoreview       the repository has no automatic reviewer
#   --unit PAGE           the unit's page, copied into the usage line
#   --usage FILE          where the usage line is appended (default <cwd>/usage.jsonl)
# Without --merge or --merge-if-no-effect it stops after an accept (accepted_not_merged).
# The last line of stdout is the status. Exit: 0 merged, or accepted and not merged; 2 usage;
# 10 the head moved; 11 the check failed; 12 the automatic reviewer has unresolved findings or did not
# answer; 13 the verdict is not an accept for this head; 14 the merge was not read back; 75 the verifier
# was cut (run again with --resume). After every run that reached the verifier ONE line is appended to
# the usage file: time, repository, pull request, head, verdict, verifier cost, merge.
set -uo pipefail
[ "$#" -ge 7 ] || { echo "usage: deliver.sh <owner/repo> <pr> <head> <role> <cwd> <prompt> <out> [options]" >&2; exit 2; }
R=$1; N=$2; H=$3; ROLE=$4; CWD=$5; PROMPT=$6; OUT=$7; shift 7
MODE=stop; RESUME=""; CHECK=rust; AUTO=1; UNIT=""; USAGE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --merge) MODE=merge ;;
    --merge-if-no-effect) MODE=noeffect ;;
    --resume) RESUME=--resume ;;
    --no-autoreview) AUTO=0 ;;
    --check) CHECK=${2:?}; shift ;;
    --unit) UNIT=${2:?}; shift ;;
    --usage) USAGE=${2:?}; shift ;;
    *) echo "usage: unknown option $1" >&2; exit 2 ;;
  esac
  shift
done
[[ "$H" =~ ^[0-9a-f]{40}$ ]] || { echo "usage: the head is a full commit id" >&2; exit 2; }
HERE=$(cd "$(dirname "$(realpath "$0")")" && pwd -P)
CWD=$(realpath -- "$CWD"); OUT=$(realpath -m -- "$OUT"); USAGE=${USAGE:-$CWD/usage.jsonl}
POLL=${DELIVER_POLL:-20}
now() { date -u +%FT%TZ; }
stop() { echo "$2"; exit "$1"; }   # the status line is the last line of stdout
head_now() { gh pr view "$N" -R "$R" --json headRefOid --jq .headRefOid; }
VERDICT=none; MERGED=""; EFFECT=unknown
usage_line() {
  python3 -I - "$USAGE" "$(now)" "$R" "$N" "$H" "$UNIT" "$ROLE" "$VERDICT" "$MERGED" "$MODE" "$EFFECT" "$OUT.run.json" <<'PY'
import json, sys
path, when, repo, pr, head, unit, role, verdict, merged, mode, effect, run = sys.argv[1:13]
try: r = json.load(open(run))
except Exception: r = {}
line = {"at": when, "repository": repo, "pull_request": int(pr), "head": head, "unit": unit or None,
        "verifier": {"role": role, "turns": r.get("turns"), "cost_usd": r.get("cost_usd"), "cut": r.get("cut")},
        "verdict": verdict, "merge_mode": mode, "no_effect": {"yes": True, "no": False}.get(effect),
        "merge_commit": merged or None}
with open(path, "a") as f: f.write(json.dumps(line, sort_keys=True) + "\n")
PY
}

# 1. The head is the one named, and the pull request is ready.
[ "$(head_now)" = "$H" ] || stop 10 "head_moved@$R#$N"
[ "$(gh pr view "$N" -R "$R" --json isDraft --jq .isDraft)" = false ] || gh pr ready "$N" -R "$R" > /dev/null

# 2. The required check on that head.
while :; do
  s=$(gh pr checks "$N" -R "$R" --json name,state --jq ".[]|select(.name==\"$CHECK\")|.state" 2>/dev/null | head -n 1)
  case "$s" in
    SUCCESS) break ;;
    ""|PENDING|QUEUED|IN_PROGRESS|WAITING|REQUESTED) sleep "$POLL" ;;
    *) stop 11 "check_failed@$R#$N $CHECK=$s" ;;
  esac
done
[ "$(head_now)" = "$H" ] || stop 10 "head_moved@$R#$N"

# 3. The automatic reviewer on that head: no unresolved finding.
if [ "$AUTO" = 1 ]; then
  a=$(bash "$HERE/await-autoreview.sh" "$R" "$N" "$H" "${DELIVER_AUTOREVIEW_MINUTES:-40}" 2>&1) || stop 12 "autoreview_unanswered@$R#$N"
  printf '%s\n' "$a" | tail -n 1 | grep -q '"has_unresolved_findings":true' && stop 12 "autoreview_findings@$R#$N"
fi

# 4. ONE verifier in a fresh context (or the cut one, resumed).
bash "$HERE/review.sh" "$ROLE" "$CWD" "$PROMPT" "$OUT" $RESUME; rc=$?
if [ "$rc" -ne 0 ]; then
  VERDICT=cut; [ "$rc" -eq 75 ] || VERDICT=runner_failed
  usage_line; stop "$([ "$rc" -eq 75 ] && echo 75 || echo 13)" "verifier_$VERDICT@$OUT"
fi
l1=$(sed -n 1p "$OUT"); l2=$(sed -n 2p "$OUT")
case "$l1" in
  "Verdict: accept") VERDICT=accept ;;
  "Verdict: repair") VERDICT=repair ;;
  *) VERDICT=unreadable ;;
esac
# An accept counts only for the head it names.
[ "$VERDICT" != accept ] || printf '%s' "$l2" | grep -qF "$H" || VERDICT=accept_of_other_head
[ "$VERDICT" = accept ] || { usage_line; stop 13 "verdict_$VERDICT@$OUT"; }

# 5. The merge of exactly that head, by the rule the caller chose.
if [ "$MODE" = noeffect ]; then
  paths=$(gh pr diff "$N" -R "$R" --name-only) || paths="?"
  EFFECT=yes
  while IFS= read -r p; do
    [[ "$p" =~ (^|/)tests?/|_tests?\.rs$|^docs/|\.md$|(^|/)test[-_][^/]*$ ]] || EFFECT=no
  done <<< "$paths"
  [ -n "$paths" ] || EFFECT=no
  [ "$EFFECT" = yes ] || { usage_line; stop 0 "accepted_effect_needs_lead@$R#$N"; }
fi
[ "$MODE" != stop ] || { usage_line; stop 0 "accepted_not_merged@$R#$N"; }
gh pr merge "$N" -R "$R" --merge --match-head-commit "$H" > /dev/null 2>&1
m=$(gh pr view "$N" -R "$R" --json state,mergeCommit --jq '.state+" "+(.mergeCommit.oid // "")')
[ "${m%% *}" = MERGED ] && [ -n "${m##* }" ] || { usage_line; stop 14 "merge_not_read_back@$R#$N"; }
MERGED=${m##* }
gh pr comment "$N" -R "$R" --body "Delivery record: required check \`$CHECK\` passed on \`$H\`; automatic reviewer: $([ "$AUTO" = 1 ] && echo "completed without unresolved findings" || echo "none on this repository"); ONE verifier in a fresh context (role \`$ROLE\`): \`Verdict: accept\` for that head; merged as \`$MERGED\` at $(now)." > /dev/null 2>&1 || true
usage_line
stop 0 "merged@$R#$N $MERGED"
