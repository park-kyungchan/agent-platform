#!/usr/bin/env bash
# Test of the reviewer runner with a fake `claude`: the role table decides the account, a cut session
# leaves no answer and a resume record, a resume uses the recorded account. Run by ./check (content part).
set -uo pipefail
runner=${1:-$(cd "$(dirname "$0")/.." && pwd -P)/content/skills/lead/tools/review.sh}
t=$(mktemp -d); trap 'rm -rf -- "$t"' EXIT
mkdir -p "$t/bin" "$t/a1" "$t/a2" "$t/lead" "$t/cwd"
cat > "$t/bin/claude" <<'FAKE'
#!/usr/bin/env bash
echo "$CLAUDE_CONFIG_DIR|$*" >> "$FAKE_LOG"; mark=$(grep -o '(review run [0-9-]*)' | tail -n 1)
id="s-$(basename "$CLAUDE_CONFIG_DIR")"
case "$(cat "$CLAUDE_CONFIG_DIR/mode" 2>/dev/null)" in
  noout)   exit 1 ;;
  other)   /bin/bash -c 'sleep 600; :' & wait $! ;;
  hang)    mkdir -p "$CLAUDE_CONFIG_DIR/projects/p"; printf '{"message":{"role":"user","content":"%s"}}\n{"message":{"role":"assistant","content":[{"type":"text","text":"Verdict: accept"}]}}\n' "$mark" > "$CLAUDE_CONFIG_DIR/projects/p/$id.jsonl"
           /bin/bash -c 'sleep 600; :' & wait $!   # two commands: the shell stays a shell and does not become the sleep
           printf '{"is_error":false,"result":"Verdict: accept\\nhead","num_turns":3,"session_id":"%s"}' "$id" ;;
  rhang)   f="$CLAUDE_CONFIG_DIR/projects/p/$id.jsonl"; printf '{"message":{"role":"user","content":"%s"}}\n' "$mark" >> "$f"
           /bin/bash -c 'sleep 600; :' & wait $! ;;
  rhang2)  f="$CLAUDE_CONFIG_DIR/projects/p/$id.jsonl"; printf '{"message":{"role":"user","content":"%s"}}\n{"message":{"role":"assistant","content":[{"type":"text","text":"Verdict: accept"}]}}\n' "$mark" >> "$f"
           /bin/bash -c 'sleep 600; :' & wait $!
           printf '{"is_error":false,"result":"Verdict: accept\\nhead","num_turns":4,"session_id":"%s"}' "$id" ;;
  rhang3)  f="$CLAUDE_CONFIG_DIR/projects/p/$id.jsonl"; { printf '{"message":{"role":"user","content":"%s"}}\n{"message":{"role":"assistant","content":[{"type":"text","text":"Verdict: accept"}]}}\n' "$mark"
             yes '{"message":{"role":"user","content":"filler filler filler filler filler filler filler filler"}}' | head -n 20000; } >> "$f"
           /bin/bash -c 'sleep 600; :' & wait $!
           printf '{"is_error":false,"result":"Verdict: accept\\nhead","num_turns":4,"session_id":"%s"}' "$id" ;;
  resets)  printf '{"is_error":false,"result":"Verdict: repair\\nhead\\nthe retry counter resets on every call","num_turns":3,"session_id":"%s"}' "$id" ;;
  errverdict) printf '{"is_error":true,"result":"Verdict: accept\\nhead\\nresets 6:30pm","num_turns":3,"session_id":"%s"}' "$id" ;;
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
# an earlier answer under the same name does not survive a cut run: it is moved aside.
echo limit45 > "$t/a1/mode"; : > "$t/calls.log"; run verify "$t/cwd" "$t/prompt.md" "$t/v.md"; rc=$?; rm -f "$t/a1/mode"
{ [ "$rc" -eq 75 ] && [ ! -e "$t/v.md" ] && grep -q '^Verdict: accept$' "$t/v.md.prev"; } && ok "cut run leaves no earlier answer in place" || bad "stale answer: rc=$rc"
# a prompt and an out file given relative to the caller's directory work although the runner changes directory.
( cd "$t" && : > calls.log && bash "$runner" integration "$t/cwd" prompt.md rel.md > stdout 2> stderr ); rc=$?
{ [ "$rc" -eq 0 ] && [ -s "$t/rel.md" ] && [ "$(wc -l < "$t/calls.log")" -eq 1 ]; } && ok "relative prompt and out paths" || bad "relative paths: rc=$rc $(cat "$t/stderr")"
# a resume that returns no envelope keeps the recorded session, so the next resume still works.
echo limit45 > "$t/a1/mode"; run verify "$t/cwd" "$t/prompt.md" "$t/k.md"; echo noout > "$t/a1/mode"; run verify "$t/cwd" "$t/prompt.md" "$t/k.md" --resume; rc=$?; rm -f "$t/a1/mode"
{ [ "$rc" -eq 75 ] && grep -q '"session": "s-a1"' "$t/k.md.run.json"; } && ok "failed resume keeps the session" || bad "failed resume: rc=$rc $(cat "$t/k.md.run.json")"
# a reviewer that waits for a shell it started: the watchdog ends that shell (below the reviewer) and the answer arrives.
echo hang > "$t/lead/mode"; s0=$(date +%s); REVIEW_IDLE_S=0 REVIEW_TIMEOUT=60 run integration "$t/cwd" "$t/prompt.md" "$t/h.md"; rc=$?; rm -f "$t/lead/mode"
{ [ "$rc" -eq 0 ] && [ -s "$t/h.md" ] && [ $(( $(date +%s) - s0 )) -lt 40 ]; } && ok "watchdog ends a leftover shell below the reviewer" || bad "watchdog: rc=$rc after $(( $(date +%s) - s0 ))s"
# another session's record with an idle verdict in the same account does not count: the reviewer's own shell lives on
# and the run ends by its own timeout (exit 75), not by the watchdog taking the other record for this run's.
mkdir -p "$t/lead/projects/q"; echo other > "$t/lead/mode"
( sleep 1; printf '{"message":{"role":"assistant","content":[{"type":"text","text":"Verdict: accept"}]}}\n' > "$t/lead/projects/q/other.jsonl" ) &
s0=$(date +%s); REVIEW_IDLE_S=0 REVIEW_TIMEOUT=6 run integration "$t/cwd" "$t/prompt.md" "$t/o.md"; rc=$?; rm -f "$t/lead/mode"; wait
{ [ "$rc" -eq 75 ] && [ $(( $(date +%s) - s0 )) -ge 5 ]; } && ok "another session's verdict does not end this run's shells" || bad "foreign record: rc=$rc after $(( $(date +%s) - s0 ))s"
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
# (I7) the runner starts claude in auto permission mode, fresh and resumed.
: > "$t/calls.log"; run integration "$t/cwd" "$t/prompt.md" "$t/pm.md"; rc=$?
{ [ "$rc" -eq 0 ] && [ "$(grep -c -- '--permission-mode auto' "$t/calls.log")" -eq 1 ] && [ "$(wc -l < "$t/calls.log")" -eq 1 ]; } && ok "a fresh run uses --permission-mode auto" || bad "permission mode (fresh): $(cat "$t/calls.log")"
: > "$t/calls.log"; run integration "$t/cwd" "$t/prompt.md" "$t/pm.md" --resume; rc=$?
{ [ "$rc" -eq 0 ] && grep -q -- '--permission-mode auto.*--resume s-lead' "$t/calls.log"; } && ok "a resumed run uses --permission-mode auto" || bad "permission mode (resume): $(cat "$t/calls.log")"
# (#155 fourth pass b, I9) a short answer that begins with Verdict: and holds the word "resets " is an answer, never a cut.
echo resets > "$t/lead/mode"; run integration "$t/cwd" "$t/prompt.md" "$t/rs.md"; rc=$?; rm -f "$t/lead/mode"
{ [ "$rc" -eq 0 ] && head -n 1 "$t/rs.md" | grep -q '^Verdict: repair$' && grep -q '"cut": false' "$t/rs.md.run.json"; } && ok "a verdict that holds the word resets is not cut" || bad "resets: rc=$rc $(cat "$t/rs.md.run.json" 2>/dev/null)"
echo errverdict > "$t/lead/mode"; run integration "$t/cwd" "$t/prompt.md" "$t/ev.md"; rc=$?; rm -f "$t/lead/mode"
{ [ "$rc" -eq 75 ] && [ ! -e "$t/ev.md" ] && grep -q '"cut": true' "$t/ev.md.run.json"; } && ok "an error answer is cut even if it begins with Verdict:" || bad "error verdict: rc=$rc"
# (#155 fourth pass a, I8) a resumed session that already holds a verdict: the OLD verdict does not end this run's shells.
echo limit45 > "$t/lead/mode"; run integration "$t/cwd" "$t/prompt.md" "$t/rh.md"; rm -f "$t/lead/mode"   # a cut run: the record to resume
mkdir -p "$t/lead/projects/p"; printf '{"message":{"role":"user","content":"earlier prompt (review run 1-1-1)"}}\n{"message":{"role":"assistant","content":[{"type":"text","text":"Verdict: repair"}]}}\n' > "$t/lead/projects/p/s-lead.jsonl"
echo rhang > "$t/lead/mode"; s0=$(date +%s); REVIEW_IDLE_S=0 REVIEW_TIMEOUT=6 run integration "$t/cwd" "$t/prompt.md" "$t/rh.md" --resume; rc=$?; rm -f "$t/lead/mode"
{ [ "$rc" -eq 75 ] && [ $(( $(date +%s) - s0 )) -ge 5 ]; } && ok "an earlier run's verdict does not end this run's shells" || bad "old verdict: rc=$rc after $(( $(date +%s) - s0 ))s"
printf '{"message":{"role":"user","content":"earlier prompt (review run 1-1-1)"}}\n{"message":{"role":"assistant","content":[{"type":"text","text":"Verdict: repair"}]}}\n' > "$t/lead/projects/p/s-lead.jsonl"
echo rhang2 > "$t/lead/mode"; s0=$(date +%s); REVIEW_IDLE_S=0 REVIEW_TIMEOUT=60 run integration "$t/cwd" "$t/prompt.md" "$t/rh.md" --resume; rc=$?; rm -f "$t/lead/mode"
{ [ "$rc" -eq 0 ] && [ -s "$t/rh.md" ] && [ $(( $(date +%s) - s0 )) -lt 40 ]; } && ok "a verdict written after this run's mark ends the leftover shell" || bad "new verdict: rc=$rc after $(( $(date +%s) - s0 ))s"
# a verdict followed by about 1 MB of lines still ends the leftover shell (no pipe that ends in grep -q under pipefail).
rm -f "$t/lead/projects/p/s-lead.jsonl"; echo rhang3 > "$t/lead/mode"; s0=$(date +%s); REVIEW_IDLE_S=0 REVIEW_TIMEOUT=60 run integration "$t/cwd" "$t/prompt.md" "$t/big.md"; rc=$?; rm -f "$t/lead/mode"
{ [ "$rc" -eq 0 ] && [ -s "$t/big.md" ] && [ $(( $(date +%s) - s0 )) -lt 40 ] && [ "$(wc -c < "$t/lead/projects/p/s-lead.jsonl")" -gt 1000000 ]; } && ok "a verdict followed by 1 MB of lines ends the leftover shell" || bad "big record: rc=$rc after $(( $(date +%s) - s0 ))s"
# the out file is refused as a whole name only: its companion files may be named in the prompt (I4).
echo "read /x/n.md.earlier-findings.md and say it in a sentence ending n.md.earlier-findings.md." > "$t/p3.md"; run integration "$t/cwd" "$t/p3.md" "$t/n.md"; rc=$?
[ "$rc" -eq 0 ] && ok "a companion file name in the prompt is allowed" || bad "companion name: rc=$rc"
for w in "write n.md" "write n.md." "(n.md)" "n.md, then"; do echo "$w" > "$t/p4.md"; run integration "$t/cwd" "$t/p4.md" "$t/n.md"; rc=$?; [ "$rc" -eq 4 ] && ok "the out file named as '$w' is refused" || bad "out named as '$w': rc=$rc"; done
[ "$fails" -eq 0 ] && echo "test-lead-review: PASS" || { echo "test-lead-review: $fails FAILED"; cat "$t/stdout" "$t/stderr"; exit 1; }
