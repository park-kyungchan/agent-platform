# The PC's side of a look of `ap run` (specs/ap/run.md R59 to R61), run as `bash -c` through `tailscale ssh`
# with one payload on standard input: the section `tar <bytes>` (the worktree's state as specs/ap/check.md R7
# sends it), the section `extra <bytes>` (a tar of the commit of origin/main, each read-only file and the
# proxy's text), then, to the end, the bytes the CLI reads on its standard input. Arguments: <work> <key>
# <nonce> <seconds> <cwd> <scratch> <session> <the six words of the proxy's `outside` form, the socket
# being the placeholder /ap-egress.sock> <n> <n words NAME=value> <the CLI's words>. The look runs in a
# sandbox that is an allow-list; the proxy's lines go to <work>/egress/<nonce>.lines and nowhere else.
work=$1 key=$2 nonce=$3 limit=$4 cwd=$5 scratch=$6 session=$7
shift 7
start=$(date +%s)
# The stage account's own login environment (the toolchain's PATH), quietly.
if [ -r "$HOME/.profile" ]; then . "$HOME/.profile" >/dev/null 2>&1 </dev/null; fi
set -u
say() { printf 'stage: pc: %s\n' "$1" >&2; }
bad() { say "$1"; exit 3; }
case $key in ????????????????) ;; *) bad "bad key" ;; esac
case $key in *[!0-9a-f]*) bad "bad key" ;; esac
case $nonce in '' | *[!0-9a-f-]*) bad "bad nonce" ;; esac
case $limit in '' | *[!0-9]*) bad "bad limit" ;; esac
case $work in /*) ;; *) bad "bad work" ;; esac
case $cwd in /*) ;; *) bad "bad cwd" ;; esac
case $scratch in /*) ;; *) bad "bad scratch" ;; esac
case $session in ????????-????-????-????-????????????) ;; *) bad "bad session" ;; esac
case $session in *[!0-9a-f-]*) bad "bad session" ;; esac
[ "$#" -ge 7 ] || bad "arguments"
[ "$1" = outside ] && [ "$2" = /ap-egress.sock ] && [ "$3" = "$nonce" ] || bad "proxy words"
role=$4 names=$5 ranges=$6 ne=$7
shift 7
case $ne in '' | *[!0-9]*) bad "bad count" ;; esac
[ "$#" -ge "$ne" ] || bad "arguments"
envw=("${@:1:$ne}")
shift "$ne"
for w in ${envw[@]+"${envw[@]}"}; do
  case ${w%%=*} in '' | [0-9]* | *[!A-Za-z0-9_]*) bad "bad variable" ;; esac
  case $w in *=*) ;; *) bad "bad variable" ;; esac
done
cli=("$@")
[ "${cli[0]-}" = claude ] || bad "not the CLI"
tree=$work/trees/$key repo=$work/repos/$key home=$work/homes/$session
tmp=$work/tmp/$nonce in=$work/tmp/$nonce/in lines=$work/egress/$nonce.lines
run= outside_pid=
# Removes paths; a link is removed, never followed (find does not follow one, even as an argument).
clean() { find "$@" -type d ! -perm -u=rwx -exec chmod u+rwx {} + 2>/dev/null; rm -rf -- "$@"; }
# At the end: the proxy's outside part ends (it removes its socket), then the call's directories go; the
# lines file stays for the fetch of the caller, which removes it.
finish() {
  if [ -n "$outside_pid" ]; then
    kill -TERM "$outside_pid" 2>/dev/null
    for _ in $(seq 1 150); do kill -0 "$outside_pid" 2>/dev/null || break; sleep 0.1; done
    kill -KILL "$outside_pid" 2>/dev/null
    wait "$outside_pid" 2>/dev/null
  fi
  if [ -n "$run" ]; then clean "$run"; fi
  clean "$tmp"
}
trap finish EXIT
mkdir -p -- "$work/trees" "$work/repos" "$work/locks" "$work/used" "$work/homes" "$work/tmp" || bad "work directory"
mkdir -p -m 0700 -- "$work/egress" && chmod 0700 -- "$work/egress" || bad "work directory"
mkdir -- "$tmp" "$in" "$in/extra" || bad "work directory"

# The payload: two framed tars, then the CLI's input.
len=
section() {
  local name n
  IFS=' ' read -r name n || return 1
  [ "$name" = "$1" ] || return 1
  case $n in '' | *[!0-9]*) return 1 ;; esac
  len=$n
}
section tar && head -c "$len" | tar -x -C "$in" -f - || bad "payload"
section extra && head -c "$len" | tar -x -C "$in/extra" -f - || bad "payload"
cat > "$in/stdin" || bad "payload"

# One look per worktree at a time: the lock of the key, held to the end. The wait counts against the
# time limit.
exec 9>"$work/locks/$key" || bad "lock"
wait=$((start + limit - $(date +%s)))
[ "$wait" -ge 0 ] || wait=0
flock -w "$wait" 9 || bad "lock"
touch -- "$work/used/$key"

# A tree no call has used for seven days goes, under its key's lock (check.md R12).
stale() {
  if [ -e "$work/used/$1" ]; then set -- "$work/used/$1"; else set -- "$work/trees/$1" "$work/repos/$1"; fi
  for p; do [ ! -e "$p" ] || [ -n "$(find "$p" -maxdepth 0 -mtime +6)" ] || return 1; done
}
for d in "$work"/trees/* "$work"/repos/*; do
  k=${d##*/}
  case $k in "$key" | *[!0-9a-f]*) continue ;; ????????????????) ;; *) continue ;; esac
  [ -e "$d" ] || continue
  ( flock -n 8 || exit 0; stale "$k" || exit 0; clean "$work/trees/$k" "$work/repos/$k" "$work/used/$k" ) 8>"$work/locks/$k"
done
# What a call left without its cleanup, a day later: a temporary directory, a home, a lines file no
# fetch removed.
for d in tmp homes egress; do
  find "$work/$d" -mindepth 1 -maxdepth 1 -mmin +1440 -print0 | while IFS= read -r -d '' old; do
    case $old in "$tmp" | "$home") continue ;; esac
    clean "$old"
  done
done

# The repository and the tree as `ap check` keeps them (check.md R6), and the ref origin/main.
g() { git --git-dir="$repo" --work-tree="$tree" -c core.hooksPath=/dev/null -c gc.auto=0 -c maintenance.auto=false "$@"; }
if [ -L "$repo" ] || [ ! -d "$repo" ] || [ ! -f "$repo/HEAD" ] || [ -L "$repo/HEAD" ]; then
  clean "$repo"
  git -c init.defaultBranch=main -c core.hooksPath=/dev/null init -q --bare -- "$repo" || bad "git init"
fi
clean "$repo/hooks" "$repo/info" "$repo/commondir" "$repo/gitdir" "$repo/objects/info/alternates" "$repo/objects/info/http-alternates" "$repo/index"
mkdir -p -- "$repo/info" && cp -- "$in/exclude" "$repo/info/exclude" && cp -- "$in/global-excludes" "$repo/ap-check-excludes" &&
  printf '[core]\n\trepositoryformatversion = 0\n\tfilemode = true\n\tbare = false\n\texcludesFile = %s\n' "$repo/ap-check-excludes" > "$repo/config" ||
  bad "repository"
head=$(cat -- "$in/head")
if [ -f "$in/head.bundle" ]; then
  g fetch -q --no-tags "$in/head.bundle" "+HEAD:refs/ap-check/head" || bad "bundle"
fi
g update-ref --no-deref HEAD "$head" && [ "$(g rev-parse -q --verify HEAD)" = "$head" ] || bad "head"
if [ -f "$in/extra/main" ]; then
  main=$(cat -- "$in/extra/main")
  case $main in '' | *[!0-9a-f]*) bad "main" ;; esac
  if [ -f "$in/extra/main.bundle" ]; then
    g fetch -q --no-tags "$in/extra/main.bundle" "+refs/remotes/origin/main:refs/ap-check/main" || bad "main"
  fi
  g update-ref refs/remotes/origin/main "$main" && [ "$(g rev-parse -q --verify "refs/remotes/origin/main^{commit}")" = "$main" ] || bad "main"
else
  g update-ref -d refs/remotes/origin/main 2>/dev/null
fi
mkdir -p -- "$tree" && find "$tree" -mindepth 1 -maxdepth 1 -print0 | while IFS= read -r -d '' old; do clean "$old"; done
tar -x -p -C "$tree" -f "$in/tree.tar" || bad "files"
[ ! -e "$tree/.git" ] && [ ! -L "$tree/.git" ] && mkdir -- "$tree/.git" || bad "files"
g update-index -z --index-info < "$in/index-info" || bad "index"
g update-index -q --refresh >/dev/null 2>&1

# The look's home, where the CLI keeps its session (so an ask resumes in it), and the two files of the
# account's Claude configuration directory it may read: its sign-in and its CLAUDE.md. A name the binds
# cover is absent or a regular file, never a link: no look loads what another wrote.
[ ! -L "$home" ] || bad "home"
[ -d "$home" ] || mkdir -m 0700 -- "$home" || bad "home"
cfg=${CLAUDE_CONFIG_DIR:-$HOME/.claude}
credentials=()
for f in .credentials.json CLAUDE.md; do
  if [ -e "$home/$f" ] || [ -L "$home/$f" ]; then
    [ -f "$home/$f" ] && [ ! -L "$home/$f" ] || bad "home"
  fi
  if [ -f "$cfg/$f" ] && [ ! -L "$cfg/$f" ]; then credentials+=(--ro-bind "$cfg/$f" "$home/$f"); fi
done

# What only this call knows, under a random name that is not the nonce: the look's scratch directory,
# the directory of the proxy's socket, the directory of its `ap`, and copies of its read-only files.
id=$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')
case $id in ????????????????????????????????) ;; *) bad "random" ;; esac
run=$work/tmp/$id
mkdir -m 0700 -- "$run" "$run/sock" "$run/bin" "$run/ro" "$run/scratch" "$run/scratch/tmp" "$run/scratch/target" || bad "work directory"
readonly_binds=()
i=0
if [ -f "$in/extra/ro-paths" ]; then
  while IFS= read -r -d '' p; do
    case $p in /*) ;; *) bad "read-only path" ;; esac
    [ -f "$in/extra/ro/$i" ] || bad "read-only file"
    cp -- "$in/extra/ro/$i" "$run/ro/$i" && chmod 0444 -- "$run/ro/$i" || bad "read-only file"
    readonly_binds+=(--ro-bind "$run/ro/$i" "$p")
    i=$((i + 1))
  done < "$in/extra/ro-paths"
fi

# The look's `ap` (R61): the build of `ap check` in the look's own view, and nothing else of `ap`.
{
  printf '#!/bin/bash\ncwd=%q\ndeadline=%s\n' "$cwd" "$((start + limit))"
  cat <<'EOF'
fail() {
  printf '%s\n' '{"error":{"code":"not_on_pc","field":"args","detail":"no door on the PC: ap fn and every other verb are not served here; read the worktree instead"}}' >&2
  exit 2
}
[ "${1-}" = check ] && [ "${2-}" = "$cwd" ] || fail
shift 2
timeout=2700
if [ "${1-}" = --timeout ]; then
  case ${2-} in '' | *[!0-9]* | 0*) fail ;; esac
  [ "$2" -le 14400 ] || fail
  timeout=$2
  shift 2
fi
cmd=(./check)
if [ "$#" -gt 0 ]; then
  [ "$1" = -- ] && [ "$#" -ge 2 ] && [ -n "$2" ] || fail
  shift
  cmd=("$@")
fi
left=$((deadline - $(date +%s)))
[ "$left" -ge 1 ] || exit 124
[ "$left" -ge "$timeout" ] || timeout=$left
cd -- "$cwd" || exit 127
export CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 AP_CHECK_HOST=pc
exec timeout -k 10 "$timeout" "${cmd[@]}"
EOF
} > "$run/bin/ap" && chmod 0755 -- "$run/bin/ap" || bad "ap"

# The proxy (pc-egress): its outside part here, outside the sandbox, on a socket in the directory the look
# sees; the nonce is in its arguments and its error file only, which no view holds. Its lines go to
# <work>/egress/<nonce>.lines (mode 0600).
sockfile=$run/sock/ap-egress.sock
prog=$(cat -- "$in/extra/egress.py") || bad "proxy"
( umask 077; : >> "$lines" ) || bad "proxy"
( exec python3 -I -c "$prog" outside "$sockfile" "$nonce" "$role" "$names" "$ranges" </dev/null >/dev/null 2>>"$lines" ) &
outside_pid=$!
for _ in $(seq 1 100); do [ -S "$sockfile" ] && break; kill -0 "$outside_pid" 2>/dev/null || break; sleep 0.1; done
[ -S "$sockfile" ] || bad "proxy"

# What a sandbox of this script sees of the PC (check.md R8), and nothing else: the system's programs
# read-only at their own paths; the toolchain of the login environment; the CLI's program and what it
# resolves to; python3 where it is outside the system's directories (the proxy's inside part runs in
# the look's own network namespace, which has the loopback alone).
rustup_home=${RUSTUP_HOME:-$HOME/.rustup}
cargo_bin=${CARGO_HOME:-$HOME/.cargo}/bin
found() {
  local p
  p=$(command -v "$1" 2>/dev/null) || return 1
  case $p in /*) [ -f "$p" ] && printf '%s' "$p" ;; *) return 1 ;; esac
}
ont=$(found ont) || ont=
claude=$(found claude) || bad "claude"
python=$(found python3) || bad "python3"
system() { case $1 in /usr/* | /etc/* | /bin/* | /sbin/* | /lib/* | /lib32/* | /lib64/* | /libx32/* | "$rustup_home"/* | "$cargo_bin"/*) return 0 ;; esac; return 1; }
view=(--tmpfs /tmp --ro-bind /usr /usr --ro-bind /etc /etc)
for p in /bin /sbin /lib /lib32 /lib64 /libx32; do
  if [ -L "$p" ]; then
    view+=(--symlink "$(readlink -- "$p")" "$p")
  elif [ -d "$p" ]; then
    view+=(--ro-bind "$p" "$p")
  fi
done
for p in "$rustup_home" "$cargo_bin"; do
  case $p in /*) [ ! -d "$p" ] || view+=(--ro-bind "$p" "$p") ;; esac
done
[ -z "$ont" ] || system "$ont" || view+=(--ro-bind "$ont" "$ont")
resolved=$(readlink -f -- "$claude") || resolved=$claude
system "$claude" || view+=(--ro-bind "$claude" "$claude")
[ "$resolved" = "$claude" ] || system "$resolved" || view+=(--ro-bind "$resolved" "$resolved")
system "$python" || view+=(--ro-bind "$python" "$python")
view+=(--dev /dev --proc /proc)
ns=(--unshare-user --uid 0 --gid 0 --unshare-pid --unshare-net --die-with-parent --new-session)

# The look's environment: the request's, then the settings of this script, so that they win.
port=$((20000 + RANDOM % 40000))
proxy=http://127.0.0.1:$port
setenv=(--clearenv)
for w in ${envw[@]+"${envw[@]}"}; do setenv+=(--setenv "${w%%=*}" "${w#*=}"); done
setenv+=(--setenv PATH "/ap-bin:$PATH" --setenv HOME "$HOME" --setenv CLAUDE_CONFIG_DIR "$home"
  --setenv TMPDIR "$scratch/tmp" --setenv CARGO_TARGET_DIR "$scratch/target"
  --setenv HTTPS_PROXY "$proxy" --setenv https_proxy "$proxy" --setenv HTTP_PROXY "$proxy" --setenv http_proxy "$proxy")
# Before the CLI the proxy's inside part must say `ready` on the loopback of the look's namespace.
inner='exec 3< <(exec python3 -I -c "$1" inside "$2" "$3" 2>/dev/null); IFS= read -r line <&3 || exit 97; [ "$line" = ready ] || exit 97; exec 3<&-; shift 3; exec "$@"'

left=$((start + limit - $(date +%s)))
[ "$left" -ge 1 ] || bad "time"
timeout -k 10 "$left" nice -n 10 bwrap "${ns[@]}" "${view[@]}" \
  --bind "$tree" "$cwd" --ro-bind "$repo" "$cwd/.git" \
  ${readonly_binds[@]+"${readonly_binds[@]}"} \
  --bind "$run/scratch" "$scratch" \
  --bind "$home" "$home" ${credentials[@]+"${credentials[@]}"} \
  --ro-bind "$run/sock" /ap-egress --ro-bind "$run/bin" /ap-bin \
  --remount-ro / "${setenv[@]}" --chdir "$cwd" -- \
  bash -c "$inner" ap-inner "$prog" /ap-egress/ap-egress.sock "$port" "${cli[@]}" < "$in/stdin"
rc=$?
touch -- "$work/used/$key"
exit "$rc"
