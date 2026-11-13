#!/usr/bin/env bash
# Install content/ of this repository as the shared instructions of the host, or put a kept copy back.
#   scripts/install-content.sh install <commit>    from this checkout, which must be clean at <commit> on origin/main
#   scripts/install-content.sh rollback <tarball>  a tarball this script wrote before an install
# The file list is content/SHA256SUMS (no fixed skill list). Target ${AP_CONTENT_DST:-/root/.agents}; kept copies
# in ${AP_CONTENT_BACKUP_DIR:-/var/backups/agent-platform-content} (mode 0700).
# Order, so that a failure never leaves a half-installed tree:
#   install : verify the checkout's checksums; copy everything to a staging directory beside the target and verify
#             it THERE; write the tarball of what will be replaced and read it back; only then swap path by path,
#             moving each old path aside; verify the target; on any failure move the old paths back.
#   rollback: verify the tarball's checksum file and that it lists only the installer's paths, extract it to a
#             staging directory, and only then swap. Nothing is removed before the replacement is on disk.
set -euo pipefail
unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_COMMON_DIR
REPO=$(cd -- "$(dirname -- "$(realpath "$0")")/.." && pwd -P)
DST=${AP_CONTENT_DST:-/root/.agents}; DST=${DST%/}
BK=${AP_CONTENT_BACKUP_DIR:-/var/backups/agent-platform-content}
refuse() { echo "refuse: $*" >&2; exit 2; }
stamp=$(date -u +%Y%m%dT%H%M%SZ)-$$
moved=()
restore_moved() { local p
  for p in "${moved[@]:-}"; do
    [ -n "$p" ] || continue
    rm -rf -- "$DST/$p" 2>/dev/null || true
    if [ -e "$DST/.old-$stamp/$p" ]; then mv -- "$DST/.old-$stamp/$p" "$DST/$p" || echo "restore_failed@$DST/$p: its previous bytes are in $DST/.old-$stamp/$p" >&2; fi
  done; return 0; }
# swap <staging root> <relative path>...: each path of the staging root replaces the same path of the target.
swap() { local stage=$1 p; shift
  mkdir -p -- "$DST/.old-$stamp/skills" || return 1
  for p in "$@"; do
    [ -e "$stage/$p" ] || continue
    moved+=("$p")
    if [ -e "$DST/$p" ]; then mv -- "$DST/$p" "$DST/.old-$stamp/$p" || return 1; fi
    mkdir -p -- "$(dirname -- "$DST/$p")" || return 1
    mv -- "$stage/$p" "$DST/$p" || return 1
  done; }
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
  mapfile -t paths < <(paths_of "$SRC/SHA256SUMS")
  mkdir -p -- "$DST" "$BK"; chmod 0700 "$BK"
  stage=$DST/.new-$stamp; rm -rf -- "$stage"; mkdir -p -- "$stage/skills"
  trap 'rm -rf -- "$stage"' EXIT
  for p in "${paths[@]}"; do cp -a -- "$SRC/$p" "$stage/$p"; done
  (cd "$stage" && sha256sum -c --quiet "$SRC/SHA256SUMS") || refuse "the staged copy does not match"
  tarball=$BK/agents-before-$stamp.tar; have=()
  for p in "${paths[@]}"; do [ -e "$DST/$p" ] && have+=("$p"); done
  printf '%s\n' "${paths[@]}" > "$tarball.paths"
  tar -C "$DST" -cpf "$tarball" --files-from=/dev/null "${have[@]}"
  tar -tf "$tarball" > /dev/null || refuse "the kept copy cannot be read back"
  (cd "$BK" && sha256sum "$(basename "$tarball")" > "$tarball.sha256")
  if ! swap "$stage" "${paths[@]}" || ! (cd "$DST" && sha256sum -c --quiet "$SRC/SHA256SUMS"); then
    restore_moved; rmdir -- "$DST/.old-$stamp/skills" "$DST/.old-$stamp" 2>/dev/null || true
    echo "install_failed_restored@$DST: the previous files are back; kept copy $tarball" >&2; exit 1
  fi
  rm -rf -- "$DST/.old-$stamp"
  echo "installed $want files=$(grep -c . "$SRC/SHA256SUMS") paths=${#paths[@]} backup=$tarball"
  ;;
rollback)
  [ "$#" -eq 2 ] || refuse "usage: rollback <tarball>"
  tarball=$2
  [ -f "$tarball" ] && [ -f "$tarball.sha256" ] && [ -f "$tarball.paths" ] || refuse "tarball, .sha256 or .paths missing"
  (cd "$(dirname -- "$tarball")" && sha256sum -c --quiet "$(basename -- "$tarball").sha256") || refuse "the tarball does not match its checksum"
  if tar -tf "$tarball" | grep -Ev '^(AGENTS\.md|runtime-references(/.*)?|skills/[A-Za-z0-9_-]+(/.*)?)$' | grep -q .; then refuse "the tarball lists a path that is not the installer's"; fi
  mapfile -t paths < "$tarball.paths"
  stage=$DST/.new-$stamp; rm -rf -- "$stage"; mkdir -p -- "$stage"
  trap 'rm -rf -- "$stage"' EXIT
  tar -C "$stage" -xpf "$tarball"
  # A path the install added (it was not in the kept copy) goes away; every other path is replaced by its kept bytes.
  mkdir -p -- "$DST/.old-$stamp/skills"
  for p in "${paths[@]}"; do
    case "$p" in AGENTS.md|runtime-references|skills/*) ;; *) refuse "unexpected path in .paths: $p" ;; esac
    if [ -e "$DST/$p" ]; then mv -- "$DST/$p" "$DST/.old-$stamp/$p"; moved+=("$p"); fi
    if [ -e "$stage/$p" ]; then mkdir -p -- "$(dirname -- "$DST/$p")"; mv -- "$stage/$p" "$DST/$p"; fi
  done
  rm -rf -- "$DST/.old-$stamp"
  echo "rolled back to $tarball"
  ;;
*) echo "usage: $0 (install <commit>|rollback <tarball>)" >&2; exit 2 ;;
esac
