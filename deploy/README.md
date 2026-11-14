# deploy: the timer of `ap deliver`

The files of this directory make `ap deliver` run with no session in the path. The contract of the command, its request, its ends and its records: `crates/ap/README.md`, section "Delivery".

| File | What it is | Where a Lead copies it |
| --- | --- | --- |
| `ap-deliver.service` | One run of `/usr/local/bin/ap deliver` with the host's request on stdin (`Type=oneshot`, in `agents.slice`, which holds the host's CPU and memory limits; `Nice=10`; at most an hour). `SuccessExitStatus=2`: a refusal (exit 2, nothing installed) is no failure of the unit; the exits 3, 4 and 5 fail it | `/etc/systemd/system/ap-deliver.service` |
| `ap-deliver.timer` | Starts the service ten minutes after boot and ten minutes after each run ended | `/etc/systemd/system/ap-deliver.timer` |
| `deliver.json` | The timer's request of this host: `repository`, `url`, `state` (`/var/lib/agent-platform/deliver`) and `build_lock` (the build lock `scripts/install-ap.sh` names). It has no `by` and no `reason`: that is the timer's form | `/etc/agent-platform/deliver.json` |

`ap deliver` installs `ap` and `content/`; it never installs a file of this directory. The service relies on systemd's default search path for `cargo`, `git`, `flock` and `ont` (on the host these files are written for, `/usr/bin/cargo`, the rustup proxy, answers in a system service with only `HOME=/root`); on a host where `cargo` lies elsewhere, add the drop-in `/etc/systemd/system/ap-deliver.service.d/path.conf` with the lines `[Service]` and `Environment=PATH=<the directory of cargo>:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`. A change of `deploy/` reaches the host only when a Lead copies it again as below.

## Enable

From a clean checkout of the merged commit, as root:

```sh
install -m 0644 deploy/ap-deliver.service deploy/ap-deliver.timer /etc/systemd/system/
install -d -m 0755 /etc/agent-platform
install -m 0644 deploy/deliver.json /etc/agent-platform/deliver.json
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

`last.json` is the record of the last call (`end`, `code`, `detail`, `at`, `by`); what each end asks of a Lead is in `crates/ap/README.md`, "What a Lead does after each end".

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

An end `failed` leaves the pair at no single commit, and every later request refuses `pair_differs`. The timer's runs then exit 3 and keep `ap-deliver.service` failed: a pair at no one commit is never repaired by waiting. Each such run replaces `last.json` with its refusal, whose `detail.install` names the failed attempt; the handles to restore (`from`, `rollback_content`, `rollback_binary`) are then that attempt's in the store. With the clone `C=/var/lib/agent-platform/deliver/agent-platform`:

```sh
systemctl stop ap-deliver.timer
systemctl is-active ap-deliver.service       # until it answers inactive or failed
C=/var/lib/agent-platform/deliver/agent-platform
python3 -I - <<'PY'   # from, rollback_content, rollback_binary: of the record, or of the attempt it names
import json, subprocess
r = json.load(open('/var/lib/agent-platform/deliver/last.json'))
if r['end'] != 'failed':
    a = json.loads(subprocess.run(['ont', 'get'], input=json.dumps({'id': r['detail']['install']}), capture_output=True, text=True).stdout)
    r = {k: v.get('value') for k, v in a['result']['objects'][0]['observations'][-1]['fields'].items()}
print({k: r.get(k) for k in ('from', 'rollback_content', 'rollback_binary')})
PY
bash "$C/scripts/install-content.sh" rollback '<rollback_content>'   # when it is not null
bash "$C/scripts/install-ap.sh" rollback '<rollback_binary>'         # when it is not null
ap --version                                                         # names <from>
git -C "$C" show '<from>:content/SHA256SUMS' > /tmp/deliver-sums && (cd /root/.agents && sha256sum -c --quiet /tmp/deliver-sums)
```

When a handle is null, that part cannot go back to `<from>`: bring the pair to the clone's `origin/main` instead (`install-ap.sh` installs only that commit): `T=$(git -C "$C" rev-parse origin/main)`, `git -C "$C" checkout --quiet --force --detach "$T"`, then `CARGO_TARGET_DIR=/var/lib/agent-platform/deliver/target bash "$C/scripts/install-ap.sh" install "$T"` and `bash "$C/scripts/install-content.sh" install "$T"`, and read the pair against `$T` as above. When the attempt is still `running` (an end `unrecorded`), close it by hand as `crates/ap/README.md` says. Then a Lead's install call (above), and `systemctl start ap-deliver.timer`.

## Undo

```sh
systemctl disable --now ap-deliver.timer
systemctl stop ap-deliver.service
rm -f /etc/systemd/system/ap-deliver.service /etc/systemd/system/ap-deliver.timer
systemctl daemon-reload
rm -f /etc/agent-platform/deliver.json
```

The state directory `/var/lib/agent-platform/deliver` (the clone, the build directory, `last.json`) stays; remove it by hand when no call will run again.
