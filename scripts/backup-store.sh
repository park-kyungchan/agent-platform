#!/usr/bin/env bash
# Back up the Ontology store with `ont backup` and copy each new backup to an rclone remote (deploy/README.md,
# "The store's backup"). No argument. Settings: the JSON file named by AP_BACKUP_CONFIG (default
# /etc/agent-platform/backup.json; a missing file means the defaults and no remote):
#   store  the store's file        (default /var/lib/ontology-platform/ont/ontology.sqlite)
#   state  a directory             (default /var/lib/agent-platform/backup): lock, last.json
#   remote an rclone path such as name:dir/sub, or absent
#   keep   files that stay on the remote, 1 to 60 (default 14)
# Exit 0: backup made, remote step copied, unchanged or none (also `backup: busy`). 3: backup made, a remote step
# failed. 4: the backup failed. 2: the settings are refused (before the lock; nothing is run, no record written).
# One line on stdout: `backup: <code or ok> <backup path or ->`. rclone's own output is discarded, so no
# credential or configuration text can reach an output or the record.
set -euo pipefail

cfg=${AP_BACKUP_CONFIG-/etc/agent-platform/backup.json}
[ -n "$cfg" ] || { echo "refuse: AP_BACKUP_CONFIG is empty" >&2; exit 2; }
# The settings, validated, one per line: store, state, remote (empty: none), keep.
mapfile -t cfgv < <(python3 -I -c '
import json, os, re, sys
p = sys.argv[1]
d = {}
if os.path.exists(p):
    d = json.load(open(p))
    assert isinstance(d, dict)
def text(k, default):
    v = d.get(k, default)
    assert isinstance(v, str) and v and v.startswith("/") and not re.search(r"[\x00-\x1f]", v), k
    return v
store = text("store", "/var/lib/ontology-platform/ont/ontology.sqlite")
state = text("state", "/var/lib/agent-platform/backup")
remote = d.get("remote") or ""
assert isinstance(remote, str) and (remote == "" or re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]*:[A-Za-z0-9_./-]*", remote) and ".." not in remote), "remote"
keep = d.get("keep", 14)
assert type(keep) is int and 1 <= keep <= 60, "keep"
print(store, state, remote, keep, sep="\n")
' "$cfg" 2> /dev/null) || { echo "refuse: the settings file is not valid" >&2; exit 2; }
[ "${#cfgv[@]}" -eq 4 ] || { echo "refuse: the settings file is not valid" >&2; exit 2; }
store=${cfgv[0]}; state=${cfgv[1]}; remote=${cfgv[2]}; keep=${cfgv[3]}
[ -n "$store" ] && [ -n "$state" ] || { echo "refuse: a path setting is empty" >&2; exit 2; }

mkdir -p -- "$state"
exec 9> "$state/lock"
flock -n 9 || { echo "backup: busy"; exit 0; }
rm -f -- "$state"/.last.json.*   # a temporary file a killed run left; this run holds the lock

now() { date -u "$1"; }
at=$(now +%Y-%m-%dT%H:%M:%SZ)
backup=""; sha=""; bytes=""; retention=null; remote_state=none; remote_name=""; code=""; deleted=()

# finish <exit status>: write last.json by a rename of a temporary file, print the line, exit.
finish() {
  local tmp
  tmp=$(mktemp "$state/.last.json.XXXXXX")
  python3 -I -c '
import json, sys
at, store, backup, sha, bytes_, retention, remote, name, code, *deleted = sys.argv[1:]
print(json.dumps({"at": at, "store": store, "backup": backup or None, "sha256": sha or None,
  "bytes": int(bytes_) if bytes_ else None, "retention_failure": json.loads(retention), "remote": remote,
  "remote_name": name or None, "deleted": deleted, "code": code or None}, indent=1))
' "$at" "$store" "$backup" "$sha" "$bytes" "$retention" "$remote_state" "$remote_name" "$code" "${deleted[@]}" > "$tmp"
  chmod 0644 "$tmp"
  mv -f -- "$tmp" "$state/last.json"
  echo "backup: ${code:-ok} ${backup:--}"
  exit "$1"
}

# The previous digest that reached the remote (read before the record is replaced).
prev=""
if [ -f "$state/last.json" ]; then
  prev=$(python3 -I -c '
import json, sys
try:
    r = json.load(open(sys.argv[1]))
    print(r["sha256"] if r["remote"] in ("copied", "unchanged") and isinstance(r["sha256"], str) else "")
except Exception:
    print("")
' "$state/last.json")
fi

answer=$(ont --store "$store" backup 2> /dev/null) || answer=""
mapfile -t got < <(python3 -I -c '
import json, os, re, sys
try:
    a = json.loads(sys.argv[1])
    r = a["result"]
    assert a["status"] == "ok" and isinstance(r["path"], str) and "\n" not in r["path"] and os.path.isfile(r["path"])
    assert re.fullmatch(r"[0-9a-f]{64}", r["sha256"])
    print(r["path"], r["sha256"], os.path.getsize(r["path"]), json.dumps(r.get("retention_failure")), sep="\n")
except Exception:
    pass
' "$answer")
if [ "${#got[@]}" -ne 4 ]; then code=backup_failed; remote_state=none; finish 4; fi
backup=${got[0]}; sha=${got[1]}; bytes=${got[2]}; retention=${got[3]}

[ -n "$remote" ] || finish 0
if [ "$sha" = "$prev" ]; then remote_state=unchanged; finish 0; fi

remote_state=failed
name=$(now +%Y%m%dT%H%M%SZ)-${sha:0:12}.sqlite
base=$remote; case "$base" in *:|*/) ;; *) base=$base/ ;; esac
rclone copyto "$backup" "$base$name" > /dev/null 2>&1 || { code=remote_copy_failed; finish 3; }
listing=$(rclone lsf "$remote" 2> /dev/null) || { code=remote_list_failed; finish 3; }
printf '%s\n' "$listing" | grep -qxF -- "$name" || { code=remote_list_failed; finish 3; }
remote_state=copied; remote_name=$name

# Only names of the form this script writes are candidates; the newest `keep` stay; the new file never goes.
mapfile -t old < <(printf '%s\n' "$listing" | grep -E '^[0-9]{8}T[0-9]{6}Z-[0-9a-f]{12}\.sqlite$' | sort || true)
excess=$(( ${#old[@]} - keep ))
for (( i = 0; i < excess; i++ )); do
  [ "${old[i]}" != "$name" ] || continue
  if rclone deletefile "$base${old[i]}" > /dev/null 2>&1; then deleted+=("${old[i]}")
  else remote_state=failed; code=remote_prune_failed; finish 3; fi
done
finish 0
