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
set -euo pipefail

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
