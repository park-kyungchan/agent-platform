#!/usr/bin/env bash
# Install content/ of this repository as the shared instructions of the host, or put a kept copy back.
#   scripts/install-content.sh install <commit>    from this checkout, which must be clean at <commit> on origin/main
#   scripts/install-content.sh check               every file of the recorded checksum list is in the target (exit 0; 1 drift; 3 not recorded)
#   scripts/install-content.sh rollback            puts back the kept copy the install record names (one level); it runs only when the
#                                                  target equals its record (check ok, no .old-* or .new-* directory of an interrupted
#                                                  run), else it refuses rollback_target_not_recorded_state: use rollback <tarball>
#   scripts/install-content.sh rollback <tarball>  a tarball this script wrote before an install
#   scripts/install-content.sh status              one JSON line: name, commit, installed_at, check, rollback
# The file list is content/SHA256SUMS (no fixed skill list). Target ${AP_CONTENT_DST:-/root/.agents}; kept copies
# in ${AP_CONTENT_BACKUP_DIR:-/var/backups/agent-platform-content} (mode 0700). `install` and `rollback` hold one
# lock (${AP_CONTENT_LOCK:-<kept copy directory>/.install.lock}) for their whole run. The target holds the record of
# the install (.content-record.json: commit, installed_at, kept, files) and the checksum list it was made from
# (.content-sums); they are swapped in with the files, so a failed install leaves the previous record. `status` says
# `available` exactly when `rollback` without an argument would run: both ask rollback_plan.
# A rollback that completed but could not write its marker <kept>.consumed (the only write after the swap) exits 4 with
# `rollback_completed_marker_failed`: the files, the record and the owned list ARE the previous install's, and a second
# rollback without an argument is not blocked, so it must not be run. Exit 2 is a refusal, exit 1 a failure that was undone.
# Order, so that a failure never leaves a half-installed tree:
#   install : verify the checkout's checksums; copy everything to a staging directory beside the target and verify
#             it THERE; write the tarball of what will be replaced and read it back; only then swap path by path,
#             moving each old path aside; verify the target; on any failure move the old paths back.
#   rollback: verify the kept copy's checksum file (the tarball and every sidecar file) and that the tarball lists
#             only the installer's paths, extract it to a staging directory, stage the record that was in force
#             before that install, and only then swap. Nothing is removed before the replacement is on disk.
set -euo pipefail
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR
REPO=$(cd -- "$(dirname -- "$(realpath "$0")")/.." && pwd -P)
DST=${AP_CONTENT_DST:-/root/.agents}; DST=${DST%/}
BK=${AP_CONTENT_BACKUP_DIR:-/var/backups/agent-platform-content}
LOCK=${AP_CONTENT_LOCK:-$BK/.install.lock}
refuse() { echo "refuse: $*" >&2; exit 2; }
REC=.content-record.json SUMS=.content-sums META=(.content-paths .content-record.json .content-sums)
take_lock() {
  mkdir -p -- "$BK"; chmod 0700 "$BK"
  if ! { exec 9>"$LOCK"; } 2>/dev/null; then refuse "install_lock_unavailable: cannot open $LOCK"; fi
  flock -n 9 || refuse "install_busy: another install or rollback holds $LOCK"
}
json_field() { sed -n "s/.*\"$2\"[[:space:]]*:[[:space:]]*\"\([^\"]*\)\".*/\1/p" "$1" | head -n 1; }
write_record() { # <file> <commit> <installed_at> <kept copy or empty> <file count>
  local kept=null; [ -z "$4" ] || kept="\"$4\""
  printf '{"commit":"%s","installed_at":"%s","kept":%s,"files":%s}\n' "$2" "$3" "$kept" "$5" > "$1"; }
REC_RE='^\{"commit":"[0-9a-f]{40,64}","installed_at":"[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z","kept":(null|"[^"\\[:cntrl:]]*"),"files":[0-9]+\}$'
# content_state: sets CHK (ok|drift|not_recorded), CHK_COMMIT, CHK_AT, CHK_KEPT (only from a well-formed record), CHK_FILES
# and CHK_BAD (up to 10 paths). A record that is empty, cut or not the one this script writes, a checksum list with a
# malformed line or a line count other than the record's `files` are drift. Writes nothing.
content_state() { local out rc=0 n bad f
  CHK_COMMIT=""; CHK_AT=""; CHK_KEPT=""; CHK_BAD=""; CHK_FILES=0
  if [ ! -f "$DST/$REC" ] || [ ! -f "$DST/$SUMS" ]; then CHK=not_recorded; return 0; fi
  CHK=drift
  n=$(grep -c '' "$DST/$SUMS" || true); CHK_FILES=$n
  bad=$(grep -vcE '^[0-9a-f]{64}  [^[:cntrl:]]+$' "$DST/$SUMS" || true)
  if [ "$(grep -c '' "$DST/$REC" || true)" -eq 1 ] && grep -Eq "$REC_RE" "$DST/$REC"; then
    CHK_COMMIT=$(json_field "$DST/$REC" commit); CHK_AT=$(json_field "$DST/$REC" installed_at); CHK_KEPT=$(json_field "$DST/$REC" kept)
    f=$(sed 's/.*"files":\([0-9]*\)}$/\1/' "$DST/$REC")
  else return 0; fi
  { [ "$n" -gt 0 ] && [ "$bad" -eq 0 ] && [ "$n" = "$f" ]; } || return 0
  out=$(cd "$DST" && sha256sum -c "$DST/$SUMS" 2>/dev/null) || rc=$?
  CHK_BAD=$(printf '%s\n' "$out" | sed -n 's/: FAILED.*$//p' | head -n 10) || true
  if [ "$rc" -eq 0 ]; then CHK=ok; fi
}
# rollback_plan: the ONE decision of whether `rollback` without an argument runs; `status` asks the same function.
# Success sets RB_KEPT. Failure sets RB_CODE and RB_MSG (it does not touch anything).
rollback_plan() { local k
  RB_KEPT=""; RB_CODE=rollback_not_available; RB_MSG=""
  content_state
  [ "$CHK" != not_recorded ] || { RB_MSG="$DST has no install record"; return 1; }
  if [ "$CHK" != ok ] || compgen -G "$DST/.old-*" > /dev/null || compgen -G "$DST/.new-*" > /dev/null; then
    RB_CODE=rollback_target_not_recorded_state
    RB_MSG="the target does not equal its install record (check says $CHK) or holds the leftovers of an interrupted run (.old-* or .new-*); read check, then use rollback <tarball> with the kept copy you mean"
    return 1
  fi
  k=$CHK_KEPT
  [ -n "$k" ] || { RB_MSG="the record names no kept copy"; return 1; }
  [ ! -e "$k.consumed" ] || { RB_MSG="a rollback was made already; the record now in force names $k, which is not offered again"; return 1; }
  case "$(basename -- "$k")" in agents-before-*.tar) ;; *) RB_MSG="the record names a path that is not a kept copy"; return 1 ;; esac
  [ "$(realpath -m -- "$(dirname -- "$k")")" = "$(realpath -m -- "$BK")" ] || { RB_MSG="the record names a kept copy outside $BK"; return 1; }
  { [ -f "$k" ] && [ -f "$k.sha256" ] && [ -f "$k.paths" ]; } || { RB_MSG="the kept copy or one of its files is missing"; return 1; }
  (cd "$(dirname -- "$k")" && sha256sum -c --quiet "$(basename -- "$k").sha256") > /dev/null 2>&1 || { RB_MSG="the kept copy (tarball or a sidecar file) does not match its checksum"; return 1; }
  RB_KEPT=$k; }
stamp=$(date -u +%Y%m%dT%H%M%SZ)-$$
moved_old=(); placed=()
# A path is there when it exists or is a symbolic link: a relative link need not resolve where it is staged.
there() { [ -e "$1" ] || [ -L "$1" ]; }
# Put back what a failed swap changed: remove what was newly placed, then move back what was moved aside.
restore_moved() { local p
  for p in "${placed[@]:-}"; do [ -n "$p" ] || continue; rm -rf -- "$DST/$p" 2>/dev/null || true; done
  for p in "${moved_old[@]:-}"; do
    [ -n "$p" ] || continue
    if there "$DST/.old-$stamp/$p" && ! there "$DST/$p"; then
      mv -- "$DST/.old-$stamp/$p" "$DST/$p" || echo "restore_failed@$DST/$p: its previous bytes are in $DST/.old-$stamp/$p" >&2
    fi
  done
  rmdir -- "$DST/.old-$stamp/skills" "$DST/.old-$stamp" 2>/dev/null || true
  return 0; }
# swap <staging root> <relative path>...: after it, each path is what the staging root holds; a path the staging
# root does not hold is moved aside (gone from the target). A path enters a list only after its move succeeded.
swap() { local stage=$1 p; shift
  mkdir -p -- "$DST/.old-$stamp/skills" || return 1
  for p in "$@"; do
    if there "$DST/$p"; then mv -- "$DST/$p" "$DST/.old-$stamp/$p" || return 1; moved_old+=("$p"); fi
    there "$stage/$p" || continue
    mkdir -p -- "$(dirname -- "$DST/$p")" || return 1
    mv -- "$stage/$p" "$DST/$p" || return 1; placed+=("$p")
  done; }
# listed <word> <list...>: success when the word is one of the list (no pipe: `grep -q` ending a pipe under pipefail is a race).
listed() { local w=$1 x; shift; for x in "$@"; do [ "$x" = "$w" ] && return 0; done; return 1; }
owned_ok() { case "$1" in *..*|skills/|skills/*/*) return 1 ;; AGENTS.md|runtime-references|skills/*) return 0 ;; esac; return 1; }
paths_of() { # <SHA256SUMS> -> the top paths it owns: AGENTS.md, runtime-references, skills/<name>
  cut -c67- "$1" | awk -F/ '$1=="skills"{print "skills/"$2; next} {print $1}' | sort -u; }
case "${1:-}" in
install)
  [ "$#" -eq 2 ] || refuse "usage: install <commit>"
  want=$2; SRC=$REPO/content
  [ "$(git -C "$REPO" rev-parse HEAD)" = "$want" ] || refuse "checkout is not at $want"
  [ -z "$(git -C "$REPO" status --porcelain -- content)" ] || refuse "content/ has uncommitted changes"
  git -C "$REPO" merge-base --is-ancestor "$want" origin/main || refuse "$want is not on origin/main"
  (cd "$SRC" && sha256sum -c --quiet SHA256SUMS) || refuse "content/ does not match its SHA256SUMS"
  mapfile -t new < <(paths_of "$SRC/SHA256SUMS")
  for p in "${new[@]}"; do owned_ok "$p" || refuse "unexpected owned path: $p"; done
  # Every refusal above comes before anything is created; from here on the lock is held for every write.
  take_lock
  # Paths the previous install owned and this content no longer names are removed too (a retired skill);
  # a skill directory this installer never installed is not touched.
  prev=(); [ -f "$DST/.content-paths" ] && mapfile -t prev < "$DST/.content-paths"
  mapfile -t paths < <(printf '%s\n' "${new[@]}" "${prev[@]:-}" | grep . | sort -u)
  for p in "${paths[@]}"; do owned_ok "$p" || refuse "unexpected owned path: $p"; done
  mkdir -p -- "$DST" "$BK"; chmod 0700 "$BK"
  stage=$DST/.new-$stamp; rm -rf -- "$stage"; mkdir -p -- "$stage/skills"
  trap 'rm -rf -- "$stage"' EXIT
  for p in "${new[@]}"; do cp -a -- "$SRC/$p" "$stage/$p"; done
  (cd "$stage" && sha256sum -c --quiet "$SRC/SHA256SUMS") || refuse "the staged copy does not match"
  tarball=$BK/agents-before-$stamp.tar; have=()
  for p in "${paths[@]}"; do there "$DST/$p" && have+=("$p"); done
  printf '%s\n' "${paths[@]}" > "$tarball.paths"
  tar -C "$DST" -cpf "$tarball" --files-from=/dev/null "${have[@]}"
  tar -tf "$tarball" > /dev/null || refuse "the kept copy cannot be read back"
  # The sidecar files of the kept copy: the owned paths, the previous owned list, record and checksum list.
  side=("$(basename "$tarball").paths")
  [ -f "$DST/.content-paths" ] && { cp -p -- "$DST/.content-paths" "$tarball.owned-before"; side+=("$(basename "$tarball").owned-before"); }
  [ -f "$DST/$REC" ] && { cp -p -- "$DST/$REC" "$tarball.record-before"; side+=("$(basename "$tarball").record-before"); }
  [ -f "$DST/$SUMS" ] && { cp -p -- "$DST/$SUMS" "$tarball.sums-before"; side+=("$(basename "$tarball").sums-before"); }
  # ONE checksum file covers the tarball and every sidecar file.
  (cd "$BK" && sha256sum "$(basename "$tarball")" "${side[@]}" > "$tarball.sha256")
  # The record and its checksum list are staged with the files and swapped in with them.
  printf '%s\n' "${new[@]}" > "$stage/.content-paths"
  cp -- "$SRC/SHA256SUMS" "$stage/$SUMS"
  write_record "$stage/$REC" "$want" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$tarball" "$(grep -c . "$SRC/SHA256SUMS")"
  if ! swap "$stage" "${paths[@]}" "${META[@]}" || ! (cd "$DST" && sha256sum -c --quiet "$SRC/SHA256SUMS"); then
    restore_moved
    echo "install_failed_restored@$DST: the previous files and record are back; kept copy $tarball" >&2; exit 1
  fi
  rm -rf -- "$DST/.old-$stamp"
  echo "installed $want files=$(grep -c . "$SRC/SHA256SUMS") paths=${#paths[@]} backup=$tarball"
  ;;
check)
  [ "$#" -eq 1 ] || refuse "usage: check"
  content_state
  case $CHK in
    not_recorded) echo "not_recorded: $DST has no install record (an install made before this version, or none)" >&2; exit 3 ;;
    ok) echo "ok commit=$CHK_COMMIT files=$CHK_FILES"; exit 0 ;;
    *) echo "drift commit=${CHK_COMMIT:-unknown}: changed or missing:"; if [ -n "$CHK_BAD" ]; then printf '%s\n' "$CHK_BAD"; else echo "(the install record or its checksum list is damaged)"; fi; exit 1 ;;
  esac
  ;;
status)
  [ "$#" -eq 1 ] || refuse "usage: status"
  [ -d "$DST" ] && [ -r "$DST" ] || { echo "target_unreadable: $DST" >&2; exit 1; }
  content_state
  rb=none; ! rollback_plan || rb=available
  if [ -n "$CHK_COMMIT" ] && [ -n "$CHK_AT" ]; then c="\"$CHK_COMMIT\""; a="\"$CHK_AT\""; else c=null; a=null; fi
  printf '{"name":"content","commit":%s,"installed_at":%s,"check":"%s","rollback":"%s"}\n' "$c" "$a" "$CHK" "$rb"
  ;;
rollback)
  [ "$#" -le 2 ] || refuse "usage: rollback [<tarball>]"
  if [ "$#" -eq 2 ]; then
    tarball=$2
    { [ -f "$tarball" ] && [ -f "$tarball.sha256" ] && [ -f "$tarball.paths" ]; } || refuse "tarball, .sha256 or .paths missing"
    take_lock
  else
    # Without an argument: only when the target equals its record (rollback_plan), under the lock.
    [ -d "$BK" ] || refuse "rollback_not_available: $BK does not exist"
    take_lock
    rollback_plan || refuse "$RB_CODE: $RB_MSG"
    tarball=$RB_KEPT
  fi
  { [ -f "$tarball" ] && [ -f "$tarball.sha256" ] && [ -f "$tarball.paths" ]; } || refuse "tarball, .sha256 or .paths missing"
  # One checksum file covers the tarball and every sidecar file; a changed or missing one refuses before any swap.
  (cd "$(dirname -- "$tarball")" && sha256sum -c --quiet "$(basename -- "$tarball").sha256") || refuse "the kept copy (tarball or a sidecar file) does not match its checksum"
  if tar -tf "$tarball" | grep -Ev '^(AGENTS\.md|runtime-references(/.*)?|skills/[A-Za-z0-9_-]+(/.*)?)$' | grep -q .; then refuse "the tarball lists a path that is not the installer's"; fi
  mapfile -t paths < "$tarball.paths"
  for p in "${paths[@]}"; do owned_ok "$p" || refuse "unexpected path in .paths: $p"; done
  # Only the kept copy of the LATEST install can be rolled back: a path the target owns now and this copy
  # does not cover came from a later install, which is rolled back first.
  cur=(); [ -f "$DST/.content-paths" ] && mapfile -t cur < "$DST/.content-paths"
  for c in "${cur[@]:-}"; do
    [ -n "$c" ] || continue
    listed "$c" "${paths[@]}" || refuse "rollback_not_latest: the target owns $c, which this kept copy does not cover; roll back the later install first"
  done
  stage=$DST/.new-$stamp; rm -rf -- "$stage"; mkdir -p -- "$stage"
  trap 'rm -rf -- "$stage"' EXIT
  tar -C "$stage" -xpf "$tarball"
  # The owned list, record and checksum list after this rollback are the ones in force before that install
  # (the same bytes; absent when there were none). The kept copy the restored record names is marked consumed
  # after the swap (<kept>.consumed, outside the checksum file: it is state, not content): one level of rollback.
  [ ! -f "$tarball.owned-before" ] || cp -p -- "$tarball.owned-before" "$stage/.content-paths"
  [ ! -f "$tarball.sums-before" ] || cp -p -- "$tarball.sums-before" "$stage/$SUMS"
  rk=""; if [ -f "$tarball.record-before" ]; then cp -p -- "$tarball.record-before" "$stage/$REC"; rk=$(json_field "$tarball.record-before" kept); fi
  # A path the install added (it is not in the kept copy) goes away; every other path gets its kept bytes back.
  # A failure in the middle puts the target back as it was before this rollback.
  if ! swap "$stage" "${paths[@]}" "${META[@]}"; then
    restore_moved
    echo "rollback_failed_restored@$DST: the target is as it was before this rollback" >&2; exit 1
  fi
  rm -rf -- "$DST/.old-$stamp"
  if [ -n "$rk" ] && [ -f "$rk" ]; then
    : > "$rk.consumed" || { echo "rollback_completed_marker_failed@$rk.consumed: the rollback itself completed (files, record and owned list are the previous install's); only the marker could not be written, so a second rollback without an argument is not blocked: do not run it" >&2; exit 4; }
  fi
  echo "rolled back to $tarball"
  ;;
*) echo "usage: $0 (install <commit>|check|rollback [<tarball>]|status)" >&2; exit 2 ;;
esac
