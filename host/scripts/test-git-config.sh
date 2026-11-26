#!/usr/bin/env bash
# Tests of git/: the includeIf matching, the roster check (git/hooks/pre-commit)
# and the alias `git task-new`. Everything runs in scratch directories from
# mktemp -d, with a synthetic roster (made-up names and ids) and Git's own
# configuration matching; no network, no root, no host path.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)

# Nothing of the caller's Git environment reaches the scratch repositories.
while IFS= read -r name; do
  unset "$name"
done < <(env | sed -n 's/^\(GIT_[A-Za-z0-9_]*\)=.*/\1/p')

scratch=$(mktemp -d)
scratch=$(cd -P "$scratch" && pwd)
trap 'rm -rf -- "$scratch"' EXIT

failures=0
checks=0

# pass_if <description> <command...>: the command must succeed.
pass_if() {
  local description=$1
  shift
  checks=$((checks + 1))
  if ! "$@"; then
    printf 'test-git-config: FAIL %s\n' "$description" >&2
    failures=$((failures + 1))
  fi
}
not() { ! "$@"; }
equals() { [[ "$1" == "$2" ]]; }

# --- the scratch configuration -------------------------------------------------
mkdir -p "$scratch/lib/hooks" "$scratch/roster" "$scratch/work" "$scratch/platforms" \
  "$scratch/other" "$scratch/elsewhere" "$scratch/home" "$scratch/xdg"

rewrite() {
  sed -e "s|/usr/local/lib/agents-git|$scratch/lib|g" \
      -e "s|/home/palantirkc/work/|$scratch/work/|g" \
      -e "s|/home/palantirkc/platforms/|$scratch/platforms/|g" \
      -e "s|/root/.local/share/academy-capture-20261113/inputs/|$scratch/roster/|g" \
      "$1" >"$2"
}
for file in work clones alias system; do
  rewrite "$repo_root/git/$file.gitconfig" "$scratch/lib/$file.gitconfig"
done
cp "$repo_root/git/hooks/pre-commit" "$scratch/lib/hooks/pre-commit"
chmod 0755 "$scratch/lib/hooks/pre-commit"

cat >"$scratch/global.gitconfig" <<CFG
[user]
	name = Test Agent
	email = test-agent@example.invalid
[init]
	defaultBranch = main
[protocol "file"]
	allow = always
[include]
	path = $scratch/lib/system.gitconfig
CFG
export GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_GLOBAL="$scratch/global.gitconfig"
export HOME="$scratch/home"
export XDG_CONFIG_HOME="$scratch/xdg"
export GIT_TERMINAL_PROMPT=0

# The synthetic roster: made-up names and ids, one non-ASCII, one numeric.
cat >"$scratch/roster/students-a.json" <<'JSON'
[
  {"student_name": "Zelda Quillfeather", "student_id": "QX7741", "grade": 3},
  {"student_name": "Ünal Bergström", "student_id": 90817},
  {"student_name": "Marta Vessel", "student_id": "MV-3302", "note": "not a term"}
]
JSON
printf '%s\n' 'Zelda Quillfeather' 'QX7741' 'Ünal Bergström' '90817' 'Marta Vessel' 'MV-3302' \
  >"$scratch/leakterms"

# --- helpers ------------------------------------------------------------------
rc=0
runner=(git)
# commit_try <repo> [git options...]: commits the staged changes; the output is
# in $scratch/out and the exit status in $rc.
commit_try() {
  local repo=$1
  shift
  rc=0
  "${runner[@]}" -C "$repo" "$@" commit -q -m probe >"$scratch/out" 2>&1 || rc=$?
}
leak_free() { ! grep -aqF -f "$scratch/leakterms" "$scratch/out"; }
out_lines() { grep -c '' "$scratch/out" || true; }
out_has() { grep -aqE -- "$1" "$scratch/out"; }

make_repo() {
  mkdir -p "$1"
  git init -q -b main "$1"
  git -C "$1" commit -q --allow-empty -m seed
}
case_number=0
new_ws() {
  case_number=$((case_number + 1))
  ws="$scratch/work/ws-case$case_number"
  make_repo "$ws"
}
stage() { # repo file content
  printf '%s\n' "$3" >"$1/$2"
  git -C "$1" add -- "$2"
}
refused_with() { # description, expected line pattern, expected number of lines
  pass_if "$1: exit status" equals "$rc" 1
  pass_if "$1: output" out_has "$2"
  pass_if "$1: line count" equals "$(out_lines)" "$3"
  pass_if "$1: no term in the output" leak_free
}

# --- 1. matching: Git's own includeIf ------------------------------------------
new_ws
pass_if "ws repository: hooksPath" equals "$(git -C "$ws" config --get core.hooksPath)" "$scratch/lib/hooks"
pass_if "ws repository: push.default" equals "$(git -C "$ws" config --get push.default)" upstream
pass_if "ws repository: scope is the system file's include" \
  equals "$(git -C "$ws" config --show-origin --get core.hooksPath | cut -f1)" "file:$scratch/lib/work.gitconfig"
stage "$ws" note.txt 'a line with Zelda Quillfeather in it'
commit_try "$ws"
refused_with "ws repository holds a term" '^private_term_staged@note.txt: 1 added lines' 1

for task_dir in ap-demo ont-demo mem-demo; do
  make_repo "$scratch/work/$task_dir"
  pass_if "$task_dir: hooksPath" equals "$(git -C "$scratch/work/$task_dir" config --get core.hooksPath)" "$scratch/lib/hooks"
done

clone="$scratch/platforms/ontology-platform"
make_repo "$clone"
pass_if "clone main checkout: hooksPath" equals "$(git -C "$clone" config --get core.hooksPath)" "$scratch/lib/hooks"
pass_if "clone main checkout: roster" \
  equals "$(git -C "$clone" config --get agents.roster)" "$scratch/roster/students-*.json"
pass_if "clone main checkout: no push.default" equals "$(git -C "$clone" config --get push.default || true)" ""
stage "$clone" note.txt 'QX7741'
commit_try "$clone"
refused_with "clone main checkout holds a term" '^private_term_staged@note.txt: 1 added lines' 1
git -C "$clone" reset -q
rm -f "$clone/note.txt"

git -C "$clone" worktree add -q -b linked "$scratch/elsewhere/linked-worktree"
linked="$scratch/elsewhere/linked-worktree"
pass_if "linked worktree: hooksPath" equals "$(git -C "$linked" config --get core.hooksPath)" "$scratch/lib/hooks"
pass_if "linked worktree: no push.default" equals "$(git -C "$linked" config --get push.default || true)" ""
stage "$linked" note.txt 'Marta Vessel'
commit_try "$linked"
refused_with "linked worktree holds a term" '^private_term_staged@note.txt: 1 added lines' 1

make_repo "$scratch/platforms/agent-vps-operations"
pass_if "second named clone: hooksPath" \
  equals "$(git -C "$scratch/platforms/agent-vps-operations" config --get core.hooksPath)" "$scratch/lib/hooks"

for unmatched in "$scratch/other/plain" "$scratch/platforms/agent-platform" \
    "$scratch/platforms/education-platform" "$scratch/work/wsx-demo" "$scratch/work/other-demo"; do
  make_repo "$unmatched"
  pass_if "unmatched $(basename "$unmatched"): no hooksPath" equals "$(git -C "$unmatched" config --get core.hooksPath || true)" ""
  pass_if "unmatched $(basename "$unmatched"): no roster" equals "$(git -C "$unmatched" config --get agents.roster || true)" ""
  pass_if "unmatched $(basename "$unmatched"): no push.default" equals "$(git -C "$unmatched" config --get push.default || true)" ""
  pass_if "unmatched $(basename "$unmatched"): the alias is there" \
    not equals "$(git -C "$unmatched" config --get alias.task-new)" ""
  stage "$unmatched" note.txt 'Zelda Quillfeather'
  commit_try "$unmatched"
  pass_if "unmatched $(basename "$unmatched"): the commit passes" equals "$rc" 0
done

# The system file names no excluded repository, and the clones' lines carry no ".git".
system_file="$repo_root/git/system.gitconfig"
pass_if "system file: no excluded name" \
  not grep -qE 'agent-platform|education-platform|education-dc-gang-a|/work/edu-' "$system_file"
pass_if "system file: six gitdir lines" equals "$(grep -c '^\[includeIf "gitdir:' "$system_file")" 6
pass_if "system file: no .git in a gitdir line" not grep -q '^\[includeIf.*\.git' "$system_file"
pass_if "hook is executable" test -x "$repo_root/git/hooks/pre-commit"
pass_if "work file has push.default upstream" \
  equals "$(git config --file "$repo_root/git/work.gitconfig" --get push.default)" upstream
pass_if "clones file has no push.default" \
  equals "$(git config --file "$repo_root/git/clones.gitconfig" --get push.default || true)" ""

# --- 2. the check: refusals ----------------------------------------------------
new_ws
stage "$ws" clean.txt 'nothing private here'
commit_try "$ws"
pass_if "a clean commit passes" equals "$rc" 0

new_ws
stage "$ws" clean.txt 'a line with Zelda Quillfeather in it'
commit_try "$ws"
refused_with "a term in an added line" '^private_term_staged@clean.txt: 1 added lines hold a roster name or id$' 1

new_ws
stage "$ws" clean.txt $'Marta Vessel\nplain\nMV-3302 and QX7741'
commit_try "$ws"
refused_with "three added lines hold terms" '^private_term_staged@clean.txt: 2 added lines' 1

new_ws
stage "$ws" clean.txt 'the id is 90817'
commit_try "$ws"
refused_with "a numeric id in an added line" '^private_term_staged@clean.txt: 1 added lines' 1

new_ws
stage "$ws" clean.txt 'Ünal Bergström was here'
commit_try "$ws"
refused_with "a non-ASCII name in an added line" '^private_term_staged@clean.txt: 1 added lines' 1

new_ws
stage "$ws" 'Zelda Quillfeather.txt' 'clean content'
commit_try "$ws"
refused_with "a term in a file name, clean content" \
  '^private_term_staged: 1 staged paths hold a roster name or id$' 1
pass_if "a term in a file name: no path in the output" not out_has 'txt'

new_ws
stage "$ws" 'QX7741.txt' 'Zelda Quillfeather in the content too'
commit_try "$ws"
refused_with "a term in a file name, matching content" \
  '^private_term_staged: 1 staged paths hold a roster name or id$' 1
pass_if "a term in a file name, matching content: no path" not out_has 'txt'

new_ws
stage "$ws" 'Ünal Bergström.txt' 'clean content'
stage "$ws" 'second.txt' 'Marta Vessel'
commit_try "$ws"
refused_with "a non-ASCII name in a file name beside a hit" '^private_term_staged' 2
pass_if "a term-named path and a clean-named hit" out_has '^private_term_staged@second.txt: 1 added lines'
pass_if "a non-ASCII term in a file name: no path of it" not out_has 'Bergstr'

new_ws
stage "$ws" 'données-é.txt' 'Zelda Quillfeather'
commit_try "$ws"
refused_with "a file with a non-ASCII name" '^private_term_staged@données-é.txt: 1 added lines' 1

new_ws
printf '%s\n' one two three four five six seven eight nine ten >"$ws/old.txt"
git -C "$ws" add old.txt
commit_try "$ws"
pass_if "rename: the seed commit passes" equals "$rc" 0
git -C "$ws" mv old.txt new.txt
printf '%s\n' 'Marta Vessel' >>"$ws/new.txt"
git -C "$ws" add new.txt
commit_try "$ws"
refused_with "a rename that adds a term" '^private_term_staged@new.txt: 1 added lines' 1

new_ws
printf '%s\n' '*.dat -diff' '*.tcx diff=tcx' >"$ws/.gitattributes"
git -C "$ws" add .gitattributes
commit_try "$ws"
pass_if "-diff: the attributes commit passes" equals "$rc" 0
stage "$ws" 'x.dat' 'Zelda Quillfeather'
commit_try "$ws"
refused_with "a -diff file" '^private_term_staged@x.dat: 1 added lines' 1

new_ws
printf '%s\n' '*.tcx diff=tcx' >"$ws/.gitattributes"
git -C "$ws" add .gitattributes
commit_try "$ws"
git -C "$ws" config diff.tcx.textconv 'echo hidden #'
stage "$ws" 'x.tcx' 'Zelda Quillfeather'
commit_try "$ws"
refused_with "a textconv driver" '^private_term_staged@x.tcx: 1 added lines' 1

new_ws
git -C "$ws" config diff.external true
stage "$ws" 'x.txt' 'Zelda Quillfeather'
commit_try "$ws"
refused_with "an external diff driver" '^private_term_staged@x.txt: 1 added lines' 1

new_ws
stage "$ws" 'x.txt' 'Zelda Quillfeather'
commit_try "$ws" -c color.diff=always -c color.ui=always
refused_with "color.diff=always" '^private_term_staged@x.txt: 1 added lines' 1

new_ws
printf 'abc\0Zelda Quillfeather\ndef\n' >"$ws/blob.bin"
git -C "$ws" add blob.bin
commit_try "$ws"
refused_with "a file holding a NUL byte" '^private_term_staged@blob.bin: 1 added lines' 1

new_ws
ln -s target "$ws/link"
git -C "$ws" add link
commit_try "$ws"
pass_if "symlink: the seed commit passes" equals "$rc" 0
rm "$ws/link"
printf '%s\n' 'Zelda Quillfeather' >"$ws/link"
git -C "$ws" add link
commit_try "$ws"
refused_with "a symbolic link changed into a file" '^private_term_staged@link: 1 added lines' 1

new_ws
stage "$ws" 'x.txt' '++ Zelda Quillfeather'
commit_try "$ws"
refused_with "an added line beginning ++" '^private_term_staged@x.txt: 1 added lines' 1

new_ws
stage "$ws" 'x.txt' '++ b/harmless'
commit_try "$ws"
pass_if "an added line that looks like a header, clean" equals "$rc" 0

new_ws
stage "$ws" 'x.txt' 'Zelda Quillfeather'
runner=(env SHELLOPTS=xtrace git)
commit_try "$ws"
runner=(git)
pass_if "xtrace inherited: refused" equals "$rc" 1
pass_if "xtrace inherited: no term in the output" leak_free

new_ws
stage "$ws" 'x.txt' 'Zelda Quillfeather'
git -C "$ws" commit -q -m probe --no-verify >/dev/null 2>&1 || true
pass_if "(control) --no-verify skips the check, as the README says" \
  equals "$(git -C "$ws" rev-list --count HEAD)" 2

# --- 3. the check: the roster cannot be read -----------------------------------
roster_case() { # name, glob (or the word ABSENT), then a staged term or clean text
  local name=$1 glob=$2 content=$3
  if [[ "$glob" == ABSENT ]]; then
    case_number=$((case_number + 1))
    ws="$scratch/other/absent-$case_number"
    make_repo "$ws"
    git -C "$ws" config core.hooksPath "$scratch/lib/hooks"
    stage "$ws" x.txt "$content"
    commit_try "$ws"
  else
    new_ws
    stage "$ws" x.txt "$content"
    commit_try "$ws" -c "agents.roster=$glob"
  fi
  refused_with "roster $name" '^roster_unreadable' 1
  pass_if "roster $name: no path in the output" not grep -aqF "$scratch" "$scratch/out"
}
roster_case "key absent" ABSENT 'clean'
roster_case "key absent, term staged" ABSENT 'Zelda Quillfeather'
roster_case "empty value" '' 'clean'

mkdir "$scratch/r-none" "$scratch/r-mixed" "$scratch/r-numeric" "$scratch/r-notjson" "$scratch/r-empty" \
  "$scratch/r-emptyplus" "$scratch/r-renamed" "$scratch/r-nokeys" "$scratch/r-notobj" "$scratch/r-top" \
  "$scratch/r-dir" "$scratch/r-link" "$scratch/r-short" "$scratch/r-nonjsonplus"
roster_case "glob names nothing" "$scratch/r-none/*.json" 'clean'
roster_case "glob names nothing, term staged" "$scratch/nowhere/*.json" 'Zelda Quillfeather'

cp "$scratch/roster/students-a.json" "$scratch/r-mixed/students-a.json"
printf '%s\n' '[{"name": "Aa Bb", "id": "123456"}]' >"$scratch/r-mixed/students-b.json"
roster_case "renamed keys in the second of two files" "$scratch/r-mixed/students-*.json" 'clean'
roster_case "renamed keys in the second of two files, term staged" "$scratch/r-mixed/students-*.json" 'Zelda Quillfeather'

printf '%s\n' '[{"student_id": 7}]' >"$scratch/r-numeric/students-a.json"
roster_case "a one-digit numeric id alone" "$scratch/r-numeric/students-*.json" 'clean'
printf '%s\n' '[{"student_name": "A", "student_id": "7"}]' >"$scratch/r-short/students-a.json"
roster_case "values shorter than 2 characters" "$scratch/r-short/students-*.json" 'clean'

printf '%s\n' 'this is not json {' >"$scratch/r-notjson/students-a.json"
roster_case "a file that is not JSON" "$scratch/r-notjson/students-*.json" 'clean'
cp "$scratch/roster/students-a.json" "$scratch/r-nonjsonplus/students-a.json"
printf '%s\n' 'not json' >"$scratch/r-nonjsonplus/students-b.json"
roster_case "a file that is not JSON beside a good one" "$scratch/r-nonjsonplus/students-*.json" 'clean'

printf '%s\n' '[]' >"$scratch/r-empty/students-a.json"
roster_case "[] alone" "$scratch/r-empty/students-*.json" 'clean'
printf '%s\n' '[{"name": "Aa Bb", "id": "123456"}]' >"$scratch/r-renamed/students-a.json"
roster_case "keys renamed" "$scratch/r-renamed/students-*.json" 'clean'
printf '%s\n' '[{}]' >"$scratch/r-nokeys/students-a.json"
roster_case "keys absent" "$scratch/r-nokeys/students-*.json" 'clean'
printf '%s\n' '["Zelda Quillfeather"]' >"$scratch/r-notobj/students-a.json"
roster_case "an element that is no object" "$scratch/r-notobj/students-*.json" 'clean'
printf '%s\n' '{"student_name": "Zelda Quillfeather"}' >"$scratch/r-top/students-a.json"
roster_case "a top-level object" "$scratch/r-top/students-*.json" 'clean'
mkdir "$scratch/r-dir/students-a.json"
roster_case "a directory named like a roster file" "$scratch/r-dir/students-*.json" 'clean'
ln -s "$scratch/r-link/gone" "$scratch/r-link/students-a.json"
roster_case "a dangling link named like a roster file" "$scratch/r-link/students-*.json" 'clean'

if [[ "$(id -u)" -ne 0 ]]; then
  mkdir "$scratch/r-perm"
  cp "$scratch/roster/students-a.json" "$scratch/r-perm/students-a.json"
  chmod 000 "$scratch/r-perm/students-a.json"
  roster_case "an unreadable file" "$scratch/r-perm/students-*.json" 'clean'
fi

# An empty array beside a good file passes, and the good file's terms still count.
cp "$scratch/roster/students-a.json" "$scratch/r-emptyplus/students-a.json"
printf '%s\n' '[]' >"$scratch/r-emptyplus/students-b.json"
new_ws
stage "$ws" x.txt 'clean'
commit_try "$ws" -c "agents.roster=$scratch/r-emptyplus/students-*.json"
pass_if "[] beside a good file passes a clean commit" equals "$rc" 0
new_ws
stage "$ws" x.txt 'MV-3302'
commit_try "$ws" -c "agents.roster=$scratch/r-emptyplus/students-*.json"
refused_with "[] beside a good file still refuses a term" '^private_term_staged@x.txt: 1 added lines' 1

# The roster errors with xtrace inherited.
new_ws
stage "$ws" x.txt 'clean'
runner=(env SHELLOPTS=xtrace git)
commit_try "$ws" -c "agents.roster=$scratch/r-notjson/students-*.json"
runner=(git)
pass_if "xtrace inherited, roster not JSON: refused" equals "$rc" 1
pass_if "xtrace inherited, roster not JSON: no term, no path" not grep -aqF "$scratch/r-notjson" "$scratch/out"

# --- 4. the alias --------------------------------------------------------------
git init -q --bare -b main "$scratch/remote.git"
export AGENTS_WORK_ROOT="$scratch/work"
export AGENTS_TASK_REMOTE="$scratch/remote.git"
alias_run() { # directory, then the arguments after `git`
  local where=$1
  shift
  rc=0
  (cd "$where" && git "$@") >"$scratch/out" 2>&1 || rc=$?
}

alias_run "$scratch/other" task-new ws-newtask
pass_if "task-new: exit status" equals "$rc" 0
newdir="$scratch/work/ws-newtask"
pass_if "task-new: the directory holds only .git" equals "$(ls -A "$newdir")" ".git"
pass_if "task-new: the commit" equals "$(git -C "$newdir" log --format=%s)" "task ws-newtask started"
pass_if "task-new: one commit" equals "$(git -C "$newdir" rev-list --count HEAD)" 1
pass_if "task-new: the branch on the remote" \
  equals "$(git --git-dir="$scratch/remote.git" rev-parse refs/heads/task/ws-newtask)" "$(git -C "$newdir" rev-parse HEAD)"
pass_if "task-new: only that branch on the remote" \
  equals "$(git --git-dir="$scratch/remote.git" for-each-ref --format='%(refname)')" "refs/heads/task/ws-newtask"
pass_if "task-new: upstream of main" \
  equals "$(git -C "$newdir" rev-parse --abbrev-ref 'main@{upstream}')" "origin/task/ws-newtask"
pass_if "task-new: the remote url" equals "$(git -C "$newdir" remote get-url origin)" "$scratch/remote.git"
pass_if "task-new: its own scope applies" equals "$(git -C "$newdir" config --get push.default)" upstream

work_listing_before=$(ls -A "$scratch/work")
alias_run "$scratch/other" task-new ws-newtask
pass_if "task-new: an existing directory" equals "$rc" 1
pass_if "task-new: exists" out_has '^task-new: exists$'

for bad in '' bad WS-upper ws- ws--a ws-a- -ws-a xx-a ws_a 'ws-a b' 'ws-é' 'ws-a/b' 'ws-../x' $'ws-a\nb' \
    "ws-$(printf 'a%.0s' $(seq 1 62))"; do
  if [[ -z "$bad" ]]; then
    alias_run "$scratch/other" task-new
  else
    alias_run "$scratch/other" task-new "$bad"
  fi
  pass_if "task-new refuses a name" equals "$rc" 1
  pass_if "task-new refuses a name: message" equals "$(cat "$scratch/out")" "task-new: invalid name"
done
alias_run "$scratch/other" task-new ws-a ws-b
pass_if "task-new refuses two names" equals "$rc" 1
pass_if "task-new refuses names: nothing was made" equals "$(ls -A "$scratch/work")" "$work_listing_before"

long="ws-$(printf 'a%.0s' $(seq 1 61))"
alias_run "$scratch/other" task-new "$long"
pass_if "task-new accepts 64 characters" equals "$rc" 0
alias_run "$scratch/other" task-new ap-two-part-name
pass_if "task-new accepts the ap scope" equals "$rc" 0

# Run from inside a linked worktree: that clone's remotes, branches and log do not change.
snapshot() {
  {
    git -C "$clone" remote -v
    git -C "$clone" for-each-ref
    git -C "$clone" log --all --format=%H
    git -C "$clone" worktree list
    sha256sum <"$clone/.git/config"
    git -C "$linked" status --short
    git -C "$linked" rev-parse --abbrev-ref HEAD
  } 2>&1
}
before=$(snapshot)
alias_run "$linked" task-new ws-fromworktree
pass_if "task-new from a linked worktree: exit status" equals "$rc" 0
pass_if "task-new from a linked worktree: the commit is in the new directory" \
  equals "$(git -C "$scratch/work/ws-fromworktree" log --format=%s)" "task ws-fromworktree started"
pass_if "task-new from a linked worktree: the clone is unchanged" equals "$(snapshot)" "$before"
pass_if "task-new from a linked worktree: the new repository's git dir" \
  equals "$(git -C "$scratch/work/ws-fromworktree" rev-parse --git-dir)" ".git"

# The check refuses the alias's own commit while the roster cannot be read: it stops
# after `remote add`, leaving only .git, and the name then answers "exists".
rc=0
(cd "$scratch/other" && git -c "agents.roster=$scratch/r-none/*.json" task-new ws-halfway) >"$scratch/out" 2>&1 || rc=$?
pass_if "task-new with an unreadable roster: refused" equals "$rc" 1
pass_if "task-new with an unreadable roster: roster_unreadable" out_has '^roster_unreadable'
pass_if "task-new with an unreadable roster: only .git" equals "$(ls -A "$scratch/work/ws-halfway")" ".git"
pass_if "task-new with an unreadable roster: no branch" \
  not git --git-dir="$scratch/remote.git" rev-parse --verify -q refs/heads/task/ws-halfway
alias_run "$scratch/other" task-new ws-halfway
pass_if "task-new after that: exists" out_has '^task-new: exists$'

if (( failures > 0 )); then
  printf 'test-git-config: %d of %d checks failed\n' "$failures" "$checks" >&2
  exit 1
fi
printf 'test-git-config: %d checks ok\n' "$checks"
