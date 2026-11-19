#!/usr/bin/env bash
# A fake `agy` for the tests of `ap run` and `ap unit`. The lines below are copied from two real
# runs of `agy --print … --output-format stream-json` (Antigravity CLI 1.5.9, model
# gemini-4-hydrogen-medium): a fresh one that ran one command, and a `--conversation` resume of it.
# Every member is kept; only the conversation id is replaced by `@CID@` (the fake sets it to a new
# id for a fresh run and to the id of `--conversation` for a resume) and the scratch working
# directory by `/tmp/agy-evidence`. `$HOME` is a home `<account>/homes/<uuid>`; this directory holds
# the state, keyed by the basename of the account: `mode-<base>` chooses the behaviour (`script`: the output of
# `script-<base>.sh` in the cwd is the answer; otherwise the lines of `lines-<base>` when it
# exists, else the real lines below), `hang-<base>` keeps the process alive after its lines,
# `exit-<base>` is its exit code, `watch` names a file copied to `seen.json` when the process
# starts. `calls.log` gets one line per call (the prompt shown as `<prompt>`), `env.<n>` the
# environment, `stdin.<n>` what the standard input is, `prompt.<n>` the prompt argument, `cwd.<n>`
# the working directory, `home.<n>` the names under `$HOME` (path, type, link target) and
# `homemode.<n>` its mode, both as the call finds them. As the real CLI does at a fresh run, the fake creates in its `$HOME`
# `.gemini/config/` (`config.json`, `mcp_config.json`, `.migrated`, `projects/`) and the conversation
# file `.gemini/antigravity-cli/conversations/<id>.db`; a `--conversation` whose file its `$HOME`
# lacks fails (stderr, exit 1, no line).
here=$(cd "$(dirname "$0")" && pwd)
n=$(( $(cat "$here/count" 2>/dev/null || echo 0) + 1 )); echo "$n" > "$here/count"
base=$(basename "$(dirname "$(dirname "$HOME")")")
cid=; prompt=; shown=(); prev=
for a in "$@"; do
  case "$prev" in --conversation) cid=$a ;; --print) prompt=$a; a='<prompt>' ;; esac
  shown+=("$a"); prev=$a
done
printf '%s|%s\n' "$HOME" "${shown[*]}" >> "$here/calls.log"
env > "$here/env.$n"
readlink "/proc/$$/fd/0" > "$here/stdin.$n"
printf '%s' "$prompt" > "$here/prompt.$n"
pwd > "$here/cwd.$n"
(cd "$HOME" && find . -printf '%p %y %l\n' | sort) > "$here/home.$n"
stat -c %a "$HOME" > "$here/homemode.$n"
again=1; [ -n "$cid" ] || { again=0; cid=$(cat /proc/sys/kernel/random/uuid); }
conv="$HOME/.gemini/antigravity-cli/conversations/$cid.db"
if [ "$again" = 1 ]; then
  [ -f "$conv" ] || { echo "conversation $cid not found" >&2; exit 1; }
else
  mkdir -p "$HOME/.gemini/config/projects" "$(dirname "$conv")"
  printf '{"plugins":{"swarm":{"enabled":true}}}' > "$HOME/.gemini/config/config.json"
  : > "$HOME/.gemini/config/mcp_config.json"; : > "$HOME/.gemini/config/.migrated"
  printf '{}' > "$HOME/.gemini/config/projects/default-cli-project.json"
  printf 'conversation' > "$conv"
fi
[ -f "$here/watch" ] && cp "$(cat "$here/watch")" "$here/seen.json"
fresh=(
  '{"event":"init","conversation_id":"@CID@","init":{"model":"gemini-4-hydrogen-medium","cwd":"/tmp/agy-evidence","tools":["ask_custom_permission","ask_permission","ask_question","browser_click_element","browser_drag_pixel_to_pixel","browser_get_dom","browser_get_network_request","browser_input","browser_list_network_requests","browser_mouse_down","browser_mouse_up","browser_move_mouse","browser_press_key","browser_refresh_page","browser_resize_window","browser_scroll","browser_scroll_dom","browser_select_option","browser_subagent","call_mcp_tool","capture_browser_console_logs","capture_browser_screenshot","click_browser_pixel","command_status","define_subagent","delete_knowledge","execute_browser_javascript","find_by_name","finish","generate_image","grep_search","invoke_subagent","list_browser_pages","list_dir","list_permissions","list_plugin_accounts","list_resources","manage_inbox","manage_subagents","manage_task","multi_replace_file_content","notebook_edit","notebook_execution","open_browser_url","read_browser_page","read_resource","read_url_content","replace_file_content","run_command","run_workflow","schedule","search_marketplace","search_web","sed_file","send_command_input","send_message","view_file","wait","wait_5_seconds","write_to_file"],"permission_mode":"always-proceed"}}'
  '{"event":"step_update","step_update":{"conversation_id":"@CID@","step_index":0,"state":"DONE","step_type":"user_input"}}'
  '{"event":"step_update","step_update":{"conversation_id":"@CID@","step_index":1,"state":"DONE","step_type":"agent_response","duration_seconds":3.156915782,"usage":{"input_tokens":12778,"output_tokens":110,"thinking_tokens":62,"cache_read_tokens":0,"total_tokens":12888}}}'
  '{"event":"step_update","step_update":{"conversation_id":"@CID@","step_index":2,"state":"ACTIVE","step_type":"tool","tool_name":"run_command","tool_info":{"name":"run_command","parameters":{"CommandLine":"echo agy-evidence"}}}}'
  '{"event":"step_update","step_update":{"conversation_id":"@CID@","step_index":2,"state":"DONE","step_type":"tool","tool_name":"run_command","duration_seconds":0.253579099,"tool_info":{"name":"run_command","parameters":{"CommandLine":"echo agy-evidence"},"output":"Created At: 2026-11-16T06:12:18Z\nCompleted At: 2026-11-16T06:12:18Z\nCommand exited with code 0.\n\nCommand output:\nagy-evidence\r\n"}}}'
  '{"event":"step_update","step_update":{"conversation_id":"@CID@","step_index":3,"state":"ACTIVE","step_type":"agent_response","text_delta":"done"}}'
  '{"event":"step_update","step_update":{"conversation_id":"@CID@","step_index":3,"state":"DONE","step_type":"agent_response","text_delta":"\n","duration_seconds":1.7687590279999998,"usage":{"input_tokens":2247,"output_tokens":24,"thinking_tokens":19,"cache_read_tokens":10724,"total_tokens":2271}}}'
  '{"event":"result","result":{"conversation_id":"@CID@","status":"SUCCESS","response":"done\n","duration_seconds":4.984669374,"num_turns":1,"usage":{"input_tokens":15025,"output_tokens":134,"thinking_tokens":81,"cache_read_tokens":10724,"total_tokens":15159}}}'
)
resumed=(
  '{"event":"init","conversation_id":"@CID@","init":{"model":"gemini-4-hydrogen-medium","cwd":"/tmp/agy-evidence","tools":["ask_custom_permission","ask_permission","ask_question","browser_click_element","browser_drag_pixel_to_pixel","browser_get_dom","browser_get_network_request","browser_input","browser_list_network_requests","browser_mouse_down","browser_mouse_up","browser_move_mouse","browser_press_key","browser_refresh_page","browser_resize_window","browser_scroll","browser_scroll_dom","browser_select_option","browser_subagent","call_mcp_tool","capture_browser_console_logs","capture_browser_screenshot","click_browser_pixel","command_status","define_subagent","delete_knowledge","execute_browser_javascript","find_by_name","finish","generate_image","grep_search","invoke_subagent","list_browser_pages","list_dir","list_permissions","list_plugin_accounts","list_resources","manage_inbox","manage_subagents","manage_task","multi_replace_file_content","notebook_edit","notebook_execution","open_browser_url","read_browser_page","read_resource","read_url_content","replace_file_content","run_command","run_workflow","schedule","search_marketplace","search_web","sed_file","send_command_input","send_message","view_file","wait","wait_5_seconds","write_to_file"],"permission_mode":"always-proceed"}}'
  '{"event":"step_update","step_update":{"conversation_id":"@CID@","step_index":4,"state":"DONE","step_type":"user_input"}}'
  '{"event":"step_update","step_update":{"conversation_id":"@CID@","step_index":5,"state":"DONE","step_type":"system_message","duration_seconds":0.000363912}}'
  '{"event":"step_update","step_update":{"conversation_id":"@CID@","step_index":6,"state":"ACTIVE","step_type":"agent_response","text_delta":"again"}}'
  '{"event":"step_update","step_update":{"conversation_id":"@CID@","step_index":6,"state":"DONE","step_type":"agent_response","text_delta":"\n","duration_seconds":5.996873795,"usage":{"input_tokens":2278,"output_tokens":28,"thinking_tokens":23,"cache_read_tokens":10922,"total_tokens":2306}}}'
  '{"event":"result","result":{"conversation_id":"@CID@","status":"SUCCESS","response":"again\n","duration_seconds":27.767046091,"num_turns":2,"usage":{"input_tokens":17303,"output_tokens":162,"thinking_tokens":104,"cache_read_tokens":21646,"total_tokens":17465}}}'
)
emit() { local l; for l in "$@"; do printf '%s\n' "${l//@CID@/$cid}"; done; }
mode=$(cat "$here/mode-$base" 2>/dev/null)
case "$mode" in
  script)
    emit "${fresh[0]}" "${fresh[1]}" "${fresh[3]}" "${fresh[4]}"
    python3 -I -c 'import json,sys; d=json.loads(sys.argv[1].replace("@CID@",sys.argv[3])); d["result"]["response"]=sys.argv[2]; print(json.dumps(d,separators=(",",":")))' "${fresh[7]}" "$(bash "$here/script-$base.sh")" "$cid" ;;
  *)
    if [ -f "$here/lines-$base" ]; then
      while IFS= read -r l; do
        printf '%s\n' "${l//@CID@/$cid}"
        # `watch-late` names a file copied to `seen-late.json` once, a second after the first line.
        if [ -f "$here/watch-late" ]; then sleep 1; cp "$(cat "$here/watch-late")" "$here/seen-late.json"; rm "$here/watch-late"; fi
      done < "$here/lines-$base"
    elif [ "$again" = 1 ]; then emit "${resumed[@]}"
    else emit "${fresh[@]}"; fi
    [ -f "$here/hang-$base" ] && sleep 600
    exit "$(cat "$here/exit-$base" 2>/dev/null || echo 0)" ;;
esac
