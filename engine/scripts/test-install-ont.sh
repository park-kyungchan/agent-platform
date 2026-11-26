#!/bin/sh
# Test scripts/install-ont.sh in a temporary directory with a small
# stand-in binary (a shell script that prints the expected version line).
# No root, no host path is touched. The engine of a commit is engine/ of that
# commit (specs/ap/deliver.md R67): the fixture is a repository whose engine/
# holds the script, and the engine's id is the tree id of engine/ at the
# commit installed.
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd)
INSTALL=$SCRIPT_DIR/install-ont.sh

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

fail() {
  printf 'FAIL: %s\n' "$1" >&2
  exit 1
}

# A disposable repository around engine/, never the host checkout. Its
# remote names a path that does not exist: the script fetches nothing.
mkdir -p "$TMP/checkout/engine/scripts"
cp "$INSTALL" "$TMP/checkout/engine/scripts/install-ont.sh"
printf '[workspace]\n' >"$TMP/checkout/engine/Cargo.toml"
INSTALL=$TMP/checkout/engine/scripts/install-ont.sh
git -C "$TMP/checkout" init -q -b main
git -C "$TMP/checkout" add -A
git -C "$TMP/checkout" -c user.name=Test -c user.email=test@example.invalid commit -qm fixture
git -C "$TMP/checkout" remote add origin "$TMP/no-such-origin.git"
COMMIT=$(git -C "$TMP/checkout" rev-parse HEAD)
GOOD=$(git -C "$TMP/checkout" rev-parse HEAD:engine)

make_standin() {
  # $1 = path, $2 = marker comment (makes the bytes differ)
  {
    printf '#!/bin/sh\n'
    printf '# %s\n' "$2"
    printf 'if [ "${1:-}" = "--version" ]; then printf "ont 0.1.0 %s\\n" "%s"; exit 0; fi\n' "$GOOD" "$GOOD"
    printf 'printf "stand-in ont\\n"\n'
  } >"$1"
  chmod 0755 "$1"
}

BIN1=$TMP/bin
STATE1=$TMP/state
make_standin "$TMP/ont-v1" v1
make_standin "$TMP/ont-v2" v2
SHA_V1=$(sha256sum "$TMP/ont-v1" | awk '{print $1}')
SHA_V2=$(sha256sum "$TMP/ont-v2" | awk '{print $1}')

run_test_install() {
  # $1 = prebuilt path; extra env is inherited from the caller.
  ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" \
    ONT_INSTALL_PREBUILT="$1" "$INSTALL" install "$COMMIT" >/dev/null
}

# install (prebuilt stand-in) and check passes; installed.json names the tree id.
run_test_install "$TMP/ont-v1"
[ -x "$BIN1/ont" ] || fail "binary was not installed"
grep -q "\"commit\":\"$GOOD\"" "$STATE1/install/installed.json" || fail "installed.json does not name the tree id"
ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" \
  "$INSTALL" check >/dev/null || fail "check failed after first install"

# A dry-run install changes nothing.
before=$(sha256sum "$BIN1/ont" | awk '{print $1}')
ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" \
  ONT_INSTALL_PREBUILT="$TMP/ont-v2" "$INSTALL" install "$COMMIT" --dry-run >/dev/null
after=$(sha256sum "$BIN1/ont" | awk '{print $1}')
[ "$before" = "$after" ] || fail "dry-run changed the installed binary"

# A second install keeps the first as previous.
run_test_install "$TMP/ont-v2"
PREV12=$(printf '%s' "$SHA_V1" | cut -c1-12)
[ -f "$STATE1/install/ont.before-$PREV12" ] \
  || fail "previous binary was not kept"
[ "$(sha256sum "$BIN1/ont" | awk '{print $1}')" = "$SHA_V2" ] || fail "second install did not replace the binary"
ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" \
  "$INSTALL" check >/dev/null || fail "check failed after second install"

# A changed binary makes check exit 2.
printf 'x' >>"$BIN1/ont"
set +e
ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" \
  "$INSTALL" check >/dev/null 2>&1
code=$?
set -e
[ "$code" -eq 2 ] || fail "check on a changed binary exited $code, want 2"

# Rollback restores the first binary and check passes.
ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" \
  "$INSTALL" rollback >/dev/null || fail "rollback failed"
[ "$(sha256sum "$BIN1/ont" | awk '{print $1}')" = "$SHA_V1" ] || fail "rollback did not restore the first binary"
ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" \
  "$INSTALL" check >/dev/null || fail "check failed after rollback"

# spec: specs/ont/cli.md#R10
# Refusals.
set +e
ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" \
  ONT_INSTALL_PREBUILT="$TMP/ont-v1" "$INSTALL" install not-a-commit >/dev/null 2>&1
code=$?
[ "$code" -eq 2 ] || fail "bad commit exited $code, want 2"

ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" \
  ONT_INSTALL_PREBUILT="$TMP/ont-v1" "$INSTALL" install "$COMMIT" >"$TMP/guard.err" 2>&1
code=$?
[ "$code" -eq 2 ] || fail "overrides without ONT_INSTALL_TEST exited $code, want 2"
rg_guard=$(cat "$TMP/guard.err")
case $rg_guard in *"require ONT_INSTALL_TEST=1"*) ;; *) fail "wrong override guard reason" ;; esac

ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR=/usr/local/bin ONT_INSTALL_STATE_DIR="$STATE1" \
  ONT_INSTALL_PREBUILT="$TMP/ont-v1" "$INSTALL" install "$COMMIT" >/dev/null 2>&1
code=$?
set -e
[ "$code" -eq 2 ] || fail "override equal to the real directory exited $code, want 2"

# Every refusal below must identify the actual guard, not merely exit 2.
expect_refusal() {
  expected=$1
  shift
  set +e
  "$@" >"$TMP/refusal" 2>&1
  code=$?
  set -e
  [ "$code" -eq 2 ] || fail "expected refusal $expected, got $code"
  case $(cat "$TMP/refusal") in *"$expected"*) ;; *) fail "missing refusal message $expected" ;; esac
}
fixture() {
  env ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" "$INSTALL" "$@"
}
expect_refusal 'no previous binary recorded' fixture rollback
expect_refusal 'require ONT_INSTALL_TEST=1' env ONT_INSTALL_PREBUILT="$TMP/ont-v1" "$INSTALL" check
expect_refusal 'must not equal the real directories' env ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR=/var/lib/ontology-platform "$INSTALL" check
ln -s /usr/local/bin "$TMP/real-bin"
ln -s /var/lib/ontology-platform "$TMP/real-state"
expect_refusal 'must not equal the real directories' env ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$TMP/real-bin" ONT_INSTALL_STATE_DIR="$STATE1" "$INSTALL" check
expect_refusal 'must not equal the real directories' env ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$TMP/real-state" "$INSTALL" check
expect_refusal 'must not equal the real directories' env ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR=/usr//local/bin/ ONT_INSTALL_STATE_DIR="$STATE1" "$INSTALL" check
# The guard reason also holds when id would report an unprivileged user.
mkdir "$TMP/tools"
printf '#!/bin/sh\nprintf "1000\\n"\n' >"$TMP/tools/id"
chmod +x "$TMP/tools/id"
expect_refusal 'require ONT_INSTALL_TEST=1' env PATH="$TMP/tools:$PATH" ONT_INSTALL_BIN_DIR="$BIN1" "$INSTALL" check

cp "$STATE1/install/installed.json" "$TMP/good.json"
sed "s/\"commit\":\"$GOOD\"/\"commit\":\"0000000000000000000000000000000000000000\"/" "$TMP/good.json" >"$STATE1/install/installed.json"
expect_refusal 'mismatch: --version commit' fixture check
cp "$TMP/good.json" "$STATE1/install/installed.json"
chmod 0755 "$STATE1/ont"
expect_refusal 'want 0700' fixture check
chmod 0700 "$STATE1/ont"
run_test_install "$TMP/ont-v2"
rm "$STATE1/install/ont.before-$PREV12"
expect_refusal 'previous binary not found' fixture rollback
# spec: specs/ont/cli.md#R9
# Bad previous version must leave binary/state intact and remove temporary names.
printf '#!/bin/sh\nexit 1\n' >"$TMP/bad-version"
cp "$TMP/bad-version" "$BIN1/ont"
run_test_install "$TMP/ont-v2"
cp "$STATE1/install/installed.json" "$TMP/before.json"
expect_refusal 'previous binary does not print a version line' fixture rollback
cmp "$TMP/before.json" "$STATE1/install/installed.json" || fail 'rollback changed state before version validation'
[ "$(sha256sum "$BIN1/ont" | awk '{print $1}')" = "$SHA_V2" ] || fail 'bad version replaced binary'
[ -z "$(find "$BIN1" -name '.ont.*' -print)" ] || fail 'stray rollback temporary'
# spec: specs/ont/cli.md#R5
# Keep at most three previous copies, only after state is written.
for n in 3 4 5 6; do
  make_standin "$TMP/ont-next" "v$n"
  run_test_install "$TMP/ont-next"
done
[ "$(find "$STATE1/install" -name 'ont.before-*' | wc -l)" -eq 3 ] || fail 'retention did not keep three copies'

# retention_preserves_recorded_previous: count alone cannot prove rollback works.
retained_sha=$(sed -n 's/.*"previous_sha256":"\([^"]*\)".*/\1/p' "$STATE1/install/installed.json")
retained12=$(printf '%s' "$retained_sha" | cut -c1-12)
[ -f "$STATE1/install/ont.before-$retained12" ] || fail 'pruned the recorded previous binary'
fixture rollback >/dev/null || fail 'rollback failed after pruning'
[ "$(sha256sum "$BIN1/ont" | awk '{print $1}')" = "$retained_sha" ] || fail 'rollback after pruning restored wrong bytes'
# rollback_creates_missing_bin_directory before opening its temporary file.
run_test_install "$TMP/ont-v2"
mv "$BIN1" "$TMP/bin-before-rollback"
fixture rollback >/dev/null || fail 'rollback did not create missing binary directory'
fixture check >/dev/null || fail 'check failed after recreating binary directory'

# spec: specs/ont/cli.md#R1
# No fetch and no ancestor check: the remote does not exist, and a commit on
# no branch installs; the refs are unchanged.
refs_before=$(git -C "$TMP/checkout" for-each-ref)
fixture install "$COMMIT" --dry-run >"$TMP/dry"
case $(cat "$TMP/dry") in *"engine/ of $COMMIT is the tree $GOOD; no fetch and no ancestor check"*) ;; *) fail 'dry run did not name the tree' ;; esac
case $(cat "$TMP/dry") in *fetch\ origin*) fail 'dry run names a fetch' ;; esac
printf 'side\n' >"$TMP/checkout/side.txt"
git -C "$TMP/checkout" add side.txt
git -C "$TMP/checkout" -c user.name=Test -c user.email=test@example.invalid commit -qm side
SIDE=$(git -C "$TMP/checkout" rev-parse HEAD)
git -C "$TMP/checkout" reset -q --hard "$COMMIT"
refs_before=$(git -C "$TMP/checkout" for-each-ref)
make_standin "$TMP/new-head" newer
ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" \
  ONT_INSTALL_PREBUILT="$TMP/new-head" "$INSTALL" install "$SIDE" >/dev/null || fail 'a commit on no branch did not install'
[ "$(git -C "$TMP/checkout" for-each-ref)" = "$refs_before" ] || fail 'install changed a ref'
fixture check >/dev/null || fail 'check failed after the install of a side commit'
# spec: specs/ont/cli.md#R6
# inherited_git_environment_does_not_change_other_repository_refs.
mkdir "$TMP/other"
git -C "$TMP/other" init -q
git -C "$TMP/other" remote add origin "$TMP/no-such-origin.git"
other_before=$(git -C "$TMP/other" for-each-ref)
env GIT_DIR="$TMP/other/.git" GIT_WORK_TREE="$TMP/other" GIT_INDEX_FILE="$TMP/other/index" GIT_COMMON_DIR="$TMP/other/.git" \
  ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" ONT_INSTALL_PREBUILT="$TMP/new-head" "$INSTALL" install "$COMMIT" >/dev/null
[ "$(git -C "$TMP/other" for-each-ref)" = "$other_before" ] || fail 'inherited Git environment changed another repository ref'
# spec: specs/ont/cli.md#R7
# unknown_commit_has_its_own_refusal; a commit without engine/ is engine_missing.
expect_refusal 'commit_unknown: 0000000000000000000000000000000000000000' fixture install 0000000000000000000000000000000000000000
git -C "$TMP/checkout" checkout -q --orphan bare
git -C "$TMP/checkout" rm -rq --cached .
git -C "$TMP/checkout" -c user.name=Test -c user.email=test@example.invalid commit -qm none --allow-empty
NOENGINE=$(git -C "$TMP/checkout" rev-parse HEAD)
git -C "$TMP/checkout" checkout -qf main
expect_refusal "engine_missing: $NOENGINE holds no engine/" fixture install "$NOENGINE"
printf '#!/bin/sh\ncase "$*" in *cat-file*) exit 128 ;; esac\nexec /usr/bin/git "$@"\n' >"$TMP/tools/git"
chmod +x "$TMP/tools/git"
expect_refusal 'git_unavailable: commit lookup failed' env PATH="$TMP/tools:$PATH" ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" ONT_INSTALL_PREBUILT="$TMP/new-head" "$INSTALL" install "$COMMIT"
rm "$TMP/tools/git"

# Fault injection uses PATH wrappers only in the disposable fixture.
# spec: specs/ont/cli.md#R4
# A corrupted backup copy must be refused before replacement.
printf '#!/bin/sh\n/usr/bin/cp "$@" || exit $?\nprintf x >>"$2"\n' >"$TMP/tools/cp"
chmod +x "$TMP/tools/cp"
expect_refusal 'previous binary copy is missing or has the wrong sha256' env PATH="$TMP/tools:$PATH" ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" ONT_INSTALL_PREBUILT="$TMP/new-head" "$INSTALL" install "$COMMIT"
cmp "$TMP/new-head" "$BIN1/ont" || fail 'bad backup replaced binary'
rm "$TMP/tools/cp"
# spec: specs/ont/cli.md#R8
# Failed state rename after replacement prints exact recovery and cleans temp files.
printf '#!/bin/sh\ncase "$2" in */installed.json) exit 1 ;; esac\nexec /usr/bin/mv "$@"\n' >"$TMP/tools/mv"
chmod +x "$TMP/tools/mv"
make_standin "$TMP/last" last
set +e
env PATH="$TMP/tools:$PATH" ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" ONT_INSTALL_PREBUILT="$TMP/last" "$INSTALL" install "$COMMIT" >"$TMP/recovery" 2>&1
code=$?
set -e
[ "$code" -ne 0 ] || fail 'state write failure succeeded'
case $(cat "$TMP/recovery") in *'manual recovery commands:'*'install -m 0755 -- '*) ;; *) fail 'manual recovery omitted' ;; esac
[ -z "$(find "$BIN1" "$STATE1/install" -name '.ont.*' -o -name '.installed.json.*')" ] || fail 'stray temporary after state failure'
rm "$TMP/tools/mv"
sed -n '/^install -m 0755 -- /p' "$TMP/recovery" >"$TMP/manual-recovery"
bash "$TMP/manual-recovery"
fixture check >/dev/null || fail 'printed recovery commands did not restore check'
# rollback_state_write_failure_prints_recovery: retry the printed command.
run_test_install "$TMP/last"
cp "$STATE1/install/installed.json" "$TMP/rollback-state-before"
printf '#!/bin/sh\ncase "$2" in */installed.json) exit 1 ;; esac\nexec /usr/bin/mv "$@"\n' >"$TMP/tools/mv"
chmod +x "$TMP/tools/mv"
set +e
env PATH="$TMP/tools:$PATH" ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" "$INSTALL" rollback >"$TMP/rollback-recovery" 2>&1
code=$?
set -e
[ "$code" -ne 0 ] || fail 'rollback state failure succeeded'
case $(cat "$TMP/rollback-recovery") in *'installed.json write failed; manual recovery commands:'*' rollback'*) ;; *) fail 'rollback manual recovery omitted' ;; esac
cmp "$TMP/rollback-state-before" "$STATE1/install/installed.json" || fail 'failed rollback state write changed recorded state'
rm "$TMP/tools/mv"
sed -n '/ rollback$/p' "$TMP/rollback-recovery" >"$TMP/manual-rollback-recovery"
ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" bash "$TMP/manual-rollback-recovery" >/dev/null
fixture check >/dev/null || fail 'printed rollback recovery command did not repair state'

# A failed install into the temporary binary is cleaned up too.
printf '#!/bin/sh\nexit 1\n' >"$TMP/tools/install"
chmod +x "$TMP/tools/install"
set +e
env PATH="$TMP/tools:$PATH" ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" ONT_INSTALL_PREBUILT="$TMP/last" "$INSTALL" install "$COMMIT" >/dev/null 2>&1
code=$?
set -e
[ "$code" -ne 0 ] || fail 'temporary install failure succeeded'
[ -z "$(find "$BIN1" -name '.ont.*')" ] || fail 'stray failed-install temporary'
rm "$TMP/tools/install"
# spec: specs/ont/cli.md#R3
# Refuse before cargo, and record that df received the mktemp build path.
printf '#!/bin/sh\nprintf "%%s\\n" "$3" >"$ONT_DF_LOG"\nprintf "Avail\\n${ONT_TEST_AVAIL:-9}G\\n"\n' >"$TMP/tools/df"
chmod +x "$TMP/tools/df"
# The fixture root is trusted so the case holds wherever $TMP lies.
expect_refusal 'free space on the build file system is under 10 GiB' env PATH="$TMP/tools:$PATH" ONT_DF_LOG="$TMP/df-path" TMPDIR="$TMP" ONT_INSTALL_TEST=1 ONT_INSTALL_TEST_TRUST="$TMP" ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" "$INSTALL" install "$COMMIT"
case $(cat "$TMP/df-path") in "$STATE1"/build/install.*) ;; *) fail 'df did not inspect the new directory under the build base' ;; esac

# At exactly 10 GiB, the guard passes and the disposable build reaches cargo.
printf '#!/bin/sh\nprintf "%%s %%s\\n" "$PWD" "$ONT_TREE_ID" >"$ONT_CARGO_LOG"\nexit 77\n' >"$TMP/tools/cargo"
# The Cargo stub does no build; CI hosts need no real host-specific lock path.
# The stub grants the build base lock (flock -n <fd>) and runs the command
# under the Cargo lock.
printf '#!/bin/sh\n[ "$1" != -n ] || exit 0\nshift\nexec "$@"\n' >"$TMP/tools/flock"
chmod +x "$TMP/tools/cargo" "$TMP/tools/flock"
set +e
env PATH="$TMP/tools:$PATH" ONT_CARGO_LOG="$TMP/cargo-log" ONT_TEST_AVAIL=10 ONT_DF_LOG="$TMP/df-path" TMPDIR="$TMP" ONT_INSTALL_TEST=1 ONT_INSTALL_TEST_TRUST="$TMP" ONT_INSTALL_BIN_DIR="$BIN1" ONT_INSTALL_STATE_DIR="$STATE1" "$INSTALL" install "$COMMIT" >"$TMP/ten-gib" 2>&1
code=$?
set -e
[ "$code" -eq 77 ] || fail "10 GiB did not reach the stand-in cargo (status $code)"
# Cargo ran in engine/ of the archive of the commit, with the tree id in ONT_TREE_ID.
case $(cat "$TMP/cargo-log") in "$STATE1"/build/install.*/src/engine\ "$GOOD") ;; *) fail "cargo did not build engine/ of the archive with ONT_TREE_ID: $(cat "$TMP/cargo-log")" ;; esac

# spec: specs/ont/cli.md#R14
# The build base: every existing component of its path is checked without
# following a link, a missing base is made 0700, one install owns it at a
# time and removes what a killed install left. The df stub refuses before
# Cargo; flock is the real one.
R14=$TMP/r14
mkdir -m 0700 "$R14" "$R14/tools"
printf '#!/bin/sh\nprintf "Avail\\n9G\\n"\n' >"$R14/tools/df"
chmod +x "$R14/tools/df"
r14_install() {
  # $1 = state directory; the fixture root is trusted.
  env PATH="$R14/tools:$PATH" ONT_INSTALL_TEST=1 ONT_INSTALL_TEST_TRUST="$TMP" ONT_INSTALL_BIN_DIR="$R14/bin" ONT_INSTALL_STATE_DIR="$1" "$INSTALL" install "$COMMIT"
}
expect_line() {
  # $1 = the exact line the last refusal must hold.
  grep -qxF -- "$1" "$TMP/refusal" || fail "missing refusal line: $1"
}
# A base under a directory any account can write.
mkdir -m 0700 "$R14/open"
chmod 0777 "$R14/open"
expect_refusal 'build_dir_unsafe' r14_install "$R14/open/s"
expect_line "refused: build_dir_unsafe: $R14/open"
[ ! -e "$R14/open/s" ] || fail 'created a base under an unsafe directory'
# A base whose path holds a symbolic link.
mkdir -m 0700 "$R14/real"
ln -s "$R14/real" "$R14/link"
expect_refusal 'build_dir_unsafe' r14_install "$R14/link/s"
expect_line "refused: build_dir_unsafe: $R14/link"
[ ! -e "$R14/real/s" ] || fail 'created a base through a link'
# Nothing beyond a link is read: an unsafe directory reached only through
# the link does not change the named component.
mkdir -m 0700 "$R14/behind"
mkdir -m 0700 "$R14/behind/s"
chmod 0777 "$R14/behind/s"
ln -s "$R14/behind" "$R14/link2"
expect_refusal 'build_dir_unsafe' r14_install "$R14/link2/s/z"
expect_line "refused: build_dir_unsafe: $R14/link2"
# A base that is itself a link, written with a trailing slash.
mkdir -m 0700 "$R14/s2" "$R14/elsewhere"
ln -s "$R14/elsewhere" "$R14/s2/build"
expect_refusal 'build_dir_unsafe' r14_install "$R14/s2/"
expect_line "refused: build_dir_unsafe: $R14/s2/build"
# A created base and its missing parents are 0700.
expect_refusal 'free space on the build file system is under 10 GiB' r14_install "$R14/new/p/s"
for d in "$R14/new" "$R14/new/p" "$R14/new/p/s" "$R14/new/p/s/build"; do
  [ "$(stat -c %a "$d")" = 700 ] || fail "created $d is not 0700"
done
[ -z "$(find "$R14/new/p/s/build" -mindepth 1 ! -name .lock)" ] || fail 'build directory was not cleaned up'
# A second install while the lock is held.
exec 8>>"$R14/new/p/s/build/.lock"
flock -n 8 || fail 'could not take the build base lock'
expect_refusal 'install_busy' r14_install "$R14/new/p/s" 8>&-
expect_line 'refused: install_busy'
exec 8>&-
# Leftovers of a killed install are removed by the next one; a link is
# removed as a link and its target is untouched.
mkdir -m 0700 "$R14/new/p/s/build/install.left" "$R14/outside"
printf 'x\n' >"$R14/new/p/s/build/install.left/file"
printf 'keep\n' >"$R14/outside/file"
ln -s "$R14/outside" "$R14/new/p/s/build/install.link"
expect_refusal 'free space on the build file system is under 10 GiB' r14_install "$R14/new/p/s"
[ ! -e "$R14/new/p/s/build/install.left" ] || fail 'leftover build directory was not removed'
[ ! -L "$R14/new/p/s/build/install.link" ] || fail 'leftover link was not removed'
[ "$(cat "$R14/outside/file")" = keep ] || fail 'removal followed a link out of the base'
[ -f "$R14/new/p/s/build/.lock" ] || fail 'the lock file was removed'
# The trust hook needs the test flag and must lie above the base.
expect_refusal 'ONT_INSTALL_TEST_TRUST require ONT_INSTALL_TEST=1' env ONT_INSTALL_TEST_TRUST="$TMP" "$INSTALL" check
expect_refusal 'ONT_INSTALL_TEST_TRUST must be a directory above the build base' env PATH="$R14/tools:$PATH" ONT_INSTALL_TEST=1 ONT_INSTALL_TEST_TRUST="$R14/elsewhere" ONT_INSTALL_BIN_DIR="$R14/bin" ONT_INSTALL_STATE_DIR="$R14/new/p/s" "$INSTALL" install "$COMMIT"
# A relative base is refused.
expect_refusal 'build_dir_unsafe' env PATH="$R14/tools:$PATH" ONT_INSTALL_TEST=1 ONT_INSTALL_BIN_DIR="$R14/bin" ONT_INSTALL_STATE_DIR=relative-state "$INSTALL" install "$COMMIT"
expect_line 'refused: build_dir_unsafe: relative-state/build'
printf 'test-install-ont: PASS\n'
