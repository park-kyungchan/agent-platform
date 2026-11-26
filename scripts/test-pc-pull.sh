#!/usr/bin/env bash
# Offline tests of scripts/pc-pull.sh in temporary directories (no network, no real $HOME/.local/bin).
# spec: specs/ap/deliver.md#R64
# spec: specs/ap/deliver.md#R70
set -uo pipefail

script=${1:-$(cd "$(dirname "$0")" && pwd -P)/pc-pull.sh}
t=$(mktemp -d)
trap 'rm -rf -- "$t"' EXIT

export HOME="$t/home"
mkdir -p "$HOME" "$t/ap-bare.git" "$t/work" "$t/bin" "$t/state"
git init --quiet --bare --initial-branch=main "$t/ap-bare.git"

# The repository: each commit holds engine/, whose Git tree id names its engine. The first commit also holds
# the file of the retired engine pin (named indirectly here), which the tree rule does not read.
old_pin="ENGINE""-PIN"
git clone --quiet "$t/ap-bare.git" "$t/work/ap"
git -C "$t/work/ap" config user.email t@example.invalid
git -C "$t/work/ap" config user.name t
mkdir -p "$t/work/ap/engine" "$t/work/ap/ontology"
printf '[workspace]\n# v1\n' > "$t/work/ap/engine/Cargo.toml"
printf '%s\n' 0123456789abcdef0123456789abcdef01234567 > "$t/work/ap/ontology/$old_pin"
git -C "$t/work/ap" add -A
git -C "$t/work/ap" commit --quiet -m "ap 1"
ap1=$(git -C "$t/work/ap" rev-parse HEAD)
id1=$(git -C "$t/work/ap" rev-parse HEAD:engine)
git -C "$t/work/ap" push --quiet origin main

git -C "$t/work/ap" rm --quiet "ontology/$old_pin"
printf '[workspace]\n# v2\n' > "$t/work/ap/engine/Cargo.toml"
git -C "$t/work/ap" add -A
git -C "$t/work/ap" commit --quiet -m "ap 2"
ap2=$(git -C "$t/work/ap" rev-parse HEAD)
id2=$(git -C "$t/work/ap" rev-parse HEAD:engine)
git -C "$t/work/ap" push --quiet origin main

# An unmerged ap commit on a side branch (not on origin/main).
git -C "$t/work/ap" checkout --quiet -b side
echo "side" > "$t/work/ap/side.txt"
git -C "$t/work/ap" add side.txt
git -C "$t/work/ap" commit --quiet -m "ap side"
ap_off_main=$(git -C "$t/work/ap" rev-parse HEAD)
git -C "$t/work/ap" push --quiet origin side

# A repository whose commit holds no engine/.
git init --quiet --bare --initial-branch=main "$t/ap-neither.git"
git clone --quiet "$t/ap-neither.git" "$t/work/ap-neither"
git -C "$t/work/ap-neither" config user.email t@example.invalid
git -C "$t/work/ap-neither" config user.name t
echo "neither" > "$t/work/ap-neither/README"
git -C "$t/work/ap-neither" add README
git -C "$t/work/ap-neither" commit --quiet -m "neither"
git -C "$t/work/ap-neither" push --quiet origin main

# Fake build stub: for the engine, verifies the exported engine/ and ONT_TREE_ID; for ap, verifies the
# detached worktree HEAD and ONT_BIN,
# and emits executable shell scripts whose --version names the commit.
cat > "$t/fake-build.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
kind="$1"
wt="$2"
out="$3"
rev="$4"
if [[ "$kind" == "ont" ]]; then
  if [[ "${ONT_TREE_ID:-}" != "$rev" || ! -f "$wt/Cargo.toml" || "$(basename "$wt")" != engine ]]; then
    echo "engine of a tree: ONT_TREE_ID=$ONT_TREE_ID rev=$rev wt=$wt" >&2
    exit 1
  fi
  printf '%s\n' "$ONT_TREE_ID" > "$FAKE_TREE_RECORD"
else
head_rev=$(git -C "$wt" rev-parse HEAD)
if [[ "$head_rev" != "$rev" ]]; then
  echo "worktree HEAD $head_rev != $rev" >&2
  exit 1
fi
fi
mkdir -p "$(dirname "$out")"
if [[ "$kind" == "ont" ]]; then
  cat > "$out" <<SCRIPT
#!/usr/bin/env bash
if [[ "\${1:-}" == "--version" ]]; then
  printf 'ont 0.1.0 %s\n' '$rev'
  exit 0
fi
exit 0
SCRIPT
  chmod 0755 "$out"
  printf '%s\n' "$out" > "$FAKE_ONT_RECORD"
elif [[ "$kind" == "ap" ]]; then
  if [[ -z "${ONT_BIN:-}" || ! -x "$ONT_BIN" ]]; then
    echo "ONT_BIN missing or not executable: ${ONT_BIN:-}" >&2
    exit 1
  fi
  expected_ont=$(cat "$FAKE_ONT_RECORD")
  if [[ "$ONT_BIN" != "$expected_ont" && "${FAKE_ONT_ANY:-}" != 1 ]]; then
    echo "ONT_BIN=$ONT_BIN != expected $expected_ont" >&2
    exit 1
  fi
  ont_ver=$("$ONT_BIN" --version)
  printf '%s|%s\n' "$ONT_BIN" "$ont_ver" > "$FAKE_AP_RECORD"
  fail_path="${FAKE_FAIL_READBACK_PATH:-}"
  cat > "$out" <<SCRIPT
#!/usr/bin/env bash
if [[ -n '$fail_path' && "\$0" == '$fail_path' ]]; then
  exit 1
fi
if [[ "\${1:-}" == "--version" ]]; then
  printf 'ap 0.1.0 %s\n' '$rev'
  exit 0
fi
exit 0
SCRIPT
  chmod 0755 "$out"
else
  echo "unknown kind $kind" >&2
  exit 1
fi
EOF
chmod 0755 "$t/fake-build.sh"

export AP_PULL_BUILD_CMD="$t/fake-build.sh"
export FAKE_ONT_RECORD="$t/ont-built-path"
export FAKE_AP_RECORD="$t/ap-saw-ont"
export FAKE_TREE_RECORD="$t/tree-built"

fails=0
ok() { echo "ok   $1"; }
bad() { echo "FAIL $1"; fails=$((fails + 1)); }
sha_of() { sha256sum -- "$1" | awk '{print $1}'; }

run_pull() {
  bash "$script" "$@" > "$t/out" 2> "$t/err"
}

# 1. Source build mode at explicit ancestor commit ap1: the engine is built from engine/ of ap1 with its tree
#    id, the file of the retired pin at ap1 is not read, and no other repository is fetched.
run_pull --ap-url "$t/ap-bare.git" --commit "$ap1" --bin-dir "$t/bin" --state-dir "$t/state"
rc=$?
if [[ "$rc" -eq 0 ]] \
  && [[ "$("$t/bin/ont" --version)" == "ont 0.1.0 $id1" ]] \
  && [[ "$("$t/bin/ap" --version)" == "ap 0.1.0 $ap1" ]] \
  && [[ "$(cat "$FAKE_TREE_RECORD")" == "$id1" ]] \
  && grep -q "|ont 0.1.0 $id1$" "$FAKE_AP_RECORD" \
  && [[ "$(ls "$t/state")" == $'agent-platform\ntarget-ap\ntarget-ont' ]] \
  && python3 -I -B -c 'import json,sys; d=json.load(open(sys.argv[1])); assert d["status"]=="installed" and d["mode"]=="build" and d["commit"]==sys.argv[2] and d["pin"]==sys.argv[3] and d["ap_sha256"]==sys.argv[4] and d["ont_sha256"]==sys.argv[5]' \
     "$t/out" "$ap1" "$id1" "$(sha_of "$t/bin/ap")" "$(sha_of "$t/bin/ont")"; then
  ok "source build at ancestor commit builds engine/ with its tree id, passes ONT_BIN and installs both binaries"
else
  bad "source build ap1 rc=$rc out=$(cat "$t/out") err=$(cat "$t/err")"
fi

ap1_sha=$(sha_of "$t/bin/ap")
ont1_sha=$(sha_of "$t/bin/ont")

# 2. Source build mode at default origin/main tip (ap2, id2) keeps .before-<sha12> backups.
run_pull --ap-url "$t/ap-bare.git" --bin-dir "$t/bin" --state-dir "$t/state"
rc=$?
if [[ "$rc" -eq 0 ]] \
  && [[ "$("$t/bin/ont" --version)" == "ont 0.1.0 $id2" ]] \
  && [[ "$("$t/bin/ap" --version)" == "ap 0.1.0 $ap2" ]] \
  && [[ "$(sha_of "$t/bin/ap.before-${ap1_sha:0:12}")" == "$ap1_sha" ]] \
  && [[ "$(sha_of "$t/bin/ont.before-${ont1_sha:0:12}")" == "$ont1_sha" ]]; then
  ok "second source build updates both binaries and keeps .before-<sha12> backups"
else
  bad "second source build rc=$rc out=$(cat "$t/out") err=$(cat "$t/err")"
fi

# 3. An installed ont that already names the tree id is not built again.
rm -f -- "$FAKE_TREE_RECORD"
FAKE_ONT_ANY=1 run_pull --ap-url "$t/ap-bare.git" --bin-dir "$t/bin" --state-dir "$t/state"
rc=$?
if [[ "$rc" -eq 0 ]] && [[ ! -e "$FAKE_TREE_RECORD" ]] \
  && [[ "$("$t/bin/ont" --version)" == "ont 0.1.0 $id2" ]] \
  && grep -q "|ont 0.1.0 $id2$" "$FAKE_AP_RECORD"; then
  ok "an engine that names the tree id is kept without a build"
else
  bad "engine skip rc=$rc out=$(cat "$t/out") err=$(cat "$t/err")"
fi

ap2_sha=$(sha_of "$t/bin/ap")
ont2_sha=$(sha_of "$t/bin/ont")

# 4. Commit off origin/main refused not_on_main (exit 2), existing binaries untouched.
run_pull --ap-url "$t/ap-bare.git" --commit "$ap_off_main" --bin-dir "$t/bin" --state-dir "$t/state"
rc=$?
if [[ "$rc" -eq 2 ]] && grep -q '^not_on_main@pc-pull\.sh:' "$t/err" \
  && [[ "$(sha_of "$t/bin/ap")" == "$ap2_sha" ]] && [[ "$(sha_of "$t/bin/ont")" == "$ont2_sha" ]]; then
  ok "commit off origin/main refused not_on_main without modifying binaries"
else
  bad "commit off origin/main rc=$rc err=$(cat "$t/err")"
fi

# 4b. The option of the retired engine repository is an unknown argument.
run_pull --ap-url "$t/ap-bare.git" --ont-url "$t/elsewhere.git" --bin-dir "$t/bin" --state-dir "$t/state"
rc=$?
if [[ "$rc" -eq 2 ]] && grep -q '^invalid_args@pc-pull\.sh:[0-9]*: unknown argument: --ont-url' "$t/err" \
  && [[ "$(sha_of "$t/bin/ap")" == "$ap2_sha" ]]; then
  ok "--ont-url is refused invalid_args"
else
  bad "--ont-url rc=$rc err=$(cat "$t/err")"
fi

# 5. Digest-verified pair mode (--from-dir) with matching digests installs cleanly.
mkdir -p "$t/pair"
printf '#!/usr/bin/env bash\necho "ap 0.1.0 %s"\n' "$ap1" > "$t/pair/ap"
printf '#!/usr/bin/env bash\necho "ont 0.1.0 %s"\n' "$id1" > "$t/pair/ont"
chmod 0755 "$t/pair/ap" "$t/pair/ont"
pair_ap_sha=$(sha_of "$t/pair/ap")
pair_ont_sha=$(sha_of "$t/pair/ont")

run_pull --from-dir "$t/pair" --ap-sha256 "$pair_ap_sha" --ont-sha256 "$pair_ont_sha" --bin-dir "$t/bin"
rc=$?
if [[ "$rc" -eq 0 ]] \
  && [[ "$(sha_of "$t/bin/ap")" == "$pair_ap_sha" ]] \
  && [[ "$(sha_of "$t/bin/ont")" == "$pair_ont_sha" ]] \
  && python3 -I -B -c 'import json,sys; d=json.load(open(sys.argv[1])); assert d["status"]=="installed" and d["mode"]=="pair" and d["ap_sha256"]==sys.argv[2] and d["ont_sha256"]==sys.argv[3] and "commit" not in d' \
     "$t/out" "$pair_ap_sha" "$pair_ont_sha"; then
  ok "--from-dir with matching digests installs cleanly"
else
  bad "--from-dir matching rc=$rc out=$(cat "$t/out") err=$(cat "$t/err")"
fi

# 6. Tampered ap or ont in --from-dir refused digest_mismatch (exit 2) before any write.
mkdir -p "$t/pair-bad-ap" "$t/pair-bad-ont"
cp "$t/pair/ap" "$t/pair-bad-ap/ap"
cp "$t/pair/ont" "$t/pair-bad-ap/ont"
echo "# tampered" >> "$t/pair-bad-ap/ap"

run_pull --from-dir "$t/pair-bad-ap" --ap-sha256 "$pair_ap_sha" --ont-sha256 "$pair_ont_sha" --bin-dir "$t/bin"
rc1=$?
err1=$(cat "$t/err")

cp "$t/pair/ap" "$t/pair-bad-ont/ap"
cp "$t/pair/ont" "$t/pair-bad-ont/ont"
echo "# tampered" >> "$t/pair-bad-ont/ont"

run_pull --from-dir "$t/pair-bad-ont" --ap-sha256 "$pair_ap_sha" --ont-sha256 "$pair_ont_sha" --bin-dir "$t/bin"
rc2=$?
err2=$(cat "$t/err")

if [[ "$rc1" -eq 2 && "$rc2" -eq 2 ]] \
  && [[ "$err1" == digest_mismatch@pc-pull.sh:* ]] \
  && [[ "$err2" == digest_mismatch@pc-pull.sh:* ]] \
  && [[ "$(sha_of "$t/bin/ap")" == "$pair_ap_sha" ]] \
  && [[ "$(sha_of "$t/bin/ont")" == "$pair_ont_sha" ]]; then
  ok "--from-dir tampered ap or ont refused digest_mismatch leaving existing binaries byte-identical"
else
  bad "--from-dir tampered rc1=$rc1 rc2=$rc2 err1=$err1 err2=$err2"
fi

# 7. Failed post-install readback restores BOTH .before-<sha12> backups.
FAKE_FAIL_READBACK_PATH="$t/bin/ap" run_pull --ap-url "$t/ap-bare.git" \
  --commit "$ap2" --bin-dir "$t/bin" --state-dir "$t/state"
rc=$?
if [[ "$rc" -eq 1 ]] && grep -q '^readback_failed_restored@pc-pull\.sh:' "$t/err" \
  && [[ "$(sha_of "$t/bin/ap")" == "$pair_ap_sha" ]] \
  && [[ "$(sha_of "$t/bin/ont")" == "$pair_ont_sha" ]] \
  && [[ -f "$t/bin/ap.before-${pair_ap_sha:0:12}" ]] \
  && [[ -f "$t/bin/ont.before-${pair_ont_sha:0:12}" ]]; then
  ok "failed post-install readback restores both binaries from .before-* backups"
else
  bad "failed readback rollback rc=$rc err=$(cat "$t/err")"
fi

# 8. A commit without engine/ is refused engine_missing (exit 2), nothing written.
pair_ap_now=$(sha_of "$t/bin/ap")
run_pull --ap-url "$t/ap-neither.git" --bin-dir "$t/bin" --state-dir "$t/state-neither"
rc=$?
if [[ "$rc" -eq 2 ]] && grep -q '^engine_missing@pc-pull\.sh:[0-9]*: .*holds no engine/' "$t/err" \
  && [[ "$(sha_of "$t/bin/ap")" == "$pair_ap_now" ]]; then
  ok "a commit without engine/ is refused engine_missing"
else
  bad "no engine rc=$rc err=$(cat "$t/err")"
fi

if [[ "$fails" -eq 0 ]]; then
  echo "test-pc-pull: PASS"
else
  echo "test-pc-pull: $fails FAILED"
  exit 1
fi
