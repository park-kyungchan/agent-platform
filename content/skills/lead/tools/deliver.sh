#!/usr/bin/env bash
# Delivers ONE pull request head: required check, automatic reviewer, ONE verifier, and the merge of
# exactly that head. It replaces the Lead's hand steps; every decision it takes is a rule written here.
#   deliver.sh <owner/repo> <pr> <head sha> <role> <cwd> <verifier prompt> <verifier out> [options]
# Options:
#   --merge               the Lead read the diff: merge when the verifier accepts
#   --merge-if-no-effect  merge on accept only when the change has no effect (decision lead-diff-reading):
#                         every changed file is MODIFIED (none added, removed or renamed) and is a file under
#                         a tests directory, a *_tests.rs file, docs/**/*.md or the root README.md, and none is
#                         under .claude/, content/, ci/ or scripts/, is named REVIEW.md, AGENTS.md or SKILL.md,
#                         or has a name that starts with test- or test_ (a script the check runs).
#                         Otherwise it stops with accepted_effect_needs_lead.
#   --resume              continue a cut verifier (review.sh --resume) instead of a fresh one
#   --check NAME          the required check (default rust)
#   --no-autoreview       the repository has no automatic reviewer
#   --unit PAGE           the unit's page, copied into the usage line
#   --usage FILE          where the usage line is appended (default <cwd>/usage.jsonl)
# Without --merge or --merge-if-no-effect it stops after an accept (accepted_not_merged).
# The last line of stdout is the status. Exit: 0 merged; 3 accepted and NOT merged; 2 usage, or the
# verifier's runner refused its arguments; 10 the head moved or the pull request is not open; 11 the check
# failed or did not answer in DELIVER_CHECK_MINUTES (90); 12 the automatic reviewer has unresolved findings
# or did not answer; 13 the verdict is not an accept of this head; 14 the merge of this head was not read
# back; 75 the verifier was cut (run again with --resume). After every run in which a verifier session ran,
# ONE line is appended to the usage file: time, repository, pull request, head, verdict, cost, merge.
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
    --check|--unit|--usage)
      [ "$#" -ge 2 ] || { echo "usage: $1 needs a value" >&2; exit 2; }
      case "$1" in --check) CHECK=$2 ;; --unit) UNIT=$2 ;; --usage) USAGE=$2 ;; esac; shift ;;
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
# "<state> <head> <draft> <merge commit or ->" of the pull request, read now.
view() { gh pr view "$N" -R "$R" --json state,headRefOid,isDraft,mergeCommit --jq '[.state,.headRefOid,(.isDraft|tostring),(.mergeCommit.oid // "-")]|join(" ")' 2>/dev/null; }
open_at_head() { local v; v=$(view); [ "$(printf '%s' "$v" | cut -d' ' -f1,2)" = "OPEN $H" ]; }
VERDICT=none; MERGED=""; EFFECT=unknown
usage_line() {
  python3 -I - "$USAGE" "$(now)" "$R" "$N" "$H" "$UNIT" "$ROLE" "$VERDICT" "$MERGED" "$MODE" "$EFFECT" "$OUT.run.json" <<'PY' || echo "usage_not_written@$USAGE" >&2
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

# 1. The pull request is open at the head named, and ready.
open_at_head || stop 10 "head_moved_or_not_open@$R#$N"
[ "$(view | cut -d' ' -f3)" = false ] || gh pr ready "$N" -R "$R" > /dev/null

# 2. The required check: EVERY entry of that name has passed, and there is at least one. A head that
#    moves while the check runs stops the delivery.
END=$(( $(date -u +%s) + ${DELIVER_CHECK_MINUTES:-90}*60 ))
while :; do
  open_at_head || stop 10 "head_moved_or_not_open@$R#$N"
  s=$(gh pr checks "$N" -R "$R" --json name,state 2>/dev/null | python3 -I -c 'import json, sys
try: states = [c.get("state") for c in json.load(sys.stdin) if c.get("name") == sys.argv[1]]
except Exception: states = []
waiting = {"PENDING", "QUEUED", "IN_PROGRESS", "WAITING", "REQUESTED"}
bad = [x for x in states if x != "SUCCESS" and x not in waiting]
print("failed " + str(bad[0]) if bad else "wait" if not states or any(x in waiting for x in states) else "passed")' "$CHECK")
  case "$s" in
    passed) break ;;
    failed*) stop 11 "check_failed@$R#$N $CHECK=${s#failed }" ;;
    *) [ "$(date -u +%s)" -lt "$END" ] || stop 11 "check_timeout@$R#$N $CHECK"
       sleep "$POLL" ;;
  esac
done

# 3. The automatic reviewer on that head. The flag is read as JSON: only an explicit false passes.
if [ "$AUTO" = 1 ]; then
  a=$(bash "$HERE/await-autoreview.sh" "$R" "$N" "$H" "${DELIVER_AUTOREVIEW_MINUTES:-40}" 2>&1) || stop 12 "autoreview_unanswered@$R#$N"
  printf '%s\n' "$a" | tail -n 1 | python3 -I -c 'import json, sys
line = sys.stdin.read()
try: ok = json.loads(line[line.index("{"):])["code_review"]["has_unresolved_findings"] is False
except Exception: ok = False
sys.exit(0 if ok else 1)' || stop 12 "autoreview_findings@$R#$N"
fi

# 4. ONE verifier in a fresh context (or the cut one, resumed). Its answer is read only after exit 0.
bash "$HERE/review.sh" "$ROLE" "$CWD" "$PROMPT" "$OUT" $RESUME; rc=$?
case "$rc" in
  0) ;;
  75) VERDICT=cut; usage_line; stop 75 "verifier_cut@$OUT" ;;
  2|4) stop 2 "verifier_runner_refused@$OUT rc=$rc" ;;   # no session ran: no usage line
  *) VERDICT=runner_failed; usage_line; stop 13 "verifier_runner_failed@$OUT rc=$rc" ;;
esac
l1=$(sed -n 1p "$OUT"); l2=$(sed -n 2p "$OUT" | tr -d '[:space:]')
case "$l1" in
  "Verdict: accept") VERDICT=accept ;;
  "Verdict: repair") VERDICT=repair ;;
  *) VERDICT=unreadable ;;
esac
# An accept counts only for the head it names: the second line IS the head.
[ "$VERDICT" != accept ] || [ "$l2" = "$H" ] || VERDICT=accept_of_other_head
[ "$VERDICT" = accept ] || { usage_line; stop 13 "verdict_$VERDICT@$OUT"; }

# 5. The merge of exactly that head, by the rule the caller chose.
if [ "$MODE" = noeffect ]; then
  files=$(gh api "repos/$R/pulls/$N/files" --paginate --jq '.[]|.status+"\t"+.filename' 2>/dev/null) || files=""
  EFFECT=$(printf '%s' "$files" | python3 -I -c 'import re, sys
rows = [l.split("\t", 1) for l in sys.stdin.read().split("\n") if l]
def quiet(status, path):
    base = path.rsplit("/", 1)[-1]
    if status != "modified" or re.match(r"(\.claude|content|ci|scripts)/", path): return False
    if base in ("REVIEW.md", "AGENTS.md", "SKILL.md") or re.match(r"test[-_]", base): return False
    return bool(re.search(r"(^|/)tests?/", path) or base.endswith(("_tests.rs", "_test.rs"))
                or re.match(r"docs/.*\.md$", path) or path == "README.md")
print("yes" if rows and all(len(r) == 2 and quiet(*r) for r in rows) else "no")')
  [ "$EFFECT" = yes ] || { usage_line; stop 3 "accepted_effect_needs_lead@$R#$N"; }
fi
[ "$MODE" != stop ] || { usage_line; stop 3 "accepted_not_merged@$R#$N"; }
gh pr merge "$N" -R "$R" --merge --match-head-commit "$H" > /dev/null 2>&1; mrc=$?
m=$(view)   # read back: merged, at this head, by this call
if [ "$mrc" -ne 0 ] || [ "$(printf '%s' "$m" | cut -d' ' -f1,2)" != "MERGED $H" ] || [ "$(printf '%s' "$m" | cut -d' ' -f4)" = "-" ]; then
  usage_line; stop 14 "merge_not_read_back@$R#$N"
fi
MERGED=$(printf '%s' "$m" | cut -d' ' -f4)
gh pr comment "$N" -R "$R" --body "Delivery record: required check \`$CHECK\` passed on \`$H\`; automatic reviewer: $([ "$AUTO" = 1 ] && echo "completed without unresolved findings" || echo "none on this repository"); ONE verifier in a fresh context (role \`$ROLE\`): \`Verdict: accept\` for that head; merged as \`$MERGED\` at $(now)." > /dev/null 2>&1 || true
usage_line
stop 0 "merged@$R#$N $MERGED"
