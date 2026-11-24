#!/usr/bin/env bash
# Tests of scripts/retire-hindsight.sh. Every case runs in a world of its own under a scratch directory from
# mktemp -d: a scratch root given with --root, a stand-in docker program given with --docker that keeps its
# containers, networks, volumes and images as files, and a tar wrapper first on PATH. No network, no root, no
# docker, no host file.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
script=$repo_root/scripts/retire-hindsight.sh

scratch=$(mktemp -d)
scratch=$(cd -P "$scratch" && pwd)
trap 'rm -rf -- "$scratch"' EXIT

failures=0
checks=0
ok() { # <description> <command...>: the command must succeed
  local description=$1
  shift
  checks=$((checks + 1))
  if ! "$@"; then
    printf 'test-retire-hindsight: FAIL %s\n' "$description" >&2
    failures=$((failures + 1))
  fi
}
not() { ! "$@"; }
equals() { [[ "$1" == "$2" ]]; }
contains() { [[ "$1" == *"$2"* ]]; }
mode_of() { stat -c %a -- "$1"; }

# ---- the stand-in docker and the tar wrapper -----------------------------------------------------------------
mkdir -p "$scratch/bin"
cat >"$scratch/bin/fake-docker" <<'EOF'
#!/usr/bin/env bash
# State: $FAKE_DOCKER/{containers,networks,volumes,images}/<name>; a container file holds "<status> <image id>",
# a volume file its mountpoint, an image file is named by the hex of its id. Every call is logged.
set -u
s=$FAKE_DOCKER
printf '%s\n' "$*" >>"$s/log"
last=${!#}
simulated() {
  if [[ ${FAKE_FAIL:-} == "$1" ]]; then
    echo "Error response from daemon: simulated failure" >&2
    exit 1
  fi
}
case "$1 $2" in
  "container ls") ls -1 "$s/containers" ;;
  "network ls") ls -1 "$s/networks" ;;
  "volume ls") ls -1 "$s/volumes" ;;
  "image ls") for f in "$s/images"/*; do [[ -e $f ]] && echo "sha256:${f##*/}"; done; true ;;
  "container inspect")
    [[ -f $s/containers/$last ]] || { echo "Error: No such container: $last" >&2; exit 1; }
    cat "$s/containers/$last" ;;
  "volume inspect")
    [[ -f $s/volumes/$last ]] || { echo "Error: No such volume: $last" >&2; exit 1; }
    cat "$s/volumes/$last" ;;
  "container rm")
    simulated "container rm:$last"
    [[ -f $s/containers/$last ]] || { echo "Error: No such container: $last" >&2; exit 1; }
    read -r status _ <"$s/containers/$last"
    [[ $status != running ]] || { echo "Error response from daemon: container is running" >&2; exit 1; }
    rm -f -- "$s/containers/$last" ;;
  "network rm")
    simulated "network rm:$last"
    [[ -f $s/networks/$last ]] || { echo "Error: No such network: $last" >&2; exit 1; }
    rm -f -- "$s/networks/$last" ;;
  "volume rm")
    simulated "volume rm:$last"
    [[ -f $s/volumes/$last ]] || { echo "Error: No such volume: $last" >&2; exit 1; }
    rm -rf -- "$(cat "$s/volumes/$last")"
    rm -f -- "$s/volumes/$last" ;;
  "image rm")
    [[ $last =~ ^sha256:[0-9a-f]{64}$ ]] || { echo "$last" >>"$s/by-name"; echo "Error: by name" >&2; exit 1; }
    simulated "image rm:$last"
    if [[ ${FAKE_CONFLICT:-} == "$last" ]]; then
      echo "Error response from daemon: conflict: unable to delete ${last:7:12} (must be forced) - image is referenced in multiple repositories" >&2
      exit 1
    fi
    [[ -f $s/images/${last#sha256:} ]] || { echo "Error: No such image: $last" >&2; exit 1; }
    rm -f -- "$s/images/${last#sha256:}" ;;
  *) echo "fake-docker: unknown call $*" >&2; exit 2 ;;
esac
EOF
cat >"$scratch/bin/tar" <<'EOF'
#!/usr/bin/env bash
# With FAKE_TAR_LIST_EXTRA set, a listing (-tf) prints one member more than the archive holds.
/usr/bin/env -u FAKE_TAR_LIST_EXTRA PATH="${FAKE_REAL_PATH}" tar "$@" || exit $?
if [[ -n ${FAKE_TAR_LIST_EXTRA:-} && $1 == -tf ]]; then echo ./planted; fi
EOF
chmod 0755 "$scratch/bin/fake-docker" "$scratch/bin/tar"
export FAKE_REAL_PATH=$PATH
export PATH=$scratch/bin:$PATH
unset FAKE_FAIL FAKE_CONFLICT FAKE_TAR_LIST_EXTRA

# ---- a world ---------------------------------------------------------------------------------------------------
img1=sha256:$(printf '1%.0s' {1..64})
img2=sha256:$(printf '2%.0s' {1..64})
img3=sha256:$(printf '3%.0s' {1..64})
secret=s3cr3t-value-never-printed
lessons=home/palantirkc/work/edu-lessons
n=0
world() { # a new world in W: root R, docker state D, both targets present and stopped
  n=$((n + 1))
  W=$scratch/w$n
  R=$W/root
  D=$W/docker
  export FAKE_DOCKER=$D
  mkdir -p "$D/containers" "$D/networks" "$D/volumes" "$D/images" "$W/vol/home/sub" "$W/vol/pg" "$W/vol/other"
  : >"$D/log"
  echo "exited $img1" >"$D/containers/hindsight-hindsight-1"
  echo "exited $img2" >"$D/containers/hindsight-postgres-1"
  echo "running $img3" >"$D/containers/other-app-1"
  : >"$D/networks/hindsight_database"
  : >"$D/networks/other_net"
  echo "$W/vol/home" >"$D/volumes/hindsight_hindsight-home"
  echo "$W/vol/pg" >"$D/volumes/hindsight_postgres-data"
  echo "$W/vol/other" >"$D/volumes/other_volume"
  : >"$D/images/${img1#sha256:}"
  : >"$D/images/${img2#sha256:}"
  : >"$D/images/${img3#sha256:}"
  echo bank >"$W/vol/home/sub/bank.db"
  echo "PGDATA=1" >"$W/vol/pg/PG_VERSION"
  mkdir -p "$R/docker/hindsight" "$R/root/.claude" "$R/root/.config/muse" "$R/root/.codex" \
    "$R/$lessons/.mcp-launchers" "$R/$lessons/.grok"
  echo "services: {}" >"$R/docker/hindsight/compose.yaml"
  echo "API_KEY=$secret" >"$R/docker/hindsight/.env"
  cat >"$R/$lessons/.mcp.json" <<'J'
{
  "mcpServers": {
    "hindsight": {
      "command": ".mcp-launchers/hindsight.sh"
    },
    "other": {
      "command": "other",
      "args": ["--x", 1.50]
    }
  }
}
J
  echo '#!/bin/sh' >"$R/$lessons/.mcp-launchers/hindsight.sh"
  echo '#!/bin/sh' >"$R/$lessons/.mcp-launchers/other.sh"
  cat >"$R/$lessons/.grok/config.toml" <<'T'
model = "m"

[mcp_servers.other]
command = "other"

[mcp_servers.hindsight]
command = ".mcp-launchers/hindsight.sh"
args = [
  "[not.a.header]",
  "x",
]

[mcp_servers.hindsight.env]
TOKEN_FILE = "t"

# the last server
[mcp_servers.last]
command = "last"
T
  cat >"$R/root/.config/muse/settings.json" <<'J'
{
  "theme": "dark",
  "skills": {
    "/s/hindsight-coding-agent/SKILL.md": {"enabled": true},
    "/s/keep/SKILL.md": {"enabled": false},
    "nested": {"/x/hindsight-control/SKILL.md": 1, "keep": [1, 2.0e1, "é"]}
  },
  "list": [{"/y/hindsight-control/SKILL.md": null}],
  "last": 1
}
J
  chmod 0640 "$R/root/.config/muse/settings.json"
  # scanned files: a mention only as a project's path and in a history line, never in a scanned key path
  cat >"$R/root/.claude.json" <<'J'
{
  "projects": {
    "/home/x/hindsight-work": {
      "mcpServers": {},
      "allowedTools": ["Bash(ls)"],
      "history": [{"display": "start hindsight"}]
    }
  },
  "mcpServers": {"tsidp": {"command": "t"}}
}
J
  cat >"$R/root/.claude/settings.json" <<'J'
{"hooks": {"PreToolUse": [{"matcher": "Bash", "hooks": [{"type": "command", "command": "/opt/guard.sh"}]}]},
 "permissions": {"allow": ["Read"]}}
J
  printf '[projects."/home/x/hindsight-work"]\ntrust_level = "trusted"\n' >"$R/root/.codex/config.toml"
}
OUT= ERR= RC=0
run() {
  RC=0
  OUT=$("$@" 2>"$scratch/err") || RC=$?
  ERR=$(<"$scratch/err")
}
rh() { run bash "$script" "$@" --root "$R" --docker fake-docker; }
rc_is() { [[ $RC == "$1" ]]; }
copy_dir_of() { sed -n 's/^copy //p' <<<"$1"; }
sig() { # everything a refused or reading call must leave alone: names, modes, sizes, times and bytes
  (cd "$W" && find . -path ./docker/log -prune -o ! -type d -print0 | sort -z | xargs -0 stat -c '%n %a %s %Y' \
    && find . -type d -print0 | sort -z | xargs -0 stat -c '%n %a' \
    && find . -type f ! -path ./docker/log -print0 | sort -z | xargs -0 sha256sum)
}
has_docker() { [[ -e $D/$1 ]]; }
no_secret() { not contains "$OUT$ERR" "$secret"; }

# ---- usage -------------------------------------------------------------------------------------------------------
world
rh
ok "no word is a usage refusal" rc_is 2
rh delete
ok "delete without a directory is a usage refusal" rc_is 2
rh check extra
ok "check with an argument is a usage refusal" rc_is 2
run bash "$script" check --root relative --docker fake-docker
ok "a relative root is refused" contains "$ERR" root_invalid

# ---- check -----------------------------------------------------------------------------------------------------------
before=$(sig)
rh check
ok "check exits 0" rc_is 0
ok "check names a container with its state and image" \
  contains "$OUT" "container hindsight-postgres-1 present exited $img2"
ok "check names the volumes" contains "$OUT" "volume hindsight_postgres-data present"
ok "check names the network" contains "$OUT" "network hindsight_database present"
ok "check names the directory" contains "$OUT" "directory /docker/hindsight present"
ok "check names a missing copy root" contains "$OUT" "copy_root /root/.hindsight-recovery absent"
ok "check names the JSON member" contains "$OUT" "entry /$lessons/.mcp.json member [\"mcpServers\",\"hindsight\"] present 1"
ok "check names the TOML tables" contains "$OUT" "entry /$lessons/.grok/config.toml table mcp_servers.hindsight present 2"
ok "check names the skill keys" contains "$OUT" "entry /root/.config/muse/settings.json keys_ending hindsight-coding-agent/SKILL.md,hindsight-control/SKILL.md present 3"
ok "check names the launcher" contains "$OUT" "entry /$lessons/.mcp-launchers/hindsight.sh file present"
ok "a scanned file whose only mention is a path has no hit" contains "$OUT" "scan /root/.claude.json read 0"
ok "an absent scanned file is named absent" contains "$OUT" "scan /root/.gemini/config/mcp_config.json absent 0"
ok "check prints no hit" not contains "$OUT" "hit "
ok "check changes nothing" equals "$(sig)" "$before"
ok "check removes nothing in docker" not grep -q ' rm ' "$D/log"

# ---- copy refuses a running container -----------------------------------------------------------------------------
echo "running $img2" >"$D/containers/hindsight-postgres-1"
before=$(sig)
rh copy
ok "copy refuses a running container" rc_is 2
ok "copy names container_running" equals "$ERR" "container_running hindsight-postgres-1"
ok "the refused copy made nothing" equals "$(sig)" "$before"
echo "exited $img2" >"$D/containers/hindsight-postgres-1"

# ---- copy with a missing copy root, delete, verify ------------------------------------------------------------------------
rh copy
ok "copy exits 0" rc_is 0
ok "copy prints no credential" no_secret
C=$(copy_dir_of "$OUT")
ok "copy prints its directory under the copy root" equals "$(dirname "$C")" "$R/root/.hindsight-recovery"
ok "the copy directory is named final-<UTC time>" eval '[[ $(basename "$C") =~ ^final-[0-9]{8}T[0-9]{6}Z$ ]]'
ok "copy made the missing copy root with mode 0700" equals "$(mode_of "$R/root/.hindsight-recovery")" 700
ok "the copy directory has mode 0700" equals "$(mode_of "$C")" 700
ok "copy prints the manifest's own sha256" equals "$(sed -n 's/^manifest_sha256 //p' <<<"$OUT")" \
  "$(sha256sum <"$C/MANIFEST.sha256" | cut -d' ' -f1)"
ok "the manifest verifies with sha256sum" eval '(cd "$C" && sha256sum --quiet --strict -c MANIFEST.sha256)'
ok "the manifest records each container's image id" eval 'grep -q "\"image\":\"$img1\"" "$C/MANIFEST.sha256" && grep -q "\"image\":\"$img2\"" "$C/MANIFEST.sha256"'
ok "the volume archive keeps the data" contains "$(tar -tf "$C/volume-hindsight_hindsight-home.tar")" "./sub/bank.db"
ok "the volume archive keeps numeric owners" contains "$(tar --numeric-owner -tvf "$C/volume-hindsight_postgres-data.tar")" "$(id -u)/$(id -g)"
ok "the directory archive holds the .env file" contains "$(tar -tf "$C/directory.tar")" "hindsight/.env"
ok "the settings files are copied" cmp -s "$C/file-4-settings.json" "$R/root/.config/muse/settings.json"
ok "the launcher is copied" test -f "$C/file-2-hindsight.sh"
ok "the copy touched no docker object" not grep -q ' rm ' "$D/log"
muse_before=$(stat -c '%u %g %a %i' "$R/root/.config/muse/settings.json")
: >"$D/log"
rh delete "$C"
ok "delete exits 0" rc_is 0
ok "delete prints no credential" no_secret
ok "delete removes the JSON member" contains "$OUT" "entry /$lessons/.mcp.json member [\"mcpServers\",\"hindsight\"] removed 1"
ok "delete removes the launcher" not test -e "$R/$lessons/.mcp-launchers/hindsight.sh"
ok "delete keeps the other launcher" test -f "$R/$lessons/.mcp-launchers/other.sh"
ok "the JSON file keeps its other members byte for byte" equals "$(<"$R/$lessons/.mcp.json")" '{
  "mcpServers": {
    "other": {
      "command": "other",
      "args": ["--x", 1.50]
    }
  }
}'
ok "the settings file keeps its other members byte for byte" equals "$(<"$R/root/.config/muse/settings.json")" '{
  "theme": "dark",
  "skills": {
    "/s/keep/SKILL.md": {"enabled": false},
    "nested": {"keep": [1, 2.0e1, "é"]}
  },
  "list": [{}],
  "last": 1
}'
ok "the settings file keeps its owner, mode and inode" \
  equals "$(stat -c '%u %g %a %i' "$R/root/.config/muse/settings.json")" "$muse_before"
ok "the TOML tables are removed by their lines" equals "$(<"$R/$lessons/.grok/config.toml")" 'model = "m"

[mcp_servers.other]
command = "other"

# the last server
[mcp_servers.last]
command = "last"'
ok "the containers are removed" not has_docker containers/hindsight-hindsight-1
ok "another container stays" has_docker containers/other-app-1
ok "the network is removed, another stays" eval 'not has_docker networks/hindsight_database && has_docker networks/other_net'
ok "the volumes are removed, another stays" eval 'not has_docker volumes/hindsight_postgres-data && has_docker volumes/other_volume'
ok "the recorded images are removed, another stays" \
  eval 'not has_docker images/${img1#sha256:} && not has_docker images/${img2#sha256:} && has_docker images/${img3#sha256:}'
ok "the directory is removed" not test -e "$R/docker/hindsight"
ok "no image was removed by a name" not test -e "$D/by-name"
ok "the removals ran in the page's order" equals "$(grep ' rm ' "$D/log")" "container rm hindsight-hindsight-1
container rm hindsight-postgres-1
network rm hindsight_database
volume rm hindsight_hindsight-home
volume rm hindsight_postgres-data
image rm $img1
image rm $img2"
ok "the copy is untouched by delete" eval '(cd "$C" && sha256sum --quiet --strict -c MANIFEST.sha256)'
rh verify "$C"
ok "verify exits 0 after delete" rc_is 0
ok "verify ends with verified" contains "$OUT" "verified $C"
rh check
ok "check after delete names every target absent" equals "$(grep -v '^copy_root ' <<<"$OUT" | grep -c ' present')" 0
ok "check after delete names the copy root present" contains "$OUT" "copy_root /root/.hindsight-recovery present"

# ---- an archive changed after the manifest --------------------------------------------------------------------------
world
rh copy
C=$(copy_dir_of "$OUT")
printf 'x' >>"$C/volume-hindsight_hindsight-home.tar"
before=$(sig)
rh delete "$C"
ok "delete refuses a changed archive" rc_is 2
ok "a changed archive is copy_not_verified" equals "$ERR" "copy_not_verified digest_mismatch volume-hindsight_hindsight-home.tar"
ok "the refused delete changed nothing" equals "$(sig)" "$before"
rh verify "$C"
ok "verify refuses the changed copy as delete does" contains "$ERR" copy_not_verified

# ---- the copy's own refusals: outside the copy root, a missing copy root, a target not in the manifest ---------------
world
rh copy
C=$(copy_dir_of "$OUT")
cp -a "$C" "$W/final-outside"
before=$(sig)
rh delete "$W/final-outside"
ok "a copy directory outside the copy root is copy_not_verified" equals "$ERR" "copy_not_verified outside_copy_root"
ln -s "$W/final-outside" "$R/root/.hindsight-recovery/final-link"
rh delete "$R/root/.hindsight-recovery/final-link"
ok "a link under the copy root is copy_not_verified" equals "$ERR" "copy_not_verified not_a_directory"
rm -f -- "$R/root/.hindsight-recovery/final-link"
cp -a "$C" "$R/root/.hindsight-recovery/other-name"
rh delete "$R/root/.hindsight-recovery/other-name"
ok "a directory not named final-* is copy_not_verified" equals "$ERR" "copy_not_verified not_final"
rm -rf -- "$R/root/.hindsight-recovery/other-name"
rh verify "$W/final-outside"
ok "verify refuses outside the copy root" equals "$ERR" "copy_not_verified outside_copy_root"
ok "the refusals changed nothing" equals "$(sig)" "$before"
mv -- "$R/root/.hindsight-recovery" "$W/moved"
rh delete "$C"
ok "a missing copy root is copy_not_verified" equals "$ERR" "copy_not_verified copy_root_missing"
mv -- "$W/moved" "$R/root/.hindsight-recovery"
world
rm -f -- "$D/networks/hindsight_database"
rh copy
C=$(copy_dir_of "$OUT")
ok "an absent target is recorded as absent" grep -q '"kind":"network","name":"hindsight_database","state":"absent"' "$C/MANIFEST.sha256"
: >"$D/networks/hindsight_database"
before=$(sig)
rh delete "$C"
ok "a target present now but absent in the manifest is copy_not_verified" \
  equals "$ERR" "copy_not_verified target_unrecorded hindsight_database"
ok "that refusal changed nothing" equals "$(sig)" "$before"

# ---- delete refuses a running container -----------------------------------------------------------------------------
world
rh copy
C=$(copy_dir_of "$OUT")
echo "running $img1" >"$D/containers/hindsight-hindsight-1"
before=$(sig)
rh delete "$C"
ok "delete refuses a running container" equals "$ERR" "container_running hindsight-hindsight-1"
ok "the refused delete changed nothing (running)" equals "$(sig)" "$before"

# ---- JSON entry files that are not strict JSON -------------------------------------------------------------------------
for bad in '{"mcpServers": {"hindsight": {}' '{"mcpServers": {"hindsight": {}}, "a": 1, "a": 2}' '{"mcpServers": {"hindsight": NaN}}'; do
  world
  rh copy
  C=$(copy_dir_of "$OUT")
  printf '%s\n' "$bad" >"$R/$lessons/.mcp.json"
  before=$(sig)
  rh delete "$C"
  ok "delete refuses $bad" equals "$ERR" "json_unreadable /$lessons/.mcp.json"
  ok "delete refused before any removal ($bad)" equals "$(sig)" "$before"
done
world
rh copy
C=$(copy_dir_of "$OUT")
printf '{"skills": {"/a/hindsight-control/SKILL.md": 1, "k": 1, "k": 2}}\n' >"$R/root/.config/muse/settings.json"
before=$(sig)
rh delete "$C"
ok "a settings file with a key given twice is json_unreadable" \
  equals "$ERR" "json_unreadable /root/.config/muse/settings.json"
ok "that refusal changed nothing" equals "$(sig)" "$before"

# ---- a delete cut after the containers, run again ------------------------------------------------------------------------
world
rh copy
C=$(copy_dir_of "$OUT")
FAKE_FAIL="network rm:hindsight_database" rh delete "$C"
ok "a failed network removal exits 1" rc_is 1
ok "the failed step is printed" contains "$ERR" "removal_failed network hindsight_database"
ok "the containers before it are removed" not has_docker containers/hindsight-postgres-1
ok "nothing after it ran: the volumes stay" has_docker volumes/hindsight_hindsight-home
ok "nothing after it ran: the images stay" has_docker "images/${img1#sha256:}"
ok "nothing after it ran: the directory stays" test -d "$R/docker/hindsight"
rh verify "$C"
ok "verify after the cut exits 1" rc_is 1
ok "verify names what is left" eval 'contains "$OUT" "left network hindsight_database" && contains "$OUT" "left volume hindsight_postgres-data" && contains "$OUT" "left directory /docker/hindsight"'
rh delete "$C"
ok "the delete run again exits 0" rc_is 0
ok "the run again finds the entries gone" contains "$OUT" "entry /$lessons/.mcp.json member [\"mcpServers\",\"hindsight\"] gone"
ok "the run again finds the containers gone" contains "$OUT" "container hindsight-hindsight-1 gone"
ok "the run again removes the network" contains "$OUT" "network hindsight_database removed"
rh verify "$C"
ok "verify after the run again exits 0" rc_is 0

# ---- a removal that fails -------------------------------------------------------------------------------------------------
world
rh copy
C=$(copy_dir_of "$OUT")
FAKE_FAIL="volume rm:hindsight_postgres-data" rh delete "$C"
ok "a failed volume removal exits 1" rc_is 1
ok "the failed volume step is printed" contains "$ERR" "removal_failed volume hindsight_postgres-data"
ok "the volume before it is removed" not has_docker volumes/hindsight_hindsight-home
ok "no image removal ran after the failure" not grep -q 'image rm' "$D/log"
ok "the directory stays after the failure" test -d "$R/docker/hindsight"

# ---- an image docker refuses to remove --------------------------------------------------------------------------------------
world
rh copy
C=$(copy_dir_of "$OUT")
FAKE_CONFLICT=$img1 rh delete "$C"
ok "an image docker refuses is no failure" rc_is 0
ok "the refused image is printed as kept" contains "$OUT" "image $img1 kept"
ok "the other image is removed" contains "$OUT" "image $img2 removed"
ok "the directory step ran after the kept image" not test -e "$R/docker/hindsight"
rh verify "$C"
ok "verify with a kept image exits 0" rc_is 0
ok "verify prints the kept image apart" contains "$OUT" "image $img1 kept"

# ---- a hit in a scanned file -------------------------------------------------------------------------------------------------
world
rh copy
C=$(copy_dir_of "$OUT")
cat >"$R/root/.claude/settings.json" <<'J'
{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "/opt/Hindsight/retain.sh"}]}]},
 "history": ["hindsight"]}
J
scanned_before=$(sha256sum <"$R/root/.claude/settings.json")
rh check
ok "a mention inside a hook command's path is a hit" \
  contains "$OUT" 'hit /root/.claude/settings.json ["hooks","Stop",0,"hooks",0,"command"]'
ok "a mention outside the key paths is no hit" equals "$(grep -c '^hit ' <<<"$OUT")" 1
ok "a hit prints no value" not contains "$OUT" retain.sh
rh delete "$C"
ok "delete with a scanned hit runs" rc_is 0
rh verify "$C"
ok "verify with a scanned hit exits 1" rc_is 1
ok "verify names the hit" contains "$OUT" 'hit /root/.claude/settings.json ["hooks","Stop",0,"hooks",0,"command"]'
ok "a scanned file is never changed" equals "$(sha256sum <"$R/root/.claude/settings.json")" "$scanned_before"

# ---- a read-back that fails -------------------------------------------------------------------------------------------------
world
before=$(sig)
FAKE_TAR_LIST_EXTRA=1 rh copy
ok "a failed read-back exits 1" rc_is 1
ok "a failed read-back is named" contains "$ERR" "read_back_failed members_mismatch"
ok "a failed read-back removes the directory it made" equals "$(ls -A "$R/root/.hindsight-recovery")" ""

if [[ $failures -ne 0 ]]; then
  printf 'test-retire-hindsight: failed (%d of %d)\n' "$failures" "$checks" >&2
  exit 1
fi
printf 'test-retire-hindsight: ok (%d checks)\n' "$checks"
