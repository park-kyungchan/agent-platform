#!/usr/bin/env bash
set -euo pipefail
# This launcher itself must come from a separately qualified immutable common checkout.
# Never use the proposed subject checkout as the controller or as Bun's startup cwd.
controller_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)
[[ $# -ge 4 && "$4" =~ ^[a-f0-9]{40}$ ]] || {
  echo 'trusted_controller_arguments_required@ci/reuse/review-pr.sh:7' >&2
  exit 1
}
cd -- "$controller_root"
controller_head=$(git --no-replace-objects -c core.hooksPath=/dev/null -c core.fsmonitor=false rev-parse HEAD)
controller_ref=$(git --no-replace-objects -c core.hooksPath=/dev/null -c core.fsmonitor=false rev-parse --abbrev-ref HEAD)
[[ "$controller_head" == "$4" && "$controller_ref" == HEAD ]] || {
  echo 'immutable_controller_required@ci/reuse/review-pr.sh:14' >&2
  exit 1
}
git --no-replace-objects -c core.hooksPath=/dev/null -c core.fsmonitor=false diff --quiet HEAD --
exec bun --no-env-file --no-install "$controller_root/ci/reuse/review-pr.ts" "$@"
