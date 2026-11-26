# deploy: the timer of `ap deliver`

The files of this directory make `ap deliver` run with no session in the path. The contract of the command, its request, its ends and its records: `crates/ap/README.md`, section "Delivery".

| File | What it is | Where it lies on the host |
| --- | --- | --- |
| `ap-deliver.service` | One run of `/usr/local/bin/ap deliver` with the host's request on stdin (`Type=oneshot`, in `agents.slice`, which holds the host's CPU and memory limits; `Nice=10`; at most an hour). `SuccessExitStatus=2`: a refusal (exit 2, nothing installed) is no failure of the unit; the exits 3, 4 and 5 fail it | `/etc/systemd/system/ap-deliver.service` |
| `ap-deliver.timer` | Starts the service ten minutes after boot and ten minutes after each run ended | `/etc/systemd/system/ap-deliver.timer` |
| `deliver.json` | The timer's request of this host: `repository`, `url`, `state` (`/var/lib/agent-platform/deliver`), `build_lock` (the build lock `scripts/install-ap.sh` names), `build_wait_seconds: 480` (wait up to eight minutes for that lock), `engine` (the engine's repository `park-kyungchan/ontology-platform` and its URL: delivery also installs the engine at `ontology/ENGINE-PIN` and a higher version of `ontology/work.schema.json`), `etc` (the operations repository `park-kyungchan/agent-vps-operations` and its URL: delivery also runs that repository's installer of the host's configuration files at its accepted tip, section "The configuration files"), and `pc` (the PC checks account `apcheck@ap-checks`, host `pc`, and target `/home/apcheck/.cargo/bin/ont`: delivery pushes the pinned `ont` binary to the checks account when the PC is reachable, section "The PC checks engine"). It has no `by` and no `reason`: that is the timer's form | `/etc/agent-platform/deliver.json` |

The build lock is one file for every repository of the host, `/var/lib/agent-platform/build.lock`: the `check` of each repository and the installers that build (`scripts/install-ap.sh` here, `scripts/install-ont.sh` of `ontology-platform`) take it. The `check` of `education-platform` also makes its fixture directory beside the lock, so a directory `edu-check.*` under `/var/lib/agent-platform` belongs to a running check of that repository and is removed when it ends.

`ap deliver` installs `ap` and `content/`, with the member `engine` the engine `ont` and the work schema, with the member `etc` the host's configuration files through the operations repository's own installer (section "The configuration files"), and the unit files of this directory (section "The unit files"); `deliver.json` and the drop-ins stay by hand. The service relies on systemd's default search path for `cargo`, `git`, `flock` and `ont` (on the host these files are written for, `/usr/bin/cargo`, the rustup proxy, answers in a system service with only `HOME=/root`); on a host where `cargo` lies elsewhere, add the drop-in `/etc/systemd/system/ap-deliver.service.d/path.conf` with the lines `[Service]` and `Environment=PATH=<the directory of cargo>:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin`.

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

On a host that is up longer than ten minutes, `enable --now` starts the first run at once (`OnBootSec` has passed). Read its end as in "Read the state" below. This first enable on a new host is by hand; from then on each install keeps the two files at the installed commit like every other unit file of this directory (section "The unit files").

## The unit files

`ap deliver` installs the unit files of the commit it installs (`specs/ap/deliver.md` R48 to R51): every `deploy/*.service` and `deploy/*.timer`, read from the clone's objects. There is no list: a unit that adds a timer adds its two files and edits nothing shared. In the call whose read finds the pair at the tip it installed, before the attempt's close, each file whose bytes differ from the host's copy under `/etc/systemd/system` (the variable `AP_UNIT_DIR`) is written through a temporary name with mode 0644; then `systemctl daemon-reload`, `enable --now` of a timer new to the host (and `is-enabled`, which must answer `enabled`), and `try-restart` of a changed timer. A host copy whose bytes no commit of the installed chain had (an edit by hand), a link or a directory is left as it is and reported `host_differs`. A Lead's rollback that ends `installed` restores the files of the commit it goes back to, and removes a file only the undone commit had (a timer after `systemctl disable --now`). Nothing else under the directory is touched: no drop-in `<name>.service.d/`, no unit this directory does not name. Delivery calls no `start`, `stop`, `restart` or `mask`: a timer a Lead stopped or disabled stays so, also when a merge changes its file. The step never changes the call's end, outcome or exit. A step a killed call missed is made up by the next install, and a file a Lead removed is written again and its timer enabled: to keep a timer off, disable it and leave its files in place (each "Undo" below). A `daemon-reload` drops the runtime `CPUQuota` until the next turn of `ap quota` (section "The CPU quota").

Where a Lead reads the result. The record of each call is one line of `journalctl -u ap-deliver.service -o cat` (the service's standard output goes to the journal), with the member `units`: one row per file, `{"name","before","after","result","enabled"}` (the SHA-256 of the host's copy before and after the step, `same`, `written`, `removed`, `host_differs` or `failed`, and for a timer the step enabled whether `is-enabled` answered `enabled`). The code is the property `code` of the attempt in the store, read with `ont get` as "After `failed`" below reads an attempt (the record's `install` is its id); the view `installs` has no column for `code`. It is the first installer's code, else `units_failed`, else `units_host_differs`.

```sh
journalctl -u ap-deliver.service -o cat | grep '"units":\[' | tail -n 1      # the record of the last call that ran the step
python3 -I -c 'import json, sys; print(json.dumps({"id": json.loads(sys.argv[1])}))' '<the record'"'"'s install>' | ont get   # the attempt, its property code
```

`last.json` holds `units` only until the next call replaces it, about ten minutes later, and no exit, detector or status line shows a `units_failed`: read the journal or the store.

- `units_host_differs`: compare the host's copy with the installed commit's file, `git -C /var/lib/agent-platform/deliver/agent-platform show <installed commit>:deploy/<name> | diff -u - /etc/systemd/system/<name>`. Then keep the hand edit as a drop-in `/etc/systemd/system/<name>.d/<file>.conf`, or copy the installed commit's file over it by hand (`git -C /var/lib/agent-platform/deliver/agent-platform show <installed commit>:deploy/<name> > /etc/systemd/system/<name>`, then `systemctl daemon-reload`); after that the next install manages the file.
- `units_failed`: read the row and `systemctl status <name>`. The next install writes again what is not at its tip's bytes.

What stays by hand: the first enable of `ap-deliver.service` and `ap-deliver.timer` on a new host (section "Enable"), `/etc/agent-platform/deliver.json`, `scripts/backup-store.sh` (section "The store's backup"), the drop-ins `<name>.service.d/`, `ap watch --record-login` (section "The watch") and `tailscale serve` (section "The status page").

Each `deploy/*.service` carries the confinement lines of `specs/ap/deliver.md` R52 in `[Service]` (`NoNewPrivileges=yes`, the `Protect*` lines, and `ProtectSystem=full` but in `ap-deliver.service`, which writes `/usr/local/bin` and `/etc/systemd/system`). They reach the host like any change of a unit file. A service that fails under them is repaired by a Lead's rollback call or by a merge, called from a session (section "A Lead's call"): a Lead's call does not run under the unit's lines.

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

With `engine` in the request (`crates/ap/README.md`, "The engine and the schema"), under the rule of a tree that holds `ontology/ENGINE-PIN`, a merged move of the pin is followed by a run that fetches the engine's repository into `/var/lib/agent-platform/deliver/ontology-platform` (a clone with the remote `origin`), checks that every merge from the installed engine to the pin was accepted, records the attempt as an `install` object of the target `/usr/local/bin/ont`, runs that clone's `scripts/install-ont.sh install <pin>` and ends; the next run installs `ap` and `content/`. A merged higher version of `ontology/work.schema.json` is followed by a run that takes `ont backup`, installs the schema from the clone of this repository and, when the tip holds an executable `ontology/migrate`, runs it with the store's path and the two versions, guarded by the marker `/var/lib/agent-platform/deliver/migrate.pending`. The engine's build is a cold one: such a run lasts up to the service's hour.

```sh
python3 -I -c 'import json; r = json.load(open("/var/lib/agent-platform/deliver/last.json")); print(r["target"], r["end"], r["code"], r.get("schema"))'
cat /var/lib/agent-platform/deliver/migrate.pending 2>/dev/null   # an owed migration, when the file exists
ont --version                                                      # the engine's revision
```

`schema_install_failed` and `migrate_failed` end a run with exit 4 and keep the service failed. A marker that cannot clear: stop the timer, run the corrected `ontology/migrate` by hand with the store's path and the marker's two versions, read the result, remove the marker, start the timer.

Undo: remove the member `engine` from `/etc/agent-platform/deliver.json`; delivery then stops on a difference of the engine or the schema as before (`engine_differs`, `schema_differs`) and reads no marker. An installed engine goes back with the engine's own rollback, with the timer stopped: `bash /var/lib/agent-platform/deliver/ontology-platform/scripts/install-ont.sh rollback` (an engine of the tree rule: "An engine change"). A schema is repaired by a higher version, installed forward; delivery never restores the store from a backup.

## Roles

Each service that runs `ap` or applies to the store has one line `Environment=ONT_ROLE=<role>` (`specs/ap/roles.md` R4): `ap-work` `dispatch`; `ap-deliver` `deliver`; `ap-watch` `watch`; `ap-route` and `ap-clause` `route`; `ap-resource`, `ap-quota`, `ap-status-page` and `ap-measure` `measure`; `ap-api-nightly` and `ap-api-weekly` `api`; `ont-backup.service` and `ont-serve.service` have none (`ont serve` authenticates remote peers over Tailscale and never inherits server `ONT_ROLE`). It is the role of the service's function calls and of a program it runs that is not `ap`; `ap` names its own role in each apply (R3). The roles of the grants are those of `deploy/roles.json`, `{"principals":{"ts:park-kyungchan@github":["lead"],"uid:0":[...]}}`, the file the engine reads beside the store (`/var/lib/ontology-platform/ont/roles.json`, mode 0644): without it every claimed role is refused `role_not_admitted` (`roles_file_absent`) and every apply with no role `not_granted`.

A call of `ap deliver` with `engine` installs a moving engine before running the roles step (`specs/ap/deliver.md` R42, R43, R58), so forward-only `roles.json` forms (such as `ts:` principals) wait until the new engine is installed and never brick an older engine; in that engine-move call the record's `roles` is `null` (R59). When the engine already equals `ontology/ENGINE-PIN`, `ap deliver` compares the blob `deploy/roles.json` of the tip with the host's file after the marker and before the schema and the pair (R58): equal bytes are `same`, a differing regular file is kept beside it as `roles.json.before-<12 hex>` and the tip's bytes are written (`written`), no blob at the tip is `absent`, and an entry that is not a regular file, or any failure, ends the call `roles_install_failed` (exit 4, `detail` `{"path"}`) with neither schema nor pair installed. The record's member `roles` is `{"before", "after", "result"}`. A change of the host's roles file by hand is put back only by the next merge's delivery (deliver.md R39, R58): the call that ends `current` does not look at it. No rollback of it is built: from the kept copy by hand (`install -m 0644 roles.json.before-<12 hex> roles.json` in that directory, read back by `sha256sum`).

Read the state of a delivery's roles step:

```sh
python3 -I -c 'import json; r = json.load(open("/var/lib/agent-platform/deliver/last.json")); print(r["end"], r["code"], r.get("roles"))'
```

## The first delivery of roles

The `ap` installed before this merge sends no role and puts no roles file, and the schema of this merge refuses an apply with no role, so the first delivery is the Lead's, by hand, in this order:

1. Stop every timer of `deploy/` but `ont-backup.timer` (`systemctl stop ap-work.timer ap-deliver.timer ap-watch.timer ap-route.timer ap-clause.timer ap-resource.timer ap-quota.timer ap-status-page.timer ap-measure.timer ap-api-nightly.timer ap-api-weekly.timer`), and read that no service of them runs.
2. Install `deploy/roles.json` of the merge at `/var/lib/ontology-platform/ont/roles.json` (owner root, mode 0644: `git -C <clone> show <merge>:deploy/roles.json` into a temporary name of that directory, then `mv`), and read it back by `sha256sum` against the blob.
3. Run `ONT_ROLE=deliver ap deliver < /etc/agent-platform/deliver.json` twice back to back, as the delivery of the schema 21 did (deliver.md R42 to R44): the first call installs the engine of the pin (its record's `target` is `/usr/local/bin/ont`), the second, at once after it, the schema and the pair; its record names the pair at the merge, `schema` `{"from": 21, "to": 22, ...}` with `migrate` `ok` and `roles` `same`. The `ap` before the merge sends no role in its applies; the engine takes the role of the environment when the body has none (engine action.md R12), so `ONT_ROLE=deliver` makes the applies of the second call after the schema step (`start_install`, `close_install`) claim `deliver`, which the grant holds. If the second call ends `store_refused` with `not_granted` or `role_not_admitted`, read the record and put the file back (point 2) before the call is run again.
4. Start the timers stopped in point 1, and read `systemctl list-timers` and the next pass of each.

The undo: the schema is not lowered (deliver.md R40). A role refused for want of the file (`role_not_admitted`, `roles_file_absent`) comes back when the file is put back: point 2 again.

## An engine change

The engine is built, checked and delivered from `engine/` of the same commit as everything else (`specs/ap/deliver.md` R66 to R70, `specs/ap/check.md` R25, R26). This is the rule of a tree without `ontology/ENGINE-PIN`; while a tree still holds that file, the section "The engine of a pin" below is its rule, unchanged. The engine's identity is the Git tree id of `engine/` at the commit, which its `--version` prints as `ont <version> <id>` (`engine/crates/ont/build.rs`: `ONT_TREE_ID`, else Git at the top level when `engine/` is unmodified, else `unknown`). A merge that does not touch `engine/` keeps the id, so nothing is rebuilt.

Made: change `engine/` in the pull request that needs it, with the schema and `ap` when they follow; there is no pin to move. `engine/` is a protected tree (`specs/ap/unit-run.md` R36): a unit that changes it is of class `contract` or `effect`, since delivery installs its build as root.

Checked: `./check` reads the id (`git ls-tree HEAD -- engine`) and takes the engine in this order (`ci/engine-pin.sh --tree`): a file `<dir>/ont-<id>`; the `ont` on PATH, else `<dir>/ont`, when its `--version` names the id (so the host's delivered engine and the PC account's pushed engine serve every worktree without a build); else one build of `engine/` into `${CARGO_TARGET_DIR:-$PWD/target}/engine-tree/build`, after which `./check` also runs `engine/check`. A modified `engine/` is always built, prints `unknown`, and the build of `ap` stops `engine_differs` until the change is committed: commit, then run `./check` again. In CI each job restores the engine cached by the id (or builds it once) and puts it on PATH; the job `test` runs `engine/check` when the change touches `engine/`. `ap check` runs a call on the PC only when the PC's `ont` names the worktree's id and `engine/` is unmodified (else `pc_engine_differs`).

Delivered: with `engine` in the request, the first call after the merge finds the installed engine's id is not T's id of `engine/`, installs the engine from this repository's clone at T (`engine/scripts/install-ont.sh install <T>`: `git archive <T> engine`, built with `ONT_TREE_ID`, its `--version` read back; no second clone, no fetch, no ancestor test: the review that accepted T accepted its engine) and ends; the next call installs `ap` and `content/` of the same T. Two records:

```sh
python3 -I -c 'import json; r = json.load(open("/var/lib/agent-platform/deliver/last.json")); print(r["target"], r["end"], r["to"], r["install"])'   # /usr/local/bin/ont installed <id> ..., then /usr/local/bin/ap installed <T> ...
git -C /var/lib/agent-platform/deliver/agent-platform rev-parse <T>:engine                                                                   # <id>
ont --version                                                                                                                                 # ont <version> <id>
```

The engine attempt names this repository and the id as its `commit`. Rollback of the engine, with the timer stopped: `bash /var/lib/agent-platform/deliver/agent-platform/engine/scripts/install-ont.sh rollback` (it restores the binary installed before, whichever rule installed it). Without `engine` in the request delivery stops `engine_differs`; a tree with neither the pin nor `engine/` is refused `engine_missing`. The PC: delivery's push (R69) sends the host's engine once a local attempt installed it at the id; the user's own account runs `scripts/pc-pull.sh`, which builds `engine/` of the pulled commit (R70) and skips the build when its `ont` already names the id.

## The engine of a pin

This section is the rule of an older tree: one that holds `ontology/ENGINE-PIN`. A tree without it follows "An engine change" above.

A stage of `ap run` holds no credential for `park-kyungchan/ontology-platform` (`specs/ap/unit-run.md` R38) and sees `/usr` read-only (`specs/ap/run-limits.md` R16), so `./check` in a stage cannot build the engine of a pin.

The Lead's step, for each unit whose branch moves `ontology/ENGINE-PIN` to a commit `<pin>` of ontology-platform's main: before that unit's first stage (`ap unit run`) and outside any stage, build `ont` from a clean checkout at `<pin>` (its HEAD is the pin and nothing is changed, so that its `--version` names the pin; the target directory lies outside the checkout, as `.github/workflows/rust.yml` builds it) with `cargo build --locked --release -p ont` under the build lock `/var/lib/agent-platform/build.lock`, install it as `/usr/local/bin/ont-<pin>` and read it back:

```sh
git -C <checkout> rev-parse HEAD                      # <pin>
git -C <checkout> status --porcelain                  # prints nothing
flock /var/lib/agent-platform/build.lock sh -c 'cd <checkout> && cargo build --locked --release -p ont --target-dir <target>'
install -m 0755 -o root -g root <target>/release/ont /usr/local/bin/ont-<pin>
/usr/local/bin/ont-<pin> --version                    # ont 0.1.0 <pin>
```

What `./check` then does (`ci/engine-pin.sh`, called before the Rust build with the pin), in this order: when `<dir>/ont-<pin>` is an executable file whose `--version` prints `ont 0.1.0 <pin>`, that file is the engine, and the line `check: engine <dir>/ont-<pin> (pinned)` says so; else the `ont` on PATH when it names the pin (`(path)`); else the base's `cargo install` of the pin into `${CARGO_TARGET_DIR:-$PWD/target}/engine-pin`, whose output is `engine-pin/install.log` (`(built)`). `<dir>` is `CHECK_ENGINE_DIR` when it is set and not empty, else `/usr/local/bin`. The steps after it take the engine as a link `engine-pin/pinned/ont` to the pinned file: some tests (`crates/ap/tests/route_engine.rs`, `clause.rs`, `resource.rs`) put the directory of `ONT_BIN` first on PATH so that `ap` reaches the engine by the name `ont`, and the directory of `/usr/local/bin/ont-<pin>` holds the host's installed engine, another revision. When none of the three holds, as in a stage whose Lead did not do the step (its build has no credential and fails), `./check` ends with exit 3 and the one line `engine_missing@ci/engine-pin.sh: <file> is not the engine of the pin, nor is ont on PATH, and the build of the pin failed (<install.log>); see deploy/README.md, "The engine of a pin"`; the network error stays in `install.log`. On a CI runner the engine is the one `rust.yml` puts on PATH.

The order in delivery: with `engine` in the request, a merged move of the pin is installed as `/usr/local/bin/ont` by a call that ends there, and the `ap` and `content/` of that tip are installed only by the next call (`specs/ap/deliver.md` R42, R43); without `engine` delivery stops `engine_differs` (R24) and installs neither. So delivery installs the engine of a pin before the `ap` that pins it, and `/usr/local/bin/ont-<pin>` serves the stages before the merge. Old `ont-<pin>` files are not removed by any of this.

## The configuration files

With the member `etc` (`specs/ap/deliver.md` R53 to R57) the host's configuration files follow the accepted tip of `main` of the operations repository `park-kyungchan/agent-vps-operations`; today that is `/etc/agent-platform/policy.json`, the policy of models and accounts that `ap run` enforces. A change of the policy is a merged unit of that repository, and the next run of the timer installs it. In each call that ends `current` or `installed` and is no rollback, after every other step and before the checkouts, delivery:

1. makes or reuses its clone `/var/lib/agent-platform/deliver/agent-vps-operations` with the remote `origin` at `etc.url`, and fetches `main` (every call that reaches the step fetches the operations repository);
2. judges the merges from the clone's `HEAD` (F) up to the fetched tip (E) by the rule of the pair's chain: each a merge whose tree is its second parent's, with an accepting packet review of that head for the repository `park-kyungchan/agent-vps-operations`, and the report of the tip's review on disk with its digest; a new clone judges E alone; E equal to F judges nothing;
3. checks the clone out at E, and only then runs that repository's installer from it: `bash scripts/install-etc.sh check <E>`, and unless that answered `same` for an E already checked out, `install <E>` and `check <E>` again. The installer checks the accepted merges itself, runs `ap policy check` on the new bytes before it writes, keeps the replaced file under `/var/backups/agent-platform-etc/` and writes only its own target.

Delivery writes no configuration file itself, adds nothing to the installer's environment, passes no `--past` and never calls its `rollback`. The step changes no end, code or exit of the call.

Add the member to the host's request once, after delivery installed an `ap` that knows it (`ap deliver check` refuses `etc` with `invalid_input` before). This is not the command of "Enable": that one replaces `/etc/agent-platform/deliver.json` whole, and the host's request holds members `deploy/deliver.json` does not (`checkouts`, which delivery and the watch's family `checkout` need). From a clean checkout of the merged commit, as root, the block below changes only the member `etc`, keeps the present file as `deliver.json.before-etc`, checks the new one, installs it with mode 0644 and prints the member names of the installed file:

```sh
python3 -I -c 'import json, sys; r = json.load(open(sys.argv[1])); r["etc"] = json.load(open(sys.argv[2]))["etc"]; open(sys.argv[3], "w").write(json.dumps(r, indent=2) + "\n")' \
  /etc/agent-platform/deliver.json deploy/deliver.json /etc/agent-platform/deliver.json.new
ap deliver check < /etc/agent-platform/deliver.json.new &&
  cp -p /etc/agent-platform/deliver.json /etc/agent-platform/deliver.json.before-etc &&
  install -m 0644 /etc/agent-platform/deliver.json.new /etc/agent-platform/deliver.json &&
  rm /etc/agent-platform/deliver.json.new
python3 -I -c 'import json; print(sorted(json.load(open("/etc/agent-platform/deliver.json"))))'
```

Where a Lead reads the result: the member `etc` of the record, one line of `journalctl -u ap-deliver.service -o cat` (`last.json` until the next call), `{"repository","from","tip","result","code","detail"}`, and its rows `etc-check` and `etc` of `installers` (exit and code of each run). The installer's kept copies lie under `/var/backups/agent-platform-etc/`; a Lead puts one back by hand, with the timer stopped, through the installer's `rollback <kept file>`.

```sh
journalctl -u ap-deliver.service -o cat | grep '"etc":{' | tail -n 1        # the record of the last call that ran the step
```

What follows each result, read with its `code`:

- `installed`: the tip is on the host and read back by the installer's own `check`.
- `same`: the host already was at the tip. With the code `not_forward` it is the normal answer of the first call of a clone new to delivery when the installer's own record already names the tip.
- `differs`: the host's file is not at the tip; only `code` tells the states apart. `not_forward`: a hand edit of the host's file while the tip has not moved; `target_changed`: a hand edit when the tip has moved; in both the installer refuses to overwrite the file: compare the edit with the tip's file, then make the change a unit of the operations repository or put the kept copy back. `verify_failed`: `ap policy check` refuses the tip's bytes, on every call until a merge repairs them. `not_accepted` as a code: the installer's own accepted check, which walks from its own record and also wants the review's pull request number to be the merge's.
- `not_accepted` as the result: delivery's own judgement, with no program run and the clone's `HEAD` where it was; review the commit `detail` names. For a commit on the chain that cannot be reviewed, the hand path, with the timer stopped: run the installer by hand from a clean checkout of the operations repository at the tip, `bash scripts/install-etc.sh install <tip> --past <commit>:<reason>`, then remove delivery's clone of the operations repository (`rm -rf /var/lib/agent-platform/deliver/agent-vps-operations`), so that the next call judges the tip alone; then start the timer.
- `fetch_failed`: the clone could not be made, fetched, read or checked out; the next call tries again.
- `failed`: the first `check` refused or did not start (its code, `busy` for the installer's lock), the last `check` did not answer, or the store could not be read (`store_unreadable`); the next call tries again.

What the step leaves and what holds it back. `install` keeps one more copy under `/var/backups/agent-platform-etc/` for every tip new to the clone, also when no byte under `etc/` changed, and nothing prunes them. The step runs only after an end `current` or `installed`: a refusal of agent-platform's own delivery (`not_accepted`, `suppressed`, `held_by_rollback`, `pair_differs`, `build_busy`) holds the configuration back too; nothing is fetched then, so no `drift` finding of the configuration files opens, and the finding `end ...` of the detector `delivery` is the signal. A hand edit of the host's file is a `drift` finding of the watch within one pass (section "The watch").

Undo: remove only the member `etc` from the host's request, a copy kept and `ap deliver check` first; then the clone may be removed. The configuration files stay as the installer last wrote them.

```sh
python3 -I -c 'import json, sys; r = json.load(open(sys.argv[1])); r.pop("etc", None); open(sys.argv[2], "w").write(json.dumps(r, indent=2) + "\n")' \
  /etc/agent-platform/deliver.json /etc/agent-platform/deliver.json.new
ap deliver check < /etc/agent-platform/deliver.json.new &&
  cp -p /etc/agent-platform/deliver.json /etc/agent-platform/deliver.json.with-etc &&
  install -m 0644 /etc/agent-platform/deliver.json.new /etc/agent-platform/deliver.json &&
  rm /etc/agent-platform/deliver.json.new
python3 -I -c 'import json; print(sorted(json.load(open("/etc/agent-platform/deliver.json"))))'
rm -rf /var/lib/agent-platform/deliver/agent-vps-operations
```

## The PC checks engine

With the member `pc` (`specs/ap/deliver.md` R60 to R63), after every call that ends `current` or `installed` and is not a rollback, delivery pushes the host's installed `ont` binary over `tailscale ssh` to the PC checks account (`apcheck@ap-checks`, and nowhere else on the PC: `ap` refuses any SSH user other than `apcheck`), recording one `install` object on host `pc` with digest and `--version` readback and automatic rollback to `.before-<12 hex>`. An offline PC ends the `pc` step `unreachable` and never changes the VPS call's `end`, `code` or exit.

Add the member `pc` to `/etc/agent-platform/deliver.json` once, after delivery installed an `ap` that knows it (keeping existing members such as `checkouts`):

```sh
python3 -I -c 'import json, sys; r = json.load(open(sys.argv[1])); r["pc"] = json.load(open(sys.argv[2]))["pc"]; open(sys.argv[3], "w").write(json.dumps(r, indent=2) + "\n")' \
  /etc/agent-platform/deliver.json deploy/deliver.json /etc/agent-platform/deliver.json.new
ap deliver check < /etc/agent-platform/deliver.json.new &&
  cp -p /etc/agent-platform/deliver.json /etc/agent-platform/deliver.json.before-pc &&
  install -m 0644 /etc/agent-platform/deliver.json.new /etc/agent-platform/deliver.json &&
  rm /etc/agent-platform/deliver.json.new
python3 -I -c 'import json; print(sorted(json.load(open("/etc/agent-platform/deliver.json"))))'
```

Read the state in `last.json` or the journal (`r.get("pc")`: `host`, `target`, `from`, `to`, `sha256`, `install`, `result`, `code`). Undo: remove the `pc` member from `/etc/agent-platform/deliver.json` after checking with `ap deliver check`.

The user's own account on the PC is never pushed to from the VPS; the user updates their own `ap` and `ont` binaries by running `scripts/pc-pull.sh` directly on the PC (`specs/ap/deliver.md` R64).

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
named = r.get('install') if r['end'] == 'failed' else (r.get('detail') or {}).get('install')
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
print({k: r.get(k) for k in ('kind', 'commit', 'from', 'rollback_content', 'rollback_binary')})
PY
git -C "$C" checkout --quiet --force --detach '<commit>'             # the scripts of the failed attempt's commit
bash "$C/scripts/install-content.sh" rollback '<rollback_content>'   # when it is not null
bash "$C/scripts/install-ap.sh" rollback '<rollback_binary>'         # when it is not null
ap --version                                                         # names <from>
git -C "$C" show '<from>:content/SHA256SUMS' > /tmp/deliver-sums && (cd /root/.agents && sha256sum -c --quiet /tmp/deliver-sums)
```

The three commands a Lead runs by hand from the clone (`install-content.sh rollback`, `install-ap.sh rollback`, and for a null handle the forward install below) all run from one tree, the `commit` of the failed attempt, after the `git checkout` above. For a forward attempt that is the tree whose installers made the kept copies and whose `rollback-check` passed the tarball (`specs/ap/deliver.md` R18), the tree the command's own rollback of step 10 runs from (R29), and the store holds its accepting review (R25; `start_install` refuses a forward attempt without one); for a rollback attempt it is a commit that was installed before; and the forward install needs `HEAD` at the commit it installs (`scripts/install-content.sh` line 65). No command of this file runs a script from a tree at a commit that no attempt of the store names.

A recorded `rollback_content` holds the tree of `from` and is a tarball `scripts/install-content.sh rollback` takes back: the command checked the archive against the manifest of `from`, and asked `install-content.sh rollback-check` of it, before it recorded it (`crates/ap/README.md`, "Delivery"), and records none when no tarball does. When a handle is null, that part cannot go back to `<from>`: bring the pair forward to `<commit>` instead, from the same tree; the forward path never reads `origin/main` by hand. `scripts/install-ap.sh` installs only the commit the clone's `origin/main` names (lines 237 to 243) and `scripts/install-content.sh` tests that the commit is an ancestor of it (line 67): when `git -C "$C" rev-parse refs/remotes/origin/main` does not print `<commit>`, first `git -C "$C" update-ref refs/remotes/origin/main '<commit>'` (the walk does the same, `scripts/test-deliver.sh` line 236; the next call's fetch moves the ref forward again). Then `CARGO_TARGET_DIR=/var/lib/agent-platform/deliver/target bash "$C/scripts/install-ap.sh" install '<commit>'` and `bash "$C/scripts/install-content.sh" install '<commit>'`, and read the pair against `<commit>` as above. When the attempt is still `running` (an end `unrecorded`), close it by hand as `crates/ap/README.md` says. Then a Lead's install call (above), and `systemctl start ap-deliver.timer`.

## Undo

Disable the timer, and wait until a run that is going on has ended; the files stay in place (a Lead's install writes an absent unit file again and enables its timer). A run stopped while it waits for the build lock has started no attempt and leaves the pair and store unchanged; the next call needs no repair of that wait. Do not stop a run while it installs: a killed call leaves an attempt `running` that the next call has to settle.

```sh
systemctl disable --now ap-deliver.timer
systemctl is-active ap-deliver.service       # until it answers inactive or failed, not activating
```

The unit files, `/etc/agent-platform/deliver.json` and the state directory `/var/lib/agent-platform/deliver` (the clone, the build directory, `last.json`) stay. A merge that changes `ap-deliver.timer` while it is disabled leaves it disabled (`try-restart` starts nothing).

## The store's backup

`scripts/backup-store.sh` backs up the Ontology store with `ont backup` (a verified copy under the store's `backup/` directory; the engine keeps the newest three) and copies each new backup to an `rclone` remote, encrypted: the copy that goes to the remote is the backup encrypted with `gpg` to the public key of `recipient`, and the plain backup never leaves the host. `ont-backup.timer` starts `ont-backup.service` every hour (`Persistent=true`: a run missed while the host was off happens at the next boot). The service is a `Type=oneshot` in `agents.slice` that runs the installed script; `SuccessExitStatus` is not set, so a failed remote copy shows as a failed unit.

`ap deliver` installs `ont-backup.service` and `ont-backup.timer` and enables the timer (section "The unit files"). The script stays by hand. Before the install, one more hand step, the key: make the key pair off the host (on the PC) and bring only the exported public key to the host, to the path `recipient` names (for example `/etc/agent-platform/backup-recipient.pub`); the private key is never on the host. Read it back there: `gpg --show-keys <file>` lists one key that can encrypt (a `pub` line and an `[E]` subkey, or a primary key whose usage includes `E`). Add `recipient` to `/etc/agent-platform/backup.json` before the new script is installed, else every run ends `recipient_missing` (exit 3; the local backup is still made). Then install the script, from a clean checkout of the merged commit, as root, before the timer's first run and again after a merge changes it:

```sh
install -d -m 0755 /usr/local/lib/agent-platform
install -m 0755 scripts/backup-store.sh /usr/local/lib/agent-platform/backup-store.sh
```

The settings are the JSON file `/etc/agent-platform/backup.json` (another path: the variable `AP_BACKUP_CONFIG`). A missing file means the defaults and no remote; an invalid file ends the run with exit 2 before anything is done. Members, all optional:

| Member | Meaning | Default |
| --- | --- | --- |
| `store` | the store's file (an absolute path) | `/var/lib/ontology-platform/ont/ontology.sqlite` |
| `state` | a directory (absolute) for the lock and `last.json` | `/var/lib/agent-platform/backup` |
| `remote` | an `rclone` path `name:dir/sub` (letters, digits, `_`, `.`, `-`, `/`; no `..`); absent or `""` means no remote. The `rclone` configuration of root names `name` | none |
| `keep` | how many files stay on the remote, 1 to 60 | 14 |
| `recipient` | an absolute path (no control character) of a file that holds exactly one OpenPGP public key, binary or armored, as `gpg --recipient-file` takes it; needed for a remote, and a value that is not such a path is refused (exit 2) | none |

Each run writes `<state>/last.json` (`at`, `store`, `backup`, `sha256`, `bytes`, `retention_failure`, `remote`, `remote_name`, `recipient_sha256`, `deleted`, `code`) by a rename and prints one line `backup: <code or ok> <backup path or ->`. `sha256` and `bytes` are those of the plain local backup; `recipient_sha256` is the SHA-256 of the recipient file, null when the run did not read it. The exit is 0 (backup made, remote `copied`, `unchanged` or `none`; also `backup: busy` when a run holds the lock), 3 (backup made, a remote step failed: `code` is `recipient_missing` (a remote and no `recipient`), `encrypt_failed` (the recipient file is absent, empty or not a regular file, `gpg` is not on the path or fails, or its output is absent, empty or an SQLite file), `remote_copy_failed`, `remote_list_failed` or `remote_prune_failed`; the local backup stays) or 4 (`backup_failed`: nothing is copied or deleted). A run with a remote makes one `gpg` public-key encryption in a GnuPG home of its own under the state directory, so the GnuPG home of root is neither read nor written; no passphrase and no private key is involved. A backup is not copied again (`remote: unchanged`) only when the previous record is `copied` or `unchanged` with the same `sha256` and the same `recipient_sha256`: a record of the earlier script has no `recipient_sha256`, so the first run after the install, and the first run after a change of the key, copy even an unchanged store. The file on the remote is named `<YYYYMMDDTHHMMSSZ>-<12 hex of the digest of the plain backup>.sqlite.gpg`. The script deletes from the remote only names of that form and of the earlier plain form (`.sqlite`), oldest first, beyond `keep` of both together, so the plain copies the earlier script wrote leave once `keep` encrypted copies are newer; any other name stays. The output of `gpg`, like that of `rclone`, is discarded, so a journal line never holds credentials. The working files (the encrypted file and the `gpg` home, names that begin with `.`) are removed at the end of each run, and a run removes those a killed run left. Read the state:

```sh
systemctl list-timers ont-backup.timer
systemctl is-failed ont-backup.service       # failed: the last run exited 3 or 4
python3 -m json.tool /var/lib/agent-platform/backup/last.json
```

There is no restore command. A remote file is first decrypted off the host (on the PC, with the private key): `gpg --output <copy> --decrypt <file>`; the SHA-256 of the copy begins with the 12 hex digits of the file's name; then `ont --store <copy> describe` reads it (the engine admits one uid, so run it as root on a scratch copy). What is not offered: the encryption keeps what the remote holds from being read, but the credential that writes the remote can also delete its files, so a root process of the host can delete the copies; a remote with a credential that writes without delete, or a retention rule of its own, is a choice the user makes separately.

Undo: disable the timer and wait until a run that is going on has ended; the unit files stay in place.

```sh
systemctl disable --now ont-backup.timer
systemctl is-active ont-backup.service       # until it answers inactive or failed, not activating
```

The script, `/etc/agent-platform/backup.json`, the public key file, the state directory and the files already on the remote stay; remove the last three by hand when no run will happen again.

## The CPU quota

`ap quota` (`specs/ap/quota.md`) is one turn: it reads the host's busy share and the slice's own use for ten seconds, and sets the slice's runtime `CPUQuota` to what the busy limit leaves after the load outside the slice, held between a floor and a ceiling. `ap-quota.service` (`Type=oneshot`, run of `/usr/local/bin/ap quota`, no `Slice=`, so the quota does not throttle it) is one turn; `ap-quota.timer` starts the first one a minute after boot and each next one twenty seconds after the last ended, so a turn comes about every 30 seconds. Each turn is a new process of the installed `ap`: a delivery of `ap` reaches the next turn with no restart. The answer of each turn is one line in the journal.

The policy file (`/etc/agent-platform/policy.json`, or the path in `AP_POLICY_FILE`) names the slice in its member `quota`; without the member the turn answers `{"skipped":"no quota"}` and does nothing:

```json
{"quota": {"slice": "agents.slice", "busy_limit": 85, "margin": 5, "ceiling": 170, "floor": 60}}
```

Only `slice` is required. The values are per cent: `busy_limit` of all CPUs, `margin` of all CPUs kept under it, `ceiling` and `floor` of one CPU (the `ceiling` is the value the slice's persistent setting holds). `ap run` does not read the member.

`ap deliver` installs `ap-quota.service` and `ap-quota.timer` and enables the timer (section "The unit files"); the slice's own persistent `CPUQuota` stays as it is.

Read the state:

```sh
systemctl list-timers ap-quota.timer
systemctl is-failed ap-quota.service                  # failed: the last turn exited 2 or 3
journalctl -u ap-quota.service -n 5 -o cat            # one answer per turn, or its error
systemctl show -p CPUQuotaPerSecUSec agents.slice     # equals the newest answer's quota (600ms is 60 %, 1.700000s is 170 %)
```

A turn that fails leaves the last value in place and the timer starts the next one. `reading_failed` with the field `quota_now` is also what a value set by hand outside the forms `ap quota` reads (`1h` for 360,000 %) gives on every turn: clear it with `systemctl set-property --runtime <slice> CPUQuota=<ceiling>%`, and the next turn reads it. A reload of the service manager (`systemctl daemon-reload`, by hand or by `ap deliver` after it wrote or removed a unit file) drops the runtime value until the next turn.

Undo: disable the timer, wait until a turn that is going on has ended, return every slice a policy has named since the install to its ceiling (not only the slice the policy names now: a slice that a later policy renamed away from keeps the last runtime value a turn set on it); the unit files stay in place.

```sh
systemctl disable --now ap-quota.timer
systemctl is-active ap-quota.service                  # until it answers inactive or failed, not activating
systemctl set-property --runtime <slice> CPUQuota=<ceiling>%     # once for each slice the policy has named since the install
```

## The dispatch pass

`ap work` (`specs/ap/work.md`) is one pass: under the lock `work.lock` of the units directory it reads the units of the store and the run units of the service manager once, starts each unit that may start now as `ap unit run <key>` in a transient unit `<prefix>-<key>.service` of the policy's slice, and notes on every `ready` unit that does not start what it waits on (`waits_on`). `ap-work.service` (`Type=oneshot`, run of `/usr/local/bin/ap work`, `TimeoutStartSec=300`, no `Slice=`: the pass runs outside the slice it fills) is one pass; `ap-work.timer` starts the first one two minutes after boot and each next one sixty seconds after the last ended. A run the pass starts is a transient unit of its own: the end of the pass's service does not end it, and the next pass sees it in flight. Each pass is a new process of the installed `ap`: a delivery of `ap` reaches the next pass with no restart.

The policy file (`/etc/agent-platform/policy.json`) holds the member `work`; without it the pass answers `{"skipped":"no work"}` and starts nothing:

```json
{"work": {"slots": 3, "slice": "agents.slice", "prefix": "ap-unit"}}
```

Each member is optional: `slots` (1 to 16) is the number of units in flight, `slice` the slice of the runs, `prefix` the beginning of the name of each run's unit. The units directory is the member `units.directory` that `ap unit run` reads; when the policy has `quota` (section "The CPU quota"), a unit starts only while the busy share is at or under `quota.busy_limit` and the load outside the slice leaves it at least `quota.floor`. A `prefix` of `ap` would read the services `ap-work`, `ap-deliver` and `ap-quota` as runs of units named `work`, `deliver` and `quota`: keep the default, or another prefix that no other service of the host begins with.

The service gives a run `HOME=/root` and `PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin` (and `LANG` and `USER` where the service manager sets them); the stages need the CLIs and every program a stage or an acceptance check calls (Node.js for a repository whose check needs it, `cargo`). Where any of them lies outside these directories, add at install the drop-in `/etc/systemd/system/ap-work.service.d/path.conf` with the lines `[Service]` and `Environment=PATH=<the directories of the CLIs and the other programs>:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin`, as for `cargo` above.

`ap deliver` installs `ap-work.service` and `ap-work.timer` and enables the timer (section "The unit files"); without the member `work` in the policy file each pass starts nothing.

Read the state:

```sh
systemctl list-timers ap-work.timer
journalctl -u ap-work.service -n 5 -o cat             # one answer per pass: started, cut, not_restarted, waiting, in_flight
systemctl list-units 'ap-unit-*'                      # the runs in flight
```

A unit listed under `cut` (a `running` unit no run holds) or `not_restarted` (its run ended with the unit as it was, or a pass was killed between its file `started` and the start) is started by no pass: read why with `ap status <key>` and call `ap unit run <key>`.

The switch: remove the member `work` from the policy file. The next pass starts nothing; the runs in flight go on.

Undo: disable the timer and wait until a pass that is going on has ended; the unit files stay in place. The runs in flight go on.

```sh
systemctl disable --now ap-work.timer
systemctl is-active ap-work.service                   # until it answers inactive or failed, not activating
```

## Routes

`ap route index` (`specs/ap/route.md`) writes one record per heading of each `*.md` file at `origin/main` of agent-platform, ontology-platform and agent-vps-operations (after `git fetch origin main` in each clone the policy's `repositories` names; no checkout is read or changed) and of the installed shared instructions under `/root/.agents`, and one per component of each repository's `map.json`, into `/var/lib/agent-platform/route`, which the store's sources `sections` and `components` read (schema version 14), and indexes the two sources. `ont fn route` answers from them. `ap-route.service` (`Type=oneshot`, run of `/usr/local/bin/ap route index --prune` with the three repositories and `--instructions /root/.agents`, in `agents.slice`, `Nice=10`) is one run; `ap-route.timer` starts it every ten minutes (`OnCalendar=*:0/10`, `Persistent=true`). With `--prune`, the files of a repository the service no longer names are removed and their sections retracted. A fetch that fails (also on Git's ref lock while `ap deliver` or `ap unit ready` fetches the same clone) keeps that repository's previous file until the next run; the other way round, their fetch can fail on this run's lock for the seconds it holds it, and they recover on their next run. The education repositories are never named: the command refuses an `education-` name.

`ap deliver` installs `ap-route.service` and `ap-route.timer` and enables the timer (section "The unit files"); a run needs the engine and the schema of version 14, which delivery installs too. For a first run now, not at the next tenth minute, as root: `systemctl start ap-route.service`.

Read the state:

```sh
systemctl list-timers ap-route.timer
journalctl -u ap-route.service -n 1 -o cat            # one line per run: records, appended, renewed, retracted, kept, removed
printf '{"parameters":{"words":"how a schema version is installed"}}' | ont fn route
```

A run that exits 3 (an index that did not hold, a file it could not write) fails the service; the files and the store's objects stay as the last run that held left them, and the function answers `"stale": true` once they are older than an hour.

Undo: disable the timer and wait until a run that is going on has ended; the unit files stay in place. The sections stay in the store, stale after an hour; to retract them, remove the files of `/var/lib/agent-platform/route` and run `ont index --source sections --source components` once.

```sh
systemctl disable --now ap-route.timer
systemctl is-active ap-route.service                  # until it answers inactive or failed, not activating
```

## The watch

`ap watch` (`specs/ap/watch.md`) is one pass: under the lock `pass.lock` of `/var/lib/agent-platform/watch/` the ten detectors read the host, and the pass opens, sees again, reopens and closes findings of the store whose detector is `watch:<name>`; it writes nothing else but its `state.json`, and it stops, starts, restarts and kills nothing:

- `delivery`: reads `/var/lib/agent-platform/deliver/last.json` for age, end state and checkout forwarding.
- `login`: compares the SSH and sudoers login path with `login.json` (the TOTP secret file by `stat` alone).
- `services`: lists failed systemd units and timers that have not fired within three intervals.
- `host`: checks disk free space, `agents.slice` `memory.events` `high` rises, `apt` history and old CLI processes; when the policy names `hosts.pc`, it reads the newest check of the PC of the last 60 minutes in `checks.jsonl` and `door.jsonl` and keeps `pc down` open while that check fell back (`pc_unreachable` or `pc_lost`): the finding asks the user to run `Start-ScheduledTask -TaskName ap-checks-keepalive` on Windows, never restarts it by itself.
- `records`: checks `/var/lib/agent-platform/backup/last.json` and recent store actions for stale local or remote backups.
- `accounts`: checks Claude account `.credentials.json` expiry (only the expiry timestamp is kept) and recent `run` usage limits (`>= 0.9`).
- `checks`: checks GitHub `main` workflow conclusions, open Dependabot alerts and `github/apply --check` settings drift for policy repositories.
- `runtimes`: checks `--version` and `--help` flags of `claude` and `agy` against the flags `ap run` invokes.
- `drift`: compares by SHA-256, without fetching, four sources: the files listed in `deployments/host-artifacts.json` at `origin/main` of `agent-vps-operations`; the unit files of `deploy/` at the commit the installed `ap` names, read from delivery's clone `/var/lib/agent-platform/deliver/agent-platform`, with `/etc/systemd/system/<name>`; the files of `etc/` at the tip delivery's last fetch of the operations repository read (its clone `/var/lib/agent-platform/deliver/agent-vps-operations`), each with the host's file of the same path under `/`; and, when the policy names a valid `apcheck@<node>` in `hosts.pc.ssh` and the PC is reachable, the checks account's `ont` binary (read-only digest probe over `tailscale ssh`, `specs/ap/watch.md` R33) against the host's `/usr/local/bin/ont`.
- `credit`: when the policy has the member `api` (`specs/ap/api.md` R3), reads the runs of role `batch` of the store by the rule `ap api batch` counts with: the month's counted spend at 80 % of a stop (`stop standard`, `stop soak`), a batch run still running more than 26 hours after its start (`unsettled <run>`), and the key file's mode by `stat` alone, never its bytes (`key mode`, urgent, when it is not a regular file of mode 600); without the member it reads nothing and its findings are neither seen nor closed.

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

`ap deliver` installs `ap-watch.service` and `ap-watch.timer` with the `ap` that has the verb and enables the timer (section "The unit files"). The drop-in above and the login record stay by hand.

Read the state:

```sh
systemctl list-timers ap-watch.timer
journalctl -u ap-watch.service -n 5 -o cat            # one answer per pass: detectors, opened, seen, reopened, closed, failed
ap status                                             # the open findings, act and urgent first
```

A finding is triaged by the Lead: `dismiss_finding` with a reason, or a unit. A finding `unreadable` of a detector says which source it could not read; that detector closes nothing until it reads again.

Undo: disable the timer and wait until a pass that is going on has ended; the unit files stay in place. The findings stay in the store; `/var/lib/agent-platform/watch/` may be removed.

```sh
systemctl disable --now ap-watch.timer
systemctl is-active ap-watch.service                  # until it answers inactive or failed, not activating
```

## The API queue

`ap-api-nightly.service` and `ap-api-weekly.service` (`Type=oneshot`, `python3 -I -B` of `evals/batch/queue` in delivery's clone `/var/lib/agent-platform/deliver/agent-platform`, in `agents.slice`, `Nice=10`, `TZ=UTC`, `TimeoutStartSec=26h` because `ap api batch` polls a batch up to 25 hours, `StateDirectory=agent-platform/api` with mode 0700, no `Restart=`) each run one pass of the drift job on the API credit with no session. `ap-api-nightly.timer` starts the nightly pass at 18:00 UTC every day (`--pass nightly --model claude-sonnet-5-5`: only the groups no earlier pass settled); `ap-api-weekly.timer` the weekly pass at 06:00 UTC on Sundays (`--pass weekly --model claude-opus-5-5`: the full pass). Both timers are `Persistent=true`: a time missed while the host was down is made up at boot.

A pass (`evals/batch/README.md`, section "The queue"): it checks that delivery's clone is at the commit K the installed `ap` names (else it writes nothing and answers `not_installed`), copies `evals/batch` of K into its directory `/var/lib/agent-platform/api/drift/<start>-<pass>/`, and runs from that copy the builder, one `ap api batch drift` (`--queue soak` in the last six days of the UTC month, else `standard`) and the reader, which opens at most ten findings of the detector `batch:drift` a pass and counts the rest. The queue computes no amount: the admission, the caps and the stops are `ap api batch`'s. The answer of each pass is one JSON line in the journal: `status`, `queue`, `commit`, `dir` and per step `exit` and `answer`.

A refused or failed pass exits 1 and fails its service, so the detector `services` of `ap watch` reports `failed ap-api-nightly.service` (or the weekly one) until the next pass succeeds. Two refusals of `ap api batch` belong here too: `over_max_requests` (the build asks more groups than `api.jobs.drift.max_requests` of the policy) and `budget_exceeded` (a cap or the month's stop would be passed); the cause is `batch.answer` of the journal's line. A pass whose batch is submitted keeps its worst case reserved against drift's cap, the day's cap and the stop until it settles, so a pass that starts while another one polls can be refused `budget_exceeded`. There is no retry before the next timer.

`ap deliver` installs the four unit files and enables both timers (section "The unit files"). The policy's member `api`, the key file and `repositories` are the host's configuration; nothing else is by hand.

Read the state:

```sh
systemctl list-timers ap-api-nightly.timer ap-api-weekly.timer
journalctl -u ap-api-nightly.service -n 1 -o cat      # the answer of the last nightly pass
journalctl -u ap-api-weekly.service -n 1 -o cat       # the answer of the last weekly pass
echo '{"parameters":{"month":"YYYY-MM"}}' | ont fn api_spend   # the month's spend by job, as the admission counts it
ap status                                             # the findings of batch:drift and watch:credit
```

Undo: disable both timers; the unit files stay in place. A pass that is already polling goes on and keeps its batch; one whose run never settles (its process stopped, a poll timeout) counts at its reservation and is reported by the detector `credit` as `unsettled` 26 hours after its start, and its close is the Lead's, by hand (`specs/ap/api.md`, "Not included"). The pass directories under `/var/lib/agent-platform/api/drift/` stay as records.

```sh
systemctl disable --now ap-api-nightly.timer ap-api-weekly.timer
systemctl is-active ap-api-nightly.service ap-api-weekly.service   # inactive or failed once no pass runs
```

## The status page

`ap-status-page.service` (`Type=oneshot`, `Nice=10`, `Environment=HOME=/root`, `TimeoutStartSec=1290`, `StateDirectory=agent-platform/status-page` mode `0755`, no `Restart=`) runs `/usr/local/bin/ap status --page` into `/var/lib/agent-platform/status-page/status.txt.new`, removes that file and exits 1 when `ap` fails, and otherwise renames it over `/var/lib/agent-platform/status-page/status.txt`. Nothing else is written, so a failed run keeps the previous page and a reader never sees a half-written file. `ap-status-page.timer` starts it every hour at minute 33 (`OnCalendar=*:33`, `AccuracySec=10s`, `Persistent=false`: a run missed while the host was down is not made up at boot). Each run is a new process of the installed `ap`, so a delivery of `ap` reaches the next run with no restart. The page is the lines of `ap status --text` and, after them, each open intent's brief, flow and week's cost, the flow, cost and month's API spend of all, and the hosts, machines and accounts with their capacity (`specs/ap/status.md` R47 to R53); the `ap status --page` run asks no step after 600 seconds, so `TimeoutStartSec=1290` is those 600 seconds, the 630 seconds of one `ont fn` begun just before them, and 60 to print and rename. A run longer than an hour delays the next one: a timer that elapses while its service is still active leaves it running and starts no second run (systemd.timer(5)). A run that passes `TimeoutStartSec` (a brief or the rows of the flow begun before the bound and slow past it) is killed and leaves the previous page, whose first line's `at` shows its age.

`ap deliver` installs `ap-status-page.service` and `ap-status-page.timer` with an `ap` that has `ap status --page` and enables the timer (section "The unit files"). For a first run now, and to read it, as root:

```sh
systemctl start ap-status-page.service
systemctl is-failed ap-status-page.service
stat -c '%y' /var/lib/agent-platform/status-page/status.txt
head -n 1 /var/lib/agent-platform/status-page/status.txt
```

Read `stat -c '%y'` and the `status <time>` first line again after the next timer run (`systemctl list-timers ap-status-page.timer`) to verify that the file is rewritten every hour.

Save the present `tailscale serve` configuration first in a subshell with `set -C` (so a second install never replaces the saved file), then publish `/var/lib/agent-platform/status-page/status.txt` and read it back:

```sh
(set -C; tailscale serve status --json > /var/lib/agent-platform/status-page/serve.before.json)
tailscale serve --bg --yes /var/lib/agent-platform/status-page/status.txt
tailscale serve status
curl -sS -D - https://<host>.<tailnet>.ts.net/
```

The `curl` response answers with `Content-Type: text/plain; charset=utf-8` and the current `status <time>` line. Every device that the tailnet's access policy lets reach this host on port 443 can read the page; it is never offered with `tailscale funnel`.

Undo: reset the serve configuration, replay one `tailscale serve --bg` per handler listed in `/var/lib/agent-platform/status-page/serve.before.json` (none when it is `{}`) with `--https=<port>` for an `HTTPS` port, `--http=<port>` for an `HTTP` port, `--tcp=<port>` for a TCP forward, and `--set-path=<mount>`, compare `tailscale serve status --json` against the saved file with `diff`, then disable the timer; the unit files stay in place:

```sh
tailscale serve reset
# Replay any handlers from /var/lib/agent-platform/status-page/serve.before.json here when it is not `{}`.
tailscale serve status --json | diff -u /var/lib/agent-platform/status-page/serve.before.json -
systemctl disable --now ap-status-page.timer
systemctl is-active ap-status-page.service            # until it answers inactive or failed, not activating
```

`/var/lib/agent-platform/status-page/` (`status.txt` and `serve.before.json`) stays; remove it by hand when no longer needed.

## The nightly measures

`ap-measure.timer` fires at 00:20 UTC and `ap-measure.service` runs three steps (`specs/ap/measure.md` R16): `ap measure sessions`, `ap measure kpis` and `ap measure routing` (`specs/ap/routing.md` R9 to R13). The first two have the `-` prefix, so that a failed measurement does not keep the routing pass from running. The routing pass applies nothing without the policy's member `routing`; with it, it opens, sees again, reopens or closes the findings `routing-bar-<runtime>-<model>-<kind>` of the detector `ap-routing`, which hold a tier off a kind of work at `ap unit add`, `amend`, `ready` and in the dispatch pass.

```sh
journalctl -u ap-measure.service -o cat | tail -n 3   # the last line of each pass
```

Undo of the routing step: remove the member `routing` from the policy file (nothing is then refused, held or applied for a tier); a bar is lifted by the Lead's `close_finding` or, for good, `dismiss_finding`.

## The tailnet Ontology server (ont-serve)

`ont-serve.service` runs `/usr/local/bin/ont serve --bind 100.95.133.99:7432` as a long-lived `Type=simple` service (`Restart=on-failure`, `RestartSec=5`, `Nice=10`, `Environment=TZ=UTC`, all eleven R52 confinement lines, and no `Environment=ONT_ROLE=` or `ONT_AGENT=`: `ont serve` authenticates remote peers over Tailscale and never inherits server `ONT_ROLE`).

### First delivery of pin-srv

Because the `ap` installed before `pin-srv` runs `roles_step` before `engine_install`, letting it deliver `pin-srv` directly would write `ts:park-kyungchan@github` into `roles.json` before the new `ont` is installed. Before or right after merging `pin-srv`, stop the timers and install `ont` `3a1eda9f95d731ab3a43ba9afa7336e965c0c04b` first, then run `ap deliver`:

```sh
systemctl stop ap-deliver.timer ap-work.timer
systemctl is-active ap-deliver.service       # must be inactive or failed, not activating
bash /var/lib/agent-platform/deliver/ontology-platform/scripts/install-ont.sh install 3a1eda9f95d731ab3a43ba9afa7336e965c0c04b
ont --version                                # ont 0.1.0 3a1eda9f95d731ab3a43ba9afa7336e965c0c04b
ap deliver < /etc/agent-platform/deliver.json
systemctl start ap-deliver.timer ap-work.timer
```

With the new `ont` already at the pin, `ap deliver` installs `deploy/roles.json`, the new `ap` (which runs `engine_install` before `roles_step` on all future pin bumps), and `/etc/systemd/system/ont-serve.service`.

### Enable, readback, restart, and undo

`ap deliver` writes `/etc/systemd/system/ont-serve.service` and runs `daemon-reload`, but R50 leaves `.service` activation to the Lead. Enable and start it once, and verify it listens only on the tailnet address:

```sh
systemctl enable --now ont-serve.service
systemctl is-active ont-serve.service        # active
ss -H -ltn '( sport = :7432 )'               # 100.95.133.99:7432 only (never 0.0.0.0 or ::)
```

After any future engine upgrade of `/usr/local/bin/ont`, restart the long-lived daemon so it runs the new binary:

```sh
systemctl restart ont-serve.service
```

Undo:

```sh
systemctl disable --now ont-serve.service
```

### Tailnet ACL grant (user step)

In the Tailscale tailnet policy, admit `park-kyungchan@github` to `tag:vps` on TCP port `7432` only (never `tag:ap-checks`, `tag:ap-stages`, or `*`).

Readback from the user's PC (`kcpalantir-wsl`):

```sh
tailscale nc 100.95.133.99 7432
curl -fsS http://100.95.133.99:7432/healthz
# {"service":"ont-serve","status":"ok","version":"3a1eda9f95d731ab3a43ba9afa7336e965c0c04b"}
```

Readback from `ap-checks` (must fail with a non-zero exit):

```sh
wsl -d ap-checks --exec tailscale nc -w 3 100.95.133.99 7432
```

Undo: remove the TCP 7432 grant from the Tailscale ACL and verify `tailscale nc 100.95.133.99 7432` / `curl -fsS http://100.95.133.99:7432/healthz` from `kcpalantir-wsl` fails.
