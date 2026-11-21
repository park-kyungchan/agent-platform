# The PC's side of `ap check` (specs/ap/check.md R6 to R12 and R18), run as `bash -c` through `tailscale ssh`
# with the payload of the worktree's state on standard input. Arguments: <work> <key> <nonce>
# <seconds> <command>... Once the command's exit status is known it is the last thing written: one line
# NUL `ap-check <nonce> <status> <MB>` on standard error, <MB> the lowest MemAvailable of this Linux
# while the command ran, `-` where no command ran. Without that line the caller counts the call lost.
# Before the command, a step fetches the dependencies of a tree with Cargo.toml and Cargo.lock (R18).
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
# The sampler of the memory (R11), its own process group and session once its setsid ran: ended
# with its group and its PID, since before that setsid the group does not exist yet (R10); the
# group once more for a child it may have started meanwhile.
sampler=
unsample() {
  [ -n "$sampler" ] || return 0
  kill -KILL -- "-$sampler" "$sampler" 2>/dev/null
  kill -KILL -- "-$sampler" 2>/dev/null
  wait "$sampler" 2>/dev/null
  sampler=
}
trap 'unsample; clean "$in" "$tmp"' EXIT
# The bound ran out before any command (the lock wait of R9, the step of R18): the call ends with 124 and `-`.
out_of_time() {
  clean "$in" "$tmp"; trap - EXIT
  printf '\0ap-check %s %s -\n' "$nonce" 124 >&2
  exit 0
}
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
  out_of_time
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

# What a sandbox of this script sees of the PC (R8), and nothing else: the system's programs
# read-only at their own paths (a link stays a link); the toolchain of the login environment
# read-only at its own paths, each part where it exists: rustup's home, the bin of the account's
# own Cargo home, and the file of ont where neither those nor the system's directories show it; a
# /proc and a /dev of its own; the call's temporary directory at /tmp. No other path of the account's
# home, of /mnt, /run or /var: the parents of a shown path hold only the names that lead to it, and
# the root is made read-only once the binds are in place (--remount-ro /).
rustup_home=${RUSTUP_HOME:-$HOME/.rustup}
cargo_bin=${CARGO_HOME:-$HOME/.cargo}/bin
ont=$(command -v ont 2>/dev/null) || ont=
case $ont in /*) [ -f "$ont" ] || ont= ;; *) ont= ;; esac
case $ont in /usr/* | /etc/* | /bin/* | /sbin/* | /lib/* | /lib32/* | /lib64/* | /libx32/* | "$rustup_home"/* | "$cargo_bin"/*) ont= ;; esac
view=(--ro-bind /usr /usr --ro-bind /etc /etc)
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
[ -z "$ont" ] || view+=(--ro-bind "$ont" "$ont")
view+=(--dev /dev --proc /proc --bind "$tmp" /tmp)
# One namespace each for the user (uid 0) and the processes, ended with the sandbox.
ns=(--unshare-user --uid 0 --gid 0 --unshare-pid --die-with-parent --new-session)

# The dependencies (R18): `cargo fetch` of the tree's lock file before the command, which has no
# network. It reads the tree as data and runs none of its code: started outside the tree, so that
# no .cargo/config.toml or rust-toolchain.toml of the tree chooses a program, a toolchain or a
# source; in a sandbox of the view above with the PC's network, the cargo home writable, the tree
# read-only, no target directory, and the file /etc/resolv.conf resolves to where that lies outside
# /etc. Its outputs go to standard error. The bound ending it ends the call; a failure otherwise is
# the PC's side failing before the command: no status line, so the call is lost and runs there.
if [ -f "$tree/Cargo.toml" ] && [ ! -L "$tree/Cargo.toml" ] && [ -f "$tree/Cargo.lock" ] && [ ! -L "$tree/Cargo.lock" ]; then
  resolv=()
  r=$(readlink -f /etc/resolv.conf 2>/dev/null) || r=
  case $r in /etc/* | '') ;; /*) [ ! -f "$r" ] || resolv=(--ro-bind "$r" "$r") ;; esac
  left=$((start + limit - $(date +%s)))
  [ "$left" -ge 1 ] || out_of_time
  timeout -k 10 "$left" nice -n 10 bwrap "${ns[@]}" "${view[@]}" ${resolv[@]+"${resolv[@]}"} \
    --ro-bind "$tree" "$tree" --ro-bind "$repo" "$tree/.git" --bind "$home" "$home" --remount-ro / \
    --setenv CARGO_HOME "$home" --setenv TMPDIR /tmp --unsetenv CARGO_NET_OFFLINE --unsetenv CARGO_TARGET_DIR \
    --chdir /tmp -- cargo fetch --locked --manifest-path "$tree/Cargo.toml" </dev/null >&2
  fetched=$?
  if [ "$fetched" -ne 0 ]; then
    case $fetched in 124 | 137) [ $(($(date +%s) - start)) -lt "$limit" ] || out_of_time ;; esac
    say "fetch"; exit 3
  fi
fi

# The memory while the command runs (R11): MemAvailable of this Linux in kB, read here, outside the
# command's namespaces: once by this script itself before the command starts, so a command however
# short has its reading, then every 2 seconds by a sampler that keeps the lowest in a file under $in,
# which the command cannot write (never under $tmp, its /tmp). No Windows process runs meanwhile.
# The sampler writes nothing to the session's streams; it is killed before the status line and at
# this script's exit, and ends by itself at the call's deadline or once this script is gone (R10).
mem() { sed -n 's/^MemAvailable: *\([0-9][0-9]*\) kB$/\1/p' /proc/meminfo 2>/dev/null; }
low=$in/low first=
left=$((start + limit - $(date +%s)))
if [ "$left" -lt 1 ]; then
  rc=124
else
  first=$(mem)
  case $first in *[!0-9]*) first= ;; esac
  if [ -n "$first" ]; then
    setsid bash -c 'low=$3; while sleep 2 && [ "$(date +%s)" -lt "$4" ] && kill -0 "$2" 2>/dev/null; do
        m=$(sed -n "s/^MemAvailable: *\([0-9][0-9]*\) kB\$/\1/p" /proc/meminfo 2>/dev/null)
        case $m in "" | *[!0-9]*) continue ;; esac
        [ "$m" -lt "$low" ] || continue
        low=$m; printf "%s\n" "$low" > "$1.new" && mv -f -- "$1.new" "$1"
      done' ap-check-sampler "$low" "$$" "$first" "$((start + limit))" </dev/null >/dev/null 2>&1 &
    sampler=$!
  fi
  # The command (R8): the view above, a network namespace of its own with the loopback alone, the
  # tree and the target directory writable, the cargo home read-only, the network off for cargo.
  timeout -k 10 "$left" nice -n 10 bwrap "${ns[@]}" --unshare-net "${view[@]}" \
    --bind "$tree" "$tree" --ro-bind "$repo" "$tree/.git" --bind "$target" "$target" --ro-bind "$home" "$home" --remount-ro / \
    --setenv CARGO_TARGET_DIR "$target" --setenv CARGO_HOME "$home" --setenv TMPDIR /tmp \
    --setenv AP_CHECK_HOST pc --setenv CARGO_INCREMENTAL 0 --setenv CARGO_PROFILE_DEV_DEBUG 0 --setenv CARGO_PROFILE_TEST_DEBUG 0 \
    --setenv CARGO_NET_OFFLINE true \
    --chdir "$tree" -- "$@" </dev/null
  rc=$?
  case $rc in 124 | 137) [ $(($(date +%s) - start)) -lt "$limit" ] || rc=124 ;; esac
fi
unsample
# The lowest reading in MB: the sampler's file where it holds a lower number, else the first
# reading; `-` where no command ran or that first reading failed.
lowest=-
if [ -n "$first" ]; then
  lowest=$first
  sampled=$(cat -- "$low" 2>/dev/null)
  case $sampled in "" | *[!0-9]*) ;; *) [ "$sampled" -ge "$lowest" ] || lowest=$sampled ;; esac
  lowest=$((lowest / 1024))
fi
touch -- "$work/used/$key"
clean "$in" "$tmp"
trap - EXIT
printf '\0ap-check %s %s %s\n' "$nonce" "$rc" "$lowest" >&2
