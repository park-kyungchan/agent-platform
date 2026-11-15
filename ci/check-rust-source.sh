#!/usr/bin/env bash
# The frozen host pane oracle of `ap pane`, using only disposable fake proc/Herdr/TTY/SQLite inputs.
# Run only in a disposable CI runner or the Lead's granted build slot.
set -euo pipefail

cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
export CARGO_BUILD_JOBS=1
export RUSTUP_TOOLCHAIN=1.95.0

cargo build --locked -j1 -p ap --example pane_fixture
python3 -I -B crates/ap/pane/golden.py --binary "${CARGO_TARGET_DIR:-target}/debug/examples/pane_fixture"
