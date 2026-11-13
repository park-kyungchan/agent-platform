#!/usr/bin/env bash
# Test of the delivery chain with a fake `gh` and a fake `claude`: it merges only the head the verifier
# accepted, stops at each red step with its own exit code, and writes one usage line per verifier run.
# Run by ./check (content part).
set -uo pipefail
tools=$(cd "$(dirname "$0")/.." && pwd -P)/content/skills/lead/tools
t=$(mktemp -d); trap 'rm -rf -- "$t"' EXIT
mkdir -p "$t/bin" "$t/acc" "$t/cwd" "$t/gh"
H=1111111111111111111111111111111111111111; O=2222222222222222222222222222222222222222
cat > "$t/bin/gh" <<'FAKE'
#!/usr/bin/env bash
# Answers from the files of $FAKE_GH; every call is logged.
echo "$*" >> "$FAKE_GH/calls"; a="$*"; g=$FAKE_GH
case "$a" in
  "pr view"*"headRefOid,isDraft"*) echo "$(cat "$g/head") $(cat "$g/draft")" ;;
  "pr view"*"headRefOid"*) cat "$g/head" ;;
  "pr view"*"isDraft"*) cat "$g/draft" ;;
  "pr view"*"comments"*) echo "<!-- codex-review-state:v1 {\"code_review\":{\"commit_sha\":\"$(cat "$g/head")\",\"status\":\"completed\",\"has_unresolved_findings\":$(cat "$g/findings")}} -->" ;;
  "pr view"*"state,mergeCommit"*) [ -e "$g/merged" ] && echo "MERGED $(cat "$g/merged")" || echo "OPEN " ;;
  "pr checks"*) cat "$g/check" ;;
  "pr ready"*) echo false > "$g/draft" ;;
  "pr diff"*) cat "$g/paths" ;;
  "pr merge"*"--match-head-commit $(cat "$g/head")"*) [ -e "$g/refuse_merge" ] || echo abc123 > "$g/merged" ;;
  "pr merge"*) exit 1 ;;
  "pr comment"*) : ;;
  *) echo "fake gh: unexpected $a" >&2; exit 9 ;;
esac
FAKE
cat > "$t/bin/claude" <<'FAKE'
#!/usr/bin/env bash
cat > /dev/null
case "$(cat "$FAKE_GH/claude" 2>/dev/null)" in
  limit) printf '{"is_error":true,"result":"You have hit your 5-hour limit · resets 6:30pm (UTC)","num_turns":45,"session_id":"s1"}' ;;
  *) printf '{"is_error":false,"result":%s,"num_turns":3,"session_id":"s1","total_cost_usd":0.5}' "$(python3 -I -c 'import json,sys; print(json.dumps(open(sys.argv[1]).read()))' "$FAKE_GH/answer")" ;;
esac
FAKE
chmod +x "$t/bin/gh" "$t/bin/claude"
printf '# role\tmodel\teffort\tdirs\nverify\tsonnet\thigh\t%s\n' "$t/acc" > "$t/roles.tsv"
export PATH="$t/bin:$PATH" FAKE_GH="$t/gh" LEAD_ROLES_FILE="$t/roles.tsv" CLAUDE_BIN="$t/bin/claude" DELIVER_POLL=0 DELIVER_AUTOREVIEW_MINUTES=0 REVIEW_WATCH_SLEEP=1
echo "review this" > "$t/prompt.md"
fails=0; ok() { echo "ok   $1"; }; bad() { echo "FAIL $1"; fails=$((fails+1)); }
reset() { rm -f "$t/gh"/* "$t/cwd/usage.jsonl" "$t"/v.md*; echo "$H" > "$t/gh/head"; echo false > "$t/gh/draft"; echo SUCCESS > "$t/gh/check"; echo false > "$t/gh/findings"; printf 'crates/a/src/lib.rs\n' > "$t/gh/paths"; printf 'Verdict: accept\n%s\n' "$H" > "$t/gh/answer"; }
run() { bash "$tools/deliver.sh" o/r 7 "$H" verify "$t/cwd" "$t/prompt.md" "$t/v.md" "$@" > "$t/stdout" 2> "$t/stderr"; }
last() { tail -n 1 "$t/stdout"; }
merges() { grep -c '^pr merge' "$t/gh/calls"; }
lines() { [ -e "$t/cwd/usage.jsonl" ] && wc -l < "$t/cwd/usage.jsonl" || echo 0; }

reset; run --merge; rc=$?
{ [ "$rc" -eq 0 ] && last | grep -q '^merged@o/r#7 abc123$' && grep -q -- "--match-head-commit $H" "$t/gh/calls" && [ "$(lines)" -eq 1 ] && grep -q '"merge_commit": "abc123"' "$t/cwd/usage.jsonl" && grep -q '"cost_usd": 0.5' "$t/cwd/usage.jsonl"; } && ok "accept with --merge merges the named head and writes one usage line" || bad "merge: rc=$rc $(last)"
reset; run; rc=$?
{ [ "$rc" -eq 0 ] && last | grep -q '^accepted_not_merged@' && [ "$(merges)" -eq 0 ]; } && ok "without a merge option an accept is not merged" || bad "no option: rc=$rc $(last)"
reset; echo "$O" > "$t/gh/head"; run --merge; rc=$?
{ [ "$rc" -eq 10 ] && [ "$(merges)" -eq 0 ] && [ "$(lines)" -eq 0 ]; } && ok "a moved head stops before the check" || bad "moved head: rc=$rc $(last)"
reset; echo FAILURE > "$t/gh/check"; run --merge; rc=$?
{ [ "$rc" -eq 11 ] && [ "$(merges)" -eq 0 ]; } && ok "a failed check stops" || bad "check: rc=$rc $(last)"
reset; echo true > "$t/gh/findings"; run --merge; rc=$?
{ [ "$rc" -eq 12 ] && [ "$(merges)" -eq 0 ] && last | grep -q '^autoreview_findings@'; } && ok "an unresolved finding of the automatic reviewer stops" || bad "findings: rc=$rc $(last)"
reset; printf 'Verdict: repair\n%s\n' "$H" > "$t/gh/answer"; run --merge; rc=$?
{ [ "$rc" -eq 13 ] && [ "$(merges)" -eq 0 ] && grep -q '"verdict": "repair"' "$t/cwd/usage.jsonl"; } && ok "a repair verdict stops and is recorded" || bad "repair: rc=$rc $(last)"
reset; printf 'Verdict: accept\n%s\n' "$O" > "$t/gh/answer"; run --merge; rc=$?
{ [ "$rc" -eq 13 ] && [ "$(merges)" -eq 0 ] && last | grep -q 'accept_of_other_head'; } && ok "an accept that names another head is no accept" || bad "other head: rc=$rc $(last)"
reset; printf 'I think it is fine.\nVerdict: accept\n' > "$t/gh/answer"; run --merge; rc=$?
{ [ "$rc" -eq 13 ] && [ "$(merges)" -eq 0 ]; } && ok "a verdict that is not the first line is no accept" || bad "unreadable: rc=$rc $(last)"
reset; echo limit > "$t/gh/claude"; run --merge; rc=$?
{ [ "$rc" -eq 75 ] && [ "$(merges)" -eq 0 ] && grep -q '"verdict": "cut"' "$t/cwd/usage.jsonl"; } && ok "a cut verifier is exit 75, never a pass" || bad "cut: rc=$rc $(last)"
rm -f "$t/gh/claude"; run --merge --resume; rc=$?
{ [ "$rc" -eq 0 ] && last | grep -q '^merged@' && [ "$(lines)" -eq 2 ]; } && ok "the resumed verifier's accept merges" || bad "resume: rc=$rc $(last)"
reset; run --merge-if-no-effect; rc=$?
{ [ "$rc" -eq 0 ] && last | grep -q '^accepted_effect_needs_lead@' && [ "$(merges)" -eq 0 ] && grep -q '"no_effect": false' "$t/cwd/usage.jsonl"; } && ok "a source path is an effect: not merged without the Lead" || bad "effect: rc=$rc $(last)"
reset; printf 'crates/a/tests/x.rs\ndocs/a.txt\nREADME.md\nci/test-lead-x.sh\ncrates/a/src/x_tests.rs\n' > "$t/gh/paths"; run --merge-if-no-effect; rc=$?
{ [ "$rc" -eq 0 ] && last | grep -q '^merged@' && grep -q '"no_effect": true' "$t/cwd/usage.jsonl"; } && ok "tests and documentation only: merged" || bad "no effect: rc=$rc $(last)"
reset; : > "$t/gh/paths"; run --merge-if-no-effect; rc=$?
{ [ "$rc" -eq 0 ] && [ "$(merges)" -eq 0 ]; } && ok "an unreadable path list is an effect" || bad "empty paths: rc=$rc $(last)"
reset; : > "$t/gh/refuse_merge"; run --merge; rc=$?
{ [ "$rc" -eq 14 ] && last | grep -q '^merge_not_read_back@'; } && ok "a merge that is not read back is exit 14" || bad "readback: rc=$rc $(last)"
reset; echo true > "$t/gh/draft"; run; rc=$?
{ [ "$rc" -eq 0 ] && grep -q '^pr ready' "$t/gh/calls"; } && ok "a draft is marked ready" || bad "draft: rc=$rc $(last)"
[ "$fails" -eq 0 ] && echo "test-lead-deliver: PASS" || { echo "test-lead-deliver: $fails FAILED"; cat "$t/stdout" "$t/stderr"; exit 1; }
