//! `ap check` (`specs/ap/check.md`) as a command, against a fake PC: a fake `tailscale` on the
//! PATH whose `ping` answers unless the file `down` exists and whose `ssh` runs the PC's command
//! here, as the PC's user would, with only a PC's own environment (`HOME` and `PATH` of the fake
//! PC), its `/proc/meminfo` and Windows' PowerShell mapped to files of the fake PC; a stand-in `bwrap` on the fake PC's PATH that changes directory, sets the variables and links the tree's `.git`
//! to its repository, and touches no other path (two tests run the real `bwrap` instead). Every path is under a scratch directory: the policy
//! file, the record file, the build lock and the PC's work directory.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "a test fails by panicking, so an unwrap or an expect in its helpers is how it reports"
)]
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

const TAILSCALE: &str = r#"#!/bin/sh
# The fake PC, its state in the directory @PC@. ping: answers unless down exists. ssh: as ssh's
# default SendEnv, LANG and LC_* of the caller go along; the probe reads loadavg, meminfo, the fake
# PowerShell and 4 CPUs; the run reads meminfo (meminfo-run where it exists), keeps its command line and payload, and with lose ends
# as a lost connection before the command, with lose-after after it (its status line dropped), with
# cut kills the script (its process group) once its sampler runs.
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
    [ ! -e "$pc/probe-fails" ] || exit 255
    cmd=$(printf '%s' "$cmd" | sed "s#/proc/loadavg#$pc/loadavg#; s#/proc/meminfo#$pc/meminfo#; s#/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe#$pc/powershell.exe#; s#(nproc)#(echo 4)#")
    run < /dev/null; exit ;;
  esac
  printf '%s' "$cmd" > "$pc/remote"
  m=$pc/meminfo; [ ! -e "$pc/meminfo-run" ] || m=$pc/meminfo-run
  cmd=$(printf '%s' "$cmd" | sed "s#/proc/meminfo#$m#g")
  cat > "$pc/payload.tar"
  [ ! -e "$pc/lose" ] || exit 255
  if [ -e "$pc/cut" ]; then
    setsid env -i HOME="$pc/home" PATH="$pc/bin:/usr/bin:/bin" sh -c "$cmd" < "$pc/payload.tar" > /dev/null 2>&1 &
    p=$! i=0
    while ! pgrep -f "ap-check-sampler $pc/work/tmp/" > /dev/null && [ $i -lt 300 ]; do sleep 0.1; i=$((i + 1)); done
    sleep 1
    kill -KILL -- "-$p"
    exit 255
  fi
  if [ -e "$pc/lose-after" ]; then
    { run < "$pc/payload.tar" 2>&1 1>&3 | grep -av 'ap-check' >&2; } 3>&1
    exit 255
  fi
  run < "$pc/payload.tar"; exit ;;
esac
exit 2
"#;

const POWERSHELL: &str = r#"#!/bin/sh
# Windows' PowerShell as interop runs it: its arguments kept, `during` noted while a command marks
# itself running; fails with windows-fails; answers the file windows-free with CRLF.
echo "$*" >> @PC@/powershell-calls
[ ! -e @PC@/running ] || echo during >> @PC@/powershell-calls
[ ! -e @PC@/windows-fails ] || exit 1
printf '%s\r\n' "$(cat @PC@/windows-free)"
"#;

/// The fake PC's `/proc/meminfo` with `kb` available.
fn meminfo(w: &World, kb: u64) {
    write(
        &w.p("pc/meminfo"),
        &format!(
            "MemTotal:       32000000 kB\nMemFree:          100000 kB\nMemAvailable:   {kb} kB\n"
        ),
        0o644,
    );
}

const BWRAP: &str = r#"#!/bin/sh
# A stand-in for bwrap: changes directory, sets and unsets the variables it is given, and puts a
# link for the one bind of the key's repository at the tree's .git: a read-only bind whose
# destination lies below the fake PC @PC@ and is named .git, from another path. Every other bind and
# option is passed over: it removes, makes or changes nothing else. Then it runs the command.
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

const CHECK: &str = r#"#!/bin/sh
echo "check ran in $PWD"
if [ -n "${HOST_CARGO_LOCK:-}" ] && [ -e "$HOST_CARGO_LOCK" ]; then
  if flock -n "$HOST_CARGO_LOCK" true; then echo "lock free"; else echo "lock held"; fi
fi
echo "check err" >&2
exit 3
"#;

/// What the worktree holds as Git lists it: the commit, the index entries, the untracked files and
/// each listed path's kind, mode and bytes.
const MANIFEST: &str = r#"export LC_ALL=C
git rev-parse HEAD
git ls-files -s
echo --others
git ls-files --others --exclude-standard
echo --files
{ git ls-files -z --cached; git ls-files -z --others --exclude-standard; } | sort -zu |
while IFS= read -r -d '' f; do
  if [ -L "$f" ]; then echo "link $f $(readlink "$f")"
  elif [ -f "$f" ]; then echo "file $f $(stat -c %a "$f") $(sha256sum < "$f" | cut -d' ' -f1)"
  elif [ -e "$f" ]; then echo "other $f"
  else echo "absent $f"; fi
done"#;

/// Every file and link of the tree but `.git`, sorted.
const FOUND: &str = "export LC_ALL=C; find . -path ./.git -prune -o \\( -type f -o -type l \\) -print | sed 's#^\\./##' | sort";

/// The cargo settings `./check` exports under the build lock, which `ap check` sets for its command.
const CARGO_SETTINGS: [(&str, &str); 3] = [
    ("CARGO_INCREMENTAL", "0"),
    ("CARGO_PROFILE_DEV_DEBUG", "0"),
    ("CARGO_PROFILE_TEST_DEBUG", "0"),
];
/// A command that prints the marker and the three settings, one line.
const SHOW_ENV: &str = "echo \"host=${AP_CHECK_HOST:-unset} ${CARGO_INCREMENTAL:-unset} ${CARGO_PROFILE_DEV_DEBUG:-unset} ${CARGO_PROFILE_TEST_DEBUG:-unset}\"";

struct World {
    root: PathBuf,
    repo: PathBuf,
}

fn write(path: &Path, text: &str, mode: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}
fn sh(dir: &Path, script: &str) -> String {
    let out = Command::new("bash")
        .args(["-c", script])
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", dir.join("../gitconfig"))
        .env("XDG_CONFIG_HOME", dir.join("../xdg"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{script}: {out:?}");
    String::from_utf8(out.stdout).unwrap()
}
fn lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::to_owned)
        .collect()
}

impl World {
    fn new() -> Self {
        Self::within(None, true)
    }
    /// A scratch root (under `parent` when given), a worktree whose origin is on GitHub, the fake
    /// PC with a policy that names it, and the stand-in `bwrap` unless `fake_bwrap` is false.
    fn within(parent: Option<&Path>, fake_bwrap: bool) -> Self {
        let mut mktemp = Command::new("mktemp");
        mktemp.arg("-d");
        if let Some(p) = parent {
            mktemp.args(["-p", p.to_str().unwrap()]);
        }
        let out = mktemp.output().unwrap();
        let root = fs::canonicalize(String::from_utf8(out.stdout).unwrap().trim()).unwrap();
        let w = World {
            repo: root.join("repo"),
            root,
        };
        write(
            &w.p("bin/tailscale"),
            &TAILSCALE.replace("@PC@", &w.s("pc")),
            0o755,
        );
        if fake_bwrap {
            write(
                &w.p("pc/bin/bwrap"),
                &BWRAP.replace("@PC@", &w.s("pc")),
                0o755,
            );
        }
        fs::create_dir_all(w.p("pc/bin")).unwrap();
        write(&w.p("pc/loadavg"), "0.10 0.10 0.10 1/100 1\n", 0o644);
        // Linux 16,384 MB available, Windows 8,192 MB free: the PC has 8,192 MB.
        meminfo(&w, 16_777_216);
        write(
            &w.p("pc/powershell.exe"),
            &POWERSHELL.replace("@PC@", &w.s("pc")),
            0o755,
        );
        write(&w.p("pc/windows-free"), "8388608", 0o644);
        // The PC user's own global excludes hide every .txt: they must not apply to the tree.
        write(&w.p("pc/home/ignore"), "*.txt\n", 0o644);
        let pc_gitconfig = format!("[core]\n\texcludesFile = {}\n", w.s("pc/home/ignore"));
        write(&w.p("pc/home/.gitconfig"), &pc_gitconfig, 0o644);
        write(&w.p("gitconfig"), "", 0o644);
        write(&w.p("xdg/git/ignore"), "glob-ignored*\n", 0o644);
        w.policy(&format!(
            r#"{{"hosts": {{"pc": {{"ssh": "tester@fake-pc", "work": "{}", "busy_load": 0.75}}}}}}"#,
            w.s("pc/work")
        ));
        fs::create_dir_all(&w.repo).unwrap();
        write(&w.repo.join("check"), CHECK, 0o755);
        write(&w.repo.join("a.txt"), "a\n", 0o644);
        write(&w.repo.join("run.sh"), "#!/bin/sh\necho run\n", 0o755);
        write(&w.repo.join("sub/b.txt"), "b\n", 0o644);
        write(&w.repo.join(".gitignore"), "ignored*\n", 0o644);
        std::os::unix::fs::symlink("a.txt", w.repo.join("link")).unwrap();
        sh(
            &w.repo,
            "git init -q -b main . && git remote add origin https://github.com/example/repo.git \
             && git add -A && git -c user.name=t -c user.email=t@e commit -q -m one",
        );
        w
    }
    fn p(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
    fn s(&self, rel: &str) -> String {
        self.p(rel).to_str().unwrap().to_owned()
    }
    fn dir(&self) -> String {
        self.repo.to_str().unwrap().to_owned()
    }
    fn policy(&self, text: &str) {
        write(&self.p("policy.json"), text, 0o644);
    }
    fn no_policy(&self) {
        let _ = fs::remove_file(self.p("policy.json"));
    }
    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_ap"));
        let path = format!("{}:{}", self.s("bin"), std::env::var("PATH").unwrap());
        c.arg("check")
            .args(args)
            .env("PATH", path)
            .env("AP_POLICY_FILE", self.p("policy.json"))
            .env("AP_CHECKS_LOG", self.p("checks.jsonl"))
            .env("HOST_CARGO_LOCK", self.p("build.lock"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", self.p("gitconfig"))
            .env("XDG_CONFIG_HOME", self.p("xdg"))
            .stdin(Stdio::null());
        // `ap check` sets these itself: the test fails when it does not, and passes on the PC too.
        for v in CARGO_SETTINGS
            .map(|(n, _)| n)
            .into_iter()
            .chain(["AP_CHECK_HOST"])
        {
            c.env_remove(v);
        }
        c
    }
    fn ap(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    fn records(&self) -> Vec<Value> {
        fs::read_to_string(self.p("checks.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }
    /// The key of the worktree on the PC: the first 16 hex of the SHA-256 of its path.
    fn key(&self, repo: &Path) -> String {
        format!("{:x}", Sha256::digest(repo.to_str().unwrap().as_bytes()))[..16].to_owned()
    }
    fn tree(&self) -> PathBuf {
        self.p(&format!("pc/work/trees/{}", self.key(&self.repo)))
    }
    fn payload_members(&self) -> Vec<String> {
        let mut m = lines(
            &Command::new("tar")
                .args(["-tf", &self.s("pc/payload.tar")])
                .output()
                .unwrap()
                .stdout,
        );
        m.sort();
        m
    }
}
impl Drop for World {
    fn drop(&mut self) {
        let _ = Command::new("chmod")
            .args(["-R", "u+rwX"])
            .arg(&self.root)
            .status();
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn error(out: &Output) -> Value {
    assert_eq!(out.status.code(), Some(2), "{out:?}");
    assert!(out.stdout.is_empty());
    let text = String::from_utf8(out.stderr.clone()).unwrap();
    assert_eq!(text.lines().count(), 1, "{text}");
    serde_json::from_str::<Value>(&text).unwrap()["error"].clone()
}

// spec: specs/ap/check.md#R1
#[test]
fn r1_a_malformed_call_is_refused_before_any_effect() {
    let w = World::new();
    let d = w.dir();
    for (args, code, field) in [
        (vec![], "usage", "args"),
        (vec!["--timeout", "5", &d], "usage", "args"),
        (vec![&d, "--timeout"], "usage", "args"),
        (vec![&d, "--"], "usage", "args"),
        (vec![&d, "extra"], "usage", "args"),
        (
            vec![&d, "--timeout", "5", "--timeout", "6"],
            "usage",
            "args",
        ),
        (vec!["repo"], "invalid_input", "dir"),
        (vec![&d, "--timeout", "0"], "invalid_input", "timeout"),
        (vec![&d, "--timeout", "14401"], "invalid_input", "timeout"),
        (vec![&d, "--timeout", "1e3"], "invalid_input", "timeout"),
        (vec![&d, "--", ""], "invalid_input", "command"),
    ] {
        let e = error(&w.ap(&args));
        assert_eq!(
            (e["code"].as_str(), e["field"].as_str()),
            (Some(code), Some(field)),
            "{args:?}"
        );
    }
    // Neither the record nor the PC is touched.
    assert!(!w.p("checks.jsonl").exists());
    assert!(!w.p("pc/calls").exists());
    // The bounds themselves are taken (here: one second does not cover the fake PC's setup on a busy host).
    w.no_policy();
    for t in ["1", "14400"] {
        assert_eq!(
            w.ap(&[&d, "--timeout", t, "--", "true"]).status.code(),
            Some(0)
        );
    }
}

// spec: specs/ap/check.md#R2
#[test]
fn r2_a_directory_that_is_not_a_worktree_on_github_is_refused() {
    let w = World::new();
    let sub = w.s("repo/sub");
    let plain = w.s("pc");
    let missing = w.s("missing");
    for dir in [&sub, &plain, &missing, &w.s("repo/a.txt")] {
        let e = error(&w.ap(&[dir]));
        assert_eq!(e["code"], "not_a_worktree", "{dir}");
        assert_eq!(e["field"], "dir");
    }
    sh(
        &w.repo,
        "git remote set-url origin https://gitlab.com/example/repo.git",
    );
    assert_eq!(error(&w.ap(&[&w.dir()]))["code"], "origin_not_github");
    sh(&w.repo, "git remote remove origin");
    assert_eq!(error(&w.ap(&[&w.dir()]))["code"], "origin_not_github");
    assert!(!w.p("checks.jsonl").exists());
    assert!(!w.p("pc/calls").exists());
}

// spec: specs/ap/check.md#R3
#[test]
fn r3_an_invalid_policy_is_refused_and_an_absent_one_runs_here() {
    let w = World::new();
    let d = w.dir();
    for (text, member) in [
        ("{", "file "),
        ("[]", "file "),
        (
            r#"{"hosts": {"pc": {"ssh": "nobody", "work": "/w", "busy_load": 0.5}}}"#,
            "hosts.pc.ssh ",
        ),
        (
            r#"{"hosts": {"pc": {"ssh": "u@h", "work": "w", "busy_load": 0.5}}}"#,
            "hosts.pc.work ",
        ),
        (
            r#"{"hosts": {"pc": {"ssh": "u@h", "work": "/w", "busy_load": 2}}}"#,
            "hosts.pc.busy_load ",
        ),
        (
            r#"{"hosts": {"pc": {"ssh": "u@h", "work": "/w", "busy_load": 0.5, "min_available_mb": -1}}}"#,
            "hosts.pc.min_available_mb ",
        ),
        (
            r#"{"hosts": {"pc": {"ssh": "u@h", "work": "/w", "busy_load": 0.5, "min_available_mb": 1048577}}}"#,
            "hosts.pc.min_available_mb ",
        ),
        (
            r#"{"hosts": {"pc": {"ssh": "u@h", "work": "/w", "busy_load": 0.5, "min_available_mb": 2048.5}}}"#,
            "hosts.pc.min_available_mb ",
        ),
        (
            r#"{"hosts": {"pc": {"ssh": "u@h", "work": "/w", "busy_load": 0.5, "min_available_mb": 8192.0}}}"#,
            "hosts.pc.min_available_mb ",
        ),
        (
            r#"{"hosts": {"pc": {"ssh": "u@h", "work": "/w", "busy_load": 0.5, "max_jobs": 4}}}"#,
            "hosts.pc ",
        ),
        (r#"{"hosts": {"vps": {}}}"#, "hosts "),
    ] {
        w.policy(text);
        let e = error(&w.ap(&[&d]));
        assert_eq!(e["code"], "policy_invalid", "{text}");
        assert_eq!(e["field"], "policy");
        assert!(e["detail"].as_str().unwrap().starts_with(member), "{e}");
    }
    assert!(!w.p("checks.jsonl").exists());
    // Without the file, and with a file without hosts: here, saying so.
    w.no_policy();
    let out = w.ap(&[&d]);
    assert_eq!(
        lines(&out.stderr)[0],
        "check: on this host (no_pc_in_policy)"
    );
    w.policy(r#"{"models": {"allowed": ["x"], "never": []}}"#);
    let out = w.ap(&[&d]);
    assert_eq!(
        lines(&out.stderr)[0],
        "check: on this host (no_pc_in_policy)"
    );
    assert!(!w.p("pc/calls").exists());
}

// spec: specs/ap/check.md#R4
// spec: specs/ap/check.md#R14
#[test]
fn r4_the_pc_runs_the_call_when_it_answers_and_is_not_busy() {
    let w = World::new();
    let d = w.dir();
    let tree = w.tree();
    let tree = tree.to_str().unwrap();
    let where_ = ["--", "sh", "-c", "echo \"in $PWD\""];
    let run = |reason: &str, place: &str| {
        let out = w.ap(&[&[d.as_str()][..], &where_].concat());
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert_eq!(lines(&out.stderr)[0], reason);
        assert_eq!(lines(&out.stdout), [format!("in {place}")]);
        let r = w.records().pop().unwrap();
        (r["host"].clone(), r["reason"].clone())
    };
    // 3.00 is 0.75 of 4 CPUs: busy; just below it is not.
    write(&w.p("pc/loadavg"), "2.99 0.10 0.10 1/100 1\n", 0o644);
    assert_eq!(
        run("check: on pc (4 CPUs, load 2.99, 8192 MB available)", tree),
        ("pc".into(), Value::Null)
    );
    write(&w.p("pc/loadavg"), "3.00 0.10 0.10 1/100 1\n", 0o644);
    assert_eq!(
        run("check: on this host (pc_busy)", &d),
        ("local".into(), "pc_busy".into())
    );
    write(&w.p("pc/probe-fails"), "", 0o644);
    assert_eq!(
        run("check: on this host (pc_unreachable)", &d),
        ("local".into(), "pc_unreachable".into())
    );
    write(&w.p("pc/down"), "", 0o644);
    assert_eq!(
        run("check: on this host (pc_unreachable)", &d),
        ("local".into(), "pc_unreachable".into())
    );
    let _ = fs::remove_file(w.p("pc/down"));
    let _ = fs::remove_file(w.p("pc/probe-fails"));
    write(&w.p("pc/loadavg"), "0.20 0.10 0.10 1/100 1\n", 0o644);
    // The memory: the smaller of Linux's MemAvailable and Windows' free memory, in MB rounded down,
    // against min_available_mb, 2,048 when the policy does not give it.
    let pc_ = |s: &str| format!("check: on pc (4 CPUs, load 0.20, {s} MB available)");
    let memory = |r: &Value| {
        Value::Array(
            [
                "pc_cpus",
                "pc_load",
                "pc_wsl_available_mb",
                "pc_windows_available_mb",
                "pc_available_mb",
            ]
            .iter()
            .map(|k| r[*k].clone())
            .collect(),
        )
    };
    let calls_before = fs::read_to_string(w.p("pc/powershell-calls")).unwrap();
    write(&w.p("pc/windows-free"), "2097152", 0o644);
    assert_eq!(run(&pc_("2048"), tree), ("pc".into(), Value::Null));
    write(&w.p("pc/windows-free"), "2097151", 0o644);
    assert_eq!(
        run("check: on this host (pc_memory_low)", &d),
        ("local".into(), "pc_memory_low".into())
    );
    let r = w.records().pop().unwrap();
    assert_eq!(memory(&r), serde_json::json!([4, 0.2, 16384, 2047, 2047]));
    assert_eq!(r["pc_min_wsl_available_mb"], Value::Null);
    // Linux short while Windows is not.
    write(&w.p("pc/windows-free"), "8388608", 0o644);
    meminfo(&w, 2_097_151);
    assert_eq!(
        run("check: on this host (pc_memory_low)", &d),
        ("local".into(), "pc_memory_low".into())
    );
    assert_eq!(
        memory(&w.records().pop().unwrap()),
        serde_json::json!([4, 0.2, 2047, 8192, 2047])
    );
    // Busy and short of memory: busy, with what the probe read.
    write(&w.p("pc/loadavg"), "3.00 0.10 0.10 1/100 1\n", 0o644);
    assert_eq!(
        run("check: on this host (pc_busy)", &d),
        ("local".into(), "pc_busy".into())
    );
    assert_eq!(
        memory(&w.records().pop().unwrap()),
        serde_json::json!([4, 3.0, 2047, 8192, 2047])
    );
    write(&w.p("pc/loadavg"), "0.20 0.10 0.10 1/100 1\n", 0o644);
    // Windows without an answer, failing, or answering no number: Linux's reading alone.
    meminfo(&w, 3_072_000);
    write(&w.p("pc/windows-free"), "not a number", 0o644);
    assert_eq!(run(&pc_("3000"), tree), ("pc".into(), Value::Null));
    write(&w.p("pc/windows-fails"), "", 0o644);
    assert_eq!(run(&pc_("3000"), tree), ("pc".into(), Value::Null));
    fs::remove_file(w.p("pc/windows-fails")).unwrap();
    fs::rename(w.p("pc/powershell.exe"), w.p("pc/powershell.off")).unwrap();
    assert_eq!(run(&pc_("3000"), tree), ("pc".into(), Value::Null));
    assert_eq!(
        memory(&w.records().pop().unwrap()),
        serde_json::json!([4, 0.2, 3000, null, 3000])
    );
    fs::rename(w.p("pc/powershell.off"), w.p("pc/powershell.exe")).unwrap();
    // The policy's bound is the one taken.
    w.policy(&format!(
        r#"{{"hosts": {{"pc": {{"ssh": "tester@fake-pc", "work": "{}", "busy_load": 0.75, "min_available_mb": 4096}}}}}}"#,
        w.s("pc/work")
    ));
    assert_eq!(
        run("check: on this host (pc_memory_low)", &d),
        ("local".into(), "pc_memory_low".into())
    );
    // Linux's memory unread: the probe did not answer, and nothing of it is kept.
    write(&w.p("pc/meminfo"), "MemTotal: 32000000 kB\n", 0o644);
    assert_eq!(
        run("check: on this host (pc_unreachable)", &d),
        ("local".into(), "pc_unreachable".into())
    );
    let r = w.records().pop().unwrap();
    assert_eq!(
        memory(&r),
        serde_json::json!([null, null, null, null, null])
    );
    // PowerShell ran once per probe that reached it, as the page's one call: no profile, no input.
    let calls = fs::read_to_string(w.p("pc/powershell-calls")).unwrap();
    let new: Vec<&str> = calls[calls_before.len()..].lines().collect();
    assert_eq!(new.len(), 8, "{calls}");
    for c in new {
        assert_eq!(
            c,
            "-NoProfile -NonInteractive -Command (Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory"
        );
    }
    meminfo(&w, 16_777_216);
    write(&w.p("pc/windows-free"), "8388608", 0o644);
    w.policy(&format!(
        r#"{{"hosts": {{"pc": {{"ssh": "tester@fake-pc", "work": "{}", "busy_load": 0.75}}}}}}"#,
        w.s("pc/work")
    ));
    // The default command is ./check; its exit status is the call's.
    let out = w.ap(&[&d]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(lines(&out.stdout), [format!("check ran in {tree}")]);
    assert_eq!(
        lines(&out.stderr),
        [
            "check: on pc (4 CPUs, load 0.20, 8192 MB available)",
            "check err"
        ]
    );
    w.no_policy();
    let out = w.ap(&[&d]);
    assert_eq!(out.status.code(), Some(3));
    assert_eq!(lines(&out.stdout), [format!("check ran in {d}")]);
    assert_eq!(
        lines(&out.stderr),
        ["check: on this host (no_pc_in_policy)", "check err"]
    );
    // A command killed by a signal: 128 and the signal.
    let out = w.ap(&[&d, "--", "sh", "-c", "kill -TERM $$"]);
    assert_eq!(out.status.code(), Some(143));
    // A command that cannot be started here: 127.
    let out = w.ap(&[&d, "--", "./no-such-program"]);
    assert_eq!(out.status.code(), Some(127));
}

// spec: specs/ap/check.md#R5
#[test]
fn r5_here_a_command_other_than_check_takes_the_build_lock() {
    let w = World::new();
    w.no_policy();
    let d = w.dir();
    let probe = "if flock -n \"$HOST_CARGO_LOCK\" true; then echo free; else echo held; fi";
    // Without the lock file nothing is locked and nothing is made.
    let out = w.ap(&[&d, "--", "sh", "-c", "echo ran"]);
    assert_eq!(lines(&out.stdout), ["ran"]);
    assert!(!w.p("build.lock").exists());
    write(&w.p("build.lock"), "", 0o644);
    let out = w.ap(&[&d, "--", "sh", "-c", probe]);
    assert_eq!(lines(&out.stdout), ["held"]);
    // ./check takes the lock by itself, with or without arguments: it is not held for it.
    let out = w.ap(&[&d]);
    assert_eq!(
        lines(&out.stdout),
        [format!("check ran in {d}"), "lock free".into()]
    );
    let out = w.ap(&[&d, "--", "./check", "content"]);
    assert_eq!(
        lines(&out.stdout),
        [format!("check ran in {d}"), "lock free".into()]
    );
}

// spec: specs/ap/check.md#R17
#[test]
fn r17_ap_check_host_is_pc_on_the_pc_only_and_the_cargo_settings_are_set_on_both() {
    let w = World::new();
    let d = w.dir();
    // On the stub PC.
    let out = w.ap(&[&d, "--", "sh", "-c", SHOW_ENV]);
    assert_eq!(lines(&out.stdout), ["host=pc 0 0 0"], "{out:?}");
    // Here, with and without the build lock, and with an inherited marker that ap check removes.
    w.no_policy();
    let out = w
        .command(&[&d, "--", "sh", "-c", SHOW_ENV])
        .env("AP_CHECK_HOST", "pc")
        .output()
        .unwrap();
    assert_eq!(lines(&out.stdout), ["host=unset 0 0 0"], "{out:?}");
    write(&w.p("build.lock"), "", 0o644);
    let out = w.ap(&[&d, "--", "sh", "-c", SHOW_ENV]);
    assert_eq!(lines(&out.stdout), ["host=unset 0 0 0"], "{out:?}");
    // An inherited setting of another value is replaced.
    let out = w
        .command(&[&d, "--", "sh", "-c", SHOW_ENV])
        .env("CARGO_INCREMENTAL", "1")
        .output()
        .unwrap();
    assert_eq!(lines(&out.stdout), ["host=unset 0 0 0"], "{out:?}");
}

// spec: specs/ap/check.md#R6
#[test]
fn r6_the_pc_tree_is_the_worktree_as_git_lists_it() {
    let w = World::new();
    let d = w.dir();
    let state = |w: &World| {
        sh(
            &w.repo,
            "echo changed >> a.txt && echo s > staged.txt && git add staged.txt \
             && echo g > ghost.txt && git add ghost.txt && rm ghost.txt && rm sub/b.txt \
             && echo n > new.txt && ln -sfn new.txt newlink && printf x > tool.sh && chmod 700 tool.sh \
             && echo i > ignored-x && echo i > info-ignored-x && echo i > glob-ignored-x \
             && mkdir -p .git/info && echo 'info-ignored*' > .git/info/exclude",
        );
    };
    state(&w);
    let manifest = |w: &World| {
        let local = sh(&w.repo, MANIFEST);
        let out = w.ap(&[&d, "--", "bash", "-c", MANIFEST]);
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert!(lines(&out.stderr)[0].starts_with("check: on pc"), "{out:?}");
        assert_eq!(String::from_utf8(out.stdout).unwrap(), local);
        // Nothing else is in the tree: the listed files that exist and no other.
        let listed: Vec<String> = local
            .lines()
            .filter(|l| l.starts_with("file ") || l.starts_with("link "))
            .map(|l| l.split(' ').nth(1).unwrap().to_owned())
            .collect();
        assert_eq!(sh(&w.tree(), FOUND).lines().collect::<Vec<_>>(), listed);
        local
    };
    let first = manifest(&w);
    for want in [
        "absent ghost.txt",
        "absent sub/b.txt",
        "file tool.sh 700 ",
        "link newlink new.txt",
        "link link a.txt",
        "file run.sh 755 ",
        "file staged.txt 644 ",
    ] {
        assert!(
            first.lines().any(|l| l.starts_with(want)),
            "{want}:\n{first}"
        );
    }
    assert!(!first.contains("ignored-x"), "{first}");
    assert!(
        first.contains("--others\nnew.txt\nnewlink\ntool.sh\n"),
        "{first}"
    );
    assert!(w.payload_members().contains(&"head.bundle".to_owned()));
    // A new commit reaches the tree as a bundle of what it lacks; the same commit needs none.
    sh(
        &w.repo,
        "git add -A && git -c user.name=t -c user.email=t@e commit -q -m two && echo again >> run.sh && git rm -q --cached a.txt",
    );
    manifest(&w);
    let pc_head = sh(&w.tree(), "git rev-parse HEAD");
    assert_eq!(pc_head, sh(&w.repo, "git rev-parse HEAD"));
    assert!(w.payload_members().contains(&"head.bundle".to_owned()));
    manifest(&w);
    assert!(!w.payload_members().contains(&"head.bundle".to_owned()));
}

// spec: specs/ap/check.md#R7
#[test]
fn r7_only_the_payload_and_the_script_reach_the_pc() {
    let w = World::new();
    let d = w.dir();
    sh(&w.repo, "echo n > new.txt && echo i > ignored-secret");
    let secret = "s3cr3t-value-of-this-host";
    let out = w
        .command(&[&d, "--", "sh", "-c", "echo ${SECRET_PROBE:-absent}"])
        .env("SECRET_PROBE", secret)
        .output()
        .unwrap();
    assert_eq!(lines(&out.stdout), ["absent"]);
    // Not even the variables ssh sends by default (LANG, LC_*): tailscale gets none of them.
    let out = w
        .command(&[
            &d,
            "--",
            "sh",
            "-c",
            "echo ${LANG:-absent} ${LC_APVERIFY:-absent} ${LC_ALL:-absent}",
        ])
        .env("LANG", "C.UTF-8")
        .env("LC_APVERIFY", "lc-value")
        .env("LC_ALL", "C")
        .output()
        .unwrap();
    assert_eq!(lines(&out.stdout), ["absent absent absent"], "{out:?}");
    // The payload of that second call: the commit is on the PC already, so no bundle goes.
    assert_eq!(
        w.payload_members(),
        [
            "exclude",
            "global-excludes",
            "head",
            "index-info",
            "tree.tar"
        ]
    );
    // The tree's tar holds exactly the listed files.
    fs::create_dir(w.p("unpacked")).unwrap();
    sh(&w.root, "tar -xf pc/payload.tar -C unpacked");
    let mut members = lines(
        &Command::new("tar")
            .args(["-tf", &w.s("unpacked/tree.tar")])
            .output()
            .unwrap()
            .stdout,
    );
    members.sort();
    assert_eq!(
        members,
        [
            ".gitignore",
            "a.txt",
            "check",
            "link",
            "new.txt",
            "run.sh",
            "sub/b.txt"
        ]
    );
    // The command line is the fixed script and its arguments: nothing of this host's environment.
    let remote = fs::read_to_string(w.p("pc/remote")).unwrap();
    assert!(
        remote.starts_with("bash -c '# The PC'\\''s side of `ap check`"),
        "{remote}"
    );
    assert!(!remote.contains(secret));
    let payload = fs::read(w.p("pc/payload.tar")).unwrap();
    assert!(
        !payload
            .windows(secret.len())
            .any(|x| x == secret.as_bytes())
    );
    assert!(
        remote.ends_with(
            " 'sh' '-c' 'echo ${LANG:-absent} ${LC_APVERIFY:-absent} ${LC_ALL:-absent}'"
        ),
        "{remote}"
    );
}

/// Whether the real `bwrap` can make the namespaces the PC's script asks for here: a user, a PID
/// and a network namespace (not on every CI runner).
fn bwrap_works() -> bool {
    Command::new("bwrap")
        .args([
            "--unshare-user",
            "--uid",
            "0",
            "--gid",
            "0",
            "--unshare-pid",
            "--unshare-net",
            "--die-with-parent",
            "--new-session",
        ])
        .args([
            "--ro-bind",
            "/",
            "/",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--",
            "true",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The first of `candidates` that a world can be made in where `work` is not covered by the
/// sandbox's own mount at `/tmp` (`specs/ap/check.md` R8): a directory that exists, resolved through
/// links is neither `/tmp` nor below it, and in which a directory can be made. Where none is usable
/// the test fails and names every candidate.
fn outside_tmp(candidates: &[&str]) -> PathBuf {
    for c in candidates {
        let Ok(dir) = fs::canonicalize(c) else {
            continue;
        };
        if !dir.is_dir() || dir.starts_with("/tmp") {
            continue;
        }
        let made = Command::new("mktemp")
            .args(["-d", "-p"])
            .arg(&dir)
            .output()
            .unwrap();
        if !made.status.success() {
            continue;
        }
        let _ = fs::remove_dir(String::from_utf8(made.stdout).unwrap().trim());
        return dir;
    }
    panic!("no directory outside /tmp to make the world of r8 in: tried {candidates:?}");
}

/// The parents of a shown path that the view has of each of `/mnt`, `/run`, `/var`, `/root` and
/// `/home` (`specs/ap/check.md` R8): the one name below it that leads to the world's root, else absent.
fn parents_of(root: &Path) -> Vec<String> {
    ["/mnt", "/run", "/var", "/root", "/home"]
        .iter()
        .map(|d| match root.strip_prefix(d) {
            Ok(rest) => {
                let name = rest.components().next().unwrap().as_os_str();
                format!("{d}: {}", name.to_str().unwrap())
            }
            Err(_) => format!("{d} absent"),
        })
        .collect()
}
/// What a sandbox shows of each of the parents of `parents_of`, one line each.
const PARENTS: &str = "for d in /mnt /run /var /root /home; do if [ -e \"$d\" ] || [ -L \"$d\" ]; then echo \"$d: $(ls -A \"$d\" | tr '\\n' ' ' | sed 's/ $//')\"; else echo \"$d absent\"; fi; done";
/// The host's `/etc/resolv.conf` resolved through its links, where that is a file outside `/etc`.
fn resolv_outside_etc() -> Option<PathBuf> {
    fs::canonicalize("/etc/resolv.conf")
        .ok()
        .filter(|r| !r.starts_with("/etc") && r.is_file())
}
fn own_net() -> String {
    fs::read_link("/proc/self/ns/net")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned()
}

// spec: specs/ap/check.md#R8
#[test]
fn r8_on_the_pc_the_command_is_uid_0_with_a_read_only_root_but_its_directories() {
    if !bwrap_works() {
        eprintln!("skipped: bwrap cannot make a user namespace on this host");
        return;
    }
    let parent = outside_tmp(&[env!("CARGO_TARGET_TMPDIR"), "/var/tmp"]);
    let w = World::within(Some(&parent), false);
    // The fake account's home: a file outside the toolchain, one in its .cargo/bin, one in its
    // .rustup; and another key's tree under work.
    let home = w.s("pc/home");
    write(&w.p("pc/home/secret"), "s\n", 0o644);
    write(&w.p("pc/home/.cargo/bin/tool"), "t\n", 0o755);
    write(&w.p("pc/home/.rustup/settings.toml"), "r\n", 0o644);
    let other = "0123456789abcdef";
    write(&w.p(&format!("pc/work/trees/{other}/f")), "o\n", 0o644);
    let script = "id -u; for p in \"$PWD/x\" \"$CARGO_TARGET_DIR/x\" \"$CARGO_HOME/x\" /tmp/x \"$HOME/x\" /etc/ap-check-x \"$CARGO_TARGET_DIR/../x\" \"$PWD/.git/x\" /ap-check-x \"$HOME/.cargo/bin/x\" \"$HOME/.rustup/x\"; do \
                  if touch \"$p\" 2>/dev/null; then echo \"rw $p\"; else echo \"ro $p\"; fi; done; echo \"tmp $TMPDIR\"; \
                  for f in \"$HOME/secret\" \"$HOME/.cargo/bin/tool\" \"$HOME/.rustup/settings.toml\" \"$1/trees/$2/f\" \"$1/repos\" \"$1/tmp\" \"$1/locks\" \"$1/used\"; do \
                  if [ -e \"$f\" ]; then echo \"seen $f\"; else echo \"unseen $f\"; fi; done; \
                  echo \"if $(sed -n '3,$p' /proc/net/dev | cut -d: -f1 | tr -d ' ' | tr '\\n' ' ')\"; \
                  echo \"offline ${CARGO_NET_OFFLINE:-unset}\"";
    let work = w.s("pc/work");
    let script = format!("{script}; {PARENTS}; readlink /proc/self/ns/net");
    let out = w.ap(&[&w.dir(), "--", "sh", "-c", &script, "x", &work, other]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    // The real bwrap carries the marker and a cargo setting itself, not the stand-in's --setenv.
    let out_env = w.ap(&[&w.dir(), "--", "sh", "-c", SHOW_ENV]);
    assert_eq!(lines(&out_env.stdout), ["host=pc 0 0 0"], "{out_env:?}");
    let key = w.key(&w.repo);
    let mut got = lines(&out.stdout);
    // A network namespace of its own, with the loopback alone.
    let net = got.pop().unwrap();
    assert!(net.starts_with("net:["), "{net}");
    assert_ne!(net, own_net());
    let mut want = vec![
        "0".to_owned(),
        format!("rw {work}/trees/{key}/x"),
        format!("rw {work}/targets/{key}/x"),
        format!("ro {work}/cargo-home/x"),
        "rw /tmp/x".into(),
        format!("ro {home}/x"),
        "ro /etc/ap-check-x".into(),
        format!("ro {work}/targets/{key}/../x"),
        format!("ro {work}/trees/{key}/.git/x"),
        "ro /ap-check-x".into(),
        format!("ro {home}/.cargo/bin/x"),
        format!("ro {home}/.rustup/x"),
        "tmp /tmp".into(),
        format!("unseen {home}/secret"),
        format!("seen {home}/.cargo/bin/tool"),
        format!("seen {home}/.rustup/settings.toml"),
        format!("unseen {work}/trees/{other}/f"),
        format!("unseen {work}/repos"),
        format!("unseen {work}/tmp"),
        format!("unseen {work}/locks"),
        format!("unseen {work}/used"),
        "if lo ".into(),
        "offline true".into(),
    ];
    want.extend(parents_of(&w.root));
    assert_eq!(got, want);
    // The temporary directory goes with the call.
    assert_eq!(fs::read_dir(w.p("pc/work/tmp")).unwrap().count(), 0);
}

/// A fake `cargo` for the step of R18 in the real `bwrap`: it says on standard error what it can
/// write and see; `@TARGET@` is the worktree's target directory and `@RESOLV@` the file the host's
/// `/etc/resolv.conf` resolves to outside `/etc`, empty where there is none.
const STEP_CARGO: &str = r#"#!/bin/sh
for a; do m=$a; done
t=${m%/Cargo.toml}
if touch "$t/x" 2>/dev/null; then echo "step tree rw" >&2; else echo "step tree ro" >&2; fi
if touch "$CARGO_HOME/x" 2>/dev/null; then echo "step cargo home rw" >&2; else echo "step cargo home ro" >&2; fi
if [ -e @TARGET@ ]; then echo "step target seen" >&2; else echo "step target unseen" >&2; fi
if [ -z "@RESOLV@" ]; then echo "step resolv none" >&2; elif [ -r "@RESOLV@" ]; then echo "step resolv seen" >&2; else echo "step resolv unseen" >&2; fi
echo "step dir $PWD" >&2
echo "step net $(readlink /proc/self/ns/net)" >&2
"#;

// spec: specs/ap/check.md#R8
// spec: specs/ap/check.md#R18
#[test]
fn r18_the_step_fetches_in_a_sandbox_of_its_own_with_the_network() {
    if !bwrap_works() {
        eprintln!("skipped: bwrap cannot make a user namespace on this host");
        return;
    }
    let parent = outside_tmp(&[env!("CARGO_TARGET_TMPDIR"), "/var/tmp"]);
    let w = World::within(Some(&parent), false);
    let key = w.key(&w.repo);
    let target = w.s(&format!("pc/work/targets/{key}"));
    let resolv = resolv_outside_etc();
    let resolv_s = resolv.as_ref().map_or("", |r| r.to_str().unwrap());
    // The fake account's cargo, in its .cargo/bin, put on PATH by its .profile.
    write(
        &w.p("pc/home/.cargo/bin/cargo"),
        &STEP_CARGO
            .replace("@TARGET@", &target)
            .replace("@RESOLV@", resolv_s),
        0o755,
    );
    write(
        &w.p("pc/home/.profile"),
        "PATH=\"$HOME/.cargo/bin:$PATH\"\n",
        0o644,
    );
    write(&w.repo.join("Cargo.toml"), "[workspace]\n", 0o644);
    write(&w.repo.join("Cargo.lock"), "version = 4\n", 0o644);
    let command = "if [ -n \"$1\" ] && [ -e \"$1\" ]; then echo \"command resolv seen\"; else echo \"command resolv unseen\"; fi";
    let out = w.ap(&[&w.dir(), "--", "sh", "-c", command, "x", resolv_s]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(lines(&out.stdout), ["command resolv unseen"], "{out:?}");
    let step: Vec<String> = lines(&out.stderr)
        .into_iter()
        .filter(|l| l.starts_with("step "))
        .collect();
    let resolv_line = if resolv.is_some() {
        "step resolv seen"
    } else {
        "step resolv none"
    };
    assert_eq!(
        step,
        [
            "step tree ro".to_owned(),
            "step cargo home rw".into(),
            "step target unseen".into(),
            resolv_line.into(),
            "step dir /tmp".into(),
            format!("step net {}", own_net()),
        ],
        "{out:?}"
    );
    assert!(!w.tree().join("x").exists());
}

// spec: specs/ap/check.md#R6
// spec: specs/ap/check.md#R8
#[test]
fn r8_what_a_command_leaves_in_git_never_runs_outside_its_sandbox() {
    // The stand-in bwrap lets the command write the repository at .git: as if the read-only bind
    // were broken, nothing it writes there may run, and no repository it points to is touched.
    let w = World::new();
    let d = w.dir();
    sh(
        &w.root,
        "git init -q -b main other && git -C other -c user.name=t -c user.email=t@e commit -q --allow-empty -m o",
    );
    let other = |w: &World| {
        sh(
            &w.root,
            "git -C other rev-parse HEAD; cat other/.git/config; ls -a other/.git other/.git/hooks",
        )
    };
    let before = other(&w);
    let mark = w.s("mark");
    let plant = format!(
        "for h in reference-transaction post-checkout post-index-change pre-auto-gc; do \
           mkdir -p .git/hooks; printf '#!/bin/sh\\ntouch {mark}\\n' > .git/hooks/$h; chmod +x .git/hooks/$h; done; \
         git config core.fsmonitor 'touch {mark}.fsmonitor'; git config core.hooksPath \"$PWD/.git/hooks\"; \
         printf '* filter=x\\n' > .git/info/attributes; git config filter.x.clean 'touch {mark}.filter; cat'"
    );
    let out = w.ap(&[&d, "--", "sh", "-c", &plant]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    sh(
        &w.repo,
        "echo changed >> a.txt && git -c user.name=t -c user.email=t@e commit -q -am two",
    );
    let out = w.ap(&[&d, "--", "git", "rev-parse", "HEAD"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        sh(&w.repo, "git rev-parse HEAD")
    );
    for m in ["", ".fsmonitor", ".filter"] {
        assert!(
            !Path::new(&format!("{mark}{m}")).exists(),
            "a planted {m} ran"
        );
    }
    // A .git replaced by a link to another repository.
    let link = format!("rm -rf .git; ln -s {}/.git .git", w.s("other"));
    w.ap(&[&d, "--", "sh", "-c", &link]);
    let out = w.ap(&[&d, "--", "git", "rev-parse", "HEAD"]);
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        sh(&w.repo, "git rev-parse HEAD")
    );
    assert_eq!(other(&w), before);
}

// spec: specs/ap/check.md#R9
#[test]
fn r9_the_target_persists_per_worktree_and_one_worktree_waits_for_itself() {
    let w = World::new();
    let d = w.dir();
    let mark = [
        "--",
        "sh",
        "-c",
        "cat \"$CARGO_TARGET_DIR/mark\" 2>/dev/null || echo none; echo kept > \"$CARGO_TARGET_DIR/mark\"",
    ];
    let other = w.p("other");
    sh(
        &w.root,
        "git clone -q repo other && git -C other remote set-url origin https://github.com/example/other.git",
    );
    let o = other.to_str().unwrap().to_owned();
    assert_eq!(
        lines(&w.ap(&[&[d.as_str()][..], &mark].concat()).stdout),
        ["none"]
    );
    assert_eq!(
        lines(&w.ap(&[&[d.as_str()][..], &mark].concat()).stdout),
        ["kept"]
    );
    let out = w.ap(&[&[o.as_str()][..], &mark].concat());
    assert_eq!(lines(&out.stdout), ["none"], "{out:?}");
    // The lock of one worktree held: its call waits to its timeout; another's runs meanwhile.
    fs::create_dir_all(w.p("pc/work/locks")).unwrap();
    let held = fs::File::create(w.p(&format!("pc/work/locks/{}", w.key(&w.repo)))).unwrap();
    rustix::fs::flock(&held, rustix::fs::FlockOperation::LockExclusive).unwrap();
    let t = Instant::now();
    let waiting = w
        .command(&[&d, "--timeout", "4", "--", "echo", "ran"])
        .output()
        .unwrap();
    assert_eq!(waiting.status.code(), Some(124), "{waiting:?}");
    assert!(waiting.stdout.is_empty());
    assert!(t.elapsed() < Duration::from_secs(20));
    let out = w.ap(&[&o, "--", "echo", "ran"]);
    assert_eq!(lines(&out.stdout), ["ran"]);
    drop(held);
}

fn running(token: &str) -> bool {
    Command::new("pgrep")
        .args(["-f", token])
        .stdout(Stdio::null())
        .status()
        .unwrap()
        .success()
}

// spec: specs/ap/check.md#R10
#[test]
fn r10_at_the_timeout_the_status_is_124_and_no_process_is_left() {
    let w = World::new();
    let d = w.dir();
    let pc_policy = || {
        w.policy(&format!(
            r#"{{"hosts": {{"pc": {{"ssh": "tester@fake-pc", "work": "{}", "busy_load": 0.75}}}}}}"#,
            w.s("pc/work")
        ))
    };
    // Twenty rounds in a row: no race of the sampler's end with the command's.
    for (round, (place, token)) in (0..20).flat_map(|r| {
        [
            (r, ("pc", "ap-check-timeout-pc-a1")),
            (r, ("here", "ap-check-timeout-here-b2")),
        ]
    }) {
        let place = format!("{place} round {round}");
        if place.starts_with("here") {
            w.no_policy();
        } else {
            pc_policy();
        }
        let t = Instant::now();
        let started = w.p(&format!("started-{round}-{}", &place[..2]));
        let started_s = started.to_str().unwrap();
        let script = "touch \"$1\"; sleep 60; :";
        let out = w.ap(&[
            &d,
            "--timeout",
            "2",
            "--",
            "sh",
            "-c",
            script,
            token,
            started_s,
        ]);
        assert_eq!(out.status.code(), Some(124), "{place}: {out:?}");
        assert!(
            t.elapsed() < Duration::from_secs(15),
            "{place}: {:?}",
            t.elapsed()
        );
        assert!(!running(token), "{place}: a process of the call is left");
        // The memory sampler as well, though it is a session of its own.
        assert!(!running(&sampler(&w)), "{place}: the sampler is left");
        let r = w.records().pop().unwrap();
        assert_eq!(r["exit"], 124);
        // On the PC a command that started has its lowest value. Without the marker either no
        // command ran and the status line carried `-` (R11), or the script took its first
        // reading and its bound ended the sandbox before the command made the marker.
        if place.starts_with("pc") {
            let low = &r["pc_min_wsl_available_mb"];
            if started.exists() {
                assert_eq!(low, &Value::from(16384), "{place}: {out:?}");
            } else {
                assert!(
                    low.is_null() || low == &Value::from(16384),
                    "{place}: no marker, and the lowest memory is {low}: {out:?}"
                );
            }
        }
    }
    // The connection cut while the command runs: the script is killed, and its sampler ends by itself.
    pc_policy();
    write(&w.p("pc/cut"), "", 0o644);
    let out = w.ap(&[
        &d,
        "--timeout",
        "30",
        "--",
        "sh",
        "-c",
        "sleep 3; :",
        "ap-check-cut-c3",
    ]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(
        lines(&out.stderr).last().unwrap(),
        "check: on this host (pc_lost)"
    );
    let t = Instant::now();
    while running(&sampler(&w)) && t.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(!running(&sampler(&w)), "the sampler is left after the cut");
}

// spec: specs/ap/check.md#R10
// spec: specs/ap/check.md#R11
#[test]
fn r11_a_command_that_cannot_start_on_the_pc_ends_at_once_with_its_own_status() {
    let w = World::new();
    let d = w.dir();
    for i in 0..50 {
        let t = Instant::now();
        let out = w.ap(&[&d, "--", "./no-such-program"]);
        assert!(
            t.elapsed() < Duration::from_secs(10),
            "call {i}: {:?}",
            t.elapsed()
        );
        // The stand-in bwrap's exec fails as the shell's 127, on the PC.
        assert_eq!(out.status.code(), Some(127), "call {i}: {out:?}");
        let r = w.records().pop().unwrap();
        assert_eq!(
            (
                r["host"].as_str(),
                &r["reason"],
                &r["pc_min_wsl_available_mb"]
            ),
            (Some("pc"), &Value::Null, &Value::from(16384)),
            "call {i}"
        );
    }
    assert!(!running(&sampler(&w)), "a sampler is left");
}

// spec: specs/ap/check.md#R11
#[test]
fn r11_the_lowest_memory_falls_back_to_the_first_reading_and_a_failed_one_is_a_dash() {
    let w = World::new();
    let d = w.dir();
    // The sampler's file made unparsable (the stand-in bwrap lets the command reach it): the first
    // reading stands.
    let spoil = "sleep 3; for f in \"$1\"/tmp/*.in; do printf 'x\\n' > \"$f/low\"; done";
    let out = w.ap(&[&d, "--", "sh", "-c", spoil, "x", &w.s("pc/work")]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let r = w.records().pop().unwrap();
    assert_eq!(r["pc_min_wsl_available_mb"], 16384, "{r}");
    // The first reading failing on the run path: `-`, the record's null, and the call not lost.
    write(&w.p("pc/meminfo-run"), "MemTotal: 32000000 kB\n", 0o644);
    let out = w.ap(&[&d, "--", "sh", "-c", "exit 4"]);
    assert_eq!(out.status.code(), Some(4), "{out:?}");
    let r = w.records().pop().unwrap();
    assert_eq!(
        (
            r["host"].as_str(),
            &r["reason"],
            &r["pc_wsl_available_mb"],
            &r["pc_min_wsl_available_mb"]
        ),
        (Some("pc"), &Value::Null, &Value::from(16384), &Value::Null)
    );
}

/// The command line of a sampler of this world's calls.
fn sampler(w: &World) -> String {
    format!("ap-check-sampler {}", w.s("pc/work/tmp/"))
}

// spec: specs/ap/check.md#R11
#[test]
fn r11_a_lost_call_runs_here_once_and_the_commands_255_is_its_own() {
    let w = World::new();
    let d = w.dir();
    let tree = w.tree();
    let where_ = ["--", "sh", "-c", "echo \"in $PWD\""];
    let args = [&[d.as_str()][..], &where_].concat();
    write(&w.p("pc/lose"), "", 0o644);
    let out = w.ap(&args);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        lines(&out.stderr),
        [
            "check: on pc (4 CPUs, load 0.10, 8192 MB available)",
            "check: on this host (pc_lost)"
        ]
    );
    assert_eq!(lines(&out.stdout), [format!("in {d}")]);
    let r = w.records().pop().unwrap();
    assert_eq!(
        (r["host"].as_str(), r["reason"].as_str()),
        (Some("local"), Some("pc_lost"))
    );
    // What the probe read is kept; no lowest memory came back.
    assert_eq!(
        (&r["pc_available_mb"], &r["pc_min_wsl_available_mb"]),
        (&Value::from(8192), &Value::Null)
    );
    // The command ran there, but its status never came back: once more here.
    fs::remove_file(w.p("pc/lose")).unwrap();
    write(&w.p("pc/lose-after"), "", 0o644);
    let out = w.ap(&args);
    assert_eq!(
        lines(&out.stdout),
        [format!("in {}", tree.to_str().unwrap()), format!("in {d}")]
    );
    assert_eq!(
        lines(&out.stderr).last().unwrap(),
        "check: on this host (pc_lost)"
    );
    // The command's own 255 comes back as its status.
    fs::remove_file(w.p("pc/lose-after")).unwrap();
    let out = w.ap(&[&d, "--", "sh", "-c", "exit 255"]);
    assert_eq!(out.status.code(), Some(255));
    assert_eq!(
        lines(&out.stderr),
        ["check: on pc (4 CPUs, load 0.10, 8192 MB available)"]
    );
    let r = w.records().pop().unwrap();
    assert_eq!(
        (r["host"].as_str(), r["exit"].as_i64()),
        (Some("pc"), Some(255))
    );
}

// spec: specs/ap/check.md#R12
#[test]
fn r12_a_tree_unused_for_seven_days_is_removed_under_its_lock() {
    let w = World::new();
    let work = w.p("pc/work");
    let old = |rel: &str| {
        let p = work.join(rel);
        fs::create_dir_all(&p).unwrap();
        write(&p.join("f"), "x", 0o444);
        sh(
            &w.root,
            &format!("touch -d '8 days ago' '{}'", p.to_str().unwrap()),
        );
    };
    let fresh = |rel: &str| write(&work.join(rel).join("f"), "x", 0o644);
    let (a, b, c, l) = (
        "aaaaaaaaaaaaaaaa",
        "bbbbbbbbbbbbbbbb",
        "cccccccccccccccc",
        "dddddddddddddddd",
    );
    old(&format!("trees/{a}"));
    old(&format!("targets/{a}"));
    old(&format!("repos/{a}"));
    write(&work.join(format!("used/{a}")), "", 0o644);
    sh(&w.root, &format!("touch -d '8 days ago' pc/work/used/{a}"));
    fresh(&format!("trees/{b}"));
    write(&work.join(format!("used/{b}")), "", 0o644);
    old(&format!("trees/{c}"));
    old("trees/not-a-key");
    old(&format!("trees/{l}"));
    write(&w.p("pc/outside/f"), "x", 0o644);
    sh(&w.root, "touch -d '8 days ago' pc/outside pc/outside/f");
    fs::create_dir_all(work.join("locks")).unwrap();
    let held = fs::File::create(work.join(format!("locks/{l}"))).unwrap();
    rustix::fs::flock(&held, rustix::fs::FlockOperation::LockExclusive).unwrap();
    let out = w.ap(&[&w.dir(), "--", "true"]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    for (rel, kept) in [
        (format!("trees/{a}"), false),
        (format!("targets/{a}"), false),
        (format!("repos/{a}"), false),
        (format!("used/{a}"), false),
        (format!("trees/{b}"), true),
        (format!("trees/{c}"), false),
        ("trees/not-a-key".into(), true),
        (format!("trees/{l}"), true),
    ] {
        assert_eq!(work.join(&rel).exists(), kept, "{rel}");
    }
    assert!(w.p("pc/outside/f").exists());
    assert!(w.tree().join("check").exists());
}

// spec: specs/ap/check.md#R13
#[test]
fn r13_output_streams_back_each_to_its_own() {
    let w = World::new();
    let d = w.dir();
    for place in ["pc", "here"] {
        if place == "here" {
            w.no_policy();
        }
        let out = w.ap(&[&d, "--", "sh", "-c", "echo out; echo err >&2; echo out2"]);
        assert_eq!(lines(&out.stdout), ["out", "out2"], "{place}");
        assert_eq!(&lines(&out.stderr)[1..], ["err"], "{place}");
        // The first line arrives while the command still runs.
        let mut child = w
            .command(&[&d, "--", "sh", "-c", "echo first; sleep 3; echo second"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut reader = BufReader::new(child.stdout.take().unwrap());
        let mut first = String::new();
        reader.read_line(&mut first).unwrap();
        let at_first = Instant::now();
        let mut second = String::new();
        reader.read_line(&mut second).unwrap();
        assert_eq!((first.as_str(), second.as_str()), ("first\n", "second\n"));
        assert!(
            at_first.elapsed() > Duration::from_secs(2),
            "{place}: the first line came at the end"
        );
        assert!(child.wait().unwrap().success());
    }
}

// spec: specs/ap/check.md#R15
#[test]
fn r15_each_call_appends_one_record_line() {
    let w = World::new();
    let d = w.dir();
    let head = sh(&w.repo, "git rev-parse HEAD").trim().to_owned();
    w.ap(&[&d, "--", "sh", "-c", "exit 7"]);
    w.no_policy();
    w.ap(&[&d]);
    let r = w.records();
    assert_eq!(r.len(), 2);
    let keys = |v: &Value| {
        let mut k: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        k.sort();
        k
    };
    let want = [
        "command",
        "dir",
        "end",
        "exit",
        "head",
        "host",
        "pc_available_mb",
        "pc_cpus",
        "pc_load",
        "pc_min_wsl_available_mb",
        "pc_windows_available_mb",
        "pc_wsl_available_mb",
        "reason",
        "repository",
        "seconds",
        "start",
    ];
    for v in &r {
        assert_eq!(keys(v), want);
        assert_eq!(v["dir"], d.as_str());
        assert_eq!(v["repository"], "example/repo");
        assert_eq!(v["head"], head.as_str());
        assert!(v["seconds"].as_f64().unwrap() >= 0.0);
        for t in ["start", "end"] {
            let s = v[t].as_str().unwrap();
            assert!(
                s.len() == 20 && s.ends_with('Z') && s.as_bytes()[10] == b'T',
                "{s}"
            );
        }
    }
    assert_eq!(
        (
            r[0]["host"].as_str(),
            &r[0]["reason"],
            r[0]["exit"].as_i64()
        ),
        (Some("pc"), &Value::Null, Some(7))
    );
    assert_eq!(r[0]["command"], serde_json::json!(["sh", "-c", "exit 7"]));
    // On the PC: what the probe read, and the lowest Linux memory while the command ran.
    for (k, v) in [
        ("pc_cpus", Value::from(4)),
        ("pc_load", Value::from(0.1)),
        ("pc_wsl_available_mb", Value::from(16384)),
        ("pc_windows_available_mb", Value::from(8192)),
        ("pc_available_mb", Value::from(8192)),
        ("pc_min_wsl_available_mb", Value::from(16384)),
    ] {
        assert_eq!((k, &r[0][k]), (k, &v));
        // Here without a probe: nothing of the PC.
        assert_eq!((k, &r[1][k]), (k, &Value::Null));
    }
    assert_eq!(
        (
            r[1]["host"].as_str(),
            r[1]["reason"].as_str(),
            r[1]["exit"].as_i64()
        ),
        (Some("local"), Some("no_pc_in_policy"), Some(3))
    );
    assert_eq!(r[1]["command"], serde_json::json!(["./check"]));
    // The lowest MemAvailable while the command ran, sampled every 2 seconds outside it, in MB
    // rounded down; no PowerShell meanwhile.
    w.policy(&format!(
        r#"{{"hosts": {{"pc": {{"ssh": "tester@fake-pc", "work": "{}", "busy_load": 0.75}}}}}}"#,
        w.s("pc/work")
    ));
    let used = "touch \"$1/running\"; printf 'MemAvailable: 1049599 kB\\n' > \"$1/meminfo\"; sleep 5; \
                printf 'MemAvailable: 16777216 kB\\n' > \"$1/meminfo\"; rm \"$1/running\"";
    let calls = fs::read_to_string(w.p("pc/powershell-calls")).unwrap();
    let out = w.ap(&[&d, "--", "sh", "-c", used, "x", &w.s("pc")]);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let r = w.records().pop().unwrap();
    assert_eq!(
        (
            &r["host"],
            &r["pc_wsl_available_mb"],
            &r["pc_min_wsl_available_mb"]
        ),
        (&Value::from("pc"), &Value::from(16384), &Value::from(1024))
    );
    let after = fs::read_to_string(w.p("pc/powershell-calls")).unwrap();
    assert_eq!(after[calls.len()..].lines().count(), 1, "{after}");
    assert!(!after.contains("during"), "{after}");
    w.no_policy();
    // A record that cannot be written is said, and the status stays the command's.
    let out = w
        .command(&[&d, "--", "sh", "-c", "exit 5"])
        .env("AP_CHECKS_LOG", w.p("missing/checks.jsonl"))
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(
        lines(&out.stderr)
            .last()
            .unwrap()
            .starts_with("check: the record could not be appended")
    );
}

/// A fake `cargo` on the fake PC: notes its directory, its arguments and three variables as it has
/// them, writes to both of its outputs; fails with `cargo-fails`, outlives any bound with `cargo-sleeps`.
const FAKE_CARGO: &str = r#"#!/bin/sh
{ echo "dir $PWD"; echo "args $*"; echo "env ${CARGO_HOME-unset} ${CARGO_NET_OFFLINE-unset} ${CARGO_TARGET_DIR-unset}"; } >> @PC@/cargo-calls
echo cargo-out; echo cargo-err >&2
[ ! -e @PC@/cargo-fails ] || exit 101
[ ! -e @PC@/cargo-sleeps ] || exec sh -c 'sleep 60; :' ap-check-fetch-sleep-d4
exit 0
"#;

// spec: specs/ap/check.md#R11
// spec: specs/ap/check.md#R18
#[test]
fn r18_the_dependencies_are_fetched_once_before_the_command_and_a_failed_fetch_is_lost() {
    let w = World::new();
    let d = w.dir();
    let tree = w.tree();
    let tree = tree.to_str().unwrap().to_owned();
    let work = w.s("pc/work");
    let target = format!("{work}/targets/{}", w.key(&w.repo));
    write(
        &w.p("pc/bin/cargo"),
        &FAKE_CARGO.replace("@PC@", &w.s("pc")),
        0o755,
    );
    // The login environment sets what the step goes without.
    write(
        &w.p("pc/home/.profile"),
        "export CARGO_NET_OFFLINE=false CARGO_TARGET_DIR=/nowhere\n",
        0o644,
    );
    write(&w.repo.join("Cargo.toml"), "[workspace]\n", 0o644);
    write(&w.repo.join("Cargo.lock"), "version = 4\n", 0o644);
    let calls = || {
        let c = fs::read_to_string(w.p("pc/cargo-calls")).unwrap_or_default();
        let _ = fs::remove_file(w.p("pc/cargo-calls"));
        lines(c.as_bytes())
    };
    let command = [
        "--",
        "sh",
        "-c",
        "echo \"command $PWD\" >> \"$1/cargo-calls\"; echo \"ran ${CARGO_NET_OFFLINE-unset} ${CARGO_TARGET_DIR-unset}\"",
        "x",
    ];
    let pc = w.s("pc");
    let args = [&[d.as_str()][..], &command, &[pc.as_str()]].concat();
    let out = w.ap(&args);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    // Nothing of the step on standard output; both its outputs on standard error.
    assert_eq!(
        lines(&out.stdout),
        [format!("ran true {target}")],
        "{out:?}"
    );
    let err = lines(&out.stderr);
    assert!(err[0].starts_with("check: on pc"), "{out:?}");
    assert!(err.contains(&"cargo-out".to_owned()), "{out:?}");
    assert!(err.contains(&"cargo-err".to_owned()), "{out:?}");
    // Once, before the command, started outside the tree, without the command's two variables.
    let c = calls();
    assert_eq!(c.len(), 4, "{c:?}");
    let dir = c[0].strip_prefix("dir ").unwrap();
    assert!(!dir.starts_with(&tree), "{c:?}");
    assert_eq!(
        c[1..],
        [
            format!("args fetch --locked --manifest-path {tree}/Cargo.toml"),
            format!("env {work}/cargo-home unset unset"),
            format!("command {tree}"),
        ]
    );
    let r = w.records().pop().unwrap();
    assert_eq!(
        (r["host"].as_str(), &r["reason"]),
        (Some("pc"), &Value::Null)
    );
    // A tree without both regular files has no step.
    for change in [
        "rm Cargo.lock",
        "rm Cargo.toml && echo v > Cargo.lock",
        "echo t > Cargo.toml && ln -sfn Cargo.toml Cargo.lock",
    ] {
        sh(&w.repo, change);
        let out = w.ap(&args);
        assert_eq!(out.status.code(), Some(0), "{change}: {out:?}");
        assert_eq!(calls(), [format!("command {tree}")], "{change}");
    }
    sh(&w.repo, "rm Cargo.lock && echo l > Cargo.lock");
    // A step that fails: the PC's side failed before the command; the command runs here once.
    write(&w.p("pc/cargo-fails"), "", 0o644);
    let out = w.ap(&args);
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    let err = lines(&out.stderr);
    assert!(err.contains(&"check: pc: fetch".to_owned()), "{out:?}");
    assert_eq!(err.last().unwrap(), "check: on this host (pc_lost)");
    assert_eq!(lines(&out.stdout).len(), 1, "{out:?}");
    assert!(lines(&out.stdout)[0].starts_with("ran "), "{out:?}");
    let c = calls();
    assert_eq!(c.len(), 4, "{c:?}");
    assert_eq!(c[3], format!("command {d}"));
    let r = w.records().pop().unwrap();
    assert_eq!(
        (r["host"].as_str(), r["reason"].as_str()),
        (Some("local"), Some("pc_lost"))
    );
    // A step the bound ends: the call's end with 124 and no reading; no command ran; nothing is left.
    fs::remove_file(w.p("pc/cargo-fails")).unwrap();
    write(&w.p("pc/cargo-sleeps"), "", 0o644);
    let t = Instant::now();
    let timed = [
        &[d.as_str(), "--timeout", "3"][..],
        &command,
        &[pc.as_str()],
    ]
    .concat();
    let out = w.ap(&timed);
    assert_eq!(out.status.code(), Some(124), "{out:?}");
    assert!(t.elapsed() < Duration::from_secs(25), "{:?}", t.elapsed());
    assert!(out.stdout.is_empty(), "{out:?}");
    assert_eq!(calls().len(), 3);
    let r = w.records().pop().unwrap();
    assert_eq!(
        (
            r["host"].as_str(),
            r["exit"].as_i64(),
            &r["pc_min_wsl_available_mb"]
        ),
        (Some("pc"), Some(124), &Value::Null)
    );
    assert!(!running("ap-check-fetch-sleep-d4"), "the step is left");
}

/// A fake `ont` of the PC's account: notes its arguments, fails with `ont-fails`, else prints the
/// file `ont-version`.
const FAKE_ONT: &str = r#"#!/bin/sh
echo "$*" >> @PC@/ont-calls
[ ! -e @PC@/ont-fails ] || exit 1
cat @PC@/ont-version
"#;

// spec: specs/ap/check.md#R4
// spec: specs/ap/check.md#R15
// spec: specs/ap/check.md#R19
#[test]
fn r19_a_pinned_engine_the_pc_does_not_have_runs_here_as_pc_engine_differs() {
    let w = World::new();
    let d = w.dir();
    let tree = w.tree();
    let tree = tree.to_str().unwrap().to_owned();
    let pin = "0123456789abcdef0123456789abcdef01234567";
    let other = "fedcba9876543210fedcba9876543210fedcba98";
    write(
        &w.repo.join("ontology/ENGINE-PIN"),
        &format!("{pin}\n"),
        0o644,
    );
    let ont = FAKE_ONT.replace("@PC@", &w.s("pc"));
    write(&w.p("pc/bin/ont"), &ont, 0o755);
    write(
        &w.p("pc/ont-version"),
        &format!("ont 0.1.0 {other}\n"),
        0o644,
    );
    let where_ = ["--", "sh", "-c", "echo \"in $PWD\""];
    let args = [&[d.as_str()][..], &where_].concat();
    let ont_calls = || {
        let c = fs::read_to_string(w.p("pc/ont-calls")).unwrap_or_default();
        let _ = fs::remove_file(w.p("pc/ont-calls"));
        lines(c.as_bytes())
    };
    let differs = |what: &str, load: f64| {
        let _ = fs::remove_file(w.p("pc/remote"));
        let out = w.ap(&args);
        assert_eq!(out.status.code(), Some(0), "{what}: {out:?}");
        assert_eq!(
            lines(&out.stderr),
            ["check: on this host (pc_engine_differs)"],
            "{what}"
        );
        assert_eq!(lines(&out.stdout), [format!("in {d}")], "{what}");
        // The PC's script is not started; the probe's readings are kept, as for pc_busy.
        assert!(!w.p("pc/remote").exists(), "{what}");
        let r = w.records().pop().unwrap();
        assert_eq!(
            (r["host"].as_str(), r["reason"].as_str()),
            (Some("local"), Some("pc_engine_differs")),
            "{what}"
        );
        for (k, v) in [
            ("pc_cpus", Value::from(4)),
            ("pc_load", Value::from(load)),
            ("pc_wsl_available_mb", Value::from(16384)),
            ("pc_windows_available_mb", Value::from(8192)),
            ("pc_available_mb", Value::from(8192)),
            ("pc_min_wsl_available_mb", Value::Null),
        ] {
            assert_eq!((k, &r[k]), (k, &v), "{what}");
        }
    };
    // Another revision: once `ont --version`, in the probe.
    differs("another engine", 0.1);
    assert_eq!(ont_calls(), ["--version"]);
    write(&w.p("pc/ont-fails"), "", 0o644);
    differs("a failing ont", 0.1);
    fs::remove_file(w.p("pc/ont-fails")).unwrap();
    // A busy PC with another engine: the engine is decided first.
    write(&w.p("pc/loadavg"), "3.00 0.10 0.10 1/100 1\n", 0o644);
    differs("busy with another engine", 3.0);
    write(&w.p("pc/loadavg"), "0.10 0.10 0.10 1/100 1\n", 0o644);
    // No ont at all: the probe has still answered.
    fs::remove_file(w.p("pc/bin/ont")).unwrap();
    differs("no ont", 0.1);
    // A pin that is not 40 hexadecimal digits equals none, even the same text.
    write(&w.repo.join("ontology/ENGINE-PIN"), "abc\n", 0o644);
    write(&w.p("pc/bin/ont"), &ont, 0o755);
    write(&w.p("pc/ont-version"), "ont 0.1.0 abc\n", 0o644);
    differs("a pin that is no revision", 0.1);
    // The same revision, from the ont the account's .profile puts on PATH: the PC takes the call.
    fs::remove_file(w.p("pc/bin/ont")).unwrap();
    write(&w.p("pc/home/.cargo/bin/ont"), &ont, 0o755);
    write(
        &w.p("pc/home/.profile"),
        "PATH=\"$HOME/.cargo/bin:$PATH\"\n",
        0o644,
    );
    write(
        &w.repo.join("ontology/ENGINE-PIN"),
        &format!(" {pin} \n"),
        0o644,
    );
    write(&w.p("pc/ont-version"), &format!("ont 0.1.0 {pin}\n"), 0o644);
    let _ = ont_calls();
    let on_pc = |what: &str| {
        let out = w.ap(&args);
        assert_eq!(out.status.code(), Some(0), "{what}: {out:?}");
        assert_eq!(
            lines(&out.stderr),
            ["check: on pc (4 CPUs, load 0.10, 8192 MB available)"],
            "{what}"
        );
        assert_eq!(lines(&out.stdout), [format!("in {tree}")], "{what}");
        let r = w.records().pop().unwrap();
        assert_eq!(
            (r["host"].as_str(), &r["reason"]),
            (Some("pc"), &Value::Null)
        );
    };
    on_pc("the same engine");
    assert_eq!(ont_calls(), ["--version"]);
    // A worktree without the file is placed as before, whatever the PC's engine.
    write(
        &w.p("pc/ont-version"),
        &format!("ont 0.1.0 {other}\n"),
        0o644,
    );
    fs::remove_file(w.repo.join("ontology/ENGINE-PIN")).unwrap();
    on_pc("no pin");
    fs::remove_file(w.p("pc/home/.cargo/bin/ont")).unwrap();
    on_pc("no pin and no ont");
}

/// `command` run under the umask `mask`, with its environment.
fn with_umask(command: &Command, mask: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c")
        .arg(format!("umask {mask} && exec \"$0\" \"$@\""))
        .arg(command.get_program())
        .args(command.get_args())
        .stdin(Stdio::null());
    for (k, v) in command.get_envs() {
        match v {
            Some(v) => c.env(k, v),
            None => c.env_remove(k),
        };
    }
    c
}

// spec: specs/ap/check.md#R15
#[test]
fn r15_a_record_file_is_made_with_mode_0600_and_one_that_exists_keeps_its_mode() {
    let w = World::new();
    w.no_policy();
    let d = w.dir();
    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o7777;
    for mask in ["022", "077", "000"] {
        let log = w.p(&format!("log-{mask}.jsonl"));
        let mut c = w.command(&[&d, "--", "true"]);
        c.env("AP_CHECKS_LOG", &log);
        let out = with_umask(&c, mask).output().unwrap();
        assert_eq!(out.status.code(), Some(0), "{out:?}");
        assert_eq!(mode(&log), 0o600, "umask {mask}");
        assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);
    }
    // A file that exists is appended to, its mode unchanged.
    let log = w.p("existing.jsonl");
    write(&log, "", 0o644);
    let mut c = w.command(&[&d, "--", "true"]);
    c.env("AP_CHECKS_LOG", &log);
    let out = with_umask(&c, "077").output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert_eq!(mode(&log), 0o644);
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);
    // A device: written to, its mode unchanged.
    let before = mode(Path::new("/dev/null"));
    let out = w
        .command(&[&d, "--", "true"])
        .env("AP_CHECKS_LOG", "/dev/null")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{out:?}");
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("could not be appended"),
        "{out:?}"
    );
    assert_eq!(mode(Path::new("/dev/null")), before);
}
