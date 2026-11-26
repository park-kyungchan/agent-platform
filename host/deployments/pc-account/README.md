# The PC's account for the checks

## What this is

`ap check` on the VPS runs a repository's check on the local PC when the PC is there and free. Before this change it entered the user's own account on the PC (`<user-account>@<user-node>`), in a WSL distribution where drive C: is mounted and Windows programs start. Now it enters an account that exists for the checks alone:

| Name | What it is |
| --- | --- |
| `ap-checks` | the WSL distribution of the checks, apart from the user's own distribution |
| `apcheck` | its account, home `/home/apcheck`, not root and without sudo |
| `ap-checks` | the distribution's own tailnet node, tagged `tag:ap-checks` |
| `apcheck@ap-checks` | the value of `hosts.pc.ssh` in `etc/agent-platform/policy.json` |
| `/home/apcheck/work/ap-checks` | the value of `hosts.pc.work`; the PC's side of `ap check` makes it at the first call |

Who uses it: `ap check` on the VPS, which reaches the PC by `tailscale ping` and `tailscale ssh <ssh>` alone (lifecycle-platform `specs/ap/check.md` R4 and R16).

Who acts. Every step on the PC and in the tailnet's admin console is the user's, and so is every change of the VPS's node identity. The one exception is the copy of L5 below, which writes one file in the account's home. The Lead's steps all run on the VPS, and they are this list and nothing else:

- **L1**: the forms of `scripts/install-etc.sh` (`install`, `check`, `rollback`) and `ap policy check /etc/agent-platform/policy.json` of the installed file.
- **L2**: the reads `tailscale status`, `tailscale ping ap-checks`, `ss -H -ltn` and `tailscale serve status`. The Lead gives their output to the user, in a message and not in a file.
- **L3**: the commands of the table in "The tailnet's rule" that start at the VPS (E1, E2, E3, E4, E7).
- **L4**: the real run of "The real run".
- **L5**: each time lifecycle-platform's engine pin (`ontology/ENGINE-PIN`) moves, until a program delivers the checks' toolchain into the account: the copy of the VPS's newly installed `/usr/local/bin/ont` to `/home/apcheck/.cargo/bin/ont`, its readback and its undo. The copy goes to a new file, the old one is kept beside it as `ont.before`, and then a rename puts the new file in place:

  ```sh
  tailscale ssh apcheck@ap-checks 'cat > ~/.cargo/bin/ont.new && chmod 755 ~/.cargo/bin/ont.new && ln -f ~/.cargo/bin/ont ~/.cargo/bin/ont.before && mv ~/.cargo/bin/ont.new ~/.cargo/bin/ont' < /usr/local/bin/ont
  ```

  Readback: `ap check <worktree> -- ont --version` of "The real run" prints `ont 0.1.0 <pin>` with `host` `pc`. Undo: `tailscale ssh apcheck@ap-checks 'mv -f ~/.cargo/bin/ont.before ~/.cargo/bin/ont'`, then the same readback, which names the old pin.

Nothing in this repository installs anything on the PC. A merge of this file or of the policy changes nothing on the host or on the PC until the Lead runs `scripts/install-etc.sh install <merge>`.

### How a command is written

Every command run from Windows has the form `wsl -d ap-checks --exec <program> <arguments>`. Without `--exec`, the login shell expands quotes and `$?`. That form runs as `apcheck`, and a step that needs root adds `--user root` before `--exec`. The commands are written for PowerShell. Where the expected answer is an exit status, the user reads `$LASTEXITCODE` straight after the command. Before relying on that, the user checks that the form passes the status on: `wsl -d ap-checks --exec false`, then `$LASTEXITCODE` prints `1`. Where it does not, the user stops and tells the Lead.

Every statement below about WSL or Tailscale that this README does not cite to a page is a readback: it comes with its expected answer, and where the answer differs the user stops and tells the Lead. No step is shown done by a success message alone.

## The distribution

As the user prepared it on 2026-11-21. One line per property, with its read command and the expected answer. The user reads them all again at step 1 of "The order".

| Property | Read command | Expected answer |
| --- | --- | --- |
| systemd on, automount off, interop off, default user | `wsl -d ap-checks --exec cat /etc/wsl.conf` | `[boot]` `systemd=true`; `[automount]` `enabled=false`; `[interop]` `enabled=false` and `appendWindowsPath=false`; `[user]` `default=apcheck` |
| no Windows drive mounted | `wsl -d ap-checks --exec grep -c drvfs /proc/mounts` | `0` (and exit 1) |
| `apcheck` is uid 1000 and in neither group `sudo` nor `adm` | `wsl -d ap-checks --exec id apcheck` | `uid=1000(apcheck)`; the groups name neither `sudo` nor `adm` |
| no sudo | `wsl -d ap-checks --exec sudo -n true` | refused, `$LASTEXITCODE` `1` |
| password locked | `wsl -d ap-checks --user root --exec passwd -S apcheck` | second field `L` |
| Rust 1.100.0 from rustup on the `PATH` through `~/.profile` (the PC's side of `ap check` reads `$HOME/.profile`), rustup's default | `wsl -d ap-checks --exec sh -c '. ~/.profile; rustup default; rustc --version; cargo --version; cargo clippy --version; rustfmt --version'` | `rustup default` names `1.100.0-<target>` as default; `rustc` and `cargo` print `1.100.0`; clippy and rustfmt each print a version line and no error. The toolchain must be the default because the step that fetches the dependencies ignores the tree's `rust-toolchain.toml` |
| no cargo home of the checks before the first call | `wsl -d ap-checks --exec test -e /home/apcheck/work/ap-checks/cargo-home` | `$LASTEXITCODE` `1`. Where it exists, the user removes it once (`wsl -d ap-checks --exec rm -rf /home/apcheck/work/ap-checks/cargo-home`), because only the step that fetches the dependencies may fill it |
| the resolver file exists (that step is shown it) | `wsl -d ap-checks --exec readlink -f /etc/resolv.conf` and `wsl -d ap-checks --exec stat -L -c %F /etc/resolv.conf` | a path, and `regular file` |
| `bwrap` runs a command in a new user namespace | `wsl -d ap-checks --exec bwrap --unshare-user --uid 0 --gid 0 --ro-bind / / id -u` | `0` |
| the programs the PC's side calls, and `python3`, which lifecycle-platform's `./check` calls | `wsl -d ap-checks --exec bash -c 'command -v bash git tar flock timeout nice setsid find python3 bwrap'` | ten paths, one per name |
| the engine, read through `~/.profile` | `wsl -d ap-checks --exec sh -c '. ~/.profile; command -v ont; ont --version'` | `/home/apcheck/.cargo/bin/ont`, then `ont 0.1.0 <pin>`. The pin is lifecycle-platform's `ontology/ENGINE-PIN` at main (`c600967d1d9725a318843fb693d1b71acd3b284f` on 2026-11-21, which the VPS's `/usr/local/bin/ont` equals). Step 1 of "The order" places this file, which is not there yet |
| glibc for the VPS's dynamically linked `ont` | `wsl -d ap-checks --exec ldd --version` | first line ends with `2.39` or a later version. Where `ont` does not start, the user stops and tells the Lead |
| no Windows program starts | the user copies a Windows program (for example `whoami.exe`) into `\\wsl.localhost\ap-checks\home\apcheck\`, runs `wsl -d ap-checks --exec /home/apcheck/whoami.exe`, then removes the copy | an error and a non-zero `$LASTEXITCODE`; nothing of Windows prints |

lifecycle-platform's `./check` uses the installed `ont` only when its version names the pin. The account cannot build an `ont` itself, because the engine's repository is private. No credential is ever put into the account to make that possible.

## What the two distributions share

`ap-checks` and the user's own distribution run in one WSL virtual machine:

| Shared | What follows | What the steps do about it | Read command (in each distribution) |
| --- | --- | --- | --- |
| One kernel | Root in `ap-checks` is root on the kernel that the user's distribution runs on | `apcheck` is not root and has no sudo, and no step gives it root | `uname -r` gives the same answer in both; `id apcheck` and `sudo -n true` as in "The distribution" |
| The mount `/mnt/wsl` | Both distributions see it | Nothing of the user's is placed there, and nothing reaches the account through it | `ls -la /mnt/wsl` shows the same listing in both, with no file of the user's |
| One network | The interfaces, the loopback and its ports are the same in both | The second `tailscaled` runs in userspace-networking mode with a UDP port of its own ("The node"); see the first line of "Not closed by this" | `ip -br addr` gives the same lines in both |
| One virtual machine's memory | A check's memory comes out of the same machine as the user's own work. No `memory` line is added to `.wslconfig`: the user decided at 2026-11-21 02:49Z not to cap the machine's memory | In `ap-checks`, the probe of `ap check` reads the Linux `MemAvailable` alone. Its Windows reading needs `powershell.exe` under `/mnt/c`, which is absent there by design, so each record line has `pc_windows_available_mb` null, and `min_available_mb` (2,048 MB while it is absent from the policy) is compared with the Linux reading only (lifecycle-platform `specs/ap/check.md` R4, R15, R16) | PowerShell: `Select-String -Path $env:USERPROFILE\.wslconfig -Pattern '^\s*memory'` prints nothing (or the file does not exist) |

## The node

These are the user's steps that make `ap-checks` a tailnet node of its own, beside the user's node in the one network, in this order. Every command of this section is a proposal with its readback. Tailscale's pages show these flags (`https://tailscale.com/kb/1112/userspace-networking`, `https://tailscale.com/kb/1241/tailscale-up`, `https://tailscale.com/kb/1278/tailscaled`, `https://tailscale.com/kb/1080/cli`, `https://tailscale.com/kb/1068/tags`). They do not show that Tailscale SSH answers on a userspace-networking node beside a second `tailscaled`, nor how the package and its unit are set up here. When a readback differs, the user stops and tells the Lead.

1. **The tags first** (the user). In the tailnet policy file, the user adds a `tagOwners` entry for `tag:ap-checks` and one for `tag:vps`, and nothing else of "The tailnet's rule" yet:

   ```json
   "tagOwners": {
     "tag:ap-checks": ["<user-login>"],
     "tag:vps": ["<user-login>"]
   }
   ```

   The policy file is not the default one (`deployments/tsidp/compose.yaml` shows a node `idp` with `tag:tsidp`), so the two keys go beside what its `tagOwners` already holds. A tag must exist in the policy file before a node can take it, and whoever joins the node must be among its owners (`https://tailscale.com/kb/1068/tags`). The user saves the file and reads both tags in the admin console, for example in a machine's "Edit tags" list. Expected: both are listed. Undo: the previous policy file, saved again.

2. **The package** (the user). Tailscale is installed in `ap-checks` from Tailscale's own package for the distribution, as root (`--user root`). Readback: `wsl -d ap-checks --exec tailscale version` prints a version.

3. **The daemon's mode** (the user). The `tailscaled` of `ap-checks` runs with `--tun=userspace-networking`, so that it does not take the interface of the user's `tailscaled`. It runs with `--port=<ap-port>`, a UDP port other than the one the user's `tailscaled` uses (`--port`, `https://tailscale.com/kb/1278/tailscaled`), and with no proxy flag (neither `--socks5-server` nor `--outbound-http-proxy-listen`). Proposal: where the package's unit reads `/etc/default/tailscaled`, set `PORT="<ap-port>"` and `FLAGS="--tun=userspace-networking"` there as root, then `wsl -d ap-checks --user root --exec systemctl restart tailscaled`. Readbacks:
   - `ps -o args= -C tailscaled`, run in the user's distribution, shows the user's own daemon and its port; `<ap-port>` differs from it.
   - `wsl -d ap-checks --exec ps -o args= -C tailscaled` prints one line containing `--tun=userspace-networking` and `--port=<ap-port>`, with neither proxy flag.
   - `wsl -d ap-checks --exec systemctl is-active tailscaled` prints `active`.
   - `wsl -d ap-checks --exec ss -H -uln` lists `<ap-port>` once and the user's port once.

   Undo: the previous `/etc/default/tailscaled`, and the same restart.

4. **The join** (the user). `wsl -d ap-checks --user root --exec tailscale up --hostname=ap-checks --ssh --advertise-tags=tag:ap-checks` prints an interactive sign-in link. The user opens it in their own browser and signs in. No auth key is made, and the link, a key and the policy file go into no chat, no message and no file of a repository. Readbacks:
   - `wsl -d ap-checks --exec tailscale status --self=true --peers=false` names `ap-checks` with the owner `tagged-devices`.
   - The admin console lists the machine `ap-checks` with `tag:ap-checks`.
   - `tailscale status` in the user's own distribution still shows the user's node online.

   Undo: `wsl -d ap-checks --user root --exec tailscale logout`, then the machine removed in the admin console.

`tailscale ping ap-checks` on the VPS is read only at step 3 of "The order", after the grant.

## The tailnet's rule

### The table (the contract)

One row per direction, with its test command and the expected result. A refused row's command runs under `timeout` and is expected to exit non-zero, 124 included. A `tailscale ping` that answers proves nothing about a refused row: no page cited here says that a ping obeys the rules. A `tailscale nc` joins its standard input and output to the connection, so an accepted connection whose input stays open also ends in 124 at the `timeout`. A refused `tailscale nc` row therefore also needs an empty standard output: no line at all, and in particular no line beginning `SSH-` (the banner an sshd sends first). A non-zero exit with any output on standard output means the connection was accepted: the row is broken, and the user stops and tells the Lead.

| Row | Direction | Expected | Test (who runs it) |
| --- | --- | --- | --- |
| E1 | the VPS to `apcheck@ap-checks` by Tailscale SSH | accepted, without a sign-in prompt | `timeout 30 tailscale ssh apcheck@ap-checks true` on the VPS exits 0 (Lead, L3) |
| E2 | the VPS to `root@ap-checks`, or to any other account there | refused | `timeout 30 tailscale ssh root@ap-checks true` on the VPS exits non-zero (Lead, L3) |
| E3 | the VPS to the node of the user's own distribution, any account and any port | refused | `timeout 30 tailscale ssh <user-account>@<user-node> true` on the VPS exits non-zero. It is read after the VPS's tag, and while `tailscale status` on the VPS shows that node online; where the VPS does not show it, the user brings the node up, and where it still does not show, tells the Lead. For the other ports, as E4 (Lead, L3) |
| E4 | the VPS to any other node of the PC (Windows, where it is a node) | refused | The saved policy holds no grant and no `acls` rule from `tag:vps` towards a node of the PC except the one of E1 (the user reads it). For each port such a node serves, `timeout 10 tailscale nc <pc-node> <port>` on the VPS exits non-zero and prints nothing on standard output (Lead, L3). Where the port's service would not send first, the row rests on the policy read alone |
| E5 | the node `ap-checks` to the VPS, any port | refused: no rule names `tag:ap-checks` as a source | Through the node's own `tailscaled`, once as root and once as `apcheck`, both non-zero and both with nothing on standard output (no line beginning `SSH-`, which the VPS's sshd on port 22 sends first to an accepted connection): `wsl -d ap-checks --user root --exec timeout 10 tailscale nc <vps-node> 22` and `wsl -d ap-checks --exec timeout 10 tailscale nc <vps-node> 22` (the user) |
| E6 | the user's own devices to the VPS | as before the change | a new plain `ssh` login to the VPS from a device of the user, plus one read of each other port of the list below from a device of the user (the user) |
| E7 | the VPS and the other nodes of the tailnet, the node `idp` (`tag:tsidp`) among them | as before | each flow of the list below, read again after the change with the command the list names for it (Lead for a flow that starts at the VPS, else the user) |

On E5: what refuses a process of `apcheck` is this row's rule, not the absent proxy. The root run reads the rule itself, through the node's own `tailscaled`; an `SSH-` line there means the rule admits the node, whatever the `apcheck` run shows. The `apcheck` run shows that a process of the account gets no further through the node, and its refusal rests on the same rule. An ordinary socket opened in `ap-checks` is not this row, because it does not leave through this node (see "Not closed by this").

On E6: the Lead's read of 2026-11-21 16:47Z (`ss -H -ltn`, `tailscale serve status`) showed these listeners on the VPS's tailnet addresses:
- TCP 22, the OpenSSH sshd of the records under `deployments/ssh/`, IPv4 only;
- TCP 443, `tailscale serve` of the status page (tailnet only);
- two high ports, which are `tailscaled`'s own listeners (60531 and 60807 at the time).

So E6 is a grant towards `tag:vps` for port 22, plus each other port of the list that a device of the user uses. That includes 443 unless the user says no device reads the status page.

### The list (before the tag)

The VPS gets the tag `tag:vps`. A tag removes the node's user-based authentication, and `autogroup:self` does not apply to a tagged node (`https://tailscale.com/kb/1068/tags`, `https://tailscale.com/kb/1337/policy-syntax`). So the user's rules stop covering the VPS, both as a source and as a destination. Before the tag, the user makes a list, kept outside every repository, from two sources: the Lead's reads L2, run again when this unit runs, and the rules that name the user or `*`. The list holds:

- what the VPS reaches and what reaches it, each flow with the command that reads it;
- for each served port, whether a device of the user uses it;
- whether the user uses Taildrop with the VPS (`https://tailscale.com/kb/1106/taildrop`);
- whether the user enters the VPS by Tailscale SSH too. Then an `ssh` rule from `<user-login>` to `tag:vps` joins the fragment below.

The grants of E6 and E7 are written from that list.

### The fragment (step 3 of "The order")

Together with the `tagOwners` of "The node", this fragment gives the table. It is a proposal: the user merges it into the saved policy file, and the admin console's own check of the policy, then the table's commands, decide. The syntax follows `https://tailscale.com/kb/1337/policy-syntax`, `https://tailscale.com/kb/1324/grants`, and Tailscale's page on Tailscale SSH (`https://tailscale.com/docs/features/tailscale-ssh`, its access rules, the connections it allows, and its SSH tests).

```jsonc
{
  "grants": [
    // E1: the VPS reaches the node ap-checks on port 22 and nothing else of the PC.
    {"src": ["tag:vps"], "dst": ["tag:ap-checks"], "ip": ["tcp:22"]},
    // E6: the user's devices reach the VPS on 22 and on each other port of the list they use.
    {"src": ["<user-login>"], "dst": ["tag:vps"], "ip": ["tcp:22", "tcp:443"]},
    // E7: one grant per flow of the list between the VPS and the other nodes, as before.
    {"src": ["<source of the flow>"], "dst": ["<destination of the flow>"], "ip": ["<port of the flow>"]}
  ],
  "ssh": [
    // E1 and E2: the account apcheck alone; never root and never autogroup:nonroot.
    {"action": "accept", "src": ["tag:vps"], "dst": ["tag:ap-checks"], "users": ["apcheck"]}
  ],
  "sshTests": [
    // E1 and E2
    {"src": "tag:vps", "dst": ["tag:ap-checks"], "accept": ["apcheck"], "deny": ["root", "<user-account>"]},
    // E3: <user-node> as the policy file can name that node (for example a "hosts" entry)
    {"src": "tag:vps", "dst": ["<user-node>"], "deny": ["<user-account>", "root"]}
  ]
}
```

E5 follows from what the fragment leaves out: no grant, no `acls` rule and no `ssh` rule has `tag:ap-checks` as a source. E4 follows the same way: no rule from `tag:vps` names a node of the PC except the grant and the `ssh` rule of E1.

Rules the user must narrow because they break a row: any grant or `acls` rule whose source is `*`, or whose source holds a tagged node. Such a rule lets `tag:ap-checks` reach the VPS (E5), and with a destination `*` it lets `tag:vps` reach the PC (E3, E4). Narrowing such a rule can cut the node `idp`, which E7 guards. Each flow of `idp` on the list gets its own grant before the rule is narrowed.

The `ssh` rule from `autogroup:member` to `autogroup:self` stays: it keeps Tailscale SSH among the user's own devices. The tag takes the VPS out of that rule, and that is why E3 is read after the tag.

Where the admin console refuses an entry's form (for example a tag as the `src` of an `sshTests` entry), the user tells the Lead, and the table's commands still decide.

### The VPS's tag (the user's procedure)

The tag changes the VPS's node identity, and it can cut the user's way into the VPS. It falls under the user's procedure of `host/README.md` ("The login path"), and the user does every step:

1. The user confirms that the provider's console is the way back into the VPS.
2. The grants of E6 and E7 are saved before the tag (step 3 of "The order").
3. The user reads the VPS's key expiry in the admin console before the tag. It is expected unchanged after the tag, because a change of tags without a new sign-in leaves it. Where the console shows an expiry date, the user notes it: an expired key cuts the VPS from the tailnet, and the date is the user's to handle.
4. The user sets the tag in the admin console (the machine's "Edit tags"). There is no command on the VPS and no new sign-in.
5. The user tests E6 with a new login from a device before closing the old session.
6. Each flow of the list is read again.

Undo, in two parts:

1. **The policy file.** The saved previous rules return beside the grants that name `tag:vps`, so the flows of the list hold again while the node keeps its tag. The console cannot remove a node's last tag. Two things do not come back this way, because they depend on the node's user and not on a rule:
   - Tailscale SSH from the VPS into a device of the user. That was the way `ap check` reached the PC before; Tailscale SSH is allowed only from a user to their own or a tagged device, and between tagged devices.
   - Taildrop to and from the VPS.
2. **The node's user**, only if the VPS must be the user's own node again. The user starts a new sign-in of the node with the user's account, from the provider's console, because it can cut the tailnet session. The user orders and signs that step; no agent runs it.

## The order

Each step names who acts, its readback and its undo.

1. **The distribution** (the user). The user reads "The distribution" again. Then the user places the `ont` described there at `/home/apcheck/.cargo/bin/ont`, with no credential and not through `/mnt/wsl`: for example, the user's own distribution copies it from the VPS, and the user moves it in through `\\wsl.localhost\ap-checks`. Readbacks:
   - `wsl -d ap-checks --exec sh -c '. ~/.profile; command -v ont'` prints that path.
   - `ont --version` and the glibc read as in "The distribution".
   - `wsl -d ap-checks --exec stat -c '%U %a' /home/apcheck/.cargo/bin/ont` prints `apcheck 755`.

   Undo: remove the file.
2. **The node** (the user; "The node"). The two tags are saved and read, then the package, the daemon's mode, the join and their readbacks. The undo is each sub-step's own.
3. **The rule** (the user, except where the Lead is named), in this order:
   1. The list (Lead: L2 again; user: the rest).
   2. The `ssh` rule, the grants and the tests are saved, and the rules with a `*` source are narrowed.
   3. After that save and before the tag: a new plain `ssh` login from a device of the user, and one read of each other port of the list. The undo of 1 to 3 is the saved previous policy file.
   4. The `ont` property of "The distribution" is read; the tag is not set while it differs.
   5. The VPS's tag, under the user's procedure above.
   6. E1 to E7 are read (Lead: L3 for the rows that start at the VPS; user: E4's policy read, E5, E6, and E7's flows that start elsewhere), and `tailscale ping ap-checks` on the VPS answers (Lead, L2).
   7. The node stays up. With every window of `ap-checks` closed for ten minutes, and again after a restart of Windows, `wsl -l --running` (the user) lists `ap-checks`, and `tailscale ping ap-checks` (Lead, L2) answers.

   Systemd services do not keep a WSL instance alive. An idle distribution is shut down after `instanceIdleTimeout`, which is set in `.wslconfig`, section `[general]`: 15000 ms by default, `-1` for never, and it applies to every distribution, the user's own included (`https://learn.microsoft.com/en-us/windows/wsl/wsl-config`, `https://learn.microsoft.com/en-us/windows/wsl/systemd`). That setting is one way to keep the node up, and its reach is that whole list of distributions.
   What keeps `ap-checks` running: on 2026-11-22 the user chose a scheduled task of the user's own Windows account, started at logon, which keeps only `ap-checks` running and leaves the user's other distributions to `instanceIdleTimeout`, and chose no automatic restart of it. No agent sets either: no agent registers or changes the task. The user runs in PowerShell:

   ```powershell
   $a = New-ScheduledTaskAction -Execute "powershell.exe" -Argument "-WindowStyle Hidden -Command wsl.exe -d ap-checks --exec sleep infinity"
   $t = New-ScheduledTaskTrigger -AtLogOn -User $env:USERNAME
   $s = New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
   Register-ScheduledTask -TaskName "ap-checks-keepalive" -Action $a -Trigger $t -Settings $s
   Start-ScheduledTask -TaskName "ap-checks-keepalive"
   ```

   Readbacks (the user): `(Get-ScheduledTask -TaskName ap-checks-keepalive).Triggers.Count` prints 1, and `wsl -l --running` lists `ap-checks`. (Lead) `tailscale ping ap-checks` answers.

   Undo: `Unregister-ScheduledTask -TaskName ap-checks-keepalive -Confirm:$false`.

   When the instance stops anyway (a `wsl --shutdown`, a closed session): every `ap check` already tests the PC at each call (ping and probe, `specs/ap/check.md` R4 of lifecycle-platform) and runs on the VPS with `pc_unreachable` meanwhile. The Lead, when it hands out work or reads such a line while `tailscale status` shows the user's PC online, asks the user at once to run `Start-ScheduledTask -TaskName ap-checks-keepalive`, then reads `tailscale ping ap-checks`.

   Why it works: a running process keeps the instance from the idle shutdown.
4. **The install** (the Lead, L1). The Lead approves this unit, and so merges it, only after the user has done steps 1 to 3; the user works from the pull request's branch. No other policy merge is installed between this merge and this step, because `install` writes the whole file and would put the new account live before its time. Then at once:
   - `scripts/install-etc.sh install <merge commit>`;
   - `ap policy check /etc/agent-platform/policy.json` prints `"status":"ok"`, and `scripts/install-etc.sh check` prints `same`.

   Undo: `scripts/install-etc.sh rollback <kept file>`. Between step 3 and this install, the policy still names the old account. Each `ap check` can then spend up to 12 s on the ping and 30 s on the probe before it runs on the VPS with `pc_unreachable`, as it does whenever the PC is off.
5. **The real run** (the Lead, L4; "The real run").
   - **`check: pc: fetch`.** Where the call says this, the step that fetches the dependencies failed: the network, or the resolver file of "The distribution". The call is `pc_lost`, and the command runs on the VPS once.
   - **A failure on the PC.** A command that fails on the PC is not run again on the VPS.
   - **A missing program.** Where the command fails for a missing program, the output names it, the user installs it, and the run is repeated.
   - **A missing access.** Where the command fails for an access the account lacks (a private repository, a credential), the Lead rolls back with step 4's undo and reports. No credential is put into the account. That rollback leaves the policy naming the old account, which the tagged VPS cannot enter, so the PC does not come back: every `ap check` runs on the VPS with `pc_unreachable`, and `scripts/install-etc.sh check` prints `differs`, until the account is fixed. Then the merged policy is installed again by `scripts/install-etc.sh rollback <the file that rollback itself kept>`, because `install <merge commit>` is refused `not_forward` there. Meanwhile no later policy merge is installed: it would put the new account back whether it is fixed or not.
6. **The old work directory** (the user). The user removes the old work directory of the checks in the user's own account whenever they choose. No agent can after step 3, and no agent removes it.

## The real run

These are the Lead's commands on the VPS and what each shows. Each call writes its record to a scratch file of its own through `AP_CHECKS_LOG`, so that the line read is the call's own. `<worktree>` is a worktree of lifecycle-platform at main.

```sh
set -o pipefail
run=$(mktemp -d)
line() { python3 -I -c 'import json,sys; l=json.loads(open(sys.argv[1]).read().splitlines()[-1]); print(l["host"], l["reason"], l["exit"], l["pc_windows_available_mb"])' "$1"; }

AP_CHECKS_LOG=$run/1.jsonl ap check <worktree>; echo "exit $?"; line $run/1.jsonl
AP_CHECKS_LOG=$run/2.jsonl ap check <worktree> -- cat /mnt/c/Windows/win.ini; echo "exit $?"; line $run/2.jsonl
AP_CHECKS_LOG=$run/3.jsonl ap check <worktree> -- /mnt/c/Windows/System32/cmd.exe /c ver; echo "exit $?"; line $run/3.jsonl
timeout 30 tailscale ssh <user-account>@<user-node> true; echo "exit $?"
AP_CHECKS_LOG=$run/4.jsonl ap check <worktree> -- ont --version; echo "exit $?"; line $run/4.jsonl
```

Expected:

1. **The repository's check.** `ap check <worktree>` exits 0, its first line on standard error begins `check: on pc (`, and the record line prints `pc None 0 None`: `host` `pc`, `reason` null, `exit` 0 and `pc_windows_available_mb` null.
2. **The Windows file and program.** The read of `/mnt/c/Windows/win.ini` and the start of `cmd.exe` each exit non-zero, with `host` `pc` in the line. Both would fail on the VPS too, and the command's view shows no path under `/mnt` on any distribution (lifecycle-platform `specs/ap/check.md` R8). So `host` shows the place, and the distribution's own state shows elsewhere: in `pc_windows_available_mb` null (the probe runs outside the sandbox, R4) and in "The distribution".
3. **E3.** The command of E3 is refused: non-zero, 124 included.
4. **The engine.** `ap check <worktree> -- ont --version` prints `ont 0.1.0 <pin>` with `host` `pc`, because the command sees the account's Cargo `bin` (R8). This is also the readback of L5.

A worktree whose `ontology/ENGINE-PIN` the account's `ont --version` does not name is not checked on the PC. `ap check` answers before the command with the reason `pc_engine_differs` and runs it on the VPS, as for `pc_unreachable` (R19). So a move of the pin needs no rollback. L5 is done after the delivery that installs the new `ont` on the VPS, or when the PC is next on, and it only brings the PC back into use for worktrees at the new pin.

`<user-account>@<user-node>` stands for the user's own account and node: the value of `hosts.pc.ssh` before this change.

## Not closed by this

- **Processes of `apcheck` outside the sandbox.** The command of a check has no network and sees of the account's home only the toolchain (R8, R18). A process of `apcheck` outside that sandbox, though, is in the one network of "What the two distributions share" and sees the home. Two such processes exist: whatever a `tailscale ssh` session from the VPS runs, and the step before the command that fetches the dependencies with the PC's network. Where the `ip -br addr` readback shows the interface of the user's node, a socket that such a process opens towards a tailnet address leaves through that interface. It then sends as the user's node to whatever the tailnet's rule admits from that node, the VPS among them (E6). Such a process can also address whatever listens on the loopback. What would close it: the user's `tailscaled` without an interface, or a virtual machine in place of the distribution.
- **Root's `authorized_keys` on the VPS** is unchanged. What would close it: a change of the VPS's login path, by the user's procedure of `host/README.md` ("The login path").
