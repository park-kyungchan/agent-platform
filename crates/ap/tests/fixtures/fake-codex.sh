#!/usr/bin/env bash
# A fake `codex` for the tests of `ap run` and `ap unit`. It prints `--json` events in the shape a
# real `codex --no-daemon exec --json` printed (Codex CLI 0.167.2): `thread.started`,
# `turn.started`, `item.started`/`item.completed`, `error`, `turn.completed` with the usage of the
# thread so far, `turn.failed`. Its directory holds its state: `mode-<home basename>` chooses the
# behaviour, `calls.log` gets one line per call, `env.<n>` the environment, `prompt.<n>` stdin.
here=$(cd "$(dirname "$0")" && pwd)
n=$(( $(cat "$here/count" 2>/dev/null || echo 0) + 1 )); echo "$n" > "$here/count"
printf '%s|%s\n' "$CODEX_HOME" "$*" >> "$here/calls.log"
env > "$here/env.$n"
cat > "$here/prompt.$n"
# A resume names the thread before its last argument `-`; a fresh thread gets a new id.
tid=$(printf '00000000-0000-7000-8000-%012d' "$n"); prev=
case " $* " in *" resume "*) for a in "$@"; do [ "$a" = - ] && tid=$prev; prev=$a; done ;; esac
k=$(( $(cat "$here/thread-$tid" 2>/dev/null || echo 0) + 1 )); echo "$k" > "$here/thread-$tid"
mode=$(cat "$here/mode-$(basename "$CODEX_HOME")" 2>/dev/null)
e() { printf '%s\n' "$1"; }
started() { e '{"type":"thread.started","thread_id":"'"$tid"'"}'; e '{"type":"turn.started"}'; }
said() { python3 -I -c 'import json,sys; print(json.dumps({"type":"item.completed","item":{"id":"item_9","type":"agent_message","text":sys.argv[1]}}))' "$1"; }
thought() { e '{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"thinking"}}'; }
tool() { e '{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"true","status":"in_progress"}}'
         e '{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"true","exit_code":0,"status":"completed"}}'; }
# The usage of the whole thread: the k-th turn of a thread reports k times one turn's usage.
usage() { printf '{"input_tokens":%d,"cached_input_tokens":%d,%s"output_tokens":%d,"reasoning_output_tokens":0}' \
  $((18839 * k)) $((12288 * k)) "$([ "$1" = nowrite ] || printf '"cache_write_input_tokens":%d,' $((100 * k)))" $((5 * k)); }
completed() { e '{"type":"turn.completed","usage":'"$(usage "$1")"'}'; }
failed() { python3 -I -c 'import json,sys; print(json.dumps({"type":"error","message":sys.argv[1]})); print(json.dumps({"type":"turn.failed","error":{"message":sys.argv[1]}}))' "$1"; exit 1; }
reconnect() { e '{"type":"error","message":"Reconnecting... '"$1"'/5 (unexpected status 401 Unauthorized: Missing bearer or basic authentication in header)"}'; }
case "$mode" in
  failed)     started; failed "The 'no-such-model' model is not supported when using Codex with a ChatGPT account." ;;
  limit)      started; failed "You've hit your usage limit. Visit https://chatgpt.com/settings/usage to purchase more credits or try again at 6:30 PM." ;;
  limit2)     started; failed "Usage limit reached. You've reached your usage limit. Increase your limits to continue using codex." ;;
  errors)     # reconnection errors (one with the usage-limit text) before a turn that completes
              started; reconnect 1; reconnect 2; e '{"type":"error","message":"You have hit your usage limit, reconnecting"}'
              said "stage ok"; completed ;;
  unfinished) started; tool; said "stage ok"; exit 1 ;;
  msgfailed)  started; said "stage ok"; failed "stream disconnected before completion" ;;
  noend)      started; tool ;;
  hang)       started; said "stage ok"; sleep 600 ;;
  empty)      started; thought; completed ;;
  nowrite)    started; said "stage ok"; completed nowrite ;;
  watch)      # the record as it is after the thread started, while the CLI runs
              started; for _ in $(seq 100); do grep -q "$tid" "$(cat "$here/watch")" && break; sleep 0.05; done
              cp "$(cat "$here/watch")" "$here/seen.json"; failed "unexpected status 401 Unauthorized" ;;
  script)     # runs `script-<home basename>.sh` in the cwd; its stdout is the answer (tests/unit.rs)
              started; tool; said "$(bash "$here/script-$(basename "$CODEX_HOME").sh")"; completed ;;
  *)          started; thought; tool; said "first message"; said "$(printf 'stage ok\nsecond line')"; completed ;;
esac
