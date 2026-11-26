# The PC's account for the stages

## What this is

`ap-stages` is a WSL distribution on the PC that exists for the stages of units alone (the stages unit `pc-stage` starts them there from the VPS). It sits beside `ap-checks`, the distribution of the checks (`deployments/pc-account/README.md`, "What this is"), and beside the user's own distribution. Its account holds the Claude Code sign-in that the user places there by hand, and nothing else of the user's.

| Name | What it is |
| --- | --- |
| `ap-stages` | the WSL distribution of the stages, apart from `ap-checks` and the user's own distribution |
| `apstage` | its account, home `/home/apstage`, not root and without sudo |
| `ap-stages` | the distribution's own tailnet node, tagged `tag:ap-stages` |
| `apstage@ap-stages` | the value of `hosts.pc.stages.ssh` in `etc/agent-platform/policy.json` |
| `/home/apstage/work/ap-stages` | the value of `hosts.pc.stages.work`; no step of this README makes or fills it |
| `/home/apstage/.claude` | the one directory of the Claude Code sign-in |

`hosts.pc.ssh` (`apcheck@ap-checks`) and `hosts.pc.work` are unchanged by this README.

Who uses it: the stages unit `pc-stage` starts a stage on the PC from the VPS through `tailscale ssh` to the login `apstage@ap-stages`.

Why a distribution of its own, and not a second account in `ap-checks`:

1. Row E2 of `deployments/pc-account/README.md` ("The tailnet's rule", "The table") refuses every account of `ap-checks` but `apcheck`, and its `ssh` rule names `apcheck` alone. A second account there would change a contract that the user and the Lead read back on 2026-11-22.
2. In one distribution, a process of `apcheck` outside a check's sandbox (a `tailscale ssh` session, the step that fetches the dependencies) would share the stage account's filesystem, `/tmp` and process list, and the node's own `tailscaled`, through which `tailscale nc` runs as `apcheck` (row E5). The sign-in would sit one file mode away from the checks' processes.
3. A node and a tag of its own let the tailnet's rule, the admin console and an undo name the stage account apart from the checks.

The cost: a second root file system, a third `tailscaled` with its own port, and a second keeper.

Who acts. Every step on the PC is the user's. The Lead's steps all run on the VPS, and they are this list and nothing else:

- **M1**: `ap policy check /etc/agent-platform/policy.json` and the forms of `scripts/install-etc.sh` (`install`, `check`, `rollback`) after delivery.
- **M2**: the tailnet steps T1 and T2 of "The tailnet's rule": the policy file written through the Tailscale API with the OAuth client the user gave the Lead, whose secret no command prints.
- **M3**: from the VPS, S1, S2, `tailscale ping ap-stages` and `tailscale status`.
- **M4**: the real run of "The real run".
- **M5**: at each move of agent-platform's engine pin (`ontology/ENGINE-PIN`), the copy of the VPS's newly installed `/usr/local/bin/ont` to `/home/apstage/.cargo/bin/ont`, exactly as L5 of `deployments/pc-account/README.md` ("What this is") with `apstage@ap-stages` in place of `apcheck@ap-checks`: the same command, its readback and its undo, each written with `apstage@ap-stages`. The readback is `tailscale ssh apstage@ap-stages '. ~/.profile; ont --version'`, which prints `ont 0.1.0 <pin>`.

Nothing in this repository installs anything on the PC. A merge of this README or of the policy changes nothing on the host or on the PC until the Lead runs `scripts/install-etc.sh install <merge>`.

### How a command is written

Every command run from Windows has the form `wsl -d ap-stages --exec <program> <arguments>`, as in `deployments/pc-account/README.md` ("How a command is written") with `ap-stages`. Without `--exec`, the login shell expands quotes and `$?`. That form runs as `apstage`, and a step that needs root adds `--user root` before `--exec`. The commands are written for PowerShell. Where the expected answer is an exit status, the user reads `$LASTEXITCODE` straight after the command. Before relying on that, the user checks that the form passes the status on: `wsl -d ap-stages --exec false`, then `$LASTEXITCODE` prints `1`. Where it does not, the user stops and tells the Lead.

Every statement below about WSL, Tailscale or Claude Code that this README does not cite to a page is a readback: it comes with its expected answer, and where the answer differs the user stops and tells the Lead. No step is shown done by a success message alone.

The pages cited here are the ones of `deployments/pc-account/README.md`:
- Tailscale: `https://tailscale.com/kb/1112/userspace-networking`, `https://tailscale.com/kb/1241/tailscale-up`, `https://tailscale.com/kb/1278/tailscaled`, `https://tailscale.com/kb/1080/cli`, `https://tailscale.com/kb/1068/tags`, `https://tailscale.com/kb/1337/policy-syntax`, `https://tailscale.com/kb/1324/grants`, `https://tailscale.com/docs/features/tailscale-ssh`.
- Microsoft: `https://learn.microsoft.com/en-us/windows/wsl/wsl-config`, `https://learn.microsoft.com/en-us/windows/wsl/systemd`.

Placeholders: `<user-login>` (the user's tailnet login), `<user-account>` and `<user-node>` (the user's own account and node on the PC), `<vps-node>` (the VPS's tailnet node), `<stage-port>` (a UDP port, "The node").

## The distribution

The user prepares it, and reads every row again at step 1 of "The order". One row per property, with its read command and the expected answer, as "The distribution" of `deployments/pc-account/README.md` with `ap-stages` and `apstage` in place of `ap-checks` and `apcheck`.

| Property | Read command | Expected answer |
| --- | --- | --- |
| systemd on, automount off, interop off, default user | `wsl -d ap-stages --exec cat /etc/wsl.conf` | `[boot]` `systemd=true`; `[automount]` `enabled=false`; `[interop]` `enabled=false` and `appendWindowsPath=false`; `[user]` `default=apstage` |
| no Windows drive mounted | `wsl -d ap-stages --exec grep -c drvfs /proc/mounts` | `0` (and exit 1) |
| `apstage` is uid 1000 and in neither group `sudo` nor `adm` | `wsl -d ap-stages --exec id apstage` | `uid=1000(apstage)`; the groups name neither `sudo` nor `adm` |
| no sudo | `wsl -d ap-stages --exec sudo -n true` | refused, `$LASTEXITCODE` `1` |
| password locked | `wsl -d ap-stages --user root --exec passwd -S apstage` | second field `L` |
| Rust 1.100.0 from rustup on the `PATH` through `~/.profile`, rustup's default | `wsl -d ap-stages --exec sh -c '. ~/.profile; rustup default; rustc --version; cargo --version; cargo clippy --version; rustfmt --version'` | `rustup default` names `1.100.0-<target>` as default; `rustc` and `cargo` print `1.100.0`; clippy and rustfmt each print a version line and no error |
| `bwrap` runs a command in a new user namespace | `wsl -d ap-stages --exec bwrap --unshare-user --uid 0 --gid 0 --ro-bind / / id -u` | `0` |
| the programs a stage calls | `wsl -d ap-stages --exec bash -c 'command -v bash git tar flock timeout nice setsid find python3 bwrap'` | ten paths, one per name |
| the engine, read through `~/.profile` | `wsl -d ap-stages --exec sh -c '. ~/.profile; command -v ont; ont --version'` | `/home/apstage/.cargo/bin/ont`, then `ont 0.1.0 <pin>`. The pin is agent-platform's `ontology/ENGINE-PIN` at main, which the VPS's `/usr/local/bin/ont` equals. Step 1 of "The order" places this file |
| glibc for the VPS's dynamically linked `ont` | `wsl -d ap-stages --exec ldd --version` | first line ends with `2.39` or a later version. Where `ont` does not start, the user stops and tells the Lead |
| Claude Code installed for `apstage` with Anthropic's own installer | `wsl -d ap-stages --exec sh -c '. ~/.profile; command -v claude; claude --version'` | a path under `/home/apstage`, then a version line |
| no Windows program starts | the user copies a Windows program (for example `whoami.exe`) into `\\wsl.localhost\ap-stages\home\apstage\`, runs `wsl -d ap-stages --exec /home/apstage/whoami.exe`, then removes the copy | an error and a non-zero `$LASTEXITCODE`; nothing of Windows prints |

Where a stage of `pc-stage`'s real run fails for a missing program, its output names it, the user installs it, and the run is repeated. No credential is ever put into the account to install anything.

## The sign-in

The user's step, and only after rows S1 to S5 of "The tailnet's rule" pass (step 5 of "The order"), so that the account is shown closed before it holds the credential. The sign-in is a claude.ai sign-in of the CLI, `apstage`'s own: nothing is copied from the user's own `~/.claude` (no settings, MCP server, plugin or hook).

1. **The directory first** (the user). The CLI makes its directory with mode 0755, so the user makes it before the login, as `apstage`: `wsl -d ap-stages --exec install -d -m 0700 /home/apstage/.claude`. Readback: `wsl -d ap-stages --exec stat -c '%U %a' /home/apstage/.claude` prints `apstage 700`. Undo: `wsl -d ap-stages --exec rmdir /home/apstage/.claude` (it works only while the directory is empty).
2. **The login** (the user). `wsl -d ap-stages --exec sh -c '. ~/.profile; CLAUDE_CONFIG_DIR=/home/apstage/.claude claude auth login'` prints a link. The user opens it in their own browser and signs in with the claude.ai account they choose. No code, token or link goes into a chat, a message or a file of a repository.
3. **The readbacks** (the user):
   - `wsl -d ap-stages --exec sh -c '. ~/.profile; CLAUDE_CONFIG_DIR=/home/apstage/.claude claude auth status --json'` shows `loggedIn` true, `authMethod` `claude.ai` and `configDirectory` `/home/apstage/.claude`. The user reads it on the PC: it also names the account's e-mail, which goes into no message.
   - `wsl -d ap-stages --exec stat -c '%U %a' /home/apstage/.claude` prints `apstage 700`, and `wsl -d ap-stages --exec stat -c '%U %a' /home/apstage/.claude/.credentials.json` prints `apstage 600`.
   - Nothing else of the user's is there: `wsl -d ap-stages --exec ls -A /home/apstage/.ssh /home/apstage/.git-credentials /home/apstage/.netrc /home/apstage/.config/gh` answers `cannot access` for each of the four; `wsl -d ap-stages --exec git config --global --get-regexp credential` prints nothing.

No command of this README prints, copies or reads the content of `.credentials.json`: `stat` and `claude auth status` are the only reads.

Undo: `wsl -d ap-stages --exec sh -c '. ~/.profile; CLAUDE_CONFIG_DIR=/home/apstage/.claude claude auth logout'`, read back by the `claude auth status --json` above showing `loggedIn` false.

## What the three distributions share

`ap-stages`, `ap-checks` and the user's own distribution run in one WSL virtual machine. The four rows of "What the two distributions share" of `deployments/pc-account/README.md` hold for the three. Each read command is run in all three distributions (`wsl -d <name> --exec ...`):

| Shared | What follows | What the steps do about it | Read command (in each distribution) |
| --- | --- | --- | --- |
| One kernel | Root in `ap-stages` is root on the kernel that the other two run on | `apstage` is not root and has no sudo, and no step gives it root | `uname -r` gives the same answer in all three; `id apstage` and `sudo -n true` as in "The distribution" |
| The mount `/mnt/wsl` | All three see it | Nothing of the user's or of `apcheck` is placed there, and nothing reaches the account through it | `ls -la /mnt/wsl` shows the same listing in all three, with no file of the user's or of `apcheck` |
| One network | The interfaces, the loopback and its ports are the same in all three | The third `tailscaled` runs in userspace-networking mode with a UDP port of its own ("The node"); see the first item of "Not closed by this" | `ip -br addr` gives the same lines in all three |
| One virtual machine's memory | A stage's memory comes out of the same machine as the user's own work and the checks'. No `memory` line is added to `.wslconfig`: the user decided at 2026-11-21 02:49Z not to cap the machine's memory | none | PowerShell: `Select-String -Path $env:USERPROFILE\.wslconfig -Pattern '^\s*memory'` prints nothing (or the file does not exist) |

## The node

The user's steps that make `ap-stages` a tailnet node of its own, beside the user's node and `ap-checks` in the one network. They come after T1 of "The tailnet's rule" (the tag exists before a node can take it, `https://tailscale.com/kb/1068/tags`), in this order. Every command is a proposal with its readback; the pages cited above show the flags, but not that Tailscale SSH answers on a userspace-networking node beside two other `tailscaled`, nor how the package and its unit are set up here. When a readback differs, the user stops and tells the Lead. These steps reuse "The node" of `deployments/pc-account/README.md` with `ap-stages`.

1. **The package** (the user). Tailscale is installed in `ap-stages` from Tailscale's own package for the distribution, as root (`--user root`). Readback: `wsl -d ap-stages --exec tailscale version` prints a version. Undo: the package removed as root.
2. **The daemon's mode** (the user). The `tailscaled` of `ap-stages` runs with `--tun=userspace-networking`, so that it does not take the interface of the user's `tailscaled`. It runs with `--port=<stage-port>`, a UDP port other than the ones the user's `tailscaled` and the one of `ap-checks` use (`https://tailscale.com/kb/1278/tailscaled`), and with no proxy flag (neither `--socks5-server` nor `--outbound-http-proxy-listen`). Proposal: where the package's unit reads `/etc/default/tailscaled`, set `PORT="<stage-port>"` and `FLAGS="--tun=userspace-networking"` there as root, then `wsl -d ap-stages --user root --exec systemctl restart tailscaled`. Readbacks:
   - `ps -o args= -C tailscaled`, run in each of the three distributions, shows that distribution's own daemon; the three ports differ.
   - `wsl -d ap-stages --exec ps -o args= -C tailscaled` prints one line containing `--tun=userspace-networking` and `--port=<stage-port>`, with neither proxy flag.
   - `wsl -d ap-stages --exec systemctl is-active tailscaled` prints `active`.
   - `ss -H -uln` in each distribution lists the three ports, each once.

   Undo: the previous `/etc/default/tailscaled`, and the same restart.
3. **The join** (the user). `wsl -d ap-stages --user root --exec tailscale up --hostname=ap-stages --ssh --advertise-tags=tag:ap-stages` prints an interactive sign-in link. The user opens it in their own browser and signs in. No auth key is made, and the link and a key go into no chat, no message and no file of a repository. Readbacks:
   - `wsl -d ap-stages --exec tailscale status --self=true --peers=false` names `ap-stages` with the owner `tagged-devices`.
   - The admin console lists the machine `ap-stages` with `tag:ap-stages`.
   - `ap-checks` and the user's node are still online (`tailscale status` in the user's own distribution, and `wsl -d ap-checks --exec tailscale status --self=true --peers=false`).

   Undo: `wsl -d ap-stages --user root --exec tailscale logout`, then the machine removed in the admin console.

`tailscale ping ap-stages` on the VPS is read only after T2.

## The tailnet's rule

The Lead's step M2, in two saves. The tailnet policy file is written through the Tailscale API (`https://tailscale.com/api`) with the OAuth client the user gave the Lead. Its secret is a root-only file, `/etc/agent-platform/credentials/tailscale-oauth` (mode 0600), which a command reads into a header without printing it, for example by passing the header on standard input (`printf 'Authorization: Bearer %s\n' "$token" | curl -H @- ...`, where `printf` is a shell builtin and the token is in no command line); no command prints the secret or the token, and neither goes into a message or a file of a repository. The endpoints and the header forms are a proposal with a readback: the read of the saved file back decides. Where an answer differs, the Lead stops and tells the user.

Each save:

1. **A copy first.** The current policy file is read with its `ETag` and kept under `/var/backups/tailscale/` as `policy.before-<UTC stamp>.hujson`, the stamp read from the clock.
2. **The save, with `If-Match`.** The save is sent with `If-Match` set to that `ETag`, and it is refused on a mismatch: a console edit made between the copy and the save is never overwritten. The tailnet's own check of the policy runs on the save.
3. **The read back.** The saved file is read back through the API, and what differs from the copy is exactly the change below.

**T1, before the join** (Lead): `tagOwners` gains `"tag:ap-stages": ["<user-login>"]`, and nothing else changes. Readback: the saved file's `tagOwners` holds the new key and the rest equals the copy; the admin console lists the tag, for example in a machine's "Edit tags" list (the user). Undo: the copy written back.

**T2, after the join** (Lead): the fragment below, and nothing else changes. Syntax: `https://tailscale.com/kb/1337/policy-syntax`, `https://tailscale.com/kb/1324/grants` and `https://tailscale.com/docs/features/tailscale-ssh`.

```jsonc
{
  "grants": [
    // S1: the VPS reaches the node ap-stages on port 22 and nothing else of it.
    {"src": ["tag:vps"], "dst": ["tag:ap-stages"], "ip": ["tcp:22"]}
  ],
  "ssh": [
    // S1 and S2: the account apstage alone; never root and never autogroup:nonroot.
    {"action": "accept", "src": ["tag:vps"], "dst": ["tag:ap-stages"], "users": ["apstage"]}
  ],
  "sshTests": [
    // S1 and S2
    {"src": "tag:vps", "dst": ["tag:ap-stages"], "accept": ["apstage"], "deny": ["root", "<user-account>"]}
  ]
}
```

No grant, `acls` rule or `ssh` rule has `tag:ap-stages` as a source, and none names `root` or `autogroup:nonroot` for it: S3 and S4 follow from what the fragment leaves out. The grant is `tcp:22` alone. Undo: T2's kept copy written back, then T1's.

The save's own check of the policy, then the table's commands, decide. Where the admin console refuses an entry's form, the Lead tells the user, and the table's commands still decide.

### The table (the contract)

One row per direction, with its test command and the expected result. A refused row's command runs under `timeout` and is expected to exit non-zero, 124 included. A `tailscale ping` that answers proves nothing about a refused row. A `tailscale nc` joins its standard input and output to the connection, so an accepted connection whose input stays open also ends in 124 at the `timeout`: a refused `tailscale nc` row therefore also needs an empty standard output, no line at all, and in particular no line beginning `SSH-` (the banner an sshd sends first). A non-zero exit with any output on standard output means the connection was accepted: the row is broken, and the user stops and tells the Lead (as "The table" of `deployments/pc-account/README.md`).

| Row | Direction | Expected | Test (who runs it) |
| --- | --- | --- | --- |
| S1 | the VPS to `apstage@ap-stages` by Tailscale SSH | accepted, without a sign-in prompt | `timeout 30 tailscale ssh apstage@ap-stages true` on the VPS exits 0 (Lead, M3) |
| S2 | the VPS to `root@ap-stages`, or to any other account there | refused | `timeout 30 tailscale ssh root@ap-stages true` on the VPS exits non-zero (Lead, M3) |
| S3 | the node `ap-stages` to the VPS, any port | refused: no rule names `tag:ap-stages` as a source | `wsl -d ap-stages --user root --exec timeout 10 tailscale nc <vps-node> 22`, and the same without `--user root`: both non-zero, both with nothing on standard output (no line beginning `SSH-`) (the user) |
| S4 | between the nodes `ap-stages` and `ap-checks`, both ways | refused | `wsl -d ap-stages --exec timeout 10 tailscale nc ap-checks 22` and `wsl -d ap-checks --exec timeout 10 tailscale nc ap-stages 22`: each non-zero with nothing on standard output (the user) |
| S5 | the contract of `ap-checks` and of the rest of the tailnet | unchanged | rows E1 to E7 of `deployments/pc-account/README.md` ("The tailnet's rule", "The table") read again by whoever that table names; and `timeout 30 tailscale ssh apstage@ap-checks true` on the VPS exits non-zero (Lead) |

On S5, row E4 is read with one added exception. Its policy read becomes: no grant and no `acls` rule from `tag:vps` towards a node of the PC except E1's and T2's (`tcp:22` to `tag:ap-stages`, `ssh` user `apstage`). Its `tailscale nc` part applies to every other node of the PC. E1 to E3 and E5 to E7 are read as written there.

On S3: what refuses a process of `apstage` is the rule, not the absent proxy. The root run reads the rule itself, through the node's own `tailscaled`; an `SSH-` line there means the rule admits the node, whatever the `apstage` run shows. An ordinary socket opened in `ap-stages` is not this row, because it does not leave through this node (see "Not closed by this").

## The order

Each step names who acts, its readback and its undo.

1. **The distribution** (the user). The user reads "The distribution" again. Then the user places the `ont` described there at `/home/apstage/.cargo/bin/ont`, as step 1 of "The order" of `deployments/pc-account/README.md` does for `apcheck`: with no credential and not through `/mnt/wsl`, for example the user's own distribution copies it from the VPS and the user moves it in through `\\wsl.localhost\ap-stages`. Readbacks: `wsl -d ap-stages --exec sh -c '. ~/.profile; command -v ont'` prints that path; `ont --version` and the glibc read as in "The distribution"; `wsl -d ap-stages --exec stat -c '%U %a' /home/apstage/.cargo/bin/ont` prints `apstage 755`. Undo: remove the file; for the whole distribution, `wsl --unregister ap-stages`.
2. **T1** (the Lead, M2). "The tailnet's rule", T1, with its readback and its undo.
3. **The node** (the user; "The node"). The package, the daemon's mode, the join, and their readbacks. The undo is each sub-step's own.
4. **T2 and the table** (the Lead and the user). T2 (Lead), then S1 to S5 each by whoever the table names. Then the node stays up: with every window of `ap-stages` closed for ten minutes, and again after a restart of Windows, `wsl -l --running` (the user) lists `ap-stages`, and `tailscale ping ap-stages` on the VPS (Lead, M3) answers. Systemd services do not keep a WSL instance alive, and an idle distribution is shut down after `instanceIdleTimeout` (`https://learn.microsoft.com/en-us/windows/wsl/wsl-config`, `https://learn.microsoft.com/en-us/windows/wsl/systemd`). What keeps `ap-stages` running is the user's choice, as "The order" step 3 of `deployments/pc-account/README.md` shows for `ap-checks`; no agent sets it, registers it or changes it. Undo: whatever the user set, as the user decides. Undo of T2: its kept copy written back.
5. **The sign-in** (the user; "The sign-in"), only after S1 to S5 pass. Readbacks and undo are those of that section.
6. **The merge and the delivery** (the Lead, M1). The Lead approves this unit, and so merges it, only after steps 1 to 5 are read back and after `ap policy check` of the pull request's policy file exits 0 with the installed `ap`. That `ap` must be one that reads `hosts.pc.stages` (`ap` `fb233b8`, unit `pc-stage`, does): where the installed `ap` does not, the check refuses the file and `scripts/install-etc.sh install` refuses `verify_failed` for it, for every later policy file too, because `ap deliver` runs it after each merge (`README.md`, "Who uses it"). Delivery installs the file. Readbacks: `ap policy check /etc/agent-platform/policy.json` prints `"status":"ok"` and `scripts/install-etc.sh check` prints `same`. Undo: at once `scripts/install-etc.sh rollback <kept file>`, and a pull request that removes `hosts.pc.stages`. The whole account is undone by the undo of "The sign-in", the undo of "The node", T2's and T1's kept copies written back, and `wsl --unregister ap-stages` by the user.

## The real run

The Lead's commands on the VPS (M4), after step 6, and what each shows:

1. The two readbacks of step 6: `ap policy check /etc/agent-platform/policy.json` prints `"status":"ok"` (exit 0), and `scripts/install-etc.sh check` prints `same`.
2. `ap policy check` of a scratch copy of the installed file (made with `mktemp -d`) whose `hosts.pc.stages.ssh` has no `@` is refused: exit 2, reader `check`, and a detail beginning `hosts.pc.stages`.
3. S1 and S2 of the table, read as written there.
4. `timeout 30 tailscale ssh apstage@ap-stages 'stat -c "%U %a" /home/apstage/.claude/.credentials.json; grep -c drvfs /proc/mounts; sudo -n true; echo $?'` prints `apstage 600`, then `0`, then a non-zero status (and no content of the credentials file).

The first stage on the PC is the real run of unit `pc-stage`, which follows this one. How a stage starts on the PC, where its `CLAUDE_CONFIG_DIR` points, its allow-lists and its view are that unit's, not this README's.

## Not closed by this

- **The one network.** A process of `apstage` outside a stage (the CLI itself, every `tailscale ssh` session from the VPS) opens ordinary sockets through WSL's network to the internet, which the CLI needs, and, where `ip -br addr` shows the interface of the user's node, a socket towards a tailnet address leaves through that interface as the user's node, to whatever the tailnet admits from it, the VPS among them (as the first item of "Not closed by this" of `deployments/pc-account/README.md`). A stage's own network is `pc-stage`'s allow-list, not this README's.
- **The one kernel.** Root of any distribution is root on it. `apstage` has no root.
- **The sign-in is usable by whoever runs as `apstage`**, the VPS's root through S1 among them, by design.
- **Root's `authorized_keys` on the VPS** is unchanged.
