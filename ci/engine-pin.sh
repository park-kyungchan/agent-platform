#!/usr/bin/env bash
# The engine of ontology/ENGINE-PIN for `./check`, chosen before the Rust build: deploy/README.md, "The engine of a pin".
#   bash ci/engine-pin.sh <pin>      (<pin>: the 40 lowercase hexadecimal digits of ontology/ENGINE-PIN)
# In this order: the file <dir>/ont-<pin> (<dir> is CHECK_ENGINE_DIR, default /usr/local/bin) when it is an
# executable that names the pin, reached through the link <root>/pinned/ont so that the directory of the engine
# holds nothing else; else the `ont` on PATH when it names the pin; else `cargo install` of the pin into <root>
# (<root> is ${CARGO_TARGET_DIR:-$PWD/target}/engine-pin), whose output goes to <root>/install.log. The chosen
# path is the one line of standard output and `check: engine <path> (<how>)` is on standard error; a build that
# fails ends with one line and exit 3. It reads only its argument, CHECK_ENGINE_DIR, CARGO_TARGET_DIR, PATH and
# the engine files, and writes only under <root>.
#   bash ci/engine-pin.sh --tree     (a tree without ontology/ENGINE-PIN that holds engine/: specs/ap/check.md R26)
# The tree rule, run at the repository's top level: the id is the Git tree id of engine/ at HEAD. When
# `git status --porcelain -- engine` prints nothing, in this order: the file <dir>/ont-<id> whose --version
# prints `ont <version> <id>`, reached through the link <root>/tree/ont; else the `ont` on PATH, else <dir>/ont
# (the same link), when it prints that line; else, and always for a modified engine/, `cargo build --locked -p ont`
# of engine/ into <root>/build (<root> is ${CARGO_TARGET_DIR:-$PWD/target}/engine-tree), whose output goes to
# <root>/build.log. A build from a modified engine/ prints `unknown`, which no tree names, so the build of `ap`
# stops engine_differs until the change is committed. The chosen path is the one line of standard output and
# `check: engine <path> (<how>)` is on standard error.
set -euo pipefail

if [ "${1:-}" = --tree ]; then
  [ "$#" -eq 1 ] || { echo "usage: ci/engine-pin.sh --tree" >&2; exit 2; }
  dir=${CHECK_ENGINE_DIR:-/usr/local/bin}
  [[ "$dir" == /* ]] || { echo "engine_dir_invalid@ci/engine-pin.sh" >&2; exit 2; }
  top=$(git rev-parse --show-toplevel 2>/dev/null) || top=
  [ -n "$top" ] && [ "$(realpath -- "$top")" = "$(realpath -- "$PWD")" ] \
    || { echo "engine_tree_unreadable@ci/engine-pin.sh: not at the top level of a Git repository" >&2; exit 2; }
  entry=$(git -c core.hooksPath=/dev/null -c core.fsmonitor=false ls-tree HEAD -- engine)
  id=${entry#040000 tree }
  id=${id%$'\t'engine}
  [[ "$entry" == "040000 tree "*$'\t'engine && "$id" =~ ^[0-9a-f]{40}$ ]] \
    || { echo "engine_tree_unreadable@ci/engine-pin.sh: HEAD holds no tree engine/" >&2; exit 2; }
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
  echo "engine_missing@ci/engine-pin.sh: no engine names the tree $id of engine/ and its build failed ($root/build.log)" >&2
  exit 3
fi

[ "$#" -eq 1 ] || { echo "usage: ci/engine-pin.sh <pin>" >&2; exit 2; }
pin=$1
[[ "$pin" =~ ^[0-9a-f]{40}$ ]] || { echo "engine_pin_invalid@ci/engine-pin.sh" >&2; exit 2; }
dir=${CHECK_ENGINE_DIR:-/usr/local/bin}
[[ "$dir" == /* ]] || { echo "engine_dir_invalid@ci/engine-pin.sh" >&2; exit 2; }

pinned=$dir/ont-$pin
root=${CARGO_TARGET_DIR:-$PWD/target}/engine-pin
want="ont 0.1.0 $pin"

if [ -f "$pinned" ] && [ -x "$pinned" ] && [ "$("$pinned" --version 2>/dev/null || true)" = "$want" ]; then
  mkdir -p "$root/pinned"
  ln -sfn "$pinned" "$root/pinned/ont"
  echo "check: engine $pinned (pinned)" >&2
  echo "$root/pinned/ont"
  exit 0
fi

path_ont=$(command -v ont || true)
if [ -n "$path_ont" ] && [ -f "$path_ont" ] && [ "$("$path_ont" --version 2>/dev/null || true)" = "$want" ]; then
  echo "check: engine $path_ont (path)" >&2
  echo "$path_ont"
  exit 0
fi

mkdir -p "$root"
if cargo install --locked --git https://github.com/park-kyungchan/ontology-platform --rev "$pin" \
  --root "$root" --target-dir "$root/build" ont > "$root/install.log" 2>&1; then
  echo "check: engine $root/bin/ont (built)" >&2
  echo "$root/bin/ont"
  exit 0
fi
echo "engine_missing@ci/engine-pin.sh: $pinned is not the engine of the pin, nor is ont on PATH, and the build of the pin failed ($root/install.log); see deploy/README.md, \"The engine of a pin\"" >&2
exit 3
