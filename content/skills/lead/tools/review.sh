#!/usr/bin/env bash
# Runs ONE headless turn of Claude Code as a reviewer and saves its answer. Claude Code only.
#   review.sh <role> <cwd> <prompt file> <out file>            a fresh session
#   review.sh <role> <cwd> <prompt file> <out file> --resume   continue the session recorded in <out>.run.json
# The role decides model, effort and the account (a Claude Code configuration directory): one line of the
# table file ${LEAD_ROLES_FILE:-/root/.config/lead/roles.tsv}, tab separated:
#   role <TAB> model <TAB> effort <TAB> configuration directory[,next directory ...]
# The table is host configuration and is not in this repository (form: roles.example.tsv). REVIEW_MODEL
# overrides the model of the row for one run.
# Exit: 0 an answer was saved to <out file>; 2 usage or a role that is not in the table; 4 the prompt names the
# out file (the reader would overwrite it); 75 the session was cut by a usage limit or an error: NO out file is
# written (an answer of an earlier run under that name is moved to <out>.prev before every run), and <out>.run.json names the session and the account to resume on; another code is the CLI's own.
# <out>.run.json is written after every run: {role, account, session, turns, cut, cost_usd}.
set -uo pipefail
[ "$#" -ge 4 ] || { echo "usage: review.sh <role> <cwd> <prompt file> <out file> [--resume]" >&2; exit 2; }
ROLE=$1; CWD=$2; PROMPT=$3; OUT=$4; RESUME=${5:-}
TABLE=${LEAD_ROLES_FILE:-/root/.config/lead/roles.tsv}
CLAUDE=${CLAUDE_BIN:-claude}
row=$(awk -F'\t' -v r="$ROLE" '$1==r && $0 !~ /^#/ {print; exit}' "$TABLE" 2>/dev/null)
[ -n "$row" ] || { echo "role_not_in_table@$TABLE: $ROLE" >&2; exit 2; }
MODEL=${REVIEW_MODEL:-$(printf '%s' "$row" | cut -f2)}; EFFORT=$(printf '%s' "$row" | cut -f3)
IFS=',' read -r -a ACCOUNTS <<< "$(printf '%s' "$row" | cut -f4)"
[ -f "$PROMPT" ] || { echo "prompt_missing@$PROMPT" >&2; exit 2; }
# Paths are resolved before the change of directory: a prompt or an out file given relative to the caller stays valid.
PROMPT=$(realpath -- "$PROMPT"); OUT=$(realpath -m -- "$OUT")
# The out file is named as a whole name: a companion such as <out>.earlier-findings.md is not the out file.
base=$(basename "$OUT"); base_re=$(printf '%s' "$base" | sed 's/[][\.*^$/+?(){}|]/\\&/g')
if grep -qE -- "${base_re}(\$|[^A-Za-z0-9._-]|\.(\$|[^A-Za-z0-9_-]))" "$PROMPT"; then echo "out_path_named_in_prompt@$PROMPT" >&2; exit 4; fi
cd "$CWD" || exit 2
J="$OUT.json"; RUN="$OUT.run.json"
SESSION=""
if [ "$RESUME" = "--resume" ]; then
  read -r SESSION ACC < <(python3 -I -c 'import json,sys
d=json.load(open(sys.argv[1])); print(d["session"], d["account"])' "$RUN" 2>/dev/null) || true
  [ -n "$SESSION" ] && [ -n "${ACC:-}" ] || { echo "resume_record_missing@$RUN" >&2; exit 2; }
  ACCOUNTS=("$ACC")
elif [ -n "$RESUME" ]; then echo "usage: review.sh <role> <cwd> <prompt file> <out file> [--resume]" >&2; exit 2; fi
# A headless run does not exit while a shell it started in the background is alive: once the session record
# holds a message that begins with "Verdict:" AFTER this run's mark and has been idle for 180 s, end the leftover shells.
# One process reads the whole record (no pipe that ends in `grep -q`: under pipefail it would fail on SIGPIPE).
descendants() { local p c; for p in "$@"; do for c in $(pgrep -P "$p" 2>/dev/null); do echo "$c"; descendants "$c"; done; done; }
# $pid is the timeout wrapper; its child is the reviewer; the leftover shells are below the reviewer.
end_leftover_shells() { local r c
  for r in $(pgrep -P "$1" 2>/dev/null); do for c in $(descendants "$r"); do
    cmd=$(tr '\0' ' ' < "/proc/$c/cmdline" 2>/dev/null); case "$cmd" in "/bin/bash -c "*) kill "$c" 2>/dev/null || true ;; esac
  done; done; }
# The session record of THIS run is the one that holds the run's mark (a last line the runner adds to the
# prompt): another session of the same account never counts.
watch() { local dir=$1 pid=$2 start=$3 f
  while kill -0 "$pid" 2>/dev/null; do sleep "${REVIEW_WATCH_SLEEP:-30}"
    f=$(find "$dir/projects" -maxdepth 2 -name '*.jsonl' -newermt "@$start" -print0 2>/dev/null | xargs -0 -r grep -lF -- "$MARK" 2>/dev/null | head -n 1)
    [ -n "$f" ] || continue
    if [ $(( $(date +%s) - $(stat -c %Y "$f") )) -ge "${REVIEW_IDLE_S:-180}" ] && awk -v m="$MARK" 'seen && index($0, "\"text\":\"Verdict:") {found=1} index($0, m) {seen=1} END {exit !found}' "$f"; then end_leftover_shells "$pid"; fi
  done; }
envelope() { python3 -I - "$J" "$RUN" "$ROLE" "$1" "$OUT" "$SESSION" <<'PY'
import json,re,sys,glob,os
j,run,role,acc,out,asked=sys.argv[1:7]
try: d=json.load(open(j))
except Exception: d={}
res=(d.get("result") or "")
# An answer that begins with "Verdict:" and is not an error is an answer, whatever words it holds.
cut=bool(not d or d.get("is_error") or (len(res)<400 and not res.lstrip().startswith("Verdict:") and re.search(r"usage limit|hit your .*limit|resets ", res, re.I)))
# A resumed run that returns no envelope keeps the session it was asked to resume: the record stays resumable.
json.dump({"role":role,"account":acc,"session":d.get("session_id") or asked or None,"turns":d.get("num_turns"),"cut":cut,"cost_usd":d.get("total_cost_usd")},open(run,"w"))
if cut:
    print("cut turns=%s" % d.get("num_turns")); sys.exit(0)
# The answer is the last message; when a later message hides a verdict, recover it from the session record.
if not res.lstrip().startswith("Verdict:") and d.get("session_id"):
    best=None
    for f in glob.glob(os.path.join(acc,"projects","*","%s.jsonl" % d["session_id"])):
        for line in open(f, errors="replace"):
            try: m=(json.loads(line).get("message") or {})
            except Exception: continue
            if m.get("role")=="assistant" and isinstance(m.get("content"), list):
                for b in m["content"]:
                    if b.get("type")=="text" and b.get("text","").lstrip().startswith("Verdict:"): best=b["text"]
    if best: res=best
open(out,"w").write(res if res.endswith("\n") else res+"\n")
print("ok turns=%s" % d.get("num_turns"))
PY
}
# An answer of an earlier run under the same name is moved aside before this run starts (kept as <out>.prev),
# so that a cut run never leaves an older verdict where a reader expects this run's answer.
if [ -e "$OUT" ]; then mv -f -- "$OUT" "$OUT.prev"; fi
MARK="(review run $(date +%s)-$$-$RANDOM)"
n=0
for ACC in "${ACCOUNTS[@]}"; do
  # Auto permission mode (store decision worker-permission-mode); the host's settings.json already gives it, the flag keeps the runner independent of that file.
  n=$((n+1)); args=(--print --model "$MODEL" --effort "$EFFORT" --permission-mode auto --output-format json --strict-mcp-config)
  [ -n "$SESSION" ] && args+=(--resume "$SESSION")
  start=$(date +%s)
  CLAUDE_CONFIG_DIR="$ACC" timeout "${REVIEW_TIMEOUT:-3300}" "$CLAUDE" "${args[@]}" < <(cat -- "$PROMPT"; printf '\n%s\n' "$MARK") > "$J" 2> "$OUT.err" & pid=$!
  watch "$ACC" "$pid" "$start" & w=$!
  wait "$pid"; rc=$?; kill "$w" 2>/dev/null; wait "$w" 2>/dev/null
  state=$(envelope "$ACC")
  echo "account=$ACC role=$ROLE model=$MODEL $state"
  case "$state" in
    ok*) exit 0 ;;
    "cut turns=None"|"cut turns=0"|"cut turns=1")
      # Cut before any work: the next account of the row may take the same prompt (never for a resume).
      [ -z "$SESSION" ] && [ "$n" -lt "${#ACCOUNTS[@]}" ] && continue
      exit 75 ;;
    *) exit 75 ;;
  esac
done
