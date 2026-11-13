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
  (cd "$BK" && sha256sum "$(basename "$tarball")" > "$tarball.sha256")
  [ -f "$DST/.content-paths" ] && cp -p -- "$DST/.content-paths" "$tarball.owned-before"
  if ! swap "$stage" "${paths[@]}" || ! (cd "$DST" && sha256sum -c --quiet "$SRC/SHA256SUMS") || ! printf '%s\n' "${new[@]}" > "$DST/.content-paths"; then
    restore_moved
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
  for p in "${paths[@]}"; do owned_ok "$p" || refuse "unexpected path in .paths: $p"; done
  stage=$DST/.new-$stamp; rm -rf -- "$stage"; mkdir -p -- "$stage"
  trap 'rm -rf -- "$stage"' EXIT
  tar -C "$stage" -xpf "$tarball"
  # A path the install added (it is not in the kept copy) goes away; every other path gets its kept bytes back.
  # A failure in the middle puts the target back as it was before this rollback.
  if ! swap "$stage" "${paths[@]}"; then
    restore_moved
    echo "rollback_failed_restored@$DST: the target is as it was before this rollback" >&2; exit 1
  fi
  if [ -f "$tarball.owned-before" ]; then cp -p -- "$tarball.owned-before" "$DST/.content-paths"; else rm -f -- "$DST/.content-paths"; fi
  rm -rf -- "$DST/.old-$stamp"
  echo "rolled back to $tarball"
  ;;
*) echo "usage: $0 (install <commit>|rollback <tarball>)" >&2; exit 2 ;;
esac
