//! The door of a stage with `limits` (`specs/ap/door.md`), with `ap check` under it
//! (`specs/ap/check.md` R20) and `ap run`'s part (`specs/ap/run-limits.md` R13). The door is driven
//! through `ap::run::execute` with a port of this file's own: its `launch` drops `systemd-run … --`
//! and `bwrap … --` and starts a fake CLI, its `environment()` names a `PATH` of fakes (the fake PC's
//! `tailscale` of `tests/check.rs`, an `ont` that notes its arguments, input and environment, a
//! `git` that notes its arguments) and record files under the test's directory. The clients are the
//! built `ap` with `AP_DOOR`, or a socket of the test that speaks the door's bytes. Nothing here
//! reaches the host's store, the real `ont`, the tailnet, the host's policy file or a file under
//! `/var/lib/agent-platform`.
#[path = "fixtures/work/forks.rs"]
mod forks;
use ap::host::{Answer, Failure, Host, Reply, System};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    os::unix::{
        fs::{FileTypeExt, PermissionsExt},
        net::UnixStream,
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    thread::JoinHandle,
    time::{Duration, Instant},
};

static NEXT: AtomicU32 = AtomicU32::new(0);
const BWRAP: &str = "/usr/bin/bwrap";

/// The fake CLI of a stage: notes its environment and whether its door is a socket, runs the file
/// `then` when there is one (its output, errors and status kept), waits for `go` while `wait`
/// exists, and answers.
const CLI: &str = r#"#!/bin/bash
here=$(cd "$(dirname "$0")" && pwd)
n=$(( $(cat "$here/count" 2>/dev/null || echo 0) + 1 )); echo "$n" > "$here/count"
tr '\0' '\n' < /proc/$$/environ > "$here/env.$n"
cat > /dev/null
if [ -S "${AP_DOOR:-/nonexistent}" ]; then echo socket > "$here/door.$n"; else echo none > "$here/door.$n"; fi
if [ -f "$here/then" ]; then bash "$here/then" > "$here/then.out" 2> "$here/then.err"; echo $? > "$here/then.rc"; fi
if [ -e "$here/wait" ]; then for _ in $(seq 600); do [ -e "$here/go" ] && break; sleep 0.1; done; fi
sid=; prev=; for a in "$@"; do case "$prev" in --session-id|--resume) sid=$a ;; esac; prev=$a; done
printf '{"type":"system","subtype":"init","session_id":"%s","mcp_servers":[]}\n' "$sid"
printf '{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"Verdict: accept","session_id":"%s","usage":{"input_tokens":1,"output_tokens":1},"permission_denials":[]}\n' "$sid"
"#;

/// The fake engine: appends its arguments to `ont.args`, keeps its input and its whole environment,
/// and answers by the file `ont.mode`: `refuse` (exit 2), `wait` (notes its start and end around a
/// second), `flood` (3 MiB of output), else one answer line and one line of errors.
const ONT: &str = r#"#!/bin/sh
d=@D@
printf '%s\n' "$*" >> "$d/ont.args"
cat > "$d/ont.input"
tr '\0' '\n' < /proc/$$/environ > "$d/ont.env"
case $(cat "$d/ont.mode" 2>/dev/null) in
refuse) echo '{"status":"refused","code":"invalid_parameter"}'; echo 'refused here' >&2; exit 2 ;;
wait) echo "start" >> "$d/ont.times"; sleep 1; echo "end" >> "$d/ont.times"; echo '{"answers":[]}' ;;
flood) head -c 3145728 /dev/zero | tr '\0' 'x' ;;
*) echo '{"answers":[{"path":"specs/ap/door.md"}]}'; echo 'engine note' >&2 ;;
esac
"#;

/// A `git` that notes its arguments and runs the real one.
const GIT: &str = "#!/bin/sh\nprintf '%s\\n' \"$*\" >> @D@/git.args\nexec /usr/bin/git \"$@\"\n";
/// The `tailscale` of a client: it notes that it ran.
const CLIENT_TAILSCALE: &str = "#!/bin/sh\necho \"$*\" >> @D@/client-tailscale\nexit 1\n";

/// The fake PC of `tests/check.rs`, its state in the directory @PC@.
const TAILSCALE: &str = r#"#!/bin/sh
pc=@PC@
run() {
  set --
  for v in $(env | sed -n 's/^\(LANG\|LC_[A-Za-z0-9_]*\)=.*/\1/p'); do eval "set -- \"\$@\" \"$v=\${$v}\""; done
  env -i HOME="$pc/home" PATH="$pc/bin:/usr/bin:/bin" "$@" sh -c "$cmd"
}
case $1 in
ping)
  echo ping >> "$pc/calls"
  if [ -e "$pc/down" ]; then exit 1; fi
  exit 0 ;;
ssh)
  shift; dest=$1; shift; cmd=$*
  echo "ssh $dest" >> "$pc/calls"
  case $cmd in
  *ap-check-probe*)
    cmd=$(printf '%s' "$cmd" | sed "s#/proc/loadavg#$pc/loadavg#; s#/proc/meminfo#$pc/meminfo#; s#/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe#$pc/powershell.exe#; s#(nproc)#(echo 4)#")
    run < /dev/null; exit ;;
  esac
  cmd=$(printf '%s' "$cmd" | sed "s#/proc/meminfo#$pc/meminfo#g")
  cat > "$pc/payload.tar"
  echo $$ > "$pc/ssh.pid"
  [ ! -e "$pc/lose" ] || exit 255
  run < "$pc/payload.tar"; exit ;;
esac
exit 2
"#;
const POWERSHELL: &str = "#!/bin/sh\nprintf '%s\\r\\n' 8388608\n";
/// The stand-in `bwrap` of `tests/check.rs`.
const PC_BWRAP: &str = r#"#!/bin/sh
while [ $# -gt 0 ]; do
  case $1 in
  --) shift; break ;;
  --chdir) cd "$2" || exit 1; shift 2 ;;
  --setenv) export "$2=$3"; shift 3 ;;
  --unsetenv) unset "$2"; shift 2 ;;
  --ro-bind)
    case $3 in
    @PC@/*/.git) [ "$2" = "$3" ] || { rm -rf "$3"; ln -s "$2" "$3"; } ;;
    esac
    shift 3 ;;
  --bind | --symlink) shift 3 ;;
  --dev | --proc | --uid | --gid | --remount-ro | --tmpfs | --dir) shift 2 ;;
  *) shift ;;
  esac
done
exec "$@"
"#;
/// The worktree's check: where it ran, one line of errors, exit 3.
const CHECK: &str = "#!/bin/sh\necho \"check ran in $PWD\"\necho \"check err\" >&2\nexit 3\n";

/// True on the PC, where `ap check` sets `AP_CHECK_HOST=pc` (`specs/ap/check.md` R17) and no systemd
/// is reachable: the tests that start the door return at their start, with one line on standard
/// error. This host and CI run them.
fn on_pc() -> bool {
    let pc = std::env::var("AP_CHECK_HOST").is_ok_and(|v| v == "pc");
    if pc {
        // Through the handle, not `eprintln!`, which the test harness would hold back.
        let _ = writeln!(std::io::stderr(), "door: skipped (pc)");
    }
    pc
}

fn write(path: &Path, text: &str, mode: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn kill_group(pgid: u32) {
    let _ = forks::forking(|| {
        Command::new("kill")
            .args(["-KILL", "--", &format!("-{pgid}")])
            .stderr(Stdio::null())
            .status()
    });
}
fn alive(pid: &str) -> bool {
    let stat = fs::read_to_string(format!("/proc/{}/stat", pid.trim()));
    stat.is_ok_and(|s| s.split_whitespace().nth(2) != Some("Z"))
}
fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(
            start.elapsed() < Duration::from_secs(60),
            "waited for {what}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// The port of a stage: the fake CLI under `<t>/bin`, the fakes the door finds under `<t>/fakes`,
/// the fake PC under `<t>/pc`, the worktree `<t>/repo`.
struct Fake {
    t: PathBuf,
    env: Vec<(String, String)>,
    policy: Option<PathBuf>,
    launches: Vec<Vec<String>>,
    launch_envs: Vec<Vec<(String, String)>>,
    groups: BTreeMap<String, u32>,
    alive: BTreeSet<String>,
}
impl Fake {
    fn new() -> Self {
        forks::gap();
        let base = std::env::temp_dir().canonicalize().unwrap();
        let t = base.join(format!(
            "ap-stage-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&t);
        for d in ["a1", "home", "work", "tmp", "elsewhere"] {
            fs::create_dir_all(t.join(d)).unwrap();
        }
        let s = |rel: &str| t.join(rel).to_str().unwrap().to_owned();
        write(&t.join("bin/claude"), CLI, 0o755);
        std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_ap"), t.join("bin/ap")).unwrap();
        write(&t.join("fakes/ont"), &ONT.replace("@D@", &s("")), 0o755);
        write(&t.join("fakes/git"), &GIT.replace("@D@", &s("")), 0o755);
        write(
            &t.join("fakes/tailscale"),
            &TAILSCALE.replace("@PC@", &s("pc")),
            0o755,
        );
        write(
            &t.join("client/tailscale"),
            &CLIENT_TAILSCALE.replace("@D@", &s("")),
            0o755,
        );
        write(
            &t.join("pc/bin/bwrap"),
            &PC_BWRAP.replace("@PC@", &s("pc")),
            0o755,
        );
        write(&t.join("pc/loadavg"), "0.10 0.10 0.10 1/100 1\n", 0o644);
        write(
            &t.join("pc/meminfo"),
            "MemTotal: 32000000 kB\nMemAvailable:   16777216 kB\n",
            0o644,
        );
        write(&t.join("pc/powershell.exe"), POWERSHELL, 0o755);
        fs::create_dir_all(t.join("pc/home")).unwrap();
        write(&t.join("gitconfig"), "", 0o644);
        fs::create_dir_all(t.join("xdg")).unwrap();
        write(&t.join("repo/check"), CHECK, 0o755);
        write(&t.join("repo/a.txt"), "a\n", 0o644);
        let made = forks::forking(|| {
            Command::new("bash")
                .args([
                    "-c",
                    "git init -q -b main . && git remote add origin https://github.com/example/repo.git \
                     && git add -A && git -c user.name=t -c user.email=t@e commit -q -m one",
                ])
                .current_dir(t.join("repo"))
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", t.join("gitconfig"))
                .output()
                .unwrap()
        });
        assert!(made.status.success(), "{made:?}");
        write(&t.join("prompt.md"), "work\n", 0o644);
        forks::settled();
        let path = format!("{}:{}:/usr/bin:/bin", s("fakes"), s("bin"));
        let env = vec![
            ("PATH".into(), path),
            ("HOME".into(), s("home")),
            ("LANG".into(), "C.UTF-8".into()),
            ("TMPDIR".into(), s("tmp")),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_CONFIG_GLOBAL".into(), s("gitconfig")),
            ("XDG_CONFIG_HOME".into(), s("xdg")),
            ("AP_DOOR_LOG".into(), s("door.jsonl")),
            ("AP_CHECKS_LOG".into(), s("checks.jsonl")),
            // Neither reaches the engine: the stage's role is its own.
            ("ONT_ROLE".into(), "lead".into()),
            ("X_SECRET".into(), "secret".into()),
        ];
        let f = Fake {
            t,
            env,
            policy: None,
            launches: vec![],
            launch_envs: vec![],
            groups: BTreeMap::new(),
            alive: BTreeSet::new(),
        };
        f.policy_with_pc(true);
        f
    }
    fn s(&self, rel: &str) -> String {
        self.t.join(rel).to_str().unwrap().to_owned()
    }
    /// The policy file: the model and account of the stage, and the fake PC when `pc`.
    fn policy_with_pc(&self, pc: bool) {
        let mut p = json!({"models": {"allowed": ["sonnet"], "never": []},
            "accounts": {"claude": [self.s("a1")]}});
        if pc {
            p["hosts"] = json!({"pc": {"ssh": "tester@fake-pc", "work": self.s("pc/work"), "busy_load": 0.75}});
        }
        write(&self.t.join("policy.json"), &p.to_string(), 0o644);
    }
    fn with_policy(mut self) -> Self {
        self.policy = Some(self.t.join("policy.json"));
        self
    }
    /// A request of `role` (`verifier`: a reviewer with `ont_role` `verifier`) with `limits`.
    fn request(&self, role: &str) -> Value {
        let mut r = json!({"role": role, "account": self.s("a1"), "model": "sonnet",
            "effort": "high", "permission_mode": "auto", "cwd": self.s("repo"),
            "prompt": self.s("prompt.md"), "out": self.s("x.md"),
            "limits": {"scratch": self.s("scratch")}});
        if role == "verifier" {
            r["role"] = json!("reviewer");
            r["ont_role"] = json!("verifier");
        }
        r
    }
    fn door(&self) -> String {
        self.s("scratch.door")
    }
    fn text(&self, rel: &str) -> String {
        fs::read_to_string(self.t.join(rel)).unwrap_or_default()
    }
    fn lines(&self, rel: &str) -> Vec<Value> {
        self.text(rel)
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    /// The stage's script, run with the stage's environment.
    fn then(&self, script: &str) {
        write(&self.t.join("bin/then"), script, 0o644);
    }
    /// The built `ap` as a client of the door at `door` (none: no `AP_DOOR`).
    fn ap(&self, door: Option<&str>, args: &[&str], input: &[u8]) -> Output {
        let mut c = self.client(door, args);
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = forks::forking(|| c.spawn()).unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        child.wait_with_output().unwrap()
    }
    fn client(&self, door: Option<&str>, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_ap"));
        c.args(args)
            .env_clear()
            .env("PATH", format!("{}:/usr/bin:/bin", self.s("client")))
            .env("HOME", self.s("home"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.s("gitconfig"))
            .env("AP_POLICY_FILE", self.s("policy.json"))
            .env("AP_CHECKS_LOG", self.s("client-checks.jsonl"))
            .env("HOST_CARGO_LOCK", self.s("build.lock"));
        if let Some(d) = door {
            c.env("AP_DOOR", d);
        }
        c
    }
    /// Starts the stage on a thread; with `hold` its CLI waits for `finish`.
    fn start(self, r: Value, hold: bool) -> Running {
        let t = self.t.clone();
        if hold {
            fs::write(t.join("bin/wait"), "").unwrap();
        }
        let handle = std::thread::spawn(move || {
            let mut f = self;
            let result = ap::run::execute(&mut f, &r);
            (f, result)
        });
        let running = Running { t, handle };
        if hold {
            wait_for("the stage", || running.t.join("bin/door.1").exists());
        }
        running
    }
    fn run(self, r: Value) -> (Fake, Result<(Value, u8), Failure>) {
        self.start(r, false).finish()
    }
}
/// What a stage's thread returns: its port and the answer of `execute`.
type Done = (Fake, Result<(Value, u8), Failure>);
struct Running {
    t: PathBuf,
    handle: JoinHandle<Done>,
}
impl Running {
    fn finish(self) -> (Fake, Result<(Value, u8), Failure>) {
        let _ = fs::write(self.t.join("bin/go"), "");
        self.handle.join().unwrap()
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.t);
    }
}
impl System for Fake {
    fn call(&mut self, program: &str, args: &[String], input: &Value, ms: u64) -> Answer {
        let reply = |success: bool, out: &[u8]| {
            Ok(Reply {
                success,
                bytes: out.to_vec(),
                stdout: out.to_vec(),
            })
        };
        match program {
            BWRAP => reply(true, b"bubblewrap 0.9.0\n"),
            // The stage runner's own store calls fail: the stage goes on (`specs/ap/run-object.md`).
            "ont" => Err(Failure {
                code: "process_failed",
                field: "ont".into(),
                detail: Value::Null,
            }),
            "systemctl" => {
                let unit = args.last().unwrap();
                match args[0].as_str() {
                    "stop" => {
                        if let Some(pgid) = self.groups.get(unit) {
                            kill_group(*pgid);
                        }
                        self.alive.remove(unit);
                        reply(true, b"")
                    }
                    _ if self.alive.contains(unit) => reply(true, b"active\n"),
                    _ => reply(false, b"inactive\n"),
                }
            }
            _ => forks::forking(|| Host.call(program, args, input, ms)),
        }
    }
    fn launch(
        &mut self,
        argv: &[String],
        env: &[(String, String)],
        cwd: &str,
        files: [fs::File; 3],
    ) -> std::io::Result<Child> {
        self.launches.push(argv.to_vec());
        self.launch_envs.push(env.to_vec());
        let unit_at = argv.iter().position(|a| a == "--").unwrap();
        assert_eq!(&argv[..2], ["systemd-run", "--scope"]);
        let at = match argv[unit_at + 1] == BWRAP {
            true => unit_at + 1 + argv[unit_at + 1..].iter().position(|a| a == "--").unwrap(),
            false => unit_at,
        };
        let unit = argv[..unit_at]
            .iter()
            .find_map(|a| a.strip_prefix("--unit="))
            .unwrap()
            .to_owned();
        forks::settled();
        let child = forks::forking(|| {
            Command::new(self.t.join("bin").join(&argv[at + 1]))
                .args(&argv[at + 2..])
                .env_clear()
                .envs(env.iter().map(|(k, v)| (k, v)))
                .current_dir(cwd)
                .stdin(files[0].try_clone()?)
                .stdout(files[1].try_clone()?)
                .stderr(files[2].try_clone()?)
                .process_group(0)
                .spawn()
        })?;
        self.groups.insert(unit.clone(), child.id());
        self.alive.insert(unit);
        Ok(child)
    }
    fn home(&self) -> Option<PathBuf> {
        Some(self.t.join("home"))
    }
    fn work_root(&self) -> PathBuf {
        self.t.join("work")
    }
    fn policy_file(&self) -> Option<PathBuf> {
        self.policy.clone()
    }
    fn environment(&self) -> Vec<(String, String)> {
        self.env.clone()
    }
}

/// A client of the test's own: sends `line` as it is and reads the answer's frames: the two
/// streams and the closing message (None when the answer ends without one).
fn raw(door: &str, line: &[u8]) -> (Vec<u8>, Vec<u8>, Option<Value>) {
    let mut s = UnixStream::connect(door).unwrap();
    s.write_all(line).unwrap();
    frames(&mut s)
}
fn frames(s: &mut UnixStream) -> (Vec<u8>, Vec<u8>, Option<Value>) {
    let (mut out, mut err) = (vec![], vec![]);
    loop {
        let mut head = [0u8; 5];
        if s.read_exact(&mut head).is_err() {
            return (out, err, None);
        }
        let len = u32::from_be_bytes([head[1], head[2], head[3], head[4]]) as usize;
        let mut bytes = vec![0u8; len];
        s.read_exact(&mut bytes).unwrap();
        match head[0] {
            1 => out.extend(bytes),
            2 => err.extend(bytes),
            3 => return (out, err, Some(serde_json::from_slice(&bytes).unwrap())),
            other => panic!("tag {other}"),
        }
    }
}
fn ask(door: &str, request: &Value) -> (Vec<u8>, Vec<u8>, Option<Value>) {
    raw(door, format!("{request}\n").as_bytes())
}
fn check_request(dir: &str, command: &[&str]) -> Value {
    json!({"door": 1, "verb": "check", "dir": dir, "timeout": 120, "command": command})
}
fn refused(code: &str, field: &str) -> Option<Value> {
    Some(json!({"refused": {"code": code, "field": field}}))
}
fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}
/// Each line of the door's record has exactly its eight members.
fn eight(line: &Value) {
    let keys: BTreeSet<&str> = line
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let want: BTreeSet<&str> = ["at", "run", "role", "verb", "name", "ms", "exit", "reason"].into();
    assert_eq!(keys, want, "{line}");
    let at = line["at"].as_str().unwrap();
    assert!(at.len() == 20 && at.ends_with('Z'), "{at}");
    assert!(line["ms"].is_u64(), "{line}");
}

// spec: specs/ap/door.md#R1
// spec: specs/ap/run-limits.md#R13
#[test]
fn r1_r13_a_stage_with_limits_has_its_door_and_ap_door_alone() {
    if on_pc() {
        return;
    }
    let f = Fake::new();
    let mut r = f.request("reviewer");
    r["env"] = json!({"AP_DOOR": "/elsewhere/door", "FOO": "bar"});
    let door = f.door();
    let running = f.start(r, true);
    let meta = fs::symlink_metadata(&door).unwrap();
    assert!(meta.file_type().is_socket());
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    let (f, result) = running.finish();
    let (v, status) = result.unwrap();
    assert_eq!((status, &v["state"]), (0, &json!("ok")), "{v}");
    assert_eq!(f.text("bin/door.1").trim(), "socket");
    assert!(fs::symlink_metadata(&door).is_err(), "the door remains");
    // `AP_DOOR` once, over the request's value.
    let doors: Vec<&(String, String)> = f.launch_envs[0]
        .iter()
        .filter(|(k, _)| k == "AP_DOOR")
        .collect();
    assert_eq!(doors, [&("AP_DOOR".to_owned(), door.clone())]);
    assert!(
        f.text("bin/env.1")
            .lines()
            .any(|l| l == format!("AP_DOOR={door}"))
    );
    // The words after `systemd-run … --` are the base's but for the view (`specs/ap/run-limits.md`
    // R16): its one bind of the door, read-only; no other word names the door.
    let launch = &f.launches[0];
    let at = launch.iter().position(|a| a == "--").unwrap();
    let after = &launch[at + 1..];
    let flags: Vec<&str> = after[..4].iter().map(String::as_str).collect();
    assert_eq!(
        flags,
        [BWRAP, "--unshare-user", "--disable-userns", "--unshare-pid"]
    );
    let dev = after.iter().position(|a| a == "--dev").unwrap();
    let head: Vec<&str> = after[dev..dev + 13].iter().map(String::as_str).collect();
    let (a1, scratch) = (f.s("a1"), f.s("scratch"));
    assert_eq!(
        head,
        [
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--ro-bind",
            "/proc/sys",
            "/proc/sys",
            "--bind",
            &a1,
            &a1,
            "--bind",
            &scratch,
            &scratch,
        ]
    );
    let end = after.iter().position(|a| a == "--cap-drop").unwrap();
    let repo = f.s("repo");
    assert_eq!(after[end - 2..end], ["--remount-ro", "/"]);
    assert_eq!(
        after[end..end + 5],
        ["--cap-drop", "ALL", "--chdir", &repo, "--"]
    );
    let bind = ["--ro-bind", &door, &door].map(str::to_owned);
    let bound = after[..dev].windows(3).position(|w| w == bind);
    assert!(bound.is_some(), "{after:?}");
    let naming: Vec<&String> = launch
        .iter()
        .filter(|a| a.contains(".door") || a.contains("AP_DOOR"))
        .collect();
    assert_eq!(naming, [&door, &door], "{launch:?}");
    // The record's `limits` is the base's and the view's, which names the door.
    let keys: Vec<&String> = v["limits"].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["bwrap", "hidden", "scratch", "view", "writable"]);
    assert!(
        v["limits"]["view"]
            .as_array()
            .unwrap()
            .contains(&json!(door)),
        "{v}"
    );
    // The first entry is the home of the launch (`specs/ap/run-limits.md` R14).
    assert_eq!(v["limits"]["writable"], json!([v["home"], scratch]));
    // A door that is asked nothing writes nothing.
    assert!(!f.t.join("door.jsonl").exists());
}

// spec: specs/ap/run-limits.md#R13
#[test]
fn r13_a_request_without_limits_has_no_door_and_no_variable() {
    if on_pc() {
        return;
    }
    let f = Fake::new();
    let mut r = f.request("reviewer");
    r.as_object_mut().unwrap().remove("limits");
    r["env"] = json!({"AP_DOOR": "/given/door"});
    let (f, result) = f.run(r);
    let (v, status) = result.unwrap();
    assert_eq!(status, 0, "{v}");
    assert_eq!(f.text("bin/door.1").trim(), "none");
    // Only the request's own variable, as any variable of `env`.
    assert!(
        f.text("bin/env.1")
            .lines()
            .any(|l| l == "AP_DOOR=/given/door")
    );
    let mut names: Vec<String> = fs::read_dir(&f.t)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert!(!names.iter().any(|n| n.ends_with(".door")), "{names:?}");
    assert!(!f.launches[0].contains(&BWRAP.to_owned()));
}

// spec: specs/ap/door.md#R1
// spec: specs/ap/door.md#R8
#[test]
fn r1_the_door_of_a_long_path_and_of_a_final_slash_is_reached() {
    if on_pc() {
        return;
    }
    let f = Fake::new();
    // A directory that makes the scratch path 200 bytes, a short name and a long one, and a path
    // written with a final `/`.
    let deep = |len: usize| {
        let base = f.s("");
        let mut p = base.clone();
        while p.len() < len {
            p.push_str(&"d".repeat(40.min(len - p.len()).max(1)));
            p.push('/');
        }
        fs::create_dir_all(&p).unwrap();
        p
    };
    let long_dir = deep(200);
    let cases = [
        (format!("{long_dir}s"), format!("{long_dir}s.door")),
        (
            format!("{}{}", deep(60), "n".repeat(110)),
            format!("{}{}.door", deep(60), "n".repeat(110)),
        ),
        (f.s("slash/"), f.s("slash.door")),
    ];
    assert!(cases[0].0.len() >= 200, "{}", cases[0].0.len());
    f.then("ap fn route < /dev/null\n");
    let mut f = f;
    for (i, (scratch, door)) in cases.iter().enumerate() {
        let _ = fs::remove_file(f.t.join("bin/count"));
        let mut r = f.request("verifier");
        r["limits"]["scratch"] = json!(scratch);
        r["out"] = json!(f.s(&format!("x{i}.md")));
        let (back, result) = f.run(r);
        f = back;
        let (v, status) = result.unwrap();
        assert_eq!(status, 0, "{scratch}: {v}");
        assert!(f.text("bin/env.1").contains(&format!("AP_DOOR={door}\n")));
        assert_eq!(f.text("bin/door.1").trim(), "socket", "{door}");
        assert_eq!(
            f.text("bin/then.rc").trim(),
            "0",
            "{}",
            f.text("bin/then.err")
        );
        assert!(f.text("bin/then.out").contains("specs/ap/door.md"));
        assert!(fs::symlink_metadata(door).is_err(), "{door} remains");
        assert!(fs::symlink_metadata(scratch.trim_end_matches('/')).is_err());
    }
    // Nothing else is left beside the doors.
    let left: Vec<String> = fs::read_dir(&f.t)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".ap-door"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

// spec: specs/ap/door.md#R1
// spec: specs/ap/run-limits.md#R13
#[test]
fn r1_a_name_in_the_way_of_the_door_fails_the_call_before_any_launch() {
    if on_pc() {
        return;
    }
    let f = Fake::new();
    write(&f.t.join("scratch.door"), "mine", 0o644);
    let r = f.request("reviewer");
    let (f, result) = f.run(r);
    let (v, status) = result.unwrap();
    assert_eq!(status, 3, "{v}");
    assert_eq!(
        (&v["state"], &v["unmet"]),
        (&json!("failed"), &json!("launch"))
    );
    assert!(f.launches.is_empty());
    assert_eq!(f.text("scratch.door"), "mine");
    assert!(!f.t.join("scratch").exists());
}

// spec: specs/ap/door.md#R2
// spec: specs/ap/door.md#R6
#[test]
fn r2_every_other_request_is_refused_invalid_request_without_effect() {
    if on_pc() {
        return;
    }
    let f = Fake::new().with_policy();
    let r = f.request("verifier");
    let door = f.door();
    let repo = f.s("repo");
    let running = f.start(r, true);
    let line = |v: Value| format!("{v}\n").into_bytes();
    let fn_ok = json!({"door": 1, "verb": "fn", "name": "route", "input": {}});
    let with = |k: &str, v: Value| {
        let mut r = fn_ok.clone();
        r[k] = v;
        line(r)
    };
    let without = |k: &str| {
        let mut r = fn_ok.clone();
        r.as_object_mut().unwrap().remove(k);
        line(r)
    };
    let check = |k: &str, v: Value| {
        let mut r = check_request(&repo, &["./check"]);
        r[k] = v;
        line(r)
    };
    let mut long = b"{\"door\": 1, \"x\": \"".to_vec();
    long.extend(vec![b'a'; 65_536]);
    long.extend(b"\"}\n");
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        ("not json", b"door please\n".to_vec(), "request"),
        ("an array", b"[1]\n".to_vec(), "request"),
        ("too long", long, "request"),
        ("door 2", with("door", json!(2)), "door"),
        ("door as text", with("door", json!("1")), "door"),
        ("no door", without("door"), "door"),
        ("another verb", with("verb", json!("action")), "verb"),
        ("no verb", without("verb"), "verb"),
        ("another member", with("extra", json!(1)), "request"),
        ("no input", without("input"), "input"),
        ("input not an object", with("input", json!([])), "input"),
        ("name not text", with("name", json!(1)), "name"),
        ("timeout 0", check("timeout", json!(0)), "timeout"),
        (
            "timeout too long",
            check("timeout", json!(14_401)),
            "timeout",
        ),
        ("timeout as text", check("timeout", json!("60")), "timeout"),
        ("no words", check("command", json!([])), "command"),
        (
            "an empty first word",
            check("command", json!([""])),
            "command",
        ),
        ("a word not text", check("command", json!([1])), "command"),
        ("dir not text", check("dir", json!(null)), "dir"),
    ];
    for (name, bytes, field) in &cases {
        let (out, err, closing) = raw(&door, bytes);
        assert_eq!(closing, refused("invalid_request", field), "{name}");
        assert!(out.is_empty() && err.is_empty(), "{name}");
    }
    // No line within 10 seconds.
    let start = Instant::now();
    let mut s = UnixStream::connect(&door).unwrap();
    s.write_all(b"{\"door\": 1").unwrap();
    let (_, _, closing) = frames(&mut s);
    assert_eq!(closing, refused("invalid_request", "request"));
    assert!(
        start.elapsed() >= Duration::from_secs(9),
        "{:?}",
        start.elapsed()
    );
    let (f, result) = running.finish();
    assert_eq!(result.unwrap().1, 0);
    // No refusal had an effect.
    assert!(!f.t.join("ont.args").exists());
    assert!(!f.t.join("pc/calls").exists());
    assert!(!f.t.join("git.args").exists());
    let lines = f.lines("door.jsonl");
    assert_eq!(lines.len(), cases.len() + 1);
    for l in &lines {
        eight(l);
        assert_eq!(
            (&l["exit"], &l["reason"]),
            (&Value::Null, &json!("invalid_request"))
        );
        assert_eq!(l["run"], json!(f.s("x.md.run.json")));
        assert_eq!(l["role"], json!("verifier"));
    }
    assert_eq!(lines[0]["verb"], Value::Null);
    assert_eq!(
        (&lines[3]["verb"], &lines[3]["name"]),
        (&json!("fn"), &json!("route"))
    );
    assert_eq!(
        (&lines[12]["verb"], &lines[12]["name"]),
        (&json!("check"), &json!("./check"))
    );
}

// spec: specs/ap/door.md#R2
#[test]
fn r2_the_door_serves_four_requests_at_once_and_the_fifth_waits() {
    if on_pc() {
        return;
    }
    let f = Fake::new();
    fs::write(f.t.join("ont.mode"), "wait").unwrap();
    let r = f.request("reviewer");
    let door = f.door();
    let clients: Vec<Command> = (0..5)
        .map(|_| {
            let mut c = f.client(Some(&door), &["fn", "route"]);
            c.stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            c
        })
        .collect();
    let t = f.t.clone();
    let running = f.start(r, true);
    let mut children: Vec<Child> = clients
        .into_iter()
        .map(|mut c| forks::forking(|| c.spawn()).unwrap())
        .collect();
    for c in &mut children {
        assert_eq!(c.wait().unwrap().code(), Some(0));
    }
    let (_f, result) = running.finish();
    assert_eq!(result.unwrap().1, 0);
    let times = fs::read_to_string(t.join("ont.times")).unwrap_or_default();
    let (mut now, mut most) = (0, 0);
    for l in times.lines() {
        now += if l == "start" { 1 } else { -1 };
        most = most.max(now);
    }
    assert_eq!(times.lines().count(), 10, "{times}");
    assert_eq!(most, 4, "{times}");
}

// spec: specs/ap/door.md#R3
#[test]
fn r3_check_is_admitted_for_its_three_roles_and_its_own_cwd_alone() {
    if on_pc() {
        return;
    }
    for role in ["implementer", "fixer", "verifier", "reviewer"] {
        let f = Fake::new().with_policy();
        fs::write(f.t.join("pc/down"), "").unwrap();
        let other = f.s("elsewhere");
        std::os::unix::fs::symlink(f.t.join("repo"), f.t.join("link")).unwrap();
        let link = f.s("link");
        let (door, repo) = (f.door(), f.s("repo"));
        let r = f.request(role);
        let running = f.start(r, true);
        let (_, _, closing) = ask(&door, &check_request(&repo, &["./check"]));
        let calls = |t: &Path| fs::read_to_string(t.join("pc/calls")).unwrap_or_default();
        if role == "reviewer" {
            assert_eq!(closing, refused("not_admitted", "role"));
        } else {
            assert_eq!(closing, Some(json!({"here": "pc_unreachable"})), "{role}");
            assert_eq!(calls(&running.t), "ping\n", "{role}");
            for dir in [&other, &link] {
                let (_, _, closing) = ask(&door, &check_request(dir, &["./check"]));
                assert_eq!(closing, refused("not_admitted", "dir"), "{role} {dir}");
            }
        }
        let t = running.t.clone();
        let (f, result) = running.finish();
        assert_eq!(result.unwrap().1, 0);
        // A refusal started no `tailscale`.
        let want = if role == "reviewer" { "" } else { "ping\n" };
        assert_eq!(calls(&t), want, "{role}");
        for l in f.lines("door.jsonl") {
            eight(&l);
        }
    }
}

// spec: specs/ap/door.md#R4
// spec: specs/ap/door.md#R6
#[test]
fn r4_the_pc_runs_the_check_and_its_streams_status_and_row_come_back() {
    if on_pc() {
        return;
    }
    let f = Fake::new().with_policy();
    let (door, repo) = (f.door(), f.s("repo"));
    let pc = f.s("pc/work/trees");
    let r = f.request("implementer");
    let running = f.start(r, true);
    let (out, err, closing) = ask(&door, &check_request(&repo, &["./check"]));
    assert_eq!(closing, Some(json!({"exit": 3})));
    assert!(
        text(&out).starts_with(&format!("check ran in {pc}/")),
        "{}",
        text(&out)
    );
    let err = text(&err);
    assert!(
        err.starts_with("check: on pc (4 CPUs, load 0.10, 8192 MB available)\n"),
        "{err}"
    );
    assert!(err.ends_with("check err\n"), "{err}");
    for n in [0, 255] {
        let script = format!("echo out {n}; echo err {n} >&2; exit {n}");
        let (out, err, closing) = ask(&door, &check_request(&repo, &["sh", "-c", &script]));
        assert_eq!(closing, Some(json!({ "exit": n })));
        assert_eq!(text(&out), format!("out {n}\n"));
        assert!(
            text(&err).ends_with(&format!("err {n}\n")),
            "{}",
            text(&err)
        );
    }
    let (f, result) = running.finish();
    assert_eq!(result.unwrap().1, 0);
    let rows = f.lines("checks.jsonl");
    assert_eq!(rows.len(), 3);
    for (row, exit) in rows.iter().zip([3, 0, 255]) {
        assert_eq!((&row["host"], &row["reason"]), (&json!("pc"), &Value::Null));
        assert_eq!((&row["dir"], &row["exit"]), (&json!(repo), &json!(exit)));
        assert_eq!(row["repository"], json!("example/repo"));
    }
    assert_eq!(rows[0]["command"], json!(["./check"]));
    // Every `git` of the door has the two settings before its command word.
    // (`ap run`'s own Git calls, which name `-C` first, carry them too.)
    let git = f.text("git.args");
    let door_calls = git.lines().filter(|l| l.starts_with("-c ")).count();
    assert!(door_calls >= 15, "{git}");
    for l in git.lines() {
        let words: Vec<&str> = l.split(' ').collect();
        let at = words.iter().position(|w| *w == "-c").unwrap_or(usize::MAX);
        assert_eq!(
            words.get(at..at + 4),
            Some(
                &[
                    "-c",
                    "core.hooksPath=/dev/null",
                    "-c",
                    "core.fsmonitor=false"
                ][..]
            ),
            "{l}"
        );
        let flags = &words[..at + 4];
        assert!(
            flags
                .iter()
                .all(|w| w.starts_with('-') || w.starts_with('/') || w.starts_with("core.")),
            "{l}"
        );
    }
    let lines = f.lines("door.jsonl");
    assert_eq!(lines.len(), 3);
    for (l, exit) in lines.iter().zip([3, 0, 255]) {
        eight(l);
        assert_eq!(
            (&l["verb"], &l["exit"], &l["reason"]),
            (&json!("check"), &json!(exit), &Value::Null)
        );
        assert_eq!(l["role"], json!("implementer"));
    }
    assert_eq!(lines[0]["name"], json!("./check"));
    assert_eq!(lines[1]["name"], json!("sh"));
}

// spec: specs/ap/door.md#R4
// spec: specs/ap/door.md#R6
#[test]
fn r4_a_pc_down_busy_or_lost_closes_here_without_a_row_or_a_command() {
    if on_pc() {
        return;
    }
    let f = Fake::new().with_policy();
    let (door, repo) = (f.door(), f.s("repo"));
    let marker = f.s("ran");
    let t = f.t.clone();
    let r = f.request("fixer");
    let running = f.start(r, true);
    let command = ["sh", "-c", &format!(": > {marker}")];
    let cases = [
        ("down", "pc_unreachable"),
        ("busy", "pc_busy"),
        ("lose", "pc_lost"),
    ];
    for (how, reason) in cases {
        match how {
            "busy" => fs::write(t.join("pc/loadavg"), "9.00 9.00 9.00 1/100 1\n").unwrap(),
            other => fs::write(t.join("pc").join(other), "").unwrap(),
        }
        let (out, err, closing) = ask(&door, &check_request(&repo, &command));
        assert_eq!(closing, Some(json!({ "here": reason })), "{how}");
        assert!(out.is_empty(), "{how}");
        if how == "lose" {
            assert!(text(&err).starts_with("check: on pc ("), "{}", text(&err));
        }
        let _ = fs::remove_file(t.join("pc/down"));
        fs::write(t.join("pc/loadavg"), "0.10 0.10 0.10 1/100 1\n").unwrap();
    }
    // A worktree the door's own reading refuses: `here` with the refusal's code.
    let (f, result) = running.finish();
    assert_eq!(result.unwrap().1, 0);
    assert!(!f.t.join("ran").exists());
    assert!(!f.t.join("checks.jsonl").exists());
    let lines = f.lines("door.jsonl");
    let reasons: Vec<&Value> = lines.iter().map(|l| &l["reason"]).collect();
    assert_eq!(
        reasons,
        [
            &json!("pc_unreachable"),
            &json!("pc_busy"),
            &json!("pc_lost")
        ]
    );
    for l in &lines {
        eight(l);
        assert_eq!(l["exit"], Value::Null);
    }
}

// spec: specs/ap/door.md#R4
#[test]
fn r4_a_policy_or_worktree_the_door_refuses_closes_here_with_its_code() {
    if on_pc() {
        return;
    }
    let f = Fake::new().with_policy();
    let (door, repo) = (f.door(), f.s("repo"));
    let t = f.t.clone();
    let r = f.request("verifier");
    let running = f.start(r, true);
    fs::write(t.join("policy.json"), "{\"hosts\": []}").unwrap();
    let (_, _, closing) = ask(&door, &check_request(&repo, &["./check"]));
    assert_eq!(closing, Some(json!({"here": "policy_invalid"})));
    fs::write(t.join("policy.json"), "{}").unwrap();
    let (_, _, closing) = ask(&door, &check_request(&repo, &["./check"]));
    assert_eq!(closing, Some(json!({"here": "no_pc_in_policy"})));
    let made = forks::forking(|| {
        Command::new("git")
            .args(["-C", &repo, "remote", "set-url", "origin", "/srv/repo.git"])
            .output()
            .unwrap()
    });
    assert!(made.status.success());
    let (_, _, closing) = ask(&door, &check_request(&repo, &["./check"]));
    assert_eq!(closing, Some(json!({"here": "origin_not_github"})));
    let (f, _) = running.finish();
    assert!(!f.t.join("pc/calls").exists());
}

// spec: specs/ap/door.md#R4
// spec: specs/ap/door.md#R6
#[test]
fn r4_a_client_gone_mid_call_ends_it_and_leaves_no_process() {
    if on_pc() {
        return;
    }
    let f = Fake::new().with_policy();
    let (door, repo) = (f.door(), f.s("repo"));
    let pid_file = f.s("cmd.pid");
    let script = format!("echo $$ > {pid_file}; exec sleep 600");
    let args = ["check", &repo, "--timeout", "30", "--", "sh", "-c", &script];
    let mut client = f.client(Some(&door), &args);
    client
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let t = f.t.clone();
    let r = f.request("implementer");
    let running = f.start(r, true);
    let mut child = forks::forking(|| client.spawn()).unwrap();
    wait_for("the command on the PC", || {
        fs::read_to_string(t.join("cmd.pid")).is_ok_and(|p| p.ends_with('\n'))
    });
    let pid = fs::read_to_string(t.join("cmd.pid")).unwrap();
    // The door's program of the call: the `tailscale` that carries it to the PC.
    let carrier = fs::read_to_string(t.join("pc/ssh.pid")).unwrap();
    assert!(alive(&pid) && alive(&carrier));
    child.kill().unwrap();
    child.wait().unwrap();
    wait_for("the call's end", || !alive(&carrier));
    wait_for("the line", || t.join("door.jsonl").exists());
    // What the fake PC still runs ends at its own bound (`specs/ap/check.md` R10); here, now.
    let _ = Command::new("kill").args(["-KILL", pid.trim()]).status();
    let (f, result) = running.finish();
    assert_eq!(result.unwrap().1, 0);
    let lines = f.lines("door.jsonl");
    assert_eq!(lines.len(), 1);
    eight(&lines[0]);
    assert_eq!(
        (&lines[0]["exit"], &lines[0]["reason"]),
        (&Value::Null, &json!("client_gone"))
    );
    assert!(!f.t.join("checks.jsonl").exists());
}

// spec: specs/ap/door.md#R5
// spec: specs/ap/door.md#R6
#[test]
fn r5_fn_asks_the_engine_as_the_stage_for_a_function_of_the_list() {
    if on_pc() {
        return;
    }
    let f = Fake::new();
    let door = f.door();
    let t = f.t.clone();
    let r = f.request("verifier");
    let running = f.start(r, true);
    let input = json!({"parameters": {"words": "the door", "limit": 2}});
    let (out, err, closing) = ask(
        &door,
        &json!({"door": 1, "verb": "fn", "name": "route", "input": input}),
    );
    assert_eq!(closing, Some(json!({"exit": 0})));
    assert_eq!(
        text(&out),
        "{\"answers\":[{\"path\":\"specs/ap/door.md\"}]}\n"
    );
    assert_eq!(text(&err), "engine note\n");
    assert_eq!(
        fs::read_to_string(t.join("ont.args")).unwrap(),
        "fn route\n"
    );
    assert_eq!(
        fs::read_to_string(t.join("ont.input")).unwrap(),
        format!("{input}\n")
    );
    let env: BTreeSet<String> = fs::read_to_string(t.join("ont.env"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    let want: BTreeSet<String> = [
        format!(
            "PATH={}:{}:/usr/bin:/bin",
            t.join("fakes").display(),
            t.join("bin").display()
        ),
        format!("HOME={}", t.join("home").display()),
        "LANG=C.UTF-8".into(),
        "ONT_ROLE=verifier".into(),
        "ONT_AGENT=claude:sonnet".into(),
    ]
    .into();
    assert_eq!(env, want);
    // The engine's refusal: its streams and its exit.
    fs::write(t.join("ont.mode"), "refuse").unwrap();
    let (out, err, closing) = ask(
        &door,
        &json!({"door": 1, "verb": "fn", "name": "route", "input": {}}),
    );
    assert_eq!(closing, Some(json!({"exit": 2})));
    assert!(text(&out).contains("invalid_parameter"));
    assert_eq!(text(&err), "refused here\n");
    // A function off the list: refused, no call.
    let (_, _, closing) = ask(
        &door,
        &json!({"door": 1, "verb": "fn", "name": "other", "input": {}}),
    );
    assert_eq!(closing, refused("not_admitted", "name"));
    assert_eq!(
        fs::read_to_string(t.join("ont.args"))
            .unwrap()
            .lines()
            .count(),
        2
    );
    // The door's bound: 2 MiB of a stream.
    fs::write(t.join("ont.mode"), "flood").unwrap();
    let (out, _, closing) = ask(
        &door,
        &json!({"door": 1, "verb": "fn", "name": "route", "input": {}}),
    );
    assert_eq!(closing, Some(json!({"ended": "bound"})));
    assert_eq!(out.len(), 2 * 1024 * 1024);
    let (f, result) = running.finish();
    assert_eq!(result.unwrap().1, 0);
    let lines = f.lines("door.jsonl");
    for l in &lines {
        eight(l);
        assert_eq!(l["verb"], json!("fn"));
    }
    let ends: Vec<(&Value, &Value, &Value)> = lines
        .iter()
        .map(|l| (&l["name"], &l["exit"], &l["reason"]))
        .collect();
    assert_eq!(
        ends,
        [
            (&json!("route"), &json!(0), &Value::Null),
            (&json!("route"), &json!(2), &Value::Null),
            (&json!("other"), &Value::Null, &json!("not_admitted")),
            (&json!("route"), &Value::Null, &json!("bound")),
        ]
    );
}

// spec: specs/ap/clause.md#R10
#[test]
fn r10_clause_is_served_as_route_is_and_a_name_off_the_list_is_still_refused() {
    if on_pc() {
        return;
    }
    let f = Fake::new();
    let door = f.door();
    let t = f.t.clone();
    let r = f.request("reviewer");
    let running = f.start(r, true);
    let input = json!({"parameters": {"key": "o/r:specs/ap/unit.md#R11"}});
    let (out, _, closing) = ask(
        &door,
        &json!({"door": 1, "verb": "fn", "name": "clause", "input": input}),
    );
    assert_eq!(closing, Some(json!({"exit": 0})));
    assert!(!out.is_empty());
    let (_, _, closing) = ask(
        &door,
        &json!({"door": 1, "verb": "fn", "name": "horizon", "input": {}}),
    );
    assert_eq!(closing, refused("not_admitted", "name"));
    assert_eq!(
        fs::read_to_string(t.join("ont.args")).unwrap(),
        "fn clause\n"
    );
    assert_eq!(
        fs::read_to_string(t.join("ont.input")).unwrap(),
        format!("{input}\n")
    );
    let (f, result) = running.finish();
    assert_eq!(result.unwrap().1, 0);
    let names: Vec<Value> = f
        .lines("door.jsonl")
        .iter()
        .map(|l| l["name"].clone())
        .collect();
    assert_eq!(names, [json!("clause"), json!("horizon")]);
}

// spec: specs/ap/door.md#R5
#[test]
fn r5_the_store_of_the_request_is_named_and_an_engine_that_cannot_start_is_127() {
    if on_pc() {
        return;
    }
    let f = Fake::new();
    let mut r = f.request("implementer");
    r["unit"] = json!("u1");
    r["store"] = json!(f.s("store"));
    let door = f.door();
    let t = f.t.clone();
    let running = f.start(r, true);
    let (_, _, closing) = ask(
        &door,
        &json!({"door": 1, "verb": "fn", "name": "route", "input": {}}),
    );
    assert_eq!(closing, Some(json!({"exit": 0})));
    assert_eq!(
        fs::read_to_string(t.join("ont.args")).unwrap(),
        format!("fn route --store {}\n", t.join("store").display())
    );
    fs::remove_file(t.join("fakes/ont")).unwrap();
    let (_, _, closing) = ask(
        &door,
        &json!({"door": 1, "verb": "fn", "name": "route", "input": {}}),
    );
    assert_eq!(closing, Some(json!({"exit": 127})));
    let (f, _) = running.finish();
    assert_eq!(f.lines("door.jsonl")[1]["exit"], json!(127));
}

/// The scenario of the record's mode under the umask the test binary was started with.
fn record_under_umask() {
    let f = Fake::new();
    f.then("ap fn route < /dev/null\n");
    let r = f.request("reviewer");
    let (f, result) = f.run(r);
    assert_eq!(result.unwrap().1, 0);
    let mode = fs::metadata(f.t.join("door.jsonl"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}
#[test]
fn r6_child_the_record_under_a_umask() {
    if std::env::var_os("AP_DOOR_TEST_UMASK").is_some() {
        record_under_umask();
    }
}

// spec: specs/ap/door.md#R6
#[test]
fn r6_the_record_is_made_0600_under_any_umask_and_a_file_that_exists_keeps_its_mode() {
    if on_pc() {
        return;
    }
    for umask in ["022", "077"] {
        let exe = std::env::current_exe().unwrap();
        let out = forks::forking(|| {
            Command::new("sh")
                .args([
                    "-c",
                    &format!("umask {umask}; exec \"$0\" \"$@\""),
                    exe.to_str().unwrap(),
                    "--exact",
                    "r6_child_the_record_under_a_umask",
                    "--test-threads=1",
                ])
                .env("AP_DOOR_TEST_UMASK", "1")
                .output()
                .unwrap()
        });
        assert!(out.status.success(), "umask {umask}: {out:?}");
        assert!(
            text(&out.stdout).contains("1 passed"),
            "{}",
            text(&out.stdout)
        );
    }
    let f = Fake::new();
    write(&f.t.join("door.jsonl"), "", 0o644);
    f.then("ap fn route < /dev/null\n");
    let r = f.request("reviewer");
    let (f, result) = f.run(r);
    assert_eq!(result.unwrap().1, 0);
    let meta = fs::metadata(f.t.join("door.jsonl")).unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o644);
    let lines = f.lines("door.jsonl");
    assert_eq!(lines.len(), 1);
    eight(&lines[0]);
    assert_eq!(
        (&lines[0]["role"], &lines[0]["name"], &lines[0]["exit"]),
        (&json!("reviewer"), &json!("route"), &json!(0))
    );
}

// spec: specs/ap/door.md#R7
#[test]
fn r7_the_closing_ends_a_call_the_pc_holds_and_nothing_of_it_lives() {
    if on_pc() {
        return;
    }
    let f = Fake::new().with_policy();
    let (door, repo) = (f.door(), f.s("repo"));
    let pid_file = f.s("cmd.pid");
    let t = f.t.clone();
    let r = f.request("verifier");
    let running = f.start(r, true);
    let mut s = UnixStream::connect(&door).unwrap();
    let script = format!("echo $$ > {pid_file}; exec sleep 600");
    let mut request = check_request(&repo, &["sh", "-c", &script]);
    request["timeout"] = json!(30);
    s.write_all(format!("{request}\n").as_bytes()).unwrap();
    wait_for("the command on the PC", || {
        fs::read_to_string(t.join("cmd.pid")).is_ok_and(|p| p.ends_with('\n'))
    });
    let pid = fs::read_to_string(t.join("cmd.pid")).unwrap();
    let carrier = fs::read_to_string(t.join("pc/ssh.pid")).unwrap();
    let start = Instant::now();
    let (f, result) = running.finish();
    assert!(
        start.elapsed() < Duration::from_secs(20),
        "{:?}",
        start.elapsed()
    );
    assert_eq!(result.unwrap().1, 0);
    assert!(!alive(&carrier), "the door's program of the call lives");
    let _ = Command::new("kill").args(["-KILL", pid.trim()]).status();
    let (_, _, closing) = frames(&mut s);
    assert!(
        closing.is_none() || closing == Some(json!({"ended": "closed"})),
        "{closing:?}"
    );
    assert!(fs::symlink_metadata(&door).is_err());
    let lines = f.lines("door.jsonl");
    assert_eq!(lines.len(), 1);
    eight(&lines[0]);
    assert_eq!(
        (&lines[0]["exit"], &lines[0]["reason"]),
        (&Value::Null, &json!("closed"))
    );
    assert!(!f.t.join("checks.jsonl").exists());
}

// spec: specs/ap/door.md#R8
#[test]
fn r8_ap_fn_refuses_before_any_connection_and_says_no_door() {
    let f = Fake::new();
    let refusal = |out: &Output, code: &str, field: &str, status: i32| {
        assert_eq!(out.status.code(), Some(status), "{out:?}");
        assert!(out.stdout.is_empty(), "{out:?}");
        let v: Value = serde_json::from_slice(&out.stderr).unwrap();
        assert_eq!(v["error"]["code"], json!(code), "{out:?}");
        assert_eq!(v["error"]["field"], json!(field), "{out:?}");
    };
    let door = Some("/nonexistent/door");
    let long = "r".repeat(65);
    for args in [
        vec!["fn"],
        vec!["fn", "route", "more"],
        vec!["fn", ""],
        vec!["fn", "Route"],
        vec!["fn", "1route"],
        vec!["fn", "ro-ute"],
        vec!["fn", long.as_str()],
    ] {
        refusal(&f.ap(door, &args, b""), "usage", "args", 2);
    }
    let help = f.ap(None, &["fn", "--help"], b"");
    assert_eq!(help.status.code(), Some(0));
    assert!(text(&help.stdout).starts_with("Usage: ap fn <name> < request.json\n"));
    for input in [
        b"[]".to_vec(),
        b"{".to_vec(),
        b"\"x\"".to_vec(),
        vec![b' '; 60_001],
    ] {
        refusal(
            &f.ap(door, &["fn", "route"], &input),
            "invalid_input",
            "stdin",
            2,
        );
    }
    refusal(
        &f.ap(None, &["fn", "route"], b"{}"),
        "no_door",
        "AP_DOOR",
        2,
    );
    refusal(
        &f.ap(Some(""), &["fn", "route"], b"{}"),
        "no_door",
        "AP_DOOR",
        2,
    );
    refusal(
        &f.ap(door, &["fn", "route"], b"{}"),
        "no_door",
        "AP_DOOR",
        3,
    );
    // A socket that closes without a closing message: the door is gone.
    let quiet = f.s("quiet.sock");
    let listener = std::os::unix::net::UnixListener::bind(&quiet).unwrap();
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut b = [0u8; 64];
        let _ = s.read(&mut b);
    });
    refusal(
        &f.ap(Some(&quiet), &["fn", "route"], b"{}"),
        "no_door",
        "AP_DOOR",
        3,
    );
    server.join().unwrap();
}

// spec: specs/ap/door.md#R8
#[test]
fn r8_ap_fn_answers_the_engine_or_the_door_refusal() {
    if on_pc() {
        return;
    }
    let f = Fake::new();
    let door = f.door();
    let t = f.t.clone();
    let r = f.request("reviewer");
    let running = f.start(r, true);
    let client = Fake::client_of(&t);
    let out = client(
        &door,
        &["fn", "route"],
        b"{\"parameters\": {\"words\": \"x\"}}",
    );
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        text(&out.stdout),
        "{\"answers\":[{\"path\":\"specs/ap/door.md\"}]}\n"
    );
    assert_eq!(text(&out.stderr), "engine note\n");
    // Empty or blank input is `{}`.
    let out = client(&door, &["fn", "route"], b" \n\t");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(fs::read_to_string(t.join("ont.input")).unwrap(), "{}\n");
    fs::write(t.join("ont.mode"), "refuse").unwrap();
    let out = client(&door, &["fn", "route"], b"");
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(text(&out.stderr), "refused here\n");
    let out = client(&door, &["fn", "other"], b"");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let v: Value = serde_json::from_slice(&out.stderr).unwrap();
    assert_eq!(v["error"]["code"], json!("not_admitted"));
    assert_eq!(v["error"]["field"], json!("name"));
    let _ = running.finish();
}
impl Fake {
    /// The built `ap fn` with `AP_DOOR`, from a directory alone.
    fn client_of(t: &Path) -> impl Fn(&str, &[&str], &[u8]) -> Output {
        let t = t.to_owned();
        move |door: &str, args: &[&str], input: &[u8]| {
            let mut c = Command::new(env!("CARGO_BIN_EXE_ap"));
            c.args(args)
                .env_clear()
                .env(
                    "PATH",
                    format!("{}:/usr/bin:/bin", t.join("client").display()),
                )
                .env("AP_DOOR", door)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            let mut child = forks::forking(|| c.spawn()).unwrap();
            child.stdin.take().unwrap().write_all(input).unwrap();
            child.wait_with_output().unwrap()
        }
    }
}

// spec: specs/ap/check.md#R20
#[test]
fn r20_ap_check_under_the_door_for_each_of_its_outcomes() {
    if on_pc() {
        return;
    }
    let f = Fake::new().with_policy();
    let (door, repo) = (f.door(), f.s("repo"));
    let t = f.t.clone();
    let show = "echo door=${AP_DOOR:-unset} in $PWD";
    let r = f.request("implementer");
    let running = f.start(r, true);
    let check = |door: Option<&str>, words: &[&str]| {
        let mut args = vec!["check", repo.as_str(), "--timeout", "120", "--"];
        args.extend(words);
        let mut c = Command::new(env!("CARGO_BIN_EXE_ap"));
        c.args(&args)
            .env_clear()
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", t.join("client").display()),
            )
            .env("HOME", t.join("home"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", t.join("gitconfig"))
            .env("AP_POLICY_FILE", t.join("policy.json"))
            .env("AP_CHECKS_LOG", t.join("client-checks.jsonl"))
            .env("HOST_CARGO_LOCK", t.join("build.lock"))
            .stdin(Stdio::null());
        if let Some(d) = door {
            c.env("AP_DOOR", d);
        }
        forks::forking(|| c.output()).unwrap()
    };
    // The door ran it on the PC: its streams and its status are `ap check`'s.
    let out = check(Some(&door), &["./check"]);
    assert_eq!(out.status.code(), Some(3), "{out:?}");
    assert!(text(&out.stdout).starts_with(&format!(
        "check ran in {}/",
        t.join("pc/work/trees").display()
    )));
    let err = text(&out.stderr);
    assert!(
        err.starts_with("check: on pc (") && err.ends_with("check err\n"),
        "{err}"
    );
    // The door closed with `here`: the line and the command here, without `AP_DOOR`.
    fs::write(t.join("pc/down"), "").unwrap();
    let out = check(Some(&door), &["sh", "-c", show]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(text(&out.stderr), "check: on this host (pc_unreachable)\n");
    assert_eq!(text(&out.stdout), format!("door=unset in {repo}\n"));
    // No door at the path: `no_door`.
    let out = check(Some("/nonexistent/door"), &["sh", "-c", show]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(text(&out.stderr), "check: on this host (no_door)\n");
    assert_eq!(text(&out.stdout), format!("door=unset in {repo}\n"));
    // A policy without `hosts.pc`: here, and the door is not asked.
    fs::remove_file(t.join("pc/down")).unwrap();
    let lines_before = fs::read_to_string(t.join("door.jsonl"))
        .unwrap()
        .lines()
        .count();
    let policy = fs::read_to_string(t.join("policy.json")).unwrap();
    write(&t.join("policy.json"), "{}", 0o644);
    let out = check(Some(&door), &["sh", "-c", show]);
    assert_eq!(text(&out.stderr), "check: on this host (no_pc_in_policy)\n");
    assert_eq!(text(&out.stdout), format!("door=unset in {repo}\n"));
    let lines_after = fs::read_to_string(t.join("door.jsonl"))
        .unwrap()
        .lines()
        .count();
    assert_eq!(lines_before, lines_after);
    // Without `AP_DOOR` a command here has none either, and `ap check` appends its own row.
    let out = check(None, &["sh", "-c", show]);
    assert_eq!(text(&out.stdout), format!("door=unset in {repo}\n"));
    write(&t.join("policy.json"), &policy, 0o644);
    let (f, _) = running.finish();
    // The door refused (a reviewer's check): `door_refused`.
    let (f, door_refused) = {
        let mut r = f.request("reviewer");
        r["out"] = json!(f.s("y.md"));
        let _ = fs::remove_file(f.t.join("bin/go"));
        let _ = fs::remove_file(f.t.join("bin/count"));
        let _ = fs::remove_file(f.t.join("bin/door.1"));
        let running = f.start(r, true);
        let out = check(Some(&door), &["sh", "-c", show]);
        let (f, _) = running.finish();
        (f, out)
    };
    assert_eq!(door_refused.status.code(), Some(0), "{door_refused:?}");
    assert_eq!(
        text(&door_refused.stderr),
        "check: on this host (door_refused)\n"
    );
    assert_eq!(
        text(&door_refused.stdout),
        format!("door=unset in {repo}\n")
    );
    // The client started no `tailscale` and appended no row; the PC's one row is the door's.
    assert!(!f.t.join("client-tailscale").exists());
    assert_eq!(f.lines("client-checks.jsonl").len(), 1);
    assert_eq!(f.lines("checks.jsonl").len(), 1);
}
