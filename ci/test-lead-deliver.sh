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
# Answers from the files of $FAKE_GH; every call is logged. A file "<name>.next" replaces "<name>" after the
# call that read it, so that an answer can change between two calls.
echo "$*" >> "$FAKE_GH/calls"; a="$*"; g=$FAKE_GH
turn() { cat "$g/$1"; [ ! -e "$g/$1.next" ] || mv "$g/$1.next" "$g/$1"; }
case "$a" in
  "pr view"*"state,headRefOid"*) echo "$(cat "$g/state") $(cat "$g/head") $(cat "$g/draft") $(cat "$g/merged" 2>/dev/null || echo -)" ;;
  "pr view"*"headRefOid,isDraft"*) echo "$(cat "$g/head") $(cat "$g/draft")" ;;
  "pr view"*"comments"*) echo "<!-- codex-review-state:v1 {\"code_review\":{\"commit_sha\":\"$(cat "$g/head")\",\"status\":\"completed\",\"has_unresolved_findings\":$(cat "$g/findings")}} -->" ;;
  "pr checks"*) turn check; [ ! -e "$g/move_at_check" ] || cat "$g/move_at_check" > "$g/head" ;;
  "pr ready"*) echo false > "$g/draft" ;;
  "api repos/o/r/pulls/7/files"*) [ ! -e "$g/files_fail" ] || exit 1; cat "$g/files" ;;
  "pr merge"*"--match-head-commit $(cat "$g/head")"*) [ ! -e "$g/refuse_merge" ] || exit 1; echo MERGED > "$g/state"; echo abc123 > "$g/merged" ;;
  "pr merge"*) [ ! -e "$g/merged_by_other" ] || { echo MERGED > "$g/state"; echo deadbeef > "$g/merged"; }; exit 1 ;;
  "pr comment"*) : ;;
  *) echo "fake gh: unexpected $a" >&2; exit 9 ;;
esac
FAKE
cat > "$t/bin/claude" <<'FAKE'
#!/usr/bin/env bash
cat > /dev/null
[ ! -e "$FAKE_GH/move_at_verifier" ] || cat "$FAKE_GH/move_at_verifier" > "$FAKE_GH/head"
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
checks() { printf '['; sep=""; for s in "$@"; do printf '%s{"name":"%s","state":"%s"}' "$sep" "${s%%=*}" "${s##*=}"; sep=","; done; printf ']\n'; }
reset() {
  rm -f "$t/gh"/* "$t/cwd/usage.jsonl" "$t"/v.md*
  echo OPEN > "$t/gh/state"; echo "$H" > "$t/gh/head"; echo false > "$t/gh/draft"; checks rust=SUCCESS > "$t/gh/check"
  echo false > "$t/gh/findings"; printf 'modified\tcrates/a/src/lib.rs\n' > "$t/gh/files"; printf 'Verdict: accept\n%s\n' "$H" > "$t/gh/answer"
}
ROLE=verify
run() { timeout 60 bash "$tools/deliver.sh" o/r 7 "$H" "$ROLE" "$t/cwd" "$t/prompt.md" "$t/v.md" "$@" > "$t/stdout" 2> "$t/stderr"; }
last() { tail -n 1 "$t/stdout"; }
merges() { grep -c '^pr merge' "$t/gh/calls"; }
lines() { [ -e "$t/cwd/usage.jsonl" ] && wc -l < "$t/cwd/usage.jsonl" || echo 0; }
# expect <name> <exit code> <merge calls> <usage lines> <pattern of the last line> [options of deliver.sh]
expect() { local name=$1 code=$2 m=$3 u=$4 pat=$5; shift 5; run "$@"; rc=$?
  { [ "$rc" -eq "$code" ] && [ "$(merges)" -eq "$m" ] && [ "$(lines)" -eq "$u" ] && last | grep -Eq "$pat"; } && ok "$name" || bad "$name: rc=$rc merges=$(merges) usage=$(lines) last=$(last)"; }

reset; expect "accept with --merge merges the named head and writes one usage line" 0 1 1 '^merged@o/r#7 abc123$' --merge
{ grep -q -- "--match-head-commit $H" "$t/gh/calls" && grep -q '"merge_commit": "abc123"' "$t/cwd/usage.jsonl" && grep -q '"cost_usd": 0.5' "$t/cwd/usage.jsonl"; } && ok "the merge names the head; the usage line has the merge commit and the cost" || bad "merge details"
reset; expect "without a merge option an accept is exit 3 and nothing is merged" 3 0 1 '^accepted_not_merged@'
reset; echo "$O" > "$t/gh/head"; expect "another head stops before the check" 10 0 0 '^head_moved_or_not_open@' --merge
reset; echo MERGED > "$t/gh/state"; expect "a pull request that is not open stops before any verifier" 10 0 0 '^head_moved_or_not_open@' --merge
reset; echo true > "$t/gh/draft"; expect "a draft is marked ready" 3 0 1 '^accepted_not_merged@'; grep -q '^pr ready' "$t/gh/calls" || bad "draft: no ready call"
# the required check
reset; checks rust=FAILURE > "$t/gh/check"; expect "a failed check stops" 11 0 0 '^check_failed@.* rust=FAILURE$' --merge
reset; checks rust=SUCCESS rust=FAILURE > "$t/gh/check"; expect "one failed entry among two of the name stops" 11 0 0 '^check_failed@' --merge
reset; checks rust=FAILURE rust=SUCCESS > "$t/gh/check"; expect "the same in the other order" 11 0 0 '^check_failed@' --merge
reset; checks rust=SKIPPED > "$t/gh/check"; expect "a skipped check is not a pass" 11 0 0 '^check_failed@' --merge
reset; checks rust-extra=SUCCESS > "$t/gh/check"; DELIVER_CHECK_MINUTES=0 expect "another check's success is not the required one" 11 0 0 '^check_timeout@' --merge
reset; checks rust=PENDING > "$t/gh/check"; DELIVER_CHECK_MINUTES=0 expect "a check that never answers is a timeout" 11 0 0 '^check_timeout@' --merge
reset; checks rust=PENDING > "$t/gh/check"; checks rust=SUCCESS > "$t/gh/check.next"; expect "a pending check is waited for" 0 1 1 '^merged@' --merge
reset; checks rust=PENDING > "$t/gh/check"; echo "$O" > "$t/gh/move_at_check"; expect "a head that moves while the check is pending stops at once" 10 0 0 '^head_moved_or_not_open@' --merge
[ "$(grep -c '^pr checks' "$t/gh/calls")" -eq 1 ] || bad "moved while pending: polled again"
# the automatic reviewer
reset; echo true > "$t/gh/findings"; expect "an unresolved finding stops" 12 0 0 '^autoreview_findings@' --merge
reset; echo ' true' > "$t/gh/findings"; expect "the flag is read as JSON (a space after the colon)" 12 0 0 '^autoreview_findings@' --merge
reset; echo null > "$t/gh/findings"; expect "a flag that is not an explicit false stops" 12 0 0 '^autoreview_findings@' --merge
# the verdict
reset; printf 'Verdict: repair\n%s\n' "$H" > "$t/gh/answer"; expect "a repair verdict stops and is recorded" 13 0 1 '^verdict_repair@' --merge; grep -q '"verdict": "repair"' "$t/cwd/usage.jsonl" || bad "repair: usage"
reset; printf 'Verdict: accept\n%s\n' "$O" > "$t/gh/answer"; expect "an accept that names another head is no accept" 13 0 1 'accept_of_other_head' --merge
reset; printf 'Verdict: accept\nreviewed %s not %s\n' "$O" "$H" > "$t/gh/answer"; expect "the head inside a longer second line is no accept" 13 0 1 'accept_of_other_head' --merge
reset; printf 'Verdict: accept\n%sab\n' "$H" > "$t/gh/answer"; expect "the head as a prefix of the second line is no accept" 13 0 1 'accept_of_other_head' --merge
reset; printf 'Verdict: accept\n%s\n' "${H:0:8}" > "$t/gh/answer"; expect "a short form of the head is no accept" 13 0 1 'accept_of_other_head' --merge
reset; printf 'Verdict: accepted with notes\n%s\n' "$H" > "$t/gh/answer"; expect "a first line that only starts like an accept is unreadable" 13 0 1 'verdict_unreadable' --merge
reset; printf 'I think it is fine.\nVerdict: accept\n' > "$t/gh/answer"; expect "a verdict that is not the first line is no accept" 13 0 1 'verdict_unreadable' --merge
reset; echo limit > "$t/gh/claude"; expect "a cut verifier is exit 75, never a pass" 75 0 1 '^verifier_cut@' --merge; grep -q '"verdict": "cut"' "$t/cwd/usage.jsonl" || bad "cut: usage"
rm -f "$t/gh/claude"; expect "the resumed verifier's accept merges" 0 1 2 '^merged@' --merge --resume
reset; printf 'Verdict: accept\n%s\n' "$H" > "$t/v.md"; echo '{"turns":99,"cost_usd":9.9}' > "$t/v.md.run.json"
ROLE=nobody expect "a runner that refuses its arguments: no merge on a left-over accept, no usage line" 2 0 0 '^verifier_runner_refused@' --merge
# the merge
reset; echo "$O" > "$t/gh/move_at_verifier"; expect "a head that moves during the verifier is not merged" 14 1 1 '^merge_not_read_back@' --merge
reset; echo "$O" > "$t/gh/move_at_verifier"; : > "$t/gh/merged_by_other"; expect "a merge of another head by someone else is not reported as this delivery" 14 1 1 '^merge_not_read_back@' --merge
grep -q '"merge_commit": null' "$t/cwd/usage.jsonl" || bad "foreign merge: usage names a merge commit"
reset; : > "$t/gh/refuse_merge"; expect "a refused merge is exit 14" 14 1 1 '^merge_not_read_back@' --merge
# the rule of --merge-if-no-effect
reset; expect "a source path is an effect: not merged without the Lead" 3 0 1 '^accepted_effect_needs_lead@' --merge-if-no-effect; grep -q '"no_effect": false' "$t/cwd/usage.jsonl" || bad "effect: usage"
reset; printf 'modified\tcrates/a/tests/x.rs\nmodified\tdocs/a.md\nmodified\tREADME.md\nmodified\tcrates/a/src/x_tests.rs\n' > "$t/gh/files"; expect "modified tests and documentation only: merged" 0 1 1 '^merged@' --merge-if-no-effect; grep -q '"no_effect": true' "$t/cwd/usage.jsonl" || bad "no effect: usage"
while IFS= read -r f; do
  reset; printf 'modified\tdocs/a.md\n%s\n' "$f" > "$t/gh/files"; expect "an effect: $(printf '%s' "$f" | tr '\t' ' ')" 3 0 1 '^accepted_effect_needs_lead@' --merge-if-no-effect
done <<EFFECTS
added	crates/a/tests/new.rs
removed	crates/a/tests/x.rs
renamed	docs/b.md
modified	REVIEW.md
modified	AGENTS.md
modified	modules/x/AGENTS.md
modified	.claude/agents/verifier.md
modified	content/skills/lead/SKILL.md
modified	content/skills/a/references/b.md
modified	ci/test-lead-x.sh
modified	scripts/test-install-x.sh
modified	crates/a/tests/test_support.rs
modified	docs/a.txt
modified	notes.md
EFFECTS
reset; : > "$t/gh/files"; expect "an empty file list is an effect" 3 0 1 '^accepted_effect_needs_lead@' --merge-if-no-effect
reset; : > "$t/gh/files_fail"; expect "an unreadable file list is an effect" 3 0 1 '^accepted_effect_needs_lead@' --merge-if-no-effect
# options
reset; run --check; [ "$?" -eq 2 ] && ok "an option without its value is a usage error" || bad "option without value"
[ "$fails" -eq 0 ] && echo "test-lead-deliver: PASS" || { echo "test-lead-deliver: $fails FAILED"; cat "$t/stdout" "$t/stderr"; exit 1; }
