#!/usr/bin/env bash
# Test of content/skills/lead/hooks/pre-commit: one pass and the refusals it documents,
# in a temporary repository. Run by ./check (content part).
set -uo pipefail
hook=$(cd "$(dirname "$0")/.." && pwd -P)/content/skills/lead/hooks/pre-commit
t=$(mktemp -d); trap 'rm -rf -- "$t"' EXIT
cd "$t" && git init -q . && git config user.email t@example.invalid && git config user.name t
printf '#!/bin/sh\nexec bash %s\n' "$hook" > .git/hooks/pre-commit && chmod +x .git/hooks/pre-commit
fails=0
expect() { # expect <refuse|pass> <code or -> <label>
  out=$(git commit -q -m "$3" 2>&1); rc=$?
  if [ "$1" = pass ] && [ "$rc" -ne 0 ]; then echo "FAIL $3: refused: $out"; fails=$((fails+1)); return; fi
  if [ "$1" = refuse ] && { [ "$rc" -eq 0 ] || ! printf '%s' "$out" | grep -q "^$2@"; }; then echo "FAIL $3: rc=$rc out=$out"; fails=$((fails+1)); return; fi
  echo "ok   $3"
}
state() { printf '# STATE\n<!-- measure -->\n%s\n<!-- /measure -->\ntext\n' "$1" > STATE.md; git add STATE.md; }
# 1. no measure script: a small STATE passes.
state "K 1"; expect pass - "small state without measure"
# 2. a STATE over the bound is refused.
head -c 12001 /dev/zero | tr '\0' x >> STATE.md; git add STATE.md; expect refuse state_too_large "oversized state"
state "K 1"
# 3. with ./measure: an equal block passes, a different one is refused, a failing measure is refused.
printf '#!/bin/sh\necho "K 2"\n' > measure; chmod +x measure; git add measure
expect refuse measure_block_stale "stale block"
state "K 2"; expect pass - "equal block"
printf '#!/bin/sh\nexit 3\n' > measure; git add measure; state "K 2"; echo x >> STATE.md; git add STATE.md
expect refuse measure_failed "failing measure"
printf '#!/bin/sh\necho "K 2"\n' > measure; git add measure
printf '# STATE\ntext\n' > STATE.md; git add STATE.md; expect refuse measure_block_missing "no block"
# a measure that ignores TERM is killed at the bound and the commit is refused, in seconds.
printf '#!/bin/bash\ntrap "" TERM\nsleep 6\necho "K 2"\n' > measure; git add measure
t0=$(date +%s); out=$(MEASURE_TIMEOUT_S=1 git commit -q -m "stuck measure" 2>&1); rc=$?; dt=$(( $(date +%s) - t0 ))
if [ "$rc" -ne 0 ] && printf '%s' "$out" | grep -q '^measure_failed@' && [ "$dt" -le 5 ]; then echo "ok   stuck measure killed after ${dt}s"; else echo "FAIL stuck measure: rc=$rc dt=$dt out=$out"; fails=$((fails+1)); fi
printf '#!/bin/sh\necho "K 2"\n' > measure; git add measure
# 4. the committed bytes decide, not the working file: a stale staged STATE is refused although the file on disk is current.
state "K 9"; printf '# STATE\n<!-- measure -->\nK 2\n<!-- /measure -->\ntext\n' > STATE.md
expect refuse measure_block_stale "staged bytes decide"
# 5. a large page is reported and does not refuse.
git add STATE.md; head -c 6500 /dev/zero | tr '\0' y > TASK-BIG.md; git add TASK-BIG.md
out=$(git commit -q -m "big page" 2>&1); rc=$?
if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q '^note: TASK-BIG.md is 6500 bytes'; then echo "ok   large page noted"; else echo "FAIL large page: rc=$rc out=$out"; fails=$((fails+1)); fi
[ "$fails" -eq 0 ] && echo "test-lead-hook: PASS" || { echo "test-lead-hook: $fails FAILED"; exit 1; }
