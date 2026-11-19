#!/usr/bin/env bash
# A fake `muse` for the tests of `ap run` and `ap unit`. It prints lines in the shapes of the
# five real lines kept of `muse exec --json` (Muse Code CLI 1.4.20): one JSON object per line,
# `payload.kind` naming it (`task_lifecycle`, `run_usage`, `run_terminal`). The configuration
# directory is `$XDG_CONFIG_HOME/muse`; its directory holds the state, keyed by the basename of
# `$XDG_CONFIG_HOME`: `mode-<base>` chooses the behaviour (`script`: the output of
# `script-<base>.sh` in the cwd is the answer; otherwise the lines of `lines-<base>`), `hang-<base>`
# keeps the process alive after its lines, `exit-<base>` is its exit code, `watch` names a file
# copied to `seen.json` when the process starts. `calls.log` gets one line per call, `env.<n>` the
# environment, `stdin.<n>` what the standard input is, `prompt.<n>` the file `--prompt-file` names.
here=$(cd "$(dirname "$0")" && pwd)
n=$(( $(cat "$here/count" 2>/dev/null || echo 0) + 1 )); echo "$n" > "$here/count"
base=$(basename "$XDG_CONFIG_HOME")
printf '%s|%s\n' "$XDG_CONFIG_HOME" "$*" >> "$here/calls.log"
env > "$here/env.$n"
readlink "/proc/$$/fd/0" > "$here/stdin.$n"
sid=; pf=; prev=; for a in "$@"; do case "$prev" in --session-id) sid=$a ;; --prompt-file) pf=$a ;; esac; prev=$a; done
cat "$pf" > "$here/prompt.$n"
[ -f "$here/watch" ] && cp "$(cat "$here/watch")" "$here/seen.json"
mode=$(cat "$here/mode-$base" 2>/dev/null)
case "$mode" in
  script)
    printf '%s\n' '{"payload":{"kind":"task_lifecycle","event":{"kind":"proposed","task_kind":"tool.run_command"}}}'
    printf '%s\n' '{"payload":{"kind":"run_usage","calls":2,"models":[{"model":"muse-wonder-1.0","input_tokens":10,"cached_tokens":4,"output_tokens":3}],"allowance":{"weekly":{"resets_at_ms":1795392000000,"used_percent":2},"window":{"resets_at_ms":1794805703000,"used_percent":7}}}}'
    python3 -I -c 'import json,sys; print(json.dumps({"payload":{"kind":"run_terminal","terminal":"completed","text":sys.argv[1],"reason":None}}))' "$(bash "$here/script-$base.sh")" ;;
  *)
    cat "$here/lines-$base"
    [ -f "$here/hang-$base" ] && sleep 600
    exit "$(cat "$here/exit-$base" 2>/dev/null || echo 0)" ;;
esac
