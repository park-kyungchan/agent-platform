#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_SCRIPT="$SCRIPT_DIR/install-ap.sh"
TMP_ROOT="$(mktemp -d)"
trap 'rm -rf "$TMP_ROOT"' EXIT
FAILURES=0

report() {
  local status="$1"
  local name="$2"
  printf '%s %s\n' "$status" "$name"
  if [[ "$status" == "FAIL" ]]; then
    FAILURES=$((FAILURES + 1))
  fi
}

make_fake_ap() {
  local path="$1"
  local rev="$2"
  cat >"$path" <<EOF
#!/usr/bin/env bash
printf 'ap 0.1.0 %s\n' '$rev'
EOF
  chmod +x "$path"
}

# malformed commit
INSTALL_DIR1="$TMP_ROOT/install1"
mkdir -p "$INSTALL_DIR1"
if AP_INSTALL_DIR="$INSTALL_DIR1" CARGO_TARGET_DIR="$TMP_ROOT/target" bash "$INSTALL_SCRIPT" install not-a-commit >"$TMP_ROOT/out1" 2>"$TMP_ROOT/err1"; then
  report FAIL malformed_commit
else
  report ok malformed_commit
fi

# relative CARGO_TARGET_DIR
if AP_INSTALL_DIR="$INSTALL_DIR1" CARGO_TARGET_DIR="relative/target" bash "$INSTALL_SCRIPT" install 0000000000000000000000000000000000000000 >"$TMP_ROOT/out2" 2>"$TMP_ROOT/err2"; then
  report FAIL relative_cargo_target_dir
else
  report ok relative_cargo_target_dir
fi

# rollback wrong name
INSTALL_DIR2="$TMP_ROOT/install2"
mkdir -p "$INSTALL_DIR2"
make_fake_ap "$INSTALL_DIR2/ap" 1111111111111111111111111111111111111111
printf 'wrong\n' >"$INSTALL_DIR2/not-a-backup"
if AP_INSTALL_DIR="$INSTALL_DIR2" bash "$INSTALL_SCRIPT" rollback "$INSTALL_DIR2/not-a-backup" >"$TMP_ROOT/out3" 2>"$TMP_ROOT/err3"; then
  report FAIL rollback_wrong_name
else
  report ok rollback_wrong_name
fi

# rollback round trip
INSTALL_DIR3="$TMP_ROOT/install3"
mkdir -p "$INSTALL_DIR3"
make_fake_ap "$INSTALL_DIR3/ap.before-abcdef0" aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
make_fake_ap "$INSTALL_DIR3/ap" bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
BEFORE_SHA="$(sha256sum "$INSTALL_DIR3/ap.before-abcdef0" | awk '{print $1}')"
if AP_INSTALL_DIR="$INSTALL_DIR3" bash "$INSTALL_SCRIPT" rollback "$INSTALL_DIR3/ap.before-abcdef0" >"$TMP_ROOT/out4" 2>"$TMP_ROOT/err4"; then
  AFTER_SHA="$(sha256sum "$INSTALL_DIR3/ap" | awk '{print $1}')"
  if [[ "$BEFORE_SHA" == "$AFTER_SHA" ]]; then
    report ok rollback_round_trip
  else
    report FAIL rollback_round_trip
  fi
else
  report FAIL rollback_round_trip
fi

# check exits 0 and writes nothing
INSTALL_DIR4="$TMP_ROOT/install4"
mkdir -p "$INSTALL_DIR4"
make_fake_ap "$INSTALL_DIR4/ap" cccccccccccccccccccccccccccccccccccccccc
SNAP_BEFORE="$(find "$INSTALL_DIR4" -type f -exec sha256sum {} + | sort)"
if AP_INSTALL_DIR="$INSTALL_DIR4" bash "$INSTALL_SCRIPT" check >"$TMP_ROOT/out5" 2>"$TMP_ROOT/err5"; then
  SNAP_AFTER="$(find "$INSTALL_DIR4" -type f -exec sha256sum {} + | sort)"
  if [[ "$SNAP_BEFORE" == "$SNAP_AFTER" ]]; then
    report ok check_writes_nothing
  else
    report FAIL check_writes_nothing
  fi
else
  report FAIL check_writes_nothing
fi

if [[ "$FAILURES" -ne 0 ]]; then
  exit 1
fi
