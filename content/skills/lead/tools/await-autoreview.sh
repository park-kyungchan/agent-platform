#!/usr/bin/env bash
# await-autoreview.sh <owner/repo> <pr number> <head sha> [max minutes, default 40]
# Waits until the automatic reviewer's summary comment names <head> as completed.
# A push to a ready pull request is not reviewed: when the pull request is ready and the
# summary names another commit, this script marks it draft and ready again, ONCE, then waits.
# A draft pull request is left alone (marking ready is the Lead's decision): exit 4.
set -euo pipefail
R="$1"; N="$2"; H="$3"; MAX="${4:-40}"
state() { gh pr view "$N" -R "$R" --json comments --jq '.comments[].body' 2>/dev/null | grep -o 'codex-review-state:v1 {.*}' | tail -n 1 | sed 's/^codex-review-state:v1 //; s/ -->.*$//' || true; }
field() { python3 -I -c 'import sys,json
try: d=json.loads(sys.argv[1]).get("code_review",{})
except Exception: d={}
print(d.get(sys.argv[2],""))' "$1" "$2"; }
V="$(gh pr view "$N" -R "$R" --json headRefOid,isDraft --jq '.headRefOid+" "+(.isDraft|tostring)')"
[ "${V%% *}" = "$H" ] || { echo "head_differs@$R#$N have=${V%% *}"; exit 4; }
[ "${V##* }" = "false" ] || { echo "pr_is_draft@$R#$N"; exit 4; }
S="$(state)"
if [ "$(field "$S" commit_sha)" != "$H" ]; then
  gh pr ready "$N" -R "$R" --undo >/dev/null; gh pr ready "$N" -R "$R" >/dev/null
  echo "retriggered=$(date -u +%FT%TZ)"
fi
END=$(( $(date -u +%s) + MAX*60 ))
while :; do
  S="$(state)"
  if [ "$(field "$S" commit_sha)" = "$H" ] && [ "$(field "$S" status)" = "completed" ]; then
    echo "reviewed=$(date -u +%FT%TZ) $S"; exit 0
  fi
  [ "$(date -u +%s)" -lt "$END" ] || { echo "autoreview_timeout@$R#$N"; exit 5; }
  sleep 45
done
