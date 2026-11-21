#!/usr/bin/env bash
# Back up the Ontology store with `ont backup` and copy each new backup, encrypted, to an rclone remote
# (deploy/README.md, "The store's backup"). No argument. Settings: the JSON file named by AP_BACKUP_CONFIG
# (default /etc/agent-platform/backup.json; a missing file means the defaults and no remote):
#   store     the store's file        (default /var/lib/ontology-platform/ont/ontology.sqlite)
#   state     a directory             (default /var/lib/agent-platform/backup): lock, last.json
#   remote    an rclone path such as name:dir/sub, or absent
#   keep      files that stay on the remote, 1 to 60 (default 14)
#   recipient an absolute path of a file that holds exactly one OpenPGP public key (binary or armored); required
#             for a remote: only the backup encrypted with `gpg` to that key leaves the host, never the plain
#             backup; no private key or passphrase is read, made or kept here
# The name on the remote: <YYYYMMDDTHHMMSSZ>-<12 hex of the plain backup's sha256>.sqlite.gpg. Pruning keeps the
# newest `keep` names of that form and of the older plain form (.sqlite) together; it deletes no other name.
# Exit 0: backup made, remote step copied, unchanged or none (also `backup: busy`). 3: backup made, a remote step
# failed (code recipient_missing, encrypt_failed, remote_copy_failed, remote_list_failed, remote_prune_failed).
# 4: the backup failed. 2: the settings are refused (before the lock; nothing is run, no record written).
# One line on stdout: `backup: <code or ok> <backup path or ->`. The output of gpg and of rclone is discarded, so
# no credential or configuration text can reach an output or the record.
set -euo pipefail

cfg=${AP_BACKUP_CONFIG-/etc/agent-platform/backup.json}
[ -n "$cfg" ] || { echo "refuse: AP_BACKUP_CONFIG is empty" >&2; exit 2; }
# The settings, validated, one per line: store, state, remote (empty: none), keep, recipient (empty: none).
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
recipient = text("recipient", "/") if "recipient" in d else ""
print(store, state, remote, keep, recipient, sep="\n")
' "$cfg" 2> /dev/null) || { echo "refuse: the settings file is not valid" >&2; exit 2; }
[ "${#cfgv[@]}" -eq 5 ] || { echo "refuse: the settings file is not valid" >&2; exit 2; }
store=${cfgv[0]}; state=${cfgv[1]}; remote=${cfgv[2]}; keep=${cfgv[3]}; recipient=${cfgv[4]}
[ -n "$store" ] && [ -n "$state" ] || { echo "refuse: a path setting is empty" >&2; exit 2; }

mkdir -p -- "$state"
exec 9> "$state/lock"
flock -n 9 || { echo "backup: busy"; exit 0; }
rm -rf -- "$state"/.last.json.* "$state"/.enc.*   # files a killed run left; this run holds the lock

now() { date -u "$1"; }
at=$(now +%Y-%m-%dT%H:%M:%SZ)
backup=""; sha=""; bytes=""; retention=null; remote_state=none; remote_name=""; rsha=""; code=""; deleted=()

# finish <exit status>: remove the working files, write last.json by a rename of a temporary file, print the line, exit.
finish() {
  local tmp
  rm -rf -- "$state"/.enc.*
  tmp=$(mktemp "$state/.last.json.XXXXXX")
  python3 -I -c '
import json, sys
at, store, backup, sha, bytes_, retention, remote, name, rsha, code, *deleted = sys.argv[1:]
print(json.dumps({"at": at, "store": store, "backup": backup or None, "sha256": sha or None,
  "bytes": int(bytes_) if bytes_ else None, "retention_failure": json.loads(retention), "remote": remote,
  "remote_name": name or None, "recipient_sha256": rsha or None, "deleted": deleted, "code": code or None}, indent=1))
' "$at" "$store" "$backup" "$sha" "$bytes" "$retention" "$remote_state" "$remote_name" "$rsha" "$code" "${deleted[@]}" > "$tmp"
  chmod 0644 "$tmp"
  mv -f -- "$tmp" "$state/last.json"
  echo "backup: ${code:-ok} ${backup:--}"
  exit "$1"
}

# The previous digest and recipient digest that reached the remote (read before the record is replaced).
prev=""; prev_rsha=""
if [ -f "$state/last.json" ]; then
  read -r prev prev_rsha <<< "$(python3 -I -c '
import json, sys
try:
    r = json.load(open(sys.argv[1]))
    ok = r["remote"] in ("copied", "unchanged") and isinstance(r["sha256"], str) and isinstance(r.get("recipient_sha256"), str)
    print(r["sha256"], r["recipient_sha256"]) if ok else print("")
except Exception:
    print("")
' "$state/last.json")" || true
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

# From here the remote step; a failure of it leaves the local backup as it is.
remote_state=failed
[ -n "$recipient" ] || { code=recipient_missing; finish 3; }
if [ -f "$recipient" ] && [ -s "$recipient" ]; then
  rsha=$(sha256sum < "$recipient" 2> /dev/null | cut -d' ' -f1) || rsha=""
  [[ "$rsha" =~ ^[0-9a-f]{64}$ ]] || rsha=""
fi
[ -n "$rsha" ] || { code=encrypt_failed; finish 3; }
if [ -n "$prev" ] && [ "$sha" = "$prev" ] && [ "$rsha" = "$prev_rsha" ]; then remote_state=unchanged; finish 0; fi

# Encrypt to the public key of `recipient`, with a GnuPG home of this run: the home of the user is not touched.
command -v gpg > /dev/null 2>&1 || { code=encrypt_failed; finish 3; }
work=$(mktemp -d "$state/.enc.XXXXXX") || { code=encrypt_failed; finish 3; }
mkdir -m 0700 -- "$work/home" || { code=encrypt_failed; finish 3; }
enc=$work/backup.gpg
gpg --homedir "$work/home" --batch --no-tty --quiet --encrypt --recipient-file "$recipient" --output "$enc" -- "$backup" \
  < /dev/null > /dev/null 2>&1 || { code=encrypt_failed; finish 3; }
# The encrypted file must exist, be nonempty and not be an SQLite file, whatever gpg did.
{ [ -f "$enc" ] && [ -s "$enc" ] && [ "$(head -c 16 -- "$enc" | od -An -v -tx1 | tr -d ' \n')" != 53514c69746520666f726d6174203300 ]; } \
  || { code=encrypt_failed; finish 3; }

name=$(now +%Y%m%dT%H%M%SZ)-${sha:0:12}.sqlite.gpg
base=$remote; case "$base" in *:|*/) ;; *) base=$base/ ;; esac
rclone copyto "$enc" "$base$name" > /dev/null 2>&1 || { code=remote_copy_failed; finish 3; }
listing=$(rclone lsf "$remote" 2> /dev/null) || { code=remote_list_failed; finish 3; }
grep -xF -- "$name" <<< "$listing" > /dev/null || { code=remote_list_failed; finish 3; }   # a here-string: no pipe, so no SIGPIPE on a long listing
remote_state=copied; remote_name=$name

# Only names of the two forms this script writes or wrote are candidates; the newest `keep` stay; the new file never goes.
mapfile -t old < <(printf '%s\n' "$listing" | grep -E '^[0-9]{8}T[0-9]{6}Z-[0-9a-f]{12}\.sqlite(\.gpg)?$' | LC_ALL=C sort || true)
excess=$(( ${#old[@]} - keep ))
for (( i = 0; i < excess; i++ )); do
  [ "${old[i]}" != "$name" ] || continue
  if rclone deletefile "$base${old[i]}" > /dev/null 2>&1; then deleted+=("${old[i]}")
  else remote_state=failed; code=remote_prune_failed; finish 3; fi
done
finish 0
