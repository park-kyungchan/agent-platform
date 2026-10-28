#!/usr/bin/env bash
set -euo pipefail
# Resolve data from the caller checkout before installing a runtime or running owner code.
version=${COMMON_CI_BUN_VERSION:-}
version_file=${COMMON_CI_BUN_VERSION_FILE:-}
if [[ -n "$version_file" ]]; then
  [[ -z "$version" && "$version_file" == .bun-version && -f "$GITHUB_WORKSPACE/$version_file" && ! -L "$GITHUB_WORKSPACE/$version_file" ]] || {
    echo 'bun_version_input_refused@ci/reuse/resolve-bun-version.sh:8' >&2
    exit 1
  }
  version=$(cat -- "$GITHUB_WORKSPACE/$version_file")
fi
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
  echo 'bun_version_required@ci/reuse/resolve-bun-version.sh:14' >&2
  exit 1
}
printf 'bun-version=%s\n' "$version" >> "$GITHUB_OUTPUT"
