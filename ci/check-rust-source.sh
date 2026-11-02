#!/usr/bin/env bash
# Ordinary source checks. Run only in a disposable CI runner or the Lead's granted build slot.
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
export CARGO_BUILD_JOBS=1
export RUSTUP_TOOLCHAIN=1.95.0

cargo fmt --all -- --check
cargo test --locked -j1 -p ap
cargo clippy --locked -j1 -p ap --all-targets -- -D warnings
cargo build --locked -j1 -p ap

# Existing independent judges and negative controls; no expected outputs are regenerated.
python3 -I -B crates/ap/conformance/verify.py --self-check
python3 -I -B crates/ap/s03/conformance/verify-v2.py --self-check
python3 -I -B crates/ap/s03/conformance/verify-commands-v1.py --self-check
binary="${CARGO_TARGET_DIR:-target}/debug/ap"
python3 -I -B crates/ap/conformance/verify.py --binary "$binary"
# mktemp creates a private 0700 parent; all install/recovery effects stay in synthetic fixtures.
scratch=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/ap-conformance.XXXXXXXX")
trap 'rmdir -- "$scratch"' EXIT
python3 -I -B crates/ap/s03/conformance/verify-v2.py --binary "$binary" --scratch-parent "$scratch"
python3 -I -B crates/ap/s03/conformance/verify-commands-v1.py --binary "$binary"
