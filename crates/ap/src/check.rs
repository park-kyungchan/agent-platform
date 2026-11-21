//! `ap check`: a repository's check for a worktree, on the local PC when the host's policy names it
//! and the PC answers and is not busy, else on this host (`specs/ap/check.md`). The PC's side is the
//! fixed script `check/pc.sh`; what reaches the PC is that script, its arguments and one payload of
//! the worktree's state.
use crate::host::{Failure, Host, System, digest, fail, new_session_id};
use crate::run::stamp;
use crate::work;
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::DirBuilderExt,
        process::{CommandExt, ExitStatusExt},
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitCode, ExitStatus, Stdio},
    sync::mpsc,
    time::{Duration, Instant, SystemTime},
};

/// The PC's side, run as `bash -c` (R6 to R12).
const PC_SCRIPT: &str = include_str!("check/pc.sh");
/// The PC's CPUs, its one-minute load, the `MemAvailable` of its Linux in kB, Windows'
/// `FreePhysicalMemory` in kB as PowerShell answers it (its whitespace made spaces; empty when the
/// call fails), and the HEAD of the worktree's repository there, if any (R4): the file read as text,
/// no git run on what a command might have left. One bounded PowerShell call, which only reads; its
/// text holds no `"`, since interop rebuilds a Windows command line from the arguments.
const PROBE: &str = r#"w=$(timeout 10 /mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe -NoProfile -NonInteractive -Command '(Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory' </dev/null 2>/dev/null) || w=; printf '%s %s %s %s\n' "$(nproc)" "$(cut -d ' ' -f 1 /proc/loadavg)" "$(sed -n 's/^MemAvailable: *\([0-9][0-9]*\) kB$/\1/p' /proc/meminfo 2>/dev/null)" "${w//[[:space:]]/ }"; [ -L "$1/HEAD" ] || head -c 80 -- "$1/HEAD" 2>/dev/null; exit 0"#;
/// The only variables of this process that a `tailscale` child gets (R7): not `LANG` or `LC_*`, which
/// the ssh client's default `SendEnv` would send to the PC, nor an agent socket.
const TAILSCALE_ENV: [&str; 4] = ["PATH", "HOME", "USER", "LOGNAME"];
const DEFAULT_TIMEOUT: u64 = 2_700;
const MAX_TIMEOUT: u64 = 14_400;
/// How long after the deadline this host waits for the PC's own end before it cuts the connection.
const GRACE: Duration = Duration::from_secs(30);
const DEFAULT_LOCK: &str = "/var/lib/agent-platform/build.lock";
const DEFAULT_LOG: &str = "/var/lib/agent-platform/checks.jsonl";
/// `hosts.pc.min_available_mb` when the policy does not give it (R16).
const DEFAULT_MIN_AVAILABLE_MB: u64 = 2_048;
const MAX_MIN_AVAILABLE_MB: u64 = 1_048_576;

/// The policy's member `hosts.pc` (R16).
#[derive(Debug, Clone, PartialEq)]
pub struct Pc {
    pub ssh: String,
    pub work: String,
    pub busy_load: f64,
    pub min_available_mb: u64,
}
impl Pc {
    fn host(&self) -> &str {
        self.ssh.split_once('@').map_or("", |(_, h)| h)
    }
}

fn invalid(member: &str, why: &str) -> Failure {
    fail("policy_invalid", "policy", json!(format!("{member} {why}")))
}
fn ssh_ok(s: &str) -> bool {
    let Some((user, host)) = s.split_once('@') else {
        return false;
    };
    let user_ok = (1..=32).contains(&user.len())
        && user.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
        && user
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c));
    let host_ok = (1..=253).contains(&host.len())
        && host.starts_with(|c: char| c.is_ascii_alphanumeric())
        && host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-".contains(&c));
    user_ok && host_ok
}
fn work_ok(s: &str) -> bool {
    s.len() <= 1024
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-/".contains(&c))
        && s.strip_prefix('/')
            .is_some_and(|r| r.split('/').all(|c| !c.is_empty() && c != "." && c != ".."))
}

/// The reader of the policy's `hosts` (R16): None without the member, else its one key `pc`.
pub fn hosts(policy: &Value) -> Result<Option<Pc>, Failure> {
    let Some(hosts) = policy.get("hosts") else {
        return Ok(None);
    };
    let pc = match hosts.as_object() {
        Some(h) if h.len() == 1 && h.contains_key("pc") => &h["pc"],
        _ => return Err(invalid("hosts", "is not an object whose one key is pc")),
    };
    let m = pc
        .as_object()
        .filter(|m| {
            ["ssh", "work", "busy_load"]
                .iter()
                .all(|k| m.contains_key(*k))
                && m.keys().all(|k| {
                    ["ssh", "work", "busy_load", "min_available_mb"].contains(&k.as_str())
                })
        })
        .ok_or_else(|| {
            invalid(
                "hosts.pc",
                "is not an object with exactly ssh, work and busy_load, and at most min_available_mb besides",
            )
        })?;
    let ssh = (m["ssh"].as_str())
        .filter(|s| ssh_ok(s))
        .ok_or_else(|| invalid("hosts.pc.ssh", "is not user@host"))?;
    let work = (m["work"].as_str()).filter(|s| work_ok(s)).ok_or_else(|| {
        invalid(
            "hosts.pc.work",
            "is not an absolute path of letters, digits and ._-/",
        )
    })?;
    let busy_load = (m["busy_load"].as_f64())
        .filter(|b| (0.1..=1.0).contains(b))
        .ok_or_else(|| invalid("hosts.pc.busy_load", "is not a number from 0.1 to 1.0"))?;
    let min_available_mb = match m.get("min_available_mb") {
        None => DEFAULT_MIN_AVAILABLE_MB,
        Some(v) => v
            .as_u64()
            .filter(|n| *n <= MAX_MIN_AVAILABLE_MB)
            .ok_or_else(|| {
                invalid(
                    "hosts.pc.min_available_mb",
                    "is not a whole number from 0 to 1048576",
                )
            })?,
    };
    Ok(Some(Pc {
        ssh: ssh.to_owned(),
        work: work.to_owned(),
        busy_load,
        min_available_mb,
    }))
}

/// One call's arguments (R1).
#[derive(Debug, PartialEq)]
pub struct Call {
    pub dir: String,
    pub timeout: u64,
    pub command: Vec<String>,
}
fn invalid_input(field: &str) -> Failure {
    fail("invalid_input", field, Value::Null)
}
/// `<dir> [--timeout <seconds>] [-- <command> [<arg>...]]` (R1).
pub fn arguments(argv: &[OsString]) -> Result<Call, Failure> {
    let usage = || fail("usage", "args", Value::Null);
    let (first, mut rest) = argv.split_first().ok_or_else(usage)?;
    if first.as_bytes().starts_with(b"-") {
        return Err(usage());
    }
    let dir = (first.to_str())
        .filter(|d| d.starts_with('/'))
        .ok_or_else(|| invalid_input("dir"))?;
    let mut timeout = DEFAULT_TIMEOUT;
    if rest.first().is_some_and(|a| a == "--timeout") {
        let [_, value, more @ ..] = rest else {
            return Err(usage());
        };
        timeout = (value.to_str())
            .filter(|v| (1..=5).contains(&v.len()) && v.bytes().all(|c| c.is_ascii_digit()))
            .and_then(|v| v.parse().ok())
            .filter(|t| (1..=MAX_TIMEOUT).contains(t))
            .ok_or_else(|| invalid_input("timeout"))?;
        rest = more;
    }
    let command = match rest {
        [] => vec!["./check".to_owned()],
        [dash, words @ ..] if dash == "--" && !words.is_empty() => words
            .iter()
            .map(|w| w.to_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
            .filter(|w| !w[0].is_empty())
            .ok_or_else(|| invalid_input("command"))?,
        _ => return Err(usage()),
    };
    Ok(Call {
        dir: dir.to_owned(),
        timeout,
        command,
    })
}

/// The worktree as Git reads it (R2).
#[derive(Debug)]
pub struct Tree {
    pub dir: String,
    pub head: String,
    pub repository: String,
}
fn git(dir: &str, words: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(words)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}
fn line(bytes: Vec<u8>) -> Option<String> {
    let s = String::from_utf8(bytes).ok()?;
    Some(s.strip_suffix('\n').unwrap_or(&s).to_owned())
}
fn is_hex(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|c| c.is_ascii_hexdigit())
}
/// `owner/name` of a GitHub remote URL (`https://`, `ssh://` or `git@github.com:`), None otherwise.
pub fn github(url: &str) -> Option<String> {
    let path =
        if let Some(r) = (url.strip_prefix("https://")).or_else(|| url.strip_prefix("ssh://")) {
            let (authority, path) = r.split_once('/')?;
            let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            if !host.eq_ignore_ascii_case("github.com") {
                return None;
            }
            path
        } else {
            url.strip_prefix("git@github.com:")?
        };
    let path = path.strip_suffix('/').unwrap_or(path);
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    let ok = |s: &str| {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    };
    (ok(owner) && ok(name)).then(|| format!("{owner}/{name}"))
}
fn worktree(dir: &str) -> Result<Tree, Failure> {
    let not = || fail("not_a_worktree", "dir", Value::Null);
    let real = fs::canonicalize(dir).map_err(|_| not())?;
    if !real.is_dir() {
        return Err(not());
    }
    let real = real.to_str().ok_or_else(not)?.to_owned();
    let top = git(&real, &["rev-parse", "--show-toplevel"])
        .and_then(line)
        .ok_or_else(not)?;
    if top != real {
        return Err(not());
    }
    let head = git(&real, &["rev-parse", "-q", "--verify", "HEAD^{commit}"])
        .and_then(line)
        .filter(|h| is_hex(h))
        .ok_or_else(not)?;
    let repository = git(&real, &["remote", "get-url", "origin"])
        .and_then(line)
        .and_then(|u| github(&u))
        .ok_or_else(|| fail("origin_not_github", "origin", Value::Null))?;
    Ok(Tree {
        dir: real,
        head,
        repository,
    })
}

/// Every refusal comes before any effect (R1 to R3): the arguments, the worktree, the policy.
pub fn prepare(s: &impl System, argv: &[OsString]) -> Result<(Call, Tree, Option<Pc>), Failure> {
    let call = arguments(argv)?;
    let tree = worktree(&call.dir)?;
    let pc = match work::policy(s)? {
        Some(v) => hosts(&v)?,
        None => None,
    };
    Ok((call, tree, pc))
}

fn variable(name: &str, default: &str) -> String {
    std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_owned())
}
fn say(text: &str) {
    let _ = writeln!(std::io::stderr(), "{text}");
}
/// A word for the PC's shell: single-quoted.
pub fn quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}
fn code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}
/// Waits for `child` until `deadline`; at it, kills the child's process group and returns None.
fn wait_until(child: &mut Child, deadline: Instant) -> Option<ExitStatus> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = Command::new("kill")
                    .args(["-KILL", "--", &format!("-{}", child.id())])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// What the probe of R4 read on the PC: its CPUs, its one-minute load (as the PC wrote it), the
/// `MemAvailable` of its Linux and Windows' free physical memory, both in MB rounded down.
#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    pub cpus: u64,
    pub load: String,
    pub wsl_mb: u64,
    pub windows_mb: Option<u64>,
}
impl Probe {
    /// The PC's available memory: the smaller of the two, or the Linux reading alone.
    pub fn available_mb(&self) -> u64 {
        self.windows_mb.map_or(self.wsl_mb, |w| w.min(self.wsl_mb))
    }
    fn load_value(&self) -> f64 {
        self.load.parse().unwrap_or(f64::MAX)
    }
}
/// The probe's first line, `<cpus> <load> <MemAvailable kB> <Windows' answer>`: None when the CPUs,
/// the load or the Linux memory cannot be parsed; Windows' answer, trimmed of whitespace, is None
/// unless it is then a whole number of kB.
pub fn probe_line(line: &str) -> Option<Probe> {
    let mut f = line.splitn(4, ' ');
    let cpus = f.next()?.parse::<u64>().ok().filter(|c| *c >= 1)?;
    let load = f
        .next()
        .filter(|l| l.parse::<f64>().is_ok_and(f64::is_finite))?;
    let wsl_kb = f
        .next()
        .filter(|k| !k.is_empty() && k.bytes().all(|c| c.is_ascii_digit()))?
        .parse::<u64>()
        .ok()?;
    let windows_kb = f.next().map(str::trim).and_then(|w| {
        (!w.is_empty() && w.bytes().all(|c| c.is_ascii_digit()))
            .then(|| w.parse::<u64>().ok())
            .flatten()
    });
    Some(Probe {
        cpus,
        load: load.to_owned(),
        wsl_mb: wsl_kb / 1024,
        windows_mb: windows_kb.map(|k| k / 1024),
    })
}
/// Where the call runs (R4), with what the probe read where it answered.
enum Place {
    Pc { probe: Probe, head: Option<String> },
    Here(&'static str, Option<Probe>),
}
/// `tailscale` with `words`, in an environment of `TAILSCALE_ENV` alone.
fn tailscale(words: &[&str]) -> Command {
    let mut c = Command::new("tailscale");
    c.args(words).env_clear();
    for k in TAILSCALE_ENV {
        if let Some(v) = std::env::var_os(k) {
            c.env(k, v);
        }
    }
    c
}
/// The standard output of a short call that succeeded within `limit`; None otherwise.
fn answer(mut c: Command, limit: Duration) -> Option<Vec<u8>> {
    let mut child = c
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .ok()?;
    let out = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = vec![];
        let _ = out.take(1 << 20).read_to_end(&mut bytes);
        let _ = tx.send(bytes);
    });
    let status = wait_until(&mut child, Instant::now() + limit)?;
    let bytes = rx.recv_timeout(Duration::from_secs(2)).ok()?;
    status.success().then_some(bytes)
}
fn place(pc: &Pc, key: &str) -> Place {
    let ping = [
        "ping",
        "--c",
        "1",
        "--timeout",
        "10s",
        "--until-direct=false",
        pc.host(),
    ];
    if answer(tailscale(&ping), Duration::from_secs(12)).is_none() {
        return Place::Here("pc_unreachable", None);
    }
    let repo = format!("{}/repos/{key}", pc.work);
    let probe = format!("bash -c {} ap-check-probe {}", quote(PROBE), quote(&repo));
    let Some(stdout) = answer(
        tailscale(&["ssh", &pc.ssh, &probe]),
        Duration::from_secs(30),
    ) else {
        return Place::Here("pc_unreachable", None);
    };
    let text = String::from_utf8_lossy(&stdout);
    let mut lines = text.lines();
    let Some(probe) = probe_line(lines.next().unwrap_or("")) else {
        return Place::Here("pc_unreachable", None);
    };
    match placement(pc, &probe) {
        Some(reason) => Place::Here(reason, Some(probe)),
        None => Place::Pc {
            probe,
            head: lines.next().map(str::to_owned).filter(|h| is_hex(h)),
        },
    }
}
/// None when the PC takes the call: its load below `busy_load` times its CPUs and its available
/// memory at least `min_available_mb`; else the reason it runs here (R4).
pub fn placement(pc: &Pc, probe: &Probe) -> Option<&'static str> {
    if probe.load_value() >= pc.busy_load * probe.cpus as f64 {
        Some("pc_busy")
    } else if probe.available_mb() < pc.min_available_mb {
        Some("pc_memory_low")
    } else {
        None
    }
}

/// The command in the worktree on this host (R5), bounded by the deadline (R10).
fn here(call: &Call, tree: &Tree, deadline: Instant) -> i32 {
    if Instant::now() >= deadline {
        return 124;
    }
    let lock = variable("HOST_CARGO_LOCK", DEFAULT_LOCK);
    let mut argv = call.command.clone();
    // `./check` takes the lock by itself, with any of its arguments; taking it here too would wait on itself.
    if argv[0] != "./check" && Path::new(&lock).exists() {
        argv.splice(0..0, ["flock".to_owned(), lock]);
    }
    let (program, rest) = argv.split_first().unwrap_or((&argv[0], &[]));
    let path = if program.contains('/') && !program.starts_with('/') {
        Path::new(&tree.dir).join(program)
    } else {
        PathBuf::from(program)
    };
    let child = Command::new(path)
        .arg0(program)
        .args(rest)
        .current_dir(&tree.dir)
        .stdin(Stdio::null())
        .process_group(0)
        .spawn();
    let Ok(mut child) = child else {
        say("check: the command could not be started");
        return 127;
    };
    wait_until(&mut child, deadline).map_or(124, code)
}

/// Removes the staging directory of one call's payload.
struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn to_file(dir: &str, words: &[&str], file: &Path) -> Result<(), &'static str> {
    let out = File::create(file).map_err(|_| "stage")?;
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(words)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if ok { Ok(()) } else { Err("git") }
}
fn bytes_of(path: Option<PathBuf>) -> Vec<u8> {
    path.and_then(|p| fs::read(p).ok()).unwrap_or_default()
}
/// The exclude files the worktree uses: `info/exclude` and the user's global excludes file.
fn excludes(dir: &str) -> (Vec<u8>, Vec<u8>) {
    let info = git(
        dir,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/exclude",
        ],
    )
    .and_then(line)
    .map(PathBuf::from);
    let global = git(dir, &["config", "--path", "--get", "core.excludesFile"])
        .and_then(line)
        .filter(|p| !p.is_empty())
        .map(|p| Path::new(dir).join(p))
        .or_else(|| {
            let xdg = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty());
            match xdg {
                Some(x) => Some(PathBuf::from(x).join("git/ignore")),
                None => std::env::var_os("HOME")
                    .filter(|v| !v.is_empty())
                    .map(|h| PathBuf::from(h).join(".config/git/ignore")),
            }
        });
    (bytes_of(info), bytes_of(global))
}
/// The payload (R6, R7): the commit (a bundle of what the PC's tree lacks), the index entries, the
/// two exclude files and a tar of the files Git lists, with their working-tree bytes and modes.
fn payload(tree: &Tree, pc_head: Option<&str>, stage: &Path) -> Result<PathBuf, &'static str> {
    let parts = stage.join("p");
    fs::create_dir(&parts).map_err(|_| "stage")?;
    let dir = tree.dir.as_str();
    fs::write(parts.join("head"), &tree.head).map_err(|_| "stage")?;
    let known =
        pc_head.filter(|h| git(dir, &["cat-file", "-e", &format!("{h}^{{commit}}")]).is_some());
    let needed = match known {
        Some(h) => git(dir, &["merge-base", "--is-ancestor", "HEAD", h]).is_none(),
        None => true,
    };
    let mut members = vec![
        "head",
        "index-info",
        "exclude",
        "global-excludes",
        "tree.tar",
    ];
    if needed {
        let file = parts.join("head.bundle");
        let file = file.to_str().ok_or("stage")?;
        let not = known.map(|h| format!("^{h}"));
        let mut words = vec!["bundle", "create", "-q", file, "HEAD"];
        words.extend(not.as_deref());
        git(dir, &words).ok_or("bundle")?;
        members.push("head.bundle");
    }
    to_file(dir, &["ls-files", "-s", "-z"], &parts.join("index-info"))?;
    let (info, global) = excludes(dir);
    fs::write(parts.join("exclude"), info).map_err(|_| "stage")?;
    fs::write(parts.join("global-excludes"), global).map_err(|_| "stage")?;
    let listed = git(
        dir,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )
    .ok_or("git")?;
    let mut seen = HashSet::new();
    let mut list = vec![];
    for p in listed.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        // A tracked file deleted from the working tree is absent on the PC as well.
        if seen.insert(p) && fs::symlink_metadata(Path::new(dir).join(OsStr::from_bytes(p))).is_ok()
        {
            list.extend_from_slice(p);
            list.push(0);
        }
    }
    let names = stage.join("names");
    fs::write(&names, list).map_err(|_| "stage")?;
    let tar = |words: Vec<&OsStr>| {
        Command::new("tar")
            .args(words)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    };
    let tree_tar = parts.join("tree.tar");
    let ok = tar(vec![
        OsStr::new("--null"),
        OsStr::new("--verbatim-files-from"),
        OsStr::new("--no-recursion"),
        OsStr::new("-C"),
        OsStr::new(dir),
        OsStr::new("-cf"),
        tree_tar.as_os_str(),
        OsStr::new("-T"),
        names.as_os_str(),
    ]);
    if !ok {
        return Err("tar");
    }
    let out = stage.join("payload.tar");
    let mut words = vec![
        OsStr::new("-C"),
        parts.as_os_str(),
        OsStr::new("-cf"),
        out.as_os_str(),
    ];
    words.extend(members.iter().map(OsStr::new));
    if !tar(words) {
        return Err("tar");
    }
    Ok(out)
}

/// The PC's standard error as it comes, less the line that carries the command's status and the
/// lowest `MemAvailable` of the PC's Linux while it ran (R11, R13): `<status> <MB>`, or `<status> -`
/// where no command ran.
pub struct Filter {
    marker: Vec<u8>,
    pending: Vec<u8>,
    done: bool,
    pub status: Option<i32>,
    pub min_wsl_available_mb: Option<u64>,
}
impl Filter {
    pub fn new(nonce: &str) -> Self {
        Filter {
            marker: format!("\0ap-check {nonce} ").into_bytes(),
            pending: vec![],
            done: false,
            status: None,
            min_wsl_available_mb: None,
        }
    }
    /// The bytes to pass on now; a tail that may begin the status line is held back.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<u8> {
        if self.done {
            return vec![];
        }
        self.pending.extend_from_slice(chunk);
        let m = self.marker.len();
        if let Some(i) = self.pending.windows(m).position(|w| w == self.marker) {
            let emit = self.pending[..i].to_vec();
            let tail = &self.pending[i + m..];
            if let Some(nl) = tail.iter().position(|b| *b == b'\n') {
                let fields = std::str::from_utf8(&tail[..nl])
                    .ok()
                    .and_then(|s| s.split_once(' '))
                    .and_then(|(status, low)| {
                        let status = status
                            .parse::<i32>()
                            .ok()
                            .filter(|s| (0..=255).contains(s))?;
                        let low = match low {
                            "-" => None,
                            mb if !mb.is_empty() && mb.bytes().all(|c| c.is_ascii_digit()) => {
                                Some(mb.parse::<u64>().ok()?)
                            }
                            _ => return None,
                        };
                        Some((status, low))
                    });
                self.status = fields.map(|f| f.0);
                self.min_wsl_available_mb = fields.and_then(|f| f.1);
                self.done = true;
                self.pending.clear();
            } else {
                self.pending.drain(..i);
            }
            return emit;
        }
        let most = (m - 1).min(self.pending.len());
        let keep = (1..=most)
            .rev()
            .find(|k| self.pending.ends_with(&self.marker[..*k]))
            .unwrap_or(0);
        let cut = self.pending.len() - keep;
        self.pending.drain(..cut).collect()
    }
    /// At the end of the stream: what was held back, unless it was the status line.
    pub fn finish(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }
}
fn relay(mut pipe: impl Read, nonce: &str) -> Option<(i32, Option<u64>)> {
    let mut filter = Filter::new(nonce);
    let mut buf = [0u8; 8192];
    let mut err = std::io::stderr();
    loop {
        let n = match pipe.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        let _ = err.write_all(&filter.feed(&buf[..n]));
    }
    let _ = err.write_all(&filter.finish());
    filter.status.map(|s| (s, filter.min_wsl_available_mb))
}

/// The command on the PC (R6 to R11): Ok with its exit status and the lowest `MemAvailable` of the
/// PC's Linux while it ran (None where no command ran), Err when that status never came back.
fn on_pc(
    pc: &Pc,
    call: &Call,
    tree: &Tree,
    key: &str,
    pc_head: Option<&str>,
    deadline: Instant,
) -> Result<(i32, Option<u64>), ()> {
    let nonce = new_session_id(&Host).map_err(|_| say("check: pc: no nonce"))?;
    let stage = std::env::temp_dir().join(format!("ap-check.{nonce}"));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&stage)
        .map_err(|_| say("check: pc: no staging directory"))?;
    let stage = Stage(stage);
    let payload = payload(tree, pc_head, &stage.0)
        .map_err(|step| say(&format!("check: pc: the payload failed at {step}")))?;
    let left = deadline
        .saturating_duration_since(Instant::now())
        .as_secs()
        .max(1);
    let mut remote = format!(
        "bash -c {} ap-check {} {key} {nonce} {left}",
        quote(PC_SCRIPT),
        quote(&pc.work)
    );
    for w in &call.command {
        remote.push(' ');
        remote.push_str(&quote(w));
    }
    let input = File::open(&payload).map_err(|_| say("check: pc: no payload"))?;
    let child = tailscale(&["ssh", &pc.ssh, &remote])
        .stdin(input)
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn();
    let Ok(mut child) = child else {
        return Err(());
    };
    let (tx, rx) = mpsc::channel();
    if let Some(pipe) = child.stderr.take() {
        let nonce = nonce.clone();
        std::thread::spawn(move || {
            let _ = tx.send(relay(pipe, &nonce));
        });
    }
    let ended = wait_until(&mut child, deadline + GRACE);
    let status = rx.recv_timeout(Duration::from_secs(5)).ok().flatten();
    drop(stage);
    match (status, ended) {
        (Some(s), _) => Ok(s),
        // Cut at the deadline's grace: the PC's own bound ends what is left there.
        (None, None) => Ok((124, None)),
        (None, Some(_)) => Err(()),
    }
}

/// The record line (R15); a write that fails is said on standard error and changes no exit status.
#[allow(clippy::too_many_arguments)]
fn record(
    start: SystemTime,
    seconds: f64,
    host: &str,
    reason: Option<&str>,
    tree: &Tree,
    call: &Call,
    exit: i32,
    probe: Option<&Probe>,
    min_wsl_available_mb: Option<u64>,
) {
    let line = json!({
        "start": stamp(start),
        "end": stamp(SystemTime::now()),
        "seconds": seconds,
        "host": host,
        "reason": reason,
        "dir": tree.dir,
        "repository": tree.repository,
        "head": tree.head,
        "command": call.command,
        "exit": exit,
        "pc_cpus": probe.map(|p| p.cpus),
        "pc_load": probe.map(Probe::load_value),
        "pc_wsl_available_mb": probe.map(|p| p.wsl_mb),
        "pc_windows_available_mb": probe.and_then(|p| p.windows_mb),
        "pc_available_mb": probe.map(Probe::available_mb),
        "pc_min_wsl_available_mb": min_wsl_available_mb,
    });
    let path = variable("AP_CHECKS_LOG", DEFAULT_LOG);
    let written = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .and_then(|mut f| f.write_all(format!("{line}\n").as_bytes()));
    if written.is_err() {
        say(&format!(
            "check: the record could not be appended to {path}"
        ));
    }
}

pub fn cli(argv: &[OsString]) -> ExitCode {
    let start = SystemTime::now();
    let started = Instant::now();
    let (call, tree, pc) = match prepare(&Host, argv) {
        Ok(p) => p,
        Err(e) => {
            let v = json!({"error": {"code": e.code, "field": e.field, "detail": e.detail}});
            return match writeln!(std::io::stderr(), "{v}") {
                Ok(()) => ExitCode::from(2),
                Err(_) => ExitCode::from(4),
            };
        }
    };
    let deadline = started + Duration::from_secs(call.timeout);
    let key = digest(tree.dir.as_bytes())[..16].to_owned();
    let local = |reason: &'static str| {
        say(&format!("check: on this host ({reason})"));
        ("local", Some(reason), here(&call, &tree, deadline))
    };
    let (probe, low, (host, reason, exit)) = match &pc {
        None => (None, None, local("no_pc_in_policy")),
        Some(pc) => match place(pc, &key) {
            Place::Here(reason, probe) => (probe, None, local(reason)),
            Place::Pc { probe, head } => {
                say(&format!(
                    "check: on pc ({} CPUs, load {}, {} MB available)",
                    probe.cpus,
                    probe.load,
                    probe.available_mb()
                ));
                match on_pc(pc, &call, &tree, &key, head.as_deref(), deadline) {
                    Ok((exit, low)) => (Some(probe), low, ("pc", None, exit)),
                    Err(()) => (Some(probe), None, local("pc_lost")),
                }
            }
        },
    };
    let seconds = (started.elapsed().as_millis() as f64) / 1000.0;
    record(
        start,
        seconds,
        host,
        reason,
        &tree,
        &call,
        exit,
        probe.as_ref(),
        low,
    );
    ExitCode::from(u8::try_from(exit).unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    // spec: specs/ap/check.md#R11
    // spec: specs/ap/check.md#R13
    #[test]
    fn the_status_line_is_taken_out_wherever_the_stream_splits_it() {
        let nonce = "0f0e0d0c-0b0a-4908-8706-050403020100";
        for (low, want) in [("1234", Some(1234)), ("-", None)] {
            let text = format!("one\ntwo\0ap-check {nonce} 255 {low}\n");
            let bytes = text.as_bytes();
            for cut in 0..bytes.len() {
                let mut f = Filter::new(nonce);
                let mut out = f.feed(&bytes[..cut]);
                out.extend(f.feed(&bytes[cut..]));
                out.extend(f.finish());
                assert_eq!(out, b"one\ntwo", "cut at {cut}");
                assert_eq!(f.status, Some(255), "cut at {cut}");
                assert_eq!(f.min_wsl_available_mb, want, "cut at {cut}");
            }
        }
        // A status line without both fields, or with another second field, carries no status.
        for bad in ["0", "0 ", "0 x", "0 -1", "0 1 2", "256 1"] {
            let mut f = Filter::new(nonce);
            let out = f.feed(format!("\0ap-check {nonce} {bad}\n").as_bytes());
            assert!(out.is_empty(), "{bad}");
            assert_eq!(f.status, None, "{bad}");
        }
        // Without the line the status is unknown and every byte is passed on.
        let mut f = Filter::new(nonce);
        let mut out = f.feed(b"partial\0ap-check ");
        out.extend(f.finish());
        assert_eq!(out, b"partial\0ap-check ");
        assert_eq!(f.status, None);
        // Another nonce is the command's own text.
        let mut f = Filter::new(nonce);
        let other = b"x\0ap-check 00000000-0000-4000-8000-000000000000 0 1\n";
        let mut out = f.feed(other);
        out.extend(f.finish());
        assert_eq!(out, other);
        assert_eq!(f.status, None);
    }

    // spec: specs/ap/check.md#R2
    #[test]
    fn github_remotes_are_named_by_owner_and_name() {
        for (url, want) in [
            ("https://github.com/o/r.git", Some("o/r")),
            ("https://github.com/o/r", Some("o/r")),
            ("https://x-access-token@github.com/o/r.git", Some("o/r")),
            ("git@github.com:o/r.git", Some("o/r")),
            ("ssh://git@github.com/o/r", Some("o/r")),
            ("https://gitlab.com/o/r.git", None),
            ("https://github.com.evil/o/r", None),
            ("/srv/r.git", None),
            ("https://github.com/o", None),
            ("https://github.com/o/r/x", None),
        ] {
            assert_eq!(github(url).as_deref(), want, "{url}");
        }
    }

    // spec: specs/ap/check.md#R16
    #[test]
    fn the_member_hosts_is_read_or_refused_by_its_member() {
        let pc = json!({"ssh": "u@pc-1", "work": "/home/u/work/ap-checks", "busy_load": 0.75});
        assert_eq!(hosts(&json!({})).unwrap(), None);
        assert_eq!(
            hosts(&json!({"hosts": {"pc": pc}})).unwrap(),
            Some(Pc {
                ssh: "u@pc-1".into(),
                work: "/home/u/work/ap-checks".into(),
                busy_load: 0.75,
                min_available_mb: 2048,
            })
        );
        let refused = |v: Value| hosts(&json!({"hosts": v})).unwrap_err().detail;
        let starts = |v: Value, member: &str| {
            let d = refused(v);
            assert!(
                d.as_str().unwrap().starts_with(&format!("{member} ")),
                "{d}"
            );
        };
        starts(json!([]), "hosts");
        starts(json!({"pc": pc, "other": pc}), "hosts");
        starts(json!({"pc": {"ssh": "u@h", "work": "/w"}}), "hosts.pc");
        let with = |k: &str, v: Value| {
            let mut p = pc.clone();
            p[k] = v;
            json!({"pc": p})
        };
        for bad in ["u", "u@", "@h", "U@h", "u@h;x", "u@h h", "u@-h"] {
            starts(with("ssh", json!(bad)), "hosts.pc.ssh");
        }
        for bad in ["w", "/", "/a/../b", "/a//b", "/a/", "/a b", "/a/$x"] {
            starts(with("work", json!(bad)), "hosts.pc.work");
        }
        for bad in [json!(0.05), json!(1.5), json!("0.5"), json!(null)] {
            starts(with("busy_load", bad), "hosts.pc.busy_load");
        }
        assert_eq!(
            hosts(&json!({"hosts": with("busy_load", json!(1))}))
                .unwrap()
                .unwrap()
                .busy_load,
            1.0
        );
        for bad in [
            json!(-1),
            json!(1_048_577),
            json!(2048.5),
            json!("2048"),
            json!(null),
        ] {
            starts(with("min_available_mb", bad), "hosts.pc.min_available_mb");
        }
        for good in [0, 4096, 1_048_576] {
            let read = hosts(&json!({"hosts": with("min_available_mb", json!(good))}));
            assert_eq!(read.unwrap().unwrap().min_available_mb, good);
        }
        // Any other new member is refused as before.
        starts(with("max_jobs", json!(4)), "hosts.pc");
    }

    // spec: specs/ap/check.md#R4
    #[test]
    fn the_probe_reads_cpus_load_and_both_memories_in_mb_rounded_down() {
        let p = probe_line("22 1.50 15022080 3761152\r ").unwrap();
        assert_eq!(
            p,
            Probe {
                cpus: 22,
                load: "1.50".into(),
                wsl_mb: 14670,
                windows_mb: Some(3673)
            }
        );
        assert_eq!(p.available_mb(), 3673);
        // 2,047 MB and 1,023 kB is 2,047 MB.
        assert_eq!(probe_line("4 0.1 2097151 ").unwrap().wsl_mb, 2047);
        // Windows answers nothing, a failure or no number: the Linux reading alone.
        for windows in ["", " ", "x", "12 34", "-5", "1.5"] {
            let p = probe_line(&format!("4 0.1 2097152 {windows}")).unwrap();
            assert_eq!(
                (p.windows_mb, p.available_mb()),
                (None, 2048),
                "{windows:?}"
            );
        }
        assert_eq!(
            probe_line("4 0.1 409600").unwrap().windows_mb,
            None,
            "no fourth field"
        );
        // The CPUs, the load or the Linux memory unread: no answer.
        for bad in [
            "",
            "4 0.1  100",
            "4 0.1 x 100",
            "0 0.1 100 100",
            "4 x 100 100",
            "4 0.1",
        ] {
            assert_eq!(probe_line(bad), None, "{bad:?}");
        }
    }

    // spec: specs/ap/check.md#R4
    #[test]
    fn the_pc_takes_the_call_when_not_busy_and_with_enough_memory() {
        let pc = Pc {
            ssh: "u@h".into(),
            work: "/w".into(),
            busy_load: 0.75,
            min_available_mb: 2048,
        };
        let probe = |load: &str, wsl: u64, windows: Option<u64>| Probe {
            cpus: 4,
            load: load.into(),
            wsl_mb: wsl,
            windows_mb: windows,
        };
        assert_eq!(placement(&pc, &probe("2.99", 2048, None)), None);
        assert_eq!(placement(&pc, &probe("2.99", 9000, Some(2048))), None);
        assert_eq!(placement(&pc, &probe("3.00", 9000, None)), Some("pc_busy"));
        // Busy and short of memory: busy.
        assert_eq!(placement(&pc, &probe("3.00", 10, None)), Some("pc_busy"));
        assert_eq!(
            placement(&pc, &probe("0.1", 2047, Some(9000))),
            Some("pc_memory_low")
        );
        assert_eq!(
            placement(&pc, &probe("0.1", 9000, Some(2047))),
            Some("pc_memory_low")
        );
        let none = Pc {
            min_available_mb: 0,
            ..pc
        };
        assert_eq!(placement(&none, &probe("0.1", 0, Some(0))), None);
    }

    #[test]
    fn a_quoted_word_is_one_word_of_the_shell() {
        let word = "it's $HOME `x` \"y\"";
        let out = Command::new("sh")
            .args(["-c", &format!("printf %s {}", quote(word))])
            .output()
            .unwrap();
        assert_eq!(out.stdout, word.as_bytes());
    }
}
