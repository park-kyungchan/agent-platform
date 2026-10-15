#!/usr/bin/env bash
# Source this helper from the two launchers. Authentication belongs to native SSH config.

hermes_backend_exec() {
  local interactive="$1"
  shift
  local target="${HERMES_BACKEND_SSH_TARGET:-root@100.95.133.99}"
  local remote_command='' argument escaped
  local -a ssh_options=(
    -o BatchMode=yes
    -o StrictHostKeyChecking=yes
    -o HostKeyAlias=srv1873899
    -o ConnectTimeout=10
    -o ServerAliveInterval=30
    -o ServerAliveCountMax=3
  )
  local -a remote_argv=(docker exec)
  if [[ "$interactive" == 'yes' ]]; then
    ssh_options+=(-tt)
    remote_argv+=(-it)
  else
    ssh_options+=(-T)
  fi
  remote_argv+=(
    --user 10000:10000
    --env HOME=/opt/data
    --env HERMES_HOME=/opt/data
    --env "TERM=${TERM:-xterm-256color}"
    --env HERMES_TUI_QUERY=
    --env HERMES_TUI_IMAGE=
    --env HERMES_TUI_GATEWAY_URL=
    --env HERMES_TUI_SIDECAR_URL=
    --workdir /opt/data
    hermes-agent-2q3y-hermes-serve-1
    "$@"
  )
  # SSH's remote command is a shell string. Quote every argv element for POSIX shells.
  for argument in "${remote_argv[@]}"; do
    escaped=${argument//\'/\'\\\'\'}
    remote_command+="'$escaped' "
  done
  exec ssh "${ssh_options[@]}" -- "$target" "$remote_command"
}
