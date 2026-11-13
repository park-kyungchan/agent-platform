#!/usr/bin/env bash
# Test of the reviewer runner with a fake `claude`: the role table decides the account, a cut session
# leaves no answer and a resume record, a resume uses the recorded account. Run by ./check (content part).
set -uo pipefail
runner=${1:-$(cd "$(dirname "$0")/.." && pwd -P)/content/skills/lead/tools/review.sh}
t=$(mktemp -d); trap 'rm -rf -- "$t"' EXIT
mkdir -p "$t/bin" "$t/a1" "$t/a2" "$t/lead" "$t/cwd"
cat > "$t/bin/claude" <<'FAKE'
#!/usr/bin/env bash
echo "$CLAUDE_CONFIG_DIR|$*" >> "$FAKE_LOG"; cat > /dev/null
id="s-$(basename "$CLAUDE_CONFIG_DIR")"
case "$(cat "$CLAUDE_CONFIG_DIR/mode" 2>/dev/null)" in
  limit1)  printf '{"is_error":true,"result":"You have hit your 5-hour limit · resets 6:30pm (UTC)","num_turns":1,"session_id":"%s"}' "$id" ;;
  limit45) printf '{"is_error":true,"result":"You have hit your 5-hour limit · resets 6:30pm (UTC)","num_turns":45,"session_id":"%s"}' "$id" ;;
  *)       printf '{"is_error":false,"result":"Verdict: accept\\nhead","num_turns":3,"session_id":"%s","total_cost_usd":0.1}' "$id" ;;
esac
FAKE
chmod +x "$t/bin/claude"
printf '# role\tmodel\teffort\tdirs\nverify\tsonnet\thigh\t%s,%s\nintegration\tfable\thigh\t%s\n' "$t/a1" "$t/a2" "$t/lead" > "$t/roles.tsv"
export LEAD_ROLES_FILE="$t/roles.tsv" CLAUDE_BIN="$t/bin/claude" FAKE_LOG="$t/calls.log" REVIEW_WATCH_SLEEP=1
echo "review this" > "$t/prompt.md"
fails=0; ok() { echo "ok   $1"; }; bad() { echo "FAIL $1"; fails=$((fails+1)); }
run() { bash "$runner" "$@" > "$t/stdout" 2> "$t/stderr"; }
# (a) the role integration runs on the Lead's directory with its model.
: > "$t/calls.log"; run integration "$t/cwd" "$t/prompt.md" "$t/i.md"; rc=$?
{ [ "$rc" -eq 0 ] && grep -q "^$t/lead|.*--model fable" "$t/calls.log" && [ "$(wc -l < "$t/calls.log")" -eq 1 ] && head -1 "$t/i.md" | grep -q '^Verdict: accept$'; } && ok "integration on the Lead's account" || bad "integration: rc=$rc"
# (b) a session cut after work: exit 75, no answer file, a resume record, the next account is NOT tried.
echo limit45 > "$t/a1/mode"; : > "$t/calls.log"; run verify "$t/cwd" "$t/prompt.md" "$t/v.md"; rc=$?
{ [ "$rc" -eq 75 ] && [ ! -e "$t/v.md" ] && [ "$(wc -l < "$t/calls.log")" -eq 1 ] && grep -q '"session": "s-a1"' "$t/v.md.run.json" && grep -q "\"account\": \"$t/a1\"" "$t/v.md.run.json" && grep -q '"cut": true' "$t/v.md.run.json"; } && ok "cut after work: no answer, resume record" || bad "cut after work: rc=$rc"
# (c) the resume uses the recorded account and session.
rm -f "$t/a1/mode"; : > "$t/calls.log"; run verify "$t/cwd" "$t/prompt.md" "$t/v.md" --resume; rc=$?
{ [ "$rc" -eq 0 ] && grep -q "^$t/a1|.*--resume s-a1" "$t/calls.log" && [ -s "$t/v.md" ]; } && ok "resume on the recorded account" || bad "resume: rc=$rc"
# (b2) cut before any work: the next account of the row takes the prompt.
echo limit1 > "$t/a1/mode"; : > "$t/calls.log"; run verify "$t/cwd" "$t/prompt.md" "$t/w.md"; rc=$?
{ [ "$rc" -eq 0 ] && [ "$(wc -l < "$t/calls.log")" -eq 2 ] && grep -q "\"account\": \"$t/a2\"" "$t/w.md.run.json"; } && ok "cut before work: next account" || bad "next account: rc=$rc"
# both accounts cut before work: exit 75 and no answer.
echo limit1 > "$t/a2/mode"; run verify "$t/cwd" "$t/prompt.md" "$t/x.md"; rc=$?
{ [ "$rc" -eq 75 ] && [ ! -e "$t/x.md" ]; } && ok "all accounts cut" || bad "all cut: rc=$rc"
# (d) a role that is not in the table: exit 2, nothing started.
: > "$t/calls.log"; run nobody "$t/cwd" "$t/prompt.md" "$t/n.md"; rc=$?
{ [ "$rc" -eq 2 ] && [ ! -s "$t/calls.log" ]; } && ok "unknown role" || bad "unknown role: rc=$rc"
# the prompt names the out file: exit 4, nothing started.
echo "write n.md" > "$t/p2.md"; run integration "$t/cwd" "$t/p2.md" "$t/n.md"; rc=$?
{ [ "$rc" -eq 4 ] && [ ! -s "$t/calls.log" ]; } && ok "out file named in the prompt" || bad "out named: rc=$rc"
[ "$fails" -eq 0 ] && echo "test-lead-review: PASS" || { echo "test-lead-review: $fails FAILED"; cat "$t/stdout" "$t/stderr"; exit 1; }
