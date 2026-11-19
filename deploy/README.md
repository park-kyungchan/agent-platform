# deploy: the timer of `ap deliver`

The files of this directory make `ap deliver` run with no session in the path. The contract of the command, its request, its ends and its records: `crates/ap/README.md`, section "Delivery".

| File | What it is | Where a Lead copies it |
| --- | --- | --- |
| `ap-deliver.service` | One run of `/usr/local/bin/ap deliver` with the host's request on stdin (`Type=oneshot`, in `agents.slice`, which holds the host's CPU and memory limits; `Nice=10`; at most an hour). `SuccessExitStatus=2`: a refusal (exit 2, nothing installed) is no failure of the unit; the exits 3, 4 and 5 fail it | `/etc/systemd/system/ap-deliver.service` |
| `ap-deliver.timer` | Starts the service ten minutes after boot and ten minutes after each run ended | `/etc/systemd/system/ap-deliver.timer` |
| `deliver.json` | The timer's request of this host: `repository`, `url`, `state` (`/var/lib/agent-platform/deliver`), `build_lock` (the build lock `scripts/install-ap.sh` names), `build_wait_seconds: 480` (wait up to eight minutes for that lock), and `engine` (the engine's repository `park-kyungchan/ontology-platform` and its URL: delivery also installs the engine at `ontology/ENGINE-PIN` and a higher version of `ontology/work.schema.json`). It has no `by` and no `reason`: that is the timer's form | `/etc/agent-platform/deliver.json` |

The build lock is one file for every repository of the host, `/var/lib/agent-platform/build.lock`: the `check` of each repository and the installers that build (`scripts/install-ap.sh` here, `scripts/install-ont.sh` of `ontology-platform`) take it. The `check` of `education-platform` also makes its fixture directory beside the lock, so a directory `edu-check.*` under `/var/lib/agent-platform` belongs to a running check of that repository and is removed when it ends.

`ap deliver` installs `ap` and `content/`, and with the member `engine` the engine `ont` and the work schema; it never installs a file of this directory. The service relies on systemd's default search path for `cargo`, `git`, `flock` and `ont` (on the host these files are written for, `/usr/bin/cargo`, the rustup proxy, answers in a system service with only `HOME=/root`); on a host where `cargo` lies elsewhere, add the drop-in `/etc/systemd/system/ap-deliver.service.d/path.conf` with the lines `[Service]` and `Environment=PATH=<the directory of cargo>:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`. A change of `deploy/` reaches the host only when a Lead copies it again as below.

## Enable

From a clean checkout of the merged commit, as root:

```sh
install -m 0644 deploy/ap-deliver.service deploy/ap-deliver.timer /etc/systemd/system/
install -d -m 0755 /etc/agent-platform
ap deliver check < deploy/deliver.json && install -m 0644 deploy/deliver.json /etc/agent-platform/deliver.json
systemctl daemon-reload
systemd-analyze verify /etc/systemd/system/ap-deliver.service /etc/systemd/system/ap-deliver.timer
systemctl enable --now ap-deliver.timer
systemctl is-enabled ap-deliver.timer
```

On a host that is up longer than ten minutes, `enable --now` starts the first run at once (`OnBootSec` has passed). Read its end as in "Read the state" below.

## Read the state

```sh
systemctl is-enabled ap-deliver.timer        # enabled: the timer runs
systemctl is-failed ap-deliver.service       # failed: the last run exited 3, 4 or 5
systemctl list-timers ap-deliver.timer       # the next run
python3 -m json.tool /var/lib/agent-platform/deliver/last.json
printf '{"view":"installs","args":{}}' | ont render   # the first row is the newest attempt
```

`journalctl -u ap-deliver.service` shows a waiting run by its stderr JSON line `{"at":"<UTC RFC 3339, to the second>","waiting":"build_lock","seconds":480}`. The request's `build_wait_seconds` bounds that wait; a free lock prints no waiting line. The run keeps the deliver lock while waiting, so another call refuses `busy`. The final answer still goes to stdout as one line.

A run that exited 3 because of `attempt_running` (an attempt is `running` that a call could not settle: a dead rollback at neither commit, a clone that cannot answer, a close the store did not apply) keeps the service failed until a call settled that attempt; a Lead's own request ends the same way with exit 2.

`last.json` is the record of the last call (`end`, `code`, `detail`, `at`, `by`); what each end asks of a Lead is in `crates/ap/README.md`, "What a Lead does after each end".

## The engine and the schema

With `engine` in the request (`crates/ap/README.md`, "The engine and the schema"), a merged move of `ontology/ENGINE-PIN` is followed by a run that fetches the engine's repository into `/var/lib/agent-platform/deliver/ontology-platform` (a clone with the remote `origin`), checks that every merge from the installed engine to the pin was accepted, records the attempt as an `install` object of the target `/usr/local/bin/ont`, runs that clone's `scripts/install-ont.sh install <pin>` and ends; the next run installs `ap` and `content/`. A merged higher version of `ontology/work.schema.json` is followed by a run that takes `ont backup`, installs the schema from the clone of this repository and, when the tip holds an executable `ontology/migrate`, runs it with the store's path and the two versions, guarded by the marker `/var/lib/agent-platform/deliver/migrate.pending`. The engine's build is a cold one: such a run lasts up to the service's hour.

```sh
python3 -I -c 'import json; r = json.load(open("/var/lib/agent-platform/deliver/last.json")); print(r["target"], r["end"], r["code"], r.get("schema"))'
cat /var/lib/agent-platform/deliver/migrate.pending 2>/dev/null   # an owed migration, when the file exists
ont --version                                                      # the engine's revision
```

`schema_install_failed` and `migrate_failed` end a run with exit 4 and keep the service failed. A marker that cannot clear: stop the timer, run the corrected `ontology/migrate` by hand with the store's path and the marker's two versions, read the result, remove the marker, start the timer.

Undo: remove the member `engine` from `/etc/agent-platform/deliver.json`; delivery then stops on a difference of the engine or the schema as before (`engine_differs`, `schema_differs`) and reads no marker. An installed engine goes back with the engine's own rollback, with the timer stopped: `bash /var/lib/agent-platform/deliver/ontology-platform/scripts/install-ont.sh rollback`. A schema is repaired by a higher version, installed forward; delivery never restores the store from a backup.

## A Lead's call

A Lead's install and a Lead's rollback take the host's request with `by` and `reason` added (and `rollback: true` for a rollback):

```sh
python3 -I -c 'import json, sys; r = json.load(open("/etc/agent-platform/deliver.json")); r.update(by=sys.argv[1], reason=sys.argv[2]); print(json.dumps(r))' '<session id>' '<reason>' | ap deliver
python3 -I -c 'import json, sys; r = json.load(open("/etc/agent-platform/deliver.json")); r.update(by=sys.argv[1], reason=sys.argv[2], rollback=True); print(json.dumps(r))' '<session id>' '<reason>' | ap deliver
```

After a rollback, and after an end `failed_restored` or `failed`, the timer installs nothing until a Lead's install call ended `installed`.

## An installer by hand

The installer scripts take no deliver lock. Stop the timer BEFORE calling `scripts/install-ap.sh` or `scripts/install-content.sh` by hand, and wait until a run that is going on has ended:

```sh
systemctl stop ap-deliver.timer
systemctl is-active ap-deliver.service       # until it answers inactive or failed, not activating
```

Start it again afterwards: `systemctl start ap-deliver.timer`.

## After `failed`

An end `failed` leaves the pair at no single commit, and every later request refuses `pair_differs`. The timer's runs then exit 3 and keep `ap-deliver.service` failed: a pair at no one commit is never repaired by waiting. Each such run replaces `last.json` with its refusal, whose `detail.install` names the failed attempt (`detail` is null when the call found none: the snippet below then reads the newest attempt of the target from the store, by `started_at`, and says so); the handles to restore (`from`, `rollback_content`, `rollback_binary`) are then that attempt's in the store. With the clone `C=/var/lib/agent-platform/deliver/agent-platform`:

```sh
systemctl stop ap-deliver.timer
systemctl is-active ap-deliver.service       # until it answers inactive or failed
C=/var/lib/agent-platform/deliver/agent-platform
python3 -I - <<'PY'   # from, rollback_content, rollback_binary: of the record, or of the attempt it names
import json, subprocess
def ont(verb, request):
    return json.loads(subprocess.run(['ont', verb], input=json.dumps(request), capture_output=True, text=True).stdout)
r = json.load(open('/var/lib/agent-platform/deliver/last.json'))
if r['end'] != 'failed':
    named = (r.get('detail') or {}).get('install')
    if named:
        a = ont('get', {'id': named})
        r = {k: v.get('value') for k, v in a['result']['objects'][0]['observations'][-1]['fields'].items()}
    else:
        # `detail` is null: no attempt is named, so read the newest attempt of the target from the store.
        found = ont('search', {'object_type': 'install', 'limit': 1000})
        rows = [{k: v.get('value') for k, v in o['head']['fields'].items()} for o in found['result']['objects']]
        rows = [x for x in rows if (x.get('target') or {}).get('native_key') == '/usr/local/bin/ap']
        r = max(rows, key=lambda x: x['started_at'], default={})
        print('detail is null: the newest attempt of the target in the store, started', r.get('started_at'))
print({k: r.get(k) for k in ('from', 'rollback_content', 'rollback_binary')})
PY
bash "$C/scripts/install-content.sh" rollback '<rollback_content>'   # when it is not null
bash "$C/scripts/install-ap.sh" rollback '<rollback_binary>'         # when it is not null
ap --version                                                         # names <from>
git -C "$C" show '<from>:content/SHA256SUMS' > /tmp/deliver-sums && (cd /root/.agents && sha256sum -c --quiet /tmp/deliver-sums)
```

A recorded `rollback_content` holds the tree of `from` and is a tarball `scripts/install-content.sh rollback` takes back: the command checked the archive against the manifest of `from`, and asked `install-content.sh rollback-check` of it, before it recorded it (`crates/ap/README.md`, "Delivery"), and records none when no tarball does. When a handle is null, that part cannot go back to `<from>`: bring the pair to the clone's `origin/main` instead (`install-ap.sh` installs only that commit): `T=$(git -C "$C" rev-parse origin/main)`, `git -C "$C" checkout --quiet --force --detach "$T"`, then `CARGO_TARGET_DIR=/var/lib/agent-platform/deliver/target bash "$C/scripts/install-ap.sh" install "$T"` and `bash "$C/scripts/install-content.sh" install "$T"`, and read the pair against `$T` as above. When the attempt is still `running` (an end `unrecorded`), close it by hand as `crates/ap/README.md` says. Then a Lead's install call (above), and `systemctl start ap-deliver.timer`.

## Undo

Disable the timer, and wait until a run that is going on has ended. A run stopped while it waits for the build lock has started no attempt and leaves the pair and store unchanged; the next call needs no repair of that wait. Do not stop a run while it installs: a killed call leaves an attempt `running` that the next call has to settle.

```sh
systemctl disable --now ap-deliver.timer
systemctl is-active ap-deliver.service       # until it answers inactive or failed, not activating
rm -f /etc/systemd/system/ap-deliver.service /etc/systemd/system/ap-deliver.timer
systemctl daemon-reload
rm -f /etc/agent-platform/deliver.json
```

The state directory `/var/lib/agent-platform/deliver` (the clone, the build directory, `last.json`) stays; remove it by hand when no call will run again.

## The store's backup

`scripts/backup-store.sh` backs up the Ontology store with `ont backup` (a verified copy under the store's `backup/` directory; the engine keeps the newest three) and copies each new backup to an `rclone` remote. `ont-backup.timer` starts `ont-backup.service` every hour (`Persistent=true`: a run missed while the host was off happens at the next boot). The service is a `Type=oneshot` in `agents.slice` that runs the installed script; `SuccessExitStatus` is not set, so a failed remote copy shows as a failed unit.

Install, from a clean checkout of the merged commit, as root:

```sh
install -d -m 0755 /usr/local/lib/agent-platform
install -m 0755 scripts/backup-store.sh /usr/local/lib/agent-platform/backup-store.sh
install -m 0644 deploy/ont-backup.service deploy/ont-backup.timer /etc/systemd/system/
systemctl daemon-reload
systemctl enable --now ont-backup.timer
```

The settings are the JSON file `/etc/agent-platform/backup.json` (another path: the variable `AP_BACKUP_CONFIG`). A missing file means the defaults and no remote; an invalid file ends the run with exit 2 before anything is done. Members, all optional:

| Member | Meaning | Default |
| --- | --- | --- |
| `store` | the store's file (an absolute path) | `/var/lib/ontology-platform/ont/ontology.sqlite` |
| `state` | a directory (absolute) for the lock and `last.json` | `/var/lib/agent-platform/backup` |
| `remote` | an `rclone` path `name:dir/sub` (letters, digits, `_`, `.`, `-`, `/`; no `..`); absent or `""` means no remote. The `rclone` configuration of root names `name` | none |
| `keep` | how many files stay on the remote, 1 to 60 | 14 |

Each run writes `<state>/last.json` (`at`, `store`, `backup`, `sha256`, `bytes`, `retention_failure`, `remote`, `remote_name`, `deleted`, `code`) by a rename and prints one line `backup: <code or ok> <backup path or ->`. The exit is 0 (backup made, remote `copied`, `unchanged` or `none`; also `backup: busy` when a run holds the lock), 3 (backup made, a remote step failed: `code` is `remote_copy_failed`, `remote_list_failed` or `remote_prune_failed`; the local backup stays) or 4 (`backup_failed`: nothing is copied or deleted). A backup whose digest equals the one the previous run copied is not copied again (`remote: unchanged`). The script deletes from the remote only names it wrote (`<YYYYMMDDTHHMMSSZ>-<12 hex of the digest>.sqlite`), oldest first, beyond `keep`; `rclone`'s own output is discarded, so a journal line never holds credentials. Read the state:

```sh
systemctl list-timers ont-backup.timer
systemctl is-failed ont-backup.service       # failed: the last run exited 3 or 4
python3 -m json.tool /var/lib/agent-platform/backup/last.json
```

There is no restore command: a copy of a remote file is read with `ont --store <copy> describe` (the engine admits one uid, so run it as root on a scratch copy).

Undo: disable the timer, wait until a run that is going on has ended, remove the three files.

```sh
systemctl disable --now ont-backup.timer
systemctl is-active ont-backup.service       # until it answers inactive or failed, not activating
rm -f /etc/systemd/system/ont-backup.service /etc/systemd/system/ont-backup.timer /usr/local/lib/agent-platform/backup-store.sh
systemctl daemon-reload
```

`/etc/agent-platform/backup.json`, the state directory and the files already on the remote stay; remove them by hand when no run will happen again.

## The CPU quota

`ap quota` (`specs/ap/quota.md`) is one turn: it reads the host's busy share and the slice's own use for ten seconds, and sets the slice's runtime `CPUQuota` to what the busy limit leaves after the load outside the slice, held between a floor and a ceiling. `ap-quota.service` (`Type=oneshot`, run of `/usr/local/bin/ap quota`, no `Slice=`, so the quota does not throttle it) is one turn; `ap-quota.timer` starts the first one a minute after boot and each next one twenty seconds after the last ended, so a turn comes about every 30 seconds. Each turn is a new process of the installed `ap`: a delivery of `ap` reaches the next turn with no restart. The answer of each turn is one line in the journal.

The policy file (`/etc/agent-platform/policy.json`, or the path in `AP_POLICY_FILE`) names the slice in its member `quota`; without the member the turn answers `{"skipped":"no quota"}` and does nothing:

```json
{"quota": {"slice": "agents.slice", "busy_limit": 85, "margin": 5, "ceiling": 170, "floor": 60}}
```

Only `slice` is required. The values are per cent: `busy_limit` of all CPUs, `margin` of all CPUs kept under it, `ceiling` and `floor` of one CPU (the `ceiling` is the value the slice's persistent setting holds). `ap run` does not read the member.

Install, from a clean checkout of the merged commit, as root (the slice's own persistent `CPUQuota` stays as it is):

```sh
install -m 0644 deploy/ap-quota.service deploy/ap-quota.timer /etc/systemd/system/
systemctl daemon-reload
systemd-analyze verify /etc/systemd/system/ap-quota.service /etc/systemd/system/ap-quota.timer
systemctl enable --now ap-quota.timer
```

Read the state:

```sh
systemctl list-timers ap-quota.timer
systemctl is-failed ap-quota.service                  # failed: the last turn exited 2 or 3
journalctl -u ap-quota.service -n 5 -o cat            # one answer per turn, or its error
systemctl show -p CPUQuotaPerSecUSec agents.slice     # equals the newest answer's quota (600ms is 60 %, 1.700000s is 170 %)
```

A turn that fails leaves the last value in place and the timer starts the next one. `reading_failed` with the field `quota_now` is also what a value set by hand outside the forms `ap quota` reads (`1h` for 360,000 %) gives on every turn: clear it with `systemctl set-property --runtime <slice> CPUQuota=<ceiling>%`, and the next turn reads it. A reload of the service manager (`systemctl daemon-reload`) drops the runtime value until the next turn.

Undo: disable the timer, wait until a turn that is going on has ended, return every slice a policy has named since the install to its ceiling (not only the slice the policy names now: a slice that a later policy renamed away from keeps the last runtime value a turn set on it), remove the two files.

```sh
systemctl disable --now ap-quota.timer
systemctl is-active ap-quota.service                  # until it answers inactive or failed, not activating
systemctl set-property --runtime <slice> CPUQuota=<ceiling>%     # once for each slice the policy has named since the install
rm -f /etc/systemd/system/ap-quota.service /etc/systemd/system/ap-quota.timer
systemctl daemon-reload
```

## The dispatch pass

`ap work` (`specs/ap/work.md`) is one pass: under the lock `work.lock` of the units directory it reads the units of the store and the run units of the service manager once, starts each unit that may start now as `ap unit run <key>` in a transient unit `<prefix>-<key>.service` of the policy's slice, and notes on every `ready` unit that does not start what it waits on (`waits_on`). `ap-work.service` (`Type=oneshot`, run of `/usr/local/bin/ap work`, `TimeoutStartSec=300`, no `Slice=`: the pass runs outside the slice it fills) is one pass; `ap-work.timer` starts the first one two minutes after boot and each next one sixty seconds after the last ended. A run the pass starts is a transient unit of its own: the end of the pass's service does not end it, and the next pass sees it in flight. Each pass is a new process of the installed `ap`: a delivery of `ap` reaches the next pass with no restart.

The policy file (`/etc/agent-platform/policy.json`) holds the member `work`; without it the pass answers `{"skipped":"no work"}` and starts nothing:

```json
{"work": {"slots": 3, "slice": "agents.slice", "prefix": "ap-unit"}}
```

Each member is optional: `slots` (1 to 16) is the number of units in flight, `slice` the slice of the runs, `prefix` the beginning of the name of each run's unit. The units directory is the member `units.directory` that `ap unit run` reads; when the policy has `quota` (section "The CPU quota"), a unit starts only while the busy share is at or under `quota.busy_limit` and the load outside the slice leaves it at least `quota.floor`. A `prefix` of `ap` would read the services `ap-work`, `ap-deliver` and `ap-quota` as runs of units named `work`, `deliver` and `quota`: keep the default, or another prefix that no other service of the host begins with.

The service gives a run `HOME=/root` and `PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin` (and `LANG` and `USER` where the service manager sets them); the stages need the CLIs and every program a stage or an acceptance check calls (Node.js for a repository whose check needs it, `cargo`). Where any of them lies outside these directories, add at install the drop-in `/etc/systemd/system/ap-work.service.d/path.conf` with the lines `[Service]` and `Environment=PATH=<the directories of the CLIs and the other programs>:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin`, as for `cargo` above.

Install, from a clean checkout of the merged commit, as root, after the member `work` is in the policy file:

```sh
install -m 0644 deploy/ap-work.service deploy/ap-work.timer /etc/systemd/system/
systemctl daemon-reload
systemd-analyze verify /etc/systemd/system/ap-work.service /etc/systemd/system/ap-work.timer
systemctl enable --now ap-work.timer
```

Read the state:

```sh
systemctl list-timers ap-work.timer
journalctl -u ap-work.service -n 5 -o cat             # one answer per pass: started, cut, not_restarted, waiting, in_flight
systemctl list-units 'ap-unit-*'                      # the runs in flight
```

A unit listed under `cut` (a `running` unit no run holds) or `not_restarted` (its run ended with the unit as it was, or a pass was killed between its file `started` and the start) is started by no pass: read why with `ap status <key>` and call `ap unit run <key>`.

The switch: remove the member `work` from the policy file. The next pass starts nothing; the runs in flight go on.

Undo: disable the timer, wait until a pass that is going on has ended, remove the two files. The runs in flight go on.

```sh
systemctl disable --now ap-work.timer
systemctl is-active ap-work.service                   # until it answers inactive or failed, not activating
rm -f /etc/systemd/system/ap-work.service /etc/systemd/system/ap-work.timer
systemctl daemon-reload
```

## The watch

`ap watch` (`specs/ap/watch.md`) is one pass: under the lock `pass.lock` of `/var/lib/agent-platform/watch/` the nine detectors read the host, and the pass opens, sees again, reopens and closes findings of the store whose detector is `watch:<name>`; it writes nothing else but its `state.json`, and it stops, starts, restarts and kills nothing:

- `delivery`: reads `/var/lib/agent-platform/deliver/last.json` for age, end state and checkout forwarding.
- `login`: compares the SSH and sudoers login path with `login.json` (the TOTP secret file by `stat` alone).
- `services`: lists failed systemd units and timers that have not fired within three intervals.
- `host`: checks disk free space, `agents.slice` `memory.events` `high` rises, `apt` history and old CLI processes.
- `records`: checks `/var/lib/agent-platform/backup/last.json` and recent store actions for stale local or remote backups.
- `accounts`: checks Claude account `.credentials.json` expiry (only the expiry timestamp is kept) and recent `run` usage limits (`>= 0.9`).
- `checks`: checks GitHub `main` workflow conclusions, open Dependabot alerts and `github/apply --check` settings drift for policy repositories.
- `runtimes`: checks `--version` and `--help` flags of `claude`, `muse` and `agy` against the flags `ap run` invokes.
- `drift`: compares host files listed in `deployments/host-artifacts.json` at `origin/main` of `agent-vps-operations` by SHA-256 without fetching.

`ap-watch.service` (`Type=oneshot`, run of `/usr/local/bin/ap watch`, `Nice=10`, `TZ=UTC` so that `systemctl` prints the times the pass reads in UTC, a `PATH` with `/usr/sbin` for `sshd`, no `Restart=`) is one pass; `ap-watch.timer` starts one at every fifth minute (`OnCalendar=*:0/5`) and makes up no pass missed while the host was down. Each pass is a new process of the installed `ap`: a delivery of `ap` reaches the next pass with no restart. The answer of each pass is one line in the journal.

Because `gh` reads the root user's GitHub sign-in under `HOME` and `claude` and `agy` lie outside the service's default `PATH`, add the drop-in `/etc/systemd/system/ap-watch.service.d/path.conf`:

```ini
[Service]
Environment=HOME=/root
Environment=PATH=<the directories of the CLIs>:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin
```

The detector `login` compares the login path with the reading in `login.json`; until it is recorded every pass keeps one finding `no baseline` open. Record it once after the install, as root, with the path of the TOTP secret file (only its presence is read, by `stat`):

```sh
ap watch --record-login --totp-file <absolute path of the TOTP secret file>
```

After a change of the login path made by the user's procedure, record it again with `--replace`; a recorded reading is never replaced without it.

Install, from a clean checkout of the merged commit, as root, after delivery installed an `ap` that has the verb (`ap --help` lists `watch`):

```sh
install -m 0644 deploy/ap-watch.service deploy/ap-watch.timer /etc/systemd/system/
systemctl daemon-reload
systemd-analyze verify /etc/systemd/system/ap-watch.service /etc/systemd/system/ap-watch.timer
systemctl enable --now ap-watch.timer
```

Read the state:

```sh
systemctl list-timers ap-watch.timer
journalctl -u ap-watch.service -n 5 -o cat            # one answer per pass: detectors, opened, seen, reopened, closed, failed
ap status                                             # the open findings, act and urgent first
```

A finding is triaged by the Lead: `dismiss_finding` with a reason, or a unit. A finding `unreadable` of a detector says which source it could not read; that detector closes nothing until it reads again.

Undo: disable the timer, wait until a pass that is going on has ended, remove the two files. The findings stay in the store; `/var/lib/agent-platform/watch/` may be removed.

```sh
systemctl disable --now ap-watch.timer
systemctl is-active ap-watch.service                  # until it answers inactive or failed, not activating
rm -f /etc/systemd/system/ap-watch.service /etc/systemd/system/ap-watch.timer
systemctl daemon-reload
```
