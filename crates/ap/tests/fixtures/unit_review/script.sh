# The reviewer of tests/unit_review.rs: run by the fake `claude` in mode `script` in the stage's cwd.
# It reads the prompt it was given (its last line is `Digest: <digest>`), notes its cwd and what the
# cwd holds, and answers `Verdict: <verdict>` (the file `verdict`, default accept) and the digest (the
# file `digest` replaces it; the file `digest-once` replaces it in a fresh launch only). Asked again
# (its input is one sentence that begins `Your last answer`), it reads the digest of the launch
# before. The file `none` makes an answer without a verdict line; the file `touch` makes it change a
# file of its cwd.
here=$(cd "$(dirname "$0")" && pwd)
n=$(cat "$here/count")
d=$(tail -n 1 "$here/prompt.$n")
asked=
case $d in "Your last answer"*) asked=1; d=$(tail -n 1 "$here/prompt.$((n - 1))") ;; esac
d=${d#Digest: }
[ -f "$here/digest" ] && d=$(cat "$here/digest")
[ -f "$here/digest-once" ] && [ -z "$asked" ] && d=$(cat "$here/digest-once")
pwd > "$here/cwd.$n"
ls -A | sort > "$here/ls.$n"
[ -f "$here/touch" ] && echo invented > changed-by-the-reviewer.txt
v=$(cat "$here/verdict" 2>/dev/null || echo accept)
if [ -f "$here/none" ]; then
  echo "An invented answer without a verdict line."
else
  printf 'Verdict: %s\nDigest: %s\nNo blocking finding.' "$v" "$d"
fi
