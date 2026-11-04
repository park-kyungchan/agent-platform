#!/usr/bin/env bash
# Ordinary source checks. Run only in a disposable CI runner or the Lead's granted build slot.
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
export CARGO_BUILD_JOBS=1
export RUSTUP_TOOLCHAIN=1.95.0

# Rust fault tests require an explicit private root with safe ancestors. The
# workspace is owned by this job; a caller may supply a separate safe parent.
test_scratch=$(mktemp -d "${AP_S03_TEST_ROOT:-$PWD}/ap-rust-test.XXXXXXXX")
export AP_S03_TEST_ROOT="$test_scratch"
matrix_scratch=""
matrix_prefix=()
cleanup() {
  local ci_status=$?
  if [[ -n "$matrix_scratch" ]]; then
    "${matrix_prefix[@]}" rmdir -- "$matrix_scratch" || ci_status=1
  fi
  rmdir -- "$test_scratch" || ci_status=1
  exit "$ci_status"
}
trap cleanup EXIT

cargo fmt --all -- --check
cargo test --locked -j1 -p ap
cargo clippy --locked -j1 -p ap --all-targets -- -D warnings
cargo build --locked -j1 -p ap

# Existing independent judges and negative controls; no expected outputs are regenerated.
python3 -I -B crates/ap/conformance/verify.py --self-check
python3 -I -B crates/ap/s03/conformance/verify-v2.py --self-check
python3 -I -B crates/ap/s03/conformance/verify-commands-v1.py --self-check
binary=$(realpath -- "${CARGO_TARGET_DIR:-target}/debug/ap")
python3 -I -B crates/ap/conformance/verify.py --binary "$binary"
# The frozen foreign-owner fixture calls chown. Keep its full 41-case matrix:
# hosted unprivileged jobs need noninteractive sudo for this disposable check.
# Root fixtures also need root-owned ancestors, so do not use /home/runner or
# /tmp for that elevated matrix. No native profile/configuration is selected.
if [[ $EUID -eq 0 ]]; then
  python3 -I -B crates/ap/s03/conformance/verify-v2.py --binary "$binary" --scratch-parent "$test_scratch"
else
  sudo -n -- true
  matrix_prefix=(sudo -n --)
  matrix_scratch=$(sudo -n -- mktemp -d /root/ap-conformance.XXXXXXXX)
  sudo -n -- python3 -I -B "$PWD/crates/ap/s03/conformance/verify-v2.py" --binary "$binary" --scratch-parent "$matrix_scratch"
fi
python3 -I -B crates/ap/s03/conformance/verify-commands-v1.py --binary "$binary"

# Frozen host pane oracle, using only disposable fake proc/Herdr/TTY/SQLite inputs.
cargo build --locked -j1 -p ap --example pane_fixture
python3 -I -B crates/ap/pane/golden.py --binary "$(dirname -- "$binary")/examples/pane_fixture"
