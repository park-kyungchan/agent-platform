#!/usr/bin/env bash
# The engine of this tree for `./check`, chosen before the Rust build (specs/ap/check.md R26, deploy/README.md,
# "An engine change").
#   bash ci/engine.sh
# Run at the repository's top level: the id is the Git tree id of engine/ at HEAD. When
# `git status --porcelain -- engine` prints nothing, in this order: (i) the file <dir>/ont-<id> (<dir> is
# CHECK_ENGINE_DIR, default /usr/local/bin) whose --version prints `ont <version> <id>`, reached through the link
# <root>/tree/ont; (ii) else the `ont` on PATH, else <dir>/ont (the same link), when it prints that line; (iii) else,
# and always for a modified engine/, `cargo build --locked -p ont` of engine/ into <root>/build (<root> is
# ${CARGO_TARGET_DIR:-$PWD/target}/engine-tree), whose output goes to <root>/build.log. A build from a modified
# engine/ prints `unknown`, which no tree names, so the build of `ap` stops engine_differs until the change is
# committed. The chosen path is the one line of standard output and `check: engine <path> (<how>)` is on standard
# error; a build that fails ends with one line and exit 3. It reads only CHECK_ENGINE_DIR, CARGO_TARGET_DIR, PATH,
# Git and the engine files, and writes only under <root>.
set -euo pipefail

[ "$#" -eq 0 ] || { echo "usage: ci/engine.sh" >&2; exit 2; }
dir=${CHECK_ENGINE_DIR:-/usr/local/bin}
[[ "$dir" == /* ]] || { echo "engine_dir_invalid@ci/engine.sh" >&2; exit 2; }
top=$(git rev-parse --show-toplevel 2>/dev/null) || top=
[ -n "$top" ] && [ "$(realpath -- "$top")" = "$(realpath -- "$PWD")" ] \
  || { echo "engine_tree_unreadable@ci/engine.sh: not at the top level of a Git repository" >&2; exit 2; }
entry=$(git -c core.hooksPath=/dev/null -c core.fsmonitor=false ls-tree HEAD -- engine)
id=${entry#040000 tree }
id=${id%$'\t'engine}
[[ "$entry" == "040000 tree "*$'\t'engine && "$id" =~ ^[0-9a-f]{40}$ ]] \
  || { echo "engine_tree_unreadable@ci/engine.sh: HEAD holds no tree engine/" >&2; exit 2; }
dirty=$(git -c core.hooksPath=/dev/null -c core.fsmonitor=false status --porcelain -- engine)
root=${CARGO_TARGET_DIR:-$PWD/target}/engine-tree
names() {
  # Whether the executable $1 prints `ont <version> <id>`.
  local line w1 w2 w3 extra
  [ -f "$1" ] && [ -x "$1" ] || return 1
  line=$("$1" --version 2>/dev/null | head -n 1) || return 1
  read -r w1 w2 w3 extra <<<"$line" || return 1
  [ "$w1" = ont ] && [ -n "$w2" ] && [ "$w3" = "$id" ] && [ -z "$extra" ]
}
linked() {
  mkdir -p "$root/tree"
  ln -sfn "$1" "$root/tree/ont"
  echo "check: engine $1 ($2)" >&2
  echo "$root/tree/ont"
  exit 0
}
if [ -z "$dirty" ]; then
  if names "$dir/ont-$id"; then linked "$dir/ont-$id" tree; fi
  path_ont=$(command -v ont || true)
  if [ -n "$path_ont" ] && names "$path_ont"; then
    echo "check: engine $path_ont (path)" >&2
    echo "$path_ont"
    exit 0
  fi
  if names "$dir/ont"; then linked "$dir/ont" installed; fi
fi
mkdir -p "$root"
if (cd engine && CARGO_TARGET_DIR="$root/build" cargo build --locked -p ont) > "$root/build.log" 2>&1; then
  echo "check: engine $root/build/debug/ont (built)" >&2
  echo "$root/build/debug/ont"
  exit 0
fi
echo "engine_missing@ci/engine.sh: no engine names the tree $id of engine/ and its build failed ($root/build.log)" >&2
exit 3
