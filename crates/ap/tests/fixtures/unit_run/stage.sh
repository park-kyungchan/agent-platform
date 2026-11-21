# Every stage of tests/unit_run.rs: run by the fake `claude` in mode `script` in the stage's cwd (the
# unit's worktree). Launch <n> does what line <n> of the file `steps` names; its stdout is the answer.
# A commit is made with an invented committer; a push goes to the branch named in the file `branch`;
# the file `pr-open` stands for the pull request the implementer opens.
here=$(cd "$(dirname "$0")" && pwd)
n=$(cat "$here/count")
act=$(sed -n "${n}p" "$here/steps")
b=$(cat "$here/branch")
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
commit() { mkdir -p src; echo "$1" >> "$2"; git add -A; git -c user.name=Invented -c user.email=invented@example.invalid commit -qm "$1"; }
push() { git push -q origin "HEAD:refs/heads/$b" 2>/dev/null; }
verdict() { printf 'Verdict: %s\n%s\nAn invented report.\n' "$1" "$(git rev-parse HEAD)"; }
# main moves on the origin by one commit that sets file $1 to $2, the worktree untouched.
mainmove() {
  git fetch -q origin main; base=$(git rev-parse FETCH_HEAD); idx=$(mktemp -u)
  GIT_INDEX_FILE=$idx git read-tree "$base"
  blob=$(printf '%s\n' "$2" | git hash-object -w --stdin)
  GIT_INDEX_FILE=$idx git update-index --add --cacheinfo "100644,$blob,$1"
  tree=$(GIT_INDEX_FILE=$idx git write-tree); rm -f "$idx"
  c=$(git -c user.name=Invented -c user.email=invented@example.invalid commit-tree "$tree" -p "$base" -m "main moved $n")
  git push -q origin "$c:refs/heads/main"
}
case "$act" in
  implement) commit "widget $n" src/widgets.rs; push; touch "$here/pr-open"; echo "Implemented." ;;
  edit)      sh "$here/edit.sh"; git add -A; git -c user.name=Invented -c user.email=invented@example.invalid commit -qm "edit $n"; push; touch "$here/pr-open"; echo "Implemented." ;;
  outside)   commit "outside $n" notes.md; push; touch "$here/pr-open"; echo "Implemented." ;;
  implementmain)  commit "widget $n" src/widgets.rs; push; mainmove src/other.rs "other $n"; touch "$here/pr-open"; echo "Implemented." ;;
  implementclash) commit "widget $n" src/widgets.rs; push; mainmove src/widgets.rs "main's widget $n"; touch "$here/pr-open"; echo "Implemented." ;;
  acceptmain) mainmove src/other.rs "other $n"; verdict accept ;;
  repairmain) mainmove src/other.rs "other $n"; verdict repair ;;
  acceptoutside) mainmove docs/other.md "other $n"; verdict accept ;;
  close)     touch "$here/pr-closed"; verdict accept ;;
  ghdown)    touch "$here/gh-down"; verdict accept ;;
  nopr)      cat "$here/nopr-answer" 2>/dev/null || echo "A rule of the page cannot hold: an invented reason." ;;
  local)     commit "local $n" src/widgets.rs; echo "Cut before the push." ;;
  dirty)     commit "widget $n" src/widgets.rs; push; touch "$here/pr-open"; echo left > src/left.rs; echo "Cut." ;;
  settle)    git add -A; git -c user.name=Invented -c user.email=invented@example.invalid commit -qm "settle $n" 2>/dev/null; push; touch "$here/pr-open"; echo "Settled." ;;
  fix)       commit "fix $n" src/widgets.rs; push; echo "Fixed: the finding." ;;
  unpushed)  commit "fix $n" src/widgets.rs; echo "Fixed, not pushed." ;;
  nothing)   echo "Not changed: the finding is wrong, shown by an invented command." ;;
  accept|repair) verdict "$act" ;;
  write)     echo written > src/verifier.rs; verdict accept ;;
  push)      commit "verifier $n" src/widgets.rs; push; verdict accept ;;
  noform)    echo "An answer without a verdict line." ;;
  *)         echo "no step $n" ;;
esac
