#!/bin/sh
set -eu

/usr/sbin/sshd -t -f /etc/ssh/sshd_config
read -r runtime_sshd_pid < /run/sshd.pid
case "$runtime_sshd_pid" in
    ''|*[!0-9]*) exit 1 ;;
esac
kill -0 "$runtime_sshd_pid"

runuser --user palantirkc -- env HOME=/home/palantirkc \
    /usr/local/bin/herdr status server --json \
    | /usr/local/bin/bun -e '
const status = await Bun.stdin.json();
if (status.status === "not_running" && status.running === false) {
  console.log("SSH bootstrap ready; Herdr daemon is not running and will start on native remote attach.");
} else if (
  status.status === "running" &&
  status.running === true &&
  status.version === "0.9.5" &&
  status.endpoint_compatible === true &&
  status.capabilities?.detached_server_daemon === true &&
  status.capabilities?.surface_interest === true &&
  status.capabilities?.health_check === true
) {
  console.log("SSH bootstrap ready; existing Herdr daemon supports native remote attach.");
} else {
  console.error("SSH bootstrap is present but Herdr returned an unsupported runtime state.");
  process.exit(1);
}
'
