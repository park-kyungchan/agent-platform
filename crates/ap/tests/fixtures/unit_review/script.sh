# The reviewer of tests/unit_review.rs: run by the fake `claude` in mode `script` in the stage's cwd.
# It reads the prompt it was given (its last line is `Digest: <digest>`), notes its cwd and what the
# cwd holds, and answers `Verdict: <verdict>` (the file `verdict`, default accept) and the digest (the
# file `digest` replaces it). The file `none` makes an answer without a verdict line; the file
# `touch` makes it change a file of its cwd.
here=$(cd "$(dirname "$0")" && pwd)
n=$(cat "$here/count")
d=$(tail -n 1 "$here/prompt.$n")
d=${d#Digest: }
[ -f "$here/digest" ] && d=$(cat "$here/digest")
pwd > "$here/cwd.$n"
ls -A | sort > "$here/ls.$n"
[ -f "$here/touch" ] && echo invented > changed-by-the-reviewer.txt
v=$(cat "$here/verdict" 2>/dev/null || echo accept)
if [ -f "$here/none" ]; then
  echo "An invented answer without a verdict line."
else
  printf 'Verdict: %s\nDigest: %s\nNo blocking finding.' "$v" "$d"
fi
