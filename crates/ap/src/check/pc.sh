# The PC's side of `ap check` (specs/ap/check.md R6 to R12), run as `bash -c` through `tailscale ssh`
# with the payload of the worktree's state on standard input. Arguments: <work> <key> <nonce>
# <seconds> <command>... Once the command's exit status is known it is the last thing written: one line
# NUL `ap-check <nonce> <status>` on standard error. Without that line the caller counts the call lost.
work=$1 key=$2 nonce=$3 limit=$4
shift 4
start=$(date +%s)
# The PC user's own login environment (the toolchain's PATH), quietly.
if [ -r "$HOME/.profile" ]; then . "$HOME/.profile" >/dev/null 2>&1 </dev/null; fi
set -u
say() { printf 'check: pc: %s\n' "$1" >&2; }
case $key in ????????????????) ;; *) say "bad key"; exit 3 ;; esac
case $key in *[!0-9a-f]*) say "bad key"; exit 3 ;; esac
case $nonce in '' | *[!0-9a-f-]*) say "bad nonce"; exit 3 ;; esac
case $limit in '' | *[!0-9]*) say "bad limit"; exit 3 ;; esac
case $work in /*) ;; *) say "bad work"; exit 3 ;; esac
tree=$work/trees/$key repo=$work/repos/$key target=$work/targets/$key home=$work/cargo-home
in=$work/tmp/$nonce.in tmp=$work/tmp/$nonce
# Removes paths; a link is removed, never followed (find does not follow one, even as an argument).
clean() { find "$@" -type d ! -perm -u=rwx -exec chmod u+rwx {} + 2>/dev/null; rm -rf -- "$@"; }
trap 'clean "$in" "$tmp"' EXIT
mkdir -p -- "$work/trees" "$work/repos" "$work/targets" "$home" "$work/locks" "$work/used" "$in" "$tmp" || { say "work directory"; exit 3; }
tar -x -C "$in" -f - || { say "payload"; exit 3; }
cat >/dev/null

# One call per worktree at a time (R9): the lock of the key, held to the end.
# The wait counts against the time limit: at its end the status is 124.
exec 9>"$work/locks/$key" || { say "lock"; exit 3; }
wait=$((start + limit - $(date +%s)))
[ "$wait" -ge 0 ] || wait=0
if ! flock -w "$wait" 9; then
  [ $(($(date +%s) - start)) -ge "$limit" ] || { say "lock"; exit 3; }
  clean "$in" "$tmp"; trap - EXIT
  printf '\0ap-check %s %s\n' "$nonce" 124 >&2
  exit 0
fi
touch -- "$work/used/$key"

# A tree or target directory no call has used for seven days goes, under its key's lock (R12).
stale() {
  if [ -e "$work/used/$1" ]; then set -- "$work/used/$1"; else set -- "$work/trees/$1" "$work/repos/$1" "$work/targets/$1"; fi
  for p; do [ ! -e "$p" ] || [ -n "$(find "$p" -maxdepth 0 -mtime +6)" ] || return 1; done
}
for d in "$work"/trees/* "$work"/repos/* "$work"/targets/*; do
  k=${d##*/}
  case $k in "$key" | *[!0-9a-f]*) continue ;; ????????????????) ;; *) continue ;; esac
  [ -e "$d" ] || continue
  ( flock -n 8 || exit 0; stale "$k" || exit 0; clean "$work/trees/$k" "$work/repos/$k" "$work/targets/$k" "$work/used/$k" ) 8>"$work/locks/$k"
done
# A temporary directory of a call that ended without its own cleanup, a day later (calls last at most 4 hours).
find "$work/tmp" -mindepth 1 -maxdepth 1 -mmin +1440 -print0 | while IFS= read -r -d '' old; do clean "$old"; done

# The repository: kept outside the tree under repos/<key>, which no command can write (the tree's
# .git is a read-only bind of it), and still reset before each use: no hook, a config of this
# script's own, no alternates, attributes or link to another repository (R6, R8). The tree: the
# worktree's files as listed, nothing else (R6).
g() { git --git-dir="$repo" --work-tree="$tree" -c core.hooksPath=/dev/null -c gc.auto=0 -c maintenance.auto=false "$@"; }
if [ -L "$repo" ] || [ ! -d "$repo" ] || [ ! -f "$repo/HEAD" ] || [ -L "$repo/HEAD" ]; then
  clean "$repo"
  git -c init.defaultBranch=main -c core.hooksPath=/dev/null init -q --bare -- "$repo" || { say "git init"; exit 3; }
fi
clean "$repo/hooks" "$repo/info" "$repo/commondir" "$repo/gitdir" "$repo/objects/info/alternates" "$repo/objects/info/http-alternates" "$repo/index"
mkdir -p -- "$repo/info" && cp -- "$in/exclude" "$repo/info/exclude" && cp -- "$in/global-excludes" "$repo/ap-check-excludes" &&
  printf '[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = false\n\texcludesFile = %s\n' "$repo/ap-check-excludes" > "$repo/config" ||
  { say "repository"; exit 3; }
head=$(cat -- "$in/head")
if [ -f "$in/head.bundle" ]; then
  g fetch -q --no-tags "$in/head.bundle" "+HEAD:refs/ap-check/head" || { say "bundle"; exit 3; }
fi
g update-ref --no-deref HEAD "$head" && [ "$(g rev-parse -q --verify HEAD)" = "$head" ] || { say "head"; exit 3; }
mkdir -p -- "$tree" && find "$tree" -mindepth 1 -maxdepth 1 -print0 | while IFS= read -r -d '' old; do clean "$old"; done
tar -x -p -C "$tree" -f "$in/tree.tar" || { say "files"; exit 3; }
[ ! -e "$tree/.git" ] && [ ! -L "$tree/.git" ] && mkdir -- "$tree/.git" || { say "files"; exit 3; }
g update-index -z --index-info < "$in/index-info" || { say "index"; exit 3; }
g update-index -q --refresh >/dev/null 2>&1
mkdir -p -- "$target" || { say "target"; exit 3; }

# The command: uid 0 of a user namespace, the root read-only but the tree (not its .git), the target directory, the
# cargo home and the temporary directory, the network kept, under nice and the time left (R8, R10).
left=$((start + limit - $(date +%s)))
if [ "$left" -lt 1 ]; then
  rc=124
else
  timeout -k 10 "$left" nice -n 10 bwrap --unshare-user --uid 0 --gid 0 --unshare-pid --die-with-parent --new-session \
    --ro-bind / / --dev /dev --proc /proc --bind "$tmp" /tmp \
    --bind "$tree" "$tree" --ro-bind "$repo" "$tree/.git" --bind "$target" "$target" --bind "$home" "$home" \
    --setenv CARGO_TARGET_DIR "$target" --setenv CARGO_HOME "$home" --setenv TMPDIR /tmp \
    --chdir "$tree" -- "$@" </dev/null
  rc=$?
  case $rc in 124 | 137) [ $(($(date +%s) - start)) -lt "$limit" ] || rc=124 ;; esac
fi
touch -- "$work/used/$key"
clean "$in" "$tmp"
trap - EXIT
printf '\0ap-check %s %s\n' "$nonce" "$rc" >&2
